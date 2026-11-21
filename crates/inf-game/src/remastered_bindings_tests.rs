//! Retail-table checks for the Remastered pad's slot-8 tables.
//!
//! Every test reads `retail/Infest.exe`; the synthetic row addresses are only
//! the port's, never the descriptors' identities.

use std::path::{Path, PathBuf};

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindings, FrontendDevice};
use crate::input::{DeviceButtons, InputRuntime};
use inf_formats::{
    parse_frontend_input_tables, parse_phase12_input_tables, parse_phase3_input_tables,
    parse_phase5_input_tables, parse_phase7_input_tables, parse_phase8_input_tables,
    parse_startup_input_tables, parse_text_entry_input_tables, parse_title_input_tables,
    AnalogInputTable, DigitalInputTable, FrontendExecutable, ScreenInputTables,
    ANALOG_INPUT_RECORD_BYTES, DIGITAL_INPUT_RECORD_BYTES, INPUT_DEVICE_SLOTS,
};

use super::*;

const ALL_SCREENS: [RemasteredScreen; 11] = [
    RemasteredScreen::Frontend,
    RemasteredScreen::Briefing,
    RemasteredScreen::Results,
    RemasteredScreen::TextEntry,
    RemasteredScreen::InWorldPause,
    RemasteredScreen::Map,
    RemasteredScreen::Startup,
    RemasteredScreen::KeyConfig,
    RemasteredScreen::Title,
    RemasteredScreen::Gameplay,
    RemasteredScreen::GameplaySub,
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn retail_tables(screen: RemasteredScreen) -> ScreenInputTables {
    let path = root().join("Infest.exe");
    let bytes = std::fs::read(&path).expect("retail Infest.exe is required");
    let source = FrontendExecutable::parse(&bytes).expect("mapped retail executable");
    match screen {
        RemasteredScreen::Frontend => parse_frontend_input_tables(source),
        RemasteredScreen::Briefing => parse_phase3_input_tables(source),
        RemasteredScreen::Results => parse_phase8_input_tables(source),
        RemasteredScreen::TextEntry => parse_text_entry_input_tables(source),
        RemasteredScreen::InWorldPause => parse_phase5_input_tables(source),
        RemasteredScreen::Map => parse_phase7_input_tables(source),
        RemasteredScreen::Startup => parse_startup_input_tables(source),
        RemasteredScreen::KeyConfig => parse_phase12_input_tables(source),
        RemasteredScreen::Title => parse_title_input_tables(source),
        RemasteredScreen::Gameplay => inf_formats::parse_screen_input_tables(
            source,
            inf_formats::GAMEPLAY_DIGITAL_TABLE_ADDRESSES,
            inf_formats::GAMEPLAY_ANALOG_TABLE_POINTERS,
        ),
        RemasteredScreen::GameplaySub => inf_formats::parse_screen_input_tables_direct(
            source,
            inf_formats::GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES,
            inf_formats::GAMEPLAY_SUB_ANALOG_TABLE_ADDRESSES,
            &crate::player_input_source::ANALOG_IDENTITIES,
        ),
    }
    .unwrap_or_else(|error| panic!("{screen:?}: {error}"))
}

fn built(screen: RemasteredScreen) -> (ScreenInputTables, ScreenInputTables) {
    let retail = retail_tables(screen);
    let tables =
        remastered_tables(screen, &retail).unwrap_or_else(|error| panic!("{screen:?}: {error}"));
    (retail, tables)
}

fn slot8(screen: RemasteredScreen) -> DigitalInputTable {
    built(screen).1.digital[NATIVE_TYPE]
        .clone()
        .unwrap_or_else(|| panic!("{screen:?} has no slot-8 digital table"))
}

fn callbacks(table: &DigitalInputTable, code: u8) -> Vec<u32> {
    table
        .records
        .iter()
        .filter(|record| record.required[0][0] == code)
        .map(|record| record.descriptor.callback_address)
        .collect()
}

fn identities(table: &DigitalInputTable, code: u8) -> Vec<u32> {
    table
        .records
        .iter()
        .filter(|record| record.required[0][0] == code)
        .map(|record| record.descriptor.identity)
        .collect()
}

fn only_callback(table: &DigitalInputTable, code: u8) -> u32 {
    let callbacks = callbacks(table, code);
    assert_eq!(
        callbacks.len(),
        1,
        "code {code} should have one row, got {callbacks:?}"
    );
    callbacks[0]
}

fn analog_signature(table: &AnalogInputTable) -> Vec<(u32, u32, u32, i32, i32, i32)> {
    table
        .records
        .iter()
        .map(|record| {
            (
                record.descriptor.identity,
                record.axis,
                record.flags,
                record.offset,
                record.scale_q31,
                record.multiplier,
            )
        })
        .collect()
}

fn assert_digital_provenance(table: &DigitalInputTable) {
    assert_ne!(table.virtual_address, 0);
    let mut next_address = table.virtual_address;
    let mut next_file = None;
    for record in &table.records {
        assert_eq!(record.virtual_address, next_address);
        assert_eq!(record.file_span.len(), DIGITAL_INPUT_RECORD_BYTES);
        if let Some(file) = next_file {
            assert_eq!(record.file_span.start, file);
        }
        next_file = Some(record.file_span.end);
        next_address += DIGITAL_INPUT_RECORD_BYTES as u32;
    }
    assert_eq!(table.terminator.start, next_address as usize);
    assert_eq!(table.terminator.len(), 4);
}

fn assert_analog_provenance(table: &AnalogInputTable) {
    assert_ne!(table.virtual_address, 0);
    let mut next_address = table.virtual_address;
    let mut next_file = None;
    for record in &table.records {
        assert_eq!(record.virtual_address, next_address);
        assert_eq!(record.file_span.len(), ANALOG_INPUT_RECORD_BYTES);
        if let Some(file) = next_file {
            assert_eq!(record.file_span.start, file);
        }
        next_file = Some(record.file_span.end);
        next_address += ANALOG_INPUT_RECORD_BYTES as u32;
    }
    assert_eq!(table.terminator.start, next_address as usize);
    assert_eq!(table.terminator.len(), 4);
}

#[test]
fn every_screen_fills_slot_eight_and_keeps_every_other_slot_retail() {
    for screen in ALL_SCREENS {
        let (retail, tables) = built(screen);
        for slot in 0..INPUT_DEVICE_SLOTS {
            if slot == NATIVE_TYPE {
                continue;
            }
            assert_eq!(
                tables.digital[slot], retail.digital[slot],
                "{screen:?} digital slot {slot}"
            );
            assert_eq!(
                tables.analog[slot], retail.analog[slot],
                "{screen:?} analog slot {slot}"
            );
        }
        let digital = tables.digital[NATIVE_TYPE]
            .as_ref()
            .unwrap_or_else(|| panic!("{screen:?} has no slot-8 digital table"));
        assert_digital_provenance(digital);
        if let Some(analog) = &tables.analog[NATIVE_TYPE] {
            assert_analog_provenance(analog);
        }
    }
}

#[test]
fn analog_rows_exist_only_where_retail_authored_them() {
    for screen in ALL_SCREENS {
        let (retail, tables) = built(screen);
        assert_eq!(
            retail.analog[9].is_some(),
            tables.analog[NATIVE_TYPE].is_some(),
            "{screen:?} analog presence"
        );
    }
    for screen in [
        RemasteredScreen::Frontend,
        RemasteredScreen::Briefing,
        RemasteredScreen::TextEntry,
        RemasteredScreen::Gameplay,
        RemasteredScreen::GameplaySub,
    ] {
        assert!(
            built(screen).1.analog[NATIVE_TYPE].is_some(),
            "{screen:?} has authored analog rows"
        );
    }
    // The pause and map constructors snapshot runtime cells, so the parser has
    // no analog tables for them; the task keeps that gap.
    for screen in [
        RemasteredScreen::Results,
        RemasteredScreen::InWorldPause,
        RemasteredScreen::Map,
        RemasteredScreen::Startup,
        RemasteredScreen::KeyConfig,
        RemasteredScreen::Title,
    ] {
        assert!(
            built(screen).1.analog[NATIVE_TYPE].is_none(),
            "{screen:?} has no retail analog rows"
        );
    }
}

#[test]
fn the_play_screen_binds_pause_first_person_and_map() {
    let table = slot8(RemasteredScreen::Gameplay);
    assert_eq!(table.records.len(), 3);
    assert_eq!(only_callback(&table, codes::START), 0x0044_3700);
    assert_eq!(only_callback(&table, codes::VIEW), 0x0044_3650);
    assert_eq!(only_callback(&table, codes::R3), 0x0044_36d0);
    let pause = table
        .records
        .iter()
        .find(|record| record.required[0][0] == codes::START)
        .unwrap();
    assert_eq!(pause.forbidden[0][0], codes::VIEW);
    // The screen's own analog table is retail's menu axes, re-coded to slot 8.
    let (retail, tables) = built(RemasteredScreen::Gameplay);
    assert_eq!(
        analog_signature(tables.analog[NATIVE_TYPE].as_ref().unwrap()),
        analog_signature(retail.analog[9].as_ref().unwrap())
    );
}

#[test]
fn the_play_sub_binds_the_agreed_buttons() {
    let table = slot8(RemasteredScreen::GameplaySub);
    assert_eq!(callbacks(&table, codes::A), [0, 0x0046_97f0]);
    assert_eq!(identities(&table, codes::A), [0x0053_a378, 0x0053_a468]);
    assert_eq!(only_callback(&table, codes::B), 0x0046_9580);
    assert_eq!(only_callback(&table, codes::Y), 0x0046_9670);
    assert_eq!(only_callback(&table, codes::X), 0x0046_9660);
    assert_eq!(only_callback(&table, codes::LB), 0);
    assert_eq!(identities(&table, codes::LB), [0x0053_a388]);
    assert_eq!(only_callback(&table, codes::RB), 0x0046_94d0);
    assert_eq!(only_callback(&table, codes::LEFT), 0x0046_96e0);
    assert_eq!(only_callback(&table, codes::RIGHT), 0x0046_9700);
    assert_eq!(only_callback(&table, codes::UP), 0x0046_98c0);
    assert_eq!(only_callback(&table, codes::DOWN), 0x0046_9740);
    assert_eq!(only_callback(&table, codes::L3), 0x0046_9450);
    // The triggers are analog only; cargo rotation is deliberately unbound.
    for code in [codes::LT, codes::RT, codes::VIEW, codes::R3, codes::START] {
        assert!(
            callbacks(&table, code).is_empty(),
            "play sub code {code} must stay unbound"
        );
    }
    for callback in [0x0046_9680, 0x0046_96a0] {
        assert!(
            !table
                .records
                .iter()
                .any(|record| record.descriptor.callback_address == callback),
            "cargo rotation {callback:#010x} must stay unbound"
        );
    }
}

#[test]
fn the_left_trigger_record_is_the_analog_thrust_axis() {
    let (retail, tables) = built(RemasteredScreen::GameplaySub);
    let thrust = tables.analog[NATIVE_TYPE]
        .as_ref()
        .unwrap()
        .records
        .iter()
        .find(|record| record.descriptor.identity == THRUST_ANALOG_DESCRIPTOR)
        .expect("the player's throttle descriptor");
    assert_eq!(thrust.axis, THRUST_AXIS);
    assert_eq!(thrust.multiplier, 1024);
    assert_eq!(thrust.offset, 0);
    assert_eq!(thrust.scale_q31, 0x4000_0000);
    assert_eq!(thrust.flags, 0);
    // Only the throttle axis and sign changed; pitch and turn keep retail's.
    let retail = analog_signature(retail.analog[9].as_ref().unwrap());
    let remastered = analog_signature(tables.analog[NATIVE_TYPE].as_ref().unwrap());
    assert_eq!(retail.len(), remastered.len());
    for (retail, remastered) in retail.iter().zip(&remastered) {
        if retail.0 == THRUST_ANALOG_DESCRIPTOR {
            assert_eq!((remastered.1, remastered.5), (THRUST_AXIS, 1024));
            assert_eq!(
                (retail.0, retail.2, retail.3, retail.4),
                (remastered.0, remastered.2, remastered.3, remastered.4)
            );
        } else {
            assert_eq!(retail, remastered);
        }
    }
}

#[test]
fn the_frontend_re_codes_confirm_back_and_adds_the_shoulders() {
    let table = slot8(RemasteredScreen::Frontend);
    assert_eq!(only_callback(&table, codes::A), 0x0044_5200);
    assert_eq!(only_callback(&table, codes::START), 0x0044_5200);
    assert_eq!(only_callback(&table, codes::B), 0x0044_51e0);
    assert_eq!(only_callback(&table, codes::UP), 0x0044_5220);
    assert_eq!(only_callback(&table, codes::DOWN), 0x0044_5240);
    assert_eq!(only_callback(&table, codes::LEFT), 0x0044_5260);
    assert_eq!(only_callback(&table, codes::RIGHT), 0x0044_5280);
    assert_eq!(only_callback(&table, codes::LB), 0x0044_5220);
    assert_eq!(only_callback(&table, codes::RB), 0x0044_5240);
    // Y was Triangle/back on the PS1 table; the Remastered back is B.
    assert!(callbacks(&table, codes::Y).is_empty());
}

#[test]
fn every_menu_keeps_its_retail_confirm_and_back() {
    let briefing = slot8(RemasteredScreen::Briefing);
    assert_eq!(only_callback(&briefing, codes::A), 0x0040_d670);
    assert_eq!(only_callback(&briefing, codes::B), 0x0040_d660);
    assert_eq!(only_callback(&briefing, codes::START), 0x0040_d660);
    assert_eq!(only_callback(&briefing, codes::UP), 0x0040_d680);
    assert_eq!(only_callback(&briefing, codes::DOWN), 0x0040_d690);
    assert_eq!(only_callback(&briefing, codes::LB), 0x0040_d680);
    assert_eq!(only_callback(&briefing, codes::RB), 0x0040_d690);

    let results = slot8(RemasteredScreen::Results);
    assert_eq!(callbacks(&results, codes::A), [0x0042_aa60, 0x0044_5200]);
    assert_eq!(
        callbacks(&results, codes::START),
        [0x0042_aa60, 0x0044_5200]
    );
    assert_eq!(only_callback(&results, codes::B), 0x0042_aa30);
    assert_eq!(only_callback(&results, codes::X), 0x0042_aaa0);
    assert_eq!(only_callback(&results, codes::UP), 0x0044_5220);
    assert_eq!(only_callback(&results, codes::DOWN), 0x0044_5240);
    assert_eq!(only_callback(&results, codes::LEFT), 0x0044_5260);
    assert_eq!(only_callback(&results, codes::RIGHT), 0x0044_5280);
    assert_eq!(only_callback(&results, codes::LB), 0x0044_5220);
    assert_eq!(only_callback(&results, codes::RB), 0x0044_5240);

    let text = slot8(RemasteredScreen::TextEntry);
    assert_eq!(only_callback(&text, codes::A), 0x0044_5200);
    assert_eq!(only_callback(&text, codes::START), 0x0045_ae90);
    assert_eq!(only_callback(&text, codes::B), 0x0044_51e0);
    assert_eq!(only_callback(&text, codes::X), 0x0045_aea0);
    assert_eq!(only_callback(&text, codes::UP), 0x0044_5220);
    assert_eq!(only_callback(&text, codes::DOWN), 0x0044_5240);
    assert_eq!(only_callback(&text, codes::LEFT), 0x0044_5260);
    assert_eq!(only_callback(&text, codes::RIGHT), 0x0044_5280);
    assert_eq!(only_callback(&text, codes::LB), 0x0044_5220);
    assert_eq!(only_callback(&text, codes::RB), 0x0044_5240);

    let pause = slot8(RemasteredScreen::InWorldPause);
    assert_eq!(only_callback(&pause, codes::A), 0x0044_3560);
    assert_eq!(only_callback(&pause, codes::B), 0x0044_3590);
    assert_eq!(only_callback(&pause, codes::START), 0x0044_3700);
    let close = pause
        .records
        .iter()
        .find(|record| record.required[0][0] == codes::START)
        .unwrap();
    assert_eq!(close.forbidden[0][0], codes::VIEW);
    assert_eq!(only_callback(&pause, codes::UP), 0x0044_34a0);
    assert_eq!(only_callback(&pause, codes::DOWN), 0x0044_34d0);
    assert_eq!(only_callback(&pause, codes::LEFT), 0x0044_3500);
    assert_eq!(only_callback(&pause, codes::RIGHT), 0x0044_3530);
    assert_eq!(only_callback(&pause, codes::LB), 0x0044_34a0);
    assert_eq!(only_callback(&pause, codes::RB), 0x0044_34d0);

    let map = slot8(RemasteredScreen::Map);
    for code in [codes::A, codes::B, codes::START, codes::VIEW, codes::R3] {
        assert_eq!(only_callback(&map, code), 0x0045_20a0);
    }
    assert_eq!(only_callback(&map, codes::UP), 0x0045_20c0);
    assert_eq!(only_callback(&map, codes::DOWN), 0x0045_20f0);
    assert_eq!(only_callback(&map, codes::LEFT), 0x0045_2120);
    assert_eq!(only_callback(&map, codes::RIGHT), 0x0045_2150);
    assert_eq!(only_callback(&map, codes::LB), 0x0045_2180);
    assert_eq!(only_callback(&map, codes::RB), 0x0045_21b0);

    let startup = slot8(RemasteredScreen::Startup);
    assert_eq!(only_callback(&startup, codes::A), 0x0043_27b0);
    assert_eq!(only_callback(&startup, codes::START), 0x0043_27b0);
    assert_eq!(only_callback(&startup, codes::LEFT), 0x0043_27c0);
    assert_eq!(only_callback(&startup, codes::RIGHT), 0x0043_27e0);
    assert_eq!(only_callback(&startup, codes::LB), 0x0043_27c0);
    assert_eq!(only_callback(&startup, codes::RB), 0x0043_27e0);
    assert!(callbacks(&startup, codes::B).is_empty());

    let keys = slot8(RemasteredScreen::KeyConfig);
    assert_eq!(only_callback(&keys, codes::A), 0x0044_5200);
    assert_eq!(only_callback(&keys, codes::B), 0x0044_51e0);
    assert_eq!(only_callback(&keys, codes::UP), 0x0044_5220);
    assert_eq!(only_callback(&keys, codes::DOWN), 0x0044_5240);
    assert_eq!(only_callback(&keys, codes::LEFT), 0x0044_5260);
    assert_eq!(only_callback(&keys, codes::RIGHT), 0x0044_5280);
    assert_eq!(only_callback(&keys, codes::LB), 0x0044_5220);
    assert_eq!(only_callback(&keys, codes::RB), 0x0044_5240);
}

#[test]
fn the_title_and_its_videos_skip_on_every_code() {
    let table = slot8(RemasteredScreen::Title);
    assert_eq!(table.records.len(), 16);
    for code in 1..=16 {
        let rows = table
            .records
            .iter()
            .filter(|record| record.required[0][0] == code)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1, "title code {code}");
        assert_eq!(rows[0].descriptor.callback_address, 0x0040_4870);
        assert_eq!(rows[0].descriptor.identity, 0x0052_e638);
        assert_eq!(rows[0].forbidden, [[0; 4]; 2]);
    }
    assert!(built(RemasteredScreen::Title).1.analog[NATIVE_TYPE].is_none());
}

