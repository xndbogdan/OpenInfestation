use inf_formats::{ByteSpan, FrontendMenu, FrontendMenuItem, FrontendMenuLayout};

use super::*;

fn data(count: usize, visible: u8, layout_flags: u8) -> MenuData {
    MenuData::new(FrontendMenu {
        virtual_address: 0x53c088,
        definition_span: ByteSpan::new(0, 20),
        items_address: 0x53c008,
        layouts_address: 0x53c068,
        initial_selection: 0,
        flags: 0,
        items: (0..count)
            .map(|index| FrontendMenuItem {
                virtual_address: 0x53c008 + index as u32 * 32,
                span: ByteSpan::new(index * 32, index * 32 + 32),
                flags: 4,
                draw_callback: 0x4834f0,
                draw_arguments: [204 + index as u32, 0],
                action_callback: 0x483490 + index as u32 * 32,
                action_arguments: [0, index as u32],
                row_height: 1,
                age: 0,
            })
            .collect(),
        layouts: vec![FrontendMenuLayout {
            virtual_address: 0x53c068,
            span: ByteSpan::new(0, 16),
            flags: layout_flags,
            item_count: count as u8,
            visible_rows: visible,
            reserved: 0,
            x: 500,
            y: 640,
            width: 450,
            height: 900,
            row_spacing: 50,
            scroll_animation: 0,
        }],
        layout_terminator: ByteSpan::new(16, 32),
    })
}

fn state(data: MenuData) -> MenuState {
    MenuState::new(data, MenuBasis::INITIAL)
}
fn stack(data: MenuData) -> MenuStack {
    let mut stack = MenuStack::new(MenuControllerSpec {
        fade_rate: 65535,
        viewport: [0, 0, 1024, 1024],
    });
    stack.push(state(data));
    stack
}
fn input(command: MenuCommand) -> MenuInput {
    MenuInput {
        command,
        visibility: MenuVisibility::OwnerFlagClear,
        context: 0x04725300,
    }
}

#[derive(Default)]
struct ProbeHost {
    actions: Vec<MenuActionRequest>,
    rotations: Vec<i32>,
    replace_on_action: Option<MenuData>,
    fail_action: bool,
    fail_rotation: bool,
}
impl MenuHost for ProbeHost {
    type Error = &'static str;
    fn action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        self.actions.push(request);
        if let Some(data) = self.replace_on_action.take() {
            stack.replace(state(data));
        }
        if self.fail_action {
            Err("action unavailable")
        } else {
            Ok(())
        }
    }
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.rotations.push(angle);
        if self.fail_rotation {
            return Err("rotation unavailable");
        }
        // Deliberate test observer mutation, not a production matrix substitute.
        basis.words[0] = basis.words[0].wrapping_add(1);
        Ok(())
    }
}

#[test]
fn reopening_shares_table_mutations_but_not_selection() {
    let data = data(3, 3, 2);
    let mut first = state(data.clone());
    first.selection = 2;
    first.age_millis = 30;
    {
        let mut records = data.records_mut();
        records.items[0].row_height = 3;
        records.items[0].age = 55;
        records.layouts[0].scroll_animation = 200;
    }
    let second = state(data.clone());
    assert_eq!((first.selection(), second.selection()), (2, 0));
    assert_eq!(first.age_millis(), 30);
    assert_eq!(first.data().records().items[0].age, 0);
    assert_eq!(second.data().records().items[0].row_height, 3);
    assert_eq!(data.records().layouts[0].scroll_animation, 0);
}

#[test]
fn reset_ages_does_not_reset_scroll_selection_height_or_animation() {
    let data = data(3, 3, 2);
    let mut menu = state(data.clone());
    menu.selection = 2;
    menu.scroll_start = 1;
    data.records_mut().layouts[0].scroll_animation = -200;
    data.records_mut().items[1].age = 500;
    menu.age_millis = 700;
    menu.reset_ages();
    assert_eq!(
        (menu.selection(), menu.scroll_start(), menu.age_millis()),
        (2, 1, 0)
    );
    assert_eq!(data.records().items[1].age, 0);
    assert_eq!(data.records().items[1].row_height, 1);
    assert_eq!(data.records().layouts[0].scroll_animation, -200);
}

