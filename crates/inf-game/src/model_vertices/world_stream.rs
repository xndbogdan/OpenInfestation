//! 004EC0A0 and direct-table00544764's world/control wrappers. Projection
//! storage is never seeded here. Declared owned slots replace raw pointers;
//! malformed spans and aliased allocation mutations are typed boundaries.

use super::*;
use crate::model_execution::ModelExecutionHost;

pub(crate) struct ModelWorldStreamRequest<'a, 't> {
    pub instruction: &'a ModelInstruction,
    pub model: &'a ModelEntry,
    pub context: &'a ModelContext,
    pub state_words: &'a [u16],
    pub parent: Option<&'a ModelVertices>,
    pub surface: ModelSurfaceBinding<'t>,
}

impl ModelVertices {
    /// Header+12/14/16 only: unlike common initialization this neither reads
    /// projection ranges+10/+18 nor calls a projector or initializes shades.
    pub(crate) fn initialize_world_ranges(
        &mut self,
        model: &ModelEntry,
        context: &ModelContext,
        ranges: [i32; 3],
    ) -> Result<(), ModelVertexError> {
        let result = self
            .ensure_running()
            .map_err(ModelVertexError::from)
            .and_then(|()| self.initialize_world_ranges_inner(model, context, ranges));
        self.finish(result)
    }

