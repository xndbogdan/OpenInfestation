use super::*;
use inf_formats::{ByteSpan, FrontendMenu, FrontendMenuItem, FrontendMenuLayout};
use std::collections::BTreeMap;

fn menu(address: u32, callbacks: &[(u32, [u32; 2])], selection: i32) -> MenuData {
    MenuData::new(FrontendMenu {
        virtual_address: address,
        definition_span: ByteSpan::new(0, 20),
        items_address: address + 100,
        layouts_address: address + 200,
        initial_selection: selection,
        flags: 0,
        items: callbacks
            .iter()
            .enumerate()
            .map(
                |(index, &(action_callback, action_arguments))| FrontendMenuItem {
                    virtual_address: address + 100 + index as u32 * 32,
                    span: ByteSpan::new(0, 32),
                    flags: 4,
                    draw_callback: 0,
                    draw_arguments: [0; 2],
                    action_callback,
                    action_arguments,
                    row_height: 1,
                    age: 0,
                },
            )
            .collect(),
        layouts: vec![FrontendMenuLayout {
            virtual_address: address + 200,
            span: ByteSpan::new(0, 16),
            flags: 1,
            item_count: callbacks.len() as u8,
            visible_rows: callbacks.len() as u8,
            reserved: 0,
            x: 0,
            y: 0,
            width: 1024,
            height: 1024,
            row_spacing: 10,
            scroll_animation: 0,
        }],
        layout_terminator: ByteSpan::new(16, 32),
    })
}

fn config(entry: FrontendEntry) -> FrontendControlConfig {
    FrontendControlConfig {
        camera_presets: (0..10)
            .map(|index| FrontendCameraPreset {
                virtual_address: 0x53c148 + index * 24,
                span: ByteSpan::new(0, 24),
                eye: [index as i32 * 1000, 0, 0],
                target: [0; 3],
            })
            .collect(),
        position_scale: 1,
        motion_scale: 1,
        entry,
        context: 71,
    }
}

fn with_root(
    config: FrontendControlConfig,
    root: MenuData,
) -> Result<FrontendControl, FrontendControlError<std::convert::Infallible>> {
    let mut control = FrontendControl::new(config)?;
    control
        .menus_mut()
        .push(MenuState::new(root, MenuBasis::INITIAL));
    Ok(control)
}

fn control(callback: u32, arguments: [u32; 2]) -> FrontendControl {
    with_root(
        config(FrontendEntry::Returning),
        menu(ROOT_MENU_ADDRESS, &[(callback, arguments)], 0),
    )
    .unwrap()
}

#[derive(Default)]
struct Host {
    events: Vec<String>,
    tables: BTreeMap<u32, MenuData>,
    profile_named: bool,
    start_unavailable: bool,
    fail_sound: bool,
    input_during_tick: Option<FrontendInput>,
    reenter_tick_and_swallow_error: bool,
    swallowed_reentry: bool,
}

