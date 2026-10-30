use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    frontend_scene::{camera_basis, SceneView},
    model_context::ModelProjection,
    process_particles::{births::ParticleBirthInputs, ProcessParticleRuntime},
    terrain_draw::{TerrainDrawInputs, TerrainGrid, TerrainLighting, TerrainRow, TerrainStripMode},
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
    world_furniture_effect::FurnitureEffectQueue,
    world_lighting::WorldLightGrid,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use inf_formats::{
    native_basis::NativeBasisMath, parse_terrain_bank, ByteSpan, FrontendExecutable,
    HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES,
    TERRAIN_PREFIX_BYTES,
};
use std::path::Path;

fn fixture() -> (FrontendAssets, PrimitiveTables, ResidentRuntime) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    (assets, tables, resident)
}

/// Supplied H12 cells isolate the water producer. Original shoreline/sine
/// tables, H3 textures, H7 materials and display conversion remain unmodified.
fn terrain(dry_column: Option<usize>) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for x in 0..512 {
        for z in 0..512 {
            let at = TERRAIN_PREFIX_BYTES + (x * 512 + z) * 2;
            let height = if dry_column == Some(x) { 16i8 } else { -16i8 };
            bytes[at] = height as u8;
            bytes[at + 1] = 0xf0;
        }
    }
    assert_eq!(TERRAIN_CELL_COUNT, 512 * 512);
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
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
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn inputs(tables: &PrimitiveTables, eye: [i32; 3]) -> TerrainDrawInputs<'_> {
    TerrainDrawInputs {
        view: SceneView {
            eye,
            basis: camera_basis(eye, [eye[0], 0, eye[2] + 1024]),
        },
        context_projection: ModelProjection {
            focal: [320, 320],
            viewport: [640, 480],
            center: [320, 240],
            fade: [1024, 4096],
        },
        footprint: crate::terrain_draw::TerrainFootprintWidth::Retail,
        coordinate_shift: 16,
        tint: [128, 160, 192],
        lighting: TerrainLighting::Disabled,
        clock: 0x1234_5678,
        world_flags: 0x40,
        night: None,
        particles_enabled: false,
        tables,
    }
}

fn strip<'a>(
    terrain: &'a TerrainRuntime,
    draw: TerrainDrawInputs<'a>,
    rows: &'a [TerrainRow],
) -> TerrainStripRequest<'a> {
    TerrainStripRequest {
        terrain,
        draw,
        rows,
        grid: TerrainGrid {
            spacing: 256,
            mask: 255,
        },
        mode: TerrainStripMode::Near,
        fade: draw.context_projection.fade,
        sort_bias: 0,
        // C6A0 must re-read H7/74; this stale supplied value cannot win.
        material: 0xdead_beef,
        far_color: 0,
    }
}

#[test]
fn terrain_emission_uses_retained_rng_and_real_pending_particle_bank() {
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let mut particles = ProcessParticleRuntime::default();
    let terrain = terrain(None);
    let rows: Vec<_> = (0..40)
        .map(|z| TerrainRow {
            z,
            left: -7,
            right: 7,
        })
        .collect();
    let request = crate::terrain_draw::TerrainParticleEmissionRequest {
        terrain: &terrain,
        context_position: [65530, 0, -32768],
        rows: &rows,
    };
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
    };
    let mut host = SceneTerrainHost {
        scene: &mut scene,
        furniture: SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            birth_inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &birth_process,
            },
            load_factor: 65536,
            crystal_flares: None,
        },
        furniture_report: FurnitureSceneReport::default(),
        omissions: Vec::new(),
        first_hard: None,
        water: false,
    };
    assert!(matches!(
        host.particles(request),
        Err(SoftwareTerrainError::Unowned(
            "0044C4A0 retained process RNG"
        ))
    ));
    assert!(host.furniture.particles.pending(1).unwrap().is_empty());
    host.scene.set_process_rng(Some(0));
    host.particles(request).unwrap();
    assert_ne!(host.scene.process_rng(), Some(0));
    let births = host.furniture.particles.pending(1).unwrap();
    assert!(!births.is_empty());
    for &id in births {
        let record = host.furniture.particles.record(id).unwrap();
        assert_eq!(record.read_dword(0x14).unwrap(), 0x1234_5678);
        assert_eq!(record.read_word(0x12).unwrap() & 0x800, 0x800);
    }
    assert_eq!(
        host.scene.stats.terrain_commands, 0,
        "births await the native particle draw pass"
    );
}

