use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use serde_json::Value;

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn fixture(spec: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_family429e20_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([65530, 20, 30]),
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
            },
            &mut 0,
        )
        .unwrap();
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = spec["size"].as_u64().unwrap_or(400) as u16;
    base.dirty_word = 0x81;
    if let Some(words) = spec["position"].as_array() {
        base.position_words = Some(std::array::from_fn(|i| words[i].as_u64().unwrap() as u16));
    }
    if spec["missing_position"].as_bool().unwrap_or(false) {
        base.position_words = None;
    }
    arena.actor_mut(actor).unwrap().words_94_9c[1] = spec["angular"].as_i64().unwrap_or(0) as u32;
    arena.family428650_actor_mut(actor).unwrap().word_22e =
        spec["initial_flags"].as_u64().unwrap_or(12) as u16;
    arena.family429e20_actor_mut(actor).unwrap().word_238 =
        spec["timer"].as_u64().unwrap_or(0) as u16;
    (arena, actor)
}

fn mutate(arena: &mut ObjectArena, actor: ObjectId, mutation: &Value) {
    let Some(fields) = mutation.as_object() else {
        return;
    };
    for (name, value) in fields {
        match name.as_str() {
            "flags" => {
                arena.family428650_actor_mut(actor).unwrap().word_22e =
                    value.as_u64().unwrap() as u16
            }
            "timer" => {
                arena.family429e20_actor_mut(actor).unwrap().word_238 =
                    value.as_u64().unwrap() as u16
            }
            "size" => {
                arena.owned_mut(actor).unwrap().state.base.word_28 = value.as_u64().unwrap() as u16
            }
            "angular" => {
                arena.actor_mut(actor).unwrap().words_94_9c[1] = value.as_i64().unwrap() as u32
            }
            "position" => {
                arena.owned_mut(actor).unwrap().state.base.position_words =
                    Some(std::array::from_fn(|i| value[i].as_u64().unwrap() as u16))
            }
            _ => panic!("unsupported supplied mutation {name}"),
        }
    }
}

struct Host<'a> {
    spec: &'a Value,
    events: Vec<Value>,
}

impl FamilyAvoidanceHost for Host<'_> {
    type Error = &'static str;
    fn scan(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        reverse: bool,
    ) -> Result<(), Self::Error> {
        self.events.push(serde_json::json!(["scan", reverse]));
        arena.family428650_actor_mut(actor).unwrap().word_22e =
            self.spec["scan_flags"].as_u64().unwrap() as u16;
        mutate(arena, actor, &self.spec["scan_mutation"]);
        if self.spec["failure"] == "scan" {
            Err("scan")
        } else {
            Ok(())
        }
    }
    fn basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        self.events.push(serde_json::json!(["basis"]));
        mutate(arena, actor, &self.spec["basis_mutation"]);
        if self.spec["failure"] == "basis" {
            return Err("basis");
        }
        Ok(if let Some(words) = self.spec["basis"].as_array() {
            NativeBasis {
                words: std::array::from_fn(|i| words[i].as_u64().unwrap() as u32),
            }
        } else {
            NativeBasis::INITIAL
        })
    }
}

fn receipt() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_avoidance_native.json"
    )))
    .unwrap()
}

#[test]
fn zero_forward_does_not_require_any_actor_state_or_service() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(crate::object_runtime::ObjectClassKind::Base);
    let spec = serde_json::json!({});
    let mut host = Host {
        spec: &spec,
        events: vec![],
    };
    let mut controls = [17, 0];
    assert_eq!(
        arena.adjust_family_avoidance(object, &mut controls, u32::MAX, &mut host),
        Ok(false)
    );
    assert_eq!(controls, [17, 0]);
    assert!(host.events.is_empty());
    assert_eq!(arena.object(object).unwrap().base.position_words, None);
}

#[test]
fn original_flag_matrix_preserves_all_probe_combinations_and_both_directions() {
    let native = receipt();
    let cases = native["matrix"].as_array().unwrap();
    assert_eq!(cases.len(), 1024);
    for (ordinal, expected) in cases.iter().enumerate() {
        let spec = serde_json::json!({
            "controls": [99, if ordinal % 2 == 0 { 777 } else { -777 }],
            "microseconds": 50000,
            "scan_flags": ((ordinal / 2) << 6) | 12,
        });
        let (mut arena, actor) = fixture(&spec);
        let mut host = Host {
            spec: &spec,
            events: vec![],
        };
        let mut controls = [99, spec["controls"][1].as_i64().unwrap() as i32];
        let result = arena
            .adjust_family_avoidance(actor, &mut controls, 50000, &mut host)
            .unwrap();
        let base = &arena.object(actor).unwrap().base;
        let position = base.position_words.unwrap();
        let actual = serde_json::json!([
            arena.family428650_actor(actor).unwrap().word_22e,
            controls[0],
            controls[1],
            arena.family429e20_actor(actor).unwrap().word_238,
            position[0],
            position[1],
            position[2],
            u8::from(result),
            host.events.len(),
        ]);
        assert_eq!(&actual, expected, "original flag/sign ordinal {ordinal}");
        assert_eq!(
            base.dirty_word, 0x81,
            "direct movement does not set dirty bits"
        );
        assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
    }
}

#[test]
fn original_avoidance_edges_and_failure_prefixes_match() {
    let native = receipt();
    let cases = native["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 88);
    for (ordinal, expected) in cases.iter().enumerate() {
        let spec = &expected["input"];
        let (mut arena, actor) = fixture(spec);
        let mut host = Host {
            spec,
            events: vec![],
        };
        let mut controls = std::array::from_fn(|i| spec["controls"][i].as_i64().unwrap() as i32);
        let result = arena.adjust_family_avoidance(
            actor,
            &mut controls,
            spec["microseconds"].as_u64().unwrap() as u32,
            &mut host,
        );
        let (handled, error) = match result {
            Ok(handled) => (Some(handled), None),
            Err(FamilyAvoidanceError::Host { error, .. }) => (None, Some(error)),
            Err(FamilyAvoidanceError::MissingPosition(_)) => (None, Some("position")),
            Err(error) => panic!("unexpected error {error:?}"),
        };
        assert_eq!(
            serde_json::json!(handled),
            expected["result"],
            "result {ordinal}"
        );
        assert_eq!(
            serde_json::json!(error),
            expected["error"],
            "failure {ordinal}"
        );
        assert_eq!(
            serde_json::json!(host.events),
            expected["events"],
            "effects {ordinal}"
        );
        assert_eq!(
            serde_json::json!(controls),
            expected["controls"],
            "controls {ordinal}"
        );
        assert_eq!(
            serde_json::json!(arena.family428650_actor(actor).unwrap().word_22e),
            expected["flags"],
            "flags {ordinal}"
        );
        assert_eq!(
            serde_json::json!(arena.family429e20_actor(actor).unwrap().word_238),
            expected["timer"],
            "timer {ordinal}"
        );
        let base = &arena.object(actor).unwrap().base;
        assert_eq!(serde_json::json!(base.dirty_word), expected["dirty"]);
        if !spec["missing_position"].as_bool().unwrap_or(false) {
            assert_eq!(
                serde_json::json!(base.position_words.unwrap()),
                expected["position"],
                "position {ordinal}"
            );
        } else {
            assert!(base.position_words.is_none());
        }
    }
}
