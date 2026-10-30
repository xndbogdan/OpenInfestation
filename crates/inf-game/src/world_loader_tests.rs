use std::cell::{Cell, RefCell};
use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorHandle, ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface,
        CreationListener, CreationListenerError, PickupMessage, AFTER_LOAD_FLAG,
    },
    process_particles::ProcessParticleRuntime,
    process_sound::ProcessSoundRuntime,
    terrain_runtime::TerrainSpacing,
    world_mission::{WorldMissionInputs, RECORD_0C_BYTES},
    Installation, ResidencyRequest,
};
use inf_formats::MissionSaveState;

const SINE: [u16; 4096] = [0; 4096];

#[test]
fn attract_first_world_consumes_suffix_and_keeps_real_offline_respawn_and_mission_tail() {
    let current = Cell::new(1);
    first_world_load(
        &current,
        &mut CreationListeners::new(),
        &mut NoListeners,
        |arena, report| {
            let mut derived = 0;
            for &object in report.placements.iter().flatten() {
                if let Some(actor) = arena.object_actor_component(object).unwrap() {
                    if arena.actor_derived_component(actor).unwrap().is_some() {
                        derived += 1;
                        assert_eq!(arena.object(actor).unwrap().base.flags & 0x0460_0000, 0);
                    }
                }
            }
            assert!(derived > 0);
        },
    );
    assert_eq!(current.get(), 1);
}

#[test]
fn attract_level1_consumes_suffix_and_keeps_real_offline_respawn_and_skip_boundaries() {
    let current = Cell::new(0xff);
    level1_world_load(&current);
    assert_eq!(current.get(), 0xff);
}

/// Supplied callback effects, not an implementation of selector18's native
/// method: change the actual current BYTE and execute a genuine command18.
/// The binding's retained owner is not consumed by this declared callback.
struct ChangeAttract<'a> {
    runtime: &'a ResidentRuntime,
    current: &'a Cell<u8>,
    seen: &'a RefCell<Vec<AttractPlacementBefore>>,
}

struct AttractPlacementBefore {
    object: ObjectId,
    mode: u8,
    flags: u32,
}

impl CreationListenerEffects for ChangeAttract<'_> {
    fn invoke(
        &mut self,
        _listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        use crate::object_runtime::ActorCommandContext;
        use inf_formats::{
            ByteSpan, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
            InstanceField,
        };
        if (arena.object(actor)?.base.placement_word as i16) < 0
            || arena.actor_derived_component(actor)?.is_none()
        {
            return Ok(());
        }
        let mode = if self.seen.borrow().len() % 2 == 0 {
            0x80
        } else {
            0
        };
        self.current.set(mode);
        let command = InstanceCommandView {
            target: InstanceCommandTarget::DerivedActor,
            command_offset: 0,
            code: InstanceField {
                span: ByteSpan::new(0, 4),
                value: 18,
            },
            proven_method: Some(0x0042_6030),
            payload_consumer: Some(0x0042_6030),
            payload: InstanceCommandPayload::ActorNegatedDword144(InstanceField {
                span: ByteSpan::new(4, 8),
                value: 3,
            }),
        };
        assert!(arena
            .apply_derived_actor_command(
                actor,
                &command,
                ActorCommandContext {
                    runtime: self.runtime,
                    first_player_grants: None,
                    terrain: None,
                }
            )
            .unwrap());
        let base = &arena.object(actor)?.base;
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, -3);
        assert_eq!(base.flags & 0x0420_0000, 0x0420_0000);
        self.seen.borrow_mut().push(AttractPlacementBefore {
            object: actor,
            mode,
            flags: base.flags,
        });
        Ok(())
    }
}

#[test]
fn each_successful_placement_reads_listener_modified_current_byte_after_registration() {
    use crate::object_runtime::{CreationListenerInterface, ObjectClassKind};
    let runtime = retail_runtime();
    let current = Cell::new(0);
    let seen = RefCell::new(Vec::new());
    let mut effects = ChangeAttract {
        runtime: &runtime,
        current: &current,
        seen: &seen,
    };
    // Keep the actual binding owner alive while CleanImport starts with an
    // empty load arena. This supplied callback does not dereference owner.
    let mut binding_owner = ObjectArena::new();
    let owner = binding_owner.allocate(ObjectClassKind::Base);
    let mut listeners = CreationListeners::new();
    let binding = listeners.register(owner, CreationListenerInterface::Selector18);
    first_world_load(&current, &mut listeners, &mut effects, |arena, report| {
        let seen = seen.borrow();
        assert!(seen.len() > 1);
        assert!(seen.iter().any(|entry| entry.mode == 0));
        assert!(seen.iter().any(|entry| entry.mode != 0));
        let actors: Vec<_> = report
            .placements
            .iter()
            .flatten()
            .copied()
            .filter(|&object| arena.object_actor_component(object).unwrap().is_some())
            .collect();
        for entry in seen.iter() {
            assert_eq!(
                arena.derived_actor(entry.object).unwrap().word_144,
                if entry.mode == 0 { -3 } else { 3000 }
            );
            let base = &arena.object(entry.object).unwrap().base;
            // Completed factory/after-load callbacks later publish flags
            // and may dispose a real row referent, changing low9 and dirty.
            // Check the suffix-owned mask here; isolated phase tests retain
            // exact whole-state custody checks before those later callbacks.
            assert_eq!(
                base.flags & 0x0460_0000,
                if entry.mode == 0 {
                    entry.flags & 0x0460_0000
                } else {
                    0
                }
            );
            assert_eq!(
                arena.actor(entry.object).unwrap().byte_78,
                actors
                    .iter()
                    .position(|&object| object == entry.object)
                    .unwrap() as u8
            );
        }
    });
    assert_eq!(listeners.get(binding).unwrap().owner, owner);
    assert_eq!(listeners.len(), 1);
    assert_eq!(binding_owner.len(), 1);
}

struct NoListeners;
impl CreationListenerEffects for NoListeners {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

fn retail_runtime() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 32] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    runtime
}

/// An H12-miss allocation environment: classification never reads the
/// unknown pre-context WORDs.
fn miss_allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 245,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

#[test]
fn offline_first_world_loads_end_to_end_like_the_recording() {
    first_world_load(
        &Cell::new(0),
        &mut CreationListeners::new(),
        &mut NoListeners,
        |_, _| {},
    );
}

fn first_world_load(
    current_attract: &Cell<u8>,
    listeners: &mut CreationListeners,
    effects: &mut dyn CreationListenerEffects,
    inspect: impl FnOnce(&ObjectArena, &WorldLoadReport),
) {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let placement_allocation = |_: u32| miss_allocation();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let inputs = WorldLoadInputs {
        equipment: None,
        manager_policy: WorldLoadManagerPolicy::CleanImport,
        process: Some(&process),
        runtime: &runtime,
        placements: &placements,
        math: &math,
        grant_table: &grants,
        pickups: PickupTables {
            grants: &grants,
            texts: &texts,
            upgrade_limits: &limits,
            network_active: false,
        },
        vehicle_table: &vehicle_table,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_84: 0,
        inverse_sine: &[0; 1024],
        local_owner: 1,
        placement_allocation: &placement_allocation,
        camera_allocation: miss_allocation(),
        // Recorded leftover WORDs (0, 2, 0) over the real H12 surface.
        vehicle_allocation: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 2, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: Some(crate::object_runtime::ActorTerrainSurface {
                    terrain: placements.terrain(),
                    plane_height_fixed: placements.plane_height_fixed(),
                }),
                coordinate_shift: 8,
                clock: 245,
                world_flags: Some(placements.world().flags),
                sine_quarter: &SINE,
            },
        },
    };
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    // 00487410's offline player, before the load.
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let mut mission = WorldMission::new(
        MissionSaveState::new_game(),
        WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: Some(Box::new([0; RECORD_0C_BYTES])),
        },
    );
    let mut world_start = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 0x0123_4567;
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let report = load_world(
        &mut arena,
        &inputs,
        WorldLoadState {
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            current_attract,
            mission: &mut mission,
            world_start: &mut world_start,
            rng: &mut rng,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            listeners,
            listener_effects: effects,
            particles: None,
            furniture: None,
            timing: None,
        },
    )
    .unwrap();
    inspect(&arena, &report);
    let loaded_map = map_load.current().unwrap();
    assert_eq!(map_load.pending(), None);
    assert_eq!(maps.jitter(loaded_map).unwrap().jitter_radius(), 0);
    assert_eq!(
        maps.plane_byte(),
        Some(((placements.world().flags >> 5) & 1) as u8)
    );
    assert_eq!(maps.grid().unwrap().indices().len(), 65_536);
    // 118 placements plus the camera and the player vehicle.
    assert_eq!(report.placements.len(), 118);
    assert!(report.placements.iter().all(Option::is_some));
    // Mission 22 needs no fail-closed skips: every constructor is owned.
    assert!(report.skipped.is_empty());
    assert_eq!(arena.len(), 120);
    assert_eq!(arena.world_load_state().actor_serial_3d0, 30);
    assert!(!arena.world_load_state().flag_54ece4);
    for object in report.placements.iter().flatten() {
        assert_ne!(
            arena.object(*object).unwrap().base.flags & AFTER_LOAD_FLAG,
            0
        );
    }
    let respawn = report.respawn.unwrap();
    assert_eq!(
        respawn.messages,
        [crate::object_runtime::RespawnMessage::Pickup(
            PickupMessage {
                prefix_text: 0x139,
                name_text: 384,
                detail_text: Some(385),
            }
        )]
    );
    let vehicle = respawn.vehicle.unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    assert_eq!(handle, ActorHandle::from_raw(0x0ce0_0001));
    assert_eq!(
        arena.object(vehicle).unwrap().base.position_words,
        Some([0x724e, 0xfffc, 0xb559])
    );
    assert_eq!(arena.world_player(player).unwrap().handle_d4, handle);
    assert_eq!(
        arena.camera_handle(),
        Some(ActorHandle::from_raw(0x08e1_0001))
    );
    // Recorded mission tail and world-start override.
    assert_eq!(mission.current_world_a0, 22);
    assert_eq!((mission.word_10, mission.byte_12), (Some(0xaa), Some(1)));
    assert_eq!(
        world_start.override_position,
        Some([0x724e, 0xffbe, 0xb559])
    );
}