#[test]
fn root_navigation_stops_at_boundaries_and_skips_masked_or_disabled_rows() {
    let data = data(4, 4, 2);
    data.records_mut().items[1].flags = 1;
    data.records_mut().items[2].flags = 4 | 8;
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    stack
        .command(input(MenuCommand::Previous), &mut host)
        .unwrap();
    assert_eq!(stack.current().unwrap().selection(), 0);
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().selection(), 3);
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().selection(), 3);
    stack
        .command(input(MenuCommand::Previous), &mut host)
        .unwrap();
    assert_eq!(stack.current().unwrap().selection(), 0);
    let mut event = input(MenuCommand::Next);
    event.visibility = MenuVisibility::from_owner_flag(true);
    stack.command(event, &mut host).unwrap();
    assert_eq!(stack.current().unwrap().selection(), 1);
}

#[test]
fn navigation_lock_does_not_block_action_or_select_last() {
    let data = data(3, 3, 2);
    data.records_mut().items[0].flags = 0; // Action path does not repeat visibility checks.
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    stack.current_mut().unwrap().set_navigation_locked(true);
    assert_eq!(
        stack.command(input(MenuCommand::Next), &mut host).unwrap(),
        MenuCommandOutcome::NavigationLocked
    );
    stack
        .command(input(MenuCommand::Activate), &mut host)
        .unwrap();
    assert_eq!(host.actions[0].command, MenuCommand::Activate);
    assert_eq!(host.actions[0].context, 0x04725300);
    stack
        .command(input(MenuCommand::SelectLast), &mut host)
        .unwrap();
    assert_eq!(stack.current().unwrap().selection(), 2);
}

#[test]
fn swapped_axes_apply_before_navigation_lock_and_dispatch() {
    let data = data(3, 3, 2);
    data.records_mut().flags = 1;
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(host.actions[0].command, MenuCommand::AdjustNext);
    assert_eq!(stack.current().unwrap().selection(), 0);
    stack
        .command(input(MenuCommand::AdjustNext), &mut host)
        .unwrap();
    assert_eq!(stack.current().unwrap().selection(), 1);
}

#[test]
fn action_can_replace_its_own_menu_and_failure_is_not_accepted() {
    let mut stack = stack(data(3, 3, 2));
    let replacement = data(2, 2, 2);
    replacement.records_mut().initial_selection = 1;
    let mut host = ProbeHost {
        replace_on_action: Some(replacement),
        fail_action: true,
        ..Default::default()
    };
    assert!(matches!(
        stack.command(input(MenuCommand::Activate), &mut host),
        Err(MenuError::Host("action unavailable"))
    ));
    assert_eq!(host.actions[0].item_index, 0);
    assert_eq!(host.actions[0].arguments, [0, 0]);
    assert_eq!(stack.current().unwrap().selection(), 1);
    assert_eq!(stack.len(), 1);
    assert!(!stack.drawn());
}

#[test]
fn null_actions_return_without_reporting_dispatch() {
    let data = data(1, 1, 2);
    data.records_mut().items[0].action_callback = 0;
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    assert_eq!(
        stack
            .command(input(MenuCommand::Activate), &mut host)
            .unwrap(),
        MenuCommandOutcome::NoAction
    );
    assert!(host.actions.is_empty());
}

#[test]
fn scroll_changes_animate_and_clear_entering_and_leaving_ages() {
    let data = data(6, 3, 2);
    let mut stack = stack(data.clone());
    for item in data.records_mut().items.iter_mut() {
        item.age = 77;
    }
    let mut host = ProbeHost::default();
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().scroll_start(), 0);
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().scroll_start(), 1);
    assert_eq!(data.records().layouts[0].scroll_animation, 200);
    assert_eq!(
        data.records()
            .items
            .iter()
            .map(|row| row.age)
            .collect::<Vec<_>>(),
        [0, 77, 77, 0, 77, 77]
    );
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().scroll_start(), 2);
    assert_eq!(data.records().layouts[0].scroll_animation, 400);
    stack
        .command(input(MenuCommand::Previous), &mut host)
        .unwrap();
    assert_eq!(stack.current().unwrap().scroll_start(), 1);
    assert_eq!(data.records().layouts[0].scroll_animation, 200);
}

