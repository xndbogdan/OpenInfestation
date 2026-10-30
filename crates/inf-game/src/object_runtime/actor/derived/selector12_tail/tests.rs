use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceHeapError,
    ActorServiceResourceState, ActorTerrainInputs, DerivedActorConstructionRequest,
    FixedPositionTarget, FollowTargetRequest, NativeTarget, Selector12GrantContext,
    TerrainProbeAllocation, TerrainProbeCache,
};
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};

// Supplied fixture H12 data, not a sampled original height/default terrain.
struct Terrain {
    current: TerrainRuntime,
    calls: usize,
    fail: bool,
    install_position: Option<[u16; 3]>,
}
impl Terrain {
    fn new() -> Self {
        Self {
            current: native_probe_fixture_terrain(&serde_json::json!({
                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                "xz":[0,0],"heights":[2,2,2,2],"alternate":false,
            })),
            calls: 0,
            fail: false,
            install_position: None,
        }
    }
}
impl ActorTerrainHost for Terrain {
    type Error = &'static str;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        if let Some(position) = self.install_position {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
        }
        if self.fail {
            Err("current H12 unavailable")
        } else {
            Ok(&self.current)
        }
    }
}

impl FamilyTerrainProbeMissHost for Terrain {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        //Explicit supplied intrinsic fixture does not own429EE0 services.
        //It stops after the real+160read and before guard/XYZ publication.
        Err("fixture miss service unavailable")
    }
    fn collide(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        panic!("fixture H12 service stops first")
    }
}

fn tables() -> &'static (NativeBasisMath, [i16; 1024], [u16; 4096]) {
    static TABLES: std::sync::OnceLock<(NativeBasisMath, [i16; 1024], [u16; 4096])> =
        std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let mapped = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let inverse = inf_formats::parse_frontend_inverse_sine_table(
            mapped.data(),
            inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )
        .unwrap()
        .values;
        let sine = assets.sine_table().values;
        (NativeBasisMath::new(sine), inverse, sine)
    })
}
fn subject(arena: &mut ObjectArena, position: Option<[u16; 3]>) -> ObjectId {
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let actor = arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: position,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &tables().2,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 100;
    actor
}
fn fixture(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = subject(&mut arena, position);
    (arena, actor)
}
fn fixed(arena: &mut ObjectArena, point: Option<[u16; 3]>) -> NativeHeapRef {
    // Explicit supplied resource imported into real retained heap backing.
    arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: point,
        word_10: 8,
    }))
}
fn tick(
    arena: &mut ObjectArena,
    actor: ObjectId,
    host: &mut Terrain,
    delta: u32,
) -> Result<(), Selector12TailError<&'static str>> {
    let (math, inverse, sine) = tables();
    arena.tick_selector12_tail(
        actor,
        Selector12TailRequest {
            firing: crate::object_runtime::ActorWeaponFireContext {
                tables: &mut crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                    super::super::selector10_tail::tests::executable(),
                )
                .unwrap(),
                rng: &mut 0,
                process_velocity: None,
            },
            microseconds: delta,
            settings_7c: 0,
            surface_snap_53a128: 1,
            math,
            inverse_sine: inverse,
            sine_quarter: sine,
            terrain: host,
        },
    )
}
fn miss(error: Selector12TailError<&'static str>, actor: ObjectId, point: [u16; 2]) {
    assert!(matches!(error, Selector12TailError::Avoidance(
        FamilyAvoidanceError::Host { effect: "00429FF0 probe scan", error:
            FamilyObstacleScanError::Host { ordinal: 0, error:
                Selector12TailProbeError::Miss { actor: actual, position_words, error: FamilyTerrainProbeMissError::Host { effect: "H12 terrain acquisition", error: "fixture miss service unavailable" } }
            }
        }
    ) if actual == actor && position_words == point));
}

#[test]
fn constructor_holes_stay_unknown_and_null_primary_bypasses_secondary_fields() {
    let (mut arena, actor) = fixture(None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_158, None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_15c, None);
    let released = fixed(&mut arena, None);
    arena.release_actor_service(released).unwrap();
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.target_154 = Some(released);
    derived.word_150 = None;
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = None;
    arena.actor_mut(actor).unwrap().words_8e_92 = [7, 2, 9];
    let mut host = Terrain::new();
    host.fail = true;
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::Terrain("current H12 unavailable"))
    ));
    assert_eq!(host.calls, 1); // Required H12 precedes +118 XYZ read.
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 2, 0]);
    let derived = arena.derived_actor(actor).unwrap();
    assert_eq!(derived.target_154, Some(released));
    assert_eq!(
        (derived.word_150, derived.word_158, derived.word_15c),
        (None, None, None)
    );
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
}

