use super::*;

#[test]
fn retails_view_keeps_retails_arena_and_a_wider_one_scales_with_its_reach() {
    // Retail's tiers: a focal half the 4:3 width.
    assert_eq!(
        world_arena_bytes([640, 480], [320, 320]),
        RETAIL_ARENA_BYTES
    );
    assert_eq!(
        world_arena_bytes([1024, 768], [512, 512]),
        RETAIL_ARENA_BYTES
    );
    // 16:9 and 21:9 at the 1024x768 tier's height and focal: by width.
    assert_eq!(
        world_arena_bytes([1365, 768], [512, 512]),
        0xc8000 * 1365 / 1024
    );
    assert_eq!(
        world_arena_bytes([1792, 768], [512, 512]),
        0xc8000 * 1792 / 1024
    );
    // The Remastered camera's 100 degrees on the 4:3 tier (focal 430, the
    // user's Tabuchi run): its reach on both axes.
    let remastered = f64::from(world_arena_bytes([1024, 768], [430, 430]));
    let expected = f64::from(RETAIL_ARENA_BYTES) * (512.0_f64 / 430.0).powi(2);
    assert!(
        (remastered - expected).abs() <= 1.0,
        "{remastered} {expected}"
    );
    // A narrower view never shrinks retail's.
    assert_eq!(
        world_arena_bytes([1024, 768], [600, 600]),
        RETAIL_ARENA_BYTES
    );
}

