//! The Prison Unit against `check_selector17.py`: the update 0046E230 with
//! an empty hold and actor list, its query 0046E200, its filter 0046E450
//! and its message 0046E4A0.

use serde_json::Value;

use super::*;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs, FamilyUpdateEffect,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector17_prison_native.json"
    ))
    .unwrap()
}

fn word(value: &Value) -> u32 {
    value
        .as_u64()
        .map(|value| value as u32)
        .unwrap_or_else(|| value.as_i64().unwrap() as u32)
}

/// A unit with the receipt's fields; its own handle is the receipt's.
fn prison(handles: &Value, input: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(word(&handles["null"])),
    })
    .unwrap();
    let prison = arena
        .allocate_selector17(
            ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &UNUSED_SINE,
                },
            },
            PRISON_RADIUS,
        )
        .unwrap();
    arena.owned_mut(prison).unwrap().state.base.position_words = Some([0x1000, 0, 0x1000]);
    let state = arena.selector17_mut(prison).unwrap();
    state.radius_138 = word(&input["radius"]);
    state.arming_13c = word(&input["arming"]) as i32;
    state.scan_140 = word(&input["scan"]) as i32;
    state.armed_154 = word(&input["armed"]) as u8;
    state.charge_156 = word(&input["charge"]) as u16;
    state.team_158 = Some(word(&input["team"]));
    state.kind_15c = word(&input["kind"]) as u16;
    state.holding_15e = word(&input["holding"]) as u8;
    (arena, prison)
}

/// The world's RNG and one team's task, as 00482C60 keeps it.
struct Host {
    rng: u32,
    null: ActorHandle,
    task: (u32, ActorHandle),
}

impl FamilyUpdateHost for Host {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err("no status effect".into())
    }
}

impl PrisonHost for Host {
    fn take_prisoner(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        Err("the empty actor list finds no one".into())
    }
    fn task_source(&mut self, team: u32, source: ActorHandle, enable: bool) -> Result<(), String> {
        assert_eq!(team, self.task.0);
        if source == self.null {
            return Ok(());
        }
        if enable {
            self.task.1 = source;
        } else if self.task.1 == source {
            self.task.1 = self.null;
        }
        Ok(())
    }
}

#[test]
fn the_update_matches_0046e230() {
    let fixture = fixture();
    let handles = &fixture["handles"];
    let null = ActorHandle::from_raw(word(&handles["null"]));
    let updates = fixture["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 50);
    for case in updates {
        let input = &case["input"];
        let output = &case["output"];
        let (mut arena, prison) = prison(handles, input);
        // The receipt's own handle stands for the unit's.
        let own = arena.actor(prison).unwrap().handle;
        let source = word(&input["task_source"]);
        let task_source = if source == word(&handles["own"]) {
            own
        } else {
            ActorHandle::from_raw(source)
        };
        let mut host = Host {
            rng: word(&input["rng"]),
            null,
            task: (word(&input["team"]), task_source),
        };
        arena
            .tick_selector17(prison, word(&input["delta"]), None, &mut host)
            .unwrap();
        let state = arena.selector17(prison).unwrap();
        assert_eq!(state.arming_13c as u32, word(&output["arming"]), "{case}");
        assert_eq!(state.scan_140 as u32, word(&output["scan"]), "{case}");
        assert_eq!(u32::from(state.armed_154), word(&output["armed"]), "{case}");
        assert_eq!(
            u32::from(state.charge_156),
            word(&output["charge"]),
            "{case}"
        );
        assert_eq!(u32::from(state.kind_15c), word(&output["kind"]), "{case}");
        assert_eq!(
            u32::from(state.holding_15e),
            word(&output["holding"]),
            "{case}"
        );
        assert_eq!(state.hold_144.handles, [null], "{case}");
        assert_eq!(host.rng, word(&output["rng"]), "{case}");
        let task = if host.task.1 == own {
            word(&handles["own"])
        } else {
            host.task.1.raw()
        };
        assert_eq!(task, word(&output["task_source"]), "{case}");
    }
}

#[test]
fn the_query_filter_and_message_match_0046e200_0046e450_and_0046e4a0() {
    let fixture = fixture();
    let handles = &fixture["handles"];
    for case in fixture["queries"].as_array().unwrap() {
        let (arena, prison) = prison(handles, &case["input"]["unit"]);
        let value = selector17_value(
            arena.selector17(prison).unwrap(),
            word(&case["input"]["query"]),
        );
        assert_eq!(u32::from(value), word(&case["output"]), "{case}");
    }
    for case in fixture["scores"].as_array().unwrap() {
        let input = &case["input"];
        let team = input["view"]
            .as_bool()
            .unwrap()
            .then(|| word(&input["team"]));
        assert_eq!(
            prison_score(team, word(&input["squared"])),
            word(&case["output"]),
            "{case}"
        );
    }
    for case in fixture["messages"].as_array().unwrap() {
        let (mut arena, prison) = prison(handles, &case["input"]["unit"]);
        arena
            .selector17_message(prison, word(&case["input"]["code"]) as i32)
            .unwrap();
        let state = arena.selector17(prison).unwrap();
        let expected = &case["output"]["state"];
        assert_eq!(state.arming_13c as u32, word(&expected["arming"]), "{case}");
        assert_eq!(
            u32::from(state.armed_154),
            word(&expected["armed"]),
            "{case}"
        );
        assert_eq!(u32::from(state.kind_15c), word(&expected["kind"]), "{case}");
        assert_eq!(
            u32::from(state.holding_15e),
            word(&expected["holding"]),
            "{case}"
        );
        assert_eq!(
            u32::from(state.charge_156),
            word(&expected["charge"]),
            "{case}"
        );
    }
}