/// Every placement of `leaf` through the real construction path, in a
/// world numbered `current_world`. Each must end Constructed,
/// command-Rejected, or UnsupportedSelector-skipped; any other error is a bug
/// in an owned path and fails loudly.
struct PlacementWalk {
    arena: ObjectArena,
    placements: usize,
    constructed: Vec<ObjectId>,
    rejected: usize,
    skipped: Vec<(u32, u16)>,
    factories: Vec<u32>,
    /// With the network flag, the placements whose network completion stops
    /// at a boundary the port holds.
    network_stops: Vec<(u32, String)>,
}

fn walk_leaf_placements(leaf: u8, current_world: u32) -> PlacementWalk {
    walk_leaf_placements_granted(leaf, current_world, &[])
}

/// [`walk_leaf_placements`] with the first player also owning `extra`
/// grants (`004750C0` closures), as a later save's player does.
fn walk_leaf_placements_granted(leaf: u8, current_world: u32, extra: &[u32]) -> PlacementWalk {
    walk_leaf_placements_with(leaf, current_world, extra, false)
}

/// [`walk_leaf_placements_granted`] with the network flag the completion's
/// `+40` reads, in a world the network has not started.
fn walk_leaf_placements_with(
    leaf: u8,
    current_world: u32,
    extra: &[u32],
    network_active: bool,
) -> PlacementWalk {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, leaf] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let allocation = std::sync::Arc::new(
        InstanceCommandAllocation::new(
            std::sync::Arc::from(placements.world_source().overlay.bytes()),
            placements.assets().worlds.main_payload,
            std::sync::Arc::from(placements.assets().worlds.relocations.clone()),
        )
        .unwrap(),
    );
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut mask = [0x2472, 0, 0x4100, 0x1440_0000, 8, 0];
    for &id in extra {
        inf_formats::grant_into(&mut mask, &grants, id).unwrap();
    }
    arena.add_world_player(mask);
    let mut world_start = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 0x0123_4567u32;
    let mut listeners = CreationListeners::new();
    // Placed Force Fields bind 00477350 for every later construction.
    let mut effects = crate::world_backend::EmptyListenerEffects;
    let mut constructed = Vec::new();
    let mut rejected = 0;
    let mut skipped: Vec<(u32, u16)> = Vec::new();
    let mut network_stops = Vec::new();
    let mut factories = Vec::new();
    // A fresh mission marks no pickup collected; placed pickups read it.
    arena.set_world_pickup_words(crate::object_runtime::PickupCompletionWords {
        current_world,
        words_b0_3cc: [0; 200],
    });
    // Nor any placement completed, which a furniture marker's finish tests.
    arena.set_world_completion([0; 4]);
    arena.begin_world_placements();
    for placement in placements.inputs() {
        let first_grants = match arena.first_player() {
            Some(player) => Some(arena.world_player(player).unwrap().grants_9c),
            None => None,
        };
        let outcome = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: placement.resource_argument,
                context: BaseContext {
                    position_words: Some(placement.position_words),
                    placement_index: placement.placement_index,
                },
                suffix: Some(allocation.commands(placement.instance)),
                command_allocation: Some(&allocation),
                network_active,
                actor: Some(ActorFactoryInputs {
                    // The loader's zero-filled pre-context WORDs, which a
                    // Force Field's constructor reads.
                    allocation: ActorConstructionRequest {
                        allocation_position: Some(crate::fresh_heap::fresh_allocation_words()),
                        ..miss_allocation()
                    },
                    rotation_words: Some(placement.rotation_words),
                    local_owner: 1,
                    first_player_grants: first_grants.as_ref(),
                    grant_table: Some(&grants),
                    math: &math,
                    derived: Some(DerivedActorFactoryInputs {
                        world_mask_limits: placements.world().actor_mask_limits(),
                        rng: &mut rng,
                    }),
                    listeners: &mut listeners,
                    listener_effects: &mut effects,
                    network_world_active: false,
                    terrain: Some(placements.terrain()),
                    world_start: Some(WorldStartContext {
                        state: &mut world_start,
                        runtime: &runtime,
                        terrain: placements.terrain(),
                    }),
                    camera_settings: None,
                    spawn_mode_1: false,
                    network_handle: None,
                    vehicle_table: None,
                }),
            },
        );
        match outcome {
            Ok(ObjectConstructionOutcome::Constructed(object)) => {
                constructed.push(object);
                if arena.selector15(object).is_ok() {
                    factories.push(placement.placement_index);
                }
            }
            Ok(ObjectConstructionOutcome::Rejected { .. }) => rejected += 1,
            Err(ObjectConstructionError::UnsupportedSelector { selector, .. }) => {
                skipped.push((placement.placement_index, selector));
            }
            Err(other) if network_active => {
                network_stops.push((placement.placement_index, format!("{other:?}")))
            }
            Err(other) => panic!(
                "placement {} failed with unexpected {other:?}",
                placement.placement_index
            ),
        }
    }
    let walk = PlacementWalk {
        arena,
        placements: placements.inputs().len(),
        constructed,
        rejected,
        skipped,
        factories,
        network_stops,
    };
    assert_eq!(
        walk.constructed.len() + walk.rejected + walk.skipped.len() + walk.network_stops.len(),
        walk.placements
    );
    walk
}

fn skipped_selectors(walk: &PlacementWalk) -> Vec<u16> {
    let mut selectors: Vec<u16> = walk.skipped.iter().map(|&(_, selector)| selector).collect();
    selectors.sort_unstable();
    selectors.dedup();
    selectors
}

#[test]
fn level1_placements_resolve_to_constructed_or_skipped() {
    let walk = walk_leaf_placements(28, 18);
    assert_eq!(walk.placements, 219);
    // Placements 0, 1 and 3 are the three selector-15 factories.
    assert_eq!(walk.factories, [0, 1, 3]);
    // Level 1 places no selector the port does not build; the barrier (69)
    // was the last.
    assert!(skipped_selectors(&walk).is_empty(), "{:?}", walk.skipped);
    assert_eq!(walk.arena.map_annotation_list().len(), 5);
}

/// Terra Nova (world 2, leaf 12) placed one selector the port did not build:
/// the family's own class (selector 9), its trucks and assault craft.
#[test]
fn terra_nova_places_only_selectors_the_port_builds() {
    let walk = walk_leaf_placements(12, 2);
    assert!(skipped_selectors(&walk).is_empty(), "{:?}", walk.skipped);
    let family = walk
        .constructed
        .iter()
        .filter(|&&object| {
            walk.arena.family4236d0_actor(object).is_ok_and(|state| {
                matches!(state.class, crate::object_runtime::Family4236d0Class::Base)
            })
        })
        .count();
    // Two of its 142 placements.
    assert_eq!((family, walk.placements), (2, 142));
}

/// Stendahl Alpha (world 3, leaf 13) placed two selectors the port did not
/// build; selector 54, the rotating terrain light, and selector 18, the
/// Force Field, are built.
#[test]
fn stendahl_alpha_places_only_selectors_the_port_builds() {
    let walk = walk_leaf_placements(13, 3);
    assert!(skipped_selectors(&walk).is_empty(), "{:?}", walk.skipped);
    let lights = walk
        .constructed
        .iter()
        .filter(|&&object| {
            matches!(
                walk.arena.object(object).unwrap().class,
                crate::object_runtime::ObjectClass::RotatingTerrainLight { .. }
            )
        })
        .count();
    assert!(lights > 0);
}

#[derive(serde::Deserialize)]
struct TerrainLight54Receipts {
    built: Vec<TerrainLight54Built>,
}

#[derive(serde::Deserialize)]
struct TerrainLight54Built {
    at: String,
    world: u32,
    resource: u16,
    position: [u16; 3],
    rotation: Option<[u16; 3]>,
    placement: u16,
    /// The 0x74-byte object as 00460120 returned.
    object: String,
}

/// Infest04 builds Oolbryte's (world 20, leaf 30) 34 rotating terrain lights;
/// the port builds the same placements into the same objects.
#[test]
fn oolbryte_rotating_terrain_lights_match_infest04() {
    use crate::object_runtime::{
        BaseObject, ObjectClass, ObjectState, RotatingVisual, TerrainLight, VisualObject,
    };
    let receipts: TerrainLight54Receipts = serde_json::from_str(include_str!(
        "../tests/fixtures/terrain_light54_infest04.json"
    ))
    .unwrap();
    let walk = walk_leaf_placements(30, 20);
    let lights: Vec<_> = walk
        .constructed
        .iter()
        .copied()
        .filter(|&object| {
            matches!(
                walk.arena.object(object).unwrap().class,
                ObjectClass::RotatingTerrainLight { .. }
            )
        })
        .collect();
    assert_eq!(lights.len(), receipts.built.len());
    for built in &receipts.built {
        assert_eq!(built.world, 20);
        let bytes: Vec<u8> = (0..built.object.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&built.object[at..at + 2], 16).unwrap())
            .collect();
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(u32_at(0), 0x0052_65fc, "{}", built.at);
        let object = *lights
            .iter()
            .find(|&&object| {
                walk.arena.object(object).unwrap().base.placement_word == built.placement
            })
            .unwrap();
        let expected = ObjectState {
            base: BaseObject {
                word_28: u16_at(0x28),
                placement_word: u16_at(0x2a),
                flags: u32_at(0x2c),
                resource_word: u16_at(0x30),
                position_words: Some([u16_at(0x32), u16_at(0x34), u16_at(0x36)]),
                dirty_word: u16_at(0x38),
                word_3c: u32_at(0x3c),
            },
            class: ObjectClass::RotatingTerrainLight {
                rotating: RotatingVisual {
                    visual: VisualObject {
                        radius_word: u16_at(0x40),
                        model_word: u16_at(0x42),
                    },
                    basis: inf_formats::native_basis::NativeBasis {
                        words: std::array::from_fn(|word| u32_at(0x44 + 4 * word)),
                    },
                },
                light: Some(TerrainLight {
                    channels: [bytes[0x6c] as i8, bytes[0x6d] as i8, bytes[0x6e] as i8],
                    draw_gate: bytes[0x72],
                    radius: u16_at(0x70),
                }),
            },
        };
        assert_eq!(expected.base.resource_word, built.resource);
        assert_eq!(expected.base.position_words, Some(built.position));
        assert_eq!(built.rotation, Some([0; 3]));
        assert_eq!(
            walk.arena.object(object).unwrap(),
            &expected,
            "{}",
            built.at
        );
    }
}

