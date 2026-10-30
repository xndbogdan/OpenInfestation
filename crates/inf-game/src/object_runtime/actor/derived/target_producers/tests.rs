use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::*;
use serde_json::{json, Value};

const SINE: [u16; 4096] = [0; 4096];
fn request(position: [u16; 3]) -> DerivedActorConstructionRequest<'static> {
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
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}
fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x12345678),
    })
    .unwrap()
}
fn selector12(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    arena
        .allocate_selector12_actor(
            request(position),
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: 0,
                    records: vec![],
                },
                first_player_grants: None,
            },
        )
        .unwrap()
}
fn profile(factory: bool) -> FamilyBehaviorProfile {
    if factory {
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        }
    } else {
        FamilyBehaviorProfile::Family4236d0 { mode: 0 }
    }
}
fn periodic(mode: &str, factory: bool) -> FamilyPeriodicProbeRequest {
    let profile = profile(factory);
    let index = if mode == "player" {
        0
    } else if factory {
        1
    } else {
        2
    };
    FamilyPeriodicProbeRequest {
        profile,
        index,
        record: profile.periodic_record(index).unwrap(),
    }
}
fn position(case: &Value, key: &str) -> [u16; 3] {
    std::array::from_fn(|i| case[key][i].as_u64().unwrap() as u16)
}