#[test]
fn water_resource_bounds_preserve_native_null_and_required_loader_paths() {
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let mut particles = ProcessParticleRuntime::default();
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
    };
    let mut host = SceneTerrainHost {
        scene: &mut scene,
        furniture: SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            birth_inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &birth_process,
            },
            load_factor: 4,
            crystal_flares: None,
        },
        furniture_report: Default::default(),
        omissions: Vec::new(),
        first_hard: None,
        water: false,
    };
    let capacity = resources.pool_capacity(RenderResourceKind::Sprite);
    let capacity = i16::try_from(capacity).unwrap();
    assert!(host.texture(-1).unwrap().is_none());
    assert!(host.texture(capacity + 1).unwrap().is_none());
    assert!(matches!(
        host.texture(capacity),
        Err(SoftwareTerrainError::Resource(RenderResourceError::NotResident { request }))
            if request.slot() == capacity
    ));
    let missing = (0..capacity)
        .find(|&slot| resident.resource(3, slot as u16).is_none())
        .expect("accepted vacant H3 slot reaches native lazy loader");
    assert!(matches!(
        host.texture(missing),
        Err(SoftwareTerrainError::Resource(RenderResourceError::NotResident { request }))
            if request.slot() == missing
    ));
    for tile in &tables.water.shoreline[1..] {
        assert!(host.texture(tile.sprite_slot).unwrap().is_some());
    }
    assert_eq!(host.scene.stats.terrain_commands, 0);
    assert_eq!(host.scene.stats.water_commands, 0);
}

#[test]
fn water_draws_original_texture_pixels_through_direct_and_e150_callbacks() {
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let terrain = terrain(None);
    for (eye, z, clipped) in [([128, 256, 0], 3, false), ([128, 256, 0], 1, true)] {
        let draw = inputs(&tables, eye);
        let mut config = startup.scene;
        config.environment.view = draw.view;
        config.environment.projection = draw.context_projection;
        config.environment.coordinate_shift = draw.coordinate_shift;
        let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (config).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config,
        })
        .unwrap();
        let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
        let pending = FurnitureEffectQueue::default();
        let lights = WorldLightGrid::default();
        let mut particles = ProcessParticleRuntime::default();
        let rows = [
            TerrainRow {
                z,
                left: 0,
                right: 1,
            },
            TerrainRow {
                z: z + 1,
                left: 0,
                right: 1,
            },
        ];
        let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let mut host = SceneTerrainHost {
            scene: &mut scene,
            furniture: SoftwareFurnitureInputs {
                state: &mut state,
                pending: &pending,
                light_grid: &lights,
                particles: &mut particles,
                executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                birth_inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &birth_process,
                },
                load_factor: 4,
                crystal_flares: None,
            },
            furniture_report: Default::default(),
            omissions: Vec::new(),
            first_hard: None,
            water: false,
        };
        let outcome = host.water(strip(&terrain, draw, &rows)).unwrap();
        assert!(outcome.quads > 0, "clipped={clipped}: {outcome:?}");
        assert!(host.scene.stats.water_commands > 0);
        assert_eq!(
            host.scene.stats.water_commands,
            host.scene.stats.terrain_commands
        );
        host.scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
        if clipped {
            assert!(host.scene.pixels.stats.textured_triangles > 0);
        } else {
            assert!(host.scene.pixels.stats.d6_quads > 0);
        }
        assert!(host.scene.pixels.stats.fragments > 0, "clipped={clipped}");
        assert!(!host.scene.stopped());
        assert!(particles.pending(2).unwrap().is_empty());
    }
}

