//! Presentation phase for the workbench, kept separate from event/state logic.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use inf_formats::{
    analyze_model_diagnostic, ModelDiagnosticCompleteness, ModelDiagnosticFaceFlags,
    ModelDiagnosticFacePresentation, ModelDiagnosticGeometry, ModelDiagnosticOmissionKind,
    ModelDiagnosticProjection, ModelDiagnosticRequest, ModelDiagnosticUnavailableKind,
    ModelDiagnosticView, ModelFanVertexAttributes, ModelMaterialDomain, OverlayKey, SceneContext,
    SpriteBlendMode, SpriteRenderMode, SpriteShade,
};
use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::camera::OrbitCamera;
use crate::capture_view::{self, CaptureViewMode, LoadedModelObservation};
use crate::catalog::{AssetCatalog, ModelVariant, VIEWER_DEFAULT_LANGUAGE};
use crate::inspection_eye::{EyeControls, InspectionEye};
use crate::model_composite::{
    build_neutral_model_composite, immediate_model_children, model_name, NeutralModelComposite,
};
use crate::model_presentation::ModelPresentation;
use crate::render::{
    decode_sprite, decode_terrain, draw_image, draw_presented_model, selected_sprite_variant,
    ModelFaceCull, ModelFacePaint, ModelFaceSurface, ModelRenderStats, PixelImage, QuadSplit,
    TerrainChannel, TextureUv,
};
use crate::ui::{
    button, checkbox, draw_text, fill, outline, Tab, UiLayout, ACCENT, BACKGROUND, BORDER,
    DISABLED, MUTED, PANEL, TEXT, VIEWPORT_BACKGROUND, WARNING,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpriteCacheKey {
    asset: usize,
    mode: SpriteRenderMode,
    physical_override: Option<usize>,
    shade: SpriteShade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerrainCacheKey {
    asset: usize,
    channel: TerrainChannel,
}

#[derive(Debug)]
struct ImageCache<Key> {
    key: Key,
    image: Result<PixelImage, String>,
}

pub(crate) struct ViewContext<'a> {
    pub tab: Tab,
    pub model_index: usize,
    pub model_variant_index: usize,
    pub model_occurrence_index: Option<usize>,
    pub model_presentation: ModelPresentation,
    pub inspection_eye: Option<InspectionEye>,
    pub eye_shift: u8,
    pub inspection_eye_text: &'a str,
    pub inspection_eye_active: bool,
    pub capture: Option<&'a LoadedModelObservation>,
    pub capture_index: usize,
    pub capture_count: usize,
    pub capture_view: CaptureViewMode,
    pub sprite_index: usize,
    pub terrain_index: usize,
    pub camera: &'a OrbitCamera,
    pub search_text: &'a str,
    pub search_active: bool,
    pub model_position_text: &'a str,
    pub model_position_active: bool,
    pub model_textures: bool,
    pub model_texture_difficulty: u8,
    pub model_texture_mode: SpriteRenderMode,
    pub model_quad_split: QuadSplit,
    pub sprite_mode: SpriteRenderMode,
    pub sprite_variant_override: Option<usize>,
    pub sprite_shade: SpriteShade,
    pub terrain_channel: TerrainChannel,
    pub status_message: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelPresentationCacheKey {
    model: usize,
    variant: usize,
    occurrence: Option<usize>,
    presentation: ModelPresentation,
    inspection_eye: Option<InspectionEye>,
    eye_shift: u8,
    scene: SceneContext,
    mode: SpriteRenderMode,
}

/// Build the explicit load context for texturing one viewed model: the level
/// comes from the model's own source OVL, difficulty from the workbench
/// control, language from the workbench default. A source that does not parse
/// as a retail OVL name resolves no surfaces instead of guessing a level.
fn scene_for_model_source(source: &str, difficulty: u8) -> Option<SceneContext> {
    OverlayKey::parse_filename(source)
        .ok()
        .map(|key| SceneContext::new(key.level, difficulty, VIEWER_DEFAULT_LANGUAGE))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvedModelPresentation {
    LocalShell,
    NeutralComposite,
    LocalFallback,
    ExplicitEye,
}

#[derive(Clone, Debug)]
struct ModelPresentationCache {
    key: ModelPresentationCacheKey,
    geometry: ModelDiagnosticGeometry,
    surfaces: Vec<ModelFaceSurface>,
    neutral_child_instances: usize,
    resolved_presentation: ResolvedModelPresentation,
}

#[derive(Debug, Default)]
pub(crate) struct ViewState {
    model_cache: Option<ModelPresentationCache>,
    sprite_cache: Option<ImageCache<SpriteCacheKey>>,
    terrain_cache: Option<ImageCache<TerrainCacheKey>>,
    model_stats: ModelRenderStats,
}

impl ViewState {
    pub fn invalidate_sprite(&mut self) {
        self.sprite_cache = None;
    }

    pub fn invalidate_terrain(&mut self) {
        self.terrain_cache = None;
    }

    pub fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        catalog: &AssetCatalog,
        data_root: &Path,
        context: &ViewContext<'_>,
    ) -> Result<(), String> {
        let (width, height) = canvas.output_size()?;
        let layout = UiLayout::new(width, height);
        canvas.set_draw_color(BACKGROUND);
        canvas.clear();
        fill(canvas, layout.viewport, VIEWPORT_BACKGROUND)?;
        fill(canvas, layout.sidebar, PANEL)?;
        fill(canvas, layout.status, Color::RGB(24, 28, 34))?;
        outline(canvas, layout.sidebar, BORDER)?;

        self.draw_scene(canvas, &layout, catalog, context)?;
        draw_toolbar(canvas, &layout, catalog, context)?;
        self.draw_sidebar(canvas, &layout, catalog, context)?;
        if context.tab == Tab::Models
            && context.model_presentation == ModelPresentation::ExplicitEye
        {
            draw_inspection_eye_controls(canvas, &layout, context)?;
        }
        draw_status(canvas, &layout, catalog, data_root, context)?;
        Ok(())
    }

    fn draw_scene(
        &mut self,
        canvas: &mut Canvas<Window>,
        layout: &UiLayout,
        catalog: &AssetCatalog,
        context: &ViewContext<'_>,
    ) -> Result<(), String> {
        if context.tab == Tab::Models
            && context.model_presentation == ModelPresentation::CapturedDraw
        {
            self.model_cache = None;
            self.model_stats = ModelRenderStats::default();
            return capture_view::draw_scene(
                canvas,
                layout,
                context.capture,
                context.capture_view,
                context.camera,
            );
        }
        match context.tab {
            Tab::Models => {
                if let Some(asset) = catalog.models.get(context.model_index) {
                    let Some(variant) = asset
                        .variant(context.model_variant_index)
                        .or_else(|| asset.default_variant())
                    else {
                        self.model_stats = ModelRenderStats::default();
                        return empty_view(canvas, layout, "MODEL SLOT HAS NO CONTENT VARIANTS");
                    };
                    let local_geometry = match &variant.diagnostic {
                        ModelDiagnosticProjection::Ready(geometry) => Some(geometry),
                        ModelDiagnosticProjection::Unavailable(_) => None,
                    };
                    let presentation_source =
                        selected_model_source(variant, context.model_occurrence_index);
                    let source = presentation_source.map(|(_, source)| source);
                    let scene = source.and_then(|source| {
                        scene_for_model_source(source, context.model_texture_difficulty)
                    });
                    let cache_key = ModelPresentationCacheKey {
                        model: context.model_index,
                        variant: context.model_variant_index,
                        occurrence: presentation_source.map(|(index, _)| index),
                        presentation: context.model_presentation,
                        inspection_eye: context.inspection_eye,
                        eye_shift: context.eye_shift,
                        scene: scene.unwrap_or(SceneContext::new(0, 0, VIEWER_DEFAULT_LANGUAGE)),
                        mode: context.model_texture_mode,
                    };
                    if self.model_cache.as_ref().map(|cache| cache.key) != Some(cache_key) {
                        let eye_diagnostic =
                            if context.model_presentation == ModelPresentation::ExplicitEye {
                                let Some(eye) = context.inspection_eye else {
                                    self.model_cache = None;
                                    self.model_stats = ModelRenderStats::default();
                                    return empty_view(
                                        canvas,
                                        layout,
                                        "EXPLICIT EYE REQUIRED: PRESS E AND ENTER ROOT X,Y,Z",
                                    );
                                };
                                Some(analyze_model_diagnostic(ModelDiagnosticRequest {
                                    entry: &variant.entry,
                                    view: ModelDiagnosticView::ExplicitEye {
                                        source: &variant.source_bytes,
                                        eye_root: eye.0,
                                        coordinate_shift: context.eye_shift,
                                    },
                                }))
                            } else {
                                None
                            };
                        let requested = match context.model_presentation {
                            ModelPresentation::LocalShell => {
                                local_geometry.map(|geometry| NeutralModelComposite {
                                    geometry: geometry.clone(),
                                    child_instances: 0,
                                })
                            }
                            ModelPresentation::NeutralComposite => source.and_then(|source| {
                                build_neutral_model_composite(catalog, variant, source)
                            }),
                            ModelPresentation::ExplicitEye => match eye_diagnostic.as_ref() {
                                Some(ModelDiagnosticProjection::Ready(geometry)) => {
                                    Some(NeutralModelComposite {
                                        geometry: geometry.clone(),
                                        child_instances: 0,
                                    })
                                }
                                Some(ModelDiagnosticProjection::Unavailable(unavailable)) => {
                                    self.model_cache = None;
                                    self.model_stats = ModelRenderStats::default();
                                    return unavailable_model_view(
                                        canvas,
                                        layout,
                                        unavailable.kind,
                                        &unavailable.reason,
                                    );
                                }
                                None => unreachable!("explicit eye checked above"),
                            },
                            ModelPresentation::CapturedDraw => {
                                unreachable!("captured draw handled separately")
                            }
                        };
                        let (presentation, resolved_presentation) = if let Some(requested) =
                            requested
                        {
                            let resolved =
                                if context.model_presentation == ModelPresentation::ExplicitEye {
                                    ResolvedModelPresentation::ExplicitEye
                                } else if requested.child_instances == 0 {
                                    ResolvedModelPresentation::LocalShell
                                } else {
                                    ResolvedModelPresentation::NeutralComposite
                                };
                            (requested, resolved)
                        } else if context.model_presentation == ModelPresentation::NeutralComposite
                        {
                            let Some(geometry) = local_geometry else {
                                self.model_cache = None;
                                self.model_stats = ModelRenderStats::default();
                                let reason = match &variant.diagnostic {
                                    ModelDiagnosticProjection::Unavailable(unavailable) => {
                                        unavailable.reason.as_str()
                                    }
                                    ModelDiagnosticProjection::Ready(_) => {
                                        "NO PROVEN STATIC HANDLER-8 CHILD COMPOSITE"
                                    }
                                };
                                return unavailable_composite_view(canvas, layout, reason);
                            };
                            (
                                NeutralModelComposite {
                                    geometry: geometry.clone(),
                                    child_instances: 0,
                                },
                                ResolvedModelPresentation::LocalFallback,
                            )
                        } else {
                            self.model_stats = ModelRenderStats::default();
                            let ModelDiagnosticProjection::Unavailable(unavailable) =
                                &variant.diagnostic
                            else {
                                unreachable!("ready local geometry was handled above")
                            };
                            return unavailable_model_view(
                                canvas,
                                layout,
                                unavailable.kind,
                                &unavailable.reason,
                            );
                        };
                        let surfaces = resolve_model_surfaces(
                            catalog,
                            source,
                            &presentation.geometry,
                            scene.as_ref(),
                            context.model_texture_mode,
                        );
                        self.model_cache = Some(ModelPresentationCache {
                            key: cache_key,
                            geometry: presentation.geometry,
                            surfaces,
                            neutral_child_instances: presentation.child_instances,
                            resolved_presentation,
                        });
                    }
                    let cache = self
                        .model_cache
                        .as_ref()
                        .expect("model presentation cache just initialized");
                    self.model_stats = draw_presented_model(
                        canvas,
                        layout.viewport,
                        &cache.geometry,
                        context.camera,
                        &cache.surfaces,
                        context.model_textures,
                        context.model_quad_split,
                    )?;
                    let projection_label = match cache.resolved_presentation {
                        ResolvedModelPresentation::ExplicitEye => {
                            "EXPLICIT EYE - VIEW-CONDITIONED MESH"
                        }
                        ResolvedModelPresentation::NeutralComposite => {
                            "PROVEN PARTIAL NEUTRAL COMPOSITE"
                        }
                        ResolvedModelPresentation::LocalFallback => {
                            "LOCAL SHELL - COMPOSITE UNAVAILABLE"
                        }
                        ResolvedModelPresentation::LocalShell => {
                            match cache.geometry.completeness {
                                ModelDiagnosticCompleteness::CompleteMesh => {
                                    "PROVEN COMPLETE STATIC MESH"
                                }
                                ModelDiagnosticCompleteness::PartialMesh => {
                                    "PROVEN PARTIAL STATIC MESH"
                                }
                            }
                        }
                    };
                    draw_text(
                        canvas,
                        layout.viewport.x + 14,
                        layout.viewport.y + 14,
                        projection_label,
                        WARNING,
                        2,
                    )?;
                    let omits_child_instance =
                        cache.geometry.omitted_effects.iter().any(|omission| {
                            omission.kind == ModelDiagnosticOmissionKind::ModelInstances
                        });
                    let presentation_note = match cache.resolved_presentation {
                        ResolvedModelPresentation::ExplicitEye => "ROOT COORDINATES / IDENTITY BASIS - INSPECTION INPUT, NOT A RETAIL CAMERA",
                        ResolvedModelPresentation::NeutralComposite => {
                            "ZERO JOINT CHANNELS - NOT A CANONICAL FRAME"
                        }
                        ResolvedModelPresentation::LocalFallback => {
                            "REQUESTED CHILD CHAIN COULD NOT BE PROVEN"
                        }
                        ResolvedModelPresentation::LocalShell if omits_child_instance => {
                            "LOCAL SHELL ONLY - CHILD POSE NEEDS RUNTIME STATE"
                        }
                        ResolvedModelPresentation::LocalShell => "NOT A CANONICAL ANIMATION FRAME",
                    };
                    draw_text(
                        canvas,
                        layout.viewport.x + 14,
                        layout.viewport.y + 36,
                        presentation_note,
                        WARNING,
                        1,
                    )?;
                    if cache.resolved_presentation == ResolvedModelPresentation::ExplicitEye {
                        let source = variant
                            .occurrences
                            .first()
                            .map_or("UNKNOWN", |occurrence| occurrence.source.as_str());
                        for (index, decision) in
                            cache.geometry.view_plane_decisions.iter().enumerate()
                        {
                            draw_clipped_text(
                                canvas,
                                layout.viewport.x + 14,
                                layout.viewport.y + 54 + index as i32 * 15,
                                layout.viewport.width.saturating_sub(28),
                                &format!(
                                    "PLANE @{:X}  VECTOR {} @{:X}..{:X}  DOT {}  {}",
                                    decision.instruction_span.start,
                                    source,
                                    decision.vector_source_span.start,
                                    decision.vector_source_span.end,
                                    decision.dot_product,
                                    if decision.jump_taken { "JUMP" } else { "KEEP" }
                                ),
                                MUTED,
                            )?;
                        }
                    }
                    draw_text(
                        canvas,
                        layout.viewport.x + 14,
                        layout.viewport.y + layout.viewport.height as i32 - 20,
                        "RMB ORBIT  WHEEL ZOOM  MMB PAN  A VIEW  T TEXTURES  D/M RESOURCE  Q SPLIT",
                        MUTED,
                        1,
                    )?;
                } else {
                    self.model_stats = ModelRenderStats::default();
                    empty_view(canvas, layout, "NO MODELS IN SELECTED CATALOG")?;
                }
            }
            Tab::Sprites => {
                if let Some(asset) = catalog.sprites.get(context.sprite_index) {
                    let key = SpriteCacheKey {
                        asset: context.sprite_index,
                        mode: context.sprite_mode,
                        physical_override: context.sprite_variant_override,
                        shade: context.sprite_shade,
                    };
                    if self.sprite_cache.as_ref().map(|cache| cache.key) != Some(key) {
                        self.sprite_cache = Some(ImageCache {
                            key,
                            image: decode_sprite(
                                &asset.source_bytes,
                                &asset.entry,
                                context.sprite_mode,
                                context.sprite_variant_override,
                                context.sprite_shade,
                            ),
                        });
                    }
                    match &self
                        .sprite_cache
                        .as_ref()
                        .expect("cache just initialized")
                        .image
                    {
                        Ok(image) => draw_image(canvas, layout.viewport, image, true)?,
                        Err(error) => empty_view(canvas, layout, error)?,
                    }
                } else {
                    empty_view(canvas, layout, "NO SPRITES IN SELECTED CATALOG")?;
                }
            }
            Tab::Terrain => {
                if let Some(asset) = catalog.terrain.get(context.terrain_index) {
                    let key = TerrainCacheKey {
                        asset: context.terrain_index,
                        channel: context.terrain_channel,
                    };
                    if self.terrain_cache.as_ref().map(|cache| cache.key) != Some(key) {
                        self.terrain_cache = Some(ImageCache {
                            key,
                            image: Ok(decode_terrain(
                                &asset.entry,
                                asset.palette,
                                context.terrain_channel,
                            )),
                        });
                    }
                    if let Ok(image) = &self
                        .terrain_cache
                        .as_ref()
                        .expect("cache just initialized")
                        .image
                    {
                        draw_image(canvas, layout.viewport, image, false)?;
                    }
                } else {
                    empty_view(canvas, layout, "NO TERRAIN IN SELECTED CATALOG")?;
                }
            }
        }
        Ok(())
    }

    fn draw_sidebar(
        &self,
        canvas: &mut Canvas<Window>,
        layout: &UiLayout,
        catalog: &AssetCatalog,
        context: &ViewContext<'_>,
    ) -> Result<(), String> {
        if context.tab == Tab::Models
            && context.model_presentation == ModelPresentation::CapturedDraw
        {
            return capture_view::draw_sidebar(
                canvas,
                layout,
                context.capture,
                context.capture_index,
                context.capture_count,
                context.capture_view,
            );
        }
        let x = layout.sidebar.x + 14;
        let width = layout.sidebar.width.saturating_sub(28);
        let mut y = layout.sidebar.y + 15;
        draw_text(canvas, x, y, context.tab.label(), ACCENT, 2)?;
        y += 26;

        match context.tab {
            Tab::Models => {
                if let Some(asset) = catalog.models.get(context.model_index) {
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("{} / {}", context.model_index + 1, catalog.models.len()),
                        MUTED,
                    )?;
                    side_line(canvas, x, &mut y, width, &asset.locator.name, TEXT)?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!(
                            "GLOBAL #{}  0X{:04X}",
                            asset.locator.global_id, asset.locator.global_id
                        ),
                        TEXT,
                    )?;
                    if let Some(variant) = selected_model_variant(catalog, context) {
                        side_line(
                            canvas,
                            x,
                            &mut y,
                            width,
                            &format!(
                                "VARIANT {} / {}",
                                context.model_variant_index + 1,
                                asset.variants.len()
                            ),
                            if asset.variants.len() > 1 {
                                ACCENT
                            } else {
                                MUTED
                            },
                        )?;
                        side_line(
                            canvas,
                            x,
                            &mut y,
                            width,
                            &format!("OCCURRENCES: {}", variant.occurrences.len()),
                            MUTED,
                        )?;
                        let selected_occurrence =
                            selected_model_source(variant, context.model_occurrence_index);
                        if let Some((occurrence_index, source)) = selected_occurrence {
                            let occurrence = variant
                                .occurrences
                                .get(occurrence_index)
                                .expect("selected source index is valid");
                            side_line(
                                canvas,
                                x,
                                &mut y,
                                width,
                                &format!("SOURCE: {source}  LOCAL #{}", occurrence.local_id),
                                TEXT,
                            )?;
                        }
                        if variant.occurrences.len() > 1 {
                            side_line(
                                canvas,
                                x,
                                &mut y,
                                width,
                                &format!(
                                    "ALSO IN: {}",
                                    additional_occurrences(
                                        variant,
                                        selected_occurrence.map(|(index, _)| index),
                                    )
                                ),
                                MUTED,
                            )?;
                        }
                        side_line(
                            canvas,
                            x,
                            &mut y,
                            width,
                            &format!("RAW VERTICES: {}", variant.entry.vertices.len()),
                            MUTED,
                        )?;
                        for child_id in immediate_model_children(variant) {
                            side_line(
                                canvas,
                                x,
                                &mut y,
                                width,
                                &format!(
                                    "CHILD: #{} {}",
                                    child_id,
                                    model_name(catalog, child_id).unwrap_or("UNRESOLVED")
                                ),
                                WARNING,
                            )?;
                        }
                        let parents = catalog.model_parents(asset.locator.global_id);
                        if !parents.is_empty() {
                            side_line(
                                canvas,
                                x,
                                &mut y,
                                width,
                                &format!(
                                    "USED BY: {}",
                                    parents
                                        .iter()
                                        .map(|parent| format!("#{parent}"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                MUTED,
                            )?;
                        }
                        let cache = self.model_cache.as_ref().filter(|cache| {
                            cache.key.model == context.model_index
                                && cache.key.variant == context.model_variant_index
                                && cache.key.occurrence
                                    == selected_occurrence.map(|(index, _)| index)
                                && cache.key.presentation == context.model_presentation
                        });
                        if context.model_presentation == ModelPresentation::NeutralComposite
                            && catalog.native_basis_math_error().is_some()
                        {
                            side_line(
                                canvas,
                                x,
                                &mut y,
                                width,
                                "ORIGINAL ROTATION TABLE UNAVAILABLE",
                                WARNING,
                            )?;
                        }
                        if let Some(cache) = cache {
                            match cache.resolved_presentation {
                                ResolvedModelPresentation::NeutralComposite => side_line(
                                    canvas,
                                    x,
                                    &mut y,
                                    width,
                                    &format!(
                                        "PROVEN NEUTRAL CHILDREN: {}",
                                        cache.neutral_child_instances
                                    ),
                                    ACCENT,
                                )?,
                                ResolvedModelPresentation::LocalFallback => side_line(
                                    canvas,
                                    x,
                                    &mut y,
                                    width,
                                    "COMPOSITE UNAVAILABLE; SHOWING LOCAL",
                                    WARNING,
                                )?,
                                ResolvedModelPresentation::LocalShell => {}
                                ResolvedModelPresentation::ExplicitEye => side_line(
                                    canvas,
                                    x,
                                    &mut y,
                                    width,
                                    "EXPLICIT EYE / ROOT IDENTITY",
                                    ACCENT,
                                )?,
                            }
                            draw_model_geometry_summary(
                                canvas,
                                x,
                                &mut y,
                                width,
                                &cache.geometry,
                                &self.model_stats,
                            )?;
                        } else {
                            match &variant.diagnostic {
                                ModelDiagnosticProjection::Ready(root_geometry) => {
                                    draw_model_geometry_summary(
                                        canvas,
                                        x,
                                        &mut y,
                                        width,
                                        root_geometry,
                                        &self.model_stats,
                                    )?;
                                }
                                ModelDiagnosticProjection::Unavailable(unavailable) => {
                                    side_line(
                                        canvas,
                                        x,
                                        &mut y,
                                        width,
                                        unavailable_kind_label(unavailable.kind),
                                        if unavailable.kind == ModelDiagnosticUnavailableKind::Empty
                                        {
                                            MUTED
                                        } else {
                                            WARNING
                                        },
                                    )?;
                                    side_line(
                                        canvas,
                                        x,
                                        &mut y,
                                        width,
                                        &unavailable.reason,
                                        WARNING,
                                    )?;
                                    side_line(
                                        canvas,
                                        x,
                                        &mut y,
                                        width,
                                        &format!(
                                            "UNRESOLVED DYNAMIC DEPS: {}",
                                            unavailable.unresolved_dependencies.len()
                                        ),
                                        MUTED,
                                    )?;
                                }
                            }
                        }
                    }
                }
                let variant_count = catalog
                    .models
                    .get(context.model_index)
                    .map_or(0, |asset| asset.variants.len());
                button(
                    canvas,
                    layout.model_variant,
                    &model_variant_label(context.model_variant_index, variant_count),
                    false,
                    variant_count > 1,
                )?;
                button(
                    canvas,
                    layout.reset_position,
                    "RESET POSITION",
                    false,
                    context.camera.has_pan(),
                )?;
                checkbox(
                    canvas,
                    layout.texture_checkbox,
                    "T TEXTURES",
                    context.model_textures,
                    true,
                )?;
                draw_clipped_text(
                    canvas,
                    x,
                    layout.texture_checkbox.y + 22,
                    width,
                    "H3 TEXTURE / H7 MATERIAL LINK PROVEN",
                    ACCENT,
                )?;
                button(
                    canvas,
                    layout.model_texture_difficulty,
                    &format!(
                        "D DIFFICULTY RESOURCES: D{}",
                        context.model_texture_difficulty
                    ),
                    false,
                    true,
                )?;
                button(
                    canvas,
                    layout.model_texture_mode,
                    match context.model_texture_mode {
                        SpriteRenderMode::Mode0 => "M TEXTURE SELECTOR: 0",
                        SpriteRenderMode::Mode1 => "M TEXTURE SELECTOR: 1",
                    },
                    false,
                    true,
                )?;
                button(
                    canvas,
                    layout.model_quad_split,
                    &format!("Q QUAD SPLIT: {}", context.model_quad_split.label()),
                    false,
                    true,
                )?;
                checkbox(
                    canvas,
                    layout.shadow_checkbox,
                    "SHADOW (DISABLED)",
                    false,
                    false,
                )?;
                draw_clipped_text(
                    canvas,
                    x,
                    layout.shadow_checkbox.y + 22,
                    width,
                    "BASE COLORS; LIGHTING/SHADOW PENDING",
                    DISABLED,
                )?;
            }
            Tab::Sprites => {
                if let Some(asset) = catalog.sprites.get(context.sprite_index) {
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("{} / {}", context.sprite_index + 1, catalog.sprites.len()),
                        MUTED,
                    )?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("GLOBAL: {}", optional_id(asset.global_id)),
                        TEXT,
                    )?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("LOCAL: {}", asset.entry.local_index),
                        TEXT,
                    )?;
                    side_line(canvas, x, &mut y, width, &asset.source, TEXT)?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("PHYSICAL VARIANTS: {}", asset.entry.variants.len()),
                        MUTED,
                    )?;
                    if let Some(variant) = selected_sprite_variant(
                        &asset.entry,
                        context.sprite_mode,
                        context.sprite_variant_override,
                    ) {
                        side_line(
                            canvas,
                            x,
                            &mut y,
                            width,
                            &format!("SIZE: {} X {}", variant.width, variant.height),
                            MUTED,
                        )?;
                        side_line(
                            canvas,
                            x,
                            &mut y,
                            width,
                            &format!("SPRITE ID: {}", variant.sprite_id),
                            MUTED,
                        )?;
                    }
                }
                button(
                    canvas,
                    layout.sprite_mode,
                    match context.sprite_mode {
                        SpriteRenderMode::Mode0 => "LOGICAL MODE: 0",
                        SpriteRenderMode::Mode1 => "LOGICAL MODE: 1",
                    },
                    false,
                    true,
                )?;
                button(
                    canvas,
                    layout.sprite_variant,
                    &sprite_variant_label(catalog, context),
                    false,
                    true,
                )?;
                button(
                    canvas,
                    layout.sprite_shade,
                    &sprite_shade_label(context.sprite_shade),
                    false,
                    true,
                )?;
            }
            Tab::Terrain => {
                if let Some(asset) = catalog.terrain.get(context.terrain_index) {
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("{} / {}", context.terrain_index + 1, catalog.terrain.len()),
                        MUTED,
                    )?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("LEVEL: {}", asset.level),
                        TEXT,
                    )?;
                    side_line(canvas, x, &mut y, width, &asset.source, TEXT)?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        &format!("ENTRY: {}", asset.entry_index),
                        TEXT,
                    )?;
                    side_line(canvas, x, &mut y, width, "GRID: 512 X 512", MUTED)?;
                    side_line(
                        canvas,
                        x,
                        &mut y,
                        width,
                        if asset.palette.is_some() {
                            "H7 PALETTE: PRESENT"
                        } else {
                            "H7 PALETTE: FALLBACK COLORS"
                        },
                        MUTED,
                    )?;
                }
                button(
                    canvas,
                    layout.terrain_channel,
                    &format!("CHANNEL: {}", context.terrain_channel.label()),
                    false,
                    true,
                )?;
            }
        }
        Ok(())
    }
}

