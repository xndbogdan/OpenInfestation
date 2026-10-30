//! Overlay layout of 00459310, after 0045A560 and before 004957F0.
//!
//! The caller has begun/reset the renderer and queued the backdrop. This
//! owner draws the stars immediately, flushes that queued backdrop, then
//! submits the feedback, rings, error lines and editable/completing name.
//! Surface acquisition, 00439270 and presentation belong to the outer frame.

use inf_formats::Font;
use thiserror::Error;

use crate::{
    frontend_scene::{FooterQuad, FrontendSceneHost, SceneCommand, StarQuad},
    menu::MenuColor,
    menu_text::{
        measure_text_width, LineTextRequest, MenuTextEmission, MenuTextLayoutError,
        MenuTextResources, MenuTextReveal, RawMenuTextRequest,
    },
    name_entry::{NameEntryError, NameEntryState, DELETE_CELL, GRID_CELLS, NAME_GRID},
};

mod rings;

/// Read-only screen fields consumed by the draw; no draw-time state mutation.
#[derive(Clone, Copy, Debug)]
pub struct NameEntryDrawState<'a> {
    pub name: &'a [u8],
    pub selection: i32,
    pub row_anim: i32,
    pub completion_delay: i32,
    pub feedback: i32,
    pub error: bool,
}

