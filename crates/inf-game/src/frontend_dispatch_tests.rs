//! Retail authored tables with a recording effects fixture. Unsupported
//! specialized menus/actions return errors; no rendering or audio is claimed.

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindings, FrontendDevice};
use crate::frontend_camera::FrontendEntry;
use crate::frontend_control::{FrontendControlConfig, QUIT_MENU_ADDRESS, ROOT_MENU_ADDRESS};
use crate::input::{DeviceButtons, DeviceSnapshot, InputHost, InputRuntime};
use crate::menu::{
    MenuCommand, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
    MenuRowDrawRequest, MenuState,
};

#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Music,
    Sound(i16),
    Persist,
    Transition,
    ProfileFailure(ProfileOperation),
    Network,
    Adjust(u32, MenuCommand, u32),
    AdjustDisplay(u32, MenuCommand, u32),
    RefreshWorld,
    RestoreDefaults,
    Volume(VolumeChannel),
    Phase(u32),
    Attract(bool),
    Exit,
    External(MenuActionRequest),
}

struct RecordingEffects {
    calls: Vec<Effect>,
    fail_sound: bool,
    settings_words: [u32; 34],
    reinit_calls: Vec<u32>,
    fail_reinit: bool,
}

impl Default for RecordingEffects {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            fail_sound: false,
            settings_words: [0; 34],
            reinit_calls: Vec::new(),
            fail_reinit: false,
        }
    }
}

impl FrontendEffects for RecordingEffects {
    type Error = &'static str;
    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::Music);
        Ok(())
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.calls.push(Effect::Sound(resource));
        if self.fail_sound {
            Err("audio unavailable")
        } else {
            Ok(())
        }
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::Persist);
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::Transition);
        Ok(())
    }
    fn profile_operation_failed(
        &mut self,
        operation: ProfileOperation,
        _: &ProfileStoreError,
    ) -> Result<(), Self::Error> {
        self.calls.push(Effect::ProfileFailure(operation));
        Ok(())
    }
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        self.calls.push(Effect::Network);
        Err("specialized network owner unavailable")
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::RefreshWorld);
        Ok(())
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::RestoreDefaults);
        Ok(())
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.calls.push(Effect::Adjust(address, command, max));
        let index = crate::frontend_settings::setting_word_index(address)
            .ok_or("settings cell outside the test state")?;
        crate::settings_row::adjust_value(command, max, &mut self.settings_words[index]);
        Ok(())
    }
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.calls
            .push(Effect::AdjustDisplay(address, command, max));
        let words = &mut self.settings_words;
        let reinit_calls = &mut self.reinit_calls;
        let fail_reinit = self.fail_reinit;
        crate::settings_row::adjust_display_setting(words, address, command, max, |variant, _| {
            reinit_calls.push(variant);
            if fail_reinit {
                return Err("backend reset unconnected");
            }
            Ok(())
        })
        .map(|_| ())
        .map_err(|_| "display adjust failed")
    }
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error> {
        self.calls.push(Effect::Volume(channel));
        Ok(())
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.calls.push(Effect::Phase(phase));
        Ok(())
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.calls.push(Effect::Attract(enabled));
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Effect::Exit);
        Ok(())
    }
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        self.calls.push(Effect::External(request));
        Err("external action unavailable")
    }
}

struct Fixture {
    catalog: FrontendMenuCatalog,
    basis: MenuBasisMath,
    control: FrontendControl,
    input: ScreenInput,
    effects: RecordingEffects,
    runtime_input: InputRuntime,
    profiles: Option<ProfileStore>,
    visibility: MenuVisibility,
}

/// Record the genuine controller-setup boundary while initial fade is zero.
/// No rows should be requested; a row request is an unavailable-renderer error.
#[derive(Default)]
struct InitialDraw {
    setups: Vec<MenuControllerDrawSetup>,
}

impl MenuDrawHost for InitialDraw {
    type Error = &'static str;
    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.setups.push(setup);
        Ok(())
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), Self::Error> {
        Err("unexpected initial basis publication")
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Err("unexpected initial clipping")
    }
    fn set_clip_rect(&mut self, _: [i16; 4]) -> Result<(), Self::Error> {
        Err("unexpected initial clipping")
    }
    fn colors(&self) -> MenuDrawColors {
        panic!("no rows before the initial fade")
    }
    fn draw_row(
        &mut self,
        _: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Self::Error> {
        Err("renderer unavailable")
    }
}