#[derive(serde::Deserialize)]
struct RigReceipts {
    built: Vec<RigCapture>,
    first: Vec<RigCapture>,
}

#[derive(serde::Deserialize)]
struct RigCapture {
    at: String,
    world: u32,
    /// The 0x144-byte rig.
    object: String,
}

/// The rig's construction and its link: flags' low 24 bits (the high byte
/// differs between recordings), `+28`, the placement and resource words,
/// the position, and the running BYTE, links, level and filling BYTE.
fn rig_capture(hex: &str) -> (u32, u16, u16, u16, [u16; 3], [u32; 4]) {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c) & 0x00ff_ffff,
        word(0x28),
        word(0x2a),
        word(0x30),
        [word(0x32), word(0x34), word(0x36)],
        [
            u32::from(bytes[0x138]),
            dword(0x13c),
            u32::from(word(0x140)),
            u32::from(bytes[0x142]),
        ],
    )
}

fn rig_state(arena: &ObjectArena, rig: ObjectId) -> (u32, u16, u16, u16, [u16; 3], [u32; 4]) {
    let base = &arena.object(rig).unwrap().base;
    let state = arena.selector52(rig).unwrap();
    (
        base.flags & 0x00ff_ffff,
        base.word_28,
        base.placement_word,
        base.resource_word,
        base.position_words.unwrap(),
        [
            u32::from(state.running_138),
            state.links_13c,
            u32::from(state.level_140),
            u32::from(state.filling_142),
        ],
    )
}

/// Helniazine (world 6, leaf 16) builds four mining rigs (placements 76,
/// 99, 102 and 109) as Infest04 does, and its course target's after-load
/// links three of them (`0043B720`'s message 7), as at their first update.
#[test]
fn helniazine_rigs_build_and_link_as_infest04() {
    let receipts: RigReceipts =
        serde_json::from_str(include_str!("../tests/fixtures/mining_rig_infest04.json")).unwrap();
    let walk = walk_leaf_placements(16, 6);
    let rigs: Vec<_> = walk
        .constructed
        .iter()
        .copied()
        .filter(|&object| walk.arena.selector52(object).is_ok())
        .collect();
    assert_eq!(rigs.len(), receipts.built.len());
    for (&rig, built) in rigs.iter().zip(&receipts.built) {
        assert_eq!(built.world, 6);
        assert_eq!(
            rig_state(&walk.arena, rig),
            rig_capture(&built.object),
            "{}",
            built.at
        );
    }
    let (_, _, world) = crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let mut rigs: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&object| world.arena.selector52(object).is_ok())
        .collect();
    rigs.sort_by_key(|&rig| world.arena.object(rig).unwrap().base.placement_word);
    assert_eq!(rigs.len(), receipts.first.len());
    for (&rig, first) in rigs.iter().zip(&receipts.first) {
        assert_eq!(
            rig_state(&world.arena, rig),
            rig_capture(&first.object),
            "{}",
            first.at
        );
    }
}

#[derive(serde::Deserialize)]
struct SirenReceipts {
    built: Vec<SirenCapture>,
    first: Vec<SirenCapture>,
}

#[derive(serde::Deserialize)]
struct SirenCapture {
    recording: String,
    at: String,
    world: u32,
    /// The 0x170-byte Siren.
    object: String,
}

/// The Siren's construction: flags' low 24 bits (the high byte differs
/// between recordings), `+28`, the placement and resource words, the
/// position, the side, the family counts and links, and its own written
/// fields (the radius, alarm, dying count, seconds, spin and turn).
fn siren_capture(hex: &str) -> (u32, u16, u16, u16, [u16; 3], u32, [u32; 4], [u32; 6]) {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c) & 0x00ff_ffff,
        word(0x28),
        word(0x2a),
        word(0x30),
        [word(0x32), word(0x34), word(0x36)],
        dword(0xb4),
        [
            dword(0x13c),
            dword(0x140),
            u32::from(bytes[0x14c]),
            u32::from(bytes[0x14d]),
        ],
        [
            dword(0x164),
            dword(0x160),
            dword(0x168),
            u32::from(bytes[0x16c]),
            u32::from(word(0x15a)),
            u32::from(word(0x15c)),
        ],
    )
}

fn siren_state(
    arena: &ObjectArena,
    siren: ObjectId,
) -> (u32, u16, u16, u16, [u16; 3], u32, [u32; 4], [u32; 6]) {
    let base = &arena.object(siren).unwrap().base;
    let actor = arena.actor(siren).unwrap();
    let state = arena.selector47(siren).unwrap();
    let family = &state.intermediate;
    (
        base.flags & 0x00ff_ffff,
        base.word_28,
        base.placement_word,
        base.resource_word,
        base.position_words.unwrap(),
        actor.word_b4,
        [
            family.word_13c as u32,
            family.word_140 as u32,
            u32::from(family.byte_14c),
            u32::from(family.byte_14d),
        ],
        [
            state.radius_164,
            state.alarm_160,
            state.dying_168,
            u32::from(
                state
                    .seconds_16c
                    .expect("every recorded placement sets its seconds"),
            ),
            u32::from(state.spin_15a),
            u32::from(state.turn_15c),
        ],
    )
}

/// Tabuchi (world 9, leaf 19) and world 11 (leaf 21) build their five
/// Sirens as Infest03 and Infest04 do, and after the load each matches its
/// first update's entry (the silence byte there is heap residue, which the
/// port leaves unknown).
#[test]
fn sirens_build_and_load_as_infest03_and_infest04() {
    let receipts: SirenReceipts =
        serde_json::from_str(include_str!("../tests/fixtures/siren_infest.json")).unwrap();
    // World 11's Sirens stay only for a player owning grant 0x30 (their
    // command 13), as Infest03's and Infest04's did.
    for (leaf, world, extra) in [(19, 9, &[][..]), (21, 11, &[0x30][..])] {
        let walk = walk_leaf_placements_granted(leaf, world, extra);
        let sirens: Vec<_> = walk
            .constructed
            .iter()
            .copied()
            .filter(|&object| walk.arena.selector47(object).is_ok())
            .collect();
        let mut matched = 0;
        for built in receipts.built.iter().filter(|built| built.world == world) {
            let expected = siren_capture(&built.object);
            let siren = *sirens
                .iter()
                .find(|&&siren| walk.arena.object(siren).unwrap().base.placement_word == expected.2)
                .unwrap();
            assert_eq!(
                siren_state(&walk.arena, siren),
                expected,
                "{} {}",
                built.recording,
                built.at
            );
            assert_eq!(walk.arena.selector47(siren).unwrap().silenced_16d, None);
            matched += 1;
        }
        // Each recording builds every one of the world's Sirens.
        assert_eq!(matched, 2 * sirens.len());
        let (_, _, loaded) = crate::test_worlds::offline_world_from_saved_start(
            world,
            0,
            extra,
            TerrainSpacing::Units256,
        );
        for first in receipts.first.iter().filter(|first| first.world == world) {
            let expected = siren_capture(&first.object);
            let siren = loaded
                .arena
                .actor_list()
                .iter()
                .copied()
                .find(|&actor| {
                    loaded.arena.selector47(actor).is_ok()
                        && loaded.arena.object(actor).unwrap().base.placement_word == expected.2
                })
                .unwrap();
            assert_eq!(
                siren_state(&loaded.arena, siren),
                expected,
                "{} {}",
                first.recording,
                first.at
            );
        }
    }
}

#[derive(serde::Deserialize)]
struct PowerballReceipts {
    built: Vec<PowerballCapture>,
    first: Vec<PowerballCapture>,
}

#[derive(serde::Deserialize)]
struct PowerballCapture {
    recording: String,
    at: String,
    world: u32,
    /// The 0x138-byte Powerball.
    object: String,
}

/// The Powerball's construction: flags' low 24 bits (the high byte differs
/// between recordings), `+28`, the placement and resource words, the
/// position, the side, the health and its maximum, the lifecycle state, the
/// payload's `+74/+76/+7A` and `+78`.
type PowerballFields = (
    u32,
    u16,
    u16,
    u16,
    [u16; 3],
    u32,
    [u16; 2],
    u32,
    [u16; 3],
    u8,
);

fn powerball_capture(hex: &str) -> PowerballFields {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c) & 0x00ff_ffff,
        word(0x28),
        word(0x2a),
        word(0x30),
        [word(0x32), word(0x34), word(0x36)],
        dword(0xb4),
        [word(0xa0), word(0xa2)],
        dword(0xc4),
        [word(0x74), word(0x76), word(0x7a)],
        bytes[0x78],
    )
}

