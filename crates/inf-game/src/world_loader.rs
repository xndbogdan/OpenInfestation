//! Offline world loader 0048DC30 over the ported owners.
//!
//! Order: the mission prelude (+A6, +3C mark, +38, +A0), header-derived mission
//! fields, completed map publication and the retained manager phase, then
//! 0048DE64's placement resets, each placement's H13 factory call with
//! 0047F6C0 on constructed actors and the fresh per-placement attract suffix,
//! then 00404D40, the 00479840 drain and both
//! +20 passes (selector 88 binds the first player), the player branch with
//! 0046A890, and the mission tail. Resource activation, sound/frame setup,
//! complete attract/network admission and the later briefing remain separate.
//! CleanImport requires an empty world-object arena. RetainedAfterMissionPrelude
//! consumes the actual post-map manager cleanup 00478CD0 over retained owners;
//! its enclosing caller already applied the mission prelude before activation.
//!
//! W+A6 is both the mission prelude's cleared byte and the world-start
//! override flag, and W+14 is both a cleared header byte and the vehicle
//! selection, so this driver keeps the two owners' views of W consistent.

use std::cell::Cell;
use std::sync::Arc;

use inf_formats::{
    native_basis::NativeBasisMath, FrontendExecutable, GrantTable, VEHICLE_BEHAVIOR_TABLE_DWORDS,
};
use thiserror::Error;

use crate::grant_text::GrantTextData;
use crate::object_runtime::{
    construct_world_object, ActorChildRequest, ActorChildResource, ActorConstructionRequest,
    ActorFactoryInputs, BaseContext, CameraFactoryInputs, CameraSettings, CreationListenerEffects,
    CreationListeners, DerivedActorFactoryInputs, InstanceCommandAllocation,
    InstanceCommandAllocationError, LiveGrantContext, LoaderPlayerSetup, NetworkRespawn,
    ObjectAfterLoadInputs, ObjectArena, ObjectConstructionError, ObjectConstructionOutcome,
    ObjectConstructionRequest, ObjectError, ObjectId, PickupTables, PlayerAttachError,
    PlayerEquipmentHost, PlayerEquipmentWorld, PlayerRespawn, PlayerRespawnInputs,
    SoundTeardownService, VehicleCarryContext, VehicleCarryEffect, VehicleCarryHost,
    WorldCleanupError, WorldCleanupRequest, WorldLoadAttractRequest, WorldLoadError,
};
use crate::process_particles::births::{ParticleBirthInputs, ParticleBirthRequest};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_particles::ProcessParticleRuntime;
use crate::process_sound::ProcessSoundRuntime;
use crate::terrain_runtime::TerrainRuntime;
use crate::world_map::{
    construct_world_map, MapLoadContext, PreparedWorldMapTerrain, WorldMapConstructionError,
    WorldMapConstructionRequest, WorldMapTerrainInputs,
};
use crate::world_mission::{WorldMission, WorldMissionError};
use crate::world_placement::WorldPlacements;
use crate::world_start::{WorldStartContext, WorldStartState};
use crate::ResidentRuntime;

mod instances;
pub(crate) use instances::WorldInstanceAllocation;

/// Genuine load entry modes; the retained caller applies the mission prelude
/// before resource activation and lends these same current grant sources.
/// Missing sources fail only when a reached manager callback consumes them.
#[derive(Clone, Copy, Debug)]
pub enum WorldLoadManagerPolicy<'a> {
    /// Separate import policy: empty world-object arena and local mission prelude.
    CleanImport,
    /// Actual retained owners after the enclosing native prelude/activation.
    /// Manager cleanup follows completed map publication without source preflight.
    RetainedAfterMissionPrelude { grants: LiveGrantContext<'a> },
}

/// 0046A890's carried-item services beyond the camera factory: the child
/// factory's fresh allocation inputs, the grant text and the particle phase.
#[derive(Clone, Copy)]
pub struct WorldLoadEquipmentInputs<'a> {
    pub executable: FrontendExecutable<'a>,
    pub text_data: Option<&'a GrantTextData<'a>>,
    /// The terrain clock the placements' fresh allocations use.
    pub actor_clock: u32,
    pub sine: &'a [u16; 4096],
    /// Session +64C, read when the particle pool is full.
    pub phase: Option<u32>,
}

