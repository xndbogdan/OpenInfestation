use super::*;
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_runtime::{read_identity, FrontendRuntimeError, SettingsTextEntryPlatform};
use crate::frontend_scene::SceneProjection;
use crate::frontend_settings::FrontendSettings;
use crate::object_runtime::{
    ActorHandle, ActorRegistryConfig, ObjectClassKind, ObjectId, ObjectList, ObjectNode,
    ObjectState, PlayerId, PlayerMessageOperation, PlayerMessageRequest, WorldPlayer,
};
use crate::platform_input::SdlInput;
use crate::player_identity::PlayerIdentity;
use crate::session_platform::linked::TextEntryPlatform;
use crate::world_mission::WorldMissionInputs;
use inf_formats::{FrontendExecutable, MissionSaveState};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Skip(IntroResultsSkippedFeature),
    PlayerName,
    Enumerate,
    Request(SessionPhase, SessionPhase),
    Save,
    ProfileLoan,
    ProfileFailure(IntroResultsProfileOperation, String),
    Identity {
        target: IdentityTarget,
        name: Vec<u8>,
        initials: Option<Vec<u8>>,
    },
}

struct IdentityOwners {
    settings: FrontendSettings,
    identity: PlayerIdentity,
    exit_requested: bool,
}

#[derive(Default)]
struct Host {
    loading_mode: std::cell::Cell<u8>,
    calls: Vec<Call>,
    keyboard: Option<SdlInput>,
    fail_enumeration: bool,
    fail_request: bool,
    fail_skip: Option<IntroResultsSkippedFeature>,
    fail_name: bool,
    profiles: Option<ProfileStore>,
    identity: Option<IdentityOwners>,
    /// What the tick's save answers.
    save_result: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DrawCall {
    Renderer(u8),
    Arena(u8, usize),
    Batch(u8),
    Publish(u8),
    Width(u32),
}

struct DrawRecorder {
    calls: Vec<DrawCall>,
    fail_at: Option<usize>,
    renderer_reads: u8,
    width_reads: usize,
    widths: [u32; 2],
    arena: Option<(u8, usize)>,
    batch: Option<u8>,
    published: Option<u8>,
}

impl Default for DrawRecorder {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            fail_at: None,
            renderer_reads: 0,
            width_reads: 0,
            widths: [640, 1024],
            arena: None,
            batch: None,
            published: None,
        }
    }
}

impl DrawRecorder {
    fn record(&mut self, call: DrawCall) -> Result<(), &'static str> {
        let index = self.calls.len();
        self.calls.push(call);
        if self.fail_at == Some(index) {
            return Err("required results draw operation failed");
        }
        Ok(())
    }
}

impl ScreenRendererPreparationHost<&'static str> for DrawRecorder {
    type Renderer = u8;

    fn renderer(&mut self) -> Result<Self::Renderer, &'static str> {
        self.renderer_reads += 1;
        let renderer = self.renderer_reads;
        self.record(DrawCall::Renderer(renderer))?;
        Ok(renderer)
    }

    fn replace_command_arena(
        &mut self,
        renderer: Self::Renderer,
        bytes: usize,
    ) -> Result<(), &'static str> {
        self.record(DrawCall::Arena(renderer, bytes))?;
        self.arena = Some((renderer, bytes));
        Ok(())
    }

    fn begin_batch(&mut self, renderer: Self::Renderer) -> Result<(), &'static str> {
        self.record(DrawCall::Batch(renderer))?;
        self.batch = Some(renderer);
        Ok(())
    }

    fn publish_current_renderer(&mut self, renderer: Self::Renderer) -> Result<(), &'static str> {
        self.record(DrawCall::Publish(renderer))?;
        self.published = Some(renderer);
        Ok(())
    }
}

impl IntroResultsDrawHost<&'static str> for DrawRecorder {
    fn surface_width_pixels(&mut self) -> Result<u32, &'static str> {
        let width = self.widths[self.width_reads];
        self.width_reads += 1;
        self.record(DrawCall::Width(width))?;
        Ok(width)
    }
}

fn draw_protocol() -> [DrawCall; 8] {
    [
        DrawCall::Renderer(1),
        DrawCall::Arena(1, 0xc8000),
        DrawCall::Renderer(2),
        DrawCall::Batch(2),
        DrawCall::Renderer(3),
        DrawCall::Publish(3),
        DrawCall::Width(640),
        DrawCall::Width(1024),
    ]
}

impl IntroResultsFactoryHost for Host {
    fn reset_loading_mode_42ac48(&mut self) {
        self.loading_mode.set(0);
    }
    type Error = &'static str;
    type IdentityError = FrontendRuntimeError;
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError> {
        self.calls.push(Call::Identity {
            target: request.target,
            name: request.name.to_vec(),
            initials: match request.initials {
                IdentityInitialsWrite::Keep => None,
                IdentityInitialsWrite::Write(bytes) => Some(bytes.to_vec()),
            },
        });
        let owners = self
            .identity
            .as_mut()
            .ok_or(FrontendRuntimeError::Unsupported(
                "results test identity backing",
            ))?;
        TextEntryPlatform::persist_identity(
            &mut SettingsTextEntryPlatform {
                settings: &mut owners.settings,
                identity: &mut owners.identity,
                exit_requested: &mut owners.exit_requested,
            },
            request,
        )
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.calls.push(Call::Enumerate);
        assert_eq!(self.loading_mode.get(), 0, "42AC48 precedes enumeration");
        if self.fail_enumeration {
            return Err("device enumeration unavailable");
        }
        self.keyboard = Some(SdlInput::attach(input).unwrap());
        Ok(vec![self.keyboard.as_ref().unwrap().frontend_device()])
    }
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error> {
        self.calls.push(Call::Skip(feature));
        if self.fail_skip == Some(feature) {
            return Err("declared results feature boundary");
        }
        Ok(())
    }
    fn player_name(&mut self) -> Result<Vec<u8>, String> {
        self.calls.push(Call::PlayerName);
        if self.fail_name {
            return Err("player name unavailable".into());
        }
        Ok(b"Tester".to_vec())
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        self.calls.push(Call::ProfileLoan);
        self.profiles.as_mut()
    }
    fn report_profile_failure(
        &mut self,
        operation: IntroResultsProfileOperation,
        error: &dyn std::error::Error,
    ) {
        self.calls
            .push(Call::ProfileFailure(operation, error.to_string()));
    }
}

impl IntroResultsTickHost for Host {
    type Error = &'static str;
    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        self.calls.push(Call::Request(previous, requested));
        if self.fail_request {
            return Err("phase request unavailable");
        }
        Ok(())
    }
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error> {
        self.calls.push(Call::Skip(feature));
        Ok(())
    }
    fn save_profile(&mut self, _: Option<&mut ProfileStore>) -> Result<bool, Self::Error> {
        self.calls.push(Call::Save);
        Ok(self.save_result)
    }
}

/// Menu services without a save owner, under gameplay byte +5C2.
fn no_menu(input: &mut InputRuntime) -> IntroResultsMenuServices<'_> {
    IntroResultsMenuServices {
        profiles: None,
        owner_byte: true,
        input,
    }
}

fn assets() -> FrontendAssets {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    FrontendAssets::open(&root).expect("retail Infest.exe")
}

fn mission(mission_selection: u32) -> WorldMission {
    let mut save = MissionSaveState::new_game();
    save.mission_selection = mission_selection;
    WorldMission::new(
        save,
        WorldMissionInputs {
            current_world_a0: 22,
            dword_1c: 0x2800,
            byte_13: None,
            record_0c: None,
        },
    )
}

fn classification(assets: &FrontendAssets) -> ProfileNameClassification {
    ProfileNameClassification::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap()
}

fn assert_fresh_results_menu(screen: &IntroResultsScreen) {
    let menu = screen.menus.as_ref().expect("reached 0042AB9D constructor");
    assert_eq!(
        menu.spec(),
        MenuControllerSpec {
            fade_rate: 65535,
            viewport: [0, 0, 1024, 1024],
        }
    );
    assert!(menu.is_empty());
    assert_eq!(menu.fade(), 0);
    assert!(menu.active());
    assert!(!menu.drawn());
}

fn attachment(
    registry: &mut ScreenRegistry<FrontendRenderContext, InputContextId>,
) -> ScreenAttachment {
    ScreenAttachment {
        session: registry.register_session(),
        context_owner: registry.register_context(FrontendRenderContext {
            projection: SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [0, 0],
            },
        }),
    }
}

