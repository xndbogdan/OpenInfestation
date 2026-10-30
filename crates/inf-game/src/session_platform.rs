//! Production session host over the real screens.
//!
//! `PortSession` composes the title (phase 1), frontend (phase 2),
//! text-entry (phases 9..11), briefing (phase 3) and gameplay (phases 4/6)
//! screen owners behind the SessionController: screen kind/reuse queries,
//! ordered construction/destruction through the real factories, the recovered
//! title-to-frontend and 2->9->2 transition effects, the fresh-heap world
//! backend for 9->3 (`004874D0 → 0048DC30` with zero-fill WORDs as an explicit
//! labeled approximation), and the offline game state the name-entry
//! completion consumes. The title video plays transcoded frames when the
//! `video/` store is present, else takes the native failed-open path to
//! phase 2. Unknown world subsystems fail closed per-feature (skip + log),
//! not per-phase. One platform value serves all factories and the transition
//! services, so the screens share its keyboard adapter. The linked-screen
//! routing lives in [`linked`]; the SDL window, presentation backends and
//! the scheduled loop are app-level pieces layered on top of this host.

use std::convert::Infallible;

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use crate::briefing_screen::{
    BriefingFactoryServices, BriefingScreen, BriefingScreenConfig, BriefingScreenError,
    BRIEFING_CALLBACK_CONTEXT,
};
use crate::frontend_assets::{FrontendAssets, FrontendMenuCatalog};
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_screen::{
    FrontendFactoryHost, FrontendFactoryServices, FrontendProcessState, FrontendScreen,
    FrontendScreenConfig, FrontendScreenError,
};
use crate::gameplay_screen::{
    GameplayFactoryServices, GameplayScreen, GameplayScreenConfig, GameplayScreenError,
    GAMEPLAY_CALLBACK_CONTEXT,
};
use crate::input::{InputContextId, InputRuntime};
use crate::intro_results_screen::IntroResultsScreen;
use crate::key_config_screen::{key_config_reuses_phase, KeyConfigScreen};
use crate::map_screen::{map_reuses_phase, MapScreen};
use crate::name_entry::IdentityWriteRequest;
use crate::offline_prepare::OfflineGame;
use crate::process_tasks::{ProcessTaskRuntime, TaskStorageError};
use crate::profile_name_format::ProfileNameClassification;
use crate::profile_store::ProfileStore;
use crate::scheduler::SchedulerActivity;
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenRegistry,
};
use crate::session::{
    ScreenFactory, ScreenKind, SessionHost, SessionPhase, SessionState, SessionTransitionAdmission,
    TickCallback, TransitionEffect,
};
use crate::session_effects::{apply_transition_effect, SessionEffectsError, SessionEffectsHost};
use crate::startup_screen::{startup_reuses_phase, StartupProcessState, StartupScreen};
use crate::text_entry_screen::{
    TextEntryFactoryHost, TextEntryFactoryServices, TextEntryProcessState, TextEntryScreen,
    TextEntryScreenConfig, TextEntryScreenError,
};
use crate::title_screen::{
    title_reuses_phase, TitleFactoryServices, TitleHost, TitleProcessState, TitleScreen,
    TitleScreenConfig, TitleScreenError, TitleVideoHost, TITLE_CALLBACK_CONTEXT,
};
use crate::world_backend::{WorldBackendError, WorldSession};
use crate::ResidentRuntime;

mod camera;
mod cheats;
mod countdown;
mod draw;
mod furniture;
mod intro;
#[cfg(test)]
pub(crate) use intro::required_miss_observer::{
    begin_collision_prefix_observation, finish_collision_prefix_observation,
};
mod key_config;
pub mod linked;
pub(crate) mod loading;
mod map;
mod particles;
pub use key_config::{KeyConfigSessionEffects, PortKeyConfigDraw};
pub mod software;
mod startup;
pub use startup::{StartupEffectError, StartupSessionEffects, StartupSettings};
mod banner;
mod network_game;
pub(crate) mod network_menus;
mod shown_menus;
mod world_actor_behavior;
pub(crate) mod world_actor_factory;
pub(crate) mod world_actor_update;
mod world_contact;
#[cfg(test)]
pub(crate) use world_contact::contact_model;
mod world_flash_host;
pub use world_actor_behavior::WorldFamilyBehaviorError;
pub(crate) use world_actor_behavior::WorldGrantGridHost;
pub(crate) use world_flash_host::DrawPulseHost;
mod world_owners;
pub use draw::{
    PortBriefingDraw, PortFrontendDraw, PortLinkedDrawError, PortNameEntryDraw, PortTitleDraw,
};
pub use linked::{
    BriefingLinkedError, GameplayLinkedError, KeyConfigLinkedError, LinkedActivityError,
    LinkedScreenError, PortLinked, StartupLinkedError, TitleLinkedError,
};
pub(crate) use world_actor_factory::WorldCameraFactoryContext;
pub use world_actor_factory::WorldCameraInputs;

/// The screens a session host serves. Handles stay valid while their
/// screen lives; the kernel never dereferences them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortScreen {
    Startup,
    KeyConfig,
    Title,
    Frontend,
    TextEntry,
    Briefing,
    Gameplay,
    IntroResults,
    Map,
}

/// App-level effects the session host cannot own by itself: the music
/// transport and the shared input baseline reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionMusicRequest {
    /// Native 00487310 argument (the owner adds one for the disc track).
    Select {
        native_argument: u32,
    },
    Resume,
    /// 00487B90, the CD service the world update's tail (00487B3C) and the
    /// paused tick (0043899D) run every tick: a finished track plays again.
    Service,
}

pub trait PortSessionServices {
    type Error;
    type IdentityError: std::error::Error;
    /// Required 004633D0/00463490 effect over the retained identity owners.
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError>;
    /// The actual retained session save owner. An unavailable backing is a
    /// reached feature boundary, never a replacement or fabricated store.
    fn profile_store(&mut self) -> Option<&mut ProfileStore>;
    fn session_music(&mut self, request: SessionMusicRequest) -> Result<(), Self::Error>;
    /// Shared DAT_0055F18C; ownership transfers into the retained world at load.
    fn process_rng(&mut self) -> &mut u32;
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_>;
    /// Actual short voice loan for synchronous tick and retirement callbacks.
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_>;
    /// Live Settings_GetPtr fields consumed by intro and camera callbacks.
    fn world_tick_inputs(&self) -> WorldTickPlatformInputs;
    /// `00463AB0()+84 |= bits`: the settings' cheat word, which a world
    /// gate grants (0048C407). False when the platform lends no settings.
    fn grant_cheat_flag(&mut self, bits: u32) -> bool {
        let _ = bits;
        false
    }
    /// Settings +84 as the computer's Cheats Menu reads it (`00413410`); see
    /// [`PortFixes::menu_cheat_word`](crate::frontend_settings::PortFixes::menu_cheat_word).
    fn cheats_menu_word(&self) -> u32 {
        self.world_tick_inputs().settings_84
    }
    /// The network game's link and what its protocol reads, where the
    /// platform has one.
    fn network_platform(&mut self) -> Option<crate::network::frontend_host::NetworkPlatform<'_>> {
        None
    }
    /// Port fix, not a retail setting: how the gameplay tick steps the world.
    fn world_tick_quantum(&self) -> WorldTickQuantum {
        WorldTickQuantum::Retail
    }
    /// Port setting, not a retail one: Ultra ground detail's shape when
    /// the settings take it ([`crate::terrain_runtime::ultra`]).
    fn ultra_ground(&self) -> Option<crate::terrain_runtime::ultra::UltraShape> {
        None
    }
    /// Port setting, not a retail one: whether the wheeled craft sink into
    /// the puddles' carve ([`crate::scene_ground_detail::bed_carve`]).
    fn puddle_wheels(&self) -> bool {
        false
    }
    /// Port fix, not a retail setting: the scientists', the alien ships' and
    /// the creatures' update step.
    fn actor_pacing(&self) -> ActorPacing {
        ActorPacing::Retail
    }
    /// Port fix, not a retail setting: a rescue completes as its last
    /// scientist boards (`PortFixes::mission_completion`).
    fn mission_completion(&self) -> bool {
        false
    }
    /// Port fix, not a retail setting: whether the computer's database draws
    /// a model the current world has not loaded from a world that loads it.
    fn database_models(&self) -> DatabaseModels {
        DatabaseModels::CurrentWorld
    }
    /// Port option, not a retail setting: Pelagia, the prototype world
    /// ([`crate::pelagia`]).
    fn prototype_world(&self) -> bool {
        false
    }
    /// Port option, not a retail setting: Pelagia's rework, world 1's
    /// lagoon, crystals, return gate and factory stock
    /// ([`crate::pelagia`]).
    fn pelagia_rework(&self) -> bool {
        false
    }
    /// A settings row's action from the in-world menu's Options: true when
    /// the platform ran it, false for a row it does not run in gameplay
    /// (the menu then keeps the setting unchanged).
    fn shown_menu_setting(
        &mut self,
        request: crate::menu::MenuActionRequest,
    ) -> Result<bool, String> {
        let _ = request;
        Ok(false)
    }
    /// 004F0340: re-baseline every input consumer and discard queued input.
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error>;
    /// Settings +64 and session +65C for the startup constructor.
    fn startup_settings(&self) -> StartupSettings;
    /// The live key records the gameplay constructor installs.
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings;
}

/// How the gameplay tick steps the world. Retail passes each frame's
/// delta, so a step like gravity's `delta >> 10` drops up to a millisecond
/// a frame (13% at 144 Hz), a `>> 13` timer stops below 8.192 ms, and
/// per-tick constants such as the drive's drag apply more often. The port's
/// [`PortFixes::exact_world_ticks`](crate::frontend_settings::PortFixes)
/// runs the world only in whole multiples of `micros`, carrying the rest to
/// the next frame, and the mission time carries its own milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldTickQuantum {
    Retail,
    Exact { micros: u32 },
}

/// How often the scientists (selector 12), the alien ships (selector 35)
/// and the creatures (selectors 10, 21 and 36) update. Retail updates them
/// every frame, and their gait and flight depend on the step: at 60 fps a
/// scientist's push (`00467DF0`, about 6 a update) never beats the 10 that
/// clears its forward speed, so it stands, and the ships fly at half the
/// speed of 30 fps. A creature in contact chooses its behavior at every
/// update (each bump's `00428040` sets `+22E` bit 0, and `004294A0` chooses
/// while it is set) and calls at one choice in four, so at 60 fps it calls
/// twice as often, and it walks slower. The port's
/// [`PortFixes::paced_actors`](crate::frontend_settings::PortFixes) runs
/// their updates once `micros` of world time has gathered, with all of it,
/// as retail's did at 30 fps; the rest of the world keeps its own step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorPacing {
    Retail,
    Paced { micros: u32 },
}

/// Where the computer's database finds an entry's model. Retail's H8 pool
/// loads only the current world's models (`00414C80`), so an entry whose
/// model another world brings shows its text alone. The port's
/// [`PortFixes::database_models`](crate::frontend_settings::PortFixes)
/// keeps a second residency activated at an overlay that loads the model
/// and draws it from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseModels {
    CurrentWorld,
    EveryWorld,
}

/// A residency activated at another overlay, for the database's models.
#[derive(Clone, Debug)]
pub struct DatabaseModelResidency {
    pub level: u8,
    pub runtime: ResidentRuntime,
}

/// The carries [`WorldTickQuantum::Exact`] keeps between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldTickCarry {
    /// Delta not yet given to the world.
    pub delta_micros: u32,
    /// World delta not yet counted into the mission time's milliseconds.
    pub mission_micros: u32,
    /// Real time since the world's last tick, which the input's held
    /// durations span.
    pub input_micros: u32,
    /// World time the paced actors have not had yet ([`ActorPacing`]).
    pub paced_micros: u32,
}

