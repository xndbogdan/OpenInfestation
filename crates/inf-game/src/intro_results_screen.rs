//! The phase-8 results screen, after the intro and after every world.
//!
//! Factory 0042A970 allocates 0x154; constructor 0042AB00 uses base 00437F90,
//! publishes 00521724 and installs one authored digital device set. Tick
//! 0042CFC0 requests phase 3 at its head when the current world +250 is 22,
//! then falls through: the profile menu's tick, the base input, the timers,
//! the save countdown +EC, the three panels, their construction once the
//! stats are shown (+F2), and the exit countdown +148, which requests the
//! briefing (or the frontend with +F5). Space or G shows the stats and then
//! arms the exit (0042AA60); S starts the save (0042AAA0); Esc arms the exit
//! through held bit 0x200 (0042D510). Draw 0042C2B0's state changes run each
//! frame through [`IntroResultsScreen::draw_effects`]: the panels build each
//! other, panel 1 scores and panel 2 awards the medal. Backspace (0042D3F0)
//! or a failed save (0042D1CE) shows the profile menu 00449A00 over +100
//! (+F7): the held slots command it, picking a slot fades it out, and the
//! tick then restarts the save into that slot. The final world (+F5) reads
//! the player's name at construction for its page's credits.

use std::convert::Infallible;
use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::frontend_screen::FrontendProcessState;
use crate::input::{InputContextId, InputRuntime, InputStateError};
use crate::menu::{
    MenuActionRequest, MenuBasis, MenuCommand, MenuControllerSpec, MenuError, MenuHost, MenuInput,
    MenuOwner, MenuStack, MenuStateError, MenuVisibility,
};
use crate::menu_basis::MenuBasisMath;
use crate::name_entry::{IdentityInitialsWrite, IdentityTarget, IdentityWriteRequest};
use crate::object_runtime::{
    ObjectArena, PlayerAttachError, PlayerCommandContext, PlayerCommandError,
    PlayerCommandOperation, PlayerCommandRequest,
};
use crate::panel_animation::PanelAnimation;
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::profile_menu::{
    ProfileMenu, ProfileMenuError, ProfileMenuServices, ProfileMenuTemplates, ProfileTickOutcome,
};
use crate::profile_name_format::ProfileNameClassification;
use crate::profile_store::{
    ProfileStore, ProfileStoreError, WorkingProfileRestoreRequest, WorkingProfileSerializeRequest,
};
use crate::scheduler::SchedulerTick;
use crate::screen_input::{
    ScreenAnalogDirection, ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError,
};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::screen_renderer_preparation::{prepare_screen_renderer, ScreenRendererPreparationHost};
use crate::session::{SessionPhase, SessionState};
use crate::world_mission::WorldMission;

pub const INTRO_RESULTS_CALLBACK_CONTEXT: u32 = 6;

/// 0042B190, 0042B350 and 0042B510: the panels' 500 ms stage at scale 0x6000.
const PANEL_DURATION_MICROS: i32 = 500_000;
const PANEL_SCALE: u16 = 0x6000;
/// 0042D24C: the shown stats restart the screen's age at 4000 ms.
const STATS_AGE_MILLIS: i32 = 4000;
/// 0042D4F8: the exit countdown, or 1 for the final world or with +F9.
const EXIT_MILLIS: u32 = 1000;
/// 0042AAF1: with +F8 the save countdown starts past its 1000 ms.
const SAVE_PAST_DUE: i32 = 0x3e9;
/// 0042B776/0042CBDF: a panel's slide-in percentage that builds the next.
const NEXT_PANEL_PERCENT: i32 = 90;
/// 0042B87C..0042B885: the score's base per ACAM not lost, and 0042B92F's
/// per kill and 0042B9ED's per collateral loss.
const SCORE_PER_SPARE_ACAM: i32 = 150;
const SCORE_PER_KILL: i32 = 10;
const SCORE_PER_COLLATERAL: i32 = 30;
/// 0042BF84: the best medal.
const BEST_MEDAL: i32 = 3;
/// The two callbacks the screen answers itself; the rest are held bits.
const SHOW_OR_CONTINUE: u32 = 0x0042_aa60;
const SAVE: u32 = 0x0042_aaa0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroResultsSkippedFeature {
    /// Live W+15 (G+1C5) has no supplied writer after input selection.
    ConstructorSaveModeUnavailable,
    /// The reached Serialize/Restore store loan has no actual backing.
    ConstructorProfileStoreUnavailable,
}

#[derive(Clone, Copy, Debug)]
pub struct IntroResultsScreenConfig {
    pub attachment: ScreenAttachment,
    pub callback_context: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroResultsProfileOperation {
    Serialize,
    Restore,
}

pub trait IntroResultsFactoryHost {
    /// Exact BYTE0 writer 0042AC48, before device enumeration.
    fn reset_loading_mode_42ac48(&mut self);
    type Error;
    type IdentityError: std::error::Error;
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError>;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
    fn profile_store(&mut self) -> Option<&mut ProfileStore>;
    /// Native42ADE4 contains this reached failure after binding. The loan
    /// has ended before diagnostics, so no store mutation is hidden here.
    fn report_profile_failure(
        &mut self,
        operation: IntroResultsProfileOperation,
        error: &dyn std::error::Error,
    );
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error>;
    /// `0042ACAE`: `00463550`'s name (phase 9's read, capped at 30 bytes),
    /// which the final world's page formats into the line naming the
    /// player.
    fn player_name(&mut self) -> Result<Vec<u8>, String>;
}

pub trait IntroResultsTickHost {
    type Error;
    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error>;
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error>;
    /// 0042D150..0042D170: the working profile serialized (00449C10) and
    /// written (0044A110) inside the error scope 00511C90/00494A00. A failed
    /// save answers false, as the scope's error arm (004923C0) does.
    fn save_profile(&mut self, profiles: Option<&mut ProfileStore>) -> Result<bool, Self::Error>;
}

/// What the profile menu over +100 borrows while it runs: the save owner
/// (session +660), gameplay byte +5C2 (its rows and visibility, 00453360)
/// and the input runtime its rebuild resets (004F0340).
pub struct IntroResultsMenuServices<'a> {
    pub profiles: Option<&'a mut ProfileStore>,
    pub owner_byte: bool,
    pub input: &'a mut InputRuntime,
}

