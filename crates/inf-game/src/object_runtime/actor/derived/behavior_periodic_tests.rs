use std::path::Path;

use inf_formats::FrontendExecutable;
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

const SINE: [u16; 4096] = [0; 4096];
const PROFILES: [FamilyBehaviorProfile; 7] = [
    FamilyBehaviorProfile::Family4236d0 { mode: 0 },
    FamilyBehaviorProfile::Family4236d0 { mode: 1 },
    FamilyBehaviorProfile::Family4236d0 { mode: 2 },
    FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    },
    FamilyBehaviorProfile::Selector12 {
        behavior_present: true,
    },
    FamilyBehaviorProfile::Selector35,
    FamilyBehaviorProfile::Selector34,
];

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 20, 30]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    arena.family428650_actor_mut(actor).unwrap().word_21c = 100;
    (arena, actor)
}

#[derive(Default)]
struct Host {
    events: Vec<Value>,
    result: u32,
    null_factory: bool,
    stop_at: Option<String>,
    retained_input: bool,
}

impl Host {
    fn event(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        mut event: Value,
        increment: u32,
    ) -> Result<(), &'static str> {
        let state = arena.family428650_actor_mut(actor).unwrap();
        let before = state.word_21c;
        state.word_21c = before.wrapping_add(increment);
        event["before"] = json!(before);
        event["after"] = json!(state.word_21c);
        let failed = self.stop_at.as_deref() == event["kind"].as_str();
        self.events.push(event);
        if failed {
            Err("required periodic service")
        } else {
            Ok(())
        }
    }
}

impl FamilyBehaviorHost for Host {
    type Error = &'static str;
    // The host owns a temporary input; this token contains no native address.
    type PeriodicInput = ObjectId;

    fn random(&mut self) -> Result<u32, Self::Error> {
        self.events.push(json!({"kind":"random"}));
        Ok(2)
    }
    fn periodic_input_factory(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        factory: u32,
    ) -> Result<Option<Self::PeriodicInput>, Self::Error> {
        self.retained_input = !self.null_factory;
        self.event(
            arena,
            actor,
            json!({"kind":"factory", "factory":factory, "allocated":!self.null_factory}),
            1,
        )?;
        Ok(self.retained_input.then_some(actor))
    }
    fn periodic_probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyPeriodicProbeRequest,
        input: Option<Self::PeriodicInput>,
    ) -> Result<u32, Self::Error> {
        assert_eq!(input.is_some(), request.record.input_factory.is_some());
        if let Some(input) = input {
            assert!(self.retained_input);
            assert_eq!(input, actor);
        }
        self.event(
            arena,
            actor,
            json!({"kind":"probe", "callback":request.record.callback,
                   "actor_offset":request.record.actor_offset,
                   "input_present":input.is_some(), "result":self.result}),
            16,
        )?;
        Ok(self.result)
    }
    fn release_periodic_input(
        &mut self,
        arena: &mut ObjectArena,
        input: Self::PeriodicInput,
    ) -> Result<(), Self::Error> {
        assert!(self.retained_input);
        self.event(arena, input, json!({"kind":"release"}), 256)?;
        self.retained_input = false;
        Ok(())
    }
    fn score(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyBehaviorProfile,
        _: i32,
        _: bool,
        _: i32,
    ) -> Result<i32, Self::Error> {
        Err("score unowned")
    }
    fn candidate_bias(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i32, Self::Error> {
        Err("bias unowned")
    }
    fn change(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyBehaviorProfile,
        _: i32,
        _: bool,
    ) -> Result<(), Self::Error> {
        Err("descriptor change unowned")
    }
    fn update(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyBehaviorProfile,
        _: i32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("descriptor update unowned")
    }
    fn occasional_sound(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u16,
    ) -> Result<(), Self::Error> {
        Err("sound unowned")
    }
}

fn receipt() -> Value {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/family_behavior_periodic_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    receipt
}

