//! Service-boundary consumers for original 004678A0/004679C0 receipts.
//! Refresh, burst and state-change mutations are supplied fixture inputs.
//! Native descriptor-thunk cases exercise the same intrinsic request here;
//! this suite does not claim production descriptor dispatch or particle birth.

use super::*;
use crate::object_runtime::ActorHandle;
use serde_json::{json, Value};
use std::cell::Cell;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn position(value: &Value) -> [u16; 3] {
    serde_json::from_value(value.clone()).unwrap()
}

fn snapshot(arena: &ObjectArena, actor: ObjectId) -> Value {
    let base = &arena.object(actor).unwrap().base;
    let shared = arena.actor(actor).unwrap();
    let derived = arena.derived_actor(actor).unwrap();
    json!({
        "size":base.word_28, "flags":base.flags,
        "position":base.position_words.unwrap(), "dirty":base.dirty_word,
        "model":shared.rotating.visual.model_word, "velocity":shared.words_8e_92,
        "angular_y":shared.words_94_9c[1], "state":shared.word_c4,
        "scales":[derived.word_16c.unwrap(),derived.word_170.unwrap()],
        "elapsed":arena.family428650_actor(actor).unwrap().words_188_19c[0].unwrap()
    })
}

fn supply(
    arena: &mut ObjectArena,
    actor: ObjectId,
    values: &Value,
    rng: &mut u32,
    process_null: &Cell<u32>,
) {
    for (field, value) in values.as_object().unwrap() {
        match field.as_str() {
            "size" => arena.owned_mut(actor).unwrap().state.base.word_28 = number(value) as u16,
            "flags" => arena.owned_mut(actor).unwrap().state.base.flags = number(value),
            "position" => {
                arena.owned_mut(actor).unwrap().state.base.position_words = Some(position(value))
            }
            "dirty" => arena.owned_mut(actor).unwrap().state.base.dirty_word = number(value) as u16,
            "model" => {
                arena.actor_mut(actor).unwrap().rotating.visual.model_word = number(value) as u16
            }
            "velocity" => arena.actor_mut(actor).unwrap().words_8e_92 = position(value),
            "angular_y" => arena.actor_mut(actor).unwrap().words_94_9c[1] = number(value),
            "state" => arena.actor_mut(actor).unwrap().word_c4 = number(value),
            "scales" => {
                let derived = arena.derived_actor_mut(actor).unwrap();
                derived.word_16c = Some(number(&value[0]));
                derived.word_170 = Some(number(&value[1]));
            }
            "elapsed" => {
                arena.family428650_actor_mut(actor).unwrap().words_188_19c[0] = Some(number(value))
            }
            "rng" => *rng = number(value),
            "process_null" => process_null.set(number(value)),
            other => panic!("unowned supplied field {other}"),
        }
    }
}

struct Host<'a> {
    input: &'a Value,
    events: Vec<Value>,
    process_null: Cell<u32>,
    null_reads: Cell<usize>,
}

impl Host<'_> {
    fn stop(&self, name: &'static str) -> Result<(), &'static str> {
        if self.input["stop"].as_str() == Some(name) {
            Err(name)
        } else {
            Ok(())
        }
    }

    fn mutate(&self, arena: &mut ObjectArena, actor: ObjectId, name: &str, rng: &mut u32) {
        supply(arena, actor, &self.input[name], rng, &self.process_null);
    }
}

impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.null_reads.set(self.null_reads.get() + 1);
        Some(ActorHandle::from_raw(self.process_null.get()))
    }
}

impl Selector12DyingHost for Host<'_> {
    type Error = &'static str;

    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind":"refresh","actor":snapshot(arena,actor),"rng":*rng}));
        self.stop("refresh")?;
        self.mutate(arena, actor, "after_refresh", rng);
        Ok(())
    }

    fn birth_burst(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({
            "kind":"burst", "actor":snapshot(arena,actor), "rng":*rng,
            "packet":{
                "count":burst.debris_count, "position":burst.position,
                "radius":burst.placement_radius,"central_type":burst.central_type,
                "debris_types":burst.debris_types,"speed":burst.speed,"flags":burst.flags
            }
        }));
        self.stop("burst")?;
        self.mutate(arena, actor, "after_burst", rng);
        Ok(())
    }

    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({
            "kind":"state_zero", "actor":snapshot(arena,actor), "rng":*rng,
            "argument":request.argument,"source":request.source.raw(),
            "target_state":request.target_state
        }));
        self.stop("state_zero")?;
        self.mutate(arena, actor, "after_state", rng);
        Ok(())
    }
}

