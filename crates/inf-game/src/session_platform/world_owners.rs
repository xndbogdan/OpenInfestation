//! Consumed initial Prepare and deleting LeaveWorld over the same actual owners.
//!
//! The retained loader owner enters PortSession before a constructor can stop.
//! Descriptor/process/CRT loans remain immutable through each lower operation;
//! callback identities are monotonic port allocation identities, not PlayerIds.
//! Leave retains the actual world through map/effects/message/manager retirement,
//! then moves its arena and carriers before the deleting player loop, and in
//! a network game closes the session. Native allocation NULL, SEH, present
//! audio and game shutdown remain required domains. A stopped pass never silently restarts or drops references.

use super::*;
use crate::grant_text::{GrantTextData, GrantTextPairClassification};
use crate::object_runtime::{PlayerConstructionPhase, WorldCleanupRequest};
use crate::player_input_source::PlayerInputSource;
use crate::player_prepare::{
    prepare_player, PlayerPrepareContext, PlayerPrepareRequest, PlayerPrepareSource,
};
use crate::world_backend::{WorldLoadOwner, WorldOwnerRetirement};

/// The process owners a paused cleanup walk's destructor exit borrows
/// beside the loaded world's own, as the world tick lends them.
struct CleanupExitOwners<'a, 'exe> {
    tasks: &'a mut crate::process_tasks::ProcessTaskRuntime,
    particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    sounds: &'a mut crate::process_sound::ProcessSoundRuntime,
    furniture: &'a mut crate::world_furniture_effect::FurnitureEffectQueue,
    residency: &'a ResidentRuntime,
    factory_process: &'a mut crate::process_particles::factories::ParticleFactoryProcessState,
    behavior_tables: &'a mut crate::object_runtime::ProcessActorBehaviorTables,
    animation_clock: &'a dyn crate::frontend_camera::AnimationClockSource,
    camera_settings: crate::object_runtime::CameraSettings,
    sine: &'a [u16; 4096],
    grant_text: &'a GrantTextData<'a>,
    executable: FrontendExecutable<'exe>,
}

/// 0043D0F0's Dormant exit for the ship the cleanup walk paused at: its
/// burst and radius over the world's live owners and RNG word.
fn run_cleanup_dormant_exit(
    world: &mut crate::world_backend::WorldSession,
    actor: crate::object_runtime::ObjectId,
    rng_on_world: bool,
    owners: CleanupExitOwners<'_, '_>,
    state: &SessionState,
) -> Result<(), String> {
    if !rng_on_world {
        return Err("the RNG word is on the platform loan, not the world's".into());
    }
    let mut damage = super::particles::damage::WorldDamageOwners {
        grant_text: Some(owners.grant_text),
        camera_settings: Some(owners.camera_settings),
        flash: Some(&mut world.flash),
        mission: &mut world.mission,
        tasks: owners.tasks,
        terrain: &mut world.terrain,
        particles: owners.particles,
        sounds: owners.sounds,
        furniture: owners.furniture,
        residency: owners.residency,
        executable: owners.executable,
        factory_process: owners.factory_process,
        behavior_tables: owners.behavior_tables,
        animation_clock: owners.animation_clock,
        session: state,
        particle_load_factor: world.fade.performance.particle_load_factor,
        behavior_factory: Some(super::particles::damage::BehaviorFactoryLoan {
            listeners: &mut world.listeners,
            sine: owners.sine,
            world_flags: world.world_flags,
        }),
        map: None,
    };
    super::world_actor_behavior::run_selector35_dormant_exit(
        &mut world.arena,
        actor,
        &mut damage,
        &mut world.rng,
    )
}

/// Reserved opaque port identity namespace, separate from screen callback IDs.
/// This is not an authored descriptor, native address or behavioral mode.
pub(super) const PLAYER_CALLBACK_CONTEXT_START: u32 = 0x8000_0000;

/// Only the constructor-proven absent branch is owned. No present pointer or
/// native audio writer is manufactured for the unavailable branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameAudioCustody {
    ConstructorAbsent,
    /// Only the tests set the unavailable branch.
    #[cfg(test)]
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayerLeavePhase {
    WorldRetirement,
    Audio,
    OwnerAndParticles,
    Players,
    Network,
}

