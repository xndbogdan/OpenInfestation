use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorConstructionRequest, ActorFollowTarget, ActorHandle, ActorRegistryConfig,
        ActorServiceHeapError, ActorTerrainInputs, ActorTerrainSurface, BaseContext,
        CameraDerivedComponent, CameraParameterState, DerivedActorConstructionRequest,
        FollowTargetPoint, FollowTargetRequest, NativeHeapRef, NativeTarget, ObjectId,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    native_basis::NativeBasisMath, parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, TERRAIN_ENTRY_FIXED_BYTES,
};

static SINE: [u16; 4096] = [0; 4096];
const POSITION: [u16; 3] = [1000, 2000, 3000];

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(POSITION),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &SINE,
        },
    }
}

fn fixture() -> (ObjectArena, ObjectId) {
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let terrain = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0);
    let terrain = TerrainRuntime::new(
        &terrain,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut request = allocation();
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    let actor = arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap();
    arena
        .initialize_context(
            actor,
            BaseContext {
                position_words: Some(POSITION),
                placement_index: 0,
            },
        )
        .unwrap();
    (arena, actor)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Parameters {
    eye: CameraParameterState,
    look: CameraParameterState,
    basis: NativeBasis,
    primary_target: Option<NativeHeapRef>,
}

impl Parameters {
    fn new() -> Self {
        // Existing current vectors must survive desired-vector publication.
        let parameter = CameraParameterState {
            current_position: Some([7, 8, 9]),
            current_velocity: Some([10, 11, 12]),
            desired_position: None,
            desired_velocity: None,
        };
        Self {
            eye: parameter,
            look: parameter,
            basis: NativeBasis::INITIAL,
            primary_target: None,
        }
    }

    fn call(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        mode: CameraDerivedTargetMode,
        process: Option<&ParticleFactoryProcessState>,
        settings_7c: u32,
        math: &NativeBasisMath,
    ) -> Result<(), CameraPoseError> {
        arena.update_selector2_camera_targets(
            CameraDerivedTargetRequest {
                component: CameraDerivedComponent {
                    object: actor,
                    special_view_active: mode == CameraDerivedTargetMode::SpecialView,
                },
                mode,
                eye: &mut self.eye,
                look: &mut self.look,
                basis: &mut self.basis,
                primary_target: self.primary_target,
                settings_7c,
                math,
            },
            process,
        )
    }
}

fn process(null: Option<u32>) -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag: 0x1234,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: null.map(ActorHandle::from_raw),
    }
}

fn supply_special_view(arena: &mut ObjectArena, actor: ObjectId) {
    // Declared complete post-constructor storage, not a native activation.
    let vehicle = arena.selector2_actor_mut(actor).unwrap();
    vehicle.special_position_1ca = [Some(2000), Some(0xfffe), Some(4000)];
    vehicle.special_forward_1d0 = [0.0_f32, 0.0, 1.0].map(|v| Some(v.to_bits()));
    vehicle.special_up_1dc = [0.0_f32, 1.0, 0.0].map(|v| Some(v.to_bits()));
}

fn primary_target(arena: &mut ObjectArena) -> ActorFollowTarget {
    let referent = arena.allocate_actor(allocation()).unwrap();
    arena
        .initialize_context(
            referent,
            BaseContext {
                position_words: Some(POSITION),
                placement_index: 0,
            },
        )
        .unwrap();
    arena.actor_mut(referent).unwrap().words_8e_92 = [101, 202, 303];
    let mut target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(referent).unwrap().handle,
        ))
        .unwrap();
    arena.refresh_follow_target(&mut target, 0).unwrap();
    target
}

