use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_display::frontend_startup;
use crate::menu_text::INITIAL_GLYPH_BASE_WORD;
use crate::residency::ResourceSelection;
use crate::{Installation, ResidencyRequest, ResidentRuntime};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn assets() -> FrontendAssets {
    FrontendAssets::open(&root()).expect("retail Infest.exe is required")
}

/// The booted root plus the frontend leaf, then leaf 2 as `00432684`
/// requests it from the Options menu.
fn startup_residency(difficulty: u8, language: u8) -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty,
                language,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 2 })
        .unwrap();
    runtime
}

#[derive(Default)]
struct Recorder {
    begins: usize,
    shadows: Vec<DirectFlatQuad>,
    cloths: Vec<DirectShadeUvQuad>,
    labels: Vec<(Vec<u8>, [i32; 2], MenuColor)>,
}

impl StartupDrawHost for Recorder {
    type Error = std::convert::Infallible;
    fn begin_carousel(&mut self) -> Result<(), Self::Error> {
        self.begins += 1;
        Ok(())
    }
    fn shadow_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error> {
        self.shadows.push(quad);
        Ok(())
    }
    fn cloth_quad(&mut self, quad: DirectShadeUvQuad) -> Result<(), Self::Error> {
        self.cloths.push(quad);
        Ok(())
    }
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        self.labels.push((
            request.text.to_vec(),
            request.position_pixels,
            request.color,
        ));
        Ok(())
    }
}

/// Infest01's first carousel draw (00433640 at 366291 with delta 125,000,
/// then 004330E0): the Options visit starts with both fades complete.
fn infest01_state() -> StartupState {
    StartupState {
        selection: 0,
        direction: 0,
        angle: 0,
        age: 2_125_000,
        idle: 125_000,
        shadow_color: 0,
        clear_color: 0,
    }
}

fn frame(sine_quarter: &[u16; 4096], focal: [i32; 2]) -> StartupDrawFrame<'_> {
    StartupDrawFrame {
        surface_pixels: [1024, 768],
        focal,
        // The Direct3D display fields 004C12F0 reads (red, green, blue).
        material_shifts: [6, 1, 4],
        glyph_base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        sine_quarter,
    }
}

#[test]
fn flags_sit_on_the_infest01_ellipse() {
    let assets = assets();
    let sine = &assets.sine_table().values;
    let state = infest01_state();
    // 00432940 arguments of the six calls at 366299..366B61.
    let expected = [
        (500, 499, 0xff),
        (759, 389, 0xaa),
        (759, 170, 0x55),
        (500, 60, 0),
        (240, 170, 0x55),
        (240, 389, 0xaa),
    ];
    for (index, (x, y, depth)) in expected.into_iter().enumerate() {
        assert_eq!(
            flag_placement(&state, index as i32, sine),
            FlagPlacement { x, y, depth },
            "flag {index}"
        );
    }
}

#[test]
fn every_strip_walks_88_quads_from_corner_to_corner() {
    let mut walker = StripWalker::default();
    walker.step();
    // 0x3A1 along the top edge, 0x1000 down the left edge.
    assert_eq!((walker.edge_a(), walker.edge_b()), ([3, 0], [0, 0x10]));
    let mut quads = 1;
    while walker.b_state != 2 && walker.a_state != 2 {
        walker.step();
        quads += 1;
    }
    // Infest01: 0x210 +84 and 0x210 +C4 calls for six flags in one draw.
    assert_eq!(quads, 88);
    assert_eq!(
        (walker.edge_a(), walker.edge_b()),
        ([0x100, 0x100], [0x100, 0x100])
    );
}