#[test]
fn real_phase8_lifecycle_binds_inputs_then_first_tick_falls_through_before_later_boundary() {
    let assets = assets();
    let mut registry = ScreenRegistry::new();
    let attachment = attachment(&mut registry);
    let mut input = InputRuntime::new(0);
    let mut screen = IntroResultsScreen::new(
        IntroResultsScreenConfig {
            attachment,
            callback_context: INTRO_RESULTS_CALLBACK_CONTEXT,
        },
        &assets,
    );
    let mut state = SessionState::default();
    assert!(screen.menus.is_none());
    state.request_phase(SessionPhase::Phase8);
    let mut host = Host::default();
    let mut arena = ObjectArena::new();
    let mut mission = mission(18);
    let classification = classification(&assets);
    screen
        .initialize(
            &mut IntroResultsFactoryServices {
                registry: &mut registry,
                input: &mut input,
                assets: &assets,
                arena: &mut arena,
                mission: &mut mission,
                process: None,
                classification: &classification,
                profile_templates: &ProfileMenuTemplates::parse(assets.executable_bytes()).unwrap(),
            },
            &mut host,
            &mut state,
        )
        .unwrap();
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Ready);
    assert_fresh_results_menu(&screen);
    let id = screen.id().unwrap();
    assert_eq!(
        registry.screen(id).unwrap().membership,
        ScreenMembership::Linked
    );
    assert_eq!(
        host.calls,
        [
            Call::Enumerate,
            Call::Skip(IntroResultsSkippedFeature::ConstructorSaveModeUnavailable),
        ]
    );
    // No phase submission during construction; phase8 really owns a tick.
    let tick = SchedulerTick {
        delta_micros: 0,
        elapsed_micros: 0,
    };
    let process = FrontendProcessState::default();
    screen
        .tick(
            tick,
            &mut IntroResultsTickServices {
                menu: no_menu(&mut input),
                process: &process,
                mission: &mission,
                attract: false,
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(screen.ticks(), 1);
    assert_eq!(
        &host.calls[2..],
        [Call::Request(SessionPhase::Phase8, SessionPhase::Phase3)]
    );
    screen
        .tick(
            tick,
            &mut IntroResultsTickServices {
                menu: no_menu(&mut input),
                process: &process,
                mission: &mission,
                attract: true,
            },
            &mut host,
        )
        .unwrap();
    // Attract asks for the briefing again; the rest of the tick runs.
    assert_eq!(
        host.calls.last(),
        Some(&Call::Request(SessionPhase::Phase8, SessionPhase::Phase3))
    );
    let mut draw_host = DrawRecorder::default();
    screen
        .draw(
            IntroResultsDrawRequest {
                attract: false,
                mission: Some(&mission),
            },
            &mut draw_host,
        )
        .unwrap();
    assert_eq!(draw_host.calls, draw_protocol());
    // F7=0 bypasses the menu tick even though the first fallthrough completed.
    assert_fresh_results_menu(&screen);
    screen
        .destroy::<_, std::convert::Infallible>(&mut registry, &mut input)
        .unwrap();
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert!(screen.menus.is_none());
    assert!(registry.screen(id).is_err());
    screen
        .destroy::<_, std::convert::Infallible>(&mut registry, &mut input)
        .unwrap();
}

/// Every world builds the screen. The final world (selection 1) reads the
/// player's name (00463550) for its page's credits (+F5, +F3).
#[test]
fn every_world_constructs_and_the_final_world_reads_the_players_name() {
    for mission_selection in [1, 0, 2, 18, 0x101, u32::MAX] {
        let mut fixture = Fixture::new();
        fixture.mission.current_world_a0 = 18;
        fixture.initialize(None, mission_selection).unwrap();
        assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
        let last = mission_selection == 1;
        assert_eq!(fixture.host.calls.contains(&Call::PlayerName), last);
        let credits = fixture.screen.credits();
        assert_eq!(credits.is_some(), last);
        if let Some(credits) = credits {
            assert_eq!(credits.lines.len(), 143);
            assert_eq!(credits.player_name, b"Tester");
        }
    }
}

#[test]
fn first_tick_uses_raw_and_capped_age_then_the_same_animation_owner_without_save() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    let custody = fixture.object_custody();
    let mission_before = format!("{:?}", fixture.mission);
    let process = FrontendProcessState::default();
    process.advance_animation(20_000);
    let constructor_calls = fixture.host.calls.len();
    assert!(fixture.host.profiles.is_none());
    fixture
        .screen
        .tick(
            SchedulerTick {
                delta_micros: 125_000,
                elapsed_micros: 250_000,
            },
            &mut IntroResultsTickServices {
                menu: no_menu(&mut fixture.input),
                process: &process,
                mission: &fixture.mission,
                attract: false,
            },
            &mut fixture.host,
        )
        .unwrap();
    assert_eq!(fixture.screen.input().age_millis(), 375);
    assert_eq!(fixture.screen.elapsed_millis_14c(), 125);
    assert_eq!(fixture.screen.input().held_mask(), 0);
    assert_eq!(fixture.screen.input().repeat_millis(), 0);
    assert!(!fixture.screen.intro_delay_elapsed());
    assert_eq!(process.animation().accumulated_micros(), 145_000);
    assert_eq!(process.animation().value(), 7);
    assert_eq!(
        &fixture.host.calls[constructor_calls..],
        [Call::Request(SessionPhase::Phase8, SessionPhase::Phase3)]
    );
    assert_eq!(fixture.object_custody(), custody);
    assert_eq!(format!("{:?}", fixture.mission), mission_before);
    assert_fresh_results_menu(&fixture.screen);
    // Completion progress, rather than an observational count, closes this
    // lane. Later passes request first and reach the existing named boundary.
    fixture
        .screen
        .tick(
            SchedulerTick {
                delta_micros: 125_000,
                elapsed_micros: 250_000,
            },
            &mut IntroResultsTickServices {
                menu: no_menu(&mut fixture.input),
                process: &process,
                mission: &fixture.mission,
                attract: true,
            },
            &mut fixture.host,
        )
        .unwrap();
    assert_eq!(fixture.screen.ticks(), 2);
    assert_eq!(fixture.screen.input().age_millis(), 750);
    assert_eq!(fixture.screen.elapsed_millis_14c(), 250);
    assert_eq!(process.animation().accumulated_micros(), 270_000);
    assert_eq!(
        fixture.host.calls.last(),
        Some(&Call::Request(SessionPhase::Phase8, SessionPhase::Phase3))
    );
}

#[test]
fn first_tick_delay_is_strict_signed_after_both_age_additions() {
    for (raw, capped, age, crossed) in [
        (3_875_000, 125_000, 4000, false),
        (3_876_000, 125_000, 4001, true),
        (-3_876_000, 125_000, -3751, false),
        (i32::MAX, i32::MIN, 0, false),
    ] {
        let mut fixture = Fixture::new();
        fixture.initialize(None, 18).unwrap();
        let process = FrontendProcessState::default();
        fixture
            .screen
            .tick(
                SchedulerTick {
                    delta_micros: capped,
                    elapsed_micros: raw,
                },
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &process,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host,
            )
            .unwrap();
        assert_eq!(fixture.screen.input().age_millis(), age);
        assert_eq!(fixture.screen.intro_delay_elapsed(), crossed);
        assert_eq!(fixture.screen.elapsed_millis_14c(), capped / 1000);
        //439560 consumes the original DWORD, unlike the signed age divisions.
        assert_eq!(
            process.animation().accumulated_micros(),
            u64::from(capped as u32)
        );
    }
}

#[test]
fn head_failure_precedes_age_analog_and_animation_and_cannot_replay() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    fixture
        .screen
        .bindings
        .release_sets(&mut fixture.input)
        .unwrap();
    let d0 = fixture.screen.bindings.consumer_d0().unwrap();
    fixture.input.release_analog_consumer(d0).unwrap();
    assert!(fixture.input.analog_value(d0).is_err());
    fixture.host.fail_request = true;
    let process = FrontendProcessState::default();
    let tick = SchedulerTick {
        delta_micros: 125_000,
        elapsed_micros: 250_000,
    };
    assert!(matches!(
        fixture.screen.tick(
            tick,
            &mut IntroResultsTickServices {
                menu: no_menu(&mut fixture.input),
                process: &process,
                mission: &fixture.mission,
                attract: false,
            },
            &mut fixture.host
        ),
        Err(IntroResultsScreenError::Host("phase request unavailable"))
    ));
    assert_eq!(fixture.screen.ticks(), 1);
    assert_eq!(fixture.screen.input().age_millis(), 0);
    assert_eq!(fixture.screen.elapsed_millis_14c(), 0);
    assert_eq!(process.animation().accumulated_micros(), 0);
    let calls = fixture.host.calls.len();
    fixture.host.fail_request = false;
    assert!(matches!(
        fixture.screen.tick(
            tick,
            &mut IntroResultsTickServices {
                menu: no_menu(&mut fixture.input),
                process: &process,
                mission: &fixture.mission,
                attract: false,
            },
            &mut fixture.host
        ),
        Err(IntroResultsScreenError::TickStopped)
    ));
    assert_eq!(fixture.host.calls.len(), calls);
    assert_eq!(fixture.screen.ticks(), 1);
}

#[test]
fn retired_live_analog_retains_head_and_raw_age_before_later_time_stores() {
    for retire_d0 in [true, false] {
        let mut fixture = Fixture::new();
        fixture.initialize(None, 18).unwrap();
        fixture
            .screen
            .bindings
            .release_sets(&mut fixture.input)
            .unwrap();
        let retired = if retire_d0 {
            fixture.screen.bindings.consumer_d0().unwrap()
        } else {
            fixture.screen.bindings.consumer_cc().unwrap()
        };
        fixture.input.release_analog_consumer(retired).unwrap();
        assert!(fixture.input.analog_value(retired).is_err());
        let process = FrontendProcessState::default();
        let before = fixture.host.calls.len();
        let tick = SchedulerTick {
            delta_micros: 125_000,
            elapsed_micros: 250_000,
        };
        assert!(matches!(
            fixture.screen.tick(
                tick,
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &process,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host
            ),
            Err(IntroResultsScreenError::Bindings(
                FrontendBindingError::Input(_)
            ))
        ));
        assert_eq!(
            &fixture.host.calls[before..],
            [Call::Request(SessionPhase::Phase8, SessionPhase::Phase3)]
        );
        assert_eq!(fixture.screen.input().age_millis(), 250);
        assert_eq!(fixture.screen.elapsed_millis_14c(), 0);
        assert_eq!(process.animation().accumulated_micros(), 0);
        assert!(matches!(
            fixture.screen.tick(
                tick,
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &process,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host
            ),
            Err(IntroResultsScreenError::TickStopped)
        ));
        assert_eq!(fixture.screen.input().age_millis(), 250);
        assert_fresh_results_menu(&fixture.screen);
    }
}

