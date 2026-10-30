use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_scene::SceneProjection;
use crate::input::DeviceButtons;
use crate::residency::ResourceSelection;
use crate::{Installation, ResidencyRequest, ResidentRuntime};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[derive(Debug, Default)]
struct FactoryHost {
    pauses: usize,
    enumerations: usize,
    title_requests: usize,
}

impl StartupFactoryHost for FactoryHost {
    type Error = &'static str;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.pauses += 1;
        Ok(())
    }
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
    fn request_title(&mut self) {
        self.title_requests += 1;
    }
}

#[derive(Debug, Default)]
struct Effects {
    language: u32,
    loaded: bool,
    selected: Vec<u32>,
    persisted: usize,
    title_requests: usize,
    fail_select: bool,
    failures: Vec<String>,
    quantum: Option<u32>,
}

impl StartupEffects for Effects {
    type Error = String;
    fn tick_quantum(&self) -> Option<u32> {
        self.quantum
    }
    fn settings_language(&self) -> u32 {
        self.language
    }
    fn settings_loaded(&self) -> bool {
        self.loaded
    }
    fn select_language(&mut self, language: u32) -> Result<(), Self::Error> {
        // The word is written before the reselect can fail.
        self.language = language;
        self.selected.push(language);
        if self.fail_select {
            return Err("reselect failed".into());
        }
        Ok(())
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.persisted += 1;
        Ok(())
    }
    fn request_title(&mut self) {
        self.title_requests += 1;
    }
    fn report_accept_failure(&mut self, error: Self::Error) {
        self.failures.push(error);
    }
}

struct Harness {
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    assets: FrontendAssets,
    residency: ResidentRuntime,
    attachment: ScreenAttachment,
    process: StartupProcessState,
    host: FactoryHost,
}

fn harness() -> Harness {
    let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
    let mut residency = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    residency
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    // The Options visit: the frontend's leaf 47 is resident.
    residency
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let mut registry = ScreenRegistry::new();
    let attachment = ScreenAttachment {
        session: registry.register_session(),
        context_owner: registry.register_context(FrontendRenderContext {
            projection: SceneProjection {
                focal: [320, 240],
                viewport: [640, 480],
                fade: [0, 0],
            },
        }),
    };
    Harness {
        registry,
        input: InputRuntime::new(0),
        assets,
        residency,
        attachment,
        process: StartupProcessState::default(),
        host: FactoryHost::default(),
    }
}

fn construct(h: &mut Harness, construction: StartupConstruction) -> StartupScreen {
    let mut screen = StartupScreen::new(
        StartupScreenConfig {
            attachment: h.attachment,
            callback_context: STARTUP_CALLBACK_CONTEXT,
        },
        &h.assets,
    );
    screen
        .initialize(
            &mut StartupFactoryServices {
                registry: &mut h.registry,
                input: &mut h.input,
                assets: &h.assets,
                residency: &mut h.residency,
                process: &mut h.process,
            },
            construction,
            &mut h.host,
        )
        .unwrap();
    screen
}

fn loaded(language: u32) -> StartupConstruction {
    StartupConstruction {
        settings_language: language,
        clear_material_word: 0x0421,
        settings_loaded: true,
    }
}

fn press(address: u32, value: u32) -> InputCallback {
    InputCallback {
        address,
        context: STARTUP_CALLBACK_CONTEXT,
        value,
        timestamp_micros: 0,
    }
}

fn tick(delta: i32) -> SchedulerTick {
    SchedulerTick {
        delta_micros: delta,
        elapsed_micros: delta,
    }
}

#[test]
fn class_and_reuse_match_the_native_queries() {
    assert_eq!(STARTUP_CLASS_ID, 0);
    assert!(startup_reuses_phase(SessionPhase::Startup));
    for phase in SessionPhase::ALL
        .into_iter()
        .filter(|phase| *phase != SessionPhase::Startup)
    {
        assert!(!startup_reuses_phase(phase), "{phase:?}");
    }
}

#[test]
fn construction_follows_00432500() {
    let mut h = harness();
    let screen = construct(&mut h, loaded(3));
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(
        *screen.state(),
        StartupState {
            selection: 3,
            direction: 0,
            angle: 0,
            age: 0,
            idle: 0,
            shadow_color: 0,
            clear_color: 0x0421,
        }
    );
    assert_eq!((h.host.pauses, h.host.enumerations), (1, 1));
    // Settings loaded at startup: the phase-1 request (dropped by a pending
    // transition) is made, and leaf 2 replaced leaf 47 under their Select.
    assert_eq!(h.host.title_requests, 1);
    assert!(h.process.constructed);
    assert_eq!(h.residency.summary().active_leaves, [0, 2]);
    assert!(h
        .input
        .analog_value(screen.bindings().consumer_cc().unwrap())
        .is_ok());
}

