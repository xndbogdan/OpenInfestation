//! Concrete renderer adapter for the name-entry overlay (00459310).
//!
//! Stars and glyphs are immediate virtual calls. The one explicit overlay
//! flush executes the backdrop model work already queued by 0045A560.

use thiserror::Error;

use crate::{
    draw_queue::DrawInvocation,
    frontend_scene::{FooterQuad, FrontendSceneHost, SceneCommand, StarQuad},
    menu_text::{
        draw_line_text, draw_raw_text, LineTextRequest, MenuGlyphQuad, MenuTextError, MenuTextHost,
        RawMenuTextRequest,
    },
    name_entry_draw::NameEntryDrawHost,
    pixel_backend::PixelBackend,
    render_resources::{RenderResourceError, RenderResources},
    scene_resources::{ResidentSceneHost, SceneResourceError},
    software_menu::{glyph_command, SoftwareGlyphError},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

#[derive(Debug, Error)]
pub enum NameEntryPresentationError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Submission(#[from] SceneResourceError<SoftwareSceneError>),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
}

/// No menu controller or row context is created by 00459310. Its text
/// requests carry the caller's already-published pixel origin explicitly.
pub struct SoftwareNameEntry<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub glyph_passes: usize,
}

impl<'a, 'f, 'r> SoftwareNameEntry<'a, 'f, 'r> {
    pub fn new(scene: &'a mut SoftwareScene<'f, 'r>, resources: RenderResources<'r>) -> Self {
        Self {
            scene,
            resources,
            glyph_passes: 0,
        }
    }

    fn resident(&mut self) -> ResidentSceneHost<'r, '_, SoftwareScene<'f, 'r>> {
        ResidentSceneHost {
            resources: self.resources,
            backend: self.scene,
        }
    }
}

impl FrontendSceneHost for SoftwareNameEntry<'_, '_, '_> {
    type Error = NameEntryPresentationError;

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

impl NameEntryDrawHost for SoftwareNameEntry<'_, '_, '_> {
    fn star(&mut self, quad: StarQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_star_direct(quad)?)
    }

    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        let resources = self.resources;
        Ok(draw_line_text(request, &resources, self)?)
    }

    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        let resources = self.resources;
        Ok(draw_raw_text(request, &resources, self)?)
    }

    fn highlight(&mut self, quad: FooterQuad) -> Result<(), Self::Error> {
        self.submit(SceneCommand::Footer(quad))
    }
}

impl MenuTextHost for SoftwareNameEntry<'_, '_, '_> {
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
        self.glyph_passes += 1;
        Ok(())
    }
}

#[cfg(test)]
#[path = "name_entry_presentation_tests.rs"]
mod tests;
