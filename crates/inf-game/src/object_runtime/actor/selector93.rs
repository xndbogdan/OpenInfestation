//! Constructor 93's demo actor, 00440010 / final table 005228F0.
//!
//! H13 resource slot 0x34 selects this class through its tag92 header; the
//! resource argument is independent of constructor93. Shared 0047A610 / 0047AD70
//! initialization and 0047ADA0 commands belong to their existing owners.
//! The complete +24 = 00440170 has a separate required-input request. The
//! direct 004400D0 factory has a separate constructor-first owner; native
//! attract transition remains unowned.

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{NativeHeapRef, ObjectArena, ObjectError, ObjectId};

mod update;
pub use update::{Selector93TickRequest, Selector93UpdateError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector93State {
    /// Full DWORD zero stores at +138/+13C/+140. The bounded current
    /// consumers do not establish a mode, angle or other semantic type.
    pub dwords_138_140: [u32; 3],
    /// Full signed DWORD smoothing accumulator at 00440442..0044045E.
    /// Velocity later consumes its signed LOWWORD at 0044046D separately.
    pub dword_144: i32,
    /// Actual type2 target allocation at +148, without referent ownership.
    /// Constructor NULL is known; destruction releases this resource before
    /// shared actor teardown. A retired None is not a native NULL-store claim.
    pub target_148: Option<NativeHeapRef>,
}

impl Selector93State {
    fn new() -> Self {
        Self {
            dwords_138_140: [0; 3],
            dword_144: 0,
            target_148: None,
        }
    }
}

impl ObjectArena {
    /// Shared 0047A7A0 followed by the exact five constructor93 DWORD writes.
    /// No derived RNG/world-mask inputs, position default or later update runs.
    pub fn allocate_selector93(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector93(Box::new(Selector93State::new()));
        Ok(object)
    }

    pub fn selector93(&self, object: ObjectId) -> Result<&Selector93State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector93(state) => Ok(state),
            _ => Err(ObjectError::NotSelector93(object)),
        }
    }

    fn selector93_mut(&mut self, object: ObjectId) -> Result<&mut Selector93State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector93(state) => Ok(state),
            _ => Err(ObjectError::NotSelector93(object)),
        }
    }
}

#[cfg(test)]
mod tests;
