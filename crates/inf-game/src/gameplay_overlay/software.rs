//! Immediate native overlay calls on the ordinary world pixel backend.

use thiserror::Error;

use super::*;
use crate::{
    draw_queue::DrawInvocation,
    menu_text::{draw_raw_text, MenuGlyphQuad, MenuTextError, MenuTextHost},
    pixel_backend::PixelBackend,
    render_resources::{
        NativeColorLayout, RenderResourceData, RenderResourceError, RenderResourceKind,
        RenderResourceRequest, RenderResources,
    },
    scene_resources::SceneResourceBackend,
    software_menu::{glyph_command, SoftwareGlyphError},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

struct Resources<'a> {
    render: RenderResources<'a>,
    color_layout: NativeColorLayout,
}

impl MenuTextResources for Resources<'_> {
    type Error = RenderResourceError;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.render.string(argument)
    }
    fn font(&self, argument: i32) -> Result<&inf_formats::Font, Self::Error> {
        self.render.font(argument)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        self.render.style(argument)
    }
    fn sprite_dimensions(&self, argument: i32) -> Result<[u16; 2], Self::Error> {
        self.render.sprite_dimensions(argument)
    }
}

impl GameplayOverlayResources for Resources<'_> {
    fn material_word(&self, argument: i32) -> Result<u32, Self::Error> {
        let resource = self.render.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        })?;
        let RenderResourceData::Material(material) = resource.data else {
            unreachable!("typed material lookup")
        };
        Ok(self.color_layout.convert_material(material))
    }
}

struct Host<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

#[derive(Debug, Error)]
pub(crate) enum GameplayOverlaySubmissionError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
}

impl GameplayOverlayHost for Host<'_, '_, '_> {
    type Error = GameplayOverlaySubmissionError;
    fn flat_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_flat_quad_direct(quad)?)
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
    current_world: u32,
    screen_age_millis: i32,
) -> Result<(), GameplayOverlayError<RenderResourceError, GameplayOverlaySubmissionError>> {
    let input = GameplayOverlayRequest {
        current_world,
        surface_pixels: [
            scene.pixels.frame.width() as i32,
            scene.pixels.frame.height() as i32,
        ],
        screen_age_millis,
        glyph_base_color: MenuColor::from_word(crate::menu_text::INITIAL_GLYPH_BASE_WORD),
    };
    let resources_owner = Resources {
        render: resources,
        color_layout: scene.color_layout(),
    };
    draw_gameplay_overlay(input, &resources_owner, &mut Host { scene, resources })
}
