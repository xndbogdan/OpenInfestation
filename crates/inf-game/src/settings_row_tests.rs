//! 00453AE0/00453D50/00453B80 checks against synthetic H0 pools and a
//! recording glyph backend. Side selection, slider glyphs and the Sound
//! actions' adjust/tail arguments are also compared with receipts from the
//! unmodified PE callbacks (`tests/fixtures/settings_rows_native.json`).

use super::*;
use crate::frontend_audio::{expanded_volume, music_aux_level};
use crate::menu::{MenuColor, MenuDrawPlacement, MenuRowDescriptor, MenuSource};
use inf_formats::{ByteSpan, Font, FontMode, GlyphMetrics};
use serde_json::Value;
use std::collections::BTreeMap;

struct Resources {
    strings: BTreeMap<i32, Vec<u8>>,
    font: Font,
}

impl Resources {
    fn new(strings: &[(i32, &[u8])]) -> Self {
        Self {
            strings: strings
                .iter()
                .map(|(id, text)| (*id, text.to_vec()))
                .collect(),
            font: Font {
                local_index: 0,
                header_span: ByteSpan::new(0, 64),
                raw_header: [0; 16],
                mode: FontMode::Direct,
                coordinate_divisor: 100,
                line_step_terms: [1000, 200],
                maximum_byte: 255,
                fallback_byte: b'?',
                metric_span: ByteSpan::new(64, 2112),
                sprite_span: ByteSpan::new(2112, 3136),
                metrics: vec![
                    GlyphMetrics {
                        advance: 600,
                        spacing: -100,
                        offset: [0, 0]
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
        }
    }
}

impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.strings
            .get(&argument)
            .map(Vec::as_slice)
            .ok_or("missing string")
    }
    fn font(&self, _argument: i32) -> Result<&Font, Self::Error> {
        Ok(&self.font)
    }
    fn style(&self, _argument: i32) -> Result<u32, Self::Error> {
        Ok(0x99112233)
    }
    fn sprite_dimensions(&self, _argument: i32) -> Result<[u16; 2], Self::Error> {
        Ok([20, 34])
    }
}

impl SettingsRowResources for Resources {
    fn string_count(&self) -> Result<usize, Self::Error> {
        Ok(self
            .strings
            .keys()
            .max()
            .map(|max| *max as usize + 1)
            .unwrap_or(0))
    }
}

#[derive(Default)]
struct Backend {
    sprites: Vec<i32>,
}

impl MenuTextHost for Backend {
    type Error = &'static str;
    fn submit_glyph(&mut self, glyph: crate::menu_text::MenuGlyphQuad) -> Result<(), Self::Error> {
        self.sprites.push(glyph.sprite_argument);
        Ok(())
    }
}

fn request(base: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::AuthoredDefinition {
            address: 0x539088,
            span: ByteSpan::new(0, 20),
        },
        layout_index: 0,
        item_index: 1,
        callback_address: 0x0045_3ae0,
        arguments: [base, 0x54_e908],
        context_origin_pixels: [320, 240],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [10, 20],
            extent_pixels: [100, 200],
            reveal: u16::MAX,
            flags: 0x40,
            age_millis: 1000,
            color: MenuColor::from_rgb([200, 100, 50]),
        },
    }
}

#[test]
fn value_selection_wraps_in_16_bits_before_plus_one() {
    assert_eq!(resolve_value_id(0xAB, 0), 0xAC);
    assert_eq!(resolve_value_id(0xAB, 1), 0xAD);
    assert_eq!(resolve_value_id(0x10, 0xFFFF), 0x10);
    assert_eq!(resolve_value_id(0, 0xFFFF), 0);
    // Only the base's low WORD participates, matching `add cx,si`.
    assert_eq!(resolve_value_id(0x1_00AB, 0), 0xAC);
    assert_eq!(resolve_value_id(0xAB, 0x1_00AB), 0x157);
}

