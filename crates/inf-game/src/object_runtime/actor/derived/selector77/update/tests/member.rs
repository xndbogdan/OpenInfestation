//! Selector 80's update against `check_selector80.py`: 00409070 over selector
//! 77's 00407CA0 (the status alone in state 2, the idle and the member's +D4
//! 00409010 in state 3). The ground 00450A30 is supplied; the retirement
//! 0047CE90 and the disposal 00479FA0 are recorded where they happen.

use super::*;
use crate::object_runtime::{NativeTarget, ObjectList};
use inf_formats::native_basis::NativeBasis;
use serde_json::{json, Value};
use std::cell::RefCell;

struct MemberHost<'a> {
    rng: u32,
    sine: &'a [u16; 4096],
    liquid: i16,
    ground: i16,
    events: RefCell<Vec<Value>>,
}

impl FamilyUpdateHost for MemberHost<'_> {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(NULL)
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        if effect == FamilyUpdateEffect::Retire {
            self.events.borrow_mut().push(json!({"kind": "retire"}));
            return Ok(());
        }
        NativeFamilyUpdateHost {
            process_rng: &mut self.rng,
            sine_quarter: self.sine,
        }
        .effect(arena, actor, effect)
        .map_err(|_| "unexpected native status boundary")
    }
}

impl Selector77UpdateHost for MemberHost<'_> {
    fn liquid_level(&self) -> Option<i16> {
        Some(self.liquid)
    }
    fn ground_height(&self, [x, z]: [u16; 2]) -> Option<i16> {
        self.events
            .borrow_mut()
            .push(json!({"kind": "ground", "x": x, "z": z, "third": 0}));
        Some(self.ground)
    }
}

fn unsigned(input: &Value, key: &str) -> u32 {
    input[key].as_i64().unwrap() as u32
}

fn words<const N: usize>(value: &Value) -> [u16; N] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn member(input: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let actor = arena
        .allocate_selector80_actor(construction(Some(words(&input["position"]))), &mut 0)
        .unwrap();
    assert_eq!(
        arena.selector80(actor).unwrap(),
        crate::object_runtime::Selector80State {
            rise_1bc: 1,
            up_1b8: 0,
            idle_1b4: 0,
        }
    );
    if input["target"] == true {
        let target =
            NativeTarget::FixedPosition(arena.fixed_position_target(Some([0; 3])).unwrap());
        let reference = arena.import_fixture_target(target);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    }
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = unsigned(input, "flags");
    base.word_28 = unsigned(input, "radius") as u16;
    let a = arena.actor_mut(actor).unwrap();
    a.word_c4 = unsigned(input, "lifecycle");
    a.word_c8 = 0;
    a.word_cc = 0;
    a.rotating.basis = NativeBasis { words: [0; 10] };
    let eye = arena.selector77_update_state_mut(actor).unwrap();
    eye.words_184_190 = words(&input["pose"]);
    eye.position_192 = Some(words(&input["origin"]));
    eye.word_198 = 3_000_000;
    eye.word_19c = Some(unsigned(input, "phase") as u16);
    eye.word_1a0 = Some(0x12345678);
    eye.word_1a4 = unsigned(input, "remaining");
    eye.word_1a8 = unsigned(input, "speed");
    eye.word_1ac = Some(0);
    let state = arena.selector80_mut(actor).unwrap();
    state.idle_1b4 = unsigned(input, "idle");
    state.up_1b8 = unsigned(input, "up");
    state.rise_1bc = unsigned(input, "rise") as i32;
    (arena, actor)
}

#[test]
fn the_member_update_matches_00409070() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../../tests/fixtures/selector80_member_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let sine = assets.sine_table().values;
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 61);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let (mut arena, actor) = member(input);
        let level = (unsigned(input, "liquid_base") as i32 >> unsigned(input, "shift")) as i16;
        let mut host = MemberHost {
            rng: unsigned(input, "rng"),
            sine: &sine,
            liquid: level,
            ground: unsigned(input, "ground") as i16,
            events: RefCell::new(Vec::new()),
        };
        let mut request = request(unsigned(input, "delta"));
        request.sine_quarter = &sine;
        arena
            .tick_selector80(actor, request, &mut host)
            .unwrap_or_else(|error| panic!("case {index}: {error:?}"));
        let eye = arena.selector77_actor(actor).unwrap();
        let state = arena.selector80(actor).unwrap();
        let output = json!({
            "y": arena.object(actor).unwrap().base.position_words.unwrap()[1],
            "idle": state.idle_1b4,
            "up": state.up_1b8,
            "rise": state.rise_1bc as u32,
            "remaining": eye.word_1a4,
            "speed": eye.word_1a8,
            "extension": eye.words_184_190[6],
            "phase": eye.word_19c.unwrap(),
            "rng": host.rng,
        });
        assert_eq!(output, case["output"], "case {index}");
        let disposed = arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|entry| entry.object == actor);
        let expected = case["events"].as_array().unwrap();
        assert_eq!(
            disposed,
            expected.iter().any(|event| event["kind"] == "dispose"),
            "case {index}"
        );
        let others: Vec<_> = expected
            .iter()
            .filter(|event| event["kind"] != "dispose")
            .map(|event| match event["kind"].as_str() {
                Some("ground") => json!({
                    "kind": "ground",
                    "x": event["x"].as_u64().unwrap() & 0xffff,
                    "z": event["z"].as_u64().unwrap() & 0xffff,
                    "third": event["third"],
                }),
                _ => event.clone(),
            })
            .collect();
        assert_eq!(host.events.into_inner(), others, "case {index}");
    }
}
