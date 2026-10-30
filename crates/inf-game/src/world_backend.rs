//! Session world backend for `004874D0 → 0048DC30` (`LoadWorld`).
//!
//! Order (retail `0048DC30`, offline, non-attract):
//! mission prelude (`+A6`, `+3C` mark, `+38`, `+A0`), header-derived mission
//! fields, placement resets, each placement's H13 factory with `0047F6C0` on
//! actors, `00404D40`, drain, both `+20` passes, player branch with `0046A890`,
//! mission tail. This backend activates resources (`48`, then `selection+10`)
//! and the lower retained policy consumes real post-map `00478CD0`. Sound/frame
//! setup, attract/network branches and the briefing callback remain separate.
//!
//! Allocation WORDs use the documented fresh-heap policy
//! (`crate::fresh_heap`): zero-fill `[0,0,0]` for every construction's
//! pre-context bytes, as an explicit labeled approximation. Allocator-history
//! ownership (which freed block each construction receives) remains a
//! follow-up. Where a subsystem is unknown, construction fails closed
//! per-feature (the placement is skipped and logged), not per-phase.
//!
//! Actual arena/mission/start/listener/player-constructor owners and process
//! RNG come from the caller. Other unowned load inputs use the named
//! `IntroCampaignReloadApproximation`: it reuses the first-load evidence for
//! the reachable intro-to-level-1 reload. This is replaceable scaffolding,
//! never general campaign or retail acceptance. `world_backend` owns the
//! follow-up for allocator history, settings, listener and clock inputs. Explicit
//! saved imports keep their clean-arena policy; retained loads consume the native
//! prelude before activation and real post-map manager cleanup.

use inf_formats::{
    native_basis::NativeBasisMath, GrantTable, MissionSaveState, PlayerSaveState,
    VEHICLE_BEHAVIOR_TABLE_DWORDS,
};
use thiserror::Error;

use crate::fresh_heap::fresh_allocation_words;
use crate::object_runtime::{
    ActorConstructionRequest, ActorTerrainInputs, ActorTerrainSurface, CameraSettings,
    CreationListener, CreationListenerEffects, CreationListenerError, CreationListeners,
    FamilyUpdateClass, FamilyUpdateError, NativeFamilyUpdateError, ObjectArena, ObjectId,
    PickupTables, PlayerId, PlayerRespawn,
};
#[cfg(test)]
use crate::terrain_runtime::TerrainSpacing;
use crate::world_fade::{WorldFadeState, WorldVisibilityReload};
use crate::world_loader::{
    load_world, WorldLoadInputs, WorldLoadReport, WorldLoadState, WorldLoaderError,
};
use crate::world_mission::{WorldMission, WorldMissionError};
use crate::world_placement::{WorldPlacementError, WorldPlacements};
use crate::world_start::WorldStartState;
#[cfg(test)]
use crate::Installation;
#[cfg(test)]
use crate::ResidencyRequest;
use crate::ResidentRuntime;

mod owners;
#[cfg(test)]
mod radius_tests;
pub use owners::{
    WorldLoadOwner, WorldLoadOwnerOrigin, WorldLoadPhase, WorldLoadPrepared, WorldLoadStatus,
};

/// Loaded world retained across briefing → gameplay.
#[derive(Debug)]
pub struct WorldSession {
    pub load_approximation: IntroCampaignReloadApproximation,
    /// H14 +28/+2C authored briefing, from this world's original allocation.
    pub briefing: inf_formats::BriefingProgram,
    /// H14 header +24, the briefing progress-marker count (separate from +28).
    /// The HUD shows the player's lost vehicles (+11C) against it, and the
    /// camera's respawn `0046A890` ends the game once the losses reach it.
    pub briefing_milestone_count: u32,
    /// H14 header +0, the bonus time `0048E290` counts down.
    pub bonus_time_00: u32,
    /// The respawn's H14 header inputs: DWORD +18 (the fallback vehicle
    /// selection) and the signed bytes +0E/+0F (the actor mask limits).
    pub respawn_header: WorldRespawnHeader,
    /// 00487250 sign-extends the H14 header WORD +14 and adds one.
    pub music_native_argument: u32,
    /// Live objects (118 placements + camera + vehicle).
    pub arena: ObjectArena,
    pub mission: WorldMission,
    /// 004874D0 seeds G+5C8/+5CC from the loaded manager; 004875A0 timing
    /// feedback and 00488380 predraw slew retain state across frames.
    pub fade: WorldFadeState,
    ///00487D70 publishes the flash sequence/color;488020 consumes it after
    ///particle transfer. The same owner survives mission reload.
    pub flash: crate::world_flash::WorldFlashState,
    pub world_start: WorldStartState,
    /// Process RNG `0055F18C` carried across loads.
    pub rng: u32,
    /// The world ticks' delta since the last light pass: `00478F20` stores
    /// each tick's in `0055F0E0`, which the draw's lights read (the ECM
    /// Unit's glow, the vehicle's pulse). The port's frames can draw without
    /// a tick, or after two, so each tick adds its delta and the light pass
    /// takes the sum: one tick a frame, it is that tick's.
    pub light_delta: u32,
    pub listeners: CreationListeners,
    pub origin: WorldLoadOwnerOrigin,
    pub constructions: Vec<crate::object_runtime::WorldPlayerConstruction>,
    pub prepare_phase: crate::player_prepare::PlayerPreparePhase,
    pub load_status: WorldLoadStatus,
    pub report: WorldLoadReport,
    /// Respawned vehicle, when a first player existed.
    pub respawn: Option<PlayerRespawn>,
    /// First player object, for input/camera routing.
    pub player: Option<PlayerId>,
    /// Owned terrain for height sampling in tick/draw (cloned from placements,
    /// independent from OVL bytes).
    pub terrain: crate::terrain_runtime::TerrainRuntime,
    /// Actual immutable allocation/registration from which terrain was prepared.
    /// Live actor acquisitions authenticate it against fresh session residency.
    pub(crate) terrain_source: crate::world_actor_terrain::WorldTerrainSource,
    /// Captured allocation identity from the actual mission+04 publication.
    /// The retained process map runtime owns its authoritative jitter/links.
    pub map: crate::world_map::WorldMapId,
    /// Retained H14 flags, used by helper admission and live camera terrain.
    pub world_flags: u32,
    ///0044FDD0 publishes resident H15/0 at00548394. These five DWORDs
    /// select ground/water impact responses; missing residency disables only
    /// reached particle terrain consumers, never invents material selectors.
    pub particle_impact_selectors: Option<[u32; 5]>,
    /// Actual retained tint 005474E0 and authored terrain lighting. Each world
    /// tick sets the tint from [`Self::tint_schedule`] (0048E2E0); the
    /// vehicle's and the HUD's draws publish a shaded copy and restore it.
    pub presentation: WorldPresentationState,
    /// `0053DED0`, the world tint's own word, which the load sets to H6 0x37
    /// and each world tick recomputes from the schedule.
    pub scheduled_tint: u32,
    /// H14's tint schedule (`0048E534`).
    pub tint_schedule: crate::world_tint::WorldTintSchedule,
    /// Selector-88 production attachment, selected from its retained script.
    pub intro_runtime: Option<IntroRuntimeState>,
    /// Shared callback custody independent of a selector88 intro attachment.
    pub actor_updates: WorldActorUpdateState,
    /// Port-only custody of the non-resumable post-effects retirement pass.
    /// A reached failure remains attached to this actual arena.
    pub owner_retirement: WorldOwnerRetirement,
    /// Diagnostic last acquisition identity. Tick and Draw each freshly call
    /// 00478E00; Draw never treats this field as camera admission or pose success.
    pub native_camera: Option<ObjectId>,
    /// Once-per-reason diagnostic, independent of intro-controller presence.
    pub camera_failure: Option<String>,
    /// Explicit owner of the arcade control adapter for this loaded scene.
    pub drive_policy: ArcadeDrivePolicy,
    /// Port option, not retail: the weather of a world that rains or snows
    /// ([`crate::world_weather`]), rolled for each visit; only the drawn
    /// and heard world reads it.
    pub weather: crate::world_weather::WorldWeather,
    /// Screen draw `00442080` focal globals `00547660/64` (BSS-zero start).
    /// Advanced by the native view setup on every world draw.
    pub view_focals: [i32; 2],
    /// Port option: the same slew for the view shown, toward its own focal
    /// (the remastered camera's wider one), where it differs from the
    /// game's.
    pub shown_focals: [i32; 2],
    /// Per-feature skip log for the native draw; logged once, then counted.
    pub draw_skip_logged: bool,
    /// Object/reason pairs already logged by the object draw and light
    /// passes. A later object, or a new reason, is still reported.
    pub draw_omissions_logged: std::collections::BTreeSet<String>,
    pub last_draw: Option<crate::world_scene::WorldDrawReceipt>,
    /// Port-only once-per-world diagnostics for unowned native simulation.
    pub tick_skips_logged: bool,
    /// The port's own: the scenery crystals' flares where the guns' bolts
    /// strike them, for the tracer's crystals.
    pub crystal_flares: crate::crystal_flares::CrystalFlares,
    /// The port's own: where the craft the camera follows has flown, along
    /// which the modern sky's clouds thin ([`crate::cloud_wake`]).
    pub cloud_wake: crate::cloud_wake::CloudWake,
    /// The port's own: the spray the player's craft throws up driving
    /// through the rain's puddles, for Vulkan's picture.
    pub craft_spray: crate::craft_spray::CraftSpray,
}

