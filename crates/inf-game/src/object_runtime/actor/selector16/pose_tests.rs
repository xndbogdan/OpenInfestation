use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface,
    DerivedActorConstructionRequest,
};
use crate::object_runtime::{ActorHandle, FollowTargetRequest};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};

const SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};
static SINE: [u16; 4096] = [0; 4096];

fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn fixture() -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_actor(allocation(Some([10000, 2000, 3000])))
        .unwrap();
    let camera = arena
        .allocate_selector16(allocation(None), SETTINGS)
        .unwrap();
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(actor).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_target(camera, Some(target), SETTINGS)
        .unwrap();
    arena.set_camera_byte_187(camera).unwrap();
    (arena, actor, camera)
}

#[derive(Default)]
struct Host {
    heights: Vec<CameraHeightRequest>,
    fail_height: bool,
    floor: Option<i16>,
    mutate_lost_camera: bool,
    inverse_inputs: Vec<i32>,
    inverse_angle: Option<i32>,
    fail_inverse_at: Option<usize>,
    actual_vehicle_targets: bool,
    derived_basis_before_error: Option<NativeBasis>,
}
impl CameraPoseHost for Host {
    fn query_height(&mut self, request: CameraHeightRequest) -> Result<i16, CameraPoseError> {
        self.heights.push(request);
        if self.fail_height {
            Err(CameraPoseError::Host("terrain unavailable".into()))
        } else {
            Ok(self.floor.unwrap_or(-10000))
        }
    }
    fn derived_component(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CameraDerivedComponent>, CameraPoseError> {
        if self.actual_vehicle_targets {
            arena.camera_derived_component(actor)
        } else {
            Ok(None)
        }
    }
    fn update_derived_camera_targets(
        &mut self,
        arena: &mut ObjectArena,
        request: CameraDerivedTargetRequest<'_>,
    ) -> Result<(), CameraPoseError> {
        if self.actual_vehicle_targets {
            return arena.update_selector2_camera_targets(request, None);
        }
        if let Some(basis) = self.derived_basis_before_error {
            *request.basis = basis;
        }
        Err(CameraPoseError::UnownedHostOperation("004218E0/00421D50"))
    }
    fn reselect_target(
        &mut self,
        _: &ObjectArena,
        _: &mut u32,
    ) -> Result<Option<ActorHandle>, CameraPoseError> {
        Err(CameraPoseError::UnownedHostOperation("0040FB40"))
    }
    fn camera_target_lost(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
        _: ActorHandle,
    ) -> Result<(), CameraPoseError> {
        if self.mutate_lost_camera {
            let state = arena.selector16_mut(camera)?;
            state.byte_184 = 1;
            state.mode_188 = 1;
        }
        Err(CameraPoseError::UnownedHostOperation("player transfer"))
    }
    fn inverse_sine(&mut self, value: i32) -> Result<i32, CameraPoseError> {
        self.inverse_inputs.push(value);
        if self.fail_inverse_at == Some(self.inverse_inputs.len()) {
            return Err(CameraPoseError::Host("inverse sine unavailable".into()));
        }
        self.inverse_angle
            .ok_or(CameraPoseError::UnownedHostOperation("0048F4D0"))
    }
    fn rotate_local_vector(
        &mut self,
        _: &mut NativeBasis,
        _: [u16; 3],
    ) -> Result<(), CameraPoseError> {
        Err(CameraPoseError::UnownedHostOperation("0047A760"))
    }
}

#[test]
fn fresh_actual_special_selection_retains_mode_snap_and_global_before_unknown_xy() {
    let (mut arena, _, camera) = fixture();
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
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
    .unwrap()
    .entries
    .remove(0);
    let terrain = TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut request = allocation(Some([10000, 2000, 3000]));
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    request.terrain.world_flags = Some(0);
    let vehicle = arena
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
            vehicle,
            crate::object_runtime::BaseContext {
                position_words: Some([10000, 2000, 3000]),
                placement_index: 0,
            },
        )
        .unwrap();
    // Supplied active byte only: the unowned input writer has not made the
    // constructor's position/forward/up holes known.
    arena.selector2_actor_mut(vehicle).unwrap().byte_1c0 = 1;
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(vehicle).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_target(camera, Some(target), SETTINGS)
        .unwrap();
    let initial = arena.selector16(camera).unwrap().clone();
    let vehicle_before = arena.object(vehicle).unwrap().clone();
    let basis_before = arena.actor(camera).unwrap().rotating.basis;
    let mut host = Host {
        actual_vehicle_targets: true,
        ..Host::default()
    };
    let mut globals = CameraGlobalState::INITIAL;
    let mut rng = 123;
    assert_eq!(
        arena.tick_camera_pose(
            camera,
            CameraPoseTickRequest {
                microseconds: 32000,
                settings: SETTINGS,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 0,
                globals: &mut globals,
                rng: &mut rng,
                math: &NativeBasisMath::new(SINE),
                host: &mut host,
            }
        ),
        Err(CameraPoseError::UninitializedSpecialViewStorage {
            actor: vehicle,
            offset: 0x1ca,
            bytes: 4
        })
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!((state.mode_188, state.byte_187), (4, 1));
    assert_eq!(state.eye_138, initial.eye_138);
    assert_eq!(state.look_15c, initial.look_15c);
    assert_eq!(arena.actor(camera).unwrap().rotating.basis, basis_before);
    assert!(globals.special_view_active);
    assert_eq!(
        globals.cached_position,
        CameraGlobalState::INITIAL.cached_position
    );
    assert_eq!(rng, 123);
    assert!(host.heights.is_empty());
    assert!(host.inverse_inputs.is_empty());
    assert_eq!(arena.object(vehicle).unwrap(), &vehicle_before);
}

#[test]
fn derived_dispatch_keeps_host_basis_prefix_before_generic_focus_requirements() {
    let (mut arena, actor, camera) = fixture();
    let math = NativeBasisMath::new(SINE);
    let mut supplied_basis = NativeBasis::INITIAL;
    supplied_basis.words[7] = 0x3f00_0000;
    for mode in [3, 4] {
        let mut state = arena.selector16(camera).unwrap().clone();
        state.mode_188 = mode;
        let primary_ref = state.target_18c.unwrap();
        let mut primary = arena.actor_follow_target(primary_ref).unwrap();
        primary.focus_08 = None;
        primary.velocity_1a = None;
        arena
            .write_native_target(
                primary_ref,
                crate::object_runtime::NativeTarget::ActorFollow(primary),
            )
            .unwrap();
        let initial = state.clone();
        let mut basis = NativeBasis::INITIAL;
        let mut host = Host {
            derived_basis_before_error: Some(supplied_basis),
            ..Host::default()
        };
        assert_eq!(
            arena.update_camera_targets(
                camera,
                &mut state,
                Some(actor),
                Some(CameraDerivedComponent {
                    object: actor,
                    special_view_active: mode == 4
                }),
                &mut basis,
                &math,
                &mut host,
                0
            ),
            Err(CameraPoseError::UnownedHostOperation("004218E0/00421D50"))
        );
        assert_eq!(state, initial);
        assert_eq!(basis, supplied_basis);
        assert_eq!(arena.actor(camera).unwrap().rotating.basis, supplied_basis);
    }
    // Every other branch, including null-derived 3/4, retains its existing
    // focus then velocity admission before mode-specific effects.
    for mode in [0, 1, 2, 3, 4, 99] {
        for missing_focus in [true, false] {
            let mut state = arena.selector16(camera).unwrap().clone();
            state.mode_188 = mode;
            let primary_ref = state.target_18c.unwrap();
            let mut primary = arena.actor_follow_target(primary_ref).unwrap();
            primary.focus_08 = if missing_focus { None } else { Some([1, 2, 3]) };
            primary.velocity_1a = if missing_focus { Some([4, 5, 6]) } else { None };
            arena
                .write_native_target(
                    primary_ref,
                    crate::object_runtime::NativeTarget::ActorFollow(primary),
                )
                .unwrap();
            let initial = state.clone();
            let arena_before = format!("{arena:?}");
            let mut basis = NativeBasis::INITIAL;
            assert_eq!(
                arena.update_camera_targets(
                    camera,
                    &mut state,
                    Some(actor),
                    None,
                    &mut basis,
                    &math,
                    &mut Host::default(),
                    0
                ),
                Err(CameraPoseError::UnwrittenTarget("primary"))
            );
            assert_eq!(state, initial);
            assert_eq!(basis, NativeBasis::INITIAL);
            assert_eq!(format!("{arena:?}"), arena_before);
        }
    }
}

#[test]
fn full_pose_without_primary_owner_bypasses_special_target_dispatch() {
    let (mut arena, _, camera) = fixture();
    let state = arena.selector16_mut(camera).unwrap();
    state.target_18c = None;
    state.mode_188 = 4;
    state.eye_138.current_position = Some([7, 8, 9]);
    let initial = state.clone();
    let basis_before = arena.actor(camera).unwrap().rotating.basis;
    let mut globals = CameraGlobalState::INITIAL;
    let mut rng = 123;
    let mut host = Host {
        actual_vehicle_targets: true,
        ..Host::default()
    };
    let pose = tick(&mut arena, camera, 32000, &mut globals, &mut rng, &mut host);
    assert_eq!(pose.position_words, [7, 8, 9]);
    assert_eq!(pose.basis, basis_before);
    assert_eq!(pose.target_state, None);
    assert_eq!(arena.selector16(camera).unwrap(), &initial);
    assert_eq!(rng, 123);
    assert!(host.heights.is_empty());
    assert!(host.inverse_inputs.is_empty());
    assert!(!globals.special_view_active);
}

fn tick(
    arena: &mut ObjectArena,
    camera: ObjectId,
    delta: u32,
    globals: &mut CameraGlobalState,
    rng: &mut u32,
    host: &mut Host,
) -> CameraPose {
    arena
        .tick_camera_pose(
            camera,
            CameraPoseTickRequest {
                microseconds: delta,
                settings: SETTINGS,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 0,
                globals,
                rng,
                math: &NativeBasisMath::new(SINE),
                host,
            },
        )
        .unwrap()
}

#[test]
fn shared_follow_pose_initializes_from_live_targets_and_then_slews() {
    let (mut arena, actor, camera) = fixture();
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 4000];
    let mut globals = CameraGlobalState::INITIAL;
    let mut rng = 123;
    let mut host = Host::default();
    let first = tick(
        &mut arena,
        camera,
        32_000,
        &mut globals,
        &mut rng,
        &mut host,
    );
    assert_eq!(first.position_words, [10000, 2150, 2000]);
    assert_eq!(first.target_state, Some(3));
    assert_eq!(first.basis.words[0], 0);
    assert_eq!(
        arena.selector16(camera).unwrap().look_15c.current_position,
        Some([10000, 2000, 3400])
    );
    assert_eq!(globals.cached_position, Some(first.position_words));
    assert_eq!(rng, 123);
    assert_eq!(arena.selector16(camera).unwrap().byte_187, 0);

