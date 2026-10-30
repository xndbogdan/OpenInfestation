use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        targets::{ActorFollowTarget, FixedPositionTarget, ObjectPositionTarget},
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
        ActorTerrainInputs, DerivedActorConstructionRequest, FamilyBehaviorProfile, NativeTarget,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    native_basis::NativeBasis, parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};
use std::path::Path;

fn terrain(spacing: u16, height_shift: u32) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for x in 0..512 {
        for z in 0..512 {
            bytes[TERRAIN_PREFIX_BYTES + (x * 512 + z) * 2] = (x * 13 + z * 7 + 37) as u8;
        }
    }
    assert_eq!(TERRAIN_CELL_COUNT, 512 * 512);
    let span = ByteSpan::new(0, bytes.len());
    let source = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &source.entries[0],
        TerrainSampling {
            spacing: match spacing {
                128 => TerrainSpacing::Units128,
                256 => TerrainSpacing::Units256,
                _ => panic!("unsupported native spacing"),
            },
            height_scale: match height_shift {
                4 => TerrainHeightScale::Units16,
                5 => TerrainHeightScale::Units32,
                _ => panic!("unsupported native height shift"),
            },
        },
    )
}

fn arena(sine: &[u16; 4096]) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([65535, 32768, 7]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: sine,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}

#[test]
fn released_current_reaches_rng_and_terrain_before_kind_read() {
    let sine = [0; 4096];
    let terrain = terrain(256, 4);
    let (mut arena, actor) = arena(&sine);
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(0);
    let released = arena.import_fixture_target(target(Some("fixed"), 8, actor).unwrap());
    arena.release_actor_service(released).unwrap();
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(released);
    let mut host = Host {
        terrain: &terrain,
        sampling_after_acquisition: None,
        acquisition: false,
        mutation: None,
        unavailable: true,
        calls: 0,
    };
    let mut rng = 7;
    assert_eq!(
        arena.update_family_roam_target(
            actor,
            FamilyRoamUpdateRequest {
                microseconds: 1,
                sine_quarter: &sine
            },
            &mut rng,
            &mut host
        ),
        Err(FamilyRoamUpdateError::Terrain("unavailable H12"))
    );
    assert_eq!(host.calls, 1);
    let mut expected_rng = 7;
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    assert_eq!(rng, expected_rng);
    host.unavailable = false;
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(0);
    assert_eq!(
        arena.update_family_roam_target(
            actor,
            FamilyRoamUpdateRequest {
                microseconds: 1,
                sine_quarter: &sine
            },
            &mut rng,
            &mut host
        ),
        Err(FamilyRoamUpdateError::Object(
            ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(released))
        ))
    );
    assert_eq!(host.calls, 2);
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(released)
    );
}

fn target(class: Option<&str>, kind: u32, actor: ObjectId) -> Option<NativeTarget> {
    let handle_04 = ActorHandle::from_raw(0x1234_5678);
    Some(match class? {
        "fixed" => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04,
            focus_08: Some([11, 12, 13]),
            word_10: kind,
        }),
        "follow" => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04,
            focus_08: Some([11, 12, 13]),
            word_10: kind,
            focus_14: Some([21, 22, 23]),
            velocity_1a: Some([31, 32, 33]),
        }),
        "object" => NativeTarget::ObjectPosition(ObjectPositionTarget {
            handle_04,
            focus_08: [11, 12, 13],
            word_10: kind,
            object_14: actor,
        }),
        _ => panic!("unsupported native target"),
    })
}

fn snapshot(target: Option<NativeTarget>, actor: ObjectId) -> Value {
    match target {
        None => Value::Null,
        Some(NativeTarget::FixedPosition(target)) => json!({"class_name":"fixed",
            "handle_04":target.handle_04.raw(),"focus_08":target.focus_08,"word_10":target.word_10}),
        Some(NativeTarget::ActorFollow(target)) => json!({"class_name":"follow",
            "handle_04":target.handle_04.raw(),"focus_08":target.focus_08,"word_10":target.word_10,
            "focus_14":target.focus_14,"velocity_1a":target.velocity_1a}),
        Some(NativeTarget::FurniturePosition(_)) => panic!("unadmitted target class"),
        Some(NativeTarget::ObjectPosition(target)) => {
            assert_eq!(target.object_14, actor);
            json!({"class_name":"object","handle_04":target.handle_04.raw(),
                "focus_08":target.focus_08,"word_10":target.word_10,"object_14":"actor"})
        }
    }
}

