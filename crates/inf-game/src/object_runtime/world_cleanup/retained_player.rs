//! Reached retained-player/manager suffix of 0048E890, after effect cleanup.
//!
//! One captured first player, two genuine controlled lookups and the canonical
//! class-only +64 policy precede the shared ordered vehicle snapshot. The same
//! player's actual message then cleans up before the existing manager drain.
//! Stable exclusive arena/registry and immutable current source loans exclude
//! callback rebinding; no source preflight, input release or retry is added.

use super::{ObjectArena, WorldCleanupError, WorldCleanupRequest};
use crate::object_runtime::{PlayerMessageOperation, PlayerMessageRequest};

impl ObjectArena {
    /// Complete post-effects player/manager phase of 0048E890. Callers retain
    /// earlier map, BYTE+12 and effect prefixes; errors preserve every reached
    /// player/cache/message/manager prefix and do not imply normal completion.
    /// The actual player FIFO and input references survive manager retirement.
    pub fn retire_world_owners(
        &mut self,
        request: WorldCleanupRequest<'_, '_>,
    ) -> Result<(), WorldCleanupError> {
        self.retire_world_player()?;
        // 0048E901 reaches captured manager+08 only after that complete prefix.
        // Its actual source/context fields stay lazy in the existing owner.
        self.cleanup_world(request)
    }

    /// `0048E8C4..0048E8FA`, the player prefix alone: the vehicle saved into
    /// the first player, then that player's message cleanup, which deletes
    /// its shown menus. Their destructors (`[Self::take_destroyed_menus]`)
    /// run before the manager drain, while the world's actors still live: a
    /// shop menu undocks from its factory (`004315E0`, `0042F2A0`).
    pub fn retire_world_player(&mut self) -> Result<(), WorldCleanupError> {
        // 0048E8C4 captures G+60's first player once and keeps it in ESI.
        if let Some(player) = self.first_player() {
            // 0048E8D1's actual NULL result skips only the vehicle copy.
            if self.player_actor(player)?.is_some() {
                // 0048E8DC is a fresh lookup, not reuse of the first result.
                // Native dereferences it without another NULL guard; missing
                // typed ownership is a required boundary, never that bypass.
                let actor = self
                    .player_actor(player)?
                    .ok_or(WorldCleanupError::RequiredSecondControlledActor(player))?;
                // The actual borrowed selector2 component's native +64 returns
                // self, so this actor is precisely the copy's vehicle identity.
                if self.actor_vehicle_component(actor)?.is_some() {
                    self.save_vehicle_into_player(player, actor)?; // 0048E8EF
                }
            }
            let recipient = self.player_message(player)?.identity();
            self.apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::CleanupWorld, // 0048E8FA
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