impl Fixture {
    fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).expect("retail frontend executable is required");
        let basis = MenuBasisMath::new(assets.sine_table().values);
        let config = FrontendControlConfig {
            camera_presets: assets.camera_presets().to_vec(),
            position_scale: assets.position_scale(),
            motion_scale: assets.motion_scale(),
            entry: FrontendEntry::Returning,
            context: 77,
        };
        let mut catalog = FrontendMenuCatalog::new(assets);
        let mut control = FrontendControl::new(config).unwrap();
        control.menus_mut().push(MenuState::new(
            catalog.menu_data(ROOT_MENU_ADDRESS).unwrap(),
            MenuBasis::INITIAL,
        ));
        Self {
            catalog,
            basis,
            control,
            input: ScreenInput::new(ScreenDispatchPolicy::Frontend, 77),
            effects: RecordingEffects::default(),
            runtime_input: InputRuntime::new(0),
            profiles: None,
            visibility: MenuVisibility::OwnerFlagClear,
        }
    }

    fn event(
        &mut self,
        address: u32,
        value: u32,
    ) -> Result<(), FrontendDispatchError<&'static str>> {
        self.callback(InputCallback {
            address,
            value,
            context: 77,
            timestamp_micros: 1234,
        })
    }

    fn callback(
        &mut self,
        event: InputCallback,
    ) -> Result<(), FrontendDispatchError<&'static str>> {
        let mut dispatch = FrontendDispatch {
            catalog: &mut self.catalog,
            basis: &self.basis,
            visibility: self.visibility,
            effects: &mut self.effects,
            profiles: self.profiles.as_mut(),
        };
        dispatch.digital_callback(&mut self.control, &mut self.input, event)
    }

    fn tick(
        &mut self,
        analog: ScreenAnalogValues,
        tick: SchedulerTick,
    ) -> Result<(), FrontendDispatchError<&'static str>> {
        let mut dispatch = FrontendDispatch {
            catalog: &mut self.catalog,
            basis: &self.basis,
            visibility: self.visibility,
            effects: &mut self.effects,
            profiles: self.profiles.as_mut(),
        };
        dispatch.tick(
            &mut self.control,
            &mut self.input,
            &mut self.runtime_input,
            analog,
            tick,
        )
    }

    fn record_initial_draw(&mut self) {
        let mut host = InitialDraw::default();
        let result = self
            .control
            .draw_menu(
                MenuDrawInput {
                    surface_pixels: [800, 600],
                    visibility: MenuVisibility::OwnerFlagClear,
                    sine_quarter: &self.catalog.assets().sine_table().values,
                },
                &mut host,
            )
            .unwrap()
            .unwrap();
        assert_eq!(host.setups.len(), 1);
        assert_eq!(result.rows, 0);
        assert!(self.control.menus().drawn());
    }
}

fn frame() -> SchedulerTick {
    SchedulerTick {
        delta_micros: 16_000,
        elapsed_micros: 16_000,
    }
}

fn profile_fixture(missions: &[u32]) -> Fixture {
    use inf_formats::{encode_save_records, SaveRecord, SAVE_PAYLOAD_BYTES};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.tmp/profile-dispatch-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let records = missions
        .iter()
        .map(|mission| {
            let mut payload = [0; SAVE_PAYLOAD_BYTES];
            payload[..6].copy_from_slice(b"Pilot\0");
            payload[6..10].copy_from_slice(&mission.to_le_bytes());
            SaveRecord::from_payload(payload).unwrap()
        })
        .collect::<Vec<_>>();
    let bytes = encode_save_records(&records).unwrap();
    let original = directory.join("original.sav");
    let sidecar = directory.join("portable.sav");
    std::fs::write(&original, &bytes).unwrap();
    std::fs::write(&sidecar, &bytes).unwrap();
    let mut fixture = Fixture::new();
    fixture.profiles = Some(ProfileStore::open(&sidecar).unwrap());
    fixture.record_initial_draw();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert_eq!(fixture.control.menus().len(), 2);
    fixture
}

