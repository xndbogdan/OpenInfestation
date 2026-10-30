//! Original selector-60 nonallocating calls and explicitly segmented allocation
//! tails. The latter establish supplied-storage results, not native malloc/free.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, AssetRelocation, ByteSpan,
    InstanceCommandRequest, InstanceCommandTarget,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::{
            derived::native_tests as common, native_tests as actor_native, ActorCommandContext,
            ActorConstructionRequest, ActorInitializationRequest,
        },
        native_model_fixture::{Fixture, Models},
        ActorRegistryConfig, ActorTerrainInputs, BaseContext, RotatingVisualContext,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

struct Allocation {
    owner: Arc<InstanceCommandAllocation>,
    pointers: Vec<u32>,
    runtime: Vec<u32>,
}

fn allocation(value: &Value) -> Allocation {
    let mut bytes: Vec<u8> = serde_json::from_value(value["bytes"].clone()).unwrap();
    let mut relocations = vec![];
    let mut pointers = vec![];
    for pointer in value["pointers"].as_array().unwrap() {
        let field = word(&pointer["offset"]);
        let target = word(&pointer["target"]["offset"]);
        assert_eq!(pointer["target"]["allocation"], value["allocation"]);
        bytes[field as usize..field as usize + 4].copy_from_slice(&target.to_le_bytes());
        relocations.push(AssetRelocation {
            table_span: ByteSpan::new(0, 0),
            field_offset: field,
            field_span: ByteSpan::new(field as usize, field as usize + 4),
            target_offset: target,
        });
        pointers.push(field);
    }
    let main = ByteSpan::new(0, bytes.len());
    Allocation {
        owner: Arc::new(
            InstanceCommandAllocation::new(bytes.into(), main, relocations.into()).unwrap(),
        ),
        pointers,
        runtime: serde_json::from_value(value["runtime_offsets"].clone()).unwrap(),
    }
}

fn row_snapshot(row: &Selector60Row) -> Value {
    json!({
        "byte_00": row.byte_00, "byte_01": row.byte_01, "word_04": row.word_04,
        "handles_08": row.handles_08.iter().map(|handle| handle.raw()).collect::<Vec<_>>(),
        "word_0c": row.word_0c, "word_10": row.word_10,
    })
}

fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    allocations: &BTreeMap<u32, Allocation>,
    rng: u32,
) -> Value {
    let mut result = common::snapshot(arena, objects, rng);
    for value in result["objects"].as_array_mut().unwrap() {
        let actor = arena
            .selector60_actor(objects[&word(&value["id"])])
            .unwrap();
        assert_eq!(actor.position_198, None);
        let retained = actor.retained_190.as_ref().map(|retained| {
            let id = allocations
                .iter()
                .find(|(_, allocation)| Arc::ptr_eq(&allocation.owner, retained.allocation()))
                .unwrap()
                .0;
            json!({"allocation": id, "offset": retained.allocation_offset})
        });
        value["selector60"] = json!({
            "byte_184":actor.byte_184,"byte_188":actor.byte_188,"byte_189":actor.byte_189,
            "word_18a":actor.word_18a,"word_18c":actor.word_18c,"retained_190":retained,
            "word_194":actor.word_194,"word_19e":actor.word_19e,"word_1a0":actor.word_1a0,
            "word_1a4":actor.word_1a4,"byte_1a8":actor.byte_1a8,
        });
    }
    result["allocations"] = allocations
        .iter()
        .map(|(&id, allocation)| {
            let image = allocation.owner.runtime_allocation();
            let mut bytes = image.bytes();
            let pointers: Vec<_> = allocation
                .pointers
                .iter()
                .map(|&offset| {
                    let target = image.dword(offset).unwrap();
                    bytes[offset as usize..offset as usize + 4].fill(0);
                    json!({"offset": offset, "target": {"allocation": id, "offset": target}})
                })
                .collect();
            let typed = image.runtime_pointers();
            let runtime: Vec<_> = allocation
                .runtime
                .iter()
                .map(
                    |&offset| match typed.iter().find(|(field, _)| *field == offset) {
                        Some((_, RuntimePointer::Selector60Row(row))) => {
                            json!({"offset":offset,"row_state":row.index})
                        }
                        None => json!({"offset":offset,"raw":image.dword(offset).unwrap()}),
                    },
                )
                .collect();
            json!({"id":id,"bytes":bytes,"pointers":pointers,"runtime":runtime})
        })
        .collect::<Vec<_>>()
        .into();
    result["runtime_rows"] = arena
        .selector60_rows
        .0
        .iter()
        .map(|row| row_snapshot(row.as_ref().unwrap()))
        .collect::<Vec<_>>()
        .into();
    result
}

