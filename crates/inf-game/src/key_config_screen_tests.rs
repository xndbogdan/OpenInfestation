use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_scene::SceneProjection;
use crate::input::{DeviceSnapshot, InputHost};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[derive(Debug, Default)]
struct FactoryHost {
    keyboard: Option<DeviceId>,
}

impl KeyConfigFactoryHost for FactoryHost {
    type Error = &'static str;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        let source = input
            .create_device(DeviceButtons::default())
            .map_err(|_| "device failed")?;
        self.keyboard = Some(source);
        Ok(vec![FrontendDevice {
            native_type: 0,
            source,
        }])
    }
}

struct Effects {
    tables: KeyBindingTables,
    bindings: KeyBindings,
    requests: Vec<SessionPhase>,
    animation: u32,
    basis: MenuBasisMath,
}

impl KeyConfigEffects for Effects {
    fn bindings(&mut self) -> (&KeyBindingTables, &mut KeyBindings) {
        (&self.tables, &mut self.bindings)
    }
    fn request_phase(&mut self, phase: SessionPhase) {
        self.requests.push(phase);
    }
    fn advance_animation(&mut self, delta_micros: u32) {
        self.animation = self.animation.wrapping_add(delta_micros);
    }
    fn visibility(&self) -> MenuVisibility {
        MenuVisibility::OwnerFlagClear
    }
    fn basis(&self) -> &MenuBasisMath {
        &self.basis
    }
}

struct Discard;