#[test]
fn special_constructor_holes_and_atomic_xy_then_z_preserve_temporary_prefix() {
    let (mut arena, actor) = fixture();
    let vehicle = arena.selector2_actor_mut(actor).unwrap();
    assert_eq!(vehicle.special_position_1ca, [None; 3]);
    assert_eq!(vehicle.special_forward_1d0, [None; 3]);
    assert_eq!(vehicle.special_up_1dc, [None; 3]);
    assert_eq!(vehicle.words_1c2_1c8, [0; 4]);
    let math = NativeBasisMath::new(SINE);
    for missing in 0..3 {
        supply_special_view(&mut arena, actor);
        arena
            .selector2_actor_mut(actor)
            .unwrap()
            .special_position_1ca[missing] = None;
        let arena_before = format!("{arena:?}");
        let mut xyz = [Some(7), None, Some(9)];
        let mut basis = NativeBasis::INITIAL;
        let initial_basis = basis;
        assert_eq!(
            arena.special_view_421d50(actor, &mut xyz, &mut basis, &math),
            Err(CameraPoseError::UninitializedSpecialViewStorage {
                actor,
                offset: if missing < 2 { 0x1ca } else { 0x1ce },
                bytes: if missing < 2 { 4 } else { 2 },
            })
        );
        assert_eq!(
            xyz,
            if missing < 2 {
                [Some(7), None, Some(9)]
            } else {
                [Some(2000), Some(0xfffe), Some(9)]
            }
        );
        assert_eq!(basis, initial_basis);
        assert_eq!(format!("{arena:?}"), arena_before);
    }
}

#[test]
fn special_each_missing_forward_or_up_component_retains_only_reached_copies() {
    let (mut arena, actor) = fixture();
    let math = NativeBasisMath::new(SINE);
    let forward = [1.0_f32, 2.0, 3.0].map(f32::to_bits);
    let up = [4.0_f32, 5.0, 6.0].map(f32::to_bits);
    let mut initial = NativeBasis::INITIAL;
    initial.words[0] = 77;
    for missing in 0..6 {
        supply_special_view(&mut arena, actor);
        let vehicle = arena.selector2_actor_mut(actor).unwrap();
        vehicle.special_forward_1d0 = forward.map(Some);
        vehicle.special_up_1dc = up.map(Some);
        if missing < 3 {
            vehicle.special_forward_1d0[missing] = None;
        } else {
            vehicle.special_up_1dc[missing - 3] = None;
        }
        let arena_before = format!("{arena:?}");
        let mut xyz = [None; 3];
        let mut basis = initial;
        assert_eq!(
            arena.special_view_421d50(actor, &mut xyz, &mut basis, &math),
            Err(CameraPoseError::UninitializedSpecialViewStorage {
                actor,
                offset: if missing < 3 {
                    0x1d0 + missing as u16 * 4
                } else {
                    0x1dc + (missing - 3) as u16 * 4
                },
                bytes: 4,
            })
        );
        assert_eq!(xyz, [Some(2000), Some(48), Some(4000)]);
        let mut expected = initial;
        for copy in 0..missing {
            if copy < 3 {
                expected.words[7 + copy] = forward[copy];
            } else {
                expected.words[4 + copy - 3] = up[copy - 3];
            }
        }
        assert_eq!(basis, expected);
        assert_eq!(format!("{arena:?}"), arena_before);
    }
}

