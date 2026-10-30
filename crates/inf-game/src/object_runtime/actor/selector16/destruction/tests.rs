//! Genuine type2 ownership plus supplied interrupted-custody domains.
//! No native free/SEH execution or dangling pointer dereference is simulated.

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, CameraDerivedComponent,
    CameraDerivedTargetRequest, CameraGlobalState, CameraHeightRequest, CameraPoseError,
    CameraPoseHost, CameraPoseTickRequest, CameraSettings, FollowTargetPoint, FollowTargetRequest,
    NativeHeapRef, NativeTarget, ObjectClass,
};
use inf_formats::native_basis::{NativeBasis, NativeBasisMath};

static SINE: [u16; 4096] = [0; 4096];
const SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};

fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn fixture() -> (ObjectArena, ObjectId, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let primary = arena.allocate_actor(allocation(Some([7, 9, 11]))).unwrap();
    let secondary = arena
        .allocate_actor(allocation(Some([0x8001, 17, 0xffff])))
        .unwrap();
    let camera = arena
        .allocate_selector16(allocation(None), SETTINGS)
        .unwrap();
    (arena, primary, secondary, camera)
}

fn target(arena: &mut ObjectArena, actor: ObjectId) -> NativeHeapRef {
    let handle = arena.actor(actor).unwrap().handle;
    arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
        .unwrap()
        .unwrap()
}

fn camera_state(
    object: &mut crate::object_runtime::ObjectState,
) -> &mut super::super::Selector16State {
    let ObjectClass::Actor(actor) = &mut object.class else {
        panic!("actor fixture")
    };
    let crate::object_runtime::ActorExtension::Selector16(state) = &mut actor.extension else {
        panic!("camera fixture")
    };
    state
}

fn arena_owners(arena: &ObjectArena) -> String {
    format!(
        "{:?}{:?}",
        (
            &arena.identity,
            &arena.slots,
            &arena.free,
            &arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations
        ),
        (
            &arena.actor_handles,
            &arena.actor_allocation,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            &arena.players,
            &arena.world_load
        )
    )
}

/// Declare precisely the camera change, then compare the entire remaining
/// arena. This does not normalize an unexamined actual owner or warm a cache.
fn expected_camera_change(
    before: &str,
    old: &crate::object_runtime::ObjectState,
    expected: &crate::object_runtime::ObjectState,
) -> String {
    let old = format!("{old:?}");
    assert_eq!(before.matches(&old).count(), 1);
    before.replacen(&old, &format!("{expected:?}"), 1)
}

#[test]
fn genuine_children_retire_before_common_ownership_and_never_destroy_referents() {
    for present in [[false, false], [true, false], [false, true], [true, true]] {
        let (mut arena, primary, secondary, camera) = fixture();
        let player = arena.add_world_player([0x8000_0001; 6]);
        if present[0] {
            let target = target(&mut arena, primary);
            arena
                .replace_camera_target(camera, Some(target), SETTINGS)
                .unwrap();
        }
        if present[1] {
            let target = target(&mut arena, secondary);
            arena
                .replace_camera_secondary_target(camera, Some(target))
                .unwrap();
        }
        let camera_handle = arena.actor(camera).unwrap().handle;
        let referents = [primary, secondary].map(|id| arena.object(id).unwrap().clone());
        let player_before = arena.world_player(player).unwrap().clone();
        let mut registry = arena.actor_handles().unwrap().clone();
        assert!(registry.release(camera_handle));
        let mut expected = arena.object(camera).unwrap().clone();
        let children = {
            let state = camera_state(&mut expected);
            [state.target_18c, state.target_190]
        };
        let lists = arena.lists.clone().map(|mut nodes| {
            nodes.retain(|node| node.object != camera);
            nodes
        });
        let actors: Vec<_> = arena
            .actor_list()
            .iter()
            .copied()
            .filter(|id| *id != camera)
            .collect();
        let (generation, live_count) = (arena.slots[camera.slot].generation, arena.live_count);
        let mut free = arena.free.clone();
        free.push(camera.slot);
        let other_owners = format!(
            "{:?}",
            (
                &arena.routes,
                &arena.map_annotations,
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.world_load,
            )
        );

        assert_eq!(
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: camera,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
                })
                .unwrap(),
            expected
        );
        for reference in children.into_iter().flatten() {
            assert_eq!(
                arena.actor_service_heap().state(reference).unwrap(),
                ActorServiceResourceState::Released
            );
            assert_eq!(
                arena
                    .actor_service_heap()
                    .retained_bytes(reference)
                    .unwrap(),
                0
            );
        }
        assert!(arena.object(camera).is_err());
        assert_eq!(arena.slots[camera.slot].generation, generation + 1);
        assert!(arena.slots[camera.slot].value.is_none());
        assert_eq!(arena.live_count, live_count - 1);
        assert_eq!(arena.free, free);
        assert_eq!(arena.lists, lists);
        assert_eq!(arena.actor_list(), actors);
        assert_eq!(
            format!("{:?}", arena.actor_handles().unwrap()),
            format!("{registry:?}")
        );
        for (id, before) in [primary, secondary].into_iter().zip(referents) {
            assert_eq!(arena.object(id).unwrap(), &before);
        }
        assert_eq!(arena.world_player(player).unwrap(), &player_before);
        assert_eq!(
            format!(
                "{:?}",
                (
                    &arena.routes,
                    &arena.map_annotations,
                    &arena.actor_payloads,
                    &arena.constructor_allocations,
                    &arena.selector60_rows,
                    &arena.world_load,
                )
            ),
            other_owners
        );
    }
}