/// Record only the native busy draw's successful preflight/handshake. It
/// must request no row glyphs, so every rendering hook remains an error.
struct BusyDraw<'a> {
    store: &'a mut ProfileStore,
}
impl MenuDrawHost for BusyDraw<'_> {
    type Error = &'static str;
    fn begin_controller(&mut self, _: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        Ok(())
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), Self::Error> {
        Err("unexpected basis")
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Err("unexpected clip")
    }
    fn set_clip_rect(&mut self, _: [i16; 4]) -> Result<(), Self::Error> {
        Err("unexpected clip")
    }
    fn colors(&self) -> MenuDrawColors {
        panic!("busy draw has no rows")
    }
    fn draw_row(
        &mut self,
        _: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Self::Error> {
        Err("unexpected row")
    }
    fn begin_profile_draw(&mut self) -> Result<(), crate::menu::MenuError<Self::Error>> {
        Ok(())
    }
    fn profile_store(&mut self) -> Result<&mut ProfileStore, crate::menu::MenuError<Self::Error>> {
        Ok(self.store)
    }
}

fn acknowledge_busy_draw(fixture: &mut Fixture) {
    let outcome = fixture
        .control
        .draw_menu(
            MenuDrawInput {
                surface_pixels: [640, 480],
                visibility: fixture.visibility,
                sine_quarter: &fixture.catalog.assets().sine_table().values,
            },
            &mut BusyDraw {
                store: fixture.profiles.as_mut().unwrap(),
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(outcome.rows, 0);
    assert!(outcome.menu_drawn);
}

#[test]
fn real_profile_open_waits_for_draw_then_rebuild_resets_live_input_queue() {
    use crate::input::{ActionDescriptor, EventPacket};
    let mut fixture = profile_fixture(&[2]);
    let consumer = fixture
        .runtime_input
        .acquire_consumer(
            ActionDescriptor {
                identity: 123,
                callback_address: None,
                flags: 0,
            },
            77,
        )
        .unwrap();
    fixture
        .runtime_input
        .emit(consumer, 1, &mut EventPacket::default(), &mut RejectDigital)
        .unwrap();
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert_eq!(fixture.runtime_input.queued_len(), 1);
    assert_eq!(fixture.profiles.as_ref().unwrap().cached_count(), 0);
    acknowledge_busy_draw(&mut fixture);
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert_eq!(fixture.runtime_input.queued_len(), 0);
    assert_eq!(fixture.profiles.as_ref().unwrap().cached_count(), 1);
    let base = fixture.control.menus().current().unwrap();
    assert_eq!(base.selection(), 1);
    assert_eq!(base.age_millis(), 0);
    assert_eq!(base.basis.words[0], 1);
    assert_eq!(base.data().records().items.len(), 15);
    assert!(!fixture
        .effects
        .calls
        .iter()
        .any(|effect| matches!(effect, Effect::Phase(_))));
}

#[test]
fn profile_action_updates_live_camera_timer_and_dispatches_delayed_load() {
    let mut fixture = profile_fixture(&[2]);
    acknowledge_busy_draw(&mut fixture);
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    fixture.effects.calls.clear();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [Effect::Transition, Effect::Sound(5)]
    );
    assert_eq!(fixture.control.camera().selected_preset(), 9);
    assert_eq!(fixture.control.timer_0120_millis(), 1);
    assert!(!fixture.control.camera().ready());
    for _ in 0..4096 {
        fixture
            .tick(ScreenAnalogValues::default(), frame())
            .unwrap();
        if fixture.effects.calls.contains(&Effect::Phase(3)) {
            break;
        }
    }
    assert!(fixture.effects.calls.contains(&Effect::Phase(3)));
    assert_eq!(fixture.profiles.as_ref().unwrap().selected(), 0);
    // 0044A170 loads the working record without assigning the current-name
    // field; its bytes stay unknown until an actual name writer runs.
    assert_eq!(fixture.profiles.as_ref().unwrap().current_name(), None);
    assert_eq!(
        fixture
            .profiles
            .as_ref()
            .unwrap()
            .working_record()
            .unwrap()
            .name(),
        b"Pilot"
    );
    assert!(!fixture.control.stopped());
}

#[test]
fn profile_owner_selection_fades_live_controller_without_early_pop() {
    let mut fixture = profile_fixture(&[2]);
    fixture.visibility = MenuVisibility::OwnerFlagSet;
    acknowledge_busy_draw(&mut fixture);
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert!(fixture
        .control
        .menus()
        .current()
        .unwrap()
        .navigation_locked());
    fixture.event(0x0048_5300, 1).unwrap();
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert!(!fixture.control.menus().active());
    assert_eq!(fixture.control.menus().len(), 2);
    assert_eq!(
        fixture.profiles.as_ref().unwrap().control_token(),
        crate::profile_store::ProfileControlToken::MinusOne
    );
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert!(fixture.control.menus().is_empty());
    assert!(!fixture
        .effects
        .calls
        .iter()
        .any(|effect| matches!(effect, Effect::Phase(_))));
}

#[test]
fn authored_options_action_uses_shared_catalog_and_camera_owner() {
    let mut fixture = Fixture::new();
    let root = fixture.catalog.menu_data(ROOT_MENU_ADDRESS).unwrap();
    root.records_mut().items[0].row_height = 237;
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    assert_eq!(fixture.control.menus().current().unwrap().selection(), 1);
    fixture.event(0x0044_5200, 1).unwrap();
    assert_eq!(fixture.control.menus().len(), 2);
    assert_eq!(fixture.control.camera().selected_preset(), 2);
    let child_address = root.records().items[1].action_arguments[1];
    assert_eq!(
        fixture
            .control
            .menus()
            .current()
            .unwrap()
            .data()
            .records()
            .source
            .authored_address()
            .unwrap(),
        child_address
    );
    let shared = fixture.catalog.menu_data(child_address).unwrap();
    shared.records_mut().items[0].row_height = 345;
    assert_eq!(
        fixture
            .control
            .menus()
            .current()
            .unwrap()
            .data()
            .records()
            .items[0]
            .row_height,
        345
    );
    assert_eq!(root.records().items[0].row_height, 237);
    assert_eq!(fixture.effects.calls, [Effect::Sound(2), Effect::Sound(5)]);
}

#[test]
fn retail_settings_row_adjusts_the_live_settings_word() {
    use MenuCommand::{AdjustNext, AdjustPrevious};
    let mut fixture = Fixture::new();
    // Root selection 0 -> 1 (Options Table), then Confirm pushes 0x539378.
    // Every press pairs with its release: a second press of a held button
    // never dispatches.
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.control.menus().len(), 2);
    // The Options push retargets the camera; wait for its gate.
    for _ in 0..5000 {
        if fixture.control.camera().ready() {
            break;
        }
        fixture
            .tick(ScreenAnalogValues::default(), frame())
            .unwrap();
    }
    assert!(fixture.control.camera().ready());
    // Options initial selection is 1, whose child push opens 0x539088.
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.control.menus().len(), 3);
    assert_eq!(
        fixture
            .control
            .menus()
            .current()
            .unwrap()
            .data()
            .records()
            .source
            .authored_address()
            .unwrap(),
        0x0053_9088
    );
    fixture.effects.calls.clear();
    // Submenu initial selection is 1: draw 00453AE0, action 00453B80 max 1.
    // Word 24 starts zeroed in the test state.
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [Effect::Adjust(0x54_E908, AdjustNext, 1), Effect::Sound(3),]
    );
    assert_eq!(fixture.effects.settings_words[24], 1);
    // Clamped at the row maximum, then decremented, then wrapped by Confirm.
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(fixture.effects.settings_words[24], 1);
    fixture.event(0x0044_5260, 1).unwrap();
    fixture.event(0x0044_5260, 0).unwrap();
    assert!(fixture
        .effects
        .calls
        .contains(&Effect::Adjust(0x54_E908, AdjustPrevious, 1)));
    assert_eq!(fixture.effects.settings_words[24], 0);
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.effects.settings_words[24], 1);
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.effects.settings_words[24], 0);
    // Selection 8 is the first 00463B50 row: adjust plus world refresh.
    for _ in 0..7 {
        fixture.event(0x0044_5240, 1).unwrap();
        fixture.event(0x0044_5240, 0).unwrap();
    }
    assert_eq!(fixture.control.menus().current().unwrap().selection(), 8);
    fixture.effects.calls.clear();
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [
            Effect::Adjust(0x54_E918, AdjustNext, 1),
            Effect::RefreshWorld,
            Effect::Sound(3),
        ]
    );
    assert_eq!(fixture.effects.settings_words[28], 1);
    // Selection 5 is the first 00462F40 row: the zeroed variant word forces
    // to 1 with no backend reset, then a set variant reinitializes. The
    // 0x445220 address navigates; 0x445260 would adjust in place instead.
    for _ in 0..3 {
        fixture.event(0x0044_5220, 1).unwrap();
        fixture.event(0x0044_5220, 0).unwrap();
    }
    assert_eq!(fixture.control.menus().current().unwrap().selection(), 5);
    fixture.effects.calls.clear();
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [
            Effect::AdjustDisplay(0x54_E8E4, AdjustNext, 1),
            Effect::Sound(3),
        ]
    );
    assert_eq!(fixture.effects.settings_words[15], 1);
    assert_eq!(fixture.effects.settings_words[3], 1);
    assert!(fixture.effects.reinit_calls.is_empty());
    fixture.event(0x0044_5260, 1).unwrap();
    fixture.event(0x0044_5260, 0).unwrap();
    assert_eq!(fixture.effects.settings_words[15], 0);
    assert_eq!(fixture.effects.reinit_calls, [1]);
    // A failing backend reset restores the row cell and fails the event.
    fixture.effects.fail_reinit = true;
    assert!(fixture.event(0x0044_5280, 1).is_err());
    assert_eq!(fixture.effects.settings_words[15], 0);
}

