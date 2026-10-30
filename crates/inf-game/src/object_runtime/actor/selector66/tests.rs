use super::*;
use crate::object_runtime::{
    ActorAttachmentHost, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectList, TargetNullHandleSource,
};
use serde_json::{json, Value};
fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn xyz(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&v[i]) as u16)
}
fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    }
}
fn fixture(i: &Value) -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena.allocate_selector66(allocation()).unwrap();
    let owner = arena.allocate_actor(allocation()).unwrap();
    for (id, prefix) in [(actor, "actor"), (owner, "owner")] {
        let field = |suffix: &str| &i[format!("{prefix}_{suffix}")];
        let base = &mut arena.owned_mut(id).unwrap().state.base;
        base.flags = u(field("flags"));
        base.dirty_word = u(field("dirty")) as u16;
        base.position_words = Some(xyz(field("position")));
        let state = arena.actor_mut(id).unwrap();
        state.rotating.visual.model_word = u(field("model")) as u16;
        state.word_6c = u(field("identity"));
        state.word_b4 = u(field("class"));
        state.owner_handle = ActorHandle::from_raw(u(field("source")));
    }
    arena.actor_mut(actor).unwrap().owner_handle = if i["owner_null"] == true {
        ActorHandle::from_raw(0)
    } else {
        arena.actor(owner).unwrap().handle
    };
    arena.actor_mut(actor).unwrap().words_8e_92 = xyz(&i["velocity"]);
    let state = arena.selector66_mut(actor).unwrap();
    state.saved_flags_138 = Some(u(&i["saved"]));
    state.elapsed_13c = u(&i["elapsed"]);
    state.idle_140 = u(&i["idle"]);
    state.previous_position_144 = xyz(&i["previous"]).map(Some);
    (arena, actor, owner)
}
fn snap(arena: &ObjectArena, a: ObjectId, p: ObjectId, i: &Value) -> Value {
    let mut out = json!({});
    for (id, prefix) in [(a, "actor"), (p, "owner")] {
        let base = &arena.object(id).unwrap().base;
        let s = arena.actor(id).unwrap();
        let source = if s.owner_handle == arena.actor(p).unwrap().handle {
            u(&i["owner_handle"])
        } else {
            s.owner_handle.raw()
        };
        out[prefix] = json!({"flags":base.flags,"dirty":base.dirty_word,"position":base.position_words.unwrap(),"model":s.rotating.visual.model_word,"identity":s.word_6c,"source":source,"classification":s.word_b4});
    }
    //A supplied failed resolution changes the service result, not its argument.
    if i["owner_null"] == true {
        out["actor"]["source"] = i["actor_source"].clone();
    }
    let s = arena.selector66(a).unwrap();
    out["saved"] = json!(s.saved_flags_138.unwrap());
    out["elapsed"] = json!(s.elapsed_13c);
    out["idle"] = json!(s.idle_140);
    out["previous"] = json!(s.previous_position_144.map(Option::unwrap));
    out["velocity"] = json!(arena.actor(a).unwrap().words_8e_92);
    out["disposed"] = json!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|n| n.object == a));
    out
}
struct Host<'a> {
    actor: ObjectId,
    owner: ObjectId,
    input: &'a Value,
    events: Vec<Value>,
}
impl Host<'_> {
    fn event(&mut self, arena: &ObjectArena, kind: &str) -> Result<(), &'static str> {
        self.events
            .push(json!({"kind":kind,"state":snap(arena,self.actor,self.owner,self.input)}));
        if self.input["stop"] == kind {
            Err("supplied service stop")
        } else {
            Ok(())
        }
    }
}
impl Selector66Host for Host<'_> {
    type Error = &'static str;
    fn refresh_model(&mut self, arena: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        self.event(arena, "refresh")?;
        if self.input["mutate_refresh"] == true {
            arena.owned_mut(self.owner).unwrap().state.base.flags = 0x12345678;
        }
        Ok(())
    }
    fn ground_height(&mut self, arena: &mut ObjectArena, _: [u16; 2]) -> Result<u16, Self::Error> {
        self.event(arena, "ground")?;
        Ok(u(&self.input["height"]) as u16)
    }
    fn particle(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        kind: Selector66Particle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        let (name, selector) = match kind {
            Selector66Particle::Smoke(n) => ("smoke", n),
            Selector66Particle::Flash(n) => ("flash", n),
        };
        let result = self.event(arena, name);
        let event = self.events.last_mut().unwrap();
        event["selector"] = json!(selector);
        event["position"] = json!(arena.selector66_position(actor).unwrap());
        event["flags"] = json!(0);
        result?;
        if name == "smoke" && self.input["mutate_smoke"] == true {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some([81, 82, 83]);
        }
        Ok(())
    }
}
impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(u(&self.input["null"])))
    }
}
impl ActorAttachmentHost for Host<'_> {
    type Error = &'static str;
    fn create_attachment(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
        source: ActorHandle,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        assert_eq!(parent, self.owner);
        self.events.push(json!({"kind":"factory","source":source.raw(),"position":arena.selector66_position(parent).unwrap()}));
        if self.input["mutate_factory"] == true {
            let a = arena.actor_mut(parent).unwrap();
            a.word_6c = 77;
            a.word_b4 = 88;
            arena.owned_mut(parent).unwrap().state.base.position_words = Some([99, 100, 101]);
        }
        if self.input["stop"] == "factory" {
            return Err("supplied service stop");
        }
        Ok((self.input["factory_null"] != true).then_some(self.actor))
    }
    fn resolve_attachment_actor(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        assert_eq!(raw, self.actor);
        self.events.push(json!({"kind":"component"}));
        Ok((self.input["component_null"] != true).then_some(raw))
    }
    fn update_attachment(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        delta: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(actor, self.actor);
        assert_eq!(delta, 0);
        self.event(arena, "update")
    }
}