fn check_receipts(bytes: &str, count: usize, change: bool) {
    let fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(number(&fixture["provenance"]["fpcw"]), 0x027f);
    assert_eq!(fixture["provenance"]["code_patched"], false);
    assert_eq!(fixture["provenance"]["hooks"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    for (index, case) in cases.iter().enumerate() {
        let (mut arena, actor) = super::tests::fixture();
        let input = &case["input"];
        let mut host = Host {
            input,
            events: Vec::new(),
            process_null: Cell::new(number(&input["process_null"])),
            null_reads: Cell::new(0),
        };
        let mut rng = number(&input["rng"]);
        supply(
            &mut arena,
            actor,
            &case["before"],
            &mut rng,
            &host.process_null,
        );
        assert_eq!(
            snapshot(&arena, actor),
            case["before"],
            "case {index} initial"
        );
        let request = if change {
            Selector12DyingRequest::Change {
                entering: number(&input["entering"]),
            }
        } else {
            Selector12DyingRequest::Update
        };
        let result = arena.run_selector12_dying(actor, request, &mut host, &mut rng);
        let expected_result = match input["stop"].as_str() {
            Some("refresh") => Err(Selector12DyingError::Refresh("refresh")),
            Some("burst") => Err(Selector12DyingError::Burst("burst")),
            Some("state_zero") => Err(Selector12DyingError::StateChange("state_zero")),
            None => Ok(()),
            other => panic!("unowned stop {other:?}"),
        };
        assert_eq!(result, expected_result, "case {index} result");
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case {index} return"
        );
        // The Rust RNG helper is not intercepted. Its complete effects are
        // checked at the first service boundary and every subsequent boundary.
        let events: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] != "rng")
            .cloned()
            .collect();
        assert_eq!(host.events, events, "case {index} service order and prefix");
        assert_eq!(
            snapshot(&arena, actor),
            case["actor"],
            "case {index} final actor"
        );
        assert_eq!(rng, number(&case["rng_after"]), "case {index} final RNG");
        let state_call = events.iter().any(|event| event["kind"] == "state_zero");
        assert_eq!(
            host.null_reads.get(),
            usize::from(state_call),
            "case {index} lazy NULL"
        );
    }
}

#[test]
fn selector12_dying_change_matches_28_native_service_receipts() {
    check_receipts(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/selector12_dying_entry_native.json"
        )),
        28,
        true,
    );
}

#[test]
fn selector12_dying_update_matches_19_native_service_receipts() {
    check_receipts(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/selector12_dying_update_native.json"
        )),
        19,
        false,
    );
}

#[test]
fn selector12_dying_descriptor_vtable_and_complete_native_bodies_are_guarded() {
    use crate::object_runtime::FamilyBehaviorProfile;
    use sha2::{Digest, Sha256};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    for (offset, address) in [
        (0x120, 0x0046_78a0),
        (0x11c, 0x0046_79c0),
        (0xb0, 0x0040_26e0),
    ] {
        assert_eq!(executable.word(0x0052_7058 + offset).unwrap(), address);
    }
    for behavior_present in [false, true] {
        let descriptor = FamilyBehaviorProfile::Selector12 { behavior_present }
            .descriptor(1)
            .unwrap();
        assert_eq!(descriptor.object_address, 0x0054_ecd8);
        assert_eq!(descriptor.vtable_address, 0x0052_6fd8);
        assert_eq!(descriptor.change_address, 0x0043_fe40);
        assert_eq!(descriptor.update_address, 0x0043_5240);
        assert_eq!(
            executable.word(descriptor.vtable_address + 8).unwrap(),
            descriptor.change_address
        );
        assert_eq!(
            executable.word(descriptor.vtable_address + 12).unwrap(),
            descriptor.update_address
        );
    }
    // Complete original text bodies, including RET cleanup. The authenticated
    // retail PE maps these text offsets as VA minus 00400000.
    for (start, end, expected) in [
        (
            0x0046_78a0usize,
            0x0046_79bd,
            "6d1461c761c57dc96e100cbff0eaae07c19d01c68c9c370c692831c9a615fc16",
        ),
        (
            0x0046_79c0,
            0x0046_79e7,
            "eab764d455e3b38de62a2bb6e23ef15d0581a3c91af27b9f5deeda76d330cabc",
        ),
        (
            0x0043_fe40,
            0x0043_fe54,
            "014a03d45a429130536719278e403d557bcdd7f758ae3777bb36ae5686f5cbb4",
        ),
        (
            0x0043_5240,
            0x0043_5254,
            "53860d6c90b2fb1e3b3992fe8b667a5ef2d47824e60d1db99334ffe0ad1152ff",
        ),
        (
            0x0040_26e0,
            0x0040_26e8,
            "7dd50d677f4f80995813ca4d17ae2aae81bd4eead10103fb0acad583254fbe4c",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x0040_0000..end - 0x0040_0000])
            ),
            expected,
            "body {start:08X}"
        );
    }
}