/// Borrow the real current owners only for this Tick. The animation cell is
/// shared with the other screens; no second clock or mission scalar is kept.
pub struct IntroResultsTickServices<'a> {
    pub menu: IntroResultsMenuServices<'a>,
    pub process: &'a FrontendProcessState,
    pub mission: &'a WorldMission,
    pub attract: bool,
}

/// The profile menu's reached owners that only the frontend has, or a
/// controller state retail never builds here.
#[derive(Debug, Error)]
pub enum IntroResultsMenuError {
    #[error("the results profile menu has no save owner lent")]
    MissingProfileStore,
    #[error("the results profile menu has no row templates")]
    MissingTemplates,
    #[error("the results controller +100 is missing")]
    MissingController,
    #[error("the results controller holds a menu 00449A00 never pushes")]
    AuthoredMenu,
    /// With +5C2 clear the menu's load and new-name paths select a
    /// frontend camera or flag a frontend transition; phase 8 has neither.
    #[error("the results profile menu reached {0}, which only the frontend owns")]
    FrontendOnly(&'static str),
    #[error(transparent)]
    State(#[from] MenuStateError),
    #[error(transparent)]
    Store(#[from] ProfileStoreError),
    #[error(transparent)]
    Profile(ProfileMenuError<Infallible>),
    #[error("the results profile menu requested phase {0}, which is not a phase")]
    Phase(u32),
}

/// Actual current owners for one 0042C2B0 Draw. Creating this loan does not
/// consume the mission; attract bypasses it and both width reads precede it.
/// What follows 0042C2B0's shared prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroResultsDrawOutcome {
    /// Attract, or the intro's world 22: nothing more.
    Done,
    /// Any other world: the results page follows.
    Page,
}

pub struct IntroResultsDrawRequest<'a> {
    pub attract: bool,
    pub mission: Option<&'a WorldMission>,
}

pub trait IntroResultsDrawHost<E>: ScreenRendererPreparationHost<E> {
    /// 00495900 returns the full DWORD surface+0, in pixels.
    fn surface_width_pixels(&mut self) -> Result<u32, E>;
}

/// The constructor's bytes +EC..+FC (0042AB51..0042AB93), +148 and +150.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResultsFlags {
    /// +EC, the save countdown in milliseconds, from 1.
    save_countdown_ec: i32,
    /// +F0, the game saved.
    saved_f0: bool,
    /// +F1, the save failed or was impossible.
    failed_f1: bool,
    /// +F2, the stats asked for.
    stats_f2: bool,
    /// +F3, the panels built.
    panels_f3: bool,
    /// +F4, more than 4000 ms of screen age.
    delay_f4: bool,
    /// +F5, the final world's identity and score.
    final_f5: bool,
    /// +F6, a save in flight.
    saving_f6: bool,
    /// +F7, the profile menu shown.
    menu_f7: bool,
    /// +F8, the save not possible; 00449C80 rewrites it as false.
    no_save_f8: bool,
    /// +F9.
    byte_f9: bool,
    /// +FC, the profile status, 3 from the constructor.
    status_fc: u32,
    /// +148, the exit countdown in milliseconds.
    exit_148: u32,
    /// +150, which ends the next held pass before any slot.
    gate_150: bool,
}

impl ResultsFlags {
    /// 0042D4D0, the exit arm.
    fn arm_exit(&mut self) {
        if !self.menu_f7 && self.exit_148 == 0 {
            self.exit_148 = if self.final_f5 || self.byte_f9 {
                1
            } else {
                EXIT_MILLIS
            };
        }
    }
}

