//! Selector10 state from unchanged native segments separated at heap boundaries.
//! Allocations/free implementations are excluded; every supplied-storage tail
//! records the actual prior request and return address in the native fixture.

use super::native_tests::{flat_terrain, selector_payload, word};
use crate::object_runtime::{Selector10Helper, Selector10HelperChildClass, Selector10HelperClass};
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::derived::native_tests as derived_fixture,
        native_model_fixture::{Fixture, Models},
        ActorCommandContext, ActorConstructionRequest, ActorHandle, ActorInitializationRequest,
        ActorRegistryConfig, ActorTerrainInputs, BaseContext, DerivedActorConstructionRequest,
        Family4236d0Class, Family4236d0InitializationRequest, ObjectArena, ObjectId,
        RetainedActorAllocation, RotatingVisualContext, RuntimeAllocation,
    },
};
use inf_formats::{
    decode_instance_command, native_basis::NativeBasisMath, ByteSpan, InstanceCommandReadScope,
    InstanceCommandRequest, InstanceCommandTarget,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Arc};

#[derive(Default)]
struct RetainedSources {
    payload: Option<Arc<RuntimeAllocation>>,
    commands: BTreeMap<u32, Arc<RuntimeAllocation>>,
    unread: Option<Arc<RuntimeAllocation>>,
}

fn retained(
    retained: &RetainedActorAllocation,
    sources: &RetainedSources,
    payload_offset: u32,
) -> Value {
    if sources
        .payload
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, retained.allocation()))
    {
        assert_eq!(retained.allocation_offset, 4 + payload_offset);
        json!({"source":"payload", "offset":payload_offset})
    } else if sources
        .unread
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, retained.allocation()))
    {
        assert_eq!(retained.allocation_offset, 0);
        json!({"source":"unmapped", "offset":0})
    } else {
        let id = sources
            .commands
            .iter()
            .find(|(_, owner)| Arc::ptr_eq(owner, retained.allocation()))
            .unwrap()
            .0;
        assert_eq!(retained.allocation_offset, 0);
        json!({"source":"command", "id":id})
    }
}

fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    rng: u32,
    sources: &RetainedSources,
) -> Value {
    let mut result = super::native_tests::snapshot_from_prefix(
        derived_fixture::snapshot_prefix(arena, objects, rng),
        arena,
        objects,
        &super::native_tests::RetainedSources::default(),
    );
    for value in result["objects"].as_array_mut().unwrap() {
        let object = objects[&word(&value["id"])];
        let family = arena.family428650_actor(object).unwrap();
        value["helper_218"] = json!(family.helper_218.as_ref().map(|h| {
            assert_eq!(h.words_00_04, [None; 2]);
            assert_eq!(
                h.records,
                [crate::object_runtime::FamilyHelperHistoryRecord::UNKNOWN; 4]
            );
            json!({"word_08":h.word_08,"other_words":vec![None::<u32>;22]})
        }));
        let parent = arena.family429e20_actor(object).unwrap();
        assert_eq!((parent.word_230, parent.word_23a), (None, None));
        let probe = parent.probe_234.as_ref().unwrap().decoded_cache();
        value["probe_234"] = json!({"word_04":probe.word_04(),"word_06":probe.word_06(),
            "byte_08":probe.byte_08(),"byte_09":probe.dimension(),"byte_0a":probe.stride_words(),
            "byte_0b":probe.byte_0b(),"words_00":probe.words()});
        let state = arena.family4236d0_actor(object).unwrap();
        assert_eq!((state.word_24c, state.byte_261), (None, None));
        let leaf = arena.selector10_actor(object).unwrap();
        value["helper_26c"] = json!(leaf
            .helper_26c
            .as_ref()
            .map(|helper| helper_snapshot(helper)));
        value["family4236d0_selector10"] = json!({
            "word_238":parent.word_238,"word_240":state.word_240,"table_244":state.table_244,
            "word_248":state.word_248,"table_250":state.table_250,"table_254":state.table_254,
            "table_258":state.table_258,"table_25c":state.table_25c,"byte_260":state.byte_260,
            "word_262":state.word_262,"word_264":state.word_264,"word_266":state.word_266,
            "retained_23c":state.retained_23c.as_ref().map(|r|retained(r,sources,0x58)),
            "word_268":leaf.word_268,
        });
        value["behavior_140"] = json!(arena.derived_actor(object).unwrap().behavior_140.as_ref().map(|b| {
            let groups:Vec<_> = b.groups_1c.iter().map(|g|json!({"words_00_08":g.dwords_00_08(),"word_0c":g.word_0c})).collect();
            json!({"word_04":b.word_04,"word_08":b.word_08,"byte_0c":b.byte_0c,"byte_0d":b.byte_0d,
                "byte_0e":b.byte_0e,"byte_0f":b.byte_0f,"byte_10":b.byte_10,"byte_11":b.byte_11,
                "table_14":b.table_14 as u32,"entries_18":b.entries_18,"groups_1c":groups,
                "word_20":b.word_20,"words_24":b.words_24,"word_2c":b.word_2c,
                "word_30":b.word_30,"word_34":b.word_34,"word_38":b.word_38})
        }));
    }
    result
}

