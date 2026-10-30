//! Selector34 supplied-state comparison with unchanged native routines.
//! The receipt covers construction through destruction, with +184=-1 and
//! null behavior allocations; the shared behavior owner has separate receipts.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, ByteSpan, InstanceCommandReadScope,
    InstanceCommandRequest, InstanceCommandTarget,
};
use serde_json::{json, Value};

use super::native_tests::{flat_terrain, selector_payload, word};
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        ActorCommandContext, ActorConstructionRequest, ActorHandle, ActorInitializationRequest,
        ActorRegistryConfig, ActorTerrainInputs, BaseContext, DerivedActorConstructionRequest,
        ObjectArena, ObjectId, RotatingVisualContext, RuntimeAllocation,
        Selector34InitializationRequest,
    },
};

#[derive(Default)]
struct RetainedSources {
    payload: Option<Arc<RuntimeAllocation>>,
    commands: BTreeMap<u32, Arc<RuntimeAllocation>>,
    unread: Option<Arc<RuntimeAllocation>>,
}

fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    rng: u32,
    sources: &RetainedSources,
) -> Value {
    let mut result = super::native_tests::snapshot(
        arena,
        objects,
        rng,
        &super::native_tests::RetainedSources::default(),
    );
    for value in result["objects"].as_array_mut().unwrap() {
        let state = arena
            .selector34_actor(objects[&word(&value["id"])])
            .unwrap();
        assert_eq!(state.words_244_248, [None; 2]);
        let retained = state.retained_23c.as_ref().map(|retained| {
            if sources
                .payload
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, retained.allocation()))
            {
                assert_eq!(retained.allocation_offset, 4 + 0x58);
                json!({"source":"payload","offset":0x58})
            } else if sources
                .unread
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, retained.allocation()))
            {
                assert_eq!(retained.allocation_offset, 0);
                json!({"source":"unmapped","offset":0})
            } else {
                let id = sources
                    .commands
                    .iter()
                    .find(|(_, owner)| Arc::ptr_eq(owner, retained.allocation()))
                    .unwrap()
                    .0;
                assert_eq!(retained.allocation_offset, 0);
                json!({"source":"command","id":id})
            }
        });
        value["selector34"] = json!({
            "word_230":state.word_230,"word_234":state.word_234,
            "byte_238":state.byte_238,"byte_239":state.byte_239,
            "word_240":state.word_240,
            // Native NULL trail pointer; construction never creates a ribbon.
            "word_25c":state.trail_25c.map_or(json!(0), |_| json!("trail object")),
            "table_24c":state.table_24c,"table_250":state.table_250,
            "table_254":state.table_254,"table_258":state.table_258,
            "retained_23c":retained,
        });
    }
    result
}