#[derive(Debug, Error)]
pub enum IntroResultsScreenError<E> {
    #[error("intro results requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("0042A970 serves requested phase8, not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    PlayerCommand(#[from] PlayerCommandError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    ScreenInput(#[from] ScreenInputError<IntroResultsMenuError>),
    #[error(transparent)]
    Menu(#[from] IntroResultsMenuError),
    #[error("intro results first Tick stopped after a retained prefix")]
    TickStopped,
    #[error("0042C314 requires the actual current world owner after both width reads")]
    DrawMissionUnavailable,
    /// State retail reads without a check, where it would fault.
    #[error("0042C2B0's results draw reads {0}, which is missing")]
    DrawState(&'static str),
    #[error("required intro results effect failed")]
    Host(E),
    /// The final world's credits (`0042AB00`): the player's name or the
    /// executable's table.
    #[error("the final world's credits: {0}")]
    Credits(String),
}

#[derive(Debug)]
pub struct IntroResultsScreen {
    config: IntroResultsScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    /// +100's actual controller, constructed before device enumeration;
    /// the profile menu 00449A00 pushes onto it.
    menus: Option<MenuStack>,
    /// 00449410's row templates, shared with the frontend's profile menu.
    profile_templates: Option<ProfileMenuTemplates>,
    basis: MenuBasisMath,
    /// Interface cues (004391D0) the profile menu played, for the host.
    cues: Vec<i16>,
    /// Constructor 0042ABD4's raw WORD; later consumers zero-extend it.
    player_word_144: u16,
    input: ScreenInput,
    /// Actual 0042ABE1 zero store, advanced at 0042D08D in milliseconds.
    elapsed_millis_14c: i32,
    flags: ResultsFlags,
    /// +130, +134 and +138: the panels 0042B190/0042B350/0042B510 build.
    panels: [Option<PanelAnimation>; 3],
    /// A required effect failed; the reached prefix stays and no tick replays.
    tick_stopped: bool,
    ticks: u64,
    /// +140, the score panel 1's draw computes.
    score_140: i32,
    /// +13C, the medal panel 2's draw awards once; -1 until then.
    medal_13c: i32,
    /// What the final world's page (+F5) rolls.
    credits: Option<crate::results_credits::FinalCredits>,
}

impl IntroResultsScreen {
    pub fn new(config: IntroResultsScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            menus: None,
            profile_templates: None,
            basis: MenuBasisMath::new(assets.sine_table().values),
            cues: Vec::new(),
            player_word_144: 0,
            input: ScreenInput::new(ScreenDispatchPolicy::Results, config.callback_context),
            elapsed_millis_14c: 0,
            flags: ResultsFlags {
                status_fc: 3,
                ..ResultsFlags::default()
            },
            panels: [None; 3],
            score_140: 0,
            medal_13c: -1,
            tick_stopped: false,
            ticks: 0,
            credits: None,
        }
    }

    /// What the final world's page rolls, read at construction.
    pub fn credits(&self) -> Option<&crate::results_credits::FinalCredits> {
        self.credits.as_ref()
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

    pub fn player_word_144(&self) -> u16 {
        self.player_word_144
    }

    pub fn input(&self) -> &ScreenInput {
        &self.input
    }

    pub fn elapsed_millis_14c(&self) -> i32 {
        self.elapsed_millis_14c
    }

    pub fn intro_delay_elapsed(&self) -> bool {
        self.flags.delay_f4
    }

    /// +F0, the game saved.
    pub fn saved(&self) -> bool {
        self.flags.saved_f0
    }

    /// +F2, the stats asked for.
    pub fn stats_shown(&self) -> bool {
        self.flags.stats_f2
    }

    /// +148, the exit countdown in milliseconds.
    pub fn exit_millis(&self) -> u32 {
        self.flags.exit_148
    }

    /// +EC, the save countdown in milliseconds; 0 when idle.
    pub fn save_countdown_millis(&self) -> i32 {
        self.flags.save_countdown_ec
    }

    /// +130, +134 and +138.
    pub fn panels(&self) -> &[Option<PanelAnimation>; 3] {
        &self.panels
    }

    /// +F7, the profile menu shown.
    pub fn menu_shown(&self) -> bool {
        self.flags.menu_f7
    }

    /// +100 while the draw shows it (+F7): 0042CA9C's controller draw.
    pub fn shown_menu_mut(&mut self) -> Option<&mut MenuStack> {
        if self.flags.menu_f7 {
            self.menus.as_mut()
        } else {
            None
        }
    }

    /// The interface cues (004391D0) the profile menu played since the last
    /// call.
    pub fn take_cues(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.cues)
    }

    /// +140, the score.
    pub fn score(&self) -> i32 {
        self.score_140
    }

    /// +13C, the medal awarded, if any.
    pub fn medal(&self) -> Option<u8> {
        u8::try_from(self.medal_13c).ok()
    }

    /// The state 0042C2B0 changes while drawing a world's results (every
    /// world but the intro): the panels build each other in turn, panel 1
    /// computes the score, and panel 2 awards the medal once, adding it to
    /// the first player's rank +11E. The profile menu draws instead of the
    /// panels; the final world draws its own page.
    pub fn draw_effects(
        &mut self,
        attract: bool,
        mission: &WorldMission,
        arena: &mut ObjectArena,
    ) -> Result<(), IntroResultsScreenError<Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if attract || mission.current_world_a0 == 22 {
            return Ok(());
        }
        if !self.flags.menu_f7 && !self.flags.final_f5 {
            // 0042CB74: panel 1's draw 0042B6D0.
            if let Some(first) = self.panels[0] {
                if self.panels[1].is_none() && slide_in_percent(first) >= NEXT_PANEL_PERCENT {
                    self.request_panel(1);
                }
                if first.current_stage >= 1 && world_status(mission)? & 4 == 0 {
                    self.score_140 = score(self.player_word_144, mission)?;
                }
            }
            // 0042CB96: panel 3 once panel 2 is 90% in, then panel 2's draw
            // 0042BBF0.
            if let Some(second) = self.panels[1] {
                if self.panels[2].is_none() && slide_in_percent(second) >= NEXT_PANEL_PERCENT {
                    self.request_panel(2);
                }
                if second.current_stage >= 1 {
                    self.award_medal(mission, arena)?;
                }
            }
        }
        // 0042CF93: the tail builds panel 1.
        if !self.flags.final_f5 && self.panels[0].is_none() {
            self.request_panel(0);
        }
        Ok(())
    }

    /// 0042BE16..0042BFA6: after 4 s the status line blinks with each half
    /// second, and the medal is only reached while it shows. A completed
    /// world (status bit 2) awards it once: the score in hundreds, from 0 to
    /// 3, added to the first player's +11E.
    fn award_medal(
        &mut self,
        mission: &WorldMission,
        arena: &mut ObjectArena,
    ) -> Result<(), IntroResultsScreenError<Infallible>> {
        let age = self.input.age_millis();
        if age >= 4000 && (age / 500) % 2 == 0 {
            return Ok(());
        }
        let save_mode = mission
            .bytes_14_16
            .ok_or(IntroResultsScreenError::DrawState("G+1C5"))?[1];
        if save_mode != 0 {
            return Ok(());
        }
        let status = world_status(mission)?;
        if status & 4 != 0 || status & 2 == 0 || self.medal_13c != -1 {
            return Ok(());
        }
        self.medal_13c = if self.score_140 < 0 {
            0
        } else {
            (self.score_140 / 100).min(BEST_MEDAL)
        };
        let player = arena
            .first_player()
            .ok_or(IntroResultsScreenError::DrawState("the first player"))?;
        arena.add_player_word_11e(player, self.medal_13c as u16)?;
        Ok(())
    }

    /// The state the results page reads, with the panels as the draw found
    /// them (before [`Self::draw_effects`]).
    pub fn page_state(
        &self,
        panels: [Option<PanelAnimation>; 3],
    ) -> crate::results_page::ResultsPageState {
        crate::results_page::ResultsPageState {
            age_millis: self.input.age_millis(),
            elapsed_millis: self.elapsed_millis_14c,
            acams_lost: self.player_word_144,
            medal: self.medal(),
            save_countdown_millis: self.flags.save_countdown_ec,
            saved: self.flags.saved_f0,
            save_failed: self.flags.failed_f1,
            profile_status: self.flags.status_fc,
            exit_millis: self.flags.exit_148,
            menu_shown: self.flags.menu_f7,
            panels,
            final_page: self.flags.final_f5,
        }
    }

