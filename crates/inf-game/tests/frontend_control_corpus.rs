//! Retail table/control integration with explicitly recorded external effects.
//! The drawing fixture records backend submissions and leaves each row on one
//! line; it makes no pixel or font-metric claim. Basis ticks use the original
//! sine table. Derived profile/network construction remains an explicit error.

use std::path::Path;

use inf_game::frontend_assets::{FrontendAssets, FrontendMenuCatalog};
use inf_game::frontend_camera::FrontendEntry;
use inf_game::frontend_control::{
    FrontendControl, FrontendControlConfig, FrontendControlError, FrontendHost, FrontendInput,
    FrontendInputOutcome, FrontendProfileContext, QUIT_MENU_ADDRESS, ROOT_MENU_ADDRESS,
};
use inf_game::menu::{
    MenuActionRequest, MenuBasis, MenuCommandOutcome, MenuControllerDrawSetup, MenuData,
    MenuDrawColors, MenuDrawHost, MenuDrawInput, MenuDrawOutcome, MenuRowDrawRequest, MenuStack,
    MenuState, MenuVisibility,
};
use inf_game::menu_basis::MenuBasisMath;
use inf_game::profile_menu::ProfileMenu;
use inf_game::scheduler::SchedulerTick;

#[derive(Debug, PartialEq, Eq)]
enum RecordedEffect {
    MenuLookup(u32),
    BaseInput,
    Music,
    Sound(i16),
    ClearHeld,
    SaveSettings,
    StartMenu,
    Phase(u32),
    Attract(bool),
    Exit,
    ControllerDraw,
    DelegatedRow(u32),
}

struct RecordingHost {
    catalog: FrontendMenuCatalog,
    basis_math: MenuBasisMath,
    effects: Vec<RecordedEffect>,
    draw_setups: Vec<MenuControllerDrawSetup>,
    draw_rows: Vec<MenuRowDrawRequest>,
    published_bases: Vec<MenuBasis>,
    clip: [i16; 4],
    clips: Vec<[i16; 4]>,
}

impl FrontendHost for RecordingHost {
    type Error = &'static str;

    fn tick_profile(
        &mut self,
        _: &mut ProfileMenu,
        _: i32,
        _: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>> {
        Err(FrontendControlError::Host("profile tick unavailable"))
    }

    fn profile_action(
        &mut self,
        _: &mut ProfileMenu,
        _: MenuActionRequest,
        _: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>> {
        Err(FrontendControlError::Host("profile action unavailable"))
    }

    fn menu_data(&mut self, address: u32) -> Result<MenuData, Self::Error> {
        self.effects.push(RecordedEffect::MenuLookup(address));
        self.catalog
            .menu_data(address)
            .map_err(|_| "retail menu lookup failed")
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.basis_math.rotate(basis, angle);
        Ok(())
    }

    fn tick_base_input(
        &mut self,
        _: &mut FrontendControl,
        _: SchedulerTick,
    ) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::BaseInput);
        Ok(())
    }

    fn clear_held_input(&mut self) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::ClearHeld);
        Ok(())
    }

    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::Music);
        Ok(())
    }

    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::Sound(resource));
        Ok(())
    }

    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::SaveSettings);
        Ok(())
    }

    fn profile_first_byte_nonzero(&mut self) -> Result<bool, Self::Error> {
        Err("unexpected profile read")
    }

    fn open_start_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::StartMenu);
        Err("derived profile menu unavailable")
    }

    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        Err("unexpected network menu construction")
    }

    fn tick_network(
        &mut self,
        _: &mut inf_game::network_menu::NetworkMenu,
        _: i32,
    ) -> Result<(), Self::Error> {
        Err("unexpected network menu tick")
    }

    fn network_action(
        &mut self,
        _: &mut MenuStack,
        _: inf_game::menu::MenuActionRequest,
    ) -> Result<(), Self::Error> {
        Err("unexpected network menu action")
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

    fn adjust_display_setting(
        &mut self,
        _: u32,
        _: inf_game::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected display adjust")
    }

    fn adjust_setting(
        &mut self,
        _: u32,
        _: inf_game::menu::MenuCommand,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected settings adjust")
    }

    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::Phase(phase));
        Err("unexpected session phase request")
    }

    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::Attract(enabled));
        if enabled {
            Err("unexpected attract request")
        } else {
            Ok(())
        }
    }

    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::Exit);
        Ok(())
    }

    fn external_action(
        &mut self,
        _: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        Err("unexpected external row action")
    }
}

