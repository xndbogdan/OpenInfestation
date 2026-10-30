//! Original 00402250 receipts own ordering; hosts are supplied boundaries.

use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| number(&value[axis]) as u16)
}

fn supplied_actor(case: &Value) -> (ObjectArena, ObjectId) {
    static SINE: [u16; 4096] = [0; 4096];
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_selector11_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some(words(&case["position"])),
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
            &mut 0x1234_5678,
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.flags = number(&case["flags"]);
    arena.actor_mut(actor).unwrap().byte_78 = number(&case["index"]) as u8;
    arena.actor_mut(actor).unwrap().word_c4 = 1;
    arena.actor_mut(actor).unwrap().health = 0;
    let DerivedActorClass::Selector11(state) = &mut arena.derived_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state.word_1b2 = number(&case["word_1b2"]) as u16;
    (arena, actor)
}

fn snapshot(arena: &ObjectArena, actor: ObjectId) -> Value {
    json!({
        "flags":arena.object(actor).unwrap().base.flags,
        "position":arena.object(actor).unwrap().base.position_words.unwrap(),
        "index":arena.actor(actor).unwrap().byte_78,
        "word_1b2":arena.selector11_actor(actor).unwrap().word_1b2,
    })
}

struct Host<'a> {
    case: &'a Value,
    actor: ObjectId,
    events: Vec<Value>,
}
impl Host<'_> {
    fn event(&self, arena: &ObjectArena, kind: &'static str) -> Value {
        let mut event = snapshot(arena, self.actor);
        event["kind"] = json!(kind);
        event
    }
    fn finish(
        &mut self,
        arena: &mut ObjectArena,
        event: Value,
        kind: &'static str,
    ) -> Result<(), &'static str> {
        self.events.push(event);
        let mutation = &self.case[format!("after_{kind}")];
        if !mutation["flags"].is_null() {
            arena.owned_mut(self.actor).unwrap().state.base.flags = number(&mutation["flags"]);
        }
        if !mutation["position"].is_null() {
            arena
                .owned_mut(self.actor)
                .unwrap()
                .state
                .base
                .position_words = Some(words(&mutation["position"]));
        }
        if !mutation["index"].is_null() {
            arena.actor_mut(self.actor).unwrap().byte_78 = number(&mutation["index"]) as u8;
        }
        if self.case["fail"].as_str() == Some(kind) {
            Err(kind)
        } else {
            Ok(())
        }
    }
}
impl Selector11RetirementHost for Host<'_> {
    type Error = &'static str;
    fn world_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Selector11RadiusOrigin>,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(request.origin.actor, self.actor);
        assert_eq!(
            request.origin.position_words(arena).unwrap(),
            arena
                .object(self.actor)
                .unwrap()
                .base
                .position_words
                .unwrap()
        );
        let kind = match request.origin.block {
            Selector11RadiusBlock::Actor1A0 => "first_radius",
            Selector11RadiusBlock::Executable531250 => "second_radius",
        };
        let mut event = self.event(arena, kind);
        event["source"] = json!(request.damage_source.raw());
        event["replicate"] = json!(request.replicate);
        self.finish(arena, event, kind)
    }
    fn birth_burst(
        &mut self,
        arena: &mut ObjectArena,
        burst: ParticleBurst,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        let mut event = self.event(arena, "burst");
        event["burst"] = json!({"debris_count":burst.debris_count,"position":burst.position,"placement_radius":burst.placement_radius,
            "central_type":burst.central_type,"debris_types":burst.debris_types,"speed":burst.speed,"flags":burst.flags});
        self.finish(arena, event, "burst")
    }
    fn scatter_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        argument: i32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(actor, self.actor);
        let mut event = self.event(arena, "scatter");
        event["argument"] = json!(argument);
        self.finish(arena, event, "scatter")
    }
    fn placement_death(
        &mut self,
        arena: &mut ObjectArena,
        index: i8,
        force: i32,
    ) -> Result<(), Self::Error> {
        let mut event = self.event(arena, "placement");
        event["placement_index"] = json!(index);
        event["force"] = json!(force);
        self.finish(arena, event, "placement")
    }
}

#[test]
fn selector11_retirement_matches_native_order_mutations_and_failure_prefixes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector11_retirement_native.json"
    ))
    .unwrap();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 13);
    for case in receipt["cases"].as_array().unwrap() {
        let (mut arena, actor) = supplied_actor(case);
        let mut host = Host {
            case,
            actor,
            events: Vec::new(),
        };
        let mut rng = number(&case["rng"]);
        let result = arena.retire_selector11(
            actor,
            ActorHandle::from_raw(number(&case["source"])),
            &mut host,
            &mut rng,
        );
        if case["failed"] == true {
            assert!(
                matches!(result,Err(Selector11RetirementError::Host(kind)) if Some(kind)==case["fail"].as_str()),
                "{}: {result:?}",
                case["name"]
            );
        } else {
            result.unwrap();
        }
        assert_eq!(json!(host.events), case["events"], "{}", case["name"]);
        assert_eq!(
            snapshot(&arena, actor),
            case["final_state"],
            "{}",
            case["name"]
        );
        assert_eq!(rng, number(&case["rng"]));
        assert_eq!(arena.actor(actor).unwrap().word_c4, 1);
        assert_eq!(arena.actor(actor).unwrap().health, 0);
    }
}

#[test]
fn live_radius_queries_keep_unknown_byte_and_original_executable_block() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector11_retirement_native.json"
    ))
    .unwrap();
    let (mut arena, actor) = supplied_actor(&receipt["cases"][0]);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut source = Selector11RadiusOrigin {
        actor,
        block: Selector11RadiusBlock::Actor1A0,
    };
    let first = source.effect_block(&arena, exe).unwrap();
    assert_eq!(first.byte_03, None);
    let DerivedActorClass::Selector11(state) = &mut arena.derived_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state.word_1a0 = 0xff80;
    state.byte_1a3 = Some(239);
    state.word_1a6 = 600;
    let reread = source.effect_block(&arena, exe).unwrap();
    assert_eq!(reread.strength, -128);
    assert_eq!(reread.byte_03, Some(239));
    assert_eq!(reread.outer_radius, 600);
    source.block = Selector11RadiusBlock::Executable531250;
    let bytes: Vec<u8> = serde_json::from_value(receipt["executable_block"].clone()).unwrap();
    assert_eq!(
        source.effect_block(&arena, exe).unwrap(),
        RadiusEffectBlock::from_authored_bytes(bytes.try_into().unwrap())
    );
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([4, 5, 6]);
    assert_eq!(source.position_words(&arena).unwrap(), [4, 5, 6]);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert!(
        matches!(source.position_words(&arena),Err(Selector11RadiusError::MissingPosition(value)) if value==actor)
    );
}

#[test]
fn retirement_missing_position_does_not_invent_burst_or_tail_writes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector11_retirement_native.json"
    ))
    .unwrap();
    let case = &receipt["cases"][1]; //alternate flag skips the first radius call.
    let (mut arena, actor) = supplied_actor(case);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut host = Host {
        case,
        actor,
        events: Vec::new(),
    };
    assert!(
        matches!(arena.retire_selector11(actor,ActorHandle::from_raw(0),&mut host,&mut 1),Err(Selector11RetirementError::MissingPosition(value)) if value==actor)
    );
    assert!(host.events.is_empty());
    assert_eq!(arena.selector11_actor(actor).unwrap().word_1b2, 123);
}
