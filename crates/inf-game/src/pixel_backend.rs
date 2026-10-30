//! Typed pixel-dispatch boundary behind the deferred draw queue.
//!
//! Retail callbacks such as `004A38F0` are three-byte virtual forwards
//! (`mov eax,[ecx]` then `jmp [eax+offset]`), not rasterizers. This owner
//! preserves that dispatch identity through
//! [`PrimitiveCallback::renderer_virtual_offset`](crate::model_primitives::PrimitiveCallback::renderer_virtual_offset)
//! and routes flushed queue commands to one required backend operation.
//! Actual pixel rasterization, palette/texture sampling, viewport clipping,
//! and depth handling remain backend work.

use crate::{
    draw_queue::{DrawInvocation, DrawQueue, DrawQueueError},
    model_primitives::PrimitiveCommand,
};

/// Required pixel sink for flushed deferred commands. There is a single
/// data-bearing call rather than a ladder of per-callback methods: the
/// callback enum inside `command` already carries the native dispatch
/// identity, and `invocation` carries the arena/receiver bookkeeping.
/// The backend resolves its own renderer allocation identity from
/// `invocation.receiver_offset`; portable renderer handles are never
/// fabricated into native addresses here.
pub trait PixelBackend<'a> {
    type Error;
    fn draw(
        &mut self,
        invocation: DrawInvocation,
        command: &mut PrimitiveCommand<'a>,
    ) -> Result<(), Self::Error>;
}

/// Flush `queue` in native replay order and route every command to
/// `backend`. Successful replay neither resets nor consumes the batch, so a
/// second flush replays the same commands. Backend failures stop the queue
/// through the shared containment rule and retain completed side effects.
pub fn flush_pixels<'a, E, B: PixelBackend<'a, Error = E>>(
    queue: &mut DrawQueue<PrimitiveCommand<'a>>,
    backend: &mut B,
) -> Result<usize, DrawQueueError<E>> {
    queue.flush(|invocation, command| backend.draw(invocation, command))
}

#[cfg(test)]
#[path = "pixel_backend_tests.rs"]
mod tests;