#[test]
fn row_height_feedback_changes_scroll_without_assuming_one_row() {
    let data = data(5, 3, 2);
    data.records_mut().items[0].row_height = 2;
    let mut stack = stack(data.clone());
    let mut host = ProbeHost::default();
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(stack.current().unwrap().scroll_start(), 1);
    assert_eq!(data.records().layouts[0].scroll_animation, 400);
}

#[test]
fn circular_layout_wraps_without_artificial_scroll_animation() {
    let data = data(3, 1, 0x18);
    let mut stack = stack(data.clone());
    let mut host = ProbeHost::default();
    stack
        .command(input(MenuCommand::Previous), &mut host)
        .unwrap();
    assert_eq!(
        (
            stack.current().unwrap().selection(),
            stack.current().unwrap().scroll_start()
        ),
        (2, 2)
    );
    assert_eq!(data.records().layouts[0].scroll_animation, 0);
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(
        (
            stack.current().unwrap().selection(),
            stack.current().unwrap().scroll_start()
        ),
        (0, 0)
    );
    assert_eq!(data.records().layouts[0].scroll_animation, 0);
}

#[test]
fn cross_layout_previous_recomputes_bottom_window_from_measured_heights() {
    let data = data(6, 2, 2);
    {
        let mut records = data.records_mut();
        records.layouts[0].item_count = 4;
        let mut second = records.layouts[0];
        second.item_count = 2;
        second.virtual_address += 16;
        records.layouts.push(second);
        records.initial_selection = 4;
    }
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    stack
        .command(input(MenuCommand::Previous), &mut host)
        .unwrap();
    assert_eq!(
        (
            stack.current().unwrap().selection(),
            stack.current().unwrap().scroll_start()
        ),
        (3, 2)
    );
    stack.command(input(MenuCommand::Next), &mut host).unwrap();
    assert_eq!(
        (
            stack.current().unwrap().selection(),
            stack.current().unwrap().scroll_start()
        ),
        (4, 0)
    );
}

#[test]
fn malformed_wrapping_rows_fail_instead_of_hanging_or_picking_hidden_item() {
    let data = data(3, 1, 0x18);
    for item in data.records_mut().items.iter_mut() {
        item.flags = 8;
    }
    let mut stack = stack(data);
    let mut host = ProbeHost::default();
    assert!(matches!(
        stack.command(input(MenuCommand::Next), &mut host),
        Err(MenuError::State(MenuStateError::NoSelectableItem))
    ));
}

#[test]
fn tick_uses_native_signed_fixed_point_and_per_call_millisecond_truncation() {
    let data = data(2, 2, 2);
    let mut menu = state(data.clone());
    let mut second_layout = data.records().layouts[0];
    second_layout.scroll_animation = -200;
    data.records_mut().layouts.push(second_layout);
    data.records_mut().layouts[0].scroll_animation = 200;
    data.records_mut().items[1].age = 65530;
    let mut host = ProbeHost::default();
    menu.tick(16000, &mut host).unwrap();
    assert_eq!(menu.age_millis(), 16);
    assert_eq!(data.records().items[1].age, 65535);
    assert_eq!(data.records().layouts[0].scroll_animation, 185);
    assert_eq!(data.records().layouts[1].scroll_animation, -184);
    menu.tick(999, &mut host).unwrap();
    menu.tick(999, &mut host).unwrap();
    assert_eq!(menu.age_millis(), 16);
    assert_eq!(host.rotations, [160, 9, 9]);
    assert_eq!(menu.basis.words[0], 3);
}

