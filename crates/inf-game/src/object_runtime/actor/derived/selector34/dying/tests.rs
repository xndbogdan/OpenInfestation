//! 00434130/004341A0 against supplied actors: the entry's target and status
//! words, the 15 s exit, the status visual's RNG and trail, and the dive.

use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionRequest, ActorFollowTarget, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, DerivedActorConstructionRequest, FamilyUpdateEffect, FamilyUpdateHost,
        FixtureActorServiceAllocation,
    },
};
use serde_json::{json, Value};

const SINE_ZERO_FIXTURE: [u16; 4096] = [0; 4096];

fn construction(position: [u16; 3]) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE_ZERO_FIXTURE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn get(case: &Value, key: &str, default: i64) -> i64 {
    case[key].as_i64().unwrap_or(default)
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_u64().unwrap() as u16)
}

/// The status visual's own boundaries: its process RNG and trail birth.
struct Visual<'a> {
    rng: &'a mut u32,
    events: &'a mut Vec<Value>,
}

impl FamilyUpdateHost for Visual<'_> {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        panic!("0047DFD0 reads no process NULL")
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        let FamilyUpdateEffect::StatusTrail {
            particle_type,
            position,
            velocity,
            flags,
        } = effect
        else {
            return Err("0047DFD0 reaches only its trail birth");
        };
        self.events
            .push(json!({"kind": "trail", "type": particle_type,
            "position": position, "velocity": velocity, "flags": flags}));
        Ok(())
    }
}

struct Host {
    process_null: u32,
    load_factor: i32,
    events: Vec<Value>,
}

impl TargetNullHandleSource for Host {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(self.process_null))
    }
}

impl Selector34DyingHost for Host {
    type Error = String;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector34DyingEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        match effect {
            Selector34DyingEffect::StatusVisual { microseconds } => arena
                .tick_status_visual(
                    actor,
                    microseconds,
                    Some(self.load_factor),
                    &mut Visual {
                        rng,
                        events: &mut self.events,
                    },
                )
                .map_err(|error| error.to_string()),
            Selector34DyingEffect::ChangeState(request) => {
                self.events.push(json!({"kind": "state",
                    "state": request.target_state as u32, "source": request.source.raw(),
                    "argument": request.argument}));
                Ok(())
            }
        }
    }
}

fn old_target(spec: &Value) -> NativeTarget {
    let handle = ActorHandle::from_raw(spec["handle"].as_u64().unwrap() as u32);
    let focus = words(&spec["focus"]);
    match spec["kind"].as_u64().unwrap() {
        8 => NativeTarget::FixedPosition(FixedPositionTarget::from_point(handle, Some(focus))),
        2 => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: Some(focus),
            word_10: 2,
            focus_14: Some(focus),
            velocity_1a: None,
        }),
        other => panic!("target kind {other}"),
    }
}

/// 00434130 and 004341A0 over 24 supplied actors: entry over old point and
/// follow targets, a NULL allocation, the argument's low byte, the 15 s exit
/// and its edge, the kind-gated dive, and the status visual's skip, timer,
/// size, velocity and step branches.
#[test]
fn selector34_dying_matches_native_entry_and_update() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector34_dying_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mut trails = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let position = words(&case["position"]);
        let actor = arena
            .allocate_selector34_actor(construction(position), &mut 0)
            .unwrap();
        let old = case["target"]
            .is_object()
            .then(|| arena.import_fixture_target(old_target(&case["target"])));
        arena.derived_actor_mut(actor).unwrap().target_148 = old;
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.word_28 = get(case, "size", 120) as u16;
        base.position_words = Some(position);
        let state = arena.actor_mut(actor).unwrap();
        state.words_8e_92 = case["velocity"]
            .is_array()
            .then(|| words(&case["velocity"]))
            .unwrap_or([0; 3]);
        state.word_c4 = 1;
        state.word_c8 = get(case, "word_c8", 0) as u16;
        state.word_ca = Some(get(case, "word_ca", 0) as u16);
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_188_19c[0] = Some(get(case, "elapsed", 0) as u32);
        family.word_228 = 0x53_1244;
        let selector34 = arena.selector34_actor_mut(actor).unwrap();
        selector34.word_230 = 500;
        selector34.byte_238 = 1;
        arena.owned_mut(actor).unwrap().state.base.dirty_word = 0;
        if !case["allocate"].as_bool().unwrap_or(true) {
            arena
                .actor_service_heap_mut()
                .script([FixtureActorServiceAllocation::Null]);
        }
        let attempts = arena.actor_service_heap().attempts().len();
        let mut host = Host {
            process_null: get(case, "process_null", 0x77) as u32,
            load_factor: get(case, "load_factor", 0x10000) as i32,
            events: Vec::new(),
        };
        let mut rng = get(case, "rng", 0x1234_5678) as u32;
        let request = match case["mode"].as_str().unwrap() {
            "entry" => Selector34DyingRequest::Change {
                entering: get(case, "entering", 0) as u32,
            },
            _ => Selector34DyingRequest::Update {
                microseconds: get(case, "delta", 33333) as u32,
            },
        };
        arena
            .run_selector34_dying(actor, request, &mut host, &mut rng)
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        // Each port allocation attempt is native operator new's 0x14 bytes.
        let mut events: Vec<Value> = (attempts..arena.actor_service_heap().attempts().len())
            .map(|_| json!({"kind": "new", "size": 20}))
            .collect();
        events.extend(host.events);
        assert_eq!(json!(events), case["events"], "{name}");
        trails += events
            .iter()
            .filter(|event| event["kind"] == "trail")
            .count();

        let target = arena
            .derived_actor(actor)
            .unwrap()
            .target_148
            .map(|reference| {
                let owner = if Some(reference) == old { "old" } else { "new" };
                match arena.native_target(reference).unwrap() {
                    NativeTarget::FixedPosition(point) => json!({"owner": owner,
                        "table": 0x51_f154, "handle": point.handle_04.raw(),
                        "focus": point.focus_08.unwrap(), "word_10": point.word_10}),
                    NativeTarget::ActorFollow(follow) => json!({"owner": owner,
                        "table": 0x52_8038, "handle": follow.handle_04.raw(),
                        "focus": follow.focus_08.unwrap(), "word_10": follow.word_10}),
                    other => panic!("{name}: {other:?}"),
                }
            });
        let family = arena.family428650_actor(actor).unwrap();
        let selector34 = arena.selector34_actor(actor).unwrap();
        let object = arena.actor(actor).unwrap();
        let port = json!({
            "target": target,
            "word_188": family.words_188_19c[0].unwrap(),
            "word_228": family.word_228,
            "word_230": selector34.word_230,
            "byte_238": selector34.byte_238,
            "word_c8": object.word_c8,
            "word_ca": object.word_ca.unwrap(),
            "dirty": arena.object(actor).unwrap().base.dirty_word,
            "rng": rng,
        });
        assert_eq!(&port, &case["after"], "{name}");
    }
    assert_eq!(cases.len(), 24);
    assert_eq!(
        trails,
        fixture["coverage"]["trails"].as_u64().unwrap() as usize
    );
}