    arena.owned_mut(actor).unwrap().state.base.position_words = Some([10000, 2000, 3200]);
    let second = tick(
        &mut arena,
        camera,
        32_000,
        &mut globals,
        &mut rng,
        &mut host,
    );
    assert_ne!(second.position_words, first.position_words);
    assert_eq!(
        arena
            .actor_follow_target(arena.selector16(camera).unwrap().target_18c.unwrap())
            .unwrap()
            .focus_08,
        Some([10000, 2000, 3200])
    );
    assert_eq!(
        arena.object(camera).unwrap().base.position_words,
        Some(second.position_words)
    );
}

#[test]
fn direct_eye_tracks_primary_and_refreshes_secondary_look_velocity() {
    let (mut arena, actor, camera) = fixture();
    let secondary = arena
        .allocate_actor(allocation(Some([10000, 3000, 4000])))
        .unwrap();
    arena.owned_mut(secondary).unwrap().state.base.flags |= 0x40000;
    arena.actor_mut(secondary).unwrap().word_74 = 17;
    arena.actor_mut(secondary).unwrap().words_8e_92 = [20, 0xffff, 0x8000];
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(secondary).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_secondary_target(camera, Some(target))
        .unwrap();
    arena.selector16_mut(camera).unwrap().byte_184 = 1;
    arena.set_camera_parameter_mode(camera, u32::MAX).unwrap();
    let mut globals = CameraGlobalState::INITIAL;
    let pose = tick(
        &mut arena,
        camera,
        125_000,
        &mut globals,
        &mut 123,
        &mut Host::default(),
    );
    assert_eq!(
        pose.position_words,
        arena.object(actor).unwrap().base.position_words.unwrap()
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.look_15c.current_position, Some([10000, 3017, 4000]));
    assert_eq!(
        state.look_15c.current_velocity,
        Some([(-20_i16) as u16, 1, 0x8000])
    );
    assert_eq!((state.block_138.word_18, state.block_15c.word_18), (1, 1));
}