fn powerball_state(arena: &ObjectArena, ball: ObjectId) -> PowerballFields {
    let base = &arena.object(ball).unwrap().base;
    let actor = arena.actor(ball).unwrap();
    (
        base.flags & 0x00ff_ffff,
        base.word_28,
        base.placement_word,
        base.resource_word,
        base.position_words.unwrap(),
        actor.word_b4,
        [actor.health, actor.maximum_health.unwrap()],
        actor.word_c4,
        [actor.word_74, actor.word_76.unwrap(), actor.word_7a],
        actor.byte_78,
    )
}

/// World 11 (leaf 21) builds its Powerball (placement 102) as Infest03 and
/// Infest04 do, and after the load it matches its first update's entry,
/// its load serial `+78` (0047F6C0) counting the five Force Fields
/// (selector 18, placements 44..47 and 50) loaded before it. Leaf 24's
/// eight, which neither recording reaches, build too.
#[test]
fn powerball_builds_and_loads_as_infest03_and_infest04() {
    let walk = walk_leaf_placements(24, 14);
    assert!(
        walk.skipped.iter().all(|&(_, selector)| selector != 57),
        "{:?}",
        walk.skipped
    );
    assert_eq!(
        walk.constructed
            .iter()
            .filter(|&&object| walk.arena.selector57(object).is_ok())
            .count(),
        8
    );
    let receipts: PowerballReceipts =
        serde_json::from_str(include_str!("../tests/fixtures/powerball_infest.json")).unwrap();
    let walk = walk_leaf_placements_granted(21, 11, &[0x30]);
    assert!(
        walk.skipped.iter().all(|&(_, selector)| selector != 57),
        "{:?}",
        walk.skipped
    );
    let balls: Vec<_> = walk
        .constructed
        .iter()
        .copied()
        .filter(|&object| walk.arena.selector57(object).is_ok())
        .collect();
    assert_eq!(balls.len(), 1);
    assert_eq!(receipts.built.len(), 2);
    for built in &receipts.built {
        assert_eq!(built.world, 11);
        assert_eq!(
            powerball_state(&walk.arena, balls[0]),
            powerball_capture(&built.object),
            "{} {}",
            built.recording,
            built.at
        );
    }
    let (_, _, loaded) = crate::test_worlds::offline_world_from_saved_start(
        11,
        0,
        &[0x30],
        TerrainSpacing::Units256,
    );
    let balls: Vec<_> = loaded
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| loaded.arena.selector57(actor).is_ok())
        .collect();
    assert_eq!(balls.len(), 1);
    assert!(walk.skipped.iter().all(|&(placement, _)| placement > 102));
    for first in &receipts.first {
        assert_eq!(first.world, 11);
        let state = powerball_state(&loaded.arena, balls[0]);
        assert_eq!(
            state,
            powerball_capture(&first.object),
            "{} {}",
            first.recording,
            first.at
        );
    }
}

#[derive(serde::Deserialize)]
struct BoulderReceipts {
    built: Vec<BoulderCapture>,
    first: Vec<BoulderCapture>,
}

#[derive(serde::Deserialize)]
struct BoulderCapture {
    recording: String,
    at: String,
    world: u32,
    /// The 0x148-byte boulder.
    object: String,
}

/// The boulder's construction: flags' low 24 bits (the high byte differs
/// between recordings), `+28`, the placement and resource words, the
/// position, the side, the health and its maximum, the mass `+8C`, and its
/// own `+138`, `+140` and BYTEs `+144..+146`.
type BoulderFields = (
    u32,
    u16,
    u16,
    u16,
    [u16; 3],
    u32,
    [u16; 2],
    u16,
    [u16; 3],
    u32,
    [u8; 3],
);

fn boulder_capture(hex: &str) -> BoulderFields {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c) & 0x00ff_ffff,
        word(0x28),
        word(0x2a),
        word(0x30),
        [word(0x32), word(0x34), word(0x36)],
        dword(0xb4),
        [word(0xa0), word(0xa2)],
        word(0x8c),
        [word(0x138), word(0x13a), word(0x13c)],
        dword(0x140),
        [bytes[0x144], bytes[0x145], bytes[0x146]],
    )
}

fn boulder_state(arena: &ObjectArena, boulder: ObjectId) -> BoulderFields {
    let base = &arena.object(boulder).unwrap().base;
    let actor = arena.actor(boulder).unwrap();
    let state = arena.selector59(boulder).unwrap();
    (
        base.flags & 0x00ff_ffff,
        base.word_28,
        base.placement_word,
        base.resource_word,
        base.position_words.unwrap(),
        actor.word_b4,
        [actor.health, actor.maximum_health.unwrap()],
        actor.word_8c,
        state.home_138.unwrap(),
        state.rest_140,
        [state.crushes_144, state.crushes_any_145, state.smokes_146],
    )
}

/// World 7 (leaf 17) builds its eight boulders (placements 43..50) as
/// Infest03 does, and world 20 (leaf 30), which now places only selectors
/// the port builds, its two (placements 2 and 3) as Infest04 does; after
/// the load each matches its first update's entry. Infest04's selector-60
/// spawner later builds three more at placement 3's place, and placement
/// 3's own boulder is gone there before the first update.
#[test]
fn boulders_build_and_load_as_infest03_and_infest04() {
    let receipts: BoulderReceipts =
        serde_json::from_str(include_str!("../tests/fixtures/boulders_infest.json")).unwrap();
    let walk = walk_leaf_placements(30, 20);
    assert!(skipped_selectors(&walk).is_empty(), "{:?}", walk.skipped);
    // Leaf 23 (world 13) places the other nineteen.
    let walk = walk_leaf_placements(23, 13);
    assert_eq!(
        walk.constructed
            .iter()
            .filter(|&&object| walk.arena.selector59(object).is_ok())
            .count(),
        19
    );
    // 00413120, the network +40: the shared 0047E2E0 (a placed boulder in a
    // world the network has not started is disposed: dirty 0x8000, and 2 for
    // the disposal's flag change), then dirty 0x10 regardless. The placed pickups stop at their network
    // update, which the port holds.
    let walk = walk_leaf_placements_with(30, 20, &[], true);
    assert!(
        walk.network_stops
            .iter()
            .all(|(_, stop)| stop.contains("PlacedPickupNetworkUpdate")),
        "{:?}",
        walk.network_stops
    );
    let boulders: Vec<_> = walk
        .constructed
        .iter()
        .copied()
        .filter(|&object| walk.arena.selector59(object).is_ok())
        .collect();
    assert_eq!(boulders.len(), 2);
    for &boulder in &boulders {
        assert_eq!(walk.arena.object(boulder).unwrap().base.dirty_word, 0x8012);
    }
    for (leaf, world, recording, count) in [(17, 7, "Infest03", 8), (30, 20, "Infest04", 2)] {
        let walk = walk_leaf_placements(leaf, world);
        assert!(
            walk.skipped.iter().all(|&(_, selector)| selector != 59),
            "{leaf}: {:?}",
            walk.skipped
        );
        let boulders: Vec<_> = walk
            .constructed
            .iter()
            .copied()
            .filter(|&object| walk.arena.selector59(object).is_ok())
            .collect();
        assert_eq!(boulders.len(), count, "{leaf}");
        let built: Vec<_> = receipts
            .built
            .iter()
            .filter(|built| built.recording == recording && built.world == world)
            .collect();
        assert!(built.len() >= count);
        for (&boulder, built) in boulders.iter().zip(&built) {
            assert_eq!(
                boulder_state(&walk.arena, boulder),
                boulder_capture(&built.object),
                "{} {}",
                built.recording,
                built.at
            );
        }
        let (_, _, loaded) =
            crate::test_worlds::offline_world_retained(world, TerrainSpacing::Units256);
        let mut boulders: Vec<_> = loaded
            .arena
            .actor_list()
            .iter()
            .copied()
            .filter(|&actor| loaded.arena.selector59(actor).is_ok())
            .collect();
        boulders.sort_by_key(|&boulder| loaded.arena.object(boulder).unwrap().base.placement_word);
        assert_eq!(boulders.len(), count, "{leaf}");
        for first in receipts
            .first
            .iter()
            .filter(|first| first.recording == recording && first.world == world)
        {
            let expected = boulder_capture(&first.object);
            let Some(&boulder) = boulders.iter().find(|&&boulder| {
                loaded.arena.object(boulder).unwrap().base.placement_word == expected.2
            }) else {
                // The spawner's boulders (placement 0xFFFF) are not loaded.
                assert_eq!(expected.2, 0xffff);
                continue;
            };
            assert_eq!(
                boulder_state(&loaded.arena, boulder),
                expected,
                "{} {}",
                first.recording,
                first.at
            );
        }
    }
}

/// Selector 21's eight placements (leaves 11, 14, 25 and 31) build as
/// selector 10 with their swings at 0x2001.
#[test]
fn nemian_placements_build_with_their_swings() {
    let mut built = 0;
    for (leaf, world) in [(11, 1), (14, 4), (25, 15), (31, 21)] {
        let walk = walk_leaf_placements(leaf, world);
        assert!(
            walk.skipped.iter().all(|&(_, selector)| selector != 21),
            "{leaf}: {:?}",
            walk.skipped
        );
        for &object in &walk.constructed {
            let Ok(state) = walk.arena.selector10_actor(object) else {
                continue;
            };
            if let Some(swing) = state.swing {
                assert_eq!((swing.swing_270, swing.swing_274), (0x2001, 0x2001));
                assert_eq!((swing.speed_278, swing.speed_27c), (0, 0));
                built += 1;
            }
        }
    }
    assert_eq!(built, 8);
}

#[test]
fn offline_level1_world_skips_unowned_constructors_fail_closed() {
    level1_world_load(&Cell::new(0));
}