impl MenuDrawHost for RecordingHost {
    type Error = &'static str;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.effects.push(RecordedEffect::ControllerDraw);
        self.draw_setups.push(setup);
        Ok(())
    }

    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.published_bases.push(basis);
        Ok(())
    }

    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok(self.clip)
    }

    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.clips.push(clip);
        self.clip = clip;
        Ok(())
    }

    fn colors(&self) -> MenuDrawColors {
        // Explicit fixture colors: native resource lookup/publication remains external.
        MenuDrawColors {
            selected_start: 0x00c8_c8c8,
            ordinary: 0x0080_8080,
            selected_end: 0x00ff_ffff,
        }
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Self::Error> {
        self.effects
            .push(RecordedEffect::DelegatedRow(request.callback_address));
        self.draw_rows.push(*request);
        // Recording backend: unchanged descriptor Y models a one-line result.
        // Native enumeration, wrapper effects and height feedback still run.
        Ok(())
    }
}

fn fixture(entry: FrontendEntry) -> (FrontendControl, RecordingHost) {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&data_root).expect("retail Infest.exe is required");
    let config = FrontendControlConfig {
        camera_presets: assets.camera_presets().to_vec(),
        position_scale: assets.position_scale(),
        motion_scale: assets.motion_scale(),
        entry,
        context: 71,
    };
    let basis_math = MenuBasisMath::new(assets.sine_table().values);
    let mut catalog = FrontendMenuCatalog::new(assets);
    let root = catalog.menu_data(ROOT_MENU_ADDRESS).unwrap();
    let mut control = FrontendControl::new(config).unwrap();
    control
        .menus_mut()
        .push(MenuState::new(root, MenuBasis::INITIAL));
    (
        control,
        RecordingHost {
            catalog,
            basis_math,
            effects: Vec::new(),
            draw_setups: Vec::new(),
            draw_rows: Vec::new(),
            published_bases: Vec::new(),
            clip: [0, 0, 800, 600],
            clips: Vec::new(),
        },
    )
}

fn draw(control: &mut FrontendControl, host: &mut RecordingHost) -> Option<MenuDrawOutcome> {
    let sine = host.catalog.assets().sine_table().values;
    control
        .draw_menu(
            MenuDrawInput {
                surface_pixels: [800, 600],
                visibility: MenuVisibility::OwnerFlagClear,
                sine_quarter: &sine,
            },
            host,
        )
        .unwrap()
}

fn tick(control: &mut FrontendControl, host: &mut RecordingHost) {
    control
        .tick(
            SchedulerTick {
                delta_micros: 8_000,
                elapsed_micros: 8_000,
            },
            host,
        )
        .unwrap();
}

fn input(
    control: &mut FrontendControl,
    host: &mut RecordingHost,
    input: FrontendInput,
) -> FrontendInputOutcome {
    control
        .input(input, MenuVisibility::OwnerFlagClear, host)
        .unwrap()
}

fn current_address(control: &FrontendControl) -> u32 {
    control
        .menus()
        .current()
        .unwrap()
        .data()
        .records()
        .source
        .authored_address()
        .unwrap()
}

