use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::menu::{MenuDrawPlacement, MenuSource};
use crate::render_resources::RenderResources;
use crate::residency::ResourceSelection;
use crate::{Installation, ResidencyRequest, ResidentRuntime};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

/// Phase 12 keeps the frontend's residency: root plus leaf 47.
fn frontend_residency() -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

fn tables() -> KeyBindingTables {
    let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
    KeyBindingTables::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        assets.key_names(),
    )
    .unwrap()
}

fn request(row: u32, flags: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::AuthoredDefinition {
            address: 0x0053_5e40,
            span: inf_formats::ByteSpan::new(0, 0),
        },
        layout_index: 0,
        item_index: row as usize + 1,
        callback_address: BINDING_ROW_CALLBACK,
        arguments: [row, 0],
        context_origin_pixels: [320, 0],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [40, 100],
            extent_pixels: [402, 18],
            reveal: u16::MAX,
            flags,
            age_millis: 0,
            color: MenuColor::from_word(0x0012_3456),
        },
    }
}

#[test]
fn an_ordinary_row_lists_four_names_with_their_spaces() {
    let runtime = frontend_residency();
    let resources = RenderResources::new(&runtime);
    let tables = tables();
    let bindings = KeyBindings::initial(&tables);
    let state = BindingRowState {
        tables: &tables,
        bindings: &bindings,
        capturing: false,
        clock: 0,
    };
    // Row 0 is Up, action 0, bound to code C8 alone.
    let plan = binding_row_plan(&request(0, 0), state, &resources).unwrap();
    assert_eq!(plan.left.as_deref(), Some(resources.string(903).unwrap()));
    assert_eq!(plan.right.as_deref(), Some(&b"Up    "[..]));
    // Three quarters of the width, signed toward zero.
    assert_eq!(plan.local.extent_pixels, [301, 18]);
    assert_eq!(plan.local.color, MenuColor::from_word(0x0012_3456));
    assert_eq!(plan.joystick, None);
}

#[test]
fn a_joystick_bound_action_adds_its_button_number() {
    let runtime = frontend_residency();
    let resources = RenderResources::new(&runtime);
    let tables = tables();
    let bindings = KeyBindings::initial(&tables);
    let thrust = tables.descriptor_for_name(b"Thrust").unwrap();
    let row = tables
        .rows
        .iter()
        .position(|row| tables.action_descriptors[usize::from(row.action_index)] == thrust)
        .unwrap();
    let plan = binding_row_plan(
        &request(row as u32, 0),
        BindingRowState {
            tables: &tables,
            bindings: &bindings,
            capturing: false,
            clock: 0,
        },
        &resources,
    )
    .unwrap();
    assert_eq!(plan.joystick.as_deref(), Some(&b"Joystick 3"[..]));
}

#[test]
fn duplicated_first_codes_take_the_selected_or_ordinary_warning_style() {
    let runtime = frontend_residency();
    let resources = RenderResources::new(&runtime);
    let tables = tables();
    let mut bindings = KeyBindings::initial(&tables);
    bindings.set_codes(0, [0x39, 0, 0, 0]).unwrap();
    let space_holder = (0..44_u16)
        .find(|action| *action != 0 && bindings.record(*action).unwrap().codes[0] == 0x39)
        .expect("Space is authored for one action");
    let state = BindingRowState {
        tables: &tables,
        bindings: &bindings,
        capturing: false,
        clock: 0,
    };
    for (flags, style) in [(0, 0x25), (1, 0x26)] {
        let plan = binding_row_plan(&request(0, flags), state, &resources).unwrap();
        assert_eq!(
            plan.local.color,
            MenuColor::from_word(resources.style(style).unwrap())
        );
    }
    let row = tables
        .rows
        .iter()
        .position(|row| row.action_index == space_holder)
        .unwrap();
    let plan = binding_row_plan(&request(row as u32, 0), state, &resources).unwrap();
    assert_eq!(
        plan.local.color,
        MenuColor::from_word(resources.style(0x25).unwrap())
    );
}

#[test]
fn a_capturing_row_blinks_the_prompt_on_odd_clock_steps() {
    let runtime = frontend_residency();
    let resources = RenderResources::new(&runtime);
    let tables = tables();
    let bindings = KeyBindings::initial(&tables);
    let prompt = resources.string(CAPTURE_PROMPT_STRING).unwrap().to_vec();
    for (clock, expected) in [
        (0, None),
        (14, None),
        (15, Some(prompt.clone())),
        (30, None),
        (45, Some(prompt)),
    ] {
        let plan = binding_row_plan(
            &request(1, 0),
            BindingRowState {
                tables: &tables,
                bindings: &bindings,
                capturing: true,
                clock,
            },
            &resources,
        )
        .unwrap();
        assert_eq!(plan.right, expected, "clock {clock}");
        assert_eq!(plan.local.extent_pixels[0], 301);
    }
    assert!(matches!(
        binding_row_plan(
            &request(44, 0),
            BindingRowState {
                tables: &tables,
                bindings: &bindings,
                capturing: false,
                clock: 0,
            },
            &resources,
        ),
        Err(BindingRowError::Row(44))
    ));
}
