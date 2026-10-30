//! 00459060/004592B0 ordering and 0045A8F0/callback routing over the retail
//! text-entry tables, with recording identity/device/effect hosts.

use super::*;
use crate::frontend_bindings::FrontendBindingPhase;
use crate::input::{DeviceButtons, DeviceSnapshot, InputHost};
use crate::name_entry::name_entry_rng_next;
use inf_formats::{encode_save_records, SaveRecord, SAVE_PAYLOAD_BYTES};
use std::path::Path;

const CONTEXT: u32 = 9;

fn assets() -> FrontendAssets {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    FrontendAssets::open(&root).expect("retail Infest.exe is required")
}

/// Uniquely named sidecar directory owned by one test; removed on drop.
struct ScratchDir(std::path::PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Store backed by an isolated sidecar; the retail original is scratch data.
fn open_store() -> (ScratchDir, ProfileStore) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.tmp/text-entry-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut payload = [0; SAVE_PAYLOAD_BYTES];
    payload[..6].copy_from_slice(b"Pilot\0");
    let record = SaveRecord::from_payload(payload).unwrap();
    let bytes = encode_save_records(&[record]).unwrap();
    std::fs::write(dir.join("original.sav"), &bytes).unwrap();
    std::fs::write(dir.join("portable.sav"), &bytes).unwrap();
    let store = ProfileStore::open(dir.join("portable.sav")).unwrap();
    (ScratchDir(dir), store)
}

struct Factory {
    events: Vec<String>,
    devices: Vec<FrontendDevice>,
    fail_identity: bool,
}

impl TextEntryFactoryHost for Factory {
    type Error = &'static str;
    fn read_identity(&mut self, phase: SessionPhase, _: u8) -> Result<LoadedIdentity, Self::Error> {
        self.events.push(format!("identity:{}", phase.id()));
        if self.fail_identity {
            return Err("identity reader unavailable");
        }
        Ok(LoadedIdentity {
            name: b"Pilot".to_vec(),
            initials: b"0up".to_vec(),
        })
    }
    fn enumerate_calibrated_devices(
        &mut self,
        _: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.events.push("devices".into());
        Ok(self.devices.clone())
    }
}

#[derive(Default)]
struct Effects {
    events: Vec<String>,
}

impl NameEntryActions for Effects {
    type Error = &'static str;
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error> {
        self.events.push(format!("cheat:{bit}"));
        Ok(())
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.events.push(format!("phase:{phase}"));
        Ok(())
    }
}

impl NameEntryCompletion for Effects {
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        assert!(matches!(
            request.initials,
            crate::name_entry::IdentityInitialsWrite::Write(_)
        ));
        self.events
            .push(format!("persist:{}", String::from_utf8_lossy(request.name)));
        Ok(())
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        Ok((MissionSaveState::new_game(), PlayerSaveState::default()))
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// Device updates only queue callbacks; polls here dispatch none.
struct Quiet;

impl InputHost for Quiet {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(0)
    }
    fn callback(&mut self, _: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        Err("no digital callback is expected in this fixture")
    }
}

/// Everything except the screen, so callbacks and ticks borrow disjoint parts.
struct World {
    assets: FrontendAssets,
    registry: ScreenRegistry<(), InputContextId>,
    input: InputRuntime,
    keyboard: DeviceId,
    joystick: DeviceId,
    keyboard_adapter: Option<DeviceId>,
    process: FrontendProcessState,
    text: TextEntryProcessState,
    store: ProfileStore,
    _dir: ScratchDir,
    effects: Effects,
    rng: u32,
    factory: Factory,
}

impl World {
    fn services(&mut self, poll_micros: u64) -> TextEntryServices<'_, Effects> {
        TextEntryServices {
            input: &mut self.input,
            store: &mut self.store,
            key_names: self.assets.key_names(),
            keyboard_adapter: self.keyboard_adapter,
            text_process: &mut self.text,
            frontend_process: &self.process,
            rng: &mut self.rng,
            effects: &mut self.effects,
            phase: 9,
            poll_micros,
        }
    }

    fn keys(&mut self, codes: &[u8]) {
        let mut buttons = DeviceButtons::default();
        for code in codes {
            buttons.set(*code, true);
        }
        self.input
            .update_device(
                self.keyboard,
                DeviceSnapshot {
                    buttons,
                    axes: [0; 8],
                },
                &mut Quiet,
            )
            .unwrap();
    }
}

struct Fixture {
    world: World,
    screen: TextEntryScreen,
}