#[test]
fn retail_root_options_action_uses_authored_child_and_camera_before_navigation_resumes() {
    let (mut control, mut host) = fixture(FrontendEntry::Returning);
    assert!(control.camera().ready());
    let root = host.catalog.menu_data(ROOT_MENU_ADDRESS).unwrap();
    let options = root.records().items[1];
    assert_eq!(options.action_callback, 0x0048_34b0);
    assert_eq!(options.action_arguments[1], 0x0053_9378);
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Next),
        FrontendInputOutcome::Menu(MenuCommandOutcome::Selection {
            previous: 0,
            current: 1
        })
    );
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::Menu(MenuCommandOutcome::ActionDispatched)
    );
    assert_eq!(
        host.effects,
        [
            RecordedEffect::Sound(2),
            RecordedEffect::MenuLookup(options.action_arguments[1]),
            RecordedEffect::Sound(5),
        ]
    );
    assert_eq!(control.menus().len(), 2);
    assert_eq!(current_address(&control), options.action_arguments[1]);
    assert_eq!(control.camera().selected_preset(), 2);
    let preset = host.catalog.assets().camera_presets()[2];
    assert_eq!(control.camera().eye().target, preset.eye.map(|v| v * 10));
    assert_eq!(
        control.camera().look_at().target,
        preset.target.map(|v| v * 10)
    );
    assert!(!control.camera().ready());
    let selection = control.menus().current().unwrap().selection();
    host.effects.clear();
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Next),
        FrontendInputOutcome::CameraBlocked
    );
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::CameraBlocked
    );
    assert_eq!(control.menus().current().unwrap().selection(), selection);
    assert!(control.accelerated());
    assert!(host.effects.is_empty());

    // Advance the recovered spring against actual presets. No rows have been
    // submitted yet, so the draw latch still prevents menu basis ticks.
    for _ in 0..5000 {
        if control.camera().ready() {
            break;
        }
        control
            .tick(
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                &mut host,
            )
            .unwrap();
    }
    assert!(
        control.camera().ready(),
        "retail Options camera never reached its gate"
    );
    assert!(host
        .effects
        .chunks_exact(2)
        .all(|pair| pair == [RecordedEffect::BaseInput, RecordedEffect::Music]));
    assert!(!control.menus().drawn());
    assert_eq!(control.menus().current().unwrap().age_millis(), 0);

    let heading = control.menus().current().unwrap().data().records().items[0];
    assert_eq!(heading.draw_callback, 0x0045_f520);
    assert_eq!(heading.draw_arguments[1], 2);
    tick(&mut control, &mut host);
    assert_eq!(control.menus().fade(), 65535);
    host.effects.clear();
    let outcome = draw(&mut control, &mut host).unwrap();
    assert!(outcome.menu_drawn);
    assert_eq!(outcome.rows, host.draw_rows.len());
    assert_eq!(
        host.effects.get(1),
        Some(&RecordedEffect::DelegatedRow(0x0045_3a80))
    );
    assert_eq!(
        host.draw_rows[0].menu_source.authored_address().unwrap(),
        options.action_arguments[1]
    );
    assert_eq!(host.draw_rows[0].item_index, 0);
    assert_eq!(host.draw_rows[0].arguments, heading.draw_arguments);
    assert!(control.menus().drawn());
    assert_eq!(control.camera().selected_preset(), 2);
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Back),
        FrontendInputOutcome::PoppedMenu
    );
    assert_eq!(current_address(&control), ROOT_MENU_ADDRESS);
    assert_eq!(
        control.camera().selected_preset(),
        2,
        "pop alone does not reset the camera"
    );
    let root_heading = root.records().items[0];
    assert_eq!(root_heading.draw_callback, 0x0048_35e0);
    host.effects.clear();
    host.draw_rows.clear();
    assert_eq!(
        draw(&mut control, &mut host),
        Some(MenuDrawOutcome {
            rows: 3,
            menu_drawn: true
        })
    );
    assert_eq!(
        host.effects.get(1),
        Some(&RecordedEffect::DelegatedRow(0x0048_34f0))
    );
    assert_eq!(host.draw_rows[0].arguments, root_heading.draw_arguments);
    assert_eq!(
        host.draw_rows[0].menu_source.authored_address().unwrap(),
        ROOT_MENU_ADDRESS
    );
    assert_eq!(host.draw_rows[0].descriptor.position_pixels, [390, 375]);
    assert_eq!(host.clip, [0, 0, 800, 600]);
    assert_eq!(control.camera().selected_preset(), 0);
    assert!(
        !control.camera().ready(),
        "root draw wrapper retargets before the next gate check"
    );
}

