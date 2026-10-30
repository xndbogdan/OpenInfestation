use std::path::Path;

use super::*;
use crate::frontend_assets::FrontendAssets;

fn tables() -> KeyBindingTables {
    let assets = FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
        .expect("retail Infest.exe is required");
    KeyBindingTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        assets.key_names(),
    )
    .unwrap()
}

#[test]
fn authored_rows_cover_every_action_once_across_both_tables() {
    let tables = tables();
    assert_eq!(
        tables.rows[0],
        KeyBindingRow {
            label_resource: 903,
            action_index: 0
        }
    );
    assert_eq!(
        tables.rows[43],
        KeyBindingRow {
            label_resource: 939,
            action_index: 43
        }
    );
    let mut actions: Vec<_> = tables.rows.iter().map(|row| row.action_index).collect();
    actions.sort_unstable();
    assert_eq!(actions, (0..44).collect::<Vec<_>>());
    // 00444E50: action 0 is 00534570, action 38 is 00534290.
    assert_eq!(tables.action_descriptors[0], 0x0053_a360);
    assert_eq!(tables.action_descriptors[38], 0x0053_4220);
    assert_eq!(tables.action_names.len(), 48);
    assert_eq!(tables.descriptor_for_name(b"Fire"), Some(FIRE_DESCRIPTOR));
    assert_eq!(tables.code_for_name(b"Enter"), Some(0x1c));
    // The first unnamed code is 0, so every empty name reloads as 0.
    assert_eq!(tables.code_for_name(b""), Some(0));
}

#[test]
fn the_first_table_defaults_forbid_alt_where_the_startup_image_does_not() {
    let tables = tables();
    for action in 0..38 {
        assert_eq!(tables.initial[action].forbidden, [0; 4], "action {action}");
        assert_eq!(tables.defaults[action].forbidden, [0x38, 0, 0, 0]);
        assert_eq!(tables.defaults[action].codes, tables.initial[action].codes);
    }
    for action in 38..44 {
        assert_eq!(tables.defaults[action], tables.initial[action]);
    }
    assert_eq!(tables.initial[0].codes, [0xc8, 0, 0, 0]);
    assert_eq!(tables.initial_buttons, tables.default_buttons);
    assert_eq!(tables.initial_buttons[0], FIRE_DESCRIPTOR);
    assert_eq!(tables.initial_button_tail, 0);
}

#[test]
fn duplicates_compare_first_codes_outside_the_cpu_cursor_actions() {
    let tables = tables();
    let mut bindings = KeyBindings::initial(&tables);
    // The CPU cursor actions 0x20..0x24 repeat the arrows and Enter.
    assert_eq!(bindings.record(0x20).unwrap().codes[0], 0xc8);
    assert!((0..44).all(|action| !bindings.duplicated(action)));
    bindings.set_codes(5, [0xc8, 0x2a, 0, 0]).unwrap();
    assert!(bindings.duplicated(0));
    assert!(bindings.duplicated(5));
    assert!(!bindings.duplicated(0x20));
    // Restore copies whole records, Alt forbid included.
    bindings.restore_defaults(&tables);
    assert_eq!(bindings.record(5).unwrap(), tables.defaults[5]);
    assert_eq!(bindings.record(0).unwrap().forbidden, [0x38, 0, 0, 0]);
    assert!(matches!(
        bindings.set_codes(44, [1, 0, 0, 0]),
        Err(KeyBindingError::UnknownAction(44))
    ));
}

#[test]
fn binding_a_button_unbinds_its_other_buttons_and_mirrors_fire() {
    let tables = tables();
    let mut bindings = KeyBindings::initial(&tables);
    let grenade = tables.descriptor_for_name(b"Grenade").unwrap();
    let holders: Vec<_> = (0..16)
        .filter(|button| bindings.buttons()[*button] == grenade)
        .collect();
    assert_eq!(holders, [1]);
    bindings.bind_button(5, grenade, 6).unwrap();
    assert_eq!(bindings.buttons()[1], UNBOUND_BUTTON_DESCRIPTOR);
    assert_eq!(bindings.buttons()[5], grenade);
    assert_eq!(bindings.button_for(grenade), Some(5));
    assert_eq!(bindings.button_tail(), 0, "only Fire is mirrored");
    bindings.bind_button(9, FIRE_DESCRIPTOR, 10).unwrap();
    assert_eq!(bindings.button_tail(), 10);
    assert!(bindings.bind_button(16, grenade, 17).is_none());
    // Restore copies the sixteen records, not the terminator byte.
    bindings.restore_defaults(&tables);
    assert_eq!(bindings.buttons(), tables.default_buttons);
    assert_eq!(bindings.button_tail(), 10);
}
