//! Normalized receipts from unmodified route helpers. Group allocation/free
//! and invalid-node deletion are outside the native fixture's executed scope.

use std::collections::BTreeMap;

use inf_formats::{ByteSpan, InstanceCommand, InstanceField, PlacementInstanceAssets};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    construction::{run_commands, CommandPolicy, ObjectInstanceCommands},
    native_model_fixture::{Fixture, Models},
    ObjectClassKind, ObjectList, ObjectNodeKind,
};

fn word(value: &Value, key: &str) -> u32 {
    value[key].as_u64().unwrap() as u32
}

struct Commands {
    placement: PlacementInstanceAssets,
    source: Vec<u8>,
}

impl Commands {
    fn new(step: &Value, direct: bool) -> Self {
        let inputs = if direct {
            vec![step.clone()]
        } else {
            step["commands"].as_array().cloned().unwrap_or_default()
        };
        let mut source = vec![];
        let commands: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let offset = source.len();
                let code = word(input, "code");
                source.extend_from_slice(&code.to_le_bytes());
                // Ignore-payload commands must decode with only their code bytes;
                // this corresponds to the native fixture's unmapped pointer.
                if !input["unmapped_payload"].as_bool().unwrap_or(false) {
                    source.extend_from_slice(
                        &(input["payload_dword"].as_u64().unwrap_or(0) as u32).to_le_bytes(),
                    );
                }
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(0, 0),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: code as i32,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        let count = step["count"].as_i64().unwrap_or(commands.len() as i64) as i32;
        Self {
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: count,
                },
                command_pointer_span: ByteSpan::new(0, 0),
                commands,
            },
            source,
        }
    }

    fn request(&self) -> ObjectInstanceCommands<'_> {
        ObjectInstanceCommands {
            placement: &self.placement,
            source: &self.source,
            main_payload: ByteSpan::new(0, self.source.len()),
            relocations: &[],
        }
    }
}

struct Comparison {
    arena: ObjectArena,
    objects: BTreeMap<u32, ObjectId>,
    groups: BTreeMap<u32, RouteGroupId>,
}

impl Comparison {
    fn object_key(&self, object: ObjectId) -> u32 {
        *self
            .objects
            .iter()
            .find(|(_, id)| **id == object)
            .unwrap()
            .0
    }

    fn group_key(&self, group: RouteGroupId) -> u32 {
        *self.groups.iter().find(|(_, id)| **id == group).unwrap().0
    }

    fn snapshot(&self) -> Value {
        let objects: Vec<_> = self
            .objects
            .iter()
            .map(|(&id, &object)| {
                let state = self.arena.object(object).unwrap();
                let ObjectClass::RouteNode { visual, route } = &state.class else {
                    panic!("route class")
                };
                let base = &state.base;
                json!({
                    "id":id, "word_28":base.word_28, "placement_word":base.placement_word,
                    "flags":base.flags, "resource_word":base.resource_word,
                    "dirty_word":base.dirty_word, "word_3c":base.word_3c,
                    "word_40":visual.radius_word, "model_word":visual.model_word,
                    "position_words":base.position_words, "distance_squared":route.distance_squared,
                    "route_flags":route.flags, "group":route.group.map(|id| self.group_key(id)),
                    "identity_word":route.identity_word, "predecessor_word":route.predecessor_word,
                })
            })
            .collect();
        let mut common_lists = serde_json::Map::new();
        for (name, role) in [
            ("flag0", ObjectList::Flag0Primary),
            ("flag1", ObjectList::Flag1Primary),
            ("flag0_secondary", ObjectList::Flag0Secondary),
            ("flag1_secondary", ObjectList::Flag1Secondary),
            ("parking", ObjectList::Parking),
            ("disposal", ObjectList::Disposal),
        ] {
            let nodes: Vec<_> = self
                .arena
                .list(role)
                .iter()
                .map(|node| {
                    [
                        self.object_key(node.object),
                        match node.kind {
                            ObjectNodeKind::Flag0 => 4,
                            ObjectNodeKind::Flag1 => 16,
                            ObjectNodeKind::Lifecycle => 28,
                        },
                    ]
                })
                .collect();
            common_lists.insert(name.to_owned(), json!(nodes));
        }
        let groups: Vec<_> = self.groups.iter().map(|(&key, &id)| {
            let group = self.arena.route_group(id).unwrap();
            json!({"id":key,"flags":group.flags,
                "members":group.members.iter().map(|&object| self.object_key(object)).collect::<Vec<_>>()})
        }).collect();
        json!({
            "objects":objects,"common_lists":common_lists,
            "groups":groups,
            "pending":self.arena.route_pending().iter().map(|&id| self.object_key(id)).collect::<Vec<_>>(),
            "group_order":self.arena.route_groups().iter().map(|group| self.group_key(group.id)).collect::<Vec<_>>(),
        })
    }

