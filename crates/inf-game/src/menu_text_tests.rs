use super::*;
use crate::menu::{MenuDrawPlacement, MenuRowDescriptor};
use inf_formats::{ByteSpan, GlyphMetrics};
use std::cell::RefCell;

pub(super) struct Resources {
    pub(super) font: Font,
    text: Vec<u8>,
    pub(super) calls: RefCell<Vec<(&'static str, i32)>>,
}

impl Resources {
    pub(super) fn new(text: &[u8]) -> Self {
        Self {
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
            text: text.to_vec(),
            calls: RefCell::new(Vec::new()),
        }
    }
}
impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.calls.borrow_mut().push(("string", argument));
        Ok(&self.text)
    }
    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        self.calls.borrow_mut().push(("font", argument));
        Ok(&self.font)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        self.calls.borrow_mut().push(("style", argument));
        Ok(0x99112233)
    }
    fn sprite_dimensions(&self, argument: i32) -> Result<[u16; 2], Self::Error> {
        self.calls.borrow_mut().push(("sprite", argument));
        Ok([20, 34])
    }
}

#[derive(Default)]
pub(super) struct RecordingBackend {
    pub(super) quads: Vec<MenuGlyphQuad>,
    pub(super) fail_at: Option<usize>,
}
impl MenuTextHost for RecordingBackend {
    type Error = &'static str;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        if self.fail_at == Some(self.quads.len()) {
            return Err("backend refused submission");
        }
        self.quads.push(glyph);
        Ok(())
    }
}
pub(super) fn request(right: bool) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: crate::menu::MenuSource::AuthoredDefinition {
            address: 0x53c088,
            span: inf_formats::ByteSpan::new(0, 20),
        },
        layout_index: 0,
        item_index: 0,
        callback_address: if right { 0x4834f0 } else { 0x453a80 },
        arguments: [0xabcd0001, 0],
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
fn width_uses_original_bytes_and_omits_only_final_spacing() {
    let mut r = Resources::new(b"AB");
    assert_eq!(measure_text_width(&r.font, b"AB\0ignored").unwrap(), 11);
    r.font.maximum_byte = 1;
    r.font.metrics[usize::from(b'!')].advance = 999;
    assert_eq!(measure_text_width(&r.font, b"AB").unwrap(), 11);
    r.font.coordinate_divisor = -100;
    assert_eq!(measure_text_width(&r.font, b"AB").unwrap(), -11);
}

#[test]
fn right_alignment_retains_full_resource_argument_and_two_ordered_quad_passes() {
    let r = Resources::new(b"AB");
    let mut h = RecordingBackend::default();
    let mut q = request(true);
    let result = draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    assert_eq!(result.advance_from_origin, 99); // width100 + last spacing(-1)
    assert_eq!(q.descriptor.position_pixels, [10, 20]);
    assert_eq!(
        &r.calls.borrow()[..2],
        &[("string", 0xabcd0001u32 as i32), ("font", 0)]
    );
    assert_eq!(h.quads.len(), 4);
    assert_eq!(
        h.quads[0].corners,
        [[99, 11], [104, 11], [104, 20], [99, 20]]
    );
    assert_eq!(h.quads[0].pass, MenuGlyphPass::Base);
    assert_eq!(
        h.quads[0].color,
        MenuColor::from_word(INITIAL_GLYPH_BASE_WORD)
    );
    assert_eq!(h.quads[1].pass, MenuGlyphPass::Color);
    assert_eq!(
        h.quads[1].color,
        MenuColor {
            rgb: [200, 100, 50],
            high_byte: None
        }
    );
    assert_eq!(h.quads[1].context_origin_pixels, [320, 240]);
}

#[test]
fn shadow_draws_entire_string_before_foreground_and_preserves_word_high_byte() {
    let r = Resources::new(b"AB");
    let mut h = RecordingBackend::default();
    let mut q = request(false);
    q.descriptor.flags = 0;
    draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    assert_eq!(h.quads.len(), 8);
    assert_eq!(h.quads[0].corners[0], [11, 12]);
    assert_eq!(h.quads[3].color, MenuColor::from_word(0x99112233));
    assert_eq!(h.quads[4].corners[0], [10, 11]);
    assert_eq!(h.quads[5].color.high_byte, None);
    assert_eq!(
        r.calls
            .borrow()
            .iter()
            .filter(|(kind, _)| *kind == "style")
            .count(),
        1
    );
}