#[test]
fn retail_second_settings_submenu_adjusts_its_own_word() {
    use MenuCommand::AdjustNext;
    let mut fixture = Fixture::new();
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.control.menus().len(), 2);
    for _ in 0..5000 {
        if fixture.control.camera().ready() {
            break;
        }
        fixture
            .tick(ScreenAnalogValues::default(), frame())
            .unwrap();
    }
    assert!(fixture.control.camera().ready());
    // Options selection 1 -> 3, whose child push opens 0x539188. Its second
    // item pairs the same 00453AE0/00453B80 callbacks with another cell, so
    // the owners must generalize beyond the first submenu's addresses.
    for _ in 0..2 {
        fixture.event(0x0044_5240, 1).unwrap();
        fixture.event(0x0044_5240, 0).unwrap();
    }
    assert_eq!(fixture.control.menus().current().unwrap().selection(), 3);
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.control.menus().len(), 3);
    assert_eq!(
        fixture
            .control
            .menus()
            .current()
            .unwrap()
            .data()
            .records()
            .source
            .authored_address()
            .unwrap(),
        0x0053_9188
    );
    fixture.effects.calls.clear();
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [Effect::Adjust(0x54_E924, AdjustNext, 1), Effect::Sound(3),]
    );
    assert_eq!(fixture.effects.settings_words[31], 1);
}