#[test]
fn the_assets_keep_the_retail_type_nine_tables() {
    let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
    for (screen, installed) in [
        (RemasteredScreen::Frontend, assets.frontend_input_tables()),
        (RemasteredScreen::Briefing, assets.phase3_input_tables()),
        (RemasteredScreen::Results, assets.phase8_input_tables()),
        (
            RemasteredScreen::TextEntry,
            assets.text_entry_input_tables(),
        ),
        (RemasteredScreen::InWorldPause, assets.phase5_input_tables()),
        (RemasteredScreen::Map, assets.phase7_input_tables()),
        (RemasteredScreen::Startup, assets.startup_input_tables()),
        (RemasteredScreen::KeyConfig, assets.phase12_input_tables()),
        (RemasteredScreen::Title, assets.title_input_tables()),
        (RemasteredScreen::Gameplay, assets.gameplay_input_tables()),
        (
            RemasteredScreen::GameplaySub,
            assets.gameplay_sub_input_tables(),
        ),
    ] {
        let retail = retail_tables(screen);
        assert_eq!(installed.digital[9], retail.digital[9], "{screen:?} slot 9");
        assert_eq!(installed.analog[9], retail.analog[9], "{screen:?} slot 9");
        assert!(
            installed.digital[NATIVE_TYPE].is_some(),
            "{screen:?} has a Remastered table"
        );
    }
}

