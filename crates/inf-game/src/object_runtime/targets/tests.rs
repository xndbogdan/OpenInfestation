use super::*;
use crate::object_runtime::{ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs};
use inf_formats::native_basis::NativeBasis;

static SINE: [u16; 4096] = [0; 4096];

pub(super) fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
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

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

#[test]
fn explicit_point_and_unknown_clone_do_not_require_registry_resolution() {
    let mut arena = ObjectArena::new();
    let handle = ActorHandle::from_raw(0x0801_0001);
    for point in [Some([10, 20, 30]), None] {
        let target = arena
            .follow_target(FollowTargetRequest {
                handle,
                point: FollowTargetPoint::ExplicitPoint(point),
            })
            .unwrap();
        assert_eq!(target.focus_08, None);
        assert_eq!(target.focus_14, point);
        assert_eq!(target.velocity_1a, None);
        assert_eq!(target.clone_native(), target);
    }
    assert_eq!(
        arena.follow_target(FollowTargetRequest::resolve_actor(handle)),
        Err(ObjectError::MissingActorRegistry)
    );
}

#[test]
fn resolve_ctor_refresh_settings_and_stale_clone_preserve_native_custody() {
    let mut arena = arena();
    let actor = arena
        .allocate_actor(allocation(Some([0x8000, 0xfff0, 0xffff])))
        .unwrap();
    arena.actor_mut(actor).unwrap().word_74 = 0x30;
    let handle = arena.actor(actor).unwrap().handle;
    let mut target = arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    assert_eq!(target.focus_08, Some([0x8000, 0x20, 0xffff]));
    assert_eq!(target.focus_14, target.focus_08);
    assert_eq!(target.velocity_1a, None);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([1, 2, 3]);
    arena.owned_mut(actor).unwrap().state.base.flags |= 0x40000;
    arena.actor_mut(actor).unwrap().words_8e_92 = [1, 0x8000, 0xffff];
    arena.actor_mut(actor).unwrap().word_c4 = 0x8000_0001;
    let before = arena.object(actor).unwrap().clone();
    assert_eq!(
        arena.refresh_follow_target(&mut target, 0).unwrap(),
        0x8000_0001
    );
    assert_eq!(target.focus_08, Some([1, 0x32, 3]));
    assert_eq!(target.focus_14, Some([0x8000, 0x20, 0xffff]));
    assert_eq!(target.velocity_1a, Some([0xffff, 0x8000, 1]));
    assert_eq!(arena.object(actor).unwrap(), &before);
    arena
        .refresh_follow_target(&mut target, 0x8000_0000)
        .unwrap();
    assert_eq!(target.focus_14, target.focus_08);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let previous = target;
    assert_eq!(arena.refresh_follow_target(&mut target, 1).unwrap(), 0);
    assert_eq!(target, previous);
    assert_eq!(
        arena.follow_target_basis(&target).unwrap(),
        NativeBasis::INITIAL
    );
    let cloned = target.clone_native();
    assert_eq!(cloned.handle_04, handle);
    assert_eq!(cloned.focus_14, target.focus_14);
    assert_eq!(cloned.focus_08, None);
    assert_eq!(cloned.velocity_1a, None);
    let stale_ctor = arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    assert_eq!(stale_ctor.focus_08, None);
    assert_eq!(stale_ctor.focus_14, None);
}

#[test]
fn object_target_same_identity_skips_live_reads_clone_uses_live_position() {
    let mut arena = arena();
    let actor = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let mut target = arena.object_position_target(actor).unwrap();
    assert_eq!(target.handle_04, ActorHandle::from_raw(0x1234));
    assert_eq!(target.word_10, 16);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([4, 5, 6]);
    assert!(arena
        .replace_object_position_target(&mut target, actor)
        .unwrap());
    assert_eq!(target.focus_08, [1, 2, 3]);
    assert_eq!(
        arena
            .clone_object_position_target(&target)
            .unwrap()
            .focus_08,
        [4, 5, 6]
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena
        .replace_object_position_target(&mut target, actor)
        .unwrap());
    assert_eq!(target.focus_08, [1, 2, 3]);
    assert_eq!(
        arena.clone_object_position_target(&target),
        Err(ObjectError::UnknownObject(actor))
    );
}

#[test]
fn object_target_changed_identity_is_retained_before_unknown_xyz_failure() {
    let mut arena = arena();
    let first = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let other = arena.allocate_actor(allocation(None)).unwrap();
    let mut target = arena.object_position_target(first).unwrap();
    assert_eq!(
        arena.replace_object_position_target(&mut target, other),
        Err(ObjectError::TargetObjectPositionUnknown(other))
    );
    assert_eq!(target.object_14, other);
    assert_eq!(target.focus_08, [1, 2, 3]);
    assert!(arena
        .replace_object_position_target(&mut target, other)
        .unwrap());
    arena.owned_mut(other).unwrap().state.base.position_words = Some([4, 5, 6]);
    assert_eq!(
        arena
            .clone_object_position_target(&target)
            .unwrap()
            .focus_08,
        [4, 5, 6]
    );
    assert_eq!(
        arena.object(first).unwrap().base.position_words,
        Some([1, 2, 3])
    );
}
