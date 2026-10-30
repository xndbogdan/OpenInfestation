//! The results page over the software scene.

use thiserror::Error;

use super::*;
use crate::{
    draw_queue::DrawInvocation,
    menu_text::{draw_line_text, draw_raw_text, MenuGlyphQuad, MenuTextError, MenuTextHost},
    pixel_backend::PixelBackend,
    render_resources::{
        RenderResourceData, RenderResourceError, RenderResourceKind, RenderResourceRequest,
        RenderResources,
    },
    scene_resources::{ResidentSceneHost, SceneResourceError},
    software_menu::{glyph_command, SoftwareGlyphError},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

impl ResultsPageResources for RenderResources<'_> {
    fn model_extent(&self, slot: i16) -> Result<u32, Self::Error> {
        let resource = self.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(slot),
        })?;
        let RenderResourceData::Model(model) = resource.data else {
            unreachable!("typed model lookup")
        };
        Ok(model.header.visibility_extent)
    }
}

#[derive(Debug, Error)]
pub(crate) enum ResultsPageSubmissionError {
    #[error(transparent)]
    Scene(#[from] SceneResourceError<SoftwareSceneError>),
    #[error(transparent)]
    Sprite(#[from] SoftwareSceneError),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
}

struct Host<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

impl<'f, 'r> Host<'_, 'f, 'r> {
    fn resident(&mut self) -> ResidentSceneHost<'r, '_, SoftwareScene<'f, 'r>> {
        ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        }
    }
}

impl FrontendSceneHost for Host<'_, '_, '_> {
    type Error = ResultsPageSubmissionError;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        Ok(self.resident().submit(command)?)
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        Ok(self.resident().footer_style_word()?)
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.resident().surface_pixels()?)
    }
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error> {
        Ok(self.resident().sprite_dimensions(slot)?)
    }
}

impl ResultsPageHost for Host<'_, '_, '_> {
    fn sprite(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_sprite_direct(quad)?)
    }
    fn reset_renderer(&mut self) -> Result<(), Self::Error> {
        self.scene.reset_renderer();
        Ok(())
    }
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        Ok(draw_raw_text(
            request,
            &self.resources,
            &mut GlyphHost {
                scene: self.scene,
                resources: self.resources,
            },
        )?)
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

pub(crate) fn draw<'f, 'r>(
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    page: &ResultsPage<'_>,
    math: &MenuBasisMath,
) -> Result<(), ResultsPageError<RenderResourceError, ResultsPageSubmissionError>> {
    draw_results_page(page, &resources, math, &mut Host { scene, resources })
}
