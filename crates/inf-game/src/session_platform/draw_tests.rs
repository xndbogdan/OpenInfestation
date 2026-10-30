use std::path::Path;

use inf_formats::{FrontendExecutable, MissionSaveState, PlayerSaveState};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_bindings::FrontendDevice,
    frontend_display::frontend_startup,
    frontend_renderer::FrontendRendererState,
    frontend_scene::SceneProjection,
    frontend_screen::FrontendFactoryHost,
    input::{DeviceButtons, DeviceId, InputRuntime},
    menu::MenuColor,
    menu_text::INITIAL_GLYPH_BASE_WORD,
    model_primitives::PrimitiveTables,
    name_entry::{IdentityWriteRequest, LoadedIdentity, NameEntryActions, NameEntryCompletion},
    name_entry_frame::NameEntryFrameError,
    name_entry_renderer::{
        NameEntryRendererError, NameEntryRendererState, SoftwareNameEntryContext,
        SoftwareNameEntryFrame,
    },
    profile_store::ProfileStore,
    render_resources::RenderResources,
    scheduler::SchedulerTick,
    screen_registry::ScreenLifecycle,
    session::{SessionController, SessionHost, SessionPhase, SessionState, TransitionEffect},
    session_platform::{software, PortScreen, PortSessionServices},
    software_frame::{SoftwareFrame, SoftwareSurface},
    software_scene::{SoftwareScene, SoftwareSceneRequest, SoftwareSceneStats},
    text_entry_screen::{
        TextEntryFactoryHost, TextEntryScreenDraw, TextEntryServices, TextEntryTick,
    },
    title_screen::{TitleHost, TitleVideoHost},
    Installation, ResidencyRequest, ResourceSelection,
};

struct DrawOwners {
    profiles: ProfileStore,
    settings: crate::frontend_settings::FrontendSettings,
    identity: crate::player_identity::PlayerIdentity,
    video: crate::frontend_video::FrontendVideoState,
}

#[derive(Default)]
struct Factory {
    terrain_detail: u32,
    loading_mode: std::cell::Cell<u8>,
    keyboard: Option<DeviceId>,
    rng: u32,
    draw_owners: Option<DrawOwners>,
}

fn keyboard(input: &mut InputRuntime) -> Vec<FrontendDevice> {
    vec![FrontendDevice {
        native_type: 0,
        source: input.create_device(DeviceButtons::default()).unwrap(),
    }]
}

impl crate::gameplay_screen::GameplayTerrainSettings for Factory {
    fn terrain_detail_setting(
        &mut self,
    ) -> Result<u32, crate::terrain_runtime::TerrainPolicyError> {
        Ok(self
            .draw_owners
            .as_ref()
            .map_or(self.terrain_detail, |owners| {
                owners.settings.words()[0x70 / 4]
            }))
    }
}

impl FrontendFactoryHost for Factory {
    type Error = &'static str;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let devices = keyboard(input);
        self.keyboard = Some(devices[0].source);
        Ok(devices)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TextEntryFactoryHost for Factory {
    type Error = &'static str;
    fn read_identity(&mut self, _: SessionPhase, _: u8) -> Result<LoadedIdentity, Self::Error> {
        Ok(LoadedIdentity {
            name: b"Pilot".to_vec(),
            initials: b"AB".to_vec(),
        })
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let devices = keyboard(input);
        self.keyboard = Some(devices[0].source);
        Ok(devices)
    }
}

impl PortSessionServices for Factory {
    type Error = &'static str;
    type IdentityError = std::io::Error;
    fn persist_identity(&mut self, _: IdentityWriteRequest<'_>) -> Result<(), Self::IdentityError> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "draw fixture has no identity persistence owner",
        ))
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        None
    }
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_> {
        crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach
    }
    fn world_tick_inputs(&self) -> crate::session_platform::WorldTickPlatformInputs {
        crate::session_platform::WorldTickPlatformInputs {
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            settings_20: 0,
            settings_70: self
                .draw_owners
                .as_ref()
                .map_or(self.terrain_detail, |owners| {
                    owners.settings.words()[0x70 / 4]
                }),
            settings_78: 0,
            settings_7c: 0,
            input_mode_08: 0,
            settings_84: 0,
            vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
        }
    }
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_> {
        crate::loading_integration::LoadingPlatformLoans {
            voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
            mode: &self.loading_mode,
            rng: &mut self.rng,
        }
    }
    fn process_rng(&mut self) -> &mut u32 {
        &mut self.rng
    }
    fn session_music(
        &mut self,
        _: crate::session_platform::SessionMusicRequest,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error> {
        input.reset_baselines_and_discard();
        Ok(())
    }
    fn startup_settings(&self) -> crate::session_platform::StartupSettings {
        crate::session_platform::StartupSettings {
            language: 0,
            loaded: true,
        }
    }
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings {
        crate::key_bindings::KeyBindings::retail_initial()
    }
}

