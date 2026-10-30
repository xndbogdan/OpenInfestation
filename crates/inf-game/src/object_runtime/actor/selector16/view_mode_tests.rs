//! Direct original+80 receipts, with native list/dirty byte authority.

use super::*;
use crate::object_runtime::{ActorHandle, FollowTargetRequest};
use crate::{
    object_runtime::{
        ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface,
        DerivedActorConstructionRequest, ObjectClass, ObjectList, ObjectNodeKind,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};
use serde_json::{json, Value};

static SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}
fn allocation(terrain: Option<&TerrainRuntime>) -> ActorConstructionRequest<'_> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0; 3]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: terrain.map(|terrain| ActorTerrainSurface {
                terrain,
                plane_height_fixed: terrain.plane_base(),
            }),
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &SINE,
        },
    }
}
fn request(terrain: Option<&TerrainRuntime>) -> DerivedActorConstructionRequest<'_> {
    DerivedActorConstructionRequest {
        actor: allocation(terrain),
        world_mask_limits: [3, 3],
    }
}
fn flat_terrain() -> TerrainRuntime {
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
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}
fn prepare(arena: &mut ObjectArena, actors: &[ObjectId]) {
    for &actor in actors {
        arena.set_flag0(actor, true).unwrap();
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.flags = 0x125;
        base.dirty_word = 0x80;
        arena.actor_mut(actor).unwrap().byte_d0 = 0x42;
    }
}
fn snapshot(arena: &ObjectArena, actors: &[ObjectId]) -> Value {
    let order = |role| {
        arena
            .list(role)
            .iter()
            .filter(|node| node.kind == ObjectNodeKind::Flag0)
            .map(|node| {
                actors
                    .iter()
                    .position(|actor| *actor == node.object)
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    json!({"states":actors.iter().map(|actor|json!({"flags":arena.object(*actor).unwrap().base.flags,
        "dirty":arena.object(*actor).unwrap().base.dirty_word,"mode":arena.actor(*actor).unwrap().byte_d0})).collect::<Vec<_>>(),
        "primary":order(ObjectList::Flag0Primary),"parking":order(ObjectList::Parking)})
}

#[test]
fn actor_view_modes_match_all_native_byte_modes_dirty_and_list_order() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/actor_view_modes_native.json"
    ))
    .unwrap();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 1024);
    let terrain = flat_terrain();
    for case in receipt["cases"].as_array().unwrap() {
        let method = case["method"].as_u64().unwrap();
        let mut arena = arena();
        let mut rng = 1;
        let actors: Vec<_> = (0..3)
            .map(|_| match method {
                0x427670 => arena
                    .allocate_derived_actor(request(None), &mut rng)
                    .unwrap(),
                0x4231b0 => arena
                    .allocate_selector2_actor(request(Some(&terrain)), &mut rng)
                    .unwrap(),
                0x4074d0 => arena
                    .allocate_selector77_actor(request(None), &mut rng)
                    .unwrap(),
                0x47aae0 => arena.allocate_actor(allocation(None)).unwrap(),
                _ => panic!("unknown original callback"),
            })
            .collect();
        prepare(&mut arena, &actors);
        let mode = case["mode"].as_u64().unwrap() as u8;
        assert_eq!(
            arena.apply_actor_view_mode(actors[1], mode).unwrap(),
            case["accepted"].as_bool().unwrap()
        );
        assert_eq!(
            snapshot(&arena, &actors),
            case["snapshots"][0],
            "{method:08X}/{mode}"
        );
        if method == 0x427670 {
            arena.apply_actor_view_mode(actors[0], mode).unwrap();
            assert_eq!(
                snapshot(&arena, &actors),
                case["snapshots"][1],
                "{method:08X}/{mode} second"
            );
            arena.apply_actor_view_mode(actors[1], mode).unwrap();
            assert_eq!(
                snapshot(&arena, &actors),
                case["snapshots"][2],
                "{method:08X}/{mode} repeated"
            );
        }
    }
}

#[test]
fn authored_derived_class_policies_reuse_flag0_owner_and_signed_byte() {
    let mut arena = arena();
    let mut rng = 1;
    // Every represented derived constructor whose+80 matrix is427670 uses
    // the same policy. This includes the first newly reached selector10.
    let actors = [
        arena
            .allocate_family428650_actor(request(None), &mut rng)
            .unwrap(),
        arena
            .allocate_selector10_actor(request(None), &mut rng)
            .unwrap(),
        arena
            .allocate_selector11_actor(request(None), &mut rng)
            .unwrap(),
        arena
            .allocate_selector60_actor(request(None), &mut rng)
            .unwrap(),
    ];
    prepare(&mut arena, &actors);
    for &actor in &actors {
        assert!(arena.apply_actor_view_mode(actor, 127).unwrap());
        assert_eq!(arena.object(actor).unwrap().base.flags & 1, 0);
        assert_eq!(arena.actor(actor).unwrap().byte_d0, 127);
        assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0x82);
        assert!(arena.apply_actor_view_mode(actor, 128).unwrap());
        assert_eq!(arena.object(actor).unwrap().base.flags & 1, 1);
        assert_eq!(arena.actor(actor).unwrap().byte_d0, 128);
    }
}

#[test]
fn selector77_camera_fallback_accepts_zero_without_mutating_actor() {
    let mut arena = arena();
    let actor = arena
        .allocate_selector77_actor(request(None), &mut 1)
        .unwrap();
    prepare(&mut arena, &[actor]);
    let settings = CameraSettings {
        first_person: 0,
        camera_style: 0,
    };
    let camera = arena
        .allocate_selector16(allocation(None), settings)
        .unwrap();
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(actor).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_target(camera, Some(target), settings)
        .unwrap();
    arena.set_camera_view_mode(camera, 1).unwrap();
    assert_eq!(arena.selector16(camera).unwrap().mode_188, 3);
    assert_eq!(arena.actor(actor).unwrap().byte_d0, 0x42);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x125);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0x80);
}

#[test]
fn invalid_new_target_keeps_old_reset_and_new_target_publication_prefix() {
    let mut arena = arena();
    let old = arena.allocate_derived_actor(request(None), &mut 1).unwrap();
    let new = arena.allocate_actor(allocation(None)).unwrap();
    let settings = CameraSettings {
        first_person: 1,
        camera_style: 0,
    };
    let camera = arena
        .allocate_selector16(allocation(None), settings)
        .unwrap();
    let old_target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(old).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_target(camera, Some(old_target), settings)
        .unwrap();
    assert_eq!(arena.object(old).unwrap().base.flags & 1, 0);
    let new_target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(new).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    // Deliberately invalid portable registry/class custody, not a retail
    // actor fixture: the reached required query must fail without rollback.
    arena.owned_mut(new).unwrap().state.class = ObjectClass::Base;
    assert!(arena
        .replace_camera_target(camera, Some(new_target), settings)
        .is_err());
    assert_eq!(arena.object(old).unwrap().base.flags & 1, 1);
    assert_eq!(arena.actor(old).unwrap().byte_d0, 0);
    assert_eq!(
        arena.selector16(camera).unwrap().target_18c,
        Some(new_target)
    );
    assert_eq!(arena.selector16(camera).unwrap().mode_188, 3);
}