#[test]
fn retail_options_arrow_traversal_stays_within_the_eight_logical_items() {
    // Options 00539378 counts thirteen slots in its second layout for eight
    // logical items. Native sums heights past the items from adjacent bytes;
    // the port stops at the logical count, so arrows never leave 1..7 (the
    // header carries the nonselectable bit).
    let (mut control, mut host) = fixture(FrontendEntry::Returning);
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Next),
        FrontendInputOutcome::Menu(MenuCommandOutcome::Selection {
            previous: 0,
            current: 1
        })
    );
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::Menu(MenuCommandOutcome::ActionDispatched)
    );
    assert_eq!(current_address(&control), 0x0053_9378);
    assert_eq!(control.menus().current().unwrap().selection(), 1);
    // The Options push retargets the camera; arrows wait for its gate.
    for _ in 0..5000 {
        if control.camera().ready() {
            break;
        }
        control
            .tick(
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                &mut host,
            )
            .unwrap();
    }
    assert!(
        control.camera().ready(),
        "retail Options camera never reached its gate"
    );
    let mut previous = 1;
    for expected in [2, 3, 4, 5, 6, 7, 7] {
        assert_eq!(
            input(&mut control, &mut host, FrontendInput::Next),
            FrontendInputOutcome::Menu(MenuCommandOutcome::Selection {
                previous,
                current: expected,
            })
        );
        let selection = control.menus().current().unwrap().selection();
        assert_eq!(selection, expected);
        assert!(selection >= 0 && selection < 8);
        previous = expected;
    }
    for expected in [6, 5, 4, 3, 2, 1, 1] {
        assert_eq!(
            input(&mut control, &mut host, FrontendInput::Previous),
            FrontendInputOutcome::Menu(MenuCommandOutcome::Selection {
                previous,
                current: expected,
            })
        );
        let selection = control.menus().current().unwrap().selection();
        assert_eq!(selection, expected);
        previous = expected;
    }
    assert_eq!(current_address(&control), 0x0053_9378);
    assert_eq!(control.menus().len(), 2);
    assert!(!control.stopped());
}

#[test]
fn retail_start_game_defers_the_required_profile_factory_and_never_substitutes_a_phase() {
    let (mut control, mut host) = fixture(FrontendEntry::Returning);
    let selected = control.menus().current().unwrap().data().records().items[0];
    assert_eq!(selected.action_callback, 0x0048_3490);
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::Menu(MenuCommandOutcome::ActionDispatched)
    );
    assert_eq!(host.effects, [RecordedEffect::Sound(5)]);
    assert_eq!(control.menus().len(), 1);
    assert!(!control.stopped());

    host.effects.clear();
    let tick = SchedulerTick {
        delta_micros: 8_000,
        elapsed_micros: 8_000,
    };
    assert!(matches!(
        control.tick(tick, &mut host),
        Err(FrontendControlError::Host(
            "derived profile menu unavailable"
        ))
    ));
    assert_eq!(
        host.effects,
        [
            RecordedEffect::BaseInput,
            RecordedEffect::Music,
            RecordedEffect::StartMenu
        ]
    );
    assert_eq!(current_address(&control), ROOT_MENU_ADDRESS);
    assert_eq!(control.menus().len(), 1);
    assert!(control.stopped());
    let effect_count = host.effects.len();
    assert!(matches!(
        control.tick(tick, &mut host),
        Err(FrontendControlError::Stopped)
    ));
    assert_eq!(host.effects.len(), effect_count);
}

#[test]
fn retail_quit_rows_preserve_confirm_asymmetry_and_unconditional_auxiliary_commands() {
    for (yes_row, command, exit) in [
        (false, FrontendInput::Confirm, false),
        (true, FrontendInput::Confirm, true),
        (false, FrontendInput::Auxiliary6, true),
        (true, FrontendInput::Auxiliary6, true),
        (false, FrontendInput::Auxiliary7, false),
        (true, FrontendInput::Auxiliary7, false),
    ] {
        let (mut control, mut host) = fixture(FrontendEntry::Returning);
        assert_eq!(
            input(&mut control, &mut host, FrontendInput::Back),
            FrontendInputOutcome::PoppedMenu
        );
        assert_eq!(
            host.effects,
            [
                RecordedEffect::ClearHeld,
                RecordedEffect::SaveSettings,
                RecordedEffect::MenuLookup(QUIT_MENU_ADDRESS)
            ]
        );
        assert_eq!(current_address(&control), QUIT_MENU_ADDRESS);
        assert_eq!(control.menus().len(), 1);
        assert_eq!(control.menus().current().unwrap().selection(), 1);
        if yes_row {
            assert_eq!(
                input(&mut control, &mut host, FrontendInput::Next),
                FrontendInputOutcome::Menu(MenuCommandOutcome::Selection {
                    previous: 1,
                    current: 2
                })
            );
        }
        let current = control.menus().current().unwrap();
        let row = current.data().records().items[current.selection() as usize];
        assert_eq!(
            row.action_callback,
            if yes_row { 0x0048_36c0 } else { 0x0048_3620 }
        );
        host.effects.clear();
        assert_eq!(
            input(&mut control, &mut host, command),
            FrontendInputOutcome::Menu(MenuCommandOutcome::ActionDispatched)
        );
        let mut expected = vec![if exit {
            RecordedEffect::Exit
        } else {
            RecordedEffect::MenuLookup(ROOT_MENU_ADDRESS)
        }];
        if command == FrontendInput::Confirm {
            expected.push(RecordedEffect::Sound(5));
        }
        assert_eq!(host.effects, expected, "yes={yes_row}, command={command:?}");
        assert_eq!(
            current_address(&control),
            if exit {
                QUIT_MENU_ADDRESS
            } else {
                ROOT_MENU_ADDRESS
            }
        );
        assert_eq!(control.menus().len(), 1);
    }
}