/// After a real world (here 18, on to 2), as in Infest02: Space (0042AA60)
/// shows the stats, whose three panels come up as the age restarts at
/// 4000 ms; S (0042AAA0) starts the save countdown, which saves past
/// 1000 ms and gates the next held pass (+150), so the first Esc after it is
/// swallowed; the next Esc (held bit 0x200, 0042D510) arms the 1000 ms exit,
/// which drops the panels and asks for the briefing.
#[test]
fn results_after_a_world_show_the_stats_save_and_continue_to_the_briefing() {
    let mut fixture = Fixture::new();
    fixture.mission.current_world_a0 = 18;
    fixture.initialize(None, 2).unwrap();
    fixture.host.save_result = true;
    let process = FrontendProcessState::default();
    let tick = SchedulerTick {
        delta_micros: 125_000,
        elapsed_micros: 125_000,
    };
    let key = |address, value| crate::input::InputCallback {
        address,
        context: INTRO_RESULTS_CALLBACK_CONTEXT,
        value,
        timestamp_micros: 0,
    };
    let run = |fixture: &mut Fixture| {
        fixture
            .screen
            .tick(
                tick,
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &process,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host,
            )
            .unwrap()
    };
    fixture
        .screen
        .input_callback(key(0x0042_aa60, 1), &mut no_menu(&mut fixture.input))
        .unwrap();
    assert!(fixture.screen.stats_shown());
    run(&mut fixture);
    assert_eq!(fixture.screen.input().age_millis(), 4000);
    assert!(fixture
        .screen
        .panels()
        .iter()
        .all(|panel| panel.is_some_and(|panel| panel.requested_stage == 2)));

    fixture
        .screen
        .input_callback(key(0x0042_aaa0, 1), &mut no_menu(&mut fixture.input))
        .unwrap();
    for _ in 0..7 {
        run(&mut fixture);
    }
    assert!(!fixture.screen.saved());
    run(&mut fixture);
    assert!(fixture.screen.saved());
    assert_eq!(
        fixture
            .host
            .calls
            .iter()
            .filter(|call| **call == Call::Save)
            .count(),
        1
    );

    fixture
        .screen
        .input_callback(key(0x0042_aa30, 1), &mut no_menu(&mut fixture.input))
        .unwrap();
    fixture
        .screen
        .input_callback(key(0x0042_aa30, 0), &mut no_menu(&mut fixture.input))
        .unwrap();
    assert_eq!(fixture.screen.exit_millis(), 0);
    fixture
        .screen
        .input_callback(key(0x0042_aa30, 1), &mut no_menu(&mut fixture.input))
        .unwrap();
    assert_eq!(fixture.screen.exit_millis(), 1000);
    for _ in 0..7 {
        run(&mut fixture);
    }
    assert_eq!(fixture.screen.exit_millis(), 125);
    assert!(!fixture
        .host
        .calls
        .contains(&Call::Request(SessionPhase::Phase8, SessionPhase::Phase3)));
    run(&mut fixture);
    assert_eq!(fixture.screen.exit_millis(), 1);
    assert_eq!(
        fixture.host.calls.last(),
        Some(&Call::Request(SessionPhase::Phase8, SessionPhase::Phase3))
    );
    assert!(fixture
        .screen
        .panels()
        .iter()
        .all(|panel| panel
            .is_some_and(|panel| panel.current_stage == 0 && panel.requested_stage == 0)));
}

/// Menu services over a save owner, under gameplay byte +5C2.
fn with_store<'a>(
    profiles: &'a mut ProfileStore,
    input: &'a mut InputRuntime,
) -> IntroResultsMenuServices<'a> {
    IntroResultsMenuServices {
        profiles: Some(profiles),
        owner_byte: true,
        input,
    }
}

/// A controller draw that only completes the draw handshake the profile
/// menu's tick waits for; the rows' pixels are the software draw's.
struct Handshake<'a>(&'a mut ProfileStore);

