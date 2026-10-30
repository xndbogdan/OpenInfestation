use super::*;
use crate::{
    draw_queue::{DrawQueue, DrawSubmission},
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
};

#[test]
fn primitive_thunks_map_to_their_proven_renderer_slots() {
    // Proven with MinGW `objdump -d -Mintel` against `infestation/Infest.exe`:
    // each entry disassembles to `mov eax,[ecx]` then `jmp [eax+offset]`.
    let expected = [
        (PrimitiveCallback::TexturedTriangleNear, 0x004a_38f0, 0x7c),
        (PrimitiveCallback::TexturedTriangleFar, 0x0044_ea50, 0x80),
        (PrimitiveCallback::FlatTriangleNear, 0x004a_39b0, 0x4c),
        (PrimitiveCallback::FlatTriangleFar, 0x004a_39e0, 0x50),
        (PrimitiveCallback::TexturedQuadNear, 0x0046_6150, 0x9c),
        (PrimitiveCallback::TexturedQuadFar, 0x0046_6160, 0xa0),
        (PrimitiveCallback::TexturedUvTriangleNear, 0x004a_3990, 0x6c),
        (PrimitiveCallback::TexturedUvTriangleFar, 0x004a_39c0, 0x70),
        (PrimitiveCallback::TexturedUvQuadNear, 0x004a_3a20, 0xb4),
        (PrimitiveCallback::TexturedUvQuadFar, 0x004a_3a60, 0xb8),
        (PrimitiveCallback::EffectQuadNear, 0x0040_ef30, 0xa4),
        (PrimitiveCallback::EffectQuadFar, 0x0041_1d50, 0xa8),
        (PrimitiveCallback::D2TriangleNear, 0x004a_3930, 0x64),
        (PrimitiveCallback::D2TriangleFar, 0x004a_3960, 0x68),
        (PrimitiveCallback::D6QuadNear, 0x0044_ea60, 0xac),
        (PrimitiveCallback::D6QuadFar, 0x0044_ea70, 0xb0),
        (PrimitiveCallback::ColoredQuadNear, 0x0044_ea80, 0x94),
        (PrimitiveCallback::ColoredQuadFar, 0x0044_ea90, 0x98),
        (PrimitiveCallback::SolidTriangleNear, 0x004a_38e0, 0x3c),
        (PrimitiveCallback::SolidTriangleFar, 0x004a_3920, 0x40),
    ];
    for (callback, address, offset) in expected {
        assert_eq!(callback.native_address(), address, "{callback:?}");
        assert_eq!(
            callback.renderer_virtual_offset(),
            Some(offset),
            "{callback:?}"
        );
    }
}

#[derive(Default)]
struct RecordingBackend {
    seen: Vec<(DrawInvocation, PrimitiveCallback, u32, usize, u32)>,
}

impl<'a> PixelBackend<'a> for RecordingBackend {
    type Error = &'static str;
    fn draw(
        &mut self,
        invocation: DrawInvocation,
        command: &mut PrimitiveCommand<'a>,
    ) -> Result<(), Self::Error> {
        self.seen.push((
            invocation,
            command.callback,
            command.depth,
            command.payload_blocks,
            invocation.receiver_offset,
        ));
        Ok(())
    }
}

fn submit(
    queue: &mut DrawQueue<PrimitiveCommand<'static>>,
    callback: PrimitiveCallback,
    depth: u32,
    receiver_offset: u32,
) {
    let mut command = PrimitiveCommand::new(callback, depth, 3);
    command.write_u32(0, depth);
    queue
        .submit(DrawSubmission {
            depth,
            payload_blocks: 3,
            receiver_offset,
            command,
        })
        .unwrap();
}

#[test]
fn flush_routes_every_command_with_native_order_and_receiver_identity() {
    let mut queue = DrawQueue::new(4096).unwrap();
    submit(&mut queue, PrimitiveCallback::FlatTriangleNear, 10, 4);
    submit(&mut queue, PrimitiveCallback::TexturedTriangleNear, 30, 8);
    submit(&mut queue, PrimitiveCallback::EffectQuadFar, 20, 12);
    queue.end_group().unwrap();

    let mut backend = RecordingBackend::default();
    let completed = flush_pixels(&mut queue, &mut backend).unwrap();
    assert_eq!(completed, 3);
    // Sorted replay is descending signed depth: 30, 20, 10.
    let order: Vec<_> = backend.seen.iter().map(|entry| entry.1).collect();
    assert_eq!(
        order,
        [
            PrimitiveCallback::TexturedTriangleNear,
            PrimitiveCallback::EffectQuadFar,
            PrimitiveCallback::FlatTriangleNear,
        ]
    );
    assert_eq!(backend.seen[0].4, 8);
    assert_eq!(backend.seen[1].4, 12);
    assert_eq!(backend.seen[2].4, 4);
    // Arena offsets are submission identities, independent of sorted replay:
    // Flat(10) was submitted first, then Textured(30), then Effect(20).
    assert!(backend.seen[2].0.command_offset < backend.seen[0].0.command_offset);
    assert!(backend.seen[0].0.command_offset < backend.seen[1].0.command_offset);
    // Wrapping receiver arithmetic matches 004A444C/004A448D.
    assert_eq!(backend.seen[0].0.native_receiver(0xffff_ffff), 7);

    // Successful replay does not consume the batch.
    let replayed = flush_pixels(&mut queue, &mut RecordingBackend::default()).unwrap();
    assert_eq!(replayed, 3);
}

#[test]
fn backend_failure_stops_the_queue_and_retains_completed_draws() {
    struct FailingBackend {
        calls: usize,
    }
    impl<'a> PixelBackend<'a> for FailingBackend {
        type Error = &'static str;
        fn draw(
            &mut self,
            _: DrawInvocation,
            _: &mut PrimitiveCommand<'a>,
        ) -> Result<(), Self::Error> {
            self.calls += 1;
            if self.calls == 2 {
                return Err("pixel sink failed");
            }
            Ok(())
        }
    }

    let mut queue = DrawQueue::new(4096).unwrap();
    submit(&mut queue, PrimitiveCallback::FlatTriangleNear, 30, 0);
    submit(&mut queue, PrimitiveCallback::FlatTriangleNear, 20, 0);
    submit(&mut queue, PrimitiveCallback::FlatTriangleNear, 10, 0);
    queue.end_group().unwrap();

    let mut backend = FailingBackend { calls: 0 };
    let error = flush_pixels(&mut queue, &mut backend).unwrap_err();
    assert!(matches!(error, DrawQueueError::Host("pixel sink failed")));
    assert_eq!(backend.calls, 2);
    assert!(queue.state().stopped);
}
