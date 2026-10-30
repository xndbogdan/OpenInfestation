use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        ObjectClassKind, ObjectList, ObjectNodeKind,
    },
};

fn math() -> NativeBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    NativeBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

fn models() -> Fixture {
    let models: Models = serde_json::from_str(
        r#"{"capacity":2,"entries":[
            {"slot":0,"dword_00":291,"dword_0e":17767},
            {"slot":1,"dword_00":0,"dword_0e":0}
        ]}"#,
    )
    .unwrap();
    Fixture::new(&models)
}

fn context() -> RotatingVisualContext {
    RotatingVisualContext {
        base: BaseContext {
            position_words: Some([1, 0xffff, 0x8000]),
            placement_index: 0x1234_5678,
        },
        model_argument: Some(0xabcd_0000),
        rotation_words: Some([0xc000, 0x1234, 0x4000]),
    }
}

fn lists(arena: &ObjectArena) -> Vec<Vec<super::super::ObjectNode>> {
    [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
        ObjectList::Disposal,
    ]
    .into_iter()
    .map(|list| arena.list(list).to_vec())
    .collect()
}

#[test]
fn intermediate_constructor_owns_identity_and_only_three_common_nodes() {
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::RotatingVisualBase);
    let second = arena.allocate(ObjectClassKind::RotatingVisualBase);
    assert_eq!(arena.object(first).unwrap().base.position_words, None);
    assert_eq!(
        *arena.rotating_visual(first).unwrap(),
        RotatingVisual {
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            basis: NativeBasis::INITIAL,
        }
    );
    for (list, kind) in [
        (ObjectList::Flag0Primary, ObjectNodeKind::Flag0),
        (ObjectList::Flag1Primary, ObjectNodeKind::Flag1),
        (ObjectList::Parking, ObjectNodeKind::Lifecycle),
    ] {
        assert_eq!(
            arena
                .list(list)
                .iter()
                .map(|node| (node.object, node.kind))
                .collect::<Vec<_>>(),
            [(first, kind), (second, kind)]
        );
    }
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
    assert!(lists(&arena).iter().all(Vec::is_empty));
}

#[test]
fn rotating_context_and_visual_replacement_share_actual_resident_metadata() {
    let fixture = models();
    let math = math();
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::RotatingVisualBase);
    arena
        .initialize_rotating_visual_context(object, context(), &fixture.runtime, &math)
        .unwrap();
    let before = arena.object(object).unwrap().clone();
    assert_eq!(before.base.position_words, Some([1, 0xffff, 0x8000]));
    assert_eq!(before.base.placement_word, 0x5678);
    assert_eq!(before.base.word_28, 0x123);
    assert_eq!(before.base.dirty_word, 0);
    let rotating = arena.rotating_visual(object).unwrap();
    assert_eq!(rotating.visual.model_word, 0);
    assert_eq!(rotating.visual.radius_word, 0x4567);
    assert_eq!(rotating.basis.words[0], 3);
    let basis = rotating.basis;
    let original_lists = lists(&arena);

    arena
        .set_visual_model(object, u16::MAX, &fixture.runtime)
        .unwrap();
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.word_28, 0x100);
    assert_eq!(state.base.dirty_word, 0x100);
    let rotating = arena.rotating_visual(object).unwrap();
    assert_eq!(rotating.visual.radius_word, 0);
    assert_eq!(rotating.visual.model_word, u16::MAX);
    assert_eq!(rotating.basis, basis);
    assert_eq!(lists(&arena), original_lists);
}

