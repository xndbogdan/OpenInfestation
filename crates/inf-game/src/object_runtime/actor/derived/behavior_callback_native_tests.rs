//! Original callback receipts and supplied descriptor-host ordering. No test
//! supplies constructor values for+220/+230 or claims an enclosing+24 update.

use std::sync::Arc;

use inf_formats::{ByteSpan, GrantTable};
use serde_json::{json, Value};

use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, RuntimeAllocation, RuntimeAllocationError,
        Selector12GrantContext,
    },
};

const PROFILE: FamilyBehaviorProfile = FamilyBehaviorProfile::Selector12 {
    behavior_present: false,
};
const UNUSED_SINE: [u16; 4096] = [0; 4096];

#[derive(Clone, Copy, Debug)]
enum Class {
    Base,
    Family4236d0,
    Selector10,
    Selector12,
    Selector34,
    Selector35,
    Selector36,
}

impl Class {
    fn from_case(case: &Value) -> Self {
        match case["actor_class"].as_str().unwrap_or("selector12") {
            "family4236d0" => Self::Family4236d0,
            "selector10" => Self::Selector10,
            "selector12" => Self::Selector12,
            "selector34" => Self::Selector34,
            "selector35" => Self::Selector35,
            "selector36" => Self::Selector36,
            _ => panic!("unknown fixture actor class"),
        }
    }

    fn word_offset(self) -> u32 {
        match self {
            Self::Selector12 => 0x10,
            Self::Selector35 => 0x0e,
            Self::Family4236d0 | Self::Selector10 | Self::Selector34 | Self::Selector36 => 0x0c,
            Self::Base => panic!("base bias remains a host effect"),
        }
    }

    fn missing_error(self) -> DerivedActorError {
        match self {
            Self::Selector12 => DerivedActorError::MissingSelector12Payload,
            Self::Selector34 => DerivedActorError::MissingSelector34Payload,
            Self::Selector35 => DerivedActorError::MissingSelector35Payload,
            Self::Family4236d0 | Self::Selector10 | Self::Selector36 => {
                DerivedActorError::MissingFamily4236d0Payload
            }
            Self::Base => panic!("base bias remains a host effect"),
        }
    }

    fn retain(
        self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        offset: u32,
    ) {
        match self {
            Self::Selector12 => arena.set_selector12_retained(actor, allocation, offset),
            Self::Selector34 => arena.set_selector34_retained(actor, allocation, offset),
            Self::Selector35 => arena.set_selector35_retained(actor, allocation, offset),
            Self::Family4236d0 | Self::Selector10 | Self::Selector36 => {
                arena.set_family4236d0_retained(actor, allocation, offset)
            }
            Self::Base => panic!("base bias remains a host effect"),
        }
        .unwrap();
    }

    fn assert_allocation_holes(self, arena: &ObjectArena, actor: ObjectId) {
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
        if !matches!(self, Self::Selector34 | Self::Selector35 | Self::Base) {
            assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        }
    }
}

fn fixture(class: Class) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let request = DerivedActorConstructionRequest {
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
                sine_quarter: &UNUSED_SINE,
            },
        },
        world_mask_limits: [3, 3],
    };
    let actor = match class {
        Class::Selector12 => arena
            .allocate_selector12_actor(
                request,
                &mut 0,
                Selector12GrantContext {
                    table: &GrantTable {
                        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                        records: vec![],
                    },
                    first_player_grants: None,
                },
            )
            .unwrap(),
        Class::Base => arena.allocate_family428650_actor(request, &mut 0).unwrap(),
        Class::Family4236d0 => arena.allocate_family4236d0_actor(request, &mut 0).unwrap(),
        Class::Selector10 => arena.allocate_selector10_actor(request, &mut 0).unwrap(),
        Class::Selector34 => arena.allocate_selector34_actor(request, &mut 0).unwrap(),
        Class::Selector35 => arena.allocate_selector35_actor(request, &mut 0).unwrap(),
        Class::Selector36 => arena.allocate_selector36_actor(request, &mut 0).unwrap(),
    };
    (arena, actor)
}

fn retained(
    arena: &mut ObjectArena,
    actor: ObjectId,
    class: Class,
    word: u16,
) -> Arc<RuntimeAllocation> {
    let mut bytes = vec![0xd3; 32];
    let offset = class.word_offset() as usize;
    bytes[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
    let allocation = Arc::new(RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, 32)).unwrap());
    class.retain(arena, actor, &allocation, 0);
    allocation
}

