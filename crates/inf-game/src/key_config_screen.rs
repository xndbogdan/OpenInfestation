//! Phase-12 key configuration screen: factory 00444AC0, constructor
//! 00444BD0, vtable 00524DC4, destructor 00444D70/00444D90.
//!
//! Native lifecycle, recovered from the supported retail `Infest.exe` and the
//! bounded Infest01 visit from Options > Controls (see the phase-12 section
//! of [frontend runtime](../../../docs/re/FRONTEND_RUNTIME.md)):
//!
//! - Constructor `00444BD0` builds the `0x130`-byte screen over base
//!   `00437F90` and a menu controller `00453380(FFFF, 0, 0, 400, 400)` at
//!   `+EC`. It clears the capture byte `+120`, stores the session's current
//!   phase `+64C` at `+12C` (the phase to return to), enumerates its devices
//!   over the phase-12 tables, selects the context and pushes the root menu
//!   `00535E40`.
//! - Held slots: `+24` `00445120` activates, `+28/+30` `00445190` move
//!   up, `+2C/+34` `004451B0` move down, all only outside a capture. `+38`
//!   `00445140` pops a page, or on the root requests the phase at `+12C`.
//! - Row action `00444B20` starts a capture (`004450F0`): capture byte
//!   `+120`, release wait `+128` clear, row `+124`, row capture byte set.
//!   `00444B50` restores the default records; `00453BE0`/`00453C50` push and
//!   pop the two further pages.
//! - Tick `00444E80` runs base input, then the capture: it waits until no
//!   key and no joystick button is held, then takes the next press. Keys
//!   become the record's required codes; a joystick button takes the
//!   action's descriptor. It then advances the shared clock `00439560` and
//!   ticks the menu controller.
//! - Teardown `00444D90` releases the binding sets, the menu controller
//!   (`00453400`) and the base. Bindings persist with the settings
//!   (`00463100` calls `00445570`), which the rebuilt frontend does.
//!
//! The keyboard rows press the screen's own held bits (`00445200..00445280`
//! and back `004451E0` on Esc and G); the joystick buttons press Confirm.

use thiserror::Error;

use crate::frontend_assets::{FrontendAssetError, FrontendAssets};
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{
    DeviceButtons, DeviceId, InputCallback, InputContextId, InputRuntime, InputStateError,
};
use crate::key_bindings::{KeyBindingError, KeyBindingTables, KeyBindings, KEY_BINDING_ROWS};
use crate::menu::{
    MenuActionRequest, MenuBasis, MenuCommand, MenuControllerSpec, MenuData, MenuError, MenuHost,
    MenuInput, MenuStack, MenuState, MenuVisibility,
};
use crate::menu_basis::MenuBasisMath;
use crate::scheduler::SchedulerTick;
use crate::screen_input::{
    ScreenButton, ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError,
};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::{SessionPhase, SessionState};

/// Vtable `00524DC4` slot `+18` (`00444D50`).
pub const KEY_CONFIG_CLASS_ID: u8 = 9;
/// Opaque callback identity, distinct from every other screen context.
pub const KEY_CONFIG_CALLBACK_CONTEXT: u32 = 8;
/// Root page pushed by the constructor.
pub const KEY_CONFIG_MENU_ADDRESS: u32 = 0x0053_5e40;
/// The two pages `00453BE0` pushes.
pub const KEY_CONFIG_PAGE_ADDRESSES: [u32; 2] = [0x0053_5b70, 0x0053_5b88];

/// Row actions of the three pages.
pub const START_CAPTURE_ACTION: u32 = 0x0044_4b20;
pub const RESTORE_DEFAULTS_ACTION: u32 = 0x0044_4b50;
pub const PUSH_PAGE_ACTION: u32 = 0x0045_3be0;
pub const POP_PAGE_ACTION: u32 = 0x0045_3c50;

/// `00444D60`: the screen serves only requested phase 12.
pub fn key_config_reuses_phase(requested: SessionPhase) -> bool {
    requested == SessionPhase::Phase12
}

