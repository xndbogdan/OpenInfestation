use super::*;
use crate::object_runtime::actor::derived::FamilyHelper482ff0;
use crate::object_runtime::{
    ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery, FamilyHelperHistoryRecord,
    FixedPositionTarget, NativeTarget,
};
use serde_json::Value;

#[path = "geometry.rs"]
mod geometry;
#[path = "ray.rs"]
mod ray;

fn word(value: &Value) -> u32 {
    value
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| value.as_i64().unwrap() as u32)
}

#[test]
fn combat_source_notification_matches_all_native_429c70_guards() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_source_notification_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let (mut arena, _, _, _) = setup();
        let source = arena
            .allocate_family429e20_actor(construction([0; 3]), &mut 1)
            .unwrap();
        let target = arena.allocate_actor(construction([0; 3]).actor).unwrap();
        arena.actor_mut(target).unwrap().handle = ActorHandle::from_raw(word(&input["source"]));
        arena.family429e20_actor_mut(source).unwrap().word_238 = word(&input["timer"]) as u16;
        if input["parameter"].as_bool().unwrap() {
            let reference =
                arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
                    handle_04: ActorHandle::from_raw(word(&input["tracked_source"])),
                    focus_08: None,
                    word_10: 0,
                }));
            arena.derived_actor_mut(source).unwrap().target_148 = Some(reference);
        }
        if input["behavior"].as_bool().unwrap() {
            arena.family428650_actor_mut(source).unwrap().helper_218 =
                Some(Box::new(FamilyHelper482ff0 {
                    words_00_04: [None, Some(word(&input["behavior_word"]))],
                    word_08: 0,
                    records: [FamilyHelperHistoryRecord::UNKNOWN; 4],
                }));
        }
        arena.notify_family_projectile_hit(source, target).unwrap();
        assert_eq!(
            arena.family429e20_actor(source).unwrap().word_238,
            word(&case["output"]["timer"]) as u16,
            "case{index}"
        );
        if let Some(helper) = arena
            .family428650_actor(source)
            .unwrap()
            .helper_218
            .as_ref()
        {
            assert_eq!(
                helper.words_00_04[1],
                Some(word(&case["output"]["behavior_word"])),
                "case{index}"
            );
        }
    }
}

/// 0047C6E0's +108 dispatch is one binding for every damage host:
/// Family429E20 runs 00429C70, while the Base, Selector34 and Selector35
/// tables bind RET4 00466D40 and leave a matching helper untouched.
#[test]
fn source_component_108_follows_the_final_family_tables() {
    let (mut arena, base, _, _) = setup();
    let target = arena.allocate_actor(construction([0; 3]).actor).unwrap();
    let handle = arena.actor(target).unwrap().handle;
    let tracked = arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: handle,
        focus_08: None,
        word_10: 0,
    }));
    let family429e20 = arena
        .allocate_family429e20_actor(construction([0; 3]), &mut 1)
        .unwrap();
    let selector34 = arena
        .allocate_selector34_actor(construction([0; 3]), &mut 1)
        .unwrap();
    let selector35 = arena
        .allocate_selector35_actor(construction([0; 3]), &mut 1)
        .unwrap();
    for source in [base, family429e20, selector34, selector35] {
        arena.derived_actor_mut(source).unwrap().target_148 = Some(tracked);
        arena.family428650_actor_mut(source).unwrap().helper_218 =
            Some(Box::new(FamilyHelper482ff0 {
                words_00_04: [None, Some(7)],
                word_08: 0,
                records: [FamilyHelperHistoryRecord::UNKNOWN; 4],
            }));
        arena.source_component_108(source, target).unwrap();
        let helper = arena
            .family428650_actor(source)
            .unwrap()
            .helper_218
            .as_ref()
            .unwrap()
            .words_00_04[1];
        let notified = source == family429e20;
        assert_eq!(helper, Some(if notified { 0 } else { 7 }), "{source:?}");
    }
    assert_eq!(
        arena.family429e20_actor(family429e20).unwrap().word_238,
        3000
    );
    assert_eq!(
        arena.source_component_108(target, base),
        Err(ObjectError::NotDerivedActor(target))
    );
}

struct DeathHost {
    actor: ObjectId,
    neighbor: ObjectId,
    fail: bool,
    observed: Vec<(i8, u32, u16, u32)>,
}
impl ActorStateChangeHost for DeathHost {
    type Error = &'static str;
    type AttributionOwner = ();
    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        panic!("428140 has no lifecycle query")
    }
    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), Self::Error> {
        panic!("428140 does not repeat attribution")
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        let ActorStateChangeEffect::RecordPlacementDeath { index } = effect else {
            panic!("unexpected family death effect")
        };
        let base = &arena.object(self.actor).unwrap().base;
        self.observed.push((
            index,
            base.flags,
            base.dirty_word,
            arena.family428650_actor(self.neighbor).unwrap().count_1a0,
        ));
        if self.fail {
            Err("placement death")
        } else {
            Ok(())
        }
    }
}

#[test]
fn combat_family_death_matches_native_gates_and_retains_broadcast_on_placement_failure() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_family_death_native.json"
    ))
    .unwrap();
    let mut covered = 0;
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        //Native lookup/broadcast stubs can stop or mutate the actor, while
        //these concrete owners have no host boundary there. Their prefixes
        //remain evidence; the concrete placement failure is exercised here.
        if input["mutate_placement"] == true
            || matches!(input["stop_at"].as_str(), Some("resolve" | "broadcast"))
        {
            continue;
        }
        covered += 1;
        let (mut arena, actor, _, _) = setup();
        arena.actor_mut(actor).unwrap().word_b4 = word(&input["category"]);
        arena.actor_mut(actor).unwrap().byte_78 = word(&input["placement"]) as u8;
        arena.derived_actor_mut(actor).unwrap().word_164 = 80;
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.flags = word(&input["flags"]);
        base.dirty_word = word(&input["dirty"]) as u16;
        base.resource_word = 17;
        let source = if input["source_exists"].as_bool().unwrap() {
            let source = arena.allocate_actor(construction([1000; 3]).actor).unwrap();
            arena.actor_mut(source).unwrap().word_b4 = word(&input["source_category"]);
            arena.actor(source).unwrap().handle
        } else {
            ActorHandle::from_raw(0)
        };
        let neighbor = add_neighbor(&mut arena, [1, 0, 0], word(&input["category"]), 17, 0x20);
        let broadcast = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "broadcast");
        let mut host = DeathHost {
            actor,
            neighbor,
            fail: input["stop_at"] == "placement_death",
            observed: vec![],
        };
        let result = arena.retire_family_actor(actor, source, &mut host);
        assert_eq!(result.is_err(), host.fail, "case{index}: {result:?}");
        assert_eq!(
            host.observed,
            vec![(
                word(&input["placement"]) as u8 as i8,
                word(&case["flags"]),
                word(&case["dirty"]) as u16,
                u32::from(broadcast.is_some())
            )],
            "case{index}"
        );
        let targets = arena.family428650_actor(neighbor).unwrap();
        if let Some(event) = broadcast {
            assert_eq!(targets.entries_1a4[0].handle, source);
            assert_eq!(targets.entries_1a4[0].word_08, word(&event["args"][1]));
            assert_eq!(targets.entries_1a4[0].word_04, word(&event["args"][2]));
        }
    }
    assert_eq!(covered, 7);
}