fn level1_world_load(current_attract: &Cell<u8>) {
    // Mission 18 (level 1 → OVL 28): placements needing unowned
    // constructor selectors skip fail-closed with a logged record
    // instead of aborting the load. The static placement → selector
    // inventory lives in GAMEPLAY_RUNTIME.md (mission-18 census
    // section); this test pins the runtime skip set.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    assert_eq!(placements.inputs().len(), 219);
    let assets = FrontendAssets::open(&root).unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let placement_allocation = |_: u32| miss_allocation();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let inputs = WorldLoadInputs {
        equipment: None,
        manager_policy: WorldLoadManagerPolicy::CleanImport,
        process: Some(&process),
        runtime: &runtime,
        placements: &placements,
        math: &math,
        grant_table: &grants,
        pickups: PickupTables {
            grants: &grants,
            texts: &texts,
            upgrade_limits: &limits,
            network_active: false,
        },
        vehicle_table: &vehicle_table,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_84: 0,
        inverse_sine: &[0; 1024],
        local_owner: 1,
        placement_allocation: &placement_allocation,
        camera_allocation: miss_allocation(),
        vehicle_allocation: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 2, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: Some(crate::object_runtime::ActorTerrainSurface {
                    terrain: placements.terrain(),
                    plane_height_fixed: placements.plane_height_fixed(),
                }),
                coordinate_shift: 8,
                clock: 245,
                world_flags: Some(placements.world().flags),
                sine_quarter: &SINE,
            },
        },
    };
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let _ = player;
    let mut mission = WorldMission::new(
        MissionSaveState {
            mission_selection: 18,
            ..MissionSaveState::new_game()
        },
        WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: Some(Box::new([0; RECORD_0C_BYTES])),
        },
    );
    let mut world_start = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 0x0123_4567;
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    let report = load_world(
        &mut arena,
        &inputs,
        WorldLoadState {
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            current_attract,
            mission: &mut mission,
            world_start: &mut world_start,
            rng: &mut rng,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            listeners: &mut listeners,
            listener_effects: &mut effects,
            particles: None,
            furniture: None,
            timing: None,
        },
    );
    // All 219 placements construct, reject, or skip fail-closed (see
    // `level1_placements_resolve_to_constructed_or_skipped`); the
    // camera no longer needs an attached player (mission-18 probe:
    // retail calls 00478E00 right after detaching) and the spawn tail
    // wires the follow target from the vehicle.
    let report = report.unwrap();
    if current_attract.get() != 0 {
        for &object in report.placements.iter().flatten() {
            if let Some(actor) = arena.object_actor_component(object).unwrap() {
                if arena.actor_derived_component(actor).unwrap().is_some() {
                    assert_eq!(arena.object(actor).unwrap().base.flags & 0x0460_0000, 0);
                }
            }
        }
    }
    assert_eq!(report.placements.len(), 219);
    assert_eq!(
        report.placements.iter().flatten().count() + report.skipped.len(),
        219
    );
    // The selector-37 bridge at placement 106 takes its authored strip, and
    // the course targets 96..98 each link to it (0043B720's message 7).
    // Infest01's three factories at the first briefing marker pass
    // (7417C0:1A2A): the ordinary marker flags, team 0, state 3, model 0x9D,
    // their stores and the grant of their own +7A.
    for (index, store, id, grants) in [(0, 2, 3, [8, 0]), (1, 2, 2, [4, 0]), (3, 0, 37, [0, 0x20])]
    {
        let factory = report.placements[index].unwrap();
        assert_eq!(arena.object(factory).unwrap().base.flags, 0x4890_002f);
        let actor = arena.actor(factory).unwrap();
        assert_eq!(
            (
                actor.word_b4,
                actor.word_c4,
                actor.rotating.visual.model_word,
                actor.word_7a
            ),
            (0, 3, 0x9d, id)
        );
        let state = arena.selector15(factory).unwrap();
        assert_eq!(
            (state.intermediate.word_140, state.intermediate.word_13c),
            (store, 2)
        );
        assert_eq!(state.grants_198.mask_04[..2], grants);
        assert_eq!((state.word_15c, state.word_160), (0, 0));
    }
    let bridge = report.placements[106].unwrap();
    let state = arena.selector37(bridge).unwrap();
    assert_eq!(
        (
            state.word_158,
            state.word_15c,
            state.byte_165,
            state.word_17c
        ),
        (3500, 300, 3, 51)
    );
    assert_eq!(
        (state.intermediate.byte_14c, state.intermediate.byte_14d),
        (3, 1)
    );
    let linked = |arena: &ObjectArena, index: usize| {
        let target = report.placements[index].unwrap();
        let retained = arena
            .selector39(target)
            .unwrap()
            .retained_158
            .clone()
            .unwrap();
        let image = retained.allocation().runtime_allocation();
        let base = retained.allocation_offset;
        assert_eq!(image.dword(base).unwrap(), 1);
        let array = image.dword(base + 4).unwrap();
        image.dword(array).unwrap()
    };
    // Each one-based reference (107) is rewritten in place to the handle.
    let bridge_handle = arena.actor(bridge).unwrap().handle.raw();
    for index in [96, 97, 98] {
        assert_eq!(linked(&arena, index), bridge_handle);
    }
    // Target 189's reference 191 names the barrier at placement 190, which
    // counts the link (message 7). Its basis, flags, length, bolts, half
    // length (+28) and untouched +74 are Infest01's and Infest02's at its
    // first update (AF439D:F9D, FEBA9:B16); its heights follow this
    // harness's coarse terrain (selector69_tests checks the fine fit).
    let barrier = report.placements[190].unwrap();
    assert_eq!(
        linked(&arena, 189),
        arena.actor(barrier).unwrap().handle.raw()
    );
    let state = arena.selector69(barrier).unwrap();
    assert_eq!(
        (
            state.bolts_144,
            state.length_14c,
            state.half_150,
            state.links_15c
        ),
        (10, 3600, Some(1800), 1)
    );
    let base = &arena.object(barrier).unwrap().base;
    assert_eq!((base.word_28, base.flags), (0x708, 0x4890_000b));
    let actor = arena.actor(barrier).unwrap();
    assert_eq!((actor.word_74, actor.word_7a), (0, 0));
    assert_eq!(
        actor.rotating.basis.words,
        [
            3,
            0x3f56_b1ad,
            0,
            0xbf0b_5b17,
            0,
            0x3f80_0000,
            0,
            0x3f0b_5b17,
            0,
            0x3f56_b1ad
        ]
    );
    // The four pickup spawners take their authored bytes. With them, every
    // later actor's death index matches Infest01's: the spawners 12..15,
    // the gate 16 and the two Selector34 ships 21 and 22. Infest01's
    // spawners stand at these X/Z; their heights follow this harness's
    // coarse terrain spacing, not Level 1's fine one.
    for (index, [x, z], bytes, death) in [
        (47, [0xc156, 0x79e6], [2, 8, 6, 1], 12),
        (50, [0x4ac2, 0x6418], [2, 5, 0x23, 1], 13),
        (51, [0x34c1, 0x8b47], [2, 5, 0x2d, 1], 14),
        (52, [0x4b32, 0x8aa9], [2, 0xa, 0x1e, 1], 15),
    ] {
        let spawner = report.placements[index].unwrap();
        let state = arena.selector53(spawner).unwrap();
        assert_eq!(
            [
                state.kind_138,
                state.count_139,
                state.seconds_13a,
                state.repeat_13b
            ],
            bytes
        );
        assert_eq!(state.timer_13c, 0);
        let object = arena.object(spawner).unwrap();
        let position = object.base.position_words.unwrap();
        assert_eq!([position[0], position[2]], [x, z]);
        assert_eq!(object.base.flags & 0xffff, 0x000a, "placement {index}");
        assert_eq!(arena.actor(spawner).unwrap().byte_78, death);
    }
    let gate = report.placements[81].unwrap();
    assert!(arena.selector30_actor(gate).is_ok());
    assert_eq!(arena.actor(gate).unwrap().byte_78, 16);
    for (index, death) in [(102, 21), (103, 22)] {
        let ship = report.placements[index].unwrap();
        assert!(arena.selector34_actor(ship).is_ok());
        assert_eq!(
            arena.actor(ship).unwrap().byte_78,
            death,
            "placement {index}"
        );
    }
    // The selector-58 pad at placement 193 is the briefing's late camera
    // target (actor ID 194); command 2 selects model 0x4E.
    let pad = arena.selector58(report.placements[193].unwrap()).unwrap();
    assert_eq!(pad.rotating.visual.model_word, 0x4e);
    assert_eq!((pad.byte_6c, pad.word_6e), (0, 0));
    let camera = arena.camera_handle().unwrap();
    let camera = arena.resolve_actor_handle(camera).unwrap().unwrap();
    let vehicle = report.respawn.unwrap().vehicle.unwrap();
    let target = arena.selector16(camera).unwrap().target_18c.unwrap();
    let target = arena.actor_follow_target(target).unwrap();
    let focus = target.focus_08.unwrap();
    let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
    assert_eq!(focus[0], position[0]);
    assert_eq!(focus[2], position[2]);
}