fn helper_snapshot(helper: &Selector10Helper) -> Value {
    let children: Vec<_> = helper
        .children_14
        .iter()
        .map(|child| {
            child.as_ref().map(|child| {
                let mut unknown = child.unknown_04_13.to_vec();
                unknown.extend(child.unknown_18_35);
                unknown.extend(child.unknown_38_4f);
                if let Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } = &child.class {
                    unknown.extend(unknown_50_73);
                }
                json!({"vtable":child.vtable_00,"size":child.allocation_bytes(),
            "word_14":child.word_14,"word_36":child.word_36,"unwritten_bytes":unknown})
            })
        })
        .collect();
    let tail = match &helper.class {
        Selector10HelperClass::Family445ae0 => None,
        Selector10HelperClass::Family446bc0 { phase_34 } => phase_34.map(u32::from),
    };
    json!({"vtable":helper.vtable_00,"size":helper.allocation_bytes(),
        "word_04":helper.word_04,"word_08":helper.word_08,"byte_0c":helper.byte_0c,
        "byte_0d":helper.byte_0d,"word_0e":helper.word_0e,"table_10":helper.table_10,
        "children_14":children,"tail_34":tail})
}

fn check_heap_boundaries(case: &Value) {
    let init = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["op"] == "initialize")
        .unwrap();
    let extra: Vec<u8> =
        serde_json::from_value(init["specialized"]["bytes_58_77"].clone()).unwrap();
    let mode = u32::from_le_bytes(extra[28..32].try_into().unwrap());
    let count = [6, 4, 4, 2, 2].get(mode as usize).copied().unwrap_or(0);
    let groups = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["op"] == "seed_guard")
        .map(|step| 1 + ((word(&step["mode_byte"]) >> 7) & 1));
    let mut allocations = BTreeMap::new();
    let mut frees = Vec::new();
    for boundary in case["heap_boundaries"].as_array().unwrap() {
        let owner = boundary["owner"].as_str().unwrap();
        let size = word(&boundary["size"]);
        match boundary["kind"].as_str().unwrap() {
            "allocate" => {
                let (expected_return, expected_size) = match owner {
                    "probe_234" => (0x0042_3774, 12),
                    "probe_words" => (0x0045_4b7c, 128),
                    "behavior_140" => (0x0042_6ccf, 0x3c),
                    "behavior_entries" => (0x0048_ccb3, 64),
                    "behavior_groups" => (0x0048_cd01, groups.unwrap() * 16),
                    "behavior_words" => (0x0048_cd3c, groups.unwrap() * 4),
                    "helper_218" => (0x0042_514f, 0x5c),
                    "helper_26c" => (
                        [
                            0x0042_52e6,
                            0x0042_5315,
                            0x0042_5341,
                            0x0042_5369,
                            0x0042_5391,
                        ][mode as usize],
                        if mode == 4 { 0x38 } else { 0x34 },
                    ),
                    child if child.starts_with("helper_child_") => {
                        let index: usize = child
                            .strip_prefix("helper_child_")
                            .unwrap()
                            .parse()
                            .unwrap();
                        assert!(index < count);
                        if mode == 4 {
                            (0x0044_6c06, 0x74)
                        } else {
                            (0x0044_5b26, 0x50)
                        }
                    }
                    other => panic!("unexpected allocation {other}"),
                };
                assert_eq!(word(&boundary["return_site"]), expected_return);
                assert_eq!(size, expected_size);
                assert!(allocations.insert(owner, size).is_none());
            }
            "free" => {
                assert_eq!(allocations[owner], size);
                if owner.starts_with("helper_child_") {
                    assert_eq!(word(&boundary["return_site"]), 0x0044_5bac);
                } else if owner == "helper_26c" {
                    assert_eq!(word(&boundary["return_site"]), 0x0044_6c75);
                }
                frees.push(owner.to_owned());
            }
            other => panic!("unexpected heap boundary {other}"),
        }
    }
    let mut expected: Vec<_> = (0..count)
        .map(|index| format!("helper_child_{index}"))
        .collect();
    if count != 0 {
        expected.push("helper_26c".to_owned());
    }
    expected.extend(["probe_words", "probe_234"].map(str::to_owned));
    if allocations.contains_key("behavior_140") {
        expected.extend(
            [
                "helper_218",
                "behavior_entries",
                "behavior_groups",
                "behavior_words",
                "behavior_140",
            ]
            .map(str::to_owned),
        );
    }
    assert_eq!(frees, expected);
}

