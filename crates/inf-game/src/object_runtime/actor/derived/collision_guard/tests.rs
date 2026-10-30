use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, Selector12GrantContext,
};
use serde_json::{json, Value};
use std::{cell::Cell, rc::Rc};

const SINE: [u16; 4096] = [0; 4096];

#[derive(Clone, Copy)]
enum Class {
    Derived,
    Family428650,
    Family429e20,
    Family4236d0,
    Selector10,
    Selector12,
    Selector34,
    Selector35,
    Selector36,
}

fn actor_fixture(class: Class) -> (ObjectArena, ObjectId) {
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
            allocation_position: Some([65530, 32768, 7]),
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
    let actor = match class {
        Class::Derived => arena.allocate_derived_actor(request, &mut 0),
        Class::Family428650 => arena.allocate_family428650_actor(request, &mut 0),
        Class::Family429e20 => arena.allocate_family429e20_actor(request, &mut 0),
        Class::Family4236d0 => arena.allocate_family4236d0_actor(request, &mut 0),
        Class::Selector10 => arena.allocate_selector10_actor(request, &mut 0),
        Class::Selector12 => arena.allocate_selector12_actor(
            request,
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                    records: vec![],
                },
                first_player_grants: None,
            },
        ),
        Class::Selector34 => arena.allocate_selector34_actor(request, &mut 0),
        Class::Selector35 => arena.allocate_selector35_actor(request, &mut 0),
        Class::Selector36 => arena.allocate_selector36_actor(request, &mut 0),
    }
    .unwrap();
    (arena, actor)
}

fn snapshot(arena: &ObjectArena, actor: ObjectId) -> Value {
    json!({
        "flags": arena.family428650_actor(actor).unwrap().word_22e,
        "position": arena.object(actor).unwrap().base.position_words,
        "velocity": arena.actor(actor).unwrap().words_8e_92,
        "dirty": arena.object(actor).unwrap().base.dirty_word,
        "byte_160": arena.derived_actor(actor).unwrap().byte_160,
        "descriptor": arena.family428650_actor(actor).unwrap().word_228,
    })
}

fn mutate(arena: &mut ObjectArena, actor: ObjectId, mutation: &Value) {
    if let Some(flags) = mutation["flags"].as_u64() {
        arena.family428650_actor_mut(actor).unwrap().word_22e = flags as u16;
    }
    if let Some(position) = mutation.get("position") {
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(std::array::from_fn(|i| {
                position[i].as_u64().unwrap() as u16
            }));
    }
    if let Some(velocity) = mutation.get("velocity") {
        arena.actor_mut(actor).unwrap().words_8e_92 =
            std::array::from_fn(|i| velocity[i].as_u64().unwrap() as u16);
    }
    if let Some(dirty) = mutation["dirty"].as_u64() {
        arena.owned_mut(actor).unwrap().state.base.dirty_word = dirty as u16;
    }
    if let Some(mode) = mutation["byte_160"].as_u64() {
        arena.derived_actor_mut(actor).unwrap().byte_160 = mode as u8;
    }
    if let Some(descriptor) = mutation["descriptor"].as_u64() {
        arena.family428650_actor_mut(actor).unwrap().word_228 = descriptor as u32;
    }
}

struct Host<'a> {
    spec: &'a Value,
    events: Vec<Value>,
}

impl FamilyCollisionGuardHost<[u32; 4]> for Host<'_> {
    type Error = &'static str;

    fn continue_collision(
        &mut self,
        arena: &mut ObjectArena,
        request: &mut FamilyCollisionGuardRequest<[u32; 4]>,
    ) -> Result<u32, Self::Error> {
        let actor = request.actor;
        let before = snapshot(arena, actor);
        mutate(arena, actor, &self.spec["mutation"]);
        self.events.push(json!({
            "service": "continuation", "operands": request.continuation_operands,
            "before": before, "after": snapshot(arena, actor),
        }));
        if self.spec["stop_at"].is_null() {
            Ok(self.spec["continuation_eax"].as_u64().unwrap() as u32)
        } else {
            Err("supplied failure")
        }
    }
}