impl crate::menu::MenuDrawHost for Handshake<'_> {
    type Error = Infallible;
    fn begin_controller(
        &mut self,
        _: crate::menu::MenuControllerDrawSetup,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), Self::Error> {
        Ok(())
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok([0, 0, 1024, 768])
    }
    fn set_clip_rect(&mut self, _: [i16; 4]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn colors(&self) -> crate::menu::MenuDrawColors {
        crate::menu::MenuDrawColors {
            selected_start: 0,
            ordinary: 0,
            selected_end: 0,
        }
    }
    fn draw_row(
        &mut self,
        _: &mut crate::menu::MenuRowDrawRequest,
        _: &mut crate::menu::MenuState,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn begin_profile_draw(&mut self) -> Result<(), MenuError<Self::Error>> {
        Ok(())
    }
    fn profile_store(&mut self) -> Result<&mut ProfileStore, MenuError<Self::Error>> {
        Ok(self.0)
    }
    fn draw_profile_row(
        &mut self,
        _: &mut crate::menu::MenuRowDrawRequest,
        _: &mut crate::menu::MenuState,
        _: &mut crate::profile_menu::ProfileMenuState,
    ) -> Result<(), MenuError<Self::Error>> {
        Ok(())
    }
    fn draw_profile_status(
        &mut self,
        _: crate::profile_menu::ProfileTimer,
        _: [i32; 2],
    ) -> Result<(), MenuError<Self::Error>> {
        Ok(())
    }
}

fn key(address: u32, value: u32) -> crate::input::InputCallback {
    crate::input::InputCallback {
        address,
        context: INTRO_RESULTS_CALLBACK_CONTEXT,
        value,
        timestamp_micros: 0,
    }
}

impl Fixture {
    fn tick_with(&mut self, profiles: Option<&mut ProfileStore>) {
        let process = FrontendProcessState::default();
        self.screen
            .tick(
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                &mut IntroResultsTickServices {
                    menu: IntroResultsMenuServices {
                        profiles,
                        owner_byte: true,
                        input: &mut self.input,
                    },
                    process: &process,
                    mission: &self.mission,
                    attract: false,
                },
                &mut self.host,
            )
            .unwrap();
    }

    /// One key's press and release, through the held pass.
    fn press(&mut self, profiles: &mut ProfileStore, address: u32) {
        for value in [1, 0] {
            self.screen
                .input_callback(
                    key(address, value),
                    &mut with_store(profiles, &mut self.input),
                )
                .unwrap();
        }
    }

    fn draw_menu(&mut self, profiles: &mut ProfileStore) {
        let sine = self.assets.sine_table().values;
        self.screen
            .shown_menu_mut()
            .unwrap()
            .draw(
                crate::menu::MenuDrawInput {
                    surface_pixels: [1024, 768],
                    visibility: MenuVisibility::OwnerFlagSet,
                    sine_quarter: &sine,
                },
                &mut Handshake(profiles),
            )
            .unwrap();
    }

    fn shown_profile(&self) -> &ProfileMenu {
        match self.screen.menus.as_ref().unwrap().current_owner() {
            Some(MenuOwner::Profile(profile)) => profile,
            other => panic!("expected the profile menu, found {other:?}"),
        }
    }
}

/// A failed save (0042D184) shows the profile menu with its two-second
/// warning (00449A00(true)) and forgets the failure (+F1).
#[test]
fn a_failed_save_shows_the_profile_menu_with_its_warning() {
    let mut fixture = Fixture::new();
    fixture.mission.current_world_a0 = 18;
    fixture.initialize(None, 2).unwrap();
    fixture
        .screen
        .input_callback(key(SAVE, 1), &mut no_menu(&mut fixture.input))
        .unwrap();
    // The eighth tick passes 1000 ms and fails the save; the gate +150
    // holds until the next tick's held pass.
    for _ in 0..8 {
        fixture.tick_with(None);
    }
    assert!(!fixture.screen.saved());
    assert!(fixture.screen.menu_shown());
    assert!(!fixture.screen.flags.failed_f1);
    assert!(fixture.screen.flags.gate_150);
    assert_eq!(
        fixture.shown_profile().state().timer(),
        Some(crate::profile_menu::ProfileTimer {
            kind: crate::profile_menu::ProfileTimerKind::InitialWarning,
            millis: 2000,
        })
    );
}

/// Backspace (0042D3F0) shows the menu; Next and Enter pick the empty row,
/// which with +5C2 set selects that slot and fades the menu (operation 6).
/// Once the fade ends, the status query (00449C90) restarts the save
/// countdown, and the save goes to the slot picked.
#[test]
fn the_profile_menu_saves_into_the_slot_the_player_picks() {
    let files = ProfileFiles::new();
    let mut store = files.open(b"Picked");
    let mut fixture = Fixture::new();
    fixture.mission.current_world_a0 = 18;
    fixture.initialize(None, 2).unwrap();
    fixture.host.save_result = true;
    fixture.press(&mut store, 0x0042_a9d0);
    assert!(fixture.screen.menu_shown());
    assert_eq!(fixture.shown_profile().state().timer(), None);
    // The fade comes in, then the menu waits for its first draw.
    fixture.tick_with(Some(&mut store));
    fixture.draw_menu(&mut store);
    fixture.tick_with(Some(&mut store));
    assert_eq!(fixture.shown_profile().base().selection(), 1);
    fixture.press(&mut store, 0x0044_5240);
    assert_eq!(fixture.shown_profile().base().selection(), 2);
    fixture.press(&mut store, 0x0044_5200);
    assert_eq!(
        fixture.shown_profile().state().pending(),
        Some(crate::profile_menu::ProfileOperation::SelectAndFade)
    );
    fixture.draw_menu(&mut store);
    fixture.tick_with(Some(&mut store));
    assert_eq!(store.selected(), 1);
    assert!(!fixture.screen.menus.as_ref().unwrap().active());
    assert!(fixture.screen.menu_shown());
    fixture.tick_with(Some(&mut store));
    assert!(!fixture.screen.menu_shown());
    assert!(fixture.screen.menus.as_ref().unwrap().is_empty());
    // +EC = 1, then the same tick's countdown adds its 125 ms.
    assert_eq!(fixture.screen.save_countdown_millis(), 126);
    for _ in 0..6 {
        fixture.tick_with(Some(&mut store));
    }
    assert!(!fixture.screen.saved());
    fixture.tick_with(Some(&mut store));
    assert!(fixture.screen.saved());
    assert_eq!(
        fixture
            .host
            .calls
            .iter()
            .filter(|call| **call == Call::Save)
            .count(),
        1
    );
}

/// Esc while the menu shows (0042D510) hides it at once and bars the save
/// (+F8); S then finds the save impossible, which shows the menu again
/// without the warning, over the hidden one.
#[test]
fn escape_hides_the_profile_menu_and_bars_the_save() {
    let files = ProfileFiles::new();
    let mut store = files.open(b"Hidden");
    let mut fixture = Fixture::new();
    fixture.mission.current_world_a0 = 18;
    fixture.initialize(None, 2).unwrap();
    fixture.press(&mut store, 0x0042_a9d0);
    fixture.press(&mut store, 0x0042_aa30);
    assert!(!fixture.screen.menu_shown());
    assert!(fixture.screen.flags.no_save_f8);
    assert!(!fixture.screen.menus.as_ref().unwrap().active());
    assert_eq!(fixture.screen.exit_millis(), 0);
    fixture.press(&mut store, SAVE);
    assert_eq!(fixture.screen.save_countdown_millis(), SAVE_PAST_DUE);
    fixture.tick_with(Some(&mut store));
    assert!(fixture.screen.menu_shown());
    assert!(!fixture.screen.flags.no_save_f8);
    assert_eq!(fixture.screen.menus.as_ref().unwrap().len(), 2);
    assert_eq!(fixture.shown_profile().state().timer(), None);
    assert!(!fixture.host.calls.contains(&Call::Save));
}

#[test]
fn live_d0_direction_bypasses_retired_cc_but_exact_threshold_reaches_it() {
    struct AnalogSetup;
    impl crate::input::InputHost for AnalogSetup {
        type Error = Infallible;
        fn clock_micros(&mut self) -> Result<u64, Self::Error> {
            Ok(0)
        }
        fn callback(
            &mut self,
            _: crate::input::InputCallback,
            _: &mut InputRuntime,
        ) -> Result<(), Self::Error> {
            panic!("original analog descriptor has no callback")
        }
    }
    for (value, held) in [(2501, Some(2)), (-2501, Some(4)), (2500, None)] {
        let mut fixture = Fixture::new();
        fixture.initialize(None, 18).unwrap();
        assert_eq!(
            fixture
                .assets
                .screen_analog_descriptors()
                .consumer_d0
                .callback_address,
            0
        );
        let d0 = fixture.screen.bindings.consumer_d0().unwrap();
        let cc = fixture.screen.bindings.consumer_cc().unwrap();
        fixture
            .screen
            .bindings
            .release_sets(&mut fixture.input)
            .unwrap();
        fixture.input.release_analog_consumer(cc).unwrap();
        assert!(fixture.input.analog_value(cc).is_err());
        // Prepare declared backing through the real input/device producer,
        // not a mutable input setter or an invented Results callback.
        let device = fixture
            .input
            .create_device(crate::input::DeviceButtons::default())
            .unwrap();
        let set = fixture
            .input
            .create_analog_binding_set(
                device,
                crate::input::AnalogPolicy::Replace,
                &[crate::input::AnalogBinding {
                    consumer: d0,
                    axis: 0,
                    transform: crate::input::AxisTransform::NATIVE_DEFAULT,
                }],
            )
            .unwrap();
        fixture
            .input
            .attach_analog_binding_set(fixture.screen.bindings.context().unwrap(), set)
            .unwrap();
        fixture
            .input
            .update_device(
                device,
                crate::input::DeviceSnapshot {
                    axes: [value, 0, 0, 0, 0, 0, 0, 0],
                    ..Default::default()
                },
                &mut AnalogSetup,
            )
            .unwrap();
        fixture.input.poll(&mut AnalogSetup).unwrap();
        assert_eq!(fixture.input.analog_value(d0).unwrap(), value);
        let process = FrontendProcessState::default();
        let result = fixture.screen.tick(
            SchedulerTick {
                delta_micros: 125_000,
                elapsed_micros: 250_000,
            },
            &mut IntroResultsTickServices {
                menu: no_menu(&mut fixture.input),
                process: &process,
                mission: &fixture.mission,
                attract: false,
            },
            &mut fixture.host,
        );
        if let Some(held) = held {
            result.unwrap();
            assert_eq!(fixture.screen.input().held_mask(), held);
            assert_eq!(fixture.screen.input().repeat_millis(), 125);
            assert!(fixture.screen.input().analog_latched());
            assert_eq!(fixture.screen.input().age_millis(), 375);
            assert_eq!(fixture.screen.elapsed_millis_14c(), 125);
            assert_eq!(process.animation().accumulated_micros(), 125_000);
        } else {
            assert!(matches!(
                result,
                Err(IntroResultsScreenError::Bindings(
                    FrontendBindingError::Input(_)
                ))
            ));
            assert_eq!(fixture.screen.input().held_mask(), 0);
            assert_eq!(fixture.screen.input().age_millis(), 250);
            assert_eq!(fixture.screen.elapsed_millis_14c(), 0);
            assert_eq!(process.animation().accumulated_micros(), 0);
        }
        assert_fresh_results_menu(&fixture.screen);
    }
}

struct Fixture {
    assets: FrontendAssets,
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    arena: ObjectArena,
    mission: WorldMission,
    classification: ProfileNameClassification,
    objects: [ObjectId; 2],
    attachment: ScreenAttachment,
    screen: IntroResultsScreen,
    state: SessionState,
    host: Host,
}

impl Fixture {
    fn new() -> Self {
        let assets = assets();
        let classification = classification(&assets);
        let mut registry = ScreenRegistry::new();
        let attachment = attachment(&mut registry);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x0801_0000),
        })
        .unwrap();
        let base = arena.allocate(ObjectClassKind::Base);
        arena.set_flag0(base, true).unwrap();
        let visual = arena.allocate(ObjectClassKind::VisualBase);
        let screen = IntroResultsScreen::new(
            IntroResultsScreenConfig {
                attachment,
                callback_context: INTRO_RESULTS_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Phase8);
        Self {
            assets,
            registry,
            input: InputRuntime::new(0),
            arena,
            mission: mission(18),
            classification,
            objects: [base, visual],
            attachment,
            screen,
            state,
            host: Host::default(),
        }
    }

    fn player(&mut self, word: u16, commands: [u32; 3]) -> PlayerId {
        let player = self.arena.add_world_player([0x1234_5678; 6]);
        let mut saved = inf_formats::PlayerSaveState::new_player([0x1234_5678; 6]);
        saved.owner_11c = word;
        saved.owner_130 = commands;
        self.arena
            .restore_player_save_prefix(player, &saved)
            .unwrap();
        player
    }

    fn initialize(
        &mut self,
        process: Option<&ParticleFactoryProcessState>,
        mission_selection: u32,
    ) -> Result<(), IntroResultsScreenError<&'static str>> {
        self.mission.save.mission_selection = mission_selection;
        self.screen.initialize(
            &mut IntroResultsFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets: &self.assets,
                arena: &mut self.arena,
                mission: &mut self.mission,
                process,
                classification: &self.classification,
                profile_templates: &ProfileMenuTemplates::parse(self.assets.executable_bytes())
                    .unwrap(),
            },
            &mut self.host,
            &mut self.state,
        )
    }

    fn prior_selection(&mut self) -> (ScreenId, InputContextId) {
        let screen = self
            .registry
            .attach_scheduler_member(self.attachment)
            .unwrap();
        self.registry.link_screen_membership(screen).unwrap();
        let context = self.input.create_context().unwrap();
        self.registry
            .bind_input(screen, context, |context| {
                self.input.select_context(context)
            })
            .unwrap();
        (screen, context)
    }

    fn object_custody(&self) -> ObjectCustody {
        ObjectCustody {
            objects: self
                .objects
                .map(|id| self.arena.object(id).unwrap().clone()),
            lists: [
                ObjectList::Flag0Primary,
                ObjectList::Flag0Secondary,
                ObjectList::Flag1Primary,
                ObjectList::Flag1Secondary,
                ObjectList::Parking,
                ObjectList::Disposal,
            ]
            .map(|role| self.arena.list(role).to_vec()),
            registry: format!("{:?}", self.arena.actor_handles().unwrap()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ObjectCustody {
    objects: [ObjectState; 2],
    lists: [Vec<ObjectNode>; 6],
    registry: String,
}

fn process(network_flag: u32) -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    }
}

fn expected_players(fixture: &Fixture, owns_command: bool) -> Vec<WorldPlayer> {
    let mut players = fixture.arena.world_players().to_vec();
    if owns_command {
        players[0].command_mask_130[1] |= 0x0080_0000;
    }
    players
}

#[test]
fn full_dword_mission_gate_follows_devices_and_precedes_player_and_input() {
    for mission_selection in [1, 0, 18, 0x101, 0x8000_0001, u32::MAX] {
        let mut fixture = Fixture::new();
        fixture.player(0x8000, [0x1020_3040, 4, 0x8877_6655]);
        fixture.player(0xffff, [0xffff_ffff; 3]);
        let expected = expected_players(&fixture, mission_selection != 1);
        let before = fixture.object_custody();
        let (previous, previous_context) = fixture.prior_selection();
        fixture.host.fail_name = true;
        let process = process(0);
        let before_process = process;
        let result = fixture.initialize(Some(&process), mission_selection);
        let id = fixture.screen.id().unwrap();
        if mission_selection == 1 {
            assert!(matches!(
                result,
                Err(IntroResultsScreenError::Credits(ref error)) if error == "player name unavailable"
            ));
            assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
            assert_eq!(fixture.screen.player_word_144(), 0);
            assert_eq!(fixture.registry.selected_screen(), Some(previous));
            assert_eq!(fixture.input.selected_context(), Some(previous_context));
            assert_eq!(fixture.registry.screen(id).unwrap().input_context, None);
            assert_eq!(fixture.host.calls, [Call::Enumerate, Call::PlayerName]);
        } else {
            result.unwrap();
            assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
            assert_eq!(fixture.screen.player_word_144(), 0x8000);
            assert_eq!(fixture.registry.selected_screen(), Some(id));
            assert_eq!(
                fixture.input.selected_context(),
                fixture.screen.bindings.context()
            );
            assert_eq!(
                fixture.host.calls,
                [
                    Call::Enumerate,
                    Call::Skip(IntroResultsSkippedFeature::ConstructorSaveModeUnavailable),
                ]
            );
        }
        assert_eq!(
            fixture.screen.bindings.phase(),
            crate::frontend_bindings::FrontendBindingPhase::Installed
        );
        assert_eq!(fixture.arena.world_players(), expected);
        assert_eq!(fixture.object_custody(), before);
        assert_eq!(process, before_process);
    }
}

#[test]
fn empty_player_prefix_keeps_constructor_zero_without_process_then_binds_input() {
    let mut fixture = Fixture::new();
    let before = fixture.object_custody();
    assert_eq!(fixture.screen.player_word_144(), 0);
    fixture.initialize(None, 18).unwrap();
    assert_eq!(fixture.screen.player_word_144(), 0);
    assert!(fixture.arena.world_players().is_empty());
    assert_eq!(fixture.object_custody(), before);
    assert_eq!(
        fixture.input.selected_context(),
        fixture.screen.bindings.context()
    );
    assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
}

#[test]
fn fresh_fifo_player_command_is_silent_and_preserves_second_player_and_other_owners() {
    let mut fixture = Fixture::new();
    let first = fixture.player(0x8000, [0x1020_3040, 4, 0x8877_6655]);
    fixture.player(0xffff, [0xffff_ffff; 3]);
    // A nonsilent accidental message would fail at the actual enabled gate.
    let recipient = fixture.arena.player_message(first).unwrap().identity();
    fixture
        .arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let expected = expected_players(&fixture, true);
    let before = fixture.object_custody();
    let process = process(0);
    let before_process = process;
    fixture.initialize(Some(&process), 18).unwrap();
    assert_eq!(fixture.screen.player_word_144(), 0x8000);
    assert_eq!(fixture.arena.first_player(), Some(first));
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.object_custody(), before);
    assert_eq!(process, before_process);
}