#[test]
fn null_primary_enters_real_motion_with_post_acquisition_xyz_and_original_tables() {
    let (mut arena, actor) = fixture(None);
    let mut host = Terrain::new();
    host.install_position = Some([10, 120, 30]);
    assert_eq!(host.current.height_at([10, 30]), 64);
    // Authentic sub32 IDIV retains reached Y snap; the supplied terrain host
    // publishes XYZ at acquisition, so an eager old XYZ read would fail.
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 31),
        Err(Selector12TailError::Motion(Selector12MotionError::Motion(
            super::super::ActorMotionError::Division {
                address: 0x0046_7e4d,
                denominator: 0,
                ..
            }
        )))
    ));
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 64, 30])
    );
    host.install_position = None;
    tick(&mut arena, actor, &mut host, 40000).unwrap();
    assert_eq!(host.calls, 2);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 64, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 2);
    assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
}

#[test]
fn false_avoidance_velocity_threshold_is_strict_signed_and_keeps_vertical() {
    for (forward, clears) in [(9_i16, true), (-9, true), (10, false), (-10, false)] {
        let (mut arena, actor) = fixture(None);
        arena.actor_mut(actor).unwrap().words_8e_92 = [37, (-17_i16) as u16, forward as u16];
        let mut host = Terrain::new();
        host.fail = true;
        assert!(matches!(
            tick(&mut arena, actor, &mut host, 0),
            Err(Selector12TailError::Terrain(_))
        ));
        assert_eq!(
            arena.actor(actor).unwrap().words_8e_92,
            if clears {
                [0, (-17_i16) as u16, 0]
            } else {
                [37, (-17_i16) as u16, forward as u16]
            }
        );
    }
}

#[test]
fn primary_refresh_status_is_retained_before_required_direct_focus_and_spacing() {
    let (mut arena, actor) = fixture(Some([10, 64, 30]));
    let reference = fixed(&mut arena, None);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    let mut host = Terrain::new();
    assert!(matches!(tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::RequiredTargetFocus { reference: actual, address: 0x0046_7b97 })
        if actual == reference));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    arena
        .write_native_target(
            reference,
            NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(0),
                focus_08: Some([310, 64, 430]),
                word_10: 8,
            }),
        )
        .unwrap();
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = None;
    assert!(matches!(tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::RequiredField { actor: actual, offset: 0x208, address: 0x0046_7c0b })
        if actual == actor));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(500));
    assert_eq!(host.calls, 0);
}

#[test]
fn actual_follow_refresh_writes_same_heap_focus_and_velocity_before_spacing_error() {
    let (mut arena, actor) = fixture(Some([10, 64, 30]));
    let followed = subject(&mut arena, Some([310, 44, 430]));
    arena.actor_mut(followed).unwrap().word_74 = 20;
    arena.actor_mut(followed).unwrap().words_8e_92 = [1, 2, 3];
    arena.actor_mut(followed).unwrap().word_c4 = 3;
    let handle = arena.actor(followed).unwrap().handle;
    let mut target = arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    target.focus_08 = None;
    target.focus_14 = Some([7, 8, 9]);
    target.velocity_1a = None;
    let reference = arena.import_fixture_target(NativeTarget::ActorFollow(target));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = None;
    let mut host = Terrain::new();
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::RequiredField { offset: 0x208, .. })
    ));
    let refreshed = arena.actor_follow_target(reference).unwrap();
    assert_eq!(refreshed.focus_08, Some([310, 64, 430]));
    assert_eq!(refreshed.velocity_1a, Some([1, 2, 3]));
    assert_eq!(refreshed.focus_14, Some([7, 8, 9])); // settings7C=0 is lazy.
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(3));
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(500));
    assert_eq!(host.calls, 0);
}

