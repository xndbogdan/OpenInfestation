//! Distinct 004ED610 root,004ED7E0 child and table00544764. This control walk
//! never selects common Near/Far, initializes shades or submits visible packets.
//! Decoded records exclude native pointer alias mutation and miss hydration.
//! Range views retain actual process scratch bytes, while reached unknown
//! values remain errors; this path never seeds visual projection/shade data.

use super::*;
use crate::model_vertices::ModelWorldStreamRequest;

struct LateInvocation<'a, 's> {
    resource: ModelResource<'a>,
    context: ModelContext,
    transform: ModelTransform,
    values: ModelValues,
    vertices: ModelVertices,
    state_words: &'s [u16],
    parent_vertices: Option<ModelVertices>,
}

impl<'a, 't> Run<'a, '_, 't> {
    pub(super) fn late_root<H: ModelExecutionHost>(
        &mut self,
        argument: i32,
        context: ModelContext,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        // 004ED65B publishes before any H8 lookup/header read. Negative/outside
        // slots have a NULL result which the root dereferences, unlike child0E.
        let transform = self.publish(&context, host)?;
        let slot = argument as i16;
        if slot < 0
            || slot as usize
                > self
                    .executor
                    .resources
                    .pool_capacity(RenderResourceKind::Model)
        {
            return Err(ModelExecutionError::InvalidLateRoot(slot));
        }
        let resource = self
            .resolve(argument)?
            .ok_or(ModelExecutionError::InvalidLateRoot(slot))?;
        let values = ModelValues::new_invocation(Arc::clone(&self.executor.sine_table));
        self.late_invoke(
            InvocationSeed {
                resource,
                context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 0,
                exact_origin: None,
            },
            Some(transform),
            host,
        )
    }

    fn late_invoke<H: ModelExecutionHost>(
        &mut self,
        seed: InvocationSeed<'a, '_>,
        published: Option<ModelTransform>,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        let InvocationSeed {
            resource,
            context,
            values,
            state_words,
            parent_vertices,
            depth,
            publications,
            exact_origin: _,
        } = seed;
        if depth > self.executor.limits.invocation_depth {
            return Err(ModelExecutionError::Limit);
        }
        self.outcome.invocations += 1;
        let mut transform = published;
        for _ in 0..publications {
            transform = Some(self.publish(&context, host)?);
        }
        let transform = transform.ok_or(ModelExecutionError::Record(
            resource.model.command_span.start,
        ))?;
        let allocation: ModelScratchAllocation = self
            .scratch
            .allocate(
                &self.scratch_scope,
                resource.model.header.vertex_slot_count,
                resource.model.header.shade_slot_count,
            )
            .map_err(scratch_allocation_error)?;
        let mut frame = LateInvocation {
            resource,
            context,
            transform,
            values,
            state_words,
            parent_vertices,
            vertices: ModelVertices::in_scratch(self.scratch, allocation.clone())
                .map_err(vertex_fault)?,
        };
        let [a, b, c, _] = resource.model.header.vertex_ranges;
        frame
            .vertices
            .initialize_world_ranges(
                resource.model,
                &frame.context,
                [i32::from(a), i32::from(b), i32::from(c)],
            )
            .map_err(lift_initialization)?;
        self.late_program(&mut frame, &resource.model.program, depth, host)?;
        // Only a normal terminating word reaches 004EBEE0. Errors preserve the
        // registered scratch/callback/context prefix and stop the enclosing owner.
        self.scratch
            .retire(&self.scratch_scope, allocation.mark())?;
        Ok(())
    }

    fn late_program<H: ModelExecutionHost>(
        &mut self,
        frame: &mut LateInvocation<'a, '_>,
        program: &ModelProgram,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        if depth > self.executor.limits.invocation_depth {
            return Err(ModelExecutionError::Limit);
        }
        let mut cursor = 0;
        while let Some(instruction) = program.instructions.get(cursor) {
            if self.outcome.instructions >= self.executor.limits.instructions {
                return Err(ModelExecutionError::Limit);
            }
            self.outcome.instructions += 1;
            let opcode = instruction.raw_command as u8;
            match opcode {
                0xf0..=0xff => {
                    frame
                        .values
                        .execute_register(instruction, host)
                        .map_err(ModelExecutionError::Value)?;
                }
                0x16 => {
                    if let ModelValueBranch::Jump {
                        words_from_arguments,
                    } = frame
                        .values
                        .branch_decision(instruction, host)
                        .map_err(ModelExecutionError::Value)?
                    {
                        cursor = branch_target(program, instruction, words_from_arguments)?;
                        continue;
                    }
                }
                0x03 => {
                    let [orientation, axis, angle] = arguments::<3, EOf<H>>(instruction)?;
                    frame.context.save_oriented_basis(orientation);
                    if angle != 0 {
                        let angle = frame
                            .values
                            .evaluate_packed(angle, host)
                            .map_err(ModelExecutionError::Value)?;
                        frame.context.rotate_saved_basis(
                            axis as i16,
                            angle,
                            &self.executor.basis_math,
                        );
                    }
                }
                0x04 => {
                    let [origin, first, second] = arguments::<3, EOf<H>>(instruction)?;
                    let origin = frame.vertices.world(origin as i16).map_err(vertex_fault)?;
                    let first = frame.vertices.world(first as i16).map_err(vertex_fault)?;
                    let second = frame.vertices.world(second as i16).map_err(vertex_fault)?;
                    frame
                        .context
                        .save_basis_from_world_points(origin, first, second);
                }
                0x0f => {
                    arguments::<0, EOf<H>>(instruction)?;
                    frame.context.save_identity_basis();
                }
                0x10 => {
                    arguments::<0, EOf<H>>(instruction)?;
                    frame.context.restore_saved_basis();
                    frame.transform = self.publish(&frame.context, host)?;
                }
                0x1c => {
                    let [axis, angle] = arguments::<2, EOf<H>>(instruction)?;
                    let angle = frame
                        .values
                        .evaluate_packed(angle, host)
                        .map_err(ModelExecutionError::Value)?;
                    frame
                        .context
                        .rotate_saved_basis(axis as i16, angle, &self.executor.basis_math);
                }
                // 004A0250, shared with the Near and Far tables.
                0x08 => {
                    // Full signed command SAR8, relative to first argument.
                    let words = (i32::from(instruction.raw_command as i16) >> 8) as i16;
                    cursor = branch_target(program, instruction, words)?;
                    continue;
                }
                // 004A0C50, shared with the Near and Far tables.
                0x13 => {
                    let [jump, low, high] = late_arguments::<3, EOf<H>>(instruction)?;
                    let threshold =
                        (i32::from(high as i16).wrapping_shl(16)).wrapping_add(i32::from(low));
                    let comparison = i32::from(frame.transform.view_position[2] >= threshold);
                    if comparison ^ (i32::from(instruction.raw_command as i16) >> 8) != 0 {
                        cursor = branch_target(program, instruction, jump as i16)?;
                        continue;
                    }
                }
                0x0b => {
                    let [jump, slot, normal] = late_arguments::<3, EOf<H>>(instruction)?;
                    if view_plane_jumps(
                        ViewPlaneOperands {
                            resource: frame.resource,
                            basis: &frame.context.basis,
                            vertices: &frame.vertices,
                            environment: &self.environment,
                        },
                        instruction,
                        slot,
                        normal,
                    )? {
                        cursor = branch_target(program, instruction, jump as i16)?;
                        continue;
                    }
                }
                0x0e => self.late_child(frame, instruction, depth, host)?,
                0x05 | 0x09 => self.late_block(frame, instruction, depth, host)?,
                0x20..=0x2d => frame
                    .vertices
                    .execute_world_stream(
                        ModelWorldStreamRequest {
                            instruction,
                            model: frame.resource.model,
                            context: &frame.context,
                            state_words: frame.state_words,
                            parent: frame.parent_vertices.as_ref(),
                            surface: self.surface,
                        },
                        &mut frame.values,
                        host,
                    )
                    .map_err(ModelExecutionError::Vertex)?,
                // Genuine record-advance handlers, not visible draw callbacks:
                // each returns the next record, which the decoded span gives.
                // 01 004ED5B0 (a fan's 4- or 6-word entries to the one whose
                // high byte is set); EB/EC/EF 004ED520 (no words); 15/EA
                // 004ED530 (1); ED 004ED540 (2); 02/EE 004ED550 (3); 07 and
                // 18..1B 004ED560 (4); D8..DE 004ED570 (5); E0..E6 004ED580
                // (6); D0/D2 004ED590 (7); D4/D6 004ED5A0 (9); E8/E9
                // 004ED5F0 (to the 0xFFFF end); 06 the shared 004A3880 list.
                0x01
                | 0x02
                | 0x06
                | 0x07
                | 0x15
                | 0x18..=0x1b
                | 0xd0
                | 0xd2
                | 0xd4
                | 0xd6
                | 0xd8
                | 0xda
                | 0xdc
                | 0xde
                | 0xe0
                | 0xe2
                | 0xe4
                | 0xe6
                | 0xe8
                | 0xe9
                | 0xea..=0xef => {}
                _ => {
                    return Err(ModelExecutionError::Unsupported {
                        opcode,
                        offset: instruction.span.start,
                    })
                }
            }
            cursor += 1;
        }
        Ok(())
    }

    fn late_child<H: ModelExecutionHost>(
        &mut self,
        frame: &mut LateInvocation<'a, '_>,
        instruction: &ModelInstruction,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        let ModelInstructionKind::ModelInstance(instance) = &instruction.kind else {
            return Err(ModelExecutionError::Record(instruction.span.start));
        };
        let slot = instance.model.raw();
        // 004ED7E0's NULL test precedes the state pointer and every publication.
        if slot < 0
            || slot as usize
                > self
                    .executor
                    .resources
                    .pool_capacity(RenderResourceKind::Model)
        {
            return Ok(());
        }
        let Some(resource) = self.resolve(i32::from(slot))? else {
            return Ok(());
        };
        let &origin = instance
            .state_words
            .first()
            .ok_or(ModelExecutionError::Record(instruction.span.start))?;
        let position = frame.vertices.world(origin as i16).map_err(vertex_fault)?;
        // 004A14A0: low bits 6 copy the saved basis, others orient (00490520).
        let context = frame.context.child(position, instance.orientation.raw());
        let mut values = frame
            .values
            .child_invocation()
            .map_err(|e| ModelExecutionError::Value(ModelValueError::Fault(e)))?;
        self.seed_child_registers(&resource, &mut values)?;
        self.late_invoke(
            InvocationSeed {
                resource,
                context,
                values,
                state_words: &instance.state_words,
                parent_vertices: Some(frame.vertices.clone()),
                depth: depth + 1,
                publications: 2,
                exact_origin: None,
            },
            None,
            host,
        )?;
        self.publish(&frame.context, host)?;
        Ok(())
    }

    fn late_block<H: ModelExecutionHost>(
        &mut self,
        frame: &mut LateInvocation<'a, '_>,
        instruction: &ModelInstruction,
        depth: usize,
        host: &mut H,
    ) -> Result<(), ModelExecutionError<H::Error>> {
        let ModelInstructionKind::NestedProgram(nested) = &instruction.kind else {
            return Err(ModelExecutionError::Record(instruction.span.start));
        };
        let opcode = instruction.raw_command as u8;
        let args = &nested.arguments;
        if args.len() != if opcode == 0x05 { 12 } else { 11 } {
            return Err(ModelExecutionError::Record(instruction.span.start));
        }
        // The saved 13 DWORDs omit+A0: inner saved-basis changes persist.
        let saved_position = frame.context.position;
        let saved_basis = frame.context.basis;
        frame.context.position = frame.vertices.world(args[0] as i16).map_err(vertex_fault)?;
        let ranges = if opcode == 0x05 {
            for (word, plane) in args[1..4].iter().copied().zip([
                crate::menu_basis::RotationPlane::Xz,
                crate::menu_basis::RotationPlane::Yz,
                crate::menu_basis::RotationPlane::Xy,
            ]) {
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
            [args[5], args[6], args[7]]
        } else {
            let first = frame.vertices.world(args[1] as i16).map_err(vertex_fault)?;
            let second = frame.vertices.world(args[2] as i16).map_err(vertex_fault)?;
            let direction = |point: [i32; 3]| {
                let words = std::array::from_fn(|i| {
                    (point[i] as u16).wrapping_sub(frame.context.position[i] as u16) as i16
                });
                inf_formats::native_basis::normalize_words(
                    words,
                    inf_formats::native_basis::NativeLengthPrecision::Retained,
                )
            };
            let forward = direction(first);
            let up = direction(second);
            inf_formats::native_basis::orient_from_forward(
                &mut frame.context.basis,
                forward,
                up,
                instruction.raw_command & 0x0100 != 0,
            );
            [args[4], args[5], args[6]]
        };
        frame.transform = self.publish(&frame.context, host)?;
        frame
            .vertices
            .initialize_world_ranges(
                frame.resource.model,
                &frame.context,
                ranges.map(|v| i32::from(v as i16)),
            )
            .map_err(lift_initialization)?;
        self.late_program(frame, &nested.program, depth + 1, host)?;
        frame.context.position = saved_position;
        frame.context.basis = saved_basis;
        frame.transform = self.publish(&frame.context, host)?;
        Ok(())
    }
}

fn late_arguments<const N: usize, E>(
    instruction: &ModelInstruction,
) -> Result<[u16; N], ModelExecutionError<E>> {
    decoded_arguments(instruction)
}

#[cfg(test)]
mod tests;