impl WorldTickCarry {
    /// The world's delta for a frame of `delta_micros`: under the exact
    /// quantum, the whole multiples of it the carry and the frame reach
    /// (zero skips the world's update), else the frame's own delta.
    pub fn world_delta(&mut self, quantum: WorldTickQuantum, delta_micros: i32) -> i32 {
        let WorldTickQuantum::Exact { micros } = quantum else {
            return delta_micros;
        };
        if delta_micros <= 0 {
            return delta_micros;
        }
        self.input_micros = self.input_micros.saturating_add(delta_micros as u32);
        let total = self.delta_micros + delta_micros as u32;
        let whole = total - total % micros;
        self.delta_micros = total - whole;
        whole as i32
    }

    /// The paced actors' delta for a world tick of `delta_micros`: the
    /// gathered world time once it reaches the pace, `None` before (they
    /// skip this tick), or the tick's own delta without pacing.
    pub fn paced_delta(&mut self, pacing: ActorPacing, delta_micros: u32) -> Option<u32> {
        let ActorPacing::Paced { micros } = pacing else {
            return Some(delta_micros);
        };
        let total = self.paced_micros.saturating_add(delta_micros);
        if total < micros {
            self.paced_micros = total;
            return None;
        }
        self.paced_micros = 0;
        Some(total)
    }

    /// The real time the input's held durations span for the world tick
    /// about to run, restarting the count. Zero outside the exact quantum.
    pub fn take_input_micros(&mut self) -> u32 {
        std::mem::take(&mut self.input_micros)
    }