struct Host {
    rng: u32,
    events: Vec<Value>,
    null_input: bool,
    null_target: bool,
    stop: Option<String>,
    retained_input: Option<NativeHeapRef>,
}
impl TargetNullHandleSource for Host {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        // Supplied fixture process source, distinct from a registry fallback.
        Some(ActorHandle::from_raw(0x12345678))
    }
}
impl FamilyTargetProducerHost for Host {
    type Error = &'static str;
    fn random(&mut self) -> Result<u32, Self::Error> {
        let value = name_entry_rng_next(&mut self.rng);
        self.events.push(json!({"kind":"random","value":value}));
        Ok(value)
    }
}
impl FamilyBehaviorHost for Host {
    type Error = &'static str;
    // The opaque token identifies the actual arena allocation and payload.
    type PeriodicInput = NativeHeapRef;
    fn random(&mut self) -> Result<u32, Self::Error> {
        FamilyTargetProducerHost::random(self)
    }
    fn periodic_input_factory(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        factory: u32,
    ) -> Result<Option<Self::PeriodicInput>, Self::Error> {
        assert_eq!(factory, 0x427640);
        let outcome = if self.stop.as_deref() == Some("input") {
            FixtureActorServiceAllocation::Error(71)
        } else if self.null_input {
            FixtureActorServiceAllocation::Null
        } else {
            FixtureActorServiceAllocation::Allocate
        };
        arena.actor_service_heap_mut().script([outcome]);
        let result = arena
            .allocate_family_radius_score_input(actor)
            .map_err(|_| "unavailable input allocation");
        if let Ok(Some(reference)) = result {
            self.retained_input = Some(reference);
        }
        result
    }
    fn periodic_probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyPeriodicProbeRequest,
        input: Option<Self::PeriodicInput>,
    ) -> Result<u32, Self::Error> {
        let request = FamilyTargetProducerRequest::from_periodic(request).unwrap();
        arena
            .actor_service_heap_mut()
            .script([if self.stop.as_deref() == Some("target") {
                FixtureActorServiceAllocation::Error(71)
            } else if self.null_target {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }]);
        arena
            .produce_family_periodic_target(actor, request, input, self)
            .map(u32::from)
            .map_err(|_| "unavailable target allocation")
    }
    fn release_periodic_input(
        &mut self,
        arena: &mut ObjectArena,
        input: Self::PeriodicInput,
    ) -> Result<(), Self::Error> {
        assert_eq!(self.retained_input, Some(input));
        arena
            .release_actor_service(input)
            .map_err(|_| "input release custody")?;
        self.retained_input = None;
        self.events.push(json!({"kind":"free","resource":"input"}));
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
        Err("change unowned")
    }
    fn update(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyBehaviorProfile,
        _: i32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("update unowned")
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

fn snapshot(target: Option<NativeTarget>, player: ObjectId, nodes: &[ObjectId]) -> Value {
    target.map_or(Value::Null, |target| match target {
        NativeTarget::ActorFollow(target) => json!({"handle_04":target.handle_04.raw(),
            "focus_08":target.focus_08,"word_10":target.word_10,"focus_14":target.focus_14,"velocity_1a":target.velocity_1a}),
        NativeTarget::FixedPosition(target) => json!({"handle_04":target.handle_04.raw(),
            "focus_08":target.focus_08,"word_10":target.word_10}),
        NativeTarget::FurniturePosition(_) => panic!("unadmitted target class"),
        NativeTarget::ObjectPosition(target) => json!({"handle_04":target.handle_04.raw(),"focus_08":target.focus_08,
            "word_10":target.word_10,"object_14":if target.object_14==player { json!("player") } else { json!(nodes.iter().position(|&node|node==target.object_14).unwrap()) }}),
    })
}

#[test]
fn literal_periodic_producers_match_original_native_wrapper_factory_and_lifecycle() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_target_producers_native.json"
    ))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 43);
    for case in cases {
        let mut arena = arena();
        let player = arena
            .allocate_family428650_actor(request(position(case, "player_position")), &mut 0)
            .unwrap();
        let allocated = arena.actor(player).unwrap().handle;
        arena.actor_handles.as_mut().unwrap().release(allocated);
        let handle = ActorHandle::from_raw(0x08010001);
        arena
            .actor_handles
            .as_mut()
            .unwrap()
            .bind(handle, player)
            .unwrap();
        arena.actor_mut(player).unwrap().handle = handle;
        arena.actor_mut(player).unwrap().word_74 = case["player_size"].as_u64().unwrap() as u16;
        arena.owned_mut(player).unwrap().state.base.word_28 =
            case["player_extent"].as_u64().unwrap() as u16;
        if case["resolves"] == false {
            arena.actor_handles.as_mut().unwrap().release(handle);
        }
        let actor = selector12(&mut arena, position(case, "actor_position"));
        arena.derived_actor_mut(actor).unwrap().word_164 = case["radius"].as_i64().unwrap() as i32;
        if case["empty_players"] != true {
            arena.add_world_player([0; 6]);
            let null = arena.actor_handles().unwrap().null_handle();
            arena.players.list[0].handle_d4 = if case["has_player"] == false {
                null
            } else {
                handle
            };
            arena.players.list[0].handle_d8 = if case["secondary_live"] == true {
                handle
            } else {
                null
            };
        }
        let mut nodes = vec![];
        for group in case["groups"].as_array().unwrap() {
            let mut previous = nodes.len() as u16;
            for (index, position) in group["positions"].as_array().unwrap().iter().enumerate() {
                let node = arena.allocate(ObjectClassKind::RouteNode);
                arena.owned_mut(node).unwrap().state.base.position_words =
                    Some(std::array::from_fn(|i| {
                        position[i].as_u64().unwrap() as u16
                    }));
                let identity = nodes.len() as u16;
                let route = arena.route_mut(node).unwrap();
                route.identity_word = identity;
                route.predecessor_word = previous;
                route.flags = group["flags"].as_u64().unwrap() as u8;
                previous = identity;
                arena.owned_mut(node).unwrap().state.base.flags = if (index == 0
                    && group["disabled_root"] == true)
                    || (index > 0 && group["disabled_children"] == true)
                {
                    0x10
                } else {
                    0
                };
                nodes.push(node);
            }
        }
        arena.finalize_routes().unwrap();
        let callback = periodic(case["mode"].as_str().unwrap(), case["factory"] == true);
        let producer = FamilyTargetProducerRequest::from_periodic(callback).unwrap();
        let old = match case["old_kind"].as_str() {
            None => None,
            Some("follow") => Some(NativeTarget::ActorFollow(ActorFollowTarget {
                handle_04: handle,
                focus_08: Some([11, 12, 13]),
                word_10: 2,
                focus_14: Some([21, 22, 23]),
                velocity_1a: Some([31, 32, 33]),
            })),
            Some("object") => Some(NativeTarget::ObjectPosition(ObjectPositionTarget {
                handle_04: arena.actor_handles().unwrap().null_handle(),
                focus_08: [11, 12, 13],
                word_10: 16,
                object_14: player,
            })),
            _ => panic!(),
        };
        let old = old.map(|data| arena.import_fixture_target(data));
        if producer.destination() == FamilyTargetSlot::Target26c {
            let Family429e20Class::Selector12(state) =
                &mut arena.family429e20_actor_mut(actor).unwrap().class
            else {
                panic!()
            };
            state.targets_26c_284[0] = old;
            state.word_270 = case["old_score"].as_u64().unwrap() as u32;
        } else {
            let state = arena.family428650_actor_mut(actor).unwrap();
            state.targets_1d4_1fc[5] = old;
            state.words_1d8_200[5] = case["old_score"].as_u64().unwrap() as u32;
        }
        let mut host = Host {
            rng: case["rng"].as_u64().unwrap() as u32,
            events: vec![],
            null_input: case["null_input"] == true,
            null_target: case["null_target"] == true,
            stop: case["stop"].as_str().map(str::to_owned),
            retained_input: None,
        };
        let allocation_before = arena.actor_service_heap().attempts().len();
        let result = arena.family_behavior_periodic_probe(
            actor,
            callback.profile,
            callback.index,
            &mut host,
        );
        if case["stopped"] == true {
            assert!(result.is_err(), "{case}");
        } else {
            assert_eq!(
                u64::from(result.unwrap()),
                case["returned"].as_u64().unwrap(),
                "{case}"
            );
        }
        let pair = arena
            .family_weighted_target(actor, producer.destination())
            .unwrap();
        let decoded = if case["custody"] == "released" {
            let reference = pair
                .target
                .expect("native stale pointer survives failed replacement");
            assert_eq!(
                arena.actor_service_heap().state(reference).unwrap(),
                ActorServiceResourceState::Released,
                "{case}"
            );
            assert!(arena.native_target(reference).is_err(), "{case}");
            None
        } else {
            pair.target
                .map(|reference| arena.native_target(reference).unwrap())
        };
        assert_eq!(
            pair.score,
            case["final_score"].as_u64().unwrap() as u32,
            "{case}"
        );
        assert_eq!(snapshot(decoded, player, &nodes), case["target"], "{case}");
        assert_eq!(
            u64::from(host.rng),
            case["final_rng"].as_u64().unwrap(),
            "{case}"
        );
        let native_events: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| {
                event["kind"] == "random"
                    || (event["kind"] == "free" && event["resource"] == "input")
            })
            .cloned()
            .collect();
        assert_eq!(host.events, native_events, "{case}");
        // Allocator calls are observed on the real heap, not synthesized as
        // host events. Fixture imports precede this per-wrapper attempt slice.
        let expected_allocations: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "allocate")
            .collect();
        let attempts = &arena.actor_service_heap().attempts()[allocation_before..];
        assert_eq!(attempts.len(), expected_allocations.len(), "{case}");
        for (kind, expected) in attempts.iter().zip(expected_allocations) {
            assert_eq!(
                kind.allocation_bytes() as u64,
                expected["bytes"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(
                matches!(kind, ActorServiceResourceKind::FamilyRadiusScoreInput),
                expected["resource"] == "input",
                "{case}"
            );
        }
        if case["input_payload"].is_object() {
            assert_eq!(
                case["input_payload"],
                json!({"vtable":0x520f2c,"actor_is_actual":true})
            );
        }
        assert_eq!(
            host.retained_input.is_some(),
            case["stopped"] == true && case["stop"] == "target" && case["factory"] == true,
            "{case}"
        );
        if let Some(input) = host.retained_input {
            assert_eq!(
                arena
                    .actor_service_heap()
                    .radius_score_actor(input)
                    .unwrap(),
                actor
            );
            assert_eq!(arena.actor_service_heap().retained_bytes(input).unwrap(), 8);
        }
    }
}