#[test]
fn target_refresh_preserves_the_explicit_point_until_settings_copy_and_keeps_missing_state() {
    let (mut arena, actor, camera) = fixture();
    arena.copy_camera_primary_to_secondary(camera).unwrap();
    let mut target = arena
        .actor_follow_target(arena.selector16(camera).unwrap().target_190.unwrap())
        .unwrap();
    assert_eq!((target.focus_08, target.velocity_1a), (None, None));
    assert_eq!(target.focus_14, Some([10000, 2000, 3000]));
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0xffff, 0xffff, 1]);
    arena.actor_mut(actor).unwrap().word_74 = 2;
    assert_eq!(arena.refresh_follow_target(&mut target, 0).unwrap(), 3);
    assert_eq!(target.focus_08, Some([0xffff, 1, 1]));
    assert_eq!(target.focus_14, Some([10000, 2000, 3000]));
    arena.refresh_follow_target(&mut target, 1).unwrap();
    assert_eq!(target.focus_14, target.focus_08);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let before = target;
    assert_eq!(arena.refresh_follow_target(&mut target, 1).unwrap(), 0);
    assert_eq!(target, before);
    assert_eq!(
        arena.follow_target_basis(&target).unwrap(),
        NativeBasis::INITIAL
    );
}

#[test]
fn first_person_branch_copies_target_basis_and_rearms_snap_each_tick() {
    let (mut arena, actor, camera) = fixture();
    arena.actor_mut(actor).unwrap().rotating.basis.words[0] = 17;
    arena.selector16_mut(camera).unwrap().mode_188 = 2;
    let mut globals = CameraGlobalState::INITIAL;
    let pose = tick(
        &mut arena,
        camera,
        32_000,
        &mut globals,
        &mut 123,
        &mut Host::default(),
    );
    assert_eq!(pose.position_words, [10000, 2000, 3000]);
    assert_eq!(pose.basis, arena.actor(actor).unwrap().rotating.basis);
    assert_eq!(arena.selector16(camera).unwrap().byte_187, 0);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([11000, 2500, 3100]);
    let second = tick(
        &mut arena,
        camera,
        1,
        &mut globals,
        &mut 123,
        &mut Host::default(),
    );
    assert_eq!(second.position_words, [11000, 2500, 3100]);
}