    /// One input callback in the screen's context. 0042AA60 and 0042AAA0
    /// act at once; the rest set or clear held bits, and a newly held bit
    /// runs the held pass 0042D460.
    pub fn input_callback(
        &mut self,
        event: crate::input::InputCallback,
        services: &mut IntroResultsMenuServices<'_>,
    ) -> Result<(), IntroResultsScreenError<Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        let flags = &mut self.flags;
        match event.address {
            SHOW_OR_CONTINUE => {
                // 0042AA60.
                if event.value != 0 && !flags.saving_f6 && flags.save_countdown_ec == 0 {
                    if !flags.stats_f2 && !flags.delay_f4 {
                        flags.stats_f2 = true;
                    } else {
                        flags.arm_exit();
                    }
                }
                Ok(())
            }
            SAVE => {
                // 0042AAA0.
                if event.value != 0
                    && !flags.menu_f7
                    && !flags.saving_f6
                    && !flags.saved_f0
                    && flags.save_countdown_ec == 0
                {
                    if !flags.delay_f4 {
                        flags.stats_f2 = true;
                    }
                    flags.save_countdown_ec = if flags.no_save_f8 { SAVE_PAST_DUE } else { 1 };
                }
                Ok(())
            }
            _ => {
                let mut menu = ResultsMenu {
                    menus: &mut self.menus,
                    templates: &self.profile_templates,
                    basis: &self.basis,
                    cues: &mut self.cues,
                    services,
                    context: self.config.callback_context,
                    phase: None,
                };
                self.input
                    .digital_callback(event, &mut |input, action| {
                        held_action(flags, &mut menu, input, action)
                    })
                    .map_err(IntroResultsScreenError::ScreenInput)?;
                // Commands never reach the profile operations that request
                // a phase; those run in the menu's tick.
                match menu.phase {
                    Some(phase) => Err(IntroResultsMenuError::Phase(phase).into()),
                    None => Ok(()),
                }
            }
        }
    }

    pub fn initialize<H: IntroResultsFactoryHost>(
        &mut self,
        services: &mut IntroResultsFactoryServices<'_>,
        host: &mut H,
        state: &mut SessionState,
    ) -> Result<(), IntroResultsScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if state.requested_phase() != SessionPhase::Phase8 {
            return Err(IntroResultsScreenError::RequestedPhase(
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
        // 0042AB9D -> 00453380: first argument FFFF is the fade rate;
        // the four WORD viewport arguments are authored 1024-unit coordinates.
        self.menus = Some(MenuStack::new(MenuControllerSpec {
            fade_rate: 65535,
            viewport: [0, 0, 1024, 1024],
        }));
        self.profile_templates = Some(services.profile_templates.clone());
        host.reset_loading_mode_42ac48();
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(IntroResultsScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.phase8_input_tables(),
            &devices,
        )?;
        // 0042AC94 reads G+1E0 after devices. This typed host has no mutable
        // mission callback; the actual owner's supplied DWORD stays current.
        if services.mission.save.mission_selection == 1 {
            // 0042ACAE..0042AD02: the player's name, the final world's page
            // (+F5, +F3), and the line naming the player for its credits.
            let name = host
                .player_name()
                .map_err(IntroResultsScreenError::Credits)?;
            self.flags.final_f5 = true;
            self.flags.panels_f3 = true;
            let executable =
                inf_formats::FrontendExecutable::parse(services.assets.executable_bytes())
                    .map_err(|error| IntroResultsScreenError::Credits(error.to_string()))?;
            self.credits = Some(
                crate::results_credits::FinalCredits::load(executable, name)
                    .map_err(|error| IntroResultsScreenError::Credits(error.to_string()))?,
            );
        }
        self.player_word_144 = initialize_results_player_prefix(services.arena, services.process)?;
        let context = self.bindings.context().expect("installed results context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => IntroResultsScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => IntroResultsScreenError::Input(error),
            })?;
        initialize_results_profile(services, host, state)?;
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// 0042CFC0. A failed required effect retains its reached prefix and
    /// stops later ticks.
    pub fn tick<H: IntroResultsTickHost>(
        &mut self,
        tick: SchedulerTick,
        services: &mut IntroResultsTickServices<'_>,
        host: &mut H,
    ) -> Result<(), IntroResultsScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if self.tick_stopped {
            return Err(IntroResultsScreenError::TickStopped);
        }
        self.tick_stopped = true;
        self.ticks += 1;
        if let Some(request) = crate::results_tick::loop_head_request(
            services.mission.current_world_a0,
            services.attract,
        ) {
            host.request_phase(SessionPhase::Phase8, request)
                .map_err(IntroResultsScreenError::Host)?;
        }
        // 0042CFF5: the shown profile menu's tick (+100's +10). Once its
        // fade has ended, the status query 00449C90(0) restarts the save
        // countdown, or bars the save.
        if self.flags.menu_f7 {
            let mut menu = ResultsMenu {
                menus: &mut self.menus,
                templates: &self.profile_templates,
                basis: &self.basis,
                cues: &mut self.cues,
                services: &mut services.menu,
                context: self.config.callback_context,
                phase: None,
            };
            let alive = menu.tick(&mut self.input, tick.delta_micros)?;
            let phase = menu.phase;
            if let Some(phase) = phase {
                let requested = SessionPhase::try_from(phase)
                    .map_err(|_| IntroResultsMenuError::Phase(phase))?;
                host.request_phase(SessionPhase::Phase8, requested)
                    .map_err(IntroResultsScreenError::Host)?;
            }
            if !alive {
                self.flags.menu_f7 = false;
                let status = services
                    .menu
                    .profiles
                    .as_deref_mut()
                    .ok_or(IntroResultsMenuError::MissingProfileStore)?
                    .query(false)
                    .map_err(IntroResultsMenuError::Store)?
                    .status as u32;
                self.flags.status_fc = status;
                if status == 3 {
                    self.flags.save_countdown_ec = 1;
                } else {
                    self.flags.no_save_f8 = true;
                }
            }
        }
        if self.flags.saving_f6 {
            self.flags.saving_f6 = false;
        }
        // 0042D065: base input 004392B0, the raw age first.
        self.input.add_age_millis(tick.elapsed_micros / 1000);
        let direction = self.analog_direction(services.menu.input)?;
        let flags = &mut self.flags;
        let mut menu = ResultsMenu {
            menus: &mut self.menus,
            templates: &self.profile_templates,
            basis: &self.basis,
            cues: &mut self.cues,
            services: &mut services.menu,
            context: self.config.callback_context,
            phase: None,
        };
        self.input
            .tick_after_age(tick, direction, &mut |input, action| {
                held_action(flags, &mut menu, input, action)
            })?;
        if let Some(phase) = menu.phase {
            return Err(IntroResultsMenuError::Phase(phase).into());
        }
        // 0042D06A..0042D0B4: the capped milliseconds into +14C and +C0,
        // the shared clock, and +F4 past 4000.
        let elapsed = crate::results_tick::scale_tick_timing(tick.delta_micros);
        self.elapsed_millis_14c = self.elapsed_millis_14c.wrapping_add(elapsed);
        self.input.add_age_millis(elapsed);
        services.process.advance_animation(tick.delta_micros as u32);
        if self.input.age_millis() > 4000 {
            self.flags.delay_f4 = true;
        }
        // 0042D0CF: 00449C80 is XOR AL,AL; RET.
        if !self.flags.no_save_f8 && !self.flags.menu_f7 {
            self.flags.no_save_f8 = false;
        }
        self.tick_save(elapsed, services.menu.profiles.as_deref_mut(), host)?;
        // 0042D207: the panels' 004554E0 with the scheduler's delta.
        for panel in self.panels.iter_mut().flatten() {
            panel.tick(tick.delta_micros);
        }
        if self.flags.stats_f2 && !self.flags.panels_f3 {
            // 0042D24C: the age restarts at 4000 and the panels come up.
            self.input.set_age_millis(STATS_AGE_MILLIS);
            for slot in 0..3 {
                self.request_panel(slot);
            }
            self.flags.panels_f3 = true;
        }
        if self.flags.exit_148 != 0 {
            // 0042D27C: the panels drop to stage 0 at once.
            for panel in self.panels.iter_mut().flatten() {
                panel.current_stage = 0;
                panel.requested_stage = 0;
            }
            let unsigned = crate::results_tick::scale_tail_timing(tick.delta_micros as u32);
            if self.flags.exit_148 > unsigned {
                self.flags.exit_148 = self.flags.exit_148.wrapping_sub(elapsed as u32);
            } else {
                self.flags.exit_148 = 1;
                let requested = if self.flags.final_f5 {
                    SessionPhase::Frontend
                } else {
                    SessionPhase::Phase3
                };
                host.request_phase(SessionPhase::Phase8, requested)
                    .map_err(IntroResultsScreenError::Host)?;
            }
        }
        self.tick_stopped = false;
        Ok(())
    }

    /// 0042D0DA..0042D200: the save countdown. Past 1000 ms, unless already
    /// saved, the profile is saved when it can be; the outcome then ends the
    /// countdown and gates the next held pass (+150).
    fn tick_save<H: IntroResultsTickHost>(
        &mut self,
        elapsed: i32,
        profiles: Option<&mut ProfileStore>,
        host: &mut H,
    ) -> Result<(), IntroResultsScreenError<H::Error>> {
        if self.flags.save_countdown_ec == 0 {
            return Ok(());
        }
        self.flags.save_countdown_ec = self.flags.save_countdown_ec.wrapping_add(elapsed);
        if self.flags.save_countdown_ec <= 1000 || self.flags.saved_f0 {
            return Ok(());
        }
        // 0042D0EA: [EBP-1], the menu's initial warning, unless the save
        // could not be tried.
        let mut warning = true;
        if !self.flags.no_save_f8 && self.flags.status_fc == 3 {
            self.flags.saving_f6 = true;
            if host
                .save_profile(profiles)
                .map_err(IntroResultsScreenError::Host)?
            {
                self.flags.saved_f0 = true;
                self.flags.failed_f1 = false;
            } else {
                self.flags.failed_f1 = true;
                self.flags.no_save_f8 = true;
            }
        } else {
            self.flags.no_save_f8 = false;
            self.flags.failed_f1 = true;
            warning = false;
        }
        self.flags.save_countdown_ec = 0;
        if self.flags.failed_f1 && !self.flags.menu_f7 {
            // 0042D1CE: 00449A00 shows the profile menu over +100.
            open_profile_menu(
                &mut self.menus,
                &self.profile_templates,
                &mut self.flags,
                warning,
            )?;
        }
        self.flags.gate_150 = true;
        Ok(())
    }

    /// 0042B190/0042B350/0042B510: build the panel if absent, then ask it
    /// for stage 2 (refilling an exhausted timer).
    fn request_panel(&mut self, slot: usize) {
        let panel = self.panels[slot].get_or_insert_with(|| {
            PanelAnimation::with_duration(PANEL_SCALE, PANEL_DURATION_MICROS)
        });
        panel.request(2);
    }

    fn analog_direction(
        &self,
        input: &InputRuntime,
    ) -> Result<ScreenAnalogDirection, FrontendBindingError> {
        let d0 = self
            .bindings
            .consumer_d0()
            .ok_or(FrontendBindingError::MissingScreenConsumer(0xd0))?;
        if input.analog_value(d0)? > 2500 {
            return Ok(ScreenAnalogDirection::Previous);
        }
        if input.analog_value(d0)? < -2500 {
            return Ok(ScreenAnalogDirection::Next);
        }
        let cc = self
            .bindings
            .consumer_cc()
            .ok_or(FrontendBindingError::MissingScreenConsumer(0xcc))?;
        if input.analog_value(cc)? < -2500 {
            return Ok(ScreenAnalogDirection::AdjustPrevious);
        }
        if input.analog_value(cc)? > 2500 {
            return Ok(ScreenAnalogDirection::AdjustNext);
        }
        Ok(ScreenAnalogDirection::Neutral)
    }

    pub fn draw<H, E>(
        &self,
        request: IntroResultsDrawRequest<'_>,
        host: &mut H,
    ) -> Result<IntroResultsDrawOutcome, IntroResultsScreenError<E>>
    where
        H: IntroResultsDrawHost<E>,
    {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        // 0042C2B0/0042C2C3 bypass every surface/game/content operand.
        if request.attract {
            return Ok(IntroResultsDrawOutcome::Done);
        }
        prepare_screen_renderer(host).map_err(IntroResultsScreenError::Host)?;
        let _first_width_pixels = host
            .surface_width_pixels()
            .map_err(IntroResultsScreenError::Host)?;
        // 0042C2E8 captures G+1B0 with LEA, without reading its fields. The
        // optional typed owner is therefore required only after width2.
        let second_width_pixels = host
            .surface_width_pixels()
            .map_err(IntroResultsScreenError::Host)?;
        let _content_width_pixels = (second_width_pixels.wrapping_mul(3) as i32) / 5;
        let mission = request
            .mission
            .ok_or(IntroResultsScreenError::DrawMissionUnavailable)?;
        // Fresh full-DWORD G+250 at0042C314 precedes all later content and
        // height/context reads. One immutable current-mission loan admits no
        // synchronous native global-owner replacement or raw pointer alias.
        if mission.current_world_a0 == 22 {
            return Ok(IntroResultsDrawOutcome::Done);
        }
        Ok(IntroResultsDrawOutcome::Page)
    }

    /// Destructor `0042AE30` (scalar deleting `0042AE10`): it frees the
    /// panels `+130..+138` (here they go with the screen), then releases the
    /// binding sets, the menu `+100` and the base.
    pub fn destroy<D, E>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), IntroResultsScreenError<E>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        // 0042AE97 releases devices before 0042AEA7 destroys +100's menu,
        // then 0042AEB6 tears down the base. No rows have been attached here.
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

    fn require_lifecycle<E>(
        &self,
        expected: ScreenLifecycle,
    ) -> Result<(), IntroResultsScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(IntroResultsScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

pub struct IntroResultsFactoryServices<'a> {
    pub registry:
        &'a mut ScreenRegistry<crate::frontend_renderer::FrontendRenderContext, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    pub arena: &'a mut ObjectArena,
    pub mission: &'a mut WorldMission,
    pub process: Option<&'a ParticleFactoryProcessState>,
    pub(crate) classification: &'a ProfileNameClassification,
    /// The frontend catalog's profile row templates (00449410).
    pub profile_templates: &'a ProfileMenuTemplates,
}

