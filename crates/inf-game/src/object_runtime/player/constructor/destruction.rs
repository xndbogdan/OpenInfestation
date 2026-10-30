//! Published player destruction 00469FE0, in the completed constructor domain.
//!
//! The original removes +C8/+CC before message 004131F0, then releases digital
//! +84..+68 and analog +64/+60/+5C. Actual input errors are retained portable
//! stops, not native faults/SEH. Allocation free and physical pointer parity
//! are excluded; an unlinked record stays in the actual construction carrier.

use super::{
    InputRuntime, ObjectArena, PlayerConstructionPhase, PlayerId, PlayerInputReleaseError,
    WorldPlayerConstruction,
};
use crate::object_runtime::player::message::PlayerMessageError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerDestructionPhase {
    Unlinked,
    MessageDestroyed,
    Complete,
}

#[derive(Debug, Error)]
pub enum PlayerDestructionError {
    #[error("player construction belongs to another object arena")]
    ForeignArena,
    #[error("player destruction requires a published construction, found {0:?}")]
    NotPublished(PlayerConstructionPhase),
    #[error("player destruction already attempted through {0:?}")]
    AlreadyAttempted(PlayerDestructionPhase),
    #[error("player {0:?} input component cleanup was already attempted")]
    InputComponentReleaseAttempted(PlayerId),
    #[error("constructed player {0:?} is absent from its actual published FIFO")]
    NotListed(PlayerId),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
    #[error(transparent)]
    Input(#[from] PlayerInputReleaseError),
}

impl ObjectArena {
    /// Complete the represented 00469FE0 phases for the same actual carrier,
    /// published player and InputRuntime. Validate before effects, then retain
    /// every reached prefix. Repeating the whole destructor is rejected even
    /// after an input failure; explicit component cleanup can recover remaining
    /// references without replaying FIFO unlink or message destruction.
    /// Unpublished carriers must use only their component cleanup method.
    pub fn destroy_constructed_world_player(
        &mut self,
        construction: &mut WorldPlayerConstruction,
        input: &mut InputRuntime,
    ) -> Result<(), PlayerDestructionError> {
        if construction.arena != self.identity {
            return Err(PlayerDestructionError::ForeignArena);
        }
        if let Some(phase) = construction.destruction_phase {
            return Err(PlayerDestructionError::AlreadyAttempted(phase));
        }
        if construction.phase != PlayerConstructionPhase::Published {
            return Err(PlayerDestructionError::NotPublished(construction.phase));
        }
        if construction.input_release_attempted {
            return Err(PlayerDestructionError::InputComponentReleaseAttempted(
                construction.player,
            ));
        }
        let index = self
            .players
            .list
            .iter()
            .position(|player| player.id == construction.player)
            .ok_or(PlayerDestructionError::NotListed(construction.player))?;

        // 0046A017/2C: remove the actual FIFO membership before lower calls.
        construction.retiring = Some(self.players.list.remove(index));
        construction.destruction_phase = Some(PlayerDestructionPhase::Unlinked);
        construction
            .retiring
            .as_ref()
            .expect("same unlinked published record")
            .message_88
            .destroy_known_empty()?;
        // 004131F0 deleted the live boxes.
        self.players
            .message_boxes
            .retain(|(player, _)| *player != construction.player);
        construction.destruction_phase = Some(PlayerDestructionPhase::MessageDestroyed);

        // 0046A03E..0046A0C6. This is not the public component-cleanup call:
        // its admission history must distinguish manually released wrappers.
        construction.inputs.release(input)?;
        construction.destruction_phase = Some(PlayerDestructionPhase::Complete);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