    pub(super) fn initialize_world_ranges_inner<C, P>(
        &mut self,
        model: &ModelEntry,
        context: &ModelContext,
        [a, b, c]: [i32; 3],
    ) -> Result<(), ModelVertexError<C, P>> {
        if a < 0
            || !(a <= b && b <= c)
            || c as usize > self.slots.len()
            || [a, b, c].iter().any(|v| v & 1 != 0)
            || c as usize / 2 > model.vertices.len()
        {
            return Err(ModelVertexFault::InvalidInitialization.into());
        }
        for index in (a as usize..c as usize).step_by(2) {
            let raw = model.vertices[index / 2];
            let local = [i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)];
            let mirrored = [-local[0], local[1], local[2]];
            let twin = |local| {
                self.exact
                    .as_deref()
                    .map(|exact| exact::local_world(context, exact.origin, local))
            };
            let [first, second] = [twin(local), twin(mirrored)];
            self.write_world(index, transform_world(context, local))?;
            self.set_twin(index, first);
            if index >= b as usize {
                self.write_world(index + 1, transform_world(context, mirrored))?;
                self.set_twin(index + 1, second);
            }
        }
        Ok(())
    }

    pub(crate) fn execute_world_stream<H: ModelExecutionHost>(
        &mut self,
        request: ModelWorldStreamRequest<'_, '_>,
        values: &mut ModelValues,
        host: &mut H,
    ) -> Result<(), ModelVertexError<H::Error>> {
        let result = self.execute_world_stream_inner(request, values, host);
        self.finish(result)
    }

    fn execute_world_stream_inner<H: ModelExecutionHost>(
        &mut self,
        request: ModelWorldStreamRequest<'_, '_>,
        values: &mut ModelValues,
        host: &mut H,
    ) -> Result<(), ModelVertexError<H::Error>> {
        self.ensure_running()?;
        let instruction = request.instruction;
        let opcode = instruction.raw_command as u8;
        let arguments = match &instruction.kind {
            ModelInstructionKind::Opaque(command) => command.arguments.as_slice(),
            ModelInstructionKind::UnresolvedDynamic(command) => command.arguments.as_slice(),
            _ => return Err(ModelVertexFault::InvalidInstruction { opcode }.into()),
        };
        let expected = match opcode {
            0x20 => 1,
            0x21 | 0x2a..=0x2c => 2,
            0x22 | 0x23 | 0x25 => 3,
            0x24 | 0x26 => 4,
            0x27 => 6,
            0x28 => 5,
            0x29 => 7,
            0x2d => 2 + usize::from(instruction.raw_command >> 12),
            _ => return Err(ModelVertexFault::InvalidInstruction { opcode }.into()),
        };
        if arguments.len() != expected {
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        let destination = i32::from(arguments[0] as i16);
        // The late table's wrappers mask the command before the shared
        // handler: 2D (004EDCA0) to AH & F1 | 1; 20 (004EDCC0), 21
        // (004EDCD0), 22 (004EDCE0), 23 (004EDCF0), 24 (004EDD00), 25
        // (004EDD10), 26 (004EDD20), 27 (004EDD30), 28 (004EDD40), 29
        // (004EDD50), 2A (004EDD60), 2B (004EDD70) and 2C (004EDD80) to AH & F5.
        let raw = if opcode == 0x2d {
            (instruction.raw_command & 0xf1ff) | 0x0100
        } else {
            instruction.raw_command & 0xf5ff
        };
        let mut modes = if opcode == 0x20 || opcode == 0x2c {
            // 004EBF25/004EC5E9 use full sign-extended DWORD SAR8, unlike the other
            // handlers' four-bit mask. Higher projection pairs remain required
            // unknown callback state; reached world writes survive that stop.
            (i32::from(raw as i16) >> 8) as u32
        } else {
            u32::from((raw >> 8) & 15)
        };

        if opcode == 0x2a {
            // 004ED350 reads destination, then the parent state reference even
            // when both mode pairs are clear. Root NULL skips that reference.
            let parent_slot = if request.parent.is_some() {
                let reference = i32::from(arguments[1] as i16);
                Some(
                    *usize::try_from(reference)
                        .ok()
                        .and_then(|i| request.state_words.get(i))
                        .ok_or(ModelVertexFault::InvalidInstruction { opcode })?
                        as i16,
                )
            } else {
                None
            };
            if request.parent.is_none() {
                if modes & 5 != 0 {
                    let world = transform_world(request.context, [0; 3]);
                    let to = self.checked_index(destination)?;
                    self.write_world(to, world)?;
                    if modes & 4 != 0 {
                        let second = self.checked_index(destination + 1)?;
                        self.write_world(second, world)?;
                    }
                }
                return Ok(());
            }
            for parity in 0..2 {
                if modes & 3 != 0 {
                    if let (Some(parent), Some(slot)) = (request.parent, parent_slot) {
                        let from = parent.checked_index(i32::from(slot) ^ parity)?;
                        let to = self.checked_index(destination + parity)?;
                        self.copy_parent_slot(to, parent, from)?;
                    }
                }
                modes >>= 2;
            }
            return Ok(());
        }

        // Packed weights run before mode gates, and the second sees mutations
        // made by the first. No register or callback defaults are supplied.
        let weight = match opcode {
            0x26 => Some(
                i32::from(
                    values
                        .evaluate_packed(arguments[3], host)
                        .map_err(ModelVertexError::Value)?,
                )
                .wrapping_shl(14),
            ),
            0x27 => Some(
                i32::from(
                    values
                        .evaluate_packed(arguments[5], host)
                        .map_err(ModelVertexError::Value)?,
                )
                .wrapping_shl(14),
            ),
            0x28 | 0x29 => {
                let at = if opcode == 0x28 { 3 } else { 5 };
                let first = values
                    .evaluate_packed(arguments[at], host)
                    .map_err(ModelVertexError::Value)?;
                let second = values
                    .evaluate_packed(arguments[at + 1], host)
                    .map_err(ModelVertexError::Value)?;
                Some((i32::from(first) + i32::from(second)).wrapping_shl(14))
            }
            _ => None,
        };
        // 20 captures its raw source even for a zero mode, before the loop.
        let raw_local = if opcode == 0x20 {
            let at = destination >> 1;
            let raw = usize::try_from(at)
                .ok()
                .and_then(|i| request.model.vertices.get(i))
                .ok_or(ModelVertexFault::InvalidSlot {
                    index: at,
                    count: request.model.vertices.len(),
                })?;
            Some([i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)])
        } else {
            None
        };
        let mut pass = 0;
        let mut output = None;
        while modes != 0 {
            if modes & 3 != 0 {
                let source = |this: &Self, at: usize| {
                    this.read_world(i32::from(arguments[at] as i16) ^ if pass == 0 { 0 } else { 1 })
                };
                let world = match opcode {
                    0x20 => {
                        let mut local = raw_local.expect("captured raw source");
                        if pass & 1 != 0 {
                            local[0] = local[0].wrapping_neg();
                        }
                        transform_world(request.context, local)
                    }
                    0x21 => {
                        let origin = transform_world(request.context, [0; 3]);
                        let base = source(self, 1)?;
                        std::array::from_fn(|i| origin[i].wrapping_mul(2).wrapping_sub(base[i]))
                    }
                    0x22 => {
                        let base = source(self, 1)?;
                        let rng = host
                            .process_rng()
                            .ok_or(ModelVertexFault::UnsupportedOpcode { opcode })?;
                        std::array::from_fn(|i| {
                            base[i].wrapping_add(
                                (crate::name_entry::name_entry_rng_next(&mut *rng) as i32)
                                    .wrapping_shl(16)
                                    >> (arguments[2] & 31),
                            )
                        })
                    }
                    0x23 | 0x24 | 0x25 | 0x26 | 0x28 => {
                        let left = source(self, 1)?;
                        let right = source(self, 2)?;
                        let third = if opcode == 0x24 {
                            Some(source(self, 3)?)
                        } else {
                            None
                        };
                        std::array::from_fn(|i| match opcode {
                            0x23 => left[i].wrapping_add(right[i]) / 2,
                            0x24 => left[i]
                                .wrapping_sub(third.expect("third source")[i])
                                .wrapping_add(right[i]),
                            0x25 => left[i]
                                .wrapping_add(right[i])
                                .wrapping_sub(request.context.position[i]),
                            _ => left[i].wrapping_add(
                                mul_q31(
                                    right[i].wrapping_sub(left[i]),
                                    weight.expect("packed weight"),
                                )
                                .wrapping_mul(2),
                            ),
                        })
                    }
                    0x27 | 0x29 => {
                        let points = [
                            source(self, 1)?,
                            source(self, 2)?,
                            source(self, 3)?,
                            source(self, 4)?,
                        ];
                        std::array::from_fn(|i| {
                            cubic(points.map(|p| p[i]), weight.expect("packed weight"))
                        })
                    }
                    0x2b | 0x2c => {
                        let mut callbacks = ModelSurfaceCallbackInputs {
                            binding: request.surface,
                            table: host.sine_quarter_table(),
                        };
                        callbacks.generate(
                            self,
                            i32::from(arguments[1] as i16) ^ if pass == 0 { 0 } else { 1 },
                            if opcode == 0x2b {
                                ModelSurfaceCallback::Ground
                            } else {
                                ModelSurfaceCallback::Project
                            },
                        )?
                    }
                    0x2d => {
                        let selector = i32::from(arguments[1] as i16);
                        host.required_custom_vertex(
                            ModelCustomVertexRequest {
                                selector,
                                extras: &arguments[2..],
                            },
                            self,
                            &mut output,
                        )
                        .map_err(ModelVertexError::Custom)?;
                        output.ok_or(ModelVertexFault::UnwrittenCustomVertex { selector })?
                    }
                    _ => return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into()),
                };
                if modes & 1 != 0 {
                    let to = self.checked_index(destination + pass)?;
                    self.write_world(to, world)?;
                }
                if modes & 2 != 0 {
                    return Err(ModelVertexFault::UnsupportedOpcode { opcode }.into());
                }
            }
            modes >>= 2;
            pass += 1;
        }
        if opcode == 0x2d && instruction.raw_command >= 0x8000 {
            // Native signed SAR12 advances backwards only after compute/store.
            // The canonical decoded record has a forward extra-word span; it
            // cannot authenticate that negative next-record pointer. Preserve
            // the callback/world prefix before reporting this reached boundary.
            return Err(ModelVertexFault::InvalidInstruction { opcode }.into());
        }
        Ok(())
    }
}

pub(super) fn mul_q31(left: i32, right: i32) -> i32 {
    ((i64::from(left) * i64::from(right)) >> 31) as i32
}

pub(super) fn cubic([p0, p1, p2, p3]: [i32; 4], t: i32) -> i32 {
    let t2 = mul_q31(t, t);
    let t3 = mul_q31(t2, t);
    let c1 = p1.wrapping_sub(p0).wrapping_mul(3);
    let c2 = p0
        .wrapping_sub(p1.wrapping_mul(2))
        .wrapping_add(p2)
        .wrapping_mul(3);
    let c3 = p1
        .wrapping_sub(p2)
        .wrapping_mul(3)
        .wrapping_sub(p0)
        .wrapping_add(p3);
    p0.wrapping_add(
        mul_q31(c1, t)
            .wrapping_add(
                mul_q31(c2, t2)
                    .wrapping_add(mul_q31(c3, t3).wrapping_mul(2))
                    .wrapping_mul(2),
            )
            .wrapping_mul(2),
    )
}

#[cfg(test)]
mod tests;