#[test]
fn special_complete_targets_use_stored_eye_fresh_xyz_and_actual_primary_velocity() {
    let (mut arena, actor) = fixture();
    supply_special_view(&mut arena, actor);
    arena.actor_mut(actor).unwrap().words_8e_92 = [7, 8, 9];
    let mut target = primary_target(&mut arena);
    // SpecialView consumes virtual +08's velocity, never the focus cache.
    target.focus_08 = None;
    target.focus_14 = None;
    let mut parameters = Parameters::new();
    parameters.primary_target =
        Some(arena.import_fixture_target(NativeTarget::ActorFollow(target)));
    let initial = parameters.clone();
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let arena_before = format!("{arena:?}");
    let missing_process_null = process(None);
    for owner in [None, Some(&missing_process_null)] {
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::SpecialView,
                owner,
                0xffff_ffff,
                &math,
            )
            .unwrap();
        assert_eq!(parameters.eye.desired_position, Some([2000, 48, 4000]));
        assert_eq!(parameters.eye.desired_velocity, Some([7, 8, 9]));
        assert_eq!(parameters.look.desired_position, Some([1000, 2000, 3500]));
        assert_eq!(parameters.look.desired_velocity, Some([101, 202, 303]));
        assert_eq!(parameters.basis.words[0], 2);
        assert_eq!(basis_z(parameters.basis), [0.0, 0.0, 1.0]);
        assert_eq!(
            parameters.eye.current_position,
            initial.eye.current_position
        );
        assert_eq!(
            parameters.eye.current_velocity,
            initial.eye.current_velocity
        );
        assert_eq!(
            parameters.look.current_position,
            initial.look.current_position
        );
        assert_eq!(
            parameters.look.current_velocity,
            initial.look.current_velocity
        );
        assert_eq!(format!("{arena:?}"), arena_before);
    }
}

#[test]
fn special_partial_basis_error_does_not_publish_either_parameter_target() {
    let (mut arena, actor) = fixture();
    supply_special_view(&mut arena, actor);
    let vehicle = arena.selector2_actor_mut(actor).unwrap();
    vehicle.special_forward_1d0 = [Some(0.5_f32.to_bits()), None, Some(1.0_f32.to_bits())];
    let mut parameters = Parameters::new();
    let initial = parameters.clone();
    let arena_before = format!("{arena:?}");
    assert_eq!(
        parameters.call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::SpecialView,
            None,
            0,
            &NativeBasisMath::new(SINE)
        ),
        Err(CameraPoseError::UninitializedSpecialViewStorage {
            actor,
            offset: 0x1d4,
            bytes: 4
        })
    );
    assert_eq!(parameters.eye, initial.eye);
    assert_eq!(parameters.look, initial.look);
    let mut expected_basis = initial.basis;
    expected_basis.words[7] = 0.5_f32.to_bits();
    assert_eq!(parameters.basis, expected_basis);
    assert_eq!(format!("{arena:?}"), arena_before);
}