#[derive(Clone, Copy, Debug)]
pub struct KeyConfigScreenConfig {
    pub attachment: ScreenAttachment,
    pub callback_context: u32,
}

/// Capture bytes `+120`, `+128` and row `+124`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCapture {
    pub row: usize,
    /// `+128`: every key and button has been seen released.
    pub armed: bool,
}

/// The three authored pages, parsed once.
#[derive(Clone, Debug)]
pub struct KeyConfigTemplates {
    root: MenuData,
    pages: [MenuData; 2],
}

impl KeyConfigTemplates {
    pub fn parse(assets: &FrontendAssets) -> Result<Self, FrontendAssetError> {
        Ok(Self {
            root: MenuData::new(assets.menu_definition(KEY_CONFIG_MENU_ADDRESS)?),
            pages: [
                MenuData::new(assets.menu_definition(KEY_CONFIG_PAGE_ADDRESSES[0])?),
                MenuData::new(assets.menu_definition(KEY_CONFIG_PAGE_ADDRESSES[1])?),
            ],
        })
    }

    fn page(&self, address: u32) -> Option<&MenuData> {
        KEY_CONFIG_PAGE_ADDRESSES
            .iter()
            .position(|candidate| *candidate == address)
            .map(|index| &self.pages[index])
    }
}

pub struct KeyConfigFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
}

pub trait KeyConfigFactoryHost {
    type Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

/// The owners a tick or callback reaches.
pub trait KeyConfigEffects {
    /// The live records and their authored tables (settings custody).
    fn bindings(&mut self) -> (&KeyBindingTables, &mut KeyBindings);
    /// `00438380`, first-request-wins.
    fn request_phase(&mut self, phase: SessionPhase);
    /// `00439560` with the scheduler delta.
    fn advance_animation(&mut self, delta_micros: u32);
    /// `00453360`: gameplay owner `+5C2`.
    fn visibility(&self) -> MenuVisibility;
    fn basis(&self) -> &MenuBasisMath;
}

#[derive(Debug, Error)]
pub enum KeyConfigScreenError<E> {
    #[error("key configuration requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("00444AC0 serves requested phase 12, not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    Assets(#[from] FrontendAssetError),
    #[error(transparent)]
    KeyBindings(#[from] KeyBindingError),
    #[error("key configuration menu: {0}")]
    Menu(String),
    #[error("key configuration base input stopped: {0}")]
    ScreenInput(String),
    #[error("key configuration callback context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("key configuration input callback {0:#010x} has no owner")]
    UnknownCallback(u32),
    #[error("key configuration row action {0:#010x} has no owner")]
    UnknownAction(u32),
    #[error("key configuration row {0} is outside the 44 authored rows")]
    Row(u32),
    #[error("{0} keys held; 004F5230 overruns its 4-byte buffer natively")]
    CaptureOverrun(u32),
    #[error("required key configuration effect failed")]
    Host(E),
}

#[derive(Debug)]
pub struct KeyConfigScreen {
    config: KeyConfigScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    input: ScreenInput,
    menus: Option<MenuStack>,
    templates: Option<KeyConfigTemplates>,
    return_phase: SessionPhase,
    capture: Option<KeyCapture>,
    /// The row table's `+4` capture bytes, drawn as the blinking prompt.
    capture_rows: [bool; KEY_BINDING_ROWS],
}

impl KeyConfigScreen {
    pub fn new(config: KeyConfigScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            input: ScreenInput::new(ScreenDispatchPolicy::Base, config.callback_context),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            menus: None,
            templates: None,
            return_phase: SessionPhase::Frontend,
            capture: None,
            capture_rows: [false; KEY_BINDING_ROWS],
        }
    }

