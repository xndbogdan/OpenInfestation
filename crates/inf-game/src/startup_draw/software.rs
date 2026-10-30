//! The startup carousel on the ordinary pixel backend.

use thiserror::Error;

use super::*;
use crate::{
    draw_queue::DrawInvocation,
    frontend_renderer::{FrontendRendererError, FrontendRendererState},
    menu_text::{draw_line_text, MenuGlyphQuad, MenuTextError, MenuTextHost},
    pixel_backend::PixelBackend,
    render_resources::{RenderResourceError, RenderResources},
    scene_resources::{ResolvedSceneCommand, SceneResourceBackend},
    screen_renderer_preparation::{prepare_screen_renderer, ScenePreparation},
    software_menu::{glyph_command, SoftwareGlyphError},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

struct Host<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

#[derive(Debug, Error)]
pub(crate) enum StartupSubmissionError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
}

impl StartupDrawHost for Host<'_, '_, '_> {
    type Error = StartupSubmissionError;
    fn begin_carousel(&mut self) -> Result<(), Self::Error> {
        self.scene.submit(ResolvedSceneCommand::BeginBatch)?;
        self.scene.reset_renderer();
        Ok(())
    }
    fn shadow_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_flat_quad_direct(quad)?)
    }
    fn cloth_quad(&mut self, quad: DirectShadeUvQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_shade_uv_quad_direct(quad)?)
    }
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        Ok(draw_line_text(
            request,
            &self.resources,
            &mut GlyphHost {
                scene: self.scene,
                resources: self.resources,
            },
        )?)
    }
}

struct GlyphHost<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

impl MenuTextHost for GlyphHost<'_, '_, '_> {
    type Error = SoftwareGlyphError;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        let mut command = glyph_command(self.resources, glyph)?;
        self.scene.pixels.draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0,
            },
            &mut command,
        )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StartupSoftwareDrawError {
    #[error("startup preamble: {0}")]
    Preamble(#[from] FrontendRendererError),
    #[error(transparent)]
    Draw(#[from] StartupDrawError<RenderResourceError, StartupSubmissionError>),
}

/// Linked +08 `004330E0` over the CPU scene: the `00439270` preamble, then
/// the carousel. `focal` is the shared template's `00546F20/24`.
pub(crate) fn draw<'f, 'r>(
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    published: &mut FrontendRendererState,
    state: &mut StartupState,
    focal: [i32; 2],
    sine_quarter: &[u16; 4096],
) -> Result<StartupDrawOutcome, StartupSoftwareDrawError> {
    prepare_screen_renderer(&mut ScenePreparation {
        scene: &mut *scene,
        state: published,
    })?;
    let frame = StartupDrawFrame {
        surface_pixels: [
            scene.pixels.frame.width() as i32,
            scene.pixels.frame.height() as i32,
        ],
        focal,
        material_shifts: scene.pixels.renderer.material_shifts,
        glyph_base_color: MenuColor::from_word(crate::menu_text::INITIAL_GLYPH_BASE_WORD),
        sine_quarter,
    };
    Ok(draw_startup_carousel(
        frame,
        state,
        &StartupResidentResources(resources),
        &mut Host { scene, resources },
    )?)
}