#[test]
fn deletion_reads_neither_stale_referent_nor_unknown_target_vectors() {
    let (mut arena, primary, secondary, camera) = fixture();
    let stale = arena.actor(primary).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: primary,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut first = arena
        .follow_target(FollowTargetRequest::resolve_actor(stale))
        .unwrap();
    assert_eq!(
        (first.focus_08, first.focus_14, first.velocity_1a),
        (None, None, None)
    );
    // Kind +10 is not the type2 vtable/deleting-destructor identity.
    first.word_10 = 0x8000_0002;
    let first = arena.import_fixture_target(NativeTarget::ActorFollow(first));
    arena
        .replace_camera_target(camera, Some(first), SETTINGS)
        .unwrap();
    let second = arena
        .follow_target(FollowTargetRequest {
            handle: arena.actor(secondary).unwrap().handle,
            point: FollowTargetPoint::ExplicitPoint(None),
        })
        .unwrap();
    let second = arena.import_fixture_target(NativeTarget::ActorFollow(second));
    arena
        .replace_camera_secondary_target(camera, Some(second))
        .unwrap();
    let secondary_before = arena.object(secondary).unwrap().clone();
    let mut expected_registry = arena.actor_handles().unwrap().clone();
    expected_registry.release(arena.actor(camera).unwrap().handle);
    let retired = arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: camera,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let ObjectClass::Actor(actor) = retired.class else {
        panic!("camera")
    };
    let crate::object_runtime::ActorExtension::Selector16(state) = actor.extension else {
        panic!("camera")
    };
    assert_eq!(
        (state.target_18c, state.target_190),
        (Some(first), Some(second))
    );
    for reference in [first, second] {
        assert_eq!(
            arena.actor_service_heap().state(reference).unwrap(),
            ActorServiceResourceState::Released
        );
    }
    assert_eq!(arena.object(secondary).unwrap(), &secondary_before);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{expected_registry:?}")
    );
}

#[test]
fn late_released_secondary_retains_primary_release_and_all_common_owners() {
    let (mut arena, primary, _, camera) = fixture();
    let first = target(&mut arena, primary);
    arena
        .replace_camera_target(camera, Some(first), SETTINGS)
        .unwrap();
    let second = target(&mut arena, primary);
    arena.release_actor_service(second).unwrap();
    arena.selector16_mut(camera).unwrap().target_190 = Some(second);
    let old = arena.object(camera).unwrap().clone();
    let before = arena_owners(&arena);
    let expected = old.clone();
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: camera,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(second)
        ))
    );
    assert_eq!(
        arena_owners(&arena),
        expected_camera_change(&before, &old, &expected)
    );
    let retained = arena_owners(&arena);
    assert_eq!(
        arena.actor_service_heap().state(first).unwrap(),
        ActorServiceResourceState::Released
    );
    let primary_error = ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(first));
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: camera,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(primary_error.clone())
    );
    assert_eq!(
        arena.set_camera_view_mode(camera, 0),
        Err(primary_error.clone())
    );
    assert_eq!(
        arena.replace_camera_target(camera, None, SETTINGS),
        Err(primary_error.clone())
    );
    assert_eq!(
        arena.copy_camera_primary_to_secondary(camera),
        Err(primary_error.clone())
    );
    assert_eq!(
        arena.replace_camera_secondary_target(camera, None),
        Err(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(second)
        ))
    );
    assert_eq!(arena_owners(&arena), retained);
}

