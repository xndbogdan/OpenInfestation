//! The view shown drawn apart from the game's draw, and the tracer's draws
//! outside its view: whatever they are, the game plays as one pass of
//! retail's view plays it.

use super::*;

/// A sound host that plays every one-shot it is asked for, and keeps them.
#[derive(Default)]
struct Audio {
    calls: Vec<String>,
}

impl crate::process_sound::ProcessSoundVoiceTeardown for Audio {
    fn delete_voice(
        &mut self,
        record: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        crate::process_sound::ProcessSoundVoiceTeardown::delete_voice(
            &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            record,
            voice,
        )
    }
}

impl crate::process_sound::ProcessSoundAudio for Audio {
    type Error = crate::platform_audio::PlatformAudioError;
    fn operation(
        &mut self,
        request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
    ) -> Result<
        crate::process_sound::ProcessSoundVoiceReply,
        crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
    > {
        use crate::process_sound::{
            ProcessSoundVoiceOperation as Operation, ProcessSoundVoiceReply as Reply,
            ProcessSoundVoiceRequest as Request, ProcessSoundVoiceServiceFault as Fault,
        };
        let Request::OneShot {
            resource,
            parameters,
            effects_volume,
            rng,
        } = request
        else {
            return Err(Fault::Required(match request {
                Request::Create { .. } => Operation::Create,
                Request::Start { .. } => Operation::Start,
                Request::Update { .. } => Operation::Update,
                Request::OneShot { .. } => unreachable!(),
            }));
        };
        self.calls.push(format!(
            "{} {parameters:?} {effects_volume} {rng:?}",
            resource.requested_slot()
        ));
        Ok(Reply::OneShot(true))
    }
}

/// The owners one world's frames lend the world draw.
struct Owners {
    process: crate::object_runtime::ObjectDrawProcessState,
    particles: crate::process_particles::ProcessParticleRuntime,
    light_grid: crate::world_lighting::WorldLightGrid,
    furniture: WorldFurnitureFixture,
    camera: WorldCameraFixture,
    sounds: crate::process_sound::ProcessSoundRuntime,
    tables: crate::object_runtime::ProcessActorBehaviorTables,
    map: crate::world_map::WorldMapState,
    policy: FixtureObjectLightPolicy,
    audio: Audio,
}

impl Owners {
    fn new(assets: &FrontendAssets, world: &WorldSession) -> Self {
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        Self {
            process: crate::object_runtime::ObjectDrawProcessState::native_startup(executable)
                .unwrap(),
            particles: Default::default(),
            light_grid: Default::default(),
            furniture: WorldFurnitureFixture::new(assets, world),
            camera: WorldCameraFixture::new(assets),
            sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
            tables: crate::object_runtime::ProcessActorBehaviorTables::from_executable(executable)
                .unwrap(),
            map: crate::world_map::WorldMapState::new(),
            policy: FixtureObjectLightPolicy::new(true, world.terrain.sampling().spacing),
            audio: Audio::default(),
        }
    }