fn draw_model_geometry_summary(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: &mut i32,
    width: u32,
    geometry: &ModelDiagnosticGeometry,
    stats: &ModelRenderStats,
) -> Result<(), String> {
    side_line(
        canvas,
        x,
        y,
        width,
        match geometry.completeness {
            ModelDiagnosticCompleteness::CompleteMesh => "DIAGNOSTIC: COMPLETE MESH",
            ModelDiagnosticCompleteness::PartialMesh => "DIAGNOSTIC: PARTIAL MESH",
        },
        if geometry.completeness == ModelDiagnosticCompleteness::CompleteMesh {
            ACCENT
        } else {
            WARNING
        },
    )?;
    side_line(
        canvas,
        x,
        y,
        width,
        &format!(
            "SURFACES: {} SOLID  {} TEXTURED",
            stats.solid_faces, stats.textured_faces
        ),
        MUTED,
    )?;
    if stats.unresolved_faces != 0 {
        side_line(
            canvas,
            x,
            y,
            width,
            &format!(
                "UNRESOLVED FACES: {}  EDGES: {}",
                stats.unresolved_faces, stats.segments
            ),
            WARNING,
        )?;
    }
    if stats.ignored_edge_commands != 0 {
        side_line(
            canvas,
            x,
            y,
            width,
            &format!("IGNORED EDGE COMMANDS: {}", stats.ignored_edge_commands),
            MUTED,
        )?;
    }
    let runtime_child_instances = geometry
        .omitted_effects
        .iter()
        .filter(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
        .map(|omission| omission.count)
        .sum::<usize>();
    if runtime_child_instances != 0 {
        side_line(
            canvas,
            x,
            y,
            width,
            &format!("RUNTIME CHILD INSTANCES: {runtime_child_instances}"),
            WARNING,
        )?;
    }
    let other_omission_groups = geometry
        .omitted_effects
        .iter()
        .filter(|omission| omission.kind != ModelDiagnosticOmissionKind::ModelInstances)
        .count();
    if other_omission_groups != 0 {
        side_line(
            canvas,
            x,
            y,
            width,
            &format!("OTHER OMITTED GROUPS: {other_omission_groups}"),
            WARNING,
        )?;
    }
    if !geometry.unresolved_dependencies.is_empty() {
        side_line(
            canvas,
            x,
            y,
            width,
            &format!(
                "UNRESOLVED RUNTIME DEPS: {}",
                geometry.unresolved_dependencies.len()
            ),
            MUTED,
        )?;
    }
    Ok(())
}

fn draw_inspection_eye_controls(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    context: &ViewContext<'_>,
) -> Result<(), String> {
    let controls = EyeControls::new(layout);
    let label = if context.inspection_eye_active {
        format!(
            "ROOT EYE X,Y,Z: {}_  ENTER APPLY / ESC CANCEL",
            context.inspection_eye_text
        )
    } else {
        format!(
            "E EDIT ROOT EYE X,Y,Z: {}",
            context
                .inspection_eye
                .map(|eye| eye.to_string())
                .unwrap_or_else(|| "NOT SUPPLIED".to_owned())
        )
    };
    text_input_field(
        canvas,
        controls.edit,
        &label,
        context.inspection_eye_active,
        true,
    )?;
    for (rect, label) in controls
        .adjustments
        .into_iter()
        .zip(["-X", "+X", "-Y", "+Y", "-Z", "+Z"])
    {
        button(canvas, rect, label, false, context.inspection_eye.is_some())?;
    }
    button(
        canvas,
        controls.shift,
        &format!("SHIFT: {}", context.eye_shift),
        false,
        true,
    )?;
    draw_clipped_text(
        canvas,
        controls.shift.x + 118,
        controls.shift.y + 9,
        layout.viewport.width.saturating_sub(418),
        "STEP 1024 / ORBIT INDEPENDENT",
        MUTED,
    )
}

fn draw_toolbar(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    catalog: &AssetCatalog,
    context: &ViewContext<'_>,
) -> Result<(), String> {
    button(
        canvas,
        layout.model_tab,
        "F1 MODELS",
        context.tab == Tab::Models,
        true,
    )?;
    button(
        canvas,
        layout.sprite_tab,
        "F2 SPRITES",
        context.tab == Tab::Sprites,
        true,
    )?;
    button(
        canvas,
        layout.terrain_tab,
        "F3 TERRAIN",
        context.tab == Tab::Terrain,
        true,
    )?;
    button(
        canvas,
        layout.model_presentation,
        &format!("A VIEW: {}", context.model_presentation.label()),
        context.model_presentation != ModelPresentation::LocalShell,
        context.tab == Tab::Models,
    )?;
    button(canvas, layout.previous, "< PREV", false, true)?;
    button(canvas, layout.next, "NEXT >", false, true)?;

    let captured =
        context.tab == Tab::Models && context.model_presentation == ModelPresentation::CapturedDraw;
    let model_controls_enabled = context.tab == Tab::Models && !captured;
    let position_label = if captured {
        format!(
            "DRAW {}/{}",
            if context.capture_count == 0 {
                0
            } else {
                context.capture_index + 1
            },
            context.capture_count
        )
    } else if context.model_position_active {
        format!(
            "MODEL {}_ / {}",
            context.model_position_text,
            catalog.models.len()
        )
    } else if model_controls_enabled {
        format!(
            "MODEL {}/{}",
            if catalog.models.is_empty() {
                0
            } else {
                context.model_index + 1
            },
            catalog.models.len()
        )
    } else {
        "MODEL POSITION".to_owned()
    };
    text_input_field(
        canvas,
        layout.model_position,
        &position_label,
        context.model_position_active,
        model_controls_enabled,
    )?;

    let search_label = if captured {
        "VERIFIED DRAW OBSERVATIONS / C: SWITCH GEOMETRY VIEW".to_owned()
    } else if model_controls_enabled {
        format!(
            "MODEL SEARCH: {}{}",
            context.search_text,
            if context.search_active { "_" } else { "" }
        )
    } else {
        "MODEL SEARCH (MODELS TAB)".to_owned()
    };
    text_input_field(
        canvas,
        layout.search,
        &search_label,
        context.search_active,
        model_controls_enabled,
    )
}

fn text_input_field(
    canvas: &mut Canvas<Window>,
    rect: crate::ui::UiRect,
    label: &str,
    active: bool,
    enabled: bool,
) -> Result<(), String> {
    fill(
        canvas,
        rect,
        if active {
            Color::RGB(31, 57, 68)
        } else {
            Color::RGB(20, 24, 30)
        },
    )?;
    outline(
        canvas,
        rect,
        if active {
            ACCENT
        } else if enabled {
            BORDER
        } else {
            DISABLED
        },
    )?;
    draw_clipped_text(
        canvas,
        rect.x + 7,
        rect.y + 9,
        rect.width.saturating_sub(14),
        label,
        if enabled { TEXT } else { DISABLED },
    )
}

fn sprite_variant_label(catalog: &AssetCatalog, context: &ViewContext<'_>) -> String {
    if let Some(index) = context.sprite_variant_override {
        format!("PHYSICAL VARIANT: {index}")
    } else if let Some(asset) = catalog.sprites.get(context.sprite_index) {
        match asset.entry.selectors.variant_index(context.sprite_mode) {
            Some(index) => format!("VARIANT: AUTO ({index})"),
            None => "VARIANT: AUTO (NONE)".to_owned(),
        }
    } else {
        "VARIANT: NONE".to_owned()
    }
}

fn selected_model_variant<'a>(
    catalog: &'a AssetCatalog,
    context: &ViewContext<'_>,
) -> Option<&'a ModelVariant> {
    let asset = catalog.models.get(context.model_index)?;
    asset
        .variant(context.model_variant_index)
        .or_else(|| asset.default_variant())
}

