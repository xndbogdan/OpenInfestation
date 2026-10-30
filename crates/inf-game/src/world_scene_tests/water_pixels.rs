use super::*;

fn load_ordinary_water_world(
    tier: u8,
) -> (
    FrontendAssets,
    ResidentRuntime,
    WorldSession,
    ProcessWorldMapRuntime,
) {
    // Original campaign selection18 selects level1 OVL leaf28: ordinary water,
    // with authored H14 flag40 clear. Separate briefing cases cover waves.
    load_campaign_world(
        tier,
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
        crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
    )
}

#[test]
fn native_world_draw_includes_authored_ordinary_water_pixels_and_retained_receipt() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, resident, mut world, _maps) = load_ordinary_water_world(tier);
        assert_eq!(world.world_flags & 0x40, 0);
        let plane = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
        let step = usize::from(world.terrain.sampling().spacing.units() / 128);
        let (_, x, z) = (step..512 - step)
            .step_by(step)
            .flat_map(|x| {
                let terrain = &world.terrain;
                (step..512 - step).step_by(step).filter_map(move |z| {
                    let highest = [[x, z], [x + step, z], [x, z + step], [x + step, z + step]]
                        .map(|[x, z]| terrain.raw_height_at([(x * 128) as u16, (z * 128) as u16]))
                        .into_iter()
                        .max()
                        .unwrap();
                    (i32::from(highest) < i32::from(plane) - 128).then_some((highest, x, z))
                })
            })
            .min()
            .expect("actual submerged H12 tile");
        let focus = [
            i32::from(((x * 128 + step * 64) as u16) as i16),
            i32::from(plane),
            i32::from(((z * 128 + step * 64) as u16) as i16),
        ];
        // Supplied camera framing exercises the production path; it does not
        // assert a native cinematic timeline or change runtime camera policy.
        let eye = [focus[0], focus[1] + 512, focus[2] - 2048];
        let basis = camera_basis(eye, focus);
        supply_native_camera(&mut world, eye.map(|v| v as u16), basis);
        let resources = RenderResources::new(&resident);
        let startup = frontend_startup(
            &resident,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let tables = PrimitiveTables::from_executable(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
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
        scene.set_terrain(Some(world.terrain.clone()));
        let mut host = ResidentSceneHost {
            resources,
            backend: &mut scene,
        };
        let camera_inputs = WorldCameraFixture::new(&assets);
        let mut furniture = WorldFurnitureFixture::new(&assets, &world);
        let mut particles = crate::process_particles::ProcessParticleRuntime::default();
        let mut lights = crate::world_lighting::WorldLightGrid::default();
        let mut animation = AnimationClock::default();
        animation.advance(256 * 20_000);
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
        let outcome = draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                    inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                ).unwrap(),
                attract_demo: &false,
                world: &mut world,
                animation: &animation,
                particles: &mut particles,
                light_grid: &mut lights,
                object_light_policy: &mut object_light_policy,
                object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                camera: camera_inputs.inputs(&resident, animation.value()),
                furniture: furniture.inputs(),
                sound: None,
            },
            WorldFrameInputs {
                template_focal: startup.projection.focal,
                focal_src: startup.projection.focal,
                focal_kick: 1.0,
                scale_5c0: 0,
                special_view_active: false,
                surface: dimensions.map(|v| v as i32),
                z_buffering: true,
                dynamic_lighting: false,
                arena_bytes: RETAIL_ARENA_BYTES,
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                settings_7c: 0,
                settings_70: 0,
                network_initials: [[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                infrared: false,
                game: crate::presentation::GameView::Retail,
                game_far: None,
            },
            &mut host,
        )
        .unwrap();
        assert!(outcome.water_commands > 0, "tier{tier}: {outcome:?}");
        assert!(outcome.terrain_commands >= outcome.water_commands);
        assert!(!outcome.skipped_terrain);
        assert!(host.backend.pixels.stats.fragments > 0);
        assert!(
            host.backend.pixels.stats.d6_quads > 0
                || host.backend.pixels.stats.textured_triangles > 0
        );
        let receipt = world.last_draw.unwrap();
        assert!(receipt.native_camera);
        assert_eq!(receipt.eye, eye.map(|v| i32::from(v as i16)));
        assert_eq!(receipt.basis_words, basis.words);
        assert_eq!(receipt.projection.viewport, dimensions.map(|v| v as i32));
        assert_eq!(receipt.outcome, outcome);
        assert_eq!(
            receipt.outcome.water_commands,
            host.backend.stats.water_commands
        );
        assert_eq!(
            receipt.terrain_height_at_eye,
            world.terrain.height_at([eye[0] as u16, eye[2] as u16])
        );
    }
}