    pub fn lifecycle(&self) -> ScreenLifecycle {
        self.lifecycle
    }
    pub fn id(&self) -> Option<ScreenId> {
        self.id
    }
    pub fn bindings(&self) -> &FrontendBindings {
        &self.bindings
    }
    pub fn screen_input(&self) -> &ScreenInput {
        &self.input
    }
    pub fn menus(&self) -> Option<&MenuStack> {
        self.menus.as_ref()
    }
    pub(crate) fn menus_mut(&mut self) -> Option<&mut MenuStack> {
        self.menus.as_mut()
    }
    pub fn return_phase(&self) -> SessionPhase {
        self.return_phase
    }
    pub fn capture(&self) -> Option<KeyCapture> {
        self.capture
    }
    pub fn row_capturing(&self, row: usize) -> bool {
        self.capture_rows.get(row).copied().unwrap_or(false)
    }

    /// Constructor `00444BD0` in native order.
    pub fn initialize<D, H: KeyConfigFactoryHost>(
        &mut self,
        services: &mut KeyConfigFactoryServices<'_, D>,
        host: &mut H,
        state: &SessionState,
    ) -> Result<(), KeyConfigScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if !key_config_reuses_phase(state.requested_phase()) {
            return Err(KeyConfigScreenError::RequestedPhase(
                state.requested_phase(),
            ));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        self.menus = Some(MenuStack::new(MenuControllerSpec {
            fade_rate: 65535,
            viewport: [0, 0, 0x400, 0x400],
        }));
        // 00444C1E: session +64C, still the previous phase mid-transition.
        self.return_phase = state.current_phase();
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(KeyConfigScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.phase12_input_tables(),
            &devices,
        )?;
        let context = self
            .bindings
            .context()
            .expect("installed key config context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => KeyConfigScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => KeyConfigScreenError::Input(error),
            })?;
        let templates = KeyConfigTemplates::parse(services.assets)?;
        self.menus
            .as_mut()
            .expect("constructed controller")
            .push(MenuState::new(templates.root.clone(), MenuBasis::INITIAL));
        self.templates = Some(templates);
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Teardown `00444D90`: release binding sets, the menu controller and
    /// the base. The records stay with their settings owner.
    pub fn destroy<D>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), KeyConfigScreenError<std::convert::Infallible>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        self.menus = None;
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