#[test]
fn preowned_and_network100_commands_still_copy_raw_word_without_lower_sources() {
    for (network, commands, word) in [
        (0, [0x1234, 0x0080_0002, 0x9876], 0xffff),
        (0x100, [0x1234, 2, 0x9876], 0x7fff),
    ] {
        let mut fixture = Fixture::new();
        fixture.player(word, commands);
        let expected = fixture.arena.world_players().to_vec();
        let before = fixture.object_custody();
        let process = process(network);
        let before_process = process;
        fixture.initialize(Some(&process), 18).unwrap();
        assert_eq!(fixture.screen.player_word_144(), word);
        assert_eq!(fixture.arena.world_players(), expected);
        assert_eq!(fixture.object_custody(), before);
        assert_eq!(process, before_process);
    }
}

#[test]
fn complete_player_phase_copies_each_word_boundary_without_world_or_phase_input() {
    let mut fixture = Fixture::new();
    let player = fixture.player(0, [0; 3]);
    let process = process(0);
    for word in [0, 0x7fff, 0x8000, 0xffff] {
        let mut saved = inf_formats::PlayerSaveState::new_player([0x1234_5678; 6]);
        saved.owner_11c = word;
        fixture
            .arena
            .restore_player_save_prefix(player, &saved)
            .unwrap();
        let expected = expected_players(&fixture, true);
        assert_eq!(
            initialize_results_player_prefix::<std::convert::Infallible>(
                &mut fixture.arena,
                Some(&process)
            )
            .unwrap(),
            word
        );
        assert_eq!(fixture.arena.world_players(), expected);
    }
}

#[test]
fn missing_process_even_preowned_retains_installed_inputs_and_previous_selection() {
    let mut fixture = Fixture::new();
    let player = fixture.player(0xffff, [0, 0x0080_0000, 0]);
    let expected = fixture.arena.world_players().to_vec();
    let before = fixture.object_custody();
    let (previous, previous_context) = fixture.prior_selection();
    assert!(matches!(
        fixture.initialize(None, 18),
        Err(IntroResultsScreenError::PlayerCommand(
            PlayerCommandError::MissingProcess { player: actual }
        )) if actual == player
    ));
    let id = fixture.screen.id().unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.screen.player_word_144(), 0);
    assert_eq!(
        fixture.screen.bindings.phase(),
        crate::frontend_bindings::FrontendBindingPhase::Installed
    );
    assert!(fixture
        .screen
        .bindings
        .analog_values(&fixture.input)
        .is_ok());
    assert_eq!(
        fixture.registry.screen(id).unwrap().membership,
        ScreenMembership::Linked
    );
    assert_eq!(fixture.registry.screen(id).unwrap().input_context, None);
    assert_eq!(fixture.registry.selected_screen(), Some(previous));
    assert_eq!(fixture.input.selected_context(), Some(previous_context));
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.object_custody(), before);
    assert_eq!(fixture.host.calls, [Call::Enumerate,]);
    fixture
        .screen
        .destroy::<_, std::convert::Infallible>(&mut fixture.registry, &mut fixture.input)
        .unwrap();
    assert!(fixture.screen.menus.is_none());
    assert_eq!(fixture.registry.selected_screen(), Some(previous));
    assert_eq!(fixture.input.selected_context(), Some(previous_context));
}

#[test]
fn device_failure_prevents_identity_player_and_binding_prefix() {
    let mut fixture = Fixture::new();
    fixture.player(0xffff, [0; 3]);
    let expected = fixture.arena.world_players().to_vec();
    let (previous, previous_context) = fixture.prior_selection();
    fixture.host.fail_enumeration = true;
    fixture.host.loading_mode.set(1);
    assert!(matches!(
        fixture.initialize(None, 1),
        Err(IntroResultsScreenError::Host(
            "device enumeration unavailable"
        ))
    ));
    assert_eq!(fixture.screen.player_word_144(), 0);
    assert_eq!(fixture.host.loading_mode.get(), 0);
    assert_eq!(fixture.arena.world_players(), expected);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.registry.selected_screen(), Some(previous));
    assert_eq!(fixture.input.selected_context(), Some(previous_context));
    assert_eq!(
        fixture
            .registry
            .screen(fixture.screen.id().unwrap())
            .unwrap()
            .input_context,
        None
    );
    assert_eq!(fixture.host.calls, [Call::Enumerate,]);
    fixture
        .screen
        .destroy::<_, std::convert::Infallible>(&mut fixture.registry, &mut fixture.input)
        .unwrap();
    assert!(fixture.screen.menus.is_none());
    assert_eq!(fixture.registry.selected_screen(), Some(previous));
    assert_eq!(fixture.input.selected_context(), Some(previous_context));
}

#[test]
fn later_identity_error_retains_owned_menu_and_installed_preplayer_prefix() {
    let mut fixture = Fixture::new();
    fixture.player(0xffff, [0; 3]);
    let expected = fixture.arena.world_players().to_vec();
    let (previous, previous_context) = fixture.prior_selection();
    fixture.host.fail_name = true;
    assert!(matches!(
        fixture.initialize(None, 1),
        Err(IntroResultsScreenError::Credits(ref error)) if error == "player name unavailable"
    ));
    assert_eq!(
        fixture.screen.bindings.phase(),
        crate::frontend_bindings::FrontendBindingPhase::Installed
    );
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.screen.player_word_144(), 0);
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.registry.selected_screen(), Some(previous));
    assert_eq!(fixture.input.selected_context(), Some(previous_context));
    assert_eq!(
        fixture
            .registry
            .screen(fixture.screen.id().unwrap())
            .unwrap()
            .input_context,
        None
    );
    assert_eq!(fixture.host.calls, [Call::Enumerate, Call::PlayerName]);
}

#[test]
fn missing_save_mode_skip_error_retains_completed_command_copy_and_new_selection() {
    let mut fixture = Fixture::new();
    fixture.player(0xffff, [0, 4, 0]);
    let expected = expected_players(&fixture, true);
    fixture.prior_selection();
    fixture.host.fail_skip = Some(IntroResultsSkippedFeature::ConstructorSaveModeUnavailable);
    let process = process(0);
    assert!(matches!(
        fixture.initialize(Some(&process), 18),
        Err(IntroResultsScreenError::Host(
            "declared results feature boundary"
        ))
    ));
    let id = fixture.screen.id().unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.screen.player_word_144(), 0xffff);
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.registry.selected_screen(), Some(id));
    assert_eq!(
        fixture.registry.screen(id).unwrap().input_context,
        fixture.screen.bindings.context()
    );
    assert_eq!(
        fixture.input.selected_context(),
        fixture.screen.bindings.context()
    );
    assert_eq!(
        fixture.host.calls.last(),
        Some(&Call::Skip(
            IntroResultsSkippedFeature::ConstructorSaveModeUnavailable
        ))
    );
}