#[test]
fn direct_distance_uses_wrapped_signed_words_and_bit31_repair() {
    let (mut arena, actor) = fixture(Some([1, 1, 1]));
    let reference = fixed(&mut arena, Some([65535, 1, 1]));
    assert_eq!(
        arena
            .selector12_tail_geometry::<()>(actor, reference, 0)
            .unwrap(),
        ([-2, 0, 0], 2)
    );
    arena
        .write_native_target(
            reference,
            NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04: ActorHandle::from_raw(0),
                focus_08: Some([32769; 3]),
                word_10: 8,
            }),
        )
        .unwrap();
    let (world, distance) = arena
        .selector12_tail_geometry::<()>(actor, reference, 0)
        .unwrap();
    assert_eq!(world, [i16::MIN; 3]);
    assert_eq!(distance, 56754); // floor(sqrt(unsigned_sum>>2))<<1, not full sqrt.
}

#[test]
fn secondary_distance_is_published_before_unknown_timer_without_releasing_target() {
    let (mut arena, actor) = fixture(Some([10, 64, 30]));
    let primary = fixed(&mut arena, None);
    let secondary = fixed(&mut arena, Some([10, 64, 430]));
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.target_148 = Some(primary);
    derived.target_154 = Some(secondary);
    let mut host = Terrain::new();
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 32),
        Err(Selector12TailError::RequiredField {
            offset: 0x15c,
            address: 0x0046_7b3c,
            ..
        })
    ));
    let derived = arena.derived_actor(actor).unwrap();
    assert_eq!(
        (derived.word_150, derived.word_158, derived.word_15c),
        (Some(4), Some(400), None)
    );
    assert_eq!(derived.word_14c, None);
    assert_eq!(derived.target_154, Some(secondary));
    assert!(matches!(
        arena.actor_service_heap().state(secondary),
        Ok(ActorServiceResourceState::Target(_))
    ));
    assert_eq!(host.calls, 0);
}

#[test]
fn secondary_signed_timer_decrements_or_releases_before_later_avoidance_failure() {
    for (timer, delta, expected_timer, released) in [
        (33_u32, 32_u32, 1_u32, false),
        (32, 32, 32, true),
        (0, u32::MAX, 1, false),
        (u32::MAX, u32::MAX, u32::MAX, true),
    ] {
        let (mut arena, actor) = fixture(Some([10, 64, 30]));
        let primary = fixed(&mut arena, None);
        let secondary = fixed(&mut arena, Some([10, 64, 430]));
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.target_148 = Some(primary);
        derived.target_154 = Some(secondary);
        derived.word_15c = Some(timer);
        let mut host = Terrain::new();
        miss(
            tick(&mut arena, actor, &mut host, delta).unwrap_err(),
            actor,
            [10, 30],
        );
        let derived = arena.derived_actor(actor).unwrap();
        assert_eq!(derived.word_158, Some(400));
        assert_eq!(derived.word_15c, Some(expected_timer));
        assert_eq!(
            derived.target_154,
            if released { None } else { Some(secondary) }
        );
        assert_eq!(
            arena.actor_service_heap().state(secondary).unwrap(),
            if released {
                ActorServiceResourceState::Released
            } else {
                ActorServiceResourceState::Target(arena.native_target(secondary).unwrap())
            }
        );
        assert_eq!(host.calls, 0);
    }
}

#[test]
fn stale_secondary_is_hard_after_primary_status_and_does_not_clear_pointer() {
    let (mut arena, actor) = fixture(Some([10, 64, 30]));
    let primary = fixed(&mut arena, None);
    let secondary = fixed(&mut arena, None);
    arena.release_actor_service(secondary).unwrap();
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.target_148 = Some(primary);
    derived.target_154 = Some(secondary);
    let mut host = Terrain::new();
    assert!(matches!(tick(&mut arena, actor, &mut host, 32),
        Err(Selector12TailError::Object(ObjectError::ActorServiceHeap(ActorServiceHeapError::Released(actual))))
        if actual == secondary));
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_154,
        Some(secondary)
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_158, None);
    assert_eq!(host.calls, 0);
}