#[test]
fn first_person_second_inverse_sample_observes_first_rotation_renormalization() {
    let (mut arena, _, camera) = fixture();
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 100;
    basis.words[2] = 0.5_f32.to_bits();
    let mut host = Host {
        inverse_angle: Some(0),
        ..Host::default()
    };
    // A synthetic zero table makes the first rotation's renormalization
    // rebuild X.y as zero. The original value would instead sample 16384.
    arena
        .apply_first_person_tilt(camera, &mut basis, &NativeBasisMath::new(SINE), &mut host)
        .unwrap();
    assert_eq!(host.inverse_inputs, [0, 0]);
    assert_eq!(basis.words[0], 1);
    assert_eq!(arena.actor(camera).unwrap().rotating.basis, basis);

    assert_eq!(compress_first_person_angle(1), 0);
    assert_eq!(compress_first_person_angle(-1), 0);
    assert_eq!(compress_first_person_angle(0x4000), 0x2000);
    assert_eq!(compress_first_person_angle(-0x4000), -0x2000);
    assert_eq!(compress_first_person_angle(0x4001), 0x2001);
    assert_eq!(compress_first_person_angle(-0x4001), -0x2001);
}

#[test]
fn first_person_second_inverse_failure_retains_reached_basis_rotation() {
    let (mut arena, _, camera) = fixture();
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 100;
    let mut host = Host {
        inverse_angle: Some(0),
        fail_inverse_at: Some(2),
        ..Host::default()
    };
    let result =
        arena.apply_first_person_tilt(camera, &mut basis, &NativeBasisMath::new(SINE), &mut host);
    assert_eq!(
        result,
        Err(CameraPoseError::Host("inverse sine unavailable".into()))
    );
    assert_eq!(basis.words[0], 0);
    assert_eq!(arena.actor(camera).unwrap().rotating.basis, basis);
}

#[test]
fn shake_consumes_the_shared_rng_in_interval_then_x_z_y_order() {
    let (mut arena, _, camera) = fixture();
    let mut globals = CameraGlobalState::INITIAL;
    globals.shake_remaining_micros = 0;
    let mut rng = 123;
    let mut expected_rng = rng;
    let interval = crate::name_entry::name_entry_rng_next(&mut expected_rng)
        .wrapping_add(0x1312d)
        .wrapping_mul(64);
    let mut jitter = [0_u16; 3];
    for axis in [0, 2, 1] {
        jitter[axis] = 32_u16
            .wrapping_sub((crate::name_entry::name_entry_rng_next(&mut expected_rng) as u16) >> 10);
    }
    let pose = arena
        .tick_camera_pose(
            camera,
            CameraPoseTickRequest {
                microseconds: 125_000,
                settings: SETTINGS,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 1,
                globals: &mut globals,
                rng: &mut rng,
                math: &NativeBasisMath::new(SINE),
                host: &mut Host::default(),
            },
        )
        .unwrap();
    assert_eq!(rng, expected_rng);
    assert_eq!(globals.shake_remaining_micros, interval);
    assert_eq!(
        pose.position_words,
        std::array::from_fn(|axis| [10000_u16, 2150, 2600][axis].wrapping_add(jitter[axis]))
    );
    assert_eq!(globals.shake_step_micros, 50_000);
}

