//! Complete original shared-derived routines on supplied storage and residency.
//! Allocating guard branches and resource-owning destructor paths are excluded.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, parse_constructor_asset_bank,
    ActorConstructorPayload, ByteSpan, ConstructorPayloadView, HandlerFraming, HandlerId,
    HandlerRegion, InstanceCommandRequest, InstanceCommandTarget, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::native_tests,
        native_model_fixture::{Fixture, Models},
        ActorCommandContext, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
        RotatingVisualContext,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

pub(in crate::object_runtime::actor) fn payload(
    value: &Value,
    extension: &Value,
) -> Arc<ActorConstructorPayload> {
    let mut bytes = [0xd3; 0x58];
    for (offset, name) in [(0, "flags"), (0x0c, "selector_0c"), (0x3c, "word_3c")] {
        bytes[offset..offset + 4].copy_from_slice(&word(&value[name]).to_le_bytes());
    }
    for (offset, name) in [
        (4, "health"),
        (8, "word_08"),
        (0x38, "word_38"),
        (0x40, "word_40"),
    ] {
        bytes[offset..offset + 2].copy_from_slice(&(word(&value[name]) as u16).to_le_bytes());
    }
    for (index, pair) in value["damage_responses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        for channel in 0..2 {
            let offset = 0x10 + index * 4 + channel * 2;
            bytes[offset..offset + 2].copy_from_slice(&(word(&pair[channel]) as u16).to_le_bytes());
        }
    }
    bytes[0x44] = word(&extension["byte_44"]) as u8;
    bytes[0x48..0x4a].copy_from_slice(&(word(&extension["word_48"]) as u16).to_le_bytes());
    bytes[0x4a..0x4c]
        .copy_from_slice(&(extension["signed_word_4a"].as_i64().unwrap() as i16).to_le_bytes());
    bytes[0x4c..0x50].copy_from_slice(&word(&extension["word_4c"]).to_le_bytes());
    for (index, value) in extension["sound_words"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let offset = 0x50 + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&(word(value) as u16).to_le_bytes());
    }
    // Selector60 exposes exactly the proven 00426BF0 / 00426C10 payload.
    // This test constructs the intermediate class, not selector60's extension.
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&59_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    source.extend_from_slice(&bytes);
    let main = ByteSpan::new(9, source.len());
    let empty = ByteSpan::new(source.len(), source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(9, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: empty,
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("selector60 must expose its proven shared-derived schema")
    };
    assert!(payload.derived.is_some());
    Arc::new(payload.clone())
}

pub(in crate::object_runtime::actor) fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    rng: u32,
) -> Value {
    let state = snapshot_prefix(arena, objects, rng);
    for value in state["objects"].as_array().unwrap() {
        assert!(arena
            .derived_actor(objects[&word(&value["id"])])
            .unwrap()
            .behavior_140
            .is_none());
    }
    state
}

pub(in crate::object_runtime::actor) fn snapshot_prefix(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    rng: u32,
) -> Value {
    let mut state = native_tests::snapshot(arena, objects);
    for value in state["objects"].as_array_mut().unwrap() {
        let derived = arena.derived_actor(objects[&word(&value["id"])]).unwrap();
        value["derived"] = json!({
            "byte_13c":derived.byte_13c,"byte_13d":derived.byte_13d,
            "word_140":u32::from(derived.behavior_140.is_some()),"word_144":derived.word_144 as u32,"word_148":0,"word_154":0,
            "byte_160":derived.byte_160,"word_164":derived.word_164 as u32,"word_168":0,
            "word_174":derived.word_174,"word_178":derived.word_178,"word_17a":derived.word_17a,
            "word_17c":derived.sound_words_17c_182.map(|words|words[0]),
            "word_17e":derived.sound_words_17c_182.map(|words|words[1]),
            "word_180":derived.sound_words_17c_182.map(|words|words[2]),
            "word_182":derived.sound_words_17c_182.map(|words|words[3]),
        });
    }
    state["rng_state"] = json!(rng);
    state
}

#[test]
fn shared_derived_actor_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/derived_actor_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 20);
    assert_eq!(fixture["coverage"]["snapshots"], 260);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["00425E60"], 20);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["00425F70"], 20);
    assert_eq!(
        fixture["coverage"]["direct_native_entries"]["00426030"],
        160
    );
    assert_eq!(fixture["coverage"]["direct_native_entries"]["00426C80"], 20);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let resident = Fixture::new(&serde_json::from_value::<Models>(case["h8"].clone()).unwrap());
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let terrain = ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &sine,
        };
        let mut objects = BTreeMap::new();
        let mut rng = 0;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = word(&step["object"]);
            match step["op"].as_str().unwrap() {
                "construct" => {
                    rng = word(&step["rng_seed"]);
                    let object = arena
                        .allocate_derived_actor(
                            DerivedActorConstructionRequest {
                                actor: ActorConstructionRequest {
                                    constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                                    allocation_position: serde_json::from_value(
                                        step["allocation_position"].clone(),
                                    )
                                    .unwrap(),
                                    owner: word(&step["owner"]) as u8,
                                    terrain,
                                },
                                world_mask_limits: serde_json::from_value(step["limits"].clone())
                                    .unwrap(),
                            },
                            &mut rng,
                        )
                        .unwrap();
                    assert!(objects.insert(id, object).is_none());
                }
                "initialize" => {
                    let payload = payload(&step["payload"], &step["extension"]);
                    arena
                        .initialize_derived_actor(
                            objects[&id],
                            ActorInitializationRequest {
                                context: RotatingVisualContext {
                                    base: BaseContext {
                                        position_words: serde_json::from_value(
                                            step["position_words"].clone(),
                                        )
                                        .unwrap(),
                                        placement_index: word(&step["placement_index"]),
                                    },
                                    model_argument: Some(word(&step["model_argument"])),
                                    rotation_words: serde_json::from_value(
                                        step["rotation_words"].clone(),
                                    )
                                    .unwrap(),
                                },
                                payload: &payload,
                                local_owner: 1,
                            },
                            &resident.runtime,
                            &math,
                        )
                        .unwrap();
                }
                "guard" => assert_eq!(
                    arena.admit_derived_actor(objects[&id]).unwrap(),
                    step["accepted"].as_bool().unwrap()
                ),
                "command" => {
                    let code = step["code"].as_i64().unwrap() as i32;
                    let mut bytes = code.to_le_bytes().to_vec();
                    for value in step["payload_dwords"].as_array().unwrap() {
                        bytes.extend_from_slice(&word(value).to_le_bytes());
                    }
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &bytes,
                        main_payload: ByteSpan::new(0, bytes.len()),
                        relocations: &[],
                        command_offset: 0,
                        target: InstanceCommandTarget::DerivedActor,
                    })
                    .unwrap();
                    assert_eq!(
                        arena
                            .apply_derived_actor_command(
                                objects[&id],
                                &view,
                                ActorCommandContext {
                                    terrain: None,
                                    runtime: &resident.runtime,
                                    first_player_grants: None,
                                },
                            )
                            .unwrap(),
                        step["accepted"].as_bool().unwrap()
                    );
                }
                "post_initialize" => arena.finish_actor_initialization(objects[&id]).unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected derived actor operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects, rng),
                step["state"],
                "{} step{index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 260);
}
