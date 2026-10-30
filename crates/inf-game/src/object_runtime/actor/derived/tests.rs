use super::*;
use crate::object_runtime::{
    ActorHandle, ActorHandleError, ActorRegistryConfig, ActorTerrainInputs,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
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
        },
        world_mask_limits: [0, 16],
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

#[test]
fn construction_preflight_preserves_rng_and_registration() {
    let mut unconfigured = ObjectArena::new();
    let mut rng = 0x1234_5678;
    assert_eq!(
        unconfigured.allocate_derived_actor(request(), &mut rng),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert_eq!(rng, 0x1234_5678);
    assert!(unconfigured.is_empty());

    let mut arena = arena();
    let mut invalid = request();
    invalid.actor.owner = 3;
    assert_eq!(
        arena.allocate_derived_actor(invalid, &mut rng),
        Err(ActorHandleError::Owner(3).into())
    );
    assert_eq!(rng, 0x1234_5678);
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());

    let actor = arena.allocate_derived_actor(request(), &mut rng).unwrap();
    assert_eq!(arena.actor(actor).unwrap().handle.slot(), 7);
    assert_ne!(rng, 0x1234_5678);
}

#[test]
fn guard_requires_initialization_preserves_no_change_and_replaces_owned_storage() {
    let mut arena = arena();
    let mut rng = 9;
    let first = arena.allocate_derived_actor(request(), &mut rng).unwrap();
    let second = arena.allocate_derived_actor(request(), &mut rng).unwrap();
    let before = arena.object(first).unwrap().clone();
    assert_eq!(
        arena.admit_derived_actor(first),
        Err(DerivedActorError::MissingGuardMode(first))
    );
    assert_eq!(arena.object(first).unwrap(), &before);

    // Model the +13D writes performed by two independent payload initializers.
    for actor in [first, second] {
        arena.derived_actor_mut(actor).unwrap().byte_13d = Some(0x81);
        assert!(arena.admit_derived_actor(actor).unwrap());
    }
    let initial = arena.derived_actor(second).unwrap().behavior_140.clone();
    let state = arena.derived_actor_mut(first).unwrap();
    state.behavior_140.as_mut().unwrap().entries_18[0][1] = 0x1234;
    state.byte_13d = Some(0xff);
    let previous = state.behavior_140.clone();
    assert!(arena.admit_derived_actor(first).unwrap());
    assert_eq!(arena.derived_actor(first).unwrap().behavior_140, previous);
    assert_eq!(arena.derived_actor(second).unwrap().behavior_140, initial);

    arena.derived_actor_mut(first).unwrap().byte_13d = Some(3);
    assert!(arena.admit_derived_actor(first).unwrap());
    let replacement = arena
        .derived_actor(first)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(replacement.entries_18.len(), 1);
    assert_eq!(replacement.groups_1c.len(), 1);
    assert_eq!(replacement.entries_18[0][1], 0);
    let retired = arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let crate::object_runtime::ObjectClass::Actor(retired_actor) = retired.class else {
        panic!("retired actor state");
    };
    let ActorExtension::Derived(retired_derived) = retired_actor.extension else {
        panic!("retired derived state");
    };
    assert!(retired_derived.behavior_140.is_none());
    assert_eq!(arena.derived_actor(second).unwrap().behavior_140, initial);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
}
