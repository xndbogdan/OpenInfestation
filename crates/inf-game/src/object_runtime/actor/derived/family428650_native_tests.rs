//! Complete common-family and selector35 calls with null owned resources.
//! These supplied-input receipts do not establish enclosing factory execution.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, parse_constructor_asset_bank,
    parse_terrain_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView, HandlerFraming,
    HandlerId, HandlerRegion, InstanceCommandRequest, InstanceCommandTarget, RelocationTail,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::derived::{native_tests as common, selector35::Selector35InitializationRequest},
        native_model_fixture::{Fixture, Models},
        ActorConstructionRequest, ActorInitializationRequest, ActorRegistryConfig,
        ActorTerrainInputs, BaseContext, RotatingVisualContext, RuntimeAllocation,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};

pub(super) fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

pub(super) fn flat_terrain(height: i8) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = height as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn ship_payload(step: &Value) -> (Arc<ActorConstructorPayload>, Arc<RuntimeAllocation>) {
    let specialized: Vec<u8> =
        serde_json::from_value(step["specialized"]["bytes_58_73"].clone()).unwrap();
    selector_payload(step, 35, &specialized)
}

pub(super) fn selector_payload(
    step: &Value,
    selector: u16,
    specialized: &[u8],
) -> (Arc<ActorConstructorPayload>, Arc<RuntimeAllocation>) {
    let value = &step["payload"];
    let extension = &step["extension"];
    let mut bytes = vec![0xd3; 4 + 0x58 + specialized.len()];
    bytes[..2].copy_from_slice(&selector.wrapping_sub(1).to_le_bytes());
    bytes[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
    for (offset, name) in [(0, "flags"), (0x0c, "selector_0c"), (0x3c, "word_3c")] {
        bytes[4 + offset..8 + offset].copy_from_slice(&word(&value[name]).to_le_bytes());
    }
    for (offset, name) in [
        (4, "health"),
        (8, "word_08"),
        (0x38, "word_38"),
        (0x40, "word_40"),
    ] {
        bytes[4 + offset..6 + offset].copy_from_slice(&(word(&value[name]) as u16).to_le_bytes());
    }
    for (index, pair) in value["damage_responses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        for channel in 0..2 {
            let offset = 4 + 0x10 + index * 4 + channel * 2;
            bytes[offset..offset + 2].copy_from_slice(&(word(&pair[channel]) as u16).to_le_bytes());
        }
    }
    bytes[4 + 0x44] = word(&extension["byte_44"]) as u8;
    bytes[4 + 0x48..4 + 0x4a].copy_from_slice(&(word(&extension["word_48"]) as u16).to_le_bytes());
    bytes[4 + 0x4a..4 + 0x4c]
        .copy_from_slice(&(extension["signed_word_4a"].as_i64().unwrap() as i16).to_le_bytes());
    bytes[4 + 0x4c..4 + 0x50].copy_from_slice(&word(&extension["word_4c"]).to_le_bytes());
    for (index, value) in extension["sound_words"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let offset = 4 + 0x50 + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&(word(value) as u16).to_le_bytes());
    }
    bytes[4 + 0x58..].copy_from_slice(specialized);
    let main = ByteSpan::new(0, bytes.len());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let source: Arc<[u8]> = bytes.into();
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(0, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: ByteSpan::new(main.end, main.end),
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    assert_eq!(bank.prefixes.entries[0].constructor_selector, selector);
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("selector {selector} payload")
    };
    (
        Arc::new(payload.clone()),
        Arc::new(RuntimeAllocation::new(source, main).unwrap()),
    )
}

#[derive(Default)]
pub(super) struct RetainedSources {
    payload: Option<Arc<RuntimeAllocation>>,
    commands: BTreeMap<u32, Arc<RuntimeAllocation>>,
    unread: Option<Arc<RuntimeAllocation>>,
}

