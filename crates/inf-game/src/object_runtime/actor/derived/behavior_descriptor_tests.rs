//! Original singleton construction, duration-wrapper and +D8 prefix receipts.
//! Enclosing+24 and remaining descriptor effects remain unowned.

use serde_json::{json, Value};

use super::super::{build_actor_behavior, FamilyHelper482ff0};
use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, Selector12GrantContext,
    },
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture(profile: Option<FamilyBehaviorProfile>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
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
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    };
    let actor = match profile {
        None => arena.allocate_family428650_actor(request, &mut 0).unwrap(),
        Some(FamilyBehaviorProfile::Family4236d0 { mode }) => {
            let actor = arena.allocate_family4236d0_actor(request, &mut 0).unwrap();
            arena.family4236d0_actor_mut(actor).unwrap().word_266 = mode;
            actor
        }
        Some(FamilyBehaviorProfile::Selector12 { behavior_present }) => {
            let actor = arena
                .allocate_selector12_actor(
                    request,
                    &mut 0,
                    Selector12GrantContext {
                        table: &inf_formats::GrantTable {
                            virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                            records: vec![],
                        },
                        first_player_grants: None,
                    },
                )
                .unwrap();
            if behavior_present {
                arena.derived_actor_mut(actor).unwrap().behavior_140 =
                    build_actor_behavior(1, 0, 0);
            }
            actor
        }
        Some(FamilyBehaviorProfile::Selector35) => {
            arena.allocate_selector35_actor(request, &mut 0).unwrap()
        }
        Some(FamilyBehaviorProfile::Selector34) => {
            arena.allocate_selector34_actor(request, &mut 0).unwrap()
        }
    };
    (arena, actor)
}

struct Host {
    rng: u32,
    events: Vec<Value>,
}
impl Host {
    fn new(rng: u32) -> Self {
        Self {
            rng,
            events: vec![],
        }
    }
}
impl FamilyBehaviorHost for Host {
    type Error = &'static str;
    type PeriodicInput = ();

