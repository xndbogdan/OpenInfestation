//! Video callback checks against unmodified retail machine-code receipts.

use super::*;
use crate::{
    frontend_settings::SETTINGS_STATE_ADDRESS,
    menu::{MenuColor, MenuDrawPlacement, MenuRowDescriptor, MenuRowDrawRequest, MenuSource},
    menu_text::{MenuGlyphQuad, MenuTextHost, MenuTextResources},
    settings_row::{
        adjust_display_setting, draw_identity_row, DisplayAdjustError, SettingsRowResources,
    },
};
use inf_formats::{ByteSpan, Font, FontMode, GlyphMetrics};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/video_menu_native.json")).unwrap()
}

fn number(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}

fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| number(v) as u8)
        .collect()
}

fn command(value: &Value) -> MenuCommand {
    [
        MenuCommand::Previous,
        MenuCommand::Next,
        MenuCommand::AdjustPrevious,
        MenuCommand::AdjustNext,
        MenuCommand::Activate,
        MenuCommand::SelectLast,
        MenuCommand::Auxiliary6,
        MenuCommand::Auxiliary7,
    ][number(value) as usize]
}

fn event<'a>(case: &'a Value, name: &str) -> Option<&'a Value> {
    case["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["event"] == name)
}

fn devices() -> Vec<VideoDevice> {
    vec![
        VideoDevice {
            id: 100,
            name: b"Primary".to_vec(),
        },
        VideoDevice {
            id: 101,
            name: b"Adapter 2".to_vec(),
        },
    ]
}

struct Resources {
    font: Font,
    labels: Vec<Vec<u8>>,
}

impl Resources {
    fn new() -> Self {
        Self {
            labels: (0..256).map(|id| format!("H0/{id}").into_bytes()).collect(),
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
        self.labels
            .get(argument as usize)
            .map(Vec::as_slice)
            .ok_or("missing label")
    }
    fn font(&self, _: i32) -> Result<&Font, Self::Error> {
        Ok(&self.font)
    }
    fn style(&self, _: i32) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        Ok([20, 34])
    }
}

impl SettingsRowResources for Resources {
    fn string_count(&self) -> Result<usize, Self::Error> {
        Ok(self.labels.len())
    }
}

#[derive(Default)]
struct Glyphs(Vec<i32>);
impl MenuTextHost for Glyphs {
    type Error = &'static str;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        self.0.push(glyph.sprite_argument);
        Ok(())
    }
}

fn row(argument: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::AuthoredDefinition {
            address: 0x539088,
            span: ByteSpan::new(0, 20),
        },
        layout_index: 0,
        item_index: 2,
        callback_address: 0x462de0,
        arguments: [argument, VIDEO_DEVICE_CELL],
        context_origin_pixels: [320, 240],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [10, 20],
            extent_pixels: [300, 200],
            reveal: u16::MAX,
            flags: 0x40,
            age_millis: 1000,
            color: MenuColor::from_rgb([200, 100, 50]),
        },
    }
}

#[test]
fn native_device_names_and_label_resolution_reach_real_glyph_composition() {
    let native = fixture();
    let resources = Resources::new();
    for case in native["device_draw"].as_array().unwrap() {
        let mut list = vec![VideoDevice {
            id: 100,
            name: bytes(&case["name"]),
        }];
        if number(&case["label_argument"]) == 16 {
            list.push(VideoDevice {
                id: 101,
                name: b"B".to_vec(),
            });
        }
        let mut state = FrontendVideoState::new(list, None).unwrap();
        state.selected = number(&case["before"]);
        state.initialized = event(case, "enumerate").is_none();
        let name = state.device_name_for_draw().unwrap();
        assert_eq!(state.selected(), number(&case["after"]), "{case}");
        let native_row = event(case, "row").unwrap();
        assert_eq!(name, bytes(&native_row["right"]), "{case}");
        let mut request = row(number(&case["label_argument"]));
        let label = request.arguments[0] as i32;
        let mut glyphs = Glyphs::default();
        draw_identity_row(&mut request, label, &name, &resources, &mut glyphs).unwrap();
        let mut expected = Vec::new();
        for side in ["left", "right"] {
            if !native_row[side].is_null() {
                for byte in bytes(&native_row[side]) {
                    expected.extend([i32::from(byte) + 1; 2]);
                }
            }
        }
        assert_eq!(glyphs.0, expected, "{case}");
    }
}

