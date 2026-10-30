//! Native factory composition using retail assets and real resource/input owners.
//! Platform enumeration, music and persistence are explicit recording effects;
//! these fixtures make no OS-input, audible-playback or rendered-pixel claim.

use std::{path::Path, rc::Rc};

use inf_game::frontend_assets::{FrontendAssets, FrontendMenuCatalog};
use inf_game::frontend_bindings::{FrontendBindingPhase, FrontendDevice};
use inf_game::frontend_camera::FrontendEntry;
use inf_game::frontend_control::ROOT_MENU_ADDRESS;
use inf_game::frontend_dispatch::{FrontendDispatch, FrontendEffects};
use inf_game::frontend_screen::{
    FrontendFactoryHost, FrontendFactoryServices, FrontendProcessState, FrontendScreen,
    FrontendScreenConfig, FrontendScreenError,
};
use inf_game::input::{DeviceButtons, InputContextId, InputRuntime, InputStateError};
use inf_game::menu::{MenuActionRequest, MenuDrawColors, MenuStack, MenuVisibility};
use inf_game::menu_basis::MenuBasisMath;
use inf_game::profile_menu::ProfileOperation;
use inf_game::profile_store::ProfileStoreError;
use inf_game::scheduler::SchedulerTick;
use inf_game::screen_registry::{
    RegistryError, ScreenAttachment, ScreenLifecycle, ScreenMembership, ScreenRegistry,
};
use inf_game::{HandlerResources, Installation, ResidencyRequest, ResidentRuntime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Pause,
    Resume,
    Enumerate,
    Persist,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecordedEffect {
    effect: Effect,
    colors: Option<MenuDrawColors>,
    tint: Option<inf_game::menu::MenuColor>,
}

struct RecordingFactory {
    process: Rc<FrontendProcessState>,
    devices: Vec<FrontendDevice>,
    fail: Option<Effect>,
    effects: Vec<RecordedEffect>,
}

impl RecordingFactory {
    fn record(&mut self, effect: Effect) -> Result<(), Effect> {
        self.effects.push(RecordedEffect {
            effect,
            colors: self.process.colors(),
            tint: self.process.tint(),
        });
        if self.fail == Some(effect) {
            Err(effect)
        } else {
            Ok(())
        }
    }

    fn sequence(&self) -> Vec<Effect> {
        self.effects.iter().map(|entry| entry.effect).collect()
    }
}

impl FrontendFactoryHost for RecordingFactory {
    type Error = Effect;

    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.record(Effect::Pause)
    }

    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.record(Effect::Resume)
    }

    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        self.record(Effect::Enumerate)?;
        // Supplied logical sources have explicit native-default calibration.
        // No Windows/SDL device identity or key translation is inferred.
        for device in &self.devices {
            input.device_buttons(device.source).unwrap();
        }
        Ok(self.devices.clone())
    }

    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.record(Effect::Persist)
    }
}

struct Fixture {
    registry: ScreenRegistry<(), InputContextId>,
    attachment: ScreenAttachment,
    input: InputRuntime,
    residency: ResidentRuntime,
    catalog: FrontendMenuCatalog,
    process: Rc<FrontendProcessState>,
    devices: Vec<FrontendDevice>,
}

#[test]
fn portable_factory_retains_rgb_without_inventing_native_pointer_high_byte() {
    let mut fixture = Fixture::new();
    let mut screen = FrontendScreen::new(
        FrontendScreenConfig {
            attachment: fixture.attachment,
            callback_context: 1,
            native_context_high_byte: None,
        },
        fixture.catalog.assets(),
    );
    let mut host = fixture.host(None);
    fixture.initialize(&mut screen, &mut host).unwrap();
    assert_eq!(
        fixture.process.tint(),
        Some(inf_game::menu::MenuColor {
            rgb: [128; 3],
            high_byte: None,
        })
    );
    fixture.destroy(&mut screen);
}

impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = FrontendMenuCatalog::new(FrontendAssets::open(&root).unwrap());
        let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        residency.apply(ResidencyRequest::Boot).unwrap();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(()),
        };
        let mut input = InputRuntime::default();
        let primary = input.create_device(DeviceButtons::default()).unwrap();
        let axes = input.create_device(DeviceButtons::default()).unwrap();
        Self {
            registry,
            attachment,
            input,
            residency,
            catalog,
            process: Rc::new(FrontendProcessState::default()),
            devices: vec![
                FrontendDevice {
                    native_type: 2,
                    source: axes,
                },
                FrontendDevice {
                    native_type: 0,
                    source: primary,
                },
            ],
        }
    }

    fn screen(&self, callback_context: u32, high_byte: u8) -> FrontendScreen {
        FrontendScreen::new(
            FrontendScreenConfig {
                attachment: self.attachment,
                callback_context,
                native_context_high_byte: Some(high_byte),
            },
            self.catalog.assets(),
        )
    }

    fn host(&self, fail: Option<Effect>) -> RecordingFactory {
        RecordingFactory {
            process: self.process.clone(),
            devices: self.devices.clone(),
            fail,
            effects: Vec::new(),
        }
    }

    fn initialize(
        &mut self,
        screen: &mut FrontendScreen,
        host: &mut RecordingFactory,
    ) -> Result<(), FrontendScreenError<Effect>> {
        screen.initialize(
            &mut FrontendFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                residency: &mut self.residency,
                catalog: &mut self.catalog,
                process: &self.process,
            },
            host,
        )
    }

    fn destroy(&mut self, screen: &mut FrontendScreen) {
        screen.destroy(&mut self.registry, &mut self.input).unwrap();
    }

    fn expected_colors(&self) -> MenuDrawColors {
        let color_word = |slot| {
            let resource = self.residency.resource(6, slot).unwrap();
            let (_, HandlerResources::Words(bank)) = resource.overlay.handler(6).unwrap() else {
                panic!("retail Handler-6 colors must be word resources");
            };
            bank.entries[usize::from(resource.registration.local_index)].raw
        };
        MenuDrawColors {
            selected_start: color_word(3),
            ordinary: color_word(2),
            selected_end: color_word(2),
        }
    }

    fn assert_base_retained(&self, screen: &FrontendScreen) {
        assert_eq!(screen.phase(), ScreenLifecycle::Stopped);
        let id = screen.id().unwrap();
        assert_eq!(
            self.registry.screen(id).unwrap().membership,
            ScreenMembership::Linked
        );
        assert_eq!(
            self.registry.screens(self.attachment.session).unwrap(),
            [id]
        );
        assert_eq!(
            self.registry
                .scheduler_members(self.attachment.session)
                .unwrap(),
            [id]
        );
        assert_eq!(
            self.registry
                .linked_screen(self.attachment.context_owner)
                .unwrap(),
            Some(id)
        );
        assert!(screen.bindings().context().is_some());
        assert_eq!(
            self.input
                .analog_consumer_state(screen.bindings().consumer_cc().unwrap())
                .unwrap()
                .references,
            1
        );
        assert_eq!(
            self.input
                .analog_consumer_state(screen.bindings().consumer_d0().unwrap())
                .unwrap()
                .references,
            1
        );
        assert!(screen.control().is_some());
        assert_eq!(screen.control().unwrap().menus().len(), 0);
        assert_eq!(self.registry.selected_screen(), None);
        assert_eq!(self.input.selected_context(), None);
    }
}

#[test]
fn pause_failure_keeps_base_and_controller_before_resource_transition() {
    let mut fixture = Fixture::new();
    let before = fixture.residency.summary();
    let retained = fixture
        .residency
        .resource(0, 0)
        .unwrap()
        .overlay
        .bytes()
        .as_ptr();
    let mut screen = fixture.screen(77, 0xab);
    let mut host = fixture.host(Some(Effect::Pause));
    assert!(matches!(
        fixture.initialize(&mut screen, &mut host),
        Err(FrontendScreenError::Host(Effect::Pause))
    ));
    fixture.assert_base_retained(&screen);
    assert_eq!(fixture.residency.summary(), before);
    assert_eq!(
        fixture
            .residency
            .resource(0, 0)
            .unwrap()
            .overlay
            .bytes()
            .as_ptr(),
        retained
    );
    assert_eq!(fixture.process.colors(), None);
    assert_eq!(fixture.process.tint(), None);
    assert_eq!(host.sequence(), [Effect::Pause]);
    assert_eq!(screen.bindings().phase(), FrontendBindingPhase::Initialized);
    fixture.destroy(&mut screen);
    assert_eq!(fixture.process.entry(), FrontendEntry::First);
    assert_eq!(fixture.residency.summary(), before);
}