#[test]
fn special_signed_word_yaw_then_pitch_is_independent_of_chase_mode_and_quarter() {
    let (mut arena, actor) = fixture();
    supply_special_view(&mut arena, actor);
    let target = primary_target(&mut arena);
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    arena.selector2_actor_mut(actor).unwrap().dwords_1e8_1f4[0] = 0xffff_ffff;
    arena.actor_mut(actor).unwrap().words_94_9c[1] = 0x7fff_ffff;
    for (pitch, direction, look_y) in [(0x4000, -1.0, 1500), (0xc000, 1.0, 2500)] {
        arena.selector2_actor_mut(actor).unwrap().words_1c2_1c8[2..4]
            .copy_from_slice(&[0x4000, pitch]);
        let mut parameters = Parameters::new();
        parameters.primary_target =
            Some(arena.import_fixture_target(NativeTarget::ActorFollow(target)));
        let before = format!("{arena:?}");
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::SpecialView,
                None,
                0,
                &math,
            )
            .unwrap();
        assert_eq!(basis_z(parameters.basis), [0.0, direction, 0.0]);
        assert_eq!(parameters.look.desired_position, Some([1000, look_y, 3000]));
        assert_eq!(parameters.basis.words[0], 2);
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn special_late_xyz_and_primary_errors_keep_basis_and_eye_before_look() {
    let (mut arena, actor) = fixture();
    supply_special_view(&mut arena, actor);
    arena.actor_mut(actor).unwrap().words_8e_92 = [7, 8, 9];
    let mut target = primary_target(&mut arena);
    target.velocity_1a = None;
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    for current_xyz in [None, Some(POSITION)] {
        arena.owned_mut(actor).unwrap().state.base.position_words = current_xyz;
        for primary in [None, Some(target)] {
            let mut parameters = Parameters::new();
            parameters.primary_target = primary
                .map(|target| arena.import_fixture_target(NativeTarget::ActorFollow(target)));
            let old_look = parameters.look;
            let before = format!("{arena:?}");
            assert_eq!(
                parameters.call(
                    &mut arena,
                    actor,
                    CameraDerivedTargetMode::SpecialView,
                    None,
                    0,
                    &math
                ),
                Err(if current_xyz.is_none() {
                    CameraPoseError::Object(ObjectError::ActorPositionUnknown(actor))
                } else {
                    CameraPoseError::UnwrittenTarget("primary")
                })
            );
            assert_eq!(parameters.eye.desired_position, Some([2000, 48, 4000]));
            assert_eq!(parameters.eye.desired_velocity, Some([7, 8, 9]));
            assert_eq!(parameters.basis.words[0], 2);
            assert_eq!(parameters.look, old_look);
            assert_eq!(format!("{arena:?}"), before);
        }
    }
}

#[test]
fn same_live_process_changes_standoff_without_registry_null_fallback() {
    let (mut arena, actor) = fixture();
    let before = format!("{arena:?}");
    let math = NativeBasisMath::new(SINE);
    let mut parameters = Parameters::new();
    let initial = parameters.clone();
    let mut process = process(Some(0));
    for (null, expected_z) in [(0, 2600), (0x1234, 2000), (0, 2600)] {
        process.process_null_handle = Some(ActorHandle::from_raw(null));
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::Chase,
                Some(&process),
                0,
                &math,
            )
            .unwrap();
        assert_eq!(
            parameters.eye.desired_position,
            Some([1000, 2150, expected_z])
        );
        assert_eq!(parameters.look.desired_position, Some([1000, 2000, 3400]));
        for parameter in [parameters.eye, parameters.look] {
            assert_eq!(parameter.current_position, initial.eye.current_position);
            assert_eq!(parameter.current_velocity, initial.eye.current_velocity);
            assert_eq!(parameter.desired_velocity, Some([0; 3]));
        }
        assert_eq!(parameters.basis, initial.basis);
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn missing_current_null_precedes_every_vector_write_and_special_view_bypasses_it() {
    let (mut arena, actor) = fixture();
    let math = NativeBasisMath::new(SINE);
    let missing = process(None);
    for owner in [None, Some(&missing)] {
        let mut parameters = Parameters::new();
        let before = parameters.clone();
        let arena_before = format!("{arena:?}");
        assert_eq!(
            parameters.call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::Chase,
                owner,
                0,
                &math
            ),
            Err(CameraPoseError::MissingVehicleProcessNullHandle)
        );
        assert_eq!(parameters, before);
        assert_eq!(format!("{arena:?}"), arena_before);
        assert_eq!(
            parameters.call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::SpecialView,
                owner,
                0,
                &math
            ),
            Err(CameraPoseError::UninitializedSpecialViewStorage {
                actor,
                offset: 0x1ca,
                bytes: 4,
            })
        );
        assert_eq!(parameters, before);
    }
}

#[test]
fn signed_velocity_clamp_and_word_wrap_are_distinct_from_position_height() {
    let (mut arena, actor) = fixture();
    let math = NativeBasisMath::new(SINE);
    let process = process(Some(0));
    arena.actor_mut(actor).unwrap().word_74 = 999;
    for (velocity, z) in [(5000, 2000), (-5000, 3600), (4, 2600), (-4, 2600)] {
        arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, velocity as u16];
        let mut parameters = Parameters::new();
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::Chase,
                Some(&process),
                0,
                &math,
            )
            .unwrap();
        assert_eq!(parameters.eye.desired_position, Some([1000, 2150, z]));
        assert_eq!(
            parameters.eye.desired_velocity,
            Some([0, 0, velocity as u16])
        );
        assert_eq!(
            parameters.look.desired_velocity,
            parameters.eye.desired_velocity
        );
    }
    arena
        .initialize_context(
            actor,
            BaseContext {
                position_words: Some([0xffff, 0xffe0, 0x0100]),
                placement_index: 0,
            },
        )
        .unwrap();
    arena.actor_mut(actor).unwrap().words_8e_92 = [0; 3];
    let mut parameters = Parameters::new();
    parameters
        .call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::Chase,
            Some(&process),
            0,
            &math,
        )
        .unwrap();
    assert_eq!(parameters.eye.desired_position, Some([0xffff, 118, 0xff70]));
}