impl FrontendHost for Host {
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
        self.events.push(format!("menu:{address:x}"));
        self.tables
            .get(&address)
            .cloned()
            .ok_or("missing authored menu")
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.events.push(format!("rotate:{angle}"));
        Ok(())
    }
    fn tick_base_input(
        &mut self,
        control: &mut FrontendControl,
        tick: SchedulerTick,
    ) -> Result<(), Self::Error> {
        self.events
            .push(format!("base:clock={}", control.animation().value()));
        if self.reenter_tick_and_swallow_error {
            self.reenter_tick_and_swallow_error = false;
            self.swallowed_reentry = matches!(
                control.tick(tick, self),
                Err(FrontendControlError::ReentrantTick)
            );
        }
        if let Some(input) = self.input_during_tick.take() {
            control
                .input(input, MenuVisibility::OwnerFlagClear, self)
                .map_err(|_| "input failure")?;
        }
        Ok(())
    }
    fn clear_held_input(&mut self) -> Result<(), Self::Error> {
        self.events.push("clear-held".into());
        Ok(())
    }
    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.events.push("music".into());
        Ok(())
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.events.push(format!("sound:{resource}"));
        if self.fail_sound {
            Err("sound unavailable")
        } else {
            Ok(())
        }
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.events.push("save".into());
        Ok(())
    }
    fn profile_first_byte_nonzero(&mut self) -> Result<bool, Self::Error> {
        self.events.push("profile-byte".into());
        Ok(self.profile_named)
    }
    fn open_start_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        self.events.push("start-menu".into());
        if self.start_unavailable {
            Err("profile menu unavailable")
        } else {
            Ok(())
        }
    }
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        self.events.push("network-menu".into());
        Err("network menu unavailable")
    }
    fn tick_network(
        &mut self,
        _: &mut crate::network_menu::NetworkMenu,
        _: i32,
    ) -> Result<(), Self::Error> {
        Err("network menu unavailable")
    }
    fn network_action(
        &mut self,
        _: &mut MenuStack,
        _: MenuActionRequest,
    ) -> Result<(), Self::Error> {
        Err("network menu unavailable")
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        self.events.push("refresh-world".into());
        Ok(())
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        self.events.push("restore-defaults".into());
        Ok(())
    }
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error> {
        self.events.push(format!("volume:{channel:?}"));
        Ok(())
    }
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.events.push(format!(
            "adjust-display:{address:x}:{}:{max}",
            command.native()
        ));
        Ok(())
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.events
            .push(format!("adjust:{address:x}:{}:{max}", command.native()));
        Ok(())
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.events.push(format!("phase:{phase}"));
        Ok(())
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.events.push(format!("attract:{enabled}"));
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.events.push("exit".into());
        Ok(())
    }
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        self.events.push(format!(
            "external:{:x}:{}:{}",
            request.callback_address,
            request.command.native(),
            request.context
        ));
        Ok(())
    }
}

fn input(
    control: &mut FrontendControl,
    command: FrontendInput,
    host: &mut Host,
) -> FrontendInputOutcome {
    control
        .input(command, MenuVisibility::OwnerFlagClear, host)
        .unwrap()
}
fn tick(control: &mut FrontendControl, delta: i32, elapsed: i32, host: &mut Host) {
    control
        .tick(
            SchedulerTick {
                delta_micros: delta,
                elapsed_micros: elapsed,
            },
            host,
        )
        .unwrap();
}

#[test]
fn start_is_deferred_after_base_input_and_music_and_missing_owner_stops() {
    let mut control = control(0x483490, [0; 2]);
    let mut host = Host {
        input_during_tick: Some(FrontendInput::Confirm),
        start_unavailable: true,
        ..Host::default()
    };
    let error = control
        .tick(
            SchedulerTick {
                delta_micros: 20_000,
                elapsed_micros: 20_000,
            },
            &mut host,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FrontendControlError::Host("profile menu unavailable")
    ));
    assert_eq!(
        host.events,
        ["base:clock=1", "sound:5", "music", "start-menu"]
    );
    assert!(control.stopped());
    assert!(matches!(
        control.input(
            FrontendInput::Confirm,
            MenuVisibility::OwnerFlagClear,
            &mut host
        ),
        Err(FrontendControlError::Stopped)
    ));
    assert_eq!(host.events.len(), 4);
}

#[test]
fn swallowed_recursive_tick_error_stops_outer_tick_before_music_or_menu_work() {
    let mut control = control(0, [0; 2]);
    control.menus_mut().mark_drawn();
    let mut host = Host {
        reenter_tick_and_swallow_error: true,
        ..Host::default()
    };
    let error = control
        .tick(
            SchedulerTick {
                delta_micros: 20_000,
                elapsed_micros: 20_000,
            },
            &mut host,
        )
        .unwrap_err();
    assert!(matches!(error, FrontendControlError::Stopped));
    assert!(host.swallowed_reentry);
    assert_eq!(host.events, ["base:clock=1"]);
    assert!(control.stopped());
    assert_eq!(control.animation().accumulated_micros(), 20_000);
    assert_eq!(control.menus().fade(), 0);
    assert_eq!(control.menus().current().unwrap().age_millis(), 0);
}