#[test]
fn first_actual_cache_miss_retains_flag_and_window_prefix_before_motion() {
    let (mut arena, actor) = fixture(Some([4096, 64, 4096]));
    let primary = fixed(&mut arena, Some([4096, 64, 4496]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(primary);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
    arena.family428650_actor_mut(actor).unwrap().word_22e = 0xfff5;
    let mut host = Terrain::new();
    miss(
        tick(&mut arena, actor, &mut host, 40000).unwrap_err(),
        actor,
        [4096, 4096],
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(400));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x15);
    let cache = arena
        .family429e20_actor(actor)
        .unwrap()
        .probe_234
        .as_ref()
        .unwrap()
        .decoded_cache();
    assert_eq!((cache.word_04(), cache.word_06()), (64, 64));
    assert!(cache.words().iter().all(|&word| word == 0));
    assert_eq!(host.calls, 0);
}

#[test]
fn supplied_populated_cache_uses_real_hit_path_before_actual_terrain_acquisition() {
    let (mut arena, actor) = fixture(Some([512, 64, 512]));
    let primary = fixed(&mut arena, Some([512, 64, 712]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(primary);
    arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(0);
    // Explicit authored cached-result fixture. It does not claim that native
    // terrain/collision misses return clear. All following probes are real hits.
    let mut cache = TerrainProbeCache::new(16);
    for z in 0..32_u16 {
        for x in 0..32_u16 {
            cache
                .probe([x * 64, z * 64], |_, _| Ok::<_, ()>(0))
                .unwrap();
        }
    }
    let before = cache.clone();
    arena.family429e20_actor_mut(actor).unwrap().probe_234 =
        Some(Box::new(TerrainProbeAllocation::new(cache)));
    let mut host = Terrain::new();
    host.fail = true;
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::Terrain(_))
    ));
    assert_eq!(host.calls, 1);
    assert_eq!(
        arena
            .family429e20_actor(actor)
            .unwrap()
            .probe_234
            .as_ref()
            .map(|allocation| allocation.decoded_cache()),
        Some(&before)
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().word_22e & 0xffe0,
        0
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(200));
}

#[test]
fn firing_uses_signed_entry18_index_and_reached_exact_gate_without_motion() {
    for (index, age, expected_fire) in [(0_u8, 0_u32, true), (0, 6_000_000, false), (255, 0, false)]
    {
        let (mut arena, actor) = fixture(Some([10, 64, 30]));
        let followed = subject(&mut arena, Some([10, 64, 130]));
        arena.actor_mut(followed).unwrap().word_74 = 0;
        arena.actor_mut(followed).unwrap().word_c4 = 3;
        let handle = arena.actor(followed).unwrap().handle;
        let target = arena
            .follow_target(FollowTargetRequest::resolve_actor(handle))
            .unwrap();
        let primary = arena.import_fixture_target(NativeTarget::ActorFollow(target));
        let mut behavior = super::super::build_actor_behavior(1, 0, 12).unwrap();
        behavior.byte_10 = index;
        behavior.entries_18[0] = [1, 0, 0, 0, 0, 0, 0, age];
        // Group allocation is distinct; no group row can substitute entry+18.
        behavior.groups_1c.clear();
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.target_148 = Some(primary);
        derived.word_164 = 1000;
        derived.behavior_140 = Some(behavior);
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(100);
        arena.family428650_actor_mut(actor).unwrap().byte_22c = 1;
        let mut host = Terrain::new();
        host.fail = true;
        let error = tick(&mut arena, actor, &mut host, 40000).unwrap_err();
        if expected_fire {
            assert!(matches!(error, Selector12TailError::Firing(
                    crate::object_runtime::ActorWeaponFireError::GroupBacking { actor: actual, index: 0, groups: 0 }) if actual == actor));
        } else if index == 255 {
            assert!(matches!(
                error,
                Selector12TailError::RequiredBehaviorEntry { index: -1, .. }
            ));
        } else {
            assert!(matches!(
                error,
                Selector12TailError::Terrain("current H12 unavailable")
            ));
        }
        assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(3));
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(100));
        assert_eq!(host.calls, usize::from(index == 0 && !expected_fire));
    }
}

#[test]
fn another_actual_class_is_rejected_without_changing_arena_or_acquiring_terrain() {
    let (mut arena, _) = fixture(Some([10, 64, 30]));
    let actor = arena
        .allocate_selector10_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 64, 30]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &tables().2,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    let before = format!("{arena:?}");
    let mut host = Terrain::new();
    assert!(matches!(tick(&mut arena, actor, &mut host, 40000),
        Err(Selector12TailError::UnsupportedClass(actual)) if actual == actor));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(host.calls, 0);
}
