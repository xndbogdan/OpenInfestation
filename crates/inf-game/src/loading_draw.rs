//! Reached loading Tick 0040E6B0 renderer/title/equalizer policy, with the
//! attract loading's four-sprite fade 0040E140. The native
//! flush/title/counter phases remain ordered. This does not claim the
//! complete original background, loading screen or attract admission. Effects borrow the actual text-entry array; mode and
//! attract bytes are read from their current owners at the original phases.
//! Required resource/submission errors preserve prior direct draws and RNG.

use thiserror::Error;

use crate::loading_runtime::LoadingTickRequest;
use crate::menu::MenuColor;
use crate::menu_text::{
    draw_line_text, measure_text_width, LineTextRequest, MenuTextError, MenuTextHost,
    MenuTextLayoutError, MenuTextResources, INITIAL_GLYPH_BASE_WORD,
};
use crate::name_entry::name_entry_rng_next;
use crate::render_resources::{
    RenderResourceError, RenderResourceKind, RenderResourceRequest, RenderResources,
};
use crate::scene_sprite::{DirectSpriteMode, DirectSpriteQuad};

#[derive(Debug, PartialEq, Eq, Error)]
pub enum LoadingStringError<E> {
    #[error("loading optional H0 slot {slot} equals the unchecked native capacity {capacity}")]
    CapacityBoundary { slot: i32, capacity: usize },
    #[error("loading optional string source: {0}")]
    Source(E),
}

pub trait LoadingDrawResources: MenuTextResources {
    /// Capture required H3 identity without adding a dimensions read.
    fn required_sprite(&self, argument: i32) -> Result<(), Self::Error>;
    /// An H3 slot `0040E140` reads: false when the pool's count is below it
    /// or the slot is vacant (its loader answers NULL), where the fade
    /// draws no more.
    fn attract_sprite(&self, argument: i32) -> Result<bool, Self::Error>;
    /// Only a genuine NULL/capacity branch may supply None. A missing lazy
    /// owner is an error, not an implicit empty string or default title.
    fn optional_loading_string(
        &self,
        argument: i32,
    ) -> Result<Option<&[u8]>, LoadingStringError<Self::Error>>;
}