#[test]
fn native_guard_and_forwarding_preserve_operands_al_and_mutation_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_collision_guard_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 78);
    assert_eq!(
        fixture["audit"]["guard_pointer_references"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    for (index, spec) in cases.iter().enumerate() {
        let (mut arena, actor) = actor_fixture(Class::Family429e20);
        mutate(
            &mut arena,
            actor,
            &json!({"flags":spec["flags"], "position":[65530,32768,7],
                "velocity":[65535,32,32767], "dirty":0x81,
                "byte_160":0x80, "descriptor":0x531200}),
        );
        let operands = std::array::from_fn(|i| spec["operands"][i].as_u64().unwrap() as u32);
        let mut request = FamilyCollisionGuardRequest {
            actor,
            continuation_operands: operands,
        };
        let mut host = Host {
            spec,
            events: vec![],
        };
        let result = arena.dispatch_family_collision_guard(&mut request, &mut host);
        if spec["stop_at"].is_null() {
            assert_eq!(
                result.unwrap(),
                spec["result"].as_u64().unwrap() as u8,
                "case {index}"
            );
        } else {
            assert_eq!(
                result,
                Err(FamilyCollisionGuardError::Host("supplied failure")),
                "case {index}"
            );
        }
        assert_eq!(request.continuation_operands, operands);
        assert_eq!(
            &host.events,
            spec["events"].as_array().unwrap(),
            "events case {index}"
        );
        assert_eq!(snapshot(&arena, actor), spec["final"], "final case {index}");
    }
}

/// Deliberately non-Clone opaque custody. The guard must not send it to a
/// continuation, require actor XYZ, or prematurely release caller custody.
struct UnownedOperands(Rc<Cell<u32>>);
impl Drop for UnownedOperands {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
struct ForbiddenContinuation;
impl FamilyCollisionGuardHost<UnownedOperands> for ForbiddenContinuation {
    type Error = &'static str;

    fn continue_collision(
        &mut self,
        _: &mut ObjectArena,
        _: &mut FamilyCollisionGuardRequest<UnownedOperands>,
    ) -> Result<u32, Self::Error> {
        panic!("guard/admission must not read or acquire unowned collision operands")
    }
}

#[test]
fn guard_routes_all_represented_vtables_without_position_or_operand_acquisition() {
    for class in [
        Class::Family429e20,
        Class::Family4236d0,
        Class::Selector10,
        Class::Selector12,
        Class::Selector36,
    ] {
        let (mut arena, actor) = actor_fixture(class);
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xffff;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let drops = Rc::new(Cell::new(0));
        let mut request = FamilyCollisionGuardRequest {
            actor,
            continuation_operands: UnownedOperands(Rc::clone(&drops)),
        };
        assert_eq!(
            arena.dispatch_family_collision_guard(&mut request, &mut ForbiddenContinuation),
            Ok(1)
        );
        assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x7fff);
        assert_eq!(arena.object(actor).unwrap().base.position_words, None);
        assert_eq!(drops.get(), 0);
        drop(request);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn admission_does_not_apply_guard_to_base_or_selector34_35_callbacks() {
    for class in [
        Class::Derived,
        Class::Family428650,
        Class::Selector34,
        Class::Selector35,
    ] {
        let (mut arena, actor) = actor_fixture(class);
        if !matches!(class, Class::Derived) {
            arena.family428650_actor_mut(actor).unwrap().word_22e = 0x8001;
        }
        let drops = Rc::new(Cell::new(0));
        let mut request = FamilyCollisionGuardRequest {
            actor,
            continuation_operands: UnownedOperands(Rc::clone(&drops)),
        };
        assert!(matches!(
            arena.dispatch_family_collision_guard(&mut request, &mut ForbiddenContinuation),
            Err(FamilyCollisionGuardError::Object(_))
        ));
        if !matches!(class, Class::Derived) {
            assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x8001);
        }
        assert_eq!(drops.get(), 0);
    }
}
