//! Original 4824A0/47AAF0 prefixes and complete 47B540 velocity receipts.
//! Native lower factory/component/basis/refresh returns are supplied inputs;
//! neither fixture nor host claims construction or visual residency success.

use super::super::{Family4236d0Class, RetainedActorAllocation, Selector36State};
use crate::object_runtime::{
    ActorChildFactoryCall, ActorChildHost, ActorChildRequest, ActorChildResource,
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectArena, ObjectId, RuntimeAllocation,
};
use inf_formats::{native_basis::NativeBasis, ByteSpan};
use serde_json::{json, Value};
use std::sync::Arc;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}
fn words<const N: usize, T: serde::de::DeserializeOwned>(value: &Value) -> [T; N]
where
    [T; N]: serde::de::DeserializeOwned,
{
    serde_json::from_value(value.clone()).unwrap()
}
fn enabled(input: &Value, name: &str) -> bool {
    input[name].as_bool().unwrap_or(true)
}
fn selector(arena: &mut ObjectArena, actor: ObjectId) -> &mut Selector36State {
    let Family4236d0Class::Selector36(state) =
        &mut arena.family4236d0_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state
}
fn supply(arena: &mut ObjectArena, actor: ObjectId, value: &Value) {
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = number(&value["flags"]);
    base.word_28 = number(&value["size"]) as u16;
    base.position_words = Some(words(&value["position"]));
    base.dirty_word = number(&value["dirty"]) as u16;
    let state = arena.actor_mut(actor).unwrap();
    state.rotating.visual.radius_word = number(&value["radius"]) as u16;
    state.rotating.visual.model_word = number(&value["model"]) as u16;
    state.rotating.basis.words = words(&value["basis"]);
    state.word_74 = number(&value["height"]) as u16;
    // Explicit supplied native memory, not a registry-construction claim.
    state.handle = ActorHandle::from_raw(number(&value["source"]));
    state.owner_handle = ActorHandle::from_raw(number(&value["parent_source"]));
    state.words_8e_92 = words(&value["velocity"]);
    state.word_b4 = number(&value["owner"]);
    arena.family4236d0_actor_mut(actor).unwrap().word_264 = number(&value["state_flags"]) as u16;
    selector(arena, actor).word_27e = number(&value["counter"]) as u16;
}
fn snapshot(arena: &ObjectArena, actor: ObjectId) -> Value {
    let base = &arena.object(actor).unwrap().base;
    let state = arena.actor(actor).unwrap();
    json!({
        "flags":base.flags,"size":base.word_28,"position":base.position_words.unwrap(),
        "dirty":base.dirty_word,"radius":state.rotating.visual.radius_word,
        "model":state.rotating.visual.model_word,"basis":state.rotating.basis.words,
        "height":state.word_74,"source":state.handle.raw(),
        "parent_source":state.owner_handle.raw(),"velocity":state.words_8e_92,
        "owner":state.word_b4,"state_flags":arena.family4236d0_actor(actor).unwrap().word_264,
        "counter":arena.selector36_actor(actor).unwrap().word_27e
    })
}

struct Fixture {
    arena: ObjectArena,
    parent: ObjectId,
    child: ObjectId,
    raw: ObjectId,
    payload: Arc<RuntimeAllocation>,
}
impl Fixture {
    fn new(case: &Value) -> Self {
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
                allocation_position: Some([100, 200, 300]),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &[0; 4096],
                },
            },
            world_mask_limits: [3; 2],
        };
        let mut rng = 0;
        let parent = arena.allocate_selector36_actor(request, &mut rng).unwrap();
        let child = arena.allocate_selector36_actor(request, &mut rng).unwrap();
        let raw = arena.allocate_selector36_actor(request, &mut rng).unwrap();
        supply(&mut arena, parent, &case["before"]);
        supply(&mut arena, child, &case["before_child"]);
        let input = &case["input"];
        let mut bytes = vec![0; 0x7a];
        bytes[0x74] = input["payload_flags"].as_u64().unwrap_or(0) as u8;
        bytes[0x76..0x78]
            .copy_from_slice(&(input["new_model"].as_u64().unwrap_or(321) as u16).to_le_bytes());
        bytes[0x78..0x7a]
            .copy_from_slice(&(input["resource"].as_u64().unwrap_or(58) as u16).to_le_bytes());
        let payload =
            Arc::new(RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, 0x7a)).unwrap());
        let state = selector(&mut arena, parent);
        state.payload_274 = Some(RetainedActorAllocation::new(&payload, 0).unwrap());
        state.words_278_27c = input
            .get("captured")
            .map(words::<3, u16>)
            .unwrap_or([400, 500, 600])
            .map(Some);
        Self {
            arena,
            parent,
            child,
            raw,
            payload,
        }
    }
}