    /// One world frame from the views `frame` makes, the view shown then
    /// changed by `shown`.
    #[allow(clippy::too_many_arguments)]
    fn draw<'r>(
        &mut self,
        world: &mut WorldSession,
        resident: &ResidentRuntime,
        executable: FrontendExecutable<'_>,
        animation: &AnimationClock,
        frame: WorldFrameInputs,
        shown: impl FnOnce(&mut ViewSpec),
        host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
    ) -> Result<WorldDrawOutcome, WorldDrawError> {
        let mut request = WorldDrawRequest {
            camera_clearance: None,
            terrain_event_5483b0: 0,
            map: Some(&mut self.map),
            pickup_text_data: None,
            weapon_tables: Some(&mut self.tables),
            world,
            animation,
            object_draw_process: &mut self.process,
            attract_demo: &false,
            particles: &mut self.particles,
            light_grid: &mut self.light_grid,
            executable,
            object_light_policy: &mut self.policy,
            object_light_failure_policy:
                crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            camera: self.camera.inputs(resident, animation.value()),
            furniture: self.furniture.inputs(),
            sound: Some(WorldDrawSound {
                queue: &mut self.sounds,
                sink: Some(&mut self.audio),
                effects_volume: 15,
                phase: 4,
            }),
        };
        let views = world_views(&mut request, &frame, host).map(|[game, mut view]| {
            shown(&mut view);
            [game, view]
        });
        draw_world_views(request, frame, views, host)
    }

    /// What the view shown's draw may not change, as text: the world's RNG,
    /// arena, tint, mission save and statistics, world start, listeners,
    /// flash, light delta, the ground's furniture and plane, the game's
    /// focal and fade slews, the particles, light grid, furniture bases,
    /// sounds, weapon tables, map and object process state, the one-shots
    /// played, and the scene's process globals (RNG, environment, primitive
    /// state, furniture registers, tint, published graphics, the textures'
    /// sort ordinals) and model scratch, its bytes and all. The two worlds'
    /// loads number their arenas and allocations apart, so those numbers
    /// are left out; the long ones are kept as their digests.
    fn game_state(&self, world: &WorldSession, scene: &SoftwareScene<'_, '_>) -> Vec<String> {
        let scratch: Vec<Option<u8>> = (0..crate::model_scratch::MODEL_SCRATCH_BYTES)
            .step_by(4096)
            .flat_map(|at| scene.scratch.read_pool::<4096>(at).unwrap())
            .collect();
        let mut parts = vec![
            format!("{:08x}", world.rng),
            format!("{:?}", world.arena),
            format!("{:08x}", world.presentation.tint_word),
            format!("{:?}", world.mission.save),
            format!("{:?}", world.mission.record_0c),
            format!("{:?}", world.world_start),
            format!("{:?}", world.listeners),
            format!("{:?}", world.flash),
            format!("{}", world.light_delta),
            format!(
                "{:?} {}",
                world.terrain.buckets(),
                world.terrain.plane_base()
            ),
            format!(
                "{:?} {:?}",
                world.view_focals,
                world.fade.retained_predraw_words()
            ),
            format!("{:?}", self.particles),
            format!(
                "{:?} {:?}",
                self.light_grid.anchors(),
                self.light_grid.cells()
            ),
            format!("{:?}", self.furniture.state),
            format!("{:?}", self.sounds),
            format!("{:?}", self.tables),
            format!("{:?}", self.map),
            // Its profiling's slots time the draw by the wall clock.
            format!(
                "{} {:?} {:?}",
                self.process.completion_53b890,
                self.process.shared_basis,
                self.process.pickup_basis
            ),
            format!("{:?}", self.audio.calls),
            format!("{:?}", scene.scratch),
            format!("{scratch:?}"),
        ];
        parts.extend(scene.globals_text());
        parts
            .into_iter()
            .map(|part| {
                let numbered = [
                    "identity: ",
                    "serial: ",
                    "arena: ",
                    "CreationListenerId { list: ",
                    "sound_process: ",
                ];
                let mut numbers: Vec<usize> = numbered
                    .iter()
                    .flat_map(|name| part.match_indices(name).map(|(at, _)| at + name.len()))
                    .collect();
                numbers.sort_unstable();
                let mut kept = String::with_capacity(part.len());
                let mut from = 0;
                for at in numbers {
                    kept.push_str(&part[from..at]);
                    from = at + part[at..].bytes().take_while(u8::is_ascii_digit).count();
                }
                kept.push_str(&part[from..]);
                if kept.len() <= 300 {
                    return kept;
                }
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::digest(kept.as_bytes()))
            })
            .collect()
    }
}