struct ProfileFiles {
    directory: std::path::PathBuf,
    retail: std::path::PathBuf,
    sidecar: std::path::PathBuf,
}

impl ProfileFiles {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "inf-results-profile-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let retail = directory.join("retail.sav");
        // One save, retail's and the port's alike.
        let sidecar = retail.clone();
        let mut payload = std::array::from_fn(|index| (index as u8).wrapping_mul(19));
        payload[..4].copy_from_slice(b"Old\0");
        let record = inf_formats::SaveRecord::from_payload(payload).unwrap();
        std::fs::write(
            &retail,
            inf_formats::encode_save_records(&[record]).unwrap(),
        )
        .unwrap();
        Self {
            directory,
            retail,
            sidecar,
        }
    }

    fn open(&self, name: &[u8]) -> ProfileStore {
        let mut store = ProfileStore::open(&self.sidecar).unwrap();
        store.load(0).unwrap();
        store.prepare_new_name(name).unwrap();
        store
    }

    fn supply_record(&self, record: &inf_formats::SaveRecord) {
        std::fs::write(
            &self.retail,
            inf_formats::encode_save_records(std::slice::from_ref(record)).unwrap(),
        )
        .unwrap();
    }

    fn identity_owners(&self, assets: &FrontendAssets) -> IdentityOwners {
        let mut settings =
            FrontendSettings::load_or_defaults(self.directory.join("settings.json"), assets)
                .unwrap();
        settings.set_player_name(b"Saved player");
        settings.set_net_player_name(b"Saved network");
        settings.set_player_initials(b"OLD");
        settings.persist().unwrap();
        let mut identity = PlayerIdentity::default();
        read_identity(&mut identity, &settings, SessionPhase::Phase9, 1).unwrap();
        settings.set_player_initials(b"LIVE");
        IdentityOwners {
            settings,
            identity,
            exit_requested: true,
        }
    }
}

impl Drop for ProfileFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn unknown_save_mode_skips_without_borrowing_the_available_store() {
    let mut fixture = Fixture::new();
    fixture.mission.bytes_14_16 = None;
    let mission_before = fixture.mission.clone();
    let files = ProfileFiles::new();
    fixture.host.profiles = Some(files.open(b"Current"));
    let store = fixture.host.profiles.as_ref().unwrap();
    let working_before = store.working_record().cloned();
    let selected_before = store.selected();
    let disk_before = std::fs::read(&files.sidecar).unwrap();
    fixture.initialize(None, 18).unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(
        fixture.host.calls,
        [
            Call::Enumerate,
            Call::Skip(IntroResultsSkippedFeature::ConstructorSaveModeUnavailable),
        ]
    );
    let store = fixture.host.profiles.as_ref().unwrap();
    assert_eq!(store.working_record(), working_before.as_ref());
    assert_eq!(store.current_name(), Some(b"Current".as_slice()));
    assert_eq!(store.selected(), selected_before);
    assert!(!store.write_attempted());
    assert_eq!(std::fs::read(&files.sidecar).unwrap(), disk_before);
    assert_eq!(fixture.mission, mission_before);
}

#[test]
fn all_nonzero_save_modes_without_a_store_skip_after_the_bound_player_prefix() {
    for mode in [1, 0x80, 0xff] {
        let mut fixture = Fixture::new();
        fixture.mission.bytes_14_16 = Some([0x31, mode, 0x73]);
        fixture.player(0x8000, [0x2468, 4, 0xfedc_ba98]);
        let expected = expected_players(&fixture, true);
        let process = process(0);
        fixture.initialize(Some(&process), 18).unwrap();
        assert_eq!(fixture.arena.world_players(), expected);
        assert_eq!(fixture.screen.player_word_144(), 0x8000);
        assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
        assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
        assert_eq!(
            fixture.host.calls,
            [
                Call::Enumerate,
                Call::ProfileLoan,
                Call::Skip(IntroResultsSkippedFeature::ConstructorProfileStoreUnavailable),
            ]
        );
    }
}

#[test]
fn nonzero_restore_with_no_player_still_persists_the_completed_name_and_keeps_initials() {
    for mode in [1, 0x80, 0xff] {
        let mut fixture = Fixture::new();
        fixture.mission.bytes_14_16 = Some([0x31, mode, 0x73]);
        let mut saved_mission = MissionSaveState::new_game();
        // A%%B emits cursor4. Its raw NUL overlaps the first mission byte;
        // zero here preserves both the genuine FORMAT and emitted cursor.
        saved_mission.mission_selection = 0x8000_0100;
        saved_mission.elapsed_seconds = 731;
        saved_mission.owner_b0[19] = 0xf123_4567;
        let saved_player = inf_formats::PlayerSaveState::new_player([0x7654_3210; 6]);
        let emitted =
            inf_formats::SaveRecord::from_state(b"A%B", &saved_mission, &saved_player, &[0xa7; 28])
                .unwrap();
        let mut payload = *emitted.payload();
        payload[..5].copy_from_slice(b"A%%B\0");
        let original = inf_formats::SaveRecord::from_payload(payload).unwrap();
        let files = ProfileFiles::new();
        files.supply_record(&original);
        fixture.host.profiles = Some(files.open(b"Prior current"));
        fixture.host.identity = Some(files.identity_owners(&fixture.assets));
        let store = fixture.host.profiles.as_ref().unwrap();
        let selected = store.selected();
        let control = store.control_token();
        let disk = std::fs::read(&files.sidecar).unwrap();
        let objects = fixture.object_custody();
        let mission_header = (
            fixture.mission.current_world_a0,
            fixture.mission.bytes_14_16,
        );
        let owners = fixture.host.identity.as_ref().unwrap();
        let mut expected_cache = owners.identity.clone();
        expected_cache.clear_loaded();
        let words = *owners.settings.words();
        fixture.initialize(None, 18).unwrap();

        assert_eq!(fixture.mission.save, saved_mission);
        assert_eq!(fixture.state.gameplay_time().seconds, 731);
        assert_eq!(
            (
                fixture.mission.current_world_a0,
                fixture.mission.bytes_14_16
            ),
            mission_header
        );
        assert!(fixture.arena.first_player().is_none());
        assert_eq!(fixture.object_custody(), objects);
        assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
        assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
        let store = fixture.host.profiles.as_ref().unwrap();
        assert_eq!(store.current_name(), Some(b"A%B".as_slice()));
        assert_eq!(store.working_record(), Some(&original));
        assert_eq!(store.selected(), selected);
        assert_eq!(store.control_token(), control);
        assert!(!store.write_attempted());
        assert_eq!(std::fs::read(&files.sidecar).unwrap(), disk);
        let owners = fixture.host.identity.as_ref().unwrap();
        assert_eq!(owners.identity, expected_cache);
        assert_eq!(owners.settings.words(), &words);
        assert_eq!(
            owners.settings.stored_player_name(),
            Some(b"A%B".as_slice())
        );
        assert_eq!(
            owners.settings.stored_net_player_name(),
            Some(b"Saved network".as_slice())
        );
        assert_eq!(
            owners.settings.stored_player_initials(),
            Some(b"LIVE".as_slice())
        );
        assert!(owners.exit_requested);
        let saved = FrontendSettings::load_or_defaults(
            files.directory.join("settings.json"),
            &fixture.assets,
        )
        .unwrap();
        assert_eq!(saved.stored_player_name(), Some(b"A%B".as_slice()));
        assert_eq!(saved.stored_player_initials(), Some(b"OLD".as_slice()));
        assert_eq!(
            fixture.host.calls,
            [
                Call::Enumerate,
                Call::ProfileLoan,
                Call::Identity {
                    target: IdentityTarget::PlayerName,
                    name: b"A%B".to_vec(),
                    initials: None,
                },
            ]
        );
    }
}

#[test]
fn reached_restore_format_error_is_labelled_and_does_not_call_the_late_writer() {
    let mut fixture = Fixture::new();
    fixture.mission.bytes_14_16 = Some([0x71, 0xff, 0x92]);
    fixture.player(0xffff, [0, 4, 0]);
    let expected = expected_players(&fixture, true);
    let files = ProfileFiles::new();
    let original = inf_formats::SaveRecord::from_state(
        b"%s",
        &MissionSaveState::new_game(),
        &inf_formats::PlayerSaveState::new_player([0x7654_3210; 6]),
        &[0xb3; 29],
    )
    .unwrap();
    files.supply_record(&original);
    fixture.host.profiles = Some(files.open(b"Prior current"));
    let mission = fixture.mission.clone();
    let state = fixture.state.clone();
    let objects = fixture.object_custody();
    let disk = std::fs::read(&files.sidecar).unwrap();
    let process = process(0);
    fixture.initialize(Some(&process), 18).unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.mission, mission);
    assert_eq!(fixture.state, state);
    assert_eq!(fixture.object_custody(), objects);
    let store = fixture.host.profiles.as_ref().unwrap();
    assert_eq!(store.current_name(), Some(b"Prior current".as_slice()));
    assert_eq!(store.working_record(), Some(&original));
    assert!(!store.write_attempted());
    assert_eq!(std::fs::read(&files.sidecar).unwrap(), disk);
    assert_eq!(fixture.host.calls.len(), 3);
    assert_eq!(fixture.host.calls[1], Call::ProfileLoan);
    assert!(matches!(
        &fixture.host.calls[2],
        Call::ProfileFailure(IntroResultsProfileOperation::Restore, message)
            if !message.is_empty()
    ));
}