#[test]
fn first_infest01_draw_matches_the_recorded_packets() {
    let assets = assets();
    let runtime = startup_residency(4, 0);
    // The 1024x768 display tier supplies Infest01's 00546F20/24 of 0x200.
    let focal = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([1024, 768]),
    )
    .unwrap()
    .projection
    .focal;
    assert_eq!(focal, [0x200, 0x200]);
    let mut state = infest01_state();
    let mut host = Recorder::default();
    let outcome = draw_startup_carousel(
        frame(&assets.sine_table().values, focal),
        &mut state,
        &StartupResidentResources(RenderResources::new(&runtime)),
        &mut host,
    )
    .unwrap();
    assert_eq!(outcome, StartupDrawOutcome::Drawn);
    assert_eq!(host.begins, 1);
    assert_eq!((host.shadows.len(), host.cloths.len()), (528, 528));
    // H7/0x22 is grey 4,4,4. Infest01's 565 display carried it as 0x2104;
    // the port's 555 layout holds the same colour as 0x1084. The clear is
    // the full blue in that layout.
    assert_eq!(state.shadow_color, 0x1084);
    assert_eq!(state.clear_color, 0x211f);
    // Shadow packets 1, 2 and 4 (366299:679, 366299:1D45, 36629A:7A3).
    assert_eq!(
        host.shadows[0].corners,
        [[466, 570], [467, 570], [464, 572], [466, 570]]
    );
    assert_eq!(
        host.shadows[1].corners,
        [[467, 570], [470, 570], [463, 574], [464, 572]]
    );
    assert_eq!(
        host.shadows[3].corners,
        [[472, 570], [474, 570], [459, 579], [461, 576]]
    );
    assert!(host.shadows.iter().all(|quad| quad.descriptor_word == 0
        && quad.color_word == 0x1084
        && quad.vertex_depths == [STARTUP_CAROUSEL_DEPTH_APPROXIMATION; 4]));
    // Cloth packets 1..3 (3662AD:9A9, 366392:2620, 366393:939).
    let cloth = |index: usize| {
        let quad = host.cloths[index];
        (quad.corners, quad.uv, quad.shades)
    };
    assert_eq!(
        cloth(0),
        (
            [[420, 447], [421, 447], [420, 453], [420, 447]],
            [[0, 0], [0x300, 0], [0, 0x1000], [0, 0]],
            [0x0048_4848; 4]
        )
    );
    assert_eq!(
        cloth(1),
        (
            [[421, 447], [424, 447], [420, 460], [420, 453]],
            [[0x300, 0], [0x700, 0], [0, 0x2000], [0, 0x1000]],
            [0x0048_4848; 4]
        )
    );
    assert_eq!(
        cloth(2),
        (
            [[424, 447], [426, 447], [420, 466], [420, 460]],
            [[0x700, 0], [0xa00, 0], [0, 0x3000], [0, 0x2000]],
            [0x0048_4848; 4]
        )
    );
    assert!(host
        .cloths
        .iter()
        .all(|quad| quad.mode == STARTUP_CLOTH_MODE
            && quad.vertex_depths == [STARTUP_CAROUSEL_DEPTH_APPROXIMATION; 4]));
    let sprites: Vec<_> = host
        .cloths
        .chunks(88)
        .map(|strip| strip[0].sprite_argument)
        .collect();
    assert_eq!(sprites, [0x4a0, 0x4a1, 0x4a2, 0x4a3, 0x4a4, 0x4a5]);
    // H0 string 0 at nine tenths of the height, centred on half the width.
    let resources = RenderResources::new(&runtime);
    let font = resources.font(0).unwrap();
    let text = resources.string(0).unwrap();
    let measured = i32::from(measure_text_width(font, text).unwrap() as i16);
    assert_eq!(host.labels.len(), 1);
    assert_eq!(host.labels[0].0, text);
    assert_eq!(host.labels[0].1, [512 - measured / 2, 691]);
    assert_eq!(
        host.labels[0].2,
        MenuColor::from_word(resources.style(STARTUP_LABEL_STYLE).unwrap())
    );
}

#[test]
fn the_first_construction_fades_in_from_nothing() {
    let assets = assets();
    let runtime = startup_residency(4, 0);
    let resources = StartupResidentResources(RenderResources::new(&runtime));
    let sine = &assets.sine_table().values;
    // Age 0 (00546B24 clear, before any tick): only the preamble runs.
    let mut state = StartupState {
        age: 0,
        clear_color: 0x1234,
        ..infest01_state()
    };
    let mut host = Recorder::default();
    assert_eq!(
        draw_startup_carousel(
            frame(sine, [0x200, 0x200]),
            &mut state,
            &resources,
            &mut host
        )
        .unwrap(),
        StartupDrawOutcome::Delayed
    );
    assert_eq!(host.begins, 0);
    assert_eq!(state.clear_color, 0x1234);
    // Half way through the fade-in: halved colours, radii and shades, and
    // the H7/0x20 clear.
    state.age = 500_000;
    let shadow = resources.material_word(STARTUP_SHADOW_MATERIAL).unwrap();
    draw_startup_carousel(
        frame(sine, [0x200, 0x200]),
        &mut state,
        &resources,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        state.shadow_color,
        scale_display_word(shadow, [6, 1, 4], 500_000, 1_000_000)
    );
    assert_eq!(
        state.clear_color,
        resources.material_word(STARTUP_CLEAR_MATERIAL).unwrap()
    );
    let front = flag_placement(&state, 0, sine);
    // 300 * 0.5 radius, plus 0x8000 extra angle: the front flag is at the back.
    assert_eq!(front.depth, 0);
    let style = resources.style(STARTUP_LABEL_STYLE).unwrap();
    assert_eq!(
        host.labels[0].2,
        MenuColor::from_word(scale_low_bytes(style, 500_000, 1_000_000))
    );
    // Half way through the background fade: the blue rises from black.
    state.age = 1_500_000;
    draw_startup_carousel(
        frame(sine, [0x200, 0x200]),
        &mut state,
        &resources,
        &mut host,
    )
    .unwrap();
    assert_eq!(state.clear_color, 0x108f);
    assert_eq!(state.shadow_color, shadow);
}

#[test]
fn display_word_fades_follow_the_draw_masks() {
    assert_eq!(background_word([6, 1, 4]), 0x211f);
    assert_eq!(
        scale_display_word(0x211f, [6, 1, 4], 500_000, 1_000_000),
        0x108f
    );
    assert_eq!(scale_display_word(0x7fff, [6, 1, 4], 0, 1_000_000), 0);
    assert_eq!(
        scale_display_word(0x7fff, [6, 1, 4], 1_000_000, 1_000_000),
        0x7fff
    );
    // The high byte of a style word survives; the RGB bytes scale.
    assert_eq!(scale_low_bytes(0x8040_80ff, 1, 2), 0x8020_407f);
}
