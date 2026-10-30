//! Native label callbacks and font passes on the shared CPU Direct3D target.
//!
//! 00453610 owns a temporary menu context; 00452570 publishes its first six
//! projection words for each row. Text corners from 004A3E30 are already
//! absolute screen pixels. The context origin must not be added to them.

use thiserror::Error;

use crate::{
    draw_queue::DrawInvocation,
    frontend_screen::FrontendProcessState,
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuRowDrawRequest,
        MenuState,
    },
    menu_text::{
        draw_menu_text_row, draw_raw_text, draw_row_text, MenuGlyphQuad, MenuRowTextRequest,
        MenuTextError, MenuTextHost, MenuTextOutcome, RawMenuTextRequest,
    },
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    pixel_backend::PixelBackend,
    render_resources::{
        RenderResourceError, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_resources::{ResolvedSceneCommand, SceneResourceBackend},
    software_frame::SoftwarePixelError,
    software_scene::{SoftwareScene, SoftwareSceneError},
};

/// Fields consumed by the supported row family. Other context-template fields
/// belong to their model/specialized callbacks, which remain explicit errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftwareMenuContext {
    pub focal: [i32; 2],
    pub viewport: [i32; 2],
    pub origin: [i32; 2],
    pub fade: [i32; 2],
}

/// Retain this owner across delegated MenuDrawHost calls. The native basis is
/// a separate global domain from the scene's current model/view basis.
#[derive(Default, Debug)]
pub struct SoftwareMenuState {
    pub context: Option<SoftwareMenuContext>,
    pub basis: Option<MenuBasis>,
    pub rows: usize,
    pub glyph_passes: usize,
}

pub struct SoftwareMenuRequest<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    /// Original display globals 00546F20/24, before the scene doubles them.
    pub focal: [i32; 2],
    pub process: &'a FrontendProcessState,
    pub state: &'a mut SoftwareMenuState,
}

#[derive(Debug, Error)]
pub enum SoftwareGlyphError {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Pixel(#[from] SoftwarePixelError),
}

#[derive(Debug, Error)]
pub enum SoftwareMenuError {
    #[error("menu colors have not been initialized by the frontend owner")]
    Colors,
    #[error("clip rectangle {0:?} is inverted (004AE870 raises)")]
    InvertedClip([i16; 4]),
    #[error("menu row requires controller context setup")]
    Context,
    #[error("row callback {callback:#010x} names slot {slot} outside its ring")]
    RingSlot { callback: u32, slot: usize },
    #[error("menu surface {requested:?} differs from the pixel target {actual:?}")]
    Surface {
        requested: [i32; 2],
        actual: [i32; 2],
    },
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
}

pub struct SoftwareMenu<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    focal: [i32; 2],
    process: &'a FrontendProcessState,
    state: &'a mut SoftwareMenuState,
}

impl<'a, 'f, 'r> SoftwareMenu<'a, 'f, 'r> {
    pub fn new(request: SoftwareMenuRequest<'a, 'f, 'r>) -> Result<Self, SoftwareMenuError> {
        request.process.colors().ok_or(SoftwareMenuError::Colors)?;
        Ok(Self {
            scene: request.scene,
            resources: request.resources,
            focal: request.focal,
            process: request.process,
            state: request.state,
        })
    }

    /// Generated row text shares the native row projection publication and
    /// glyph sink. Its caller retains the returned descriptor +18 advance.
    pub fn draw_row_text(
        &mut self,
        request: MenuRowTextRequest<'_>,
    ) -> Result<MenuTextOutcome, SoftwareMenuError> {
        self.publish_row_origin(request.context_origin_pixels)?;
        let resources = self.resources;
        let outcome = draw_row_text(request, &resources, self)?;
        self.state.rows += 1;
        Ok(outcome)
    }

    /// Raw overlays do not perform the row walk's context publication.
    pub fn draw_raw_text(
        &mut self,
        request: &mut RawMenuTextRequest<'_>,
    ) -> Result<(), SoftwareMenuError> {
        let resources = self.resources;
        draw_raw_text(request, &resources, self)?;
        Ok(())
    }

    /// A controller's own scene work between its setup and its rows.
    pub(crate) fn scene_mut(&mut self) -> &mut SoftwareScene<'f, 'r> {
        self.scene
    }

    pub(crate) fn resources(&self) -> RenderResources<'r> {
        self.resources
    }

    /// The current shared 0055F23C/40 domain, which may still be the scene's
    /// origin before the first row publishes the local controller context.
    pub fn published_origin_pixels(&self) -> [i32; 2] {
        self.scene.primitive_state.projection.center
    }

    pub(crate) fn publish_row_origin(&mut self, origin: [i32; 2]) -> Result<(), SoftwareMenuError> {
        let context = self
            .state
            .context
            .as_mut()
            .ok_or(SoftwareMenuError::Context)?;
        context.origin = origin;
        // 00452C3C copies exactly +38..+4F. Fade/fog and the inherited view
        // are NOT rewritten here, even though the local context has +64/+68.
        self.scene
            .publish_model_window(crate::process_model_graphics::ModelProjectionWindow {
                focal: context.focal,
                viewport: context.viewport,
                center: context.origin,
            })?;
        Ok(())
    }
}

