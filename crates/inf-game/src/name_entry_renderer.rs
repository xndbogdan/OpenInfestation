//! Full name-entry draw over the resident CPU renderer.
//!
//! This adapter binds one live registry context to one renderer explicitly.
//! Native multi-renderer dispatch belongs to the generic frame owner; this
//! portable surface cannot redirect a context to a different pixel backend.

use thiserror::Error;

use crate::{
    frontend_renderer::FrontendRenderContext,
    frontend_scene::{FrontendSceneHost, SceneCommand, SceneProjection},
    input::InputContextId,
    menu::MenuColor,
    menu_basis::MenuBasisMath,
    name_entry_draw::{draw_name_entry_overlay, NameEntryDrawError, NameEntryDrawFrame},
    name_entry_frame::{NameEntryFrameHost, NameEntryFrameInput},
    name_entry_presentation::{NameEntryPresentationError, SoftwareNameEntry},
    name_entry_scene::{draw_name_entry_scene, NameEntrySceneFrame},
    render_resources::{RenderResourceError, RenderResources},
    screen_registry::{ContextOwnerId, RegistryError, ScreenAttachment, ScreenRegistry},
    screen_renderer_preparation::ScreenRendererPreparationHost,
    software_frame::SoftwareSurface,
    software_scene::{SoftwareRendererId, SoftwareScene, SoftwareSceneError},
};

/// The portable context-to-renderer association, independent of the selected
/// surface. Native 0045936B captures session +648 (initialized to 00546E88 by
/// 00438780), not a scheduler-owner identity. This single-backend adapter maps
/// that context to the drawing attachment's registry owner explicitly; the
/// association must match that owner. The generic frame host permits other
/// stable context identities and distinct backdrop/overlay renderers.
#[derive(Clone, Copy, Debug)]
pub struct SoftwareNameEntryContext {
    pub owner: ContextOwnerId,
    pub renderer: SoftwareRendererId,
}

#[derive(Default, Debug)]
pub struct NameEntryRendererState {
    pub published_renderer: Option<SoftwareRendererId>,
    pub caller_context: Option<ContextOwnerId>,
    pub glyph_passes: usize,
    pub finished_draws: usize,
}

#[derive(Debug, Error)]
pub enum NameEntryRendererError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Presentation(#[from] NameEntryPresentationError),
    #[error(transparent)]
    Overlay(#[from] NameEntryDrawError<RenderResourceError, NameEntryPresentationError>),
    #[error("captured context has no association with this pixel renderer")]
    ContextRenderer,
    #[error("selected renderer does not own this pixel surface")]
    Renderer,
    #[error(
        "surface dimensions {actual:?} differ from renderer {expected:?}; recreation is required"
    )]
    Surface {
        actual: [u32; 2],
        expected: [u32; 2],
    },
    #[error("draw arena size does not fit a native DWORD")]
    ArenaSize,
}

pub struct SoftwareNameEntryFrame<'a, 'f, 'r, 'w> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub registry: &'a ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub context: SoftwareNameEntryContext,
    /// Shared template/focal inputs, never the caller context's projection.
    /// The backdrop replaces viewport dimensions with a fresh surface read.
    pub projection: SceneProjection,
    pub sine_quarter: &'a [u16; 4096],
    pub context_origin_pixels: [i32; 2],
    pub base_color: MenuColor,
    pub state: &'a mut NameEntryRendererState,
    pub surface: SoftwareSurface<'w>,
}

impl SoftwareNameEntryFrame<'_, '_, '_, '_> {
    fn selected(&self, renderer: SoftwareRendererId) -> Result<(), NameEntryRendererError> {
        if renderer == self.scene.id() {
            Ok(())
        } else {
            Err(NameEntryRendererError::Renderer)
        }
    }

    fn dimensions(&self) -> [u32; 2] {
        self.surface.dimensions(self.scene.pixels.frame)
    }
}