impl<'a> From<&'a NameEntryState> for NameEntryDrawState<'a> {
    fn from(entry: &'a NameEntryState) -> Self {
        Self {
            name: entry.name(),
            selection: entry.selection(),
            row_anim: entry.row_anim(),
            completion_delay: entry.completion_delay(),
            feedback: entry.feedback(),
            error: entry.error(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NameEntryDrawFrame<'a> {
    pub state: NameEntryDrawState<'a>,
    /// Base screen +C0, signed milliseconds.
    pub age_millis: i32,
    pub surface_pixels: [i32; 2],
    /// Shared 00546F20/24, without frontend focal doubling.
    pub focal: [i32; 2],
    pub context_origin_pixels: [i32; 2],
    /// Shared glyph base word 0055F8F8; the draw leaves it untouched.
    pub base_color: MenuColor,
    pub sine_quarter: &'a [u16; 4096],
}

/// Direct renderer operations, distinct from deferred scene primitives.
pub trait NameEntryDrawHost: FrontendSceneHost {
    /// Renderer +84 with depth 0x7FFFFFFF and the H7 material word. Unlike
    /// frontend stars, this neither changes context +04 nor queues 00466170.
    fn star(&mut self, quad: StarQuad) -> Result<(), Self::Error>;
    /// Renderer +A4, sprite H3/7, style H6/3 and vertex attribute 0x4000.
    fn highlight(&mut self, quad: FooterQuad) -> Result<(), Self::Error>;
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error>;
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum NameEntryDrawError<R, H> {
    #[error("name-entry draw resource: {0}")]
    Resource(R),
    #[error("name-entry draw backend: {0}")]
    Host(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error(transparent)]
    State(#[from] NameEntryError),
}

/// Execute 004593AC..0045A53A in retail order. Failures preserve earlier
/// draws and stop immediately; there is no native recovery scope here.
pub fn draw_name_entry_overlay<R: MenuTextResources, H: NameEntryDrawHost>(
    frame: NameEntryDrawFrame<'_>,
    resources: &R,
    host: &mut H,
) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
    // Contain the native unchecked grid access before producing any pixels.
    if !(0..GRID_CELLS as i32).contains(&frame.state.selection) {
        return Err(NameEntryError::Selection(frame.state.selection).into());
    }
    let font = resources.font(0).map_err(NameEntryDrawError::Resource)?;
    let color = resources
        .style(2)
        .map(MenuColor::from_word)
        .map_err(NameEntryDrawError::Resource)?;
    let mut draw = DrawContext {
        resources,
        host,
        frame,
        font,
    };
    draw.stars()?;
    draw.host
        .submit(SceneCommand::FlushBatch)
        .map_err(NameEntryDrawError::Host)?;
    if frame.state.feedback != 0 {
        let text = resources
            .string(0xe0)
            .map_err(NameEntryDrawError::Resource)?;
        draw.raw(
            text,
            [
                frame.surface_pixels[0] / 20,
                frame.surface_pixels[1].wrapping_mul(3) / 20,
            ],
            color,
            1000i32.wrapping_sub(frame.state.feedback),
        )?;
    }
    // CMP/JBE is unsigned: only exactly 1 suppresses all three rings.
    if frame.state.completion_delay != 1 {
        draw.rings()?;
    }
    let name_source = if frame.state.error {
        NameTextSource::RetryLabel(draw.error_lines()?)
    } else {
        NameTextSource::Formatted
    };
    draw.name_line(name_source)
}

struct DrawContext<'a, R, H> {
    resources: &'a R,
    host: &'a mut H,
    frame: NameEntryDrawFrame<'a>,
    font: &'a Font,
}

/// 00459Fxx replaces descriptor +00 with the retry label. Later sprintf
/// calls still fill the local buffer, but never restore that pointer.
#[derive(Clone, Copy)]
enum NameTextSource<'a> {
    Formatted,
    RetryLabel(&'a [u8]),
}

impl NameTextSource<'_> {
    fn text<'a>(&'a self, formatted: &'a [u8]) -> &'a [u8] {
        match self {
            Self::Formatted => formatted,
            Self::RetryLabel(text) => text,
        }
    }
}

impl<'a, R: MenuTextResources, H: NameEntryDrawHost> DrawContext<'a, R, H> {
    fn color(&self, style: i32) -> Result<MenuColor, NameEntryDrawError<R::Error, H::Error>> {
        self.resources
            .style(style)
            .map(MenuColor::from_word)
            .map_err(NameEntryDrawError::Resource)
    }

    fn width(&self, text: &[u8]) -> Result<i32, NameEntryDrawError<R::Error, H::Error>> {
        Ok(i32::from(measure_text_width(self.font, text)? as i16))
    }

    fn line(
        &mut self,
        text: &[u8],
        position_pixels: [i32; 2],
        color: MenuColor,
    ) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        self.host
            .line_text(&mut LineTextRequest {
                text,
                font: self.font,
                position_pixels,
                color,
                base_color: self.frame.base_color,
                context_origin_pixels: self.frame.context_origin_pixels,
            })
            .map_err(NameEntryDrawError::Host)
    }

    fn raw(
        &mut self,
        text: &[u8],
        position_pixels: [i32; 2],
        color: MenuColor,
        age_millis: i32,
    ) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        self.host
            .raw_text(&mut RawMenuTextRequest {
                text,
                font: self.font,
                position_pixels,
                color,
                width_pixels: self.frame.surface_pixels[0],
                base_color: self.frame.base_color,
                context_origin_pixels: self.frame.context_origin_pixels,
                emission: MenuTextEmission::Draw,
                reveal: MenuTextReveal::Timed { age_millis },
            })
            .map_err(NameEntryDrawError::Host)
    }

    fn error_lines(&mut self) -> Result<&'a [u8], NameEntryDrawError<R::Error, H::Error>> {
        let [width, height] = self.frame.surface_pixels;
        let blinking = if (self.frame.age_millis / 250) % 2 != 0 {
            3
        } else {
            2
        };
        let color = self.color(blinking)?;
        let text = self
            .resources
            .string(0x367)
            .map_err(NameEntryDrawError::Resource)?;
        self.line(
            text,
            [
                width.wrapping_sub(self.width(text)?) / 2,
                height.wrapping_mul(13) / 20,
            ],
            color,
        )?;
        let text = self
            .resources
            .string(0x27)
            .map_err(NameEntryDrawError::Resource)?;
        let color = self.color(2)?;
        self.line(
            text,
            [
                width.wrapping_sub(self.width(text)?) / 2,
                height.wrapping_mul(14) / 20,
            ],
            color,
        )?;
        Ok(text)
    }

    fn name_line(
        &mut self,
        source: NameTextSource<'_>,
    ) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        let color = self.color(3)?;
        if self.frame.state.completion_delay != 0 {
            return self.completing_name(color, source);
        }
        let mut text = self.name_prefix()?;
        text.extend_from_slice(c_string(self.frame.state.name));
        // The preview blink is 300 ms. Delete removes the last byte of the
        // whole formatted string, even when the name itself is empty.
        if (self.frame.age_millis / 300) % 2 != 0 {
            if self.frame.state.selection == DELETE_CELL {
                text.pop();
            } else {
                text.push(NAME_GRID[self.frame.state.selection as usize]);
            }
        }
        let fade = self.frame.age_millis.wrapping_mul(4).min(0x7fff);
        let color = MenuColor {
            rgb: color
                .rgb
                .map(|byte| (i32::from(byte).wrapping_mul(fade) >> 15) as u8),
            ..color
        };
        let [width, height] = self.frame.surface_pixels;
        self.line(
            source.text(&text),
            [width / 20, height.wrapping_mul(2) / 20],
            color,
        )
    }

    fn name_prefix(&self) -> Result<Vec<u8>, NameEntryDrawError<R::Error, H::Error>> {
        let mut text = c_string(
            self.resources
                .string(0x115)
                .map_err(NameEntryDrawError::Resource)?,
        )
        .to_vec();
        text.extend_from_slice(b": ");
        Ok(text)
    }

    fn completing_name(
        &mut self,
        color: MenuColor,
        source: NameTextSource<'_>,
    ) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        let [width, height] = self.frame.surface_pixels;
        let delay = self.frame.state.completion_delay;
        let mut prefix = self.name_prefix()?;
        let start = [width / 20, height.wrapping_mul(2) / 20];
        // Measure BEFORE truncation; the name starts after the full prefix.
        let name_x = start[0].wrapping_add(self.width(source.text(&prefix))?);
        let visible = (prefix.len() as i32)
            .wrapping_mul(delay.wrapping_sub(500))
            .wrapping_mul(2)
            / 1000;
        // Outside the native timer range the stack write may exceed the
        // string. Keep the unchanged string rather than emulate corruption.
        prefix.truncate(visible.max(0) as usize);
        self.line(source.text(&prefix), start, color)?;
        let color = self.color(3)?;
        let travel_time = 1000i32.wrapping_sub(delay).wrapping_sub(300);
        let move_towards = |distance: i32| {
            distance.min((distance.wrapping_mul(travel_time).wrapping_mul(2) / 1000).max(0))
        };
        let name = source.text(c_string(self.frame.state.name));
        let target_x = width.wrapping_sub(self.width(name)?) / 2;
        let position = [
            name_x.wrapping_add(move_towards(target_x.wrapping_sub(name_x))),
            start[1].wrapping_add(move_towards(height.wrapping_mul(6) / 20)),
        ];
        self.line(name, position, color)?;
        let reveal_age = 900i32.wrapping_sub(delay);
        if reveal_age > 0 {
            let text = self
                .resources
                .string(0x162)
                .map_err(NameEntryDrawError::Resource)?;
            // Separate truncating halves, unlike the error-line centering.
            self.raw(
                text,
                [(width / 2).wrapping_sub(self.width(text)? / 2), height / 2],
                color,
                reveal_age,
            )?;
        }
        Ok(())
    }
}

fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len())]
}

fn divide(numerator: i32, divisor: i32) -> Result<i32, MenuTextLayoutError> {
    numerator
        .checked_div(divisor)
        .ok_or(MenuTextLayoutError::Division { numerator, divisor })
}

#[cfg(test)]
mod native_tests;

#[cfg(test)]
mod tests;