#[test]
fn null_rotation_preserves_basis_but_zero_rotation_advances_it() {
    let fixture = models();
    let math = math();
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::RotatingVisualBase);
    arena
        .initialize_rotating_visual_context(object, context(), &fixture.runtime, &math)
        .unwrap();
    let before = arena.object(object).unwrap().clone();
    let basis = arena.rotating_visual(object).unwrap().basis;
    let mut next = context();
    next.base.position_words = None;
    next.model_argument = None;
    next.rotation_words = None;
    arena
        .initialize_rotating_visual_context(object, next, &fixture.runtime, &math)
        .unwrap();
    assert_eq!(arena.rotating_visual(object).unwrap().basis, basis);
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        before.base.position_words
    );
    assert_eq!(arena.rotating_visual(object).unwrap().visual.model_word, 0);
    assert_eq!(arena.object(object).unwrap().base.word_28, 0x100);

    next.rotation_words = Some([0; 3]);
    arena
        .initialize_rotating_visual_context(object, next, &fixture.runtime, &math)
        .unwrap();
    assert_eq!(arena.rotating_visual(object).unwrap().basis.words[0], 6);
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
}

#[test]
fn model_boundary_rejection_precedes_context_and_basis_mutation() {
    let fixture = models();
    let math = math();
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::RotatingVisualBase);
    arena
        .initialize_rotating_visual_context(object, context(), &fixture.runtime, &math)
        .unwrap();
    let before = arena.object(object).unwrap().clone();
    let original_lists = lists(&arena);
    let mut request = context();
    request.base.position_words = Some([7, 8, 9]);
    request.base.placement_index = 8;
    request.model_argument = Some(0xffff_0002);
    assert_eq!(
        arena.initialize_rotating_visual_context(object, request, &fixture.runtime, &math),
        Err(ObjectError::ModelSlotBoundary {
            slot: 2,
            capacity: 2
        })
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    assert_eq!(lists(&arena), original_lists);
}

#[test]
fn rotating_operations_reject_other_classes_and_expired_ids_without_mutation() {
    let fixture = models();
    let math = math();
    let mut arena = ObjectArena::new();
    for class in [
        ObjectClassKind::Base,
        ObjectClassKind::VisualBase,
        ObjectClassKind::TerrainLight,
        ObjectClassKind::RouteNode,
    ] {
        let object = arena.allocate(class);
        let before = arena.object(object).unwrap().clone();
        let original_lists = lists(&arena);
        assert_eq!(
            arena.initialize_rotating_visual_context(object, context(), &fixture.runtime, &math),
            Err(ObjectError::NotRotatingVisual(object))
        );
        assert_eq!(
            arena.rotate_visual(object, RotationPlane::Yz, 4, &math),
            Err(ObjectError::NotRotatingVisual(object))
        );
        assert_eq!(
            arena.transform_visual_vector_words(object, [1, 2, 3]),
            Err(ObjectError::NotRotatingVisual(object))
        );
        assert_eq!(arena.object(object).unwrap(), &before);
        assert_eq!(lists(&arena), original_lists);
    }
    let old = arena.allocate(ObjectClassKind::RotatingVisualBase);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: old,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let live = arena.allocate(ObjectClassKind::RotatingVisualBase);
    assert_ne!(old, live);
    assert_eq!(
        arena.rotate_visual(old, RotationPlane::Xz, 0, &math),
        Err(ObjectError::UnknownObject(old))
    );
    assert_eq!(
        arena.rotating_visual(live).unwrap().basis,
        NativeBasis::INITIAL
    );
}

#[test]
fn shared_after_load_and_update_policies_preserve_rotating_state() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::RotatingVisualBase);
    arena
        .rotate_visual(object, RotationPlane::Xy, 0x1234, &math())
        .unwrap();
    let rotating = *arena.rotating_visual(object).unwrap();
    arena
        .initialize_after_load(object, Default::default())
        .unwrap();
    assert_eq!(arena.object(object).unwrap().base.flags, 0x0880_0003);
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
    arena.update(object).unwrap();
    assert_eq!(arena.object(object).unwrap().base.flags, 0x0880_0001);
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 2);
    assert_eq!(*arena.rotating_visual(object).unwrap(), rotating);
}
