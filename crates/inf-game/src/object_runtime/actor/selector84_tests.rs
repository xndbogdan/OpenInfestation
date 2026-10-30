//! The capture node against its original bodies under Unicorn
//! ([selector84_native.json](../../../tests/fixtures/selector84_native.json),
//! `check_selector84.py`).

use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn cases(mode: &str) -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector84_native.json"
    ))
    .unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["input"]["mode"] == mode)
        .cloned()
        .collect()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn request(sine: &[u16; 4096]) -> ActorConstructionRequest<'_> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0, 0, 0]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: sine,
        },
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

/// A node as the case's input left it, the process counts beside it. Its
/// placement word matches no reference the cases use.
fn node(arena: &mut ObjectArena, input: &Value) -> ObjectId {
    let node = arena.allocate_selector84(request(&UNUSED_SINE)).unwrap();
    let base = &mut arena.owned_mut(node).unwrap().state.base;
    base.flags = number(&input["flags"]);
    base.dirty_word = number(&input["dirty"]) as u16;
    base.placement_word = 0x7ffe;
    arena.actor_mut(node).unwrap().word_b4 = number(&input["team"]);
    let state = arena.selector84_mut(node).unwrap();
    state.link_138 = number(&input["link"]);
    state.clock_13c = number(&input["clock"]);
    arena.capture_nodes = CaptureNodeCounts {
        aliens_547620: number(&input["counts"][0]),
        others_547624: number(&input["counts"][1]),
    };
    node
}

/// A plain actor with a team, a dirty word and a placement word.
fn actor(arena: &mut ObjectArena, team: u32, dirty: u32, placement: u16) -> ObjectId {
    let actor = arena.allocate_actor(request(&UNUSED_SINE)).unwrap();
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.dirty_word = dirty as u16;
    base.placement_word = placement;
    arena.actor_mut(actor).unwrap().word_b4 = team;
    actor
}

fn counts(arena: &ObjectArena) -> [u32; 2] {
    let counts = arena.capture_node_counts();
    [counts.aliens_547620, counts.others_547624]
}

fn expected_counts(output: &Value) -> [u32; 2] {
    [number(&output["counts"][0]), number(&output["counts"][1])]
}

/// The node's own fields the cases compare.
fn assert_node(arena: &ObjectArena, node: ObjectId, output: &Value, case: &Value) {
    let base = &arena.object(node).unwrap().base;
    let state = arena.selector84(node).unwrap();
    assert_eq!(
        (
            arena.actor(node).unwrap().word_b4,
            state.clock_13c,
            base.flags,
            base.dirty_word as u32 & 0xff,
            counts(arena),
        ),
        (
            number(&output["team"]),
            number(&output["clock"]),
            number(&output["flags"]),
            number(&output["dirty"]),
            expected_counts(output),
        ),
        "{}",
        case["input"]
    );
}

#[test]
fn the_registers_match_0043c200() {
    let cases = cases("registers");
    assert_eq!(cases.len(), 80);
    for case in cases {
        let input = &case["input"];
        let state = Selector84State {
            link_138: number(&input["link"]),
            clock_13c: number(&input["clock"]),
        };
        let register = number(&input["args"][1]);
        assert_eq!(
            u32::from(state.model_value(register)),
            number(&case["output"]["eax"]),
            "{input}"
        );
    }
}

#[test]
fn the_finish_counts_the_node_and_flags_an_alien_one() {
    for case in cases("finish") {
        let mut arena = arena();
        let node = node(&mut arena, &case["input"]);
        arena.finish_selector84_initialization(node).unwrap();
        assert_node(&arena, node, &case["output"], &case);
    }
}

#[test]
fn the_update_runs_a_started_clock() {
    for case in cases("update") {
        let mut arena = arena();
        let node = node(&mut arena, &case["input"]);
        arena
            .tick_selector84(node, number(&case["input"]["args"][0]))
            .unwrap();
        assert_node(&arena, node, &case["output"], &case);
    }
}

#[test]
fn the_after_load_links_the_referenced_placement() {
    let cases = cases("after_load");
    assert_eq!(cases.len(), 16);
    for case in cases {
        let input = &case["input"];
        let mut arena = arena();
        let node = node(&mut arena, input);
        let listed: Vec<(ObjectId, u32)> = input["actors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|listed| {
                let object = actor(
                    &mut arena,
                    number(&listed["team"]),
                    number(&listed["dirty"]),
                    number(&listed["placement"]) as u16,
                );
                (object, number(&listed["handle"]))
            })
            .collect();
        arena.initialize_selector84_after_load(node).unwrap();
        // Native handles are the case's; the port's are its registry's.
        let native = number(&case["output"]["link"]);
        let expected = match listed.iter().find(|(_, handle)| *handle == native) {
            Some((object, _)) => arena.actor(*object).unwrap().handle.raw(),
            None => {
                assert_eq!(native, number(&input["null"]), "{input}");
                arena.actor_handles().unwrap().null_handle().raw()
            }
        };
        assert_eq!(
            arena.selector84(node).unwrap().link_138,
            expected,
            "{input}"
        );
        for ((object, _), after) in listed
            .iter()
            .zip(case["output"]["actors"].as_array().unwrap())
        {
            assert_eq!(
                (
                    arena.actor(*object).unwrap().word_b4,
                    arena.object(*object).unwrap().base.dirty_word as u32 & 0xff
                ),
                (number(&after["team"]), number(&after["dirty"])),
                "{input}"
            );
        }
    }
}