    /// Tick `00444E80`. `keyboard` is adapter slot 0 (`00439610(0)`); the
    /// port has no joystick adapter in slot 2.
    pub fn tick<E: KeyConfigEffects>(
        &mut self,
        input: &InputRuntime,
        tick: SchedulerTick,
        keyboard: Option<DeviceId>,
        effects: &mut E,
    ) -> Result<(), KeyConfigScreenError<std::convert::Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        let result = self.tick_inner(input, tick, keyboard, effects);
        if result.is_err() {
            self.lifecycle = ScreenLifecycle::Stopped;
        }
        result
    }

    fn tick_inner<E: KeyConfigEffects>(
        &mut self,
        input: &InputRuntime,
        tick: SchedulerTick,
        keyboard: Option<DeviceId>,
        effects: &mut E,
    ) -> Result<(), KeyConfigScreenError<std::convert::Infallible>> {
        let analog = self.bindings.analog_values(input)?;
        let mut held = HeldParts {
            menus: self.menus.as_mut().expect("ready controller"),
            templates: self.templates.as_ref().expect("ready templates"),
            capture: &mut self.capture,
            capture_rows: &mut self.capture_rows,
            return_phase: self.return_phase,
            context: self.config.callback_context,
        };
        let mut failure = None;
        let result = self.input.tick(tick, analog, &mut |_, action| {
            held.dispatch(action, effects).map_err(|error| {
                failure = Some(error);
            })
        });
        if let Err(error) = result {
            return Err(failure.unwrap_or_else(|| {
                KeyConfigScreenError::ScreenInput(screen_input_message(&error))
            }));
        }
        if let Some(capture) = self.capture {
            let keys = match keyboard {
                Some(device) => input.device_buttons(device)?,
                None => DeviceButtons::default(),
            };
            let any = keys.0.iter().any(|word| *word != 0);
            if !capture.armed {
                if !any {
                    self.capture = Some(KeyCapture {
                        armed: true,
                        ..capture
                    });
                }
            } else if any {
                let codes = list_codes(keys)?;
                let (tables, bindings) = effects.bindings();
                let action = tables.rows[capture.row].action_index;
                bindings.set_codes(action, codes)?;
                self.capture_rows[capture.row] = false;
                self.capture = None;
            }
        }
        effects.advance_animation(tick.delta_micros as u32);
        let mut host = ActionHost {
            templates: self.templates.as_ref().expect("ready templates"),
            capture: &mut self.capture,
            capture_rows: &mut self.capture_rows,
            effects,
        };
        self.menus
            .as_mut()
            .expect("ready controller")
            .tick(tick.delta_micros, &mut host)
            .map_err(menu_error)?;
        Ok(())
    }

    /// Held-bit callbacks queued for this screen's context; system chords
    /// belong to the router.
    pub fn digital_callback<E: KeyConfigEffects>(
        &mut self,
        event: InputCallback,
        effects: &mut E,
    ) -> Result<(), KeyConfigScreenError<std::convert::Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if event.context != self.config.callback_context {
            return Err(KeyConfigScreenError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        let button = ScreenButton::from_callback(event.address)
            .filter(|button| {
                matches!(
                    button,
                    ScreenButton::Confirm
                        | ScreenButton::Previous
                        | ScreenButton::Next
                        | ScreenButton::AdjustPrevious
                        | ScreenButton::AdjustNext
                        | ScreenButton::Back
                )
            })
            .ok_or(KeyConfigScreenError::UnknownCallback(event.address))?;
        if event.value == 0 {
            self.input.release(button);
            return Ok(());
        }
        let mut held = HeldParts {
            menus: self.menus.as_mut().expect("ready controller"),
            templates: self.templates.as_ref().expect("ready templates"),
            capture: &mut self.capture,
            capture_rows: &mut self.capture_rows,
            return_phase: self.return_phase,
            context: self.config.callback_context,
        };
        let mut failure = None;
        let result = self.input.press(button, &mut |_, action| {
            held.dispatch(action, effects).map_err(|error| {
                failure = Some(error);
            })
        });
        if let Err(error) = result {
            self.lifecycle = ScreenLifecycle::Stopped;
            return Err(failure.unwrap_or_else(|| {
                KeyConfigScreenError::ScreenInput(screen_input_message(&error))
            }));
        }
        Ok(())
    }

    fn require_lifecycle<E>(
        &self,
        expected: ScreenLifecycle,
    ) -> Result<(), KeyConfigScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(KeyConfigScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

/// The borrowed screen parts the `00439470` dispatch reaches.
struct HeldParts<'a> {
    menus: &'a mut MenuStack,
    templates: &'a KeyConfigTemplates,
    capture: &'a mut Option<KeyCapture>,
    capture_rows: &'a mut [bool; KEY_BINDING_ROWS],
    return_phase: SessionPhase,
    context: u32,
}

impl HeldParts<'_> {
    /// `00445120/00445190/004451B0/00445140`: nothing runs during a capture.
    fn dispatch<E: KeyConfigEffects>(
        &mut self,
        action: ScreenInputAction,
        effects: &mut E,
    ) -> Result<(), KeyConfigScreenError<std::convert::Infallible>> {
        if self.capture.is_some() {
            return Ok(());
        }
        let command = match action {
            ScreenInputAction::Confirm => MenuCommand::Activate,
            ScreenInputAction::Previous | ScreenInputAction::AdjustPrevious => {
                MenuCommand::Previous
            }
            ScreenInputAction::Next | ScreenInputAction::AdjustNext => MenuCommand::Next,
            ScreenInputAction::Back => {
                // 00453540 counts the controller's menus.
                if self.menus.len() > 1 {
                    self.menus.pop(1);
                } else {
                    effects.request_phase(self.return_phase);
                }
                return Ok(());
            }
            ScreenInputAction::Auxiliary6
            | ScreenInputAction::Auxiliary7
            | ScreenInputAction::RequestAttract
            | ScreenInputAction::ResultsGate
            | ScreenInputAction::Bit200 => return Ok(()),
        };
        let visibility = effects.visibility();
        let mut host = ActionHost {
            templates: self.templates,
            capture: self.capture,
            capture_rows: self.capture_rows,
            effects,
        };
        self.menus
            .command(
                MenuInput {
                    command,
                    visibility,
                    context: self.context,
                },
                &mut host,
            )
            .map_err(menu_error)?;
        Ok(())
    }
}

