use super::*;
use crate::{
    model_scratch::{ModelScratchDrawState, ProcessModelScratch},
    terrain_draw::{terrain_draw_plan, TerrainDrawRequest},
};

/// 0044AA10's strip frames dispose a full arena (0x301) like any error: the
/// strip is omitted, the queue resumes for the next strip, and the packets
/// that fit stay queued for the caller's flush.
#[test]
fn a_full_arena_omits_the_strip_and_the_queue_resumes() {
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
    // Existing supplied flooded H12 cells isolate real ground/water command
    // allocation. Original H3/H7, shoreline tables and renderer are unchanged.
    // This pose is a declared geometry fixture, not a retail camera claim.
    let mut terrain = terrain(None);
    let draw = inputs(&tables, [128, 256, 0]);
    terrain_draw_plan(
        draw.view.basis,
        draw.context_projection.fade,
        TerrainGrid {
            spacing: 256,
            mask: 255,
        },
        crate::terrain_draw::TerrainFootprintWidth::Retail,
    )
    .unwrap();
    let mut config = startup.scene;
    config.environment.view = draw.view;
    config.environment.projection = draw.context_projection;
    config.environment.coordinate_shift = draw.coordinate_shift;
    // Genuine allocator capacity: the first terrain packet fits; a later
    // real packet exhausts the same arena. No fabricated host error/receiver.
    config.queue_capacity = 128;
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let untouched_pixels = pixels.rgba().to_vec();
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            scratch.clone(),
            (config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config,
    })
    .unwrap();
    // Do not begin_frame: that method installs the ordinary large arena and
    // would replace the explicit capacity this allocator fixture owns.
    scene.set_terrain(Some(terrain.clone()));
    scene.install_terrain_surface_callbacks(draw.world_flags, draw.clock);
    scene.set_process_rng(Some(0x1020_3040));
    let initial_queue = scene.queue.state();
    assert_eq!(initial_queue.capacity, 128);
    assert!(scene.queue.has_only_implicit_root());
    let scratch_before = scratch.read_pool::<64>(0).unwrap();
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let mut particles = ProcessParticleRuntime::default();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let result = scene.draw_terrain(SoftwareTerrainDrawRequest {
        traversal: TerrainDrawRequest {
            terrain: &mut terrain,
            inputs: draw,
        },
        furniture: SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable,
            birth_inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &process,
            },
            load_factor: 4,
            crystal_flares: None,
        },
    });
    let outcome = result.expect("the strip frames contain a full arena");
    assert!(!outcome.omissions.is_empty());
    assert!(
        outcome
            .omissions
            .iter()
            .all(|omission| omission.reason.starts_with("draw arena exhausted")),
        "{outcome:?}"
    );
    let queue = scene.queue.state();
    assert!(!queue.stopped && !scene.stopped());
    assert_eq!(queue.capacity, initial_queue.capacity);
    assert!(queue.command_count > 0, "the packets that fit stay queued");
    assert!(queue.command_cursor > initial_queue.command_cursor);
    assert_eq!(queue.command_count, scene.stats.terrain_commands);
    assert_eq!(queue.last_flush_completed, 0);
    assert_eq!(scene.stats.begins, 0, "no Begin/reset is error containment");
    assert_eq!(scene.stats.flushes, 0, "AA10 does not replay this batch");
    assert!(scene.furniture_omissions.is_empty());
    assert_eq!(scratch.state().unwrap(), ModelScratchDrawState::Ready);
    assert_eq!(scratch.cursor().unwrap(), 0);
    assert_eq!(scratch.read_pool::<64>(0).unwrap(), scratch_before);
    drop(scene);
    assert_eq!(pixels.rgba(), untouched_pixels.as_slice());
}