struct Host {
    class: Class,
    rng: u32,
    events: Vec<Value>,
    allocation: Option<Arc<RuntimeAllocation>>,
    after_rng_word: Option<u16>,
    score_mutates_word: bool,
    score_replaces_retained: Option<Arc<RuntimeAllocation>>,
}

impl Host {
    fn new(rng: u32, class: Class) -> Self {
        Self {
            class,
            rng,
            events: vec![],
            allocation: None,
            after_rng_word: None,
            score_mutates_word: false,
            score_replaces_retained: None,
        }
    }
}

impl FamilyBehaviorHost for Host {
    type Error = &'static str;
    type PeriodicInput = ();

    fn random(&mut self) -> Result<u32, Self::Error> {
        let value = name_entry_rng_next(&mut self.rng);
        self.events
            .push(json!({"kind":"random", "value":value, "rng":self.rng}));
        if let Some(word) = self.after_rng_word {
            self.allocation
                .as_ref()
                .unwrap()
                .write_word(self.class.word_offset(), word)
                .unwrap();
        }
        Ok(value)
    }
    fn periodic_probe(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyPeriodicProbeRequest,
        _: Option<Self::PeriodicInput>,
    ) -> Result<u32, Self::Error> {
        Err("periodic probe unowned")
    }
    fn periodic_input_factory(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
    ) -> Result<Option<Self::PeriodicInput>, Self::Error> {
        Err("periodic input factory unowned")
    }
    fn release_periodic_input(
        &mut self,
        _: &mut ObjectArena,
        _: Self::PeriodicInput,
    ) -> Result<(), Self::Error> {
        Err("periodic input release unowned")
    }
    fn score(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _: FamilyBehaviorProfile,
        index: i32,
        current: bool,
        bias: i32,
    ) -> Result<i32, Self::Error> {
        self.events
            .push(json!({"kind":"score", "index":index, "current":current, "bias":bias}));
        if self.score_mutates_word && index == 3 {
            self.allocation
                .as_ref()
                .unwrap()
                .write_word(self.class.word_offset(), 65535)
                .unwrap();
        }
        if let Some(allocation) = &self.score_replaces_retained {
            if index == 3 {
                self.class.retain(arena, actor, allocation, 32);
            }
        }
        Ok(if index == 2 { 10 } else { 0 })
    }
    fn candidate_bias(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i32, Self::Error> {
        self.events.push(json!({"kind":"host_bias"}));
        Ok(77)
    }
    fn change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _: FamilyBehaviorProfile,
        index: i32,
        entering: bool,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"change", "index":index, "entering":entering, "current":arena.family428650_actor(actor).unwrap().word_184 as u32}));
        Ok(())
    }
    fn update(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _: FamilyBehaviorProfile,
        index: i32,
        microseconds: u32,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"update", "index":index, "microseconds":microseconds, "duration":arena.family428650_actor(actor).unwrap().words_188_19c[5].unwrap()}));
        Ok(())
    }
    fn occasional_sound(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u16,
    ) -> Result<(), Self::Error> {
        Err("audio unowned")
    }
}

