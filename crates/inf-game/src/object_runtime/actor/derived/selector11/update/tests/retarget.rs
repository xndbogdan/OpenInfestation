//! 004015F0 segment B against supplied worlds: scanner A with filter A and
//! the occlusion test, scanner B, the follow and point targets, the release
//! of an old target and the retaliation boundary.

use super::*;
use crate::object_runtime::{ActorExtension, ActorTerrainSurface};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
use inf_formats::ByteSpan;
use std::collections::BTreeMap;

const PROCESS_NULL: u32 = 0x77;

struct RetargetHost {
    tasks: ProcessTaskRuntime,
    network: u32,
    terrain: TerrainRuntime,
    rng: u32,
    native_handles: BTreeMap<u32, u32>,
    events: Vec<Value>,
}

impl FamilyUpdateHost for RetargetHost {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(PROCESS_NULL))
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        panic!("004015F0 reaches no family effect")
    }
}

impl Selector11ScanHost for RetargetHost {
    fn scan_tasks(&mut self) -> Option<&mut ProcessTaskRuntime> {
        Some(&mut self.tasks)
    }
    fn scan_network_flag(&self) -> Option<u32> {
        Some(self.network)
    }
    fn scan_terrain(&self) -> Option<&TerrainRuntime> {
        Some(&self.terrain)
    }
}

impl Selector11UpdateHost for RetargetHost {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables> {
        panic!("004015F0 reads no behavior rows")
    }
    fn tasks(&self) -> Option<&ProcessTaskRuntime> {
        panic!("004015F0 reads tasks only through filter A")
    }
    fn player_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error> {
        panic!("outside 004015F0")
    }
    fn player_notice(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
        _: u32,
    ) -> Result<(), Self::Error> {
        panic!("outside 004015F0")
    }
    fn create_child(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: [u16; 3],
    ) -> Result<(), Self::Error> {
        panic!("outside 004015F0")
    }
    fn refresh_model(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        panic!("outside 004015F0")
    }
    fn retaliation(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind": "retaliation", "handle": self.native_handles[&source.raw()]}));
        Ok(())
    }
    fn queue_shot(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: [u32; 3],
    ) -> Result<(), Self::Error> {
        panic!("outside 004015F0")
    }
}