impl TitleVideoHost for Factory {
    type Error = &'static str;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(None)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err("title video presentation is absent")
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err("title video close is absent")
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TitleHost for Factory {
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let devices = keyboard(input);
        self.keyboard = Some(devices[0].source);
        Ok(devices)
    }
}

impl software::PortDrawPlatform for Factory {
    fn world_draw_parts(&mut self) -> software::WorldDrawParts<'_> {
        let owners = self
            .draw_owners
            .as_mut()
            .expect("this fixture has no world content loan");
        software::WorldDrawParts {
            settings: &owners.settings,
            audio: None,
            video: None,
            network: None,
        }
    }

    fn frontend_draw_parts(&mut self) -> software::FrontendDrawParts<'_> {
        let owners = self
            .draw_owners
            .as_mut()
            .expect("this fixture has no frontend content loan");
        software::FrontendDrawParts {
            profiles: &mut owners.profiles,
            settings: &owners.settings,
            identity: &mut owners.identity,
            video: &mut owners.video,
        }
    }
}

fn fixture() -> (PortSession<Factory>, SessionController) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    let mut session = PortSession::new(
        assets,
        residency,
        Factory::default(),
        FrontendRenderContext {
            projection: SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [0, 0],
            },
        },
        None,
    )
    .unwrap();
    session.boot_frontend().unwrap();
    let mut controller = SessionController::default();
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut session).unwrap();
    (session, controller)
}

struct NoCompletion;

impl NameEntryActions for NoCompletion {
    type Error = &'static str;
    fn grant_cheat_flag(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("draw preparation does not enter cheats")
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("draw preparation does not request a phase")
    }
}