#[test]
fn native_enumerator_keeps_first_four_adapter_identities_and_descriptions() {
    let native = fixture();
    let mut list = Vec::new();
    for case in native["enumeration"].as_array().unwrap() {
        list.push(VideoDevice {
            id: number(&case["input_guid"]) as i32,
            name: bytes(&case["input_description"]),
        });
        let state = FrontendVideoState::new(list.clone(), None).unwrap();
        assert_eq!(state.devices.len(), number(&case["count"]) as usize);
        assert_eq!(
            state
                .devices
                .iter()
                .map(|d| d.id as u32)
                .collect::<Vec<_>>(),
            case["guids"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            state
                .devices
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
            case["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(bytes)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn native_device_action_preserves_completed_tail_effects_after_caught_reset_errors() {
    let native = fixture();
    for case in native["device_action"].as_array().unwrap() {
        let mut state = FrontendVideoState::new(devices(), None).unwrap();
        state.selected = number(&case["before"]);
        state.device_name_for_draw().unwrap();
        let mut words = [0; SETTINGS_WORDS];
        words[3] = case.get("variant").map(number).unwrap_or(2);
        let before_words = words;
        let fail = case["fail"].as_bool().unwrap();
        let mut persisted = None;
        let mut reset = None;
        let result = state
            .adjust_device(
                VIDEO_DEVICE_CELL,
                command(&case["command"]),
                &words,
                |device, request| {
                    persisted = Some(device.name.clone());
                    reset = Some((request.variant, request.device));
                    assert_eq!(*request.words, before_words);
                    if fail {
                        Err(0x987)
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap();
        assert_eq!(result.is_err(), fail);
        assert_eq!(state.selected(), number(&case["selected"]), "{case}");
        assert_eq!(
            state.published_device() as u32,
            number(&case["guid"]),
            "{case}"
        );
        assert_eq!(
            persisted.unwrap(),
            bytes(&event(case, "persist-display").unwrap()["name"])
        );
        assert_eq!(
            reset.unwrap(),
            (
                number(&event(case, "reset").unwrap()["variant"]),
                number(&case["guid"]) as i32
            )
        );
        assert_eq!(event(case, "dispose-error").is_some(), fail);
        assert_eq!(event(case, "pop-error-scope").is_some(), !fail);
        assert_eq!(words, before_words);
    }
}

#[test]
fn native_display_adjust_matches_early_returns_variant_stores_and_row_rollback() {
    let native = fixture();
    for case in native["display_adjust"].as_array().unwrap() {
        let mut words = [0; SETTINGS_WORDS];
        let offset = number(&case["offset"]);
        words[3] = number(&case["variant"]);
        words[offset as usize / 4] = number(&case["before"]);
        let fail = case["fail"].as_bool().unwrap();
        let mut resets = Vec::new();
        let result = adjust_display_setting(
            &mut words,
            SETTINGS_STATE_ADDRESS + offset,
            command(&case["command"]),
            number(&case["maximum"]),
            |variant, current| {
                resets.push(variant);
                assert_eq!(current[3], variant);
                if fail {
                    Err(0x987)
                } else {
                    Ok(())
                }
            },
        );
        let expected_reset = event(case, "reset");
        assert_eq!(
            resets,
            expected_reset
                .map(|v| vec![number(&v["variant"])])
                .unwrap_or_default(),
            "{case}"
        );
        assert_eq!(
            matches!(result, Err(DisplayAdjustError::Host(0x987))),
            event(case, "dispose-error").is_some(),
            "{case}"
        );
        assert_eq!(words[offset as usize / 4], number(&case["after"]), "{case}");
        assert_eq!(words[3], number(&case["variant_after"]), "{case}");
    }
}

#[test]
fn portable_device_boundaries_reject_invalid_indices_without_hiding_native_clamp() {
    assert!(matches!(
        FrontendVideoState::new(Vec::new(), None),
        Err(VideoError::NoDevices)
    ));
    let mut state = FrontendVideoState::new(devices(), None).unwrap();
    let words = [0; SETTINGS_WORDS];
    assert_eq!(
        state.adjust_device(
            VIDEO_DEVICE_CELL,
            MenuCommand::Activate,
            &words,
            |_, _| Ok::<_, ()>(())
        ),
        Err(VideoError::Uninitialized)
    );
    assert_eq!(
        state.adjust_device(0, MenuCommand::Activate, &words, |_, _| Ok::<_, ()>(())),
        Err(VideoError::Cell(0))
    );
    state.selected = u32::MAX;
    assert_eq!(
        state.device_name_for_draw(),
        Err(VideoError::Selection(u32::MAX))
    );
    // Negative selections do not satisfy the native signed upper clamp.
    assert!(state.initialized);
    state.selected = 500;
    assert_eq!(
        state.device_name_for_draw(),
        Err(VideoError::Selection(500))
    );
    // The clamp runs only on first draw, not on every name lookup.
    state.initialized = false;
    assert_eq!(state.device_name_for_draw().unwrap(), b"Adapter 2");
    assert_eq!(state.selected(), 1);
    assert_eq!(device_row_name(b"A\0ignored"), b"A");
}

#[test]
fn saved_output_name_chooses_the_matching_portable_identity_without_reordering() {
    let state = FrontendVideoState::new(devices(), Some(b"Adapter 2")).unwrap();
    assert_eq!(state.selected(), 1);
    assert_eq!(state.published_device(), 101);
    let state = FrontendVideoState::new(devices(), Some(b"Unavailable")).unwrap();
    assert_eq!(state.selected(), 0);
    assert_eq!(state.published_device(), 100);

    let state = FrontendVideoState::new(devices(), Some(b"Adapter 2\0ignored")).unwrap();
    assert_eq!(state.selected(), 1);
    assert_eq!(state.published_device(), 101);
    for preferred in [b"".as_slice(), b"\0ignored".as_slice()] {
        let mut list = devices();
        list[1].name.clear();
        // 004631E0 skips the saved-name enumeration when its first byte is NUL.
        let state = FrontendVideoState::new(list, Some(preferred)).unwrap();
        assert_eq!(state.selected(), 0);
        assert_eq!(state.published_device(), 100);
    }
}

#[test]
fn startup_retains_last_matching_output_then_first_draw_clamps_only_the_row() {
    let mut list: Vec<_> = (0..6)
        .map(|id| VideoDevice {
            id: 100 + id,
            name: format!("Output {id}").into_bytes(),
        })
        .collect();
    list[1].name = b"Repeated".to_vec();
    list[4].name = b"Repeated".to_vec();
    let mut state = FrontendVideoState::new(list, Some(b"Repeated")).unwrap();
    // 00463320 keeps enumerating after a match; its selected ordinal and GUID
    // come from the last match, before the row's separate four-entry cap.
    assert_eq!(state.selected(), 4);
    assert_eq!(state.published_device(), 104);
    assert_eq!(state.devices.len(), 4);
    assert_eq!(state.device_name_for_draw().unwrap(), b"Output 3");
    assert_eq!(state.selected(), 3);
    assert_eq!(state.published_device(), 104);
    // Even an ignored command then publishes the clamped row's device.
    let words = [0; SETTINGS_WORDS];
    state
        .adjust_device(
            VIDEO_DEVICE_CELL,
            MenuCommand::Previous,
            &words,
            |device, reset| {
                assert_eq!(device.id, 103);
                assert_eq!(reset.device, 103);
                Ok::<_, ()>(())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(state.published_device(), 103);
}

#[test]
fn startup_renderer_repair_tests_the_full_dword_before_display_byte_narrowing() {
    for (input, expected) in [
        (0, 1),
        (1, 1),
        (0x100, 0x100),
        (0x101, 0x101),
        (u32::MAX, u32::MAX),
    ] {
        let mut words = std::array::from_fn(|index| 0x1234_0000 + index as u32);
        words[0x38 / 4] = input;
        let old = words;
        prepare_video_startup(&mut words);
        assert_eq!(words[0x38 / 4], expected);
        for index in 0..SETTINGS_WORDS {
            if index != 0x38 / 4 {
                assert_eq!(words[index], old[index]);
            }
        }
    }
}

#[test]
fn pipeline_uses_bilinear_low_byte_without_applying_world_depth_or_colour_depth_words() {
    use crate::{
        frontend_display::frontend_startup, Installation, ResidencyRequest, ResidentRuntime,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let baseline = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([320, 240]),
    )
    .unwrap()
    .scene;
    for (bilinear, expected_filter) in [(0, 1), (1, 2), (0x100, 1), (0x102, 2), (u32::MAX, 2)] {
        for depth in [0, 1, 2, u32::MAX] {
            let mut scene = baseline;
            let mut words = [0; SETTINGS_WORDS];
            words[0x38 / 4] = 0x101; // Renderer byte1, despite nonzero upper bytes.
            words[0x3c / 4] = bilinear;
            words[0x44 / 4] = depth;
            words[0x50 / 4] = depth;
            apply_video_pipeline(&words, &mut scene).unwrap();
            assert_eq!(scene.texture_state.sampling.min_filter, expected_filter);
            assert_eq!(scene.texture_state.sampling.mag_filter, expected_filter);
            assert_eq!(scene.renderer, baseline.renderer);
            assert_eq!(
                scene.pipeline.depth_enabled,
                baseline.pipeline.depth_enabled
            );
            assert_eq!(scene.pipeline.depth_write, baseline.pipeline.depth_write);
            assert_eq!(
                scene.pipeline.depth_function,
                baseline.pipeline.depth_function
            );
            assert_eq!(
                scene.texture_state.upload.source_rgb_shifts,
                baseline.texture_state.upload.source_rgb_shifts
            );
            assert_eq!(
                scene.texture_state.upload.global_policy,
                baseline.texture_state.upload.global_policy
            );
        }
    }
    let mut words = [0; SETTINGS_WORDS];
    words[0x38 / 4] = 0x100; // Startup does not repair this nonzero DWORD.
    prepare_video_startup(&mut words);
    let mut scene = baseline;
    assert_eq!(
        apply_video_pipeline(&words, &mut scene),
        Err(VideoError::Renderer(0))
    );
    assert_eq!(
        scene.texture_state.sampling.min_filter,
        baseline.texture_state.sampling.min_filter
    );
    assert_eq!(
        scene.texture_state.sampling.mag_filter,
        baseline.texture_state.sampling.mag_filter
    );
}

#[test]
fn authored_display_menu_keeps_its_actual_labels_cells_and_callback_policies() {
    use crate::{
        frontend_assets::FrontendAssets, render_resources::RenderResources, Installation,
        ResidencyRequest, ResidentRuntime,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let menu = assets.menu_definition(0x539088).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resources = RenderResources::new(&runtime);
    let expected = [
        ("Display", 0x45f520, 0, 0, 0),
        (
            "Targetter",
            0x453ae0,
            0x453b80,
            1,
            SETTINGS_STATE_ADDRESS + 0x60,
        ),
        ("Rendering", 0x462de0, 0x463000, 1, VIDEO_DEVICE_CELL),
        (
            "Resolution",
            0x453ae0,
            0x462f40,
            4,
            SETTINGS_STATE_ADDRESS + 0x0c,
        ),
        (
            "Colour Depth",
            0x453ae0,
            0x462f40,
            2,
            SETTINGS_STATE_ADDRESS + 0x44,
        ),
        (
            "Bilinear Filtering",
            0x453ae0,
            0x462f40,
            1,
            SETTINGS_STATE_ADDRESS + 0x3c,
        ),
        (
            "Z Buffering",
            0x453ae0,
            0x453b80,
            1,
            SETTINGS_STATE_ADDRESS + 0x50,
        ),
        (
            "Display",
            0x453ae0,
            0x462f40,
            1,
            SETTINGS_STATE_ADDRESS + 0x40,
        ),
        (
            "Detail Level",
            0x453ae0,
            0x463b50,
            1,
            SETTINGS_STATE_ADDRESS + 0x70,
        ),
        (
            "First Person",
            0x453ae0,
            0x463b50,
            1,
            SETTINGS_STATE_ADDRESS + 0x68,
        ),
        (
            "Camera Style",
            0x453ae0,
            0x463b50,
            1,
            SETTINGS_STATE_ADDRESS + 0x6c,
        ),
        (
            "Dynamic Lighting",
            0x453ae0,
            0x463b50,
            1,
            SETTINGS_STATE_ADDRESS + 0x80,
        ),
        (
            "Pause On Hints",
            0x453ae0,
            0x453b80,
            1,
            SETTINGS_STATE_ADDRESS + 0x78,
        ),
        ("Back", 0x453a80, 0x453c50, 0, 0),
    ];
    assert_eq!(menu.items.len(), expected.len());
    for (row, (label, draw, action, maximum, cell)) in menu.items.iter().zip(expected) {
        assert_eq!(
            resources.string(row.draw_arguments[0] as i32).unwrap(),
            label.as_bytes()
        );
        assert_eq!(row.draw_callback, draw, "{label}");
        assert_eq!(row.action_callback, action, "{label}");
        assert_eq!(row.action_arguments, [maximum, cell], "{label}");
        if cell != 0 {
            assert_eq!(row.draw_arguments[1], cell, "{label}");
        }
    }
}

#[test]
fn filtering_names_its_levels_and_anisotropy_per_renderer() {
    use crate::presentation::RendererBackend;
    let english: Vec<_> = (0..=6)
        .map(|level| String::from_utf8(filtering_text(level, 0)).unwrap())
        .collect();
    assert_eq!(
        english,
        [
            "Point",
            "Bilinear",
            "Trilinear",
            "Anisotropic 2x",
            "Anisotropic 4x",
            "Anisotropic 8x",
            "Anisotropic 16x"
        ]
    );
    assert_eq!(filtering_text(3, 3), b"Anisotropico 2x");
    assert_eq!(
        (0..=6).map(filtering_anisotropy).collect::<Vec<_>>(),
        [1, 1, 1, 2, 4, 8, 16]
    );
    assert_eq!(filtering_most(RendererBackend::Vulkan), 6);
    assert_eq!(filtering_most(RendererBackend::OpenGl), 1);
    assert_eq!(filtering_most(RendererBackend::Software), 1);
}

/// Lighting moves traced or not (the raster grades its world too), and
/// names its preset in each language.
#[test]
fn the_lighting_row_moves_traced_or_not() {
    use crate::menu::MenuCommand;
    use crate::modern_settings::{Lighting, ModernSettings};
    let support = ModernSupport::default();
    let mut modern = ModernSettings::default();
    assert_eq!(ModernRow::Lighting.label(1), b"Eclairage");
    assert_eq!(
        ModernRow::Lighting.value_text(&modern, support, 0),
        b"Remaster"
    );
    assert!(ModernRow::Lighting.adjust(&mut modern, support, MenuCommand::AdjustNext));
    assert_eq!(modern.lighting, Lighting::EyeCandy);
    assert_eq!(
        ModernRow::Lighting.value_text(&modern, support, 2),
        b"Augenschmaus"
    );
    modern.ray_tracing = false;
    assert!(ModernRow::Lighting.adjust(&mut modern, support, MenuCommand::AdjustPrevious));
    assert_eq!(modern.lighting, Lighting::Remaster);
    // A GPU without ray tracing moves it too.
    modern.ray_tracing = true;
    let untraced = ModernSupport {
        ray_tracing: false,
        ..support
    };
    assert!(ModernRow::Lighting.adjust(&mut modern, untraced, MenuCommand::AdjustNext));
    assert_eq!(modern.lighting, Lighting::EyeCandy);
}

#[test]
fn modern_rows_move_through_their_values_and_name_them() {
    use crate::menu::MenuCommand;
    use crate::modern_settings::{AntiAliasing, ModernSettings, UpscaleQuality, Upscaler};
    let mut modern = ModernSettings::default();
    for row in ModernRow::ALL {
        assert_eq!(ModernRow::from_address(row.address()), Some(row));
    }
    assert_eq!(ModernRow::from_address(MODERN_CELLS - 4), None);
    // Sideways stops at the ends; Return wraps.
    assert!(ModernRow::Upscaler.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustNext
    ));
    assert_eq!(modern.upscaler, Upscaler::Fsr);
    assert!(!ModernRow::RayTracing.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustNext
    ));
    for _ in 0..6 {
        ModernRow::Upscaling.adjust(
            &mut modern,
            ModernSupport::default(),
            MenuCommand::AdjustNext,
        );
    }
    assert_eq!(modern.quality, UpscaleQuality::UltraPerformance);
    assert!(ModernRow::RayTracing.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::Activate
    ));
    assert!(!modern.ray_tracing);
    // Without ray tracing the upscaler still brings the world up from its
    // quality's size, and the row moves.
    assert!(ModernRow::Upscaling.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustPrevious
    ));
    assert_eq!(modern.quality, UpscaleQuality::Performance);
    // Ray Reconstruction, with no traced light to denoise, is not offered.
    modern.upscaler = Upscaler::Dlss;
    assert!(ModernRow::Upscaler.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustNext
    ));
    assert_eq!(modern.upscaler, Upscaler::Xess);
    modern.upscaler = Upscaler::Fsr;
    // Under an upscaler, Anti-aliasing names it and does not move.
    assert!(!ModernRow::AntiAliasing.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustNext
    ));
    assert_eq!(
        ModernRow::AntiAliasing.value_text(&modern, ModernSupport::default(), 0),
        b"FSR"
    );
    modern.upscaler = Upscaler::Off;
    // The walk starts at Off; a fresh install defaults to TAA.
    modern.anti_aliasing = AntiAliasing::Off;
    assert!(ModernRow::AntiAliasing.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustNext
    ));
    assert_eq!(modern.anti_aliasing, AntiAliasing::Fxaa);
    assert_eq!(
        ModernRow::AntiAliasing.value_text(&modern, ModernSupport::default(), 0),
        b"FXAA"
    );
    // Retail's own Off in French, and the port's text.
    assert_eq!(
        ModernRow::RayTracing.value_text(&modern, ModernSupport::default(), 1),
        b"Non"
    );
    assert_eq!(ModernRow::Upscaling.label(1), b"Qualit\xe9 d'\xe9chelle");
    // With no upscaler there is nothing to scale: Upscaling shows Off and
    // does not move, traced or not, and the saved quality stays.
    modern.ray_tracing = true;
    assert_eq!(modern.upscaler, Upscaler::Off);
    assert_eq!(
        ModernRow::Upscaling.value_text(&modern, ModernSupport::default(), 0),
        b"Off"
    );
    assert!(!ModernRow::Upscaling.adjust(
        &mut modern,
        ModernSupport::default(),
        MenuCommand::AdjustPrevious
    ));
    assert_eq!(modern.quality, UpscaleQuality::Performance);
}

/// On a card without DLSS a saved DLSS shows as FSR, the upscaler it runs
/// as, and the Upscaler row steps past DLSS to XeSS; without ray tracing
/// the row says so and does not move.
#[test]
fn modern_rows_show_and_move_what_the_device_runs() {
    use crate::menu::MenuCommand;
    use crate::modern_settings::{ModernSettings, Upscaler};
    let amd = ModernSupport {
        ray_tracing: true,
        upscalers: [true, true, false, false, true],
        modern_sky: true,
    };
    let mut modern = ModernSettings {
        upscaler: Upscaler::Dlss,
        ..ModernSettings::default()
    };
    assert_eq!(ModernRow::Upscaler.value_text(&modern, amd, 0), b"FSR");
    assert_eq!(ModernRow::AntiAliasing.value_text(&modern, amd, 0), b"FSR");
    assert!(ModernRow::Upscaler.adjust(&mut modern, amd, MenuCommand::AdjustNext));
    assert_eq!(modern.upscaler, Upscaler::Xess);
    assert!(!ModernRow::Upscaler.adjust(&mut modern, amd, MenuCommand::AdjustNext));
    let bare = ModernSupport {
        ray_tracing: false,
        upscalers: [true, false, false, false, false],
        modern_sky: true,
    };
    assert_eq!(
        ModernRow::RayTracing.value_text(&modern, bare, 2),
        b"Nicht verf\xfcgbar"
    );
    assert!(!ModernRow::RayTracing.adjust(&mut modern, bare, MenuCommand::Activate));
    assert!(modern.ray_tracing);
    assert_eq!(ModernRow::Upscaler.value_text(&modern, bare, 0), b"Off");
    assert!(!ModernRow::Upscaler.adjust(&mut modern, bare, MenuCommand::AdjustNext));
}

/// While playing, the Vulkan rows and Filtering move at once; the rows
/// that need a display reset wait for the frontend.
#[test]
fn playing_display_rows_take_the_rows_that_need_no_reset() {
    use crate::frontend_settings::SETTINGS_STATE_ADDRESS;
    for row in ModernRow::ALL {
        assert_eq!(
            playing_display_row(row.address()),
            PlayingDisplayRow::Modern(row)
        );
    }
    assert_eq!(
        playing_display_row(SETTINGS_STATE_ADDRESS + 0x3c),
        PlayingDisplayRow::Filtering
    );
    for cell in [0x0c, 0x40, 0x44] {
        assert_eq!(
            playing_display_row(SETTINGS_STATE_ADDRESS + cell),
            PlayingDisplayRow::Frontend
        );
    }
}

#[test]
fn the_vulkan_video_menu_scrolls_its_added_rows_before_back() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let mut menu = assets.menu_definition(VIDEO_MENU).unwrap();
    let retail = menu.clone();
    append_modern_rows(&mut menu);
    let rows = ModernRow::ALL.len();
    assert_eq!(menu.items.len(), retail.items.len() + rows);
    let added: Vec<_> = menu.items[retail.items.len() - 1..retail.items.len() + rows - 1]
        .iter()
        .map(|row| ModernRow::from_address(row.draw_arguments[1]))
        .collect();
    assert_eq!(added, ModernRow::ALL.map(Some));
    assert_eq!(menu.items.last(), retail.items.last());
    let layout = menu.layouts.last().unwrap();
    let before = retail.layouts.last().unwrap();
    assert_eq!(layout.item_count, before.item_count + rows as u8);
    assert_eq!(layout.visible_rows, before.visible_rows);
}

/// Under Vulkan the in-world Options' Video menu (005390A0) carries the rows
/// too. It binds the frontend's rows (00538EC8) under the in-world chain
/// (00538D58), which the in-world Options and their Sounds and Controls
/// share, so it lays its rows out on a chain of its own and theirs keeps
/// its thirteen; the Sound menus, with their Ambience row, have their own
/// too. Either menu may open first.
#[test]
fn both_video_menus_carry_the_vulkan_rows() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let options = 0x0053_9390;
    for order in [
        [VIDEO_MENU, options, IN_WORLD_VIDEO_MENU],
        [options, IN_WORLD_VIDEO_MENU, VIDEO_MENU],
    ] {
        let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
            crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
        );
        catalog.set_modern_rows(true);
        for address in order {
            catalog.menu_data(address).unwrap();
        }
        for address in [VIDEO_MENU, IN_WORLD_VIDEO_MENU] {
            let menu = catalog.menu_data(address).unwrap();
            let records = menu.records();
            let rows: Vec<_> = records
                .items
                .iter()
                .filter_map(|row| ModernRow::from_address(row.draw_arguments[1]))
                .collect();
            assert_eq!(rows, ModernRow::ALL);
            let counted: usize = records
                .layouts
                .iter()
                .map(|layout| usize::from(layout.item_count))
                .sum();
            assert_eq!(counted, records.items.len());
        }
        let menu = catalog.menu_data(options).unwrap();
        assert_eq!(menu.layout_source().unwrap().address, 0x0053_8d58);
        assert_eq!(menu.records().layouts[1].item_count, 13);
        // Both Sound menus carry the Ambience and Headphones rows, on a
        // chain of their own.
        for address in [
            crate::frontend_sound::SOUND_MENU,
            crate::frontend_sound::IN_WORLD_SOUND_MENU,
        ] {
            let menu = catalog.menu_data(address).unwrap();
            let records = menu.records();
            for cell in [
                crate::frontend_sound::AMBIENCE_CELL,
                crate::frontend_sound::HEADPHONES_CELL,
            ] {
                let rows = records
                    .items
                    .iter()
                    .filter(|row| row.draw_arguments[1] == cell)
                    .count();
                assert_eq!(rows, 1);
            }
            let counted: usize = records
                .layouts
                .iter()
                .map(|layout| usize::from(layout.item_count))
                .sum();
            assert!(counted >= records.items.len());
        }
    }
}

