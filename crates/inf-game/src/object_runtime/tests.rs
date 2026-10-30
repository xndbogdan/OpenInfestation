use std::collections::BTreeMap;

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    #[serde(flatten)]
    operation: Operation,
    state: Snapshot,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Construct {
        object: usize,
        fill_byte: u8,
    },
    Context {
        object: usize,
        position_words: Option<[u16; 3]>,
        index_dword: u32,
    },
    Flags {
        object: usize,
        payload_dword: u32,
    },
    SetFlag0 {
        object: usize,
        argument_dword: u32,
    },
    SetFlag1 {
        object: usize,
        argument_dword: u32,
    },
    Dispose {
        object: usize,
    },
    Destroy {
        object: usize,
    },
    SeedFields {
        object: usize,
        flags: Option<u32>,
        dirty_word: Option<u16>,
        word_3c: Option<u32>,
    },
    SwapOwners {
        role: String,
    },
}

#[derive(Deserialize)]
struct Snapshot {
    objects: Vec<NativeObject>,
    lists: BTreeMap<String, Vec<[usize; 2]>>,
    owners: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct NativeObject {
    id: usize,
    flags: u32,
    scale_word: u16,
    placement_word: u16,
    resource_word: u16,
    position_words: [u16; 3],
    dirty_word: u16,
    word_3c: u32,
}

struct LiveReceiptObject {
    id: ObjectId,
    fill: u16,
    position_written: bool,
}

#[test]
fn common_objects_match_unmodified_native_routines_and_owner_swaps() {
    let receipt: Receipt =
        serde_json::from_str(include_str!("../../tests/fixtures/object_base_native.json")).unwrap();
    assert_eq!(receipt.cases.len(), 68);
    let mut snapshots = 0;
    for case in receipt.cases {
        let mut arena = ObjectArena::new();
        let mut live = BTreeMap::<usize, LiveReceiptObject>::new();
        for (step_index, step) in case.steps.into_iter().enumerate() {
            match step.operation {
                Operation::Construct { object, fill_byte } => {
                    assert!(live
                        .insert(
                            object,
                            LiveReceiptObject {
                                id: arena.allocate(ObjectClassKind::Base),
                                fill: u16::from(fill_byte) * 0x101,
                                position_written: false,
                            }
                        )
                        .is_none());
                }
                Operation::Context {
                    object,
                    position_words,
                    index_dword,
                } => {
                    let object = live.get_mut(&object).unwrap();
                    arena
                        .initialize_context(
                            object.id,
                            BaseContext {
                                position_words,
                                placement_index: index_dword,
                            },
                        )
                        .unwrap();
                    object.position_written |= position_words.is_some();
                }
                Operation::Flags {
                    object,
                    payload_dword,
                } => {
                    arena
                        .apply_payload_flags(live[&object].id, payload_dword)
                        .unwrap();
                }
                Operation::SetFlag0 {
                    object,
                    argument_dword,
                } => {
                    // Native bool stack arguments inspect only their low byte.
                    arena
                        .set_flag0(live[&object].id, argument_dword as u8 != 0)
                        .unwrap();
                }
                Operation::SetFlag1 {
                    object,
                    argument_dword,
                } => {
                    arena
                        .set_flag1(live[&object].id, argument_dword as u8 != 0)
                        .unwrap();
                }
                Operation::Dispose { object } => {
                    arena.dispose(live[&object].id).unwrap();
                }
                Operation::Destroy { object } => {
                    let id = live.remove(&object).unwrap().id;
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: id,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                    assert_eq!(arena.object(id), Err(ObjectError::UnknownObject(id)));
                }
                Operation::SeedFields {
                    object,
                    flags,
                    dirty_word,
                    word_3c,
                } => {
                    // Explicit fixture setup, never exposed as a production API.
                    let state = &mut arena.owned_mut(live[&object].id).unwrap().state.base;
                    if let Some(flags) = flags {
                        state.flags = flags;
                    }
                    if let Some(dirty_word) = dirty_word {
                        state.dirty_word = dirty_word;
                    }
                    if let Some(word_3c) = word_3c {
                        state.word_3c = word_3c;
                    }
                }
                Operation::SwapOwners { role } => match role.as_str() {
                    "flag0" => arena.swap_flag0_lists(),
                    "flag1" => arena.swap_flag1_lists(),
                    _ => panic!("unknown native owner {role}"),
                },
            }
            let label = format!("{} step {step_index}", case.id);
            assert_snapshot(&arena, &live, &step.state, &label);
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 361);
}

fn assert_snapshot(
    arena: &ObjectArena,
    live: &BTreeMap<usize, LiveReceiptObject>,
    snapshot: &Snapshot,
    label: &str,
) {
    assert_eq!(arena.len(), snapshot.objects.len(), "{label}");
    assert_eq!(arena.len(), live.len(), "{label}");
    for expected in &snapshot.objects {
        let object = &live[&expected.id];
        let position_words = if object.position_written {
            Some(expected.position_words)
        } else {
            // Native poisoned storage proves the constructor/null context did
            // not write a position. The port keeps that state explicitly unknown.
            assert_eq!(expected.position_words, [object.fill; 3], "{label}");
            None
        };
        assert_eq!(
            &arena.object(object.id).unwrap().base,
            &BaseObject {
                word_28: expected.scale_word,
                placement_word: expected.placement_word,
                flags: expected.flags,
                resource_word: expected.resource_word,
                position_words,
                dirty_word: expected.dirty_word,
                word_3c: expected.word_3c,
            },
            "{label} object {}",
            expected.id
        );
    }
    let native_nodes = |entries: &[[usize; 2]]| -> Vec<ObjectNode> {
        entries
            .iter()
            .map(|&[object, offset]| ObjectNode {
                object: live[&object].id,
                kind: match offset {
                    4 => ObjectNodeKind::Flag0,
                    16 => ObjectNodeKind::Flag1,
                    28 => ObjectNodeKind::Lifecycle,
                    _ => panic!("unexpected embedded node offset {offset}"),
                },
            })
            .collect()
    };
    let names = [
        "flag0",
        "flag0_secondary",
        "flag1",
        "flag1_secondary",
        "parking",
        "disposal",
    ];
    // Physical order checks also detect duplicate or missing embedded nodes.
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            arena.lists[index],
            native_nodes(&snapshot.lists[*name]),
            "{label} {name}"
        );
    }
    for (role, key, other) in [
        (
            ObjectList::Flag0Primary,
            "flag0",
            ObjectList::Flag0Secondary,
        ),
        (
            ObjectList::Flag1Primary,
            "flag1",
            ObjectList::Flag1Secondary,
        ),
    ] {
        let owner = &snapshot.owners[key];
        assert_eq!(
            arena.list(role),
            native_nodes(&snapshot.lists[owner]),
            "{label} current {key}"
        );
        let secondary = format!("{key}_secondary");
        let other_key = if owner == key {
            secondary.as_str()
        } else {
            key
        };
        assert_eq!(
            arena.list(other),
            native_nodes(&snapshot.lists[other_key]),
            "{label} paired {key}"
        );
    }
}