#[test]
fn exact_modes_signed_quarter_and_local_rotation_preserve_actor_basis() {
    let (mut arena, actor) = fixture();
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let process = process(Some(0));
    let original_basis = arena.actor(actor).unwrap().rotating.basis;
    for (mode, angle, changes) in [
        (0, 16384, false),
        (0x101, 16384, false),
        (1, -3, false),
        (2, 3, false),
        (1, 16384, true),
        (2, -16384, true),
    ] {
        arena.selector2_actor_mut(actor).unwrap().dwords_1e8_1f4[0] = mode;
        arena.actor_mut(actor).unwrap().words_94_9c[1] = angle as u32;
        let mut parameters = Parameters::new();
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::Chase,
                Some(&process),
                0,
                &math,
            )
            .unwrap();
        assert_eq!(parameters.eye.desired_position, Some([1000, 2150, 2600]));
        assert_eq!(
            parameters.look.desired_position != Some([1000, 2000, 3400]),
            changes
        );
        assert_eq!(arena.actor(actor).unwrap().rotating.basis, original_basis);
        assert_eq!(parameters.basis, original_basis);
    }
}

#[test]
fn real_follow_refresh_uses_raw08_not_retained14_and_own_velocity() {
    for settings in [0, 1] {
        let (mut arena, actor) = fixture();
        let referent = arena.allocate_actor(allocation()).unwrap();
        arena
            .initialize_context(
                referent,
                BaseContext {
                    position_words: Some([1000, 2100, 3400]),
                    placement_index: 0,
                },
            )
            .unwrap();
        arena.actor_mut(referent).unwrap().word_74 = 25;
        arena.actor_mut(referent).unwrap().words_8e_92 = [100, 200, 300];
        arena.actor_mut(actor).unwrap().words_8e_92 = [7, 8, 9];
        let target = arena
            .follow_target(FollowTargetRequest {
                handle: arena.actor(referent).unwrap().handle,
                point: FollowTargetPoint::ExplicitPoint(Some([500, 600, 700])),
            })
            .unwrap();
        let reference = arena.import_fixture_target(NativeTarget::ActorFollow(target));
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
        let math = NativeBasisMath::new(SINE);
        let mut parameters = Parameters::new();
        parameters
            .call(
                &mut arena,
                actor,
                CameraDerivedTargetMode::Chase,
                Some(&process(Some(0))),
                settings,
                &math,
            )
            .unwrap();
        let NativeTarget::ActorFollow(target) = arena
            .native_target(arena.derived_actor(actor).unwrap().target_148.unwrap())
            .unwrap()
        else {
            unreachable!()
        };
        assert_eq!(target.focus_08, Some([1000, 2125, 3400]));
        assert_eq!(
            target.focus_14,
            Some(if settings == 0 {
                [500, 600, 700]
            } else {
                [1000, 2125, 3400]
            })
        );
        assert_eq!(target.velocity_1a, Some([100, 200, 300]));
        assert_eq!(parameters.look.desired_velocity, Some([7, 8, 9]));
        assert!(parameters.look.desired_position.unwrap()[2] > POSITION[2]);
    }
}