#[test]
fn word_lookahead_wraps_before_second_word_and_feedback_is_actual_line_height() {
    let r = Resources::new(b"A BB");
    let mut h = RecordingBackend::default();
    let mut q = request(false);
    q.descriptor.extent_pixels[0] = 13;
    let result = draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    assert_eq!(q.descriptor.position_pixels[1], 32);
    assert_eq!(result.advance_from_origin, 10);
    assert_eq!(h.quads.len(), 8); // lookahead emits no duplicate glyphs
    assert_eq!(h.quads[4].corners[0], [10, 23]);
    // The first word is drawn before lookahead begins, even when over width.
    let r = Resources::new(b"AAAA");
    let mut q = request(false);
    q.descriptor.extent_pixels[0] = 1;
    draw_menu_text_row(&mut q, &r, &mut RecordingBackend::default()).unwrap();
    assert_eq!(q.descriptor.position_pixels[1], 20);
}

#[test]
fn animated_measurement_commits_y_before_backend_failure_but_plain_draw_does_not() {
    for animated in [false, true] {
        let r = Resources::new(b"A BB");
        let mut q = request(false);
        q.descriptor.extent_pixels[0] = 13;
        if animated {
            q.descriptor.reveal = 0;
        }
        let mut h = RecordingBackend {
            fail_at: Some(1),
            ..Default::default()
        };
        assert_eq!(
            draw_menu_text_row(&mut q, &r, &mut h),
            Err(MenuTextError::Backend("backend refused submission"))
        );
        assert_eq!(h.quads.len(), 1);
        assert_eq!(
            q.descriptor.position_pixels[1],
            if animated { 32 } else { 20 }
        );
    }
}

#[test]
fn reveal_uses_original_table_and_right_label_half_second_delay() {
    for right in [false, true] {
        let r = Resources::new(b"AB");
        let mut q = request(right);
        q.descriptor.reveal = 0;
        q.descriptor.age_millis = if right { 525 } else { 25 };
        let mut h = RecordingBackend::default();
        draw_menu_text_row(&mut q, &r, &mut h).unwrap();
        assert_eq!(h.quads.len(), 2);
        assert_eq!(h.quads[1].color.rgb, [206, 103, 51]); // final table byte132
        q.descriptor.age_millis -= 25;
        let mut h = RecordingBackend::default();
        draw_menu_text_row(&mut q, &r, &mut h).unwrap();
        assert!(h.quads.is_empty());
        q.descriptor.flags |= 0x20; // outgoing row skips reveal entirely
        draw_menu_text_row(&mut q, &r, &mut h).unwrap();
        assert_eq!(h.quads.len(), 4);
    }
}

#[test]
fn reveal_modulation_preserves_known_whole_word_high_byte() {
    let r = Resources::new(b"A");
    let mut q = request(false);
    q.descriptor.color = MenuColor::from_word(0xab3264c8);
    q.descriptor.reveal = 0;
    q.descriptor.age_millis = 25;
    let mut h = RecordingBackend::default();
    draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    assert_eq!(
        h.quads[1].color,
        MenuColor {
            rgb: [206, 103, 51],
            high_byte: Some(0xab)
        }
    );
}

#[test]
fn center_flag_is_only_applied_in_animated_left_branch() {
    let r = Resources::new(b"AB");
    for animated in [false, true] {
        let mut q = request(false);
        q.descriptor.flags |= 2;
        if animated {
            q.descriptor.reveal = 1;
        }
        let mut h = RecordingBackend::default();
        draw_menu_text_row(&mut q, &r, &mut h).unwrap();
        assert_eq!(h.quads[0].corners[0][0], if animated { 5 } else { 10 });
    }
}

#[test]
fn disabled_glyph_callback_keeps_layout_and_shadow_resource_lookup() {
    let r = Resources::new(b"A BB");
    let mut q = request(false);
    q.descriptor.flags = 8;
    q.descriptor.extent_pixels[0] = 13;
    let mut h = RecordingBackend::default();
    draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    assert!(h.quads.is_empty());
    assert_eq!(q.descriptor.position_pixels[1], 32);
    assert!(r.calls.borrow().contains(&("style", 7)));
    assert!(!r.calls.borrow().iter().any(|(kind, _)| *kind == "sprite"));
}