#[test]
fn periodic_records_match_original_executable_for_every_profile() {
    let bytes =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
            .unwrap();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    for (profile, source) in PROFILES
        .into_iter()
        .zip(receipt()["profiles"].as_array().unwrap())
    {
        let (table, count) = profile.periodic_table();
        assert_eq!(u64::from(table), source["table"].as_u64().unwrap());
        assert_eq!(i64::from(count), source["count"].as_i64().unwrap());
        for index in 0..count as u16 {
            let record = profile.periodic_record(index).unwrap();
            let words = [
                record.callback,
                record.actor_offset,
                record.input_factory.unwrap_or(0),
            ];
            for (word_index, word) in words.into_iter().enumerate() {
                assert_eq!(
                    word,
                    executable
                        .word(table + u32::from(index) * 12 + word_index as u32 * 4)
                        .unwrap()
                );
            }
        }
        assert!(profile.periodic_record(count as u16).is_none());
        assert!(profile.periodic_record(u16::MAX).is_none());
    }
}

#[test]
fn original_periodic_wrapper_matches_mutable_services_and_failed_prefixes() {
    let receipt = receipt();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 137);
    for case in receipt["cases"].as_array().unwrap() {
        let (mut arena, actor) = fixture();
        let mut host = Host {
            result: case["result"].as_u64().unwrap() as u32,
            null_factory: case["null_factory"].as_bool().unwrap_or(false),
            stop_at: case["stop_at"].as_str().map(str::to_owned),
            ..Default::default()
        };
        let result = arena.family_behavior_periodic_probe(
            actor,
            PROFILES[case["profile"].as_u64().unwrap() as usize],
            case["index"].as_u64().unwrap() as u16,
            &mut host,
        );
        if let Some(boundary) = host.stop_at.as_deref() {
            let expected_effect = match boundary {
                "factory" => "periodic input factory",
                "probe" => "periodic probe",
                "release" => "periodic input release",
                _ => unreachable!(),
            };
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Host {
                    effect: expected_effect,
                    error: "required periodic service"
                })
            );
        } else {
            assert_eq!(result.unwrap(), case["result_byte"].as_u64().unwrap() != 0);
            assert!(!host.retained_input);
        }
        assert_eq!(host.events, *case["events"].as_array().unwrap(), "{case}");
        assert_eq!(
            u64::from(arena.family428650_actor(actor).unwrap().word_21c),
            case["final_timer"].as_u64().unwrap()
        );
    }
}

#[test]
fn dispatcher_publishes_rng_timer_and_cursor_before_required_periodic_failure() {
    let (mut arena, actor) = fixture();
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.word_21c = 0;
    state.word_220 = Some(0);
    state.word_22e = 0;
    let mut host = Host {
        stop_at: Some("probe".into()),
        ..Default::default()
    };
    let result = arena.dispatch_family_behavior(
        actor,
        super::super::FamilyBehaviorDispatchRequest {
            microseconds: 10,
            profile: Some(PROFILES[3]),
            periodic_scan: true,
        },
        &mut host,
    );
    assert_eq!(
        result,
        Err(FamilyBehaviorError::Host {
            effect: "periodic probe",
            error: "required periodic service"
        })
    );
    assert_eq!(host.events[0], json!({"kind":"random"}));
    assert_eq!(host.events[1]["before"], 250008);
    assert_eq!(host.events[2]["before"], 250009);
    assert!(
        host.retained_input,
        "failed callback has not reached release"
    );
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.word_21c, 250025);
    assert_eq!(state.word_220, Some(1));
}

#[test]
fn invalid_periodic_index_fails_before_any_service_or_actor_write() {
    let (mut arena, actor) = fixture();
    let mut host = Host::default();
    let profile = PROFILES[3];
    assert_eq!(
        arena.family_behavior_periodic_probe(actor, profile, 5, &mut host),
        Err(FamilyBehaviorError::PeriodicIndex {
            actor,
            profile,
            index: 5
        })
    );
    assert!(host.events.is_empty());
    assert_eq!(arena.family428650_actor(actor).unwrap().word_21c, 100);
}
