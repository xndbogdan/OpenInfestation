//! Minimal phase-4 gameplay screen: factory `00440C60`, constructor `00440990`.
//!
//! Native lifecycle (see `GAMEPLAY_RUNTIME.md`):
//! - Factory allocates the screen, constructs over inner `004408B0` plus base
//!   `00437F90`, publishes vtable `00523370`, enumerates two device sets,
//!   looks up the live player, builds the `+0xFC` sub-object, reads settings.
//! - Tick `00442B50` gates devices, latches settings, runs inner worker
//!   `00440D80`, with the tail open. Deselect `00440BA0` and menu handlers
//!   (`00443070`, `00442FD0`, `00442BC0`, `00440940`) have pure-decision
//!   owners in `gameplay_menu`.
//! - Draw `00442080` reaches world draw `00488020`; shared scene owners submit
//!   the world, and `gameplay_overlay` owns world-22 masks/location text.
//!   Other HUD callbacks remain separate. Session tick callback `004388D0` runs world
//!   update `004875A0` plus elapsed-time accounting.
//!
//! This owner retains the factory/constructor/screen-clock contract with the
//! authored gameplay input tables. Actual world simulation and drawing are
//! delegated by the session to their subsystem owners. Per-feature unknowns
//! fail closed (skip + log) via those owners, never per-phase.

use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::scheduler::SchedulerTick;
use crate::screen_input::{
    ScreenButton, ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError,
};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::{SessionPhase, SessionState};

/// `00440B60` reuse rule: the gameplay screen serves requested phases 4 and 6.
pub fn gameplay_reuses_phase(requested: SessionPhase) -> bool {
    matches!(requested, SessionPhase::Gameplay | SessionPhase::Phase6)
}

/// Opaque callback identity for the gameplay screen.
pub const GAMEPLAY_CALLBACK_CONTEXT: u32 = 5;

/// Opaque callback identity for the phase-5 screen's input, which the
/// gameplay screen carries while it serves phase 5 (InWorldMenuScreenReuse).
pub const IN_WORLD_MENU_CALLBACK_CONTEXT: u32 = 10;

/// What the phase-5 screen's held slots ask of its message (`+FC`, the first
/// player's): `00443140` and `00443100`, `00443110`, `00443120` and
/// `00443130` pass codes 4 and 0..3 to `00413950`, and `004430F0` is
/// `004133E0`, which pops a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InWorldMenuCommand {
    Code(u32),
    Back,
}

/// `00443190`'s input: its own context over the phase-5 tables and the base
/// held mask and repeat (`00437F90`, zeroed by the constructor).
#[derive(Debug)]
struct InWorldMenuInput {
    bindings: FrontendBindings,
    input: ScreenInput,
}

/// The context-resolving wrappers `004434A0`..`00443590`: each resolves the
/// current screen (`004391C0`) and presses its bit through `00439430`, or
/// clears it on release.
fn in_world_menu_button(address: u32) -> Option<ScreenButton> {
    Some(match address {
        0x0044_34a0 => ScreenButton::Previous,
        0x0044_34d0 => ScreenButton::Next,
        0x0044_3500 => ScreenButton::AdjustPrevious,
        0x0044_3530 => ScreenButton::AdjustNext,
        0x0044_3560 => ScreenButton::Confirm,
        0x0044_3590 => ScreenButton::Back,
        _ => return None,
    })
}

/// `00439470` over the phase-5 vtable `005233D8`'s held slots `+24..+38`.
fn in_world_menu_command(action: ScreenInputAction) -> Option<InWorldMenuCommand> {
    Some(match action {
        ScreenInputAction::Confirm => InWorldMenuCommand::Code(4),
        ScreenInputAction::Previous => InWorldMenuCommand::Code(0),
        ScreenInputAction::Next => InWorldMenuCommand::Code(1),
        ScreenInputAction::AdjustPrevious => InWorldMenuCommand::Code(2),
        ScreenInputAction::AdjustNext => InWorldMenuCommand::Code(3),
        ScreenInputAction::Back => InWorldMenuCommand::Back,
        _ => return None,
    })
}

/// The gameplay vtable `00523370`'s held slots: Previous, Next, AdjustPrevious
/// and AdjustNext send codes 0..3 to the first player's message (`00443760`,
/// `00443770`, `00443780` and `00443790`, each through `00443740`); Confirm
/// and Back are RETs (`00401310`).
fn gameplay_held_command(action: ScreenInputAction) -> Option<InWorldMenuCommand> {
    Some(InWorldMenuCommand::Code(match action {
        ScreenInputAction::Previous => 0,
        ScreenInputAction::Next => 1,
        ScreenInputAction::AdjustPrevious => 2,
        ScreenInputAction::AdjustNext => 3,
        _ => return None,
    }))
}

/// `00442BF8..00442C24`: F12 sets First Person (settings `+68`) to 0 when it
/// is above 0 (signed), else to 1.
pub fn first_person_toggled(value: u32) -> u32 {
    u32::from((value as i32) <= 0)
}

/// `00442C38..00442C4D`: U sets the HUD setting (`+60`) to 1 when it is 0,
/// else to 0 (`sete`).
pub fn hud_toggled(value: u32) -> u32 {
    u32::from(value == 0)
}

/// `00443020`: terrain detail (`+70`) becomes 0 when set, else 1.
pub fn terrain_detail_toggled(value: u32) -> u32 {
    u32::from(value == 0)
}

/// `00442FF0`'s next world: the current one (`+250`) mod 33, plus one.
pub fn next_world_selection(current: u32) -> u32 {
    ((current as i32) % 33 + 1) as u32
}