#[test]
fn resume_failure_keeps_frontend_leaf_loaded_before_colors_or_bindings() {
    let mut fixture = Fixture::new();
    let mut screen = fixture.screen(77, 0xab);
    let mut host = fixture.host(Some(Effect::Resume));
    assert!(matches!(
        fixture.initialize(&mut screen, &mut host),
        Err(FrontendScreenError::Host(Effect::Resume))
    ));
    fixture.assert_base_retained(&screen);
    assert_eq!(fixture.residency.summary().active_leaves, [0, 47]);
    assert!(fixture.residency.resource(6, 2).is_some());
    assert_eq!(fixture.process.colors(), None);
    assert_eq!(fixture.process.tint(), None);
    assert_eq!(host.sequence(), [Effect::Pause, Effect::Resume]);
    fixture.destroy(&mut screen);
    assert_eq!(fixture.residency.summary().active_leaves, [0, 47]);
}

#[test]
fn enumeration_failure_observes_published_retail_colors_before_binding_installation() {
    let mut fixture = Fixture::new();
    let mut screen = fixture.screen(77, 0xab);
    let mut host = fixture.host(Some(Effect::Enumerate));
    assert!(matches!(
        fixture.initialize(&mut screen, &mut host),
        Err(FrontendScreenError::Host(Effect::Enumerate))
    ));
    fixture.assert_base_retained(&screen);
    let colors = fixture.expected_colors();
    assert_eq!(fixture.process.colors(), Some(colors));
    assert_eq!(host.effects[0].colors, None);
    assert_eq!(host.effects[1].colors, None);
    assert_eq!(host.effects[2].colors, Some(colors));
    assert_eq!(fixture.process.tint(), None);
    assert_eq!(screen.bindings().phase(), FrontendBindingPhase::Initialized);
    assert_eq!(
        host.sequence(),
        [Effect::Pause, Effect::Resume, Effect::Enumerate]
    );
    fixture.destroy(&mut screen);
    assert_eq!(fixture.process.colors(), Some(colors));
}

#[test]
fn persistence_failure_retains_selected_input_and_root_without_final_tint() {
    let mut fixture = Fixture::new();
    let mut screen = fixture.screen(77, 0xab);
    let mut host = fixture.host(Some(Effect::Persist));
    assert!(matches!(
        fixture.initialize(&mut screen, &mut host),
        Err(FrontendScreenError::Host(Effect::Persist))
    ));
    assert_eq!(screen.phase(), ScreenLifecycle::Stopped);
    assert_eq!(screen.bindings().phase(), FrontendBindingPhase::Installed);
    assert_eq!(fixture.registry.selected_screen(), screen.id());
    assert_eq!(
        fixture.input.selected_context(),
        screen.bindings().context()
    );
    let menu = screen.control().unwrap().menus().current().unwrap();
    assert_eq!(
        menu.data().records().source.authored_address().unwrap(),
        ROOT_MENU_ADDRESS
    );
    assert_eq!(screen.control().unwrap().menus().len(), 1);
    assert_eq!(fixture.process.colors(), Some(fixture.expected_colors()));
    assert_eq!(fixture.process.tint(), None);
    assert_eq!(
        host.sequence(),
        [
            Effect::Pause,
            Effect::Resume,
            Effect::Enumerate,
            Effect::Persist
        ]
    );
    assert!(host.effects.iter().all(|entry| entry.tint.is_none()));
    let context = screen.bindings().context().unwrap();
    fixture.destroy(&mut screen);
    assert_eq!(fixture.input.selected_context(), None);
    assert_eq!(fixture.registry.selected_screen(), None);
    assert_eq!(
        fixture.input.select_context(Some(context)),
        Err(InputStateError::InvalidContext)
    );
}