fn selected_model_source(
    variant: &ModelVariant,
    occurrence_override: Option<usize>,
) -> Option<(usize, &str)> {
    let index = occurrence_override
        .filter(|&index| index < variant.occurrences.len())
        .unwrap_or(0);
    variant
        .occurrences
        .get(index)
        .map(|occurrence| (index, occurrence.source.as_str()))
}

fn resolve_model_surfaces(
    catalog: &AssetCatalog,
    model_source: Option<&str>,
    geometry: &ModelDiagnosticGeometry,
    scene: Option<&SceneContext>,
    mode: SpriteRenderMode,
) -> Vec<ModelFaceSurface> {
    let (Some(model_source), Some(scene)) = (model_source, scene) else {
        return vec![ModelFaceSurface::unresolved(); geometry.faces.len()];
    };
    #[derive(Clone)]
    struct ResolvedTexture {
        image: Arc<PixelImage>,
        blend_mode: SpriteBlendMode,
    }

    let mut texture_cache = HashMap::<u16, Option<ResolvedTexture>>::new();

    let mut surfaces = geometry
        .faces
        .iter()
        .zip(&geometry.face_presentation)
        .map(|(face, presentation)| {
            let Ok(global_id) = u16::try_from(presentation.material.global_id) else {
                return unresolved_model_surface(presentation);
            };
            let paint = match presentation.material.domain {
                ModelMaterialDomain::Handler7Material => catalog
                    .model_material(global_id, scene, model_source)
                    .map(|material| ModelFacePaint::Solid {
                        rgba: rgb555_rgba(material.value.rgb555().raw()),
                    })
                    .unwrap_or(ModelFacePaint::Unresolved),
                ModelMaterialDomain::Handler3Texture => {
                    let texture = texture_cache
                        .entry(global_id)
                        .or_insert_with(|| {
                            let asset = catalog.model_texture(global_id, scene, model_source)?;
                            let blend_mode = selected_sprite_variant(&asset.entry, mode, None)?
                                .flags
                                .blend_mode();
                            let image = decode_sprite(
                                &asset.source_bytes,
                                &asset.entry,
                                mode,
                                None,
                                SpriteShade::Base,
                            )
                            .ok()
                            .map(Arc::new)?;
                            Some(ResolvedTexture { image, blend_mode })
                        })
                        .clone();
                    let Some(texture) = texture else {
                        return unresolved_model_surface(presentation);
                    };
                    let ResolvedTexture { image, blend_mode } = texture;
                    let Some((uvs, implicit_uv)) =
                        model_face_uvs(face.len(), presentation, [image.width, image.height])
                    else {
                        return unresolved_model_surface(presentation);
                    };
                    let fallback_rgba = average_opaque_rgba(&image).unwrap_or([82, 98, 112, 255]);
                    ModelFacePaint::Textured {
                        image,
                        uvs,
                        fallback_rgba,
                        implicit_uv,
                        blend_mode,
                    }
                }
            };
            ModelFaceSurface {
                paint,
                cull: model_face_cull(presentation),
            }
        })
        .collect::<Vec<_>>();
    surfaces.resize(geometry.faces.len(), ModelFaceSurface::unresolved());
    surfaces
}

