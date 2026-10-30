use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, ActorTerrainSurface,
        CameraGlobalState, CameraPoseTickRequest, CameraSettings, DerivedActorConstructionRequest,
        FollowTargetRequest, PlayerActorLookupError,
    },
    terrain_runtime::{TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    native_basis::NativeBasisMath, parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

static SINE: [u16; 4096] = [32767; 4096];
static INVERSE: [i16; 1024] = [123; 1024];
const SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};
const PROCESS: ParticleFactoryProcessState = ParticleFactoryProcessState {
    network_flag: 0,
    local_slot: 1,
    host_flag: 0,
    settings: [0; 20],
    process_null_handle: Some(ActorHandle::from_raw(0)),
};

fn terrain(scale: TerrainHeightScale) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[..4].copy_from_slice(&(-1000_i32 << 8).to_le_bytes());
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = (-7_i8) as u8;
    }
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
    TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: scale,
        },
    )
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0xffff, 2000, 0x8000]),
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

#[derive(Default)]
struct Services {
    transfers: Vec<(PlayerId, ObjectId, u32)>,
    fail_transfer: bool,
}
impl NativeCameraServices for Services {
    fn attach_player_to_camera(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        camera: ObjectId,
    ) -> Result<bool, CameraPoseError> {
        self.transfers
            .push((player, camera, arena.selector16(camera)?.mode_188));
        if self.fail_transfer {
            return Err(CameraPoseError::UnownedHostOperation(
                "player attach callback",
            ));
        }
        Ok(true)
    }
}

#[test]
fn retained_terrain_queries_keep_raw_interpolated_plane_and_wave_units_distinct() {
    for scale in [TerrainHeightScale::Units16, TerrainHeightScale::Units32] {
        let terrain = terrain(scale);
        let mut services = Services::default();
        let mut host = NativeCameraHost::new(
            NativeCameraInputs {
                terrain: &terrain,
                world_flags: 0x40,
                sine_quarter: &SINE,
                inverse_sine: &INVERSE,
                process: None,
            },
            &mut services,
        );
        let xz = [0xffff, 0x8000];
        let expected = match scale {
            TerrainHeightScale::Units16 => -112,
            TerrainHeightScale::Units32 => -224,
        };
        assert_eq!(
            host.query_height(CameraHeightRequest::RawTerrainCell { xz })
                .unwrap(),
            expected
        );
        assert_eq!(
            host.query_height(CameraHeightRequest::InterpolatedTerrain { xz })
                .unwrap(),
            terrain.height_at(xz)
        );
        assert_eq!(
            host.query_height(CameraHeightRequest::WaterPlane).unwrap(),
            (terrain.plane_base() >> terrain.plane_shift()) as i16
        );
        assert_eq!(
            host.query_height(CameraHeightRequest::WaterSurface {
                xz,
                terrain_height: -2000
            })
            .unwrap(),
            terrain.water_surface_height(
                TerrainWaterSurfaceRequest {
                    xz,
                    wave_clock: 0,
                    world_flags: 0x40,
                    terrain_height: -2000
                },
                &SINE
            )
        );
        assert_eq!(host.inverse_sine(i32::MIN).unwrap(), -123);
    }
}

#[test]
fn derived_query_uses_the_concrete_vehicle_view_and_its_live_special_byte() {
    let mut arena = arena();
    let helper = arena.allocate_selector90(allocation()).unwrap();
    assert_eq!(arena.camera_derived_component(helper).unwrap(), None);
    let terrain = terrain(TerrainHeightScale::Units32);
    let mut request = allocation();
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    request.terrain.world_flags = Some(0);
    let mut rng = 1;
    let vehicle = arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut rng,
        )
        .unwrap();
    assert_eq!(
        arena.camera_derived_component(vehicle).unwrap(),
        Some(CameraDerivedComponent {
            object: vehicle,
            special_view_active: false
        })
    );
    let DerivedActorClass::Selector2(vehicle_state) =
        &mut arena.derived_actor_mut(vehicle).unwrap().class
    else {
        unreachable!()
    };
    vehicle_state.byte_1c0 = 1;
    assert!(
        arena
            .camera_derived_component(vehicle)
            .unwrap()
            .unwrap()
            .special_view_active
    );
}