#[test]
fn successful_factory_teardown_and_reentry_preserve_process_and_authored_owners() {
    let mut fixture = Fixture::new();
    let mut first = fixture.screen(77, 0xab);
    let mut host = fixture.host(None);
    fixture.initialize(&mut first, &mut host).unwrap();
    assert_eq!(first.phase(), ScreenLifecycle::Ready);
    assert_eq!(first.control().unwrap().entry_state(), FrontendEntry::First);
    assert_eq!(
        fixture.process.tint(),
        Some(inf_game::menu::MenuColor::from_word(0xab80_8080))
    );
    assert_eq!(fixture.process.colors(), Some(fixture.expected_colors()));
    assert_eq!(
        host.sequence(),
        [
            Effect::Pause,
            Effect::Resume,
            Effect::Enumerate,
            Effect::Persist
        ]
    );
    let shared_root = fixture.catalog.menu_data(ROOT_MENU_ADDRESS).unwrap();
    shared_root.records_mut().items[0].row_height = 9;
    shared_root.records_mut().items[0].age = 123;
    shared_root.records_mut().layouts[0].scroll_animation = 50;
    let old_id = first.id().unwrap();
    let old_context = first.bindings().context().unwrap();
    let old_cc = first.bindings().consumer_cc().unwrap();
    let old_d0 = first.bindings().consumer_d0().unwrap();
    let resident_before_destroy = fixture.residency.summary();
    let frontend_bytes = fixture
        .residency
        .resource(6, 2)
        .unwrap()
        .overlay
        .bytes()
        .as_ptr();
    fixture.destroy(&mut first);
    assert_eq!(first.phase(), ScreenLifecycle::Destroyed);
    assert_eq!(first.bindings().phase(), FrontendBindingPhase::Released);
    assert_eq!(fixture.process.entry(), FrontendEntry::Returning);
    assert_eq!(
        fixture.process.tint(),
        Some(inf_game::menu::MenuColor::from_word(0xab80_8080))
    );
    assert_eq!(fixture.residency.summary(), resident_before_destroy);
    assert_eq!(fixture.registry.selected_screen(), None);
    assert_eq!(fixture.input.selected_context(), None);
    assert!(fixture
        .registry
        .screens(fixture.attachment.session)
        .unwrap()
        .is_empty());
    assert!(
        matches!(fixture.registry.linked_screen(fixture.attachment.context_owner), Err(RegistryError::StaleLinkedScreen { screen, .. }) if screen == old_id)
    );
    assert_eq!(
        fixture.input.analog_value(old_cc),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(
        fixture.input.analog_value(old_d0),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(
        fixture.input.select_context(Some(old_context)),
        Err(InputStateError::InvalidContext)
    );
    for device in &fixture.devices {
        assert!(fixture.input.device_buttons(device.source).is_ok());
    }
    fixture.destroy(&mut first); // explicit teardown is idempotent

    let mut second = fixture.screen(77, 0xcd);
    let mut host = fixture.host(None);
    fixture.initialize(&mut second, &mut host).unwrap();
    assert_ne!(second.id(), Some(old_id));
    assert_ne!(second.bindings().consumer_cc(), Some(old_cc));
    assert_eq!(
        second.control().unwrap().entry_state(),
        FrontendEntry::Returning
    );
    let camera = second.control().unwrap().camera();
    assert_eq!(camera.eye().position, camera.eye().target);
    assert_eq!(camera.look_at().position, camera.look_at().target);
    assert_eq!(
        fixture
            .registry
            .linked_screen(fixture.attachment.context_owner)
            .unwrap(),
        second.id()
    );
    assert_eq!(
        fixture.process.tint(),
        Some(inf_game::menu::MenuColor::from_word(0xcd80_8080))
    );
    assert_eq!(
        host.effects.last().unwrap().tint,
        Some(inf_game::menu::MenuColor::from_word(0xab80_8080))
    );
    assert_eq!(
        fixture
            .residency
            .resource(6, 2)
            .unwrap()
            .overlay
            .bytes()
            .as_ptr(),
        frontend_bytes
    );
    let reopened = second.control().unwrap().menus().current().unwrap();
    assert_eq!(reopened.data().records().items[0].row_height, 9);
    assert_eq!(shared_root.records().items[0].age, 0);
    assert_eq!(shared_root.records().layouts[0].scroll_animation, 0);
    fixture.destroy(&mut second);
}

#[test]
fn destroying_unselected_newer_screen_preserves_selected_other_but_leaves_stale_link() {
    let mut fixture = Fixture::new();
    let mut older = fixture.screen(77, 0xab);
    let mut newer = fixture.screen(88, 0xcd);
    let mut host = fixture.host(None);
    fixture.initialize(&mut older, &mut host).unwrap();
    fixture.initialize(&mut newer, &mut host).unwrap();
    let older_id = older.id().unwrap();
    let newer_id = newer.id().unwrap();
    let selected_context = older.bindings().context().unwrap();
    fixture
        .registry
        .select_input(older_id, |context| fixture.input.select_context(context))
        .unwrap();
    assert_eq!(
        fixture
            .registry
            .screens(fixture.attachment.session)
            .unwrap(),
        [newer_id, older_id]
    );
    fixture.destroy(&mut newer);
    assert_eq!(fixture.registry.selected_screen(), Some(older_id));
    assert_eq!(fixture.input.selected_context(), Some(selected_context));
    assert_eq!(
        fixture
            .registry
            .current_screen(fixture.attachment.session)
            .unwrap(),
        Some(older_id)
    );
    assert!(
        matches!(fixture.registry.linked_screen(fixture.attachment.context_owner), Err(RegistryError::StaleLinkedScreen { screen, .. }) if screen == newer_id)
    );
    assert!(fixture
        .input
        .analog_value(older.bindings().consumer_cc().unwrap())
        .is_ok());
    assert!(fixture
        .input
        .analog_value(older.bindings().consumer_d0().unwrap())
        .is_ok());
    fixture.destroy(&mut older);
}

#[derive(Default)]
struct TickEffects {
    music_services: usize,
    attract_flags: Vec<bool>,
}

impl FrontendEffects for TickEffects {
    type Error = &'static str;
    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.music_services += 1;
        Ok(())
    }
    fn play_sound(&mut self, _: i16) -> Result<(), Self::Error> {
        Err("unexpected sound")
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        Err("unexpected tick persistence")
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        Err("unexpected transition flag")
    }
    fn profile_operation_failed(
        &mut self,
        _: ProfileOperation,
        _: &ProfileStoreError,
    ) -> Result<(), Self::Error> {
        Err("unexpected profile failure")
    }
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        Err("unexpected network menu")
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        Err("unexpected world refresh")
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        Err("unexpected restore defaults")
    }
    fn apply_volume(
        &mut self,
        _: inf_game::settings_row::VolumeChannel,
    ) -> Result<(), Self::Error> {
        Err("unexpected volume apply")
    }
    fn adjust_setting(
        &mut self,
        _: u32,
        _: inf_game::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected settings adjust")
    }
    fn adjust_display_setting(
        &mut self,
        _: u32,
        _: inf_game::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected display adjust")
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("unexpected phase request")
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.attract_flags.push(enabled);
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        Err("unexpected exit request")
    }
    fn external_action(
        &mut self,
        _: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        Err("unexpected derived action")
    }
}