#[test]
fn mode_zero_without_store_is_a_reached_skip_after_player_and_input_binding() {
    let mut fixture = Fixture::new();
    fixture.mission.bytes_14_16 = Some([0x71, 0, 0x92]);
    fixture.player(0xffff, [0, 4, 0]);
    let expected = expected_players(&fixture, true);
    fixture.prior_selection();
    let process = process(0);
    fixture.initialize(Some(&process), 18).unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(fixture.screen.player_word_144(), 0xffff);
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
    assert_eq!(
        fixture.input.selected_context(),
        fixture.screen.bindings.context()
    );
    assert_eq!(
        fixture.host.calls,
        [
            Call::Enumerate,
            Call::ProfileLoan,
            Call::Skip(IntroResultsSkippedFeature::ConstructorProfileStoreUnavailable),
        ]
    );
}

#[test]
fn mode_zero_serializes_the_actual_current_mission_elapsed_and_first_player() {
    let mut fixture = Fixture::new();
    fixture.mission.bytes_14_16 = Some([0x71, 0, 0x92]);
    fixture.mission.save.elapsed_seconds = 321;
    fixture.mission.save.owner_b0[7] = 0x8000_1234;
    fixture.state.restore_mission_elapsed_seconds(987);
    let first = fixture.player(0x8000, [0x2468, 4, 0xfedc_ba98]);
    fixture.player(0xffff, [0xffff_ffff; 3]);
    let expected = expected_players(&fixture, true);
    let objects_before = fixture.object_custody();
    let mission_before = fixture.mission.clone();
    let files = ProfileFiles::new();
    fixture.host.profiles = Some(files.open(b"A%%B"));
    let store = fixture.host.profiles.as_ref().unwrap();
    let selected_before = store.selected();
    let disk_before = std::fs::read(&files.sidecar).unwrap();
    let process = process(0);
    let process_before = process;
    fixture.initialize(Some(&process), 18).unwrap();
    let store = fixture.host.profiles.as_ref().unwrap();
    let working = store.working_record().unwrap();
    assert_eq!(working.name(), b"A%B");
    let mut expected_mission = mission_before.save.clone();
    expected_mission.elapsed_seconds = 987;
    assert_eq!(working.mission(), expected_mission);
    assert_eq!(
        working.player(),
        fixture.arena.export_player_save_state(first).unwrap()
    );
    assert_eq!(store.current_name(), Some(b"A%%B".as_slice()));
    assert_eq!(store.selected(), selected_before);
    assert!(!store.write_attempted());
    assert_eq!(std::fs::read(&files.sidecar).unwrap(), disk_before);
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(fixture.object_custody(), objects_before);
    assert_eq!(fixture.mission, mission_before);
    assert_eq!(process, process_before);
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(fixture.screen.player_word_144(), 0x8000);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
    assert_eq!(
        fixture.input.selected_context(),
        fixture.screen.bindings.context()
    );
    assert_eq!(fixture.host.calls, [Call::Enumerate, Call::ProfileLoan,]);
}

#[test]
fn reached_serialize_error_is_reported_after_the_loan_and_keeps_screen_ready() {
    let mut fixture = Fixture::new();
    fixture.mission.bytes_14_16 = Some([0x71, 0, 0x92]);
    fixture.player(0xffff, [0, 4, 0]);
    let expected = expected_players(&fixture, true);
    let files = ProfileFiles::new();
    fixture.host.profiles = Some(files.open(b"%s"));
    let working_before = fixture
        .host
        .profiles
        .as_ref()
        .unwrap()
        .working_record()
        .cloned();
    let disk_before = std::fs::read(&files.sidecar).unwrap();
    fixture.prior_selection();
    let process = process(0);
    fixture.initialize(Some(&process), 18).unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(fixture.screen.player_word_144(), 0xffff);
    assert_fresh_results_menu(&fixture.screen);
    assert_eq!(fixture.registry.selected_screen(), fixture.screen.id());
    assert_eq!(
        fixture.input.selected_context(),
        fixture.screen.bindings.context()
    );
    assert_eq!(fixture.arena.world_players(), expected);
    assert_eq!(
        fixture.host.profiles.as_ref().unwrap().working_record(),
        working_before.as_ref()
    );
    assert_eq!(std::fs::read(&files.sidecar).unwrap(), disk_before);
    assert_eq!(fixture.host.calls.len(), 3);
    assert_eq!(fixture.host.calls[0], Call::Enumerate);
    assert_eq!(fixture.host.calls[1], Call::ProfileLoan);
    assert!(
        matches!(&fixture.host.calls[2], Call::ProfileFailure(IntroResultsProfileOperation::Serialize, message) if !message.is_empty())
    );
}

