use super::*;
use crate::object_runtime::{BaseContext, ObjectList, ObjectNode, ObjectNodeKind};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn request(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
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
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

#[test]
fn constructor_null_uses_live_process_independently_from_registry_snapshot() {
    let mut arena = arena();
    let mut process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    for raw in [0x7654_3210, 0xabcd_4321] {
        process.process_null_handle = Some(ActorHandle::from_raw(raw));
        let actor = arena
            .allocate_actor(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::Process(&process),
                ..request(None)
            })
            .unwrap();
        assert_eq!(arena.actor(actor).unwrap().owner_handle.raw(), raw);
        assert_eq!(arena.actor_handles().unwrap().null_handle().raw(), 0x1234);
    }
    process.process_null_handle = None;
    let before = format!("{arena:?}");
    assert_eq!(
        arena.allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::Process(&process),
            ..request(None)
        }),
        Err(ActorConstructionError::MissingProcessNull)
    );
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn canonical_58_and_7c_queries_are_pure_with_distinct_behavior_presence() {
    let mut arena = arena();
    let mut rng = 0;
    let construction = DerivedActorConstructionRequest {
        actor: request(None),
        world_mask_limits: [3, 3],
    };
    let base = arena
        .allocate_derived_actor(construction, &mut rng)
        .unwrap();
    let family = arena
        .allocate_family428650_actor(construction, &mut rng)
        .unwrap();
    for actor in [base, family] {
        // The base's +CC is null, and the family's separate +22C is zero.
        // Neither changes the +58 self callback; +140 is also genuinely null.
        assert!(!arena.intro_active_component(actor).unwrap());
        assert_eq!(arena.derived_actor(actor).unwrap().behavior_140, None);
        arena.actor_mut(actor).unwrap().word_b4 = u32::MAX;
        arena.actor_mut(actor).unwrap().word_c4 = i32::MIN as u32;
        let before = arena.object(actor).unwrap().clone();
        assert_eq!(arena.actor_derived_component(actor).unwrap(), Some(actor));
        assert!(!arena.actor_behavior_presence_7c(actor).unwrap());
        assert_eq!(arena.object(actor).unwrap(), &before);
        arena.derived_actor_mut(actor).unwrap().behavior_140 =
            behavior::build_actor_behavior(1, 0, 3);
        let before = arena.object(actor).unwrap().clone();
        assert_eq!(arena.actor_derived_component(actor).unwrap(), Some(actor));
        assert!(arena.actor_behavior_presence_7c(actor).unwrap());
        assert_eq!(arena.object(actor).unwrap(), &before);
    }
    arena.family428650_actor_mut(family).unwrap().byte_22c = u8::MAX;
    assert!(arena.intro_active_component(family).unwrap());
    let before = arena.object(family).unwrap().clone();
    assert_eq!(arena.actor_derived_component(family).unwrap(), Some(family));
    assert!(arena.actor_behavior_presence_7c(family).unwrap());
    assert_eq!(arena.object(family).unwrap(), &before);

    let null_queries = [
        arena.allocate_actor(request(None)).unwrap(),
        arena
            .allocate_selector16(
                request(None),
                CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
            )
            .unwrap(),
        arena.allocate_selector30_actor(request(None)).unwrap(),
        arena.allocate_selector88(request(None)).unwrap(),
        arena.allocate_selector90(request(None)).unwrap(),
    ];
    for actor in null_queries {
        let before = arena.object(actor).unwrap().clone();
        assert_eq!(arena.actor_derived_component(actor).unwrap(), None);
        assert!(!arena.actor_behavior_presence_7c(actor).unwrap());
        assert_eq!(arena.object(actor).unwrap(), &before);
    }
    let nonactor = arena.allocate(ObjectClassKind::Base);
    assert_eq!(
        arena.actor_derived_component(nonactor),
        Err(ObjectError::NotActor(nonactor))
    );
    assert_eq!(
        arena.actor_behavior_presence_7c(nonactor),
        Err(ObjectError::NotActor(nonactor))
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: base,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(
        arena.actor_derived_component(base),
        Err(ObjectError::UnknownObject(base))
    );
    assert_eq!(
        arena.actor_behavior_presence_7c(base),
        Err(ObjectError::UnknownObject(base))
    );
}

#[test]
fn deletion_owns_four_nodes_and_invalidates_the_cached_handle() {
    let mut arena = arena();
    let objects = std::array::from_fn::<_, 3, _>(|_| arena.allocate_actor(request(None)).unwrap());
    let handles = objects.map(|object| arena.actor(object).unwrap().handle);
    assert_eq!(arena.actor_list(), &objects);
    assert_eq!(handles.map(|handle| handle.slot()), [7, 6, 5]);
    assert_eq!(
        arena.resolve_actor_handle(handles[1]).unwrap(),
        Some(objects[1])
    );
    arena.dispose(objects[1]).unwrap();
    // Disposal retains registration and the actor node until destruction.
    assert_eq!(arena.actor_list(), &objects);
    assert_eq!(
        arena.resolve_actor_handle(handles[1]).unwrap(),
        Some(objects[1])
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: objects[1],
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.actor_list(), &[objects[0], objects[2]]);
    assert_eq!(arena.resolve_actor_handle(handles[1]).unwrap(), None);
    assert!(matches!(
        arena.object(objects[1]),
        Err(ObjectError::UnknownObject(_))
    ));
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
        ObjectList::Disposal,
    ] {
        assert!(!arena
            .list(list)
            .iter()
            .any(|node| node.object == objects[1]));
    }
    // Reusing an allocation slot cannot revive its retired ObjectId.
    let replacement = arena.allocate_actor(request(None)).unwrap();
    assert_ne!(replacement, objects[1]);
    assert_eq!(arena.actor_list(), &[objects[0], objects[2], replacement]);
    for object in [objects[0], objects[2], replacement] {
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
}

#[test]
fn allocation_words_are_distinct_from_later_context_and_unknown_fields_stay_unknown() {
    let mut arena = arena();
    let object = arena
        .allocate_actor(request(Some([0xffff, 1, 0x8000])))
        .unwrap();
    let actor = arena.actor(object).unwrap();
    assert_eq!(actor.word_76, None);
    assert_eq!(actor.maximum_health, None);
    assert_eq!(actor.owner_handle.raw(), 0x1234);
    assert_eq!(actor.health, 1000);
    let before = arena.object(object).unwrap().base.clone();
    assert_eq!(before.position_words, Some([0xffff, 1, 0x8000]));
    assert_eq!(before.flags, 0x4800_0127);
    assert_eq!(before.dirty_word, 2);
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some([4, 5, 6]),
                placement_index: 42,
            },
        )
        .unwrap();
    arena.apply_payload_flags(object, 0x2f).unwrap();
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([4, 5, 6])
    );
    assert_eq!(arena.object(object).unwrap().base.flags, 0x4800_002f);
    arena
        .initialize_after_load(object, Default::default())
        .unwrap();
    assert_eq!(arena.object(object).unwrap().base.flags, 0x4880_002f);
    // Actor vtable +24/+3C do not share the visual's flag-1-clear callback.
    let before = arena.object(object).unwrap().clone();
    let nodes = arena.list(ObjectList::Flag1Primary).to_vec();
    assert_eq!(
        arena.update(object),
        Err(ObjectError::ActorUpdateBoundary(object))
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    assert_eq!(arena.list(ObjectList::Flag1Primary), nodes);
}