/// Status of the consumed player/message/manager retirement, not a native field.
/// Retrying a stopped manager would replay callbacks and destructor prefixes.
#[derive(Debug)]
pub enum WorldOwnerRetirement {
    NotStarted,
    Completed,
    Stopped(crate::object_runtime::WorldCleanupError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPresentationState {
    /// 0048DDE7/EC copy H6/37 into0053DED0, then005474E0.
    pub tint_word: u32,
    /// H6/37 itself, the world's first light, which the tint's schedule
    /// moves from (Artoria's falls when its generators die): the modern sky
    /// follows the light's fall from it.
    pub base_tint_word: u32,
    /// H12 light normalized into00548380/84/88 by 0044FDD0.
    pub terrain_light_direction: [f32; 3],
    /// H14/0 header DWORD+10, returned by 0048EDA0.
    pub backdrop_height: i32,
}

#[derive(Debug)]
pub struct IntroRuntimeState {
    pub controller: ObjectId,
    /// Set exactly once when the production callback first attaches its owner.
    pub deadline_approximation_applied: bool,
    /// A failed required effect retains reached native fields and stops only
    /// controller dispatch. It must not retry a half-completed stage silently.
    pub controller_failure: Option<String>,
}

#[derive(Debug, Default)]
pub struct WorldActorUpdateState {
    /// World diagnostics for allocation-owned disposal callback/destructor stops.
    pub disposal: crate::object_runtime::ObjectDisposalState,
    pub unowned_callbacks_logged: bool,
    /// Reached selector67 ribbon +24 004866A0 failures, kept like the others.
    pub selector67_stops: Vec<(ObjectId, String)>,
    /// Cannon shots whose update (00486250) stopped after reached writes.
    pub shot32_stops: Vec<(ObjectId, String)>,
    /// Grenades whose update (00455E10) stopped after reached writes.
    pub grenade26_stops: Vec<(ObjectId, String)>,
    /// Flamethrowers whose update (00431FE0) stopped after reached writes.
    pub selector76_stops: Vec<(ObjectId, String)>,
    /// Settings +84 bits a gate's pass granted this tick (00463AB0()+84),
    /// which the session applies to the live settings.
    pub cheat_flags_84: u32,
    /// Log each newly reached actor callback, including later list admissions.
    pub skipped_callbacks: Vec<(ObjectId, bool)>,
    ///Timed nonactor pickups retain a reached countdown/disposal failure.
    pub pickup_stops: Vec<(ObjectId, String)>,
    /// Selector-50 hint triggers whose update stopped; each logs once.
    pub selector50_stops: Vec<(ObjectId, String)>,
    /// Selector-48 altitude ceilings whose update stopped; each logs once.
    pub selector48_stops: Vec<(ObjectId, String)>,
    /// Selector-43 timed spawners whose update stopped; each logs once.
    pub selector43_stops: Vec<(ObjectId, String)>,
    /// Selector-44 falling rocks whose update stopped; each logs once.
    pub selector44_stops: Vec<(ObjectId, String)>,
    /// Selector-64 course gates whose update or pass stopped.
    pub selector64_stops: Vec<(ObjectId, String)>,
    /// Factories whose update (0042EE30) stopped after reached writes.
    pub selector15_stops: Vec<(ObjectId, String)>,
    /// FactoryDockPointApproximation is logged once per world.
    pub factory_dock_approximation_logged: bool,
    /// The shop's command, menu-destruction or cue drains that stopped.
    pub shop_stop: Option<String>,
    /// Reached +24 boundaries retain preceding mutations. Only a new runtime
    /// attachment resets this custody; no stopped prefix is silently retried.
    pub family_prefix_stops: Vec<FamilyPrefixStop>,
    /// Complete selector93 errors retain reached prefixes and stop this actor
    /// until world replacement, without stopping other callbacks or phases.
    pub selector93_stops: Vec<(ObjectId, String)>,
    /// Reached selector77+24 failures retain status/timer/pose/RNG prefixes.
    /// These actors remain stopped until world replacement, without blocking
    /// other callbacks or replaying a partially completed update.
    pub selector77_stops: Vec<(ObjectId, String)>,
    /// Reached Selector11 004019E0 failures, kept the same way.
    pub selector11_stops: Vec<(ObjectId, String)>,
    /// Reached Selector30 0048B3F0 failures, kept the same way.
    pub selector30_stops: Vec<(ObjectId, String)>,
    /// A stopped first-player block (0046A0E0/0046A260). Its reached writes
    /// stay; the vehicle returns to the update loop, whose +24 (00416090)
    /// stops a live vehicle at its controlled update.
    pub player_update_stop: Option<String>,
    /// A stopped vehicle update (00416170); the block keeps running.
    pub player_vehicle_stop: Option<String>,
    /// Vehicles whose +24 (00416090) on the update loop stopped after
    /// reached writes; each stays stopped until world replacement.
    pub vehicle_dying_stops: Vec<(ObjectId, String)>,
    /// Save pads whose update (00475420) stopped after reached writes.
    pub selector58_stops: Vec<(ObjectId, String)>,
    pub selector83_stops: Vec<(ObjectId, String)>,
    /// Pickup spawners whose update (004548E0) stopped after reached writes.
    pub selector53_stops: Vec<(ObjectId, String)>,
    /// Mining rigs whose update (00454630) stopped after reached writes.
    pub selector52_stops: Vec<(ObjectId, String)>,
    /// Powerballs whose update (004863F0) stopped after reached writes.
    pub selector57_stops: Vec<(ObjectId, String)>,
    /// ECM Units whose update (00450450) stopped after reached writes.
    pub selector73_stops: Vec<(ObjectId, String)>,
    /// Remote Beamers whose update (00473D70) stopped after reached writes.
    pub selector55_stops: Vec<(ObjectId, String)>,
    /// Prison Units whose update (0046E230) stopped after reached writes.
    pub selector17_stops: Vec<(ObjectId, String)>,
    /// Zones whose update (0042D570) stopped after reached writes.
    pub selector74_stops: Vec<(ObjectId, String)>,
    /// Pursuit Ships whose update (00412A10) stopped after reached writes.
    pub selector61_stops: Vec<(ObjectId, String)>,
    pub selector62_stops: Vec<(ObjectId, String)>,
    /// Ending directors whose update (004072A0) stopped after reached writes.
    pub selector78_stops: Vec<(ObjectId, String)>,
    pub selector81_stops: Vec<(ObjectId, String)>,
    /// Missile launchers whose update (00445930) stopped after reached writes.
    pub selector94_stops: Vec<(ObjectId, String)>,
    /// Boulders whose update (004057D0) stopped after reached writes.
    pub selector59_stops: Vec<(ObjectId, String)>,
    /// Force Fields whose update (00476AE0) stopped after reached writes.
    pub selector18_stops: Vec<(ObjectId, String)>,
    /// Sirens whose update (0047FAC0) stopped after reached writes.
    pub selector47_stops: Vec<(ObjectId, String)>,
    /// Course targets whose update (0043BB80) stopped after reached writes.
    pub selector39_stops: Vec<(ObjectId, String)>,
    /// Barriers whose update (00411380) stopped after reached writes.
    pub selector69_stops: Vec<(ObjectId, String)>,
    /// Stabilisers whose update (004765A0) stopped after reached writes.
    pub selector49_stops: Vec<(ObjectId, String)>,
    /// Bridges whose update (00409A20) stopped after reached writes.
    pub selector37_stops: Vec<(ObjectId, String)>,
    /// Territories whose update (00475800) stopped after reached writes.
    pub selector46_stops: Vec<(ObjectId, String)>,
    /// The race's and its wrong-way markers' updates that stopped.
    pub race_stops: Vec<(ObjectId, String)>,
    /// Banners and bases whose update (0045B5A0, 0045BDB0) stopped after
    /// reached writes.
    pub selector40_stops: Vec<(ObjectId, String)>,
    /// Omitted key calls already logged in this world.
    pub key_omissions_logged: std::collections::BTreeSet<crate::object_runtime::PlayerKeyOmission>,
    /// A stopped key callback; later player keys are skipped.
    pub player_keys_stop: Option<String>,
    /// 004791F0 terrain-phase stops: the actor skips terrain contact and its
    /// water classification until world replacement.
    pub terrain_contact_stops: Vec<(ObjectId, String)>,
    /// 004791F0 furniture-phase stops; the actor's terrain phase keeps running.
    pub furniture_contact_stops: Vec<(ObjectId, String)>,
    /// 004790C0 pairs whose 0047B610 failed; other pairs keep running.
    pub pair_contact_stops: Vec<(ObjectId, ObjectId, String)>,
    pub list_failure: Option<String>,
}

impl WorldActorUpdateState {
    /// Logs each omitted key call once per world; the key's callback, or the
    /// player update's message-key repeat, did everything else.
    pub fn log_key_omissions(
        &mut self,
        omissions: std::collections::BTreeSet<crate::object_runtime::PlayerKeyOmission>,
    ) {
        for omission in omissions {
            if self.key_omissions_logged.insert(omission) {
                eprintln!("Player key call omits {omission:?} (not ported)");
            }
        }
    }
}

#[derive(Debug)]
pub struct FamilyPrefixStop {
    pub actor: ObjectId,
    pub class: FamilyUpdateClass,
    pub reason: FamilyPrefixStopReason,
}

#[derive(Debug)]
pub enum FamilyPrefixStopReason {
    /// The actual +D8 completed, but its enclosing class movement/weapon
    /// tail remains required. This is not a completed native +24 callback.
    AfterBehaviorDispatch,
    Selector35TailFailed(crate::session_platform::world_actor_update::WorldSelector35TailError),
    Selector34TailFailed(crate::session_platform::world_actor_update::WorldSelector34TailError),
    Selector10TailFailed(
        crate::object_runtime::Selector10TailPrefixError<
            crate::world_actor_collision::WorldActorCollisionError,
        >,
    ),
    Selector10MotionFailed(
        crate::object_runtime::Selector10MotionError<
            crate::world_actor_terrain::WorldActorTerrainError,
            crate::world_actor_contact::WorldActorContactHeaderError,
        >,
    ),
    Selector10PersistentAudioFailed(crate::object_runtime::Selector10PersistentAudioError),
    /// Selector 21's swings after selector 10's motion (00405615).
    Selector21SwingFailed(crate::object_runtime::DerivedActorError),
    /// The family's own +118 00424CF0 (selector 9).
    Family4236d0MotionFailed(
        crate::object_runtime::Family4236d0MotionError<
            crate::world_actor_terrain::WorldActorTerrainError,
            crate::world_actor_contact::WorldActorContactHeaderError,
        >,
    ),
    /// Exact481B10 stopped after its reached camera/record prefix.
    Selector36PersistentAudioFailed(
        crate::object_runtime::Selector36PersistentAudioError<
            crate::object_runtime::PlayerAttachError,
        >,
    ),
    Selector36MotionFailed(
        crate::object_runtime::Selector36MotionError<
            crate::world_actor_terrain::WorldActorTerrainError,
            crate::world_actor_contact::WorldActorContactHeaderError,
        >,
    ),
    Selector36TailFailed(
        crate::object_runtime::Selector36TailPrefixError<
            crate::world_actor_collision::WorldActorCollisionError,
        >,
    ),
    /// Selector12 reached its real post-D8 target/steering/motion tail. An
    /// error preserves that partial visit and is never retried as a fresh tick.
    Selector12TailFailed(
        crate::object_runtime::Selector12TailError<
            crate::world_actor_collision::WorldActorCollisionError,
        >,
    ),
    BehaviorFailed(
        crate::object_runtime::FamilyBehaviorError<
            crate::session_platform::WorldFamilyBehaviorError,
        >,
    ),
    /// Source acquisition failed after the class prefix and before its +D8.
    BehaviorSourceUnavailable(inf_formats::FrontendExecutableError),
    Failed(FamilyUpdateError<NativeFamilyUpdateError>),
}

/// Whether the loaded scene is driven by a cinematic controller. Selector 88
/// attaches during after-load, but the loader's later respawn clears +D8
/// again; attachment alone is insufficient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArcadeDrivePolicy {
    PlayerVehicle,
    /// Replaceable until selector-88/90/camera simulation owns this scene.
    /// Selected from an actual retained controller, never a mission number.
    FrozenIntroControlApproximation {
        controller: ObjectId,
    },
}

/// Actual owners borrowed by one offline load. The same owner, process map
/// and RNG survive Err; success returns scene metadata without moving them.
pub struct OfflineWorldLoadInputs<'a> {
    /// Retained5372F0/F4, published before the H12 lookup and placement calls.
    pub terrain_policy: &'a mut crate::terrain_runtime::ProcessTerrainPolicy,
    pub sound: crate::object_runtime::SoundTeardownService<'a>,
    pub owner: &'a mut WorldLoadOwner,
    pub assets: &'a crate::frontend_assets::FrontendAssets,
    /// Same retained process005483C0 output through all loads and failures.
    pub furniture_radius: &'a mut crate::terrain_runtime::ProcessFurnitureRadius,
    /// Camera factory current-null478E3D and reached retained-manager network
    /// and event10 source reads use this same actual immutable process owner.
    pub process: Option<&'a crate::process_particles::factories::ParticleFactoryProcessState>,
    pub map: crate::world_map::MapLoadContext<'a>,
    /// Actual retained process RNG loan; reached advances survive load Err.
    pub rng: &'a mut u32,
    pub reads: crate::loading_integration::WorldLoadReads<'a>,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    pub policy: OfflineWorldLoadPolicy<'a>,
    /// The process particle pool and session +64C for the respawn's carry
    /// effect; a respawn that reaches it without the pool stops there.
    pub particles: Option<&'a mut crate::process_particles::ProcessParticleRuntime>,
    pub phase: Option<u32>,
    /// The detail setting and the process furniture state a furniture
    /// marker's load-time removal (00437110) takes. None stops at a marker
    /// whose completion bit is set.
    pub furniture: Option<OfflineFurnitureLoan<'a>>,
    /// Settings +84, the cheat word the load's respawn reads.
    pub settings_84: u32,
    /// Port setting, not retail: Ultra ground detail's shape, which the
    /// terrain takes before anything is placed on it
    /// ([`crate::terrain_runtime::ultra`]).
    pub ultra: Option<crate::terrain_runtime::ultra::UltraShape>,
}

