//! Shared 00442080 focal consumers over genuine loaded intro and mission worlds.
//! Supplied focal history/camera pose isolate the native view-mode branch;
//! these frames do not assert a complete camera timeline or retail pixels.

use super::*;

#[test]
fn intro_and_mission_draws_consume_current_special_view_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    for selection in [22, 18] {
        let (residency, mut world, _maps) = match selection {
            22 => {
                let (residency, world, maps, _map_load) = load_first_world();
                (residency, world, maps)
            }
            18 => load_level1_session(),
            _ => unreachable!(),
        };
        assert_eq!(world.mission.save.mission_selection, selection);
        supply_native_camera(&mut world, [0, 1000, 0], MenuBasis::INITIAL);
        let resources = RenderResources::new(&residency);
        let startup = frontend_startup(
            &residency,
            crate::presentation::NativeFrame::original([640, 480]),
        )
        .unwrap();
        let source = startup.projection.focal;
        let mut object_draw_process =
            crate::object_runtime::ObjectDrawProcessState::native_startup(executable).unwrap();
        let camera_inputs = WorldCameraFixture::new(&assets);
        let mut furniture = WorldFurnitureFixture::new(&assets, &world);
        let mut particles = crate::process_particles::ProcessParticleRuntime::default();
        let mut lights = crate::world_lighting::WorldLightGrid::default();
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
        // Distinct retained histories exercise decay, quarter easing and the
        // native decay overshoot (which corrects to source on the next draw).
        // Every draw reuses this actual world without reconstructing its camera.
        // Port option: then the view shown's own slew toward the remastered
        // camera's wider focal, apart from the game's toward retail's: into a
        // special view (the scope), whose focal is retail's, it eases there
        // as the game's would, and out of it it falls back by 0x50 a frame.
        // 100 degrees across for retail's 90 (tan 45 / tan 50).
        // The view shown's focal after each draw, worked by hand from 640x480's
        // 320: down by 0x50 a frame, or to the source when at or below it; in
        // a special view a quarter of the way to it, rounded toward the
        // larger (an arithmetic shift).
        assert_eq!(source, [320, 320]);
        let wide = source.map(|focal| focal * 84 / 100);
        assert_eq!(wide, [268, 268]);
        for (
            special_view_active,
            history_delta,
            expected_delta,
            focal_kick,
            shown_src,
            own,
            shown,
        ) in [
            (
                false,
                [0x100, 0x100],
                [0xb0, 0xb0],
                1.0,
                source,
                false,
                [496, 496],
            ),
            (
                true,
                [0x100, 0x100],
                [0xc0, 0xc0],
                1.0,
                source,
                false,
                [512, 512],
            ),
            (
                false,
                [0x10, -0x10],
                [-0x40, 0],
                1.0,
                source,
                false,
                [256, 320],
            ),
            // Port option, the remastered camera's speed kick: the view drawn
            // widens past the slew, whose history keeps the source.
            (false, [0, 0], [0, 0], 0.8, source, false, [320, 320]),
            // 320 - 0x50 overshoots the wide 268.
            (false, [0, 0], [0, 0], 1.0, wide, true, [240, 240]),
            // Into the scope: 240 + 80/4, then 260 + 60/4.
            (true, [0, 0], [0, 0], 1.0, source, true, [260, 260]),
            (true, [0, 0], [0, 0], 1.0, source, true, [275, 275]),
            // Out of it: 275 - 0x50 overshoots, then the wide focal.
            (false, [0, 0], [0, 0], 1.0, wide, true, [195, 195]),
            (false, [0, 0], [0, 0], 1.0, wide, true, [268, 268]),
        ] {
            world.view_focals = [source[0] + history_delta[0], source[1] + history_delta[1]];
            // The view shown follows the game's history, or keeps its own.
            if !own {
                world.shown_focals = world.view_focals;
            }
            let mut frame = SoftwareFrame::new(640, 480, [16, 24, 64, 255]);
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                    startup.scene.environment,
                ),
                resources,
                tables: &tables,
                frame: &mut frame,
                config: startup.scene,
            })
            .unwrap();
            scene.set_terrain(Some(world.terrain.clone()));
            let mut host = ResidentSceneHost {
                resources,
                backend: &mut scene,
            };
            let outcome = draw_world_frame(
                WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                    weapon_tables: None,
                    world: &mut world,
                    animation: &AnimationClock::default(),
                    object_draw_process: &mut object_draw_process,
                    attract_demo: &false,
                    particles: &mut particles,
                    light_grid: &mut lights,
                    executable,
                    object_light_policy: &mut object_light_policy,
                    object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
                    camera: camera_inputs.inputs(&residency, 0),
                    furniture: furniture.inputs(),
                    sound: None,
                },
                WorldFrameInputs {
                    template_focal: source,
                    focal_src: shown_src,
                    focal_kick,
                    scale_5c0: 0,
                    special_view_active,
                    surface: [640, 480],
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
            let expected = [source[0] + expected_delta[0], source[1] + expected_delta[1]];
            assert_eq!(
                outcome.focals,
                shown.map(|focal| (focal as f32 * focal_kick).round() as i32),
                "selection{selection} special={special_view_active}"
            );
            assert_eq!(world.view_focals, expected);
            assert_eq!(world.shown_focals, shown);
            // Two passes where the view shown is not the game's, or sees past
            // retail's cone (a focal below the source, as a decay that
            // overshoots leaves it), which the game's draw keeps.
            let wider = (0..2).any(|axis| shown[axis] < source[axis]);
            assert_eq!(
                outcome.two_pass,
                shown != expected || focal_kick != 1.0 || wider,
                "selection{selection} shown {shown:?}"
            );
            let receipt = world.last_draw.unwrap();
            assert_eq!(receipt.game_projection.focal, expected);
        }
        // Back out of the special view, past the decay's overshoot.
        assert_eq!(world.shown_focals, wide);
    }
}