    /// The mission time's milliseconds for a world delta: retail's
    /// truncation, or under the exact quantum the carried whole ones.
    pub fn mission_millis(&mut self, quantum: WorldTickQuantum, delta_micros: i32) -> u32 {
        match quantum {
            WorldTickQuantum::Retail => (delta_micros / 1000) as u32,
            WorldTickQuantum::Exact { .. } => {
                let total = self.mission_micros + delta_micros.max(0) as u32;
                self.mission_micros = total % 1000;
                total / 1000
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldTickPlatformInputs {
    pub camera_settings: crate::object_runtime::CameraSettings,
    /// Settings +20, the network game's respawning power-ups (0046D950).
    pub settings_20: u32,
    /// Settings +70, the terrain detail 00450370 rereads.
    pub settings_70: u32,
    /// Settings +78: 1 pauses the game on a hint.
    pub settings_78: u32,
    pub settings_7c: u32,
    /// Settings +8, the input mode 0046A260 reads: 0 keyboard only.
    pub input_mode_08: u32,
    /// Settings +84, the cheat word, as the respawn reads it (0046ADED):
    /// bit 0x40 gives the Cranial Implanter. See
    /// [`PortFixes::respawn_cheat_word`](crate::frontend_settings::PortFixes::respawn_cheat_word).
    pub settings_84: u32,
    /// Port fix, not a retail setting: the vehicle drive's position step.
    pub vehicle_position_step: crate::object_runtime::VehiclePositionStep,
}

#[derive(Debug, Error)]
pub enum PortSessionError<FH, TH, VH, S> {
    #[error(transparent)]
    Frontend(FrontendScreenError<FH>),
    #[error(transparent)]
    Text(TextEntryScreenError<TH>),
    #[error(transparent)]
    Title(TitleScreenError<VH>),
    #[error(transparent)]
    Briefing(BriefingScreenError<FH>),
    #[error(transparent)]
    Gameplay(GameplayScreenError<FH>),
    #[error("intro results: {0}")]
    IntroResults(String),
    #[error("startup screen: {0}")]
    Startup(String),
    #[error("key configuration screen: {0}")]
    KeyConfig(String),
    #[error("map screen: {0}")]
    Map(String),
    #[error("frontend load 00487380: {0}")]
    FrontendLoad(String),
    #[error("gameplay entry player lockout: {0}")]
    PlayerLockout(String),
    #[error(transparent)]
    World(#[from] WorldBackendError),
    #[error(transparent)]
    Prepare(
        #[from]
        crate::player_prepare::PlayerPrepareError<
            crate::player_input_source::PlayerInputSourceError,
        >,
    ),
    #[error(transparent)]
    PlayerRetirement(#[from] crate::object_runtime::PlayerDestructionError),
    #[error(transparent)]
    WorldCleanup(#[from] crate::object_runtime::WorldCleanupError),
    #[error("world ownership boundary: {0}")]
    WorldOwnerBoundary(&'static str),
    #[error(transparent)]
    MapCleanup(#[from] crate::world_map::WorldMapCleanupError),
    #[error(transparent)]
    MapLookup(#[from] crate::world_map::WorldMapLookupError),
    #[error(transparent)]
    Tasks(#[from] TaskStorageError),
    #[error(transparent)]
    Sound(#[from] crate::process_sound::ProcessSoundRecordError),
    #[error(transparent)]
    Particles(#[from] crate::process_particles::ParticleError),
    #[error(transparent)]
    WorldTiming(#[from] crate::process_particles::timing::TimingDivisionByZero),
    #[error(transparent)]
    Effects(#[from] SessionEffectsError<S>),
    #[error("port session serves only the startup, title, frontend, text-entry, briefing, gameplay, map and intro-results factories, not {0:?}")]
    UnsupportedFactory(ScreenFactory),
    #[error("port session has no {0:?} screen; its tick callback is not connected")]
    UnsupportedTick(TickCallback),
    #[error("port session already owns a {0:?} screen")]
    ScreenAlreadyPresent(PortScreen),
    #[error("port session owns no {0:?} screen")]
    MissingScreen(PortScreen),
    #[error("port session has not implemented the transition: {0:?}")]
    Boundary(SessionBoundary),
}

/// Transitions the port refuses before 004383B0 touches the current screen.
/// Each names the native owner still missing; none is a successful default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionBoundary {
    /// Effects/factory of this transition are not ported (for example the
    /// 2->3 00487380 restoration or the 9->3 004874D0 world load).
    Phase {
        previous: SessionPhase,
        requested: SessionPhase,
    },
    /// 2->9 with the frontend attract flag 0055F14C: 00487410's demo branch.
    AttractDemo,
    /// 0055F448 set: 00487410's network mission-selection branch.
    Network,
}

/// The single platform value behind all screen factories and the
/// transition services.
pub trait PortPlatform:
    FrontendFactoryHost
    + TextEntryFactoryHost
    + TitleHost
    + PortSessionServices
    + crate::gameplay_screen::GameplayTerrainSettings
{
}

impl<
        P: FrontendFactoryHost
            + TextEntryFactoryHost
            + TitleHost
            + PortSessionServices
            + crate::gameplay_screen::GameplayTerrainSettings,
    > PortPlatform for P
{
}

pub type PortSessionErrorOf<P> = PortSessionError<
    <P as FrontendFactoryHost>::Error,
    <P as TextEntryFactoryHost>::Error,
    <P as TitleVideoHost>::Error,
    <P as PortSessionServices>::Error,
>;

#[derive(Debug, Error)]
pub enum PortBootError {
    #[error(transparent)]
    Terrain(#[from] crate::terrain_runtime::TerrainPolicyError),
    #[error(transparent)]
    ModelProcess(#[from] crate::model_process::ModelProcessOwnerError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    CameraTables(#[from] inf_formats::FrontendError),
    #[error(transparent)]
    Loading(#[from] crate::loading_runtime::LoadingStartupError),
    #[error(transparent)]
    Tasks(#[from] TaskStorageError),
}

impl<FH, TH, VH, S> From<FrontendScreenError<FH>> for PortSessionError<FH, TH, VH, S> {
    fn from(error: FrontendScreenError<FH>) -> Self {
        PortSessionError::Frontend(error)
    }
}

impl<FH, TH, VH, S> From<TextEntryScreenError<TH>> for PortSessionError<FH, TH, VH, S> {
    fn from(error: TextEntryScreenError<TH>) -> Self {
        PortSessionError::Text(error)
    }
}

/// Teardown errors are host-free by construction (`Infallible`); map them
/// explicitly instead of overlapping the generic `From` above.
fn map_frontend_teardown<FH, TH, VH, S>(
    error: FrontendScreenError<Infallible>,
) -> PortSessionError<FH, TH, VH, S> {
    match error {
        FrontendScreenError::Host(never) => match never {},
        FrontendScreenError::Phase { expected, actual } => {
            PortSessionError::Frontend(FrontendScreenError::Phase { expected, actual })
        }
        FrontendScreenError::Registry(error) => {
            PortSessionError::Frontend(FrontendScreenError::Registry(error))
        }
        FrontendScreenError::Bindings(error) => {
            PortSessionError::Frontend(FrontendScreenError::Bindings(error))
        }
        FrontendScreenError::Input(error) => {
            PortSessionError::Frontend(FrontendScreenError::Input(error))
        }
        FrontendScreenError::Assets(error) => {
            PortSessionError::Frontend(FrontendScreenError::Assets(error))
        }
        FrontendScreenError::Residency(error) => {
            PortSessionError::Frontend(FrontendScreenError::Residency(error))
        }
        FrontendScreenError::Control(error) => {
            PortSessionError::Frontend(FrontendScreenError::Control(error))
        }
        FrontendScreenError::ColorResource(slot) => {
            PortSessionError::Frontend(FrontendScreenError::ColorResource(slot))
        }
        // Teardown releases bindings and registry membership only; it cannot
        // dispatch callbacks, so the host-free signature never yields these.
        FrontendScreenError::Dispatch(_) => {
            unreachable!("screen teardown cannot dispatch frontend callbacks")
        }
    }
}

fn map_text_teardown<FH, TH, VH, S>(
    error: TextEntryScreenError<Infallible>,
) -> PortSessionError<FH, TH, VH, S> {
    match error {
        TextEntryScreenError::Host(never) => match never {},
        TextEntryScreenError::Lifecycle { expected, actual } => {
            PortSessionError::Text(TextEntryScreenError::Lifecycle { expected, actual })
        }
        TextEntryScreenError::RequestedPhase(phase) => {
            PortSessionError::Text(TextEntryScreenError::RequestedPhase(phase))
        }
        TextEntryScreenError::Registry(error) => {
            PortSessionError::Text(TextEntryScreenError::Registry(error))
        }
        TextEntryScreenError::Bindings(error) => {
            PortSessionError::Text(TextEntryScreenError::Bindings(error))
        }
        TextEntryScreenError::Input(error) => {
            PortSessionError::Text(TextEntryScreenError::Input(error))
        }
        TextEntryScreenError::State(error) => {
            PortSessionError::Text(TextEntryScreenError::State(error))
        }
        TextEntryScreenError::KeyText(error) => {
            PortSessionError::Text(TextEntryScreenError::KeyText(error))
        }
        // Teardown releases bindings and registry membership only; it cannot
        // dispatch screen input, so the host-free signature never yields this.
        TextEntryScreenError::ScreenInput(_) => {
            unreachable!("screen teardown cannot dispatch screen input")
        }
        TextEntryScreenError::MissingKeyboard => {
            PortSessionError::Text(TextEntryScreenError::MissingKeyboard)
        }
        TextEntryScreenError::Context { expected, actual } => {
            PortSessionError::Text(TextEntryScreenError::Context { expected, actual })
        }
        TextEntryScreenError::UnexpectedAction(action) => {
            PortSessionError::Text(TextEntryScreenError::UnexpectedAction(action))
        }
    }
}

fn map_title_teardown<FH, TH, VH, S>(
    error: TitleScreenError<VH>,
) -> PortSessionError<FH, TH, VH, S> {
    match error {
        TitleScreenError::Host(error) => PortSessionError::Title(TitleScreenError::Host(error)),
        TitleScreenError::Lifecycle { expected, actual } => {
            PortSessionError::Title(TitleScreenError::Lifecycle { expected, actual })
        }
        TitleScreenError::RequestedPhase(phase) => {
            PortSessionError::Title(TitleScreenError::RequestedPhase(phase))
        }
        TitleScreenError::Registry(error) => {
            PortSessionError::Title(TitleScreenError::Registry(error))
        }
        TitleScreenError::Bindings(error) => {
            PortSessionError::Title(TitleScreenError::Bindings(error))
        }
        TitleScreenError::Input(error) => PortSessionError::Title(TitleScreenError::Input(error)),
        TitleScreenError::Context { expected, actual } => {
            PortSessionError::Title(TitleScreenError::Context { expected, actual })
        }
        // Teardown releases bindings and registry membership only; it cannot
        // dispatch input, so the host-free signature never yields this.
        TitleScreenError::UnknownCallback(_) => {
            unreachable!("screen teardown cannot dispatch title input")
        }
    }
}

fn map_briefing_teardown<FH, TH, VH, S>(
    error: BriefingScreenError<Infallible>,
) -> PortSessionError<FH, TH, VH, S> {
    match error {
        BriefingScreenError::Host(never) => match never {},
        BriefingScreenError::Terrain(error) => {
            PortSessionError::Briefing(BriefingScreenError::Terrain(error))
        }
        BriefingScreenError::Lifecycle { expected, actual } => {
            PortSessionError::Briefing(BriefingScreenError::Lifecycle { expected, actual })
        }
        BriefingScreenError::RequestedPhase(phase) => {
            PortSessionError::Briefing(BriefingScreenError::RequestedPhase(phase))
        }
        BriefingScreenError::Registry(error) => {
            PortSessionError::Briefing(BriefingScreenError::Registry(error))
        }
        BriefingScreenError::Bindings(error) => {
            PortSessionError::Briefing(BriefingScreenError::Bindings(error))
        }
        BriefingScreenError::Input(error) => {
            PortSessionError::Briefing(BriefingScreenError::Input(error))
        }
        BriefingScreenError::MissingProgram => {
            PortSessionError::Briefing(BriefingScreenError::MissingProgram)
        }
        BriefingScreenError::MissingControls => {
            PortSessionError::Briefing(BriefingScreenError::MissingControls)
        }
        BriefingScreenError::ColorResource(slot) => {
            PortSessionError::Briefing(BriefingScreenError::ColorResource(slot))
        }
        BriefingScreenError::Controls(error) => {
            PortSessionError::Briefing(BriefingScreenError::Controls(error))
        }
        BriefingScreenError::ScreenInput(error) => {
            PortSessionError::Briefing(BriefingScreenError::ScreenInput(error))
        }
    }
}

fn map_gameplay_teardown<FH, TH, VH, S>(
    error: GameplayScreenError<Infallible>,
) -> PortSessionError<FH, TH, VH, S> {
    match error {
        GameplayScreenError::Host(never) => match never {},
        GameplayScreenError::Terrain(error) => {
            PortSessionError::Gameplay(GameplayScreenError::Terrain(error))
        }
        GameplayScreenError::Lifecycle { expected, actual } => {
            PortSessionError::Gameplay(GameplayScreenError::Lifecycle { expected, actual })
        }
        GameplayScreenError::RequestedPhase(phase) => {
            PortSessionError::Gameplay(GameplayScreenError::RequestedPhase(phase))
        }
        GameplayScreenError::Registry(error) => {
            PortSessionError::Gameplay(GameplayScreenError::Registry(error))
        }
        GameplayScreenError::Bindings(error) => {
            PortSessionError::Gameplay(GameplayScreenError::Bindings(error))
        }
        GameplayScreenError::Input(error) => {
            PortSessionError::Gameplay(GameplayScreenError::Input(error))
        }
        GameplayScreenError::KeyBindings(error) => {
            PortSessionError::Gameplay(GameplayScreenError::KeyBindings(error))
        }
        GameplayScreenError::Executable(error) => {
            PortSessionError::Gameplay(GameplayScreenError::Executable(error))
        }
    }
}

/// Port custody of the single process RNG while existing world callbacks
/// borrow its word. A failed new load leaves the platform loan authoritative;
/// the previous world's success snapshot must never overwrite that prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorldRngCustody {
    Platform,
    LoadedWorld,
}

pub struct PortSession<P> {
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    residency: ResidentRuntime,
    /// Bumped whenever an installed display replaces the residency lineage.
    residency_generation: u64,
    catalog: FrontendMenuCatalog,
    process: FrontendProcessState,
    object_draw_process: crate::object_runtime::ObjectDrawProcessState,
    /// [`WorldTickQuantum::Exact`]'s carries.
    world_tick_carry: WorldTickCarry,
    /// The last gameplay tick reached no whole quantum and skipped the world.
    world_tick_skipped: bool,
    /// [`DatabaseModels::EveryWorld`]'s residency and a counter bumped when
    /// it changes, so the renderer rebinds its snapshot.
    database_models: Option<DatabaseModelResidency>,
    database_models_generation: u64,
    /// [`PortFixes::prototype_world`](crate::frontend_settings::PortFixes)'s
    /// residency, activated at the Command Centre's overlay while world 1 is
    /// loaded, so the world draw borrows its models (the perships' 665).
    pelagia_models: Option<DatabaseModelResidency>,
    model_process: crate::model_process::ModelProcessOwners,
    /// Mutable executable behavior rows/registration survive world, screen and
    /// display replacement. Reached cache writers borrow this one process owner.
    actor_behavior_tables: crate::object_runtime::ProcessActorBehaviorTables,
    /// 43F590 lazily initializes this altitude policy once for the process.
    selector35_process: crate::object_runtime::Selector35RouteProcessState,
    actor_motion: crate::object_runtime::ProcessActorMotionState,
    text_process: TextEntryProcessState,
    loading_process: crate::loading_runtime::LoadingProcessState,
    loading_reads: crate::loading_read::ResourceReadState,
    title_process: TitleProcessState,
    /// Process byte 00546B24, set by the first startup construction.
    startup_process: StartupProcessState,
    attachment: ScreenAttachment,
    startup: Option<StartupScreen>,
    key_config: Option<KeyConfigScreen>,
    title: Option<TitleScreen>,
    frontend: Option<FrontendScreen>,
    text: Option<TextEntryScreen>,
    briefing: Option<BriefingScreen>,
    gameplay: Option<GameplayScreen>,
    intro_results: Option<IntroResultsScreen>,
    map: Option<MapScreen>,
    /// Loaded world retained across briefing → gameplay. `None` until the
    /// 9→3 `LoadWorld` effect runs with the fresh-heap policy.
    world: Option<WorldSession>,
    /// The same actual manager/mission/player owners while no completed draw
    /// metadata is admitted. It is published before Prepare and retained on Err.
    pending_world: Option<crate::world_backend::WorldLoadOwner>,
    player_analog: crate::player_input_source::PlayerAnalogDescriptors,
    player_callback_next: u32,
    player_cycle: crate::attract_cycle::AttractCycleState,
    player_prepare_stopped: Option<String>,
    player_leave_state: world_owners::PlayerLeaveState,
    player_audio_5d0: world_owners::GameAudioCustody,
    /// Explicit debug selection; applied only after the native Prepare reset.
    mission_override: Option<u32>,
    /// The start harness's state ([`crate::harness_start`]) and how far its
    /// load has gone.
    harness_start: Option<crate::harness_start::HarnessStart>,
    harness_stage: HarnessStage,
    /// CRT 0044FCF0 owns the process paired-marker header; terrain plane and
    /// grid54ED78 also outlive each map/world/display allocation.
    maps: crate::world_map::ProcessWorldMapRuntime,
    /// Native mission+04 publication plus a port-only retained constructor
    /// prefix. A failed load must not discard either allocation custody.
    map_load: crate::world_map::MapLoadState,
    world_rng_custody: WorldRngCustody,
    /// Aggregate process dependency acquisition 00478A70→00482C40. The
    /// 0055F100 allocations survive world replacement; task execution remains
    /// a separate required owner. Initial source 0054E894 is BSS zero.
    tasks: ProcessTaskRuntime,
    /// G+0, the recharging grenades' countdown: 2 s from the game object's
    /// constructor (00487BE0), then 4 s each grant.
    network_recharge: i32,
    /// A stopped network tick is logged once.
    network_stop_logged: bool,
    /// 0054ED18, the network keys' countdown.
    network_key_countdown: u32,
    /// `0054E71C`, each slot's initials as the game protocol holds them,
    /// for the vehicles' name tags.
    network_initials: [[u8; 6]; crate::network::game::SLOTS],
    /// `0054E81C`, the host's team flag, for Powerball's vehicles' draw.
    network_team_flag: u8,
    /// Session constructor 00438640 constructs G+80 through 00487BE0 and
    /// publishes it through 00487180. Screen/world replacement keeps this
    /// allocation;004871C0 clears admitted particles rather than rebuilding it.
    particles: crate::process_particles::ProcessParticleRuntime,
    /// Process00546E08/10 pending furniture records; native 00436B80 clears
    /// this queue on world teardown, independently of process allocation.
    furniture: crate::world_furniture_effect::FurnitureEffectQueue,
    /// Process 00546B58/00546B88 bases, seeded by service 00533228's
    /// 00435690 acquisition. World teardown clears the queue separately.
    furniture_draw: crate::terrain_furniture::draw::FurnitureDrawState,
    furniture_failure: Option<String>,
    /// A failed required callback retains its prefix and stops this feature
    /// until the owning world is cleared; other world features still run.
    /// Process00548404 grid/anchors survive world replacement. Each world
    /// draw clears cells, then publishes anchors independently.
    light_grid: crate::world_lighting::WorldLightGrid,
    ///0047FF60's positional one-shots retain H9 allocation custody until
    ///00480110 services them during draw; PCM voices have separate lifetime.
    sounds: crate::process_sound::ProcessSoundRuntime,
    /// Process 0040FD10 state survives screen destruction and world reload.
    camera_globals: crate::object_runtime::CameraGlobalState,
    /// 0053FF00 from the same executable retained by the asset catalog.
    camera_inverse_sine: [i16; 1024],
    /// Authored process BYTE0053A128, read by alive Selector12 motion467DF0.
    /// Seeded once from the executable; world/screen/display replacement keeps
    /// this owner. No recovered direct native writer is currently represented.
    surface_snap_53a128: u8,
    /// Horizontal sampling globals survive world and screen replacement.
    terrain_policy: crate::terrain_runtime::ProcessTerrainPolicy,
    /// Process005483C0 survives world/screen/display changes. World loads
    /// publish through the same WORD, including reached failure prefixes.
    furniture_radius: crate::terrain_runtime::ProcessFurnitureRadius,
    ///Shared fallback/lazy query storage survives world, screen and renderer
    ///replacement. Queue cleanup does not reset this process owner.
    furniture_query: crate::world_furniture_effect::ProcessFurnitureQueryState,
    /// Port option, not retail: the remastered camera's own copy of the
    /// furniture queries' process state, so its sight tests leave the
    /// game's untouched, and the lift it keeps between frames
    /// ([`crate::camera_clearance`]).
    camera_furniture_query: crate::world_furniture_effect::ProcessFurnitureQueryState,
    camera_clearance: crate::camera_clearance::CameraClearance,
    /// Current process0055F280, seeded once by CRT48F630→402690. Known
    /// consumers borrow it; screen/world/display replacement never resets it.
    /// Unmodeled computed native rebindings remain a separate writer domain.
    process_up: [f32; 3],
    /// 005483B0 BSS-zero start. Native event writers remain required owners.
    camera_shake_enabled: u32,
    /// Current semantic 00545C74 binding, seeded once from the same supported
    /// file-backed EXE and retained across world/screen/display replacement.
    /// Unmodeled native rebindings are separate reached ownership boundaries.
    profile_name_classification: ProfileNameClassification,
    platform: P,
    /// 0055F14C: the single owner. The frontend root tick writes it and both
    /// 00487410 and the text-entry tick read it. A module's initialise
    /// (004852D0) clears it, as the session starts it.
    pub attract: bool,
    /// One live owner of raw network0055F448 and NULL0054E894, initialized
    /// from the supported executable's BSS startup. Reached services may
    /// mutate it; phase remains unavailable until its separate owner is wired.
    pub factory_process: crate::process_particles::factories::ParticleFactoryProcessState,
    /// 00487410 byte +5C2 / 004871C0 clears it.
    owner_byte: bool,
    offline_game: Option<OfflineGame>,
    /// The Cheats Menu's process bytes `00546A04` and `0052F880`, which
    /// outlive worlds; each world's message environment gets a copy.
    computer_cheats: crate::object_runtime::ComputerCheats,
}

impl<P> PortSession<P> {
    /// The residency must already be booted (Select then Boot); the frontend
    /// factory activates leaf 47 on top of it. `context` is the linked context
    /// owner's display projection. Player grants remain lazy reads in the
    /// actual constructor; startup does not precompute a saved-player template.
    /// The same input/source owners survive any later PreparePhase9 failure.
    pub fn new(
        assets: FrontendAssets,
        residency: ResidentRuntime,
        platform: P,
        context: FrontendRenderContext,
        mission_override: Option<u32>,
    ) -> Result<Self, PortBootError> {
        let camera_inverse_sine = inf_formats::parse_frontend_inverse_sine_table(
            FrontendExecutable::parse(assets.executable_bytes())?.data(),
            inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )?
        .values;
        let factory_process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let profile_name_classification = match ProfileNameClassification::native_startup(
            FrontendExecutable::parse(assets.executable_bytes())?,
        ) {
            Ok(classification) => classification,
            Err(error) => {
                // Missing source custody stops only the reached formatting
                // feature. No default table is substituted for current data.
                eprintln!("ProfileNameClassificationUnavailable: {error}");
                ProfileNameClassification { high_bytes: None }
            }
        };
        let player_analog =
            match crate::player_input_source::PlayerAnalogDescriptors::native_startup(
                assets.executable_bytes(),
            ) {
                Ok(current) => current,
                Err(error) => {
                    eprintln!("PlayerAnalogDescriptorsUnavailable: {error}");
                    crate::player_input_source::PlayerAnalogDescriptors::from_current([None; 3])
                }
            };
        let mut tasks = ProcessTaskRuntime::default();
        tasks.retain(
            factory_process
                .process_null_handle
                .expect("known native startup NULL"),
        )?;
        let sounds = crate::process_sound::ProcessSoundRuntime::new(FrontendExecutable::parse(
            assets.executable_bytes(),
        )?)?;
        let furniture_draw = crate::terrain_furniture::draw::FurnitureDrawState::new(
            &inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values),
        );
        let loading_process =
            crate::loading_runtime::LoadingProcessState::native_startup(assets.executable_bytes())?;
        let object_draw_process = crate::object_runtime::ObjectDrawProcessState::native_startup(
            FrontendExecutable::parse(assets.executable_bytes())?,
        )?;
        let actor_behavior_tables =
            crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                FrontendExecutable::parse(assets.executable_bytes())?,
            )?;
        let actor_motion = crate::object_runtime::ProcessActorMotionState::from_executable(
            FrontendExecutable::parse(assets.executable_bytes())?,
        )?;
        let surface_snap_53a128 = FrontendExecutable::parse(assets.executable_bytes())?
            .mapped(0x0053_a128, 1)?
            .bytes[0];
        let computer_cheats =
            cheats::computer_cheats_startup(FrontendExecutable::parse(assets.executable_bytes())?)?;
        let terrain_policy = crate::terrain_runtime::ProcessTerrainPolicy::native_startup(
            FrontendExecutable::parse(assets.executable_bytes())?,
        )?;
        let furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
            FrontendExecutable::parse(assets.executable_bytes())?,
        )?;
        let furniture_query =
            crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(
                FrontendExecutable::parse(assets.executable_bytes())?,
            )?;
        let camera_furniture_query =
            crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(
                FrontendExecutable::parse(assets.executable_bytes())?,
            )?;
        let model_process = crate::model_process::ModelProcessOwners::native_startup(
            FrontendExecutable::parse(assets.executable_bytes())?,
        )?;
        //00438640 owns the admitted initial renderer; later scene/epoch loans
        //do not reconstruct its546E60 selected table.
        model_process
            .selected_renderer_constructed()
            .map_err(crate::model_process::ModelProcessOwnerError::from)?;
        let catalog = FrontendMenuCatalog::new(assets);
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(context),
        };
        eprintln!(
            "FreshParticlePoolZeroApproximation: particle pool payload bytes start zero, as Infest01's fresh pool block did; cell 32 +00..+17 stay unknown"
        );
        Ok(Self {
            registry,
            input: InputRuntime::new(0),
            residency,
            residency_generation: 0,
            catalog,
            process: FrontendProcessState::default(),
            object_draw_process,
            world_tick_carry: WorldTickCarry::default(),
            world_tick_skipped: false,
            database_models: None,
            database_models_generation: 0,
            pelagia_models: None,
            model_process,
            actor_behavior_tables,
            selector35_process: crate::object_runtime::Selector35RouteProcessState::default(),
            actor_motion,
            text_process: TextEntryProcessState::default(),
            loading_process,
            loading_reads: crate::loading_read::ResourceReadState::native_startup(),
            title_process: TitleProcessState::default(),
            startup_process: StartupProcessState::default(),
            attachment,
            startup: None,
            key_config: None,
            title: None,
            frontend: None,
            text: None,
            briefing: None,
            gameplay: None,
            intro_results: None,
            map: None,
            world: None,
            pending_world: None,
            player_analog,
            player_callback_next: world_owners::PLAYER_CALLBACK_CONTEXT_START,
            player_cycle: crate::attract_cycle::AttractCycleState::from_native_constructor_fields(),
            player_prepare_stopped: None,
            player_leave_state: world_owners::PlayerLeaveState::NotStarted,
            player_audio_5d0: world_owners::GameAudioCustody::ConstructorAbsent,
            mission_override,
            harness_start: None,
            harness_stage: HarnessStage::Idle,
            maps: crate::world_map::ProcessWorldMapRuntime::native_startup(),
            map_load: crate::world_map::MapLoadState::native_mission_constructor(),
            world_rng_custody: WorldRngCustody::Platform,
            tasks,
            network_recharge: 2_000_000,
            network_initials: [[0; 6]; crate::network::game::SLOTS],
            network_team_flag: 0,
            network_stop_logged: false,
            network_key_countdown: 0,
            particles:
                crate::process_particles::ProcessParticleRuntime::fresh_pool_zero_approximation(),
            furniture: crate::world_furniture_effect::FurnitureEffectQueue::default(),
            furniture_draw,
            furniture_failure: None,
            light_grid: crate::world_lighting::WorldLightGrid::default(),
            sounds,
            camera_globals: crate::object_runtime::CameraGlobalState::INITIAL,
            camera_inverse_sine,
            surface_snap_53a128,
            terrain_policy,
            furniture_radius,
            furniture_query,
            camera_furniture_query,
            camera_clearance: Default::default(),
            process_up: [0.0, 1.0, 0.0],
            camera_shake_enabled: 0,
            profile_name_classification,
            platform,
            attract: false,
            factory_process,
            owner_byte: false,
            offline_game: None,
            computer_cheats,
        })
    }

    fn retire_world_map(&mut self) -> Result<(), crate::world_map::WorldMapCleanupError> {
        crate::world_map::retire_current_world_map(crate::world_map::MapLoadContext {
            process: &mut self.maps,
            state: &mut self.map_load,
        })
    }

    pub fn input(&mut self) -> &mut InputRuntime {
        &mut self.input
    }
    pub fn registry(&mut self) -> &mut ScreenRegistry<FrontendRenderContext, InputContextId> {
        &mut self.registry
    }
    /// The database's second residency, while one is loaded.
    pub fn database_model_residency(&self) -> Option<&ResidentRuntime> {
        self.database_models.as_ref().map(|models| &models.runtime)
    }

    pub fn database_models_generation(&self) -> u64 {
        self.database_models_generation
    }

    /// Pelagia's second residency, activated at the Command Centre's overlay
    /// while world 1 is loaded, so the world draw borrows its models.
    pub fn pelagia_model_residency(&self) -> Option<&ResidentRuntime> {
        self.pelagia_models.as_ref().map(|models| &models.runtime)
    }

    pub fn residency(&self) -> &ResidentRuntime {
        &self.residency
    }
    /// Identifies the residency lineage; see [`Self::install_display`].
    pub fn residency_generation(&self) -> u64 {
        self.residency_generation
    }
    pub fn assets(&self) -> &FrontendAssets {
        self.catalog.assets()
    }
    pub fn process_tasks(&self) -> &ProcessTaskRuntime {
        &self.tasks
    }
    pub fn process_tasks_mut(&mut self) -> &mut ProcessTaskRuntime {
        &mut self.tasks
    }
    /// The game's animation clock (50 a second), as the world's draw takes
    /// it: the sky's lightning and the water's waves run on it.
    pub fn animation_value(&self) -> u32 {
        self.process.animation().value()
    }
    pub fn process_particles(&self) -> &crate::process_particles::ProcessParticleRuntime {
        &self.particles
    }
    /// The terrain furniture's bases (`00546B88`), which turn each item by
    /// its record's state.
    pub fn furniture_draw(&self) -> &crate::terrain_furniture::draw::FurnitureDrawState {
        &self.furniture_draw
    }
    pub fn process_particles_mut(
        &mut self,
    ) -> &mut crate::process_particles::ProcessParticleRuntime {
        &mut self.particles
    }
    pub fn frontend_screen(&self) -> Option<&FrontendScreen> {
        self.frontend.as_ref()
    }
    pub fn title_screen(&self) -> Option<&TitleScreen> {
        self.title.as_ref()
    }
    pub fn title_process(&self) -> &TitleProcessState {
        &self.title_process
    }
    pub fn text_screen(&self) -> Option<&TextEntryScreen> {
        self.text.as_ref()
    }
    pub fn briefing_screen(&self) -> Option<&BriefingScreen> {
        self.briefing.as_ref()
    }
    pub fn gameplay_screen(&self) -> Option<&GameplayScreen> {
        self.gameplay.as_ref()
    }
    pub fn intro_results_screen(&self) -> Option<&IntroResultsScreen> {
        self.intro_results.as_ref()
    }
    pub fn world(&self) -> Option<&WorldSession> {
        self.world.as_ref()
    }
    pub fn world_mut(&mut self) -> Option<&mut WorldSession> {
        self.world.as_mut()
    }
    pub fn model_process(&self) -> &crate::model_process::ModelProcessOwners {
        &self.model_process
    }
    pub fn frontend_process(&self) -> &FrontendProcessState {
        &self.process
    }
    pub fn attachment(&self) -> ScreenAttachment {
        self.attachment
    }
    pub fn platform(&self) -> &P {
        &self.platform
    }
    pub fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }
    /// 00487410 byte +5C2, read by the profile/name-entry owners.
    pub fn owner_byte(&self) -> bool {
        self.owner_byte
    }

    /// The briefing's store on the first player's message +1 (the player
    /// 0040A8CC captured). A world without a player has no message to set.
    fn briefing_player_message(
        &mut self,
        operation: crate::object_runtime::PlayerMessageOperation<'static>,
    ) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let Some(player) = world.arena.first_player() else {
            return;
        };
        let result = world
            .arena
            .player_message(player)
            .map(|message| message.identity());
        let result = match result {
            Ok(recipient) => world
                .arena
                .apply_player_message(crate::object_runtime::PlayerMessageRequest {
                    recipient,
                    operation,
                })
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = result {
            eprintln!("Briefing player message {operation:?} failed: {error}");
        }
    }
    /// The offline state 00487410 published, cleared by 004871C0.
    pub fn offline_game(&self) -> Option<&OfflineGame> {
        self.offline_game.as_ref()
    }
    /// 0055F160 byte +5C4, infrared vision's request, which makes 00438120
    /// present packed black. It outlives the world that set it, until the
    /// next world load clears it.
    pub fn gameplay_forces_black(&self) -> bool {
        self.camera_globals.infrared_5c4.get()
    }

    /// Port fix: under [`WorldTickQuantum::Exact`], a gameplay pass whose
    /// world tick was skipped neither draws nor presents, so the draw's own
    /// steps (the flash's sequence, the view focal's easing, the draw's
    /// random draws) run once per world tick, as retail's did at 60 fps.
    pub fn holds_gameplay_frame(&self, state: &SessionState) -> bool {
        self.world_tick_skipped && state.tick_callback() == TickCallback::Gameplay
    }

    /// Commit a prepared display (00437DF0): the linked context's projection
    /// and the residency selected for its tier. Renderers bound to a snapshot
    /// of the previous lineage must be retired; the previous residency is
    /// returned to the caller.
    pub fn install_display(
        &mut self,
        residency: ResidentRuntime,
        context: FrontendRenderContext,
    ) -> Result<ResidentRuntime, RegistryError> {
        self.registry
            .replace_backend(self.attachment.context_owner, context)?;
        self.residency_generation += 1;
        Ok(std::mem::replace(&mut self.residency, residency))
    }

    /// 004945D5..004945E3: the selected input object 0055F4B0 belongs to
    /// this session.
    pub fn input_is_selected(&self) -> bool {
        self.registry.selected_screen().is_some_and(|screen| {
            self.registry.screen(screen).is_ok_and(|registration| {
                registration.attachment.session == self.attachment.session
            })
        })
    }

    /// The owned screen the registry links for the second pass, resolved
    /// afresh for every callback as 004943A0 rereads context +18.
    pub fn linked_screen(&self) -> Result<PortScreen, LinkedScreenError> {
        let id = self
            .registry
            .linked_screen(self.attachment.context_owner)?
            .ok_or(LinkedScreenError::Unlinked)?;
        if self.frontend.as_ref().and_then(FrontendScreen::id) == Some(id) {
            Ok(PortScreen::Frontend)
        } else if self.text.as_ref().and_then(TextEntryScreen::id) == Some(id) {
            Ok(PortScreen::TextEntry)
        } else if self.title.as_ref().and_then(TitleScreen::id) == Some(id) {
            Ok(PortScreen::Title)
        } else if self.briefing.as_ref().and_then(BriefingScreen::id) == Some(id) {
            Ok(PortScreen::Briefing)
        } else if self.gameplay.as_ref().and_then(GameplayScreen::id) == Some(id) {
            Ok(PortScreen::Gameplay)
        } else if self.intro_results.as_ref().and_then(IntroResultsScreen::id) == Some(id) {
            Ok(PortScreen::IntroResults)
        } else if self.map.as_ref().and_then(MapScreen::id) == Some(id) {
            Ok(PortScreen::Map)
        } else if self.startup.as_ref().and_then(StartupScreen::id) == Some(id) {
            Ok(PortScreen::Startup)
        } else if self.key_config.as_ref().and_then(KeyConfigScreen::id) == Some(id) {
            Ok(PortScreen::KeyConfig)
        } else {
            Err(LinkedScreenError::Foreign(id))
        }
    }

    /// Base +10/+14 of both screens (004947F0/ 00494810) for the linked
    /// screen: resume selects its input context, suspend deselects it.
    pub fn linked_activity(
        &mut self,
        activity: SchedulerActivity,
    ) -> Result<(), LinkedActivityError> {
        let screen = match self.linked_screen()? {
            PortScreen::Startup => self.startup.as_ref().and_then(StartupScreen::id),
            PortScreen::KeyConfig => self.key_config.as_ref().and_then(KeyConfigScreen::id),
            PortScreen::Title => self.title.as_ref().and_then(TitleScreen::id),
            PortScreen::Frontend => self.frontend.as_ref().and_then(FrontendScreen::id),
            PortScreen::TextEntry => self.text.as_ref().and_then(TextEntryScreen::id),
            PortScreen::Briefing => self.briefing.as_ref().and_then(BriefingScreen::id),
            PortScreen::Gameplay => self.gameplay.as_ref().and_then(GameplayScreen::id),
            PortScreen::IntroResults => {
                self.intro_results.as_ref().and_then(IntroResultsScreen::id)
            }
            PortScreen::Map => self.map.as_ref().and_then(MapScreen::id),
        }
        .expect("linked screens are registered");
        let input = &mut self.input;
        let result = match activity {
            SchedulerActivity::Resume => self
                .registry
                .select_input(screen, |context| input.select_context(context)),
            SchedulerActivity::Suspend => self
                .registry
                .deselect_input(screen, |context| input.select_context(context)),
        };
        result.map_err(|error| match error {
            RegistryCallbackError::Registry(error) => LinkedActivityError::Registry(error),
            RegistryCallbackError::Callback(error) => LinkedActivityError::Input(error),
        })
    }

    /// Port-only admission (see `SessionFrameHost::admit_transition`): the
    /// transitions whose effects and factories exist. Phase 2 from the
    /// port's reachable phases, 2->9 without the attract or network branches
    /// of 00487410 (checked in attract-then-network order), 9->3 via the
    /// fresh-heap world backend, 3->4 via authored briefing completion, and
    /// the bounded intro-only 4->8->3 continuation. Other world phases stay
    /// explicit boundaries.
    pub fn admit_transition(&self, state: &SessionState) -> Result<(), SessionBoundary> {
        let previous = state.current_phase();
        let requested = state.requested_phase();
        match requested {
            SessionPhase::Frontend => Ok(()),
            // Options > Language (00453C70 with 0) and its accept, which
            // returns through the title to the rebuilt frontend.
            SessionPhase::Startup if previous == SessionPhase::Frontend => Ok(()),
            SessionPhase::Title if previous == SessionPhase::Startup => Ok(()),
            // Options > Controls row 3 (00453C70 with 0x0C); its Back returns
            // to phase 2 through the frontend arm above.
            SessionPhase::Phase12 if previous == SessionPhase::Frontend => Ok(()),
            // 00487410 takes the network world (host: its arena's, guest:
            // the host's) where a session runs.
            SessionPhase::Phase9 if previous == SessionPhase::Frontend => {
                if self.attract {
                    Err(SessionBoundary::AttractDemo)
                } else {
                    Ok(())
                }
            }
            // Name entry → world load → briefing, with fresh-heap WORDs as an
            // explicit labeled approximation (allocator history is a follow-up).
            // Requires the offline game published by 00487410.
            SessionPhase::Phase3
                if matches!(previous, SessionPhase::Phase9 | SessionPhase::Phase8) =>
            {
                if self.offline_game.is_none() {
                    Err(SessionBoundary::Phase {
                        previous,
                        requested,
                    })
                } else {
                    Ok(())
                }
            }
            // Profile menu Load (operation 1, after 0044A170): 00487380 restores
            // the working profile, then LoadWorld enters its saved selection.
            // Unported worlds still load and stop at their own boundaries.
            SessionPhase::Phase3 if previous == SessionPhase::Frontend => {
                if self.factory_process.network_flag != 0 {
                    Err(SessionBoundary::Network)
                } else {
                    Ok(())
                }
            }
            // Options > Network's name and initials rows (00453C70 with 10/11)
            // reuse the text-entry owner; completion writes the net identity
            // and requests phase 2. Neither needs a network session.
            SessionPhase::Phase10 | SessionPhase::Phase11 if previous == SessionPhase::Frontend => {
                Ok(())
            }
            // Empty intro briefing and visible briefing share native completion;
            // authored records exhausting alone never request gameplay.
            SessionPhase::Gameplay if previous == SessionPhase::Phase3 => Ok(()),
            // The computer's database and hint list (00414480, 004147D0)
            // request phase 4 when they close, though the game may still be
            // in it: 00438380 accepts the current phase, and 004383B0 keeps
            // the gameplay screen, stores the 500 ms lockout and resets the
            // input baselines.
            SessionPhase::Gameplay if previous == SessionPhase::Gameplay => Ok(()),
            // A hint pauses the game (phase 6, the reused gameplay screen) and
            // its expiry or dismissal returns to phase 4.
            SessionPhase::Phase6 if previous == SessionPhase::Gameplay => Ok(()),
            SessionPhase::Gameplay if previous == SessionPhase::Phase6 => Ok(()),
            // A second hint asks for phase 6 again once the first has paused
            // the game: 00438380 takes any request when none is pending, and
            // the gameplay screen's +1C accepts 6 again.
            SessionPhase::Phase6 if previous == SessionPhase::Phase6 => Ok(()),
            // Escape's in-world menu (00443070) over the gameplay screen, also
            // during a hint's pause (phase 6, the same screen); its menu's
            // destructor 00443400 returns to the phase it was built over, 6 as
            // 4, and Quit requests phase 2.
            SessionPhase::Phase5
                if matches!(previous, SessionPhase::Gameplay | SessionPhase::Phase6) =>
            {
                Ok(())
            }
            SessionPhase::Gameplay if previous == SessionPhase::Phase5 => Ok(()),
            // The computer's "Map and Objectives" (00415170), shown over the
            // running world or a hint's pause, and the map screen's return
            // (004520A0, 00438380(4)).
            SessionPhase::Phase7
                if matches!(previous, SessionPhase::Gameplay | SessionPhase::Phase6) =>
            {
                Ok(())
            }
            SessionPhase::Gameplay if previous == SessionPhase::Phase7 => Ok(()),
            // 00487560's world end: the results, then the next world's
            // briefing from the selection it stored.
            SessionPhase::Phase8
                if previous == SessionPhase::Gameplay
                    && self.offline_game.is_some()
                    && self.world.is_some() =>
            {
                Ok(())
            }
            // The port's Teleport to world (the Cheats Menu with all cheats):
            // the world it stored loads and its briefing shows, as after the
            // frontend's Load, with no results page.
            SessionPhase::Phase3
                if previous == SessionPhase::Gameplay
                    && self.offline_game.is_some()
                    && self.world.is_some() =>
            {
                Ok(())
            }
            _ => Err(SessionBoundary::Phase {
                previous,
                requested,
            }),
        }
    }

    /// AttractDemoDeferredApproximation: keep the live menu usable while the
    /// initial attract player preparation, world load and gameplay owners
    /// remain unavailable. Only this pending idle-demo request is deferred; other
    /// transition boundaries still fail before native screen teardown.
    pub fn admit_frame_transition(
        &mut self,
        state: &SessionState,
    ) -> Result<SessionTransitionAdmission, SessionBoundary> {
        if state.transition_pending()
            && state.current_phase() == SessionPhase::Frontend
            && state.requested_phase() == SessionPhase::Phase9
            && self.attract
            && self
                .frontend
                .as_mut()
                .is_some_and(FrontendScreen::reset_idle_after_attract_deferral)
        {
            self.attract = false;
            eprintln!(
                "Infestation: idle attract demo deferred (AttractDemoDeferredApproximation); native demo preparation is not owned"
            );
            return Ok(SessionTransitionAdmission::Deferred);
        }
        self.admit_transition(state)
            .map(|()| SessionTransitionAdmission::Proceed)
    }

    /// Host-free teardown of whichever screens remain, including a partial
    /// factory's retained acquisitions, for a stopped or finished run.
    pub fn shutdown(&mut self) -> Result<(), PortSessionErrorOf<P>>
    where
        P: PortPlatform,
    {
        if self.startup.is_some() {
            self.destroy_startup()?;
        }
        if self.key_config.is_some() {
            self.destroy_key_config()?;
        }
        if let Some(mut owner) = self.title.take() {
            owner
                .destroy(&mut self.registry, &mut self.input, &mut self.platform)
                .map_err(map_title_teardown)?;
        }
        if let Some(mut owner) = self.text.take() {
            owner
                .destroy(&mut self.registry, &mut self.input)
                .map_err(map_text_teardown)?;
        }
        if let Some(mut owner) = self.frontend.take() {
            owner
                .destroy(&mut self.registry, &mut self.input)
                .map_err(map_frontend_teardown)?;
        }
        if let Some(mut owner) = self.briefing.take() {
            owner
                .destroy(&mut self.registry, &mut self.input)
                .map_err(map_briefing_teardown)?;
        }
        if let Some(mut owner) = self.gameplay.take() {
            owner
                .destroy(&mut self.registry, &mut self.input)
                .map_err(map_gameplay_teardown)?;
        }
        if let Some(mut owner) = self.intro_results.take() {
            owner
                .destroy::<_, Infallible>(&mut self.registry, &mut self.input)
                .map_err(|error| PortSessionError::IntroResults(error.to_string()))?;
        }
        if let Some(mut owner) = self.map.take() {
            owner
                .destroy(&mut self.registry, &mut self.input)
                .map_err(|error| PortSessionError::Map(error.to_string()))?;
        }
        // Mission destruction48DAD9 reaches the same local map cleanup as
        // world retirement. Process-global44FD30 remains a separate owner.
        self.retire_world_map()?;
        if self.tasks.references() != 0 {
            self.tasks.release()?;
        }
        Ok(())
    }

    /// Whether the Video menu carries the port's Vulkan rows.
    pub fn set_modern_rows(&mut self, on: bool) {
        self.catalog.set_modern_rows(on);
    }

    /// Disjoint borrows for one linked callback: routing state, the live
    /// residency (cues, display resets) and the platform.
    pub fn split(&mut self) -> PortSplit<'_, P> {
        PortSplit {
            linked: PortLinked {
                registry: &mut self.registry,
                input: &mut self.input,
                catalog: &mut self.catalog,
                process: &self.process,
                text_process: &mut self.text_process,
                attachment: self.attachment,
                startup: self.startup.as_mut(),
                key_config: self.key_config.as_mut(),
                title: self.title.as_mut(),
                frontend: self.frontend.as_mut(),
                text: self.text.as_mut(),
                briefing: self.briefing.as_mut(),
                gameplay: self.gameplay.as_mut(),
                intro_results: self.intro_results.as_mut(),
                map: self.map.as_mut(),
                attract: &mut self.attract,
                factory_process: &self.factory_process,
                terrain_policy: &mut self.terrain_policy,
                owner_byte: self.owner_byte,
                offline_game: self.offline_game.as_ref(),
                profile_name_classification: &self.profile_name_classification,
            },
            residency: &mut self.residency,
            platform: &mut self.platform,
            world: &mut self.world,
            sounds: &mut self.sounds,
        }
    }
}

/// See [`PortSession::split`].
pub struct PortSplit<'a, P> {
    pub linked: PortLinked<'a>,
    /// Mutable for the startup accept's 004EE160 reselect; other screens
    /// only read it (cues, display resets).
    pub residency: &'a mut ResidentRuntime,
    pub platform: &'a mut P,
    pub world: &'a mut Option<crate::world_backend::WorldSession>,
    pub sounds: &'a mut crate::process_sound::ProcessSoundRuntime,
}

impl<P: PortPlatform> PortSession<P> {
    /// Port-authored H0 strings for the current draw, if Pelagia is on and
    /// loaded: the briefing, the map and the menus name world 1 in the
    /// residency's language. Anywhere else retail's bytes read as ever.
    pub fn world_strings(&self) -> Option<crate::render_resources::WorldStrings> {
        if !self.platform.prototype_world() {
            return None;
        }
        if self
            .world
            .as_ref()
            .map(|world| world.mission.current_world_a0)
            != Some(crate::pelagia::WORLD)
        {
            return None;
        }
        Some(crate::render_resources::WorldStrings {
            world: crate::pelagia::WORLD,
            language: self.residency.summary().selection.language,
            rework: self.platform.pelagia_rework(),
        })
    }
    /// The process words the network session sets: the flag `0055F448`, this
    /// machine's slot `0055F2F4` (1 offline) and the host flag `0055F3A0`.
    pub fn sync_network_process(&mut self) {
        let Some(network) = self.platform.network_platform() else {
            return;
        };
        let state = &network.link.transport.state;
        self.factory_process.network_flag = u32::from(state.network_active);
        self.factory_process.local_slot = if state.network_active {
            state.local_slot
        } else {
            1
        };
        self.factory_process.host_flag = u32::from(state.network_active && state.host);
        self.factory_process
            .settings
            .copy_from_slice(&network.settings.words()[..20]);
    }
    /// Construct the frontend screen outside the controller, for tests that
    /// drive transitions manually. Production boots the title screen and
    /// reaches the frontend through the real scheduler transition.
    pub fn boot_frontend(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        if self.frontend.is_some() {
            return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Frontend));
        }
        self.construct_frontend()
    }

    /// Boot through the real title factory `00404A10` while the session is
    /// still in its initial Title/Title state with no pending request. The
    /// video backend reports absent, so the factory takes the failed-open
    /// path and makes no phase request; the first linked tick then finishes
    /// to phase 2 and the scheduler transitions to the frontend. A failed
    /// factory keeps its partial acquisitions for explicit teardown through
    /// `shutdown`, as the construction contract requires.
    ///
    /// Retail boot `004389C0` constructs the startup screen first, and its
    /// constructor sets process byte `00546B24` whatever settings say. The
    /// port's boot stands in for that construction, so it keeps the byte:
    /// a later Options > Language visit then starts with the fades complete.
    pub fn boot_title(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        if self.title.is_some() {
            return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Title));
        }
        self.startup_process.constructed = true;
        self.construct_title(&mut SessionState::default())
    }

    fn construct_title(&mut self, state: &mut SessionState) -> Result<(), PortSessionErrorOf<P>> {
        let mut screen = TitleScreen::new(
            TitleScreenConfig {
                attachment: self.attachment,
                callback_context: TITLE_CALLBACK_CONTEXT,
            },
            self.catalog.assets(),
        );
        let result = screen.initialize(
            &mut TitleFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets: self.catalog.assets(),
                process: &mut self.title_process,
            },
            &mut self.platform,
            state,
        );
        self.title = Some(screen);
        result.map_err(PortSessionError::Title)
    }

    fn construct_frontend(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        let mut screen = FrontendScreen::new(
            FrontendScreenConfig {
                attachment: self.attachment,
                callback_context: FRONTEND_CALLBACK_CONTEXT,
                native_context_high_byte: None,
            },
            self.catalog.assets(),
        );
        let result = screen.initialize(
            &mut FrontendFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                residency: &mut self.residency,
                catalog: &mut self.catalog,
                process: &self.process,
            },
            &mut self.platform,
        );
        // A failed factory keeps its partial acquisitions for explicit
        // teardown through `shutdown`, as the construction contract requires.
        self.frontend = Some(screen);
        result.map_err(PortSessionError::Frontend)
    }
}

/// How far the start harness has taken the session: requested, loading
/// its world, then handed to gameplay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HarnessStage {
    Idle,
    Pending,
    Loading,
    Done,
}

impl<P: PortPlatform> PortSession<P> {
    /// Start the session in the harness's world and state
    /// ([`crate::harness_start`]) instead of waiting at the frontend.
    pub fn set_harness_start(&mut self, start: crate::harness_start::HarnessStart) {
        self.harness_start = Some(start);
        self.harness_stage = HarnessStage::Pending;
    }

    /// The start harness takes the phases a loaded save takes: from the
    /// frontend to the load (phase 3, as the profile menu's load requests),
    /// then from the briefing to gameplay (as "Start Mission" requests),
    /// dropping its spawn once the world has used it.
    pub(crate) fn advance_harness(&mut self, state: &mut SessionState) {
        if state.transition_pending() {
            return;
        }
        match (self.harness_stage, state.current_phase()) {
            // The title's own request for the frontend, as a key makes it.
            (HarnessStage::Pending, SessionPhase::Title) => {
                state.request_phase(SessionPhase::Frontend);
            }
            (HarnessStage::Pending, SessionPhase::Frontend) => {
                if state.request_phase(SessionPhase::Phase3) {
                    // The profile menu's load sets 0054694C, so the loading
                    // screen shows its equalizer instead of the ACAM model.
                    self.platform.loading_platform().mode.set(1);
                    self.harness_stage = HarnessStage::Loading;
                }
            }
            (HarnessStage::Loading, SessionPhase::Phase3) => {
                // A briefing worked by keys ends through its own Start.
                if self
                    .harness_start
                    .as_ref()
                    .and_then(|start| start.briefing_keys)
                    == Some(true)
                {
                    return;
                }
                // The start block's briefing frames hold the briefing first.
                if let Some(frames) = self
                    .harness_start
                    .as_mut()
                    .and_then(|start| start.briefing_frames.as_mut())
                    .filter(|frames| **frames > 0)
                {
                    *frames -= 1;
                    return;
                }
                if state.request_phase(SessionPhase::Gameplay) {
                    // The briefing's Start Mission 0040D510 resets the
                    // animation clock (0048E7F0 -> 00439530) after it.
                    self.process.reset_animation();
                    self.harness_stage = HarnessStage::Done;
                    if let Some(world) = self.world.as_mut() {
                        world.world_start.harness_spawn = None;
                    }
                }
            }
            // The briefing finished by itself (`briefing_keys`); its finish
            // reset the animation clock already.
            (HarnessStage::Loading, SessionPhase::Gameplay) => {
                self.harness_stage = HarnessStage::Done;
                if let Some(world) = self.world.as_mut() {
                    world.world_start.harness_spawn = None;
                }
            }
            _ => {}
        }
    }
}

/// Opaque callback identities of the two portable screens, not addresses.
pub const FRONTEND_CALLBACK_CONTEXT: u32 = 1;
pub const TEXT_ENTRY_CALLBACK_CONTEXT: u32 = 2;

impl<P: PortPlatform> SessionEffectsHost for PortSession<P> {
    type Error = PortSessionErrorOf<P>;
    fn prepare_offline_game(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        self.prepare_offline_player(state)
    }
    fn clear_prepared_world(&mut self, state: &SessionState) -> Result<(), Self::Error> {
        self.clear_constructed_world(state)
    }
    fn select_frontend_music(&mut self) -> Result<(), Self::Error> {
        self.platform
            .session_music(SessionMusicRequest::Select { native_argument: 7 })
            .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))
    }
    fn reset_input_baselines(&mut self) -> Result<(), Self::Error> {
        self.platform
            .reset_input_baselines(&mut self.input)
            .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))
    }
}

