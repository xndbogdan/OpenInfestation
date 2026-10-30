//! Presentation of validated observations, separate from static model policy.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use inf_formats::observed_model::{load_healthsmall_bundle, HealthSmallObservation};
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::camera::OrbitCamera;
use crate::captured_model::CapturedModelPresentation;
use crate::catalog::{AssetCatalog, ModelSelection};
use crate::render::project_model_geometry;
use crate::ui::{button, draw_text, UiLayout, UiRect, ACCENT, MUTED, TEXT, WARNING};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum CaptureViewMode {
    #[default]
    ScreenProjection,
    SourceGeometry,
}

impl CaptureViewMode {
    pub fn next(self) -> Self {
        match self {
            Self::ScreenProjection => Self::SourceGeometry,
            Self::SourceGeometry => Self::ScreenProjection,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ScreenProjection => "CAPTURED XY",
            Self::SourceGeometry => "ORIENTED SOURCE",
        }
    }
}

#[derive(Debug)]
pub(crate) struct LoadedModelObservation {
    pub bundle: PathBuf,
    pub observation: HealthSmallObservation,
    pub presentation: CapturedModelPresentation,
    pub selection: ModelSelection,
}

impl LoadedModelObservation {
    pub fn load(bundle: &Path, catalog: &AssetCatalog) -> Result<Self, String> {
        // The importer independently hashes these exact canonical bytes. The
        // catalog supplies the physical occurrence, never a nearby model slot.
        let selection = catalog
            .models
            .iter()
            .enumerate()
            .find_map(|(model_index, asset)| {
                if asset.locator.global_id != 168 {
                    return None;
                }
                asset
                    .variants
                    .iter()
                    .enumerate()
                    .find_map(|(variant_index, variant)| {
                        variant
                            .occurrences
                            .iter()
                            .position(|occurrence| {
                                occurrence.source.eq_ignore_ascii_case("1.ovl")
                                    && occurrence.local_id == 166
                            })
                            .map(|occurrence_index| ModelSelection {
                                model_index,
                                variant_index,
                                occurrence_index: Some(occurrence_index),
                            })
                    })
            })
            .ok_or("capture requires canonical 1.ovl H8 local166/global168 in the catalog")?;
        let variant = &catalog.models[selection.model_index].variants[selection.variant_index];
        let source = variant
            .entry
            .content_span
            .get(&variant.source_bytes)
            .ok_or("canonical model content span is outside its source")?;
        let observation = load_healthsmall_bundle(bundle, source)
            .map_err(|error| format!("{}: {error}", bundle.display()))?;
        let presentation = CapturedModelPresentation::build(&observation)
            .map_err(|error| format!("{}: {error}", bundle.display()))?;
        Ok(Self {
            bundle: bundle.to_owned(),
            observation,
            presentation,
            selection,
        })
    }
}

pub(crate) fn draw_scene(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    capture: Option<&LoadedModelObservation>,
    mode: CaptureViewMode,
    camera: &OrbitCamera,
) -> Result<(), String> {
    let viewport = layout.viewport;
    let Some(capture) = capture else {
        text_line(canvas, viewport, 24, "NO VERIFIED DRAW LOADED", WARNING)?;
        return text_line(
            canvas,
            viewport,
            44,
            "OPEN WITH --capture <HealthSmallDraw bundle>",
            MUTED,
        );
    };
    let drawing = UiRect::new(
        viewport.x,
        viewport.y + 64,
        viewport.width,
        viewport.height.saturating_sub(116),
    );
    let (vertices, scale) = match mode {
        CaptureViewMode::ScreenProjection => {
            let mut fit = capture
                .presentation
                .fit_screen([drawing.width, drawing.height]);
            for point in &mut fit.vertices {
                point.x = (point.x - drawing.width as f32 * 0.5) * camera.zoom
                    + drawing.width as f32 * 0.5
                    + camera.pan_x;
                point.y = (point.y - drawing.height as f32 * 0.5) * camera.zoom
                    + drawing.height as f32 * 0.5
                    + camera.pan_y;
            }
            (
                fit.vertices,
                Some(fit.pixels_per_captured_pixel * camera.zoom),
            )
        }
        CaptureViewMode::SourceGeometry => (
            project_model_geometry(
                &capture.presentation.source_geometry,
                [drawing.width, drawing.height],
                camera,
            ),
            None,
        ),
    };
    let old_clip = canvas.clip_rect();
    canvas.set_clip_rect(drawing.as_sdl());
    let result: Result<(), String> = (|| {
        let mut edges = BTreeSet::new();
        for [a, b, c] in &capture.presentation.triangles {
            for (a, b) in [(*a, *b), (*b, *c), (*c, *a)] {
                edges.insert((a.min(b), a.max(b)));
            }
        }
        let point = |index: usize| {
            Point::new(
                drawing.x + vertices[index].x.round() as i32,
                drawing.y + vertices[index].y.round() as i32,
            )
        };
        canvas.set_draw_color(Color::RGB(69, 151, 164));
        for (a, b) in edges {
            canvas.draw_line(point(a), point(b))?;
        }
        canvas.set_draw_color(ACCENT);
        for index in 0..vertices.len() {
            let p = point(index);
            canvas.fill_rect(Rect::new(p.x - 2, p.y - 2, 5, 5))?;
        }
        Ok(())
    })();
    canvas.set_clip_rect(old_clip);
    result?;
    text_line(
        canvas,
        viewport,
        14,
        match mode {
            CaptureViewMode::ScreenProjection => "CAPTURED SCREEN COORDINATES",
            CaptureViewMode::SourceGeometry => "SOURCE VERTICES / OBSERVED BASIS",
        },
        ACCENT,
    )?;
    let note = scale.map_or_else(
        || "ORBITABLE GEOMETRY / WORLD TRANSLATION OMITTED".to_owned(),
        |scale| format!("{scale:.1}X MAGNIFICATION / NATIVE PIXEL QUANTIZATION RETAINED"),
    );
    text_line(canvas, viewport, 34, &note, MUTED)?;
    text_line(
        canvas,
        viewport,
        viewport.height.saturating_sub(38) as i32,
        "84 PROJECTION INPUTS / 120 AUTHORED TRIANGLES / ALL EDGES SHOWN",
        TEXT,
    )?;
    text_line(
        canvas,
        viewport,
        viewport.height.saturating_sub(20) as i32,
        "GEOMETRY INSPECTION / MATERIAL / CULLING / FRAME APPEARANCE UNVERIFIED",
        WARNING,
    )
}

