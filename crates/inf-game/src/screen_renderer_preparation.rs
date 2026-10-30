//! Shared screen renderer preparation 00439270.
//!
//! Each required effect uses a fresh surface renderer lookup. The caller owns
//! the backend and publication slot; later context, dimensions and draw phases
//! retain their separate renderer custody.

use crate::frontend_renderer::{FrontendRendererError, FrontendRendererState};
use crate::scene_resources::{ResolvedSceneCommand, SceneResourceBackend};
use crate::software_scene::{SoftwareRendererId, SoftwareScene};

/// Required effects of 00439270, using the owning caller's existing error type.
/// Renderer identities are owned handles, never native pointer values. A host
/// with one backend must still validate each selected renderer.
pub trait ScreenRendererPreparationHost<E> {
    type Renderer: Copy;

    /// 00495800 reads the surface's current renderer separately at each call.
    fn renderer(&mut self) -> Result<Self::Renderer, E>;
    /// 004A43A0 replaces the command arena, even when its size is unchanged.
    /// The native allocation rounds the requested byte count to 16 bytes.
    fn replace_command_arena(&mut self, renderer: Self::Renderer, bytes: usize) -> Result<(), E>;
    /// 004A4400 begins/resets this renderer's deferred batch.
    fn begin_batch(&mut self, renderer: Self::Renderer) -> Result<(), E>;
    /// Publish the third renderer lookup in the caller's 00546E88 domain.
    fn publish_current_renderer(&mut self, renderer: Self::Renderer) -> Result<(), E>;
}

/// Complete 00439270 before the caller's later draw phases. Failures preserve
/// completed effects and propagate unchanged; this phase has no recovery or
/// allocation rollback and does not return a renderer for later reuse.
pub fn prepare_screen_renderer<H, E>(host: &mut H) -> Result<(), E>
where
    H: ScreenRendererPreparationHost<E>,
{
    let renderer = host.renderer()?;
    host.replace_command_arena(renderer, 0xc8000)?;
    let renderer = host.renderer()?;
    host.begin_batch(renderer)?;
    let renderer = host.renderer()?;
    host.publish_current_renderer(renderer)
}

/// `00439270` on the one CPU renderer, publishing it in the caller's slot.
pub(crate) struct ScenePreparation<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub state: &'a mut FrontendRendererState,
}

impl ScenePreparation<'_, '_, '_> {
    fn selected(&self, renderer: SoftwareRendererId) -> Result<(), FrontendRendererError> {
        if renderer != self.scene.id() {
            return Err(FrontendRendererError::Renderer);
        }
        Ok(())
    }
}

impl ScreenRendererPreparationHost<FrontendRendererError> for ScenePreparation<'_, '_, '_> {
    type Renderer = SoftwareRendererId;

    fn renderer(&mut self) -> Result<Self::Renderer, FrontendRendererError> {
        Ok(self.scene.id())
    }

    fn replace_command_arena(
        &mut self,
        renderer: Self::Renderer,
        bytes: usize,
    ) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.scene.replace_command_arena(
            bytes
                .try_into()
                .map_err(|_| FrontendRendererError::ArenaSize)?,
        )?;
        Ok(())
    }

    fn begin_batch(&mut self, renderer: Self::Renderer) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.scene.submit(ResolvedSceneCommand::BeginBatch)?;
        Ok(())
    }

    fn publish_current_renderer(
        &mut self,
        renderer: Self::Renderer,
    ) -> Result<(), FrontendRendererError> {
        self.selected(renderer)?;
        self.state.published_renderer = Some(renderer);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