#[test]
fn adjust_commands_map_to_floor_clamp_and_wrap() {
    let mut value = 5;
    assert_eq!(
        adjust_value(MenuCommand::AdjustPrevious, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 4);
    assert_eq!(
        adjust_value(MenuCommand::AdjustPrevious, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    value = 0;
    assert_eq!(
        adjust_value(MenuCommand::AdjustPrevious, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 0);
    value = 5;
    assert_eq!(
        adjust_value(MenuCommand::AdjustNext, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 6);
    value = 7;
    assert_eq!(
        adjust_value(MenuCommand::AdjustNext, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 7);
    value = 7;
    assert_eq!(
        adjust_value(MenuCommand::Activate, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 0);
    value = 5;
    assert_eq!(
        adjust_value(MenuCommand::Activate, 7, &mut value),
        AdjustOutcome::Adjusted
    );
    assert_eq!(value, 6);
    for command in [
        MenuCommand::Previous,
        MenuCommand::Next,
        MenuCommand::Auxiliary6,
        MenuCommand::Auxiliary7,
    ] {
        value = 5;
        assert_eq!(adjust_value(command, 7, &mut value), AdjustOutcome::Ignored);
        assert_eq!(value, 5);
    }
}

#[test]
fn label_and_value_draw_in_order_through_the_shared_descriptor() {
    let resources = Resources::new(&[(0xAB, b"A"), (0xAC, b"B")]);
    let mut backend = Backend::default();
    let mut row = request(0xAB);
    draw_settings_row(&mut row, 0, &resources, &mut backend).unwrap();
    // One glyph per side with two ordered passes each; label quads precede
    // value quads. Sprite arguments follow the identity sprite table.
    assert_eq!(backend.sprites, [66, 66, 67, 67]);
}

#[test]
fn out_of_range_value_renders_the_label_alone() {
    let resources = Resources::new(&[(0xAB, b"A")]);
    let mut backend = Backend::default();
    let mut row = request(0xAB);
    // 0xAB + 500 + 1 = 0x2A0 exceeds the pool; the native null right side
    // draws nothing instead of failing.
    draw_settings_row(&mut row, 500, &resources, &mut backend).unwrap();
    assert_eq!(backend.sprites, [66, 66]);
}

#[test]
fn negative_label_renders_the_value_alone() {
    let resources = Resources::new(&[(0, b"V")]);
    let mut backend = Backend::default();
    let mut row = request(0xFFFF_FFFF);
    // Base -1 fails the label's low-word sign test while the wrapped value
    // index (0xFFFF + 0 + 1) & 0xFFFF selects H0 0.
    draw_settings_row(&mut row, 0, &resources, &mut backend).unwrap();
    assert_eq!(backend.sprites, [87, 87]);
}

#[test]
fn one_past_the_pool_is_an_explicit_error_on_either_side() {
    // Count derives from the highest authored string (0xAC).
    let resources = Resources::new(&[(0xAB, b"A"), (0xAC, b"B")]);
    assert_eq!(resources.string_count().unwrap(), 0xAD);
    let mut backend = Backend::default();
    // Value index 0xAC addresses exactly one past a 0xAC pool.
    let tight = Resources::new(&[(0xAB, b"A")]);
    assert_eq!(tight.string_count().unwrap(), 0xAC);
    let mut row = request(0xAB);
    assert!(matches!(
        draw_settings_row(&mut row, 0, &tight, &mut backend),
        Err(SettingsRowError::Side(RowSideError::PoolBoundary {
            index: 0xAC
        }))
    ));
    // Label base one past the pool fails the same way.
    let mut row = request(0xAC);
    assert!(matches!(
        draw_settings_row(&mut row, 0, &tight, &mut backend),
        Err(SettingsRowError::Side(RowSideError::PoolBoundary {
            index: 0xAC
        }))
    ));
}

#[test]
fn missing_label_strings_are_explicit_resource_errors() {
    let resources = Resources::new(&[(0xAC, b"B")]);
    let mut backend = Backend::default();
    let mut row = request(0xAB);
    assert!(matches!(
        draw_settings_row(&mut row, 0, &resources, &mut backend),
        Err(SettingsRowError::Resource(_))
    ));
    assert!(backend.sprites.is_empty());
}

#[test]
fn identity_row_draws_fixed_label_then_literal_text() {
    let resources = Resources::new(&[(0x7A, b"Net"), (0x7B, b"Name")]);
    let mut backend = Backend::default();
    let mut row = request(0x7A);
    draw_identity_row(&mut row, 0x7A, b"Pilot", &resources, &mut backend).unwrap();
    // Identity sprite arguments follow the byte value plus one per side, in
    // label-then-text order with two ordered passes each.
    assert_eq!(
        backend.sprites,
        [79, 79, 102, 102, 117, 117, 81, 81, 106, 106, 109, 109, 112, 112, 117, 117]
    );
}

#[test]
fn identity_row_draws_empty_text_without_glyphs() {
    let resources = Resources::new(&[(0x7B, b"Name")]);
    let mut backend = Backend::default();
    let mut row = request(0x7B);
    draw_identity_row(&mut row, 0x7B, b"", &resources, &mut backend).unwrap();
    // Only the four label glyphs; the empty right side submits nothing.
    assert_eq!(backend.sprites.len(), 8);
}

#[test]
fn identity_row_label_keeps_the_pool_bound_rule() {
    let tight = Resources::new(&[(0x79, b"Net")]);
    assert_eq!(tight.string_count().unwrap(), 0x7A);
    let mut backend = Backend::default();
    let mut row = request(0x7A);
    assert!(matches!(
        draw_identity_row(&mut row, 0x7A, b"Pilot", &tight, &mut backend),
        Err(SettingsRowError::Side(RowSideError::PoolBoundary {
            index: 0x7A
        }))
    ));
    assert!(backend.sprites.is_empty());
}

#[test]
fn display_variant_resolves_force_clamp_and_reinit_targets() {
    assert_eq!(resolve_display_reinit(0), (1, None));
    assert_eq!(resolve_display_reinit(0xFFFF_FFFF), (1, None));
    assert_eq!(resolve_display_reinit(1), (1, Some(1)));
    assert_eq!(resolve_display_reinit(4), (4, Some(4)));
    assert_eq!(resolve_display_reinit(5), (4, Some(4)));
    assert_eq!(resolve_display_reinit(0x8000_0000), (1, None));
}

#[derive(Default)]
struct Reinit {
    calls: Vec<u32>,
    fail: bool,
}

fn display_words() -> [u32; SETTINGS_WORDS] {
    let mut words = [0xA5A5_A5A5; SETTINGS_WORDS];
    words[3] = 2;
    words
}

fn reinit(
    host: &mut Reinit,
) -> impl FnMut(u32, &[u32; SETTINGS_WORDS]) -> Result<(), &'static str> + '_ {
    move |variant, _| {
        host.calls.push(variant);
        if host.fail {
            return Err("backend reset unconnected");
        }
        Ok(())
    }
}

#[test]
fn display_adjust_skips_variant_work_when_the_cell_survives() {
    let mut words = display_words();
    words[15] = 7;
    let mut host = Reinit::default();
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x3C,
            MenuCommand::Previous,
            9,
            reinit(&mut host)
        ),
        Ok(DisplayAdjustOutcome::Unchanged)
    );
    assert_eq!(words[15], 7);
    assert_eq!(words[3], 2);
    assert!(host.calls.is_empty());
}

#[test]
fn display_adjust_reinitializes_and_rolls_the_cell_back_on_failure() {
    let mut words = display_words();
    words[15] = 7;
    let mut host = Reinit::default();
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x3C,
            MenuCommand::AdjustNext,
            9,
            reinit(&mut host)
        ),
        Ok(DisplayAdjustOutcome::AdjustedReinitialized)
    );
    assert_eq!(words[15], 8);
    assert_eq!(host.calls, [2]);
    // A failed backend reset restores the row cell like the native setjmp
    // handler while the variant-word stores persist.
    host.fail = true;
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x3C,
            MenuCommand::AdjustNext,
            9,
            reinit(&mut host)
        ),
        Err(DisplayAdjustError::Host("backend reset unconnected"))
    );
    assert_eq!(words[15], 8);
    assert_eq!(words[3], 2);
}

#[test]
fn display_adjust_forces_variant_without_reinit_and_rejects_cells() {
    let mut words = display_words();
    words[3] = 0;
    words[15] = 7;
    let mut host = Reinit::default();
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x3C,
            MenuCommand::AdjustNext,
            9,
            reinit(&mut host)
        ),
        Ok(DisplayAdjustOutcome::AdjustedNoReinit)
    );
    assert_eq!(words[15], 8);
    assert_eq!(words[3], 1);
    assert!(host.calls.is_empty());
    // The variant cell aliases row and variant through one memory location.
    words[3] = 2;
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x0C,
            MenuCommand::AdjustNext,
            4,
            reinit(&mut host)
        ),
        Ok(DisplayAdjustOutcome::AdjustedReinitialized)
    );
    assert_eq!(words[3], 3);
    assert_eq!(host.calls, [3]);
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_0000,
            MenuCommand::AdjustNext,
            4,
            reinit(&mut host)
        ),
        Err(DisplayAdjustError::Cell {
            address: 0x0054_0000
        })
    );
    assert_eq!(
        adjust_display_setting(
            &mut words,
            0x0054_E8A8 + 0x01,
            MenuCommand::AdjustNext,
            4,
            reinit(&mut host)
        ),
        Err(DisplayAdjustError::Cell {
            address: 0x0054_E8A8 + 0x01
        })
    );
}