#[test]
fn reselect_rng_skips_inactive_and_camera_candidates_and_keeps_first_equal_camera() {
    let mut arena = arena();
    let inactive = arena.allocate_actor(allocation()).unwrap();
    let first = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let second = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    arena.set_flag0(inactive, false).unwrap();
    arena.set_flag0(first, true).unwrap();
    arena.set_flag0(second, true).unwrap();
    let mut rng = 0x1234_5678;
    assert_eq!(
        arena.reselect_camera_target(&mut rng).unwrap(),
        Some(arena.actor(first).unwrap().handle)
    );
    assert_eq!(rng, 0x1234_5678);
    arena.set_flag0(inactive, true).unwrap();
    let mut expected_rng = rng;
    name_entry_rng_next(&mut expected_rng);
    assert_eq!(
        arena.reselect_camera_target(&mut rng).unwrap(),
        Some(arena.actor(inactive).unwrap().handle)
    );
    assert_eq!(rng, expected_rng);
}

#[test]
fn stopped_construction_camera_candidate_consumes_no_score_rng_and_healthy_target_wins() {
    let mut arena = arena();
    let stopped = arena.allocate_actor(allocation()).unwrap();
    let healthy = arena.allocate_actor(allocation()).unwrap();
    for object in [stopped, healthy] {
        arena.set_flag0(object, true).unwrap();
    }
    arena.actor_mut(stopped).unwrap().word_c4 = 3;
    arena.actor_mut(stopped).unwrap().word_b4 = 2;
    arena.stop_object_construction(stopped).unwrap();
    let before = arena.object(stopped).unwrap().clone();
    let nodes = arena.actor_list().to_vec();
    let mut rng = 456;
    let mut expected_rng = rng;
    name_entry_rng_next(&mut expected_rng);

    assert_eq!(
        arena.reselect_camera_target(&mut rng).unwrap(),
        Some(arena.actor(healthy).unwrap().handle)
    );
    assert_eq!(rng, expected_rng);
    assert_eq!(arena.object(stopped).unwrap(), &before);
    assert_eq!(arena.actor_list(), nodes);
}

#[test]
fn reselect_speed_overflow_and_signed_b4_range_preserve_native_score_bits() {
    let mut arena = arena();
    let slow = arena.allocate_actor(allocation()).unwrap();
    let fast = arena.allocate_actor(allocation()).unwrap();
    for object in [slow, fast] {
        arena.set_flag0(object, true).unwrap();
    }
    arena.actor_mut(slow).unwrap().word_b4 = u32::MAX;
    arena.actor_mut(fast).unwrap().words_8e_92 = [0x8000; 3];
    let mut rng = 456;
    assert_eq!(
        arena.reselect_camera_target(&mut rng).unwrap(),
        Some(arena.actor(fast).unwrap().handle)
    );
    arena.actor_mut(slow).unwrap().word_b4 = 2;
    let mut rng = 456;
    assert_eq!(
        arena.reselect_camera_target(&mut rng).unwrap(),
        Some(arena.actor(slow).unwrap().handle)
    );
}

#[test]
fn lost_target_transfers_matching_owner_but_tests_first_players_d8_only() {
    let mut arena = arena();
    let original = arena.allocate_actor(allocation()).unwrap();
    let current = arena.allocate_actor(allocation()).unwrap();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let first = arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
    let owner = arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
    let previous = arena.actor(original).unwrap().handle;
    arena.players.list[0].handle_d4 = arena.actor(current).unwrap().handle;
    arena.players.list[1].handle_d8 = previous;
    let terrain = terrain(TerrainHeightScale::Units32);
    let mut services = Services {
        fail_transfer: true,
        ..Services::default()
    };
    let mut host = NativeCameraHost::new(
        NativeCameraInputs {
            terrain: &terrain,
            world_flags: 0,
            sine_quarter: &SINE,
            inverse_sine: &INVERSE,
            process: Some(&PROCESS),
        },
        &mut services,
    );
    assert!(host
        .camera_target_lost(&mut arena, camera, previous)
        .is_err());
    assert_eq!(host.services.transfers, [(owner, camera, 3)]);
    // +D4 is alive but was not the +D8-only 0046B060 query.
    assert_ne!(owner, first);
    arena.players.list[0].handle_d8 = arena.actor(current).unwrap().handle;
    host.camera_target_lost(&mut arena, camera, previous)
        .unwrap();
    assert_eq!(host.services.transfers.len(), 1);
    arena.actor_mut(current).unwrap().word_c4 = 2;
    assert!(host
        .camera_target_lost(&mut arena, camera, previous)
        .is_err());
    assert_eq!(host.services.transfers.len(), 2);
}