fn model_face_cull(presentation: &ModelDiagnosticFacePresentation) -> ModelFaceCull {
    match presentation.source_flags {
        ModelDiagnosticFaceFlags::Face(flags) => ModelFaceCull::new(
            flags.cull_enabled(),
            flags.reverse_cull_direction(),
            presentation.mirrored_instance,
        ),
        ModelDiagnosticFaceFlags::Fan(flags) if flags.culls_from_first_triangle() => {
            ModelFaceCull::fan_gate(
                flags.reverse_cull_direction(),
                presentation.mirrored_instance,
                // A missing index cannot be evaluated honestly; the sentinel
                // leaves the fan gate unset and therefore rejects the face.
                presentation.fan_triangle_index.unwrap_or(u16::MAX),
                flags.culls_each_triangle(),
            )
        }
        ModelDiagnosticFaceFlags::Fan(flags) => ModelFaceCull::new(
            flags.culls_each_triangle(),
            flags.reverse_cull_direction(),
            presentation.mirrored_instance,
        ),
    }
}

fn unresolved_model_surface(presentation: &ModelDiagnosticFacePresentation) -> ModelFaceSurface {
    ModelFaceSurface {
        paint: ModelFacePaint::Unresolved,
        cull: model_face_cull(presentation),
    }
}