fn receipt() -> Value {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/family_behavior_callbacks_native.json"
    ))
    .unwrap();
    assert_eq!(
        value["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    value
}

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

#[test]
fn authenticated_original_duration_bias_live_read_and_failure_prefixes_match() {
    let receipt = receipt();
    assert_eq!(receipt["kernels"].as_array().unwrap().len(), 258);
    for case in receipt["kernels"].as_array().unwrap() {
        let class = Class::from_case(case);
        let (mut arena, actor) = fixture(class);
        let mut host = Host::new(word(&case["rng"]), class);
        if !case["missing"].as_bool().unwrap_or(false) {
            if case["short"].as_bool().unwrap_or(false) {
                let allocation = Arc::new(
                    RuntimeAllocation::new(vec![0xd3; 16].into(), ByteSpan::new(0, 16)).unwrap(),
                );
                // Retention permits one-past, but the reached WORD read cannot.
                class.retain(&mut arena, actor, &allocation, 16);
            } else {
                host.allocation = Some(retained(
                    &mut arena,
                    actor,
                    class,
                    case["word"].as_u64().unwrap_or(1000) as u16,
                ));
            }
        }
        host.after_rng_word = case["after_rng_word"].as_u64().map(|value| value as u16);
        let result = if case["kind"] == "duration" {
            arena
                .family_behavior_duration(
                    actor,
                    PROFILE,
                    case["index"].as_i64().unwrap() as i32,
                    "duration",
                    &mut host,
                )
                .map(i64::from)
        } else {
            arena
                .family_behavior_candidate_bias(actor, &mut host)
                .map(i64::from)
        };
        if case["missing"].as_bool().unwrap_or(false) && case["kind"] == "bias" {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Derived(class.missing_error())),
                "{}",
                case["name"]
            );
        } else if case["short"].as_bool().unwrap_or(false) {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Derived(DerivedActorError::Allocation(
                    RuntimeAllocationError::Range {
                        offset: 16 + class.word_offset(),
                        length: 2,
                        allocation_bytes: 16
                    }
                ))),
                "{}",
                case["name"]
            );
        } else {
            assert_eq!(
                result.unwrap(),
                case["result"].as_i64().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(host.rng, word(&case["final_rng"]), "{}", case["name"]);
        let draws: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "random")
            .cloned()
            .collect();
        assert_eq!(host.events, draws, "{}", case["name"]);
        assert_eq!(host.events.len(), 1);
        class.assert_allocation_holes(&arena, actor);
    }
}

#[test]
fn dispatcher_matches_original_duration_order_live_aliases_and_later_failure() {
    let receipt = receipt();
    assert_eq!(receipt["dispatches"].as_array().unwrap().len(), 30);
    for case in receipt["dispatches"].as_array().unwrap() {
        let class = Class::from_case(case);
        let (mut arena, actor) = fixture(class);
        arena.actor_mut(actor).unwrap().word_c4 = word(&case["lifecycle"]);
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_184 = case["current"].as_i64().unwrap() as i32;
        state.word_22e = case["flags"].as_u64().unwrap_or(0) as u16;
        state.word_21c = 1_000_000;
        state.words_188_19c[0] = Some(0);
        state.words_188_19c[5] = Some(0);
        state.words_204_208[0] = Some(100);
        state.words_204_208[1] = Some(0x1234_5678);
        state.byte_22c = 0x51;
        if let Ok(state) = arena.family4236d0_actor_mut(actor) {
            state.byte_260 = 0x62;
        }
        match &mut arena.family428650_actor_mut(actor).unwrap().class {
            Family428650Class::Selector35(state) => {
                (state.word_230, state.word_234, state.byte_238) = (500, 512, 0x73)
            }
            Family428650Class::Selector34(state) => {
                (state.word_230, state.word_234, state.byte_238) = (500, 512, 0x73)
            }
            _ => {}
        }
        arena.owned_mut(actor).unwrap().state.base.flags = 0x0400_0100;
        arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x40;
        let mut host = Host::new(word(&case["rng"]), class);
        if !case["missing"].as_bool().unwrap_or(false) {
            host.allocation = Some(retained(
                &mut arena,
                actor,
                class,
                word(&case["word"]) as u16,
            ));
        }
        host.score_mutates_word = case["score_mutates_word"].as_bool().unwrap_or(false);
        if case["score_replaces_retained"].as_bool().unwrap_or(false) {
            let mut bytes = vec![0xd3; 64];
            let offset = 32 + class.word_offset() as usize;
            bytes[offset..offset + 2].copy_from_slice(&65535u16.to_le_bytes());
            host.score_replaces_retained = Some(Arc::new(
                RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, 64)).unwrap(),
            ));
        }
        let result = arena.dispatch_family_behavior(
            actor,
            FamilyBehaviorDispatchRequest {
                microseconds: 10,
                profile: Some(PROFILE),
                periodic_scan: case["unknown_cursor"].as_bool().unwrap_or(false),
            },
            &mut host,
        );
        if case["unknown_cursor"] == true {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Uninitialized {
                    actor,
                    offset: 0x220
                })
            );
        } else if case["failed"] == true {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Derived(class.missing_error()))
            );
        } else {
            result.unwrap();
        }
        let expected_events: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| {
                !matches!(
                    event["kind"].as_str(),
                    Some("retained_read" | "required_read")
                )
            })
            .cloned()
            .collect();
        assert_eq!(host.events, expected_events, "{}", case["name"]);
        assert_eq!(host.rng, word(&case["final_rng"]));
        let state = arena.family428650_actor(actor).unwrap();
        let expected = &case["final"];
        assert_eq!(state.word_184 as u32, word(&expected["current"]));
        assert_eq!(state.words_188_19c[0], Some(word(&expected["elapsed"])));
        assert_eq!(state.words_188_19c[5], Some(word(&expected["duration"])));
        assert_eq!(state.word_21c, word(&expected["timer"]));
        assert_eq!(u32::from(state.word_22e), word(&expected["flags"]));
        let clear = &expected["clear"];
        assert_eq!(state.words_204_208[1], Some(word(&clear["word_208"])));
        assert_eq!(u32::from(state.byte_22c), word(&clear["byte_22c"]));
        assert_eq!(
            arena.object(actor).unwrap().base.flags,
            word(&clear["flags"])
        );
        assert_eq!(
            u32::from(arena.object(actor).unwrap().base.dirty_word),
            word(&clear["dirty_word"])
        );
        if let Ok(state) = arena.family4236d0_actor(actor) {
            assert_eq!(u32::from(state.byte_260), word(&clear["byte_260"]));
        }
        // Shared 0043F910 suffix under both final vtables.
        let scalars = match &arena.family428650_actor(actor).unwrap().class {
            Family428650Class::Selector35(state) => {
                Some((state.word_230, state.word_234, state.byte_238))
            }
            Family428650Class::Selector34(state) => {
                Some((state.word_230, state.word_234, state.byte_238))
            }
            _ => None,
        };
        if let Some((word_230, word_234, byte_238)) = scalars {
            assert_eq!(word_230, word(&clear["word_230"]));
            assert_eq!(word_234, word(&clear["word_234"]));
            assert_eq!(u32::from(byte_238), word(&clear["byte_238"]));
        }
        class.assert_allocation_holes(&arena, actor);
    }
}