#[test]
fn two_null_sides_draw_nothing() {
    let resources = Resources::new(&[(0, b"V")]);
    let mut backend = Backend::default();
    // Base -2: label fails; value index (0xFFFE + 0 + 1) = 0xFFFF, whose
    // signed WORD is negative, so the value fails too.
    let mut row = request(0xFFFF_FFFE);
    draw_settings_row(&mut row, 0, &resources, &mut backend).unwrap();
    assert!(backend.sprites.is_empty());
}

#[test]
fn label_uses_only_the_base_low_word() {
    let resources = Resources::new(&[(0xAB, b"A"), (0xAC, b"B")]);
    let mut backend = Backend::default();
    // 00453AE0 tests and sign-extends SI, so 0x1_00AB still labels H0 0xAB.
    let mut row = request(0x1_00AB);
    draw_settings_row(&mut row, 0, &resources, &mut backend).unwrap();
    assert_eq!(backend.sprites, [66, 66, 67, 67]);
}

#[test]
fn slider_template_spans_the_high_word_width_before_its_fill() {
    let mut empty = vec![0x1C; 15];
    empty[0] = 0x1B;
    empty[14] = 0x1D;
    let mut one = empty.clone();
    one[0] = 0x1E;
    assert_eq!(volume_slider(0x000F_008C, 1).unwrap(), one);
    assert_eq!(volume_slider(0x000F_008C, 15).unwrap(), [0x1E; 15]);
    // Non-positive DWORDs leave the template untouched.
    assert_eq!(volume_slider(0x000F_008C, 0xFFFF_FFFF).unwrap(), empty);
    assert_eq!(volume_slider(0x000F_008C, 0x8000_0000).unwrap(), empty);
    // Widths of one or less (the arithmetic high word) collapse to `1D`.
    assert_eq!(volume_slider(0xFFFF_0000, 1).unwrap(), [0x1E]);
    assert_eq!(volume_slider(0x0001_0000, 0xFFFF_FFFF).unwrap(), [0x1D]);
    assert_eq!(
        volume_slider(0x0002_0000, 0x8000_0000).unwrap(),
        [0x1B, 0x1D]
    );
    assert_eq!(volume_slider(0x003F_0000, 63).unwrap(), [0x1E; 63]);
}

