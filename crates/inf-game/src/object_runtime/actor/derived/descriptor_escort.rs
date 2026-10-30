//! The Flying Escort descriptor of selectors 34 and 35 (index 7 of
//! `005329F8`, index 9 of `00533D38`): score `0043FCE0` over the escort
//! source `00428990`, change `00435220` over the actor's `+E4`, which is
//! `00428AB0` on both tables, and update `0043FD20` over `00428B80`.

use thiserror::Error;

use super::{DerivedActorError, Family428650Class};
use crate::object_runtime::{ActorHandle, FollowTargetRequest, ObjectArena, ObjectError, ObjectId};

/// `00428AD6`: the status at `+228`, "Flying Escort".
pub const FLYING_ESCORT_STATUS: u32 = 0x0053_1210;
/// `00428B2B`/`00428B52`: the least reach and span the change stores.
const LEAST_REACH: i32 = 0xa4;
const LEAST_SPAN: i32 = 0xfa;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FlyingEscortError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    /// `00428B12` calls the new target's `+10` without a NULL test.
    #[error("actor {0:?}'s escort target allocation returned NULL, whose +10 retail calls")]
    NullTarget(ObjectId),
    #[error("actor {actor:?}'s escort update reads its unwritten +{offset:X}")]
    Uninitialized { actor: ObjectId, offset: u16 },
}

impl ObjectArena {
    /// `00428AB0`, the escort's `+E4`. Entering, `+228` names it, a follow
    /// target on the `+20C` actor (`004825D0(+20C, 0)`) replaces `+148`
    /// (the old one is not released), and with that actor resolved (its
    /// `+10`) `+234` becomes its radius WORD `+28`, at least `0xA4`, and
    /// `+230` that plus twice the escort's own, at least `0xFA`. Leaving
    /// does nothing.
    pub fn change_flying_escort(
        &mut self,
        actor: ObjectId,
        entering: bool,
    ) -> Result<(), FlyingEscortError> {
        if !entering {
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.word_228 = FLYING_ESCORT_STATUS;
        let handle = ActorHandle::from_raw(self.family428650_actor(actor)?.word_20c);
        let target = self
            .follow_target_resource(FollowTargetRequest::resolve_actor(handle))?
            .ok_or(FlyingEscortError::NullTarget(actor))?;
        self.derived_actor_mut(actor)?.target_148 = Some(target);
        let data = self.native_target(target)?;
        let Some(escorted) = self.resolve_target_actor(&data)? else {
            return Ok(());
        };
        let reach = i32::from(self.object(escorted)?.base.word_28).max(LEAST_REACH);
        let own = i32::from(self.object(actor)?.base.word_28);
        let span = reach.wrapping_add(own.wrapping_mul(2)).max(LEAST_SPAN);
        let (reach, span) = (reach as u32, span as u32);
        match &mut self.family428650_actor_mut(actor)?.class {
            Family428650Class::Selector35(state) => {
                (state.word_234, state.word_230) = (reach, span)
            }
            Family428650Class::Selector34(state) => {
                (state.word_234, state.word_230) = (reach, span)
            }
            _ => return Err(DerivedActorError::NotSelector35(actor).into()),
        }
        Ok(())
    }

    /// `00428B80`, the escort's update: past half a second in it (`+188`,
    /// signed) and within 500 of its target (`+14C`, signed), the escort
    /// asks for a new choice (`+22E |= 1`).
    pub fn update_flying_escort(&mut self, actor: ObjectId) -> Result<(), FlyingEscortError> {
        let unknown = |offset| FlyingEscortError::Uninitialized { actor, offset };
        let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(unknown(0x188))?;
        if (elapsed as i32) <= 500_000 {
            return Ok(());
        }
        let distance = self.derived_actor(actor)?.word_14c.ok_or(unknown(0x14c))?;
        if (distance as i32) < 500 {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "descriptor_escort_tests.rs"]
mod tests;