struct Host<'a> {
    terrain: &'a TerrainRuntime,
    sampling_after_acquisition: Option<&'a TerrainRuntime>,
    acquisition: bool,
    mutation: Option<&'a str>,
    unavailable: bool,
    calls: usize,
}

impl ActorTerrainHost for Host<'_> {
    type Error = &'static str;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        if self.unavailable {
            return Err("unavailable H12");
        }
        if self.acquisition {
            match self.mutation {
                Some("kind8") | Some("kind2") => {
                    let kind = if self.mutation == Some("kind8") { 8 } else { 2 };
                    let reference = arena.derived_actor(actor).unwrap().target_148.unwrap();
                    let mut data = arena.native_target(reference).unwrap();
                    match &mut data {
                        NativeTarget::FixedPosition(target) => target.word_10 = kind,
                        NativeTarget::ActorFollow(target) => target.word_10 = kind,
                        NativeTarget::ObjectPosition(target) => target.word_10 = kind,
                        NativeTarget::FurniturePosition(target) => target.word_10 = kind,
                    }
                    arena.write_native_target(reference, data).unwrap();
                }
                Some("null") => arena.derived_actor_mut(actor).unwrap().target_148 = None,
                Some("replacement") => {
                    let replacement =
                        arena.import_fixture_target(target(Some("fixed"), 8, actor).unwrap());
                    arena.derived_actor_mut(actor).unwrap().target_148 = Some(replacement);
                }
                Some("position") => {
                    arena.owned_mut(actor).unwrap().state.base.position_words = Some([7, 8, 9])
                }
                Some("timer") => {
                    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] =
                        Some(0x7654_3210)
                }
                Some("sampling") => {}
                None => {}
                _ => panic!("unsupported acquisition mutation"),
            }
            self.acquisition = false;
        }
        Ok(self.sampling_after_acquisition.unwrap_or(self.terrain))
    }
}

fn profile(name: &str) -> FamilyBehaviorProfile {
    match name {
        "family-mode1" => FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        "family-mode2" => FamilyBehaviorProfile::Family4236d0 { mode: 2 },
        "selector12-absent" => FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        "selector12-present" => FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        "selector35" => FamilyBehaviorProfile::Selector35,
        _ => panic!("unsupported native Roam profile"),
    }
}