#[test]
fn clean_import_preserves_early_arena_admission_and_late_map_publication() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let placement_allocation = |_: u32| miss_allocation();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let inputs = WorldLoadInputs {
        equipment: None,
        manager_policy: WorldLoadManagerPolicy::CleanImport,
        process: Some(&process),
        runtime: &runtime,
        placements: &placements,
        math: &math,
        grant_table: &grants,
        pickups: PickupTables {
            grants: &grants,
            texts: &texts,
            upgrade_limits: &limits,
            network_active: false,
        },
        vehicle_table: &vehicle_table,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_84: 0,
        inverse_sine: &[0; 1024],
        local_owner: 1,
        placement_allocation: &placement_allocation,
        camera_allocation: miss_allocation(),
        vehicle_allocation: miss_allocation(),
    };
    let mut arena = ObjectArena::new();
    arena.allocate(crate::object_runtime::ObjectClassKind::Base);
    let mut mission = WorldMission::new(
        MissionSaveState::new_game(),
        WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: None,
        },
    );
    let before = mission.clone();
    let current_attract = Cell::new(0);
    let mut world_start = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 7;
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    assert!(matches!(
        load_world(
            &mut arena,
            &inputs,
            WorldLoadState {
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                current_attract: &current_attract,
                mission: &mut mission,
                world_start: &mut world_start,
                rng: &mut rng,
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load
                },
                listeners: &mut listeners,
                listener_effects: &mut effects,
                particles: None,
                furniture: None,
                timing: None,
            },
        ),
        Err(WorldLoaderError::ArenaNotClean(1))
    ));
    assert_eq!(mission, before);
    assert_eq!(rng, 7);
    assert_eq!(map_load.current(), None);
    assert_eq!(map_load.pending(), None);
    assert!(maps.grid().is_none());
    assert_eq!(maps.plane_byte(), None);

    // Native48DE41 publishes the completed map BEFORE any reached placement
    // completion read. This typed late index boundary must keep publication,
    // the whole grid and all65,536 advances in the actual caller RNG loan.
    let mut empty_arena = ObjectArena::new();
    mission.save.mission_selection = 34;
    assert!(matches!(
        load_world(
            &mut empty_arena,
            &inputs,
            WorldLoadState {
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                current_attract: &current_attract,
                mission: &mut mission,
                world_start: &mut world_start,
                rng: &mut rng,
                map: crate::world_map::MapLoadContext {
                    process: &mut maps,
                    state: &mut map_load
                },
                listeners: &mut listeners,
                listener_effects: &mut effects,
                particles: None,
                furniture: None,
                timing: None,
            }
        ),
        Err(WorldLoaderError::Mission(
            WorldMissionError::WorldOutOfRange(34)
        ))
    ));
    let published = map_load.current().unwrap();
    assert_eq!(map_load.pending(), None);
    assert_eq!(maps.jitter(published).unwrap().jitter_radius(), 0);
    assert_eq!(maps.grid().unwrap().indices().len(), 65_536);
    let mut expected_rng = 7_u32;
    for _ in 0..65_536 {
        expected_rng = expected_rng.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
    }
    assert_eq!(rng, expected_rng);
    assert_eq!(mission.current_world_a0, 34);
    assert!(empty_arena.is_empty());
}

/// Outcome-robustness probe for the unowned allocation WORDs behind
/// `load_world`. It sweeps candidate triples through the real classifier and
/// the native wave-height function over owned retail inputs only (terrain,
/// plane, flags, sine, clock), reporting how much of the triple space
/// reproduces the recorded first-load outcomes. See the LoadWorld boundary
/// in the briefing runtime document for what this does and does not unblock.
#[test]
fn allocation_word_outcomes_across_the_triple_space() {
    use crate::object_runtime::{
        classify_actor_terrain, plane_height_at, ActorTerrainClassification,
    };

    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let surface = ActorTerrainSurface {
        terrain: placements.terrain(),
        plane_height_fixed: placements.plane_height_fixed(),
    };
    let inputs = ActorTerrainInputs {
        terrain: Some(surface),
        coordinate_shift: 8,
        clock: 245,
        world_flags: Some(placements.world().flags),
        sine_quarter: &assets.sine_table().values,
    };
    let wave_bit = |x: u16, y: u16, z: u16| -> u8 {
        let ground = placements.terrain().height_at([x, z]);
        let plane = placements.plane_height_fixed().wrapping_shr(8);
        let height = plane_height_at(
            [x, z],
            plane,
            ground,
            245,
            true,
            &assets.sine_table().values,
        );
        u8::from((i32::from(y as i16) - i32::from(height)).abs() < 64)
    };

    // The recorded vehicle triple reproduces the recorded outcomes: miss-
    // identical classification and a clear wave bit.
    assert_eq!(
        classify_actor_terrain(Some([0, 2, 0]), inputs),
        Ok(ActorTerrainClassification::Set40000000)
    );
    assert_eq!(wave_bit(0, 2, 0), 0);

    // Full y sweep at the recorded cell: how wide is the reproducing band?
    let mut joint = 0u32;
    let mut class_match = 0u32;
    let mut bit_clear = 0u32;
    let mut first_mismatch: Option<u16> = None;
    let mut last_mismatch: Option<u16> = None;
    for y in 0..=u16::MAX {
        let class_ok = classify_actor_terrain(Some([0, y, 0]), inputs)
            == Ok(ActorTerrainClassification::Set40000000);
        let bit_ok = wave_bit(0, y, 0) == 0;
        if class_ok {
            class_match += 1;
        }
        if bit_ok {
            bit_clear += 1;
        }
        if class_ok && bit_ok {
            joint += 1;
        } else {
            first_mismatch.get_or_insert(y);
            last_mismatch = Some(y);
        }
    }
    println!("y-band reproducing recorded outcomes at cell (0,0): {joint}/65536");
    println!("y values classifying Set40000000 at cell (0,0): {class_match}/65536");
    println!("y values with a clear wave bit at cell (0,0): {bit_clear}/65536");
    println!("mismatch y range: {first_mismatch:?}..={last_mismatch:?}");
    // The fossil sits deep inside the robust region: only large-magnitude
    // negative heights (signed y below about -82, i.e. far above the plane)
    // flip classification to Set20000000. Small-magnitude WORDs reproduce
    // everywhere probed, but that does not bless any particular substitute:
    // reloads read fresh leftovers per construction (see the briefing
    // runtime boundary), and zeros contradict the recorded nonzero entry.
    assert_eq!(joint, 32850);
    assert_eq!(class_match, 32977);
    assert_eq!(bit_clear, 65409);
    assert_eq!((first_mismatch, last_mismatch), (Some(32768), Some(65454)));

    // Coarse cell sweep at the recorded height: how typical is the fossil?
    let mut cells = 0u32;
    let mut typical = 0u32;
    for x in (0..512).step_by(4) {
        for z in (0..512).step_by(4) {
            cells += 1;
            if classify_actor_terrain(Some([x as u16, 2, z as u16]), inputs)
                == Ok(ActorTerrainClassification::Set40000000)
                && wave_bit(x as u16, 2, z as u16) == 0
            {
                typical += 1;
            }
        }
    }
    println!("cells reproducing recorded outcomes at y=2: {typical}/{cells}");
    // At the recorded height every sampled cell agrees, so the fossil's
    // cell is nothing special: near-surface WORDs are unobservable in
    // these two outcome dimensions across the whole map.
    assert_eq!((typical, cells), (16384, 16384));
}

/// These probes lend actual retained owners to the native post-publication
/// manager phase. The fixture's imported players do not claim full469D60.
mod retained_manager {
    use super::*;
    use crate::object_runtime::{
        ActorVehicleCleanupNotificationError, DerivedActorConstructionRequest, ObjectClassKind,
        PlayerActorLookupError, WorldCleanupError,
    };
    use crate::process_particles::factories::ParticleFactoryProcessState;
    use crate::world_map::{MapLoadState, ProcessWorldMapRuntime};