#[test]
fn slider_buffer_overruns_are_explicit_errors() {
    assert_eq!(
        volume_slider(0x000F_008C, 16),
        Err(RowSideError::SliderOverrun {
            value: 16,
            width: 15
        })
    );
    assert_eq!(
        volume_slider(0xFFFF_0000, 2),
        Err(RowSideError::SliderOverrun { value: 2, width: 1 })
    );
    assert_eq!(
        volume_slider(0x0040_0000, 1),
        Err(RowSideError::SliderWidth { width: 64 })
    );
}

#[test]
fn volume_row_zero_selects_the_next_string_and_others_draw_the_slider() {
    assert_eq!(
        volume_row_sides(0x000F_008C, 0, 0x200),
        Ok(RowSides {
            label: Some(0x8C),
            value: RowValueText::Pool(Some(0x8D)),
        })
    );
    // The zero string wraps in the low word: 0xFFFF + 1 selects H0 0.
    assert_eq!(
        volume_row_sides(0x0003_FFFF, 0, 0x200),
        Ok(RowSides {
            label: None,
            value: RowValueText::Pool(Some(0)),
        })
    );
    assert_eq!(
        volume_row_sides(0x0003_0005, 2, 0x200),
        Ok(RowSides {
            label: Some(5),
            value: RowValueText::Slider(vec![0x1E, 0x1E, 0x1D]),
        })
    );
    // Slider errors precede the label lookup, as 00453C90 builds first.
    assert_eq!(
        volume_row_sides(0x0003_0200, 4, 0x200),
        Err(RowSideError::SliderOverrun { value: 4, width: 3 })
    );
    assert_eq!(
        volume_row_sides(0x0003_0200, 3, 0x200),
        Err(RowSideError::PoolBoundary { index: 0x200 })
    );
}

