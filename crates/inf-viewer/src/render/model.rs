//! Model projection, presentation, and software rasterization.

use std::collections::BTreeSet;
use std::sync::Arc;

use inf_formats::{ModelDiagnosticGeometry, SpriteBlendMode};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Point;
use sdl2::render::{BlendMode, Canvas, ClippingRect};
use sdl2::video::Window;

use super::PixelImage;
use crate::camera::OrbitCamera;
use crate::ui::UiRect;

/// Orthographically projected model vertex in raster-local pixel coordinates.
///
/// A lower `depth` value is closer to the viewer. Keeping depth separate from
/// SDL's integer [`Point`] lets the diagnostic renderer resolve overlapping
/// model faces before uploading the resulting image to the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedModelVertex {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
}

/// Texture coordinate normalized across the complete source image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureUv {
    pub u: f32,
    pub v: f32,
}

/// A projected vertex with the texture coordinate used by a textured face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexturedModelVertex {
    pub position: ProjectedModelVertex,
    pub uv: TextureUv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFaceCull {
    mode: ModelCullMode,
    reverse: bool,
    mirror_pass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelCullMode {
    Disabled,
    PerFace,
    /// Handler-8 fan bit 1 tests the first triangle and uses that result for
    /// the complete fan. If bit 2 is also present, later triangles resume
    /// individual tests and the first rejection terminates the fan.
    FanGate {
        triangle_index: u16,
        cull_remaining: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FanCullState {
    Unset,
    Proceeding,
    Halted,
}

impl ModelFaceCull {
    pub const DISABLED: Self = Self {
        mode: ModelCullMode::Disabled,
        reverse: false,
        mirror_pass: false,
    };

    pub const fn new(enabled: bool, reverse: bool, mirror_pass: bool) -> Self {
        Self {
            mode: if enabled {
                ModelCullMode::PerFace
            } else {
                ModelCullMode::Disabled
            },
            reverse,
            mirror_pass,
        }
    }

    pub const fn fan_gate(
        reverse: bool,
        mirror_pass: bool,
        triangle_index: u16,
        cull_remaining: bool,
    ) -> Self {
        Self {
            mode: ModelCullMode::FanGate {
                triangle_index,
                cull_remaining,
            },
            reverse,
            mirror_pass,
        }
    }

    fn accepts(
        self,
        vertices: &[ProjectedModelVertex],
        fan_cull_state: &mut [FanCullState; 2],
    ) -> bool {
        match self.mode {
            ModelCullMode::Disabled => true,
            ModelCullMode::PerFace => self.accepts_winding(vertices),
            ModelCullMode::FanGate {
                triangle_index: 0, ..
            } => {
                let accepted = self.accepts_winding(vertices);
                fan_cull_state[usize::from(self.mirror_pass)] = if accepted {
                    FanCullState::Proceeding
                } else {
                    FanCullState::Halted
                };
                accepted
            }
            ModelCullMode::FanGate { cull_remaining, .. } => {
                let state = &mut fan_cull_state[usize::from(self.mirror_pass)];
                match *state {
                    FanCullState::Proceeding if cull_remaining => {
                        let accepted = self.accepts_winding(vertices);
                        if !accepted {
                            *state = FanCullState::Halted;
                        }
                        accepted
                    }
                    FanCullState::Proceeding => true,
                    FanCullState::Unset | FanCullState::Halted => false,
                }
            }
        }
    }

    fn accepts_winding(self, vertices: &[ProjectedModelVertex]) -> bool {
        let Some([first, second, third]) = vertices.get(..3) else {
            return false;
        };
        // The orbit projection looks toward the origin from positive view Z
        // and already reflects screen Y. In that convention an outward face
        // has a positive edge value; applying another determinant sign flip
        // selects the inside/back shell instead.
        let front_facing = edge_function(*first, *second, third.x, third.y) > 0.0;
        (front_facing ^ self.reverse) != self.mirror_pass
    }
}

/// Resolved presentation for one diagnostic face.
#[derive(Clone, Debug)]
pub struct ModelFaceSurface {
    pub paint: ModelFacePaint,
    pub cull: ModelFaceCull,
}

impl ModelFaceSurface {
    pub const fn unresolved() -> Self {
        Self {
            paint: ModelFacePaint::Unresolved,
            cull: ModelFaceCull::DISABLED,
        }
    }
}

/// Handler-7 faces become exact RGB555 solids. Handler-3 faces retain their
/// decoded source image and per-corner UVs; `implicit_uv` distinguishes the
/// backend-generated triangle/quad coordinates from explicit fan UV words.
#[derive(Clone, Debug)]
pub enum ModelFacePaint {
    Solid {
        rgba: [u8; 4],
    },
    Textured {
        image: Arc<PixelImage>,
        uvs: Vec<TextureUv>,
        fallback_rgba: [u8; 4],
        implicit_uv: bool,
        /// Handler-3's recovered fixed-function blend state. Source-alpha
        /// remains unresolved because the current decoder only proves binary
        /// mask alpha, not authored per-pixel alpha.
        blend_mode: SpriteBlendMode,
    },
    Unresolved,
}

/// CPU color/depth target used by the Infestation model workbench.
///
/// This deliberately owns no SDL resources. Recovered model presentation can
/// therefore be tested independently of a window, and the caller can upload
/// [`Self::into_image`] through the same path used for sprite diagnostics.
#[derive(Clone, Debug)]
pub struct ModelRaster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    depth: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FragmentBlend {
    Replace,
    Additive,
    DarkenByInverseSourceColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FragmentPolicy {
    blend: FragmentBlend,
    write_depth: bool,
}

impl FragmentPolicy {
    const OPAQUE_OR_MASKED: Self = Self {
        blend: FragmentBlend::Replace,
        write_depth: true,
    };

    /// Recovered Handler-3 surface policy. Transparent effects are tested
    /// against the existing depth target but do not claim it for later faces.
    const fn for_sprite_blend(blend_mode: SpriteBlendMode) -> Option<Self> {
        match blend_mode {
            SpriteBlendMode::OpaqueOrMasked => Some(Self::OPAQUE_OR_MASKED),
            SpriteBlendMode::Additive => Some(Self {
                blend: FragmentBlend::Additive,
                write_depth: false,
            }),
            SpriteBlendMode::DarkenByInverseSourceColor => Some(Self {
                blend: FragmentBlend::DarkenByInverseSourceColor,
                write_depth: false,
            }),
            SpriteBlendMode::SourceAlpha => None,
        }
    }
}

impl ModelRaster {
    pub fn new(width: u32, height: u32, clear_rgba: [u8; 4]) -> Self {
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .expect("model raster dimensions fit in memory");
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for _ in 0..pixel_count {
            rgba.extend_from_slice(&clear_rgba);
        }
        Self {
            width,
            height,
            rgba,
            depth: vec![f32::INFINITY; pixel_count],
        }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixel_rgba(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let offset = self.pixel_offset(x, y)?;
        Some(
            self.rgba[offset..offset + 4]
                .try_into()
                .expect("RGBA pixel"),
        )
    }

    pub fn pixel_depth(&self, x: u32, y: u32) -> Option<f32> {
        let index = self.pixel_index(x, y)?;
        Some(self.depth[index])
    }

    pub fn into_image(self) -> PixelImage {
        PixelImage {
            width: self.width,
            height: self.height,
            rgba: self.rgba,
        }
    }

    /// Rasterize one solid triangle, returning the number of fragments that
    /// passed alpha and depth testing.
    pub fn rasterize_solid_triangle(
        &mut self,
        vertices: [ProjectedModelVertex; 3],
        rgba: [u8; 4],
    ) -> usize {
        self.rasterize_triangle(vertices, FragmentPolicy::OPAQUE_OR_MASKED, |_| {
            (rgba[3] != 0).then_some(rgba)
        })
    }

    /// Rasterize a convex polygon using the archive's fan winding.
    pub fn rasterize_solid_polygon(
        &mut self,
        vertices: &[ProjectedModelVertex],
        rgba: [u8; 4],
        quad_split: QuadSplit,
    ) -> usize {
        triangulate_indices(vertices.len(), quad_split)
            .into_iter()
            .map(|triangle| {
                self.rasterize_solid_triangle(
                    [
                        vertices[triangle[0]],
                        vertices[triangle[1]],
                        vertices[triangle[2]],
                    ],
                    rgba,
                )
            })
            .sum()
    }

    /// Rasterize one textured triangle with nearest-neighbour sampling.
    ///
    /// Texture alpha zero is a cutout: it writes neither color nor depth, so a
    /// farther face remains visible through transparent sprite texels.
    /// Source-alpha surfaces remain unresolved because decoded Handler-3
    /// pixels currently prove only binary mask alpha.
    pub fn rasterize_textured_triangle(
        &mut self,
        vertices: [TexturedModelVertex; 3],
        texture: &PixelImage,
        blend_mode: SpriteBlendMode,
    ) -> usize {
        let Some(policy) = FragmentPolicy::for_sprite_blend(blend_mode) else {
            return 0;
        };
        let positions = vertices.map(|vertex| vertex.position);
        self.rasterize_triangle(positions, policy, |weights| {
            let uv = TextureUv {
                u: vertices[0].uv.u * weights[0]
                    + vertices[1].uv.u * weights[1]
                    + vertices[2].uv.u * weights[2],
                v: vertices[0].uv.v * weights[0]
                    + vertices[1].uv.v * weights[1]
                    + vertices[2].uv.v * weights[2],
            };
            let rgba = sample_texture_nearest(texture, uv)?;
            (rgba[3] != 0).then_some(rgba)
        })
    }

    fn rasterize_textured_fallback_polygon(
        &mut self,
        vertices: &[ProjectedModelVertex],
        rgba: [u8; 4],
        blend_mode: SpriteBlendMode,
        quad_split: QuadSplit,
    ) -> usize {
        let Some(policy) = FragmentPolicy::for_sprite_blend(blend_mode) else {
            return 0;
        };
        triangulate_indices(vertices.len(), quad_split)
            .into_iter()
            .map(|triangle| {
                self.rasterize_triangle(
                    [
                        vertices[triangle[0]],
                        vertices[triangle[1]],
                        vertices[triangle[2]],
                    ],
                    policy,
                    |_| (rgba[3] != 0).then_some(rgba),
                )
            })
            .sum()
    }

    fn rasterize_triangle<F>(
        &mut self,
        vertices: [ProjectedModelVertex; 3],
        policy: FragmentPolicy,
        mut shade: F,
    ) -> usize
    where
        F: FnMut([f32; 3]) -> Option<[u8; 4]>,
    {
        if self.width == 0
            || self.height == 0
            || vertices
                .iter()
                .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite())
        {
            return 0;
        }

        let area = edge_function(vertices[0], vertices[1], vertices[2].x, vertices[2].y);
        if !area.is_finite() || area.abs() <= f32::EPSILON {
            return 0;
        }

        let minimum_x = vertices
            .iter()
            .map(|vertex| vertex.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let maximum_x = vertices
            .iter()
            .map(|vertex| vertex.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.width.saturating_sub(1) as f32) as u32;
        let minimum_y = vertices
            .iter()
            .map(|vertex| vertex.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let maximum_y = vertices
            .iter()
            .map(|vertex| vertex.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.height.saturating_sub(1) as f32) as u32;
        if minimum_x > maximum_x || minimum_y > maximum_y {
            return 0;
        }

        let mut written = 0;
        for y in minimum_y..=maximum_y {
            for x in minimum_x..=maximum_x {
                let sample_x = x as f32 + 0.5;
                let sample_y = y as f32 + 0.5;
                let weights = [
                    edge_function(vertices[1], vertices[2], sample_x, sample_y) / area,
                    edge_function(vertices[2], vertices[0], sample_x, sample_y) / area,
                    edge_function(vertices[0], vertices[1], sample_x, sample_y) / area,
                ];
                if weights.iter().any(|weight| *weight < -1.0e-5) {
                    continue;
                }
                let depth = vertices[0].depth * weights[0]
                    + vertices[1].depth * weights[1]
                    + vertices[2].depth * weights[2];
                let Some(rgba) = shade(weights) else {
                    continue;
                };
                if self.write_fragment(x, y, depth, rgba, policy) {
                    written += 1;
                }
            }
        }
        written
    }

    fn write_fragment(
        &mut self,
        x: u32,
        y: u32,
        depth: f32,
        rgba: [u8; 4],
        policy: FragmentPolicy,
    ) -> bool {
        if rgba[3] == 0 || !depth.is_finite() {
            return false;
        }
        let Some(index) = self.pixel_index(x, y) else {
            return false;
        };
        // The original renderer uses LEQUAL: later coplanar command layers
        // must be allowed to decorate an earlier base polygon.
        if depth > self.depth[index] {
            return false;
        }
        let offset = index * 4;
        let destination = self.rgba[offset..offset + 4]
            .try_into()
            .expect("RGBA pixel");
        let result = blend_fragment(destination, rgba, policy.blend);
        self.rgba[offset..offset + 4].copy_from_slice(&result);
        if policy.write_depth {
            self.depth[index] = depth;
        }
        true
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width && y < self.height).then_some(y as usize * self.width as usize + x as usize)
    }

    fn pixel_offset(&self, x: u32, y: u32) -> Option<usize> {
        self.pixel_index(x, y).map(|index| index * 4)
    }
}

fn blend_fragment(destination: [u8; 4], source: [u8; 4], blend: FragmentBlend) -> [u8; 4] {
    match blend {
        FragmentBlend::Replace => source,
        FragmentBlend::Additive => [
            destination[0].saturating_add(source[0]),
            destination[1].saturating_add(source[1]),
            destination[2].saturating_add(source[2]),
            destination[3].saturating_add(source[3]),
        ],
        FragmentBlend::DarkenByInverseSourceColor => [
            multiply_u8(destination[0], u8::MAX - source[0]),
            multiply_u8(destination[1], u8::MAX - source[1]),
            multiply_u8(destination[2], u8::MAX - source[2]),
            destination[3],
        ],
    }
}

fn multiply_u8(left: u8, right: u8) -> u8 {
    ((u16::from(left) * u16::from(right) + 127) / 255) as u8
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelRenderStats {
    pub segments: usize,
    pub ignored_edge_commands: usize,
    pub solid_faces: usize,
    pub textured_faces: usize,
    pub implicit_uv_faces: usize,
    pub unresolved_faces: usize,
    pub culled_faces: usize,
    pub fragments: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuadSplit {
    #[default]
    Diagonal02,
    Diagonal13,
}

impl QuadSplit {
    pub const fn next(self) -> Self {
        match self {
            Self::Diagonal02 => Self::Diagonal13,
            Self::Diagonal13 => Self::Diagonal02,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Diagonal02 => "0-2",
            Self::Diagonal13 => "1-3",
        }
    }
}

/// Split a face into the triangle fan used by the original model commands.
pub fn triangulate_face(face: &[usize]) -> Vec<[usize; 3]> {
    if face.len() < 3 {
        return Vec::new();
    }
    (1..face.len() - 1)
        .map(|index| [face[0], face[index], face[index + 1]])
        .collect()
}

fn triangulate_indices(vertex_count: usize, quad_split: QuadSplit) -> Vec<[usize; 3]> {
    if vertex_count == 4 && quad_split == QuadSplit::Diagonal13 {
        return vec![[1, 2, 3], [1, 3, 0]];
    }
    let indices = (0..vertex_count).collect::<Vec<_>>();
    triangulate_face(&indices)
}

/// Return each topology edge once, independent of the direction in which its
/// adjacent faces reference it.
pub fn deduplicated_face_edges(faces: &[Vec<usize>]) -> Vec<[usize; 2]> {
    let mut edges = BTreeSet::new();
    for face in faces {
        if face.len() < 2 {
            continue;
        }
        for index in 0..face.len() {
            let start = face[index];
            let end = face[(index + 1) % face.len()];
            if start != end {
                edges.insert([start.min(end), start.max(end)]);
            }
        }
    }
    edges.into_iter().collect()
}

/// Project all diagnostic vertices into a viewport-local orthographic raster.
pub fn project_model_geometry(
    geometry: &ModelDiagnosticGeometry,
    viewport_size: [u32; 2],
    camera: &OrbitCamera,
) -> Vec<ProjectedModelVertex> {
    if geometry.vertices.is_empty() {
        return Vec::new();
    }

    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for vertex in &geometry.vertices {
        for (axis, coordinate) in vertex.position.iter().enumerate() {
            let coordinate = *coordinate as f32;
            minimum[axis] = minimum[axis].min(coordinate);
            maximum[axis] = maximum[axis].max(coordinate);
        }
    }
    let center = [
        (minimum[0] + maximum[0]) * 0.5,
        (minimum[1] + maximum[1]) * 0.5,
        (minimum[2] + maximum[2]) * 0.5,
    ];
    let extent = (0..3)
        .map(|axis| maximum[axis] - minimum[axis])
        .fold(1.0_f32, f32::max);
    let fit = viewport_size[0].min(viewport_size[1]) as f32 * 0.78 / extent;
    let scale = fit * camera.zoom;

    geometry
        .vertices
        .iter()
        .map(|vertex| {
            project_model_vertex(
                vertex.position.map(|coordinate| coordinate as f32),
                center,
                viewport_size,
                camera,
                scale,
            )
        })
        .collect()
}

/// Apply the current orbit camera while retaining view-space depth.
pub fn project_model_vertex(
    point: [f32; 3],
    center: [f32; 3],
    viewport_size: [u32; 2],
    camera: &OrbitCamera,
    scale: f32,
) -> ProjectedModelVertex {
    let x = point[0] - center[0];
    let y = point[1] - center[1];
    let z = point[2] - center[2];
    let (yaw_sine, yaw_cosine) = camera.yaw.sin_cos();
    let yaw_x = yaw_cosine * x + yaw_sine * z;
    let yaw_z = -yaw_sine * x + yaw_cosine * z;
    let (pitch_sine, pitch_cosine) = camera.pitch.sin_cos();
    let pitch_y = pitch_cosine * y - pitch_sine * yaw_z;
    let pitch_z = pitch_sine * y + pitch_cosine * yaw_z;
    ProjectedModelVertex {
        x: viewport_size[0] as f32 * 0.5 + yaw_x * scale + camera.pan_x,
        y: viewport_size[1] as f32 * 0.5 - pitch_y * scale + camera.pan_y,
        // The camera observes the origin from positive view Z, so negate view
        // Z to give the rasterizer its conventional "smaller is nearer" depth.
        depth: -pitch_z,
    }
}

pub fn draw_model(
    canvas: &mut Canvas<Window>,
    viewport: UiRect,
    geometry: &ModelDiagnosticGeometry,
    camera: &OrbitCamera,
) -> Result<ModelRenderStats, String> {
    let edges = deduplicated_face_edges(&geometry.faces);
    if edges.is_empty() {
        return Ok(ModelRenderStats {
            segments: 0,
            ignored_edge_commands: geometry.ignored_edge_commands,
            ..ModelRenderStats::default()
        });
    }

    let projected = project_model_geometry(geometry, [viewport.width, viewport.height], camera);
    draw_wire_edges(
        canvas,
        viewport,
        &projected,
        &edges,
        Color::RGB(91, 210, 234),
    )?;

    Ok(ModelRenderStats {
        segments: edges.len(),
        ignored_edge_commands: geometry.ignored_edge_commands,
        unresolved_faces: geometry.faces.len(),
        ..ModelRenderStats::default()
    })
}

/// Draw recovered Handler-7 fills and Handler-3 textures into a software
/// color/depth target, then overlay only faces whose resource could not be
/// resolved. Unsupported presentation remains visible without turning every
/// successfully recovered model back into a wireframe.
pub fn draw_presented_model(
    canvas: &mut Canvas<Window>,
    viewport: UiRect,
    geometry: &ModelDiagnosticGeometry,
    camera: &OrbitCamera,
    surfaces: &[ModelFaceSurface],
    show_textures: bool,
    quad_split: QuadSplit,
) -> Result<ModelRenderStats, String> {
    let frame = rasterize_presented_model(
        geometry,
        camera,
        [viewport.width, viewport.height],
        surfaces,
        show_textures,
        quad_split,
    );
    draw_raster_image(canvas, viewport, &frame.image)?;
    if !frame.unresolved_edges.is_empty() {
        draw_wire_edges(
            canvas,
            viewport,
            &frame.projected,
            &frame.unresolved_edges,
            Color::RGB(91, 210, 234),
        )?;
    }
    Ok(frame.stats)
}

#[derive(Debug)]
struct PresentedModelFrame {
    image: PixelImage,
    projected: Vec<ProjectedModelVertex>,
    unresolved_edges: Vec<[usize; 2]>,
    stats: ModelRenderStats,
}

fn rasterize_presented_model(
    geometry: &ModelDiagnosticGeometry,
    camera: &OrbitCamera,
    viewport_size: [u32; 2],
    surfaces: &[ModelFaceSurface],
    show_textures: bool,
    quad_split: QuadSplit,
) -> PresentedModelFrame {
    let projected = project_model_geometry(geometry, viewport_size, camera);
    let mut raster = ModelRaster::new(viewport_size[0], viewport_size[1], [0, 0, 0, 0]);
    let mut stats = ModelRenderStats {
        ignored_edge_commands: geometry.ignored_edge_commands,
        ..ModelRenderStats::default()
    };
    let mut unresolved_faces = Vec::new();
    // Fan bit 1 gates an entire source fan from its first triangle. Mirror
    // instances are independent passes, so retain one gate result per pass.
    let mut fan_cull_state = [FanCullState::Unset; 2];

    for (face_index, face) in geometry.faces.iter().enumerate() {
        let Some(surface) = surfaces.get(face_index) else {
            stats.unresolved_faces += 1;
            unresolved_faces.push(face.clone());
            continue;
        };
        let Some(vertices) = face
            .iter()
            .map(|&index| projected.get(index).copied())
            .collect::<Option<Vec<_>>>()
        else {
            stats.unresolved_faces += 1;
            unresolved_faces.push(face.clone());
            continue;
        };

        if !surface.cull.accepts(&vertices, &mut fan_cull_state) {
            stats.culled_faces += 1;
            continue;
        }

        match &surface.paint {
            ModelFacePaint::Solid { rgba } => {
                stats.solid_faces += 1;
                stats.fragments += raster.rasterize_solid_polygon(&vertices, *rgba, quad_split);
            }
            ModelFacePaint::Textured {
                blend_mode: SpriteBlendMode::SourceAlpha,
                ..
            } => {
                stats.unresolved_faces += 1;
                unresolved_faces.push(face.clone());
            }
            ModelFacePaint::Textured {
                image,
                uvs,
                implicit_uv,
                blend_mode,
                ..
            } if show_textures && uvs.len() == face.len() => {
                stats.textured_faces += 1;
                stats.implicit_uv_faces += usize::from(*implicit_uv);
                for triangle in triangulate_indices(face.len(), quad_split) {
                    stats.fragments += raster.rasterize_textured_triangle(
                        triangle.map(|corner| TexturedModelVertex {
                            position: vertices[corner],
                            uv: uvs[corner],
                        }),
                        image,
                        *blend_mode,
                    );
                }
            }
            ModelFacePaint::Textured {
                fallback_rgba,
                blend_mode,
                ..
            } => {
                stats.solid_faces += 1;
                stats.fragments += raster.rasterize_textured_fallback_polygon(
                    &vertices,
                    *fallback_rgba,
                    *blend_mode,
                    quad_split,
                );
            }
            ModelFacePaint::Unresolved => {
                stats.unresolved_faces += 1;
                unresolved_faces.push(face.clone());
            }
        }
    }

    let unresolved_edges = deduplicated_face_edges(&unresolved_faces);
    stats.segments = unresolved_edges.len();
    PresentedModelFrame {
        image: raster.into_image(),
        projected,
        unresolved_edges,
        stats,
    }
}

fn draw_wire_edges(
    canvas: &mut Canvas<Window>,
    viewport: UiRect,
    projected: &[ProjectedModelVertex],
    edges: &[[usize; 2]],
    color: Color,
) -> Result<(), String> {
    let previous_clip = canvas.clip_rect();
    canvas.set_clip_rect(model_clip_rect(previous_clip, viewport));
    canvas.set_draw_color(color);
    let result = edges.iter().try_for_each(|edge| {
        let start = projected[edge[0]];
        let end = projected[edge[1]];
        canvas.draw_line(
            projected_point(start, viewport),
            projected_point(end, viewport),
        )
    });
    canvas.set_clip_rect(previous_clip);
    result
}

fn draw_raster_image(
    canvas: &mut Canvas<Window>,
    viewport: UiRect,
    image: &PixelImage,
) -> Result<(), String> {
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, image.width, image.height)
        .map_err(|error| error.to_string())?;
    texture.set_blend_mode(BlendMode::Blend);
    texture
        .update(None, &image.rgba, image.width as usize * 4)
        .map_err(|error| error.to_string())?;
    canvas.copy(&texture, None, viewport.as_sdl())
}

fn model_clip_rect(previous: ClippingRect, viewport: UiRect) -> ClippingRect {
    previous.intersection(viewport.as_sdl().into())
}

fn projected_point(vertex: ProjectedModelVertex, viewport: UiRect) -> Point {
    let center_x = viewport.width as f32 * 0.5;
    let center_y = viewport.height as f32 * 0.5;
    Point::new(
        viewport.x + viewport.width as i32 / 2 + (vertex.x - center_x) as i32,
        viewport.y + viewport.height as i32 / 2 + (vertex.y - center_y) as i32,
    )
}

fn edge_function(start: ProjectedModelVertex, end: ProjectedModelVertex, x: f32, y: f32) -> f32 {
    (x - start.x) * (end.y - start.y) - (y - start.y) * (end.x - start.x)
}

fn sample_texture_nearest(texture: &PixelImage, uv: TextureUv) -> Option<[u8; 4]> {
    if texture.width == 0
        || texture.height == 0
        || !uv.u.is_finite()
        || !uv.v.is_finite()
        || texture.rgba.len() < texture.width as usize * texture.height as usize * 4
    {
        return None;
    }
    // Original software samplers use the integer part of unsigned 16.16
    // texel coordinates. Fixed faces remain strictly below normalized 1.0;
    // clamping here is only a defensive boundary for diagnostic inputs.
    let x = (uv.u.clamp(0.0, 1.0) * texture.width as f32).floor() as usize;
    let y = (uv.v.clamp(0.0, 1.0) * texture.height as f32).floor() as usize;
    let x = x.min(texture.width.saturating_sub(1) as usize);
    let y = y.min(texture.height.saturating_sub(1) as usize);
    let offset = (y * texture.width as usize + x) * 4;
    Some(
        texture.rgba[offset..offset + 4]
            .try_into()
            .expect("validated texture pixel"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{ModelDiagnosticCompleteness, ModelDiagnosticVertex, ModelSlotRef};
    use sdl2::rect::Rect;

    #[test]
    fn model_projection_uses_only_shared_diagnostic_geometry() {
        let geometry = ModelDiagnosticGeometry {
            view_plane_decisions: Vec::new(),
            vertices: vec![
                ModelDiagnosticVertex {
                    slot: ModelSlotRef::from_raw(0),
                    position: [10, 0, 0],
                },
                ModelDiagnosticVertex {
                    slot: ModelSlotRef::from_raw(2),
                    position: [20, 1, 0],
                },
                ModelDiagnosticVertex {
                    slot: ModelSlotRef::from_raw(4),
                    position: [30, 0, 1],
                },
            ],
            faces: vec![vec![0, 1, 2]],
            face_presentation: Vec::new(),
            ignored_edge_commands: 4,
            completeness: ModelDiagnosticCompleteness::CompleteMesh,
            unresolved_dependencies: Vec::new(),
            omitted_effects: Vec::new(),
        };

        let camera = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let projected = project_model_geometry(&geometry, [100, 100], &camera);
        assert_eq!(projected.len(), 3);
        assert!((projected[0].x - 11.0).abs() < 0.001);
        assert!((projected[1].x - 50.0).abs() < 0.001);
        assert!((projected[2].x - 89.0).abs() < 0.001);
        assert_eq!(projected[0].depth, 0.5);
        assert_eq!(projected[2].depth, -0.5);
    }

    #[test]
    fn shared_wireframe_edges_are_deduplicated_without_direction() {
        let edges = deduplicated_face_edges(&[vec![0, 1, 2], vec![2, 1, 3]]);
        assert_eq!(edges, vec![[0, 1], [0, 2], [1, 2], [1, 3], [2, 3]]);
    }

    #[test]
    fn face_triangulation_preserves_fan_winding() {
        assert_eq!(
            triangulate_face(&[4, 8, 12, 16]),
            vec![[4, 8, 12], [4, 12, 16]]
        );
        assert!(triangulate_face(&[4, 8]).is_empty());
        assert_eq!(
            triangulate_indices(4, QuadSplit::Diagonal02),
            [[0, 1, 2], [0, 2, 3]]
        );
        assert_eq!(
            triangulate_indices(4, QuadSplit::Diagonal13),
            [[1, 2, 3], [1, 3, 0]]
        );
    }

    #[test]
    fn face_culling_uses_the_orbit_cameras_view_side_and_archive_xors() {
        let back_facing = covering_triangle(0.0);
        let front_facing = [back_facing[0], back_facing[2], back_facing[1]];
        assert!(cull_accepts(
            ModelFaceCull::new(true, false, false),
            &front_facing
        ));
        assert!(!cull_accepts(
            ModelFaceCull::new(true, true, false),
            &front_facing
        ));
        assert!(!cull_accepts(
            ModelFaceCull::new(true, false, true),
            &front_facing
        ));
        assert!(cull_accepts(
            ModelFaceCull::new(true, false, true),
            &back_facing
        ));
        assert!(cull_accepts(ModelFaceCull::DISABLED, &back_facing));
    }

    #[test]
    fn gunmain_outward_face_is_visible_from_the_default_camera() {
        // Retail GUNMAIN01 slots [14, 22, 12]. Their positive XYZ normal
        // faces the default orbit camera, which also lies in +X/+Y/+Z.
        let camera = OrbitCamera::default();
        let vertices = [[39.0, -21.0, 138.0], [83.0, 36.0, 76.0], [0.0, 81.0, 72.0]]
            .map(|point| project_model_vertex(point, [0.0; 3], [1_000, 1_000], &camera, 1.0));
        assert!(edge_function(vertices[0], vertices[1], vertices[2].x, vertices[2].y) > 0.0);
        assert!(cull_accepts(
            ModelFaceCull::new(true, false, false),
            &vertices
        ));
    }

    #[test]
    fn fan_first_triangle_gate_and_per_triangle_mode_are_distinct() {
        let back_facing = covering_triangle(0.0);
        let front_facing = [back_facing[0], back_facing[2], back_facing[1]];

        let mut accepted_gate = [FanCullState::Unset; 2];
        assert!(ModelFaceCull::fan_gate(false, false, 0, false)
            .accepts(&front_facing, &mut accepted_gate));
        assert!(ModelFaceCull::fan_gate(false, false, 1, false)
            .accepts(&back_facing, &mut accepted_gate));

        let mut rejected_gate = [FanCullState::Unset; 2];
        assert!(!ModelFaceCull::fan_gate(false, false, 0, false)
            .accepts(&back_facing, &mut rejected_gate));
        assert!(!ModelFaceCull::fan_gate(false, false, 1, false)
            .accepts(&front_facing, &mut rejected_gate));

        let mut gate_then_per_face = [FanCullState::Unset; 2];
        assert!(ModelFaceCull::fan_gate(false, false, 0, true)
            .accepts(&front_facing, &mut gate_then_per_face));
        assert!(!ModelFaceCull::fan_gate(false, false, 1, true)
            .accepts(&back_facing, &mut gate_then_per_face));
        assert!(!ModelFaceCull::fan_gate(false, false, 2, true)
            .accepts(&front_facing, &mut gate_then_per_face));
    }

    #[test]
    fn solid_rasterization_keeps_the_nearest_fragment() {
        let mut raster = ModelRaster::new(4, 4, [0, 0, 0, 0]);
        raster.rasterize_solid_triangle(covering_triangle(10.0), [200, 20, 10, 255]);
        raster.rasterize_solid_triangle(covering_triangle(1.0), [10, 200, 20, 255]);
        raster.rasterize_solid_triangle(covering_triangle(20.0), [10, 20, 200, 255]);

        assert_eq!(raster.pixel_rgba(1, 1), Some([10, 200, 20, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(1.0));
    }

    #[test]
    fn transparent_texture_texels_write_neither_color_nor_depth() {
        let texture = PixelImage {
            width: 1,
            height: 1,
            rgba: vec![240, 10, 20, 0],
        };
        let mut raster = ModelRaster::new(4, 4, [3, 4, 5, 255]);
        let textured = covering_triangle(1.0).map(|position| TexturedModelVertex {
            position,
            uv: TextureUv { u: 0.0, v: 0.0 },
        });

        assert_eq!(
            raster
                .rasterize_textured_triangle(textured, &texture, SpriteBlendMode::OpaqueOrMasked,),
            0
        );
        raster.rasterize_solid_triangle(covering_triangle(5.0), [20, 40, 80, 255]);

        assert_eq!(raster.pixel_rgba(1, 1), Some([20, 40, 80, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(5.0));
    }

    #[test]
    fn opaque_texture_uses_lequal_and_writes_depth() {
        let texture = one_pixel_texture([20, 200, 40, 255]);
        let mut raster = ModelRaster::new(4, 4, [0, 0, 0, 0]);
        raster.rasterize_solid_triangle(covering_triangle(1.0), [200, 20, 10, 255]);

        assert_ne!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(1.0),
                &texture,
                SpriteBlendMode::OpaqueOrMasked,
            ),
            0
        );
        raster.rasterize_solid_triangle(covering_triangle(2.0), [10, 20, 200, 255]);

        assert_eq!(raster.pixel_rgba(1, 1), Some([20, 200, 40, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(1.0));
    }

    #[test]
    fn additive_texture_blends_at_equal_depth_without_writing_depth() {
        let texture = one_pixel_texture([20, 30, 250, 255]);
        let mut raster = ModelRaster::new(4, 4, [0, 0, 0, 0]);
        raster.rasterize_solid_triangle(covering_triangle(1.0), [250, 10, 20, 255]);

        assert_ne!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(1.0),
                &texture,
                SpriteBlendMode::Additive,
            ),
            0
        );
        assert_eq!(raster.pixel_rgba(1, 1), Some([255, 40, 255, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(1.0));

        assert_ne!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(0.5),
                &one_pixel_texture([0, 1, 0, 255]),
                SpriteBlendMode::Additive,
            ),
            0
        );
        assert_eq!(raster.pixel_depth(1, 1), Some(1.0));

        let before = raster.pixel_rgba(1, 1);
        assert_eq!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(2.0),
                &texture,
                SpriteBlendMode::Additive,
            ),
            0
        );
        assert_eq!(raster.pixel_rgba(1, 1), before);
    }

    #[test]
    fn inverse_source_color_texture_darkens_without_writing_depth() {
        let texture = one_pixel_texture([64, 128, 255, 255]);
        let mut raster = ModelRaster::new(4, 4, [0, 0, 0, 0]);
        raster.rasterize_solid_triangle(covering_triangle(1.0), [200, 100, 50, 255]);

        assert_ne!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(0.5),
                &texture,
                SpriteBlendMode::DarkenByInverseSourceColor,
            ),
            0
        );

        assert_eq!(raster.pixel_rgba(1, 1), Some([150, 50, 0, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(1.0));
    }

    #[test]
    fn source_alpha_texture_remains_explicitly_unsupported() {
        let texture = one_pixel_texture([200, 100, 50, 255]);
        let mut raster = ModelRaster::new(4, 4, [3, 4, 5, 255]);

        assert_eq!(
            raster.rasterize_textured_triangle(
                covering_textured_triangle(1.0),
                &texture,
                SpriteBlendMode::SourceAlpha,
            ),
            0
        );
        assert_eq!(raster.pixel_rgba(1, 1), Some([3, 4, 5, 255]));
        assert_eq!(raster.pixel_depth(1, 1), Some(f32::INFINITY));
    }

    #[test]
    fn normalized_uv_corners_select_the_four_texture_corners() {
        let texture = PixelImage {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        };

        assert_eq!(
            sample_texture_nearest(&texture, TextureUv { u: 0.0, v: 0.0 }),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            sample_texture_nearest(&texture, TextureUv { u: 1.0, v: 0.0 }),
            Some([0, 255, 0, 255])
        );
        assert_eq!(
            sample_texture_nearest(&texture, TextureUv { u: 0.0, v: 1.0 }),
            Some([0, 0, 255, 255])
        );
        assert_eq!(
            sample_texture_nearest(&texture, TextureUv { u: 1.0, v: 1.0 }),
            Some([255, 255, 255, 255])
        );
    }

    #[test]
    fn q0_16_sampling_uses_texel_integer_part_like_the_software_backend() {
        let texture = PixelImage {
            width: 4,
            height: 1,
            rgba: vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255],
        };
        assert_eq!(
            sample_texture_nearest(
                &texture,
                TextureUv {
                    u: 16_384.0 / 65_536.0,
                    v: 0.0,
                },
            ),
            Some([20, 0, 0, 255])
        );
        assert_eq!(
            sample_texture_nearest(
                &texture,
                TextureUv {
                    u: f32::from(u16::MAX) / 65_536.0,
                    v: 0.0,
                },
            ),
            Some([40, 0, 0, 255])
        );
    }

    #[test]
    fn model_clip_intersects_viewport_with_existing_clip() {
        let viewport = UiRect::new(10, 20, 100, 80);
        assert_eq!(
            model_clip_rect(ClippingRect::None, viewport),
            ClippingRect::Some(Rect::new(10, 20, 100, 80))
        );
        assert_eq!(
            model_clip_rect(ClippingRect::Some(Rect::new(0, 0, 50, 50)), viewport),
            ClippingRect::Some(Rect::new(10, 20, 40, 30))
        );
        assert_eq!(
            model_clip_rect(ClippingRect::Zero, viewport),
            ClippingRect::Zero
        );
    }

    fn covering_triangle(depth: f32) -> [ProjectedModelVertex; 3] {
        [
            ProjectedModelVertex {
                x: -1.0,
                y: -1.0,
                depth,
            },
            ProjectedModelVertex {
                x: 8.0,
                y: -1.0,
                depth,
            },
            ProjectedModelVertex {
                x: -1.0,
                y: 8.0,
                depth,
            },
        ]
    }

    fn cull_accepts(cull: ModelFaceCull, vertices: &[ProjectedModelVertex]) -> bool {
        cull.accepts(vertices, &mut [FanCullState::Unset; 2])
    }

    fn covering_textured_triangle(depth: f32) -> [TexturedModelVertex; 3] {
        covering_triangle(depth).map(|position| TexturedModelVertex {
            position,
            uv: TextureUv { u: 0.0, v: 0.0 },
        })
    }

    fn one_pixel_texture(rgba: [u8; 4]) -> PixelImage {
        PixelImage {
            width: 1,
            height: 1,
            rgba: rgba.to_vec(),
        }
    }
}