/// Immutable inputs of one offline load.
pub struct WorldLoadInputs<'a> {
    pub manager_policy: WorldLoadManagerPolicy<'a>,
    /// None stops a respawn that reaches the equipment loop.
    pub equipment: Option<WorldLoadEquipmentInputs<'a>>,
    pub process: Option<&'a crate::process_particles::factories::ParticleFactoryProcessState>,
    pub runtime: &'a ResidentRuntime,
    pub placements: &'a WorldPlacements<'a>,
    pub math: &'a NativeBasisMath,
    pub grant_table: &'a GrantTable,
    pub pickups: PickupTables<'a>,
    pub vehicle_table: &'a [u32; VEHICLE_BEHAVIOR_TABLE_DWORDS],
    pub camera_settings: CameraSettings,
    /// Settings +84, consumed by the respawn's optional 0x33 pickup.
    pub settings_84: u32,
    /// 0048F4D0's table, for a network respawn at the camera.
    pub inverse_sine: &'a [i16; 1024],
    /// Process byte 0055F2F4.
    pub local_owner: u8,
    /// Pre-context allocation WORDs and terrain inputs for placement actors,
    /// by placement index. They are observations, never placement positions.
    pub placement_allocation: &'a dyn Fn(u32) -> ActorConstructionRequest<'a>,
    pub camera_allocation: ActorConstructionRequest<'a>,
    pub vehicle_allocation: ActorConstructionRequest<'a>,
}

/// Mutable process and world-owner state carried across loads.
pub struct WorldLoadState<'b> {
    pub sound: crate::object_runtime::SoundTeardownService<'b>,
    /// Actual current0055F14C BYTE, reread after each successful registration.
    /// The enclosing caller owns ordinary/attract admission and this lifetime.
    pub current_attract: &'b Cell<u8>,
    pub mission: &'b mut WorldMission,
    pub world_start: &'b mut WorldStartState,
    /// Process RNG 0055F18C.
    pub rng: &'b mut u32,
    pub map: MapLoadContext<'b>,
    pub listeners: &'b mut CreationListeners,
    pub listener_effects: &'b mut dyn CreationListenerEffects,
    /// The process particle pool the respawn's carry effect births into.
    pub particles: Option<&'b mut ProcessParticleRuntime>,
    /// The world's terrain and furniture loans for a furniture marker whose
    /// finish found its completion bit (00437110). None stops at such a
    /// marker.
    pub furniture: Option<crate::world_furniture_effect::MarkerFurniture<'b>>,
    /// The world timing ring (`G+588..G+5B0`), where an arena world's
    /// completion words lie.
    pub timing: Option<&'b crate::process_particles::timing::WorldTimingRing>,
}

/// `0048EA80`'s words for an arena world, `W+240 + 16 * world` past the
/// mission into the game object: world 26's are the timing ring's samples
/// 1 to 4 (`G+590..G+59F`), world 27's samples 5 to 7 and their average
/// (`G+5A0..G+5AF`). The others' are fields the port does not keep.
fn arena_completion_words(
    world: u32,
    ring: &crate::process_particles::timing::WorldTimingRing,
) -> Option<[u32; 4]> {
    let samples = ring.samples;
    match world {
        26 => Some([samples[1], samples[2], samples[3], samples[4]]),
        27 => Some([samples[5], samples[6], samples[7], ring.average]),
        _ => None,
    }
}

#[derive(Debug)]
pub struct WorldLoadReport {
    /// The actual H14 image shared by load-time commands and later spawns.
    pub(crate) instances: WorldInstanceAllocation,
    /// Each placement's object, or None where the factory rejected it.
    pub placements: Vec<Option<ObjectId>>,
    /// Placements skipped fail-closed for unowned constructor selectors.
    /// Retail would construct these; the port logs and skips them per
    /// feature so one unowned class never aborts the whole phase.
    /// Every other construction error still aborts the load.
    pub skipped: Vec<SkippedPlacement>,
    /// Present when a first player existed and was respawned.
    pub respawn: Option<PlayerRespawn>,
}