#[test]
fn supplied_wrapper_and_actual_actor_vtable_remain_independent() {
    let (mut arena, base) = fixture(Class::Base);
    let mut host = Host::new(0x12345678, Class::Base);
    let duration = arena
        .family_behavior_duration(base, PROFILE, 2, "duration", &mut host)
        .unwrap();
    assert_eq!(
        duration,
        ((host.events[0]["value"].as_u64().unwrap() as u32 & 0xffff) + 125000) << 5
    );
    assert_eq!(
        arena
            .family_behavior_candidate_bias(base, &mut host)
            .unwrap(),
        77
    );
    assert_eq!(host.events[1], json!({"kind":"host_bias"}));

    let (mut arena, selector12) = fixture(Class::Selector12);
    let ordinary = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let mut host = Host::new(0x12345678, Class::Selector12);
    assert_eq!(
        arena
            .family_behavior_duration(selector12, ordinary, 2, "duration", &mut host)
            .unwrap(),
        ((host.events[0]["value"].as_u64().unwrap() as u32 & 0xffff) + 46_875) << 6
    );
    assert_ne!(host.rng, 0x12345678);
    assert_eq!(host.events.len(), 1);
    assert_eq!(host.events[0]["kind"], "random");
    assert_eq!(
        arena.family_behavior_candidate_bias(selector12, &mut host),
        Err(FamilyBehaviorError::Derived(
            DerivedActorError::MissingSelector12Payload
        ))
    );
    assert_eq!(host.events.len(), 2);
    assert_eq!(host.events[1]["kind"], "random");
}

#[test]
fn recovered_callback_addresses_match_actual_wrapper_and_actor_tables() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    assert_eq!(executable.word(0x0052_71ac + 0x0c).unwrap(), 0x0046_90f0);
    assert_eq!(executable.word(0x0052_7058 + 0x110).unwrap(), 0x0046_8e00);
    for vtable in [0x0052_0b84, 0x0052_0cc8, 0x0052_7ee0] {
        assert_eq!(executable.word(vtable + 0x110).unwrap(), 0x0043_50e0);
    }
    assert_eq!(executable.word(0x0052_273c + 0x110).unwrap(), 0x0043_f960);
}