/// Settings +70, which 00450370 rereads after the marker's fine search, and
/// the process furniture state holding the fallback record 005483C4.
pub struct OfflineFurnitureLoan<'a> {
    pub detail: u32,
    pub query: &'a mut crate::world_furniture_effect::ProcessFurnitureQueryState,
}

/// Saved import keeps the old clean-arena policy. RetainedManager consumes the
/// actual native prelude before activation and real post-map manager cleanup.
/// CRT data is an optional actual immutable projection, never literal defaults.
#[derive(Clone, Copy, Debug)]
pub enum OfflineWorldLoadPolicy<'a> {
    CleanImport,
    RetainedManager {
        text_data: Option<crate::grant_text::GrantTextData<'a>>,
    },
}

/// Success-only scene data. The caller still owns the actual borrowed manager
/// through all failures and moves it into WorldSession only after this returns.
#[derive(Debug)]
pub struct WorldLoadMetadata {
    pub load_approximation: IntroCampaignReloadApproximation,
    pub briefing: inf_formats::BriefingProgram,
    pub briefing_milestone_count: u32,
    pub bonus_time_00: u32,
    pub respawn_header: WorldRespawnHeader,
    pub music_native_argument: u32,
    pub rng: u32,
    pub report: WorldLoadReport,
    pub respawn: Option<PlayerRespawn>,
    pub map: crate::world_map::WorldMapId,
    pub intro_runtime: Option<IntroRuntimeState>,
    pub drive_policy: ArcadeDrivePolicy,
}