#[test]
fn terrain_failure_keeps_reached_refresh_timer_mode_and_parameter_writes() {
    let (mut arena, actor, camera) = fixture();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([11000, 2500, 3100]);
    let state = arena.selector16_mut(camera).unwrap();
    state.byte_186 = 1;
    state.dword_19c = 7;
    state.mode_188 = 99;
    let mut globals = CameraGlobalState::INITIAL;
    globals.special_view_active = true;
    let mut host = Host {
        fail_height: true,
        ..Host::default()
    };
    let result = arena.tick_camera_pose(
        camera,
        CameraPoseTickRequest {
            microseconds: 32000,
            settings: SETTINGS,
            settings_7c: 0,
            up: [0.0, 1.0, 0.0],
            shake_enabled: 0,
            globals: &mut globals,
            rng: &mut 123,
            math: &NativeBasisMath::new(SINE),
            host: &mut host,
        },
    );
    assert_eq!(
        result,
        Err(CameraPoseError::Host("terrain unavailable".into()))
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!(
        arena
            .actor_follow_target(state.target_18c.unwrap())
            .unwrap()
            .focus_08,
        Some([11000, 2500, 3100])
    );
    assert_eq!(state.dword_19c, 32007);
    assert_eq!(state.mode_188, 2);
    assert_eq!(state.eye_138.desired_position, Some([11000, 2500, 3100]));
    assert_eq!(
        state.eye_138.current_position,
        state.eye_138.desired_position
    );
    assert_eq!(state.look_15c.current_position, Some([11000, 2500, 3500]));
    assert_eq!(state.byte_187, 1);
    assert!(!globals.special_view_active);
    assert_eq!(
        globals.cached_position,
        CameraGlobalState::INITIAL.cached_position
    );
}

#[test]
fn lost_target_callback_alias_mutation_is_preserved_on_failure() {
    let (mut arena, actor, camera) = fixture();
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    arena.owned_mut(camera).unwrap().state.base.position_words = Some([10000, 2150, 2600]);
    let mut host = Host {
        mutate_lost_camera: true,
        ..Host::default()
    };
    let mut globals = CameraGlobalState::INITIAL;
    let result = arena.tick_camera_pose(
        camera,
        CameraPoseTickRequest {
            microseconds: 32000,
            settings: SETTINGS,
            settings_7c: 0,
            up: [0.0, 1.0, 0.0],
            shake_enabled: 0,
            globals: &mut globals,
            rng: &mut 123,
            math: &NativeBasisMath::new(SINE),
            host: &mut host,
        },
    );
    assert_eq!(
        result,
        Err(CameraPoseError::UnownedHostOperation("player transfer"))
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!((state.byte_184, state.mode_188), (1, 1));
    assert_eq!(
        arena
            .actor_follow_target(state.target_18c.unwrap())
            .unwrap()
            .velocity_1a,
        Some([0; 3])
    );
    assert_eq!(
        arena.actor(camera).unwrap().words_8e_92,
        [0, 50, (-200_i16) as u16]
    );
}

#[test]
fn terrain_line_clips_desired_eye_and_floor_gap_uses_full_signed_difference() {
    let mut host = Host {
        floor: Some(0),
        ..Host::default()
    };
    let mut eye = [0, 100, 0];
    terrain::clip_eye_to_terrain(&mut eye, [0, 100, 1000], 128, &mut host).unwrap();
    assert_eq!(eye, [0, 313, 23]);
    let mut state = Selector16State::new(SETTINGS);
    state.eye_138.set_target([55, 0x8000, 66], [0; 3]);
    state.look_15c.set_target([77, 0x7fff, 88], [0; 3]);
    state.eye_138.current_position = Some([999, 20, 1000]);
    state.eye_138.current_velocity = Some([0, (-9_i16) as u16, 0]);
    host.floor = Some(20);
    host.heights.clear();
    terrain::clamp_eye(&mut state, &mut host).unwrap();
    assert_eq!(state.eye_138.current_position, Some([999, 100, 1000]));
    assert_eq!(state.eye_138.current_velocity, Some([0, 2, 0]));
    assert_eq!(
        host.heights,
        [
            CameraHeightRequest::InterpolatedTerrain { xz: [55, 66] },
            CameraHeightRequest::WaterSurface {
                xz: [55, 66],
                terrain_height: 20
            },
        ]
    );
}