fn model_face_uvs(
    vertex_count: usize,
    presentation: &ModelDiagnosticFacePresentation,
    texture_size: [u32; 2],
) -> Option<(Vec<TextureUv>, bool)> {
    if let Some(attributes) = presentation.fan_vertex_attributes.as_ref() {
        let uvs = attributes
            .iter()
            .map(|attributes| match attributes {
                ModelFanVertexAttributes::Textured { uv_words, .. } => Some(TextureUv {
                    // FUN_004B4660 treats each word as unsigned Q0.16 and
                    // multiplies it by the selected Handler-3 dimensions.
                    u: f32::from(uv_words[0]) / 65_536.0,
                    v: f32::from(uv_words[1]) / 65_536.0,
                }),
                ModelFanVertexAttributes::Untextured { .. } => None,
            })
            .collect::<Option<Vec<_>>>()?;
        return (uvs.len() == vertex_count).then_some((uvs, false));
    }

    // FUN_004B1D20 / FUN_004B8330 construct fixed-face maxima as
    // `(dimension << 16) - 1` in unsigned 16.16 texel space. Normalizing for
    // this rasterizer therefore yields `1 - 1 / (dimension * 65536)`, keeping
    // the endpoint inside the final texel instead of placing it on 1.0. The
    // bottom-right third corner on triangles is intentional.
    let maximum_u = fixed_face_uv_endpoint(texture_size[0])?;
    let maximum_v = fixed_face_uv_endpoint(texture_size[1])?;
    let uvs = match vertex_count {
        3 => vec![
            TextureUv { u: 0.0, v: 0.0 },
            TextureUv {
                u: maximum_u,
                v: 0.0,
            },
            TextureUv {
                u: maximum_u,
                v: maximum_v,
            },
        ],
        4 => vec![
            TextureUv { u: 0.0, v: 0.0 },
            TextureUv {
                u: maximum_u,
                v: 0.0,
            },
            TextureUv {
                u: maximum_u,
                v: maximum_v,
            },
            TextureUv {
                u: 0.0,
                v: maximum_v,
            },
        ],
        _ => return None,
    };
    Some((uvs, true))
}