/// 0042B741..0042B776: a panel's elapsed share of its stage, in percent.
fn slide_in_percent(panel: PanelAnimation) -> i32 {
    let elapsed = panel.duration_micros.wrapping_sub(panel.remaining_micros);
    let tenths = elapsed / 10;
    let thousandths = panel.duration_micros / 1000;
    if thousandths == 0 {
        return 0;
    }
    tenths / thousandths
}

/// G+1EC[G+250], the current world's status.
fn world_status(mission: &WorldMission) -> Result<u32, IntroResultsScreenError<Infallible>> {
    crate::results_page::current_world_status(mission).ok_or(IntroResultsScreenError::DrawState(
        "the current world's status",
    ))
}

/// 0042B887..0042B9F8: 150 for each of two ACAMs not lost, 10 per kill and
/// -30 per collateral loss, from the mission record +0C.
fn score(lost: u16, mission: &WorldMission) -> Result<i32, IntroResultsScreenError<Infallible>> {
    let record = mission
        .record_0c
        .as_deref()
        .ok_or(IntroResultsScreenError::DrawState("the mission record +0C"))?;
    let [kills, collateral] = crate::results_page::record_totals(record).map(|total| total as i32);
    Ok(2_i32
        .wrapping_sub(i32::from(lost))
        .wrapping_mul(SCORE_PER_SPARE_ACAM)
        .wrapping_add(kills.wrapping_mul(SCORE_PER_KILL))
        .wrapping_sub(collateral.wrapping_mul(SCORE_PER_COLLATERAL)))
}