#[test]
fn factory_resets_shared_animation_only_after_successful_persistence() {
    let mut fixture = Fixture::new();
    let mut older = fixture.screen(77, 0xab);
    let mut host = fixture.host(None);
    fixture.initialize(&mut older, &mut host).unwrap();
    let basis = MenuBasisMath::new(fixture.catalog.assets().sine_table().values);
    let mut effects = TickEffects::default();
    older
        .tick(
            &mut fixture.input,
            SchedulerTick {
                delta_micros: 16_000,
                elapsed_micros: 16_000,
            },
            &mut FrontendDispatch {
                catalog: &mut fixture.catalog,
                basis: &basis,
                visibility: MenuVisibility::OwnerFlagClear,
                effects: &mut effects,
                profiles: None,
            },
        )
        .unwrap();
    assert_eq!(effects.music_services, 1);
    assert_eq!(effects.attract_flags, [false]);
    assert_eq!(fixture.process.animation().accumulated_micros(), 16_000);
    assert_eq!(
        older.control().unwrap().animation().accumulated_micros(),
        16_000
    );

    let mut failed = fixture.screen(88, 0xcd);
    let mut host = fixture.host(Some(Effect::Persist));
    assert!(matches!(
        fixture.initialize(&mut failed, &mut host),
        Err(FrontendScreenError::Host(Effect::Persist))
    ));
    assert_eq!(fixture.process.animation().accumulated_micros(), 16_000);
    assert_eq!(
        failed.control().unwrap().animation().accumulated_micros(),
        16_000
    );
    fixture.destroy(&mut failed);
    assert_eq!(older.control().unwrap().entry_state(), FrontendEntry::First);

    let mut newer = fixture.screen(99, 0xef);
    let mut host = fixture.host(None);
    fixture.initialize(&mut newer, &mut host).unwrap();
    assert_eq!(fixture.process.animation().accumulated_micros(), 0);
    assert_eq!(older.control().unwrap().animation().accumulated_micros(), 0);
    assert_eq!(newer.control().unwrap().animation().accumulated_micros(), 0);
    fixture.destroy(&mut newer);
    fixture.destroy(&mut older);
}