fn fixed_face_uv_endpoint(dimension: u32) -> Option<f32> {
    if dimension == 0 {
        return None;
    }
    let fixed_extent = f64::from(dimension) * 65_536.0;
    let endpoint = (1.0 - 1.0 / fixed_extent) as f32;
    // Large dimensions can round the exact endpoint to 1.0 in f32. Preserve
    // the original strict upper bound with the closest representable value
    // below one rather than crossing into a nonexistent next texel.
    Some(endpoint.min(f32::from_bits(1.0_f32.to_bits() - 1)))
}

fn rgb555_rgba(value: u16) -> [u8; 4] {
    let expand = |component: u16| {
        let component = component as u8;
        (component << 3) | (component >> 2)
    };
    [
        expand((value >> 10) & 0x1f),
        expand((value >> 5) & 0x1f),
        expand(value & 0x1f),
        255,
    ]
}

fn average_opaque_rgba(image: &PixelImage) -> Option<[u8; 4]> {
    let mut sum = [0_u64; 3];
    let mut count = 0_u64;
    for pixel in image.rgba.chunks_exact(4).filter(|pixel| pixel[3] != 0) {
        for channel in 0..3 {
            sum[channel] += u64::from(pixel[channel]);
        }
        count += 1;
    }
    (count != 0).then(|| {
        [
            (sum[0] / count) as u8,
            (sum[1] / count) as u8,
            (sum[2] / count) as u8,
            255,
        ]
    })
}

fn model_variant_label(index: usize, count: usize) -> String {
    if count == 0 {
        "V VARIANT: NONE".to_owned()
    } else {
        format!("V VARIANT: {} / {count}", index.min(count - 1) + 1)
    }
}