pub(super) fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    rng: u32,
    sources: &RetainedSources,
) -> Value {
    snapshot_from_prefix(
        common::snapshot(arena, objects, rng),
        arena,
        objects,
        sources,
    )
}

pub(super) fn snapshot_from_prefix(
    mut result: Value,
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    sources: &RetainedSources,
) -> Value {
    for value in result["objects"].as_array_mut().unwrap() {
        let object = objects[&word(&value["id"])];
        let state = arena.family428650_actor(object).unwrap();
        assert_eq!(state.words_188_19c, [None; 6]);
        assert_eq!(state.words_204_208, [None; 2]);
        assert_eq!((state.word_220, state.byte_22d), (None, None));
        let entries: Vec<_> = state
            .entries_1a4
            .iter()
            .map(|entry| [entry.handle.raw(), entry.word_04, entry.word_08])
            .collect();
        let pairs: Vec<_> = state
            .words_1d8_200
            .iter()
            .map(|&value| [0, value])
            .collect();
        value["common_428650"] = json!({
            "word_184":state.word_184 as u32,"word_1a0":state.count_1a0,"entries_1a4_1d0":entries,
            "pairs_1d4_200":pairs,"word_20c":state.word_20c,"word_210":state.word_210,
            "word_214":state.word_214,"word_218":u32::from(state.helper_218.is_some()),"word_21c":state.word_21c,"home_222":state.position_222,
            "word_228":state.word_228,"byte_22c":state.byte_22c,"word_22e":state.word_22e,
        });
        if let Family428650Class::Selector35(ship) = &state.class {
            assert_eq!(ship.word_260, None);
            let retained = ship.retained_23c.as_ref().map(|retained| {
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
            let low_word = match ship.bytes_240[..2] {
                [Some(a), Some(b)] => Some(u16::from_le_bytes([a, b])),
                [None, None] => None,
                _ => panic!("partial lowWORD"),
            };
            value["selector35"] = json!({
                "word_230":ship.word_230,"word_234":ship.word_234,"byte_238":ship.byte_238,"byte_239":ship.byte_239,
                "retained_23c":retained,"word_240":low_word,"byte_242":ship.bytes_240[2].unwrap(),"byte_243":ship.bytes_240[3].unwrap(),
                "word_244":ship.word_244,"word_248":ship.word_248,"byte_250":ship.byte_250,"byte_251":ship.byte_251,
                "word_254":ship.word_254,"word_258":ship.word_258,"word_25c":ship.word_25c,"word_264":ship.word_264,
                "word_268":ship.handle_268.raw(),"byte_26c":ship.byte_26c,"table_270":ship.table_270,
            });
        }
    }
    result
}

fn compare(fixture: &Value, ship: bool) {
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
        let mut rng = 0;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = step["object"].as_u64().unwrap_or(1) as u32;
            match step["op"].as_str().unwrap() {
                "construct" => {
                    rng = word(&step["rng_seed"]);
                    let request = DerivedActorConstructionRequest {
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
                        world_mask_limits: serde_json::from_value(step["limits"].clone()).unwrap(),
                    };
                    let object = if ship {
                        arena.allocate_selector35_actor(request, &mut rng)
                    } else {
                        arena.allocate_family428650_actor(request, &mut rng)
                    }
                    .unwrap();
                    objects.insert(id, object);
                }
                "initialize" => {
                    let (payload, allocation) = if ship {
                        let (payload, allocation) = ship_payload(step);
                        (payload, Some(allocation))
                    } else {
                        (common::payload(&step["payload"], &step["extension"]), None)
                    };
                    let actor = ActorInitializationRequest {
                        context: RotatingVisualContext {
                            base: BaseContext {
                                position_words: serde_json::from_value(
                                    step["position_words"].clone(),
                                )
                                .unwrap(),
                                placement_index: word(&step["placement_index"]),
                            },
                            model_argument: Some(word(&step["model_argument"])),
                            rotation_words: serde_json::from_value(step["rotation_words"].clone())
                                .unwrap(),
                        },
                        payload: &payload,
                        local_owner: 1,
                    };
                    if let Some(allocation) = allocation {
                        arena
                            .initialize_selector35_actor(
                                objects[&id],
                                Selector35InitializationRequest {
                                    actor,
                                    allocation: &allocation,
                                    payload_offset: 4,
                                },
                                &resident.runtime,
                                &math,
                            )
                            .unwrap();
                        sources.payload = Some(allocation);
                    } else {
                        arena
                            .initialize_derived_actor(objects[&id], actor, &resident.runtime, &math)
                            .unwrap();
                    }
                }
                "guard" => assert_eq!(
                    arena.admit_family428650_actor(objects[&id]).unwrap(),
                    step["accepted"].as_bool().unwrap()
                ),
                "context" => arena
                    .initialize_context(
                        objects[&id],
                        BaseContext {
                            position_words: serde_json::from_value(step["position_words"].clone())
                                .unwrap(),
                            placement_index: word(&step["placement_index"]),
                        },
                    )
                    .unwrap(),
                "terrain" => {
                    terrain = Some(flat_terrain(step["raw_height"].as_i64().unwrap() as i8))
                }
                "command" | "point_command" => {
                    let code = step["code"].as_i64().unwrap() as i32;
                    if ship && code == 23 {
                        // Native accepts an unread unmapped operand address;
                        // the portable equivalent is a valid one-past owner.
                        let owner = Arc::new(
                            RuntimeAllocation::new(Arc::from([]), ByteSpan::new(0, 0)).unwrap(),
                        );
                        arena
                            .set_selector35_retained(objects[&id], &owner, 0)
                            .unwrap();
                        sources.unread = Some(owner);
                    } else {
                        let mut bytes = code.to_le_bytes().to_vec();
                        if let Some(values) = step["words"].as_array() {
                            for value in values {
                                bytes.extend_from_slice(&(word(value) as u16).to_le_bytes());
                            }
                        } else {
                            for value in step["payload_dwords"].as_array().unwrap() {
                                bytes.extend_from_slice(&word(value).to_le_bytes());
                            }
                        }
                        let command = decode_instance_command(InstanceCommandRequest {
                            read_scope: inf_formats::InstanceCommandReadScope::Dispatch,
                            source: &bytes,
                            main_payload: ByteSpan::new(0, bytes.len()),
                            relocations: &[],
                            command_offset: 0,
                            target: if ship {
                                InstanceCommandTarget::Constructor(35)
                            } else {
                                InstanceCommandTarget::Family428650
                            },
                        })
                        .unwrap();
                        let context = ActorCommandContext {
                            runtime: &resident.runtime,
                            first_player_grants: None,
                            terrain: terrain.as_ref(),
                        };
                        let accepted = if ship {
                            arena.apply_selector35_command(objects[&id], &command, context)
                        } else {
                            arena.apply_family428650_command(objects[&id], &command, context)
                        }
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
                        .set_selector35_retained(objects[&id], &allocation, 0)
                        .unwrap();
                    sources.commands.insert(word(&step["id"]), allocation);
                }
                "after_load" => arena
                    .initialize_family428650_after_load(objects[&id])
                    .unwrap(),
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
                "post_initialize" => {
                    if ship {
                        arena
                            .finish_selector35_initialization(objects[&id])
                            .unwrap();
                    } else {
                        arena.finish_actor_initialization(objects[&id]).unwrap();
                    }
                }
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected family operation {operation}"),
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

#[test]
fn common_family_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/family428650_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 4);
    assert_eq!(fixture["coverage"]["snapshots"], 88);
    compare(&fixture, false);
}

#[test]
fn selector35_matches_complete_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector35_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 8);
    assert_eq!(fixture["coverage"]["snapshots"], 152);
    compare(&fixture, true);
}
