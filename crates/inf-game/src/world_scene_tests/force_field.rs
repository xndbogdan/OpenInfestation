use super::*;

/// Tabuchi's Force Field (world 9) seen from outside, on and off: the world
/// draw reaches its +2C without an omission either way, and on, its dome's
/// D6 quads reach the pixels.
#[test]
fn tabuchi_force_field_dome_reaches_the_pixels() {
    let (assets, resident, mut world, _maps) = load_campaign_world(
        4,
        9,
        crate::terrain_runtime::TerrainSpacing::Units256,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
    );
    let field = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector18(actor).is_ok())
        .expect("Tabuchi's field");
    assert!(world.arena.selector18(field).unwrap().on_176);
    // Faded in, as after its first half second of updates.
    world
        .arena
        .supply_selector18_fade(field, crate::object_runtime::FORCE_FIELD_FADE)
        .unwrap();
    let [x, y, z] = world
        .arena
        .object(field)
        .unwrap()
        .base
        .position_words
        .unwrap();
    // 4000 short of its centre along Z, looking along +Z, above its top.
    supply_native_camera(
        &mut world,
        [x, y.wrapping_add(900), z.wrapping_sub(4000)],
        MenuBasis::INITIAL,
    );
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let camera_inputs = WorldCameraFixture::new(&assets);
    let mut furniture = WorldFurnitureFixture::new(&assets, &world);
    let mut animation = AnimationClock::default();
    animation.advance(256 * 20_000);
    let mut draw = |world: &mut WorldSession| {
        let frame = crate::presentation::NativeFrame::original([1024, 768]);
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
        let outcome = draw_world_frame(
            WorldDrawRequest {
                camera_clearance: None,
                terrain_event_5483b0: 0,
                map: None,
                pickup_text_data: None,
                weapon_tables: None,
                object_draw_process:
                    &mut crate::object_runtime::ObjectDrawProcessState::native_startup(
                        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                    )
                    .unwrap(),
                attract_demo: &false,
                world,
                animation: &animation,
                particles: &mut crate::process_particles::ProcessParticleRuntime::default(),
                light_grid: &mut crate::world_lighting::WorldLightGrid::default(),
                object_light_policy: &mut object_light_policy,
                object_light_failure_policy:
                    crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
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
                arena_bytes: world_arena_bytes(
                    [width as i32, height as i32],
                    startup.projection.focal,
                ),
                mirror_copy: crate::model_vertices::MirrorCopySource::Partner,
                hovercraft_fans: false,
                guided_mounts: false,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
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
        drop(host);
        let stats = [scene.pixels.stats.d6_quads, scene.pixels.stats.unsupported];
        drop(scene);
        (outcome, stats)
    };
    let (on, on_pixels) = draw(&mut world);
    world.arena.switch_selector18(field, false, 0).unwrap();
    let (off, off_pixels) = draw(&mut world);
    assert_eq!(on.skips.object_draw, off.skips.object_draw, "{on:?}");
    assert_eq!(on_pixels[1], 0, "{on_pixels:?}");
    assert!(
        on_pixels[0] > off_pixels[0],
        "on {on_pixels:?}, off {off_pixels:?}"
    );
}
