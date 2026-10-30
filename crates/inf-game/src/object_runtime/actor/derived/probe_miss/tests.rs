use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest,
    },
    terrain_runtime::native_probe_fixture_terrain,
};
use serde_json::{json, Value};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn actor_fixture(spec: &Value) -> (ObjectArena, ObjectId) {
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
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(std::array::from_fn(|i| {
        spec["position"][i].as_u64().unwrap() as u16
    }));
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x81;
    arena.family428650_actor_mut(actor).unwrap().word_22e = spec["flags"].as_u64().unwrap() as u16;
    arena.derived_actor_mut(actor).unwrap().byte_160 = spec["byte_160"].as_u64().unwrap() as u8;
    (arena, actor)
}

fn snapshot(arena: &ObjectArena, actor: ObjectId) -> Value {
    json!({
        "position": arena.object(actor).unwrap().base.position_words,
        "flags": arena.family428650_actor(actor).unwrap().word_22e,
        "byte_160": arena.derived_actor(actor).unwrap().byte_160,
        "dirty": arena.object(actor).unwrap().base.dirty_word,
    })
}

fn mutate(arena: &mut ObjectArena, actor: ObjectId, mutation: &Value) {
    if let Some(position) = mutation.get("position") {
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(std::array::from_fn(|i| {
                position[i].as_u64().unwrap() as u16
            }));
    }
    if let Some(flags) = mutation["flags"].as_u64() {
        arena.family428650_actor_mut(actor).unwrap().word_22e = flags as u16;
    }
    if let Some(mode) = mutation["byte_160"].as_u64() {
        arena.derived_actor_mut(actor).unwrap().byte_160 = mode as u8;
    }
    if let Some(dirty) = mutation["dirty"].as_u64() {
        arena.owned_mut(actor).unwrap().state.base.dirty_word = dirty as u16;
    }
}

struct Host<'a> {
    spec: &'a Value,
    terrain: TerrainRuntime,
    events: Vec<Value>,
}

impl Host<'_> {
    fn service(
        &mut self,
        name: &str,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), &'static str> {
        let before = snapshot(arena, actor);
        let mutation = self
            .spec
            .get(format!("{name}_mutation"))
            .cloned()
            .unwrap_or(Value::Null);
        mutate(arena, actor, &mutation);
        self.events
            .push(json!({ "service": name, "before": before, "after": snapshot(arena, actor) }));
        if self.spec["stop_at"].as_str() == Some(name) {
            Err("supplied failure")
        } else {
            Ok(())
        }
    }
}

impl FamilyTerrainProbeMissHost for Host<'_> {
    type Error = &'static str;

    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        if self.spec["acquire"].as_bool().unwrap_or(false) {
            self.service("acquire", arena, actor)?;
        }
        Ok(&self.terrain)
    }

    fn collide(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error> {
        self.service("collision", arena, actor)
    }
}

#[test]
fn native_miss_matches_temporary_guard_and_service_mutation_prefixes() {
    // Both terrain routine and miss wrapper execute unchanged in the checker;
    // only H12 acquisition and actual collision are explicit supplied hosts.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/terrain_slope_probe_native.json"
    ))
    .unwrap();
    let cases = fixture["miss_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 168);
    for (index, spec) in cases.iter().enumerate() {
        let (mut arena, actor) = actor_fixture(spec);
        let mut host = Host {
            spec,
            terrain: native_probe_fixture_terrain(spec),
            events: Vec::new(),
        };
        let result = arena.probe_family_terrain_miss(
            actor,
            std::array::from_fn(|i| spec["xz"][i].as_u64().unwrap() as u16),
            &mut host,
        );
        if spec["stop_at"].is_null() {
            assert_eq!(
                result.unwrap(),
                spec["result"].as_u64().unwrap() != 0,
                "case {index}"
            );
        } else {
            assert!(
                matches!(result, Err(FamilyTerrainProbeMissError::Host { .. })),
                "case {index}: {result:?}"
            );
        }
        assert_eq!(
            &host.events,
            spec["events"].as_array().unwrap(),
            "events case {index}"
        );
        assert_eq!(snapshot(&arena, actor), spec["final"], "final case {index}");
    }
}

#[test]
fn blocked_terrain_never_reads_missing_actor_position_or_sets_guard() {
    let spec = json!({"spacing":128, "height_shift":5, "plane_shift":8,
        "xz":[32768,65535], "heights":[0,0,0,0], "alternate":false,
        "plane_base":256, "position":[1,2,3], "flags":13, "byte_160":0});
    let (mut arena, actor) = actor_fixture(&spec);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut host = Host {
        spec: &spec,
        terrain: native_probe_fixture_terrain(&spec),
        events: Vec::new(),
    };
    assert_eq!(
        arena.probe_family_terrain_miss(actor, [32768, 65535], &mut host),
        Ok(true)
    );
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 13);
    assert!(host.events.is_empty());
}

#[test]
fn clear_terrain_sets_guard_before_missing_position_failure() {
    let spec = json!({"spacing":128, "height_shift":5, "plane_shift":8,
        "xz":[32768,65535], "heights":[0,0,0,0], "alternate":false,
        "plane_base":0, "position":[1,2,3], "flags":13, "byte_160":8});
    let (mut arena, actor) = actor_fixture(&spec);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut host = Host {
        spec: &spec,
        terrain: native_probe_fixture_terrain(&spec),
        events: Vec::new(),
    };
    assert_eq!(
        arena.probe_family_terrain_miss(actor, [32768, 65535], &mut host),
        Err(FamilyTerrainProbeMissError::MissingPosition(actor))
    );
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x800d);
    assert!(host.events.is_empty());
}