#[test]
fn missing_process_null_stops_loss_before_fallback_resolution_or_view_change() {
    let mut arena = arena();
    let original = arena.allocate_actor(allocation()).unwrap();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
    let previous = arena.actor(original).unwrap().handle;
    let terrain = terrain(TerrainHeightScale::Units32);
    let unavailable = ParticleFactoryProcessState {
        process_null_handle: None,
        ..PROCESS
    };
    for process in [None, Some(&unavailable)] {
        let before = arena.selector16(camera).unwrap().clone();
        let cache = arena.actor_handles.as_ref().unwrap().cache();
        let players = arena.world_players().to_vec();
        let mut services = Services::default();
        let mut host = NativeCameraHost::new(
            NativeCameraInputs {
                terrain: &terrain,
                world_flags: 0,
                sine_quarter: &SINE,
                inverse_sine: &INVERSE,
                process,
            },
            &mut services,
        );
        assert_eq!(
            host.camera_target_lost(&mut arena, camera, previous),
            Err(CameraPoseError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            ))
        );
        assert!(host.services.transfers.is_empty());
        assert_eq!(arena.selector16(camera).unwrap(), &before);
        assert_eq!(arena.actor_handles.as_ref().unwrap().cache(), cache);
        assert_eq!(arena.world_players(), players);
    }
}

#[test]
fn process_null_source_bypass_precedes_fresh_first_player_fallback() {
    let mut arena = arena();
    let original = arena.allocate_actor(allocation()).unwrap();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let first = arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
    let owner = arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]);
    let previous = arena.actor(original).unwrap().handle;
    arena.players.list[1].handle_d8 = previous;
    let terrain = terrain(TerrainHeightScale::Units32);
    let process = ParticleFactoryProcessState {
        process_null_handle: Some(previous),
        ..PROCESS
    };
    let mut services = Services::default();
    let mut host = NativeCameraHost::new(
        NativeCameraInputs {
            terrain: &terrain,
            world_flags: 0,
            sine_quarter: &SINE,
            inverse_sine: &INVERSE,
            process: Some(&process),
        },
        &mut services,
    );
    host.camera_target_lost(&mut arena, camera, previous)
        .unwrap();
    assert_eq!(host.services.transfers.len(), 1);
    assert_eq!(host.services.transfers[0].0, first);
    assert_ne!(first, owner);
}

#[test]
fn tick_missing_process_null_preserves_escape_xyz_velocity_and_published_camera_state() {
    let mut arena = arena();
    let original = arena.allocate_actor(allocation()).unwrap();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(
            arena.actor(original).unwrap().handle,
        ))
        .unwrap()
        .unwrap();
    arena
        .replace_camera_target(camera, Some(target), SETTINGS)
        .unwrap();
    arena.actor_mut(original).unwrap().word_c4 = 2;
    arena.owned_mut(camera).unwrap().state.base.position_words =
        Some([10000, (-1000_i16) as u16, 3000]);
    arena.selector16_mut(camera).unwrap().mode_188 = 2;
    arena.selector16_mut(camera).unwrap().byte_186 = 0;
    arena.selector16_mut(camera).unwrap().byte_187 = 0;
    let terrain = terrain(TerrainHeightScale::Units32);
    let mut services = Services::default();
    let mut host = NativeCameraHost::new(
        NativeCameraInputs {
            terrain: &terrain,
            world_flags: 0,
            sine_quarter: &SINE,
            inverse_sine: &INVERSE,
            process: None,
        },
        &mut services,
    );
    let mut globals = CameraGlobalState::INITIAL;
    let mut rng = 123;
    let result = arena.tick_camera_pose(
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
        },
    );
    assert_eq!(
        result,
        Err(CameraPoseError::PlayerLookup(
            PlayerActorLookupError::MissingProcessNullHandle
        ))
    );
    assert!(host.services.transfers.is_empty());
    assert_eq!(arena.selector16(camera).unwrap().mode_188, 2);
    assert_eq!(arena.selector16(camera).unwrap().byte_187, 1);
    assert_eq!(
        arena.actor(camera).unwrap().words_8e_92,
        [0, 50, (-200_i16) as u16]
    );
    assert_eq!(
        arena.object(camera).unwrap().base.position_words,
        Some([10000, (-84_i16) as u16, 2950])
    );
    assert_eq!(globals, CameraGlobalState::INITIAL);
    assert_eq!(rng, 123);
}

#[test]
fn draw_publication_sign_extends_xyz_and_keeps_counter_and_basis_bits() {
    let mut arena = arena();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let mut basis = NativeBasis::INITIAL;
    basis.words[0] = 99;
    basis.words[2] = (-0.0_f32).to_bits();
    arena.actor_mut(camera).unwrap().rotating.basis = basis;
    let request = arena.camera_draw_request(camera).unwrap();
    assert_eq!(
        request,
        CameraDrawRequest {
            eye: [-1, 2000, -32768],
            basis,
            coordinate_shift: 16
        }
    );
    assert_eq!(
        super::super::CameraPose {
            position_words: [0xffff, 2000, 0x8000],
            basis,
            target_state: None
        }
        .draw_request(),
        request
    );
}