#[test]
fn glyph_mapping_differs_from_width_and_rejects_bad_terminal_fallback() {
    let mut r = Resources::new(b"z");
    r.font.maximum_byte = 100;
    r.font.sprite_words[usize::from(b'!')] = 0;
    let mut h = RecordingBackend::default();
    draw_menu_text_row(&mut request(false), &r, &mut h).unwrap();
    assert_eq!(h.quads[0].sprite_argument, i32::from(b'?') + 1);
    r.text = b"A ".to_vec();
    r.font.sprite_words[0] = 0;
    assert_eq!(
        draw_menu_text_row(&mut request(false), &r, &mut h),
        Err(MenuTextError::Layout(MenuTextLayoutError::PastTerminator))
    );
}

#[test]
fn grid_mode_selects_fractional_plane_and_preserves_distinct_offset_division() {
    let mut r = Resources::new(b"AA");
    r.font.mode = FontMode::PositionGrid {
        columns: 2,
        rows: 2,
    };
    r.font.coordinate_divisor = 400;
    r.font.metrics.fill(GlyphMetrics {
        advance: 100,
        spacing: 0,
        offset: [-1, -1],
    });
    r.font.sprite_words = (1..=1024).collect();
    let (sprite, position) = glyph_position(&r.font, b'A', r.font.metrics[65], [600, 200]).unwrap();
    assert_eq!(sprite, 65 + 1 + 3 * 256);
    assert_eq!(position, [1, 0]); // pixel quotient + offset quotient, not summed/divided
    r.font.mode = FontMode::Direct;
    assert_eq!(
        glyph_position(&r.font, b'A', r.font.metrics[65], [400, 0])
            .unwrap()
            .1,
        [0, 0]
    );
    r.font.mode = FontMode::PositionGrid {
        columns: 2,
        rows: 2,
    };
    assert_eq!(
        glyph_position(&r.font, b'A', r.font.metrics[65], [400, 0])
            .unwrap()
            .1,
        [1, 0]
    );
}

#[test]
fn width_narrows_before_right_alignment_and_corners_keep_signed_word_wrap() {
    let mut r = Resources::new(b"AA");
    r.font.coordinate_divisor = 1;
    r.font.metrics.fill(GlyphMetrics {
        advance: 20000,
        spacing: 0,
        offset: [0, 0],
    });
    let mut q = request(true);
    q.descriptor.position_pixels = [32767, 20];
    let mut h = RecordingBackend::default();
    draw_menu_text_row(&mut q, &r, &mut h).unwrap();
    let start = 32767i32 + 100 - i32::from(40000u16 as i16);
    assert_eq!(h.quads[0].corners[0][0], start as i16);
    assert_eq!(h.quads[0].corners[1][0], start.wrapping_add(5) as i16);
}

#[test]
fn unsupported_callback_and_division_fault_are_explicit_without_fake_draw() {
    let mut r = Resources::new(b"A");
    let mut q = request(false);
    q.callback_address = 0x453ae0;
    let mut h = RecordingBackend::default();
    assert_eq!(
        draw_menu_text_row(&mut q, &r, &mut h),
        Err(MenuTextError::UnsupportedCallback(0x453ae0))
    );
    assert!(r.calls.borrow().is_empty());
    r.font.coordinate_divisor = 0;
    assert!(matches!(
        draw_menu_text_row(&mut request(false), &r, &mut h),
        Err(MenuTextError::Layout(MenuTextLayoutError::Division { .. }))
    ));
    assert!(h.quads.is_empty());
}

#[test]
fn original_executable_reveal_policy_matches_the_native_read_only_table() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        exe.mapped(REVEAL_TABLE_ADDRESS as u32, 17).unwrap().bytes,
        REVEAL_MULTIPLIERS
    );
    assert_eq!(exe.word(0x0054_15dc).unwrap(), REVEAL_STEP_MILLIS as u32);
}

