//! Actual retained loader owners, distinct from successful loaded metadata.
//! Moving these owners never exports and reimports a player or releases input.

use super::{
    OfflineWorldLoadRequest, WorldBackendError, WorldLoadMetadata, WorldOwnerRetirement,
    WorldSession,
};
use crate::object_runtime::{
    ActorAllocationPolicy, ActorHandle, ActorRegistryConfig, CreationListeners, ObjectArena,
    PlayerId, WorldPlayerConstruction,
};
use crate::player_prepare::PlayerPreparePhase;
use crate::process_particles::timing::WorldTimingRing;
use crate::world_loader::WorldLoaderError;
use crate::world_mission::{WorldMission, ARENA_WORLDS};
use crate::world_start::WorldStartState;

/// Import is an explicit fixture/data policy; it never owns constructor input
/// references. PreparedPlayers retains the actual successful or stopped carriers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldLoadOwnerOrigin {
    SavedImport,
    PreparedPlayers,
}

/// Port custody of one non-resumable backend attempt, not native mission fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldLoadPhase {
    Validation,
    MissionPrelude,
    Activation,
    AssetPreparation,
    WorldObjects,
    FinalMetadata,
}

#[derive(Clone, Debug)]
pub enum WorldLoadStatus {
    NotStarted,
    Loading(WorldLoadPhase),
    Complete,
    Stopped {
        phase: WorldLoadPhase,
        reason: String,
    },
}

/// Same arena/mission/listeners/carriers retained before preparation, through
/// load errors and across reload. The process map store and RNG remain separate
/// actual process loans; dropping this value is not native player destruction.
#[derive(Debug)]
pub struct WorldLoadOwner {
    pub origin: WorldLoadOwnerOrigin,
    pub arena: ObjectArena,
    pub mission: WorldMission,
    pub world_start: WorldStartState,
    pub listeners: CreationListeners,
    pub player: Option<PlayerId>,
    pub constructions: Vec<WorldPlayerConstruction>,
    pub prepare_phase: PlayerPreparePhase,
    pub performance: WorldTimingRing,
    pub flash: crate::world_flash::WorldFlashState,
    /// Reached immutable preparation, including a later stopped placement pass.
    /// An old source retained across activation still requires fresh identity
    /// authentication; this never supplies replacement/default terrain.
    pub prepared: Option<WorldLoadPrepared>,
    pub owner_retirement: WorldOwnerRetirement,
    pub load_status: WorldLoadStatus,
}

#[derive(Debug)]
pub struct WorldLoadPrepared {
    pub terrain: crate::terrain_runtime::TerrainRuntime,
    pub(crate) terrain_source: crate::world_actor_terrain::WorldTerrainSource,
    pub world_flags: u32,
    pub particle_impact_selectors: Option<[u32; 5]>,
    pub presentation: super::WorldPresentationState,
    pub tint_schedule: crate::world_tint::WorldTintSchedule,
}

impl WorldLoadOwner {
    /// Genuine represented mission constructor with successful statistics
    /// allocation, before any player allocation. Real Prepare fills this same
    /// arena. Registry capacity remains the existing explicit port boot domain:
    /// 17 owners of 256 slots, as a module's initialise (0045C080) configures
    /// them through 004622F0 by the first world; it also clears five pool
    /// pointers that only network code fills.
    pub fn unprepared() -> Result<Self, WorldBackendError> {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 17,
            initial_capacity: 256,
            null_handle: ActorHandle::from_raw(0),
        })
        .map_err(|_| WorldBackendError::MissingWorld)?;
        // Explicit once-per-retained-arena extended backing approximation.
        // Direct native/supplied fixtures remain strict unless they select a mode.
        arena.select_actor_allocation_policy(
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
        )?;
        Ok(Self {
            origin: WorldLoadOwnerOrigin::PreparedPlayers,
            arena,
            mission: WorldMission::native_constructor(),
            world_start: WorldStartState {
                pair_index: 0, //48EBA0 clears the actual constructor's A4.
                override_position: None,
                vehicle_selection: 0,
                harness_spawn: None,
                port_spawn: None,
            },
            listeners: CreationListeners::new(),
            player: None,
            constructions: Vec::new(),
            prepare_phase: PlayerPreparePhase::NotStarted,
            performance: WorldTimingRing::default(),
            flash: crate::world_flash::WorldFlashState::default(),
            prepared: None,
            owner_retirement: WorldOwnerRetirement::NotStarted,
            load_status: WorldLoadStatus::NotStarted,
        })
    }

    /// Explicit saved-data import, independent from native469D60 preparation.
    /// Keep the old selected-player projection and visibility import policies.
    pub fn import_saved(request: OfflineWorldLoadRequest) -> Result<Self, WorldBackendError> {
        for world in [request.previous_world, request.mission.mission_selection] {
            if world >= ARENA_WORLDS.end {
                return Err(crate::world_mission::WorldMissionError::WorldOutOfRange(world).into());
            }
        }
        let mut owner = Self::unprepared()?;
        owner.origin = WorldLoadOwnerOrigin::SavedImport;
        owner.world_start.pair_index = request.mission.owner_a4;
        owner.mission.save = request.mission;
        owner.mission.current_world_a0 = request.previous_world;
        if let Some(visibility) = request.visibility {
            owner.mission.dword_1c = visibility.manager_far;
            owner.performance = visibility.performance;
        }
        let player = owner.arena.add_world_player(request.player.owner_a0);
        owner
            .arena
            .restore_player_save_prefix(player, &request.player)
            .map_err(WorldLoaderError::Player)?;
        owner.player = Some(player);
        Ok(owner)
    }
}