/// `00442B50`'s device gates over the port's one adapter, the keyboard.
/// `004399F0` counts the live adapters, and an attached keyboard always
/// counts (`005222B0` binds no status input to type 0); none asks for
/// phase 5. `00439A20` counts types 1, 2, 8 and 9, of which the port has the
/// joystick (type 2, which binds no status input either): without it the
/// input mode (settings `+8`, the Controls menu's Joystick) drops to 0
/// (Disabled), with it Disabled turns to 1 (Analogue) and Digital (2) stays.
/// Returns the phase 5 request.
pub fn gameplay_device_gates(
    keyboard_attached: bool,
    joystick_live: bool,
    input_mode_08: &mut u32,
) -> bool {
    if !keyboard_attached && !joystick_live {
        return true;
    }
    if !joystick_live {
        *input_mode_08 = 0;
    } else if *input_mode_08 == 0 {
        *input_mode_08 = 1;
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayInputAction {
    /// 00443700 nonzero callback invokes current screen virtual+64,
    /// 00443070. The caller consumes live attract/world/player fields.
    MenuKey,
    /// F12: 00443650 hands the key to the current screen's +44, the
    /// gameplay screen's 00442BF0, which on a press flips the First Person
    /// setting (+68) between 1 and 0 and refreshes the world (0040F740).
    FirstPerson,
    /// U: 00443680 hands the key to the screen's +48, 00442C30, which on a
    /// press flips the HUD setting (+60, [`hud_toggled`]).
    Hud,
    /// M: 004436D0 hands the key to +40, 00442BC0
    /// ([`crate::gameplay_menu::map_requested`]).
    Map,
    /// Shift+Esc: 00443470 hands the key to +54, 00442FD0
    /// ([`crate::gameplay_menu::return_to_frontend_requested`]).
    ReturnToFrontend,
    /// Alt+N: 00443720 hands the key to +4C, 00442FF0, which on a press ends
    /// the world for the next one: `00487560(world, 1, 0)` with
    /// [`next_world_selection`].
    NextWorld,
    /// Alt+F3: 004436B0 hands the key to +50, 00443020, which on a press
    /// flips terrain detail (+70, [`terrain_detail_toggled`]) and reselects
    /// the terrain spacing (004502C0).
    TerrainDetail,
    /// T (004435F0 to +5C, 00442CF0) and Y (00443620 to +60, 00442EE0) open
    /// a network session's menus; without one (0055F448) they return.
    NetworkMenu {
        address: u32,
    },
    /// Backspace: 004435C0 hands the key to +58, 00440940, which holds the
    /// full scoreboard (+F8) while it is down.
    Scores {
        held: bool,
    },
    NoAction,
    /// Feature-level callback boundary, reported by the production caller.
    Unsupported {
        address: u32,
        value: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum GameplayCallbackError {
    #[error("gameplay callback requires its Ready screen")]
    NotReady,
    #[error("gameplay callback context {actual} differs from {expected}")]
    Context { expected: u32, actual: u32 },
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayScreenConfig {
    pub attachment: ScreenAttachment,
    pub callback_context: u32,
}

#[derive(Debug, Error)]
pub enum GameplayScreenError<E> {
    #[error("gameplay screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("00440C60 serves requested phases 4/6 (Gameplay), not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    Terrain(#[from] crate::terrain_runtime::TerrainPolicyError),
    #[error(transparent)]
    KeyBindings(#[from] crate::key_bindings::KeyBindingError),
    #[error(transparent)]
    Executable(#[from] inf_formats::FrontendExecutableError),
    #[error("required gameplay effect failed")]
    Host(E),
}

/// Factory effects with no successful defaults.
/// Late Settings_GetPtr read, after the constructor's input prefix.
pub trait GameplayTerrainSettings {
    fn terrain_detail_setting(&mut self)
        -> Result<u32, crate::terrain_runtime::TerrainPolicyError>;
}

pub trait GameplayFactoryHost: GameplayTerrainSettings {
    type Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

/// Any frontend factory host already owns the shared keyboard adapter the
/// gameplay constructor enumerates. Reuse that enumeration so the gameplay
/// screen needs no separate platform trait.
impl<T: crate::frontend_screen::FrontendFactoryHost + GameplayTerrainSettings> GameplayFactoryHost
    for T
{
    type Error = T::Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        <T as crate::frontend_screen::FrontendFactoryHost>::enumerate_calibrated_devices(
            self, input,
        )
    }
}

/// Tick effects: none with successful defaults yet; the skeleton tick
/// succeeds without requesting phases. In-world menu requests (`00438380(5)`)
/// and world-update calls stay future owners.
pub trait GameplayTickHost {
    type Error;
}

#[derive(Debug)]
pub struct GameplayScreen {
    config: GameplayScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    /// The +0xFC sub-object's sets over the first player's callback context.
    player_bindings: Option<FrontendBindings>,
    /// DWORD 0053A278, the Grenade double tap's window. Only 004694D0
    /// reads it and nothing writes it, so initialization reads it once.
    double_tap_window: Option<i32>,
    ticks: u64,
    /// The base input's age, held mask, repeat and analog latch (+C0..+D4),
    /// which 00440D80's 004392B0 advances.
    held_input: ScreenInput,
    /// The phase-5 screen's input while this screen serves phase 5.
    in_world_menu: Option<InWorldMenuInput>,
    /// Its held slots' commands, or this screen's own, for the session to
    /// run on the message.
    in_world_menu_commands: Vec<InWorldMenuCommand>,
    /// `+EC` and `+F0`: the animation clock before Y and T open their
    /// network menus again (`004408B0` zeroes them in each new screen).
    pub(crate) vehicle_menu_ec: u32,
    pub(crate) team_menu_f0: u32,
    /// `+F8`: Backspace held (`00440940`), the network game's full
    /// scoreboard.
    pub(crate) scores_f8: bool,
}

impl GameplayScreen {
    pub fn new(config: GameplayScreenConfig, assets: &FrontendAssets) -> Self {
        let context = config.callback_context;
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            player_bindings: None,
            double_tap_window: None,
            ticks: 0,
            held_input: ScreenInput::new(ScreenDispatchPolicy::Base, context),
            in_world_menu: None,
            vehicle_menu_ec: 0,
            team_menu_f0: 0,
            scores_f8: false,
            in_world_menu_commands: Vec::new(),
        }
    }

    pub fn lifecycle(&self) -> ScreenLifecycle {
        self.lifecycle
    }

    pub fn id(&self) -> Option<ScreenId> {
        self.id
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Native base-screen +C0, consumed by 00440F00's location caption.
    pub fn screen_age_millis(&self) -> i32 {
        self.held_input.age_millis()
    }

    /// The screen's own callback context; the +0xFC sub-object's rows call
    /// back with the first player's instead.
    pub fn callback_context(&self) -> u32 {
        self.config.callback_context
    }

    /// DWORD 0053A278, read when the screen initializes.
    pub fn double_tap_window(&self) -> Option<i32> {
        self.double_tap_window
    }

    pub fn digital_callback(
        &self,
        event: InputCallback,
    ) -> Result<GameplayInputAction, GameplayCallbackError> {
        if self.lifecycle != ScreenLifecycle::Ready {
            return Err(GameplayCallbackError::NotReady);
        }
        if event.context != self.config.callback_context {
            return Err(GameplayCallbackError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        Ok(match (event.address, event.value) {
            (0x0044_3700, 0) => GameplayInputAction::NoAction,
            (0x0044_3700, _) => GameplayInputAction::MenuKey,
            (0x0044_3650, 0) => GameplayInputAction::NoAction,
            (0x0044_3650, _) => GameplayInputAction::FirstPerson,
            (
                0x0044_3680 | 0x0044_36d0 | 0x0044_3470 | 0x0044_3720 | 0x0044_36b0 | 0x0044_35f0
                | 0x0044_3620,
                0,
            ) => GameplayInputAction::NoAction,
            (0x0044_3680, _) => GameplayInputAction::Hud,
            (0x0044_36d0, _) => GameplayInputAction::Map,
            (0x0044_3470, _) => GameplayInputAction::ReturnToFrontend,
            (0x0044_3720, _) => GameplayInputAction::NextWorld,
            (0x0044_36b0, _) => GameplayInputAction::TerrainDetail,
            (address @ (0x0044_35f0 | 0x0044_3620), _) => {
                GameplayInputAction::NetworkMenu { address }
            }
            (0x0044_35c0, value) => GameplayInputAction::Scores { held: value != 0 },
            _ => GameplayInputAction::Unsupported {
                address: event.address,
                value: event.value,
            },
        })
    }

    /// Constructor `00440990` skeleton in native order: inner/base, vtable
    /// publish, first device enumeration, player lookup (future owner),
    /// `+0xFC` sub-object (future owner), settings read (future owner).
    /// Only requested phases 4/6 reach retail construction.
    pub fn initialize<H: GameplayFactoryHost>(
        &mut self,
        services: &mut GameplayFactoryServices<'_>,
        host: &mut H,
        state: &mut SessionState,
    ) -> Result<(), GameplayScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if !gameplay_reuses_phase(state.requested_phase()) {
            return Err(GameplayScreenError::RequestedPhase(state.requested_phase()));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(GameplayScreenError::Host)?;
        // The live records phase 12 edits replace the authored keyboard rows.
        let mut screen_tables = services.assets.gameplay_input_tables().clone();
        let mut sub_tables = services.assets.gameplay_sub_input_tables().clone();
        let executable =
            inf_formats::FrontendExecutable::parse(services.assets.executable_bytes())?;
        services.key_bindings.apply_to_gameplay_tables(
            &mut screen_tables,
            &mut sub_tables,
            executable,
        )?;
        self.double_tap_window = Some(executable.word(0x0053_a278)? as i32);
        self.bindings
            .install(services.input, &screen_tables, &devices)?;
        let context = self.bindings.context().expect("installed gameplay context");
        // 00440990 then installs the +0xFC sub-object over the first player
        // (00488AA0) into the same context, before selection.
        if let Some(player_context) = services.player_context {
            let mut player =
                FrontendBindings::new(player_context, services.assets.screen_analog_descriptors());
            let result = player.install_sub_object(services.input, context, &sub_tables, &devices);
            self.player_bindings = Some(player);
            result?;
        }
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => GameplayScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => GameplayScreenError::Input(error),
            })?;
        //440B20 obtains settings only after input selection. The constructor
        //Booleanizes +70;450370's raw restore policy is deliberately different.
        let detail = host.terrain_detail_setting()?;
        services.terrain.configure(u32::from(detail != 0))?;
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Destructor `00440BE0` (scalar deleting `00440B80`): the `+0x60` and
    /// `+0xFC` binding sets (`00439D50`), then the base `00438080`.
    pub fn destroy<D, H>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), GameplayScreenError<H>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.release_in_world_menu(input)?;
        // Teardown releases the +0x60 sets, then the +0xFC sub-object's.
        self.bindings.release_sets(input)?;
        if let Some(player) = self.player_bindings.as_mut() {
            player.release_sets(input)?;
            player.release_base(input)?;
        }
        if let Some(id) = self.id {
            if registry.screen(id)?.membership == ScreenMembership::Linked {
                registry.unlink_screen_membership(id)?;
            }
        }
        self.bindings.release_base(input)?;
        if let Some(id) = self.id {
            registry.remove_screen(id)?;
            self.id = None;
        }
        self.lifecycle = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Tick `00442B50` skeleton: device gates and inner worker stay future
    /// owners. Counts the tick and succeeds without world simulation. The
    /// session-level `TickCallback::Gameplay` (`004388D0` → `004875A0`) is a
    /// separate owner handled by the world backend.
    pub fn tick<H: GameplayTickHost>(
        &mut self,
        tick: SchedulerTick,
        input: &InputRuntime,
        _host: &mut H,
    ) -> Result<(), GameplayScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        // 00440D80 forwards both arguments to 004392B0: the age, then this
        // screen's analog menu axes (the shared cells: a keyboard binds none)
        // and the held repeat. While the screen serves phase 5 its own input
        // has those; retail has no phase-4 screen then.
        if self.in_world_menu.is_none() {
            let analog = self.bindings.analog_values(input)?;
            let commands = &mut self.in_world_menu_commands;
            let result = self.held_input.tick(tick, analog, &mut |_, action| {
                commands.extend(gameplay_held_command(action));
                Ok::<(), std::convert::Infallible>(())
            });
            match result {
                Ok(()) => {}
                Err(ScreenInputError::Dispatch(never)) => match never {},
                Err(_) => {
                    return Err(GameplayScreenError::Lifecycle {
                        expected: ScreenLifecycle::Ready,
                        actual: ScreenLifecycle::Stopped,
                    })
                }
            }
        }
        self.ticks += 1;
        Ok(())
    }

    /// InWorldMenuScreenReuse: retail replaces this screen with the phase-5
    /// screen `00443190` (its draw `004437A0` is this screen's `00442080`,
    /// plus H0 `0x10C` when no adapter counts, which a keyboard never
    /// allows), whose context holds only its own tables
    /// (`00522B48`, `00534418`, `00522C28`, `00522CE0`), so the player's
    /// rows go quiet and its base input repeats held keys. The port keeps
    /// this screen and gives it that input: a new context with the phase-5
    /// tables, selected, and a zeroed held mask and repeat.
    pub fn enter_in_world_menu(
        &mut self,
        input: &mut InputRuntime,
        assets: &FrontendAssets,
        devices: &[FrontendDevice],
    ) -> Result<(), GameplayScreenError<std::convert::Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if self.in_world_menu.is_some() {
            return Ok(());
        }
        let mut bindings = FrontendBindings::new(
            IN_WORLD_MENU_CALLBACK_CONTEXT,
            assets.screen_analog_descriptors(),
        );
        bindings.initialize(input)?;
        let installed = bindings.install(input, assets.phase5_input_tables(), devices);
        let context = bindings.context();
        self.in_world_menu = Some(InWorldMenuInput {
            bindings,
            input: ScreenInput::new(ScreenDispatchPolicy::Base, IN_WORLD_MENU_CALLBACK_CONTEXT),
        });
        (self.vehicle_menu_ec, self.team_menu_f0, self.scores_f8) = (0, 0, false);
        installed?;
        input.select_context(context)?;
        Ok(())
    }

    /// Back from phase 5: the phase-5 context's sets and base go, and this
    /// screen's own context is selected again.
    pub fn leave_in_world_menu(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<(), GameplayScreenError<std::convert::Infallible>> {
        if self.in_world_menu.is_none() {
            return Ok(());
        }
        self.release_in_world_menu(input)?;
        input.select_context(self.bindings.context())?;
        // Retail builds a new phase-4 screen here, its base input zeroed
        // (00437F90).
        self.held_input =
            ScreenInput::new(ScreenDispatchPolicy::Base, self.config.callback_context);
        (self.vehicle_menu_ec, self.team_menu_f0, self.scores_f8) = (0, 0, false);
        Ok(())
    }

    fn release_in_world_menu<E>(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<(), GameplayScreenError<E>> {
        if let Some(mut menu) = self.in_world_menu.take() {
            menu.bindings.release_sets(input)?;
            menu.bindings.release_base(input)?;
        }
        self.in_world_menu_commands.clear();
        Ok(())
    }

    /// Whether this screen carries the phase-5 screen's input.
    pub fn serves_in_world_menu(&self) -> bool {
        self.in_world_menu.is_some()
    }

    /// A phase-5 callback: a wrapper's press dispatches every held bit and
    /// restarts the 250 ms repeat (`00439430`); its release clears the bit.
    /// The menu key `00443700` stays the gameplay action it is.
    pub fn in_world_menu_callback(
        &mut self,
        event: InputCallback,
    ) -> Result<GameplayInputAction, GameplayCallbackError> {
        let menu = self
            .in_world_menu
            .as_mut()
            .ok_or(GameplayCallbackError::NotReady)?;
        if event.address == 0x0044_3700 {
            return Ok(if event.value == 0 {
                GameplayInputAction::NoAction
            } else {
                GameplayInputAction::MenuKey
            });
        }
        let Some(button) = in_world_menu_button(event.address) else {
            return Ok(GameplayInputAction::Unsupported {
                address: event.address,
                value: event.value,
            });
        };
        if event.value == 0 {
            menu.input.release(button);
            return Ok(GameplayInputAction::NoAction);
        }
        let commands = &mut self.in_world_menu_commands;
        menu.input
            .press(
                button,
                &mut |_, action| -> Result<(), std::convert::Infallible> {
                    commands.extend(in_world_menu_command(action));
                    Ok(())
                },
            )
            .map_err(|_| GameplayCallbackError::NotReady)?;
        Ok(GameplayInputAction::NoAction)
    }

    /// The phase-5 tick's `004392B0`: age, the analog direction, then the
    /// repeat, which dispatches every held bit each 100 ms after the first
    /// 250.
    pub fn tick_in_world_menu(
        &mut self,
        tick: SchedulerTick,
        input: &InputRuntime,
    ) -> Result<(), GameplayScreenError<std::convert::Infallible>> {
        let Some(menu) = self.in_world_menu.as_mut() else {
            return Ok(());
        };
        let analog = menu.bindings.analog_values(input)?;
        let commands = &mut self.in_world_menu_commands;
        let result = menu.input.tick(tick, analog, &mut |_,
                                                         action|
         -> Result<
            (),
            std::convert::Infallible,
        > {
            commands.extend(in_world_menu_command(action));
            Ok(())
        });
        match result {
            Ok(()) => Ok(()),
            Err(ScreenInputError::Dispatch(never)) => match never {},
            Err(_) => Err(GameplayScreenError::Lifecycle {
                expected: ScreenLifecycle::Ready,
                actual: ScreenLifecycle::Stopped,
            }),
        }
    }

    /// The phase-5 commands since the last call, in dispatch order.
    pub fn take_in_world_menu_commands(&mut self) -> Vec<InWorldMenuCommand> {
        std::mem::take(&mut self.in_world_menu_commands)
    }

    /// Draw `00442080` skeleton: succeeds without submitting world pixels.
    /// Terrain/model submission via `00488020` stays open.
    pub fn draw(&self) -> Result<(), GameplayDrawError> {
        if self.lifecycle != ScreenLifecycle::Ready {
            return Err(GameplayDrawError::NotReady);
        }
        Ok(())
    }

    fn require_lifecycle<E>(
        &self,
        expected: ScreenLifecycle,
    ) -> Result<(), GameplayScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(GameplayScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum GameplayDrawError {
    #[error("gameplay draw requires a Ready screen")]
    NotReady,
}

pub struct GameplayFactoryServices<'a> {
    pub terrain: crate::terrain_runtime::TerrainDetailContext<'a>,
    pub registry:
        &'a mut ScreenRegistry<crate::frontend_renderer::FrontendRenderContext, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    /// The live key records the constructor's binding tables hold.
    pub key_bindings: &'a crate::key_bindings::KeyBindings,
    /// The first player's callback context, when one exists.
    pub player_context: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_gates_pause_without_a_keyboard_and_clear_the_input_mode() {
        let mut mode = 1;
        assert!(gameplay_device_gates(false, false, &mut mode));
        assert_eq!(mode, 1);
        assert!(!gameplay_device_gates(true, false, &mut mode));
        assert_eq!(mode, 0);
    }

    /// A live joystick turns the Controls menu's Disabled into Analogue,
    /// keeps Digital, and alone still keeps play from pausing.
    #[test]
    fn a_live_joystick_latches_analogue_and_keeps_digital() {
        let mut mode = 0;
        assert!(!gameplay_device_gates(true, true, &mut mode));
        assert_eq!(mode, 1);
        mode = 2;
        assert!(!gameplay_device_gates(true, true, &mut mode));
        assert_eq!(mode, 2);
        assert!(!gameplay_device_gates(false, true, &mut mode));
        assert_eq!(mode, 2);
        assert!(!gameplay_device_gates(true, false, &mut mode));
        assert_eq!(mode, 0);
    }
    use crate::frontend_assets::FrontendAssets;
    use crate::frontend_bindings::FrontendDevice;
    use crate::input::InputRuntime;
    use crate::screen_registry::{ScreenAttachment, ScreenRegistry};
    use crate::session::SessionState;
    use std::path::Path;

    struct TestHost {
        keyboard: Option<crate::platform_input::SdlInput>,
        /// A joystick (type 2) at rest joins the keyboard.
        joystick: bool,
    }

    impl crate::gameplay_screen::GameplayTerrainSettings for TestHost {
        fn terrain_detail_setting(
            &mut self,
        ) -> Result<u32, crate::terrain_runtime::TerrainPolicyError> {
            Ok(0)
        }
    }

    impl GameplayFactoryHost for TestHost {
        type Error = std::convert::Infallible;
        fn enumerate_calibrated_devices(
            &mut self,
            input: &mut InputRuntime,
        ) -> Result<Vec<FrontendDevice>, Self::Error> {
            if self.keyboard.is_none() {
                self.keyboard =
                    Some(crate::platform_input::SdlInput::attach(input).expect("attach keyboard"));
            }
            let devices = self.keyboard.as_mut().expect("attached");
            if !self.joystick {
                return Ok(vec![devices.frontend_device()]);
            }
            devices
                .update(stick([0x8000, 0x8000]), input, &mut Discard)
                .unwrap();
            Ok(devices.frontend_devices(input, 10).unwrap())
        }
    }

    struct TickHost;
    impl GameplayTickHost for TickHost {
        type Error = std::convert::Infallible;
    }

    fn assets() -> FrontendAssets {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        FrontendAssets::open(&root).expect("retail Infest.exe")
    }

    #[test]
    fn gameplay_reuse_serves_phases_4_and_6() {
        assert!(gameplay_reuses_phase(SessionPhase::Gameplay));
        assert!(gameplay_reuses_phase(SessionPhase::Phase6));
        for phase in SessionPhase::ALL
            .iter()
            .copied()
            .filter(|phase| *phase != SessionPhase::Gameplay && *phase != SessionPhase::Phase6)
        {
            assert!(!gameplay_reuses_phase(phase), "{phase:?}");
        }
    }

    struct Discard;
    impl crate::input::InputHost for Discard {
        type Error = std::convert::Infallible;
        fn clock_micros(&mut self) -> Result<u64, Self::Error> {
            Ok(0)
        }
        fn callback(&mut self, _: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// 00440990 installs the +0xFC sub-object over the first player, so its
    /// rows share the player's consumers, and the rows carry the live records
    /// phase 12 edits.
    #[test]
    fn the_sub_object_binds_the_live_records_to_the_player_consumers() {
        const PLAYER: u32 = 0x8000_0000;
        const THRUST: u32 = 0x0053_a378;
        let assets = assets();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(
                crate::frontend_renderer::FrontendRenderContext {
                    projection: crate::frontend_scene::SceneProjection {
                        focal: [320, 240],
                        viewport: [640, 480],
                        fade: [0, 0],
                    },
                },
            ),
        };
        let mut input = InputRuntime::new(0);
        // The player constructor acquires its references first.
        let raw = inf_formats::parse_digital_action_descriptor(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            THRUST,
        )
        .unwrap();
        let thrust = input
            .acquire_consumer(
                crate::input::ActionDescriptor {
                    identity: raw.identity,
                    callback_address: (raw.callback_address != 0).then_some(raw.callback_address),
                    flags: raw.flags,
                },
                PLAYER,
            )
            .unwrap();
        let mut bindings = crate::key_bindings::KeyBindings::retail_initial();
        // Action 4 is Thrust (Space); rebind it to K.
        bindings.set_codes(4, [0x25, 0, 0, 0]).unwrap();
        let mut screen = GameplayScreen::new(
            GameplayScreenConfig {
                attachment,
                callback_context: GAMEPLAY_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Gameplay);
        let mut host = TestHost {
            keyboard: None,
            joystick: false,
        };
        screen
            .initialize(
                &mut GameplayFactoryServices {
                    terrain: crate::terrain_runtime::TerrainDetailContext {
                        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                            crate::terrain_runtime::TerrainSpacing::Units256,
                        ),
                        world_flags: Some(0x20),
                        terrain: Some(&mut crate::terrain_runtime::native_probe_fixture_terrain(
                            &serde_json::json!({
                                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                                "xz":[0,0],"heights":[0,0,0,0],"alternate":false
                            }),
                        )),
                    },
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                    key_bindings: &bindings,
                    player_context: Some(PLAYER),
                },
                &mut host,
                &mut state,
            )
            .unwrap();
        let keyboard = host.keyboard.as_mut().unwrap();
        let mut press = |key: &str, pressed: bool, input: &mut InputRuntime| {
            keyboard
                .update(
                    crate::platform_input::InputNotification::Key {
                        scancode: sdl2::keyboard::Scancode::from_name(key).unwrap(),
                        pressed,
                    },
                    input,
                    &mut Discard,
                )
                .unwrap();
            // The edge publishes when the queue is polled.
            input.poll(&mut Discard).unwrap();
        };
        press("Space", true, &mut input);
        assert!(!input.held(thrust).unwrap());
        press("Space", false, &mut input);
        press("K", true, &mut input);
        assert!(input.held(thrust).unwrap());
        press("K", false, &mut input);
        assert!(!input.held(thrust).unwrap());
        screen
            .destroy::<_, std::convert::Infallible>(&mut registry, &mut input)
            .unwrap();
    }

    /// A joystick reading with the stick at X/Y, the other axes centred.
    fn stick([x, y]: [u32; 2]) -> crate::platform_input::InputNotification {
        crate::platform_input::InputNotification::Pad {
            native_type: 2,
            axes: 5,
            sample: crate::platform_input::PadSample::Read {
                buttons: 0,
                axes: [x, y, 0x8000, 0x8000, 0x8000, 0],
            },
        }
    }

    /// 00440D80's base input on the gameplay screen: the joystick's menu axes
    /// past 2500 send the message codes of slots +28..+34 (00443760..90),
    /// Y before X, once at the push and each 100 ms after the first 250.
    #[test]
    fn the_joysticks_menu_axes_reach_the_players_message() {
        let assets = assets();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(
                crate::frontend_renderer::FrontendRenderContext {
                    projection: crate::frontend_scene::SceneProjection {
                        focal: [320, 240],
                        viewport: [640, 480],
                        fade: [0, 0],
                    },
                },
            ),
        };
        let mut input = InputRuntime::new(0);
        let mut screen = GameplayScreen::new(
            GameplayScreenConfig {
                attachment,
                callback_context: GAMEPLAY_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Gameplay);
        let mut host = TestHost {
            keyboard: None,
            joystick: true,
        };
        screen
            .initialize(
                &mut GameplayFactoryServices {
                    terrain: crate::terrain_runtime::TerrainDetailContext {
                        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                            crate::terrain_runtime::TerrainSpacing::Units256,
                        ),
                        world_flags: Some(0x20),
                        terrain: Some(&mut crate::terrain_runtime::native_probe_fixture_terrain(
                            &serde_json::json!({
                                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                                "xz":[0,0],"heights":[0,0,0,0],"alternate":false
                            }),
                        )),
                    },
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                    key_bindings: &crate::key_bindings::KeyBindings::retail_initial(),
                    player_context: None,
                },
                &mut host,
                &mut state,
            )
            .unwrap();
        let ms = |millis: i32| SchedulerTick {
            delta_micros: millis * 1000,
            elapsed_micros: millis * 1000,
        };
        let mut push = |axes: [u32; 2], input: &mut InputRuntime| {
            host.keyboard
                .as_mut()
                .unwrap()
                .update(stick(axes), input, &mut Discard)
                .unwrap();
            input.poll(&mut Discard).unwrap();
        };
        let codes = |screen: &mut GameplayScreen| -> Vec<u32> {
            screen
                .take_in_world_menu_commands()
                .into_iter()
                .map(|command| match command {
                    InWorldMenuCommand::Code(code) => code,
                    InWorldMenuCommand::Back => panic!("no Back slot"),
                })
                .collect()
        };
        screen.tick(ms(8), &input, &mut TickHost).unwrap();
        assert_eq!(codes(&mut screen), Vec::<u32>::new());
        for (axes, code) in [
            ([0x8000, 0x0000], 0),
            ([0x8000, 0xffff], 1),
            ([0x0000, 0x8000], 2),
            ([0xffff, 0x8000], 3),
        ] {
            push(axes, &mut input);
            let analog = screen.bindings.analog_values(&input).unwrap();
            assert!(analog.consumer_d0.abs() > 2500 || analog.consumer_cc.abs() > 2500);
            screen.tick(ms(8), &input, &mut TickHost).unwrap();
            assert_eq!(codes(&mut screen), [code], "{axes:x?} {analog:?}");
            screen.tick(ms(200), &input, &mut TickHost).unwrap();
            assert_eq!(codes(&mut screen), Vec::<u32>::new());
            screen.tick(ms(50), &input, &mut TickHost).unwrap();
            assert_eq!(codes(&mut screen), [code]);
            push([0x8000, 0x8000], &mut input);
            screen.tick(ms(8), &input, &mut TickHost).unwrap();
            assert_eq!(codes(&mut screen), Vec::<u32>::new());
        }
    }

    #[test]
    fn gameplay_tick_counts_without_simulation() {
        let assets = assets();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(
                crate::frontend_renderer::FrontendRenderContext {
                    projection: crate::frontend_scene::SceneProjection {
                        focal: [320, 240],
                        viewport: [640, 480],
                        fade: [0, 0],
                    },
                },
            ),
        };
        let mut input = InputRuntime::new(0);
        let mut screen = GameplayScreen::new(
            GameplayScreenConfig {
                attachment,
                callback_context: GAMEPLAY_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Gameplay);
        let mut host = TestHost {
            keyboard: None,
            joystick: false,
        };
        screen
            .initialize(
                &mut GameplayFactoryServices {
                    terrain: crate::terrain_runtime::TerrainDetailContext {
                        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                            crate::terrain_runtime::TerrainSpacing::Units256,
                        ),
                        world_flags: Some(0x20),
                        terrain: Some(&mut crate::terrain_runtime::native_probe_fixture_terrain(
                            &serde_json::json!({
                                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                                "xz":[0,0],"heights":[0,0,0,0],"alternate":false
                            }),
                        )),
                    },
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                    key_bindings: &crate::key_bindings::KeyBindings::retail_initial(),
                    player_context: None,
                },
                &mut host,
                &mut state,
            )
            .unwrap();
        let tick = SchedulerTick {
            delta_micros: 8_000,
            elapsed_micros: 8_000,
        };
        let mut tick_host = TickHost;
        screen.tick(tick, &input, &mut tick_host).unwrap();
        screen.tick(tick, &input, &mut tick_host).unwrap();
        assert_eq!(screen.ticks(), 2);
        assert_eq!(screen.screen_age_millis(), 16);
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/gameplay_overlay_native.json"
        ))
        .unwrap();
        for case in native["clock_cases"].as_array().unwrap() {
            screen
                .held_input
                .set_age_millis(case["age"].as_u64().unwrap() as i32);
            screen
                .tick(
                    SchedulerTick {
                        delta_micros: 125000,
                        elapsed_micros: case["elapsed"].as_i64().unwrap() as i32,
                    },
                    &input,
                    &mut tick_host,
                )
                .unwrap();
            assert_eq!(
                screen.screen_age_millis() as u32,
                case["result"].as_u64().unwrap() as u32
            );
        }
        assert!(screen.draw().is_ok());
        let callback = |address, value| InputCallback {
            address,
            value,
            context: GAMEPLAY_CALLBACK_CONTEXT,
            timestamp_micros: 0,
        };
        assert_eq!(
            screen.digital_callback(callback(0x0044_3700, 1)).unwrap(),
            GameplayInputAction::MenuKey
        );
        assert_eq!(
            screen.digital_callback(callback(0x0044_3700, 0)).unwrap(),
            GameplayInputAction::NoAction
        );
        // F12: a press flips First Person, a release does nothing. A value
        // above 0 becomes 0, any other 1.
        assert_eq!(
            [0, 1, 2, 0x8000_0000].map(first_person_toggled),
            [1, 0, 0, 1]
        );
        assert_eq!(
            screen.digital_callback(callback(0x0044_3650, 1)).unwrap(),
            GameplayInputAction::FirstPerson
        );
        assert_eq!(
            screen.digital_callback(callback(0x0044_3650, 0)).unwrap(),
            GameplayInputAction::NoAction
        );
        // U flips the HUD (+60) with sete; M and Shift+Esc reach the map and
        // frontend requests; Alt+N skips to world (current mod 33) + 1 and
        // Alt+F3 flips terrain detail (+70). Releases do nothing.
        assert_eq!([0, 1, 2].map(hud_toggled), [1, 0, 0]);
        assert_eq!([0, 1, 2].map(terrain_detail_toggled), [1, 0, 0]);
        assert_eq!(
            [1, 21, 32, 33, 34].map(next_world_selection),
            [2, 22, 33, 1, 2]
        );
        for (address, action) in [
            (0x0044_3680, GameplayInputAction::Hud),
            (0x0044_36d0, GameplayInputAction::Map),
            (0x0044_3470, GameplayInputAction::ReturnToFrontend),
            (0x0044_3720, GameplayInputAction::NextWorld),
            (0x0044_36b0, GameplayInputAction::TerrainDetail),
            (
                0x0044_35f0,
                GameplayInputAction::NetworkMenu {
                    address: 0x0044_35f0,
                },
            ),
            (
                0x0044_3620,
                GameplayInputAction::NetworkMenu {
                    address: 0x0044_3620,
                },
            ),
        ] {
            assert_eq!(
                screen.digital_callback(callback(address, 1)).unwrap(),
                action
            );
            assert_eq!(
                screen.digital_callback(callback(address, 0)).unwrap(),
                GameplayInputAction::NoAction
            );
        }
        assert_eq!(
            screen.digital_callback(callback(0x0044_3760, 1)).unwrap(),
            GameplayInputAction::Unsupported {
                address: 0x0044_3760,
                value: 1
            }
        );
        let mut wrong = callback(0x0044_3700, 1);
        wrong.context += 1;
        assert!(matches!(
            screen.digital_callback(wrong),
            Err(GameplayCallbackError::Context { .. })
        ));
    }

    struct Collect(Vec<InputCallback>);
    impl crate::input::InputHost for Collect {
        type Error = std::convert::Infallible;
        fn clock_micros(&mut self) -> Result<u64, Self::Error> {
            Ok(0)
        }
        fn callback(
            &mut self,
            event: InputCallback,
            _: &mut InputRuntime,
        ) -> Result<(), Self::Error> {
            self.0.push(event);
            Ok(())
        }
    }

    /// Reported 2026-10-19: holding Right on the in-world
    /// Options' volume moved it once. Retail's phase-5 screen presses its
    /// held bits through 00439430 and repeats them in 004392B0, every 100 ms
    /// after the first 250 (the lab: 16 adjust cues over 16 held passes).
    #[test]
    fn the_in_world_menu_repeats_held_keys_as_the_phase_5_screen_does() {
        let assets = assets();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(
                crate::frontend_renderer::FrontendRenderContext {
                    projection: crate::frontend_scene::SceneProjection {
                        focal: [320, 240],
                        viewport: [640, 480],
                        fade: [0, 0],
                    },
                },
            ),
        };
        let mut input = InputRuntime::new(0);
        let mut screen = GameplayScreen::new(
            GameplayScreenConfig {
                attachment,
                callback_context: GAMEPLAY_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Gameplay);
        let mut host = TestHost {
            keyboard: None,
            joystick: false,
        };
        screen
            .initialize(
                &mut GameplayFactoryServices {
                    terrain: crate::terrain_runtime::TerrainDetailContext {
                        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                            crate::terrain_runtime::TerrainSpacing::Units256,
                        ),
                        world_flags: Some(0x20),
                        terrain: Some(&mut crate::terrain_runtime::native_probe_fixture_terrain(
                            &serde_json::json!({
                                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                                "xz":[0,0],"heights":[0,0,0,0],"alternate":false
                            }),
                        )),
                    },
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                    key_bindings: &crate::key_bindings::KeyBindings::retail_initial(),
                    player_context: None,
                },
                &mut host,
                &mut state,
            )
            .unwrap();
        let devices = host.enumerate_calibrated_devices(&mut input).unwrap();
        screen
            .enter_in_world_menu(&mut input, &assets, &devices)
            .unwrap();
        assert!(screen.serves_in_world_menu());
        let keyboard = host.keyboard.as_mut().unwrap();
        let mut key =
            |name: &str, pressed: bool, input: &mut InputRuntime, screen: &mut GameplayScreen| {
                let mut events = Collect(Vec::new());
                keyboard
                    .update(
                        crate::platform_input::InputNotification::Key {
                            scancode: sdl2::keyboard::Scancode::from_name(name).unwrap(),
                            pressed,
                        },
                        input,
                        &mut events,
                    )
                    .unwrap();
                input.poll(&mut events).unwrap();
                events
                    .0
                    .into_iter()
                    .map(|event| {
                        assert_eq!(event.context, IN_WORLD_MENU_CALLBACK_CONTEXT);
                        screen.in_world_menu_callback(event).unwrap()
                    })
                    .collect::<Vec<_>>()
            };
        let tick = |screen: &mut GameplayScreen, input: &InputRuntime| {
            screen
                .tick_in_world_menu(
                    SchedulerTick {
                        delta_micros: 100_000,
                        elapsed_micros: 100_000,
                    },
                    input,
                )
                .unwrap();
            screen.take_in_world_menu_commands()
        };
        let adjust = vec![InWorldMenuCommand::Code(3)];
        key("Right", true, &mut input, &mut screen);
        assert_eq!(screen.take_in_world_menu_commands(), adjust);
        // 250 ms before the first repeat, then one each 100 ms.
        assert!(tick(&mut screen, &input).is_empty());
        assert!(tick(&mut screen, &input).is_empty());
        assert_eq!(tick(&mut screen, &input), adjust);
        assert_eq!(tick(&mut screen, &input), adjust);
        key("Right", false, &mut input, &mut screen);
        assert!(tick(&mut screen, &input).is_empty());
        // Up is Previous (code 0), Return the select (4), Escape the Back.
        key("Up", true, &mut input, &mut screen);
        assert_eq!(
            screen.take_in_world_menu_commands(),
            [InWorldMenuCommand::Code(0)]
        );
        key("Up", false, &mut input, &mut screen);
        key("Escape", true, &mut input, &mut screen);
        assert_eq!(
            screen.take_in_world_menu_commands(),
            [InWorldMenuCommand::Back]
        );
        key("Escape", false, &mut input, &mut screen);
        // P is the menu key 00443700, the phase-5 screen's +64.
        assert_eq!(
            key("P", true, &mut input, &mut screen),
            [GameplayInputAction::MenuKey]
        );
        key("P", false, &mut input, &mut screen);
        screen.leave_in_world_menu(&mut input).unwrap();
        assert!(!screen.serves_in_world_menu());
        screen
            .destroy::<_, std::convert::Infallible>(&mut registry, &mut input)
            .unwrap();
    }
}
