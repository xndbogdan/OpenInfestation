//! Canonical class-only +64 ownership; constructors supply every class.

use super::*;
use crate::object_runtime::{
    ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface, ObjectClassKind,
    Selector12GrantContext,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, GrantTable, HandlerFraming, HandlerId, HandlerRegion,
    GRANT_TABLE_ADDRESS, TERRAIN_ENTRY_FIXED_BYTES,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

pub(in crate::object_runtime) fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn derived() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: allocation(),
        world_mask_limits: [3, 3],
    }
}

pub(in crate::object_runtime) fn configured_arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 32,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

pub(in crate::object_runtime) fn vehicle(arena: &mut ObjectArena, rng: &mut u32) -> ObjectId {
    // Explicit parsed flat terrain supplies the actual constructor's wave
    // phase; it is neither a query input nor a missing-source fallback.
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0; 3]),
                    terrain: ActorTerrainInputs {
                        terrain: Some(ActorTerrainSurface {
                            terrain: &terrain,
                            plane_height_fixed: 0,
                        }),
                        world_flags: Some(0),
                        ..allocation().terrain
                    },
                    ..allocation()
                },
                ..derived()
            },
            rng,
        )
        .unwrap()
}

fn canonical_actors() -> (ObjectArena, Vec<ObjectId>, ObjectId) {
    let mut arena = configured_arena();
    let mut rng = 0x1234_5678;
    let mut actors = vec![
        arena.allocate_actor(allocation()).unwrap(),
        arena.allocate_derived_actor(derived(), &mut rng).unwrap(),
        arena
            .allocate_family428650_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_family429e20_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_family4236d0_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector10_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector34_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector35_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector36_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector11_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector60_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector77_actor(derived(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector16(
                allocation(),
                CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
            )
            .unwrap(),
        arena.allocate_selector30_actor(allocation()).unwrap(),
        arena.allocate_selector88(allocation()).unwrap(),
        arena.allocate_selector90(allocation()).unwrap(),
        arena.allocate_selector93(allocation()).unwrap(),
    ];
    // Actual no-first-player constructor arm does not consume records.
    let table = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    actors.push(
        arena
            .allocate_selector12_actor(
                derived(),
                &mut rng,
                Selector12GrantContext {
                    table: &table,
                    first_player_grants: None,
                },
            )
            .unwrap(),
    );
    let vehicle = vehicle(&mut arena, &mut rng);
    actors.push(vehicle);
    assert_eq!(actors.len(), 19);
    (arena, actors, vehicle)
}

#[test]
fn canonical_class_query_returns_actual_vehicle_borrow_and_preserves_all_owners() {
    let (mut arena, actors, vehicle) = canonical_actors();
    arena.add_world_player([0x8000_0001; 6]);
    arena.add_world_player([0x1122_3344; 6]);
    // B4 does not choose the virtual binding. Query neither needs the
    // constructor registry after publication nor any behavior/position data.
    for &actor in &actors {
        arena.actor_mut(actor).unwrap().word_b4 = u32::MAX;
    }
    arena.actor_handles = None;
    let before = format!("{arena:?}");
    for &actor in &actors {
        let component = arena.actor_vehicle_component(actor).unwrap();
        assert_eq!(component.is_some(), actor == vehicle);
        if let Some(component) = component {
            assert!(std::ptr::eq(
                component,
                arena.selector2_actor(vehicle).unwrap()
            ));
        }
    }
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn query_and_camera_adapter_keep_class_identity_distinct_from_live_special_byte() {
    let mut arena = configured_arena();
    let mut rng = 1;
    let vehicle = vehicle(&mut arena, &mut rng);
    let helper = arena.allocate_selector93(allocation()).unwrap();
    // Supplied live bytes are not class/vtable changes; missing behavior and
    // registry remain irrelevant to the pure query and this camera adapter.
    arena.actor_handles = None;
    for byte in [0, 1, 0x80, 0xff] {
        let DerivedActorClass::Selector2(state) =
            &mut arena.derived_actor_mut(vehicle).unwrap().class
        else {
            unreachable!()
        };
        state.byte_1c0 = byte;
        let before = format!("{arena:?}");
        assert!(arena.actor_vehicle_component(vehicle).unwrap().is_some());
        assert_eq!(arena.actor_vehicle_component(helper).unwrap(), None);
        assert_eq!(
            arena.camera_derived_component(vehicle).unwrap(),
            Some(CameraDerivedComponent {
                object: vehicle,
                special_view_active: byte != 0,
            })
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn class_query_authenticates_object_lifetime_and_nonactor_without_effects() {
    let mut arena = configured_arena();
    let actor = arena.allocate_actor(allocation()).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let nonactor = arena.allocate(ObjectClassKind::VisualBase);
    let mut other = configured_arena();
    let foreign = other.allocate_actor(allocation()).unwrap();
    let before = format!("{arena:?}");
    assert_eq!(
        arena.actor_vehicle_component(actor),
        Err(ObjectError::UnknownObject(actor))
    );
    assert_eq!(
        arena.actor_vehicle_component(nonactor),
        Err(ObjectError::NotActor(nonactor))
    );
    assert_eq!(
        arena.actor_vehicle_component(foreign),
        Err(ObjectError::UnknownObject(foreign))
    );
    assert_eq!(format!("{arena:?}"), before);
}