    fn random(&mut self) -> Result<u32, Self::Error> {
        let value = name_entry_rng_next(&mut self.rng);
        self.events.push(json!({"value":value, "rng":self.rng}));
        Ok(value)
    }
    fn periodic_input_factory(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
    ) -> Result<Option<()>, Self::Error> {
        Err("periodic factory unowned")
    }
    fn periodic_probe(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyPeriodicProbeRequest,
        _: Option<()>,
    ) -> Result<u32, Self::Error> {
        Err("periodic probe unowned")
    }
    fn release_periodic_input(&mut self, _: &mut ObjectArena, _: ()) -> Result<(), Self::Error> {
        Err("periodic release unowned")
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
        Err("descriptor score unowned")
    }
    fn candidate_bias(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i32, Self::Error> {
        Err("actor bias unowned")
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
        profile: FamilyBehaviorProfile,
        index: i32,
        time: u32,
    ) -> Result<(), Self::Error> {
        let (table, count) = profile.descriptor_table().unwrap();
        self.events
            .push(json!({"table":table, "count":count, "index":index, "microseconds":time}));
        Err("stop at descriptor update")
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

fn profile(name: &str) -> FamilyBehaviorProfile {
    match name {
        "family-mode0" => FamilyBehaviorProfile::Family4236d0 { mode: 0 },
        "family-mode1" => FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        "family-mode2" => FamilyBehaviorProfile::Family4236d0 { mode: 2 },
        "selector12-absent" => FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        "selector12-present" => FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        "selector35" => FamilyBehaviorProfile::Selector35,
        "selector34" => FamilyBehaviorProfile::Selector34,
        _ => panic!("unknown native profile"),
    }
}
fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn receipt() -> Value {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/family_behavior_descriptors_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    receipt
}

#[test]
fn authenticated_singletons_and_all_seven_pointer_tables_match_native_initialization() {
    let receipt = receipt();
    assert_eq!(receipt["profiles"].as_array().unwrap().len(), 7);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut entries = 0;
    let mut objects = std::collections::BTreeSet::new();
    for case in receipt["profiles"].as_array().unwrap() {
        let profile = profile(case["name"].as_str().unwrap());
        let (table, count) = profile.descriptor_table().unwrap();
        assert_eq!(table, word(&case["table"]));
        assert_eq!(count as u32, word(&case["count"]));
        for expected in case["descriptors"].as_array().unwrap() {
            let index = word(&expected["index"]);
            let descriptor = profile.descriptor(index as i32).unwrap();
            assert_eq!(descriptor.object_address, word(&expected["object_address"]));
            assert_eq!(descriptor.vtable_address, word(&expected["vtable_address"]));
            assert_eq!(descriptor.score_address, word(&expected["score_address"]));
            assert_eq!(descriptor.change_address, word(&expected["change_address"]));
            assert_eq!(descriptor.update_address, word(&expected["update_address"]));
            assert_eq!(
                descriptor.duration_address,
                expected["duration_address"].as_u64().map(|v| v as u32)
            );
            assert_eq!(
                exe.word(table + index * 4).unwrap(),
                descriptor.object_address
            );
            for (slot, callback) in [
                (4, descriptor.score_address),
                (8, descriptor.change_address),
                (12, descriptor.update_address),
            ] {
                assert_eq!(
                    exe.word(descriptor.vtable_address + slot).unwrap(),
                    callback
                );
            }
            if let Some(duration) = descriptor.duration_address {
                assert_eq!(exe.word(descriptor.vtable_address + 16).unwrap(), duration);
            }
            objects.insert(descriptor.object_address);
            entries += 1;
        }
        assert_eq!(profile.descriptor(-1), None);
        assert_eq!(profile.descriptor(count), None);
    }
    assert_eq!(entries, 57);
    assert_eq!(objects.len(), 39);
}

#[test]
fn authenticated_duration_wrappers_match_all_indices_kernels_and_rng_boundaries() {
    let receipt = receipt();
    assert_eq!(receipt["durations"].as_array().unwrap().len(), 438);
    let (mut arena, actor) = fixture(None);
    for case in receipt["durations"].as_array().unwrap() {
        let mut host = Host::new(word(&case["rng"]));
        let result = arena
            .family_behavior_duration(
                actor,
                profile(case["profile"].as_str().unwrap()),
                case["index"].as_i64().unwrap() as i32,
                "duration",
                &mut host,
            )
            .unwrap();
        assert_eq!(result, word(&case["result"]), "{case}");
        assert_eq!(host.rng, word(&case["final_rng"]), "{case}");
        assert_eq!(host.events, *case["events"].as_array().unwrap(), "{case}");
    }
}

#[test]
fn unsupported_positive_duration_index_fails_before_rng_and_preserves_actor() {
    let (mut arena, actor) = fixture(None);
    let before = arena.family428650_actor(actor).unwrap().clone();
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 0 };
    let mut host = Host::new(123);
    assert_eq!(
        arena.family_behavior_duration(actor, profile, 3, "duration", &mut host),
        Err(FamilyBehaviorError::DescriptorIndex {
            actor,
            profile,
            index: 3
        })
    );
    assert_eq!(host.rng, 123);
    assert!(host.events.is_empty());
    assert_eq!(arena.family428650_actor(actor).unwrap(), &before);
}

#[test]
fn authenticated_concrete_d8_profiles_and_signed_timer_prefixes_match() {
    let receipt = receipt();
    assert_eq!(receipt["entry_prefixes"].as_array().unwrap().len(), 31);
    for case in receipt["entry_prefixes"].as_array().unwrap() {
        let profile = profile(case["profile"].as_str().unwrap());
        let (mut arena, actor) = fixture(Some(profile));
        arena.actor_mut(actor).unwrap().word_c4 = 3;
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_184 = 2;
        state.word_22e = 2; // Owned lock avoids unowned later selection effects.
        state.words_188_19c[0] = Some(0);
        state.word_210 = word(&case["timer"]);
        state.word_20c = 0x8765_4321;
        state.helper_218 = case["helper"].as_u64().map(|count| {
            let mut helper = FamilyHelper482ff0::new(None);
            helper.word_08 = count as u32;
            Box::new(helper)
        });
        let helper_before = state.helper_218.clone();
        let mut host = Host::new(123);
        let time = word(&case["microseconds"]);
        assert_eq!(
            arena.dispatch_actor_family_behavior(
                actor,
                time,
                super::super::FamilyHelperHistoryInputs {
                    // check_family_behavior_descriptors.py supplies this
                    // actual fixture process DWORD at 0054E894.
                    process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
                },
                &mut host
            ),
            Err(FamilyBehaviorError::Host {
                effect: "behavior update",
                error: "stop at descriptor update"
            })
        );
        let boundary = &case["boundary"];
        let state = arena.family428650_actor(actor).unwrap();
        assert_eq!(state.word_210, word(&boundary["timer"]), "{case}");
        assert_eq!(state.word_20c, word(&boundary["target"]), "{case}");
        assert_eq!(
            state.helper_218, helper_before,
            "empty/negative helper preserves holes"
        );
        assert_eq!(
            host.events,
            [
                json!({"table":boundary["descriptor"][2], "count":boundary["descriptor"][1], "index":2, "microseconds":time})
            ]
        );
        assert_eq!(host.rng, 123);
        assert_eq!(state.word_220, None);
        assert_eq!(state.words_188_19c[5], None);
        // Selector34/35 derive directly from 428650, without 429E20's +230.
        if !matches!(
            profile,
            FamilyBehaviorProfile::Selector35 | FamilyBehaviorProfile::Selector34
        ) {
            assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        }
    }
}

#[test]
fn positive_helper_entries_read_unwritten_weight_only_on_wrappers_that_fade() {
    for profile in [
        FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
    ] {
        let (mut arena, actor) = fixture(Some(profile));
        arena.actor_mut(actor).unwrap().word_c4 = 3;
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_184 = 2;
        state.word_22e = 2;
        state.words_188_19c[0] = Some(0);
        let mut helper = FamilyHelper482ff0::new(None);
        helper.word_08 = 1;
        state.helper_218 = Some(Box::new(helper));
        let before = state.clone();
        let mut host = Host::new(123);
        let result = arena.dispatch_actor_family_behavior(
            actor,
            40000,
            super::super::FamilyHelperHistoryInputs {
                process_null_handle: None,
            },
            &mut host,
        );
        if matches!(
            profile,
            FamilyBehaviorProfile::Selector12 {
                behavior_present: false
            }
        ) {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Host {
                    effect: "behavior update",
                    error: "stop at descriptor update"
                })
            );
            assert_eq!(host.events.len(), 1);
        } else {
            assert_eq!(
                result,
                Err(FamilyBehaviorError::Helper(
                    super::super::FamilyHelperHistoryError::Uninitialized {
                        actor,
                        offset: 0x1c
                    }
                ))
            );
            assert!(host.events.is_empty());
            assert_eq!(arena.family428650_actor(actor).unwrap(), &before);
        }
        assert_eq!(host.rng, 123);
    }
}