#[test]
fn briefing_detail_tick_clears_both_scroll_bytes_before_one_base_tick() {
    let data = data(2, 2, 2);
    let mut menu = state(data.clone()).with_tick_policy(MenuTickPolicy::BriefingDetail);
    let mut second_layout = data.records().layouts[0];
    second_layout.scroll_animation = -0x2345;
    {
        let mut records = data.records_mut();
        records.layouts[0].scroll_animation = 0x1234;
        records.layouts.push(second_layout);
        records.items[0].age = 42;
        records.items[1].age = 65530;
    }
    let mut host = ProbeHost::default();
    menu.tick(16000, &mut host).unwrap();
    assert_eq!(menu.age_millis(), 32, "D5F0 then 452470 each advance +44");
    let records = data.records();
    assert!(records
        .layouts
        .iter()
        .all(|layout| layout.scroll_animation == 0));
    assert_eq!(records.layouts[0].row_spacing, 50);
    assert_eq!(records.layouts[1].flags, 2);
    assert_eq!(
        records.items[0].age, 58,
        "row ages advance only in the base"
    );
    assert_eq!(records.items[1].age, 65535);
    assert_eq!(host.rotations, [160]);
    assert_eq!(menu.basis.words[0], 1);
}

#[test]
fn briefing_detail_tick_retains_native_prefix_when_rotation_fails() {
    let data = data(1, 1, 2);
    let mut menu = state(data.clone()).with_tick_policy(MenuTickPolicy::BriefingDetail);
    data.records_mut().layouts[0].scroll_animation = -0x1234;
    let mut host = ProbeHost {
        fail_rotation: true,
        ..Default::default()
    };
    assert!(matches!(
        menu.tick(16000, &mut host),
        Err(MenuError::Host("rotation unavailable"))
    ));
    assert_eq!(menu.age_millis(), 32);
    assert_eq!(data.records().layouts[0].scroll_animation, 0);
    assert_eq!(data.records().items[0].age, 16);
    assert_eq!(host.rotations, [160]);
    assert_eq!(menu.basis.words[0], 0);
}

#[test]
fn briefing_detail_age_keeps_signed_truncation_and_dword_wrapping() {
    let data = data(1, 1, 2);
    let mut menu = state(data.clone()).with_tick_policy(MenuTickPolicy::BriefingDetail);
    let mut host = ProbeHost::default();
    menu.tick(999, &mut host).unwrap();
    menu.tick(999, &mut host).unwrap();
    assert_eq!(menu.age_millis(), 0);
    menu.age_millis = i32::MAX - 1;
    menu.tick(2000, &mut host).unwrap();
    assert_eq!(menu.age_millis(), i32::MIN + 2);
    menu.tick(-1999, &mut host).unwrap();
    assert_eq!(menu.age_millis(), i32::MIN);
    assert_eq!(data.records().items[0].age, 1);
    assert_eq!(host.rotations, [9, 9, 20, -19]);
}

#[test]
fn failed_rotation_retains_prior_native_timing_mutations() {
    let data = data(1, 1, 2);
    let mut menu = state(data.clone());
    let mut host = ProbeHost {
        fail_rotation: true,
        ..Default::default()
    };
    assert!(matches!(
        menu.tick(16000, &mut host),
        Err(MenuError::Host("rotation unavailable"))
    ));
    assert_eq!(menu.age_millis(), 16);
    assert_eq!(data.records().items[0].age, 16);
    assert_eq!(menu.basis, MenuBasis::INITIAL);
}

#[test]
fn controller_requires_full_fade_and_real_draw_before_menu_tick() {
    let mut stack = stack(data(3, 3, 2));
    let mut host = ProbeHost::default();
    assert!(stack.tick(8000, &mut host).unwrap());
    assert_eq!(stack.fade(), 65535);
    assert_eq!(stack.current().unwrap().age_millis(), 0);
    assert!(host.rotations.is_empty());
    stack.mark_drawn();
    stack.tick(8000, &mut host).unwrap();
    assert_eq!(stack.current().unwrap().age_millis(), 8);
    stack.push(state(data(1, 1, 2)));
    stack.tick(8000, &mut host).unwrap();
    assert_eq!(stack.current().unwrap().age_millis(), 0);
    assert_eq!(host.rotations, [80]);
    stack.set_active(false);
    assert!(!stack.tick(8000, &mut host).unwrap());
    assert!(stack.is_empty());
}