#[test]
fn retail_first_entry_requires_ready_draw_before_returning_camera_acceleration() {
    let (mut control, mut host) = fixture(FrontendEntry::First);
    assert_eq!(control.entry_state(), FrontendEntry::First);
    assert!(!control.camera().ready());
    assert!(!control.menus().drawn());
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::CameraBlocked
    );
    assert!(!control.accelerated(), "first entrance cannot accelerate");
    assert!(host.effects.is_empty());

    // This bound stays below the 60-second attract threshold. The spring uses
    // original entrance coordinates, preset zero, and executable speed scales.
    for _ in 0..400 {
        if control.camera().ready() {
            break;
        }
        let result = draw(&mut control, &mut host);
        assert_eq!(result, None);
        control
            .tick(
                SchedulerTick {
                    delta_micros: 125_000,
                    elapsed_micros: 125_000,
                },
                &mut host,
            )
            .unwrap();
        assert_eq!(control.entry_state(), FrontendEntry::First);
        assert!(!control.menus().drawn());
    }
    assert!(host.draw_setups.is_empty());
    assert!(host.draw_rows.is_empty());
    assert!(
        control.camera().ready(),
        "retail entrance never reached its gate"
    );
    assert_eq!(control.menus().current().unwrap().age_millis(), 0);

    let root_heading = control.menus().current().unwrap().data().records().items[0];
    // The ready gate opens after the previous tick's camera integration. One
    // normal tick now advances the real menu controller to full fade.
    tick(&mut control, &mut host);
    assert_eq!(control.menus().fade(), 65535);
    assert_eq!(control.entry_state(), FrontendEntry::First);
    assert!(!control.menus().drawn());
    host.effects.clear();
    let result = draw(&mut control, &mut host);
    assert_eq!(
        result,
        Some(MenuDrawOutcome {
            rows: 3,
            menu_drawn: true
        })
    );
    assert_eq!(
        host.effects,
        [
            RecordedEffect::ControllerDraw,
            RecordedEffect::DelegatedRow(0x0048_34f0),
            RecordedEffect::DelegatedRow(0x0048_34f0),
            RecordedEffect::DelegatedRow(0x0048_34f0),
        ]
    );
    assert_eq!(host.draw_rows[0].arguments, root_heading.draw_arguments);
    assert_eq!(host.draw_rows[0].item_index, 0);
    assert_eq!(
        host.draw_rows[0].menu_source.authored_address().unwrap(),
        ROOT_MENU_ADDRESS
    );
    assert_eq!(
        host.draw_rows
            .iter()
            .map(|r| r.descriptor.position_pixels)
            .collect::<Vec<_>>(),
        [[390, 375], [390, 404], [390, 433]]
    );
    assert_eq!(host.clip, [0, 0, 800, 600]);
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(control.menus().drawn());
    assert!(!control.accelerated());

    // Once drawn, a normal tick executes original-table basis math, rather
    // than a successful no-op rotation hook.
    tick(&mut control, &mut host);
    assert_eq!(control.menus().current().unwrap().basis.words[0], 1);
    assert_eq!(control.menus().current().unwrap().age_millis(), 8);

    // Pushing the real Options definition clears the draw latch again.
    input(&mut control, &mut host, FrontendInput::Next);
    input(&mut control, &mut host, FrontendInput::Confirm);
    assert_eq!(current_address(&control), 0x0053_9378);
    assert_eq!(control.camera().selected_preset(), 2);
    assert!(!control.camera().ready());
    assert!(!control.menus().drawn());
    let effect_count = host.effects.len();
    assert_eq!(
        input(&mut control, &mut host, FrontendInput::Confirm),
        FrontendInputOutcome::CameraBlocked
    );
    assert!(control.accelerated());
    assert_eq!(host.effects.len(), effect_count);
}