#[test]
fn options_uses_second_argument_and_camera_change_precedes_catalog_failure() {
    let mut control = control(0x4834b0, [0xdead, 0x539378]);
    let mut host = Host::default();
    let error = control
        .input(
            FrontendInput::Confirm,
            MenuVisibility::OwnerFlagClear,
            &mut host,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FrontendControlError::Host("missing authored menu")
    ));
    assert_eq!(control.camera().selected_preset(), 2);
    assert_eq!(host.events, ["menu:539378"]);
    assert_eq!(control.menus().len(), 1);
    assert!(control.stopped());
}

#[test]
fn authored_child_push_is_concrete_and_draw_callback_changes_nested_camera() {
    let child = menu(0x539378, &[(0x453c50, [0; 2])], 0);
    let mut control = control(0x4834b0, [0, 0x539378]);
    let mut host = Host::default();
    host.tables.insert(0x539378, child.clone());
    input(&mut control, FrontendInput::Confirm, &mut host);
    assert_eq!(control.menus().len(), 2);
    assert_eq!(host.events, ["menu:539378", "sound:5"]);
    assert_eq!(control.camera().selected_preset(), 2);
    let delegated = control
        .with_row_draw(0x45f520, [0xa2, 5], |address| Ok::<_, ()>(address))
        .unwrap();
    assert_eq!(delegated, 0x453a80);
    assert_eq!(control.camera().selected_preset(), 5);
    assert!(!control.menus().drawn());
    let delegated = control
        .with_row_draw(0x4835e0, [204, 0], |address| Ok::<_, ()>(address))
        .unwrap();
    assert_eq!(delegated, 0x4834f0);
    assert_eq!(control.camera().selected_preset(), 0);
}

#[test]
fn settings_adjust_action_forwards_address_command_and_max() {
    let mut control = control(0x0045_3b80, [7, 0x54e908]);
    let mut host = Host::default();
    input(&mut control, FrontendInput::AdjustNext, &mut host);
    input(&mut control, FrontendInput::AdjustPrevious, &mut host);
    input(&mut control, FrontendInput::Confirm, &mut host);
    assert_eq!(
        host.events,
        [
            "adjust:54e908:3:7",
            "sound:3",
            "adjust:54e908:2:7",
            "sound:3",
            "adjust:54e908:4:7",
            "sound:5",
        ]
    );
}

#[test]
fn world_refresh_action_adjusts_then_refreshes_in_native_order() {
    let mut control = control(0x0046_3b50, [3, 0x54e918]);
    let mut host = Host::default();
    input(&mut control, FrontendInput::AdjustNext, &mut host);
    assert_eq!(
        host.events,
        ["adjust:54e918:3:3", "refresh-world", "sound:3",]
    );
}

#[test]
fn sound_row_actions_adjust_then_apply_their_fixed_channel() {
    let mut effects = control(0x0046_3ac0, [15, 0x54e8a8]);
    let mut host = Host::default();
    input(&mut effects, FrontendInput::AdjustNext, &mut host);
    assert_eq!(
        host.events,
        ["adjust:54e8a8:3:15", "volume:Effects", "sound:3"]
    );
    let mut music = control(0x0046_3b00, [15, 0x54e8ac]);
    let mut host = Host::default();
    input(&mut music, FrontendInput::AdjustPrevious, &mut host);
    assert_eq!(
        host.events,
        ["adjust:54e8ac:2:15", "volume:Music", "sound:3"]
    );
    // The tail runs for every action command, even one 00453B80 ignores.
    let mut host = Host::default();
    input(&mut music, FrontendInput::Auxiliary7, &mut host);
    assert_eq!(&host.events[..2], ["adjust:54e8ac:7:15", "volume:Music"]);
}

