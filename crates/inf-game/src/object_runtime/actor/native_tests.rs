//! Complete shared actor construction/destruction receipts. Allocation bytes
//! are explicit synthetic inputs; this does not establish allocator custody.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainCell,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_GRID_SIDE, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{ActorTerrainSurface, ObjectList, ObjectNodeKind},
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn terrain(config: &Value) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[..4].copy_from_slice(&word(&config["plane_dword"]).to_le_bytes());
    let cells = &config["cells"];
    for x in 0..TERRAIN_GRID_SIDE {
        for z in 0..TERRAIN_GRID_SIDE {
            let (height, material) = match cells["kind"].as_str().unwrap() {
                "constant" => (word(&cells["raw_height"]) as u8, 0),
                "pattern" => {
                    let seed = word(&cells["seed"]) as usize;
                    (
                        ((x * 13 + z * 29 + seed) & 255) as u8,
                        ((x * 7 + z * 11 + seed) & 127) as u8,
                    )
                }
                kind => panic!("unknown terrain pattern {kind}"),
            };
            let offset = TERRAIN_PREFIX_BYTES + (x * TERRAIN_GRID_SIDE + z) * 2;
            bytes[offset..offset + 2].copy_from_slice(&[height, material]);
        }
    }
    // Canonical H12 parsing also validates the unrelated sparse markers. The
    // native actor reads only the dense cell; give each marker a valid record.
    for z in 0..TERRAIN_GRID_SIDE {
        let mut count = 0_u16;
        for x in 0..TERRAIN_GRID_SIDE {
            let offset = TERRAIN_PREFIX_BYTES + (x * TERRAIN_GRID_SIDE + z) * 2;
            let cell = TerrainCell::from_raw(u16::from_le_bytes(
                bytes[offset..offset + 2].try_into().unwrap(),
            ));
            if cell.has_sparse_record() {
                bytes.extend_from_slice(&(x as u16).to_le_bytes());
                bytes.extend_from_slice(&[0, 0]);
                count += 1;
            }
        }
        let offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2;
        bytes[offset..offset + 2].copy_from_slice(&(count * 4).to_le_bytes());
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
            spacing: match word(&config["spacing_mask"]) {
                127 => TerrainSpacing::Units128,
                255 => TerrainSpacing::Units256,
                mask => panic!("unsupported spacing mask {mask}"),
            },
            height_scale: match word(&config["height_shift"]) {
                4 => TerrainHeightScale::Units16,
                5 => TerrainHeightScale::Units32,
                shift => panic!("unsupported height shift {shift}"),
            },
        },
    )
}

fn normalized(object: ObjectId, objects: &BTreeMap<u32, ObjectId>) -> u32 {
    *objects.iter().find(|(_, id)| **id == object).unwrap().0
}

pub(in crate::object_runtime) fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
) -> Value {
    let states: Vec<_> = objects
        .iter()
        .filter_map(|(&id, &object)| {
            let base = &arena.object(object).ok()?.base;
            let actor = arena.actor(object).unwrap();
            Some(json!({
                "id":id,
                "word_28":base.word_28,"placement_word":base.placement_word,"flags":base.flags,
                "resource_word":base.resource_word,"position_words":base.position_words,
                "dirty_word":base.dirty_word,"word_3c":base.word_3c,
                "radius_word":actor.rotating.visual.radius_word,
                "model_word":actor.rotating.visual.model_word,"basis_words":actor.rotating.basis.words,
                "word_6c":actor.word_6c,"word_74":actor.word_74,"word_76":actor.word_76,
                "byte_78":actor.byte_78,"word_7a":actor.word_7a,
                "byte_7c":actor.byte_7c,"byte_7d":actor.byte_7d,"word_80":actor.word_80,
                "handle":actor.handle.raw(),"owner_handle":actor.owner_handle.raw(),
                "word_8c":actor.word_8c,"words_8e_92":actor.words_8e_92,"words_94_9c":actor.words_94_9c,
                "health":actor.health,"maximum_health":actor.maximum_health,
                "damage_responses":actor.damage_responses(),"word_b4":actor.word_b4,
                "word_c4":actor.word_c4,"word_c8":actor.word_c8,"word_cc":actor.word_cc,
                "word_ce":actor.word_ce,"byte_d0":actor.byte_d0,"word_d8":actor.word_d8,
                "word_dc":actor.word_dc,"word_e0":actor.word_e0,"word_e4":actor.word_e4,
            }))
        })
        .collect();
    let mut lists = serde_json::Map::new();
    for (name, role) in [
        ("flag0", ObjectList::Flag0Primary),
        ("flag1", ObjectList::Flag1Primary),
        ("flag0_secondary", ObjectList::Flag0Secondary),
        ("flag1_secondary", ObjectList::Flag1Secondary),
        ("parking", ObjectList::Parking),
        ("disposal", ObjectList::Disposal),
    ] {
        let nodes: Vec<_> = arena
            .list(role)
            .iter()
            .map(|node| {
                let offset = match node.kind {
                    ObjectNodeKind::Flag0 => 4,
                    ObjectNodeKind::Flag1 => 16,
                    ObjectNodeKind::Lifecycle => 28,
                };
                [normalized(node.object, objects), offset]
            })
            .collect();
        lists.insert(name.to_owned(), json!(nodes));
    }
    let actor_nodes: Vec<_> = arena
        .actor_list()
        .iter()
        .map(|&object| [normalized(object, objects), 0xA8])
        .collect();
    lists.insert("actors".to_owned(), json!(actor_nodes));

    let handles = arena.actor_handles().unwrap();
    let owners: Vec<_> = handles
        .owners()
        .iter()
        .enumerate()
        .map(|(owner, table)| {
            let slots: Vec<_> = table
                .slots()
                .iter()
                .map(|slot| {
                    json!({"handle":slot.handle.raw(),
                        "object":slot.object.map(|object|normalized(object,objects)),
                        "next_free":slot.next_free})
                })
                .collect();
            json!({"owner":owner,"word_00":table.word_00,"word_04":table.word_04,
                "free_head":table.free_head,"free_tail":table.free_tail,"slots":slots})
        })
        .collect();
    let cache = handles.cache();
    json!({"objects":states,"lists":lists,"registry":{"owners":owners,
        "cache":{"handle":cache.handle.raw(),"object":cache.object.map(|object|normalized(object,objects))}}})
}

