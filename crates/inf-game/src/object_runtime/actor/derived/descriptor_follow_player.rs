//! The 004236D0 family's Follow Player descriptor (`00546A5C`, index 8 of
//! the mode-2 table `0053076C`): score `00425D30` over the actor's virtual
//! `+12C` (`00423B40`), change `00425D70` over `+134` (`00423A80`). Its
//! update `00425D90` over `+130` (`00423A20`) is the shared timed retry.
//! Selectors 9, 10, 21 and 36 (tables `00520B84`, `00520CC8`, `0051F7A8`,
//! `00527EE0`) share all three virtuals; other classes never reach them.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{Family428650Class, Family429e20Class};
use crate::object_runtime::{FollowTargetRequest, ObjectArena, ObjectError, ObjectId};

/// The status `00423AA5` publishes at `+228`: "Follow Player".
pub const FOLLOW_PLAYER_STATUS: u32 = 0x0053_0d20;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyFollowPlayerError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} has another class's virtual +12C/+134, not 00423B40/00423A80")]
    RequiredActorVirtual(ObjectId),
}

impl ObjectArena {
    /// Whether the actor's final table is one of the 004236D0 family's.
    pub(super) fn follows_player_as_family4236d0(
        &self,
        actor: ObjectId,
    ) -> Result<bool, ObjectError> {
        Ok(matches!(
            &self.family428650_actor(actor)?.class,
            Family428650Class::Family429e20(state)
                if matches!(&state.class, Family429e20Class::Family4236d0(_))
        ))
    }

    /// `00423B40`, the score's source. Nothing while target `+1F4` or
    /// `+1D4` is held, without a first player's unit (`0046B030`) or while
    /// that unit's lifecycle `+C4` is at most 1. Otherwise the squared
    /// distance (signed WORD differences, DWORD squares) must lie above
    /// `0x40000` (512 units) and below the DWORD square of `+164`, both
    /// unsigned, and `00427620` returns `+164` less its root (`0048EFC0`).
    /// The current flag the wrapper passes is ignored (`RET 4`).
    pub(super) fn follow_player_source(&mut self, actor: ObjectId) -> Result<i32, ObjectError> {
        let targets = self.family428650_actor(actor)?.targets_1d4_1fc;
        if targets[4].is_some() || targets[0].is_some() {
            return Ok(0);
        }
        let Some(player) = self.first_player_primary_actor()? else {
            return Ok(0);
        };
        if (self.actor(player)?.word_c4 as i32) <= 1 {
            return Ok(0);
        }
        let own = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let other = self
            .object(player)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(player))?;
        let squared = (0..3).fold(0u32, |sum, axis| {
            let delta = i32::from(own[axis].wrapping_sub(other[axis]) as i16);
            sum.wrapping_add(delta.wrapping_mul(delta) as u32)
        });
        let radius = self.derived_actor(actor)?.word_164;
        if squared >= radius.wrapping_mul(radius) as u32 || squared <= 0x4_0000 {
            return Ok(0);
        }
        Ok(radius.wrapping_sub(isqrt_48efc0(squared as i32) as i32))
    }

    /// `00423A80`. Entering publishes "Follow Player" at `+228`, then looks
    /// up the first player's unit (`0046B030`). With one and neither target
    /// `+1F4` nor `+1D4` held, `+148` takes a 0x20 follow target on the
    /// unit's handle (`004825D0`, null when the allocation fails) and the
    /// distance `+208` the unit's size `+28` plus 0x100. Otherwise it sets
    /// `+22E` bit 2. The dispatcher's `+10C` has already released the old
    /// `+148`. Leaving does nothing.
    pub fn change_family_follow_player(
        &mut self,
        actor: ObjectId,
        entering: bool,
    ) -> Result<(), FamilyFollowPlayerError> {
        if !entering {
            return Ok(());
        }
        if !self.follows_player_as_family4236d0(actor)? {
            return Err(FamilyFollowPlayerError::RequiredActorVirtual(actor));
        }
        self.family428650_actor_mut(actor)?.word_228 = FOLLOW_PLAYER_STATUS;
        let player = self.first_player_primary_actor()?;
        let targets = self.family428650_actor(actor)?.targets_1d4_1fc;
        match player {
            Some(player) if targets[4].is_none() && targets[0].is_none() => {
                let handle = self.actor(player)?.handle;
                let target =
                    self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
                self.derived_actor_mut(actor)?.target_148 = target;
                let size = u32::from(self.object(player)?.base.word_28);
                self.family428650_actor_mut(actor)?.words_204_208[1] = Some(size + 0x100);
            }
            _ => self.family428650_actor_mut(actor)?.word_22e |= 4,
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "descriptor_follow_player_tests.rs"]
mod tests;