#[test]
fn authored_root_and_options_labels_use_resident_fonts_strings_and_glyph_dimensions() {
    use crate::{
        frontend_assets::FrontendAssets, render_resources::RenderResources, Installation,
        ResidencyRequest, ResidentRuntime, ResourceSelection,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let menus = [
        assets.root_menu().clone(),
        assets.menu_definition(0x0053_9378).unwrap(),
    ];
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let mut observed = Vec::new();
    for difficulty in 0..5 {
        runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty,
                    language: 0,
                },
            })
            .unwrap();
        let resources = RenderResources::new(&runtime);
        let mut widths = Vec::new();
        let mut total = 0;
        for menu in &menus {
            for (index, item) in menu.items.iter().enumerate() {
                let right = match item.draw_callback {
                    0x0048_34f0 | 0x0048_35e0 => true,
                    0x0045_3a80 | 0x0045_f520 => false,
                    other => panic!("unexpected authored label callback {other:08X}"),
                };
                let mut q = request(right);
                q.menu_source = crate::menu::MenuSource::AuthoredDefinition {
                    address: menu.virtual_address,
                    span: menu.definition_span,
                };
                q.item_index = index;
                q.arguments = item.draw_arguments;
                q.descriptor.extent_pixels[0] = 300;
                q.descriptor.reveal = 0;
                q.descriptor.age_millis = 10_000;
                let mut h = RecordingBackend::default();
                draw_menu_text_row(&mut q, &resources, &mut h).unwrap();
                let text = resources.string(item.draw_arguments[0] as i32).unwrap();
                let font = resources.font(0).unwrap();
                widths.push(measure_text_width(font, text).unwrap());
                assert_eq!(h.quads.len(), text.len() * 2);
                for pair in h.quads.chunks_exact(2) {
                    assert_eq!(pair[0].corners, pair[1].corners);
                    let dimensions = resources
                        .sprite_dimensions(pair[0].sprite_argument)
                        .unwrap();
                    assert_eq!(
                        pair[0].corners[1][0].wrapping_sub(pair[0].corners[0][0]),
                        (dimensions[0] >> 2) as i16
                    );
                    assert_eq!(
                        pair[0].corners[2][1].wrapping_sub(pair[0].corners[1][1]),
                        ((u32::from(dimensions[1]) + 2) / 4) as i16
                    );
                }
                total += h.quads.len();
            }
        }
        observed.push((difficulty, widths, total));
    }
    assert_eq!(
        observed,
        [
            (0, vec![61, 72, 41, 72, 38, 37, 46, 47, 41, 91, 24], 198),
            (1, vec![97, 116, 66, 116, 60, 59, 74, 75, 66, 146, 38], 198),
            (2, vec![122, 145, 83, 145, 76, 74, 93, 94, 83, 183, 48], 198),
            (3, vec![122, 145, 83, 145, 76, 74, 93, 94, 83, 183, 48], 198),
            (4, vec![122, 145, 83, 145, 76, 74, 93, 94, 83, 183, 48], 198),
        ]
    );
}

#[test]
fn retail_options_label_wrap_uses_selected_font_line_terms_for_y_feedback() {
    use crate::{
        frontend_assets::FrontendAssets, render_resources::RenderResources, Installation,
        ResidencyRequest, ResidentRuntime, ResourceSelection,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let menu = assets.menu_definition(0x0053_9378).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let mut heights = Vec::new();
    for difficulty in 0..5 {
        runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty,
                    language: 0,
                },
            })
            .unwrap();
        let resources = RenderResources::new(&runtime);
        let item = menu
            .items
            .iter()
            .find(|item| {
                let text = resources.string(item.draw_arguments[0] as i32).unwrap();
                text.iter().filter(|&&b| b == b' ').count() == 1 && !text.contains(&b'-')
            })
            .expect("authored two-word Options label");
        assert!(matches!(item.draw_callback, 0x0045_3a80 | 0x0045_f520));
        let mut q = request(false);
        q.arguments = item.draw_arguments;
        q.descriptor.extent_pixels[0] = 0;
        q.descriptor.reveal = 0;
        q.descriptor.age_millis = 10_000;
        let mut backend = RecordingBackend::default();
        draw_menu_text_row(&mut q, &resources, &mut backend).unwrap();
        heights.push(q.descriptor.position_pixels[1]);
        assert!(!backend.quads.is_empty());
    }
    assert_eq!(heights, [39, 35, 39, 39, 39]); // originalY20 + 7693/divisor
}