/// The H14 header words `0046A890` reads when the camera respawns the
/// player's vehicle mid-world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldRespawnHeader {
    pub vehicle_selection_18: u32,
    pub actor_mask_limits: [i8; 2],
}

/// Saved-data source for explicit imports. Actual prepared/reloaded players
/// instead remain in WorldLoadOwner and are never serialized into this request.
#[derive(Clone, Debug)]
pub struct OfflineWorldLoadRequest {
    pub mission: MissionSaveState,
    pub player: PlayerSaveState,
    /// Runtime +A0 before 0048DC30 copies the selected mission into it.
    pub previous_world: u32,
    /// Retained manager W+1C and G timing history on offline reload. None
    /// selects source-proven 0048D9F0/ 00487BE0 constructor state.
    pub visibility: Option<WorldVisibilityReload>,
}

/// Explicit approximation for load inputs whose later writers are unowned.
/// First-load evidence establishes these values only for the initial load.
/// Saved imports still create a new arena/listener owner; retained loads borrow
/// the actual owners and execute manager cleanup without replacing players.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntroCampaignReloadApproximation {
    pub actor_clock: u32,
    pub camera_settings: CameraSettings,
}

impl IntroCampaignReloadApproximation {
    const FIRST_LOAD_INPUTS: Self = Self {
        actor_clock: 245,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
    };
}

/// 00487250's signed H14 WORD +14, converted to the native music request.
pub fn world_music_native_argument(header: &inf_formats::WorldResource) -> u32 {
    i32::from(header.header_words_00_to_34[5] as u16 as i16).wrapping_add(1) as u32
}