#[test]
fn retail_sound_submenu_adjusts_then_applies_each_channel() {
    use MenuCommand::{AdjustNext, AdjustPrevious};
    let mut fixture = Fixture::new();
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    for _ in 0..5000 {
        if fixture.control.camera().ready() {
            break;
        }
        fixture
            .tick(ScreenAnalogValues::default(), frame())
            .unwrap();
    }
    assert!(fixture.control.camera().ready());
    // Options selection 2 pushes the authored Sound menu at 0x538E68.
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    let current = fixture.control.menus().current().unwrap();
    assert_eq!(
        current.data().records().source.authored_address().unwrap(),
        0x0053_8E68
    );
    assert_eq!(current.selection(), 1);
    fixture.effects.calls.clear();
    fixture.event(0x0044_5280, 1).unwrap();
    fixture.event(0x0044_5280, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [
            Effect::Adjust(0x54_E8A8, AdjustNext, 15),
            Effect::Volume(VolumeChannel::Effects),
            Effect::Sound(3),
        ]
    );
    assert_eq!(fixture.effects.settings_words[0], 1);
    fixture.event(0x0044_5240, 1).unwrap();
    fixture.event(0x0044_5240, 0).unwrap();
    fixture.effects.calls.clear();
    fixture.event(0x0044_5260, 1).unwrap();
    fixture.event(0x0044_5260, 0).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [
            Effect::Adjust(0x54_E8AC, AdjustPrevious, 15),
            Effect::Volume(VolumeChannel::Music),
            Effect::Sound(3),
        ]
    );
    // The zeroed music word floors at zero; the tail still runs.
    assert_eq!(fixture.effects.settings_words[1], 0);
}