#[test]
fn later_constructions_start_with_the_fades_complete() {
    let mut h = harness();
    h.process.constructed = true;
    let screen = construct(
        &mut h,
        StartupConstruction {
            settings_loaded: false,
            ..loaded(0)
        },
    );
    assert_eq!(screen.state().age, 2_000_000);
    assert!(screen.state().input_ready());
    // No settings at startup: the carousel waits for a choice.
    assert_eq!(h.host.title_requests, 0);
}

#[test]
fn turns_wait_for_the_fade_in_then_wrap_and_ease() {
    let mut h = harness();
    let mut screen = construct(&mut h, loaded(5));
    let mut effects = Effects::default();
    screen
        .digital_callback(press(STARTUP_NEXT_CALLBACK, 1), &mut effects)
        .unwrap();
    assert_eq!(screen.state().direction, 1);
    screen.tick(&h.input, tick(500_000), &mut effects).unwrap();
    // Before the gate the turn stays pending.
    assert_eq!(
        (
            screen.state().selection,
            screen.state().direction,
            screen.state().angle
        ),
        (5, 1, 0)
    );
    screen.tick(&h.input, tick(600_000), &mut effects).unwrap();
    // 5 + 1 wraps to 0; the angle gains 0x10000/6 then keeps seven eighths.
    assert_eq!(
        (
            screen.state().selection,
            screen.state().direction,
            screen.state().angle
        ),
        (0, 0, 10_922 * 7 / 8)
    );
    screen
        .digital_callback(press(STARTUP_PREVIOUS_CALLBACK, 1), &mut effects)
        .unwrap();
    screen.tick(&h.input, tick(125_000), &mut effects).unwrap();
    // 0 - 1 wraps to 5; -10922 joins the decaying angle.
    assert_eq!(screen.state().selection, 5);
    assert_eq!(screen.state().angle, (9_556 - 10_922) * 7 / 8);
    assert_eq!(effects.title_requests, 0);
}

#[test]
fn arrow_callbacks_reset_idle_and_turn_only_on_press() {
    let mut h = harness();
    let mut screen = construct(&mut h, loaded(0));
    let mut effects = Effects::default();
    screen.tick(&h.input, tick(125_000), &mut effects).unwrap();
    assert_eq!(screen.state().idle, 125_000);
    screen
        .digital_callback(press(STARTUP_PREVIOUS_CALLBACK, 0), &mut effects)
        .unwrap();
    assert_eq!((screen.state().idle, screen.state().direction), (0, 0));
    screen.tick(&h.input, tick(125_000), &mut effects).unwrap();
    screen
        .digital_callback(press(STARTUP_NEXT_CALLBACK, 0), &mut effects)
        .unwrap();
    assert_eq!((screen.state().idle, screen.state().direction), (0, 0));
    // The accept release does nothing, and the accept never resets idle.
    screen
        .digital_callback(press(STARTUP_ACCEPT_CALLBACK, 0), &mut effects)
        .unwrap();
    assert!(matches!(
        screen.digital_callback(press(0x0043_9260, 1), &mut effects),
        Err(StartupScreenError::UnknownCallback(0x0043_9260))
    ));
    assert!(matches!(
        screen.digital_callback(
            InputCallback {
                context: 1,
                ..press(STARTUP_ACCEPT_CALLBACK, 1)
            },
            &mut effects
        ),
        Err(StartupScreenError::Context { .. })
    ));
    assert_eq!(effects.title_requests, 0);
}

#[test]
fn held_adjust_slots_set_the_turn_like_the_arrows() {
    let mut state = StartupState {
        selection: 0,
        direction: 0,
        angle: 0,
        age: 0,
        idle: 0,
        shadow_color: 0,
        clear_color: 0,
    };
    for (action, direction) in [
        (ScreenInputAction::AdjustPrevious, -1),
        (ScreenInputAction::AdjustNext, 1),
    ] {
        held_action(&mut state, action);
        assert_eq!(state.direction, direction);
    }
    for action in [
        ScreenInputAction::Confirm,
        ScreenInputAction::Previous,
        ScreenInputAction::Next,
        ScreenInputAction::Back,
    ] {
        held_action(&mut state, action);
        assert_eq!(state.direction, 1, "{action:?} is the empty 00401310");
    }
}