impl LoadingDrawResources for RenderResources<'_> {
    fn required_sprite(&self, argument: i32) -> Result<(), RenderResourceError> {
        self.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument,
        })?;
        Ok(())
    }

    fn attract_sprite(&self, argument: i32) -> Result<bool, RenderResourceError> {
        if argument as usize > self.pool_capacity(RenderResourceKind::Sprite) {
            return Ok(false);
        }
        match self.required_sprite(argument) {
            Ok(()) => Ok(true),
            Err(RenderResourceError::NotResident { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn optional_loading_string(
        &self,
        argument: i32,
    ) -> Result<Option<&[u8]>, LoadingStringError<RenderResourceError>> {
        // These consumers supply literal positive full DWORDs. Do not change
        // general MenuTextResources string policy or infer lazy NULL from a
        // missing registration. Native index==capacity is unchecked backing.
        let capacity = self.pool_capacity(RenderResourceKind::String);
        if argument >= 0 && capacity < argument as usize {
            return Ok(None);
        }
        if argument >= 0 && capacity == argument as usize {
            return Err(LoadingStringError::CapacityBoundary {
                slot: argument,
                capacity,
            });
        }
        self.string(argument)
            .map(Some)
            .map_err(LoadingStringError::Source)
    }
}

pub trait LoadingDrawHost: MenuTextHost {
    /// Fresh Tick acquisition, distinct from callback's initial NULL check.
    fn acquire_renderer(&mut self) -> Result<(), Self::Error>;
    fn surface_width(&mut self) -> Result<i32, Self::Error>;
    fn surface_height(&mut self) -> Result<i32, Self::Error>;
    fn acquire_surface(&mut self) -> Result<(), Self::Error>;
    /// Exact 0040E6FD marker: preserve old queued resources before this call.
    fn reset_batch_40e6fd(&mut self) -> Result<(), Self::Error>;
    fn reset_surface_40e70a(&mut self) -> Result<(), Self::Error>;
    fn current_attract_mode(&mut self) -> u8;
    fn current_loading_mode(&mut self) -> u8;
    fn draw_model_background(
        &mut self,
        state: &crate::loading_runtime::LoadingProcessState,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    fn finish_background_495770(&mut self) -> Result<(), Self::Error>;
    fn flush_batch_4a4460(&mut self) -> Result<(), Self::Error>;
    fn submit_loading_quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error>;
}

pub struct LoadingDrawRequest<'a> {
    pub tick: LoadingTickRequest<'a>,
    /// Actual captured renderer context's existing glyph metadata. It is not
    /// added to screen pixels and is not inferred from a zero/default context.
    pub context_origin_pixels: [i32; 2],
}

#[derive(Debug, Error)]
pub enum LoadingDrawError<R, H> {
    #[error("required loading resource: {0}")]
    Resource(R),
    #[error(transparent)]
    OptionalString(#[from] LoadingStringError<R>),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error("loading text: {0}")]
    Text(MenuTextError<R, H>),
    #[error("required loading renderer phase: {0}")]
    Host(H),
}

pub fn draw_loading_tick_40e6b0<R: LoadingDrawResources, H: LoadingDrawHost>(
    request: LoadingDrawRequest<'_>,
    resources: &R,
    host: &mut H,
) -> Result<(), LoadingDrawError<R::Error, H::Error>> {
    host.acquire_renderer().map_err(LoadingDrawError::Host)?;
    let width = host.surface_width().map_err(LoadingDrawError::Host)?;
    let height = host.surface_height().map_err(LoadingDrawError::Host)?;
    let decay_step = request.tick.delta_micros >> 13;
    host.acquire_surface().map_err(LoadingDrawError::Host)?;
    host.reset_batch_40e6fd().map_err(LoadingDrawError::Host)?;
    host.reset_surface_40e70a()
        .map_err(LoadingDrawError::Host)?;
    let attract = host.current_attract_mode();
    let mode = host.current_loading_mode();

    if attract != 0 {
        if mode == 0 {
            draw_attract_fade_40e140(request.tick.state, width, height, resources, host)?;
            host.finish_background_495770()
                .map_err(LoadingDrawError::Host)?;
            host.flush_batch_4a4460().map_err(LoadingDrawError::Host)?;
        }
        let delta_millis = request.tick.delta_micros / 1000;
        let state = request.tick.state;
        // Original updates all three nonzero DWORDs, then applies the three
        // signed strict>2000 tests. Ordinary loading does not touch them.
        if state.counter_546920 != 0 {
            state.counter_546920 = state.counter_546920.wrapping_add(delta_millis as u32);
        }
        if state.counter_54691c != 0 {
            state.counter_54691c = state.counter_54691c.wrapping_add(delta_millis as u32);
        }
        if state.counter_546918 != 0 {
            state.counter_546918 = state.counter_546918.wrapping_add(delta_millis as u32);
        }
        if state.counter_546920 as i32 > 2000 {
            state.counter_546920 = 0;
        }
        if state.counter_54691c as i32 > 2000 {
            state.counter_54691c = 0;
        }
        if state.counter_546918 as i32 > 2000 {
            state.counter_546918 = 0;
        }
        return Ok(());
    }

    if mode == 0 {
        host.draw_model_background(request.tick.state, request.tick.rng)
            .map_err(LoadingDrawError::Host)?;
    }
    host.finish_background_495770()
        .map_err(LoadingDrawError::Host)?;
    host.flush_batch_4a4460().map_err(LoadingDrawError::Host)?;
    // This is a fresh mode read after the common flush, not cached `mode`.
    if host.current_loading_mode() != 0 {
        let margin = width.wrapping_shl(4) / 120;
        let half_width = width / 2;
        let center_y = height.wrapping_mul(65) / 100;
        let bar_width = width / 80;
        let mut horizontal = 0i32;
        for index in 0..16 {
            resources
                .required_sprite(7)
                .map_err(LoadingDrawError::Resource)?;
            let x = half_width
                .wrapping_sub(horizontal / 60)
                .wrapping_add(margin);
            let value = i32::from(request.tick.effect_bytes[index] as i8);
            let half_height = value.wrapping_mul(height) / 1200;
            let style = resources.style(1).map_err(LoadingDrawError::Resource)?;
            let right = x.wrapping_add(bar_width) as i16;
            let top = center_y.wrapping_add(half_height) as i16;
            let bottom = center_y.wrapping_sub(half_height) as i16;
            host.submit_loading_quad(DirectSpriteQuad {
                sprite_slot: 7,
                corners: [
                    [x as i16, top],
                    [right, top],
                    [right, bottom],
                    [x as i16, bottom],
                ],
                vertex_depths: [0x40; 4],
                descriptor_word: 0,
                mode: DirectSpriteMode::Effect { color_word: style },
            })
            .map_err(LoadingDrawError::Host)?;
            // Submission precedes both the genuine RNG and this byte's store.
            if name_entry_rng_next(request.tick.rng) & 15 == 0 {
                request.tick.effect_bytes[index] = 0x7f;
            }
            let current = request.tick.effect_bytes[index];
            let signed = i32::from(current as i8);
            request.tick.effect_bytes[index] = if signed > decay_step {
                current.wrapping_sub((signed.wrapping_mul(decay_step) / 32) as u8)
            } else {
                0
            };
            horizontal = horizontal.wrapping_add(width);
        }
    }

    // Required font precedes required style even if the optional text is NULL.
    let font = resources.font(0).map_err(LoadingDrawError::Resource)?;
    let style = resources.style(4).map_err(LoadingDrawError::Resource)?;
    let title_mode = host.current_loading_mode();
    let (title, mut y) = if title_mode == 0 {
        (354, height / 2)
    } else {
        (283, height.wrapping_mul(33) / 100)
    };
    let mut title_context = LoadingTitleContext {
        resources,
        host,
        font,
        width,
        color: MenuColor::from_word(style),
        context_origin_pixels: request.context_origin_pixels,
    };
    if let Some(text) = resources.optional_loading_string(title)? {
        title_context.emit(text, y)?;
    }
    if request.tick.caller_status >= 3 {
        if let Some(text) = resources.optional_loading_string(218)? {
            y = y.wrapping_add(height.wrapping_mul(5) / 100);
            title_context.emit(text, y)?;
        }
    }
    Ok(())
}

/// `0040E140`: H3 sprites 8 to 11 as a two-by-two grid of quads a quarter of
/// the surface wide and 5/16 tall around its centre, in H6 style 14 with each
/// colour byte scaled by the fade (alpha kept). The fade is 256, from 0 while
/// counter 00546920 runs (`<< 8` over 2000), 256 while 0054691C runs, and
/// back down while 00546918 runs (`(2000 - it) << 8` over 2000), clamped to
/// 0..=256. Each quad is a queued effect packet (`004A42B0`, `0040EF30`) that
/// the following flush draws; none overlaps, so their sort order changes
/// nothing. A sprite past the pool's count or vacant ends the fade there.
fn draw_attract_fade_40e140<R: LoadingDrawResources, H: LoadingDrawHost>(
    state: &crate::loading_runtime::LoadingProcessState,
    width: i32,
    height: i32,
    resources: &R,
    host: &mut H,
) -> Result<(), LoadingDrawError<R::Error, H::Error>> {
    let ramp = |counter: u32| (counter.wrapping_shl(8) as i32) / 2000;
    let mut fade = 0x100;
    if state.counter_546920 != 0 {
        fade = ramp(state.counter_546920);
    }
    if state.counter_54691c != 0 {
        fade = 0x100;
    }
    if state.counter_546918 != 0 {
        fade = ramp(2000_u32.wrapping_sub(state.counter_546918));
    }
    let fade = fade.clamp(0, 0x100);
    let style = resources.style(14).map_err(LoadingDrawError::Resource)?;
    let channel = |shift: u32| (((style >> shift) & 0xff) as i32 * fade / 0x100) as u32 & 0xff;
    let colour = (style & 0xff00_0000) | channel(16) << 16 | channel(8) << 8 | channel(0);
    let quarter = width / 4;
    let tall = height.wrapping_mul(5) / 16;
    let left = width / 2 - quarter;
    let top = height / 2 - tall;
    for (index, [column, row]) in [[0, 0], [1, 0], [0, 1], [1, 1]].into_iter().enumerate() {
        let sprite = 8 + index as i32;
        if !resources
            .attract_sprite(sprite)
            .map_err(LoadingDrawError::Resource)?
        {
            return Ok(());
        }
        let x = left.wrapping_add(column * quarter);
        let y = top.wrapping_add(row * tall);
        let (right, bottom) = (x.wrapping_add(quarter), y.wrapping_add(tall));
        host.submit_loading_quad(DirectSpriteQuad {
            sprite_slot: sprite as u16,
            corners: [[x, y], [right, y], [right, bottom], [x, bottom]]
                .map(|corner| corner.map(|value| value as i16)),
            vertex_depths: [0x40; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect { color_word: colour },
        })
        .map_err(LoadingDrawError::Host)?;
    }
    Ok(())
}

/// Shared captured font/style/renderer context for the two real title calls.
/// Per-string Y and source bytes remain explicit at each reached consumer.
struct LoadingTitleContext<'a, R, H> {
    resources: &'a R,
    host: &'a mut H,
    font: &'a inf_formats::Font,
    width: i32,
    color: MenuColor,
    context_origin_pixels: [i32; 2],
}

impl<R: LoadingDrawResources, H: LoadingDrawHost> LoadingTitleContext<'_, R, H> {
    fn emit(&mut self, text: &[u8], y: i32) -> Result<(), LoadingDrawError<R::Error, H::Error>> {
        let measured = i32::from(measure_text_width(self.font, text)? as i16);
        let x = (self.width / 2).wrapping_sub(measured / 2);
        draw_line_text(
            &mut LineTextRequest {
                text,
                font: self.font,
                position_pixels: [x, y],
                color: self.color,
                base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                context_origin_pixels: self.context_origin_pixels,
            },
            self.resources,
            self.host,
        )
        .map_err(LoadingDrawError::Text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
