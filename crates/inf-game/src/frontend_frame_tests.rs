use std::{collections::VecDeque, path::Path};

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_camera::FrontendEntry;
use crate::frontend_control::FrontendControlConfig;
use crate::menu::{
    MenuActionRequest, MenuBasis, MenuControllerDrawSetup, MenuData, MenuDrawColors, MenuHost,
    MenuRowDrawRequest, MenuStack, MenuState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Arena,
    Validate,
    Model,
    Footer,
    FinalFlush,
    MenuBegin,
    Row,
    Load4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fault {
    point: Point,
    code: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Effect(&'static str),
    Lookup(u8),
    Arena(u8, usize),
    Publish(u8),
    Context,
    Reset(u8),
    Validate,
    Environment,
    Scene(u8, &'static str),
    Style,
    Surface,
    Sprite(u16),
    MenuBegin,
    MenuBasis,
    ClipRead,
    Clip([i16; 4]),
    Row(usize),
    State(u8, u16),
    Load(u8, u16),
    Dispose(Fault),
}

// Required operations are explicitly recorded. This host does not render pixels
// or stand in for a production renderer, native device, or resource lazy loader.
struct Recorder {
    events: Vec<Event>,
    renderers: VecDeque<u8>,
    failure: Option<Fault>,
    h4: FrameResourceState,
    h6: FrameResourceState,
    clip: [i16; 4],
    context_captured: bool,
    failed_row_seen: bool,
    fail_row_index: usize,
}

impl Recorder {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            renderers: VecDeque::from([11, 12, 13, 21, 31, 41]),
            failure: None,
            h4: FrameResourceState::Resident,
            h6: FrameResourceState::Resident,
            clip: [0, 0, 640, 480],
            context_captured: false,
            failed_row_seen: false,
            fail_row_index: 0,
        }
    }
    fn fail(&mut self, point: Point) -> Result<(), Fault> {
        if self.failure.is_some_and(|failure| failure.point == point) {
            Err(self.failure.take().unwrap())
        } else {
            Ok(())
        }
    }
    fn fail_once(point: Point, code: Option<u32>) -> Self {
        Self {
            failure: Some(Fault { point, code }),
            ..Self::new()
        }
    }
}

impl MenuDrawHost for Recorder {
    type Error = Fault;
    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Fault> {
        assert!(self.context_captured);
        assert_eq!(setup.surface_pixels, [640, 480]);
        self.events.push(Event::MenuBegin);
        self.fail(Point::MenuBegin)
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), Fault> {
        self.events.push(Event::MenuBasis);
        Ok(())
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Fault> {
        self.events.push(Event::ClipRead);
        Ok(self.clip)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Fault> {
        self.events.push(Event::Clip(clip));
        self.clip = clip;
        Ok(())
    }
    fn colors(&self) -> MenuDrawColors {
        MenuDrawColors {
            selected_start: 0x0080_8080,
            ordinary: 0x00c8_c8c8,
            selected_end: 0x00ff_ffff,
        }
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Fault> {
        assert_eq!(request.callback_address, 0x4834f0);
        self.events.push(Event::Row(request.item_index));
        request.descriptor.position_pixels[1] += 60;
        if request.item_index == self.fail_row_index {
            let result = self.fail(Point::Row);
            self.failed_row_seen |= result.is_err();
            result
        } else {
            Ok(())
        }
    }
}

impl ScreenRendererPreparationHost<Fault> for Recorder {
    type Renderer = u8;
    fn renderer(&mut self) -> Result<u8, Fault> {
        let renderer = self
            .renderers
            .pop_front()
            .expect("unexpected renderer lookup");
        self.events.push(Event::Lookup(renderer));
        Ok(renderer)
    }
    fn replace_command_arena(&mut self, renderer: u8, bytes: usize) -> Result<(), Fault> {
        self.events.push(Event::Arena(renderer, bytes));
        self.fail(Point::Arena)
    }
    fn begin_batch(&mut self, renderer: u8) -> Result<(), Fault> {
        FrontendFrameHost::scene_submit(self, renderer, SceneCommand::BeginBatch)
    }
    fn publish_current_renderer(&mut self, renderer: u8) -> Result<(), Fault> {
        self.events.push(Event::Publish(renderer));
        Ok(())
    }
}

impl FrontendFrameHost for Recorder {
    fn capture_caller_context(&mut self) -> Result<(), Fault> {
        self.events.push(Event::Context);
        self.context_captured = true;
        Ok(())
    }
    fn reset_renderer(&mut self, renderer: u8) -> Result<(), Fault> {
        self.events.push(Event::Reset(renderer));
        Ok(())
    }
    fn validate_surface(&mut self) -> Result<(), Fault> {
        self.events.push(Event::Validate);
        self.fail(Point::Validate)
    }
    fn scene_environment(&mut self) -> Result<FrontendSceneEnvironment, Fault> {
        assert!(self.context_captured);
        self.events.push(Event::Environment);
        Ok(FrontendSceneEnvironment {
            projection: SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [10000, 20000],
            },
            inherited_view: SceneView {
                eye: [42, -71, 17],
                basis: MenuBasis::INITIAL,
            },
        })
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Fault> {
        self.events.push(Event::Surface);
        Ok([640, 480])
    }
    fn scene_submit(&mut self, renderer: u8, command: SceneCommand) -> Result<(), Fault> {
        let label = match command {
            SceneCommand::CoordinateShift(0) => "shift",
            SceneCommand::CoordinateShift(_) => panic!("unexpected coordinate shift"),
            SceneCommand::FarColor(_) => panic!("frontend does not write context far color here"),
            SceneCommand::DirectionalLight { .. } => "light",
            SceneCommand::AmbientLight(_) => "ambient",
            SceneCommand::Setup(_) => "setup",
            SceneCommand::PublishFade(_) => panic!("frontend does not replace fade independently"),
            SceneCommand::BeginBatch => "begin",
            SceneCommand::FlushBatch => "flush",
            SceneCommand::StarView(view) => {
                assert_eq!(view.eye, [42, -71, 17]);
                "inherited"
            }
            SceneCommand::Star(_) => "star",
            SceneCommand::PublishView(_) => "view",
            SceneCommand::Model(_) => "model",
            SceneCommand::ResetAfterFirstModel => "model-reset",
            SceneCommand::Footer(_) => "footer",
        };
        self.events.push(Event::Scene(renderer, label));
        match command {
            SceneCommand::Model(_) => self.fail(Point::Model),
            SceneCommand::Footer(_) => self.fail(Point::Footer),
            SceneCommand::FlushBatch if renderer == 41 => self.fail(Point::FinalFlush),
            _ => Ok(()),
        }
    }
    fn footer_style_word(&mut self) -> Result<u32, Fault> {
        self.events.push(Event::Style);
        Ok(0x12345678)
    }
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Fault> {
        assert!((1806..=1811).contains(&slot));
        self.events.push(Event::Sprite(slot));
        Ok([32, 16])
    }
    fn resource_state(&mut self, handler: u8, slot: u16) -> Result<FrameResourceState, Fault> {
        self.events.push(Event::State(handler, slot));
        Ok(match (handler, slot) {
            (4, 0) => self.h4,
            (6, 3) => self.h6,
            _ => panic!("unexpected tail slot"),
        })
    }
    fn load_resource(&mut self, handler: u8, slot: u16) -> Result<(), Fault> {
        self.events.push(Event::Load(handler, slot));
        match (handler, slot) {
            (4, 0) => {
                self.fail(Point::Load4)?;
                self.h4 = FrameResourceState::Resident;
            }
            (6, 3) => self.h6 = FrameResourceState::Resident,
            _ => panic!("unexpected resource load"),
        }
        Ok(())
    }
    fn native_error_code(error: &Fault) -> Option<u32> {
        error.code
    }
    fn dispose_caught_error(&mut self, error: Fault) -> Result<(), Fault> {
        self.events.push(Event::Dispose(error));
        Ok(())
    }
}

impl MenuHost for Recorder {
    type Error = Fault;
    fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), Fault> {
        panic!("unexpected menu action during fixture setup")
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), Fault> {
        panic!("fixture setup precedes the first actual draw")
    }
}

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn owners(assets: &FrontendAssets, ready: bool) -> (FrontendControl, FrontendScene) {
    let mut presets = assets.camera_presets().to_vec();
    if ready {
        // Explicit synthetic camera fixture: first-entry pose is already ready,
        // while the process returning bit is false until a successful draw.
        presets[0].eye = [-1200, 0, 7000];
        presets[0].target = [2000, 0, 5000];
    }
    let mut control = FrontendControl::new(FrontendControlConfig {
        camera_presets: presets,
        position_scale: assets.position_scale(),
        motion_scale: assets.motion_scale(),
        entry: FrontendEntry::First,
        context: 77,
    })
    .unwrap();
    control.menus_mut().push(MenuState::new(
        MenuData::new(assets.root_menu().clone()),
        MenuBasis::INITIAL,
    ));
    control
        .menus_mut()
        .tick(16000, &mut Recorder::new())
        .unwrap();
    let scene = FrontendScene::new(
        assets.scene_models()[..2].to_vec(),
        0,
        assets.sine_table().values,
    );
    (control, scene)
}

fn input(assets: &FrontendAssets) -> FrontendFrameInput<'_> {
    FrontendFrameInput {
        age_millis: 2468,
        visibility: MenuVisibility::OwnerFlagClear,
        sine_quarter: &assets.sine_table().values,
    }
}

#[test]
fn full_frame_preserves_live_renderer_lookups_and_ordered_scene_footer_menu_tail() {
    let assets = assets();
    let (mut control, scene) = owners(&assets, true);
    let mut host = Recorder::new();
    host.h4 = FrameResourceState::Vacant;
    host.h6 = FrameResourceState::Vacant;
    let result = draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).unwrap();
    assert!(matches!(
        result,
        FrontendFrameOutcome::Drawn {
            scene: SceneDrawOutcome {
                submitted_models: 2,
                ..
            },
            menu: Some(MenuDrawOutcome {
                menu_drawn: true,
                ..
            })
        }
    ));
    assert_eq!(
        &host.events[..13],
        &[
            Event::Lookup(11),
            Event::Arena(11, 0xc8000),
            Event::Lookup(12),
            Event::Scene(12, "begin"),
            Event::Lookup(13),
            Event::Publish(13),
            Event::Context,
            Event::Lookup(21),
            Event::Scene(21, "begin"),
            Event::Reset(21),
            Event::Validate,
            Event::Lookup(31),
            Event::Environment,
        ]
    );
    let style = host.events.iter().position(|e| *e == Event::Style).unwrap();
    assert_eq!(
        &host.events[style - 1..style + 3],
        &[
            Event::Scene(31, "begin"),
            Event::Style,
            Event::Surface,
            Event::Sprite(1807)
        ]
    );
    assert!(host
        .events
        .iter()
        .filter(|e| matches!(e, Event::Scene(_, "footer")))
        .all(|e| *e == Event::Scene(21, "footer")));
    let final_flush = host
        .events
        .iter()
        .position(|e| *e == Event::Scene(41, "flush"))
        .unwrap();
    assert_eq!(
        &host.events[final_flush - 1..final_flush + 3],
        &[
            Event::Lookup(41),
            Event::Scene(41, "flush"),
            Event::Surface,
            Event::MenuBegin
        ]
    );
    assert_eq!(
        &host.events[host.events.len() - 4..],
        &[
            Event::State(4, 0),
            Event::Load(4, 0),
            Event::State(6, 3),
            Event::Load(6, 3)
        ]
    );
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(control.menus().drawn());
    assert!(!control.stopped());
}

#[test]
fn unready_camera_still_draws_scene_and_tail_without_menu_queries() {
    let assets = assets();
    let (mut control, scene) = owners(&assets, false);
    assert!(!control.camera().ready());
    let mut host = Recorder::new();
    host.h6 = FrameResourceState::OutOfRange;
    assert!(matches!(
        draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).unwrap(),
        FrontendFrameOutcome::Drawn { menu: None, .. }
    ));
    assert_eq!(
        host.events.iter().filter(|e| **e == Event::Surface).count(),
        1
    );
    assert!(!host.events.contains(&Event::MenuBegin));
    assert!(!host.events.iter().any(|e| matches!(e, Event::Load(..))));
    assert_eq!(control.entry_state(), FrontendEntry::First);
    assert!(!control.menus().drawn());
}