/// The supplied H12 entry: 128-unit cells and 32-unit heights.
fn terrain(case: &Value) -> TerrainRuntime {
    let mut bytes = vec![0; inf_formats::TERRAIN_ENTRY_FIXED_BYTES];
    for cell in case["cells"].as_array().into_iter().flatten() {
        let [x, z, height] = [0, 1, 2].map(|index| cell[index].as_u64().unwrap() as usize);
        bytes[0x14 + (x * 512 + z) * 2] = height as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = inf_formats::parse_terrain_bank(
        &bytes,
        &inf_formats::HandlerRegion {
            handler: inf_formats::HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: inf_formats::HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

/// 004015F0 over 66 supplied worlds: every filter A gate and relation, both
/// list orders through the occlusion test, the mount origins, the model-0x41
/// height and vehicle gates, scanner B, and the point target's RNG offsets.
#[test]
fn selector11_retarget_matches_native_segment_b() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../../tests/fixtures/selector11_retarget_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mut found = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let terrain = terrain(case);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: NULL,
        })
        .unwrap();
        let actor = arena
            .allocate_selector11_actor(construction(words(&case["position"])), &mut 0)
            .unwrap();
        if case["behavior"].as_bool().unwrap_or(true) {
            arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
            arena.admit_derived_actor(actor).unwrap();
            let behavior = arena
                .derived_actor_mut(actor)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap();
            behavior.byte_10 = 0;
            behavior.entries_18 = vec![[5, 0, 0, 0, 0, 0, 0, 0]];
            let group: Vec<u32> = case["group"].as_array().map_or(vec![0; 7], |group| {
                group
                    .iter()
                    .map(|word| word.as_u64().unwrap() as u32)
                    .collect()
            });
            behavior.groups_1c = vec![ActorBehaviorGroup {
                coordinates_00_0a: std::array::from_fn(|index| Some(group[index] as u16)),
                word_0c: group[6],
            }];
        }
        let old = case["old_target"].as_bool().unwrap_or(false).then(|| {
            arena.import_fixture_target(NativeTarget::FixedPosition(
                FixedPositionTarget::from_point(ActorHandle::from_raw(0x4444), Some([0; 3])),
            ))
        });
        arena.derived_actor_mut(actor).unwrap().target_148 = old;
        arena.derived_actor_mut(actor).unwrap().word_164 = get(case, "range", 2500) as i32;
        arena.owned_mut(actor).unwrap().state.base.word_28 = 120;
        arena.owned_mut(actor).unwrap().state.base.flags = 0x24;
        let basis: Vec<f64> = case["basis"].as_array().map_or_else(
            || vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            |values| values.iter().map(|value| value.as_f64().unwrap()).collect(),
        );
        let state = arena.actor_mut(actor).unwrap();
        state.rotating.basis = NativeBasis {
            words: std::array::from_fn(|index| match index {
                0 => 0,
                _ => (basis[index - 1] as f32).to_bits(),
            }),
        };
        state.rotating.visual.model_word = get(case, "model", 0x30) as u16;
        state.word_74 = 0x77;
        state.word_b4 = get(case, "team", 2) as u32;
        state.word_c4 = 3;
        let state = arena.selector11_mut(actor).unwrap();
        state.word_184 = Some(0xaaaa_5555);
        state.word_190 = 0x12345;
        state.word_1ae = get(case, "flags_1ae", 0) as u16;
        state.word_1b0 = get(case, "timer_1b0", 0) as u16;

        let mut native_handles = BTreeMap::new();
        for (index, spec) in case["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let position = words(&spec["position"]);
            let candidate = match spec["class"].as_str().unwrap_or("shared") {
                "shared" => arena.allocate_actor(construction(position).actor),
                "gate" => arena.allocate_selector30_actor(construction(position).actor),
                "vehicle" => {
                    let mut request = construction(position);
                    request.actor.terrain.terrain = Some(ActorTerrainSurface {
                        terrain: &terrain,
                        plane_height_fixed: 0,
                    });
                    arena.allocate_selector2_actor(request, &mut 0)
                }
                other => panic!("{other}"),
            }
            .unwrap();
            let base = &mut arena.owned_mut(candidate).unwrap().state.base;
            base.flags = get(spec, "flags", 0x24) as u32;
            base.word_28 = get(spec, "size", 100) as u16;
            base.position_words = Some(position);
            let state = arena.actor_mut(candidate).unwrap();
            state.word_74 = get(spec, "word_74", 0x30) as u16;
            state.word_b4 = get(spec, "team", 0x12) as u32;
            state.word_c4 = get(spec, "state", 3) as u32;
            if let ActorExtension::Derived(derived) = &mut state.extension {
                if let DerivedActorClass::Selector2(vehicle) = &mut derived.class {
                    vehicle.dwords_1e8_1f4[0] = get(spec, "vehicle_state", 0) as u32;
                }
            }
            native_handles.insert(state.handle.raw(), 0x20000 + index as u32 + 1);
        }

        let mut tasks = ProcessTaskRuntime::default();
        tasks
            .construct(get(case, "team", 2) as u32, ActorHandle::from_raw(0x55))
            .unwrap();
        let mut host = RetargetHost {
            tasks,
            network: get(case, "network", 0) as u32,
            terrain,
            rng: case["rng"].as_u64().unwrap() as u32,
            native_handles,
            events: Vec::new(),
        };
        arena
            .selector11_retarget(actor, &mut host)
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        let mut events = Vec::new();
        if let Some(old) = old {
            if arena.native_target(old).is_err() {
                events.push(json!({"kind": "free_target"}));
            }
        }
        events.extend(host.events);
        assert_eq!(json!(events), case["events"], "{name}");
        let handle = |raw: u32| host.native_handles.get(&raw).copied().unwrap_or(raw);
        let target = arena
            .derived_actor(actor)
            .unwrap()
            .target_148
            .map(|reference| match arena.native_target(reference).unwrap() {
                NativeTarget::FixedPosition(point) => json!({"kind": 8,
                    "handle": point.handle_04.raw(), "focus": point.focus_08.unwrap(),
                    "word_10": point.word_10}),
                NativeTarget::ActorFollow(follow) => json!({"kind": 2,
                    "handle": handle(follow.handle_04.raw()), "focus_08": follow.focus_08.unwrap(),
                    "focus_14": follow.focus_14.unwrap(), "word_10": follow.word_10}),
                other => panic!("{name}: {other:?}"),
            });
        found += usize::from(target.as_ref().is_some_and(|target| target["kind"] == 2));
        let state = arena.selector11_actor(actor).unwrap();
        let port = json!({
            "target": target,
            "word_184": handle(state.word_184.unwrap()),
            "word_190": state.word_190,
            "word_1ae": state.word_1ae,
            "rng": host.rng,
        });
        assert_eq!(&port, &case["after"], "{name}");
    }
    assert_eq!(cases.len(), 66);
    assert_eq!(
        found,
        fixture["coverage"]["found"].as_u64().unwrap() as usize
    );
}
