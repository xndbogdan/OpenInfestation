use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, TerrainProbeAllocation, TerrainProbeCache,
};
use serde_json::{json, Value};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn mutate(arena: &mut ObjectArena, actor: ObjectId, fields: &Value) {
    let Some(fields) = fields.as_object() else {
        return;
    };
    for (name, value) in fields {
        match name.as_str() {
            "flags" => {
                arena.family428650_actor_mut(actor).unwrap().word_22e =
                    value.as_u64().unwrap() as u16
            }
            "position" => {
                arena.owned_mut(actor).unwrap().state.base.position_words = value
                    .as_array()
                    .map(|v| std::array::from_fn(|i| v[i].as_u64().unwrap() as u16))
            }
            "cache" => {
                arena.family429e20_actor_mut(actor).unwrap().probe_234 = value
                    .as_bool()
                    .unwrap()
                    .then(|| Box::new(TerrainProbeAllocation::new(TerrainProbeCache::new(8))))
            }
            "basis" => {
                let basis = &mut arena.actor_mut(actor).unwrap().rotating.basis;
                basis.words[7] = value[0].as_u64().unwrap() as u32;
                basis.words[9] = value[1].as_u64().unwrap() as u32;
            }
            _ => panic!("unowned supplied mutation {name}"),
        }
    }
}

struct Host<'a> {
    spec: &'a Value,
    events: Vec<Value>,
}
impl FamilyObstacleProbeHost for Host<'_> {
    type Error = ();
    fn probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        point: [u16; 2],
    ) -> Result<u32, ()> {
        let ordinal = self.events.len();
        self.events.push(json!([
            point[0],
            point[1],
            arena.family428650_actor(actor).unwrap().word_22e
        ]));
        mutate(arena, actor, &self.spec["mutations"][ordinal.to_string()]);
        if self.spec["failure"].as_u64() == Some(ordinal as u64) {
            return Err(());
        }
        Ok(self.spec["results"][ordinal].as_u64().unwrap_or(0) as u32)
    }
}

#[test]
fn original_geometry_order_flags_mutations_and_failure_prefixes_match() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/family_obstacle_scan_native.json"
    )))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 208);
    for (ordinal, expected) in cases.iter().enumerate() {
        let spec = &expected["input"];
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
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
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
        mutate(
            &mut arena,
            actor,
            &json!({
                "flags":spec["initial_flags"].as_u64().unwrap_or(65535),
                "position":spec.get("position").cloned().unwrap_or(json!([65530,20,30])),
                "cache":spec["cache"].as_bool().unwrap_or(true),
                "basis":spec.get("basis").cloned().unwrap_or(json!([0,0x3f800000u32])),
            }),
        );
        arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x81;
        if spec["missing_position"] == true {
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
        }
        let mut host = Host {
            spec,
            events: vec![],
        };
        let result = arena.scan_family_obstacles(
            actor,
            spec["reverse"].as_bool().unwrap_or(false),
            &mut host,
        );
        let error = match result {
            Ok(()) => None,
            Err(FamilyObstacleScanError::MissingCache {
                ordinal: failed, ..
            }) => {
                assert_eq!(usize::from(failed), host.events.len());
                Some("cache")
            }
            Err(FamilyObstacleScanError::MissingPosition { .. }) => Some("position"),
            Err(FamilyObstacleScanError::Host {
                ordinal: failed, ..
            }) => {
                assert_eq!(usize::from(failed) + 1, host.events.len());
                Some("host")
            }
            Err(other) => panic!("unexpected case{ordinal}: {other:?}"),
        };
        let actual = json!({"error":error,"events":host.events,"flags":arena.family428650_actor(actor).unwrap().word_22e,
            "position":arena.object(actor).unwrap().base.position_words,"cache":arena.family429e20_actor(actor).unwrap().probe_234.is_some(),
            "dirty":arena.object(actor).unwrap().base.dirty_word});
        let mut expected = expected.clone();
        expected.as_object_mut().unwrap().remove("input");
        assert_eq!(actual, expected, "native sequence {ordinal}");
        assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
    }
}