/// Tabuchi as loaded, twice, with its objects' lights (the terrain's
/// dynamic lighting off) and a sound host, a
/// one-shot queued each frame at the first object. One plays retail's game
/// at the Original camera on 4:3, one pass a frame. The other plays it with
/// a view shown unlike retail's: the Remastered camera's 100 degrees on a
/// 16:9 frame, its arena widened, from an eye moving ahead past an actor
/// only it sees at first (the first object, 2000 ahead and 2500 to the
/// right); on the way the tracer's longer reach with the look under the
/// water, Vulkan's murk under it, infrared vision, and a network game's
/// initials and team; then, toward the water ramp (the arena test's view),
/// an arena that fills at the objects (0x1000 bytes: the 16:9 frame's sky
/// alone fills 0x800), and one that cannot be made, which fails the draw.
/// After every frame the game is the same in both, though the view shown
/// drew more (its picture's models more than the game's).
#[test]
fn the_view_shown_draws_apart_and_leaves_retails_game_as_one_pass_leaves_it() {
    let load = || {
        load_campaign_world(
            4,
            9,
            crate::terrain_runtime::TerrainSpacing::Units256,
            crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        )
    };
    let (assets, resident_a, mut world_a, _maps_a) = load();
    let (_, resident_b, mut world_b, _maps_b) = load();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let ramp = [-13712, -583, -29682].map(|v: i32| v as u16);
    let basis = MenuBasis {
        words: [
            0, 3192831838, 0, 1065007702, 3198811349, 1064359848, 3180080004, 3211518361,
            3199045599, 3192031099,
        ],
    };
    let [right, forward] =
        [1, 7].map(|at| [0, 1, 2].map(|axis| f32::from_bits(basis.words[at + axis])));
    let side = world_a
        .arena
        .object(
            world_a
                .arena
                .list(crate::object_runtime::ObjectList::Flag0Primary)[0]
                .object,
        )
        .unwrap()
        .base
        .clone();
    let beside = [0, 1, 2].map(|axis| {
        let at = side.position_words.unwrap()[axis] as i16;
        (f32::from(at) - 2000.0 * forward[axis] - 2500.0 * right[axis]).round() as i32 as u16
    });
    let four_three = crate::presentation::NativeFrame::original([1024, 768]);
    let wide = crate::presentation::NativeFrame::for_output(
        [1024, 768],
        [1920, 1080],
        crate::presentation::ClassicScaling::Widescreen,
    );
    let resources_a = RenderResources::new(&resident_a);
    let resources_b = RenderResources::new(&resident_b);
    let startup_a = frontend_startup(&resident_a, four_three).unwrap();
    let startup_b = frontend_startup(&resident_b, wide).unwrap();
    let mut pixels_a = SoftwareFrame::new(1024, 768, [17, 23, 31, 255]);
    let mut pixels_b = SoftwareFrame::new(1365, 768, [17, 23, 31, 255]);
    let mut scene_a = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup_a.scene.environment,
        ),
        resources: resources_a,
        tables: &tables,
        frame: &mut pixels_a,
        config: startup_a.scene,
    })
    .unwrap();
    let mut scene_b = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup_b.scene.environment,
        ),
        resources: resources_b,
        tables: &tables,
        frame: &mut pixels_b,
        config: startup_b.scene,
    })
    .unwrap();
    scene_a.set_terrain(Some(world_a.terrain.clone()));
    scene_b.set_terrain(Some(world_b.terrain.clone()));
    let mut owners_a = Owners::new(&assets, &world_a);
    let mut owners_b = Owners::new(&assets, &world_b);
    let template = startup_a.projection.focal;
    assert_eq!(
        (template, startup_b.projection.focal),
        ([512, 512], template)
    );
    let inputs = |surface: [i32; 2], focal_src: [i32; 2], arena_bytes: u32| WorldFrameInputs {
        template_focal: template,
        focal_src,
        focal_kick: 1.0,
        scale_5c0: 0,
        special_view_active: false,
        surface,
        z_buffering: true,
        dynamic_lighting: false,
        arena_bytes,
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
    };
    let widened = world_arena_bytes([1365, 768], [430, 430]);
    let mut animation = AnimationClock::default();
    animation.advance(256 * 20_000);
    #[derive(Clone, Copy, PartialEq)]
    enum Shown {
        Plain,
        Deeper,
        Murk,
    }
    let sound = crate::process_particles::audio::ParticleSoundRequest {
        sound: 6,
        position: side.position_words.unwrap(),
        volume: 65536,
        pitch: 65536,
        flags: 0,
    };
    let moving = 10;
    let mut cases: Vec<_> = (0..moving)
        .map(|step| {
            let shown = match step {
                2 | 3 => Shown::Deeper,
                4 => Shown::Murk,
                _ => Shown::Plain,
            };
            (widened, shown, (5..7).contains(&step), step == 7)
        })
        .collect();
    cases.extend([
        (0x1000, Shown::Plain, false, false),
        (u32::MAX, Shown::Plain, false, false),
    ]);
    let mut sounds_played = 0;
    for (frame, (arena_bytes, look, infrared, network)) in cases.into_iter().enumerate() {
        let eye = if frame < moving {
            let ahead = 150.0 * frame as f32;
            [0, 1, 2].map(|axis| {
                (f32::from(beside[axis] as i16) + ahead * forward[axis]).round() as i32 as u16
            })
        } else {
            ramp
        };
        supply_native_camera(&mut world_a, eye, basis);
        supply_native_camera(&mut world_b, eye, basis);
        owners_a.sounds.enqueue(&resident_a, sound).unwrap();
        owners_b.sounds.enqueue(&resident_b, sound).unwrap();
        let inputs = |surface: [i32; 2], focal_src: [i32; 2], arena_bytes: u32| {
            let mut inputs = inputs(surface, focal_src, arena_bytes);
            inputs.infrared = infrared;
            if network {
                inputs.network_initials = [*b"AB\0CD\0"; crate::network::game::SLOTS];
                inputs.network_team_flag = 1;
            }
            inputs
        };
        let one = owners_a
            .draw(
                &mut world_a,
                &resident_a,
                executable,
                &animation,
                inputs([1024, 768], template, RETAIL_ARENA_BYTES),
                |_| {},
                &mut ResidentSceneHost {
                    resources: resources_a,
                    backend: &mut scene_a,
                },
            )
            .unwrap();
        assert!(!one.two_pass);
        let two = owners_b.draw(
            &mut world_b,
            &resident_b,
            executable,
            &animation,
            inputs([1365, 768], [430, 430], arena_bytes),
            |view| match look {
                Shown::Plain => {}
                Shown::Deeper => {
                    view.fade[1] = view.fade[1] * 115 / 100;
                    view.under_water = true;
                    view.look = UnderwaterLook::Wet;
                }
                Shown::Murk => {
                    view.under_water = true;
                    view.look = UnderwaterLook::Murk;
                }
            },
            &mut ResidentSceneHost {
                resources: resources_b,
                backend: &mut scene_b,
            },
        );
        let (retail, receipt) = (world_a.last_draw.unwrap(), world_b.last_draw.unwrap());
        assert_eq!(
            (receipt.game_eye, receipt.game_projection),
            (retail.eye, retail.projection),
            "frame {frame}"
        );
        assert_eq!(
            one.sounds_started,
            owners_a.audio.calls.len() - sounds_played
        );
        sounds_played = owners_a.audio.calls.len();
        match frame.checked_sub(moving) {
            Some(1) => assert!(two.is_err(), "{two:?}"),
            _ => {
                let two = two.unwrap_or_else(|error| panic!("frame {frame}: {error:?}"));
                assert!(two.two_pass && receipt.game_outcome.two_pass);
                assert_eq!(receipt.game_outcome.submitted_models, one.submitted_models);
                assert_eq!(receipt.projection.focal, [430, 430]);
                assert_eq!(two.arena_overflow, frame == moving, "{two:?}");
                if frame == 0 {
                    assert!(
                        two.submitted_models > one.submitted_models,
                        "{} shown, {} retail's",
                        two.submitted_models,
                        one.submitted_models
                    );
                }
            }
        }
        let [a, b] = [
            owners_a.game_state(&world_a, &scene_a),
            owners_b.game_state(&world_b, &scene_b),
        ];
        let changed: Vec<_> = a
            .iter()
            .zip(&b)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(item, (_, b))| (item, &b[..b.len().min(300)]))
            .collect();
        assert!(changed.is_empty(), "frame {frame} changed: {changed:?}");
        animation.advance(33_000);
    }
    // The sound host was asked: the game's draw served the queue.
    assert!(sounds_played > 0);
}

