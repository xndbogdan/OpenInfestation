//! A start's wait made visible (2026-10-23: something must show so
//! the player knows the game is actively loading).
//! Before the title, while the device makes the tracer and its upscaler
//! (0.7 s with the driver's cache warm, 3 s when a new build's shaders
//! compile cold), the window shows one frame of a line in the port's words
//! ("Preparing graphics..."; retail's own, H0 354, says "Stand by... Warp in
//! progress"), drawn as `0040E6B0` draws retail's loading line in mode 0:
//! H4 font 0 in H6 style 4, its green, centred at half the height.

use sdl2::render::Texture;

use super::display::{copy_framebuffer, present_observed, Output};
use crate::{
    draw_queue::DrawInvocation,
    menu::MenuColor,
    menu_text::{
        draw_line_text, measure_text_width, LineTextRequest, MenuGlyphQuad, MenuTextHost,
        MenuTextResources, INITIAL_GLYPH_BASE_WORD,
    },
    pixel_backend::PixelBackend,
    port_text::PortText,
    presentation::ClassicScaling,
    render_resources::RenderResources,
    session_platform::software::present_linked_clear,
    software_menu::glyph_command,
    software_scene::SoftwareScene,
};

/// Draw the line in `language` into `scene`'s frame, `dimensions` native
/// pixels, present it through `output` and clear the frame to `clear`.
#[allow(clippy::too_many_arguments)]
pub(super) fn show_loading_line<'r>(
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    language: u8,
    dimensions: [u32; 2],
    clear: [u8; 4],
    output: &mut Output,
    texture: Option<&mut Texture<'_>>,
    scaling: ClassicScaling,
) -> Result<(), String> {
    let font = resources.font(0).map_err(|error| error.to_string())?;
    let style = resources.style(4).map_err(|error| error.to_string())?;
    let text = PortText::PreparingGraphics.text(language);
    let measured =
        i32::from(measure_text_width(font, text).map_err(|error| error.to_string())? as i16);
    let [width, height] = dimensions.map(|side| side as i32);
    let origin = scene.primitive_state.projection.center;
    scene.reset_renderer();
    draw_line_text(
        &mut LineTextRequest {
            text,
            font,
            position_pixels: [width / 2 - measured / 2, height / 2],
            color: MenuColor::from_word(style),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: origin,
        },
        &resources,
        &mut Glyphs {
            scene: &mut *scene,
            resources,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    present_linked_clear(scene, clear, |frame| {
        copy_framebuffer(output, texture, frame, scaling)?;
        present_observed(output, None, "starting", None);
        Ok::<(), String>(())
    })
    .map_err(|error| error.to_string())
}

/// The line's glyphs, drawn at once through the scene's pixels.
struct Glyphs<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

impl MenuTextHost for Glyphs<'_, '_, '_> {
    type Error = String;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), String> {
        let mut command =
            glyph_command(self.resources, glyph).map_err(|error| error.to_string())?;
        self.scene
            .pixels
            .draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )
            .map_err(|error| error.to_string())
    }
}
