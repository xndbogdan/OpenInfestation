use std::path::Path;

use serde::Deserialize;

use super::*;
use crate::{
    object_runtime::{ObjectClassKind, ObjectList, ObjectNodeKind},
    Installation, ResidencyRequest,
};

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    operation: String,
    fill_byte: u8,
    initial: Option<Fields>,
    h8: Table,
    context: Option<Context>,
    command: Option<Command>,
    expected: Fields,
    accepted: Option<bool>,
}

#[derive(Deserialize)]
struct Fields {
    word_28: u16,
    placement_word: u16,
    flags: u32,
    resource_word: u16,
    position_words: [u16; 3],
    dirty_word: u16,
    word_3c: u32,
    word_40: u16,
    model_word: u16,
}

impl Fields {
    fn base(&self) -> BaseObject {
        BaseObject {
            word_28: self.word_28,
            placement_word: self.placement_word,
            flags: self.flags,
            resource_word: self.resource_word,
            position_words: Some(self.position_words),
            dirty_word: self.dirty_word,
            word_3c: self.word_3c,
        }
    }

    fn visual(&self) -> VisualObject {
        VisualObject {
            radius_word: self.word_40,
            model_word: self.model_word,
        }
    }
}

#[derive(Deserialize)]
struct Table {
    capacity: usize,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    slot: u16,
    dword_00: u32,
    dword_0e: u32,
}

impl Table {
    // This native fixture deliberately maps an extra slot at the inclusive
    // limit. It tests field policies, not the resident adapter's safe table
    // boundary. The public adapter's rejection is tested separately below.
    fn metadata(&self, word: u16) -> Option<ModelMetadata> {
        if (word as i16) < 0 || usize::from(word) > self.capacity {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.slot == word)
            .map(|entry| ModelMetadata {
                extent_word: entry.dword_00 as u16,
                radius_word: entry.dword_0e as u16,
            })
    }
}

#[derive(Deserialize)]
struct Context {
    position_words: Option<[u16; 3]>,
    index_dword: u32,
    model_argument_dword: Option<u32>,
}

#[derive(Deserialize)]
struct Command {
    code: u32,
    payload_dword: u32,
}

#[test]
fn visual_field_policies_match_native_context_refresh_and_model_command_receipts() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/object_visual_native.json"
    ))
    .unwrap();
    assert_eq!(receipt.cases.len(), 78);
    for case in receipt.cases {
        let mut arena = ObjectArena::new();
        let object = arena.allocate(ObjectClassKind::VisualBase);
        if let Some(initial) = &case.initial {
            let state = &mut arena.owned_mut(object).unwrap().state;
            state.base = initial.base();
            state.class = ObjectClass::Visual(initial.visual());
        }
        let state = &mut arena.owned_mut(object).unwrap().state;
        let (base, visual) = visual_parts(state).unwrap();
        match case.operation.as_str() {
            "construct" => {
                assert!(case.initial.is_none());
                assert_eq!(base.position_words, None, "{} untouched position", case.id);
                assert_eq!(
                    case.expected.position_words,
                    [u16::from_le_bytes([case.fill_byte; 2]); 3],
                    "{} native poison remains",
                    case.id
                );
            }
            "context" => {
                let context = case.context.unwrap();
                let word = context.model_argument_dword.map(|argument| argument as u16);
                initialize_visual_fields(
                    base,
                    visual,
                    BaseContext {
                        position_words: context.position_words,
                        placement_index: context.index_dword,
                    },
                    word,
                    word.and_then(|word| case.h8.metadata(word)),
                );
            }
            "refresh" => {
                refresh_visual_fields(base, visual, case.h8.metadata(visual.model_word));
            }
            "command" => {
                let command = case.command.unwrap();
                // Admission itself is also covered through the canonical
                // decoder and real class command walker in construction tests.
                assert_eq!(case.accepted, Some(command.code <= 2), "{}", case.id);
                if command.code == 2 {
                    let word = command.payload_dword as u16;
                    set_visual_fields(base, visual, word, case.h8.metadata(word));
                }
            }
            other => panic!("unknown native operation {other}"),
        }
        let mut expected_base = case.expected.base();
        if case.operation == "construct" {
            expected_base.position_words = None;
        }
        assert_eq!(*base, expected_base, "{} base", case.id);
        assert_eq!(*visual, case.expected.visual(), "{} visual", case.id);
    }
}