#[test]
fn a_touch_from_an_answering_partner_captures_an_idle_node() {
    let cases = cases("contact");
    assert_eq!(cases.len(), 49);
    let mut captures = 0;
    for case in cases {
        let input = &case["input"];
        let mut arena = arena();
        let node = node(&mut arena, input);
        let partner = &input["partner"];
        let capturer = actor(&mut arena, number(&partner["team"]), 0, 0x7ffd);
        let linked = (!input["linked"].is_null()).then(|| {
            let linked = &input["linked"];
            actor(
                &mut arena,
                number(&linked["team"]),
                number(&linked["dirty"]),
                0x7ffc,
            )
        });
        // The case's link resolves to its linked actor, or to nothing.
        let link = match linked {
            Some(linked) => arena.actor(linked).unwrap().handle,
            None => arena.actor_handles().unwrap().null_handle(),
        };
        arena.selector84_mut(node).unwrap().link_138 = link.raw();
        // Selector 12's +5C answers itself; the shared table's, nothing.
        let answer = partner["answers"].as_bool().unwrap().then_some(capturer);
        arena.touch_selector84(node, answer).unwrap();
        assert_node(&arena, node, &case["output"], &case);
        if let Some(linked) = linked {
            assert_eq!(
                (
                    arena.actor(linked).unwrap().word_b4,
                    arena.object(linked).unwrap().base.dirty_word as u32 & 0xff
                ),
                (
                    number(&case["output"]["linked_team"]),
                    number(&case["output"]["linked_dirty"])
                ),
                "{input}"
            );
        }
        if number(&case["output"]["clock"]) != number(&input["clock"]) {
            captures += 1;
        }
    }
    assert_eq!(captures, 9);
}

#[test]
fn the_teardown_takes_the_node_from_its_count() {
    for case in cases("teardown") {
        let mut arena = arena();
        let node = node(&mut arena, &case["input"]);
        arena.release_selector84(node).unwrap();
        assert_node(&arena, node, &case["output"], &case);
    }
}

/// The Alien Brain (world 19) builds its six nodes: each counts on its
/// payload's team, the five with command 16 hand that team to their linked
/// placement, and a touch from a selector 12 moves a node between the counts.
#[test]
fn the_alien_brain_builds_its_six_nodes_and_counts_them() {
    let (_resident, _assets, mut world) = crate::test_worlds::offline_world_with_services(
        19,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let arena = &mut world.arena;
    let nodes: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector84(actor).is_ok())
        .collect();
    assert_eq!(nodes.len(), 6);
    let teams: Vec<u32> = nodes
        .iter()
        .map(|&node| arena.actor(node).unwrap().word_b4)
        .collect();
    let aliens = teams.iter().filter(|&&team| team == 0x12).count() as u32;
    let counts = arena.capture_node_counts();
    assert_eq!(
        (counts.aliens_547620, counts.others_547624),
        (aliens, 6 - aliens)
    );
    let null = arena.actor_handles().unwrap().null_handle();
    let mut linked = 0;
    for &node in &nodes {
        let link = ActorHandle::from_raw(arena.selector84(node).unwrap().link_138);
        if link == null {
            continue;
        }
        let target = arena.resolve_actor_handle(link).unwrap().unwrap();
        assert_eq!(
            arena.actor(target).unwrap().word_b4,
            arena.actor(node).unwrap().word_b4
        );
        linked += 1;
    }
    assert_eq!(linked, 5);
    // A touch from an answering partner on another team takes an idle node.
    let node = nodes[0];
    let capturer = arena.allocate_actor(request(&UNUSED_SINE)).unwrap();
    arena.actor_mut(capturer).unwrap().word_b4 = 1;
    let before = arena.capture_node_counts();
    let old = arena.actor(node).unwrap().word_b4;
    arena.touch_selector84(node, Some(capturer)).unwrap();
    let after = arena.capture_node_counts();
    assert_eq!(arena.actor(node).unwrap().word_b4, 1);
    assert_eq!(arena.selector84(node).unwrap().clock_13c, 1);
    if old == 0x12 {
        assert_eq!(after.aliens_547620, before.aliens_547620 - 1);
        assert_eq!(after.others_547624, before.others_547624 + 1);
    }
}
