use std::{collections::BTreeMap, sync::Arc};

use inf_formats::{
    parse_constructor_asset_bank, AssetRelocation, ByteSpan, InstanceField, PlacementInstanceAssets,
};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    construct_world_object, BaseContext, InstanceCommandAllocation, ObjectClassKind,
    ObjectConstructionOutcome, ObjectConstructionRequest, ObjectList, ObjectNodeKind,
};
use crate::{terrain_runtime::TerrainSpacing, world_placement::WorldPlacements};

#[path = "factory_tests.rs"]
mod factory_tests;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn retail_runtime() -> crate::ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime =
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    runtime.apply(crate::ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 32] {
        runtime
            .apply(crate::ResidencyRequest::Activate { level })
            .unwrap();
    }
    runtime
}

#[test]
fn complete_native_construction_mutation_callbacks_and_destruction_match() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector86_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    let mut snapshots = 0;
    for case in cases {
        let mut arena = ObjectArena::new();
        let mut objects = BTreeMap::new();
        let mut allocations: BTreeMap<u32, Arc<RuntimeAllocation>> = BTreeMap::new();
        for step in case["steps"].as_array().unwrap() {
            let id = step["object"].as_u64().unwrap_or(0) as u32;
            match step["op"].as_str().unwrap() {
                "allocation" => {
                    let bytes: Vec<u8> = serde_json::from_value(step["bytes"].clone()).unwrap();
                    let main = ByteSpan::new(0, bytes.len());
                    allocations.insert(
                        number(&step["allocation"]),
                        Arc::new(RuntimeAllocation::new(bytes.into(), main).unwrap()),
                    );
                }
                "construct" => {
                    objects.insert(id, arena.allocate(ObjectClassKind::Selector86));
                }
                "initialize" => {
                    let allocation = &allocations[&number(&step["allocation"])];
                    let offset = number(&step["offset"]);
                    arena
                        .initialize_context(
                            objects[&id],
                            BaseContext {
                                position_words: serde_json::from_value(
                                    step["position_words"].clone(),
                                )
                                .unwrap(),
                                placement_index: number(&step["placement_index"]),
                            },
                        )
                        .unwrap();
                    arena
                        .apply_payload_flags(objects[&id], allocation.dword(offset).unwrap())
                        .unwrap();
                    arena
                        .set_selector86_data(objects[&id], allocation, offset + 4)
                        .unwrap();
                }
                "command" => {
                    let data = step["allocation"]
                        .as_u64()
                        .map(|key| (&allocations[&(key as u32)], number(&step["offset"])));
                    let accepted = arena
                        .apply_selector86_command(
                            objects[&id],
                            step["code"].as_i64().unwrap() as i32,
                            data,
                        )
                        .unwrap();
                    assert_eq!(accepted, step["accepted"].as_bool().unwrap());
                }
                "draw_context" => arena
                    .set_selector86_draw_distance(
                        objects[&id],
                        step["scene_word_68"].as_i64().unwrap() as i32,
                    )
                    .unwrap(),
                "after_load" => arena
                    .initialize_after_load(objects[&id], Default::default())
                    .unwrap(),
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects.remove(&id).unwrap(),
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                operation => panic!("unexpected {operation}"),
            }
            let mut state_objects = vec![];
            for (&key, &object) in &objects {
                let base = &arena.object(object).unwrap().base;
                let emitter = arena.selector86(object).unwrap();
                let retained = emitter.data_40.as_ref().map(|data| {
                    let owner = allocations
                        .iter()
                        .find(|(_, owner)| Arc::ptr_eq(owner, data.allocation()))
                        .unwrap()
                        .0;
                    json!({"allocation":owner,"offset":data.allocation_offset})
                });
                state_objects.push(json!({"id":key,"word_28":base.word_28,"placement_word":base.placement_word,"flags":base.flags,"resource_word":base.resource_word,"position_words":base.position_words,"dirty_word":base.dirty_word,"word_3c":base.word_3c,"retained_40":retained,"word_44":emitter.elapsed_44,"word_48":emitter.distance_squared_48}));
            }
            let lists: serde_json::Map<String, Value> = [
                ("flag0", ObjectList::Flag0Primary),
                ("flag1", ObjectList::Flag1Primary),
                ("flag0_secondary", ObjectList::Flag0Secondary),
                ("flag1_secondary", ObjectList::Flag1Secondary),
                ("parking", ObjectList::Parking),
                ("disposal", ObjectList::Disposal),
            ]
            .into_iter()
            .map(|(name, role)| {
                let nodes: Vec<_> = arena
                    .list(role)
                    .iter()
                    .map(|node| {
                        let key = objects
                            .iter()
                            .find(|(_, object)| **object == node.object)
                            .unwrap()
                            .0;
                        let offset = match node.kind {
                            ObjectNodeKind::Flag0 => 4,
                            ObjectNodeKind::Flag1 => 16,
                            ObjectNodeKind::Lifecycle => 28,
                        };
                        json!([key, offset])
                    })
                    .collect();
                (name.to_owned(), json!(nodes))
            })
            .collect();
            let state_allocations: Vec<_> = allocations
                .iter()
                .map(|(id, owner)| json!({"id":id,"bytes":owner.bytes()}))
                .collect();
            assert_eq!(
                json!({"objects":state_objects,"lists":lists,"allocations":state_allocations}),
                step["state"],
                "{} {}",
                case["id"],
                step["op"]
            );
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 98);
}