impl ScreenRendererPreparationHost<NameEntryRendererError>
    for SoftwareNameEntryFrame<'_, '_, '_, '_>
{
    type Renderer = SoftwareRendererId;

    fn renderer(&mut self) -> Result<Self::Renderer, NameEntryRendererError> {
        Ok(self.scene.id())
    }

    fn replace_command_arena(
        &mut self,
        renderer: Self::Renderer,
        bytes: usize,
    ) -> Result<(), NameEntryRendererError> {
        self.selected(renderer)?;
        self.scene.replace_command_arena(
            bytes
                .try_into()
                .map_err(|_| NameEntryRendererError::ArenaSize)?,
        )?;
        Ok(())
    }

    fn begin_batch(&mut self, renderer: Self::Renderer) -> Result<(), NameEntryRendererError> {
        self.selected(renderer)?;
        SoftwareNameEntry::new(self.scene, self.resources).submit(SceneCommand::BeginBatch)?;
        Ok(())
    }

    fn publish_current_renderer(
        &mut self,
        renderer: Self::Renderer,
    ) -> Result<(), NameEntryRendererError> {
        self.selected(renderer)?;
        self.state.published_renderer = Some(renderer);
        Ok(())
    }
}

impl NameEntryFrameHost for SoftwareNameEntryFrame<'_, '_, '_, '_> {
    type Error = NameEntryRendererError;
    type Context = ContextOwnerId;

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.dimensions().map(|value| value as i32))
    }

    fn capture_caller_context(&mut self) -> Result<Self::Context, Self::Error> {
        // Session +648 is independent of the newest session +48 member.
        // Another owner's linked screen can be newer in the same session;
        // it must not redirect this drawing attachment's caller context.
        let owner = self.attachment.context_owner;
        self.registry.backend(owner)?;
        self.state.caller_context = Some(owner);
        Ok(owner)
    }

    fn reset_renderer(&mut self, renderer: Self::Renderer) -> Result<(), Self::Error> {
        self.selected(renderer)?;
        self.scene.reset_renderer();
        Ok(())
    }

    fn validate_surface(&mut self) -> Result<(), Self::Error> {
        self.surface
            .validate_dimensions(self.scene.pixels.frame)
            .map_err(|mismatch| NameEntryRendererError::Surface {
                actual: mismatch.actual,
                expected: mismatch.expected,
            })
    }

    fn draw_backdrop(
        &mut self,
        context: Self::Context,
        input: NameEntryFrameInput<'_>,
    ) -> Result<(), Self::Error> {
        // Resolve the captured owner again. A caller context does not supply
        // projection values to 0045A560; only its renderer DWORD is used.
        self.registry.backend(context)?;
        if context != self.context.owner || self.context.renderer != self.scene.id() {
            return Err(NameEntryRendererError::ContextRenderer);
        }
        let mut projection = self.projection;
        projection.viewport = self.surface_pixels()?;
        draw_name_entry_scene(
            NameEntrySceneFrame {
                projection,
                age_millis: input.age_millis,
                animation: input.animation,
                selector_one: input.selector_one,
            },
            &MenuBasisMath::new(*self.sine_quarter),
            &mut SoftwareNameEntry::new(self.scene, self.resources),
        )?;
        Ok(())
    }

    fn draw_overlay(
        &mut self,
        renderer: Self::Renderer,
        input: NameEntryFrameInput<'_>,
        surface_pixels: [i32; 2],
    ) -> Result<(), Self::Error> {
        self.selected(renderer)?;
        let mut host = SoftwareNameEntry::new(self.scene, self.resources);
        let result = draw_name_entry_overlay(
            NameEntryDrawFrame {
                state: input.state,
                age_millis: input.age_millis,
                surface_pixels,
                focal: self.projection.focal,
                context_origin_pixels: self.context_origin_pixels,
                base_color: self.base_color,
                sine_quarter: self.sine_quarter,
            },
            &self.resources,
            &mut host,
        );
        self.state.glyph_passes += host.glyph_passes;
        Ok(result?)
    }

    fn finish_surface_draw(&mut self) -> Result<(), Self::Error> {
        // 004957F0 unlocks a native surface only when it was locked. CPU
        // pixels never acquire that lock; completion does not present/clear.
        self.state.finished_draws += 1;
        Ok(())
    }
}