/// 00463B70 takes no command, so Left, Right and Activate all restore the
/// defaults before the dispatcher's own navigation/confirmation sound.
#[test]
fn restore_defaults_runs_for_every_action_command() {
    let mut control = control(0x0046_3b70, [0, 0]);
    for (command, sound) in [
        (FrontendInput::AdjustPrevious, "sound:3"),
        (FrontendInput::AdjustNext, "sound:3"),
        (FrontendInput::Confirm, "sound:5"),
    ] {
        let mut host = Host::default();
        input(&mut control, command, &mut host);
        assert_eq!(host.events, ["restore-defaults", sound]);
    }
}

#[test]
fn display_adjust_action_forwards_address_command_and_max() {
    let mut control = control(0x0046_2f40, [2, 0x54e8e4]);
    let mut host = Host::default();
    input(&mut control, FrontendInput::AdjustNext, &mut host);
    assert_eq!(host.events, ["adjust-display:54e8e4:3:2", "sound:3",]);
}

#[test]
fn command_gate_preserves_auxiliary_inputs_and_accelerates_only_confirm_back() {
    let mut control = control(0x1111, [0; 2]);
    control
        .with_row_draw(0x45f520, [0, 2], |_| Ok::<_, ()>(()))
        .unwrap();
    let mut host = Host::default();
    assert_eq!(
        input(&mut control, FrontendInput::Next, &mut host),
        FrontendInputOutcome::CameraBlocked
    );
    assert!(!control.accelerated());
    input(&mut control, FrontendInput::RequestAttract, &mut host);
    input(&mut control, FrontendInput::Auxiliary7, &mut host);
    assert_eq!(control.idle_millis(), 60001);
    assert_eq!(host.events, ["external:1111:7:71"]);
    assert_eq!(
        input(&mut control, FrontendInput::Confirm, &mut host),
        FrontendInputOutcome::CameraBlocked
    );
    assert!(control.accelerated());
    assert_eq!(control.idle_millis(), 0);
    assert_eq!(
        input(&mut control, FrontendInput::Back, &mut host),
        FrontendInputOutcome::CameraBlocked
    );
    assert_eq!(host.events, ["external:1111:7:71", "clear-held"]);
}

#[test]
fn first_entry_blocked_confirm_does_not_enable_returning_acceleration() {
    let mut control = with_root(
        config(FrontendEntry::First),
        menu(ROOT_MENU_ADDRESS, &[(0, [0; 2])], 0),
    )
    .unwrap();
    let mut host = Host::default();
    assert_eq!(
        input(&mut control, FrontendInput::Confirm, &mut host),
        FrontendInputOutcome::CameraBlocked
    );
    assert!(!control.accelerated());
    assert!(host.events.is_empty());
}

#[test]
fn successful_ready_draw_changes_first_entry_to_returning_even_if_row_changes_target() {
    let mut initial = config(FrontendEntry::First);
    initial.camera_presets[0].eye = [-1200, 0, 7000];
    let mut control = with_root(initial, menu(ROOT_MENU_ADDRESS, &[(0, [0; 2])], 0)).unwrap();
    assert!(control.camera().ready());
    assert_eq!(control.entry_state(), FrontendEntry::First);
    let mut calls = 0;
    let result = control
        .draw_menu_if_ready(
            |_| None,
            |control| {
                calls += 1;
                control.with_row_draw(0x45f520, [0, 2], |address| Ok::<_, ()>(address))
            },
        )
        .unwrap();
    assert_eq!(result, Some(0x453a80));
    assert_eq!(calls, 1);
    assert!(!control.camera().ready());
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(control.menus().drawn());
    let mut host = Host::default();
    input(&mut control, FrontendInput::Confirm, &mut host);
    assert!(control.accelerated());
    let gated = control
        .draw_menu_if_ready(
            |_| None,
            |_| {
                calls += 1;
                Ok::<_, FrontendControlError<()>>(())
            },
        )
        .unwrap();
    assert_eq!(gated, None);
    assert_eq!(calls, 1);
    assert!(control.accelerated());
    control
        .with_row_draw(0x4835e0, [0; 2], |_| Ok::<_, ()>(()))
        .unwrap();
    control
        .draw_menu_if_ready(|_| None, |_| Ok::<_, FrontendControlError<()>>(()))
        .unwrap();
    assert!(!control.accelerated());
}