#[test]
fn selector60_matches_native_calls_and_separate_allocation_tails() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector60_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 10);
    assert_eq!(fixture["coverage"]["snapshots"], 96);
    assert_eq!(fixture["coverage"]["allocation_boundaries"], 12);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let terrain = ActorTerrainInputs {
        terrain: None,
        coordinate_shift: 8,
        clock: 0,
        world_flags: Some(0),
        sine_quarter: &sine,
    };
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
        let mut allocations = BTreeMap::new();
        let mut rng = 0;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = step["object"].as_u64().unwrap_or(1) as u32;
            match step["op"].as_str().unwrap() {
                "construct" => {
                    rng = word(&step["rng_seed"]);
                    let object = arena
                        .allocate_selector60_actor(
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
                    let payload = common::payload(&step["payload"], &step["extension"]);
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
                    let command = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Dispatch,
                        source: &bytes,
                        main_payload: ByteSpan::new(0, bytes.len()),
                        relocations: &[],
                        command_offset: 0,
                        target: InstanceCommandTarget::Constructor(60),
                    })
                    .unwrap();
                    assert_eq!(
                        arena
                            .apply_derived_actor_command(
                                objects[&id],
                                &command,
                                ActorCommandContext {
                                    terrain: None,
                                    runtime: &resident.runtime,
                                    first_player_grants: None
                                }
                            )
                            .unwrap(),
                        step["accepted"].as_bool().unwrap()
                    );
                }
                "allocation" => {
                    assert!(allocations
                        .insert(word(&step["allocation"]), allocation(step))
                        .is_none());
                }
                "table_command" => arena
                    .set_selector60_spawn_list(
                        objects[&id],
                        &allocations[&word(&step["allocation"])].owner,
                        word(&step["offset"]),
                    )
                    .unwrap(),
                "post_initialize" => arena.finish_actor_initialization(objects[&id]).unwrap(),
                "after_load" | "after_load_tails" => arena
                    .initialize_after_load(objects[&id], Default::default())
                    .unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected selector60 operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects, &allocations, rng),
                step["state"],
                "{} step {index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 96);
}

#[test]
fn selector60_row_matches_supplied_storage_native_tails() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector60_row_allocation_tails_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["allocation_prefixes"], 42);
    assert_eq!(fixture["coverage"]["postallocation_tails"], 38);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    for case in fixture["cases"].as_array().unwrap() {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let actor = arena
            .allocate_actor(ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([0, 1, 0]),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            })
            .unwrap();
        arena
            .initialize_context(
                actor,
                BaseContext {
                    position_words: None,
                    placement_index: 0,
                },
            )
            .unwrap();
        let objects = BTreeMap::from([(1, actor)]);
        let bytes: Vec<u8> = serde_json::from_value(case["row_bytes"].clone()).unwrap();
        let size = bytes.len();
        let retained = RetainedSpawnList {
            allocation: Arc::new(
                InstanceCommandAllocation::new(bytes.into(), ByteSpan::new(0, size), Arc::from([]))
                    .unwrap(),
            ),
            allocation_offset: 0,
        };
        if !case["tail_executed"].as_bool().unwrap() {
            let capacity = case["capacity"].as_i64().unwrap() as i16;
            assert_eq!(
                arena.construct_selector60_row(&retained, 0),
                Err(Selector60Error::NegativeCapacity(capacity))
            );
            assert_eq!(
                word(&case["allocation_boundary"]["request_bytes"]),
                (i32::from(capacity) * 4) as u32
            );
        } else {
            assert_eq!(
                actor_native::snapshot(&arena, &objects),
                case["actors_before"]
            );
            let row = arena.construct_selector60_row(&retained, 0).unwrap();
            assert_eq!(
                row_snapshot(arena.selector60_row(row).unwrap()),
                case["state"],
                "{}",
                case["id"]
            );
        }
        assert_eq!(
            actor_native::snapshot(&arena, &objects),
            case["actors"],
            "{}",
            case["id"]
        );
    }
}