#[test]
fn water_failure_keeps_queued_prefix_and_allows_following_strip_pixels() {
    let (assets, mut tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let capacity = resources.pool_capacity(RenderResourceKind::Sprite);
    let missing = (0..capacity)
        .find(|&slot| resident.resource(3, slot as u16).is_none())
        .expect("accepted vacant H3 slot") as i16;
    // The first tile is all submerged (combo15). Its neighbour has only
    // the left corners submerged (combo3), reaching this required loader.
    tables.water.shoreline[3].sprite_slot = missing;
    let draw = inputs(&tables, [128, 256, 0]);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut config = startup.scene;
    config.environment.view = draw.view;
    config.environment.projection = draw.context_projection;
    config.environment.coordinate_shift = draw.coordinate_shift;
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config,
    })
    .unwrap();
    let terrain = terrain(Some(4));
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let mut particles = ProcessParticleRuntime::default();
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
    };
    let mut host = SceneTerrainHost {
        scene: &mut scene,
        furniture: SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            birth_inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &birth_process,
            },
            load_factor: 4,
            crystal_flares: None,
        },
        furniture_report: Default::default(),
        omissions: Vec::new(),
        first_hard: None,
        water: false,
    };
    let bad = [
        TerrainRow {
            z: 3,
            left: 0,
            right: 2,
        },
        TerrainRow {
            z: 4,
            left: 0,
            right: 2,
        },
    ];
    let error = host.water(strip(&terrain, draw, &bad)).unwrap_err();
    assert!(matches!(
        error,
        SoftwareTerrainError::Resource(RenderResourceError::NotResident { .. })
    ));
    let prefix = host.scene.stats.water_commands;
    assert!(prefix > 0);
    assert_eq!(prefix, host.scene.stats.terrain_commands);
    host.omitted(TerrainFeature::WaterStrip, TerrainDrawError::Host(error));
    assert_eq!(host.omissions.len(), 1);
    assert_eq!(host.scene.water_omissions.len(), 1);
    let good = [
        TerrainRow {
            z: 3,
            left: 0,
            right: 1,
        },
        TerrainRow {
            z: 4,
            left: 0,
            right: 1,
        },
    ];
    assert!(host.water(strip(&terrain, draw, &good)).unwrap().quads > 0);
    assert!(host.scene.stats.water_commands > prefix);
    host.scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
    assert!(host.scene.pixels.stats.fragments > 0);
    assert!(!host.scene.stopped());
}

#[test]
fn terrain_emission_retains_native_row_rng_and_allocation_before_missing_source() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_particle_emission_native.json"
    ))
    .unwrap();
    let case = receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "negative-spans")
        .unwrap();
    assert_eq!(case["spacing"], 256);
    assert_eq!(case["height_shift"], 5);
    let origin = std::array::from_fn(|axis| case["origin"][axis].as_i64().unwrap() as i32);
    let rows: Vec<_> = case["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| TerrainRow {
            z: row[0].as_i64().unwrap() as i16,
            left: row[1].as_i64().unwrap() as i16,
            right: row[2].as_i64().unwrap() as i16,
        })
        .collect();
    let first_rng = case["requests"][0]["rng_before"].as_u64().unwrap() as u32;
    // Height changes the emitted Y operand, not the native row RNG cadence.
    // The stopped constructor consumes no position data or birth RNG.
    let terrain = terrain(None);
    let (assets, tables, resident) = fixture();
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    scene.set_process_rng(Some(case["rng"].as_u64().unwrap() as u32));
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut particles = ProcessParticleRuntime::default();
    let record = particles
        .record(crate::process_particles::ParticleId::Pool(399))
        .unwrap();
    let untouched = (
        record.source_bytes::<24>(0).unwrap(),
        record.source_bytes::<28>(0x24).unwrap(),
    );
    let mut host = SceneTerrainHost {
        scene: &mut scene,
        furniture: SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            birth_inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &process,
            },
            load_factor: 65536,
            crystal_flares: None,
        },
        furniture_report: Default::default(),
        omissions: Vec::new(),
        first_hard: None,
        water: false,
    };
    let error = host
        .particles(crate::terrain_draw::TerrainParticleEmissionRequest {
            terrain: &terrain,
            context_position: origin,
            rows: &rows,
        })
        .unwrap_err();
    assert!(matches!(&error, SoftwareTerrainError::Particle(reason)
        if reason.contains("00489EB2")));
    assert_eq!(host.scene.process_rng(), Some(first_rng));
    assert_eq!(host.scene.stats.terrain_commands, 0);
    assert!(!host.scene.stopped());
    drop(host);
    assert_eq!(particles.free_count(), 399);
    assert_eq!(particles.allocation_marker(), Some(399));
    assert_eq!(particles.occupant(399), None);
    let record = particles
        .record(crate::process_particles::ParticleId::Pool(399))
        .unwrap();
    assert_eq!(
        (
            record.source_bytes::<24>(0).unwrap(),
            record.source_bytes::<28>(0x24).unwrap(),
        ),
        untouched
    );
    for category in 0..crate::process_particles::PARTICLE_LIST_COUNT {
        assert!(particles.pending(category).unwrap().is_empty());
    }
    assert_eq!(process.process_null_handle, None);
}

mod hard_custody;