#[test]
fn registry_errors_do_not_leave_orphaned_object_nodes() {
    let mut absent = ObjectArena::new();
    assert_eq!(
        absent.allocate_actor(request(None)),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert!(absent.is_empty());
    let mut arena = arena();
    let existing = arena.allocate(ObjectClassKind::Base);
    let mut invalid = request(None);
    invalid.owner = 3;
    assert_eq!(
        arena.allocate_actor(invalid),
        Err(ActorHandleError::Owner(3).into())
    );
    assert_eq!(arena.len(), 1);
    assert!(arena.actor_list().is_empty());
    assert_eq!(
        arena.list(ObjectList::Parking),
        &[ObjectNode {
            object: existing,
            kind: ObjectNodeKind::Lifecycle,
        }]
    );
    let actor = arena.allocate_actor(request(None)).unwrap();
    assert_eq!(arena.actor(actor).unwrap().handle.slot(), 7);
    assert_eq!(arena.actor(existing), Err(ObjectError::NotActor(existing)));
}

#[test]
fn placement_lookup_follows_actor_publication_order_and_signed_word_comparison() {
    let mut arena = arena();
    let nonactor = arena.allocate(ObjectClassKind::Base);
    arena
        .initialize_context(
            nonactor,
            BaseContext {
                position_words: None,
                placement_index: 4,
            },
        )
        .unwrap();
    assert_eq!(arena.find_actor_by_placement(5), None);
    let first = arena.allocate_actor(request(None)).unwrap();
    let second = arena.allocate_actor(request(None)).unwrap();
    for object in [first, second] {
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: None,
                    placement_index: 4,
                },
            )
            .unwrap();
    }
    let handle = arena.actor(first).unwrap().handle;
    assert_eq!(arena.find_actor_by_placement(5), Some(first));
    assert_eq!(arena.actor_handle_by_placement(5).unwrap(), handle);
    arena.dispose(first).unwrap();
    assert_eq!(arena.find_actor_by_placement(5), Some(first));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.find_actor_by_placement(5), Some(second));
    for (placement, matching) in [(0xffff, Some(0)), (0x7fff, Some(32768)), (0x8000, None)] {
        arena
            .initialize_context(
                second,
                BaseContext {
                    position_words: None,
                    placement_index: placement,
                },
            )
            .unwrap();
        for reference in [i32::MIN, -1, 0, 32768, 32769, 65536, i32::MAX] {
            assert_eq!(
                arena.find_actor_by_placement(reference),
                matching.filter(|value| *value == reference).map(|_| second)
            );
        }
    }
    assert_eq!(arena.actor_handle_by_placement(-1).unwrap().raw(), 0x1234);
    assert_eq!(
        ObjectArena::new().actor_handle_by_placement(1),
        Err(ActorConstructionError::MissingRegistry)
    );
}