/// One placement skipped for an unowned constructor selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkippedPlacement {
    pub placement_index: u32,
    pub resource_argument: u32,
    pub selector: u16,
}

#[derive(Debug, Error)]
pub enum WorldLoaderError {
    #[error("world load requires completed cleanup 00478CD0; the arena still holds {0} objects")]
    ArenaNotClean(usize),
    #[error(transparent)]
    Mission(#[from] WorldMissionError),
    #[error(transparent)]
    Allocation(#[from] InstanceCommandAllocationError),
    #[error(transparent)]
    Construction(#[from] ObjectConstructionError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Passes(#[from] WorldLoadError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Map(#[from] WorldMapConstructionError),
    #[error(transparent)]
    Cleanup(#[from] WorldCleanupError),
    #[error(
        "furniture marker {0:?}'s removal 00437110 needs the world's terrain and furniture loans"
    )]
    MarkerFurniture(ObjectId),
    #[error("furniture marker {object:?}'s removal 00437110: {error}")]
    Marker {
        object: ObjectId,
        error: crate::world_furniture_effect::FurnitureMarkerError<String>,
    },
}

/// 0048DC30 in the offline network domain, with a fresh current-attract source.
pub fn load_world(
    arena: &mut ObjectArena,
    inputs: &WorldLoadInputs<'_>,
    mut state: WorldLoadState<'_>,
) -> Result<WorldLoadReport, WorldLoaderError> {
    if matches!(inputs.manager_policy, WorldLoadManagerPolicy::CleanImport) && !arena.is_empty() {
        return Err(WorldLoaderError::ArenaNotClean(arena.len()));
    }
    let placements = inputs.placements;
    let header = placements.world();
    if matches!(inputs.manager_policy, WorldLoadManagerPolicy::CleanImport) {
        state.mission.begin_world_load()?;
        state.world_start.override_position = None;
    }
    state.mission.apply_world_header(header);
    // The saved mission's +A4 and the start pair 0048DAF0/0048DBC0 read are
    // one native field. The save reader 0048ECC0 and a gate's end 0048E820
    // write it before the load, so a menu load or a gate to a world's later
    // start (Oolbryte's far gate, placement 4, to Terra Nova's third start
    // and hidden base) arrives at that start.
    state.world_start.pair_index = state.mission.save.owner_a4;
    state.world_start.vehicle_selection = 0;
    //44B590 preparation is the admitted immutable terrain domain. Native
    //48DE38/41 constructs/publishes the map before478CD0/placement resets.
    // CleanImport preserves its separate early clean-arena admission.
    construct_world_map(
        state.map,
        WorldMapConstructionRequest {
            plane_byte: ((header.flags >> 5) & 1) as u8,
            terrain: WorldMapTerrainInputs {
                residency: inputs.runtime,
                prepared: Some(PreparedWorldMapTerrain::new(
                    placements.terrain_source(),
                    placements.terrain(),
                    placements.plane_height_fixed(),
                )),
            },
            rng: &mut *state.rng,
        },
    )?;
    if let WorldLoadManagerPolicy::RetainedAfterMissionPrelude { grants } = inputs.manager_policy {
        //0048DE41 publication precedes the captured manager478CD0. This is
        // neither48E890 controlled-player retirement nor map cleanup. Even
        // an empty manager reaches inhibit1/0; errors retain the whole prefix.
        arena.cleanup_world(WorldCleanupRequest {
            mission: &mut *state.mission,
            grants: &grants,
            sound: state.sound.reborrow(),
            listeners: &mut *state.listeners,
            furniture: None,
        })?;
    }
    // An arena's completion words are the game object's fields past the
    // mission; each reader stops on the ones the port does not keep.
    // 0055F448, 0055F3A0 and settings +10 where the process is lent.
    let (network, host, game_mode) = inputs.process.map_or((false, false, 0), |process| {
        (
            process.network_flag != 0,
            process.host_flag != 0,
            process.settings[0x10 / 4],
        )
    });
    let completion = match state.mission.current_world_completion() {
        Err(crate::world_mission::WorldMissionError::ArenaCompletion(world)) => state
            .timing
            .and_then(|ring| arena_completion_words(world, ring)),
        other => Some(other?),
    };
    // Placed pickups' 0046D7D0 tests W+B0 (0048E930) during these placements.
    let mut pickup_words = [0; 200];
    pickup_words[..100].copy_from_slice(&state.mission.save.owner_b0);
    pickup_words[100..].copy_from_slice(&state.mission.save.owner_240);
    arena.set_world_pickup_words(crate::object_runtime::PickupCompletionWords {
        current_world: state.mission.current_world_a0,
        words_b0_3cc: pickup_words,
    });
    // A furniture marker's finish (00435310) tests these (0048EA80).
    if let Some(completion) = completion {
        arena.set_world_completion(completion);
    }
    arena.begin_world_placements();
    let allocation = Arc::new(InstanceCommandAllocation::new(
        Arc::from(placements.world_source().overlay.bytes()),
        placements.assets().worlds.main_payload,
        Arc::from(placements.assets().worlds.relocations.clone()),
    )?);
    let mut objects = Vec::with_capacity(placements.inputs().len());
    let mut skipped = Vec::new();
    for placement in placements.inputs() {
        // Selector 12 queries the current first player's live grant mask.
        let first_grants = match arena.first_player() {
            Some(player) => Some(arena.world_player(player)?.grants_9c),
            None => None,
        };
        let outcome = match construct_world_object(
            arena,
            inputs.runtime,
            ObjectConstructionRequest {
                resource_argument: placement.resource_argument,
                context: BaseContext {
                    position_words: Some(placement.position_words),
                    placement_index: placement.placement_index,
                },
                suffix: Some(allocation.commands(placement.instance)),
                command_allocation: Some(&allocation),
                network_active: network,
                actor: Some(ActorFactoryInputs {
                    allocation: (inputs.placement_allocation)(placement.placement_index),
                    rotation_words: Some(placement.rotation_words),
                    local_owner: inputs.local_owner,
                    first_player_grants: first_grants.as_ref(),
                    grant_table: Some(inputs.grant_table),
                    math: inputs.math,
                    derived: Some(DerivedActorFactoryInputs {
                        world_mask_limits: header.actor_mask_limits(),
                        rng: &mut *state.rng,
                    }),
                    listeners: &mut *state.listeners,
                    listener_effects: &mut *state.listener_effects,
                    network_world_active: host,
                    terrain: Some(placements.terrain()),
                    world_start: Some(WorldStartContext {
                        state: &mut *state.world_start,
                        runtime: inputs.runtime,
                        terrain: placements.terrain(),
                    }),
                    camera_settings: None,
                    spawn_mode_1: false,
                    network_handle: None,
                    vehicle_table: None,
                }),
            },
        ) {
            Ok(outcome) => outcome,
            // Unowned constructor class: skip this placement fail-closed
            // and keep loading. Any other construction failure aborts.
            Err(ObjectConstructionError::UnsupportedSelector {
                resource_argument,
                selector,
            }) => {
                eprintln!(
                    "world load: skipping placement {} with unsupported constructor {selector} (argument {resource_argument:#010x})",
                    placement.placement_index,
                );
                skipped.push(SkippedPlacement {
                    placement_index: placement.placement_index,
                    resource_argument,
                    selector,
                });
                objects.push(None);
                continue;
            }
            // A hint trigger's command 5 queries the first player through
            // NULL natively, which faults; a playerless load skips it.
            Err(ObjectConstructionError::Selector50(error))
                if matches!(
                    *error,
                    crate::object_runtime::Selector50Error::MissingFirstPlayer(_)
                ) =>
            {
                eprintln!(
                    "world load: skipping placement {}: {error} (native faults without a player)",
                    placement.placement_index,
                );
                skipped.push(SkippedPlacement {
                    placement_index: placement.placement_index,
                    resource_argument: placement.resource_argument,
                    selector: 50,
                });
                objects.push(None);
                continue;
            }
            // Any other construction failure still aborts the load.
            Err(other) => return Err(other.into()),
        };
        let object = match outcome {
            ObjectConstructionOutcome::Constructed(object) => {
                // 0048DD2B: only offline does 0047F6C0 count the actor.
                if let Some(actor) = arena.object_actor_component(object)? {
                    if !network {
                        arena.register_loaded_actor(actor, completion.as_ref())?;
                    }
                }
                // The marker's finish found its bit: its 00437110 removes the
                // chain from the world's terrain before the next placement.
                if arena.selector75(object).is_ok() {
                    if let Some((xz, kind)) = arena.take_selector75_chain(object)? {
                        state
                            .furniture
                            .as_mut()
                            .ok_or(WorldLoaderError::MarkerFurniture(object))?
                            .remove(xz, kind)
                            .map_err(|error| WorldLoaderError::Marker { object, error })?;
                    }
                }
                arena.apply_world_load_attract(WorldLoadAttractRequest {
                    object,
                    current_attract: state.current_attract,
                })?;
                Some(object)
            }
            ObjectConstructionOutcome::Rejected { .. } => None,
        };
        objects.push(object);
    }
    let mut factory = CameraFactoryInputs {
        process: inputs.process,
        runtime: inputs.runtime,
        math: inputs.math,
        allocation: inputs.camera_allocation,
        local_owner: inputs.local_owner,
        settings: inputs.camera_settings,
        listeners: &mut *state.listeners,
        listener_effects: &mut *state.listener_effects,
        rng: Some(&mut *state.rng),
    };
    // The retained manager's live grant owners let a placement that the save
    // marked complete run its class +30 during 0048DC30's disposal drain.
    let load_grants = match inputs.manager_policy {
        WorldLoadManagerPolicy::RetainedAfterMissionPrelude { grants } => Some(grants),
        _ => None,
    };
    arena.finish_world_placements(
        ObjectAfterLoadInputs {
            world_completion: completion.as_ref(),
        },
        Some(&mut factory),
        load_grants
            .as_ref()
            .map(|grants| crate::object_runtime::WorldLoadDisposal {
                mission: &mut *state.mission,
                grants,
                sound: state.sound.reborrow(),
            }),
    )?;
    let respawn = match arena.run_loader_player_setup(network, game_mode, &mut factory)? {
        LoaderPlayerSetup::Respawn(player) => {
            let start = WorldStartContext {
                state: &mut *state.world_start,
                runtime: inputs.runtime,
                terrain: placements.terrain(),
            };
            let mut equipment = match (inputs.equipment, inputs.process) {
                (Some(equipment), Some(process)) => Some(WorldEquipment {
                    inputs: equipment,
                    residency: inputs.runtime,
                    process,
                    terrain: placements.terrain(),
                    world_flags: placements.world().flags,
                    camera_settings: inputs.camera_settings,
                    sounds: match &mut state.sound {
                        SoundTeardownService::Available { sounds, .. } => Some(&mut **sounds),
                        SoundTeardownService::RequiredAtReach => None,
                    },
                    particles: state.particles.as_deref_mut(),
                }),
                _ => None,
            };
            // The camera factory lent the loader's RNG to 0040FB10 until now;
            // the respawn draws from it next.
            let rng = factory.rng.take().expect("the loader lends its RNG");
            Some(
                arena.respawn_player(
                    player,
                    PlayerRespawnInputs {
                        start,
                        world_header_18: header.header_words_00_to_34[6],
                        vehicle_allocation: inputs.vehicle_allocation,
                        world_mask_limits: header.actor_mask_limits(),
                        rng,
                        vehicle_table: inputs.vehicle_table,
                        current_world_a0: state.mission.current_world_a0,
                        settings_84: inputs.settings_84,
                        pickups: inputs.pickups,
                        equipment: equipment
                            .as_mut()
                            .map(|host| host as &mut dyn PlayerEquipmentHost),
                        network: inputs
                            .process
                            .filter(|process| process.network_flag != 0)
                            .map(|process| NetworkRespawn {
                                local_slot: process.local_slot,
                                host: process.host_flag != 0,
                                settings: &process.settings,
                                inverse_sine: inputs.inverse_sine,
                            }),
                    },
                    &mut factory,
                )?,
            )
        }
        LoaderPlayerSetup::NoPlayer | LoaderPlayerSetup::AttachedToCamera(_) => None,
    };
    state.mission.finish_world_load(header)?;
    Ok(WorldLoadReport {
        instances: WorldInstanceAllocation::retain(placements, allocation),
        placements: objects,
        skipped,
        respawn,
    })
}

/// 0046A890's equipment services, during a load or the camera's respawn:
/// the vehicle's +4C through the live child factory, then 00421810 with its
/// particle and sounds.
pub(crate) struct WorldEquipment<'x> {
    pub(crate) inputs: WorldLoadEquipmentInputs<'x>,
    pub(crate) residency: &'x ResidentRuntime,
    pub(crate) process: &'x ParticleFactoryProcessState,
    pub(crate) terrain: &'x TerrainRuntime,
    pub(crate) world_flags: u32,
    pub(crate) camera_settings: CameraSettings,
    pub(crate) sounds: Option<&'x mut ProcessSoundRuntime>,
    pub(crate) particles: Option<&'x mut ProcessParticleRuntime>,
}

impl PlayerEquipmentHost for WorldEquipment<'_> {
    fn create_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        resource: ActorChildResource,
        factory: &mut CameraFactoryInputs<'_>,
        world: &mut PlayerEquipmentWorld<'_>,
    ) -> Result<Option<ObjectId>, String> {
        let mut host = crate::session_platform::world_actor_factory::WorldActorChildHost {
            residency: self.residency,
            executable: self.inputs.executable,
            process: self.process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.inputs.actor_clock,
            sine: self.inputs.sine,
            math: factory.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *world.world_start),
            listeners: &mut *factory.listeners,
            listener_effects: &mut *factory.listener_effects,
        };
        // 0046AD1C pushes a NULL position; the caller tests the out slot.
        let mut item = None;
        arena
            .create_actor_child(
                vehicle,
                ActorChildRequest {
                    resource,
                    position: None,
                },
                Some(&mut item),
                &mut host,
                world.rng,
            )
            .map_err(|error| error.to_string())?;
        Ok(item)
    }

    fn attach_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        item: ObjectId,
        rng: &mut u32,
    ) -> Result<bool, String> {
        let context = VehicleCarryContext {
            process: self.process,
            executable: self.inputs.executable,
            resident: self.residency,
            text_data: self.inputs.text_data,
        };
        arena
            .attach_vehicle_item(vehicle, item, context, self, rng)
            .map_err(|error| error.to_string())
    }
}

impl VehicleCarryHost for WorldEquipment<'_> {
    type Error = String;

    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: VehicleCarryEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            VehicleCarryEffect::Particle {
                selector,
                position,
                flags,
            } => {
                let particles = self
                    .particles
                    .as_deref_mut()
                    .ok_or("00489A90 requires the process particle pool")?;
                particles
                    .birth_pickup_flash(
                        ParticleBirthRequest {
                            particle_type: selector,
                            position,
                            velocity: None,
                            flags,
                            inputs: ParticleBirthInputs {
                                network_active: self.process.network_flag != 0,
                                phase: self.inputs.phase,
                                process: self.process,
                            },
                        },
                        self.inputs.executable,
                        rng,
                    )
                    .map_err(|error| error.to_string())?;
            }
            VehicleCarryEffect::Sound(request) => {
                let sounds = self
                    .sounds
                    .as_deref_mut()
                    .ok_or("00487EC0 requires the process sound records")?;
                sounds
                    .enqueue(self.residency, request)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "world_loader_tests.rs"]
mod tests;