struct Host<'a> {
    input: &'a Value,
    parent: ObjectId,
    child: ObjectId,
    raw: ObjectId,
    payload: Arc<RuntimeAllocation>,
    events: Vec<Value>,
    child_wrapper: bool,
}
impl Host<'_> {
    fn stop(&self, event: &'static str) -> Result<(), &'static str> {
        if self.input["stop"].as_str() == Some(event) {
            Err(event)
        } else {
            Ok(())
        }
    }
    fn mutate(&self, arena: &mut ObjectArena, name: &str) {
        let Some(values) = self.input[name].as_object() else {
            return;
        };
        for (field, value) in values {
            match field.as_str() {
                "basis" => {
                    arena.actor_mut(self.parent).unwrap().rotating.basis.words = words(value)
                }
                "position" => {
                    arena
                        .owned_mut(self.parent)
                        .unwrap()
                        .state
                        .base
                        .position_words = Some(words(value))
                }
                "child_basis" => {
                    arena.actor_mut(self.child).unwrap().rotating.basis.words = words(value)
                }
                "state_flags" => {
                    arena.family4236d0_actor_mut(self.parent).unwrap().word_264 =
                        number(value) as u16
                }
                "size" => {
                    arena.owned_mut(self.parent).unwrap().state.base.word_28 = number(value) as u16
                }
                "payload_flags" => self.payload.write_word(0x74, number(value) as u16).unwrap(),
                "new_model" => self.payload.write_word(0x76, number(value) as u16).unwrap(),
                "source" => {
                    arena.actor_mut(self.parent).unwrap().handle =
                        ActorHandle::from_raw(number(value))
                }
                "owner" => arena.actor_mut(self.parent).unwrap().word_b4 = number(value),
                "flags" => arena.owned_mut(self.parent).unwrap().state.base.flags = number(value),
                other => panic!("unknown supplied mutation {other}"),
            }
        }
    }
}
impl ActorChildHost for Host<'_> {
    type Error = &'static str;
    fn create_child_object(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorChildFactoryCall,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        assert_eq!(request.parent, self.parent);
        let mut event = json!({
            "kind":"create","source":0,"position":arena.object(self.parent).unwrap().base.position_words.unwrap(),
            "rotation":0,"index":u32::MAX,"flags":u32::from(request.spawn_mode_1),
            "process_null":self.input["process_null"].as_u64().unwrap_or(0x34567890)
        });
        match (self.child_wrapper, request.resource) {
            (true, ActorChildResource::StackPositionLowWord(word)) => {
                event["resource_word"] = json!(word)
            }
            (false, ActorChildResource::Full(argument)) => event["resource"] = json!(argument),
            other => panic!("resource custody changed: {other:?}"),
        }
        self.events.push(event);
        self.stop("create")?;
        self.mutate(
            arena,
            if self.child_wrapper {
                "after_factory"
            } else {
                "after_create"
            },
        );
        Ok(enabled(self.input, "raw").then_some(self.raw))
    }
    fn resolve_child_actor(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        assert_eq!(object, self.raw);
        self.events.push(json!({"kind":"resolve"}));
        self.stop("resolve")?;
        self.mutate(arena, "after_resolve");
        let field = if self.child_wrapper {
            "child"
        } else {
            "component"
        };
        Ok(enabled(self.input, field).then_some(self.child))
    }
    fn child_source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<NativeBasis>, Self::Error> {
        assert_eq!(parent, self.parent);
        let child = arena.actor(self.child).unwrap();
        let mut event = json!({
            "kind":"basis","child_owner":child.word_b4,"child_source":child.owner_handle.raw(),
            "child_dirty":arena.object(self.child).unwrap().base.dirty_word
        });
        if !self.child_wrapper {
            // The out-slot itself is checked after return/error below. This
            // native field records whether an optional slot was requested.
            event["out_child"] = json!(enabled(self.input, "out"));
        }
        self.events.push(event);
        self.stop("basis")?;
        self.mutate(arena, "at_basis");
        Ok(enabled(self.input, "basis_present")
            .then(|| arena.actor(parent).unwrap().rotating.basis))
    }
    fn create_registry_object(
        &mut self,
        _: &mut ObjectArena,
        _: u16,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Err("the selector-36 child builds no registry object")
    }
    fn take_shot(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: crate::object_runtime::ActorHandle,
    ) -> Result<bool, Self::Error> {
        Err("the selector-36 child gives no player a shot")
    }
    fn refresh_child_parent_model(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<(), Self::Error> {
        assert_eq!(parent, self.parent);
        let actor = arena.actor(parent).unwrap();
        self.events.push(json!({
            "kind":"refresh","model":actor.rotating.visual.model_word,
            "state_flags":arena.family4236d0_actor(parent).unwrap().word_264,
            "dirty":arena.object(parent).unwrap().base.dirty_word,"height":actor.word_74,
            "counter":arena.selector36_actor(parent).unwrap().word_27e
        }));
        self.stop("refresh")?;
        self.mutate(arena, "after_refresh");
        Ok(())
    }
}