#[test]
fn removed_and_foreign_handles_cannot_mutate_reused_objects() {
    let mut arena = ObjectArena::new();
    let old = arena.allocate(ObjectClassKind::Base);
    arena.dispose(old).unwrap();
    let before = arena.object(old).unwrap().clone();
    assert_eq!(
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: old,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
            })
            .unwrap(),
        before
    );
    let current = arena.allocate(ObjectClassKind::Base);
    assert_eq!(old.slot, current.slot);
    assert_ne!(old, current);
    let foreign = ObjectArena::new().allocate(ObjectClassKind::Base);
    for bad in [old, foreign] {
        assert_eq!(
            arena.set_flag0(bad, false),
            Err(ObjectError::UnknownObject(bad))
        );
        assert_eq!(
            arena.set_flag1(bad, false),
            Err(ObjectError::UnknownObject(bad))
        );
        assert_eq!(arena.dispose(bad), Err(ObjectError::UnknownObject(bad)));
        assert_eq!(
            arena.destroy(crate::object_runtime::ObjectDestructionRequest {
                object: bad,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
            }),
            Err(ObjectError::UnknownObject(bad))
        );
        assert_eq!(
            arena.apply_payload_flags(bad, 0),
            Err(ObjectError::UnknownObject(bad))
        );
        assert_eq!(
            arena.initialize_context(
                bad,
                BaseContext {
                    position_words: Some([1, 2, 3]),
                    placement_index: 0,
                }
            ),
            Err(ObjectError::UnknownObject(bad))
        );
    }
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.object(current).unwrap().base.flags, 0x0800_0003);
    assert_eq!(arena.object(current).unwrap().base.position_words, None);
    assert_eq!(
        arena.list(ObjectList::Parking),
        &[ObjectNode {
            object: current,
            kind: ObjectNodeKind::Lifecycle
        }]
    );
}