#[test]
fn complete_actor_constructor_and_destructor_match_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_base_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 54);
    assert_eq!(fixture["coverage"]["snapshots"], 120);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["0047A7A0"], 60);
    assert_eq!(fixture["coverage"]["direct_native_entries"]["0047AA60"], 60);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let config = &case["terrain"];
        let source = terrain(config);
        let inputs = ActorTerrainInputs {
            terrain: config["present"]
                .as_bool()
                .unwrap()
                .then_some(ActorTerrainSurface {
                    terrain: &source,
                    plane_height_fixed: word(&config["plane_dword"]) as i32,
                }),
            coordinate_shift: word(&config["coordinate_shift"]),
            clock: word(&config["clock"]),
            world_flags: config["world_flags"].as_u64().map(|value| value as u32),
            sine_quarter: &sine,
        };
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let mut objects = BTreeMap::new();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let id = word(&step["object"]);
            match step["op"].as_str().unwrap() {
                "construct" => {
                    let object = arena
                        .allocate_actor(ActorConstructionRequest {
                            constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                            allocation_position: serde_json::from_value(
                                step["allocation_position"].clone(),
                            )
                            .unwrap(),
                            owner: word(&step["owner"]) as u8,
                            terrain: inputs,
                        })
                        .unwrap();
                    assert!(objects.insert(id, object).is_none());
                }
                "destroy" => {
                    let object = objects[&id];
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: object,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                    assert!(arena.object(object).is_err());
                }
                operation => panic!("unexpected actor operation {operation}"),
            }
            assert_eq!(
                snapshot(&arena, &objects),
                step["state"],
                "{} step{index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 120);
}

#[test]
fn host_preflight_errors_preserve_existing_actor_lists_and_registry() {
    // This is portable error policy, not an assertion that native unknown
    // memory or its late missing-H14 dereference safely returns an error.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_base_native.json"
    ))
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "near_signed_cell255")
        .unwrap();
    let source = terrain(&case["terrain"]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let missing = ActorTerrainInputs {
        terrain: None,
        coordinate_shift: 8,
        clock: 0,
        world_flags: None,
        sine_quarter: &sine,
    };
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let existing = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
            owner: 1,
            terrain: missing,
        })
        .unwrap();
    let handle = arena.actor(existing).unwrap().handle;
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(existing));
    let objects = BTreeMap::from([(1, existing)]);
    let before = snapshot(&arena, &objects);
    let present = ActorTerrainInputs {
        terrain: Some(ActorTerrainSurface {
            terrain: &source,
            plane_height_fixed: 25_600,
        }),
        ..missing
    };
    for (allocation_position, expected) in [
        (None, ActorTerrainError::UnknownAllocationPosition),
        (Some([0, 100, 0]), ActorTerrainError::MissingWorldResource),
    ] {
        assert_eq!(
            arena.allocate_actor(ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position,
                owner: 1,
                terrain: present,
            }),
            Err(ActorConstructionError::Terrain(expected))
        );
        assert_eq!(snapshot(&arena, &objects), before);
    }
}
