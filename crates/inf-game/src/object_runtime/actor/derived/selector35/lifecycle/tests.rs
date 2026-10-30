use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&v[i]) as u16)
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0; 3]),
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
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}
struct Host<'a> {
    input: &'a Value,
    events: Vec<Value>,
    source: Option<ActorHandle>,
}
impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.source
    }
}
impl Selector35LifecycleHost for Host<'_> {
    type Error = &'static str;
    fn particle_load_factor(&mut self) -> Result<u32, Self::Error> {
        Ok(u(&self.input["load"]))
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector35LifecycleEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        let mut event = match effect {
            Selector35LifecycleEffect::Burst(b) => {
                json!({"kind":"burst","count":b.debris_count,"position":b.position,"radius":b.placement_radius,"central":b.central_type,"debris":b.debris_types,"speed":b.speed,"flags":b.flags})
            }
            Selector35LifecycleEffect::Radius {
                block_address,
                source,
            } => {
                json!({"kind":"radius","block":block_address,"source":source.raw(),"replicate":1,"position":arena.object(actor).unwrap().base.position_words.unwrap()})
            }
            Selector35LifecycleEffect::Trail {
                kind,
                position,
                velocity,
            } => {
                json!({"kind":"trail","particle_type":kind,"position":position,"velocity":velocity,"flags":0})
            }
            Selector35LifecycleEffect::TerrainScatter => {
                json!({"kind":"scatter","position":arena.object(actor).unwrap().base.position_words.unwrap(),"count_minus_one":0})
            }
            Selector35LifecycleEffect::ChangeState(r) => {
                json!({"kind":"state","target":r.target_state,"source":r.source.raw(),"argument":r.argument})
            }
        };
        event["rng"] = json!(*rng);
        let kind = event["kind"].as_str().unwrap().to_owned();
        self.events.push(event);
        if self.input["stop"] == kind {
            return Err("supplied service stop");
        }
        let m = &self.input["mutation"];
        if !m.is_null() && m["at"] == kind {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&m["position"]));
            arena.owned_mut(actor).unwrap().state.base.flags = u(&m["flags"]);
            // Supplied callback mutation represents the native +84 read; this
            // receipt does not resolve that synthetic handle through registry.
            arena.actor_mut(actor).unwrap().handle = ActorHandle::from_raw(u(&m["source"]));
            *rng = u(&m["rng"]);
        }
        Ok(())
    }
}
#[test]
fn selector35_lifecycle_matches_original_callbacks_and_stopped_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_lifecycle_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["provenance"]["exe_sha256"].as_str().unwrap()
    );
    for guard in fixture["guards"].as_array().unwrap() {
        let at = (u(&guard["address"]) - 0x400000) as usize;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + u(&guard["size"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let i = &case["input"];
        let (mut arena, actor) = self::fixture();
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&i["position"]));
        arena.owned_mut(actor).unwrap().state.base.word_28 = u(&i["size"]) as u16;
        arena.owned_mut(actor).unwrap().state.base.flags = u(&i["flags"]);
        let a = arena.actor_mut(actor).unwrap();
        a.handle = ActorHandle::from_raw(u(&i["source"]));
        a.words_8e_92 = words(&i["velocity"]);
        a.words_94_9c = std::array::from_fn(|n| u(&i["angular"][n]));
        a.word_c8 = u(&i["color"]) as u16;
        a.word_ca = Some(u(&i["color_timer"]) as u16);
        let state = arena.family428650_actor_mut(actor).unwrap();
        state.word_228 = u(&i["status"]);
        state.words_188_19c[0] = Some(u(&i["elapsed"]));
        let state = arena.selector35_actor_mut(actor).unwrap();
        state.byte_238 = u(&i["byte238"]) as u8;
        state.word_230 = u(&i["offset"]);
        let target =
            arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(77),
                focus_08: Some(words(&i["target_focus"])),
                word_10: u(&i["target_kind"]),
            }));
        arena.derived_actor_mut(actor).unwrap().target_148 =
            i["current"].as_bool().unwrap().then_some(target);
        arena
            .actor_service_heap_mut()
            .script([if i["stop"] == "allocate" {
                FixtureActorServiceAllocation::Error(1)
            } else if i["allocation_null"].as_bool().unwrap() {
                FixtureActorServiceAllocation::Null
            } else {
                FixtureActorServiceAllocation::Allocate
            }]);
        let request = match i["method"].as_str().unwrap() {
            "dying_change" => Selector35LifecycleRequest::DyingChange {
                entering: u(&i["entering"]) != 0,
            },
            "dying_update" => Selector35LifecycleRequest::DyingUpdate,
            "dormant_change" => Selector35LifecycleRequest::DormantChange {
                entering: u(&i["entering"]) != 0,
            },
            _ => Selector35LifecycleRequest::DormantUpdate,
        };
        let mut host = Host {
            input: i,
            source: Some(ActorHandle::from_raw(u(&i["source_null"]))),
            events: vec![],
        };
        let mut rng = u(&i["rng"]);
        let result = arena.run_selector35_lifecycle(actor, request, &mut host, &mut rng);
        assert_eq!(
            result.is_err(),
            !case["stopped_at"].is_null(),
            "case {index}: {result:?}"
        );
        if i["method"] == "dying_change" && u(&i["entering"]) != 0 {
            host.events
                .insert(0, json!({"kind":"allocate","size":20,"rng":u(&i["rng"])}));
        }
        assert_eq!(json!(host.events), case["events"], "case {index} events");
        let a = arena.actor(actor).unwrap();
        let s = arena.selector35_actor(actor).unwrap();
        let target=arena.derived_actor(actor).unwrap().target_148.map(|r| { let t=arena.native_target(r).unwrap();json!({"focus":t.raw_focus_08_words().unwrap(),"kind":t.kind_word(),"source":t.handle_04().raw()}) });
        assert_eq!(
            json!({"position":arena.object(actor).unwrap().base.position_words.unwrap(),"velocity":a.words_8e_92,"angular":a.words_94_9c,"flags":arena.object(actor).unwrap().base.flags,"source":a.handle.raw(),"status":arena.family428650_actor(actor).unwrap().word_228,"offset":s.word_230,"byte238":s.byte_238,"color":a.word_c8,"color_timer":a.word_ca.unwrap(),"rng":rng,"target":target}),
            case["output"],
            "case {index} state"
        );
    }
}
#[test]
fn selector35_dying_source_failure_follows_allocation_but_null_allocation_skips_source() {
    let (mut arena, actor) = fixture();
    let input = json!({"load":0,"stop":null,"mutation":null});
    let mut host = Host {
        input: &input,
        events: vec![],
        source: None,
    };
    let mut rng = 1;
    let before_attempts = arena.actor_service_heap().attempts().len();
    assert!(matches!(
        arena.run_selector35_lifecycle(
            actor,
            Selector35LifecycleRequest::DyingChange { entering: true },
            &mut host,
            &mut rng
        ),
        Err(Selector35LifecycleError::MissingProcessNull { .. })
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_228, 0x530d3c);
    assert_eq!(
        arena.actor_service_heap().attempts().len(),
        before_attempts + 1
    );
    assert_eq!(rng, 1);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    arena
        .run_selector35_lifecycle(
            actor,
            Selector35LifecycleRequest::DyingChange { entering: true },
            &mut host,
            &mut rng,
        )
        .unwrap();
    assert!(arena.derived_actor(actor).unwrap().target_148.is_none());
    assert_ne!(rng, 1);
}
