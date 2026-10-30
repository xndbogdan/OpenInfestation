//! Selector 86: nonvisual emitter construction 0042DE50, retained mutable
//! data 0042DEC0, command handler 0042DEF0 and draw-distance policy 0042DF20.
//! Its effects-spawning update 0042DF40 uses live camera/terrain/trail services.

mod update;
pub use update::{Selector86Trail, Selector86UpdateError, Selector86UpdateHost};

use std::sync::Arc;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, RuntimeAllocation};

#[derive(Clone, Debug)]
pub struct RetainedSelector86Data {
    allocation: Arc<RuntimeAllocation>,
    pub allocation_offset: u32,
}

impl RetainedSelector86Data {
    pub fn allocation(&self) -> &Arc<RuntimeAllocation> {
        &self.allocation
    }
    pub fn words(&self) -> [u16; 4] {
        std::array::from_fn(|index| {
            self.allocation
                .word(self.allocation_offset + index as u32 * 2)
                .expect("validated retained data")
        })
    }
}

impl PartialEq for RetainedSelector86Data {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}
impl Eq for RetainedSelector86Data {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector86State {
    /// Portable per-allocation containment; native fields/list links stay intact.
    pub update_stopped: bool,
    pub data_40: Option<RetainedSelector86Data>,
    pub elapsed_44: u32,
    pub distance_squared_48: u32,
}

impl Selector86State {
    pub(super) fn new() -> Self {
        Self {
            update_stopped: false,
            data_40: None,
            elapsed_44: 0,
            distance_squared_48: 1_000_000,
        }
    }
}

impl ObjectArena {
    /// Exact 0042DEF0: codes 0/1 accept without reading an operand; code 2
    /// retains and normalizes its data, and every other signed code rejects.
    pub fn apply_selector86_command(
        &mut self,
        object: ObjectId,
        code: i32,
        data: Option<(&Arc<RuntimeAllocation>, u32)>,
    ) -> Result<bool, super::ObjectConstructionError> {
        self.selector86(object)?;
        match code {
            0 | 1 => Ok(true),
            2 => {
                let (allocation, offset) =
                    data.ok_or(super::InstanceCommandAllocationError::MissingSelector86Owner)?;
                self.set_selector86_data(object, allocation, offset)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn selector86(&self, object: ObjectId) -> Result<&Selector86State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector86(state) => Ok(state),
            _ => Err(ObjectError::NotSelector86(object)),
        }
    }

    fn selector86_mut(&mut self, object: ObjectId) -> Result<&mut Selector86State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector86(state) => Ok(state),
            _ => Err(ObjectError::NotSelector86(object)),
        }
    }

    /// 0042DEC0 retains the pointer then rewrites data +6. Invalid host ranges
    /// are preflighted before mutation; they would fault in the native game.
    pub fn set_selector86_data(
        &mut self,
        object: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        offset: u32,
    ) -> Result<(), super::ObjectConstructionError> {
        self.selector86(object)?;
        allocation.validate_range(offset, 8)?;
        allocation.validate_word_write(offset + 6)?;
        let rate = allocation.word(offset + 2)? as i16;
        let interval = if rate > 0 {
            (60_000 / i32::from(rate)) as u16
        } else {
            30_000
        };
        self.selector86_mut(object)?.data_40 = Some(RetainedSelector86Data {
            allocation: Arc::clone(allocation),
            allocation_offset: offset,
        });
        allocation.write_word(offset + 6, interval)?;
        Ok(())
    }

    /// Virtual +2C=0042DF20 reads a signed scene DWORD +68, divides by two
    /// toward zero and stores its wrapping square. It does not draw geometry.
    pub fn set_selector86_draw_distance(
        &mut self,
        object: ObjectId,
        scene_word_68: i32,
    ) -> Result<(), ObjectError> {
        let half = scene_word_68 / 2;
        self.selector86_mut(object)?.distance_squared_48 = half.wrapping_mul(half) as u32;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