#[test]
fn selector34_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector34_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 12);
    assert_eq!(fixture["coverage"]["snapshots"], 228);
    for address in ["00433B20", "00433C70", "00433B90"] {
        assert_eq!(fixture["coverage"]["direct_native_entries"][address], 12);
    }
    assert_eq!(fixture["coverage"]["direct_native_entries"]["00434FA0"], 16);
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
        let mut objects = BTreeMap::new();
        let mut sources = RetainedSources::default();
        let mut terrain = None;
        let mut terrain_unavailable = false;
        let mut rng = 0;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = step["object"].as_u64().unwrap_or(1) as u32;
            match step["op"].as_str().unwrap() {
                "construct" => {
                    rng = word(&step["rng_seed"]);
                    let object = arena
                        .allocate_selector34_actor(
                            DerivedActorConstructionRequest {
                                actor: ActorConstructionRequest {
                                    constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                                    allocation_position: serde_json::from_value(
                                        step["allocation_position"].clone(),
                                    )
                                    .unwrap(),
                                    owner: word(&step["owner"]) as u8,
                                    terrain: ActorTerrainInputs {
                                        terrain: None,
                                        coordinate_shift: 8,
                                        clock: 0,
                                        world_flags: Some(0),
                                        sine_quarter: &sine,
                                    },
                                },
                                world_mask_limits: serde_json::from_value(step["limits"].clone())
                                    .unwrap(),
                            },
                            &mut rng,
                        )
                        .unwrap();
                    objects.insert(id, object);
                }
                "initialize" => {
                    let specialized: Vec<u8> =
                        serde_json::from_value(step["specialized"]["bytes_58_69"].clone()).unwrap();
                    let (payload, allocation) = selector_payload(step, 34, &specialized);
                    arena
                        .initialize_selector34_actor(
                            objects[&id],
                            Selector34InitializationRequest {
                                actor: ActorInitializationRequest {
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
                                allocation: &allocation,
                                payload_offset: 4,
                            },
                            &resident.runtime,
                            &math,
                        )
                        .unwrap();
                    sources.payload = Some(allocation);
                }
                "guard" => assert_eq!(
                    arena.admit_family428650_actor(objects[&id]).unwrap(),
                    step["accepted"].as_bool().unwrap()
                ),
                "terrain" => {
                    terrain = Some(flat_terrain(step["raw_height"].as_i64().unwrap() as i8))
                }
                "command" | "point_command" => {
                    let code = step["code"].as_i64().unwrap() as i32;
                    if code == 23 {
                        // An owned one-past pointer is retained without reading, matching
                        // native storage of an unmapped address until postinitialization.
                        let owner = Arc::new(
                            RuntimeAllocation::new(Arc::from([]), ByteSpan::new(0, 0)).unwrap(),
                        );
                        arena
                            .set_selector34_retained(objects[&id], &owner, 0)
                            .unwrap();
                        sources.unread = Some(owner);
                        assert!(step["accepted"].as_bool().unwrap());
                    } else {
                        let mut bytes = code.to_le_bytes().to_vec();
                        if let Some(words) = step["words"].as_array() {
                            for value in words {
                                bytes.extend_from_slice(&(word(value) as u16).to_le_bytes());
                            }
                        } else {
                            for value in step["payload_dwords"].as_array().unwrap() {
                                bytes.extend_from_slice(&word(value).to_le_bytes());
                            }
                        }
                        let command = decode_instance_command(InstanceCommandRequest {
                            read_scope: InstanceCommandReadScope::Dispatch,
                            source: &bytes,
                            main_payload: ByteSpan::new(0, bytes.len()),
                            relocations: &[],
                            command_offset: 0,
                            target: InstanceCommandTarget::Constructor(34),
                        })
                        .unwrap();
                        let accepted = arena
                            .apply_selector34_command(
                                objects[&id],
                                &command,
                                ActorCommandContext {
                                    runtime: &resident.runtime,
                                    first_player_grants: None,
                                    terrain: terrain.as_ref(),
                                },
                            )
                            .unwrap();
                        assert_eq!(accepted, step["accepted"].as_bool().unwrap());
                    }
                }
                "retained_command" => {
                    let bytes: Vec<u8> = serde_json::from_value(step["bytes"].clone()).unwrap();
                    let size = bytes.len();
                    let allocation = Arc::new(
                        RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, size)).unwrap(),
                    );
                    arena
                        .set_selector34_retained(objects[&id], &allocation, 0)
                        .unwrap();
                    sources.commands.insert(word(&step["id"]), allocation);
                    assert!(step["accepted"].as_bool().unwrap());
                }
                "after_load_wrapper" => arena
                    .initialize_after_load(objects[&id], Default::default())
                    .unwrap(),
                "network" => {
                    crate::object_runtime::construction::actor_factory::initialize_network_actor(
                        &mut arena,
                        objects[&id],
                        step["world_active"].as_bool().unwrap(),
                    )
                    .unwrap()
                }
                "postinit_inputs" => {
                    let object = objects[&id];
                    let base = &mut arena.owned_mut(object).unwrap().state.base;
                    base.flags = word(&step["flags"]);
                    base.position_words =
                        serde_json::from_value(step["position_words"].clone()).unwrap();
                    arena.actor_mut(object).unwrap().rotating.visual.radius_word =
                        word(&step["radius_word"]) as u16;
                    terrain_unavailable = step["terrain_table_unmapped"].as_bool().unwrap();
                }
                "post_initialize" => arena
                    .finish_selector34_initialization(
                        objects[&id],
                        if terrain_unavailable {
                            None
                        } else {
                            terrain.as_ref()
                        },
                    )
                    .unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected selector34 operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects, rng, &sources),
                step["state"],
                "{} step {index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, word(&fixture["coverage"]["snapshots"]) as usize);
}