#[test]
fn selector66_attachment_update_cleanup_and_draw_match_36_native_segments() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector66_native.json"
    ))
    .unwrap();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 36);
    for (index, c) in receipt["cases"].as_array().unwrap().iter().enumerate() {
        let i = &c["input"];
        let (mut arena, actor, owner) = fixture(i);
        let mut host = Host {
            actor,
            owner,
            input: i,
            events: Vec::new(),
        };
        let success = match i["op"].as_str().unwrap() {
            "update" => arena
                .tick_selector66(actor, u(&i["delta"]), &mut host, &mut 123)
                .is_ok(),
            "cleanup" => arena.cleanup_selector66(actor).is_ok(),
            "attach" => arena
                .attach_actor_effect(
                    (i["null_parent"] != true).then_some(owner),
                    &mut host,
                    &mut 123,
                )
                .is_ok(),
            "draw" => arena
                .draw_selector66_with(actor, |arena, actual| {
                    assert_eq!(actual, owner);
                    host.event(arena, "draw")?;
                    if i["mutate_draw"] == true {
                        arena.owned_mut(owner).unwrap().state.base.position_words =
                            Some([77, 88, 99]);
                    }
                    Ok::<_, &str>(())
                })
                .is_ok(),
            _ => unreachable!(),
        };
        assert_eq!(json!(success), c["normal_return"], "case{index}");
        assert_eq!(snap(&arena, actor, owner, i), c["output"], "case{index}");
        let events: Vec<_> = c["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] != "resolve")
            .cloned()
            .collect();
        assert_eq!(host.events, events, "case{index}");
        if !success && i["op"] == "update" {
            let saved = snap(&arena, actor, owner, i);
            assert!(matches!(
                arena.tick_selector66(actor, 1, &mut host, &mut 123),
                Err(Selector66Error::PreviouslyStopped(_))
            ));
            assert_eq!(snap(&arena, actor, owner, i), saved);
        }
        if !success && i["op"] == "draw" {
            let saved = snap(&arena, actor, owner, i);
            assert!(matches!(
                arena.draw_selector66_with::<&str>(actor, |_, _| {
                    panic!("a stopped draw must not replay its callback")
                }),
                Err(Selector66Error::PreviouslyStopped(_))
            ));
            assert_eq!(snap(&arena, actor, owner, i), saved);
        }
    }
}

#[test]
fn selector66_cyclic_draw_stops_at_the_same_allocation_without_recursing() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector66_native.json"
    ))
    .unwrap();
    let (mut arena, actor, _) = fixture(&receipt["cases"][0]["input"]);
    arena.actor_mut(actor).unwrap().owner_handle = arena.actor(actor).unwrap().handle;
    let result = arena.draw_selector66_with(actor, |arena, owner| {
        assert_eq!(owner, actor);
        arena.draw_selector66_with::<&str>(owner, |_, _| {
            panic!("cyclic referent must stop before another draw callback")
        })
    });
    assert!(matches!(
        result,
        Err(Selector66Error::Service {
            error: Selector66Error::PreviouslyStopped(stopped),
            ..
        }) if stopped == actor
    ));
    assert!(arena.selector66(actor).unwrap().draw_stopped);
    assert_ne!(arena.object(actor).unwrap().base.flags & 1, 0);
}

