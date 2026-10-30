use std::path::Path;

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_scene::SceneProjection;
use crate::input::DeviceButtons;

#[derive(Debug, Default)]
struct FakeHost {
    open_result: Option<Result<Option<()>, &'static str>>,
    status: u32,
    opens: usize,
    closes: usize,
    preambles: usize,
    frames: usize,
    restores: usize,
    enumerations: usize,
    /// (previous, requested) pairs passed to 00438380.
    requests: Vec<(SessionPhase, SessionPhase)>,
    /// When true, the phase request reports a pending-transition rejection
    /// (first-request-wins), mirroring the ignored factory finish in the
    /// accepted 0->1 title trace.
    reject_request: bool,
}

impl TitleEffects for FakeHost {
    type Error = &'static str;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        self.opens += 1;
        self.open_result.take().unwrap_or(Ok(None))
    }

    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(self.status)
    }

    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.frames += 1;
        Ok(())
    }

    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.closes += 1;
        Ok(())
    }

    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        self.preambles += 1;
        Ok(())
    }

    fn restore_display(&mut self) -> Result<(), Self::Error> {
        self.restores += 1;
        Ok(())
    }

    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        self.requests.push((previous, requested));
        if self.reject_request {
            return Err("pending");
        }
        Ok(())
    }
}

impl TitleVideoHost for FakeHost {
    type Error = &'static str;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        TitleEffects::open_title(self)
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        TitleEffects::playback_status(self)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        TitleEffects::present_frame(self)
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        TitleEffects::close_title(self)
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        TitleEffects::restore_display(self)
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        TitleEffects::draw_preamble(self)
    }
}

impl TitleHost for FakeHost {
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.enumerations += 1;
        let source = input
            .create_device(DeviceButtons::default())
            .map_err(|_| "device failed")?;
        Ok(vec![FrontendDevice {
            native_type: 0,
            source,
        }])
    }
}
struct Harness {
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    assets: FrontendAssets,
    attachment: ScreenAttachment,
    process: TitleProcessState,
    state: SessionState,
    host: FakeHost,
}

fn projection() -> SceneProjection {
    SceneProjection {
        focal: [320, 240],
        viewport: [640, 480],
        fade: [0, 0],
    }
}

fn harness(host: FakeHost) -> Harness {
    let assets = {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        FrontendAssets::open(&root).expect("retail Infest.exe is required")
    };
    let mut registry = ScreenRegistry::new();
    let attachment = ScreenAttachment {
        session: registry.register_session(),
        context_owner: registry.register_context(FrontendRenderContext {
            projection: projection(),
        }),
    };
    Harness {
        registry,
        input: InputRuntime::new(0),
        assets,
        attachment,
        process: TitleProcessState::default(),
        state: SessionState::default(),
        host,
    }
}

fn ready_screen(h: &mut Harness) -> TitleScreen {
    let mut screen = TitleScreen::new(
        TitleScreenConfig {
            attachment: h.attachment,
            callback_context: TITLE_CALLBACK_CONTEXT,
        },
        &h.assets,
    );
    screen
        .initialize(
            &mut TitleFactoryServices {
                registry: &mut h.registry,
                input: &mut h.input,
                assets: &h.assets,
                process: &mut h.process,
            },
            &mut h.host,
            &mut h.state,
        )
        .unwrap();
    screen
}

#[test]
fn class_and_reuse_match_the_native_queries() {
    assert_eq!(TITLE_CLASS_ID, 1);
    assert!(title_reuses_phase(SessionPhase::Title));
    for phase in SessionPhase::ALL
        .into_iter()
        .filter(|phase| *phase != SessionPhase::Title)
    {
        assert!(!title_reuses_phase(phase), "{phase:?}");
    }
}

#[test]
fn tick_and_draw_before_construction_are_rejected() {
    let mut h = harness(FakeHost::default());
    let mut screen = TitleScreen::new(
        TitleScreenConfig {
            attachment: h.attachment,
            callback_context: TITLE_CALLBACK_CONTEXT,
        },
        &h.assets,
    );
    assert!(matches!(
        screen.tick(&mut h.host),
        Err(TitleScreenError::Lifecycle { .. })
    ));
    assert!(matches!(
        screen.draw(&mut h.host),
        Err(TitleScreenError::Lifecycle { .. })
    ));
    assert!(matches!(
        screen.digital_callback(
            InputCallback {
                address: TITLE_SKIP_CALLBACK,
                context: TITLE_CALLBACK_CONTEXT,
                value: 1,
                timestamp_micros: 0,
            },
            &mut h.host
        ),
        Err(TitleScreenError::Lifecycle { .. })
    ));
}

#[test]
fn first_factory_opens_video_and_sets_the_process_global() {
    let mut h = harness(FakeHost {
        open_result: Some(Ok(Some(()))),
        ..FakeHost::default()
    });
    let screen = ready_screen(&mut h);
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Ready);
    assert!(screen.video_present());
    assert!(!screen.finished());
    assert!(h.process.title_seen);
    assert_eq!(h.host.opens, 1);
    assert!(h.host.requests.is_empty());
    assert_eq!(h.host.enumerations, 1);
}

#[test]
fn failed_open_leaves_the_handle_clear_without_a_phase_request() {
    let mut h = harness(FakeHost::default());
    let mut screen = ready_screen(&mut h);
    assert!(!screen.video_present());
    assert!(!screen.finished());
    assert!(!h.process.title_seen);
    assert!(h.host.requests.is_empty());
    // The failed open leaves the first tick to finish.
    let outcome = screen.tick(&mut h.host).unwrap();
    assert_eq!(outcome, TitleTickOutcome::Finished);
    assert!(screen.finished());
    assert_eq!(
        h.host.requests,
        [(SessionPhase::Title, SessionPhase::Frontend)]
    );
}