/// The tracer's models outside its view and the craft round a first-person
/// eye, drawn for its feed alone on copies of the game, leave the game and
/// the picture as they were. Tabuchi twice, both collecting their feed as a
/// traced frame does ([`crate::software_scene::FEED_UNTRACED`]), one taking
/// nothing from outside its view, both from an eye 600 ahead of the first
/// object and looking away from it, so it lies behind the eye and near; on
/// the fourth frame from 500 behind the first player's craft, the buggy, and
/// from the fifth with the craft in first person, the eye in it. After every
/// frame both games and both pictures are the same, though the second drew
/// outside its view (the craft too in first person): its feed holds their
/// faces (the object's, then the craft's, more of them than from behind),
/// and none of their wheels or exhausts.
#[test]
fn the_tracer_draws_outside_the_view_and_leaves_the_game_as_it_found_it() {
    let load = || {
        load_campaign_world(
            4,
            9,
            crate::terrain_runtime::TerrainSpacing::Units256,
            crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        )
    };
    let (assets, resident_a, mut world_a, _maps_a) = load();
    let (_, resident_b, mut world_b, _maps_b) = load();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let basis = MenuBasis {
        words: [
            0, 3192831838, 0, 1065007702, 3198811349, 1064359848, 3180080004, 3211518361,
            3199045599, 3192031099,
        ],
    };
    let forward = [0, 1, 2].map(|axis| f32::from_bits(basis.words[7 + axis]));
    let first = world_a
        .arena
        .list(crate::object_runtime::ObjectList::Flag0Primary)[0]
        .object;
    let behind = world_a
        .arena
        .object(first)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let eye = [0, 1, 2]
        .map(|axis| (f32::from(behind[axis] as i16) + 600.0 * forward[axis]).round() as i32 as u16);
    let frame = crate::presentation::NativeFrame::original([1024, 768]);
    let resources_a = RenderResources::new(&resident_a);
    let resources_b = RenderResources::new(&resident_b);
    let startup_a = frontend_startup(&resident_a, frame).unwrap();
    let startup_b = frontend_startup(&resident_b, frame).unwrap();
    let mut pixels_a = SoftwareFrame::new(1024, 768, [17, 23, 31, 255]);
    let mut pixels_b = SoftwareFrame::new(1024, 768, [17, 23, 31, 255]);
    let mut scene_a = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup_a.scene.environment,
        ),
        resources: resources_a,
        tables: &tables,
        frame: &mut pixels_a,
        config: startup_a.scene,
    })
    .unwrap();
    let mut scene_b = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup_b.scene.environment,
        ),
        resources: resources_b,
        tables: &tables,
        frame: &mut pixels_b,
        config: startup_b.scene,
    })
    .unwrap();
    scene_a.set_terrain(Some(world_a.terrain.clone()));
    scene_b.set_terrain(Some(world_b.terrain.clone()));
    let mut owners_a = Owners::new(&assets, &world_a);
    let mut owners_b = Owners::new(&assets, &world_b);
    let template = startup_a.projection.focal;
    let inputs = WorldFrameInputs {
        template_focal: template,
        focal_src: template,
        focal_kick: 1.0,
        scale_5c0: 0,
        special_view_active: false,
        surface: [1024, 768],
        z_buffering: true,
        dynamic_lighting: false,
        arena_bytes: RETAIL_ARENA_BYTES,
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
    };
    let mut animation = AnimationClock::default();
    animation.advance(256 * 20_000);
    let mut drawn = Vec::new();
    let mut faces = Vec::new();
    crate::software_scene::FEED_UNTRACED.set(true);
    let mut eye = eye;
    for frame in 0..6 {
        let craft_at = |world: &WorldSession| {
            let craft = world.arena.first_player_drive().unwrap().0;
            let at = world
                .arena
                .object(craft)
                .unwrap()
                .base
                .position_words
                .unwrap();
            at.map(|word| f32::from(word as i16))
        };
        if frame == 3 {
            // The craft as the buggy (a loaded world's craft has no form
            // yet, and draws nothing), the eye 500 behind it.
            for world in [&mut world_a, &mut world_b] {
                let craft = world.arena.first_player_drive().unwrap().0;
                let state = world.arena.selector2_actor_mut_for_tests(craft).unwrap();
                state.dwords_1e8_1f4[0] = 1;
            }
            let at = craft_at(&world_a);
            eye = [0, 1, 2].map(|axis| (at[axis] - 500.0 * forward[axis]).round() as i32 as u16);
        }
        if frame == 4 {
            for world in [&mut world_a, &mut world_b] {
                let craft = world.arena.first_player_drive().unwrap().0;
                assert!(world.arena.apply_actor_view_mode(craft, 1).unwrap());
                assert_eq!(world.arena.first_person_craft(), Some(craft));
            }
            // The eye in the craft, as a first-person eye is.
            eye = craft_at(&world_a).map(|v| v as i32 as u16);
        }
        supply_native_camera(&mut world_a, eye, basis);
        supply_native_camera(&mut world_b, eye, basis);
        NOTHING_OUTSIDE.set(true);
        let inside = owners_a.draw(
            &mut world_a,
            &resident_a,
            executable,
            &animation,
            inputs,
            |_| {},
            &mut ResidentSceneHost {
                resources: resources_a,
                backend: &mut scene_a,
            },
        );
        NOTHING_OUTSIDE.set(false);
        let (_, [none, wheels, exhausts]) = crate::software_scene::FEED_ENDED.take();
        assert_eq!(none, 0, "frame {frame}: nothing outside the view");
        let outside = owners_b
            .draw(
                &mut world_b,
                &resident_b,
                executable,
                &animation,
                inputs,
                |_| {},
                &mut ResidentSceneHost {
                    resources: resources_b,
                    backend: &mut scene_b,
                },
            )
            .unwrap();
        let inside = inside.unwrap();
        assert_eq!(inside.outside, 0);
        assert_eq!(outside.submitted_models, inside.submitted_models);
        drawn.push(outside.outside);
        let (objects, [culled, wheels_b, exhausts_b]) = crate::software_scene::FEED_ENDED.take();
        faces.push(culled);
        // World b's own ids, its arena numbered apart from world a's: the
        // object behind the eye, then the craft, in view from behind, and
        // round the eye in first person.
        let object = world_b
            .arena
            .list(crate::object_runtime::ObjectList::Flag0Primary)[0]
            .object;
        let craft = world_b.arena.first_player_drive().unwrap().0;
        match frame {
            0..3 => assert!(objects.contains(&object), "frame {frame}: {objects:?}"),
            3 => assert!(!objects.contains(&craft), "frame {frame}: {objects:?}"),
            _ => assert!(objects.contains(&craft), "frame {frame}: {objects:?}"),
        }
        // The wheels and exhausts the view laid alone (the craft's wakes and
        // trails as retail's draw leaves them, none of the hull's).
        assert_eq!(
            [wheels_b, exhausts_b],
            [wheels, exhausts],
            "frame {frame}: the wheels and exhausts"
        );
        let [a, b] = [
            owners_a.game_state(&world_a, &scene_a),
            owners_b.game_state(&world_b, &scene_b),
        ];
        let changed: Vec<_> = a
            .iter()
            .zip(&b)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(item, (_, b))| (item, &b[..b.len().min(300)]))
            .collect();
        assert!(changed.is_empty(), "frame {frame} changed: {changed:?}");
        assert!(
            scene_a.pixels.frame.rgba() == scene_b.pixels.frame.rgba(),
            "frame {frame}'s picture"
        );
        animation.advance(33_000);
    }
    crate::software_scene::FEED_UNTRACED.set(false);
    // The object behind the eye, and in first person the craft too, drawn
    // and their faces in the feed (a stalled draw lays none): more of them
    // in first person than from behind the craft.
    assert!(drawn[..3].iter().all(|&count| count > 0), "{drawn:?}");
    assert!(
        drawn[4..].iter().all(|&count| count > drawn[3]),
        "{drawn:?}"
    );
    assert!(faces[..3].iter().all(|&count| count > 0), "{faces:?}");
    assert!(
        faces[4..].iter().all(|&count| count > faces[3]),
        "{faces:?}"
    );
}