#[derive(Debug, Error)]
pub enum WorldBackendError {
    #[error(transparent)]
    ActorAllocationPolicy(#[from] crate::object_runtime::ActorAllocationPolicyError),
    #[error(transparent)]
    RenderResource(#[from] crate::render_resources::RenderResourceError),
    #[error(transparent)]
    Briefing(#[from] inf_formats::BriefingError),
    #[error(transparent)]
    Residency(#[from] crate::Error),
    #[error(transparent)]
    Placement(#[from] WorldPlacementError),
    #[error(transparent)]
    FurnitureRadius(
        #[from]
        crate::terrain_runtime::TerrainFurnitureRadiusError<
            crate::world_furniture_effect::resident::ResidentFurnitureResourceError,
        >,
    ),
    #[error(transparent)]
    Mission(#[from] WorldMissionError),
    #[error(transparent)]
    Loader(#[from] WorldLoaderError),
    #[error(transparent)]
    Tint(#[from] crate::world_tint::WorldTintError),
    #[error("world backend requiresHandler-14 slot 0 after activation")]
    MissingWorld,
    #[error("offline world reload requires its retained player")]
    MissingPlayer,
    #[error("world backend load already attempted: {0:?}")]
    LoadAlreadyAttempted(WorldLoadStatus),
    #[error("world backend retains a stopped retirement: {0}")]
    RetirementStopped(String),
}

impl WorldSession {
    /// Export complete actual save data for the explicit saved-import policy.
    /// This export does not select the arena's first player; the shared
    /// working-profile serializer's fresh lookup remains a separate phase.
    pub fn offline_reload_request(&self) -> Result<OfflineWorldLoadRequest, WorldBackendError> {
        let player = self.player.ok_or(WorldBackendError::MissingPlayer)?;
        let saved = self
            .arena
            .export_player_save_state(player)
            .map_err(WorldLoaderError::Player)?;
        Ok(OfflineWorldLoadRequest {
            mission: self.mission.save.clone(),
            player: saved,
            previous_world: self.mission.current_world_a0,
            visibility: Some(WorldVisibilityReload {
                manager_far: self.mission.dword_1c,
                performance: self.fade.performance.clone(),
            }),
        })
    }
}

/// Genuine boundary for the admitted empty creation-list domain. A reached
/// listener is rejected by its actual interface; this supplies no callback stub.
pub struct EmptyListenerEffects;
impl CreationListenerEffects for EmptyListenerEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        // The recording proves the creation list empty for all 30 first-world
        // actor constructions. A Force Field's binding runs 00477350, the
        // ending director's 00407350.
        match listener.interface {
            crate::object_runtime::CreationListenerInterface::Selector18 => {
                Ok(arena.selector18_listener(listener.owner, actor)?)
            }
            // 00407350: the ending director watches for its bug.
            crate::object_runtime::CreationListenerInterface::Selector78 => {
                Ok(arena.selector78_listener(listener.owner, actor)?)
            }
        }
    }
}

/// Load the selected offline world with caller-owned campaign state and RNG.
///
/// - Activates residency `48`, then `selection+10` (32 for offline 22).
/// - Builds `WorldPlacements` over the activated residency.
/// - Runs `load_world` with zero-fill WORDs and real terrain surfaces.
/// - Unknown constructors fail closed per-placement (skipped + logged).
/// - Unowned reload inputs retain `IntroCampaignReloadApproximation`.
pub fn load_offline_world(
    residency: &mut ResidentRuntime,
    mut context: OfflineWorldLoadInputs<'_>,
) -> Result<WorldLoadMetadata, WorldBackendError> {
    if !matches!(context.owner.load_status, WorldLoadStatus::NotStarted) {
        return Err(WorldBackendError::LoadAlreadyAttempted(
            context.owner.load_status.clone(),
        ));
    }
    if let WorldOwnerRetirement::Stopped(error) = &context.owner.owner_retirement {
        return Err(WorldBackendError::RetirementStopped(error.to_string()));
    }
    context.owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::Validation);
    let result = load_offline_world_inner(residency, &mut context);
    match &result {
        Ok(_) => context.owner.load_status = WorldLoadStatus::Complete,
        Err(error) => {
            let phase = match &context.owner.load_status {
                WorldLoadStatus::Loading(phase) => *phase,
                _ => unreachable!("backend retains its actual reached phase"),
            };
            context.owner.load_status = WorldLoadStatus::Stopped {
                phase,
                reason: error.to_string(),
            };
        }
    }
    result
}

fn load_offline_world_inner(
    residency: &mut ResidentRuntime,
    context: &mut OfflineWorldLoadInputs<'_>,
) -> Result<WorldLoadMetadata, WorldBackendError> {
    let assets = context.assets;
    let map = &mut context.map;
    let rng = &mut *context.rng;
    let owner = &mut *context.owner;
    let approximation = IntroCampaignReloadApproximation::FIRST_LOAD_INPUTS;
    match context.policy {
        OfflineWorldLoadPolicy::CleanImport => {
            for world in [
                owner.mission.current_world_a0,
                owner.mission.save.mission_selection,
            ] {
                if world >= crate::world_mission::ARENA_WORLDS.end {
                    return Err(WorldMissionError::WorldOutOfRange(world).into());
                }
            }
        }
        OfflineWorldLoadPolicy::RetainedManager { .. } => {
            owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::MissionPrelude);
            owner.mission.begin_world_load()?;
            owner.world_start.override_position = None;
            // Native prelude stores precede the bounded activation-domain read.
            let selected = owner.mission.save.mission_selection;
            if selected >= crate::world_mission::ARENA_WORLDS.end {
                return Err(WorldMissionError::WorldOutOfRange(selected).into());
            }
        }
    }
    owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::Activation);
    let ovl = (owner.mission.save.mission_selection + 10) as u8;
    crate::loading_integration::activate_world_resources(residency, ovl, rng, &mut context.reads)?;
    owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::AssetPreparation);

    let mut placements = WorldPlacements::new(residency, &mut *context.terrain_policy)?;
    // Port setting: the placements stand on Ultra's ground, as play does.
    placements.terrain_mut().set_ultra(context.ultra);
    //0044FDD0: publish radius before retained presentation-light/map work.
    //WorldPlacements already computes pure slope normalization and terrain
    //diagonals. Native slope publication follows4508E0 within44FDD0; native
    //diagonal44B590 follows44FDD0. Radius reads neither prepared component.
    //H15 pointer publication remains separate.
    crate::terrain_runtime::produce_resident_furniture_radius(
        crate::terrain_runtime::TerrainFurnitureRadiusRequest::new(
            placements.terrain_mut(),
            context.furniture_radius.word_mut(),
        ),
        residency,
    )?;
    let briefing = inf_formats::parse_briefing_program(
        placements.world_source().overlay.bytes(),
        &placements.assets().worlds,
        placements.world(),
    )?;
    let music_native_argument = world_music_native_argument(placements.world());
    let briefing_milestone_count = placements.world().header_words_00_to_34[9];
    let respawn_header = WorldRespawnHeader {
        vehicle_selection_18: placements.world().header_words_00_to_34[6],
        actor_mask_limits: placements.world().actor_mask_limits(),
    };
    let presentation = load_world_presentation(residency, &placements)?;
    let tint_schedule = crate::world_tint::WorldTintSchedule::from_world(
        &placements.assets().worlds,
        placements.world_source().overlay.bytes(),
        placements.world().allocation_offset,
        &placements.world().header_words_00_to_34,
    )?;
    let particle_impact_selectors = residency.resource(15, 0).and_then(|resource| {
        match resource
            .overlay
            .handlers()
            .get(resource.registration.region_index)
        {
            Some(crate::HandlerResources::MissionParameters(bank)) => bank
                .entries
                .get(usize::from(resource.registration.local_index))
                .map(|entry| entry.fields),
            _ => None,
        }
    });
    owner.prepared = Some(WorldLoadPrepared {
        terrain: placements.terrain().clone(),
        terrain_source: crate::world_actor_terrain::WorldTerrainSource::retain(
            placements.terrain_source(),
        ),
        world_flags: placements.world().flags,
        particle_impact_selectors,
        presentation,
        tint_schedule,
    });
    let math = NativeBasisMath::new(assets.sine_table().values);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes())
        .map_err(|_| WorldBackendError::MissingWorld)?;
    let grants: GrantTable =
        inf_formats::parse_grant_table(executable).map_err(|_| WorldBackendError::MissingWorld)?;
    let inverse_sine = inf_formats::parse_frontend_inverse_sine_table(
        executable.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .map_err(|_| WorldBackendError::MissingWorld)?
    .values;
    let texts = inf_formats::parse_grant_texts(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes())
            .map_err(|_| WorldBackendError::MissingWorld)?,
    )
    .map_err(|_| WorldBackendError::MissingWorld)?;
    let limits = inf_formats::parse_vehicle_upgrade_limits(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes())
            .map_err(|_| WorldBackendError::MissingWorld)?,
    )
    .map_err(|_| WorldBackendError::MissingWorld)?;
    let vehicle_table: &[u32; VEHICLE_BEHAVIOR_TABLE_DWORDS] =
        &inf_formats::parse_vehicle_behavior_table(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes())
                .map_err(|_| WorldBackendError::MissingWorld)?,
        )
        .map_err(|_| WorldBackendError::MissingWorld)?;

    // Fresh-heap WORDs with real terrain surfaces. Placements, camera and
    // vehicle all read zero-fill instead of recorded heap leftovers. The
    // census proves zero sits inside the reproducing band for the two
    // outcome dimensions (classification `Set40000000`, wave bit clear).
    let sine = assets.sine_table().values;
    let surface = ActorTerrainSurface {
        terrain: placements.terrain(),
        plane_height_fixed: placements.plane_height_fixed(),
    };
    let world_flags = placements.world().flags;
    let fresh = fresh_allocation_words();
    let constructor_null = match context.process {
        Some(process) => crate::object_runtime::ActorConstructorNull::Process(process),
        // Offline import callers can explicitly lack the live process owner.
        None => crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
    };
    // 0055F2F4: 1 offline, the session's slot in a network game.
    let slot = context.process.map_or(1, |process| process.local_slot);
    let placement_allocation = |_: u32| ActorConstructionRequest {
        constructor_null,
        allocation_position: Some(fresh),
        owner: slot,
        terrain: ActorTerrainInputs {
            terrain: Some(surface),
            coordinate_shift: 8,
            clock: approximation.actor_clock,
            world_flags: Some(world_flags),
            sine_quarter: &sine,
        },
    };
    let camera_allocation = ActorConstructionRequest {
        constructor_null,
        allocation_position: Some(fresh),
        owner: slot,
        terrain: ActorTerrainInputs {
            terrain: Some(surface),
            coordinate_shift: 8,
            clock: approximation.actor_clock,
            world_flags: Some(world_flags),
            sine_quarter: &sine,
        },
    };
    let vehicle_allocation = ActorConstructionRequest {
        constructor_null,
        allocation_position: Some(fresh),
        owner: slot,
        terrain: ActorTerrainInputs {
            terrain: Some(surface),
            coordinate_shift: 8,
            clock: approximation.actor_clock,
            world_flags: Some(world_flags),
            sine_quarter: &sine,
        },
    };

    let manager_policy = match context.policy {
        OfflineWorldLoadPolicy::CleanImport => {
            crate::world_loader::WorldLoadManagerPolicy::CleanImport
        }
        OfflineWorldLoadPolicy::RetainedManager { text_data: _ } => {
            crate::world_loader::WorldLoadManagerPolicy::RetainedAfterMissionPrelude {
                grants: crate::object_runtime::LiveGrantContext {
                    process: context.process,
                    executable: Some(executable),
                    resident: Some(&*residency),
                    text_data: match &context.policy {
                        OfflineWorldLoadPolicy::RetainedManager { text_data } => text_data.as_ref(),
                        OfflineWorldLoadPolicy::CleanImport => unreachable!(),
                    },
                },
            }
        }
    };
    owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::WorldObjects);
    let text_data = match &context.policy {
        OfflineWorldLoadPolicy::RetainedManager { text_data } => text_data.as_ref(),
        OfflineWorldLoadPolicy::CleanImport => None,
    };
    let inputs = WorldLoadInputs {
        manager_policy,
        equipment: Some(crate::world_loader::WorldLoadEquipmentInputs {
            executable,
            text_data,
            actor_clock: approximation.actor_clock,
            sine: &sine,
            phase: context.phase,
        }),
        process: context.process,
        runtime: residency,
        placements: &placements,
        math: &math,
        grant_table: &grants,
        pickups: PickupTables {
            grants: &grants,
            texts: &texts,
            upgrade_limits: &limits,
            network_active: context
                .process
                .is_some_and(|process| process.network_flag != 0),
        },
        vehicle_table,
        camera_settings: approximation.camera_settings,
        settings_84: context.settings_84,
        inverse_sine: &inverse_sine,
        // 0055F2F4: 1 offline, the session's slot in a network game.
        local_owner: context.process.map_or(1, |process| process.local_slot),
        placement_allocation: &placement_allocation,
        camera_allocation,
        vehicle_allocation,
    };

    let report = load_world(
        &mut owner.arena,
        &inputs,
        WorldLoadState {
            sound: context.sound.reborrow(),
            // This enclosing backend admits only the offline, non-attract
            // domain. The general lower driver borrows and freshly reads the
            // native BYTE at each constructed placement; no world ID selects it.
            current_attract: &std::cell::Cell::new(0),
            mission: &mut owner.mission,
            world_start: &mut owner.world_start,
            rng: &mut *rng,
            map: crate::world_map::MapLoadContext {
                process: &mut *map.process,
                state: &mut *map.state,
            },
            listeners: &mut owner.listeners,
            listener_effects: &mut *context.listener_effects,
            particles: context.particles.as_deref_mut(),
            timing: Some(&owner.performance),
            furniture: match (context.furniture.as_mut(), owner.prepared.as_mut()) {
                (Some(loan), Some(prepared)) => {
                    Some(crate::world_furniture_effect::MarkerFurniture {
                        terrain: &mut prepared.terrain,
                        chain: crate::world_furniture_effect::FurnitureChainLoan {
                            policy: &mut *context.terrain_policy,
                            detail: loan.detail,
                            world_flags: Some(world_flags),
                            network_active: false,
                        },
                        resources: crate::world_furniture_effect::ResidentFurnitureChainResources {
                            resident: &*residency,
                            executable,
                        },
                        query: &mut *loan.query,
                    })
                }
                _ => None,
            },
        },
    )?;
    let map_id = map
        .state
        .current()
        .expect("completed world load published its actual map");
    let respawn = report.respawn.clone();
    owner.load_status = WorldLoadStatus::Loading(WorldLoadPhase::FinalMetadata);
    owner.player = owner.arena.first_player();
    let mut drive_policy = ArcadeDrivePolicy::PlayerVehicle;
    for &object in owner.arena.actor_list() {
        let actor = owner
            .arena
            .actor(object)
            .map_err(WorldLoaderError::Object)?;
        if matches!(
            &actor.extension,
            crate::object_runtime::ActorExtension::Selector88(controller)
                if controller.retained_13c.is_some()
        ) {
            drive_policy =
                ArcadeDrivePolicy::FrozenIntroControlApproximation { controller: object };
            break;
        }
    }
    let intro_runtime =
        if let ArcadeDrivePolicy::FrozenIntroControlApproximation { controller } = drive_policy {
            Some(IntroRuntimeState {
                controller,
                deadline_approximation_applied: false,
                controller_failure: None,
            })
        } else {
            None
        };
    Ok(WorldLoadMetadata {
        load_approximation: approximation,
        briefing,
        briefing_milestone_count,
        bonus_time_00: placements.world().header_words_00_to_34[0],
        respawn_header,
        music_native_argument,
        rng: *rng,
        report,
        respawn,
        map: map_id,
        intro_runtime,
        drive_policy,
    })
}

