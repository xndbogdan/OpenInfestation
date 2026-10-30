//! The event22 branch of 00468BE0 and its pure 004753A0 grant difference.
//! This is a direct callback body, not a general virtual +98 dispatcher.

use thiserror::Error;

use super::DerivedActorError;
#[cfg(test)]
use crate::object_runtime::PlayerId;
use crate::object_runtime::{GrantOwner, ObjectArena, ObjectId, PlayerAttachError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector12GrantDifferenceRequest {
    pub actor: ObjectId,
}

#[derive(Debug, Error)]
pub enum Selector12GrantDifferenceError {
    #[error(transparent)]
    Object(#[from] crate::object_runtime::ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
}

/// 004752E0 within the bounded IDs0..174 supplied by 004753A0. ID0 returns
/// true before an owner read; every other reached query reads one mask DWORD.
/// Arbitrary raw DWORD IDs and their adjacent-storage accesses are not admitted.
fn grant_owner_has(
    arena: &ObjectArena,
    owner: GrantOwner,
    id: u16,
) -> Result<bool, Selector12GrantDifferenceError> {
    if id == 0 {
        return Ok(true);
    }
    let index = usize::from(id >> 5);
    let word = match owner {
        GrantOwner::Selector12Actor24c(actor) => {
            arena.selector12_actor(actor)?.grants_24c.mask_04[index]
        }
        GrantOwner::Player9c(player) => arena.world_player(player)?.grants_9c[index],
        GrantOwner::Selector15Factory198(factory) => {
            arena.selector15(factory)?.grants_198.mask_04[index]
        }
    };
    Ok(word & (1_u32 << (id & 31)) != 0)
}

/// 004753A0 captures source and target identities once. Source is queried
/// first for each ID; only its owned IDs consume the target. Equal identities
/// are valid, and an empty source does not require any nonzero target query.
fn grant_difference(
    arena: &ObjectArena,
    source: GrantOwner,
    target: GrantOwner,
) -> Result<bool, Selector12GrantDifferenceError> {
    for id in 0..0xaf {
        if grant_owner_has(arena, source, id)? && !grant_owner_has(arena, target, id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl ObjectArena {
    /// 00468BE0's event22 body: select the actual first player, compare actor
    /// +24C source against player +9C target and store the Boolean at +29D.
    /// No player preserves +29D. All successful exits contribute full DWORD0.
    ///
    /// The represented Selector12 enum admits this body independently of B4.
    /// Raw stale-this/no-player bypasses, selector13, other events, broadcast
    /// traversal and production virtual dispatch remain separate boundaries.
    pub fn notify_selector12_grant_difference(
        &mut self,
        request: Selector12GrantDifferenceRequest,
    ) -> Result<u32, Selector12GrantDifferenceError> {
        self.selector12_actor(request.actor)?;
        let Some(player) = self.first_player() else {
            return Ok(0);
        };
        let differs = grant_difference(
            self,
            GrantOwner::Selector12Actor24c(request.actor),
            GrantOwner::Player9c(player),
        )?;
        // 00468C10/00468C1D write a literal byte, not the previous value OR AL.
        self.selector12_actor_mut(request.actor)?.byte_29d = u8::from(differs);
        Ok(0)
    }
}

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod tests;