#[test]
fn failed_ready_draw_does_not_publish_returning_or_draw_completion() {
    let mut initial = config(FrontendEntry::First);
    initial.camera_presets[0].eye = [-1200, 0, 7000];
    let mut control = with_root(initial, menu(ROOT_MENU_ADDRESS, &[(0, [0; 2])], 0)).unwrap();
    let error = control
        .draw_menu_if_ready(
            |_| None,
            |_| Err::<(), _>(FrontendControlError::Host("draw failed")),
        )
        .unwrap_err();
    assert!(matches!(error, FrontendControlError::Host("draw failed")));
    assert_eq!(control.entry_state(), FrontendEntry::First);
    assert!(!control.menus().drawn());
    assert!(control.stopped());
}

#[test]
fn swallowed_recursive_draw_cannot_be_acknowledged_or_revived_by_native_error_policy() {
    let mut control = control(0, [0; 2]);
    let result = control.draw_menu_if_ready(
        |_| Some(0x301),
        |control| {
            assert!(matches!(
                control.draw_menu_if_ready(|_: &()| Some(0x301), |_| Ok(())),
                Err(FrontendControlError::ReentrantDraw)
            ));
            Err::<(), _>(FrontendControlError::Host(()))
        },
    );
    assert!(matches!(result, Err(FrontendControlError::Host(()))));
    assert!(control.stopped());
    assert!(!control.menus().drawn());
    assert!(matches!(
        control.draw_menu_if_ready(|_: &()| Some(0x301), |_| Ok(())),
        Err(FrontendControlError::Stopped)
    ));
}