#[test]
fn actual_wrapper_fades_positive_history_before_descriptor_update() {
    for profile in [
        FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        FamilyBehaviorProfile::Selector35,
    ] {
        let (mut arena, actor) = fixture(Some(profile));
        arena.actor_mut(actor).unwrap().word_c4 = 3;
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_184 = 2;
        state.word_22e = 2;
        state.words_188_19c[0] = Some(0);
        let mut helper = FamilyHelper482ff0::new(None);
        helper.word_08 = 2;
        helper.records[0].weight = Some(50);
        helper.records[1].weight = Some(0);
        helper.records[1].handle = Some(ActorHandle::from_raw(7));
        helper.records[1].padding = Some(0xbeef);
        state.helper_218 = Some(Box::new(helper));
        let before = state.helper_218.clone();
        let mut host = Host::new(123);
        assert_eq!(
            arena.dispatch_actor_family_behavior(
                actor,
                40000,
                super::super::FamilyHelperHistoryInputs {
                    process_null_handle: Some(ActorHandle::from_raw(0x1234_5678))
                },
                &mut host
            ),
            Err(FamilyBehaviorError::Host {
                effect: "behavior update",
                error: "stop at descriptor update"
            })
        );
        assert_eq!(host.events.len(), 1);
        let helper = arena
            .family428650_actor(actor)
            .unwrap()
            .helper_218
            .as_ref()
            .unwrap();
        if matches!(
            profile,
            FamilyBehaviorProfile::Selector12 {
                behavior_present: false
            } | FamilyBehaviorProfile::Selector35
        ) {
            assert_eq!(Some(helper.clone()), before);
        } else {
            assert_eq!(helper.word_08, 1);
            assert_eq!(helper.records[0].weight, Some(31));
            assert_eq!(
                helper.records[1].handle,
                Some(ActorHandle::from_raw(0x1234_5678))
            );
            assert_eq!(helper.records[1].padding, Some(0xbeef));
        }
        assert_eq!(host.rng, 123);
    }
}