impl Fixture {
    fn new() -> Self {
        let assets = assets();
        let mut input = InputRuntime::default();
        let keyboard = input.create_device(DeviceButtons::default()).unwrap();
        let joystick = input.create_device(DeviceButtons::default()).unwrap();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(()),
        };
        let screen = TextEntryScreen::new(
            TextEntryScreenConfig {
                attachment,
                callback_context: CONTEXT,
            },
            &assets,
        );
        let (dir, store) = open_store();
        Self {
            world: World {
                assets,
                registry,
                input,
                keyboard,
                joystick,
                keyboard_adapter: Some(keyboard),
                process: FrontendProcessState::default(),
                text: TextEntryProcessState::default(),
                store,
                _dir: dir,
                effects: Effects::default(),
                rng: 0,
                factory: Factory {
                    events: Vec::new(),
                    devices: vec![
                        FrontendDevice {
                            native_type: 0,
                            source: keyboard,
                        },
                        FrontendDevice {
                            native_type: 2,
                            source: joystick,
                        },
                    ],
                    fail_identity: false,
                },
            },
            screen,
        }
    }

    fn try_initialize(
        &mut self,
        requested: SessionPhase,
    ) -> Result<(), TextEntryScreenError<&'static str>> {
        let world = &mut self.world;
        self.screen.initialize(
            requested,
            &mut TextEntryFactoryServices {
                registry: &mut world.registry,
                input: &mut world.input,
                assets: &world.assets,
                process: &world.process,
                local_slot: 1,
            },
            &mut world.factory,
        )
    }

    fn initialize(&mut self) {
        self.try_initialize(SessionPhase::Phase9).unwrap();
    }

    fn event(
        &mut self,
        address: u32,
        context: u32,
        value: u32,
    ) -> Result<Option<NameCompletion>, TextEntryScreenError<&'static str>> {
        let event = InputCallback {
            address,
            context,
            value,
            timestamp_micros: 0,
        };
        self.screen
            .digital_callback(event, &mut self.world.services(256_000))
    }

    fn callback(
        &mut self,
        address: u32,
        value: u32,
    ) -> Result<Option<NameCompletion>, TextEntryScreenError<&'static str>> {
        self.event(address, CONTEXT, value)
    }

    fn tick_split(
        &mut self,
        delta_micros: i32,
        elapsed_micros: i32,
        poll_micros: u64,
    ) -> Result<Vec<NameCompletion>, TextEntryScreenError<&'static str>> {
        self.screen.tick(
            TextEntryTick {
                scheduler: SchedulerTick {
                    delta_micros,
                    elapsed_micros,
                },
                attract: false,
            },
            &mut self.world.services(poll_micros),
        )
    }

    fn tick(
        &mut self,
        delta_micros: i32,
        poll_micros: u64,
    ) -> Result<Vec<NameCompletion>, TextEntryScreenError<&'static str>> {
        self.tick_split(delta_micros, delta_micros, poll_micros)
    }

    fn selection(&self) -> i32 {
        self.screen.entry().unwrap().selection()
    }
}

#[test]
fn construction_follows_00459060_and_selects_the_text_entry_context() {
    let mut fixture = Fixture::new();
    fixture.world.process.advance_animation(80_000);
    assert_ne!(fixture.world.process.animation(), Default::default());
    fixture.initialize();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    // The identity read precedes device enumeration and binding install.
    assert_eq!(fixture.world.factory.events, ["identity:9", "devices"]);
    let entry = fixture.screen.entry().unwrap();
    assert_eq!(entry.name(), b"Pilot");
    assert_eq!(entry.selection(), 77);
    assert_eq!(entry.loaded().initials, b"0up");
    let id = fixture.screen.id().unwrap();
    assert_eq!(
        fixture.world.registry.screen(id).unwrap().membership,
        ScreenMembership::Linked
    );
    assert_eq!(
        fixture.world.input.selected_context(),
        fixture.screen.bindings().context()
    );
    // 00439530 resets the shared animation clock during construction.
    assert_eq!(fixture.world.process.animation(), Default::default());
    assert!(text_entry_reuses_phase(SessionPhase::Phase9));
    assert!(text_entry_reuses_phase(SessionPhase::Phase11));
    assert!(!text_entry_reuses_phase(SessionPhase::Frontend));
    assert!(!text_entry_reuses_phase(SessionPhase::Phase12));
}