impl MenuDrawHost for SoftwareMenu<'_, '_, '_> {
    type Error = SoftwareMenuError;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        let actual = self.scene.surface_pixels()?;
        if setup.surface_pixels != actual {
            return Err(SoftwareMenuError::Surface {
                requested: setup.surface_pixels,
                actual,
            });
        }
        self.state.context = Some(SoftwareMenuContext {
            focal: self.focal,
            viewport: setup.surface_pixels,
            origin: [setup.surface_pixels[0] / 2, setup.context_4c],
            fade: [setup.context_64, setup.context_68],
        });
        // 0043B5D0 calls 00490030 BEFORE 00490130. The first operation uses
        // the old ambient bytes when it derives the shared effect color.
        self.scene.submit(ResolvedSceneCommand::DirectionalLight {
            direction: setup.light_direction,
            rgb: setup.light_color,
        })?;
        self.scene.submit(ResolvedSceneCommand::AmbientLight(
            setup.ambient_color.map(i32::from),
        ))?;
        Ok(())
    }

    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.state.basis = Some(basis);
        Ok(())
    }

    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok(self.scene.pixels.pipeline.viewport)
    }

    /// Renderer `+24` (`004AE870`): the left and top clamp at zero, the
    /// right and bottom at the surface; an inverted rectangle is an error.
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        let [left, top, right, bottom] = clip;
        if right < left || bottom < top {
            return Err(SoftwareMenuError::InvertedClip(clip));
        }
        let [width, height] = self.scene.surface_pixels()?;
        let edge = |value: i16, size: i32| i32::from(value).min(size) as i16;
        self.scene.pixels.pipeline.viewport = [
            left.max(0),
            top.max(0),
            edge(right, width),
            edge(bottom, height),
        ];
        Ok(())
    }

    fn colors(&self) -> MenuDrawColors {
        // Publication can replace these colors between rows. Construction
        // requires initialization; the process owner cannot uninitialize them.
        self.process.colors().expect("initialized menu colors")
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Self::Error> {
        self.publish_row_origin(request.context_origin_pixels)?;
        let resources = self.resources;
        if request.callback_address == 0x0048_3550 {
            // Quit wrapper copies seven descriptor DWORDs, shifts ONLY local
            // X by (-width*2)/100, then calls453810 with right text. It never
            // copies the descriptor back, including on a partial text error.
            let mut local = quit_label_request(*request, self.scene.pixels.frame.width() as i32);
            draw_menu_text_row(&mut local, &resources, self)?;
        } else {
            draw_menu_text_row(request, &resources, self)?;
        }
        self.state.rows += 1;
        Ok(())
    }
}

fn quit_label_request(mut request: MenuRowDrawRequest, surface_width: i32) -> MenuRowDrawRequest {
    let offset = surface_width.wrapping_neg().wrapping_mul(2) / 100;
    request.descriptor.position_pixels[0] =
        request.descriptor.position_pixels[0].wrapping_add(offset as i16);
    request.callback_address = 0x0048_34f0;
    request
}

impl MenuTextHost for SoftwareMenu<'_, '_, '_> {
    type Error = SoftwareGlyphError;

    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        let mut command = glyph_command(self.resources, glyph)?;
        // The font calls virtual+A4 directly for BOTH passes. Do not enqueue,
        // merge the passes, select one variant early, or flush unrelated work.
        self.scene.pixels.draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0,
            },
            &mut command,
        )?;
        self.state.glyph_passes += 1;
        Ok(())
    }
}

pub(crate) fn glyph_command(
    resources: RenderResources<'_>,
    glyph: MenuGlyphQuad,
) -> Result<PrimitiveCommand<'_>, RenderResourceError> {
    let resource = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Sprite,
        argument: glyph.sprite_argument,
    })?;
    let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadNear, 0, 3);
    for (index, [x, y]) in glyph.corners.into_iter().enumerate() {
        command.write_u32(index * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
        command.write_u32(index * 8 + 4, 0x40);
    }
    command.write_u32(0x20, glyph.pass as u32);
    command.write_texture(0x24, Some(resource));
    for (channel, value) in glyph.color.rgb.into_iter().enumerate() {
        command.payload[0x28 + channel] = Some(value);
    }
    command.payload[0x2b] = glyph.color.high_byte;
    Ok(command)
}

#[cfg(test)]
#[path = "software_menu_tests.rs"]
mod tests;