/// Without Vulkan the Video and Sound menus carry retail's rows only: the
/// port adds nothing, in the frontend's menus or the in-world ones.
#[test]
fn menus_without_vulkan_carry_retails_rows_only() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    for address in [
        VIDEO_MENU,
        IN_WORLD_VIDEO_MENU,
        crate::frontend_sound::SOUND_MENU,
        crate::frontend_sound::IN_WORLD_SOUND_MENU,
    ] {
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let retail = assets.menu_definition(address).unwrap();
        let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
            crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
        );
        let menu = catalog.menu_data(address).unwrap();
        let records = menu.records();
        assert_eq!(records.items.len(), retail.items.len());
        assert!(records
            .items
            .iter()
            .all(|row| ModernRow::from_address(row.draw_arguments[1]).is_none()));
        assert!(records
            .items
            .iter()
            .all(|row| row.draw_arguments[1] != crate::frontend_sound::AMBIENCE_CELL));
        assert!(records
            .items
            .iter()
            .all(|row| row.draw_arguments[1] != crate::frontend_sound::HEADPHONES_CELL));
    }
}

/// The appended Vulkan rows sit before Back, so the authored initial
/// selection still lands on a retail row that never locks.
#[test]
fn appended_rows_keep_the_initial_selection_on_a_retail_row() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
        crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
    );
    catalog.set_modern_rows(true);
    for address in [VIDEO_MENU, IN_WORLD_VIDEO_MENU] {
        let menu = catalog.menu_data(address).unwrap();
        let records = menu.records();
        let modern = records.items.len() as i32 - 7;
        assert!((0..modern).contains(&records.initial_selection));
    }
}