/// The presentation part of 0048DC30, before terrain preparation/placements.
/// Raw H6 RGB is independent from the display's converted H7 material words.
pub(crate) fn load_world_presentation(
    residency: &ResidentRuntime,
    placements: &WorldPlacements<'_>,
) -> Result<WorldPresentationState, WorldBackendError> {
    let source = placements.terrain_source();
    let terrain = match source
        .overlay
        .handlers()
        .get(source.registration.region_index)
    {
        Some(crate::HandlerResources::Terrain(bank)) => bank
            .entries
            .get(usize::from(source.registration.local_index)),
        _ => None,
    }
    .ok_or(WorldPlacementError::Registration { handler: 12 })?;
    let request = crate::render_resources::RenderResourceRequest {
        kind: crate::render_resources::RenderResourceKind::Style,
        argument: 0x37,
    };
    let crate::render_resources::RenderResourceData::Style(tint_word) =
        crate::render_resources::RenderResources::new(residency)
            .resolve(request)?
            .data
    else {
        unreachable!("typed H6 tint")
    };
    Ok(WorldPresentationState {
        tint_word,
        base_tint_word: tint_word,
        terrain_light_direction: crate::terrain_runtime::normalize_terrain_light(
            terrain.prefix.light_direction_raw,
        ),
        backdrop_height: placements.world().header_words_00_to_34[4] as i32,
    })
}

/// Headless helper for tests: open retail data-root, boot residency, load
/// the first world with fresh-heap WORDs. Returns placement/object counts
/// and the vehicle position for smoke checks.
#[cfg(test)]
pub fn load_first_world_for_test(
    data_root: &std::path::Path,
) -> Result<(usize, usize, Option<[u16; 3]>), WorldBackendError> {
    let (world, _maps) = load_first_world_session_for_test(data_root)?;
    let vehicle_pos = world.respawn.as_ref().and_then(|respawn| {
        respawn.vehicle.and_then(|vehicle| {
            world
                .arena
                .object(vehicle)
                .ok()
                .and_then(|object| object.base.position_words)
        })
    });
    Ok((
        world.report.placements.len(),
        world.arena.len(),
        vehicle_pos,
    ))
}