#[test]
fn other_requested_phases_are_rejected_before_any_acquisition() {
    let mut fixture = Fixture::new();
    assert!(matches!(
        fixture.try_initialize(SessionPhase::Frontend),
        Err(TextEntryScreenError::RequestedPhase(SessionPhase::Frontend))
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::New);
    assert_eq!(fixture.screen.id(), None);
    assert!(fixture.world.factory.events.is_empty());
    assert_eq!(fixture.world.input.selected_context(), None);
}

#[test]
fn destroy_releases_bindings_membership_and_screen() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    let id = fixture.screen.id().unwrap();
    fixture
        .screen
        .destroy(&mut fixture.world.registry, &mut fixture.world.input)
        .unwrap();
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Destroyed);
    assert_eq!(
        fixture.screen.bindings().phase(),
        FrontendBindingPhase::Released
    );
    assert_eq!(fixture.screen.bindings().consumer_cc(), None);
    assert_eq!(fixture.screen.bindings().consumer_d0(), None);
    assert!(fixture.world.registry.screen(id).is_err());
    assert_eq!(fixture.world.input.selected_context(), None);
    // Destroying twice is a no-op.
    fixture
        .screen
        .destroy(&mut fixture.world.registry, &mut fixture.world.input)
        .unwrap();
}

#[test]
fn failed_construction_leaves_acquisitions_for_explicit_teardown() {
    let mut fixture = Fixture::new();
    fixture.world.factory.fail_identity = true;
    assert!(matches!(
        fixture.try_initialize(SessionPhase::Phase9),
        Err(TextEntryScreenError::Host("identity reader unavailable"))
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
    let id = fixture.screen.id().unwrap();
    assert_eq!(
        fixture.world.registry.screen(id).unwrap().membership,
        ScreenMembership::Linked
    );
    fixture
        .screen
        .destroy(&mut fixture.world.registry, &mut fixture.world.input)
        .unwrap();
    assert!(fixture.world.registry.screen(id).is_err());
    assert_eq!(
        fixture.screen.bindings().phase(),
        FrontendBindingPhase::Released
    );
}

#[test]
fn held_callbacks_route_to_the_name_entry_slots() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    // Up (00445220 -> slot +28) from cell 77.
    fixture.callback(0x0044_5220, 1).unwrap();
    assert_eq!(fixture.selection(), 51);
    fixture.callback(0x0044_5220, 0).unwrap();
    // Return (00445200 -> slot +24) appends the grid byte at cell 51 ('z').
    fixture.callback(0x0044_5200, 1).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().name(), b"Pilotz");
    fixture.callback(0x0044_5200, 0).unwrap();
    // Left (00445260 -> +30) and Right (00445280 -> +34) wrap within row 2.
    fixture.callback(0x0044_5260, 1).unwrap();
    assert_eq!(fixture.selection(), 50);
    assert_eq!(fixture.screen.entry().unwrap().row_anim(), -2520);
    fixture.callback(0x0044_5260, 0).unwrap();
    fixture.callback(0x0044_5280, 1).unwrap();
    assert_eq!(fixture.selection(), 51);
    assert_eq!(fixture.screen.entry().unwrap().row_anim(), 0);
    fixture.callback(0x0044_5280, 0).unwrap();
    // Esc (004451E0 -> slot +38) requests phase 2.
    fixture.callback(0x0044_51E0, 1).unwrap();
    assert_eq!(fixture.world.effects.events, ["phase:2"]);
}

#[test]
fn tab_and_retry_follow_the_error_gate_and_fire_on_press_only() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    fixture.callback(CONFIRM_NAME_CALLBACK, 0).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().completion_delay(), 0);
    // R with the error byte clear does nothing.
    assert!(fixture.callback(RETRY_NAME_CALLBACK, 1).unwrap().is_none());
    assert!(fixture.world.effects.events.is_empty());
    fixture.screen.entry.as_mut().unwrap().set_error(true);
    // Tab is suppressed while the error byte is set (0045AC00).
    fixture.callback(CONFIRM_NAME_CALLBACK, 1).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().completion_delay(), 0);
    // An R release is ignored; an R press reruns completion without phase 3.
    assert!(fixture.callback(RETRY_NAME_CALLBACK, 0).unwrap().is_none());
    assert!(fixture.world.effects.events.is_empty());
    assert!(matches!(
        fixture.callback(RETRY_NAME_CALLBACK, 1).unwrap(),
        Some(NameCompletion::ErrorPending)
    ));
    assert_eq!(fixture.world.effects.events, ["persist:Pilot"]);
    fixture.screen.entry.as_mut().unwrap().set_error(false);
    fixture.callback(CONFIRM_NAME_CALLBACK, 1).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().completion_delay(), 1000);
}

