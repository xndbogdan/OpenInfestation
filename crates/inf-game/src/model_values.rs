//! Handler-8 packed operands and register execution (004ED4B0, 004A0260..0770).
//!
//! This owns the independent words at context +70..+9B, not a model renderer.
//! Although packed indices use six bits, +9C is the parent-context pointer and
//! +A0 begins a saved basis: indices 22..63 alias other native context fields.
//! Those indices require the complete context owner and are rejected here.
//! Top-level 0049FEF0 does not initialize the independent words. Child 004A0ED0
//! copies only words 0..3; nested 05/09 programs retain the same value owner.

use std::sync::Arc;

use inf_formats::{ModelInstruction, ModelInstructionKind};
use thiserror::Error;

const INDEPENDENT_REGISTERS: usize = 22;

///004ED4B0's immediate arm, shared with auxiliary30. None retains the need
///for the actual context register or virtual callback; it is not a zero value.
pub(crate) fn packed_immediate(raw: u16) -> Option<u16> {
    if raw & 0x80 != 0 {
        return None;
    }
    let value = (raw & 0xff03).rotate_left(u32::from((raw >> 2) & 15));
    Some(if raw & 0x40 != 0 { !value } else { value })
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ModelValueFault {
    #[error("model register {index} has not been initialized in this invocation")]
    UninitializedRegister { index: u8 },
    #[error("model register {index} aliases native context state outside the value owner")]
    ContextAliasedRegister { index: u8 },
    #[error("model register destination {index} is outside the packed index range")]
    InvalidRegister { index: i16 },
    #[error("model opcode {opcode:#04x} is not supported by this value operation")]
    UnsupportedOpcode { opcode: u8 },
    #[error("model value opcode {opcode:#04x} has an incompatible decoded record")]
    InvalidInstruction { opcode: u8 },
    #[error("model value invocation stopped after an execution failure")]
    Stopped,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ModelValueError<E> {
    #[error(transparent)]
    Fault(#[from] ModelValueFault),
    #[error("model packed-value callback failed: {0}")]
    Callback(E),
}

/// The callback receives the live invocation, as the native virtual call does.
/// F0..FF evaluate left before right; opcode16 evaluates right before left.
/// Mutations from the first operand are visible to the second. There is no
/// default or neutral callback value.
pub trait ModelValueCallback {
    type Error;

    fn packed_value(&mut self, values: &mut ModelValues, selector: u32)
        -> Result<u16, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelValueBranch {
    FallThrough,
    /// 004A0C90 adds this signed WORD count to the first argument pointer,
    /// which is two bytes after the command. The program owner validates and
    /// follows that address; this value owner does not skip geometry commands.
    Jump {
        words_from_arguments: i16,
    },
}

#[derive(Clone, Debug)]
pub struct ModelValues {
    registers: [Option<u16>; INDEPENDENT_REGISTERS],
    sine_table: Arc<[u16; 4096]>,
    stopped: bool,
}

/// A root context's register words as its stack slot keeps them between two
/// draws at the same depth. `None` is a word nothing has written there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelRegisterBlock([Option<u16>; INDEPENDENT_REGISTERS]);

impl ModelValues {
    /// A root over a stack slot that an earlier root at the same depth left.
    pub fn over_register_block(sine_table: Arc<[u16; 4096]>, block: ModelRegisterBlock) -> Self {
        Self {
            registers: block.0,
            sine_table,
            stopped: false,
        }
    }

    /// The words this invocation leaves in its context's stack slot.
    pub fn register_block(&self) -> ModelRegisterBlock {
        ModelRegisterBlock(self.registers)
    }

    /// Original executable table at 0053DF00. No trigonometric reconstruction.
    /// Unknown slots represent uninitialized native stack memory, never zero.
    pub fn new_invocation(sine_table: Arc<[u16; 4096]>) -> Self {
        Self {
            registers: [None; INDEPENDENT_REGISTERS],
            sine_table,
            stopped: false,
        }
    }

    /// 004A10B9..004A10FD copies precisely +70/+72/+74/+76 into the child.
    /// No copy-back occurs. Unknown inherited words remain unknown until read.
    /// A nested inline program must reuse `self`, not call this method.
    pub fn child_invocation(&self) -> Result<Self, ModelValueFault> {
        self.ensure_running()?;
        let mut child = Self::new_invocation(Arc::clone(&self.sine_table));
        child.registers[..4].copy_from_slice(&self.registers[..4]);
        Ok(child)
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Inspection remains available after a failure; it does not turn an
    /// unknown word into a value or read aliased native context bytes.
    pub fn register(&self, index: i16) -> Result<Option<u16>, ModelValueFault> {
        Ok(self.registers[checked_index(index)?])
    }

    /// Explicit writes also support callbacks that modify their invocation.
    pub fn write_register(&mut self, index: i16, value: u16) -> Result<(), ModelValueFault> {
        self.ensure_running()?;
        self.registers[checked_index(index)?] = Some(value);
        Ok(())
    }

    /// The low WORD consumed by the recovered value operations. 004ED4B0's
    /// incidental upper EAX bits are not a separate initialized return value.
    pub fn evaluate_packed<C: ModelValueCallback>(
        &mut self,
        raw: u16,
        callback: &mut C,
    ) -> Result<u16, ModelValueError<C::Error>> {
        self.ensure_running()?;
        let result = self.evaluate_inner(raw, callback);
        self.finish(result)
    }

    /// Executes exactly one F0..FF instruction. Other opcodes fail explicitly.
    /// Invalid record/destination checks precede callbacks as Rust containment;
    /// the original unchecked out-of-bounds writes are not emulated.
    pub fn execute_register<C: ModelValueCallback>(
        &mut self,
        instruction: &ModelInstruction,
        callback: &mut C,
    ) -> Result<u16, ModelValueError<C::Error>> {
        self.ensure_running()?;
        let result = self.execute_inner(instruction, callback);
        self.finish(result)
    }

    /// Executes the packed-value comparison of opcode 16 (004A0C90).
    pub fn branch_decision<C: ModelValueCallback>(
        &mut self,
        instruction: &ModelInstruction,
        callback: &mut C,
    ) -> Result<ModelValueBranch, ModelValueError<C::Error>> {
        self.ensure_running()?;
        let result = self.branch_inner(instruction, callback);
        self.finish(result)
    }

    fn evaluate_inner<C: ModelValueCallback>(
        &mut self,
        raw: u16,
        callback: &mut C,
    ) -> Result<u16, ModelValueError<C::Error>> {
        if let Some(value) = packed_immediate(raw) {
            Ok(value)
        } else if raw & 0x40 != 0 {
            let index = (raw & 63) as u8;
            self.register(i16::from(index))?
                .ok_or(ModelValueFault::UninitializedRegister { index }.into())
        } else {
            self.call(callback, u32::from(raw & 0xff3f))
        }
    }

    fn execute_inner<C: ModelValueCallback>(
        &mut self,
        instruction: &ModelInstruction,
        callback: &mut C,
    ) -> Result<u16, ModelValueError<C::Error>> {
        let opcode = instruction.raw_command as u8;
        if opcode < 0xf0 {
            return Err(ModelValueFault::UnsupportedOpcode { opcode }.into());
        }
        let ModelInstructionKind::RegisterOperation(command) = &instruction.kind else {
            return Err(ModelValueFault::InvalidInstruction { opcode }.into());
        };
        let [destination, left, right] = command.arguments.as_slice() else {
            return Err(ModelValueFault::InvalidInstruction { opcode }.into());
        };
        let destination = checked_index(*destination as i16)?;
        let left = self.evaluate_inner(*left, callback)?;
        let right = self.evaluate_inner(*right, callback)?;
        // Handlers, Near/Far/Late tables alike: F0 004A0260, F1 004A0290,
        // F2 004A02C0, F3 004A0300, F4 004A0360, F5 004A03A0, F6 004A03E0,
        // F7 004A0420, F8 004A0480, F9 004A04F0, FA 004A0530, FB 004A0580,
        // FC 004A05D0, FD 004A0620, FE 004A06B0, FF 004A0740.
        let value = match opcode {
            0xf0 => left.wrapping_add(right),
            0xf1 => left.wrapping_sub(right),
            0xf2 => left.wrapping_mul(right),
            0xf3 => {
                if right == 0 {
                    left
                } else {
                    left / right
                }
            }
            // 32-bit x86 shifts mask by31, then store low16. A u16 Rust
            // wrapping_shl would incorrectly mask by15 (004A03A0).
            0xf4 => (u32::from(left) >> (right & 31)) as u16,
            0xf5 => u32::from(left).wrapping_shl(u32::from(right & 31)) as u16,
            0xf6 => left.max(right),
            0xf7 => {
                if left < right {
                    left
                } else {
                    right.wrapping_sub(1)
                }
            }
            0xf8 => {
                if right == 0 {
                    left
                } else {
                    ((u32::from(left) << 16) / u32::from(right)) as u16
                }
            }
            0xf9 => (i32::from(left as i16) >> (right & 31)) as u16,
            // 004A0530 does NOT remask this sum withFF3F or truncate it.
            0xfa => self.call(callback, u32::from(left) + u32::from(right))?,
            0xfb => {
                if left >= right {
                    left
                } else {
                    0
                }
            }
            0xfc => {
                if left <= right {
                    left
                } else {
                    0
                }
            }
            0xfd => self.scaled_sine(left, right),
            0xfe => self.scaled_sine(left, right.wrapping_add(0x4000)),
            0xff => left & right,
            _ => unreachable!("u8 opcode checked to be F0..FF"),
        };
        self.ensure_running()?;
        self.registers[destination] = Some(value);
        Ok(value)
    }

    fn branch_inner<C: ModelValueCallback>(
        &mut self,
        instruction: &ModelInstruction,
        callback: &mut C,
    ) -> Result<ModelValueBranch, ModelValueError<C::Error>> {
        let opcode = instruction.raw_command as u8;
        if opcode != 0x16 {
            return Err(ModelValueFault::UnsupportedOpcode { opcode }.into());
        }
        let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
            return Err(ModelValueFault::InvalidInstruction { opcode }.into());
        };
        let [jump, left, right] = command.arguments.as_slice() else {
            return Err(ModelValueFault::InvalidInstruction { opcode }.into());
        };
        // 004A0CC4 jumps to the first argument word plus `jump` WORDs and
        // 004A0CD1 falls through to it plus three, so a jump of three lands
        // where falling through does and the comparison decides nothing. Its
        // operands are still read in native order, a callback with all its
        // effects, but an unwritten register then holds stack residue whose
        // value no later command sees (the factory model H8 157's register 17
        // at 1.ovl+1C19E).
        let decides = *jump as i16 != 3;
        // 004A0C98..004A0CAC: argument +4 precedes argument +2. Unlike
        // F0..FF, a right callback can initialize the subsequent left read.
        let right = self.branch_operand(*right, decides, callback)?;
        let left = self.branch_operand(*left, decides, callback)?;
        let (Some(left), Some(right)) = (left, right) else {
            return Ok(ModelValueBranch::FallThrough);
        };
        if !decides {
            return Ok(ModelValueBranch::FallThrough);
        }
        let comparison = u32::from(left != right);
        let mode = (instruction.raw_command as i16 as i32 as u32) >> 8;
        Ok(if comparison != mode {
            ModelValueBranch::Jump {
                words_from_arguments: *jump as i16,
            }
        } else {
            ModelValueBranch::FallThrough
        })
    }

    /// One opcode-16 operand. A comparison that decides nothing reads an
    /// unwritten register as `None` instead of stopping; every other fault,
    /// and every read of a deciding comparison, keeps its strict owner.
    fn branch_operand<C: ModelValueCallback>(
        &mut self,
        raw: u16,
        decides: bool,
        callback: &mut C,
    ) -> Result<Option<u16>, ModelValueError<C::Error>> {
        match self.evaluate_inner(raw, callback) {
            Err(ModelValueError::Fault(ModelValueFault::UninitializedRegister { .. }))
                if !decides =>
            {
                Ok(None)
            }
            result => result.map(Some),
        }
    }

    /// 004A0620/06B0 duplicate the table WORD into signed32, optionally
    /// negate it, then IMUL by unsigned16 and extract product bits31..46.
    fn scaled_sine(&self, amplitude: u16, angle: u16) -> u16 {
        let mut offset = angle & 0x3ffc;
        if angle & 0x4000 != 0 {
            offset = 0x3ffc - offset;
        }
        let word = u32::from(self.sine_table[usize::from(offset >> 2)]);
        let mut signed = (word | (word << 16)) as i32;
        if angle & 0x8000 != 0 {
            signed = signed.wrapping_neg();
        }
        ((i64::from(signed) * i64::from(amplitude)) >> 31) as u16
    }

    fn call<C: ModelValueCallback>(
        &mut self,
        callback: &mut C,
        selector: u32,
    ) -> Result<u16, ModelValueError<C::Error>> {
        let result = callback
            .packed_value(self, selector)
            .map_err(ModelValueError::Callback);
        // Required fallible Rust host policy, not native callback recovery:
        // preserve an immediate host error; catch swallowed nested failures.
        self.finish(result)
    }

    fn ensure_running(&self) -> Result<(), ModelValueFault> {
        if self.stopped {
            Err(ModelValueFault::Stopped)
        } else {
            Ok(())
        }
    }

    fn finish<T, E>(
        &mut self,
        result: Result<T, ModelValueError<E>>,
    ) -> Result<T, ModelValueError<E>> {
        match result {
            Err(error) => {
                self.stopped = true;
                Err(error)
            }
            Ok(value) => {
                self.ensure_running()?;
                Ok(value)
            }
        }
    }
}

fn checked_index(index: i16) -> Result<usize, ModelValueFault> {
    match index {
        0..=21 => Ok(index as usize),
        22..=63 => Err(ModelValueFault::ContextAliasedRegister { index: index as u8 }),
        _ => Err(ModelValueFault::InvalidRegister { index }),
    }
}

#[cfg(test)]
#[path = "model_values_tests.rs"]
mod tests;