/// A locked modern row neither moves nor takes the cursor: the row's
/// `most` of 0 locks both its adjust and its navigation bit, and its text
/// draws at half brightness.
#[test]
fn locked_modern_rows_skip_and_dim_and_unlock_again() {
    use crate::modern_settings::{ModernSettings, ModernSupport, Upscaler};
    use inf_formats::FrontendMenuItem;
    let support = ModernSupport::default();
    let template = FrontendMenuItem {
        virtual_address: 0,
        span: ByteSpan { start: 0, end: 0 },
        flags: 1,
        draw_callback: 0,
        draw_arguments: [0, 0],
        action_callback: 0,
        action_arguments: [0, 0],
        row_height: 1,
        age: 0,
    };
    let mut items = ModernRow::ALL
        .into_iter()
        .map(|row| FrontendMenuItem {
            draw_arguments: [0, row.address()],
            ..template
        })
        .collect::<Vec<_>>();
    items.push(FrontendMenuItem {
        draw_arguments: [0, SETTINGS_STATE_ADDRESS + 0x3c],
        ..template
    });
    // The default settings trace with no upscaler: Upscaling alone locks.
    let plain = ModernSettings::default();
    assert!(modern_row_locked(
        &plain,
        support,
        [0, ModernRow::Upscaling.address()]
    ));
    assert!(!modern_row_locked(
        &plain,
        support,
        [0, ModernRow::AntiAliasing.address()]
    ));
    assert!(!modern_row_locked(
        &plain,
        support,
        [0, SETTINGS_STATE_ADDRESS + 0x3c]
    ));
    refresh_disabled_rows(&mut items, &plain, support);
    let flags = |items: &[FrontendMenuItem], row: ModernRow| {
        items
            .iter()
            .find(|item| item.draw_arguments[1] == row.address())
            .unwrap()
            .flags
    };
    assert_eq!(flags(&items, ModernRow::Upscaling), 1 | DISABLED_ROW);
    assert_eq!(flags(&items, ModernRow::AntiAliasing), 1);
    assert_eq!(flags(&items, ModernRow::Lighting), 1);
    assert_eq!(items.last().unwrap().flags, 1);
    // An upscaler locks Anti-aliasing and frees Upscaling; without ray
    // tracing, Ray Tracing locks too, and Lighting stays free.
    let traced = ModernSettings {
        upscaler: Upscaler::Dlss,
        ..ModernSettings::default()
    };
    let untraced = ModernSupport {
        ray_tracing: false,
        ..ModernSupport::default()
    };
    refresh_disabled_rows(&mut items, &traced, untraced);
    assert_eq!(flags(&items, ModernRow::RayTracing), 1 | DISABLED_ROW);
    assert_eq!(flags(&items, ModernRow::Upscaler), 1);
    assert_eq!(flags(&items, ModernRow::Upscaling), 1);
    assert_eq!(flags(&items, ModernRow::AntiAliasing), 1 | DISABLED_ROW);
    assert_eq!(flags(&items, ModernRow::Lighting), 1);
    assert_eq!(items.last().unwrap().flags, 1);
    // Toggling the upscaler flips the pair: no state unlocks both, since
    // Anti-aliasing needs no upscaler and Upscaling needs one.
    refresh_disabled_rows(&mut items, &plain, support);
    assert_eq!(flags(&items, ModernRow::Upscaling), 1 | DISABLED_ROW);
    assert_eq!(flags(&items, ModernRow::AntiAliasing), 1);
    refresh_disabled_rows(&mut items, &traced, support);
    assert_eq!(flags(&items, ModernRow::Upscaling), 1);
    assert_eq!(flags(&items, ModernRow::AntiAliasing), 1 | DISABLED_ROW);
    // The dim keeps the row's own hue at half brightness.
    assert_eq!(
        dimmed_row_color(MenuColor::from_rgb([200, 101, 50])),
        MenuColor::from_rgb([100, 50, 25])
    );
    assert_eq!(
        dimmed_row_color(MenuColor::from_word(0xcc_bb_aa_99)),
        MenuColor {
            rgb: [0x99 / 2, 0xaa / 2, 0xbb / 2],
            high_byte: Some(0xcc),
        }
    );
}