#[test]
fn native301_inside_each_guarded_phase_is_disposed_before_tail_and_skips_remainder() {
    let assets = assets();
    for point in [
        Point::Validate,
        Point::Model,
        Point::Footer,
        Point::FinalFlush,
        Point::MenuBegin,
    ] {
        let (mut control, scene) = owners(&assets, true);
        let mut host = Recorder::fail_once(point, Some(0x301));
        assert_eq!(
            draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).unwrap(),
            FrontendFrameOutcome::RecoveredNative301
        );
        assert_eq!(
            &host.events[host.events.len() - 3..],
            &[
                Event::Dispose(Fault {
                    point,
                    code: Some(0x301)
                }),
                Event::State(4, 0),
                Event::State(6, 3)
            ]
        );
        assert!(!control.stopped());
        assert!(!control.menus().drawn());
        assert_eq!(control.entry_state(), FrontendEntry::First);
    }
}

#[test]
fn caught_native301_row_keeps_completed_rows_and_clip_without_ack_and_next_frame_runs() {
    let assets = assets();
    let (mut control, scene) = owners(&assets, true);
    let mut host = Recorder::fail_once(Point::Row, Some(0x301));
    host.fail_row_index = 1;
    let before = control.menus().current().unwrap().data().records().items[0].row_height;
    assert_eq!(
        draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).unwrap(),
        FrontendFrameOutcome::RecoveredNative301
    );
    assert!(host.failed_row_seen);
    assert_ne!(
        control.menus().current().unwrap().data().records().items[0].row_height,
        before
    );
    let failed = host
        .events
        .iter()
        .position(|e| *e == Event::Row(1))
        .unwrap();
    assert!(!host.events[failed + 1..]
        .iter()
        .any(|e| matches!(e, Event::Clip(_))));
    assert!(!control.stopped());
    assert!(!control.menus().drawn());
    host.renderers = VecDeque::from([11, 12, 13, 21, 31, 41]);
    assert!(matches!(
        draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).unwrap(),
        FrontendFrameOutcome::Drawn { menu: Some(_), .. }
    ));
    assert!(control.menus().drawn());
}