/// Corpus fixture retaining the actual process map store beside the world.
#[cfg(test)]
fn load_first_world_session_for_test(
    data_root: &std::path::Path,
) -> Result<(WorldSession, crate::world_map::ProcessWorldMapRuntime), WorldBackendError> {
    let installation = Installation::open(data_root)?;
    let mut residency = ResidentRuntime::new(installation)?;
    residency.apply(ResidencyRequest::Select {
        selection: crate::residency::ResourceSelection {
            difficulty: 2,
            language: 0,
        },
    })?;
    residency.apply(ResidencyRequest::Boot)?;
    // Frontend leaf 47 mirrors production boot; world load releases it via 48.
    residency.apply(ResidencyRequest::Activate { level: 47 })?;
    let assets = crate::frontend_assets::FrontendAssets::open(data_root)
        .map_err(|_| WorldBackendError::MissingWorld)?;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes())
        .map_err(|_| WorldBackendError::MissingWorld)?;
    let grants =
        inf_formats::parse_grant_table(executable).map_err(|_| WorldBackendError::MissingWorld)?;
    let offline = crate::offline_prepare::offline_game(&grants)
        .map_err(|_| WorldBackendError::MissingWorld)?;
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner =
        crate::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        })?;
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = load_offline_world(
        &mut residency,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )?;
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    Ok((world, maps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unprepared_and_saved_import_keep_their_distinct_actual_player_owners() {
        let mut mission = MissionSaveState::new_game();
        mission.owner_a4 = 7;
        let unprepared = WorldLoadOwner::unprepared().unwrap();
        assert_eq!(unprepared.origin, WorldLoadOwnerOrigin::PreparedPlayers);
        assert_eq!(unprepared.arena.first_player(), None);
        assert_eq!(unprepared.player, None);
        assert!(unprepared.constructions.is_empty());
        assert_eq!(
            unprepared.prepare_phase,
            crate::player_prepare::PlayerPreparePhase::NotStarted
        );
        assert_eq!(unprepared.mission.save, MissionSaveState::new_game());
        assert_eq!(unprepared.world_start.pair_index, 0);
        assert_eq!(unprepared.mission.current_world_a0, 1);
        assert_eq!(unprepared.mission.inhibit_byte_08, 0);
        assert_eq!(unprepared.mission.byte_a6, Some(0));
        assert_eq!(unprepared.mission.dword_38, None);
        assert_eq!(unprepared.mission.word_10, None);
        assert_eq!(unprepared.mission.byte_12, Some(0));
        assert_eq!(unprepared.mission.byte_13, Some(1));
        assert_eq!(unprepared.mission.bytes_14_16, Some([0; 3]));
        assert_eq!(unprepared.mission.dword_1c, 0x2800);
        assert_eq!(unprepared.mission.dword_20, Some(0x400));
        assert_eq!(unprepared.mission.dword_24, Some(0));
        assert_eq!(unprepared.mission.word_28, Some(0));
        assert_eq!(unprepared.mission.dword_2c, Some(0));
        assert!(unprepared
            .mission
            .record_0c
            .as_deref()
            .unwrap()
            .iter()
            .all(|&byte| byte == 0));
        assert!(unprepared.prepared.is_none());

        let saved = PlayerSaveState {
            owner_100: [0x1234; 6],
            owner_118: 0x8000_0118,
            owner_120: [1, 2, 3, 0x8000_012c],
            owner_13c: [0x4180_ff00, 0x81fe_0042],
            owner_a0: [0x7654; 6],
            ..PlayerSaveState::default()
        };
        let mut performance = crate::process_particles::timing::WorldTimingRing::default();
        performance.tick(30_000, 30_000).unwrap();
        let imported = WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: mission.clone(),
            player: saved.clone(),
            previous_world: 18,
            visibility: Some(WorldVisibilityReload {
                manager_far: 0x2345,
                performance: performance.clone(),
            }),
        })
        .unwrap();
        assert_eq!(imported.origin, WorldLoadOwnerOrigin::SavedImport);
        let player = imported.player.unwrap();
        assert_eq!(imported.arena.first_player(), Some(player));
        assert_eq!(
            imported.arena.export_player_save_state(player).unwrap(),
            saved
        );
        assert!(imported.constructions.is_empty());
        assert_eq!(imported.mission.save, mission);
        assert_eq!(imported.mission.current_world_a0, 18);
        assert_eq!(imported.mission.dword_1c, 0x2345);
        assert_eq!(imported.performance, performance);
    }

    #[test]
    fn stopped_retained_prelude_keeps_prefix_and_rejects_a_second_attempt() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        let mut save = MissionSaveState::new_game();
        save.mission_selection = 34;
        save.owner_3c[1] = 2;
        let mut owner = WorldLoadOwner::unprepared().unwrap();
        owner.mission.save = save;
        owner.world_start.override_position = Some([11, 22, 33]);
        let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
        let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
        let map_before = format!("{maps:?}/{map_load:?}");
        let mut rng = 0x1234_5678;
        let mut effects = EmptyListenerEffects;
        //Explicit startup-proven fixture process owner, shared by retries below.
        let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let error = load_offline_world(
            &mut residency,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut owner,
                assets: &assets,
                process: None,
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load,
                },
                rng: &mut rng,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::RetainedManager { text_data: None },
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorldBackendError::Mission(WorldMissionError::WorldOutOfRange(34))
        ));
        assert_eq!(owner.mission.byte_a6, Some(0));
        assert_eq!(owner.mission.save.owner_3c[1], 6);
        assert_eq!(owner.mission.dword_38, Some(0));
        assert_eq!(owner.mission.current_world_a0, 34);
        assert_eq!(owner.world_start.override_position, None);
        assert!(matches!(
            &owner.load_status,
            WorldLoadStatus::Stopped {
                phase: WorldLoadPhase::MissionPrelude,
                ..
            }
        ));
        assert!(owner.prepared.is_none());
        assert_eq!(rng, 0x1234_5678);
        assert_eq!(format!("{maps:?}/{map_load:?}"), map_before);
        let stopped = format!("{owner:?}");
        let error = load_offline_world(
            &mut residency,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut owner,
                assets: &assets,
                process: None,
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load,
                },
                rng: &mut rng,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::RetainedManager { text_data: None },
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorldBackendError::LoadAlreadyAttempted(WorldLoadStatus::Stopped {
                phase: WorldLoadPhase::MissionPrelude,
                ..
            })
        ));
        assert_eq!(format!("{owner:?}"), stopped);
        assert_eq!(rng, 0x1234_5678);
        assert_eq!(format!("{maps:?}/{map_load:?}"), map_before);
    }

    #[test]
    fn retained_backend_completes_the_actual_playerless_branch_without_import() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        residency.apply(ResidencyRequest::Boot).unwrap();
        let mut save = MissionSaveState::new_game();
        save.mission_selection = 18;
        let mut owner = WorldLoadOwner::unprepared().unwrap();
        owner.mission.save = save;
        let arena_before = format!("{:?}", owner.arena.world_players());
        let process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
        let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
        let mut rng = 0x1234_5678;
        let mut effects = EmptyListenerEffects;
        //Explicit startup-proven fixture process owner, shared by retries below.
        let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let metadata = load_offline_world(
            &mut residency,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut owner,
                assets: &assets,
                process: Some(&process),
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load,
                },
                rng: &mut rng,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::RetainedManager { text_data: None },
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap();
        assert!(!metadata.report.placements.is_empty());
        assert!(metadata.report.respawn.is_none());
        assert!(metadata.respawn.is_none());
        assert_eq!(metadata.rng, rng);
        assert_ne!(rng, 0x1234_5678);
        assert_eq!(map_load.current(), Some(metadata.map));
        assert_eq!(owner.arena.first_player(), None);
        assert_eq!(owner.player, None);
        assert_eq!(format!("{:?}", owner.arena.world_players()), arena_before);
        assert!(owner.constructions.is_empty());
        assert_eq!(owner.mission.current_world_a0, 18);
        assert_eq!(owner.mission.inhibit_byte_08, 0);
        assert_eq!(owner.mission.byte_12, Some(1));
        assert!(matches!(&owner.load_status, WorldLoadStatus::Complete));
        let world = WorldSession::from_loaded(owner, metadata);
        assert_eq!(world.origin, WorldLoadOwnerOrigin::PreparedPlayers);
        assert_eq!(world.player, None);
        assert!(world.respawn.is_none());
        assert_eq!(world.arena.first_player(), None);
        assert!(world.constructions.is_empty());
        assert_eq!(maps.jitter(world.map).unwrap().jitter_radius(), 0);
    }

    #[test]
    fn retained_backend_keeps_post_map_network_failure_and_current_preparation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        residency.apply(ResidencyRequest::Boot).unwrap();
        let mut save = MissionSaveState::new_game();
        save.mission_selection = 18;
        save.owner_3c[1] = 2;
        let mut owner = WorldLoadOwner::unprepared().unwrap();
        owner.mission.save = save;
        owner.mission.record_0c = Some(Box::new([0x69; crate::world_mission::RECORD_0C_BYTES]));
        let actor = owner
            .arena
            .allocate_actor(ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 245,
                    world_flags: None,
                    sine_quarter: &assets.sine_table().values,
                },
            })
            .unwrap();
        let arena_before = format!("{:?}", owner.arena);
        let start_before = owner.world_start.clone();
        let listeners_before = format!("{:?}", owner.listeners);
        let performance = owner.performance.clone();
        let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
        let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
        let initial_rng = 0x0123_4567_u32;
        let mut rng = initial_rng;
        let mut effects = EmptyListenerEffects;
        //Explicit startup-proven fixture process owner, shared by retries below.
        let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let error = load_offline_world(
            &mut residency,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut owner,
                assets: &assets,
                process: None,
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load,
                },
                rng: &mut rng,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::RetainedManager { text_data: None },
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(error, WorldBackendError::Loader(WorldLoaderError::Cleanup(
            crate::object_runtime::WorldCleanupError::MissingProcess(actual)
        )) if actual == actor)
        );
        let map = map_load.current().unwrap();
        assert_eq!(map_load.pending(), None);
        assert_eq!(maps.jitter(map).unwrap().jitter_radius(), 0);
        assert_eq!(maps.grid().unwrap().indices().len(), 65_536);
        let mut expected_rng = initial_rng;
        for _ in 0..65_536 {
            expected_rng = expected_rng.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
        }
        assert_eq!(rng, expected_rng);
        assert_eq!(owner.mission.current_world_a0, 18);
        assert_eq!(owner.mission.save.owner_3c[1], 6);
        assert_eq!(owner.mission.inhibit_byte_08, 1);
        assert_eq!(owner.mission.byte_12, Some(0));
        assert_eq!(
            owner.mission.record_0c.as_deref(),
            Some(&[0x69; crate::world_mission::RECORD_0C_BYTES])
        );
        assert_eq!(format!("{:?}", owner.arena), arena_before);
        assert_eq!(owner.arena.actor_list(), &[actor]);
        assert_eq!(owner.arena.first_player(), None);
        assert!(owner.constructions.is_empty());
        assert_eq!(owner.world_start, start_before);
        assert_eq!(format!("{:?}", owner.listeners), listeners_before);
        assert_eq!(owner.performance, performance);
        assert!(matches!(
            &owner.load_status,
            WorldLoadStatus::Stopped {
                phase: WorldLoadPhase::WorldObjects,
                ..
            }
        ));
        let prepared = owner.prepared.as_ref().unwrap();
        assert!(prepared
            .terrain_source
            .is_current(&residency.resource(12, 0).unwrap()));
        let placements = WorldPlacements::new(&residency, TerrainSpacing::Units256).unwrap();
        assert_eq!(
            format!("{:?}", prepared.terrain),
            format!("{:?}", placements.terrain())
        );
        assert_eq!(prepared.world_flags, placements.world().flags);
        let stopped = format!("{owner:?}");
        let maps_before_retry = format!("{maps:?}/{map_load:?}");
        let process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let error = load_offline_world(
            &mut residency,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut owner,
                assets: &assets,
                process: Some(&process),
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load,
                },
                rng: &mut rng,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::RetainedManager { text_data: None },
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorldBackendError::LoadAlreadyAttempted(WorldLoadStatus::Stopped {
                phase: WorldLoadPhase::WorldObjects,
                ..
            })
        ));
        assert_eq!(format!("{owner:?}"), stopped);
        assert_eq!(format!("{maps:?}/{map_load:?}"), maps_before_retry);
        assert_eq!(rng, expected_rng);
    }

    #[test]
    fn loaded_owner_move_retains_actual_arena_mission_listeners_and_timing() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let (mut world, _maps) = load_first_world_session_for_test(&root).unwrap();
        world.fade.performance.tick(30_000, 30_000).unwrap();
        let player = world.player;
        let arena = format!("{:?}", world.arena);
        let mission = world.mission.clone();
        let start = world.world_start.clone();
        let listeners = format!("{:?}", world.listeners);
        let performance = world.fade.performance.clone();
        let terrain = format!("{:?}", world.terrain);
        let flags = world.world_flags;
        let presentation = world.presentation;
        let owner = world.into_load_owner();
        assert_eq!(
            owner.origin,
            crate::world_backend::WorldLoadOwnerOrigin::SavedImport
        );
        assert_eq!(owner.player, player);
        assert_eq!(format!("{:?}", owner.arena), arena);
        assert_eq!(owner.mission, mission);
        assert_eq!(owner.world_start, start);
        assert_eq!(format!("{:?}", owner.listeners), listeners);
        assert_eq!(owner.performance, performance);
        assert!(matches!(
            owner.load_status,
            crate::world_backend::WorldLoadStatus::NotStarted
        ));
        let prepared = owner.prepared.as_ref().unwrap();
        assert_eq!(format!("{:?}", prepared.terrain), terrain);
        assert_eq!(prepared.world_flags, flags);
        assert_eq!(prepared.presentation, presentation);
        assert_eq!(owner.arena.first_player(), player);
    }

    /// 0046ADF6 in the load's respawn: settings +84 bit 0x40 gives a new
    /// game's player the Cranial Implanter, beside the gate's 0x80 or not.
    /// The load keeps its messages undelivered, as the briefing that covers
    /// it suppresses the player's messages.
    #[test]
    fn a_load_under_the_completion_bonus_bit_gives_the_player_the_cranial_implanter() {
        for (cheats, owned) in [(0x80, false), (0xc0, true), (0x40, true)] {
            let (_resident, _assets, world) = crate::test_worlds::offline_world_with_cheat_word(
                11,
                cheats,
                crate::terrain_runtime::TerrainSpacing::Units256,
            );
            let player = world.player.unwrap();
            let state = world.arena.world_player(player).unwrap();
            let bit = 1 << (0x33 - 32);
            assert_eq!(state.bits_100[1] & bit != 0, owned, "{cheats:#x}");
            assert_eq!(state.grants_9c[1] & bit != 0, owned, "{cheats:#x}");
            let notice = crate::object_runtime::RespawnMessage::Notice(
                crate::object_runtime::GrantNotification {
                    id: 0x33,
                    text: 538,
                },
            );
            assert_eq!(
                world.respawn.as_ref().unwrap().messages.contains(&notice),
                owned
            );
        }
    }

    #[test]
    fn fresh_heap_first_world_loads_like_the_recording() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let (placements, objects, vehicle) =
            load_first_world_for_test(&root).expect("fresh-heap first world loads");
        // 118 placements plus the camera and the player vehicle, matching the
        // headless loader test with recorded WORDs.
        assert_eq!(placements, 118);
        assert_eq!(objects, 120);
        // Vehicle respawns from start state, not WORDs; fresh zero-fill still
        // yields a placed vehicle.
        assert!(vehicle.is_some(), "vehicle respawned with fresh-heap WORDs");
    }

    #[test]
    fn music_word_is_signed_and_independent_of_the_neighboring_header_word() {
        let mut header = inf_formats::WorldResource {
            local_index: 0,
            entry_pointer_span: inf_formats::ByteSpan::new(0, 0),
            allocation_offset: 0,
            header_span: inf_formats::ByteSpan::new(0, 0),
            header_words_00_to_34: [0; 14],
            flags: 0,
            placement_pointer_span: inf_formats::ByteSpan::new(0, 0),
            placements: vec![],
        };
        for (word, argument) in [
            (0xffff, 0),
            (0xfffe, u32::MAX),
            (0, 1),
            (7, 8),
            (0x8000, 0xffff_8001),
        ] {
            header.header_words_00_to_34[5] = 0xbeef_0000 | word;
            assert_eq!(world_music_native_argument(&header), argument);
        }
    }

    /// The Alien Brain's worlds (0 and 19, levels 10 and 29) play disc
    /// track 5, the track their beds hush: the argument the frontend reads
    /// plus the one 00487310 adds.
    #[test]
    fn the_brains_worlds_play_track_5() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        for world in [0_u8, 19] {
            let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
            runtime.apply(ResidencyRequest::Boot).unwrap();
            for level in [47, 48, world + 10] {
                runtime.apply(ResidencyRequest::Activate { level }).unwrap();
            }
            let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
            let argument = world_music_native_argument(placements.world());
            assert_eq!(argument.wrapping_add(1), 5, "world {world}");
        }
    }
}
