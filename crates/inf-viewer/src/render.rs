//! Diagnostic asset rendering for SDL's software canvas.

mod model;

pub use model::{
    deduplicated_face_edges, draw_model, draw_presented_model, project_model_geometry,
    project_model_vertex, triangulate_face, ModelFaceCull, ModelFacePaint, ModelFaceSurface,
    ModelRaster, ModelRenderStats, ProjectedModelVertex, QuadSplit, TextureUv, TexturedModelVertex,
};

use inf_formats::{
    MaterialPalette8, SpriteEntry, SpriteRenderMode, SpriteShade, SpriteVariant, TerrainEntry,
    TERRAIN_GRID_SIDE,
};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas};
use sdl2::video::Window;

use crate::ui::UiRect;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainChannel {
    #[default]
    Height,
    Material,
    Shade,
    Sparse,
    Diagonal,
}

impl TerrainChannel {
    pub const fn next(self) -> Self {
        match self {
            Self::Height => Self::Material,
            Self::Material => Self::Shade,
            Self::Shade => Self::Sparse,
            Self::Sparse => Self::Diagonal,
            Self::Diagonal => Self::Height,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Height => "HEIGHT",
            Self::Material => "MATERIAL",
            Self::Shade => "SHADE4",
            Self::Sparse => "SPARSE RECORDS",
            Self::Diagonal => "STORED DIAGONAL",
        }
    }
}

pub fn selected_sprite_variant(
    entry: &SpriteEntry,
    mode: SpriteRenderMode,
    physical_override: Option<usize>,
) -> Option<&SpriteVariant> {
    physical_override
        .and_then(|index| entry.variants.get(index))
        .or_else(|| entry.variant(mode))
}

pub fn decode_sprite(
    source: &[u8],
    entry: &SpriteEntry,
    mode: SpriteRenderMode,
    physical_override: Option<usize>,
    shade: SpriteShade,
) -> Result<PixelImage, String> {
    let variant = selected_sprite_variant(entry, mode, physical_override)
        .ok_or_else(|| "selected sprite mode has no physical variant".to_owned())?;
    let decoded = variant
        .decode_rgba(source, shade)
        .map_err(|error| error.to_string())?;
    let mut rgba = Vec::with_capacity(decoded.len() * 4);
    for pixel in decoded {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    Ok(PixelImage {
        width: u32::from(variant.width),
        height: u32::from(variant.height),
        rgba,
    })
}

pub fn decode_terrain(
    entry: &TerrainEntry,
    palette: Option<MaterialPalette8>,
    channel: TerrainChannel,
) -> PixelImage {
    let mut rgba = vec![0; TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE * 4];
    for y in 0..TERRAIN_GRID_SIDE {
        for x in 0..TERRAIN_GRID_SIDE {
            let cell = entry
                .cell(x, y)
                .expect("terrain diagnostic coordinates stay inside 512x512");
            let color = match channel {
                TerrainChannel::Height => {
                    let value = (i16::from(cell.height_i8()) + 128) as u8;
                    [value, value, value]
                }
                TerrainChannel::Material => palette
                    .map(|palette| palette.entries()[usize::from(cell.material())])
                    .map(|material| {
                        let color = material.rgb555();
                        [
                            expand5(color.red5()),
                            expand5(color.green5()),
                            expand5(color.blue5()),
                        ]
                    })
                    .unwrap_or_else(|| match cell.material() {
                        0 => [72, 123, 63],
                        1 => [158, 135, 83],
                        2 => [77, 114, 148],
                        _ => [139, 92, 131],
                    }),
                TerrainChannel::Shade => {
                    let value = cell.shade4() * 17;
                    [value, value, value]
                }
                TerrainChannel::Sparse => {
                    if cell.has_sparse_record() {
                        [255, 119, 41]
                    } else {
                        [35, 42, 51]
                    }
                }
                TerrainChannel::Diagonal => {
                    if cell.stored_alternate_diagonal() {
                        [235, 92, 92]
                    } else {
                        [61, 96, 130]
                    }
                }
            };
            let offset = (y * TERRAIN_GRID_SIDE + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    PixelImage {
        width: TERRAIN_GRID_SIDE as u32,
        height: TERRAIN_GRID_SIDE as u32,
        rgba,
    }
}

const fn expand5(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

pub fn draw_image(
    canvas: &mut Canvas<Window>,
    viewport: UiRect,
    image: &PixelImage,
    checkerboard: bool,
) -> Result<(), String> {
    if checkerboard {
        draw_checkerboard(canvas, viewport)?;
    }
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, image.width, image.height)
        .map_err(|error| error.to_string())?;
    texture.set_blend_mode(BlendMode::Blend);
    texture
        .update(None, &image.rgba, image.width as usize * 4)
        .map_err(|error| error.to_string())?;
    let destination = fit_rect(viewport, image.width, image.height, 28);
    canvas.copy(&texture, None, destination.as_sdl())
}

fn draw_checkerboard(canvas: &mut Canvas<Window>, viewport: UiRect) -> Result<(), String> {
    const TILE: i32 = 24;
    for y in (viewport.y..viewport.y + viewport.height as i32).step_by(TILE as usize) {
        for x in (viewport.x..viewport.x + viewport.width as i32).step_by(TILE as usize) {
            let parity = ((x - viewport.x) / TILE + (y - viewport.y) / TILE) & 1;
            canvas.set_draw_color(if parity == 0 {
                Color::RGB(25, 29, 36)
            } else {
                Color::RGB(38, 43, 52)
            });
            canvas.fill_rect(Rect::new(x, y, TILE as u32, TILE as u32))?;
        }
    }
    Ok(())
}

fn fit_rect(viewport: UiRect, width: u32, height: u32, margin: u32) -> UiRect {
    let available_width = viewport.width.saturating_sub(margin * 2).max(1);
    let available_height = viewport.height.saturating_sub(margin * 2).max(1);
    let scale = (available_width as f32 / width.max(1) as f32)
        .min(available_height as f32 / height.max(1) as f32)
        .max(0.01);
    let fitted_width = (width as f32 * scale).round().max(1.0) as u32;
    let fitted_height = (height as f32 * scale).round().max(1.0) as u32;
    UiRect::new(
        viewport.x + (viewport.width.saturating_sub(fitted_width) / 2) as i32,
        viewport.y + (viewport.height.saturating_sub(fitted_height) / 2) as i32,
        fitted_width,
        fitted_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_channels_cycle_without_hidden_state() {
        let mut channel = TerrainChannel::Height;
        for expected in [
            TerrainChannel::Material,
            TerrainChannel::Shade,
            TerrainChannel::Sparse,
            TerrainChannel::Diagonal,
            TerrainChannel::Height,
        ] {
            channel = channel.next();
            assert_eq!(channel, expected);
        }
    }

    #[test]
    fn image_fit_preserves_aspect_and_centers() {
        let fitted = fit_rect(UiRect::new(10, 20, 400, 300), 200, 100, 0);
        assert_eq!(fitted, UiRect::new(10, 70, 400, 200));
    }
}