#[test]
fn request_validation_and_missing_position_stop_before_rng_or_input_dereference() {
    let mut original = periodic("player", true);
    original.record.actor_offset = 0x1fc;
    assert!(FamilyTargetProducerRequest::from_periodic(original).is_none());
    let mut arena = arena();
    let actor = selector12(&mut arena, [0, 0, 0]);
    let candidate = arena
        .allocate_family428650_actor(request([1, 0, 0]), &mut 0)
        .unwrap();
    let handle = arena.actor(candidate).unwrap().handle;
    arena.add_world_player([0; 6]);
    arena.players.list[0].handle_d4 = handle;
    let producer = FamilyTargetProducerRequest::from_periodic(periodic("player", true)).unwrap();
    let mut host = Host {
        rng: 7,
        events: vec![],
        null_input: false,
        null_target: false,
        stop: None,
        retained_input: None,
    };
    assert_eq!(
        arena.produce_family_periodic_target(actor, producer, None, &mut host),
        Err(FamilyTargetProducerError::MissingInput)
    );
    arena
        .owned_mut(candidate)
        .unwrap()
        .state
        .base
        .position_words = None;
    assert_eq!(
        arena.produce_family_periodic_target(actor, producer, None, &mut host),
        Err(FamilyTargetProducerError::MissingPosition(candidate))
    );
    assert!(host.events.is_empty());
    assert_eq!(host.rng, 7);
}