#[test]
fn selector66_strict_storage_and_named_constructor_policy_remain_distinct() {
    let i: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector66_native.json"
    ))
    .unwrap();
    let i = &i["cases"][0]["input"];
    let (mut arena, actor, owner) = fixture(i);
    let mut host = Host {
        actor,
        owner,
        input: i,
        events: Vec::new(),
    };
    arena.selector66_mut(actor).unwrap().elapsed_13c = 1;
    arena.selector66_mut(actor).unwrap().previous_position_144 = [None; 3];
    assert!(matches!(
        arena.tick_selector66(actor, 10, &mut host, &mut 0),
        Err(Selector66Error::Unwritten { offset: 0x144, .. })
    ));
    assert_eq!(arena.selector66_position(owner).unwrap(), [100, 200, 300]);
    assert!(host.events.is_empty());
    for policy in [
        crate::object_runtime::ActorAllocationPolicy::Unowned,
        crate::object_runtime::ActorAllocationPolicy::FreshHeapZeroCarryApproximation,
        crate::object_runtime::ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
    ] {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        arena.select_actor_allocation_policy(policy).unwrap();
        let actor = arena.allocate_selector66(allocation()).unwrap();
        let s = arena.selector66(actor).unwrap();
        assert_eq!(s.saved_flags_138, None);
        assert_eq!(s.elapsed_13c, 0);
        assert_eq!(s.idle_140, 0);
        assert_eq!(s.previous_position_144,if policy==crate::object_runtime::ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation {[Some(0);3]} else {[None;3]});
    }
}

#[test]
fn selector66_retail_resource46_factory_and_complete_native_bindings_are_owned() {
    use crate::object_runtime::{
        construct_world_object, ActorFactoryInputs, BaseContext, CreationListener,
        CreationListenerEffects, CreationListenerError, CreationListeners,
        ObjectConstructionOutcome, ObjectConstructionRequest,
    };
    use inf_formats::{
        native_basis::NativeBasisMath, parse_constructor_asset_bank, FrontendExecutable,
    };
    use sha2::{Digest, Sha256};
    struct Empty;
    impl CreationListenerEffects for Empty {
        fn invoke(
            &mut self,
            listener: CreationListener,
            _: ObjectId,
            _: &mut CreationListeners,
            _: &mut ObjectArena,
        ) -> Result<(), CreationListenerError> {
            Err(CreationListenerError::UnsupportedInterface(
                listener.interface,
            ))
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let original = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&original)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let exe = FrontendExecutable::parse(&original).unwrap();
    for (offset, function) in [
        (0, 0x405190),
        (4, 0x47ada0),
        (8, 0x47a610),
        (12, 0x47ad70),
        (0x18, 0x404610),
        (0x1c, 0x47aad0),
        (0x20, 0x401320),
        (0x24, 0x4051c0),
        (0x28, 0x401350),
        (0x2c, 0x4054c0),
        (0x30, 0x4053c0),
        (0x34, 0x401310),
        (0x48, 0x4026b0),
        (0x98, 0x405490),
        (0xb0, 0x4026e0),
        (0xb4, 0x47cbc0),
    ] {
        assert_eq!(exe.word(0x51f6c0 + offset).unwrap(), function);
    }
    let mut runtime =
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    runtime.apply(crate::ResidencyRequest::Boot).unwrap();
    runtime
        .apply(crate::ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let resource = runtime.resource(13, 46).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    let record = &bank.prefixes.entries[resource.registration.local_index as usize];
    assert_eq!(record.constructor_selector, 66);
    assert_eq!(record.argument_word, 0);
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            resource_argument: 46,
            context: BaseContext {
                position_words: Some([100, 200, 300]),
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(ActorFactoryInputs {
                allocation: allocation(),
                rotation_words: None,
                local_owner: 1,
                first_player_grants: None,
                grant_table: None,
                math: &math,
                derived: None,
                listeners: &mut CreationListeners::new(),
                listener_effects: &mut Empty,
                network_world_active: false,
                terrain: None,
                world_start: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
            }),
        },
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(actor) = result else {
        panic!("actual resource46");
    };
    assert_eq!(arena.object(actor).unwrap().base.resource_word, 46);
    assert_eq!(arena.selector66_position(actor).unwrap(), [100, 200, 300]);
    assert_eq!(arena.selector66(actor).unwrap().elapsed_13c, 0);
    assert_eq!(arena.selector66(actor).unwrap().saved_flags_138, None);
    assert_eq!(
        arena.selector66(actor).unwrap().previous_position_144,
        [None; 3]
    );
}