#[test]
fn volume_row_draws_label_then_slider_glyphs() {
    let resources = Resources::new(&[(5, b"A")]);
    let mut backend = Backend::default();
    let mut row = request(0x0003_0005);
    row.callback_address = 0x0045_3D50;
    draw_volume_row(&mut row, 2, &resources, &mut backend).unwrap();
    assert_eq!(backend.sprites, [66, 66, 31, 31, 31, 31, 30, 30]);
    // A slider overrun submits nothing.
    let mut backend = Backend::default();
    assert!(matches!(
        draw_volume_row(&mut row, 4, &resources, &mut backend),
        Err(SettingsRowError::Side(RowSideError::SliderOverrun { .. }))
    ));
    assert!(backend.sprites.is_empty());
}

fn native_rows() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/settings_rows_native.json")).unwrap()
}

fn native_u32(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn native_text(value: &Value) -> Option<Vec<u8>> {
    value
        .as_array()
        .map(|bytes| bytes.iter().map(|b| b.as_u64().unwrap() as u8).collect())
}

#[test]
fn row_sides_match_the_original_value_and_slider_draws() {
    let fixture = native_rows();
    let count = fixture["pool_count"].as_u64().unwrap() as usize;
    let string = |id: i32| format!("L{id:04X}").into_bytes();
    let draws = fixture["draws"].as_array().unwrap();
    assert!(draws.len() > 500);
    for case in draws {
        let argument = native_u32(&case["argument"]);
        let value = native_u32(&case["value"]);
        let sides = match native_u32(&case["callback"]) {
            0x0045_3AE0 => settings_row_sides(argument, value, count),
            0x0045_3D50 => volume_row_sides(argument, value, count),
            other => panic!("unexpected callback {other:08X}"),
        }
        .unwrap_or_else(|error| panic!("{case}: {error}"));
        let events = case["events"].as_array().unwrap();
        assert_eq!(events.len(), 1, "{case}");
        let left = sides.label.map(string);
        let right = match sides.value {
            RowValueText::Pool(id) => id.map(string),
            RowValueText::Slider(glyphs) => Some(glyphs),
        };
        assert_eq!(left, native_text(&events[0]["left"]), "{case}");
        assert_eq!(right, native_text(&events[0]["right"]), "{case}");
    }
}

#[test]
fn sound_actions_match_the_original_adjust_and_audio_tail_arguments() {
    let fixture = native_rows();
    let actions = fixture["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 300);
    for case in actions {
        let mut words: Vec<u32> = case["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(native_u32)
            .collect();
        let after: Vec<u32> = case["after"]
            .as_array()
            .unwrap()
            .iter()
            .map(native_u32)
            .collect();
        let command = match native_u32(&case["command"]) {
            2 => MenuCommand::AdjustPrevious,
            3 => MenuCommand::AdjustNext,
            4 => MenuCommand::Activate,
            6 => MenuCommand::Auxiliary6,
            7 => MenuCommand::Auxiliary7,
            other => panic!("unexpected command {other}"),
        };
        let index = setting_word_index(native_u32(&case["cell"])).unwrap();
        adjust_value(command, native_u32(&case["max"]), &mut words[index]);
        assert_eq!(words, after, "{case}");
        let events = case["events"].as_array().unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        // Each tail reads its fixed word, whichever cell the row adjusted.
        match native_u32(&case["callback"]) {
            0x0046_3AC0 => {
                assert_eq!(kinds, ["fx-master", "check"], "{case}");
                assert_eq!(
                    native_u32(&events[0]["value"]),
                    expanded_volume(after[0]),
                    "{case}"
                );
            }
            0x0046_3B00 => {
                assert_eq!(kinds, ["music-resume", "aux"], "{case}");
                assert_eq!(
                    native_u32(&events[0]["owner"]),
                    native_u32(&fixture["owner"])
                );
                assert_eq!(native_u32(&events[0]["music"]), after[1], "{case}");
                assert_eq!(
                    native_u32(&events[1]["value"]),
                    music_aux_level(after[1]),
                    "{case}"
                );
            }
            other => panic!("unexpected callback {other:08X}"),
        }
    }
}