#[test]
fn a_type_eight_device_accepts_every_remastered_table() {
    let assets = FrontendAssets::open(&root()).expect("retail Infest.exe is required");
    let mut input = InputRuntime::default();
    let source = input.create_device(DeviceButtons::default()).unwrap();
    let device = FrontendDevice {
        native_type: NATIVE_TYPE as u32,
        source,
    };
    for (screen, tables) in [
        (RemasteredScreen::Frontend, assets.frontend_input_tables()),
        (RemasteredScreen::Briefing, assets.phase3_input_tables()),
        (RemasteredScreen::Results, assets.phase8_input_tables()),
        (
            RemasteredScreen::TextEntry,
            assets.text_entry_input_tables(),
        ),
        (RemasteredScreen::InWorldPause, assets.phase5_input_tables()),
        (RemasteredScreen::Map, assets.phase7_input_tables()),
        (RemasteredScreen::Startup, assets.startup_input_tables()),
        (RemasteredScreen::KeyConfig, assets.phase12_input_tables()),
        (RemasteredScreen::Title, assets.title_input_tables()),
        (RemasteredScreen::Gameplay, assets.gameplay_input_tables()),
        (
            RemasteredScreen::GameplaySub,
            assets.gameplay_sub_input_tables(),
        ),
    ] {
        let mut bindings = FrontendBindings::new(77, assets.screen_analog_descriptors());
        bindings
            .initialize(&mut input)
            .unwrap_or_else(|error| panic!("{screen:?}: {error}"));
        bindings
            .install(&mut input, tables, &[device])
            .unwrap_or_else(|error| panic!("{screen:?}: {error}"));
    }
}