#[test]
fn accept_waits_for_the_gate_and_requests_the_title() {
    let mut h = harness();
    let mut screen = construct(&mut h, loaded(0));
    let mut effects = Effects {
        loaded: true,
        ..Effects::default()
    };
    screen
        .tick(&h.input, tick(1_000_000), &mut effects)
        .unwrap();
    // Exactly the gate is not past it.
    assert_eq!(
        screen
            .digital_callback(press(STARTUP_ACCEPT_CALLBACK, 1), &mut effects)
            .unwrap(),
        StartupAcceptOutcome::NotReady
    );
    assert_eq!(effects.title_requests, 0);
    screen
        .digital_callback(press(STARTUP_NEXT_CALLBACK, 1), &mut effects)
        .unwrap();
    screen.tick(&h.input, tick(1), &mut effects).unwrap();
    assert_eq!(
        screen
            .digital_callback(press(STARTUP_ACCEPT_CALLBACK, 1), &mut effects)
            .unwrap(),
        StartupAcceptOutcome::Accepted
    );
    // A changed language is written and reselected; loaded settings are not
    // persisted here (the rebuilt frontend persists them).
    assert_eq!(effects.selected, [1]);
    assert_eq!((effects.persisted, effects.title_requests), (0, 1));
}

#[test]
fn accept_persists_without_loaded_settings_and_recovers_failures() {
    let mut h = harness();
    h.process.constructed = true;
    let mut screen = construct(&mut h, loaded(4));
    let mut effects = Effects {
        language: 4,
        ..Effects::default()
    };
    assert_eq!(screen.accept(&mut effects), StartupAcceptOutcome::Accepted);
    assert!(effects.selected.is_empty());
    assert_eq!((effects.persisted, effects.title_requests), (1, 1));

    let mut failing = Effects {
        language: 0,
        fail_select: true,
        ..Effects::default()
    };
    assert_eq!(screen.accept(&mut failing), StartupAcceptOutcome::Recovered);
    // The word was written; persistence was skipped; phase 1 still follows.
    assert_eq!(failing.language, 4);
    assert_eq!(failing.failures, ["reselect failed"]);
    assert_eq!((failing.persisted, failing.title_requests), (0, 1));
}

#[test]
fn thirty_idle_seconds_accept_without_input() {
    let mut h = harness();
    h.process.constructed = true;
    let mut screen = construct(&mut h, loaded(2));
    let mut effects = Effects {
        language: 2,
        loaded: true,
        ..Effects::default()
    };
    for _ in 0..240 {
        assert_eq!(
            screen.tick(&h.input, tick(125_000), &mut effects).unwrap(),
            StartupAcceptOutcome::NotReady
        );
    }
    assert_eq!(screen.state().idle, 30_000_000);
    assert_eq!(
        screen.tick(&h.input, tick(125_000), &mut effects).unwrap(),
        StartupAcceptOutcome::Accepted
    );
    assert_eq!(effects.title_requests, 1);
}

#[test]
fn teardown_releases_the_screen_but_keeps_residency() {
    let mut h = harness();
    let mut screen = construct(&mut h, loaded(0));
    let id = screen.id().unwrap();
    screen.destroy(&mut h.registry, &mut h.input).unwrap();
    assert_eq!(screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert!(h.registry.screen(id).is_err());
    assert_eq!(h.residency.summary().active_leaves, [0, 2]);
    // Idempotent once destroyed.
    screen.destroy(&mut h.registry, &mut h.input).unwrap();
}

/// After a turn, retail eases the carousel by 7/8 a tick, so it settles in
/// a quarter of the time at 4 ms ticks that it takes at 16 ms; the port's
/// quantum (16384 µs) eases it once a quantum, so it settles alike.
#[test]
fn quantized_carousel_easing_keeps_its_pace_at_high_frame_rates() {
    let settle = |tick_micros: i32, quantum: Option<u32>| {
        let mut h = harness();
        let mut screen = construct(&mut h, loaded(5));
        let mut effects = Effects {
            quantum,
            ..Effects::default()
        };
        screen
            .tick(&h.input, tick(1_100_000), &mut effects)
            .unwrap();
        screen
            .digital_callback(press(STARTUP_NEXT_CALLBACK, 1), &mut effects)
            .unwrap();
        screen
            .tick(&h.input, tick(tick_micros), &mut effects)
            .unwrap();
        let mut elapsed = tick_micros;
        while screen.state().angle != 0 {
            screen
                .tick(&h.input, tick(tick_micros), &mut effects)
                .unwrap();
            elapsed += tick_micros;
        }
        elapsed
    };
    // Retail: the settling time scales with the tick.
    assert!(settle(4000, None) * 3 < settle(16000, None));
    // Quantized: within one quantum of the same time at both rates.
    let (fast, slow) = (settle(4000, Some(16384)), settle(16000, Some(16384)));
    assert!((fast - slow).abs() <= 16384, "{fast} µs against {slow} µs");
}