/// 0042D460, the held pass: a set +150 clears +C4 and ends it. While the
/// profile menu is shown (+F7) the base slots 0042D3B0, 0042D370, 0042D390,
/// 0042D330 and 0042D350 send it commands 4, 0, 1, 2 and 3, and bits 0x40
/// and 0x80 (0042D3D0, 0042D440) commands 6 and 7, through 004537E0; Back
/// is the bare RET 00401310. Bit
/// 0x100 (0042D3F0) shows the menu; bit 0x200 (0042D510) hides it or arms
/// the exit.
fn held_action(
    flags: &mut ResultsFlags,
    menu: &mut ResultsMenu<'_, '_>,
    input: &mut ScreenInput,
    action: ScreenInputAction,
) -> Result<(), IntroResultsMenuError> {
    let command = match action {
        ScreenInputAction::ResultsGate => {
            if flags.gate_150 {
                input.clear_held();
                flags.gate_150 = false;
            }
            return Ok(());
        }
        ScreenInputAction::Back => return Ok(()),
        ScreenInputAction::RequestAttract => {
            if !flags.menu_f7 {
                open_profile_menu(menu.menus, menu.templates, flags, false)?;
            }
            return Ok(());
        }
        ScreenInputAction::Bit200 => {
            if flags.menu_f7 {
                // Controller virtual +04 (004151C0) starts the fade out.
                menu.menus
                    .as_mut()
                    .ok_or(IntroResultsMenuError::MissingController)?
                    .set_active(false);
                flags.no_save_f8 = true;
                flags.menu_f7 = false;
            } else {
                flags.arm_exit();
            }
            return Ok(());
        }
        ScreenInputAction::Confirm => MenuCommand::Activate,
        ScreenInputAction::Previous => MenuCommand::Previous,
        ScreenInputAction::Next => MenuCommand::Next,
        ScreenInputAction::AdjustPrevious => MenuCommand::AdjustPrevious,
        ScreenInputAction::AdjustNext => MenuCommand::AdjustNext,
        ScreenInputAction::Auxiliary6 => MenuCommand::Auxiliary6,
        ScreenInputAction::Auxiliary7 => MenuCommand::Auxiliary7,
    };
    if flags.menu_f7 {
        menu.command(input, command)?;
    }
    Ok(())
}