#[test]
fn first_released_slot_stops_before_secondary_and_true_null_stays_published() {
    for first_released in [false, true] {
        let (mut arena, _, secondary, camera) = fixture();
        let second = target(&mut arena, secondary);
        let interrupted = target(&mut arena, secondary);
        arena.release_actor_service(interrupted).unwrap();
        if first_released {
            arena
                .replace_camera_secondary_target(camera, Some(second))
                .unwrap();
            arena.selector16_mut(camera).unwrap().target_18c = Some(interrupted);
        } else {
            arena.selector16_mut(camera).unwrap().target_190 = Some(interrupted);
        }
        let before = arena_owners(&arena);
        assert_eq!(
            arena.destroy(crate::object_runtime::ObjectDestructionRequest {
                object: camera,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
            }),
            Err(ObjectError::ActorServiceHeap(
                ActorServiceHeapError::Released(interrupted)
            ))
        );
        assert_eq!(arena_owners(&arena), before);
        let state = arena.selector16(camera).unwrap();
        if first_released {
            assert_eq!(state.target_190, Some(second));
        } else {
            assert_eq!(state.target_18c, None);
        }
    }
}

#[test]
fn normal_replacements_publish_null_and_retain_new_target_on_late_mode_failure() {
    let (mut arena, primary, _, camera) = fixture();
    let first = target(&mut arena, primary);
    arena
        .replace_camera_target(camera, Some(first), SETTINGS)
        .unwrap();
    arena.copy_camera_primary_to_secondary(camera).unwrap();
    let secondary = arena.selector16(camera).unwrap().target_190.unwrap();
    let second_data = arena.actor_follow_target(secondary).unwrap();
    assert_ne!(secondary, first);
    assert_eq!(
        second_data.focus_14,
        arena.actor_follow_target(first).unwrap().focus_14
    );
    assert_eq!(
        (second_data.focus_08, second_data.velocity_1a),
        (None, None)
    );
    arena.replace_camera_secondary_target(camera, None).unwrap();
    arena.replace_camera_target(camera, None, SETTINGS).unwrap();
    assert_eq!(
        arena.actor_service_heap().state(first).unwrap(),
        ActorServiceResourceState::Released
    );
    assert_eq!(
        arena.actor_service_heap().state(secondary).unwrap(),
        ActorServiceResourceState::Released
    );
    let invalid = arena
        .follow_target(FollowTargetRequest {
            handle: ActorHandle::from_raw(u32::MAX),
            point: FollowTargetPoint::ExplicitPoint(None),
        })
        .unwrap();
    let invalid = arena.import_fixture_target(NativeTarget::ActorFollow(invalid));
    // A stale handle with a current registry resolves to NULL and succeeds.
    // Supply the typed unavailable-registry domain after target construction;
    // the old primary is NULL, so only the new target's late mode read stops.
    arena.actor_handles = None;
    let old = arena.object(camera).unwrap().clone();
    let before = arena_owners(&arena);
    let mut expected = old.clone();
    let expected_state = camera_state(&mut expected);
    expected_state.target_18c = Some(invalid);
    assert_eq!(
        arena.replace_camera_target(camera, Some(invalid), SETTINGS),
        Err(ObjectError::MissingActorRegistry)
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.target_18c, Some(invalid));
    assert!(matches!(
        arena.actor_service_heap().state(invalid).unwrap(),
        ActorServiceResourceState::Target(_)
    ));
    assert_eq!(
        arena_owners(&arena),
        expected_camera_change(&before, &old, &expected)
    );
}

