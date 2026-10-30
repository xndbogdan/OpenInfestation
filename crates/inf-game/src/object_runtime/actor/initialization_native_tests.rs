//! Complete shared actor initialization/command receipts. Native allocation
//! bytes, resident model headers, payloads and first-player masks are explicit
//! synthetic inputs. Derived classes and factory completion are not exercised.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, parse_constructor_asset_bank,
    ActorConstructorPayload, ByteSpan, ConstructorPayloadView, GrantClosureError, HandlerFraming,
    HandlerId, HandlerRegion, InstanceCommand, InstanceCommandRequest, InstanceCommandTarget,
    InstanceField, PlacementInstanceAssets, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        BaseContext, ObjectInstanceCommands, RotatingVisualContext,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn payload(value: &Value) -> Arc<ActorConstructorPayload> {
    let mut payload = [0xd3; 0x42];
    for (offset, name) in [(0, "flags"), (0x0c, "selector_0c"), (0x3c, "word_3c")] {
        payload[offset..offset + 4].copy_from_slice(&word(&value[name]).to_le_bytes());
    }
    for (offset, name) in [
        (4, "health"),
        (8, "word_08"),
        (0x38, "word_38"),
        (0x40, "word_40"),
    ] {
        payload[offset..offset + 2].copy_from_slice(&(word(&value[name]) as u16).to_le_bytes());
    }
    for (index, pair) in value["damage_responses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        for channel in 0..2 {
            let offset = 0x10 + index * 4 + channel * 2;
            payload[offset..offset + 2]
                .copy_from_slice(&(word(&pair[channel]) as u16).to_le_bytes());
        }
    }
    // Selector 56's known 0047AD70 schema supplies a canonical shared prefix.
    // This fixture does not construct/admit that derived class through H13.
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&55_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    source.extend_from_slice(&payload);
    let main = ByteSpan::new(9, source.len());
    let empty = ByteSpan::new(source.len(), source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let region = HandlerRegion {
        handler: HandlerId::new(13).unwrap(),
        requested_count: 1,
        span: ByteSpan::new(9, source.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload: main,
            relocation_count: 0,
            relocation_offsets: empty,
            entry_offsets: ByteSpan::new(main.end, source.len()),
        }),
    };
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("selector 56 must expose its proven shared actor payload");
    };
    Arc::new(payload.clone())
}

fn command_bytes(command: &Value) -> Vec<u8> {
    let mut bytes = word(&command["code_dword"]).to_le_bytes().to_vec();
    if let Some(words) = command["payload_words"].as_array() {
        for value in words {
            bytes.extend_from_slice(&word(value).to_le_bytes());
        }
    }
    bytes
}

struct Commands {
    source: Vec<u8>,
    placement: PlacementInstanceAssets,
}

impl Commands {
    fn new(count: u32, input: &[Value]) -> Self {
        let mut source = vec![];
        let commands = input
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let offset = source.len();
                source.extend_from_slice(&command_bytes(command));
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(0, 0),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: word(&command["code_dword"]) as i32,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        Self {
            source,
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: count as i32,
                },
                command_pointer_span: ByteSpan::new(0, 0),
                commands,
            },
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

fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    payloads: &[Arc<ActorConstructorPayload>],
) -> Value {
    let mut state = super::native_tests::snapshot(arena, objects);
    for value in state["objects"].as_array_mut().unwrap() {
        let actor = arena.actor(objects[&word(&value["id"])]).unwrap();
        value["damage_source"] = match &actor.damage_table {
            ActorDamageTable::ConstructorDefault => json!({"kind":"default"}),
            ActorDamageTable::Payload(backing) => json!({"kind":"payload", "payload_id":
                payloads.iter().position(|source| Arc::ptr_eq(source, backing)).unwrap()}),
        };
    }
    state
}

