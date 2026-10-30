//! Outer name-entry draw 00459310 and its 00439270 preparation.
//!
//! Renderer lookups, caller-context capture and surface release execute in
//! native order. Backdrop and overlay have different renderer custody; the
//! required host connects their existing concrete owners. The final surface
//! release is not the linked +0C presentation/clear callback 0040D1D0.

use thiserror::Error;

use crate::{
    frontend_camera::AnimationClock,
    name_entry_draw::NameEntryDrawState,
    screen_registry::ScreenLifecycle,
    screen_renderer_preparation::{prepare_screen_renderer, ScreenRendererPreparationHost},
};

/// Screen state is immutable throughout this linked draw callback. Rendering
/// reads the actual age/state and the process animation clock, without a tick
/// or completion side effect.
#[derive(Clone, Copy, Debug)]
pub struct NameEntryFrameInput<'a> {
    pub state: NameEntryDrawState<'a>,
    pub age_millis: i32,
    pub animation: AnimationClock,
    /// Live process word 00533760 for object callback 0043A270.
    pub selector_one: u16,
}

/// Every effect is required. A context is a stable identity, not a snapshot
/// of its renderer field. Surface renderer handles are resolved separately at
/// each native lookup; hosts with one backend must still enforce its identity.
pub trait NameEntryFrameHost: ScreenRendererPreparationHost<Self::Error> {
    type Error;
    type Context: Copy;

    /// Ordered 00495900/00495910 reads. The overlay retains this first size.
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error>;
    /// Capture session +648 after the initial size reads (0045936B).
    fn capture_caller_context(&mut self) -> Result<Self::Context, Self::Error>;
    /// Captured overlay renderer's virtual +14(0).
    fn reset_renderer(&mut self, renderer: Self::Renderer) -> Result<(), Self::Error>;
    /// 00495770. A failed/rebuild-needed surface must stop the draw.
    fn validate_surface(&mut self) -> Result<(), Self::Error>;
    /// Execute 0045A560. Its local context copies the shared template, reads
    /// fresh surface dimensions, and takes the renderer from the captured
    /// caller context's *live* +00. Never substitute the overlay renderer or
    /// the published 00546E88 renderer, nor overwrite the caller context.
    fn draw_backdrop(
        &mut self,
        context: Self::Context,
        input: NameEntryFrameInput<'_>,
    ) -> Result<(), Self::Error>;
    /// Execute the existing overlay owner on the fourth, captured renderer.
    /// Its direct stars precede the backdrop flush; all text follows it.
    fn draw_overlay(
        &mut self,
        renderer: Self::Renderer,
        input: NameEntryFrameInput<'_>,
        surface_pixels: [i32; 2],
    ) -> Result<(), Self::Error>;
    /// 004957F0 -> 0050DCC0: unlock only if the surface is locked. This does
    /// not present, clear, reset depth, flush, or consume another clock tick.
    fn finish_surface_draw(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum NameEntryFrameError<E> {
    #[error("name-entry draw requires Ready lifecycle, currently {0:?}")]
    Lifecycle(ScreenLifecycle),
    #[error("required name-entry frame operation failed: {0}")]
    Host(E),
}

/// Complete the draw callback, excluding the scheduler's separate +0C pass.
/// 00459310 has no local recovery scope: even native 0x301 stops this call,
/// preserves completed effects and skips the final surface unlock on failure.
pub fn draw_name_entry_frame<H: NameEntryFrameHost>(
    input: NameEntryFrameInput<'_>,
    host: &mut H,
) -> Result<(), NameEntryFrameError<H::Error>> {
    let result = (|| {
        // 00439270: three live renderer reads, intentionally not coalesced.
        prepare_screen_renderer(host)?;

        let surface_pixels = host.surface_pixels()?;
        let context = host.capture_caller_context()?;
        let overlay_renderer = host.renderer()?;
        host.begin_batch(overlay_renderer)?;
        host.reset_renderer(overlay_renderer)?;
        host.validate_surface()?;
        host.draw_backdrop(context, input)?;
        host.draw_overlay(overlay_renderer, input, surface_pixels)?;
        host.finish_surface_draw()
    })();
    result.map_err(NameEntryFrameError::Host)
}

#[cfg(test)]
mod tests;