#[test]
fn results_draw_uses_fresh_renderers_and_widths_then_exits_before_intro_content() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    // These unavailable later fields cannot become prerequisites for the
    // actual current-world22 gate, even when the constructor selected18.
    fixture.mission.record_0c = None;
    let screen = format!("{:?}", fixture.screen);
    let mission = format!("{:?}", fixture.mission);
    let custody = fixture.object_custody();
    let mut host = DrawRecorder::default();
    fixture
        .screen
        .draw(
            IntroResultsDrawRequest {
                attract: false,
                mission: Some(&fixture.mission),
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(host.calls, draw_protocol());
    assert_eq!(host.arena, Some((1, 0xc8000)));
    assert_eq!(host.batch, Some(2));
    assert_eq!(host.published, Some(3));
    assert_eq!(format!("{:?}", fixture.screen), screen);
    assert_eq!(format!("{:?}", fixture.mission), mission);
    assert_eq!(fixture.object_custody(), custody);

    // The wrapped signed width calculation is discarded on22, and must
    // not turn a full-DWORD surface width into an admission failure.
    let mut host = DrawRecorder {
        widths: [0, u32::MAX],
        ..Default::default()
    };
    fixture
        .screen
        .draw(
            IntroResultsDrawRequest {
                attract: false,
                mission: Some(&fixture.mission),
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(
        &host.calls[6..],
        [DrawCall::Width(0), DrawCall::Width(u32::MAX)]
    );
}

#[test]
fn results_draw_attract_bypasses_all_unavailable_draw_inputs() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    let mut host = DrawRecorder {
        fail_at: Some(0),
        arena: Some((9, 0x1000)),
        batch: Some(8),
        published: Some(7),
        ..Default::default()
    };
    fixture
        .screen
        .draw(
            IntroResultsDrawRequest {
                attract: true,
                mission: None,
            },
            &mut host,
        )
        .unwrap();
    assert!(host.calls.is_empty());
    assert_eq!(host.arena, Some((9, 0x1000)));
    assert_eq!(host.batch, Some(8));
    assert_eq!(host.published, Some(7));
}

#[test]
fn results_draw_requires_mission_only_after_both_widths_and_retains_preparation() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    let mut host = DrawRecorder::default();
    assert!(matches!(
        fixture.screen.draw(
            IntroResultsDrawRequest {
                attract: false,
                mission: None,
            },
            &mut host,
        ),
        Err(IntroResultsScreenError::DrawMissionUnavailable)
    ));
    assert_eq!(host.calls, draw_protocol());
    assert_eq!(host.published, Some(3));
    // Any world but the intro's 22 (here a full DWORD past it) goes on to
    // the results page.
    fixture.mission.current_world_a0 = 0x1_0016;
    let mut host = DrawRecorder::default();
    assert_eq!(
        fixture
            .screen
            .draw(
                IntroResultsDrawRequest {
                    attract: false,
                    mission: Some(&fixture.mission),
                },
                &mut host,
            )
            .unwrap(),
        IntroResultsDrawOutcome::Page
    );
    assert_eq!(host.calls, draw_protocol());
}

#[test]
fn results_draw_host_failures_keep_each_completed_prefix_without_frontend_recovery() {
    let mut fixture = Fixture::new();
    fixture.initialize(None, 18).unwrap();
    let before = format!("{:?}", fixture.screen);
    for fail_at in 0..8 {
        let mut host = DrawRecorder {
            fail_at: Some(fail_at),
            ..Default::default()
        };
        assert!(matches!(
            fixture.screen.draw(
                IntroResultsDrawRequest {
                    attract: false,
                    mission: None,
                },
                &mut host,
            ),
            Err(IntroResultsScreenError::Host(
                "required results draw operation failed"
            ))
        ));
        assert_eq!(host.calls, draw_protocol()[..=fail_at]);
        assert_eq!(host.arena, (fail_at > 1).then_some((1, 0xc8000)));
        assert_eq!(host.batch, (fail_at > 3).then_some(2));
        assert_eq!(host.published, (fail_at > 5).then_some(3));
        assert_eq!(format!("{:?}", fixture.screen), before);
    }
}

#[test]
fn results_draw_nonready_ownership_fails_before_attract_or_renderer_effects() {
    let fixture = Fixture::new();
    let mut host = DrawRecorder::default();
    assert!(matches!(
        fixture.screen.draw(
            IntroResultsDrawRequest {
                attract: true,
                mission: None,
            },
            &mut host,
        ),
        Err(IntroResultsScreenError::Lifecycle {
            expected: ScreenLifecycle::Ready,
            actual: ScreenLifecycle::New,
        })
    ));
    assert!(host.calls.is_empty());
}

/// 0042C2B0's state on a world's results (18, on to 2), frame by frame
/// after each tick: the tail builds the first panel, each panel 90% in
/// builds the next, panel 1 scores (2 - 1 ACAM lost) * 150 + 8 kills * 10 =
/// 230, and panel 2 awards medal 2 once, adding it to the player's +11E, as
/// Infest02's "Medal awarded : Silver" after Level 1.
#[test]
fn results_draw_builds_the_panels_scores_and_awards_the_medal_once() {
    let mut fixture = Fixture::new();
    let player = fixture.player(1, [0; 3]);
    fixture.mission.current_world_a0 = 18;
    fixture.mission.save.owner_3c[18] |= 2;
    // The world header's +14..+16, as Level 1's load left them.
    fixture.mission.bytes_14_16 = Some([0; 3]);
    let record = fixture
        .mission
        .record_0c
        .get_or_insert_with(|| Box::new([0; crate::world_mission::RECORD_0C_BYTES]));
    record[0xa0..0xa4].copy_from_slice(&5_u32.to_le_bytes());
    record[0xb0..0xb4].copy_from_slice(&3_u32.to_le_bytes());
    let process = process(0);
    fixture.initialize(Some(&process), 2).unwrap();
    assert_eq!(fixture.screen.player_word_144(), 1);
    let frontend = FrontendProcessState::default();
    let frame = |fixture: &mut Fixture| {
        fixture
            .screen
            .tick(
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &frontend,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host,
            )
            .unwrap();
        fixture
            .screen
            .draw_effects(false, &fixture.mission, &mut fixture.arena)
            .unwrap();
    };
    frame(&mut fixture);
    assert!(fixture.screen.panels()[0].is_some());
    assert!(fixture.screen.panels()[1].is_none());
    for _ in 1..13 {
        frame(&mut fixture);
    }
    assert_eq!(fixture.screen.score(), 230);
    assert!(fixture.screen.panels()[1].is_some());
    assert_eq!(fixture.screen.medal(), None);
    frame(&mut fixture);
    assert_eq!(fixture.screen.medal(), Some(2));
    assert_eq!(fixture.arena.world_player(player).unwrap().word_11e, 2);
    for _ in 14..20 {
        frame(&mut fixture);
    }
    assert!(fixture.screen.panels().iter().all(Option::is_some));
    assert_eq!(fixture.arena.world_player(player).unwrap().word_11e, 2);
}

/// Infest02's results after Level 1's gate, tick by tick (0042CFC0 calls
/// 4EA591..4F1978): the trace's scheduler deltas, S before tick 10 (released
/// before 11) and Space before tick 22. Every tick's age +C0, +14C, save
/// countdown +EC, exit countdown +148 and repeat +C8 match the trace, as do
/// the draws' writes: panel 1 at the first draw, the score 230 at the sixth
/// and the medal 2 at the fifteenth (the fourteenth's age 5213 blinks the
/// status line off). The save lands at tick 17, and tick 30 asks for phase 3.
#[test]
fn infest02_results_match_the_trace_tick_by_tick() {
    // Each call's (delta, elapsed) arguments, then +C0, +14C, +EC and +148
    // after it.
    const TICKS: [(i32, i32); 30] = [
        (0x1e848, 0x80e80),
        (0x1e078, 0x1e078),
        (0x1a5e0, 0x1a5e0),
        (0x19e10, 0x19e10),
        (0x18a88, 0x18a88),
        (0x19e10, 0x19e10),
        (0x1c520, 0x1c520),
        (0x1e848, 0x1ffb8),
        (0x1e848, 0x26930),
        (0x1e848, 0x29810),
        (0x1e848, 0x2bb38),
        (0x1e848, 0x2c308),
        (0x1e848, 0x2b750),
        (0x1e848, 0x2ab98),
        (0x1e848, 0x50910),
        (0x1e848, 0x4b320),
        (0x1e848, 0x40358),
        (0x1e848, 0x474a0),
        (0x1e848, 0x48058),
        (0x1e848, 0x43a08),
        (0x1e848, 0x45560),
        (0x1e848, 0x43238),
        (0x186a0, 0x186a0),
        (0x1b198, 0x1b198),
        (0x1b580, 0x1b580),
        (0x1adb0, 0x1adb0),
        (0x1d4c0, 0x1d4c0),
        (0x1e078, 0x1e078),
        (0x1d4c0, 0x1d4c0),
        (0x1d0d8, 0x1d0d8),
    ];
    const AGE: [i32; 30] = [
        0x28d, 0x383, 0x45b, 0x52f, 0x5f9, 0x6cd, 0x7b5, 0x8b5, 0x9d0, 0xfa0, 0x10d0, 0x1202,
        0x1331, 0x145d, 0x1624, 0x17d5, 0x1959, 0x1afa, 0x1c9e, 0x1e30, 0x1fc9, 0x2159, 0x2221,
        0x22ff, 0x23df, 0x24bb, 0x25ab, 0x26a1, 0x2791, 0x287f,
    ];
    const ELAPSED_14C: [i32; 30] = [
        0x7d, 0xf8, 0x164, 0x1ce, 0x233, 0x29d, 0x311, 0x38e, 0x40b, 0x488, 0x505, 0x582, 0x5ff,
        0x67c, 0x6f9, 0x776, 0x7f3, 0x870, 0x8ed, 0x96a, 0x9e7, 0xa64, 0xac8, 0xb37, 0xba7, 0xc15,
        0xc8d, 0xd08, 0xd80, 0xdf7,
    ];
    const SAVE_EC: [i32; 30] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0x7e, 0xfb, 0x178, 0x1f5, 0x272, 0x2ef, 0x36c, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    const EXIT_148: [u32; 30] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x36b, 0x307, 0x298, 0x228,
        0x1ba, 0x142, 0xc7, 0x4f, 1,
    ];
    let mut fixture = Fixture::new();
    // One ACAM lost, Level 1 completed (the pass's bit 2, then bit 1), and
    // its eight kills.
    let player = fixture.player(1, [0; 3]);
    fixture.mission.current_world_a0 = 18;
    fixture.mission.save.owner_3c[18] |= 3;
    fixture.mission.bytes_14_16 = Some([0; 3]);
    fixture
        .mission
        .record_0c
        .get_or_insert_with(|| Box::new([0; crate::world_mission::RECORD_0C_BYTES]))[0xa0] = 8;
    let process = process(0);
    fixture.initialize(Some(&process), 2).unwrap();
    fixture.host.save_result = true;
    fixture.host.calls.clear();
    let frontend = FrontendProcessState::default();
    let key = |address, value| crate::input::InputCallback {
        address,
        context: INTRO_RESULTS_CALLBACK_CONTEXT,
        value,
        timestamp_micros: 0,
    };
    for (index, &(delta_micros, elapsed_micros)) in TICKS.iter().enumerate() {
        let tick = index + 1;
        match tick {
            // S: 0042AAA0 and the held bit 0x40 (00485300).
            10 => {
                for address in [0x0042_aaa0, 0x0048_5300] {
                    fixture
                        .screen
                        .input_callback(key(address, 1), &mut no_menu(&mut fixture.input))
                        .unwrap();
                }
            }
            11 => {
                for address in [0x0042_aaa0, 0x0048_5300] {
                    fixture
                        .screen
                        .input_callback(key(address, 0), &mut no_menu(&mut fixture.input))
                        .unwrap();
                }
            }
            // Space: 0042AA60 arms the exit once the stats are shown.
            22 => {
                fixture
                    .screen
                    .input_callback(key(0x0042_aa60, 1), &mut no_menu(&mut fixture.input))
                    .unwrap();
                fixture
                    .screen
                    .input_callback(key(0x0042_aa60, 0), &mut no_menu(&mut fixture.input))
                    .unwrap();
            }
            _ => {}
        }
        fixture
            .screen
            .tick(
                SchedulerTick {
                    delta_micros,
                    elapsed_micros,
                },
                &mut IntroResultsTickServices {
                    menu: no_menu(&mut fixture.input),
                    process: &frontend,
                    mission: &fixture.mission,
                    attract: false,
                },
                &mut fixture.host,
            )
            .unwrap();
        let screen = &fixture.screen;
        assert_eq!(
            screen.input().age_millis(),
            AGE[index],
            "+C0 after tick {tick}"
        );
        assert_eq!(
            screen.elapsed_millis_14c(),
            ELAPSED_14C[index],
            "+14C after tick {tick}"
        );
        assert_eq!(
            screen.save_countdown_millis(),
            SAVE_EC[index],
            "+EC after tick {tick}"
        );
        assert_eq!(
            screen.exit_millis(),
            EXIT_148[index],
            "+148 after tick {tick}"
        );
        assert_eq!(
            screen.intro_delay_elapsed(),
            tick >= 11,
            "+F4 after tick {tick}"
        );
        assert_eq!(screen.saved(), tick >= 17, "+F0 after tick {tick}");
        assert_eq!(
            screen.panels()[1].is_some() && screen.panels()[2].is_some(),
            tick >= 10,
            "+134/+138 after tick {tick}"
        );
        match tick {
            10 => assert_eq!(screen.input().repeat_millis(), 0x7d),
            11 => assert_eq!(screen.input().repeat_millis(), 0),
            _ => {}
        }
        assert_eq!(
            fixture.host.calls.contains(&Call::Save),
            tick >= 17,
            "save by tick {tick}"
        );
        assert_eq!(
            fixture
                .host
                .calls
                .contains(&Call::Request(SessionPhase::Phase8, SessionPhase::Phase3)),
            tick == 30,
            "phase 3 by tick {tick}"
        );
        fixture
            .screen
            .draw_effects(false, &fixture.mission, &mut fixture.arena)
            .unwrap();
        let screen = &fixture.screen;
        assert!(screen.panels()[0].is_some(), "+130 after draw {tick}");
        assert_eq!(
            screen.score(),
            if tick >= 6 { 230 } else { 0 },
            "+140 after draw {tick}"
        );
        assert_eq!(
            screen.medal(),
            (tick >= 15).then_some(2),
            "+13C after draw {tick}"
        );
    }
    assert_eq!(
        fixture
            .host
            .calls
            .iter()
            .filter(|call| **call == Call::Save)
            .count(),
        1
    );
    assert_eq!(fixture.arena.world_player(player).unwrap().word_11e, 2);
}