#[test]
fn shared_actor_initialization_and_commands_match_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_initialization_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 48);
    assert_eq!(fixture["coverage"]["snapshots"], 216);
    assert_eq!(fixture["coverage"]["direct_native_entry_count"], 233);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let mut compared = 0;
    let mut bounded_queries = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let models: Models = serde_json::from_value(case["h8"].clone()).unwrap();
        let resident = Fixture::new(&models);
        let payloads: Vec<_> = case["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .map(payload)
            .collect();
        assert_eq!(case["terrain"]["present"], false);
        let terrain = ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &sine,
        };
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let mut objects = BTreeMap::new();
        let mut local_owner = 1;
        let mut grants: Option<[u32; 6]> = None;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = word(&step["object"]);
            let command_context = ActorCommandContext {
                terrain: None,
                runtime: &resident.runtime,
                first_player_grants: grants.as_ref(),
            };
            match step["op"].as_str().unwrap() {
                "construct" => {
                    local_owner = word(&step["owner"]) as u8;
                    let object = arena
                        .allocate_actor(ActorConstructionRequest {
                            constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                            allocation_position: serde_json::from_value(
                                step["allocation_position"].clone(),
                            )
                            .unwrap(),
                            owner: local_owner,
                            terrain,
                        })
                        .unwrap();
                    assert!(objects.insert(id, object).is_none());
                }
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                "process_owner" => local_owner = word(&step["owner"]) as u8,
                "seed" => {
                    let base = &mut arena.owned_mut(objects[&id]).unwrap().state.base;
                    if let Some(value) = step.get("dirty_word") {
                        base.dirty_word = word(value) as u16;
                    }
                    if let Some(value) = step.get("word_28") {
                        base.word_28 = word(value) as u16;
                    }
                }
                "payload" => arena
                    .initialize_actor_payload(
                        objects[&id],
                        &payloads[word(&step["payload_id"]) as usize],
                        local_owner,
                    )
                    .unwrap(),
                "wrapper" => {
                    let context = &step["context"];
                    arena
                        .initialize_actor(
                            objects[&id],
                            ActorInitializationRequest {
                                context: RotatingVisualContext {
                                    base: BaseContext {
                                        position_words: serde_json::from_value(
                                            context["position_words"].clone(),
                                        )
                                        .unwrap(),
                                        placement_index: word(&context["index_dword"]),
                                    },
                                    model_argument: serde_json::from_value(
                                        context["model_argument"].clone(),
                                    )
                                    .unwrap(),
                                    rotation_words: serde_json::from_value(
                                        context["rotation_words"].clone(),
                                    )
                                    .unwrap(),
                                },
                                payload: &payloads[word(&step["payload_id"]) as usize],
                                local_owner,
                            },
                            &resident.runtime,
                            &math,
                        )
                        .unwrap();
                }
                "postinitialize" => arena.finish_actor_initialization(objects[&id]).unwrap(),
                "world" => {
                    grants = step["set_bits"].as_array().map(|bits| {
                        let mut mask = [0_u32; 6];
                        for bit in bits {
                            let bit = word(bit);
                            if bit < 192 {
                                mask[(bit >> 5) as usize] |= 1 << (bit & 31);
                            }
                        }
                        mask
                    })
                }
                "command" => {
                    let source = command_bytes(step);
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &source,
                        main_payload: ByteSpan::new(0, source.len()),
                        relocations: &[],
                        command_offset: 0,
                        target: InstanceCommandTarget::SharedActor,
                    })
                    .unwrap();
                    let before = snapshot(&arena, &objects, &payloads);
                    let result = arena.apply_actor_command(objects[&id], &view, command_context);
                    if matches!(view.code.value, 13 | 14)
                        && word(&step["payload_words"][0]) as u16 == u16::MAX
                    {
                        // Native synthetic storage extends beyond a player's six
                        // DWORDs. Its completed query is outside the portable
                        // domain; compare explicit host error/unchanged state,
                        // not the native return. Earlier disposal makes the
                        // resulting state equal in these four particular cases.
                        assert_eq!(
                            result,
                            Err(ActorCommandError::Grant(GrantClosureError::Id(65535)))
                        );
                        assert_eq!(snapshot(&arena, &objects, &payloads), before);
                        bounded_queries += 1;
                    } else {
                        assert_eq!(
                            result.unwrap(),
                            step["result"]["accepted"].as_bool().unwrap()
                        );
                    }
                }
                "commands" => {
                    let commands = step["commands"]
                        .as_array()
                        .map(|input| Commands::new(word(&step["count_dword"]), input));
                    let rejection = arena
                        .run_actor_commands(
                            objects[&id],
                            word(&step["resource_dword"]),
                            commands.as_ref().map(Commands::request),
                            command_context,
                        )
                        .unwrap();
                    assert_eq!(
                        rejection.is_none(),
                        step["result"]["accepted"].as_bool().unwrap()
                    );
                    if let Some(rejection) = rejection {
                        assert!(step["unreachable_poison_tail"].as_bool().unwrap());
                        assert_eq!((rejection.index, rejection.code), (1, 16));
                    }
                }
                operation => panic!("unexpected actor operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects, &payloads),
                step["state"],
                "{} step{index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 216);
    assert_eq!(bounded_queries, 4);
}