pub(crate) fn draw_sidebar(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    capture: Option<&LoadedModelObservation>,
    index: usize,
    count: usize,
    mode: CaptureViewMode,
) -> Result<(), String> {
    let rect = layout.sidebar;
    let Some(capture) = capture else {
        return text_line(canvas, rect, 15, "CAPTURED DRAW UNAVAILABLE", WARNING);
    };
    let observed = &capture.observation;
    let rows = [
        format!("DRAW {} / {count}", index + 1),
        "HEALTHSMALL  H8 #168".to_owned(),
        "SOURCE 1.OVL / LOCAL 166".to_owned(),
        "FULL SOURCE HASH VERIFIED".to_owned(),
        format!(
            "THREAD {:X}  PARENT {:08X}",
            observed.events[0].thread_id, observed.parent_context
        ),
        format!("CONTEXT {:08X}", observed.events[0].context_address),
        format!("ENTRY {:08X}", observed.events[0].entry_address),
        format!(
            "CALLBACK {:08X} / SELECTOR {}",
            observed.callback.target_address, observed.callback.selector
        ),
        format!(
            "RETURN {:04X} -> R1 {:04X}",
            observed.callback.result, observed.callback.register1
        ),
        format!(
            "BASIS COUNTER {:X} -> {:X}",
            observed.incoming.basis.words[0], observed.nested.basis.words[0]
        ),
        format!(
            "ORIGIN {} / {} / {}",
            observed.nested.origin[0], observed.nested.origin[1], observed.nested.origin[2]
        ),
        "84 EVEN SLOTS 2..168 / FAR".to_owned(),
    ];
    for (row, text) in rows.iter().enumerate() {
        text_line(
            canvas,
            rect,
            15 + row as i32 * 17,
            text,
            if row < 4 { ACCENT } else { TEXT },
        )?;
    }
    button(
        canvas,
        layout.model_variant,
        &format!("C VIEW: {}", mode.label()),
        false,
        true,
    )?;
    button(canvas, layout.reset_position, "R RESET VIEW", false, true)?;
    let mut y = 306;
    for (label, event_index) in [
        ("ENTRY", 0),
        ("CALL", 2),
        ("RETURN", 3),
        ("WRITE", 4),
        ("POST05", 6),
        ("END", 7),
    ] {
        text_line(
            canvas,
            rect,
            y,
            &format!("{label:6} {}", observed.events[event_index].position),
            MUTED,
        )?;
        y += 15;
    }
    let depth_min = observed
        .projection_slots
        .iter()
        .map(|p| p.depth_word as i32)
        .min()
        .unwrap_or(0);
    let depth_max = observed
        .projection_slots
        .iter()
        .map(|p| p.depth_word as i32)
        .max()
        .unwrap_or(0);
    let [xmin, ymin, xmax, ymax] = capture.presentation.screen_bounds;
    let command_start = capture
        .presentation
        .triangle_sources
        .iter()
        .map(|source| source.command_span.start)
        .min()
        .unwrap_or(0);
    let command_end = capture
        .presentation
        .triangle_sources
        .iter()
        .map(|source| source.command_span.end)
        .max()
        .unwrap_or(0);
    for text in [
        format!("XY {xmin}..{xmax} / {ymin}..{ymax}"),
        format!("DEPTH {depth_min}..{depth_max}"),
        format!("FACE COMMANDS {command_start:X}..{command_end:X}"),
        "WORLD XYZ: SLOT 0 ONLY".to_owned(),
        "PROJECTION SLOT XYZ: UNKNOWN".to_owned(),
        "MATERIAL / LIGHTING: UNOBSERVED".to_owned(),
        "LEFT/RIGHT: PREVIOUS/NEXT DRAW".to_owned(),
        "WHEEL: ZOOM / MIDDLE: PAN".to_owned(),
        if mode == CaptureViewMode::SourceGeometry {
            "RIGHT DRAG: ORBIT SOURCE"
        } else {
            "SCREEN AXES REMAIN FIXED"
        }
        .to_owned(),
    ] {
        text_line(canvas, rect, y + 12, &text, MUTED)?;
        y += 17;
    }
    Ok(())
}

fn text_line(
    canvas: &mut Canvas<Window>,
    rect: UiRect,
    y: i32,
    text: &str,
    color: Color,
) -> Result<(), String> {
    if y < 0 || y + 8 > rect.height as i32 {
        return Ok(());
    }
    let capacity = (rect.width.saturating_sub(28) / 6) as usize;
    let text = if text.chars().count() > capacity {
        format!(
            "{}...",
            text.chars()
                .take(capacity.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        text.to_owned()
    };
    draw_text(canvas, rect.x + 14, rect.y + y, &text, color, 1)
}