/// 0042D3F0 and 0042D1CE: 00449A00 pushes the profile menu onto +100,
/// whose virtual +08 (004151D0) fades it in; the screen then marks it shown
/// and forgets the save's outcome.
fn open_profile_menu(
    menus: &mut Option<MenuStack>,
    templates: &Option<ProfileMenuTemplates>,
    flags: &mut ResultsFlags,
    initial_warning: bool,
) -> Result<(), IntroResultsMenuError> {
    let templates = templates
        .clone()
        .ok_or(IntroResultsMenuError::MissingTemplates)?;
    let menus = menus
        .as_mut()
        .ok_or(IntroResultsMenuError::MissingController)?;
    menus.push(ProfileMenu::new(
        templates,
        MenuBasis::INITIAL,
        initial_warning,
    ));
    menus.set_active(true);
    flags.menu_f7 = true;
    flags.failed_f1 = false;
    flags.save_countdown_ec = 0;
    flags.saved_f0 = false;
    flags.saving_f6 = false;
    Ok(())
}

/// The screen's profile-menu owners for one dispatch: +100, the row
/// templates, the basis kernels and the borrowed services.
struct ResultsMenu<'a, 's> {
    menus: &'a mut Option<MenuStack>,
    templates: &'a Option<ProfileMenuTemplates>,
    basis: &'a MenuBasisMath,
    cues: &'a mut Vec<i16>,
    services: &'a mut IntroResultsMenuServices<'s>,
    /// 004537E0's context: the screen.
    context: u32,
    /// A phase the menu's operations requested, for the tick's host.
    phase: Option<u32>,
}

impl ResultsMenu<'_, '_> {
    /// 004537E0: the command to +100's current menu (its +04, 00452E20).
    fn command(
        &mut self,
        screen_input: &mut ScreenInput,
        command: MenuCommand,
    ) -> Result<(), IntroResultsMenuError> {
        let visibility = MenuVisibility::from_owner_flag(self.services.owner_byte);
        let context = self.context;
        let (menus, mut host) = self.split(screen_input)?;
        menus
            .command(
                MenuInput {
                    command,
                    visibility,
                    context,
                },
                &mut host,
            )
            .map(|_| ())
            .map_err(flatten_menu_error)
    }

    /// +100's +10 (00453560); false once its fade has ended, which destroys
    /// its menus.
    fn tick(
        &mut self,
        screen_input: &mut ScreenInput,
        delta_micros: i32,
    ) -> Result<bool, IntroResultsMenuError> {
        let (menus, mut host) = self.split(screen_input)?;
        menus
            .tick(delta_micros, &mut host)
            .map_err(flatten_menu_error)
    }

    fn split<'b>(
        &'b mut self,
        screen_input: &'b mut ScreenInput,
    ) -> Result<(&'b mut MenuStack, ResultsMenuHost<'b>), IntroResultsMenuError> {
        let menus = self
            .menus
            .as_mut()
            .ok_or(IntroResultsMenuError::MissingController)?;
        Ok((
            menus,
            ResultsMenuHost {
                store: self.services.profiles.as_deref_mut(),
                owner_byte: self.services.owner_byte,
                services: ResultsProfileServices {
                    basis: self.basis,
                    screen_input,
                    runtime: &mut *self.services.input,
                    cues: &mut *self.cues,
                    phase: &mut self.phase,
                },
            },
        ))
    }
}

struct ResultsMenuHost<'a> {
    store: Option<&'a mut ProfileStore>,
    owner_byte: bool,
    services: ResultsProfileServices<'a>,
}

impl MenuHost for ResultsMenuHost<'_> {
    type Error = IntroResultsMenuError;

    fn action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        let Some((MenuOwner::Profile(profile), active)) = stack.current_owner_and_active_mut()
        else {
            return Err(IntroResultsMenuError::AuthoredMenu);
        };
        let store = self
            .store
            .as_deref_mut()
            .ok_or(IntroResultsMenuError::MissingProfileStore)?;
        profile
            .execute_callback(
                request.callback_address,
                request.command,
                request.arguments,
                self.owner_byte,
                store,
                &mut ActiveServices {
                    services: &mut self.services,
                    active,
                },
            )
            .map_err(flatten_profile_error)
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.services.basis.rotate(basis, angle);
        Ok(())
    }

    fn tick_profile(
        &mut self,
        profile: &mut ProfileMenu,
        delta_micros: i32,
        active: &mut bool,
    ) -> Result<(), MenuError<Self::Error>> {
        let store = self
            .store
            .as_deref_mut()
            .ok_or(MenuError::Host(IntroResultsMenuError::MissingProfileStore))?;
        let outcome = profile
            .tick(
                delta_micros,
                self.owner_byte,
                store,
                &mut ActiveServices {
                    services: &mut self.services,
                    active,
                },
            )
            .map_err(flatten_profile_error)
            .map_err(MenuError::Host)?;
        // Retail's setjmp catch keeps the menu running with its failure timer.
        if let ProfileTickOutcome::OperationFailed { operation, error } = outcome {
            eprintln!("Results profile menu: {operation:?} failed: {error}");
        }
        Ok(())
    }
}

/// The menu's services, without the controller's fade byte +24.
struct ResultsProfileServices<'a> {
    basis: &'a MenuBasisMath,
    screen_input: &'a mut ScreenInput,
    runtime: &'a mut InputRuntime,
    cues: &'a mut Vec<i16>,
    phase: &'a mut Option<u32>,
}