#[derive(Default)]
struct ActualComponentHost {
    calls: Vec<&'static str>,
}
impl CameraPoseHost for ActualComponentHost {
    fn query_height(&mut self, _: CameraHeightRequest) -> Result<i16, CameraPoseError> {
        self.calls.push("height");
        Err(CameraPoseError::UnownedHostOperation("height"))
    }
    fn derived_component(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CameraDerivedComponent>, CameraPoseError> {
        self.calls.push("component");
        arena.camera_derived_component(actor)
    }
    fn update_derived_camera_targets(
        &mut self,
        _: &mut ObjectArena,
        _: CameraDerivedTargetRequest<'_>,
    ) -> Result<(), CameraPoseError> {
        self.calls.push("derived");
        Err(CameraPoseError::UnownedHostOperation("derived"))
    }
    fn reselect_target(
        &mut self,
        _: &ObjectArena,
        _: &mut u32,
    ) -> Result<Option<ActorHandle>, CameraPoseError> {
        self.calls.push("reselect");
        Err(CameraPoseError::UnownedHostOperation("reselect"))
    }
    fn camera_target_lost(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), CameraPoseError> {
        self.calls.push("lost");
        Err(CameraPoseError::UnownedHostOperation("lost"))
    }
    fn inverse_sine(&mut self, _: i32) -> Result<i32, CameraPoseError> {
        self.calls.push("inverse");
        Err(CameraPoseError::UnownedHostOperation("inverse"))
    }
    fn rotate_local_vector(
        &mut self,
        _: &mut NativeBasis,
        _: [u16; 3],
    ) -> Result<(), CameraPoseError> {
        self.calls.push("rotate");
        Err(CameraPoseError::UnownedHostOperation("rotate"))
    }
}

#[test]
fn pose_checks_primary_first_and_secondary_only_after_actual_refresh_and_actor_reads() {
    for primary_released in [true, false] {
        let (mut arena, primary, _, camera) = fixture();
        let target = target(&mut arena, primary);
        arena
            .replace_camera_target(camera, Some(target), SETTINGS)
            .unwrap();
        // Supplied current lifecycle and alternate-camera byte; class remains
        // genuine shared actor, so its actual +64 query returns no component.
        arena.actor_mut(primary).unwrap().word_c4 = 3;
        arena.selector16_mut(camera).unwrap().byte_184 = 1;
        let interrupted = if primary_released {
            target
        } else {
            self::target(&mut arena, primary)
        };
        arena.release_actor_service(interrupted).unwrap();
        if !primary_released {
            arena.selector16_mut(camera).unwrap().target_190 = Some(interrupted);
        }
        let old = arena.object(camera).unwrap().clone();
        let before = arena_owners(&arena);
        let mut globals = CameraGlobalState::INITIAL;
        globals.special_view_active = true;
        let mut rng = 0x8000_1234;
        let mut host = ActualComponentHost::default();
        let math = NativeBasisMath::new(SINE);
        assert_eq!(
            arena.tick_camera_pose(
                camera,
                CameraPoseTickRequest {
                    microseconds: 32000,
                    settings: SETTINGS,
                    settings_7c: 1,
                    up: [0.0, 1.0, 0.0],
                    shake_enabled: 1,
                    globals: &mut globals,
                    rng: &mut rng,
                    math: &math,
                    host: &mut host,
                }
            ),
            Err(CameraPoseError::Object(ObjectError::ActorServiceHeap(
                ActorServiceHeapError::Released(interrupted)
            )))
        );
        assert_eq!(rng, 0x8000_1234);
        let expected = old.clone();
        if primary_released {
            assert!(host.calls.is_empty());
            assert!(globals.special_view_active);
        } else {
            assert_eq!(host.calls, ["component"]);
            let actor = arena.actor(primary).unwrap();
            let position = arena.object(primary).unwrap().base.position_words.unwrap();
            let mut target_data = arena.actor_follow_target(target).unwrap();
            let focus = [
                position[0],
                position[1].wrapping_add(actor.word_74),
                position[2],
            ];
            target_data.focus_08 = Some(focus);
            target_data.focus_14 = Some(focus);
            target_data.velocity_1a = Some(actor.words_8e_92);
            assert_eq!(arena.actor_follow_target(target).unwrap(), target_data);
            assert!(!globals.special_view_active);
        }
        assert_eq!(
            arena_owners(&arena),
            expected_camera_change(&before, &old, &expected)
        );
        let mut expected_globals = CameraGlobalState::INITIAL;
        expected_globals.special_view_active = primary_released;
        assert_eq!(globals, expected_globals);
    }
}