impl WorldSession {
    /// Move the completed real owner into loaded metadata. No player import,
    /// constructor reference duplication or new manager allocation occurs.
    pub fn from_loaded(owner: WorldLoadOwner, metadata: WorldLoadMetadata) -> Self {
        let prepared = owner
            .prepared
            .expect("completed load prepared its actual terrain");
        let fade = crate::world_fade::WorldFadeState::loaded(
            owner.mission.dword_1c,
            owner
                .mission
                .dword_20
                .expect("completed load applied the header"),
            owner.performance,
        );
        Self {
            load_approximation: metadata.load_approximation,
            briefing: metadata.briefing,
            briefing_milestone_count: metadata.briefing_milestone_count,
            bonus_time_00: metadata.bonus_time_00,
            respawn_header: metadata.respawn_header,
            music_native_argument: metadata.music_native_argument,
            arena: owner.arena,
            mission: owner.mission,
            fade,
            flash: owner.flash,
            world_start: owner.world_start,
            rng: metadata.rng,
            listeners: owner.listeners,
            origin: owner.origin,
            constructions: owner.constructions,
            prepare_phase: owner.prepare_phase,
            load_status: owner.load_status,
            report: metadata.report,
            respawn: metadata.respawn,
            player: owner.player,
            terrain: prepared.terrain,
            terrain_source: prepared.terrain_source,
            map: metadata.map,
            world_flags: prepared.world_flags,
            particle_impact_selectors: prepared.particle_impact_selectors,
            presentation: prepared.presentation,
            scheduled_tint: prepared.presentation.tint_word,
            tint_schedule: prepared.tint_schedule,
            intro_runtime: metadata.intro_runtime,
            actor_updates: super::WorldActorUpdateState::default(),
            owner_retirement: WorldOwnerRetirement::NotStarted,
            native_camera: None,
            weather: crate::world_weather::WorldWeather::constant(),
            camera_failure: None,
            drive_policy: metadata.drive_policy,
            view_focals: [0, 0],
            shown_focals: [0, 0],
            draw_skip_logged: false,
            draw_omissions_logged: Default::default(),
            light_delta: 0,
            last_draw: None,
            tick_skips_logged: false,
            crystal_flares: Default::default(),
            cloud_wake: Default::default(),
            craft_spray: Default::default(),
        }
    }

    /// Discard only loaded-scene metadata before a new explicit attempt. The
    /// actual manager/player/input/listener and timing owners move intact.
    pub fn into_load_owner(self) -> WorldLoadOwner {
        WorldLoadOwner {
            origin: self.origin,
            arena: self.arena,
            mission: self.mission,
            world_start: self.world_start,
            listeners: self.listeners,
            player: self.player,
            constructions: self.constructions,
            prepare_phase: self.prepare_phase,
            performance: self.fade.performance,
            flash: self.flash,
            prepared: Some(WorldLoadPrepared {
                terrain: self.terrain,
                terrain_source: self.terrain_source,
                world_flags: self.world_flags,
                particle_impact_selectors: self.particle_impact_selectors,
                presentation: self.presentation,
                tint_schedule: self.tint_schedule,
            }),
            owner_retirement: self.owner_retirement,
            load_status: WorldLoadStatus::NotStarted,
        }
    }
}