impl<P: PortPlatform> SessionHost for PortSession<P> {
    type Screen = PortScreen;
    type ScreenContext = ScreenAttachment;
    type Error = PortSessionErrorOf<P>;

    fn current_screen(&self) -> Option<Self::Screen> {
        if self.frontend.is_some() {
            Some(PortScreen::Frontend)
        } else if self.text.is_some() {
            Some(PortScreen::TextEntry)
        } else if self.title.is_some() {
            Some(PortScreen::Title)
        } else if self.briefing.is_some() {
            Some(PortScreen::Briefing)
        } else if self.gameplay.is_some() {
            Some(PortScreen::Gameplay)
        } else if self.intro_results.is_some() {
            Some(PortScreen::IntroResults)
        } else if self.map.is_some() {
            Some(PortScreen::Map)
        } else if self.startup.is_some() {
            Some(PortScreen::Startup)
        } else if self.key_config.is_some() {
            Some(PortScreen::KeyConfig)
        } else {
            None
        }
    }

    fn screen_kind(
        &mut self,
        screen: Self::Screen,
        _: &mut SessionState,
    ) -> Result<ScreenKind, Self::Error> {
        Ok(match screen {
            PortScreen::Startup => ScreenKind::Startup,
            PortScreen::KeyConfig => ScreenKind::Phase12,
            PortScreen::Title => ScreenKind::Title,
            PortScreen::Frontend => ScreenKind::Frontend,
            PortScreen::TextEntry => ScreenKind::TextEntry,
            PortScreen::Briefing => ScreenKind::Phase3,
            PortScreen::Gameplay => ScreenKind::Gameplay,
            PortScreen::IntroResults => ScreenKind::Phase8,
            PortScreen::Map => ScreenKind::Phase7,
        })
    }