#[test]
fn back_clears_the_live_dispatch_mask_before_auxiliary_exit_can_run() {
    let mut fixture = Fixture::new();
    // The root Start row ignores command6, leaving its held bit set. If Back
    // failed to clear this live owner, the newly opened Quit row would exit.
    fixture.event(0x0048_5300, 1).unwrap();
    assert_eq!(fixture.input.held_mask(), 0x40);
    fixture.event(0x0044_51e0, 1).unwrap();
    assert_eq!(fixture.input.held_mask(), 0);
    assert_eq!(fixture.input.repeat_millis(), 250);
    assert_eq!(
        fixture
            .control
            .menus()
            .current()
            .unwrap()
            .data()
            .records()
            .source
            .authored_address()
            .unwrap(),
        QUIT_MENU_ADDRESS
    );
    assert_eq!(fixture.effects.calls, [Effect::Persist]);
    // A separate later Auxiliary6 press really reaches the required exit effect.
    fixture.event(0x0048_5300, 1).unwrap();
    assert_eq!(fixture.effects.calls, [Effect::Persist, Effect::Exit]);
}

struct RejectDigital;
impl InputHost for RejectDigital {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(1234)
    }
    fn callback(&mut self, _: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        Err("unexpected digital callback in analog fixture")
    }
}

#[test]
fn published_binding_values_drive_nested_base_input_before_music_and_real_basis_rotation() {
    let mut fixture = Fixture::new();
    fixture.record_initial_draw();
    let mut runtime = InputRuntime::default();
    let source = runtime.create_device(DeviceButtons::default()).unwrap();
    let mut bindings =
        FrontendBindings::new(77, fixture.catalog.assets().screen_analog_descriptors());
    bindings.initialize(&mut runtime).unwrap();
    bindings
        .install(
            &mut runtime,
            fixture.catalog.assets().frontend_input_tables(),
            &[FrontendDevice {
                native_type: 2,
                source,
            }],
        )
        .unwrap();
    runtime.select_context(bindings.context()).unwrap();
    runtime
        .update_device(
            source,
            DeviceSnapshot {
                axes: [0, -24_008, 0, 0, 0, 0, 0, 0],
                ..DeviceSnapshot::default()
            },
            &mut RejectDigital,
        )
        .unwrap();
    assert_eq!(bindings.analog_values(&runtime).unwrap().consumer_d0, 0);
    runtime.poll(&mut RejectDigital).unwrap();
    let values = bindings.analog_values(&runtime).unwrap();
    assert_eq!(values.consumer_d0, 3001);
    let old_basis = fixture.control.menus().current().unwrap().basis;
    fixture.tick(values, frame()).unwrap();
    assert_eq!(
        fixture.effects.calls,
        [Effect::Sound(2), Effect::Music, Effect::Attract(false)]
    );
    assert_eq!(fixture.input.age_millis(), 16);
    assert_eq!(fixture.input.repeat_millis(), 234);
    let menu = fixture.control.menus().current().unwrap();
    assert_eq!(menu.basis.words[0], 1);
    assert_ne!(menu.basis.words[1..], old_basis.words[1..]);
    assert_eq!(menu.age_millis(), 16);
    assert_eq!(fixture.control.animation().accumulated_micros(), 16_000);
    bindings.release_sets(&mut runtime).unwrap();
    bindings.release_base(&mut runtime).unwrap();
}

#[test]
fn deferred_start_requires_specialized_owner_after_menu_tick_and_does_not_request_phase() {
    let mut fixture = Fixture::new();
    fixture.record_initial_draw();
    fixture.event(0x0044_5200, 1).unwrap();
    fixture.event(0x0044_5200, 0).unwrap();
    assert_eq!(fixture.effects.calls, [Effect::Sound(5)]);
    assert!(matches!(
        fixture.tick(ScreenAnalogValues::default(), frame()),
        Err(FrontendDispatchError::MissingProfileStore)
    ));
    assert_eq!(fixture.effects.calls, [Effect::Sound(5), Effect::Music]);
    let root = fixture.control.menus().current().unwrap();
    assert_eq!(root.basis.words[0], 1);
    assert_eq!(root.age_millis(), 16);
    assert!(fixture.control.stopped());
    assert!(
        !fixture.input.stopped(),
        "the required effect failed after base input completed"
    );
    let count = fixture.effects.calls.len();
    assert!(matches!(
        fixture.tick(ScreenAnalogValues::default(), frame()),
        Err(FrontendDispatchError::Control(
            FrontendControlError::Stopped
        ))
    ));
    assert_eq!(fixture.effects.calls.len(), count);
}