impl NameEntryCompletion for NoCompletion {
    fn persist_identity(&mut self, _: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        Err("draw preparation does not persist identity")
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        Err("draw preparation does not complete a name")
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// Advance the actual screen through 0045A8F0, including base input and the
/// shared animation owner. No independent presentation snapshot is created.
fn mature_screen(session: &mut PortSession<Factory>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sidecar = root.join(".tmp").join(format!(
        "name-entry-frame-{}-{}.sav",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut store = ProfileStore::open_copy(root.join("retail/Infest.sav"), &sidecar).unwrap();
    let mut effects = NoCompletion;
    let mut rng = 0;
    let mut expected_animation = session.frontend_process().animation();
    expected_animation.advance(125_000);
    let completed = session
        .text
        .as_mut()
        .unwrap()
        .tick(
            TextEntryTick {
                scheduler: SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 9_000_000,
                },
                attract: false,
            },
            &mut TextEntryServices {
                input: &mut session.input,
                store: &mut store,
                key_names: session.catalog.assets().key_names(),
                keyboard_adapter: session.platform.keyboard,
                text_process: &mut session.text_process,
                frontend_process: &session.process,
                rng: &mut rng,
                effects: &mut effects,
                phase: 9,
                poll_micros: 9_000_000,
            },
        )
        .unwrap();
    assert!(completed.is_empty());
    assert_eq!(
        session.text_screen().unwrap().screen_input().age_millis(),
        9000
    );
    assert_eq!(session.frontend_process().animation(), expected_animation);
    drop(store);
    std::fs::remove_file(sidecar).unwrap();
}

#[derive(Debug)]
struct DrawReport {
    scene: SoftwareSceneStats,
    glyphs: usize,
    finished: usize,
    changed_pixels: usize,
}

fn render(
    draw: PortNameEntryDraw<'_>,
    mismatched_context: bool,
    surface: SoftwareSurface<'_>,
) -> Result<DrawReport, NameEntryFrameError<NameEntryRendererError>> {
    // The actual registry guard is active while the split borrows are lent.
    assert_eq!(
        draw.registry.attach_screen(draw.attachment),
        Err(RegistryError::ContextBusy(draw.attachment.context_owner))
    );
    let resources = RenderResources::new(draw.residency);
    let startup = frontend_startup(
        draw.residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(draw.assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let clear = [2, 3, 5, 255];
    let mut pixels = SoftwareFrame::new(640, 480, clear);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup.scene.environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let renderer = scene.id();
    let context_owner = if mismatched_context {
        draw.registry.register_context(FrontendRenderContext {
            projection: startup.projection,
        })
    } else {
        draw.attachment.context_owner
    };
    // A caller projection deliberately unlike the template must be ignored.
    let caller_projection = SceneProjection {
        focal: [123, 456],
        viewport: [800, 600],
        fade: [3, 7],
    };
    draw.registry
        .replace_backend(
            draw.attachment.context_owner,
            FrontendRenderContext {
                projection: caller_projection,
            },
        )
        .unwrap();
    let sine = draw.assets.sine_table().values;
    let mut state = NameEntryRendererState::default();
    let mut host = SoftwareNameEntryFrame {
        scene: &mut scene,
        resources,
        registry: draw.registry,
        attachment: draw.attachment,
        context: SoftwareNameEntryContext {
            owner: context_owner,
            renderer,
        },
        projection: startup.projection,
        sine_quarter: &sine,
        context_origin_pixels: [320, 240],
        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        state: &mut state,
        surface,
    };
    let result = draw.screen.draw_frame(
        TextEntryScreenDraw {
            animation: draw.animation,
            selector_one: 1,
        },
        &mut host,
    );
    assert_eq!(
        host.registry
            .backend(draw.attachment.context_owner)
            .unwrap()
            .projection,
        caller_projection,
        "name-entry draw never mutates the caller projection"
    );
    if result.is_ok() {
        assert_eq!(
            host.scene.environment.projection.focal,
            startup.projection.focal
        );
        assert_eq!(host.scene.environment.projection.viewport, [640, 480]);
        assert_eq!(host.scene.pixels.stats.unsupported, 0);
        assert_eq!(host.state.published_renderer, Some(renderer));
        assert_eq!(
            host.state.caller_context,
            Some(draw.attachment.context_owner)
        );
    } else {
        assert_eq!(host.state.finished_draws, 0);
    }
    result?;
    let rgba = host.scene.pixels.frame.rgba();
    let changed_pixels = rgba.chunks_exact(4).filter(|pixel| *pixel != clear).count();
    if let Some(directory) = std::env::var_os("INF_NAME_ENTRY_FRAME_SMOKE_DIR") {
        let path = Path::new(&directory).join(format!(
            "name-entry-session-age{}.png",
            draw.screen.screen_input().age_millis()
        ));
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, 640, 480);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(rgba)
            .unwrap();
    }
    Ok(DrawReport {
        scene: host.scene.stats,
        glyphs: host.state.glyph_passes,
        finished: host.state.finished_draws,
        changed_pixels,
    })
}

#[test]
fn session_text_frame_reaches_pixels_then_returns_without_stale_draw() {
    let (mut session, mut controller) = fixture();
    assert!(session
        .with_name_entry_draw::<(), &'static str>(|_| panic!("frontend is active"))
        .unwrap()
        .is_none());
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    session.frontend_process().advance_animation(123_000);
    let expected_animation = session.frontend_process().animation();
    let report = session
        .with_name_entry_draw(|draw| {
            assert_eq!(draw.animation, expected_animation);
            assert_eq!(draw.screen.entry().unwrap().name(), b"Pilot");
            render(draw, false, SoftwareSurface::Pixels)
        })
        .unwrap()
        .unwrap();
    assert_eq!(report.scene.frame_begins, 1);
    assert_eq!(report.scene.begins, 2);
    assert_eq!(report.scene.models, 3);
    assert_eq!(report.scene.stars, 1000);
    assert_eq!(report.scene.flushes, 1);
    assert_eq!(report.scene.depth_resets, 1);
    assert_eq!(report.scene.presentation_clears, 0);
    assert!(report.glyphs > 0);
    assert_eq!(report.finished, 1);
    assert!(report.changed_pixels > 1000, "{report:?}");
    assert_eq!(
        session.text_screen().unwrap().lifecycle(),
        ScreenLifecycle::Ready
    );

    mature_screen(&mut session);
    let mature = session
        .with_name_entry_draw(|draw| render(draw, false, SoftwareSurface::Pixels))
        .unwrap()
        .unwrap();
    assert_eq!(mature.scene.models, 3);
    assert_eq!(mature.scene.stars, 1000);
    assert!(
        mature.changed_pixels > report.changed_pixels,
        "mature backdrop must add visible pixels"
    );
    assert_eq!(mature.finished, 1);

    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut session).unwrap();
    assert!(session
        .with_name_entry_draw::<(), &'static str>(|_| panic!("text screen is destroyed"))
        .unwrap()
        .is_none());
}

#[test]
fn renderer_association_error_stops_screen_and_releases_context_guard() {
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    assert!(matches!(
        session.with_name_entry_draw(|draw| render(draw, true, SoftwareSurface::Pixels)),
        Err(PortLinkedDrawError::Callback(NameEntryFrameError::Host(
            NameEntryRendererError::ContextRenderer
        )))
    ));
    assert_eq!(
        session.text_screen().unwrap().lifecycle(),
        ScreenLifecycle::Stopped
    );
    assert!(matches!(
        session.with_name_entry_draw(|draw| render(draw, false, SoftwareSurface::Pixels)),
        Err(PortLinkedDrawError::Callback(
            NameEntryFrameError::Lifecycle(ScreenLifecycle::Stopped)
        ))
    ));
    // Teardown plus fresh frontend attachment would fail if the draw left
    // the linked-context busy flag set after its ordinary error.
    controller.request_phase(SessionPhase::Frontend);
    controller.transition(&mut session).unwrap();
    assert!(session.frontend_screen().is_some());
}

fn results_fixture() -> (PortSession<Factory>, SessionController) {
    let (mut session, mut controller) = fixture();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    let mut load_state = controller.state().clone();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut load_state)
        .unwrap();
    session
        .world
        .as_mut()
        .unwrap()
        .mission
        .save
        .mission_selection = 18;
    assert!(controller.request_phase(SessionPhase::Phase8));
    controller.transition(&mut session).unwrap();
    assert_eq!(session.linked_screen().unwrap(), PortScreen::IntroResults);
    (session, controller)
}

/// The intro's results (world 22) end the draw after its prefix; any other
/// world goes on to the page (below).
#[test]
fn results_linked_draw_prepares_the_actual_queue_and_publication_without_depth_or_content() {
    for current_world in [22] {
        let (mut session, controller) = results_fixture();
        session.world.as_mut().unwrap().mission.current_world_a0 = current_world;
        let snapshot = session.residency().clone();
        let resources = RenderResources::new(&snapshot);
        let assets = session.catalog.assets();
        let startup = frontend_startup(
            &snapshot,
            crate::presentation::NativeFrame::original([640, 480]),
        )
        .unwrap();
        let tables = PrimitiveTables::from_executable(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let frontend_scene = crate::frontend_scene::FrontendScene::new(
            assets.scene_models().to_vec(),
            assets.position_scale(),
            assets.sine_table().values,
        );
        let sine = assets.sine_table().values;
        let mut pixels = SoftwareFrame::new(640, 480, [19, 31, 47, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: session.model_process().clone(),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        scene.environment.coordinate_shift = 7;
        scene.graphics.publish_shift(7).unwrap(); // explicit supplied fixture publication
        scene.primitive_state.effect_color = 0x93a7_5cf1;
        scene.set_process_rng(Some(0x8123_4567));
        scene
            .queue
            .submit(crate::draw_queue::DrawSubmission {
                depth: 123,
                payload_blocks: 3,
                receiver_offset: 0,
                command: crate::model_primitives::PrimitiveCommand::new(
                    crate::model_primitives::PrimitiveCallback::FlatQuadNear,
                    123,
                    3,
                ),
            })
            .unwrap();
        assert_eq!(scene.queue.state().command_count, 1);
        let renderer = scene.id();
        let pixels_before = scene.pixels.frame.rgba().to_vec();
        let depth_before = scene.pixels.frame.pixel_depth(0, 0);
        let environment = format!("{:?}", scene.environment);
        let primitive = scene.primitive_state;
        let raster = format!("{:?}", scene.pixels.pipeline);
        let renderer_before = scene.pixels.renderer;
        let stats = scene.stats;
        let arena = format!("{:?}", session.world().unwrap().arena);
        let mission = format!("{:?}", session.world().unwrap().mission);
        let registry = session
            .registry
            .screens(session.attachment.session)
            .unwrap()
            .to_vec();
        let input = session.input.selected_context();
        let process = session.factory_process;
        let screen = session.intro_results_screen().unwrap().id();
        let mut frontend_state = FrontendRendererState::default();
        let mut name_state = NameEntryRendererState::default();
        let outcome = software::SoftwareLinkedFrame {
            scene: &mut scene,
            resources,
            database_resources: None,
            frontend_scene: &frontend_scene,
            sine_quarter: &sine,
            template_projection: startup.projection,
            frontend_state: &mut frontend_state,
            name_entry_state: &mut name_state,
            surface: SoftwareSurface::Pixels,
        }
        .draw::<_, std::convert::Infallible>(&mut session, controller.state())
        .unwrap();
        assert!(matches!(outcome, software::LinkedDrawOutcome::IntroResults));
        assert_eq!(frontend_state.published_renderer, Some(renderer));
        assert_eq!(frontend_state.caller_context, None);
        assert_eq!(frontend_state.disposed_native_errors, 0);
        assert_eq!(name_state.published_renderer, None);
        assert_eq!(scene.queue.state().capacity, 0xc8000);
        assert_eq!(scene.queue.state().command_count, 0);
        assert_eq!(scene.stats.frame_begins, stats.frame_begins + 1);
        assert_eq!(scene.stats.begins, stats.begins + 1);
        assert_eq!(scene.stats.depth_resets, stats.depth_resets);
        assert_eq!(scene.stats.models, stats.models);
        assert_eq!(scene.stats.flushes, stats.flushes);
        assert_eq!(scene.stats.presentation_clears, stats.presentation_clears);
        assert_eq!(scene.pixels.frame.rgba(), pixels_before);
        assert_eq!(scene.pixels.frame.pixel_depth(0, 0), depth_before);
        assert_eq!(format!("{:?}", scene.environment), environment);
        assert_eq!(scene.primitive_state, primitive);
        assert_eq!(format!("{:?}", scene.pixels.pipeline), raster);
        assert_eq!(scene.pixels.renderer, renderer_before);
        assert_eq!(scene.process_rng(), Some(0x8123_4567));
        assert_eq!(session.intro_results_screen().unwrap().id(), screen);
        assert_eq!(format!("{:?}", session.world().unwrap().arena), arena);
        assert_eq!(format!("{:?}", session.world().unwrap().mission), mission);
        assert_eq!(session.factory_process, process);
        assert_eq!(session.input.selected_context(), input);
        assert_eq!(
            session
                .registry
                .screens(session.attachment.session)
                .unwrap(),
            registry
        );
    }
}

/// After Level 1 (world 18, on to 2), phase 8 draws the results page with
/// the session's own residency. The first frame draws the planet, then
/// model 98 twice after the depth reset; no panel has opened. The whole page,
/// every panel open as at Infest02's tick 19, resolves each font, text,
/// style, sprite and model it reads.
#[test]
fn a_worlds_results_page_draws_with_the_sessions_residency() {
    let (mut session, mut controller) = fixture();
    session
        .prepare_offline_player(&mut SessionState::default())
        .unwrap();
    // The prepared load owner selects the world LoadWorld loads.
    session
        .pending_world
        .as_mut()
        .unwrap()
        .mission
        .save
        .mission_selection = 18;
    let mut load_state = controller.state().clone();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut load_state)
        .unwrap();
    {
        let mission = &mut session.world.as_mut().unwrap().mission;
        assert_eq!(mission.current_world_a0, 18);
        mission.save.owner_3c[18] |= 3;
        mission.save.mission_selection = 2;
    }
    assert!(controller.request_phase(SessionPhase::Phase8));
    controller.transition(&mut session).unwrap();
    assert_eq!(session.linked_screen().unwrap(), PortScreen::IntroResults);

    let snapshot = session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let assets = session.catalog.assets();
    let startup = frontend_startup(
        &snapshot,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let frontend_scene = crate::frontend_scene::FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let sine = assets.sine_table().values;
    let mut pixels = SoftwareFrame::new(640, 480, [19, 31, 47, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    let stats = scene.stats;
    let before = scene.pixels.frame.rgba().to_vec();
    let mut frontend_state = FrontendRendererState::default();
    let mut name_state = NameEntryRendererState::default();
    let outcome = software::SoftwareLinkedFrame {
        scene: &mut scene,
        resources,
        database_resources: None,
        frontend_scene: &frontend_scene,
        sine_quarter: &sine,
        template_projection: startup.projection,
        frontend_state: &mut frontend_state,
        name_entry_state: &mut name_state,
        surface: SoftwareSurface::Pixels,
    }
    .draw::<_, std::convert::Infallible>(&mut session, controller.state())
    .unwrap();
    assert!(matches!(outcome, software::LinkedDrawOutcome::IntroResults));
    assert_eq!(scene.stats.models, stats.models + 2);
    assert_eq!(scene.stats.depth_resets, stats.depth_resets + 1);
    assert_ne!(scene.pixels.frame.rgba(), before);
    // The draw's tail built panel 1 for the next frame.
    assert!(session.intro_results_screen().unwrap().panels()[0].is_some());

    let screen = session.intro_results_screen().unwrap();
    let mut open = crate::panel_animation::PanelAnimation::with_duration(0x6000, 500_000);
    open.current_stage = 2;
    open.requested_stage = 2;
    let world = session.world().unwrap();
    let page = crate::results_page::ResultsPage {
        state: crate::results_page::ResultsPageState {
            age_millis: 0x1c9e,
            medal: Some(2),
            ..screen.page_state([Some(open); 3])
        },
        mission: &world.mission,
        rank_word: Some(2),
        tokens: world.arena.pickup_kind7_count(),
        surface_pixels: [640, 480],
        projection: SceneProjection {
            viewport: [640, 480],
            ..startup.projection
        },
        animation: session.process.animation(),
        selector_one: session.process.object_selector_one(),
        glyph_base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        credits: None,
    };
    let stats = scene.stats;
    crate::results_page::software::draw(
        &mut scene,
        resources,
        &page,
        &crate::menu_basis::MenuBasisMath::new(sine),
    )
    .unwrap();
    assert_eq!(scene.stats.models, stats.models + 5);
}

#[test]
fn results_linked_attract_draw_preserves_the_existing_queue_and_publication() {
    let (mut session, controller) = results_fixture();
    session.attract = true;
    let snapshot = session.residency().clone();
    let resources = RenderResources::new(&snapshot);
    let assets = session.catalog.assets();
    let startup = frontend_startup(
        &snapshot,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let frontend_scene = crate::frontend_scene::FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let sine = assets.sine_table().values;
    let mut pixels = SoftwareFrame::new(640, 480, [17, 37, 73, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: session.model_process().clone(),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    scene
        .queue
        .submit(crate::draw_queue::DrawSubmission {
            depth: 456,
            payload_blocks: 3,
            receiver_offset: 0,
            command: crate::model_primitives::PrimitiveCommand::new(
                crate::model_primitives::PrimitiveCallback::FlatQuadNear,
                456,
                3,
            ),
        })
        .unwrap();
    let queue = scene.queue.state();
    let stats = scene.stats;
    let color = scene.pixels.frame.rgba().to_vec();
    let mut frontend_state = FrontendRendererState::default();
    let mut name_state = NameEntryRendererState::default();
    // The unavailable mission is an explicit typed input; the live attract
    // branch must return without needing it or any frontend content loan.
    let retained_world = session.world.take();
    let outcome = software::SoftwareLinkedFrame {
        scene: &mut scene,
        resources,
        database_resources: None,
        frontend_scene: &frontend_scene,
        sine_quarter: &sine,
        template_projection: startup.projection,
        frontend_state: &mut frontend_state,
        name_entry_state: &mut name_state,
        surface: SoftwareSurface::Pixels,
    }
    .draw::<_, std::convert::Infallible>(&mut session, controller.state())
    .unwrap();
    assert!(matches!(outcome, software::LinkedDrawOutcome::IntroResults));
    assert_eq!(scene.queue.state(), queue);
    assert_eq!(scene.stats, stats);
    assert_eq!(scene.pixels.frame.rgba(), color);
    assert_eq!(frontend_state.published_renderer, None);
    assert_eq!(frontend_state.caller_context, None);
    assert_eq!(name_state.published_renderer, None);
    assert!(session.attract);
    assert!(session.world.is_none());
    session.world = retained_world;
}

/// Run serially with --ignored: creates a hidden real SDL window, without
/// presenting it or depending on a dummy video driver.
#[test]
#[ignore = "opens a hidden native SDL window to validate the live surface path"]
fn name_entry_window_draw_and_resize_validation() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let mut window = video
        .window("Infestation name-entry frame test", 640, 480)
        .hidden()
        .build()
        .unwrap();
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    mature_screen(&mut session);
    let report = session
        .with_name_entry_draw(|draw| render(draw, false, SoftwareSurface::Window(&window)))
        .unwrap()
        .unwrap();
    assert_eq!(report.scene.stars, 1000);
    assert_eq!(report.finished, 1);
    window.set_size(800, 600).unwrap();
    assert!(matches!(
        session.with_name_entry_draw(|draw| render(draw, false, SoftwareSurface::Window(&window))),
        Err(PortLinkedDrawError::Callback(NameEntryFrameError::Host(
            NameEntryRendererError::Surface {
                actual: [800, 600],
                expected: [640, 480]
            }
        )))
    ));
    assert_eq!(
        session.text_screen().unwrap().lifecycle(),
        ScreenLifecycle::Stopped
    );
}

#[test]
fn scoped_draw_rejects_a_different_linked_screen_before_callback() {
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    let expected = session.text_screen().unwrap().id();
    let attachment = session.attachment();
    let other = session.registry().attach_screen(attachment).unwrap();
    assert!(
        matches!(session.with_name_entry_draw::<(), &'static str>(|_| panic!("wrong linked screen")),
            Err(PortLinkedDrawError::LinkedScreen { expected: found, actual: Some(actual) }) if found == expected && actual == other
        )
    );
}

#[test]
fn briefing_builder_lends_the_actual_nullable_process_without_preflight() {
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    controller.request_phase(SessionPhase::Phase3);
    controller.transition(&mut session).unwrap();
    let process_address = std::ptr::addr_of!(session.factory_process);
    let registry_null = session
        .world
        .as_ref()
        .unwrap()
        .arena
        .actor_handles()
        .unwrap()
        .null_handle();
    let current_world = session.world.as_ref().unwrap().mission.current_world_a0;
    for source in [
        None,
        Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
    ] {
        session.factory_process.process_null_handle = source;
        let before_rng = session.world.as_ref().unwrap().rng;
        let result = session
            .with_briefing_draw::<_, &'static str>(SessionPhase::Phase3, |draw| {
                assert!(std::ptr::eq(
                    draw.effects.birth_inputs.process,
                    process_address
                ));
                assert_eq!(
                    draw.effects.birth_inputs.process.process_null_handle,
                    source
                );
                assert_eq!(
                    draw.world.arena.actor_handles().unwrap().null_handle(),
                    registry_null
                );
                assert_eq!(draw.effects.birth_inputs.phase, Some(3));
                // 0040C1EE's pickups draw and collect as in gameplay.
                let pickups = draw.effects.pickups.as_ref().unwrap();
                assert_eq!(*pickups.current_world, current_world);
                assert!(pickups.text.is_some());
                // No emission: even absent source must not suppress callback work.
                *draw.process_rng = draw.process_rng.wrapping_add(1);
                Ok(17)
            })
            .unwrap();
        assert_eq!(result, Some(17));
        assert_eq!(
            session.world.as_ref().unwrap().rng,
            before_rng.wrapping_add(1)
        );
        assert_eq!(session.factory_process.process_null_handle, source);
    }
}

#[test]
fn briefing_draw_uses_retained_world_rng_and_keeps_reached_writes_on_failure() {
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    controller.request_phase(SessionPhase::Phase3);
    controller.transition(&mut session).unwrap();
    session.world.as_mut().unwrap().rng = 0x1234;
    let map = session.world.as_ref().unwrap().map;
    session
        .maps
        .jitter_mut(map)
        .unwrap()
        .apply(crate::world_map::WorldMapJitterWrite::MissionEvent48D980)
        .unwrap();
    let actor_ids = session.world.as_ref().unwrap().arena.actor_list().to_vec();
    session.platform.rng = 0x5678;
    let result = session.with_briefing_draw::<(), _>(SessionPhase::Phase3, |draw| {
        assert_eq!(draw.world.arena.actor_list(), actor_ids);
        assert_eq!(draw.world.map_state.jitter_radius(), 0x1fff);
        assert_eq!(draw.world.network_active, false);
        let rng = draw.process_rng;
        assert_eq!(*rng, 0x1234);
        *rng = rng.wrapping_add(1);
        Err("failed after a native random draw")
    });
    assert!(matches!(
        result,
        Err(PortLinkedDrawError::Callback(
            "failed after a native random draw"
        ))
    ));
    assert_eq!(session.world.as_ref().unwrap().rng, 0x1235);
    assert_eq!(session.platform.rng, 0x5678);

    let world = session.world.take();
    assert!(matches!(
        session.with_briefing_draw::<(), &'static str>(SessionPhase::Phase3, |_| panic!(
            "missing world owner"
        )),
        Err(PortLinkedDrawError::MissingWorld)
    ));
    session.world = world;
    controller.tick(&mut session, 400, 400).unwrap();
    assert_eq!(
        session
            .maps
            .jitter(session.world().unwrap().map)
            .unwrap()
            .jitter_radius(),
        0x1fff,
        "phase-3 session ticking does not run mission map decay"
    );
    controller.request_phase(SessionPhase::Gameplay);
    controller.transition(&mut session).unwrap();
    controller.tick(&mut session, 400, 400).unwrap();
    assert_eq!(
        session
            .maps
            .jitter(session.world().unwrap().map)
            .unwrap()
            .jitter_radius(),
        0x1ffd,
        "the gameplay callback executes native signed delta/200 map decay"
    );
}

#[test]
fn newer_session_member_does_not_replace_the_drawing_context() {
    let (mut session, mut controller) = fixture();
    controller.request_phase(SessionPhase::Phase9);
    controller.transition(&mut session).unwrap();
    let drawing_attachment = session.attachment();
    let text_id = session.text_screen().unwrap().id();
    let other_owner = session.registry().register_context(FrontendRenderContext {
        projection: SceneProjection {
            focal: [1, 2],
            viewport: [3, 4],
            fade: [5, 6],
        },
    });
    let other_screen = session
        .registry()
        .attach_screen(ScreenAttachment {
            session: drawing_attachment.session,
            context_owner: other_owner,
        })
        .unwrap();
    assert_eq!(
        session
            .registry()
            .current_screen(drawing_attachment.session)
            .unwrap(),
        Some(other_screen)
    );
    assert_eq!(
        session
            .registry()
            .linked_screen(drawing_attachment.context_owner)
            .unwrap(),
        text_id
    );
    let report = session
        .with_name_entry_draw(|draw| render(draw, false, SoftwareSurface::Pixels))
        .unwrap()
        .unwrap();
    assert_eq!(report.scene.models, 3);
    assert_eq!(report.scene.stars, 1000);
    assert_eq!(report.finished, 1);
    assert_eq!(
        session.text_screen().unwrap().lifecycle(),
        ScreenLifecycle::Ready
    );
}

#[test]
fn gameplay_linked_draw_freshly_acquires_before_first_tick_without_advancing_pose_or_clock() {
    let (mut session, mut controller) = fixture();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let directory = root.join(format!("../.tmp/predraw-owned-{}", std::process::id()));
    let settings = crate::frontend_settings::FrontendSettings::load_or_defaults(
        directory.join("settings.json"),
        session.catalog.assets(),
    )
    .unwrap();
    let video = crate::frontend_video::FrontendVideoState::new(
        vec![crate::frontend_video::VideoDevice {
            id: 0,
            name: b"Predraw Test Display".to_vec(),
        }],
        settings.current_display(),
    )
    .unwrap();
    session.platform.draw_owners = Some(DrawOwners {
        profiles: ProfileStore::open_copy(root.join("Infest.sav"), directory.join("profiles.sav"))
            .unwrap(),
        settings,
        identity: Default::default(),
        video,
    });
    session
        .prepare_offline_player(&mut controller.state().clone())
        .unwrap();
    for selection in [22, 18] {
        // Both real loads borrow the same prepared player and arena. The second
        // runs the native retained-manager cleanup before constructing its world.
        if let Some(world) = session.world.as_mut() {
            world.mission.save.mission_selection = selection;
        } else {
            session
                .pending_world
                .as_mut()
                .unwrap()
                .mission
                .save
                .mission_selection = selection;
        }
        let mut load_state = controller.state().clone();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut load_state)
            .unwrap();
        if controller.state().current_phase() != SessionPhase::Gameplay {
            controller.request_phase(SessionPhase::Gameplay);
            controller.transition(&mut session).unwrap();
        }
        let world = session.world.as_mut().unwrap();
        let camera = world
            .arena
            .resolve_actor_handle(world.arena.camera_handle().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(world.native_camera, None, "not selected by Tick");
        let expected = world.arena.camera_draw_request(camera).unwrap();
        let camera_state = format!("{:?}", world.arena.object(camera).unwrap());
        let players = format!("{:?}", world.arena.world_players());
        world.camera_failure = Some("existing pose diagnostic".into());
        let retained_process = session.factory_process;
        let clock = session.process.animation();
        let globals = session.camera_globals.clone();
        let snapshot = session.residency.clone();
        let resources = RenderResources::new(&snapshot);
        let assets = session.catalog.assets();
        let startup = frontend_startup(
            &snapshot,
            crate::presentation::NativeFrame::original([640, 480]),
        )
        .unwrap();
        let tables = PrimitiveTables::from_executable(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let frontend_scene = crate::frontend_scene::FrontendScene::new(
            assets.scene_models().to_vec(),
            assets.position_scale(),
            assets.sine_table().values,
        );
        let sine = assets.sine_table().values;
        let mut pixels = SoftwareFrame::new(640, 480, [17, 37, 73, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: session.model_process().clone(),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        scene.set_terrain(Some(session.world.as_ref().unwrap().terrain.clone()));
        let mut frontend_state = FrontendRendererState::default();
        let mut name_state = NameEntryRendererState::default();
        let outcome = software::SoftwareLinkedFrame {
            scene: &mut scene,
            resources,
            database_resources: None,
            frontend_scene: &frontend_scene,
            sine_quarter: &sine,
            template_projection: startup.projection,
            frontend_state: &mut frontend_state,
            name_entry_state: &mut name_state,
            surface: SoftwareSurface::Pixels,
        }
        .draw::<_, std::convert::Infallible>(&mut session, controller.state())
        .unwrap();
        assert!(matches!(outcome, software::LinkedDrawOutcome::Gameplay));
        let world = session.world.as_ref().unwrap();
        assert_eq!(world.native_camera, Some(camera));
        assert_eq!(
            world.camera_failure.as_deref(),
            Some("existing pose diagnostic")
        );
        assert_eq!(scene.environment.view.eye, expected.eye);
        assert_eq!(scene.environment.view.basis, expected.basis);
        assert_eq!(scene.environment.coordinate_shift, 16);
        assert_eq!(
            format!("{:?}", world.arena.object(camera).unwrap()),
            camera_state
        );
        assert_eq!(format!("{:?}", world.arena.world_players()), players);
        assert_eq!(session.factory_process, retained_process);
        assert_eq!(session.process.animation(), clock);
        assert_eq!(session.camera_globals, globals);
        assert!(world.last_draw.unwrap().native_camera);
        // Downstream Draw owns its existing terrain/model RNG and queues;
        // this test does not attribute those effects to camera acquisition.
    }
}