#[test]
fn back_cancels_editing_before_save_or_pops_and_installs_quit() {
    let mut control = control(0, [0; 2]);
    let mut host = Host::default();
    host.tables.insert(
        QUIT_MENU_ADDRESS,
        menu(QUIT_MENU_ADDRESS, &[(0x483620, [0; 2])], 0),
    );
    control
        .menus_mut()
        .current_mut()
        .unwrap()
        .set_navigation_locked(true);
    assert_eq!(
        input(&mut control, FrontendInput::Back, &mut host),
        FrontendInputOutcome::CancelledEditing
    );
    assert_eq!(host.events, ["clear-held"]);
    assert_eq!(
        input(&mut control, FrontendInput::Back, &mut host),
        FrontendInputOutcome::PoppedMenu
    );
    assert_eq!(
        host.events,
        ["clear-held", "clear-held", "save", "menu:53c130"]
    );
    assert_eq!(
        control
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
}

#[test]
fn quit_rows_have_distinct_confirm_and_same_auxiliary_policies() {
    for (callback, command, exits) in [
        (0x483620, FrontendInput::Confirm, false),
        (0x4836c0, FrontendInput::Confirm, true),
        (0x483620, FrontendInput::Auxiliary6, true),
        (0x4836c0, FrontendInput::Auxiliary6, true),
        (0x483620, FrontendInput::Auxiliary7, false),
        (0x4836c0, FrontendInput::Auxiliary7, false),
    ] {
        let mut control = control(callback, [0; 2]);
        let mut host = Host::default();
        host.tables.insert(
            ROOT_MENU_ADDRESS,
            menu(ROOT_MENU_ADDRESS, &[(0, [0; 2])], 0),
        );
        input(&mut control, command, &mut host);
        assert_eq!(host.events[0], if exits { "exit" } else { "menu:53c088" });
        assert_eq!(
            host.events.len(),
            if command == FrontendInput::Confirm {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn idle_uses_raw_elapsed_strict_threshold_and_only_one_menu_clears_flag() {
    let mut control = control(0, [0; 2]);
    let mut host = Host::default();
    tick(&mut control, 1_000, 60_000_000, &mut host);
    assert_eq!(control.idle_millis(), 60000);
    assert_eq!(host.events.last().unwrap(), "attract:false");
    tick(&mut control, 1_000, 1_000, &mut host);
    assert_eq!(
        &host.events[host.events.len() - 2..],
        ["attract:true", "phase:9"]
    );
    assert_eq!(control.animation().accumulated_micros(), 2_000);
    control.menus_mut().push(MenuState::new(
        menu(1, &[(0, [0; 2])], 0),
        MenuBasis::INITIAL,
    ));
    host.events.clear();
    tick(&mut control, 1_000, 1_000, &mut host);
    assert_eq!(control.idle_millis(), 0);
    assert_eq!(host.events, ["base:clock=0", "music"]);
}

#[test]
fn settings_delay_checks_old_value_and_new_close_timer_loses_same_frame_delta() {
    let mut control = control(0, [0; 2]);
    control.menus_mut().push(MenuState::new(
        menu(1, &[(0, [0; 2])], 0),
        MenuBasis::INITIAL,
    ));
    let mut host = Host::default();
    control.arm_settings_close();
    tick(&mut control, 1_000_000, 1_000_000, &mut host);
    assert_eq!(control.state.persist_delay_millis, 1001);
    assert!(!host.events.iter().any(|event| event == "save"));
    control.arm_settings_close();
    tick(&mut control, 125_000, 125_000, &mut host);
    assert_eq!(control.state.close_delay_millis, 2875);
    assert_eq!(
        &host.events[host.events.len() - 2..],
        ["save", "profile-byte"]
    );
    assert_eq!(control.menus().len(), 2);
    tick(&mut control, 2_875_000, 2_875_000, &mut host);
    assert_eq!(control.menus().len(), 1);
    assert_eq!(control.state.close_delay_millis, 0);
}

#[test]
fn menu_tick_requires_real_draw_and_camera_gate_precedes_motion() {
    let mut control = control(0, [0; 2]);
    let mut host = Host::default();
    tick(&mut control, 20_000, 20_000, &mut host);
    assert!(!host.events.iter().any(|event| event.starts_with("rotate:")));
    control.menus_mut().mark_drawn();
    tick(&mut control, 20_000, 20_000, &mut host);
    assert!(host.events.iter().any(|event| event == "rotate:200"));
    assert_eq!(control.menus().current().unwrap().age_millis(), 20);
    control
        .with_row_draw(0x45f520, [0, 2], |_| Ok::<_, ()>(()))
        .unwrap();
    host.events.clear();
    tick(&mut control, 20_000, 20_000, &mut host);
    assert_eq!(control.menus().current().unwrap().age_millis(), 0);
    assert!(!host.events.iter().any(|event| event.starts_with("rotate:")));
}

#[test]
fn preset_nine_restarts_timer_on_each_real_row_draw_and_tick_ages_it() {
    let mut control = control(0, [0; 2]);
    let mut host = Host::default();
    control
        .with_row_draw(0x45f520, [0, 9], |_| Ok::<_, ()>(()))
        .unwrap();
    tick(&mut control, 20_000, 20_000, &mut host);
    assert_eq!(control.timer_0120_millis(), 21);
    control
        .with_row_draw(0x45f520, [0, 9], |_| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(control.timer_0120_millis(), 1);
    control
        .with_row_draw(0x4835e0, [0; 2], |_| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(control.timer_0120_millis(), 1);
}

#[test]
fn phase_request_precedes_sound_failure_and_is_not_rolled_back() {
    let mut control = control(0x453c70, [12, 0]);
    let mut host = Host {
        fail_sound: true,
        ..Host::default()
    };
    let error = control
        .input(
            FrontendInput::Confirm,
            MenuVisibility::OwnerFlagClear,
            &mut host,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FrontendControlError::Host("sound unavailable")
    ));
    assert_eq!(host.events, ["phase:12", "sound:5"]);
    assert!(control.stopped());
}
