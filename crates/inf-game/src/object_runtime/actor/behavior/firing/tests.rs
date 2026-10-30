use super::*;
use crate::object_runtime::{
    ActorBehaviorGroup, ActorConstructionRequest, ActorFollowTarget, ActorHandle,
    ActorRegistryConfig, ActorServiceResourceState, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation, NativeTarget,
    SoundTeardownService,
};
use serde_json::{json, Value};

fn fixture() -> (ObjectArena, ObjectId, ProcessActorBehaviorTables) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([100, 200, 300]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    let mut behavior = super::super::build_actor_behavior(1, 0, 1).unwrap();
    behavior.byte_10 = 0;
    behavior.word_08 = 0;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(behavior);
    (
        arena,
        actor,
        ProcessActorBehaviorTables {
            descriptor_bytes: Some(Box::new([Some(0); 860])),
            registry_bytes: Some(Box::new([Some(0); 528])),
            default_packet_bytes: Some([Some(0); 4]),
            pursuit_ship_bytes: None,
        },
    )
}
fn u(v: &Value) -> u32 {
    v.as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| v.as_i64().unwrap() as u32)
}
fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&v[i]) as u16)
}
fn write(tables: &mut ProcessActorBehaviorTables, address: u32, bytes: &[u8]) {
    tables
        .write_process_bytes(
            address,
            &bytes.iter().copied().map(Some).collect::<Vec<_>>(),
        )
        .unwrap();
}
fn row16(tables: &mut ProcessActorBehaviorTables, offset: u32, value: u16) {
    write(tables, 0x0053_0e60 + offset, &value.to_le_bytes());
}
fn row32(tables: &mut ProcessActorBehaviorTables, offset: u32, value: u32) {
    write(tables, 0x0053_0e60 + offset, &value.to_le_bytes());
}
#[test]
fn firing_direction_producers_match_native_float_bits_rng_and_lead() {
    let source: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/firing_producers_native.json"
    ))
    .unwrap();
    assert_eq!(source["provenance"]["fpcw"], 0x27f);
    for (index, case) in source["cases"].as_array().unwrap().iter().enumerate() {
        let (mut arena, actor, mut tables) = fixture();
        let input = &case["input"];
        let expected = &case["output"];
        let b = arena.weapon_behavior_mut(actor).unwrap();
        b.word_04 = u(&input["word_04"]);
        b.byte_0c = u(&input["byte_0c"]) as u8;
        b.word_20 = u(&input["initial_word20"]);
        b.entries_18[0][0] = 1;
        row16(&mut tables, 2, u(&input["speed"]) as u16);
        row16(&mut tables, 6, 1);
        row32(&mut tables, 8, 1);
        write(
            &mut tables,
            0x0053_dbb4,
            &u(&input["registry_flags"]).to_le_bytes(),
        );
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&input["actor_velocity"]);
        let target = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: ActorHandle::from_raw(0),
            focus_08: None,
            word_10: 2,
            focus_14: None,
            velocity_1a: Some(words(&input["target_velocity"])),
        }));
        let mut rng = u(&input["rng"]);
        arena
            .fire_actor_weapon(
                actor,
                ActorWeaponFireRequest {
                    target: Some(target),
                    distance: u(&input["distance"]),
                    world: words(&input["world"]).map(|v| v as i16),
                    microseconds: 1,
                    mode: if input["mode"] == "scattered" {
                        ActorWeaponFireMode::Scattered
                    } else {
                        ActorWeaponFireMode::Predictive {
                            publish_lead: input["publish_lead"].as_bool().unwrap(),
                        }
                    },
                },
                &mut ActorWeaponFireContext {
                    tables: &mut tables,
                    rng: &mut rng,
                    process_velocity: None,
                },
            )
            .unwrap();
        let b = arena.weapon_behavior(actor).unwrap();
        let node = arena
            .actor_services
            .weapon_fire_node(b.fire_head_00.unwrap())
            .unwrap();
        assert_eq!(
            json!(node.direction_bits),
            expected["direction_float_bits"],
            "case {index}"
        );
        assert_eq!(json!(rng), expected["rng_after"], "case {index}");
        assert_eq!(
            json!(b.word_20),
            expected["behavior_word20"],
            "case {index}"
        );
    }
}
fn snapshot(arena: &ObjectArena, actor: ObjectId, rng: u32) -> Value {
    let b = arena.weapon_behavior(actor).unwrap();
    let refs: Vec<_> = arena
        .actor_services
        .fixture_references()
        .into_iter()
        .filter(|r| {
            matches!(
                arena.actor_services.state(*r),
                Ok(ActorServiceResourceState::WeaponFireNode(_))
            )
        })
        .collect();
    let id = |reference: Option<NativeHeapRef>| {
        reference.map(|r| refs.iter().position(|item| *item == r).unwrap())
    };
    let nodes:Vec<_>=refs.iter().enumerate().map(|(index,r)|{
        let node=arena.actor_services.weapon_fire_node(*r).unwrap();
        json!({"id":index,"words":[node.direction_bits[0],node.direction_bits[1],node.direction_bits[2],node.compensation as u32,node.group,node.selection,node.speed_scale],"next":id(node.next)})
    }).collect();
    json!({"head":id(b.fire_head_00),"entry_words":b.entries_18[0],
        "group_flags":b.groups_1c.iter().map(|g|g.word_0c).collect::<Vec<_>>(),
        "actor_flags":arena.object(actor).unwrap().base.flags,
        "actor_dirty":arena.object(actor).unwrap().base.dirty_word,
        "rng_after":rng,"nodes":nodes})
}
#[test]
fn firing_queue_matches_native_guards_rounds_null_and_failure_prefixes() {
    let source: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/firing_queue_native.json"
    ))
    .unwrap();
    for (index, case) in source["cases"].as_array().unwrap().iter().enumerate() {
        let (mut arena, actor, mut tables) = fixture();
        let input = &case["input"];
        let b = arena.weapon_behavior_mut(actor).unwrap();
        b.byte_0d = u(&input["group_count"]) as u8;
        b.byte_11 = if input["alternate"].as_bool().unwrap() {
            0
        } else {
            1
        };
        b.word_08 = u(&input["random_delay"]);
        b.entries_18[0] = [
            u(&input["ammo"]),
            111,
            222,
            333,
            444,
            u(&input["cooldown"]),
            u(&input["cursor"]),
            u(&input["age"]),
        ];
        b.groups_1c = (0..4)
            .map(|i| ActorBehaviorGroup {
                coordinates_00_0a: [None; 6],
                word_0c: i,
            })
            .collect();
        arena.owned_mut(actor).unwrap().state.base.flags = 0x82000003;
        arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x80;
        row16(&mut tables, 6, u(&input["shot_count"]) as u16);
        for (offset, key) in [
            (8, "cooldown_step"),
            (12, "group_timer"),
            (16, "wrap_delay"),
            (28, "age_step"),
        ] {
            row32(&mut tables, offset, u(&input[key]));
        }
        if input["prior_head"].as_bool().unwrap() {
            let node = arena
                .actor_services
                .allocate(ActorServiceAllocationRequest {
                    kind: ActorServiceResourceKind::WeaponFireNode,
                })
                .unwrap()
                .unwrap();
            arena
                .actor_services
                .construct_weapon_fire_node(
                    node,
                    NativeWeaponFireNode {
                        direction_bits: [1, 2, 3],
                        compensation: 4,
                        group: 5,
                        selection: 6,
                        speed_scale: 7,
                        next: None,
                    },
                )
                .unwrap();
            arena.weapon_behavior_mut(actor).unwrap().fire_head_00 = Some(node);
        }
        if input["stop_allocation"].as_bool().unwrap() {
            arena
                .actor_services
                .script([FixtureActorServiceAllocation::Error(1)]);
        } else if u(&input["null_allocation"]) > 0 {
            let n = u(&input["null_allocation"]);
            arena.actor_services.script((1..=n).map(|i| {
                if i == n {
                    FixtureActorServiceAllocation::Null
                } else {
                    FixtureActorServiceAllocation::Allocate
                }
            }));
        }
        let mut rng = u(&input["rng"]);
        let result = arena.queue_actor_weapon_direction(
            actor,
            u(&input["interval"]),
            [1056964608, 3204448256, 1060439283],
            u(&input["speed_scale"]),
            &mut ActorWeaponFireContext {
                tables: &mut tables,
                rng: &mut rng,
                process_velocity: None,
            },
        );
        assert_eq!(
            result.is_err(),
            input["stop_allocation"].as_bool().unwrap(),
            "case {index}: {result:?}"
        );
        assert_eq!(snapshot(&arena, actor, rng), case["output"], "case {index}");
    }
}
#[test]
fn firing_teardown_and_guard_replacement_release_linked_allocations() {
    for replace in [false, true] {
        let (mut arena, actor, mut tables) = fixture();
        row16(&mut tables, 6, 1);
        row32(&mut tables, 8, 10);
        arena
            .queue_actor_weapon_direction(
                actor,
                21,
                [1, 2, 3],
                0,
                &mut ActorWeaponFireContext {
                    tables: &mut tables,
                    rng: &mut 0,
                    process_velocity: None,
                },
            )
            .unwrap();
        let nodes = arena.actor_services.fixture_references();
        assert_eq!(nodes.len(), 3);
        if replace {
            arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
            arena.admit_derived_actor(actor).unwrap();
            assert!(arena.weapon_behavior(actor).unwrap().fire_head_00.is_none());
        } else {
            arena
                .teardown_actor_behavior(actor, &mut SoundTeardownService::RequiredAtReach)
                .unwrap();
            assert!(arena.derived_actor(actor).unwrap().behavior_140.is_none());
        }
        for node in nodes {
            assert_eq!(
                arena.actor_services.state(node),
                Ok(ActorServiceResourceState::Released)
            );
            assert_eq!(arena.actor_services.retained_bytes(node), Ok(0));
        }
    }
}
#[test]
fn firing_missing_registry_preserves_lead_and_missing_group_preserves_ammo() {
    let (mut arena, actor, mut tables) = fixture();
    row16(&mut tables, 2, 3000);
    tables.registry_bytes = None;
    let target = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: None,
        word_10: 2,
        focus_14: None,
        velocity_1a: Some([0; 3]),
    }));
    let mut rng = 7;
    assert!(matches!(
        arena.fire_actor_weapon(
            actor,
            ActorWeaponFireRequest {
                target: Some(target),
                distance: 1000,
                world: [300, -400, 1200],
                microseconds: 1,
                mode: ActorWeaponFireMode::Predictive { publish_lead: true }
            },
            &mut ActorWeaponFireContext {
                tables: &mut tables,
                rng: &mut rng,
                process_velocity: None
            }
        ),
        Err(ActorWeaponFireError::Storage(_))
    ));
    assert_eq!(arena.weapon_behavior(actor).unwrap().word_20, 426);
    assert_eq!(rng, 7);
    row16(&mut tables, 6, 1);
    let b = arena.weapon_behavior_mut(actor).unwrap();
    b.entries_18[0][0] = 3;
    b.groups_1c.clear();
    assert!(matches!(
        arena.queue_actor_weapon_direction(
            actor,
            1,
            [0; 3],
            0,
            &mut ActorWeaponFireContext {
                tables: &mut tables,
                rng: &mut rng,
                process_velocity: None
            }
        ),
        Err(ActorWeaponFireError::GroupBacking { .. })
    ));
    assert_eq!(arena.weapon_behavior(actor).unwrap().entries_18[0][0], 2);
}