#[test]
fn stale_known_cache_succeeds_but_unknown_cache_preserves_only_eye_prefix() {
    let (mut arena, actor) = fixture();
    let referent = arena.allocate_actor(allocation()).unwrap();
    let handle = arena.actor(referent).unwrap().handle;
    let mut target = arena
        .follow_target(FollowTargetRequest {
            handle,
            point: FollowTargetPoint::ExplicitPoint(Some([1, 2, 3])),
        })
        .unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: referent,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let math = NativeBasisMath::new(SINE);
    for cache in [Some([1000, 2000, 3400]), None] {
        target.focus_08 = cache;
        let reference = arena.import_fixture_target(NativeTarget::ActorFollow(target));
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
        let mut parameters = Parameters::new();
        let old_look = parameters.look;
        let result = parameters.call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::Chase,
            Some(&process(Some(0))),
            1,
            &math,
        );
        assert_eq!(parameters.eye.desired_position, Some([1000, 2150, 2600]));
        assert_eq!(
            arena
                .derived_actor(actor)
                .unwrap()
                .target_148
                .map(|reference| arena.native_target(reference).unwrap()),
            Some(NativeTarget::ActorFollow(target))
        );
        if cache.is_some() {
            result.unwrap();
            assert_eq!(parameters.look.desired_position, Some([1000, 2000, 3400]));
        } else {
            assert_eq!(
                result,
                Err(CameraPoseError::UnwrittenTarget("vehicle +148 raw +08"))
            );
            assert_eq!(parameters.look, old_look);
        }
    }
}

#[test]
fn late_custody_errors_retain_eye_and_non2_bypasses_missing_registry() {
    let (mut arena, actor) = fixture();
    let fixed = arena.fixed_position_target(None).unwrap();
    let follow = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(actor).unwrap().handle,
        ))
        .unwrap();
    let math = NativeBasisMath::new(SINE);
    let fixed = arena.import_fixture_target(NativeTarget::FixedPosition(fixed));
    let follow_ref = arena.import_fixture_target(NativeTarget::ActorFollow(follow));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(fixed);
    arena.actor_handles = None;
    let mut parameters = Parameters::new();
    parameters
        .call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::Chase,
            Some(&process(Some(0))),
            1,
            &math,
        )
        .unwrap();
    assert_eq!(parameters.look.desired_position, Some([1000, 2000, 3400]));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(follow_ref);
    let mut parameters = Parameters::new();
    let old_look = parameters.look;
    assert_eq!(
        parameters.call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::Chase,
            Some(&process(Some(0))),
            1,
            &math
        ),
        Err(CameraPoseError::Object(ObjectError::MissingActorRegistry))
    );
    assert_eq!(parameters.eye.desired_position, Some([1000, 2150, 2600]));
    assert_eq!(parameters.look, old_look);
    assert_eq!(
        arena.derived_actor(actor).unwrap().target_148,
        Some(follow_ref)
    );
    arena.release_actor_service(follow_ref).unwrap();
    let mut parameters = Parameters::new();
    let old_look = parameters.look;
    assert_eq!(
        parameters.call(
            &mut arena,
            actor,
            CameraDerivedTargetMode::Chase,
            Some(&process(Some(0))),
            0,
            &math
        ),
        Err(CameraPoseError::Object(ObjectError::ActorServiceHeap(
            ActorServiceHeapError::Released(follow_ref)
        )))
    );
    assert_eq!(parameters.eye.desired_position, Some([1000, 2150, 2600]));
    assert_eq!(parameters.look, old_look);
}

#[test]
fn projection_keeps_full_signed_dword_inputs_and_yz_before_x_order() {
    assert_eq!(projection_421d30([1.0, 1e20, -1e20], [1, 1, 1]), 1);
    assert_eq!(projection_421d30([0.0, 0.0, 0.5], [0, 0, 0x1_0002]), 0x8001);
    assert_eq!(projection_421d30([0.0, 0.0, 0.5], [0, 0, -9]), -4);
}
