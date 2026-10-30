use super::*;
use crate::menu::{
    MenuBasis, MenuCommand, MenuCommandOutcome, MenuInput, MenuState, MenuVisibility,
    PreparedCommand,
};

fn row() -> FrontendMenuItem {
    FrontendMenuItem {
        virtual_address: 0x536da8,
        span: ByteSpan::new(0, 32),
        flags: 5,
        draw_callback: 0x449320,
        draw_arguments: [0, 0],
        action_callback: 0x449300,
        action_arguments: [0, 0],
        row_height: 2,
        age: 7,
    }
}

fn layouts() -> MenuLayouts {
    MenuLayouts::new(
        0x536d38,
        ByteSpan::new(16, 32),
        vec![FrontendMenuLayout {
            virtual_address: 0x536d38,
            span: ByteSpan::new(0, 16),
            flags: 2,
            item_count: 1,
            visible_rows: 1,
            reserved: 0,
            x: 80,
            y: 360,
            width: 840,
            height: 200,
            row_spacing: 45,
            scroll_animation: 19,
        }],
    )
}

fn derived(items: Option<Vec<FrontendMenuItem>>, layouts: Option<MenuLayouts>) -> MenuData {
    MenuData::derived(MenuDerivedData {
        constructor: 0x448220,
        vtable: 0x524f28,
        items,
        layouts,
        initial_selection: 0,
        flags: 0,
    })
}

#[test]
fn derived_rows_are_owned_per_instance_and_layouts_retain_process_identity() {
    let shared = layouts();
    let first = derived(Some(vec![row()]), Some(shared.clone()));
    let second = derived(Some(vec![row()]), Some(shared.clone()));
    let first_alias = first.clone();
    first.records_mut().items[0].row_height = 8;
    first.records_mut().items[0].age = 123;
    assert_eq!(first_alias.records().items[0].row_height, 8);
    assert_eq!(second.records().items[0].row_height, 2);
    assert_eq!(second.records().items[0].age, 7);
    first.records_mut().layouts[0].scroll_animation = -170;
    assert_eq!(second.records().layouts[0].scroll_animation, -170);
    assert_eq!(shared.records()[0].scroll_animation, -170);
    assert_eq!(first.records().items[0].virtual_address, 0x536da8);
    assert_eq!(first.layout_source().unwrap().address, 0x536d38);
    assert_eq!(first.source().authored_address(), None);
}

#[test]
fn null_row_binding_is_distinct_from_bound_empty_rows() {
    let mut absent = MenuState::new(derived(None, None), MenuBasis::INITIAL);
    let input = MenuInput {
        command: MenuCommand::Activate,
        visibility: MenuVisibility::OwnerFlagClear,
        context: 7,
    };
    assert!(matches!(
        absent.prepare_command(input).unwrap(),
        PreparedCommand::Complete(MenuCommandOutcome::NoAction)
    ));
    let mut bound_empty = MenuState::new(derived(Some(Vec::new()), None), MenuBasis::INITIAL);
    assert!(matches!(
        bound_empty.prepare_command(input),
        Err(crate::menu::MenuStateError::ItemOutOfBounds { index: 0, count: 0 })
    ));
}

#[test]
fn derived_callback_retains_owner_provenance_without_a_definition_address() {
    let mut menu = MenuState::new(
        derived(Some(vec![row()]), Some(layouts())),
        MenuBasis::INITIAL,
    );
    let PreparedCommand::Action(request) = menu
        .prepare_command(MenuInput {
            command: MenuCommand::Activate,
            visibility: MenuVisibility::OwnerFlagClear,
            context: 7,
        })
        .unwrap()
    else {
        panic!("actual row callback required");
    };
    assert_eq!(
        request.menu_source,
        MenuSource::Derived {
            constructor: 0x448220,
            vtable: 0x524f28
        }
    );
    assert_eq!(request.callback_address, 0x449300);
}

#[test]
fn derived_rebind_preserves_base_state_and_shared_layout_animation() {
    let shared = layouts();
    let mut menu = MenuState::new(
        derived(Some(vec![row()]), Some(shared.clone())),
        MenuBasis::INITIAL,
    );
    menu.age_millis = 42;
    menu.basis.words[0] = 11;
    menu.scroll_start = 5;
    menu.set_navigation_locked(true);
    shared.records_mut()[0].scroll_animation = -81;
    menu.replace_data(derived(Some(vec![row()]), Some(shared.clone())), 1, None);
    assert_eq!(menu.selection(), 1);
    assert_eq!(menu.scroll_start(), 5);
    assert_eq!(menu.age_millis(), 42);
    assert_eq!(menu.basis.words[0], 11);
    assert!(menu.navigation_locked());
    assert_eq!(shared.records()[0].scroll_animation, -81);
    assert_eq!(menu.data().records().items[0].age, 7);
    menu.replace_data(derived(Some(vec![row()]), Some(shared.clone())), 0, Some(0));
    assert_eq!(menu.scroll_start(), 0);
    assert_eq!(shared.records()[0].scroll_animation, -81);
}