#[test]
fn other_native_and_external_errors_escape_without_disposal_or_tail() {
    let assets = assets();
    for (point, code) in [
        (Point::Validate, Some(0x711)),
        (Point::Model, None),
        (Point::Row, Some(0x302)),
    ] {
        let (mut control, scene) = owners(&assets, true);
        let mut host = Recorder::fail_once(point, code);
        assert!(draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).is_err());
        assert!(!host
            .events
            .iter()
            .any(|e| matches!(e, Event::Dispose(_) | Event::State(..))));
        if point == Point::Row {
            assert!(control.stopped());
            assert!(matches!(host.events.last(), Some(Event::Clip(_))));
            host.events.clear();
            assert!(draw_frontend_frame(&mut control, &scene, input(&assets), &mut host).is_err());
            assert!(host.events.is_empty());
        }
    }
}

#[test]
fn preguard_and_tail_native301_are_not_caught_and_successful_draw_ack_survives_tail_error() {
    let assets = assets();
    for point in [Point::Arena, Point::Load4] {
        let (mut control, scene) = owners(&assets, true);
        let mut host = Recorder::fail_once(point, Some(0x301));
        host.h4 = FrameResourceState::Vacant;
        assert!(matches!(
            draw_frontend_frame(&mut control, &scene, input(&assets), &mut host),
            Err(FrontendFrameError::Host(Fault {
                code: Some(0x301),
                ..
            }))
        ));
        assert!(!host.events.iter().any(|e| matches!(e, Event::Dispose(_))));
        assert!(!host.events.contains(&Event::State(6, 3)));
        assert_eq!(control.menus().drawn(), point == Point::Load4);
        assert_eq!(
            control.entry_state(),
            if point == Point::Load4 {
                FrontendEntry::Returning
            } else {
                FrontendEntry::First
            }
        );
    }
}

