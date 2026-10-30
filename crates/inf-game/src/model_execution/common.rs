//! Common Near/Far inline05: complete 004A1150..004A12D7.
//! Same invocation and selected Common table. Late 004ED9C0 is distinct.
use super::*;

impl<'a, 't> Run<'a, '_, 't> {
    pub(super) fn common_block_05<H: ModelExecutionHost>(
        &mut self,
        frame: &mut Invocation<'a, '_, '_>,
        instruction: &ModelInstruction,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        let ModelInstructionKind::NestedProgram(nested) = &instruction.kind else {
            return Err(ModelExecutionError::Record(instruction.span.start));
        };
        let args: [u16; 12] = nested
            .arguments
            .as_slice()
            .try_into()
            .map_err(|_| ModelExecutionError::Record(instruction.span.start))?;
        // Save exactly position+08 and ten basis words+14. Saved basis+A0,
        // registers, state words and the active byte views retain inner writes.
        let saved_position = frame.context.position;
        let saved_basis = frame.context.basis;
        frame.context.position = frame.vertices.world(args[0] as i16).map_err(vertex_fault)?;
        for (word, plane) in args[1..4].iter().copied().zip([
            crate::menu_basis::RotationPlane::Xz,
            crate::menu_basis::RotationPlane::Yz,
            crate::menu_basis::RotationPlane::Xy,
        ]) {
            // All three packed calls and rotations execute, including zero.
            let angle = frame
                .values
                .evaluate_packed(word, host)
                .map_err(ModelExecutionError::Value)?;
            self.executor.basis_math.rotate_in_plane(
                &mut frame.context.basis,
                i32::from(angle),
                plane,
            );
        }
        frame.transform = self.publish(&frame.context, host)?;
        frame.projector.transform = frame.transform;
        frame
            .vertices
            .initialize_projection_ranges(
                frame.resource.model,
                [args[4], args[6], args[8]].map(|v| i32::from(v as i16)),
                &mut frame.projector,
            )
            .map_err(lift_initialization)?;
        frame
            .vertices
            .initialize_world_ranges(
                frame.resource.model,
                &frame.context,
                [args[5], args[6], args[7]].map(|v| i32::from(v as i16)),
            )
            .map_err(lift_initialization)?;
        frame.shades.initialize_normal_ranges(
            frame.resource.model,
            frame.resource.source.bytes,
            &frame.context.basis,
            self.environment.lighting,
            [args[9], args[10], args[11]].map(|v| i32::from(v as i16)),
        )?;
        // No allocation, cull, dispatch selection, ambient seed or child values.
        self.program(frame, &nested.program, depth + 1, host)?;
        // Native restore is a normal-return suffix, never finally/error cleanup.
        frame.context.position = saved_position;
        frame.context.basis = saved_basis;
        frame.transform = self.publish(&frame.context, host)?;
        frame.projector.transform = frame.transform;
        Ok(())
    }
}
