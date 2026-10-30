//! Virtual +A8 = 0042A760 for the 00429E20 family. The guard arm owns no
//! collision geometry; the clear arm requires the actual 0047BFD0 service.

use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use thiserror::Error;

/// One callback's actor and opaque custody of its four ordered native operands.
/// At 0047C67F these are the 20-byte contact result, H11 four-byte selector
/// record, terrain-cell XYZ WORDs, and saved actor XYZ WORDs. The contact's
/// three floats and signed low-WORD displacement are consumed by 0047BFD0;
/// this wrapper neither decodes that packet nor constructs any missing input.
/// Borrowing the request keeps operand custody alive even on the guard arm.
pub struct FamilyCollisionGuardRequest<O> {
    pub actor: ObjectId,
    pub continuation_operands: O,
}

pub trait FamilyCollisionGuardHost<O> {
    type Error;

    /// Actual mutable 0047BFD0, receiving all four operands in original order.
    /// Successful return preserves AL, independently of arbitrary upper EAX.
    /// An error preserves every reached host mutation without cleanup.
    fn continue_collision(
        &mut self,
        arena: &mut ObjectArena,
        request: &mut FamilyCollisionGuardRequest<O>,
    ) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyCollisionGuardError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("family collision host failed at 0047BFD0: {0}")]
    Host(E),
}

impl ObjectArena {
    /// Bounded 0042A760, admitted only for the represented 00429E20 family.
    /// Admission is a portable class preflight, outside native prefix claims.
    /// There is no attachment to the enclosing 0047C1D0 geometry interpreter.
    pub fn dispatch_family_collision_guard<O, H: FamilyCollisionGuardHost<O>>(
        &mut self,
        request: &mut FamilyCollisionGuardRequest<O>,
        host: &mut H,
    ) -> Result<u8, FamilyCollisionGuardError<H::Error>> {
        let actor = request.actor;
        self.family429e20_actor(actor)?;
        let flags = &mut self.family428650_actor_mut(actor)?.word_22e;
        if *flags & 0x8000 != 0 {
            // 0042A76C..0042A77A reads no stack operand and clears only 8000.
            // In particular, do not acquire/decode the contact before this arm.
            *flags &= 0x7fff;
            return Ok(1);
        }
        host.continue_collision(self, request)
            .map(|eax| eax as u8)
            .map_err(FamilyCollisionGuardError::Host)
    }
}

#[cfg(test)]
mod tests;