#[test]
fn missing_unconditional_tail_slot_fails_after_draw_without_trying_second_pool() {
    let assets = assets();
    let (mut control, scene) = owners(&assets, true);
    let mut host = Recorder::new();
    host.h4 = FrameResourceState::OutOfRange;
    assert!(matches!(
        draw_frontend_frame(&mut control, &scene, input(&assets), &mut host),
        Err(FrontendFrameError::RequiredResource {
            handler: 4,
            slot: 0
        })
    ));
    assert_eq!(host.events.last(), Some(&Event::State(4, 0)));
    assert!(control.menus().drawn());
}

impl crate::frontend_screen::FrontendFactoryHost for Recorder {
    type Error = &'static str;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::Effect("pause"));
        Ok(())
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::Effect("resume"));
        Ok(())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        _: &mut crate::input::InputRuntime,
    ) -> Result<Vec<crate::frontend_bindings::FrontendDevice>, Self::Error> {
        // Explicit recording fixture with no eligible logical devices. This
        // does not claim that a platform enumerator has been implemented.
        self.events.push(Event::Effect("enumerate-empty-fixture"));
        Ok(Vec::new())
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::Effect("factory-persist"));
        Ok(())
    }
}

impl crate::frontend_dispatch::FrontendEffects for Recorder {
    type Error = &'static str;
    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::Effect("music"));
        Ok(())
    }
    fn play_sound(&mut self, _: i16) -> Result<(), Self::Error> {
        Err("unexpected sound")
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        Err("unexpected tick persistence")
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        Err("unexpected profile transition")
    }
    fn profile_operation_failed(
        &mut self,
        _: crate::profile_menu::ProfileOperation,
        _: &crate::profile_store::ProfileStoreError,
    ) -> Result<(), Self::Error> {
        Err("unexpected profile operation")
    }
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        Err("unsupported network menu")
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        Err("unexpected world refresh")
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        Err("unexpected restore defaults")
    }
    fn apply_volume(&mut self, _: crate::settings_row::VolumeChannel) -> Result<(), Self::Error> {
        Err("unexpected volume apply")
    }
    fn adjust_setting(
        &mut self,
        _: u32,
        _: crate::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected settings adjust")
    }
    fn adjust_display_setting(
        &mut self,
        _: u32,
        _: crate::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected display adjust")
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("unexpected phase")
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        if enabled {
            return Err("unexpected attract start");
        }
        self.events.push(Event::Effect("clear-attract"));
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        Err("unexpected exit")
    }
    fn external_action(
        &mut self,
        _: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        Err("unsupported action")
    }
}