fn additional_occurrences(variant: &ModelVariant, selected_index: Option<usize>) -> String {
    variant
        .occurrences
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != selected_index)
        .map(|(_, occurrence)| format!("{}#{}", occurrence.source, occurrence.local_id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sprite_shade_label(shade: SpriteShade) -> String {
    match shade {
        SpriteShade::Base => "SHADE: BASE".to_owned(),
        SpriteShade::Level(level) => format!("SHADE: LEVEL {level}"),
    }
}

fn draw_status(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    catalog: &AssetCatalog,
    data_root: &Path,
    context: &ViewContext<'_>,
) -> Result<(), String> {
    let text = if context.tab == Tab::Models
        && context.model_presentation == ModelPresentation::CapturedDraw
    {
        context.capture.map_or_else(
            || "NO VERIFIED CAPTURE LOADED".to_owned(),
            |capture| format!("VERIFIED QUERY BUNDLE  {}", capture.bundle.display()),
        )
    } else if context.status_message.is_empty() {
        format!(
            "DIRECT OVL CATALOG  MODELS {}  VARIANTS {}  REGISTRATIONS {}  H3 {}  H7 {}  TERRAIN {}  ROOT {}",
            catalog.models.len(),
            catalog.model_variant_count(),
            catalog.model_occurrence_count(),
            catalog.sprites.len(),
            catalog.materials.len(),
            catalog.terrain.len(),
            data_root.display()
        )
    } else {
        context.status_message.to_owned()
    };
    draw_clipped_text(
        canvas,
        layout.status.x + 12,
        layout.status.y + 13,
        layout.status.width.saturating_sub(24),
        &text,
        if context.status_message.is_empty() {
            MUTED
        } else {
            WARNING
        },
    )
}

fn optional_id(value: Option<u16>) -> String {
    value
        .map(|value| format!("#{value} / 0X{value:04X}"))
        .unwrap_or_else(|| "UNMAPPED".to_owned())
}

fn empty_view(canvas: &mut Canvas<Window>, layout: &UiLayout, message: &str) -> Result<(), String> {
    draw_clipped_text(
        canvas,
        layout.viewport.x + 24,
        layout.viewport.y + layout.viewport.height as i32 / 2,
        layout.viewport.width.saturating_sub(48),
        message,
        WARNING,
    )
}

fn unavailable_composite_view(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    reason: &str,
) -> Result<(), String> {
    let center_y = layout.viewport.y + layout.viewport.height as i32 / 2;
    draw_clipped_text(
        canvas,
        layout.viewport.x + 24,
        center_y - 12,
        layout.viewport.width.saturating_sub(48),
        "PROVEN NEUTRAL COMPOSITE UNAVAILABLE",
        WARNING,
    )?;
    draw_clipped_text(
        canvas,
        layout.viewport.x + 24,
        center_y + 6,
        layout.viewport.width.saturating_sub(48),
        reason,
        MUTED,
    )
}

fn unavailable_model_view(
    canvas: &mut Canvas<Window>,
    layout: &UiLayout,
    kind: ModelDiagnosticUnavailableKind,
    reason: &str,
) -> Result<(), String> {
    let center_y = layout.viewport.y + layout.viewport.height as i32 / 2;
    draw_clipped_text(
        canvas,
        layout.viewport.x + 24,
        center_y - 12,
        layout.viewport.width.saturating_sub(48),
        if kind == ModelDiagnosticUnavailableKind::Empty {
            "INTENTIONALLY EMPTY MODEL SLOT"
        } else {
            "MODEL DIAGNOSTIC UNAVAILABLE"
        },
        if kind == ModelDiagnosticUnavailableKind::Empty {
            MUTED
        } else {
            WARNING
        },
    )?;
    draw_clipped_text(
        canvas,
        layout.viewport.x + 24,
        center_y + 6,
        layout.viewport.width.saturating_sub(48),
        reason,
        MUTED,
    )
}

const fn unavailable_kind_label(kind: ModelDiagnosticUnavailableKind) -> &'static str {
    match kind {
        ModelDiagnosticUnavailableKind::Empty => "DIAGNOSTIC: EMPTY SENTINEL",
        ModelDiagnosticUnavailableKind::NoPolygonGeometry => "DIAGNOSTIC: NO POLYGON MESH",
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry => "DIAGNOSTIC: RUNTIME GEOMETRY",
        ModelDiagnosticUnavailableKind::NestedProgram => "DIAGNOSTIC: NESTED PROGRAM",
        ModelDiagnosticUnavailableKind::ModelInstance => "DIAGNOSTIC: MODEL INSTANCE",
        ModelDiagnosticUnavailableKind::UnresolvedCommandEffects => {
            "DIAGNOSTIC: UNRESOLVED COMMANDS"
        }
    }
}

fn side_line(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: &mut i32,
    width: u32,
    text: &str,
    color: Color,
) -> Result<(), String> {
    draw_clipped_text(canvas, x, *y, width, text, color)?;
    *y += 15;
    Ok(())
}

fn draw_clipped_text(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    width: u32,
    text: &str,
    color: Color,
) -> Result<(), String> {
    let capacity = (width / 6) as usize;
    if capacity == 0 {
        return Ok(());
    }
    let character_count = text.chars().count();
    let rendered = if character_count > capacity {
        let retained = capacity.saturating_sub(3);
        let mut shortened = text.chars().take(retained).collect::<String>();
        shortened.push_str("...");
        shortened
    } else {
        text.to_owned()
    };
    draw_text(canvas, x, y, &rendered, color, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{
        ModelDiagnosticFaceLighting, ModelFanFlags, ModelInstructionKind, ModelMaterialRef,
        ModelSlotRef,
    };

    fn presentation(
        attributes: Option<Vec<ModelFanVertexAttributes>>,
        mirrored_instance: bool,
    ) -> ModelDiagnosticFacePresentation {
        ModelDiagnosticFacePresentation {
            source_opcode: if attributes.is_some() { 0x01 } else { 0xd2 },
            material: ModelMaterialRef {
                domain: ModelMaterialDomain::Handler3Texture,
                global_id: 7,
            },
            source_flags: ModelDiagnosticFaceFlags::Fan(ModelFanFlags::from_raw(8)),
            source_lighting: ModelDiagnosticFaceLighting::Shared(ModelSlotRef::from_raw(0)),
            fan_vertex_attributes: attributes,
            fan_triangle_index: None,
            mirrored_instance,
        }
    }

    #[test]
    fn local_and_composite_presentations_never_share_a_cache_entry() {
        let local = ModelPresentationCacheKey {
            model: 42,
            variant: 0,
            occurrence: Some(1),
            presentation: ModelPresentation::LocalShell,
            inspection_eye: None,
            eye_shift: 0,
            scene: SceneContext::new(17, 0, VIEWER_DEFAULT_LANGUAGE),
            mode: SpriteRenderMode::Mode0,
        };
        assert_ne!(
            local,
            ModelPresentationCacheKey {
                presentation: ModelPresentation::NeutralComposite,
                ..local
            }
        );
        let eye = ModelPresentationCacheKey {
            presentation: ModelPresentation::ExplicitEye,
            inspection_eye: Some(InspectionEye([0, 0, -1024])),
            ..local
        };
        assert_ne!(local, eye);
        assert_ne!(
            eye,
            ModelPresentationCacheKey {
                inspection_eye: Some(InspectionEye([0, 0, 1024])),
                ..eye
            }
        );
        assert_ne!(
            eye,
            ModelPresentationCacheKey {
                eye_shift: 17,
                ..eye
            }
        );
        assert_ne!(
            eye,
            ModelPresentationCacheKey {
                inspection_eye: None,
                ..eye
            }
        );
    }

    #[test]
    fn fixed_texture_uvs_match_the_original_software_backend() {
        let texture_size = [4, 2];
        let maximum_u = 1.0 - 1.0 / (texture_size[0] as f32 * 65_536.0);
        let maximum_v = 1.0 - 1.0 / (texture_size[1] as f32 * 65_536.0);
        let triangle = model_face_uvs(3, &presentation(None, false), texture_size).unwrap();
        assert_eq!(
            triangle,
            (
                vec![
                    TextureUv { u: 0.0, v: 0.0 },
                    TextureUv {
                        u: maximum_u,
                        v: 0.0,
                    },
                    TextureUv {
                        u: maximum_u,
                        v: maximum_v,
                    },
                ],
                true,
            )
        );
        let quad = model_face_uvs(4, &presentation(None, false), texture_size).unwrap();
        assert_eq!(
            quad.0[3],
            TextureUv {
                u: 0.0,
                v: maximum_v
            }
        );
        assert!(quad.1);

        let last_texel_start = (texture_size[0] - 1) as f32 / texture_size[0] as f32;
        assert!(maximum_u > last_texel_start);
        assert!(maximum_u < 1.0);
        let expected_interval =
            1.0 / texture_size[0] as f32 - 1.0 / (texture_size[0] as f32 * 65_536.0);
        assert!((maximum_u - last_texel_start - expected_interval).abs() < f32::EPSILON);
    }

    #[test]
    fn fan_uv_words_are_unsigned_q0_16_and_do_not_flip_for_mirror_pass() {
        let attributes = vec![
            ModelFanVertexAttributes::Textured {
                uv_words: [0, 0],
                lighting_and_end: [0, 0],
            },
            ModelFanVertexAttributes::Textured {
                uv_words: [u16::MAX, 0],
                lighting_and_end: [0, 0],
            },
            ModelFanVertexAttributes::Textured {
                uv_words: [u16::MAX, u16::MAX],
                lighting_and_end: [0, 0],
            },
        ];
        let normal =
            model_face_uvs(3, &presentation(Some(attributes.clone()), false), [4, 2]).unwrap();
        assert_eq!(normal.0[0], TextureUv { u: 0.0, v: 0.0 });
        let maximum = f32::from(u16::MAX) / 65_536.0;
        assert_eq!(
            normal.0[2],
            TextureUv {
                u: maximum,
                v: maximum
            }
        );
        assert!(!normal.1);

        let mirrored = model_face_uvs(3, &presentation(Some(attributes), true), [4, 2]).unwrap();
        assert_eq!(mirrored.0[0], TextureUv { u: 0.0, v: 0.0 });
        assert_eq!(
            mirrored.0[2],
            TextureUv {
                u: maximum,
                v: maximum
            }
        );
    }

    #[test]
    fn texture_fallback_ignores_masked_pixels() {
        let image = PixelImage {
            width: 2,
            height: 1,
            rgba: vec![250, 1, 1, 0, 20, 40, 80, 255],
        };
        assert_eq!(average_opaque_rgba(&image), Some([20, 40, 80, 255]));
    }

    fn polygon_materials(variant: &ModelVariant) -> Vec<ModelMaterialRef> {
        variant
            .entry
            .program
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.kind {
                ModelInstructionKind::Triangle(face) => Some(face.material),
                ModelInstructionKind::Quad(face) => Some(face.material),
                ModelInstructionKind::TriangleFan(face) => Some(face.material),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn retail_mark_and_missile_preserve_original_surface_domains_and_raw_projection_geometry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, None).unwrap();

        let mark = catalog.models[1].default_variant().unwrap();
        let ModelDiagnosticProjection::Ready(mark_geometry) = &mark.diagnostic else {
            panic!("MARK has independently initialized raw projection inputs")
        };
        assert_eq!(
            mark_geometry.completeness,
            inf_formats::ModelDiagnosticCompleteness::CompleteMesh
        );
        assert_eq!(mark_geometry.faces.len(), 30);
        let mark_materials = polygon_materials(mark);
        assert!(!mark_materials.is_empty());
        assert!(mark_materials
            .iter()
            .all(|material| material.domain == ModelMaterialDomain::Handler7Material));

        let missile = catalog.models[3].default_variant().unwrap();
        let ModelDiagnosticProjection::Ready(missile_geometry) = &missile.diagnostic else {
            panic!("MISSILE retains proven raw-projected faces")
        };
        assert!(!missile_geometry.faces.is_empty());
        assert!(polygon_materials(missile)
            .iter()
            .any(|material| material.domain == ModelMaterialDomain::Handler3Texture));
    }

    #[test]
    fn filtered_bigdish_renders_proven_geometry_with_cross_overlay_texture_resources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, Some(Path::new("17.ovl"))).unwrap();
        let bigdish = catalog
            .models
            .iter()
            .find(|asset| asset.locator.name.eq_ignore_ascii_case("BIGDISH"))
            .and_then(|asset| asset.default_variant())
            .expect("17.ovl should contain BIGDISH");
        let ModelDiagnosticProjection::Ready(geometry) = &bigdish.diagnostic else {
            panic!("BIGDISH has proven raw projection inputs")
        };
        assert_eq!(
            geometry.completeness,
            inf_formats::ModelDiagnosticCompleteness::CompleteMesh
        );
        assert_eq!(geometry.faces.len(), 46);

        let source = selected_model_source(bigdish, None)
            .map(|(_, source)| source)
            .expect("BIGDISH source");
        let cross_overlay_texture = polygon_materials(bigdish)
            .into_iter()
            .filter(|material| material.domain == ModelMaterialDomain::Handler3Texture)
            .filter_map(|material| u16::try_from(material.global_id).ok())
            .filter_map(|global_id| {
                catalog.model_texture(
                    global_id,
                    &SceneContext::new(17, 4, VIEWER_DEFAULT_LANGUAGE),
                    source,
                )
            })
            .find(|asset| !asset.source.eq_ignore_ascii_case(source))
            .expect("BIGDISH cross-overlay Handler-3 texture");
        let image = decode_sprite(
            &cross_overlay_texture.source_bytes,
            &cross_overlay_texture.entry,
            SpriteRenderMode::Mode0,
            None,
            SpriteShade::Base,
        )
        .expect("BIGDISH cross-overlay texture decodes");
        assert_eq!((image.width, image.height), (64, 64));
    }

    #[test]
    fn retail_gatebod_retains_handler3_blend_and_fan_flags_with_proven_geometry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, None).unwrap();
        let gatebod = catalog
            .models
            .iter()
            .find(|asset| asset.locator.name.eq_ignore_ascii_case("GATEBOD"))
            .and_then(|asset| asset.default_variant())
            .expect("retail catalog should contain GATEBOD");
        let ModelDiagnosticProjection::Ready(geometry) = &gatebod.diagnostic else {
            panic!("GATEBOD has proven raw projection inputs")
        };
        assert!(!geometry.faces.is_empty());

        let source = selected_model_source(gatebod, None)
            .map(|(_, source)| source)
            .expect("GATEBOD source");
        let scene = scene_for_model_source(source, 0).expect("retail OVL source parses");
        let blend_modes = polygon_materials(gatebod)
            .into_iter()
            .filter(|material| material.domain == ModelMaterialDomain::Handler3Texture)
            .filter_map(|material| u16::try_from(material.global_id).ok())
            .filter_map(|global_id| catalog.model_texture(global_id, &scene, source))
            .filter_map(|asset| {
                selected_sprite_variant(&asset.entry, SpriteRenderMode::Mode0, None)
                    .map(|variant| variant.flags.blend_mode())
            })
            .collect::<Vec<_>>();
        assert!(blend_modes.contains(&SpriteBlendMode::OpaqueOrMasked));
        assert!(blend_modes.contains(&SpriteBlendMode::Additive));

        let fan = gatebod
            .entry
            .program
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.kind {
                ModelInstructionKind::TriangleFan(fan) if fan.flags.raw() == 0x000c => Some(fan),
                _ => None,
            })
            .expect("GATEBOD textured per-triangle-cull fan command");
        assert!(fan.flags.has_textured_records());
        assert!(fan.flags.culls_each_triangle());
        assert!(!fan.flags.reverse_cull_direction());
    }
}