impl InputHost for Discard {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(0)
    }
    fn callback(&mut self, _: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct Harness {
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    input: InputRuntime,
    screen: KeyConfigScreen,
    host: FactoryHost,
    effects: Effects,
}

impl Harness {
    fn new() -> Self {
        let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
        let tables = KeyBindingTables::from_executable(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            assets.key_names(),
        )
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
        let mut input = InputRuntime::new(0);
        let mut host = FactoryHost::default();
        let mut screen = KeyConfigScreen::new(
            KeyConfigScreenConfig {
                attachment,
                callback_context: KEY_CONFIG_CALLBACK_CONTEXT,
            },
            &assets,
        );
        // The Frontend -> Phase12 transition: current 2, requested 12.
        let state = SessionState::pending_for_tests(SessionPhase::Frontend, SessionPhase::Phase12);
        screen
            .initialize(
                &mut KeyConfigFactoryServices {
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                },
                &mut host,
                &state,
            )
            .unwrap();
        Self {
            registry,
            input,
            screen,
            host,
            effects: Effects {
                bindings: KeyBindings::initial(&tables),
                tables,
                requests: Vec::new(),
                animation: 0,
                basis: MenuBasisMath::new(assets.sine_table().values),
            },
        }
    }

    fn tick(&mut self) {
        self.screen
            .tick(
                &self.input,
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                self.host.keyboard,
                &mut self.effects,
            )
            .unwrap();
    }

    fn press(&mut self, address: u32) {
        for value in [1, 0] {
            self.screen
                .digital_callback(
                    InputCallback {
                        address,
                        context: KEY_CONFIG_CALLBACK_CONTEXT,
                        value,
                        timestamp_micros: 0,
                    },
                    &mut self.effects,
                )
                .unwrap();
            self.tick();
        }
    }

    fn hold(&mut self, codes: &[u8]) {
        let mut buttons = DeviceButtons::default();
        for code in codes {
            buttons.0[usize::from(*code / 32)] |= 1 << (code % 32);
        }
        self.input
            .update_device(
                self.host.keyboard.unwrap(),
                DeviceSnapshot {
                    buttons,
                    axes: [0; 8],
                },
                &mut Discard,
            )
            .unwrap();
    }

    fn selection(&self) -> i32 {
        self.screen.menus().unwrap().current().unwrap().selection()
    }
}

const CONFIRM: u32 = 0x0044_5200;
const NEXT: u32 = 0x0044_5240;
const PREVIOUS: u32 = 0x0044_5220;
const BACK: u32 = 0x0044_51e0;

#[test]
fn class_and_reuse_match_the_native_queries() {
    assert_eq!(KEY_CONFIG_CLASS_ID, 9);
    assert!(key_config_reuses_phase(SessionPhase::Phase12));
    assert!(!key_config_reuses_phase(SessionPhase::Frontend));
}

#[test]
fn construction_pushes_the_root_page_and_remembers_the_frontend() {
    let h = Harness::new();
    assert_eq!(h.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(h.screen.return_phase(), SessionPhase::Frontend);
    let menus = h.screen.menus().unwrap();
    assert_eq!(menus.len(), 1);
    assert_eq!(h.selection(), 1);
    assert_eq!(h.screen.capture(), None);
}

#[test]
fn infest01_visit_opens_the_second_page_then_backs_out_twice() {
    let mut h = Harness::new();
    // Infest01 32C1F5..3529C9: eighteen Next commands from row 1 reach the
    // page row at 19, Activate pushes 00535B70, Back pops, Back leaves.
    for _ in 0..18 {
        h.press(NEXT);
    }
    assert_eq!(h.selection(), 19);
    h.press(CONFIRM);
    assert_eq!(h.screen.menus().unwrap().len(), 2);
    assert_eq!(
        h.screen
            .menus()
            .unwrap()
            .current()
            .unwrap()
            .data()
            .source()
            .authored_address(),
        Some(KEY_CONFIG_PAGE_ADDRESSES[0])
    );
    h.press(BACK);
    assert_eq!(h.screen.menus().unwrap().len(), 1);
    assert!(h.effects.requests.is_empty());
    h.press(BACK);
    assert_eq!(h.effects.requests, [SessionPhase::Frontend]);
    // 00439560 ran on every tick.
    assert!(h.effects.animation > 0);
}

#[test]
fn a_capture_waits_for_release_then_binds_the_held_codes() {
    let mut h = Harness::new();
    h.press(NEXT); // Row 2: Down, action 1.
    h.hold(&[0x1c]); // Return still held when the capture starts.
    h.screen
        .digital_callback(
            InputCallback {
                address: CONFIRM,
                context: KEY_CONFIG_CALLBACK_CONTEXT,
                value: 1,
                timestamp_micros: 0,
            },
            &mut h.effects,
        )
        .unwrap();
    assert_eq!(
        h.screen.capture(),
        Some(KeyCapture {
            row: 1,
            armed: false
        })
    );
    assert!(h.screen.row_capturing(1));
    h.tick();
    // Still held: not armed, nothing bound.
    assert!(!h.screen.capture().unwrap().armed);
    // Navigation and Back are ignored during a capture.
    h.press(PREVIOUS);
    h.press(BACK);
    assert_eq!(h.selection(), 2);
    assert!(h.effects.requests.is_empty());
    h.hold(&[]);
    h.tick();
    assert!(h.screen.capture().unwrap().armed);
    // A chord binds every held code in ascending order.
    h.hold(&[0x2a, 0x25]);
    h.tick();
    assert_eq!(h.screen.capture(), None);
    assert!(!h.screen.row_capturing(1));
    assert_eq!(
        h.effects.bindings.record(1).unwrap().codes,
        [0x25, 0x2a, 0, 0]
    );
}

#[test]
fn five_held_keys_overrun_the_native_buffer() {
    assert!(matches!(
        list_codes(DeviceButtons([0x1f, 0, 0, 0, 0, 0, 0, 0])),
        Err(KeyConfigScreenError::CaptureOverrun(5))
    ));
    assert_eq!(
        list_codes(DeviceButtons([0, 1 << 5, 0, 0, 0, 0, 1 << 8, 0])).unwrap(),
        [37, 200, 0, 0]
    );
}

#[test]
fn restore_defaults_copies_the_default_records() {
    let mut h = Harness::new();
    h.effects.bindings.set_codes(0, [0x39, 0, 0, 0]).unwrap();
    // Root rows 1..17 are bindings; 18/19 the pages; 20 Restore Defaults.
    for _ in 0..19 {
        h.press(NEXT);
    }
    assert_eq!(h.selection(), 20);
    h.press(CONFIRM);
    assert_eq!(
        h.effects.bindings.record(0).unwrap(),
        h.effects.tables.defaults[0]
    );
}

#[test]
fn unknown_callbacks_and_contexts_fail_before_mutation() {
    let mut h = Harness::new();
    for address in [0x0044_3700, 0x0043_9260] {
        assert!(matches!(
            h.screen.digital_callback(
                InputCallback {
                    address,
                    context: KEY_CONFIG_CALLBACK_CONTEXT,
                    value: 1,
                    timestamp_micros: 0,
                },
                &mut h.effects
            ),
            Err(KeyConfigScreenError::UnknownCallback(found)) if found == address
        ));
    }
    assert!(matches!(
        h.screen.digital_callback(
            InputCallback {
                address: CONFIRM,
                context: 1,
                value: 1,
                timestamp_micros: 0,
            },
            &mut h.effects
        ),
        Err(KeyConfigScreenError::Context { .. })
    ));
}

#[test]
fn teardown_releases_the_screen() {
    let mut h = Harness::new();
    let id = h.screen.id().unwrap();
    h.screen.destroy(&mut h.registry, &mut h.input).unwrap();
    assert_eq!(h.screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert!(h.registry.screen(id).is_err());
    h.screen.destroy(&mut h.registry, &mut h.input).unwrap();
}
