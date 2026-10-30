use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixedPositionTarget, NativeTarget,
};
pub(crate) fn assets() -> &'static crate::frontend_assets::FrontendAssets {
    static ASSETS: std::sync::OnceLock<crate::frontend_assets::FrontendAssets> =
        std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        crate::frontend_assets::FrontendAssets::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap()
    })
}
pub(crate) fn executable() -> inf_formats::FrontendExecutable<'static> {
    inf_formats::FrontendExecutable::parse(assets().executable_bytes()).unwrap()
}
pub(crate) fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut random = 0;
    let actor = arena
        .allocate_selector10_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([384, 64, 640]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &assets().sine_table().values,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut random,
        )
        .unwrap();
    (arena, actor)
}
pub(crate) fn resident() -> crate::ResidentRuntime {
    let mut resident = crate::ResidentRuntime::new(
        crate::Installation::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap(),
    )
    .unwrap();
    resident.apply(crate::ResidencyRequest::Boot).unwrap();
    resident
}
pub(crate) fn sound() -> crate::process_sound::ProcessSoundRuntime {
    crate::process_sound::ProcessSoundRuntime::new(executable()).unwrap()
}
fn fixed(arena: &mut ObjectArena, position: Option<[u16; 3]>) -> NativeHeapRef {
    arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: position,
        word_10: 8,
    }))
}
struct Unreached;
impl FamilyTerrainProbeMissHost for Unreached {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&crate::terrain_runtime::TerrainRuntime, Self::Error> {
        panic!("this supplied branch must not acquire a miss terrain")
    }
    fn collide(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        panic!("unreached collision")
    }
}
fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    delta: u32,
) -> Result<Selector10TailNextPhase, Selector10TailPrefixError<&'static str>> {
    arena.tick_selector10_tail_prefix(
        actor,
        Selector10TailPrefixRequest {
            firing: crate::object_runtime::ActorWeaponFireContext {
                tables: &mut crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                    executable(),
                )
                .unwrap(),
                rng: &mut 0,
                process_velocity: None,
            },
            microseconds: delta,
            settings_7c: 0,
            terrain: &mut Unreached,
        },
    )
}
#[test]
fn null_primary_bypasses_unknown_carries_without_repeating_the_owned_prefix() {
    let (mut arena, actor) = fixture();
    arena.derived_actor_mut(actor).unwrap().word_17a = 0x321;
    let stale = fixed(&mut arena, None);
    arena.release_actor_service(stale).unwrap();
    arena.derived_actor_mut(actor).unwrap().target_154 = Some(stale);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_15c, None);
    assert_eq!(
        visit(&mut arena, actor, 125_000).unwrap(),
        Selector10TailNextPhase::Motion { controls: [0, 0] }
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 0x321);
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, None);
}
#[test]
fn primary_refresh_and_distance_are_published_before_unknown_spacing() {
    let (mut arena, actor) = fixture();
    let reference = fixed(&mut arena, Some([384, 64, 740]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    assert!(matches!(
        visit(&mut arena, actor, 1),
        Err(Selector10TailPrefixError::RequiredField {
            offset: 0x208,
            address: 0x424a79,
            ..
        })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(100));
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(reference)
    );
}
#[test]
fn negative_spacing_zero_distance_preserves_the_native_division_fault_prefix() {
    let (mut arena, actor) = fixture();
    let reference = fixed(&mut arena, Some([384, 64, 640]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some((-1_i32) as u32);
    assert!(matches!(
        visit(&mut arena, actor, 1),
        Err(Selector10TailPrefixError::Division(_))
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
}
#[test]
fn secondary_unknown_timer_and_normal_release_use_the_same_real_heap_reference() {
    let (mut arena, actor) = fixture();
    let primary = fixed(&mut arena, Some([384, 64, 840]));
    let secondary = fixed(&mut arena, Some([384, 64, 740]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(primary);
    arena.derived_actor_mut(actor).unwrap().target_154 = Some(secondary);
    assert!(matches!(
        visit(&mut arena, actor, 32),
        Err(Selector10TailPrefixError::RequiredField { offset: 0x15c, .. })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_158, Some(100));
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_154,
        Some(secondary)
    );
    arena.derived_actor_mut(actor).unwrap().word_15c = Some(32);
    arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 0;
    visit(&mut arena, actor, 32).unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().target_154, None);
    assert!(matches!(
        arena.native_target(secondary),
        Err(ObjectError::ActorServiceHeap(_))
    ));
    assert!(arena.native_target(primary).is_ok());
}
#[test]
fn missing_focus_stops_after_the_actual_refresh_status_and_before_geometry() {
    let (mut arena, actor) = fixture();
    let reference = fixed(&mut arena, None);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    assert!(matches!(visit(&mut arena,actor,1),
        Err(Selector10TailPrefixError::RequiredTargetFocus {reference:actual,..}) if actual==reference));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
}
#[test]
fn avoidance_is_a_real_reached_phase_after_geometry_and_control_construction() {
    let (mut arena, actor) = fixture();
    let reference = fixed(&mut arena, Some([384, 64, 840]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
    struct RequiredMiss;
    impl FamilyTerrainProbeMissHost for RequiredMiss {
        type Error = &'static str;
        fn terrain(
            &mut self,
            _: &mut ObjectArena,
            _: ObjectId,
        ) -> Result<&crate::terrain_runtime::TerrainRuntime, Self::Error> {
            Err("actual reached H12 unavailable")
        }
        fn collide(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
            panic!("H12 error precedes collision")
        }
    }
    let installed = std::ptr::from_ref(
        &**arena
            .family429e20_actor(actor)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap(),
    );
    assert!(matches!(
        arena.tick_selector10_tail_prefix(
            actor,
            Selector10TailPrefixRequest {
                firing: crate::object_runtime::ActorWeaponFireContext {
                    tables:
                        &mut crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                            executable()
                        )
                        .unwrap(),
                    rng: &mut 0,
                    process_velocity: None,
                },
                microseconds: 125_000,
                settings_7c: 0,
                terrain: &mut RequiredMiss,
            }
        ),
        Err(Selector10TailPrefixError::Avoidance(_))
    ));
    assert_eq!(
        std::ptr::from_ref(
            &**arena
                .family429e20_actor(actor)
                .unwrap()
                .probe_234
                .as_ref()
                .unwrap()
        ),
        installed
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(200));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
}

#[test]
fn real_follow_refresh_status_admits_each_firing_owner_without_repeating_weapon_tick() {
    for decay in [0, 1] {
        let (mut arena, actor) = fixture();
        let mut random = 7;
        let referent = arena
            .allocate_selector10_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([384, 64, 740]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &assets().sine_table().values,
                        },
                    },
                    world_mask_limits: [3; 2],
                },
                &mut random,
            )
            .unwrap();
        arena.actor_mut(referent).unwrap().word_74 = 0;
        arena.actor_mut(referent).unwrap().word_c4 = 3;
        let handle = arena.actor(referent).unwrap().handle;
        let reference = arena
            .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
                handle,
            ))
            .unwrap()
            .unwrap();
        let behavior = super::super::super::behavior::build_actor_behavior(3, 0, 10).unwrap();
        let state = arena.derived_actor_mut(actor).unwrap();
        state.target_148 = Some(reference);
        state.behavior_140 = Some(behavior);
        state.word_17a = decay;
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        arena.family4236d0_actor_mut(actor).unwrap().byte_260 = 0;
        // Both real direction producers return into the native motion phase.
        assert!(matches!(
            visit(&mut arena, actor, 125_000),
            Ok(Selector10TailNextPhase::Motion { .. })
        ));
        let weapon = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert!(weapon.fire_head_00.is_some());
        assert_ne!(weapon.entries_18[0][5], 0);
        assert_ne!(arena.object(actor).unwrap().base.flags & 0x0100_0000, 0);
        assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(3));
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(100));
        assert_eq!(arena.derived_actor(actor).unwrap().word_17a, decay);
    }
}

/// `00424AAF..00424B4A` against `family_fire_gate_native.json`: each case's
/// behavior entry, +148, +150, +164, +17A, +22C, distance and target
/// geometry give the original's producer and arguments, or no shot. The
/// cases take every condition to its edge, the signed and unsigned ones
/// past the sign bit.
#[test]
fn fire_gate_matches_the_original_at_every_edge() {
    use crate::object_runtime::ActorWeaponFireMode;
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_fire_gate_native.json"
    ))
    .unwrap();
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 33);
    let number = |value: &serde_json::Value| value.as_u64().unwrap() as u32;
    let words = |value: &serde_json::Value| -> [i16; 3] {
        std::array::from_fn(|axis| value[axis].as_i64().unwrap() as i16)
    };
    for case in cases {
        let (input, output) = (&case["input"], &case["output"]);
        let (mut arena, actor) = fixture();
        let target = fixed(&mut arena, Some([0, 0, 0]));
        let mut behavior = super::super::super::behavior::build_actor_behavior(1, 0, 9).unwrap();
        behavior.byte_10 = number(&input["selected"]) as u8;
        for (index, entry) in input["entries"].as_array().unwrap().iter().enumerate() {
            behavior.entries_18[index][0] = number(&entry["count"]);
            behavior.entries_18[index][5] = number(&entry["timer"]);
            behavior.entries_18[index][7] = number(&entry["pressure"]);
        }
        let state = arena.derived_actor_mut(actor).unwrap();
        state.behavior_140 = input["behavior"].as_bool().unwrap().then_some(behavior);
        state.target_148 = Some(target);
        state.word_150 = Some(number(&input["status"]));
        state.word_164 = number(&input["radius"]) as i32;
        state.word_17a = number(&input["timer_17a"]) as u16;
        arena.family428650_actor_mut(actor).unwrap().byte_22c = number(&input["byte_22c"]) as u8;
        let request = arena
            .selector10_fire_request::<()>(
                actor,
                number(&input["distance"]),
                words(&input["world"]),
                words(&input["local"]),
                number(&input["delta"]),
            )
            .unwrap();
        let Some(fire) = output["fire"].as_str() else {
            assert_eq!(request, None, "{input}");
            continue;
        };
        let request = request.unwrap_or_else(|| panic!("no shot for {input}"));
        assert_eq!(
            request.mode,
            match fire {
                "predictive" => ActorWeaponFireMode::Predictive {
                    publish_lead: number(&output["flag"]) != 0
                },
                _ => ActorWeaponFireMode::Scattered,
            },
            "{input}"
        );
        assert!(output["actor"].as_bool().unwrap() && output["target"].as_bool().unwrap());
        assert_eq!(request.target, Some(target));
        assert_eq!(request.distance, number(&output["distance"]), "{input}");
        assert_eq!(request.world, words(&output["world"]), "{input}");
        assert_eq!(request.microseconds, number(&output["delta"]), "{input}");
    }
}