#[test]
fn first_world_six_emitters_construct_with_owned_commands_and_unchanged_evidence() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let source: Arc<[u8]> = Arc::from(placements.world_source().overlay.bytes());
    let original = source.to_vec();
    let allocation = Arc::new(
        InstanceCommandAllocation::new(
            source,
            placements.assets().worlds.main_payload,
            placements.assets().worlds.relocations.clone().into(),
        )
        .unwrap(),
    );
    let mut arena = ObjectArena::new();
    let mut count = 0;
    for index in 0..placements.world().placements.len() {
        let input = placements.input(index).unwrap();
        let resource = runtime
            .resource(13, input.resource_argument as u16)
            .unwrap();
        let region = &resource.overlay.layout().regions[resource.registration.region_index];
        let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
        if bank.prefixes.entries[resource.registration.local_index as usize].constructor_selector
            != 86
        {
            continue;
        }
        let outcome = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: input.resource_argument,
                context: BaseContext {
                    position_words: Some(input.position_words),
                    placement_index: input.placement_index,
                },
                suffix: Some(allocation.commands(input.instance)),
                command_allocation: Some(&allocation),
                network_active: true,
                actor: None,
            },
        )
        .unwrap();
        let ObjectConstructionOutcome::Constructed(object) = outcome else {
            panic!("retail emitter rejected")
        };
        let state = arena.selector86(object).unwrap();
        let data = state.data_40.as_ref().unwrap();
        assert!(Arc::ptr_eq(
            data.allocation(),
            allocation.runtime_allocation()
        ));
        let words = data.words();
        let expected = if words[1] as i16 > 0 {
            (60000 / i32::from(words[1] as i16)) as u16
        } else {
            30000
        };
        assert_eq!(words[3], expected);
        assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
        let before = arena.object(object).unwrap().clone();
        assert_eq!(
            arena.update(object),
            Err(ObjectError::Selector86UpdateBoundary(object))
        );
        assert_eq!(arena.object(object).unwrap(), &before);
        arena
            .initialize_after_load(object, Default::default())
            .unwrap();
        count += 1;
    }
    assert_eq!(count, 6);
    assert_eq!(allocation.source(), original);
    assert_eq!(placements.world_source().overlay.bytes(), original);
}

#[test]
fn owned_walk_rereads_overlapping_command_codes() {
    // First command's retained interval lands on the next command's code.
    // Rate=30000 writes 2, turning authored rejection 99 into another code 2.
    let mut source = vec![0; 40];
    source[0..4].copy_from_slice(&2_i32.to_le_bytes());
    source[4..8].copy_from_slice(&12_u32.to_le_bytes());
    source[8..12].copy_from_slice(&22_u32.to_le_bytes());
    source[12..16].copy_from_slice(&2_i32.to_le_bytes());
    source[18..20].copy_from_slice(&30000_i16.to_le_bytes());
    source[22..26].copy_from_slice(&99_i32.to_le_bytes());
    source[28..30].copy_from_slice(&2_i16.to_le_bytes());
    let relocations: Vec<_> = [(4, 12), (8, 22)]
        .into_iter()
        .map(|(field, target)| AssetRelocation {
            table_span: ByteSpan::new(0, 0),
            field_offset: field,
            field_span: ByteSpan::new(field as usize, field as usize + 4),
            target_offset: target,
        })
        .collect();
    let owner = Arc::new(
        InstanceCommandAllocation::new(source.into(), ByteSpan::new(0, 40), relocations.into())
            .unwrap(),
    );
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 2,
        },
        command_pointer_span: ByteSpan::new(4, 12),
        commands: vec![],
    };
    let runtime = retail_runtime();
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector86);
    assert_eq!(
        crate::object_runtime::construction::run_commands(
            &mut arena,
            &runtime,
            object,
            crate::object_runtime::construction::CommandPolicy::OwnedConstructor {
                selector: 86,
                allocation: &owner
            },
            0x1234,
            Some(owner.commands(&placement))
        )
        .unwrap(),
        None
    );
    let retained = arena.selector86(object).unwrap().data_40.as_ref().unwrap();
    assert_eq!(retained.allocation_offset, 26);
    assert_eq!(retained.words()[3], 30000);
    assert_eq!(owner.runtime_allocation().word(22).unwrap(), 2);
    assert_eq!(
        u16::from_le_bytes(owner.source()[22..24].try_into().unwrap()),
        99
    );
}