/// The models outside the view the tracer keeps ([`reaching_view`]), the
/// ground 200 under the eye: one near the eye whatever the sun; one behind
/// it beyond that, only where a low sun behind the eye throws its shadow
/// ahead into the view; one beside the view, only where a low sun beyond it
/// throws its shadow in; one above the view, only where the water's mirror
/// shows it.
#[test]
fn the_tracer_keeps_the_models_outside_the_view_that_reach_it() {
    let camera = crate::scene_feed::SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [512.0, 512.0],
        // A view looking down: most of the frame below the horizon.
        centre: [512.0, 100.0],
        native: [1024, 768],
        near: 64.0,
        far: 8000.0,
    };
    let mut arena = crate::object_runtime::ObjectArena::default();
    let kept = |arena: &mut crate::object_runtime::ObjectArena,
                offset: [i32; 3],
                sun: [f32; 3],
                water: Option<f32>| {
        let object = crate::object_runtime::CulledObject {
            id: arena.allocate(crate::object_runtime::ObjectClassKind::Base),
            offset,
            extent: 100,
        };
        let length = sun.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
        let sun = sun.map(|axis| axis / length);
        reaching_view(&[object], &camera, sun, water, |_| -200.0) == [object.id]
    };
    let night = [0.0, -1.0, 0.0];
    assert!(kept(&mut arena, [0, 0, -500], night, None));
    let behind = [0, -150, -1300];
    assert!(kept(&mut arena, behind, [0.0, 0.1, -1.0], None));
    assert!(!kept(&mut arena, behind, [0.0, 1.0, 1.0], None));
    let beside = [2500, -150, 1500];
    assert!(kept(&mut arena, beside, [1.0, 0.1, 0.0], None));
    assert!(!kept(&mut arena, beside, [-1.0, 0.1, 0.0], None));
    let above = [0, 1000, 3000];
    assert!(kept(&mut arena, above, night, Some(-200.0)));
    assert!(!kept(&mut arena, above, night, None));
}