#[test]
fn roam_matches_original_literal_wrappers_signed_timers_and_live_acquisition() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_descriptor_roam_native.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let terrains = [
        terrain(128, 4),
        terrain(128, 5),
        terrain(256, 4),
        terrain(256, 5),
    ];
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 460);
    let mut updates = 0;
    for case in cases {
        let descriptor = profile(case["profile"].as_str().unwrap())
            .descriptor(case["index"].as_u64().unwrap() as i32)
            .unwrap();
        assert_eq!(descriptor.update_address, 0x0043_fbc0);
        let (mut arena, actor) = arena(&sine);
        arena.actor_mut(actor).unwrap().rotating.basis = NativeBasis {
            words: std::array::from_fn(|i| case["basis"][i].as_u64().unwrap() as u32),
        };
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(std::array::from_fn(|i| {
                case["position"][i].as_u64().unwrap() as u16
            }));
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] =
            Some(case["timer"].as_u64().unwrap() as u32);
        let current = target(
            case["old_class"].as_str(),
            case["old_kind"].as_u64().unwrap() as u32,
            actor,
        )
        .map(|data| arena.import_fixture_target(data));
        arena.derived_actor_mut(actor).unwrap().target_148 = current;
        let stop = case["stop"].as_str();
        if stop == Some("position") {
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
        }
        let before = arena.object(actor).unwrap().base.clone();
        let family_before = arena.family428650_actor(actor).unwrap().clone();
        let basis_before = arena.actor(actor).unwrap().rotating.basis;
        let terrain_index =
            usize::from(case["spacing"] == 256) * 2 + usize::from(case["height_shift"] == 5);
        let mut host = Host {
            terrain: &terrains[terrain_index],
            acquisition: case["acquisition"] == true,
            sampling_after_acquisition: (case["mutation"] == "sampling").then_some(&terrains[2]),
            mutation: case["mutation"].as_str(),
            unavailable: stop == Some("terrain"),
            calls: 0,
        };
        let mut rng = case["rng"].as_u64().unwrap() as u32;
        for step in case["steps"].as_array().unwrap() {
            updates += 1;
            let result = arena.update_family_roam_target(
                actor,
                FamilyRoamUpdateRequest {
                    microseconds: step["delta"].as_u64().unwrap() as u32,
                    sine_quarter: &sine,
                },
                &mut rng,
                &mut host,
            );
            assert_eq!(
                result,
                match step["stopped"].as_str() {
                    Some("position") => Err(FamilyRoamUpdateError::MissingPosition(actor)),
                    Some("terrain") => Err(FamilyRoamUpdateError::Terrain("unavailable H12")),
                    Some("null_target") => Err(FamilyRoamUpdateError::MissingTarget(actor)),
                    None => Ok(()),
                    _ => panic!("unknown native stopped prefix"),
                },
                "{case}"
            );
            assert_eq!(
                snapshot(
                    arena
                        .derived_actor(actor)
                        .unwrap()
                        .target_148
                        .map(|reference| arena.native_target(reference).unwrap()),
                    actor
                ),
                step["target"],
                "{case}"
            );
            assert_eq!(
                arena.family428650_actor(actor).unwrap().words_188_19c[1].unwrap() as u64,
                step["timer"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(rng as u64, step["rng"].as_u64().unwrap(), "{case}");
        }
        let mut expected_base = before;
        if case["mutation"] == "position" {
            expected_base.position_words = Some([7, 8, 9]);
        }
        assert_eq!(arena.object(actor).unwrap().base, expected_base, "{case}");
        let mut expected_family = family_before;
        expected_family.words_188_19c[1] = Some(case["final_timer"].as_u64().unwrap() as u32);
        assert_eq!(
            arena.family428650_actor(actor).unwrap(),
            &expected_family,
            "{case}"
        );
        assert_eq!(
            arena.actor(actor).unwrap().rotating.basis,
            basis_before,
            "{case}"
        );
        assert_eq!(
            host.calls,
            case["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] == "height_read"
                    || event["kind"] == "acquire" && stop == Some("terrain"))
                .count(),
            "{case}"
        );
        assert_eq!(case["unrelated_actor_bytes_unchanged"], true);
    }
    assert_eq!(updates, 480);
}

#[test]
fn unknown_timer_fails_before_rng_and_decay_bypasses_unavailable_later_sources() {
    let sine = [0; 4096];
    let (mut arena, actor) = arena(&sine);
    let terrain = terrain(128, 5);
    let mut host = Host {
        terrain: &terrain,
        sampling_after_acquisition: None,
        acquisition: true,
        mutation: None,
        unavailable: true,
        calls: 0,
    };
    let mut rng = 0x1234_5678;
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = None;
    assert_eq!(
        arena.update_family_roam_target(
            actor,
            FamilyRoamUpdateRequest {
                microseconds: 1,
                sine_quarter: &sine
            },
            &mut rng,
            &mut host
        ),
        Err(FamilyRoamUpdateError::MissingTimer(actor))
    );
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(host.calls, 0);
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[1] = Some(0x7fff_ffff);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.derived_actor_mut(actor).unwrap().target_148 = None;
    assert_eq!(
        arena.update_family_roam_target(
            actor,
            FamilyRoamUpdateRequest {
                microseconds: 0xffff_ffff,
                sine_quarter: &sine
            },
            &mut rng,
            &mut host
        ),
        Ok(())
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[1],
        Some(0x8000_0000)
    );
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(host.calls, 0);
}