struct ActiveServices<'a, 'b> {
    services: &'a mut ResultsProfileServices<'b>,
    active: &'a mut bool,
}

impl ProfileMenuServices for ActiveServices<'_, '_> {
    type Error = IntroResultsMenuError;
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.services.basis.rotate(basis, angle);
        Ok(())
    }
    fn reset_profile_input(&mut self) -> Result<(), Self::Error> {
        self.services.screen_input.clear_held_low_word();
        self.services.runtime.reset_baselines_and_discard();
        Ok(())
    }
    fn select_camera(&mut self, _: i32) -> Result<(), Self::Error> {
        Err(IntroResultsMenuError::FrontendOnly("a frontend camera"))
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        Err(IntroResultsMenuError::FrontendOnly(
            "the frontend's transition flag 0054694C",
        ))
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        *self.services.phase = Some(phase);
        Ok(())
    }
    /// 004391D0's interface cue.
    fn play_sound(&mut self, slot: i16) -> Result<(), Self::Error> {
        self.services.cues.push(slot);
        Ok(())
    }
    /// Controller virtual +04, 004151C0.
    fn begin_controller_fade(&mut self) -> Result<(), Self::Error> {
        *self.active = false;
        Ok(())
    }
}

fn flatten_menu_error(error: MenuError<IntroResultsMenuError>) -> IntroResultsMenuError {
    match error {
        MenuError::State(error) => IntroResultsMenuError::State(error),
        MenuError::ProfileStore(error) => IntroResultsMenuError::Store(error),
        MenuError::Host(error) => error,
    }
}

fn flatten_profile_error(error: ProfileMenuError<IntroResultsMenuError>) -> IntroResultsMenuError {
    IntroResultsMenuError::Profile(match error {
        ProfileMenuError::Host(error) => return error,
        ProfileMenuError::State(error) => return IntroResultsMenuError::State(error),
        ProfileMenuError::Store(error) => return IntroResultsMenuError::Store(error),
        ProfileMenuError::UnsupportedCallback(address) => {
            ProfileMenuError::UnsupportedCallback(address)
        }
        ProfileMenuError::InvalidSlot(index) => ProfileMenuError::InvalidSlot(index),
        ProfileMenuError::MissingPendingIndex(operation) => {
            ProfileMenuError::MissingPendingIndex(operation)
        }
    })
}

/// 0042AD41..0042ADE9: read fresh mode only after binding, then borrow the
/// actual save owner. Complete working/name/classification backing is the
/// shared operation's admission, never supplied by screen defaults.
fn initialize_results_profile<H: IntroResultsFactoryHost>(
    services: &mut IntroResultsFactoryServices<'_>,
    host: &mut H,
    state: &mut SessionState,
) -> Result<(), IntroResultsScreenError<H::Error>> {
    let skipped = match services.mission.bytes_14_16.map(|bytes| bytes[1]) {
        None => Some(IntroResultsSkippedFeature::ConstructorSaveModeUnavailable),
        Some(0) => {
            let result = host.profile_store().map(|store| {
                store.serialize_current_working(WorkingProfileSerializeRequest {
                    mission: &services.mission.save,
                    elapsed_seconds: state.gameplay_time().seconds,
                    arena: services.arena,
                    classification: services.classification,
                })
            });
            match result {
                None => Some(IntroResultsSkippedFeature::ConstructorProfileStoreUnavailable),
                Some(Err(error)) => {
                    host.report_profile_failure(IntroResultsProfileOperation::Serialize, &error);
                    None
                }
                Some(Ok(())) => None,
            }
        }
        Some(_) => {
            // 00449BFB requires the restored current name only after mission
            // and the fresh optional player. The owned result ends the store
            // loan before the same host is borrowed for persistence/reporting.
            let result = host.profile_store().map(|store| {
                store.restore_current_working(WorkingProfileRestoreRequest {
                    mission: &mut services.mission.save,
                    state,
                    arena: services.arena,
                    classification: services.classification,
                })
            });
            match result {
                None => Some(IntroResultsSkippedFeature::ConstructorProfileStoreUnavailable),
                Some(Err(error)) => {
                    host.report_profile_failure(IntroResultsProfileOperation::Restore, &error);
                    None
                }
                Some(Ok(name)) => {
                    if let Err(error) = host.persist_identity(IdentityWriteRequest {
                        target: IdentityTarget::PlayerName,
                        name: &name,
                        initials: IdentityInitialsWrite::Keep,
                    }) {
                        host.report_profile_failure(IntroResultsProfileOperation::Restore, &error);
                    }
                    None
                }
            }
        }
    };
    if let Some(feature) = skipped {
        host.skip_feature(feature)
            .map_err(IntroResultsScreenError::Host)?;
    }
    Ok(())
}

/// Complete 0042AD0A..0042AD36 player policy, independent of screen eligibility.
/// The admitted lookups are pure and literal silent1 has no mutable lower
/// callback, so a nonempty list stays nonempty between the fresh lookups.
/// Raw manager rebinding/dangling storage is outside this typed owner.
fn initialize_results_player_prefix<E>(
    arena: &mut ObjectArena,
    process: Option<&ParticleFactoryProcessState>,
) -> Result<u16, IntroResultsScreenError<E>> {
    if arena.first_player().is_none() {
        return Ok(0);
    }
    let player = arena
        .first_player()
        .expect("pure results lookup preserves a nonempty player list");
    arena.apply_player_command(
        PlayerCommandRequest {
            player,
            operation: PlayerCommandOperation::Set {
                raw_id: 0x37,
                silent: 1,
            },
        },
        &PlayerCommandContext {
            process,
            executable: None,
            resident: None,
        },
    )?;
    let player = arena
        .first_player()
        .expect("silent results command preserves the player list");
    // 0042AD28 MOVSX followed by 0042AD2F's AX store retains these raw bits.
    Ok(arena.world_player(player)?.word_11c)
}

#[cfg(test)]
#[path = "intro_results_screen_tests.rs"]
mod tests;