#[test]
fn selector36_child_native_creation_and_model_prefixes_match() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector36_child_native.json"
    )))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 25);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let mut f = Fixture::new(case);
        let mut host = Host {
            input: &case["input"],
            parent: f.parent,
            child: f.child,
            raw: f.raw,
            payload: Arc::clone(&f.payload),
            events: vec![],
            child_wrapper: true,
        };
        let mut rng = 0xdeadbeef;
        let result = f
            .arena
            .spawn_selector36_child(f.parent, &mut host, &mut rng);
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case {index}: {result:?}"
        );
        assert_eq!(
            host.events,
            case["events"].as_array().unwrap().as_slice(),
            "case {index} events"
        );
        assert_eq!(
            snapshot(&f.arena, f.parent),
            case["parent"],
            "case {index} parent"
        );
        assert_eq!(
            snapshot(&f.arena, f.child),
            case["child"],
            "case {index} child"
        );
        assert_eq!(rng, 0xdeadbeef, "supplied lower services draw no RNG");
    }
}

#[test]
fn actor_child_factory_native_raw_component_and_source_order_match() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/actor_child_factory_native.json"
    )))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 19);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let mut f = Fixture::new(case);
        let input = &case["input"];
        let mut host = Host {
            input,
            parent: f.parent,
            child: f.child,
            raw: f.raw,
            payload: Arc::clone(&f.payload),
            events: vec![],
            child_wrapper: false,
        };
        let mut output = Some(f.parent); // supplied poison distinct from child/null
        let request = ActorChildRequest {
            resource: ActorChildResource::Full(
                input["resource"].as_u64().unwrap_or(0x1234003a) as u32
            ),
            position: enabled(input, "override").then(|| {
                input
                    .get("override_position")
                    .map(words)
                    .unwrap_or([1100, 2200, 3300])
            }),
        };
        let mut rng = 0xdeadbeef;
        let result = f.arena.create_actor_child(
            f.parent,
            request,
            enabled(input, "out").then_some(&mut output),
            &mut host,
            &mut rng,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case {index}: {result:?}"
        );
        if let Ok(raw) = result {
            let expected_raw = case["raw_return"].as_bool().unwrap().then_some(f.raw);
            assert_eq!(raw, expected_raw, "case {index} raw");
        }
        let output = if output == Some(f.child) {
            "child"
        } else if output.is_none() {
            "null"
        } else {
            "poison"
        };
        assert_eq!(
            output,
            case["out_child"].as_str().unwrap(),
            "case {index} out-slot"
        );
        assert_eq!(
            host.events,
            case["events"].as_array().unwrap().as_slice(),
            "case {index} events"
        );
        assert_eq!(
            snapshot(&f.arena, f.parent),
            case["parent"],
            "case {index} parent"
        );
        assert_eq!(
            snapshot(&f.arena, f.child),
            case["child"],
            "case {index} child"
        );
        assert_eq!(rng, 0xdeadbeef);
    }
}

#[test]
fn actor_child_velocity_native_transform_and_word_additions_match() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/actor_child_velocity_native.json"
    )))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 9);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let mut supplied = case.clone();
        supplied["before_child"] = case["before"].clone();
        let mut f = Fixture::new(&supplied);
        f.arena
            .add_actor_local_linear_velocity(f.child, words(&case["input"]["local"]))
            .unwrap();
        let state = f.arena.actor(f.child).unwrap();
        assert_eq!(json!(state.words_8e_92), case["velocity"], "case {index}");
        assert_eq!(
            json!(state.rotating.basis.words),
            case["basis"],
            "case {index}"
        );
        let mut expected = case["before"].clone();
        expected["velocity"] = case["velocity"].clone();
        assert_eq!(
            snapshot(&f.arena, f.child),
            expected,
            "case {index}: unrelated writes"
        );
    }
}