#[test]
fn system_and_foreign_callbacks_fail_without_mutation() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    // The shared Alt+Enter system record has no owner in this screen.
    assert!(matches!(
        fixture.callback(0x0043_7D30, 1),
        Err(TextEntryScreenError::ScreenInput(
            ScreenInputError::UnknownCallback(0x0043_7D30)
        ))
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert!(matches!(
        fixture.event(0x0044_5220, CONTEXT + 1, 1),
        Err(TextEntryScreenError::Context { .. })
    ));
    assert_eq!(fixture.selection(), 77);
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
}

#[test]
fn tick_types_through_the_translator_and_advances_shared_state() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    fixture.world.keys(&[0x10]);
    fixture.tick(40_000, 256_000).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().name(), b"PilotQ");
    assert!(fixture.world.text.key_latch.held());
    // 00439560 advanced the shared clock; sixteen RNG draws moved the word.
    assert_eq!(fixture.world.process.animation().value(), 2);
    let mut expected = 0u32;
    for _ in 0..16 {
        name_entry_rng_next(&mut expected);
    }
    assert_eq!(fixture.world.rng, expected);
    // Holding the key reports nothing more; releasing clears the latch.
    fixture.tick(16_000, 256_000).unwrap();
    assert_eq!(fixture.screen.entry().unwrap().name(), b"PilotQ");
    fixture.world.keys(&[]);
    fixture.tick(16_000, 256_000).unwrap();
    assert!(!fixture.world.text.key_latch.held());
}

#[test]
fn idle_ticks_snap_to_confirm_when_the_poll_clock_byte_is_zero() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    // Down (00445240) from 77 moves to 25 and starts the 250 ms repeat.
    fixture.callback(0x0044_5240, 1).unwrap();
    fixture.callback(0x0044_5240, 0).unwrap();
    assert_eq!(fixture.selection(), 25);
    // A nonzero stale byte leaves the selection alone.
    fixture.tick(16_000, 256_000).unwrap();
    assert_eq!(fixture.selection(), 25);
    // Bits 8..15 of the poll's millisecond clock are zero: snap to 77.
    fixture.tick(16_000, 0).unwrap();
    assert_eq!(fixture.selection(), 77);
}

#[test]
fn held_slots_repeat_on_the_capped_delta_and_age_on_elapsed_time() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    fixture.callback(0x0044_5240, 1).unwrap();
    assert_eq!(fixture.selection(), 25);
    // 004392B0 ages by the uncapped elapsed time and counts the 250 ms repeat
    // down by the capped delta: 125 ms leaves 125 ms, 300 ms of age.
    fixture.tick_split(125_000, 300_000, 256_000).unwrap();
    assert_eq!(fixture.selection(), 25);
    assert_eq!(fixture.screen.screen_input().age_millis(), 300);
    assert_eq!(fixture.screen.screen_input().repeat_millis(), 125);
    fixture.tick_split(125_000, 125_000, 256_000).unwrap();
    assert_eq!(fixture.selection(), 51);
}

#[test]
fn analog_values_reach_the_slots_through_base_input() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    // The type-2 authored Y gain is -1/8: -24008 publishes +D0 = 3001 on the
    // next poll, above 004392B0's 2500 threshold, which presses bit 2
    // (Previous/Up) through 00439430.
    let joystick = fixture.world.joystick;
    fixture
        .world
        .input
        .update_device(
            joystick,
            DeviceSnapshot {
                axes: [0, -24_008, 0, 0, 0, 0, 0, 0],
                ..DeviceSnapshot::default()
            },
            &mut Quiet,
        )
        .unwrap();
    fixture.world.input.poll(&mut Quiet).unwrap();
    assert_eq!(
        fixture
            .screen
            .bindings()
            .analog_values(&fixture.world.input)
            .unwrap()
            .consumer_d0,
        3001
    );
    fixture.tick(16_000, 256_000).unwrap();
    assert_eq!(fixture.selection(), 51);
    assert!(fixture.screen.screen_input().analog_latched());
}

#[test]
fn missing_keyboard_adapter_is_an_explicit_error() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    // No screen in this process has cached a type-0 adapter.
    fixture.world.keyboard_adapter = None;
    assert!(matches!(
        fixture.tick(16_000, 256_000),
        Err(TextEntryScreenError::MissingKeyboard)
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
}

#[path = "text_entry_frame_tests.rs"]
mod frame_tests;