#[test]
fn retail_screen_caught301_allows_actual_tick_and_next_frame_but_fatal_error_stops_owner() {
    use crate::frontend_assets::FrontendMenuCatalog;
    use crate::frontend_dispatch::FrontendDispatch;
    use crate::frontend_screen::{
        FrontendFactoryServices, FrontendProcessState, FrontendScreen, FrontendScreenConfig,
        FrontendScreenDraw,
    };
    use crate::input::InputRuntime;
    use crate::menu_basis::MenuBasisMath;
    use crate::scheduler::SchedulerTick;
    use crate::screen_registry::ScreenLifecycle;
    use crate::screen_registry::{ScreenAttachment, ScreenRegistry};
    use crate::{Installation, ResidencyRequest, ResidentRuntime};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut catalog = FrontendMenuCatalog::new(FrontendAssets::open(&root).unwrap());
    let sine = catalog.assets().sine_table().values;
    let basis = MenuBasisMath::new(sine);
    let scene = FrontendScene::new(
        catalog.assets().scene_models().to_vec(),
        catalog.assets().position_scale(),
        sine,
    );
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    let mut registry = ScreenRegistry::new();
    let attachment = ScreenAttachment {
        session: registry.register_session(),
        context_owner: registry.register_context(()),
    };
    let mut input = InputRuntime::default();
    let process = FrontendProcessState::default();
    let mut screen = FrontendScreen::new(
        FrontendScreenConfig {
            attachment,
            callback_context: 99,
            native_context_high_byte: Some(0xab),
        },
        catalog.assets(),
    );
    let mut host = Recorder::fail_once(Point::Model, Some(0x301));
    screen
        .initialize(
            &mut FrontendFactoryServices {
                registry: &mut registry,
                input: &mut input,
                residency: &mut residency,
                catalog: &mut catalog,
                process: &process,
            },
            &mut host,
        )
        .unwrap();
    let draw = || FrontendScreenDraw {
        scene: &scene,
        visibility: MenuVisibility::OwnerFlagClear,
        sine_quarter: &sine,
    };
    assert_eq!(
        screen.draw_frame(draw(), &mut host).unwrap(),
        FrontendFrameOutcome::RecoveredNative301
    );
    assert_eq!(screen.phase(), ScreenLifecycle::Ready);
    screen
        .tick(
            &mut input,
            SchedulerTick {
                delta_micros: 16000,
                elapsed_micros: 16000,
            },
            &mut FrontendDispatch {
                catalog: &mut catalog,
                basis: &basis,
                visibility: MenuVisibility::OwnerFlagClear,
                effects: &mut host,
                profiles: None,
            },
        )
        .unwrap();
    assert_eq!(screen.screen_input().age_millis(), 16);
    assert!(host.events.contains(&Event::Effect("music")));
    host.renderers = VecDeque::from([11, 12, 13, 21, 31, 41]);
    assert!(matches!(
        screen.draw_frame(draw(), &mut host).unwrap(),
        FrontendFrameOutcome::Drawn {
            scene: SceneDrawOutcome {
                submitted_models: 9,
                ..
            },
            ..
        }
    ));
    assert_eq!(screen.phase(), ScreenLifecycle::Ready);
    host.renderers = VecDeque::from([11, 12, 13, 21, 31, 41]);
    host.failure = Some(Fault {
        point: Point::Validate,
        code: Some(0x711),
    });
    assert!(screen.draw_frame(draw(), &mut host).is_err());
    assert_eq!(screen.phase(), ScreenLifecycle::Stopped);
    let effects = host.events.len();
    assert!(screen.draw_frame(draw(), &mut host).is_err());
    assert!(screen
        .tick(
            &mut input,
            SchedulerTick {
                delta_micros: 16000,
                elapsed_micros: 16000
            },
            &mut FrontendDispatch {
                catalog: &mut catalog,
                basis: &basis,
                visibility: MenuVisibility::OwnerFlagClear,
                effects: &mut host,
                profiles: None,
            }
        )
        .is_err());
    assert_eq!(host.events.len(), effects);
    screen.destroy(&mut registry, &mut input).unwrap();
}
