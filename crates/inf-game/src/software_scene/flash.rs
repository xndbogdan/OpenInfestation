//! Deferred world flash quads share the existing command arena and pixels.
use super::*;
use crate::d3d_effect::{read_effect_quad, EffectPixelError};
use crate::software_frame::SoftwarePixelError;
use crate::world_flash::{WorldFlashDrawHost, WorldFlashError, WorldFlashState};

///All admitted PrimitiveCommand producers share this recorder. Known stores
///overwrite the same allocation bytes; untouched bytes survive a rewind.
///Relocated pointers invalidate four bytes instead of fabricating an address.
pub(super) fn record_primitive_memory(command: &PrimitiveCommand<'_>, memory: &mut [Option<u8>]) {
    for (i, value) in std::iter::once(command.callback.native_address())
        .chain(command.header_words)
        .enumerate()
    {
        memory[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes().map(Some));
    }
    // Branch-free, so the loop vectorises: a known byte replaces, an
    // unknown one keeps what was there.
    for (target, value) in memory[16..].iter_mut().zip(&command.payload) {
        *target = value.or(*target);
    }
    if let Some((offset, _)) = command.texture {
        memory[16 + offset..16 + offset + 4].fill(None);
    }
}

#[derive(Debug, Error)]
pub enum SoftwareFlashError {
    #[error(
        "world flash leaves payload+20 to unknown native allocation carry, and that word changes its quad"
    )]
    UnknownAllocationMode,
    #[error(transparent)]
    Pixel(SoftwarePixelError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Identity(#[from] SoftwareSceneServiceError),
    #[error(transparent)]
    Queue(#[from] DrawQueueFault),
}

impl<'r> SoftwareScene<'_, 'r> {
    pub fn draw_world_flash(
        &mut self,
        flash: &mut WorldFlashState,
        viewport: [u32; 2],
        executable: inf_formats::FrontendExecutable<'_>,
    ) -> Result<bool, WorldFlashError<SoftwareFlashError>> {
        let allocation_mode = self.queue.next_payload_word(0x20);
        flash.draw(viewport, allocation_mode, executable, self)
    }
}

impl<'r> WorldFlashDrawHost<'r> for SoftwareScene<'_, 'r> {
    type Error = SoftwareFlashError;
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(slot),
        };
        if slot < 0 || slot as usize > self.resources.pool_capacity(RenderResourceKind::Sprite) {
            return Ok(None);
        }
        // An in-range missing allocation requires the real lazy supplier.
        // It is not the native negative/out-of-range NULL path.
        Ok(Some(self.resources.resolve(request)?))
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        Ok(SoftwareScene::material_sorting(self))
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Ok(self.texture_identities.resolve(texture)?)
    }
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        //Explicit per-feature admission: the actual pixel callback reads this
        //unwritten field. Unknown carry enqueues only when no word changes
        //the quad (settle_effect_mode); do not poison the shared queue with
        //a packet its flush cannot draw.
        if command.read_u32(0x20).is_none() {
            let settled = match read_effect_quad(&command) {
                Ok(Some(quad)) => self.pixels.effect_mode(&quad).map(drop),
                Ok(None) => Ok(()),
                Err(error) => Err(error.into()),
            };
            settled.map_err(|error| match error {
                SoftwarePixelError::Effect(EffectPixelError::Unknown { offset: 0x20, .. }) => {
                    SoftwareFlashError::UnknownAllocationMode
                }
                error => SoftwareFlashError::Pixel(error),
            })?;
        }
        self.queue.submit(DrawSubmission {
            depth: command.depth,
            payload_blocks: command.payload_blocks,
            receiver_offset: 0,
            command,
        })?;
        Ok(())
    }
}
