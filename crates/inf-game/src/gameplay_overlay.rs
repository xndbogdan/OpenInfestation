//! World-22 overlay from `00440F00`, native segment `00441003..0044120F`.
//!
//! The two masks are immediate renderer +84 calls after the world flush.
//! Their dimensions are viewport fractions, not a reduced world viewport.
//! Caption layout uses the existing H4 font and timed H0 text consumer.

use thiserror::Error;

use crate::{
    menu::MenuColor,
    menu_text::{
        measure_text_width, MenuTextEmission, MenuTextLayoutError, MenuTextResources,
        MenuTextReveal, RawMenuTextRequest,
    },
    scene_quad::DirectFlatQuad,
};

pub trait GameplayOverlayResources: MenuTextResources {
    /// Converted resident H7 DWORD, as returned by 00405540.
    fn material_word(&self, argument: i32) -> Result<u32, Self::Error>;
}

pub trait GameplayOverlayHost {
    type Error;
    fn flat_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error>;
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayOverlayRequest {
    /// Live G+250 / mission+A0, distinct from serialized world selection.
    pub current_world: u32,
    pub surface_pixels: [i32; 2],
    /// Screen +C0, initialized by 00437F90 and advanced by 004392B0.
    pub screen_age_millis: i32,
    pub glyph_base_color: MenuColor,
}

#[derive(Debug, Error)]
pub enum GameplayOverlayError<R, H> {
    #[error("gameplay overlay resource: {0}")]
    Resource(R),
    #[error("gameplay overlay submission: {0}")]
    Backend(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
}

/// Preserve the native submission/resource order on failure. A stopped lower
/// call never submits the remaining band or caption on that draw invocation.
pub fn draw_gameplay_overlay<R: GameplayOverlayResources, H: GameplayOverlayHost>(
    request: GameplayOverlayRequest,
    resources: &R,
    host: &mut H,
) -> Result<(), GameplayOverlayError<R::Error, H::Error>> {
    if request.current_world != 22 {
        return Ok(());
    }
    let color_word = resources
        .material_word(32)
        .map_err(GameplayOverlayError::Resource)?;
    let [width, height] = request.surface_pixels;
    for [top, bottom] in [[0, height / 8], [height.wrapping_mul(7) / 8, height]] {
        host.flat_quad(DirectFlatQuad {
            corners: [
                [0, top as i16],
                [width as i16, top as i16],
                [width as i16, bottom as i16],
                [0, bottom as i16],
            ],
            vertex_depths: [0x40; 4],
            descriptor_word: 0,
            color_word,
        })
        .map_err(GameplayOverlayError::Backend)?;
    }
    // These lookups and measurement precede the reveal-age gate in retail.
    let font = resources.font(0).map_err(GameplayOverlayError::Resource)?;
    let color = resources.style(0).map_err(GameplayOverlayError::Resource)?;
    let text = resources
        .string(0xdf)
        .map_err(GameplayOverlayError::Resource)?;
    let measured = i32::from(measure_text_width(font, text)? as i16);
    let position_pixels = [
        width.wrapping_sub(measured) / 2,
        height.wrapping_mul(15) / 16,
    ];
    let age_millis = (request.screen_age_millis / 2).wrapping_sub(1000).max(0);
    if age_millis > 0 {
        host.raw_text(&mut RawMenuTextRequest {
            text,
            font,
            position_pixels,
            width_pixels: width,
            color: MenuColor::from_word(color),
            base_color: request.glyph_base_color,
            context_origin_pixels: [0, 0],
            emission: MenuTextEmission::Draw,
            reveal: MenuTextReveal::Timed { age_millis },
        })
        .map_err(GameplayOverlayError::Backend)?;
    }
    Ok(())
}

pub(crate) mod software;

#[cfg(test)]
mod tests;