    fn screen_context(&self, _: PortScreen) -> Self::ScreenContext {
        self.attachment
    }

    fn reuse_screen(
        &mut self,
        screen: Self::Screen,
        requested: SessionPhase,
        state: &mut SessionState,
    ) -> Result<bool, Self::Error> {
        Ok(match screen {
            PortScreen::Startup => startup_reuses_phase(requested),
            PortScreen::KeyConfig => key_config_reuses_phase(requested),
            PortScreen::Title => title_reuses_phase(requested),
            // 00483B80 accepts phase 2 only.
            PortScreen::Frontend => requested == SessionPhase::Frontend,
            PortScreen::TextEntry => matches!(
                requested,
                SessionPhase::Phase9 | SessionPhase::Phase10 | SessionPhase::Phase11
            ),
            PortScreen::Briefing => crate::briefing_screen::briefing_reuses_phase(requested),
            PortScreen::Gameplay if requested == SessionPhase::Phase5 => {
                // InWorldMenuScreenReuse: retail replaces the gameplay screen
                // with the phase-5 screen 00443190 (whose reuse 00443330
                // accepts phase 5 only, and whose destructor 00443360 closes
                // the menu through 004133C0 and releases its bindings before
                // its base's), whose constructor shows
                // the in-world menu over the same world; the port keeps the
                // gameplay screen and shows the menu here, with that screen's
                // input.
                self.show_in_world_menu(state.current_phase());
                self.enter_in_world_menu_input()?;
                true
            }
            PortScreen::Gameplay => {
                let reused = crate::gameplay_screen::gameplay_reuses_phase(requested);
                if reused {
                    self.leave_in_world_menu_input()?;
                }
                reused
            }
            // 0042AE00 accepts phase 8 only.
            PortScreen::IntroResults => requested == SessionPhase::Phase8,
            PortScreen::Map => map_reuses_phase(requested),
        })
    }