    fn with_sources(
        process: Option<&ParticleFactoryProcessState>,
        probe: impl FnOnce(&WorldLoadInputs<'_>),
    ) {
        let runtime = retail_runtime();
        let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let math = NativeBasisMath::new(assets.sine_table().values);
        let executable =
            || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let grants = inf_formats::parse_grant_table(executable()).unwrap();
        let texts = inf_formats::parse_grant_texts(executable()).unwrap();
        let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
        let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
        let placement_allocation = |_: u32| miss_allocation();
        let inputs = WorldLoadInputs {
            equipment: None,
            manager_policy: WorldLoadManagerPolicy::RetainedAfterMissionPrelude {
                grants: LiveGrantContext {
                    process,
                    executable: None,
                    resident: None,
                    text_data: None,
                },
            },
            process,
            runtime: &runtime,
            placements: &placements,
            math: &math,
            grant_table: &grants,
            pickups: PickupTables {
                grants: &grants,
                texts: &texts,
                upgrade_limits: &limits,
                network_active: false,
            },
            vehicle_table: &vehicle_table,
            camera_settings: CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            settings_84: 0,
            inverse_sine: &[0; 1024],
            local_owner: 1,
            placement_allocation: &placement_allocation,
            camera_allocation: miss_allocation(),
            vehicle_allocation: ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([0, 2, 0]),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: Some(ActorTerrainSurface {
                        terrain: placements.terrain(),
                        plane_height_fixed: placements.plane_height_fixed(),
                    }),
                    coordinate_shift: 8,
                    clock: 245,
                    world_flags: Some(placements.world().flags),
                    sine_quarter: &SINE,
                },
            },
        };
        probe(&inputs);
    }

    struct Owners {
        arena: ObjectArena,
        mission: WorldMission,
        start: WorldStartState,
        rng: u32,
        maps: ProcessWorldMapRuntime,
        map: MapLoadState,
        listeners: CreationListeners,
        effects: NoListeners,
        current_attract: Cell<u8>,
    }

    impl Owners {
        fn new(selection: u32) -> Self {
            let mut save = MissionSaveState::new_game();
            save.mission_selection = selection;
            save.owner_3c[1] = 2;
            Self {
                arena: ObjectArena::with_actor_registry(ActorRegistryConfig {
                    owner_count: 17,
                    initial_capacity: 256,
                    null_handle: ActorHandle::from_raw(0),
                })
                .unwrap(),
                mission: WorldMission::new(
                    save,
                    WorldMissionInputs {
                        current_world_a0: 1,
                        dword_1c: 0x2800,
                        byte_13: Some(1),
                        record_0c: Some(Box::new([0x69; RECORD_0C_BYTES])),
                    },
                ),
                start: WorldStartState {
                    pair_index: 0,
                    override_position: Some([3, 4, 5]),
                    vehicle_selection: 7,
                    harness_spawn: None,
                    port_spawn: None,
                },
                rng: 0x0123_4567,
                maps: ProcessWorldMapRuntime::native_startup(),
                map: MapLoadState::native_mission_constructor(),
                listeners: CreationListeners::new(),
                effects: NoListeners,
                current_attract: Cell::new(0),
            }
        }

        fn prepare(&mut self) {
            // The actual retained caller consumes this native prelude before
            // activation. Selection25 makes an accidental second call fail
            // before map publication, discriminating this explicit contract.
            self.mission.begin_world_load().unwrap();
            self.start.override_position = None;
            assert_eq!(self.mission.save.owner_3c[1], 6);
            assert_eq!(self.mission.byte_a6, Some(0));
            assert_eq!(self.mission.dword_38, Some(0));
            assert_eq!(
                self.mission.current_world_a0,
                self.mission.save.mission_selection
            );
        }

        fn load(
            &mut self,
            inputs: &WorldLoadInputs<'_>,
        ) -> Result<WorldLoadReport, WorldLoaderError> {
            load_world(
                &mut self.arena,
                inputs,
                WorldLoadState {
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    current_attract: &self.current_attract,
                    mission: &mut self.mission,
                    world_start: &mut self.start,
                    rng: &mut self.rng,
                    map: MapLoadContext {
                        process: &mut self.maps,
                        state: &mut self.map,
                    },
                    listeners: &mut self.listeners,
                    listener_effects: &mut self.effects,
                    particles: None,
                    furniture: None,
                    timing: None,
                },
            )
        }

        fn assert_map_prefix(&self, initial_rng: u32, inputs: &WorldLoadInputs<'_>) {
            let map = self.map.current().unwrap();
            assert_eq!(self.map.pending(), None);
            assert_eq!(self.maps.jitter(map).unwrap().jitter_radius(), 0);
            assert_eq!(self.maps.grid().unwrap().indices().len(), 65_536);
            assert_eq!(
                self.maps.plane_byte(),
                Some(((inputs.placements.world().flags >> 5) & 1) as u8)
            );
            let mut expected = initial_rng;
            for _ in 0..65_536 {
                expected = expected.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
            }
            assert_eq!(self.rng, expected);
            assert_eq!(self.start.override_position, None);
            assert_eq!(self.start.vehicle_selection, 0);
            // Late stopped paths must not run the record/tail reset.
            assert_eq!(
                self.mission.record_0c.as_deref(),
                Some(&[0x69; RECORD_0C_BYTES])
            );
            assert_eq!(self.mission.byte_12, None);
        }
    }

    #[test]
    fn retained_noop_and_empty_managers_run_after_map_without_grant_sources() {
        with_sources(None, |inputs| {
            for with_objects in [false, true] {
                let mut owners = Owners::new(34);
                let first = owners.arena.add_world_player([0x35, 1, 2, 3, 4, 5]);
                owners.arena.add_world_player([0x19, 9, 8, 7, 6, 5]);
                let mut saved = owners.arena.export_player_save_state(first).unwrap();
                saved.owner_144 = 0x8100_1234;
                saved.owner_150 = std::array::from_fn(|i| 0xCA00_0000 | i as u32);
                owners
                    .arena
                    .restore_player_save_prefix(first, &saved)
                    .unwrap();
                let players = owners.arena.world_players().to_vec();
                let ids = if with_objects {
                    vec![
                        owners.arena.allocate(ObjectClassKind::Base),
                        owners.arena.allocate(ObjectClassKind::VisualBase),
                    ]
                } else {
                    vec![]
                };
                let registry = format!("{:?}", owners.arena.actor_handles());
                let load_state = owners.arena.world_load_state();
                let rng = owners.rng;
                owners.mission.inhibit_byte_08 = 7;
                owners.prepare();
                assert!(matches!(
                    owners.load(inputs),
                    Err(WorldLoaderError::Mission(
                        WorldMissionError::WorldOutOfRange(34)
                    ))
                ));
                owners.assert_map_prefix(rng, inputs);
                assert!(owners.arena.is_empty());
                assert_eq!(owners.arena.world_players(), players);
                assert_eq!(owners.arena.first_player(), Some(first));
                assert_eq!(owners.arena.export_player_save_state(first).unwrap(), saved);
                assert_eq!(owners.mission.inhibit_byte_08, 0);
                assert_eq!(owners.arena.world_load_state(), load_state);
                assert_eq!(format!("{:?}", owners.arena.actor_handles()), registry);
                for id in ids {
                    assert!(owners.arena.object(id).is_err());
                }
            }
        });
    }

    #[test]
    fn retained_missing_network_source_preserves_map_and_selected_actor() {
        with_sources(None, |inputs| {
            let mut owners = Owners::new(25);
            let actor = owners.arena.allocate_actor(miss_allocation()).unwrap();
            let player = owners.arena.add_world_player([1; 6]);
            let before_arena = format!("{:?}", owners.arena);
            let load_state = owners.arena.world_load_state();
            let rng = owners.rng;
            owners.prepare();
            assert!(
                matches!(owners.load(inputs), Err(WorldLoaderError::Cleanup(WorldCleanupError::MissingProcess(actual))) if actual == actor)
            );
            owners.assert_map_prefix(rng, inputs);
            assert_eq!(owners.mission.inhibit_byte_08, 1);
            assert_eq!(owners.arena.world_load_state(), load_state);
            assert_eq!(format!("{:?}", owners.arena), before_arena);
            assert_eq!(owners.arena.first_player(), Some(player));
        });
    }

    #[test]
    fn retained_vehicle_failure_keeps_prior_deletion_and_current_player_fifo() {
        let process = ParticleFactoryProcessState {
            process_null_handle: None,
            ..ParticleFactoryProcessState::native_startup()
        };
        with_sources(Some(&process), |inputs| {
            let mut owners = Owners::new(25);
            let earlier = owners.arena.allocate_actor(miss_allocation()).unwrap();
            let earlier_handle = owners.arena.actor(earlier).unwrap().handle;
            let vehicle = owners
                .arena
                .allocate_selector2_actor(
                    DerivedActorConstructionRequest {
                        actor: inputs.vehicle_allocation,
                        world_mask_limits: inputs.placements.world().actor_mask_limits(),
                    },
                    &mut owners.rng,
                )
                .unwrap();
            let vehicle_before = owners.arena.object(vehicle).unwrap().clone();
            owners.arena.add_world_player([1; 6]);
            owners.arena.add_world_player([2; 6]);
            let players = owners.arena.world_players().to_vec();
            let rng = owners.rng;
            let load_state = owners.arena.world_load_state();
            owners.prepare();
            assert!(matches!(
                owners.load(inputs),
                Err(WorldLoaderError::Cleanup(WorldCleanupError::Vehicle(
                    ActorVehicleCleanupNotificationError::PlayerLookup(
                        PlayerActorLookupError::MissingProcessNullHandle
                    )
                )))
            ));
            owners.assert_map_prefix(rng, inputs);
            assert_eq!(owners.mission.inhibit_byte_08, 1);
            assert!(owners.arena.object(earlier).is_err());
            assert_eq!(owners.arena.actor_list(), &[vehicle]);
            assert_eq!(owners.arena.object(vehicle).unwrap(), &vehicle_before);
            assert_eq!(owners.arena.world_players(), players);
            assert_eq!(owners.arena.world_load_state(), load_state);
            assert_eq!(owners.arena.len(), 1);
            assert_eq!(
                owners.arena.resolve_actor_handle(earlier_handle).unwrap(),
                None
            );
        });
    }

    #[test]
    fn retained_manager_load_keeps_real_players_through_placement_and_respawn() {
        let process = ParticleFactoryProcessState::native_startup();
        with_sources(Some(&process), |inputs| {
            let mut owners = Owners::new(22);
            let obsolete = owners.arena.allocate(ObjectClassKind::VisualBase);
            let first = owners
                .arena
                .add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
            let second = owners.arena.add_world_player([0x8100_0000; 6]);
            let second_before = owners.arena.world_player(second).unwrap().clone();
            owners.prepare();
            let report = owners.load(inputs).unwrap();
            assert!(owners.arena.object(obsolete).is_err());
            assert_eq!(owners.arena.first_player(), Some(first));
            assert_eq!(owners.arena.world_players().len(), 2);
            assert_eq!(owners.arena.world_player(second).unwrap(), &second_before);
            assert_eq!(report.placements.len(), 118);
            assert!(report.skipped.is_empty());
            assert!(report.placements.iter().all(Option::is_some));
            assert!(report.respawn.unwrap().vehicle.is_some());
            assert_eq!(owners.arena.len(), 120);
            assert_eq!(owners.mission.inhibit_byte_08, 0);
            assert_eq!(owners.mission.byte_12, Some(1));
            assert!(owners
                .mission
                .record_0c
                .as_deref()
                .unwrap()
                .iter()
                .all(|&byte| byte == 0));
            assert!(owners.map.current().is_some());
        });
    }
}