#[test]
fn selector10_matches_native_heap_boundary_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector10_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 8);
    assert_eq!(fixture["coverage"]["snapshots"], 161);
    assert_eq!(fixture["coverage"]["heap_boundaries"], 118);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        check_heap_boundaries(case);
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
                    let object = arena
                        .allocate_selector10_actor(
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
                        serde_json::from_value(step["specialized"]["bytes_58_77"].clone()).unwrap();
                    let (payload, allocation) = selector_payload(step, 10, &specialized);
                    arena
                        .initialize_selector10_actor(
                            objects[&id],
                            Family4236d0InitializationRequest {
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
                    if code == 24 {
                        // An owned one-past pointer is retained without reading, matching
                        // native storage of an unmapped address until postinitialization.
                        let owner = Arc::new(
                            RuntimeAllocation::new(Arc::from([]), ByteSpan::new(0, 0)).unwrap(),
                        );
                        arena
                            .set_family4236d0_retained(objects[&id], &owner, 0)
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
                            target: InstanceCommandTarget::Constructor(10),
                        })
                        .unwrap();
                        let accepted = arena
                            .apply_selector10_command(
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
                        .set_family4236d0_retained(objects[&id], &allocation, 0)
                        .unwrap();
                    sources.commands.insert(word(&step["id"]), allocation);
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
                "seed_guard" => {
                    arena.derived_actor_mut(objects[&id]).unwrap().byte_13d =
                        Some(word(&step["mode_byte"]) as u8);
                }
                "post_initialize" => arena
                    .finish_selector10_initialization(objects[&id])
                    .unwrap(),
                "set_mode" => {
                    let Family4236d0Class::Selector10(state) =
                        &mut arena.family4236d0_actor_mut(objects[&id]).unwrap().class
                    else {
                        panic!("selector10")
                    };
                    state.word_268 = Some(word(&step["value"]));
                }
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&id],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected selector10 operation {operation}"),
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