/// Port-only non-resumable pass receipt. The actual lower retired records stay
/// in their carriers; this receipt does not replace source field ownership.
#[derive(Debug)]
pub(super) enum PlayerLeaveState {
    NotStarted,
    Reached(PlayerLeavePhase),
    Completed,
    Stopped {
        phase: PlayerLeavePhase,
        reason: String,
    },
}

/// Same current545C74 semantic data shape consumed by grant text. Unavailable
/// binding/individual bytes remain unavailable;545900 is a separate unowned
/// NULL-substitute binding. Building this value validates neither name FORMAT
/// nor any reached H0 text and invents no classification defaults.
pub(super) fn current_grant_text(classification: &ProfileNameClassification) -> GrantTextData<'_> {
    GrantTextData {
        null_substitute: None,
        pair_classification: classification.high_bytes.as_ref().map(|bytes| {
            GrantTextPairClassification {
                colon_high: bytes[b':' as usize],
                space_high: bytes[b' ' as usize],
            }
        }),
    }
}

fn allocate_player_callback_context(next: &mut u32) -> Result<u32, &'static str> {
    let context = *next;
    *next = context
        .checked_add(1)
        .ok_or("player callback identity exhausted")?;
    Ok(context)
}

/// Actual fresh head and unique retained carrier are selected at each reached
/// iteration. Completed records remain retained. Imported players and partial
/// unpublished constructors are not converted into a fake deleting carrier.
fn retire_constructed_players(
    owner: &mut WorldLoadOwner,
    input: &mut InputRuntime,
) -> Result<(), PlayerLoopError> {
    while let Some(player) = owner.arena.first_player() {
        let mut matching = owner
            .constructions
            .iter()
            .enumerate()
            .filter(|(_, construction)| construction.player() == player);
        let index = matching
            .next()
            .map(|(index, _)| index)
            .ok_or(PlayerLoopError::Boundary(
                "current player has no retained constructor carrier",
            ))?;
        if matching.next().is_some() {
            return Err(PlayerLoopError::Boundary(
                "current player has ambiguous retained constructor carriers",
            ));
        }
        owner
            .arena
            .destroy_constructed_world_player(&mut owner.constructions[index], input)?;
    }
    // The native fresh-head loop has completed before this portable admission
    // checks unpublished constructor custody. It must not suppress deletion of
    // an earlier actual FIFO player because a later construction stopped.
    if owner
        .constructions
        .iter()
        .any(|construction| construction.phase() != PlayerConstructionPhase::Published)
    {
        return Err(PlayerLoopError::Boundary(
            "unpublished player preparation requires explicit component/unwind custody",
        ));
    }
    owner.player = None;
    Ok(())
}