#[test]
fn external_failure_during_digital_action_stops_both_owners_and_preserves_request() {
    let mut fixture = Fixture::new();
    let root = fixture.catalog.menu_data(ROOT_MENU_ADDRESS).unwrap();
    root.records_mut().items[0].action_callback = 0x1234_5678;
    root.records_mut().items[0].action_arguments = [7, 8];
    assert!(matches!(
        fixture.event(0x0048_5300, 1),
        Err(FrontendDispatchError::Effect("external action unavailable"))
    ));
    let Effect::External(request) = fixture.effects.calls[0] else {
        panic!("required external request");
    };
    assert_eq!(
        request.menu_source.authored_address().unwrap(),
        ROOT_MENU_ADDRESS
    );
    assert_eq!(request.item_index, 0);
    assert_eq!(request.callback_address, 0x1234_5678);
    assert_eq!(request.command, MenuCommand::Auxiliary6);
    assert_eq!(request.arguments, [7, 8]);
    assert_eq!(request.context, 77);
    assert!(fixture.control.stopped());
    assert!(fixture.input.stopped());
    assert!(matches!(
        fixture.event(0x0048_5300, 0),
        Err(FrontendDispatchError::ScreenInput(
            ScreenInputError::Stopped
        ))
    ));
    assert_eq!(fixture.effects.calls.len(), 1);
}

#[test]
fn invalid_callback_context_address_and_policy_reject_without_poisoning_or_effects() {
    let mut fixture = Fixture::new();
    assert!(matches!(
        fixture.callback(InputCallback {
            address: 0x0044_5200,
            context: 88,
            value: 1,
            timestamp_micros: 0
        }),
        Err(FrontendDispatchError::ScreenInput(
            ScreenInputError::Context {
                expected: 77,
                actual: 88
            }
        ))
    ));
    assert!(matches!(
        fixture.event(0x0043_9260, 1),
        Err(FrontendDispatchError::ScreenInput(
            ScreenInputError::UnknownCallback(0x0043_9260)
        ))
    ));
    assert_eq!(fixture.input.held_mask(), 0);
    assert!(!fixture.input.stopped());
    assert!(!fixture.control.stopped());
    assert!(fixture.effects.calls.is_empty());
    fixture.input = ScreenInput::new(ScreenDispatchPolicy::Base, 77);
    assert!(matches!(
        fixture.tick(ScreenAnalogValues::default(), frame()),
        Err(FrontendDispatchError::ScreenPolicy(
            ScreenDispatchPolicy::Base
        ))
    ));
    assert_eq!(fixture.control.animation().accumulated_micros(), 0);
    assert_eq!(fixture.input.age_millis(), 0);
    fixture.input = ScreenInput::new(ScreenDispatchPolicy::Frontend, 88);
    assert!(matches!(
        fixture.tick(ScreenAnalogValues::default(), frame()),
        Err(FrontendDispatchError::Context {
            control: 77,
            input: 88
        })
    ));
    assert!(!fixture.control.stopped());
    assert!(!fixture.input.stopped());
}

#[test]
fn attract_requests_use_required_effects_and_sound_failure_prevents_later_tick_work() {
    let mut fixture = Fixture::new();
    fixture.event(0x0042_a9d0, 1).unwrap();
    fixture
        .tick(ScreenAnalogValues::default(), frame())
        .unwrap();
    assert_eq!(
        fixture.effects.calls,
        [Effect::Music, Effect::Attract(true), Effect::Phase(9)]
    );

    let mut fixture = Fixture::new();
    fixture.effects.fail_sound = true;
    assert!(matches!(
        fixture.tick(
            ScreenAnalogValues {
                consumer_cc: 0,
                consumer_d0: 3001
            },
            frame()
        ),
        Err(FrontendDispatchError::Effect("audio unavailable"))
    ));
    assert_eq!(fixture.effects.calls, [Effect::Sound(2)]);
    assert!(fixture.control.stopped());
    assert!(fixture.input.stopped());
    assert_eq!(fixture.control.menus().current().unwrap().basis.words[0], 0);
}