#[test]
fn controller_zero_delta_at_initial_zero_destroys_stack_as_retail_does() {
    let mut stack = stack(data(1, 1, 2));
    assert!(!stack.tick(0, &mut ProbeHost::default()).unwrap());
    assert!(stack.is_empty());
}

#[test]
fn pop_resets_exposed_ages_and_draw_gate_but_retains_selection_and_animation() {
    let first = data(3, 3, 2);
    let mut stack = stack(first.clone());
    stack.current_mut().unwrap().selection = 2;
    first.records_mut().layouts[0].scroll_animation = 200;
    stack.push(state(data(1, 1, 2)));
    first.records_mut().items[2].age = 77;
    stack.mark_drawn();
    stack.pop(1);
    assert_eq!(stack.current().unwrap().selection(), 2);
    assert_eq!(first.records().items[2].age, 0);
    assert_eq!(first.records().layouts[0].scroll_animation, 200);
    assert!(!stack.drawn());
    first.records_mut().items[2].age = 88;
    stack.pop(0);
    assert_eq!(first.records().items[2].age, 0);
}

/// Access Database (`0052FA60`) has two rows, the title and the entry, under
/// a second layout that counts two rows from row 1 and wraps. Previous wraps
/// from the entry to row 2, the record past the array: the definition
/// itself, whose first DWORD (its rows' address `0052FA20`) is no visible
/// row's flags, so `00452E20` steps on to the entry row again. Before, the
/// port stopped at row 2 and every shown menu stopped answering the keys.
#[test]
fn the_database_wraps_past_its_rows_back_to_the_entry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
        crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
    );
    let database = catalog.menu_data(0x0052_fa60).unwrap();
    assert_eq!(database.records().items.len(), 2);
    assert_eq!(database.census_flags(2), Some(0x0052_fa20));
    let mut stack = stack(database);
    let mut host = ProbeHost::default();
    for command in [
        MenuCommand::Previous,
        MenuCommand::Next,
        MenuCommand::Previous,
        MenuCommand::Previous,
    ] {
        // Gameplay's owner byte +5C2 is set (0048742E): rows with bit 1 show.
        let input = MenuInput {
            command,
            visibility: MenuVisibility::OwnerFlagSet,
            context: 0,
        };
        stack.command(input, &mut host).unwrap();
        assert_eq!(stack.current().unwrap().selection(), 1, "{command:?}");
    }
}

/// The derived menus' and the base controller's tables: `+00`, the scalar
/// deleting destructor, calls the body; both derived bodies end in the base
/// menu's `00452420`, which unlinks the menu.
#[test]
fn menu_classes_destroy_through_their_tables() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let code = |address: u32, length: usize| &exe[(address - 0x0040_0000) as usize..][..length];
    let word = |address: u32| u32::from_le_bytes(code(address, 4).try_into().unwrap());
    let relative = |instruction: u32| (instruction + 5).wrapping_add(word(instruction + 1));
    for (name, table, deleting, body) in [
        ("profile", 0x0052_4f28, 0x0044_82b0, 0x0044_82d0),
        ("briefing detail", 0x0052_0394, 0x0040_d580, 0x0040_d5a0),
        ("base controller", 0x0052_561c, 0x0045_33e0, 0x0045_3400),
    ] {
        assert_eq!(word(table), deleting, "{name}");
        assert_eq!(code(deleting, 4), [0x56, 0x8b, 0xf1, 0xe8], "{name}");
        assert_eq!(relative(deleting + 3), body, "{name}");
        assert_eq!(code(deleting + 8, 4), [0xf6, 0x44, 0x24, 0x08], "{name}");
    }
    // The profile menu frees its rows +3C, then calls the base; the
    // briefing detail menu's body is a jump to it.
    assert_eq!(code(0x0044_82f3, 3), [0x8b, 0x46, 0x3c]);
    assert_eq!(code(0x0044_8315, 1), [0xe8]);
    assert_eq!(relative(0x0044_8315), 0x0045_2420);
    assert_eq!(code(0x0040_d5a0, 1), [0xe9]);
    assert_eq!(relative(0x0040_d5a0), 0x0045_2420);
}