#[test]
fn resident_binding_uses_model_words_and_reports_the_native_one_past_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let capacity = runtime.resource_capacity(8).unwrap();
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::VisualBase);
    let context = BaseContext {
        position_words: Some([0x8000, 0xffff, 1]),
        placement_index: 0xbeef_0003,
    };
    let mut populated = 0;
    let mut missing = 0;
    for slot in 0..capacity {
        if let Some(resource) = runtime.resource(8, slot as u16) {
            let HandlerResources::Models(bank) =
                &resource.overlay.handlers()[resource.registration.region_index]
            else {
                panic!("H8 resource is not a model bank");
            };
            let header = &bank.entries[usize::from(resource.registration.local_index)].header;
            arena
                .initialize_visual_context(object, context, Some(slot as u32), &runtime)
                .unwrap();
            let state = arena.object(object).unwrap();
            let ObjectClass::Visual(visual) = state.class else {
                unreachable!()
            };
            let extent = header.visibility_extent as u16;
            assert_eq!(state.base.word_28, if extent == 0 { 0x100 } else { extent });
            assert_eq!(visual.radius_word, header.raw_0e);
            assert_eq!(visual.model_word, slot as u16);
            assert_eq!(state.base.position_words, context.position_words);
            assert_eq!(state.base.placement_word, 3);
            populated += 1;
        } else {
            assert!(resident_model_metadata(&runtime, slot as u16)
                .unwrap()
                .is_none());
            missing += 1;
        }
    }
    assert!(populated > 0);
    assert!(missing > 0);
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.set_visual_model(object, capacity as u16, &runtime),
        Err(ObjectError::ModelSlotBoundary {
            slot: capacity as u16,
            capacity
        })
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    for word in [u16::MAX, 0x8000, (capacity + 1) as u16] {
        assert!(resident_model_metadata(&runtime, word).unwrap().is_none());
    }
    let base = arena.allocate(ObjectClassKind::Base);
    assert_eq!(
        arena.initialize_visual_context(base, context, None, &runtime),
        Err(ObjectError::NotVisual(base))
    );
}

#[test]
fn exact_class_after_load_and_update_keep_dirty_and_membership_order() {
    for class in [
        ObjectClassKind::Base,
        ObjectClassKind::VisualBase,
        ObjectClassKind::TerrainLight,
        ObjectClassKind::RouteNode,
    ] {
        let mut arena = ObjectArena::new();
        let first = arena.allocate(class);
        let second = arena.allocate(class);
        arena.owned_mut(first).unwrap().state.base.dirty_word = 0xa180;
        arena
            .initialize_after_load(first, Default::default())
            .unwrap();
        assert_eq!(arena.object(first).unwrap().base.flags, 0x0880_0003);
        assert_eq!(arena.object(first).unwrap().base.dirty_word, 0xa180);
        arena.update(first).unwrap();
        arena.update(second).unwrap();
        arena.update(first).unwrap();
        assert_eq!(arena.object(first).unwrap().base.flags, 0x0880_0001);
        assert_eq!(arena.object(first).unwrap().base.dirty_word, 0xa182);
        assert!(arena.list(ObjectList::Flag1Primary).is_empty());
        assert_eq!(
            arena
                .list(ObjectList::Parking)
                .iter()
                .map(|node| (node.object, node.kind))
                .collect::<Vec<_>>(),
            vec![
                (first, ObjectNodeKind::Lifecycle),
                (second, ObjectNodeKind::Lifecycle),
                (second, ObjectNodeKind::Flag1),
                (first, ObjectNodeKind::Flag1)
            ]
        );
    }
}
