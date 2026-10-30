//! The bytecode consumed by FUN_004F2C50, separate from the renderer's WORD
//! program. Alignment is relative to the relocated model, never the OVL file.
//! This module decodes structure; callbacks, hit selection and child execution
//! still need the native request context.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, ModelOrientation, ModelResourceRef};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelAuxiliaryProgram {
    /// Through the one-byte 88 terminator, excluding retained trailing bytes.
    pub span: ByteSpan,
    pub instructions: Vec<ModelAuxiliaryInstruction>,
    pub terminator: ByteSpan,
    pub padding: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelAuxiliaryInstruction {
    /// Includes opcode, skipped/alignment bytes and arguments.
    pub span: ByteSpan,
    pub opcode: u8,
    pub arguments_span: ByteSpan,
    /// Includes the shared-handler skipped byte and all alignment bytes.
    pub skipped_bytes: Vec<u8>,
    pub raw_arguments: Vec<u8>,
    pub kind: ModelAuxiliaryInstructionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ModelAuxiliaryInstructionKind {
    /// Reuses a renderer vertex handler with opcode << 8 as its command value.
    /// The byte immediately after the opcode is skipped, not used as a mode.
    SharedVertexHandler {
        renderer_opcode: u8,
        handler_command: u16,
        arguments: Vec<u16>,
    },
    ChildModel {
        model: ModelResourceRef,
        orientation: ModelOrientation,
        /// Signed byte displacement from the first argument to the next opcode.
        argument_bytes: i16,
        /// First WORD is the signed attachment vertex; subsequent state is
        /// passed to the child context without assigning invented semantics.
        state_words: Vec<u16>,
    },
    Rotate {
        second_present_raw: u8,
        first: ModelAuxiliaryRotation,
        second: Option<ModelAuxiliaryRotation>,
    },
    /// FUN_004A0940 builds a saved basis from three world-space vertex slots.
    Transform {
        vertex_slots: [i16; 3],
    },
    RegisterOperation {
        selector: i16,
        destination: i16,
        packed_operands: [u16; 2],
    },
    /// Request +1C owns the callback table. Slot is signed at the consumer;
    /// parameters are signed native integer scalars, not projected pixels.
    Callback {
        callback_offset: u8,
        parameters: Vec<i32>,
        vertex_slot: i16,
    },
    /// Callback failure jumps to the fallback last installed by GroupBegin.
    GroupCallback {
        callback_offset: u8,
        vertex_slots: Vec<i16>,
    },
    GroupRequireHit {
        retained_word: u16,
    },
    GroupBegin {
        displacement_bytes: i16,
        target: usize,
    },
    GroupEnd,
    /// Fall through only when result[0] == 1, not merely nonzero.
    BranchUnlessHit {
        displacement_bytes: i16,
        target: usize,
    },
    NoOp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelAuxiliaryRotation {
    /// The first record applies orientation when nonzero; the second applies
    /// it when zero. Preserve this native asymmetry.
    pub mode_raw: u8,
    pub packed_angle: u16,
    pub orientation_raw: i8,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelAuxiliaryError {
    #[error("auxiliary program at {offset:#x} has no 88 terminator")]
    MissingTerminator { offset: usize },
    #[error("unknown auxiliary opcode {opcode:#04x} at {offset:#x}")]
    UnknownOpcode { offset: usize, opcode: u8 },
    #[error("truncated auxiliary opcode {opcode:#04x} at {offset:#x}: requires argument end {needed_end:#x}, span ends at {available_end:#x}")]
    Truncated {
        offset: usize,
        opcode: u8,
        needed_end: usize,
        available_end: usize,
    },
    #[error("auxiliary child at {offset:#x} has overlapping/backward signed argument byte extent {argument_bytes}, unsupported by this structural decoder")]
    UnsupportedChildExtent { offset: usize, argument_bytes: i16 },
    #[error("auxiliary branch at {offset:#x} has displacement {displacement_bytes} outside its program or into another instruction")]
    InvalidBranchTarget {
        offset: usize,
        displacement_bytes: i16,
    },
}

/// Decode one active auxiliary span beginning at model +20. Returned offsets
/// start at zero. The caller's span must exclude the following vertex pool.
pub fn decode_model_auxiliary(bytes: &[u8]) -> Result<ModelAuxiliaryProgram, ModelAuxiliaryError> {
    decode_model_auxiliary_at(bytes, 0)
}

pub(crate) fn decode_model_auxiliary_at(
    bytes: &[u8],
    base: usize,
) -> Result<ModelAuxiliaryProgram, ModelAuxiliaryError> {
    let mut cursor = 0;
    let mut instructions = Vec::new();
    let terminator;
    loop {
        let start = cursor;
        let Some(&opcode) = bytes.get(start) else {
            return Err(ModelAuxiliaryError::MissingTerminator {
                offset: base + start,
            });
        };
        if opcode == 0x88 {
            terminator = ByteSpan::new(base + start, base + start + 1);
            cursor += 1;
            break;
        }
        let (arguments_start, argument_bytes) = if let Some((_, size)) = vertex_handler(opcode) {
            ((start + 3) & !1, size)
        } else {
            match opcode {
                0x05 => {
                    let args = (start + 2) & !1;
                    require(bytes, base, start, opcode, args + 6)?;
                    let extent = word(bytes, args + 4) as i16;
                    // Native has no positive/forward check. A displacement
                    // below eight overlaps fields it reads unconditionally;
                    // reject that unsupported layout rather than loop or
                    // pretend it is a normal sequential instruction.
                    if extent < 8 {
                        return Err(ModelAuxiliaryError::UnsupportedChildExtent {
                            offset: base + start,
                            argument_bytes: extent,
                        });
                    }
                    (args, extent as usize)
                }
                0x30 => {
                    let args = (start + 2) & !1;
                    require(bytes, base, start, opcode, args + 1)?;
                    (args, if bytes[args] == 0 { 5 } else { 9 })
                }
                0x31 | 0x8a => ((start + 2) & !1, 6),
                0x36 | 0x89 => ((start + 2) & !1, 8),
                0x8b | 0x8c | 0x95 => ((start + 2) & !1, 2),
                0x8e => ((start + 4) & !3, 6),
                0x8f => ((start + 4) & !3, 14),
                0x90 => ((start + 4) & !3, 10),
                0x87 | 0x8d | 0x93 | 0x94 => (start + 1, 0),
                _ => {
                    return Err(ModelAuxiliaryError::UnknownOpcode {
                        offset: base + start,
                        opcode,
                    })
                }
            }
        };
        cursor = arguments_start + argument_bytes;
        require(bytes, base, start, opcode, cursor)?;
        let args = &bytes[arguments_start..cursor];
        let kind = decode_kind(
            opcode,
            args,
            base + start,
            base + arguments_start,
            bytes.len() + base,
        )?;
        instructions.push(ModelAuxiliaryInstruction {
            span: ByteSpan::new(base + start, base + cursor),
            opcode,
            arguments_span: ByteSpan::new(base + arguments_start, base + cursor),
            skipped_bytes: bytes[start + 1..arguments_start].to_vec(),
            raw_arguments: args.to_vec(),
            kind,
        });
    }
    // Structural targets must land on an opcode, including the terminator.
    // Decode both outcomes without executing or guessing callback results.
    for instruction in &instructions {
        if let ModelAuxiliaryInstructionKind::GroupBegin {
            displacement_bytes,
            target,
        }
        | ModelAuxiliaryInstructionKind::BranchUnlessHit {
            displacement_bytes,
            target,
        } = instruction.kind
        {
            if target != terminator.start && !instructions.iter().any(|i| i.span.start == target) {
                return Err(ModelAuxiliaryError::InvalidBranchTarget {
                    offset: instruction.span.start,
                    displacement_bytes,
                });
            }
        }
    }
    Ok(ModelAuxiliaryProgram {
        span: ByteSpan::new(base, base + cursor),
        instructions,
        terminator,
        padding: ByteSpan::new(base + cursor, base + bytes.len()),
    })
}

fn require(
    bytes: &[u8],
    base: usize,
    start: usize,
    opcode: u8,
    end: usize,
) -> Result<(), ModelAuxiliaryError> {
    if end > bytes.len() {
        return Err(ModelAuxiliaryError::Truncated {
            offset: base + start,
            opcode,
            needed_end: base + end,
            available_end: base + bytes.len(),
        });
    }
    Ok(())
}

fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn words(bytes: &[u8]) -> Vec<u16> {
    bytes.chunks_exact(2).map(|b| word(b, 0)).collect()
}

/// Native dispatch 004F2D6B..004F2EAA / 004F2F53..004F2F95 / 004F3436..
/// passes opcode <<8, and the shared handler returns the next byte address.
fn vertex_handler(opcode: u8) -> Option<(u8, usize)> {
    Some(match opcode {
        0x07 => (0x20, 2),
        0x18 => (0x23, 6),
        0x2c => (0x25, 6),
        0x2d => (0x24, 8),
        0x2e => (0x21, 4),
        0x2f => (0x26, 8),
        0x3d => (0x22, 6),
        0x49 => (0x2b, 4),
        0x61 => (0x27, 12),
        0x7e => (0x28, 10),
        0x85 => (0x2a, 4),
        0x96 => (0x2c, 4),
        0x97 => (0x2d, 22),
        _ => return None,
    })
}

fn rotation(args: &[u8]) -> ModelAuxiliaryRotation {
    ModelAuxiliaryRotation {
        mode_raw: args[0],
        packed_angle: word(args, 1),
        orientation_raw: args[3] as i8,
    }
}

fn decode_kind(
    opcode: u8,
    args: &[u8],
    offset: usize,
    args_offset: usize,
    end: usize,
) -> Result<ModelAuxiliaryInstructionKind, ModelAuxiliaryError> {
    use ModelAuxiliaryInstructionKind::*;
    if let Some((renderer_opcode, _)) = vertex_handler(opcode) {
        return Ok(SharedVertexHandler {
            renderer_opcode,
            handler_command: u16::from(opcode) << 8,
            arguments: words(args),
        });
    }
    Ok(match opcode {
        0x05 => ChildModel {
            model: ModelResourceRef::from_raw(word(args, 0)),
            orientation: ModelOrientation::from_raw(word(args, 2)),
            argument_bytes: word(args, 4) as i16,
            state_words: words(&args[6..]),
        },
        0x30 => Rotate {
            second_present_raw: args[0],
            first: rotation(&args[1..5]),
            second: (args[0] != 0).then(|| rotation(&args[5..9])),
        },
        0x31 => Transform {
            vertex_slots: [
                word(args, 0) as i16,
                word(args, 2) as i16,
                word(args, 4) as i16,
            ],
        },
        0x36 => RegisterOperation {
            selector: word(args, 0) as i16,
            destination: word(args, 2) as i16,
            packed_operands: [word(args, 4), word(args, 6)],
        },
        0x8e..=0x90 => Callback {
            callback_offset: match opcode {
                0x8e => 0,
                0x8f => 8,
                _ => 4,
            },
            parameters: args[..args.len() - 2]
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
                .collect(),
            vertex_slot: word(args, args.len() - 2) as i16,
        },
        0x89 | 0x8a => GroupCallback {
            callback_offset: if opcode == 0x89 { 12 } else { 16 },
            vertex_slots: words(args).into_iter().map(|w| w as i16).collect(),
        },
        0x8b => GroupRequireHit {
            retained_word: word(args, 0),
        },
        0x8c | 0x95 => {
            let displacement_bytes = word(args, 0) as i16;
            let target = args_offset
                .checked_add_signed(isize::from(displacement_bytes))
                .filter(|&t| t < end)
                .ok_or(ModelAuxiliaryError::InvalidBranchTarget {
                    offset,
                    displacement_bytes,
                })?;
            if opcode == 0x8c {
                GroupBegin {
                    displacement_bytes,
                    target,
                }
            } else {
                BranchUnlessHit {
                    displacement_bytes,
                    target,
                }
            }
        }
        0x8d => GroupEnd,
        0x87 | 0x93 | 0x94 => NoOp,
        _ => unreachable!("framed known opcode"),
    })
}