    fn destroy_screen(
        &mut self,
        screen: Self::Screen,
        _: &mut SessionState,
    ) -> Result<(), Self::Error> {
        match screen {
            PortScreen::Startup => self.destroy_startup()?,
            PortScreen::KeyConfig => self.destroy_key_config()?,
            PortScreen::Title => {
                let mut owner = self
                    .title
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::Title))?;
                owner
                    .destroy(&mut self.registry, &mut self.input, &mut self.platform)
                    .map_err(map_title_teardown)?;
            }
            PortScreen::Frontend => {
                let mut owner = self
                    .frontend
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::Frontend))?;
                owner
                    .destroy(&mut self.registry, &mut self.input)
                    .map_err(map_frontend_teardown)?;
            }
            PortScreen::TextEntry => {
                let mut owner = self
                    .text
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::TextEntry))?;
                owner
                    .destroy(&mut self.registry, &mut self.input)
                    .map_err(map_text_teardown)?;
            }
            PortScreen::Briefing => {
                let mut owner = self
                    .briefing
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::Briefing))?;
                // 0040AA79: the teardown enables the same player's message
                // before releasing its bindings.
                self.briefing_player_message(crate::object_runtime::PlayerMessageOperation::Enable);
                owner
                    .destroy(&mut self.registry, &mut self.input)
                    .map_err(map_briefing_teardown)?;
            }
            PortScreen::Gameplay => {
                let mut owner = self
                    .gameplay
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::Gameplay))?;
                owner
                    .destroy(&mut self.registry, &mut self.input)
                    .map_err(map_gameplay_teardown)?;
            }
            PortScreen::IntroResults => {
                let mut owner = self
                    .intro_results
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::IntroResults))?;
                owner
                    .destroy::<_, Infallible>(&mut self.registry, &mut self.input)
                    .map_err(|error| PortSessionError::IntroResults(error.to_string()))?;
            }
            PortScreen::Map => {
                let mut owner = self
                    .map
                    .take()
                    .ok_or(PortSessionError::MissingScreen(PortScreen::Map))?;
                owner
                    .destroy(&mut self.registry, &mut self.input)
                    .map_err(|error| PortSessionError::Map(error.to_string()))?;
            }
        }
        Ok(())
    }

    fn network_session_active(&self) -> bool {
        self.factory_process.network_flag != 0
    }

    fn transition_effect(
        &mut self,
        effect: TransitionEffect,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        match effect {
            // Native preparation owns the same arena before any fallible load.
            // The shared backend keeps reached mission/map/manager prefixes.
            TransitionEffect::LoadWorld => {
                self.load_world_effect(state, loading::WorldLoadPresentation::NoCallbacks)
            }
            TransitionEffect::PrepareFromFrontend => self.prepare_frontend_load(state),
            TransitionEffect::SelectWorldMusic => {
                let world = self
                    .world
                    .as_ref()
                    .ok_or(PortSessionError::World(WorldBackendError::MissingWorld))?;
                // 0048E780 reads signed H14 header WORD +14; 00487250 adds one.
                self.platform
                    .session_music(SessionMusicRequest::Select {
                        native_argument: world.music_native_argument,
                    })
                    .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))?;
                // 00487250 resumes inside the selection routine.
                self.platform
                    .session_music(SessionMusicRequest::Resume)
                    .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))
            }
            TransitionEffect::ResumeWorldMusic => self
                .platform
                .session_music(SessionMusicRequest::Resume)
                .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error))),
            // 004383B0's gameplay entry: the first player's +F4 lockout.
            TransitionEffect::SetPlayerTimer { microseconds } => {
                if let Some(world) = self.world.as_mut() {
                    if let Some(player) = world.arena.first_player() {
                        world
                            .arena
                            .set_player_lockout_f4(player, microseconds)
                            .map_err(|error| PortSessionError::PlayerLockout(error.to_string()))?;
                    }
                }
                Ok(())
            }
            TransitionEffect::PrepareResults => self.prepare_intro_results(state),
            // Residency ownership note: the title factory loads no resources, the
            // frontend factory activates leaf 47 and the retail sequence keeps
            // leaf 0 resident; the text-entry phases load no additional
            // resources, so 1->2->9->2 leaves the same resident set. World phases
            // change residency via `LoadWorld` above.
            _ => match apply_transition_effect(effect, self, state) {
                Ok(()) => Ok(()),
                Err(SessionEffectsError::Host(error)) => Err(error),
                Err(SessionEffectsError::Unsupported(effect)) => Err(PortSessionError::Effects(
                    SessionEffectsError::Unsupported(effect),
                )),
            },
        }
    }

    fn create_screen(
        &mut self,
        factory: ScreenFactory,
        _: Self::ScreenContext,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        match factory {
            ScreenFactory::Startup => {
                if self.startup.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Startup));
                }
                self.construct_startup(state)?;
            }
            ScreenFactory::Phase12 => {
                if self.key_config.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(
                        PortScreen::KeyConfig,
                    ));
                }
                self.construct_key_config(state)?;
            }
            ScreenFactory::Title => {
                if self.title.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Title));
                }
                self.construct_title(state)?;
            }
            ScreenFactory::Frontend => {
                if self.frontend.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Frontend));
                }
                self.construct_frontend()?;
            }
            ScreenFactory::TextEntry => {
                if self.text.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(
                        PortScreen::TextEntry,
                    ));
                }
                let mut screen = TextEntryScreen::new(
                    TextEntryScreenConfig {
                        attachment: self.attachment,
                        callback_context: TEXT_ENTRY_CALLBACK_CONTEXT,
                    },
                    self.catalog.assets(),
                );
                let result = screen.initialize(
                    state.requested_phase(),
                    &mut TextEntryFactoryServices {
                        registry: &mut self.registry,
                        input: &mut self.input,
                        assets: self.catalog.assets(),
                        process: &self.process,
                        local_slot: self.factory_process.local_slot,
                    },
                    &mut self.platform,
                );
                self.text = Some(screen);
                result.map_err(PortSessionError::Text)?;
            }
            ScreenFactory::Phase3 => {
                if self.briefing.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Briefing));
                }
                let templates = self.catalog.briefing_templates().map_err(|error| {
                    PortSessionError::Frontend(FrontendScreenError::Assets(error))
                })?;
                let world = self
                    .world
                    .as_ref()
                    .ok_or(PortSessionError::World(WorldBackendError::MissingWorld))?;
                let program = world.briefing.clone();
                let mission_selection = world.mission.save.mission_selection;
                let program = crate::pelagia::briefing_program_for(
                    mission_selection,
                    self.platform.prototype_world(),
                    self.platform.pelagia_rework(),
                    &program,
                );
                let mut screen = BriefingScreen::new(
                    BriefingScreenConfig {
                        attachment: self.attachment,
                        callback_context: BRIEFING_CALLBACK_CONTEXT,
                    },
                    self.catalog.assets(),
                );
                screen.bind_world(
                    crate::briefing_screen::BriefingWorldBinding {
                        program,
                        mission_selection,
                        milestone_count: world.briefing_milestone_count,
                        list_close_millis: crate::pelagia::list_close_millis(
                            mission_selection,
                            self.platform.prototype_world(),
                            self.platform.pelagia_rework(),
                        ),
                    },
                    templates,
                );
                let result = screen.initialize(
                    &mut BriefingFactoryServices {
                        terrain: crate::terrain_runtime::TerrainDetailContext {
                            process: &mut self.terrain_policy,
                            world_flags: self.world.as_ref().map(|world| world.world_flags),
                            terrain: self.world.as_mut().map(|world| &mut world.terrain),
                        },
                        registry: &mut self.registry,
                        input: &mut self.input,
                        assets: self.catalog.assets(),
                        residency: &self.residency,
                        process: &self.process,
                    },
                    &mut self.platform,
                    state,
                );
                self.briefing = Some(screen);
                result.map_err(PortSessionError::Briefing)?;
                // 0040A947: the constructor suppresses the first player's
                // message once its own menu is registered.
                self.briefing_player_message(
                    crate::object_runtime::PlayerMessageOperation::Suppress,
                );
            }
            ScreenFactory::Gameplay => {
                if self.gameplay.is_some() {
                    return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Gameplay));
                }
                let mut screen = GameplayScreen::new(
                    GameplayScreenConfig {
                        attachment: self.attachment,
                        callback_context: GAMEPLAY_CALLBACK_CONTEXT,
                    },
                    self.catalog.assets(),
                );
                let key_bindings = self.platform.key_bindings();
                // 00488AA0's first player owns the +0xFC sub-object context.
                let player_context = self.world.as_ref().and_then(|world| {
                    let first = world.arena.first_player()?;
                    world
                        .constructions
                        .iter()
                        .find(|construction| construction.player() == first)
                        .map(|construction| construction.callback_context())
                });
                let result = screen.initialize(
                    &mut GameplayFactoryServices {
                        terrain: crate::terrain_runtime::TerrainDetailContext {
                            process: &mut self.terrain_policy,
                            world_flags: self.world.as_ref().map(|world| world.world_flags),
                            terrain: self.world.as_mut().map(|world| &mut world.terrain),
                        },
                        registry: &mut self.registry,
                        input: &mut self.input,
                        assets: self.catalog.assets(),
                        key_bindings: &key_bindings,
                        player_context,
                    },
                    &mut self.platform,
                    state,
                );
                self.gameplay = Some(screen);
                result.map_err(PortSessionError::Gameplay)?;
            }
            ScreenFactory::Phase7 => self.construct_map(state)?,
            ScreenFactory::Phase8 => self.construct_intro_results(state)?,
            other => return Err(PortSessionError::UnsupportedFactory(other)),
        }
        Ok(())
    }

    fn tick(
        &mut self,
        callback: TickCallback,
        delta_micros: i32,
        elapsed_micros: i32,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        match callback {
            TickCallback::Idle => Ok(()),
            // `004388D0` world update (`004875A0`) plus elapsed-time accounting.
            TickCallback::Gameplay => {
                // A host without the poll's own drain
                // (`PortSession::after_input_poll`) still runs the keys'
                // menu commands before the world's update.
                self.after_input_poll(state);
                // Port fix: the exact quantum steps the world in whole
                // quanta and skips a frame that reaches none.
                let quantum = self.platform.world_tick_quantum();
                let frame_delta = delta_micros;
                let delta_micros = self.world_tick_carry.world_delta(quantum, delta_micros);
                let elapsed_micros = if delta_micros == frame_delta {
                    elapsed_micros
                } else {
                    delta_micros
                };
                self.world_tick_skipped = delta_micros == 0 && frame_delta > 0;
                if self.world_tick_skipped {
                    // The vehicle reads held keys at the next tick, over
                    // this frame's time too.
                    self.input.carry_durations();
                    return Ok(());
                }
                let input_span = self.world_tick_carry.take_input_micros();
                if input_span != 0 {
                    self.input.scale_durations(delta_micros as u32, input_span);
                }
                // 004875BE..004875CE: infrared vision takes its request.
                self.camera_globals.infrared_5c3 = self.camera_globals.infrared_5c4.get();
                // Port setting: Ultra ground detail, offline only, as a
                // network game takes the host's detail.
                let ultra = self
                    .platform
                    .ultra_ground()
                    .filter(|_| self.factory_process.network_flag == 0);
                if let Some(world) = self.world.as_mut() {
                    world.terrain.set_ultra(ultra);
                }
                // Port setting: the wheels sink into the puddles' carve,
                // offline only (a network game's weather is retail's).
                let puddle_wheels =
                    self.platform.puddle_wheels() && self.factory_process.network_flag == 0;
                if let Some(world) = self.world.as_mut() {
                    let carve = puddle_wheels
                        .then(|| {
                            let plane =
                                (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
                            crate::scene_ground_detail::CarveLook::for_world(
                                world.weather.puddles(),
                                f32::from(plane),
                                &crate::modern_tuning::current()
                                    .water(world.mission.current_world_a0),
                            )
                        })
                        .flatten();
                    world.terrain.set_puddle_carve(carve);
                }
                if let Some(world) = self.world.as_mut() {
                    // 004875A0 records capped/raw timing before 00439560.
                    world
                        .fade
                        .performance
                        .tick(delta_micros as u32, elapsed_micros as u32)?;
                }
                // 004875A0 calls 00439560 before mission/object/camera work.
                // Use the existing process clock, not the load-time receipt.
                self.process.advance_animation(delta_micros as u32);
                // The countdown's explosions and the unit's end, which need
                // the world's factories (0048E380..0048E515).
                let mut countdown_unit = None;
                let mut tick_millis = 0;
                if let Some(world) = self.world.as_mut() {
                    // 0048E312..0048E335: the mission time G+1E8 gains the
                    // capped delta's whole milliseconds first.
                    let time = world
                        .mission
                        .dword_38
                        .as_mut()
                        .expect("loaded world prelude");
                    let millis = self.world_tick_carry.mission_millis(quantum, delta_micros);
                    *time = time.wrapping_add(millis);
                    tick_millis = millis;
                    // 0048E33E..0048E51A: the mission's countdown. Only a
                    // running one looks up the first player's unit
                    // (00488AA0, 0046AE60).
                    let unit = match world.arena.first_player() {
                        Some(player) if world.mission.dword_24 != Some(0) => {
                            world.arena.player_actor(player)
                        }
                        _ => Ok(None),
                    };
                    use crate::world_mission::MissionCountdownTick;
                    match unit.map_err(|error| error.to_string()).and_then(|unit| {
                        world
                            .mission
                            .tick_countdown(delta_micros as u32, unit.is_some())
                            .map(|tick| (unit, tick))
                            .map_err(|error| error.to_string())
                    }) {
                        Ok((
                            Some(unit),
                            tick @ (MissionCountdownTick::Explosions
                            | MissionCountdownTick::UnitEnd),
                        )) => countdown_unit = Some((unit, tick)),
                        Ok((_, MissionCountdownTick::MissionFailed { world: selection })) => {
                            // 0048E4F8: 00487560(W+A0, 1, 0), the results with
                            // +15 set ("Mission Failed"), then this world again.
                            let outcome = crate::world_completion::complete_world(
                                &mut world.mission,
                                &mut world.arena,
                                crate::world_completion::WorldCompletionRequest {
                                    next_selection: selection,
                                    start_ordinal: 1,
                                    actor: None,
                                },
                                crate::world_completion::WorldCompletionInputs {
                                    network_active: self.factory_process.network_flag != 0,
                                    process: Some(&self.factory_process),
                                },
                                &mut self.sounds,
                            );
                            eprintln!("World mission countdown ended the world into {selection}: {outcome:?}");
                            if let Ok(crate::world_completion::WorldCompletionOutcome::Accepted {
                                request_phase,
                            }) = outcome
                            {
                                let _ = state.request_phase(request_phase);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => eprintln!("World mission countdown stopped: {error}"),
                    }
                }
                if let Some((unit, tick)) = countdown_unit {
                    use crate::world_mission::MissionCountdownTick;
                    match tick {
                        MissionCountdownTick::Explosions => self.countdown_explosion(unit, state),
                        _ => {
                            self.countdown_retire_unit(unit, state);
                            if let Some(world) = self.world.as_mut() {
                                world.mission.arm_doom_last_seconds();
                            }
                        }
                    }
                }
                if let Some(world) = self.world.as_mut() {
                    // 0048E521..0048E6BA: the mission's timer +2C and the
                    // world tint's schedule.
                    crate::world_tint::tick_world_tint(
                        world,
                        tick_millis,
                        self.camera_globals.infrared_5c3,
                        &self.residency,
                    );
                    // Port option: the weather's phases, fades and wetness;
                    // the ground soaks only where the rain falls (Pelagia's
                    // waits for the night).
                    let shown = crate::world_scene::rain_shown(
                        world,
                        &self.residency,
                        self.platform.pelagia_rework(),
                    );
                    world.weather.tick(tick_millis as f32 / 1000.0, shown);
                    // Manager 0048E2E0 consumes the just-written timing
                    // feedback before actor/camera work, after 00439560.
                    world.mission.dword_1c = crate::world_fade::update_manager_far(
                        world.mission.dword_1c,
                        world.mission.dword_20.expect("loaded world header"),
                        world.fade.performance.particle_load_factor as i32,
                    );
                    //0048E73D rotates the process pickup basis after the far
                    //distance store and before the map's +24 decay callback.
                    self.object_draw_process.advance_pickup_rotation(
                        delta_micros,
                        &inf_formats::native_basis::NativeBasisMath::new(
                            self.catalog.assets().sine_table().values,
                        ),
                    );
                    // 004875A0 calls mission 0048E2E0 before actor/camera
                    // work. Its map +24 decay uses the signed scheduler delta.
                    // The slot can be cleared during results while the
                    // arena/player save carrier remains retained.
                    if let Some(map) = self.map_load.current() {
                        self.maps.jitter_mut(map)?.decay(delta_micros);
                    }
                }
                self.lend_network_game();
                //0048770C pending furniture effects precede actor callbacks.
                self.tick_furniture_effects(delta_micros as u32, state);
                self.tick_world_actor_runtime(delta_micros as u32, state);
                self.apply_terrain_event();
                //00487758/68: current camera acquisition and pose follow the
                // object/collision boundary for every loaded world.
                self.tick_world_camera(delta_micros as u32);
                //0048778D/95: pending transfer and physical particle update
                // follow actor/collision/camera work, before elapsed accounting.
                self.tick_process_particles(delta_micros, state);
                self.apply_terrain_event();
                self.return_network_game();
                // 004878A0..00487982: the network branch.
                self.tick_network(delta_micros as u32, state);
                // 00487B2E..00487B41: every update ends in the music service.
                self.platform
                    .session_music(SessionMusicRequest::Service)
                    .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))?;
                if let Some(world) = self.world.as_mut() {
                    if !world.tick_skips_logged {
                        eprintln!("World tick skipped features: native mission/task work and player pre/post-update work remain open; supported disposal, shared actor callbacks, the 004791F0/004790C0 contact passes, intro/controller/helpers and the shared camera phase execute separately (ACTOR_BEHAVIOR_RUNTIME.md, WORLD_CONTACT.md, INTRO_SEQUENCE.md, CAMERA_RUNTIME.md)");
                        world.tick_skips_logged = true;
                    }
                    // 004388D0 increments session+248 after world update.
                    // That DWORD is serialized mission W+34. Project the
                    // controller's post-success value into its save view;
                    // the controller commits the sole persistent counter
                    // immediately after this required callback succeeds.
                    let mut elapsed = state.gameplay_time();
                    elapsed.advance(frame_delta);
                    world.mission.save.elapsed_seconds = elapsed.seconds;
                }
                self.dispatch_shown_menu_actions(state);
                self.note_crash_state(state);
                Ok(())
            }
            // `00438950`: the first player's message and the controlled
            // unit's sound, then `00487B90`; the world does not update.
            TickCallback::Player => {
                self.after_input_poll(state);
                self.tick_in_world_menu_input(delta_micros, elapsed_micros, state)?;
                self.tick_paused_player(delta_micros, state);
                self.platform
                    .session_music(SessionMusicRequest::Service)
                    .map_err(|error| PortSessionError::Effects(SessionEffectsError::Host(error)))?;
                self.dispatch_shown_menu_actions(state);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "session_platform_tests.rs"]
mod tests;

#[cfg(test)]
mod selector12_surface_snap_native_tests;