#[test]
fn firing_teardown_native_chain_order_and_free_failure_keep_reached_head() {
    let source: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/firing_teardown_native.json"
    ))
    .unwrap();
    let mut checked = 0;
    for case in source["cases"].as_array().unwrap() {
        let count = u(&case["input"]["node_count"]) as usize;
        let stop = u(&case["input"]["stop_at_free"]) != 0;
        // Rust-owned descriptor Vec drops have no external deallocator failure
        // service. These receipts check node frees and the null-audio suffix.
        if u(&case["input"]["sound"]) != 0 || (count == 0 && stop) {
            continue;
        }
        let (mut arena, actor, _) = fixture();
        let mut references = Vec::new();
        for _ in 0..count {
            references.push(
                arena
                    .actor_services
                    .allocate(ActorServiceAllocationRequest {
                        kind: ActorServiceResourceKind::WeaponFireNode,
                    })
                    .unwrap()
                    .unwrap(),
            );
        }
        for (index, reference) in references.iter().copied().enumerate() {
            arena
                .actor_services
                .construct_weapon_fire_node(
                    reference,
                    NativeWeaponFireNode {
                        direction_bits: [0; 3],
                        compensation: 0,
                        group: 0,
                        selection: 0,
                        speed_scale: 0,
                        next: references.get(index + 1).copied(),
                    },
                )
                .unwrap();
        }
        arena.weapon_behavior_mut(actor).unwrap().fire_head_00 = references.first().copied();
        if stop {
            arena.actor_services.fixture_fail_next_release(5);
        }
        let result =
            arena.teardown_actor_behavior(actor, &mut SoundTeardownService::RequiredAtReach);
        assert_eq!(
            result.is_ok(),
            case["output"]["normal_return"].as_bool().unwrap()
        );
        let actual: Vec<_> = arena
            .actor_services
            .fixture_release_attempts()
            .iter()
            .map(|r| references.iter().position(|v| v == r).unwrap())
            .collect();
        let expected: Vec<_> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["owner"] == "node")
            .map(|c| u(&c["node"]) as usize)
            .collect();
        assert_eq!(actual, expected);
        let head = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .and_then(|b| b.fire_head_00)
            .map(|r| references.iter().position(|v| *v == r).unwrap());
        assert_eq!(json!(head), case["output"]["head"]);
        if stop {
            assert!(!arena.weapon_behavior(actor).unwrap().entries_18.is_empty());
            assert_eq!(arena.actor_services.retained_bytes(references[0]), Ok(32));
        }
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn firing_timer_read_failure_keeps_published_node_and_actor_flag() {
    let (mut arena, actor, mut tables) = fixture();
    row16(&mut tables, 6, 1);
    tables.write_process_bytes(0x0053_0e6c, &[None]).unwrap();
    arena.weapon_behavior_mut(actor).unwrap().entries_18[0][0] = 3;
    let error = arena
        .queue_actor_weapon_direction(
            actor,
            1,
            [1, 2, 3],
            77,
            &mut ActorWeaponFireContext {
                tables: &mut tables,
                rng: &mut 0,
                process_velocity: None,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ActorWeaponFireError::Storage(BehaviorCacheStorageError::Unknown {
            address: 0x0053_0e6c
        })
    ));
    let behavior = arena.weapon_behavior(actor).unwrap();
    assert_eq!(behavior.entries_18[0], [2, 0, 0, 0, 0, 0, 0, 0]);
    assert_ne!(arena.object(actor).unwrap().base.flags & 0x01000000, 0);
    let node = arena
        .actor_services
        .weapon_fire_node(behavior.fire_head_00.unwrap())
        .unwrap();
    assert_eq!(node.direction_bits, [1, 2, 3]);
    assert_eq!(node.speed_scale, 77);
    assert_eq!(behavior.groups_1c[0].word_0c & 4, 4);
}
#[test]
fn firing_stationary_infinite_round_reports_guard_after_reached_writes() {
    let (mut arena, actor, mut tables) = fixture();
    row16(&mut tables, 6, 1);
    let error = arena
        .queue_actor_weapon_direction(
            actor,
            1,
            [0; 3],
            0,
            &mut ActorWeaponFireContext {
                tables: &mut tables,
                rng: &mut 0,
                process_velocity: None,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ActorWeaponFireError::PortableNonProgressingWeaponQueueGuard { .. }
    ));
    let behavior = arena.weapon_behavior(actor).unwrap();
    assert!(behavior.fire_head_00.is_some());
    assert_eq!(behavior.entries_18[0][6], 0);
    assert_ne!(arena.object(actor).unwrap().base.flags & 0x01000000, 0);
}