#[test]
fn repeat_factory_finishes_immediately_without_reopening() {
    let mut h = harness(FakeHost::default());
    h.process.title_seen = true;
    let screen = ready_screen(&mut h);
    assert_eq!(h.host.opens, 0);
    assert_eq!(h.host.closes, 0);
    assert_eq!(h.host.restores, 1);
    // No transition is pending in the fresh boot state, so the skip-path
    // request is accepted through the session adapter.
    assert!(h.state.transition_pending());
    assert_eq!(h.state.requested_phase(), SessionPhase::Frontend);
    assert!(screen.finished());
    assert!(!screen.video_present());
}

#[test]
fn factory_finish_rejection_is_reported_not_absorbed() {
    let mut h = harness(FakeHost {
        reject_request: true,
        ..FakeHost::default()
    });
    h.process.title_seen = true;
    let mut screen = TitleScreen::new(
        TitleScreenConfig {
            attachment: h.attachment,
            callback_context: TITLE_CALLBACK_CONTEXT,
        },
        &h.assets,
    );
    let error = screen.factory(&mut h.process, &mut h.host).unwrap_err();
    assert!(matches!(error, TitleScreenError::Host("pending")));
    // The native finish restores the display before the rejected request.
    assert_eq!(h.host.restores, 1);
}

#[test]
fn tick_waits_while_playing_and_finishes_at_status_two() {
    let mut h = harness(FakeHost {
        open_result: Some(Ok(Some(()))),
        status: 1,
        ..FakeHost::default()
    });
    let mut screen = ready_screen(&mut h);
    let outcome = screen.tick(&mut h.host).unwrap();
    assert_eq!(outcome, TitleTickOutcome::Playing);
    assert!(!screen.finished());
    assert!(h.host.requests.is_empty());

    h.host.status = PLAYBACK_FINISHED;
    let outcome = screen.tick(&mut h.host).unwrap();
    assert_eq!(outcome, TitleTickOutcome::Finished);
    assert!(screen.finished());
    assert!(!screen.video_present());
    assert_eq!(h.host.closes, 1);
    assert_eq!(
        h.host.requests,
        [(SessionPhase::Title, SessionPhase::Frontend)]
    );
}

#[test]
fn draw_presents_only_with_a_handle_but_always_runs_the_preamble() {
    let mut h = harness(FakeHost::default());
    let screen = ready_screen(&mut h);
    assert_eq!(screen.draw(&mut h.host).unwrap(), TitleDrawOutcome::NoVideo);

    let mut h = harness(FakeHost {
        open_result: Some(Ok(Some(()))),
        ..FakeHost::default()
    });
    let screen = ready_screen(&mut h);
    assert_eq!(
        screen.draw(&mut h.host).unwrap(),
        TitleDrawOutcome::Presented
    );
    assert_eq!(h.host.preambles, 1);
    assert_eq!(h.host.frames, 1);
}

#[test]
fn skip_callback_finishes_on_press_only() {
    let mut h = harness(FakeHost::default());
    let mut screen = ready_screen(&mut h);
    let press = InputCallback {
        address: TITLE_SKIP_CALLBACK,
        context: TITLE_CALLBACK_CONTEXT,
        value: 1,
        timestamp_micros: 0,
    };
    // Releases never finish, matching `test edx,edx / je`.
    screen
        .digital_callback(InputCallback { value: 0, ..press }, &mut h.host)
        .unwrap();
    assert!(!screen.finished());
    assert!(h.host.requests.is_empty());
    // A press finishes through 004049C0.
    screen.digital_callback(press, &mut h.host).unwrap();
    assert!(screen.finished());
    assert_eq!(
        h.host.requests,
        [(SessionPhase::Title, SessionPhase::Frontend)]
    );
    // Post-finish presses return without another request.
    screen.digital_callback(press, &mut h.host).unwrap();
    assert_eq!(h.host.requests.len(), 1);
}

#[test]
fn callback_context_and_address_mismatches_fail_before_mutation() {
    let mut h = harness(FakeHost::default());
    let mut screen = ready_screen(&mut h);
    assert!(matches!(
        screen.digital_callback(
            InputCallback {
                address: TITLE_SKIP_CALLBACK,
                context: TITLE_CALLBACK_CONTEXT + 1,
                value: 1,
                timestamp_micros: 0,
            },
            &mut h.host
        ),
        Err(TitleScreenError::Context { .. })
    ));
    assert!(matches!(
        screen.digital_callback(
            InputCallback {
                address: 0x0043_9260,
                context: TITLE_CALLBACK_CONTEXT,
                value: 1,
                timestamp_micros: 0,
            },
            &mut h.host
        ),
        Err(TitleScreenError::UnknownCallback(_))
    ));
    assert!(!screen.finished());
    assert!(h.host.requests.is_empty());
}

#[test]
fn destroy_closes_without_a_phase_request() {
    let mut h = harness(FakeHost {
        open_result: Some(Ok(Some(()))),
        ..FakeHost::default()
    });
    let mut screen = ready_screen(&mut h);
    assert!(screen.video_present());
    let id = screen.id().expect("linked title screen");
    screen
        .destroy(&mut h.registry, &mut h.input, &mut h.host)
        .unwrap();
    assert_eq!(h.host.closes, 1);
    assert!(h.host.requests.is_empty());
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert!(h.registry.screen(id).is_err());
    // Destroying again is a no-op for retained partial acquisitions.
    screen
        .destroy(&mut h.registry, &mut h.input, &mut h.host)
        .unwrap();
    assert_eq!(h.host.closes, 1);
}