/// The view behind the reported Tabuchi bug: from its start, driving toward
/// the water ramp, with fine terrain (Detail Level 1) and the far distance a
/// fast machine reaches. Its 16:9 frame overflows retail's arena, whose
/// terrain strip frames (0044AA10) drop the strips that do not fit, and the
/// frame draws what did. The width-sized arena holds it, as retail's holds
/// the 4:3 view of the same pose. Retail's game at 16:9 draws that 4:3 view
/// in retail's arena, as the 4:3 frame does, the view shown drawn apart in
/// its own; the remaster's draws the frame's view in the frame's arena.
#[test]
fn tabuchi_water_view_fits_the_widescreen_arena_and_overflows_retails() {
    let (assets, resident, mut world, _maps) = load_campaign_world(
        4,
        9,
        crate::terrain_runtime::TerrainSpacing::Units128,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
    );
    crate::terrain_runtime::TerrainDetailContext {
        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
            crate::terrain_runtime::TerrainSpacing::Units128,
        ),
        world_flags: Some(world.world_flags),
        terrain: Some(&mut world.terrain),
    }
    .configure(1)
    .unwrap();
    assert_eq!(world.terrain.sampling().spacing.units(), 128);
    // 0048E2E0 grows the far distance to 0x3100 on a fast machine; the
    // predraw slew stops within 64 of it.
    world.mission.dword_1c = 0x3100;
    let header = world.mission.dword_20.unwrap();
    world.fade = crate::world_fade::WorldFadeState::loaded(
        0x3100 - 64,
        header,
        world.fade.performance.clone(),
    );
    // The heaviest frame of the reproduction drive (release build, 31 ms
    // fixed step, Right after Up from Tabuchi's start).
    let eye = [-13712, -583, -29682].map(|v: i32| v as u16);
    let basis = MenuBasis {
        words: [
            0, 3192831838, 0, 1065007702, 3198811349, 1064359848, 3180080004, 3211518361,
            3199045599, 3192031099,
        ],
    };
    supply_native_camera(&mut world, eye, basis);
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut animation = AnimationClock::default();
    animation.advance(256 * 20_000);
    let mut draw = |frame: crate::presentation::NativeFrame,
                    arena_bytes: u32,
                    game: crate::presentation::GameView| {
        let resources = RenderResources::new(&resident);
        let startup = frontend_startup(&resident, frame).unwrap();
        let [width, height] = frame.size;
        let mut pixels = SoftwareFrame::new(width, height, [17, 23, 31, 255]);
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
        let mut object_light_policy =
            FixtureObjectLightPolicy::new(false, world.terrain.sampling().spacing);
        world.last_draw = None;
        let result = draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process: &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                    FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                ).unwrap(),
                attract_demo: &false,
                world: &mut world,
                animation: &animation,
                particles: &mut crate::process_particles::ProcessParticleRuntime::default(),
                light_grid: &mut crate::world_lighting::WorldLightGrid::default(),
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
                surface: [width as i32, height as i32],
                z_buffering: true,
                dynamic_lighting: false,
                arena_bytes,
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                settings_7c: 0,
                settings_70: 0,
                network_initials: [[0; 6]; crate::network::game::SLOTS],
                network_team_flag: 0,
                infrared: false,
                game,
                game_far: None,
            },
            &mut host,
        );
        (result, world.last_draw)
    };
    let four_three = crate::presentation::NativeFrame::original([1024, 768]);
    let wide = crate::presentation::NativeFrame::for_output(
        [1024, 768],
        [1920, 1080],
        crate::presentation::ClassicScaling::Widescreen,
    );
    assert_eq!(wide.size, [1365, 768]);

    use crate::presentation::GameView::{Remaster, Retail};
    let remaster = Remaster { traced: false };
    let (outcome, receipt) = draw(
        four_three,
        world_arena_bytes([1024, 768], [512, 512]),
        Retail,
    );
    let outcome = outcome.unwrap();
    assert!(!outcome.two_pass);
    // The reproduction's fade: near (far - header) / 4, far 12480; the
    // tier's focal, as the arenas here are sized.
    let receipt = receipt.unwrap();
    assert_eq!(receipt.projection.fade, [2795, 12480]);
    assert_eq!(receipt.projection.focal, [512, 512]);
    assert_eq!(outcome.terrain_omissions, 0, "{outcome:?}");
    let four_three_terrain = outcome.terrain_commands;

    // The remaster's game in retail's arena: the frame's view overflows it.
    let (overflow, receipt) = draw(wide, RETAIL_ARENA_BYTES, remaster);
    let overflow = overflow.unwrap();
    assert!(!overflow.two_pass && receipt.is_some());
    assert!(overflow.terrain_omissions > 0, "{overflow:?}");

    // Retail's: its draw is the 4:3 frame's whatever the view shown's
    // arena, which fills or holds that view.
    for (arena_bytes, holds) in [
        (RETAIL_ARENA_BYTES, false),
        (world_arena_bytes([1365, 768], [512, 512]), true),
    ] {
        let (outcome, receipt) = draw(wide, arena_bytes, Retail);
        let (outcome, receipt) = (outcome.unwrap(), receipt.unwrap());
        assert!(outcome.two_pass);
        assert_eq!(
            (
                receipt.game_outcome.terrain_commands,
                receipt.game_outcome.terrain_omissions
            ),
            (four_three_terrain, 0)
        );
        assert_eq!(receipt.game_projection.viewport, [1024, 768]);
        assert_eq!(outcome.terrain_omissions == 0, holds, "{outcome:?}");
        if holds {
            assert!(outcome.terrain_commands > overflow.terrain_commands);
            assert!(
                outcome.terrain_commands > four_three_terrain,
                "{} wide, {four_three_terrain} 4:3",
                outcome.terrain_commands
            );
        }
    }

    // Outside the terrain, 00440F00's frame takes the 0x301 from the models,
    // skips the rest of the world draw and still flushes what was queued:
    // the remaster's game's own draw, and retail's view shown, apart from
    // its game's, whose arena is retail's.
    let (small, receipt) = draw(four_three, 0x800, remaster);
    let small = small.unwrap();
    assert!(small.arena_overflow && !small.two_pass, "{small:?}");
    assert!(small.skipped_terrain && receipt.is_some());
    let (small, receipt) = draw(four_three, 0x800, Retail);
    let (small, receipt) = (small.unwrap(), receipt.unwrap());
    assert!(small.arena_overflow && small.two_pass, "{small:?}");
    assert!(small.skipped_terrain);
    assert!(!receipt.game_outcome.arena_overflow && !receipt.game_outcome.skipped_terrain);
}