    fn step(&mut self, step: &Value, runtime: &ResidentRuntime) {
        let op = step["op"].as_str().unwrap();
        let key = step["object"].as_u64().map(|id| id as u32);
        let object = key.and_then(|id| self.objects.get(&id).copied());
        match op {
            "construct" => {
                let id = self.arena.allocate(ObjectClassKind::RouteNode);
                assert!(self.objects.insert(key.unwrap(), id).is_none());
            }
            "context" | "initialize" => {
                let position_words =
                    serde_json::from_value(step["position_words"].clone()).unwrap();
                self.arena
                    .initialize_route_context(
                        object.unwrap(),
                        BaseContext {
                            position_words,
                            placement_index: word(step, "index_dword"),
                        },
                        step["model_argument"].as_u64().map(|word| word as u32),
                        runtime,
                    )
                    .unwrap();
                if op == "initialize" {
                    self.arena
                        .apply_payload_flags(object.unwrap(), word(step, "payload_flags"))
                        .unwrap();
                }
            }
            "seed" => {
                let object = object.unwrap();
                if let Some(value) = step["dirty_word"].as_u64() {
                    self.arena.owned_mut(object).unwrap().state.base.dirty_word = value as u16;
                }
                let group = step
                    .get("group")
                    .map(|value| value.as_u64().map(|id| self.groups[&(id as u32)]));
                let route = self.arena.route_mut(object).unwrap();
                if let Some(value) = step["route_flags"].as_u64() {
                    route.flags = value as u8;
                }
                if let Some(value) = step["distance_squared"].as_u64() {
                    route.distance_squared = value as u32;
                }
                if let Some(group) = group {
                    route.group = group;
                }
            }
            "command" | "admit" => {
                let object = object.unwrap();
                let direct = op == "command";
                let commands = Commands::new(step, direct);
                let resource = if direct {
                    u32::from(self.arena.object(object).unwrap().base.resource_word)
                } else {
                    word(step, "resource_argument")
                };
                let suffix =
                    (!step["null_suffix"].as_bool().unwrap_or(false)).then(|| commands.request());
                let accepted = run_commands(
                    &mut self.arena,
                    runtime,
                    object,
                    CommandPolicy::Constructor(51),
                    resource,
                    suffix,
                )
                .unwrap()
                .is_none();
                assert_eq!(accepted, step["accepted"].as_bool().unwrap());
            }
            "destroy" => {
                self.arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: object.unwrap(),
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
                self.objects.remove(&key.unwrap());
            }
            "group_construct" => {
                let group = self.arena.allocate_route_group();
                assert!(self.groups.insert(word(step, "group"), group).is_none());
            }
            "group_destroy" => {
                let id = self.groups[&word(step, "group")];
                assert!(self.arena.route_group(id).unwrap().members.is_empty());
                // Native fixture directly calls empty-group 00404FD0. The
                // portable owner reaches the same empty-group unlink through
                // destruction of a temporary bound node; no deletion policy is
                // copied into this test adapter.
                let temporary = self.arena.allocate(ObjectClassKind::RouteNode);
                self.arena.route_mut(temporary).unwrap().group = Some(id);
                self.arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: temporary,
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
                self.groups.remove(&word(step, "group"));
            }
            "append" => {
                let accepted = if let Some(object) = object {
                    assert_eq!(
                        self.arena.route_node(object).unwrap().group,
                        Some(self.groups[&word(step, "group")])
                    );
                    self.arena.append_route_node(object).unwrap()
                } else {
                    // Native null-pointer no-op has no ObjectId representation.
                    // Its unchanged snapshot is retained as native ABI evidence.
                    false
                };
                assert_eq!(accepted, step["accepted"].as_bool().unwrap());
            }
            "resolve" => {
                let group = self.arena.resolve_route_group(object.unwrap()).unwrap();
                assert_eq!(json!(group.map(|id| self.group_key(id))), step["value"]);
            }
            "finalize" => self.arena.finalize_routes().unwrap(),
            "next" | "previous" | "next_linear" | "find" | "group_first" | "group_last" => {
                let value = match op {
                    "next" => self.arena.route_next(object.unwrap()).unwrap(),
                    "previous" => self.arena.route_previous(object.unwrap()).unwrap(),
                    "find" => self.arena.find_route_node(word(step, "one_based")),
                    "group_first" | "group_last" => {
                        let members = &self
                            .arena
                            .route_group(self.groups[&word(step, "group")])
                            .unwrap()
                            .members;
                        if op == "group_first" {
                            members.first().copied()
                        } else {
                            members.last().copied()
                        }
                    }
                    "next_linear" => {
                        let object = object.unwrap();
                        let group = self.arena.route_node(object).unwrap().group.unwrap();
                        let members = &self.arena.route_group(group).unwrap().members;
                        let index = members.iter().position(|&id| id == object).unwrap();
                        members.get(index + 1).copied()
                    }
                    _ => unreachable!(),
                };
                assert_eq!(json!(value.map(|id| self.object_key(id))), step["value"]);
            }
            "first_group" | "next_group" => {
                let groups = self.arena.route_groups();
                let index = if op == "first_group" {
                    0
                } else {
                    let group = self.groups[&word(step, "group")];
                    groups
                        .iter()
                        .position(|candidate| candidate.id == group)
                        .unwrap()
                        + 1
                };
                assert_eq!(
                    json!(groups.get(index).map(|group| self.group_key(group.id))),
                    step["value"]
                );
            }
            other => panic!("unknown native step {other}"),
        }
    }
}

#[test]
fn route_lifetime_commands_groups_and_finalizer_match_native_snapshots() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/object_route_native.json"
    ))
    .unwrap();
    let models: Models = serde_json::from_value(fixture["resident_models"].clone()).unwrap();
    let resident = Fixture::new(&models);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    let mut snapshots = 0;
    for case in cases {
        let mut comparison = Comparison {
            arena: ObjectArena::new(),
            objects: BTreeMap::new(),
            groups: BTreeMap::new(),
        };
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            comparison.step(step, &resident.runtime);
            assert_eq!(
                comparison.snapshot(),
                step["state"],
                "{} step {index}: {}",
                case["id"],
                step["op"]
            );
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 308);
}