#[derive(Debug)]
enum PlayerLoopError {
    Boundary(&'static str),
    Destruction(crate::object_runtime::PlayerDestructionError),
}

impl From<crate::object_runtime::PlayerDestructionError> for PlayerLoopError {
    fn from(error: crate::object_runtime::PlayerDestructionError) -> Self {
        Self::Destruction(error)
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// Real 00487410 preparation. Prior complete calls can append a new actual
    /// player; a failed attempt must retain its prefix rather than repeat it.
    pub(super) fn prepare_offline_player(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        self.prepare_session_player(state, PlayerPrepareSource::NewGame)
    }

    /// Real 00487380, Frontend to phase 3 after the profile menu loaded a
    /// slot: the same player construction, 0048EBA0(G+1E0, 0), then 00449B80
    /// restores the working profile into W and the fresh first player before
    /// its 004633D0 name write. LoadWorld then enters the restored selection.
    pub(super) fn prepare_frontend_load(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        self.prepare_session_player(state, PlayerPrepareSource::FrontendLoad)
    }

    fn prepare_session_player(
        &mut self,
        state: &mut SessionState,
        source: PlayerPrepareSource,
    ) -> Result<(), PortSessionErrorOf<P>> {
        if self.player_prepare_stopped.is_some() {
            return Err(PortSessionError::WorldOwnerBoundary(
                "initial player preparation already stopped",
            ));
        }
        if matches!(
            self.player_leave_state,
            PlayerLeaveState::Stopped { .. } | PlayerLeaveState::Reached(_)
        ) {
            return Err(PortSessionError::WorldOwnerBoundary(
                "player LeaveWorld pass is incomplete",
            ));
        }
        if self.world.is_some() {
            return Err(PortSessionError::WorldOwnerBoundary(
                "initial player preparation requires its retained pre-world owner",
            ));
        }
        // Retain the actual empty manager before any lower fallible phase.
        //0048D9F0 supplies the represented mission fields and22/Clear save
        // before initial Prepare; no debug override precedes those phases.
        if self.pending_world.is_none() {
            self.pending_world = Some(WorldLoadOwner::unprepared()?);
        }
        let callback_context = allocate_player_callback_context(&mut self.player_callback_next)
            .map_err(PortSessionError::WorldOwnerBoundary)?;
        // 00487485: the host's arena world (00488A20 over settings +1C), or a
        // guest's host world word 0054E814.
        let network_world = (self.factory_process.network_flag != 0)
            .then(|| PortSessionServices::network_platform(&mut self.platform))
            .flatten()
            .map(|network| {
                if self.factory_process.host_flag != 0 {
                    crate::network_menu::arena_at(network.settings.words()[0x1c / 4]).world
                } else {
                    network.link.game.host_world as u32
                }
            });
        self.player_leave_state = PlayerLeaveState::NotStarted;

        let result: Result<(), PortSessionErrorOf<P>> = (|| {
            let assets = self.catalog.assets();
            let executable = FrontendExecutable::parse(assets.executable_bytes())
                .expect("FrontendAssets retains its validated executable");
            let mut descriptors = PlayerInputSource {
                analog: &self.player_analog,
                executable,
            };
            let text = current_grant_text(&self.profile_name_classification);
            let grants = crate::object_runtime::LiveGrantContext {
                process: Some(&self.factory_process),
                executable: Some(executable),
                resident: Some(&self.residency),
                text_data: Some(&text),
            };
            let owner = self
                .pending_world
                .as_mut()
                .expect("retained preparation owner");
            prepare_player(
                PlayerPrepareRequest {
                    arena: &mut owner.arena,
                    constructions: &mut owner.constructions,
                    mission: &mut owner.mission.save,
                    owner: &mut self.owner_byte,
                    cycle: &mut self.player_cycle,
                    phase: &mut owner.prepare_phase,
                    callback_context,
                    attract: self.attract,
                    source,
                    network_world,
                },
                PlayerPrepareContext {
                    input: &mut self.input,
                    descriptors: &mut descriptors,
                    grants: &grants,
                    selections: assets.attract_selections(),
                },
            )
            .map_err(PortSessionError::Prepare)?;
            let harness = self
                .harness_start
                .as_ref()
                .filter(|_| self.harness_stage == super::HarnessStage::Loading);
            if let (PlayerPrepareSource::FrontendLoad, Some(start)) = (source, harness) {
                // The start harness: the mission and player prefix 00449B80
                // would restore come from its state. With `profile`, the
                // slot is loaded and restored first, name write included,
                // as the profile menu's load does.
                if let Some(slot) = start.profile {
                    let store = PortSessionServices::profile_store(&mut self.platform).ok_or(
                        PortSessionError::WorldOwnerBoundary(
                            "the start harness's profile requires the retained profile store",
                        ),
                    )?;
                    store
                        .load(slot)
                        .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                    let name = store
                        .restore_current_working(
                            crate::profile_store::WorkingProfileRestoreRequest {
                                mission: &mut owner.mission.save,
                                state,
                                arena: &mut owner.arena,
                                classification: &self.profile_name_classification,
                            },
                        )
                        .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                    PortSessionServices::persist_identity(
                        &mut self.platform,
                        crate::name_entry::IdentityWriteRequest {
                            target: crate::name_entry::IdentityTarget::PlayerName,
                            name: &name,
                            initials: crate::name_entry::IdentityInitialsWrite::Keep,
                        },
                    )
                    .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                }
                let table = inf_formats::parse_grant_table(executable)
                    .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                owner.mission.save = start.mission();
                if let Some(far) = start.world_far {
                    // The retained manager far the load clamps the header to.
                    owner.mission.dword_1c = far;
                }
                state.restore_mission_elapsed_seconds(owner.mission.save.elapsed_seconds);
                if let Some(player) = owner.arena.first_player() {
                    let mut saved = owner
                        .arena
                        .export_player_save_state(player)
                        .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                    start
                        .apply_player(&mut saved, &table)
                        .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                    owner
                        .arena
                        .restore_player_save_prefix(player, &saved)
                        .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                }
                owner.world_start.harness_spawn = start.spawn();
                eprintln!(
                    "Start harness: world {} start {} spawn {:?}",
                    owner.mission.save.mission_selection,
                    owner.mission.save.owner_a4,
                    owner.world_start.harness_spawn
                );
            } else if source == PlayerPrepareSource::FrontendLoad {
                // 00449B80 over the slot the menu's 0044A170 load selected. Its
                // mission read projects W+34 into the session's elapsed time.
                let store = PortSessionServices::profile_store(&mut self.platform).ok_or(
                    PortSessionError::WorldOwnerBoundary(
                        "frontend load requires the retained profile store",
                    ),
                )?;
                let name = store
                    .restore_current_working(crate::profile_store::WorkingProfileRestoreRequest {
                        mission: &mut owner.mission.save,
                        state,
                        arena: &mut owner.arena,
                        classification: &self.profile_name_classification,
                    })
                    .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
                PortSessionServices::persist_identity(
                    &mut self.platform,
                    crate::name_entry::IdentityWriteRequest {
                        target: crate::name_entry::IdentityTarget::PlayerName,
                        name: &name,
                        initials: crate::name_entry::IdentityInitialsWrite::Keep,
                    },
                )
                .map_err(|error| PortSessionError::FrontendLoad(error.to_string()))?;
            } else {
                //0048EBA0 cleared actual W+34; its session alias changes seconds,
                // while scheduler remainder remains in the same SessionState.
                state.restore_mission_elapsed_seconds(owner.mission.save.elapsed_seconds);
                if let Some(selection) = self.mission_override {
                    // Explicit debug policy after the real reset, not an authored
                    // constructor value or attract/network selector substitute.
                    owner.mission.save.mission_selection = selection;
                }
            }
            // First FIFO export is an enclosing save projection. The newly
            // constructed receipt must not silently select a later player.
            let player = owner
                .arena
                .first_player()
                .expect("complete player construction published an actual FIFO player");
            owner.player = Some(player);
            let saved = owner
                .arena
                .export_player_save_state(player)
                .map_err(crate::world_loader::WorldLoaderError::Player)
                .map_err(WorldBackendError::from)?;
            self.offline_game = Some(OfflineGame {
                mission: owner.mission.save.clone(),
                player: saved,
            });
            Ok(())
        })();
        if let Err(error) = &result {
            self.player_prepare_stopped = Some(error.to_string());
        }
        result
    }

    /// 0048E890's manager walk over the loaded world (`retire_world_owners`),
    /// which Results and LeaveWorld both run. The walk pauses at a ship whose
    /// destructor exit needs world services (0043E120): the exit runs here
    /// over the world's live owners, and the walk resumes at that ship's
    /// destructor, without the retained-player prefix.
    pub(super) fn retire_loaded_world_owners(
        &mut self,
        state: &SessionState,
    ) -> Result<(), crate::object_runtime::WorldCleanupError> {
        // 0048E8C4..0048E8FA, then the destructors of the menus its message
        // cleanup deleted, while the world's actors still live: a shop menu
        // left open undocks from its factory, as in retail. Any a stopped
        // shop leaves behind die with this world.
        let world = self.world.as_mut().expect("a loaded world to retire");
        world.arena.retire_world_player()?;
        self.drain_shown_menu_commands(state);
        let world = self.world.as_mut().expect("a loaded world to retire");
        discard_world_menus(&mut world.arena);
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .expect("FrontendAssets retains its validated executable");
        let text = current_grant_text(&self.profile_name_classification);
        // Settings +70, which a furniture marker's 00450370 rereads.
        let detail = self.platform.world_tick_inputs().settings_70;
        let camera_settings = self.platform.world_tick_inputs().camera_settings;
        let network_active = self.factory_process.network_flag != 0;
        let rng_on_world = self.world_rng_custody == WorldRngCustody::LoadedWorld;
        let mut voices = self.platform.sound_voice_teardown();
        let world = self.world.as_mut().expect("a loaded world to retire");
        loop {
            let grants = crate::object_runtime::LiveGrantContext {
                process: Some(&self.factory_process),
                executable: Some(executable),
                resident: Some(&self.residency),
                text_data: Some(&text),
            };
            let request = WorldCleanupRequest {
                sound: crate::object_runtime::SoundTeardownService::Available {
                    sounds: &mut self.sounds,
                    voices: &mut voices,
                },
                mission: &mut world.mission,
                grants: &grants,
                listeners: &mut world.listeners,
                furniture: Some(crate::world_furniture_effect::MarkerFurniture {
                    terrain: &mut world.terrain,
                    chain: crate::world_furniture_effect::FurnitureChainLoan {
                        policy: &mut self.terrain_policy,
                        detail,
                        world_flags: Some(world.world_flags),
                        network_active,
                    },
                    resources: crate::world_furniture_effect::ResidentFurnitureChainResources {
                        resident: &self.residency,
                        executable,
                    },
                    query: &mut self.furniture_query,
                }),
            };
            match world.arena.cleanup_world(request) {
                Ok(()) => return Ok(()),
                Err(crate::object_runtime::WorldCleanupError::DestructionExitPending(actor)) => {
                    run_cleanup_dormant_exit(
                        world,
                        actor,
                        rng_on_world,
                        CleanupExitOwners {
                            tasks: &mut self.tasks,
                            particles: &mut self.particles,
                            sounds: &mut self.sounds,
                            furniture: &mut self.furniture,
                            residency: &self.residency,
                            factory_process: &mut self.factory_process,
                            behavior_tables: &mut self.actor_behavior_tables,
                            animation_clock: &self.process,
                            camera_settings,
                            sine: &self.catalog.assets().sine_table().values,
                            grant_text: &text,
                            executable,
                        },
                        state,
                    )
                    .map_err(|reason| {
                        crate::object_runtime::WorldCleanupError::DestructionExit { actor, reason }
                    })?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Complete the admitted deleting LeaveWorld sequence. This is distinct
    /// from Results retirement, which preserves players/input/carriers.
    pub(super) fn clear_constructed_world(
        &mut self,
        state: &SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        match &self.player_leave_state {
            PlayerLeaveState::Stopped { phase, reason } => {
                eprintln!("LeaveWorld replay refused at {phase:?}: {reason}");
                return Err(PortSessionError::WorldOwnerBoundary(
                    "LeaveWorld already stopped",
                ));
            }
            PlayerLeaveState::Reached(_) => {
                return Err(PortSessionError::WorldOwnerBoundary(
                    "LeaveWorld already stopped",
                ));
            }
            _ => {}
        }
        let result = self.clear_constructed_world_phases(state);
        match &result {
            Ok(()) => self.player_leave_state = PlayerLeaveState::Completed,
            Err(error) => {
                let phase = match self.player_leave_state {
                    PlayerLeaveState::Reached(phase) => phase,
                    _ => unreachable!("LeaveWorld publishes its phase before fallible work"),
                };
                self.player_leave_state = PlayerLeaveState::Stopped {
                    phase,
                    reason: error.to_string(),
                };
            }
        }
        result
    }

    fn clear_constructed_world_phases(
        &mut self,
        state: &SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        self.player_leave_state = PlayerLeaveState::Reached(PlayerLeavePhase::WorldRetirement);
        self.retire_world_map()?; //48E89D..BB, before W+12/effects.
        if let Some(world) = self.world.as_mut() {
            world.mission.byte_12 = Some(0);
        } else if let Some(owner) = self.pending_world.as_mut() {
            owner.mission.byte_12 = Some(0);
        }
        self.clear_world_effects();
        if let Some(world) = self.world.as_mut() {
            if matches!(world.owner_retirement, WorldOwnerRetirement::Stopped(_)) {
                return Err(PortSessionError::WorldOwnerBoundary(
                    "loaded-world retirement already stopped",
                ));
            }
            // A completed Results pass does not replace this distinct native
            // Leave call: it must freshly perform message and manager phases.
            world.owner_retirement = WorldOwnerRetirement::NotStarted;
            let retirement = self.retire_loaded_world_owners(state);
            let world = self.world.as_mut().expect("retained loaded world");
            if let Err(error) = retirement {
                eprintln!("LeaveWorld retirement feature stopped: {error}");
                world.owner_retirement = WorldOwnerRetirement::Stopped(error);
                return Err(PortSessionError::WorldOwnerBoundary(
                    "loaded-world retirement stopped after reached writes",
                ));
            }
            world.owner_retirement = WorldOwnerRetirement::Completed;
        } else if let Some(owner) = self.pending_world.as_mut() {
            if matches!(owner.owner_retirement, WorldOwnerRetirement::Stopped(_)) {
                return Err(PortSessionError::WorldOwnerBoundary(
                    "pending-world retirement already stopped",
                ));
            }
            owner.owner_retirement = WorldOwnerRetirement::NotStarted;
            let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                .expect("FrontendAssets retains its validated executable");
            let text = current_grant_text(&self.profile_name_classification);
            let grants = crate::object_runtime::LiveGrantContext {
                process: Some(&self.factory_process),
                executable: Some(executable),
                resident: Some(&self.residency),
                text_data: Some(&text),
            };
            // Settings +70, which a furniture marker's 00450370 rereads.
            let detail = self.platform.world_tick_inputs().settings_70;
            let network_active = self.factory_process.network_flag != 0;
            let mut voices = self.platform.sound_voice_teardown();
            owner
                .arena
                .retire_world_owners(WorldCleanupRequest {
                    sound: crate::object_runtime::SoundTeardownService::Available {
                        sounds: &mut self.sounds,
                        voices: &mut voices,
                    },
                    mission: &mut owner.mission,
                    grants: &grants,
                    listeners: &mut owner.listeners,
                    furniture: owner.prepared.as_mut().map(|prepared| {
                        crate::world_furniture_effect::MarkerFurniture {
                            terrain: &mut prepared.terrain,
                            chain: crate::world_furniture_effect::FurnitureChainLoan {
                                policy: &mut self.terrain_policy,
                                detail,
                                world_flags: Some(prepared.world_flags),
                                network_active,
                            },
                            resources:
                                crate::world_furniture_effect::ResidentFurnitureChainResources {
                                    resident: &self.residency,
                                    executable,
                                },
                            query: &mut self.furniture_query,
                        }
                    }),
                })
                .map_err(PortSessionError::WorldCleanup)?;
            discard_world_menus(&mut owner.arena);
            owner.owner_retirement = WorldOwnerRetirement::Completed;
        }
        // Keep self.world present through real effect/retirement services.
        // Only its metadata can be retired now; no actual player is recreated.
        if self.world.is_some() {
            if self.pending_world.is_some() {
                return Err(PortSessionError::WorldOwnerBoundary(
                    "loaded and pending manager owners overlap",
                ));
            }
            let world = self.world.take().expect("completed loaded retirement");
            if self.world_rng_custody == WorldRngCustody::LoadedWorld {
                *self.platform.process_rng() = world.rng;
            }
            self.world_rng_custody = WorldRngCustody::Platform;
            self.pending_world = Some(world.into_load_owner());
        }

        self.player_leave_state = PlayerLeaveState::Reached(PlayerLeavePhase::Audio);
        //487C71 initializes this actual field to NULL. Later present/unknown
        // audio writers and4F60E0/4923C0 are not invented successful defaults.
        if self.player_audio_5d0 != GameAudioCustody::ConstructorAbsent {
            return Err(PortSessionError::WorldOwnerBoundary(
                "current game audio owner unavailable",
            ));
        }
        self.player_leave_state = PlayerLeaveState::Reached(PlayerLeavePhase::OwnerAndParticles);
        self.owner_byte = false; //4871F4
        self.sounds.cleanup_one_shots()?; //actual4800E0 mask0x02 cleanup
        self.particles.clear()?; //4871FB ->464F60

        self.player_leave_state = PlayerLeaveState::Reached(PlayerLeavePhase::Players);
        if let Some(owner) = self.pending_world.as_mut() {
            retire_constructed_players(owner, &mut self.input).map_err(|error| match error {
                PlayerLoopError::Boundary(reason) => PortSessionError::WorldOwnerBoundary(reason),
                PlayerLoopError::Destruction(error) => PortSessionError::PlayerRetirement(error),
            })?;
        }
        self.player_leave_state = PlayerLeaveState::Reached(PlayerLeavePhase::Network);
        if self.factory_process.network_flag != 0 {
            // 0048721E: 00493650 closes the session; 0055F448 clears and the
            // slot goes back to 1.
            if let Some(network) = self.platform.network_platform() {
                network.link.close();
            }
            self.factory_process.network_flag = 0;
            self.factory_process.local_slot = 1;
            self.factory_process.host_flag = 0;
        }
        self.offline_game = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Destroyed menus whose destructors cannot run any more: the shop that
/// would have run them stopped, or the world is already drained. They
/// belong to the world being retired, so they go with it rather than reach
/// the next world's first drain against its own objects; logged.
fn discard_world_menus(arena: &mut crate::object_runtime::ObjectArena) {
    let leftover = arena.take_destroyed_menus();
    if !leftover.is_empty() {
        eprintln!(
            "World retirement: {} shown menu destructor(s) did not run before the world's actors went: {leftover:?}",
            leftover.len()
        );
    }
}