/// Row actions of the three pages.
struct ActionHost<'a, E> {
    templates: &'a KeyConfigTemplates,
    capture: &'a mut Option<KeyCapture>,
    capture_rows: &'a mut [bool; KEY_BINDING_ROWS],
    effects: &'a mut E,
}

impl<E: KeyConfigEffects> MenuHost for ActionHost<'_, E> {
    type Error = KeyConfigScreenError<std::convert::Infallible>;

    fn action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        if request.command != MenuCommand::Activate {
            return match request.callback_address {
                START_CAPTURE_ACTION
                | RESTORE_DEFAULTS_ACTION
                | PUSH_PAGE_ACTION
                | POP_PAGE_ACTION => Ok(()),
                address => Err(KeyConfigScreenError::UnknownAction(address)),
            };
        }
        match request.callback_address {
            START_CAPTURE_ACTION => {
                // 004450F0: only outside a capture.
                let row = request.arguments[0];
                if usize::try_from(row).map_or(true, |row| row >= KEY_BINDING_ROWS) {
                    return Err(KeyConfigScreenError::Row(row));
                }
                if self.capture.is_none() {
                    *self.capture = Some(KeyCapture {
                        row: row as usize,
                        armed: false,
                    });
                    self.capture_rows[row as usize] = true;
                }
            }
            RESTORE_DEFAULTS_ACTION => {
                let (tables, bindings) = self.effects.bindings();
                bindings.restore_defaults(tables);
            }
            PUSH_PAGE_ACTION => {
                let page = self
                    .templates
                    .page(request.arguments[1])
                    .ok_or(KeyConfigScreenError::UnknownAction(request.arguments[1]))?;
                stack.push(MenuState::new(page.clone(), MenuBasis::INITIAL));
            }
            POP_PAGE_ACTION => stack.pop(1),
            address => return Err(KeyConfigScreenError::UnknownAction(address)),
        }
        Ok(())
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.effects.basis().rotate(basis, angle);
        Ok(())
    }
}

/// `004F5230` with capacity 4: every held code, ascending, then zeros. A
/// fifth held key overruns the buffer natively and stops the port.
pub fn list_codes(
    keys: DeviceButtons,
) -> Result<[u8; 4], KeyConfigScreenError<std::convert::Infallible>> {
    let held: u32 = keys.0.iter().map(|word| word.count_ones()).sum();
    if held > 4 {
        return Err(KeyConfigScreenError::CaptureOverrun(held));
    }
    let mut codes = [0; 4];
    let mut next = 0;
    for (word, bits) in keys.0.iter().enumerate() {
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                codes[next] = (word * 32 + bit) as u8;
                next += 1;
            }
        }
    }
    Ok(codes)
}

fn menu_error(
    error: MenuError<KeyConfigScreenError<std::convert::Infallible>>,
) -> KeyConfigScreenError<std::convert::Infallible> {
    match error {
        MenuError::Host(error) => error,
        other => KeyConfigScreenError::Menu(other.to_string()),
    }
}

fn screen_input_message(error: &ScreenInputError<()>) -> String {
    error.to_string()
}

#[cfg(test)]
#[path = "key_config_screen_tests.rs"]
mod tests;