#[test]
fn level1_respawn_carries_the_saved_items_back_into_the_vehicle() {
    // Every retail save carries +190 items (2,2,2) or (2,3,2). H13 slots 2
    // and 3 are both selector 12: 0046A890 creates each through the
    // vehicle's +4C and 00421810 captures it into a +1A4 slot.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let sine = assets.sine_table().values;
    let math = NativeBasisMath::new(sine);
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let placement_allocation = |_: u32| miss_allocation();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let inputs = WorldLoadInputs {
        equipment: Some(WorldLoadEquipmentInputs {
            executable: executable(),
            text_data: None,
            actor_clock: 245,
            sine: &sine,
            phase: Some(4),
        }),
        manager_policy: WorldLoadManagerPolicy::CleanImport,
        process: Some(&process),
        runtime: &runtime,
        placements: &placements,
        math: &math,
        grant_table: &grants,
        pickups: PickupTables {
            grants: &grants,
            texts: &texts,
            upgrade_limits: &limits,
            network_active: false,
        },
        vehicle_table: &vehicle_table,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_84: 0,
        inverse_sine: &[0; 1024],
        local_owner: 1,
        placement_allocation: &placement_allocation,
        camera_allocation: miss_allocation(),
        vehicle_allocation: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 2, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: Some(crate::object_runtime::ActorTerrainSurface {
                    terrain: placements.terrain(),
                    plane_height_fixed: placements.plane_height_fixed(),
                }),
                coordinate_shift: 8,
                clock: 245,
                world_flags: Some(placements.world().flags),
                sine_quarter: &SINE,
            },
        },
    };
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let mut saved = arena.export_player_save_state(player).unwrap();
    saved.owner_190 = [2, 3, 2];
    arena.restore_player_save_prefix(player, &saved).unwrap();
    let mut mission = WorldMission::new(
        MissionSaveState {
            mission_selection: 18,
            ..MissionSaveState::new_game()
        },
        WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: Some(Box::new([0; RECORD_0C_BYTES])),
        },
    );
    let mut world_start = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 0x0123_4567;
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    let mut sounds = ProcessSoundRuntime::new(executable()).unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let report = load_world(
        &mut arena,
        &inputs,
        WorldLoadState {
            sound: crate::object_runtime::SoundTeardownService::Available {
                sounds: &mut sounds,
                voices: &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            },
            current_attract: &Cell::new(0),
            mission: &mut mission,
            world_start: &mut world_start,
            rng: &mut rng,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            listeners: &mut listeners,
            listener_effects: &mut effects,
            particles: Some(&mut particles),
            furniture: None,
            timing: None,
        },
    )
    .unwrap();
    let vehicle = report.respawn.unwrap().vehicle.unwrap();
    let vehicle_handle = arena.actor(vehicle).unwrap().handle;
    let slots = arena.selector2_actor(vehicle).unwrap().handles_1a4.clone();
    assert_eq!(slots.states, [Some(2); 3]);
    for (index, (handle, saved)) in slots.handles.iter().zip(&slots.saved_flags).enumerate() {
        let item = arena.resolve_actor_handle(*handle).unwrap().unwrap();
        assert_eq!(
            arena.object(item).unwrap().base.resource_word,
            [2, 3, 2][index]
        );
        assert!(arena.selector12_actor(item).is_ok());
        let actor = arena.actor(item).unwrap();
        // 0047D7C0 saved the prior flags in both places and marked +80.
        let prior = actor.capture_flags_d4.unwrap();
        assert_eq!(saved.bytes, Some(prior.to_le_bytes().map(Some)));
        assert_eq!(actor.capture_handle_70, Some(vehicle_handle));
        assert_eq!(arena.object(item).unwrap().base.flags & 0xc0, 0xc0);
        // The child inherits the vehicle's team, so 00468C40 returns at once.
        assert_eq!(actor.word_b4, 2);
        assert_eq!(Some(actor.health), actor.maximum_health);
        assert_eq!(
            arena.family428650_actor(item).unwrap().word_228,
            crate::object_runtime::CAPTURED_STATUS_TEXT
        );
    }
    // One 004122A0 effect per slot and three one-shots per attach.
    assert_eq!(particles.pending(1).unwrap().len(), 3);
    assert_eq!(sounds.records().linked_len(), 9);
    assert_eq!(
        arena.export_player_save_state(player).unwrap().owner_190,
        [2, 3, 2]
    );

    // 004122C0 rejects an actor it already carries before any capture, and
    // 00421810 then skips its team, health and sound suffix.
    struct Recorded(Vec<VehicleCarryEffect>);
    impl VehicleCarryHost for Recorded {
        type Error = ();
        fn effect(
            &mut self,
            _: &mut ObjectArena,
            effect: VehicleCarryEffect,
            _: &mut u32,
        ) -> Result<(), ()> {
            self.0.push(effect);
            Ok(())
        }
    }
    let first = arena
        .resolve_actor_handle(slots.handles[0])
        .unwrap()
        .unwrap();
    let context = VehicleCarryContext {
        process: &process,
        executable: executable(),
        resident: &runtime,
        text_data: None,
    };
    let mut recorded = Recorded(Vec::new());
    assert!(!arena
        .attach_vehicle_item(vehicle, first, context, &mut recorded, &mut rng)
        .unwrap());
    assert!(recorded.0.is_empty());
    assert!(matches!(
        arena.attach_vehicle_item(first, first, context, &mut recorded, &mut rng),
        Err(crate::object_runtime::VehicleCarryError::Derived(_))
    ));
}

#[derive(serde::Deserialize)]
struct ForceFieldReceipts {
    first: Vec<ForceFieldCapture>,
}

#[derive(serde::Deserialize)]
struct ForceFieldCapture {
    recording: String,
    at: String,
    world: u32,
    /// The 0x184-byte field at its first update.
    object: String,
}

/// The field at its first update: flags, `+28`, the placement and resource
/// words, the position, the marks `+6C`, the grant `+7A`, the side and
/// state, the radius and height, the off timer, fade and clock, the bit,
/// the warning time, BYTEs `+174..+178`, the key `+17A`, the links and the
/// hint.
type ForceFieldFields = (
    (u32, u16, u16, u16, [u16; 3]),
    (u32, u16, u32, u32),
    (u32, u32, i32, i32, u32),
    (u32, u32, [bool; 5], u16, i32, u32),
);

fn force_field_capture(hex: &str) -> ForceFieldFields {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        (
            dword(0x2c),
            word(0x28),
            word(0x2a),
            word(0x30),
            [word(0x32), word(0x34), word(0x36)],
        ),
        (dword(0x6c), word(0x7a), dword(0xb4), dword(0xc4)),
        (
            dword(0x154),
            dword(0x158),
            dword(0x15c) as i32,
            dword(0x160) as i32,
            dword(0x164),
        ),
        (
            dword(0x16c),
            dword(0x170),
            std::array::from_fn(|at| bytes[0x174 + at] != 0),
            word(0x17a),
            dword(0x17c) as i32,
            dword(0x180),
        ),
    )
}

fn force_field_state(arena: &ObjectArena, field: ObjectId) -> ForceFieldFields {
    let base = &arena.object(field).unwrap().base;
    let actor = arena.actor(field).unwrap();
    let state = arena.selector18(field).unwrap();
    (
        (
            base.flags,
            base.word_28,
            base.placement_word,
            base.resource_word,
            base.position_words.unwrap(),
        ),
        (actor.word_6c, actor.word_7a, actor.word_b4, actor.word_c4),
        (
            state.radius_154,
            state.height_158,
            state.off_timer_15c,
            state.fade_160,
            state.clock_164,
        ),
        (
            state.bit_16c,
            state.warned_170,
            [
                state.secured_told_174,
                state.linked_175,
                state.on_176,
                state.shown_177,
                state.team_passes_178,
            ],
            state.key_grant_17a,
            state.links_17c,
            state.hint_180,
        ),
    )
}

/// Every world's placements build their Force Fields: 26 in worlds 3, 7,
/// 8, 9, 10, 11, 12, 13, 14, 17 and 21. Loaded, Tabuchi (world 9) and
/// Artoria (world 11) match each field's first update in Infest03 and
/// Infest04: the commands' shape, side, key and grant, the generators'
/// links (message 7 at their after-load), each field on and marking the
/// fields around it. The bits come from a process-wide cursor, so a world
/// loaded after another's fields holds them shifted.
#[test]
fn force_fields_build_and_load_as_infest03_and_infest04() {
    let mut census = Vec::new();
    for world in 0..34_u32 {
        let walk = walk_leaf_placements((world + 10) as u8, world);
        assert!(
            walk.skipped.iter().all(|&(_, selector)| selector != 18),
            "{world}: {:?}",
            walk.skipped
        );
        let fields = walk
            .constructed
            .iter()
            .filter(|&&object| walk.arena.selector18(object).is_ok())
            .count();
        if fields != 0 {
            census.push((world, fields));
        }
    }
    assert_eq!(
        census,
        [
            (3, 2),
            (7, 1),
            (8, 5),
            (9, 1),
            (10, 1),
            (11, 6),
            (12, 2),
            (13, 2),
            (14, 2),
            (17, 2),
            (21, 2)
        ]
    );
    let receipts: ForceFieldReceipts =
        serde_json::from_str(include_str!("../tests/fixtures/force_fields_infest.json")).unwrap();
    for world in [9, 11] {
        let (_, _, loaded) =
            crate::test_worlds::offline_world_retained(world, TerrainSpacing::Units256);
        let fields: Vec<_> = loaded
            .arena
            .actor_list()
            .iter()
            .copied()
            .filter(|&actor| loaded.arena.selector18(actor).is_ok())
            .collect();
        for recording in ["Infest03", "Infest04"] {
            let firsts: Vec<_> = receipts
                .first
                .iter()
                .filter(|first| first.recording == recording && first.world == world)
                .collect();
            assert_eq!(firsts.len(), fields.len(), "{recording} {world}");
            let lowest = firsts
                .iter()
                .map(|first| force_field_capture(&first.object).3 .0)
                .min()
                .unwrap();
            let shift = lowest.trailing_zeros() - 1;
            for first in firsts {
                let mut expected = force_field_capture(&first.object);
                expected.1 .0 >>= shift;
                expected.3 .0 >>= shift;
                let &field = fields
                    .iter()
                    .find(|&&field| {
                        loaded.arena.object(field).unwrap().base.placement_word == expected.0 .2
                    })
                    .unwrap();
                assert_eq!(
                    force_field_state(&loaded.arena, field),
                    expected,
                    "{} {}",
                    first.recording,
                    first.at
                );
            }
        }
    }
}
