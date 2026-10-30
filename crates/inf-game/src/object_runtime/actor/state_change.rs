//! Shared actor lifecycle transitions, 0047D670 and retirement 0047CE90.
//!
//! +C4 is a signed lifecycle state, not a constructor/class selector. Each
//! reached state is published before its virtual callbacks. Required host
//! queries/effects preserve those boundaries and may mutate the live arena.

use thiserror::Error;

use crate::object_runtime::{
    mark_flag_change, ActorConstructionError, ActorHandle, ObjectArena, ObjectError, ObjectId,
};
use crate::world_statistics::{ActorRetirementAttribution, ActorStatisticCategory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorStateChangeRequest {
    pub target_state: i32,
    /// Native second stack argument, passed to virtual +B0 at state zero.
    pub argument: u32,
    pub source: ActorHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorStateChangeQuery {
    /// Virtual +68: nonzero skips attribution and the selector-18 statistic.
    ExcludesAttribution68,
    /// Virtual +58: a nonnull derived object enables 0048EAD0.
    HasDerived58,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorStateChangeEffect<O> {
    /// Increment the +154 statistic on the owner captured before attribution.
    IncrementSelector18Statistic { owner: O },
    /// 0048EAD0 sign-extends actor byte +78 and receives argument zero.
    RecordPlacementDeath { index: i8 },
    /// Virtual +AC; actor health has already been cleared.
    StateOne {
        actor: ObjectId,
        source: ActorHandle,
    },
    /// Virtual +B0.
    StateZero {
        actor: ObjectId,
        source: ActorHandle,
        argument: u32,
    },
}

pub trait ActorStateChangeHost {
    type Error;
    type AttributionOwner: Copy;

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error>;

    /// Capture G+1BC (mission W+0C), classify the source with 0047F750,
    /// classify the target with 0047F710, then record 00481010. Return the
    /// captured owner, even if a later callback replaces the world's owner.
    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<Self::AttributionOwner, Self::Error>;

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<Self::AttributionOwner>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ActorStateChangeError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} retirement requires process0054E894 at0047CEA0")]
    MissingProcessNullHandle(ObjectId),
    #[error("actor lifecycle host operation failed")]
    Host(E),
}

impl ObjectArena {
    /// 0047F750 first resolves/classifies the source; 0047F710 then classifies
    /// the target. Null and stale handles are native source category 4.
    pub fn retirement_attribution(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<ActorRetirementAttribution, ActorConstructionError> {
        let source = if let Some(source) = self.resolve_actor_handle(source)? {
            ActorStatisticCategory::from_actor(
                self.object(source)?.base.flags,
                self.actor(source)?.word_b4,
            )
        } else {
            ActorStatisticCategory::B4ZeroOrUnresolved
        };
        let target = ActorStatisticCategory::from_actor(
            self.object(actor)?.base.flags,
            self.actor(actor)?.word_b4,
        );
        Ok(ActorRetirementAttribution { target, source })
    }

    /// 0047D670. Downward callbacks can change +C4; the next comparison reads
    /// their actual result. A failure retains all writes/effects already
    /// reached, including the state write before the failed callback.
    pub fn change_actor_state<H: ActorStateChangeHost>(
        &mut self,
        actor: ObjectId,
        request: ActorStateChangeRequest,
        host: &mut H,
    ) -> Result<(), ActorStateChangeError<H::Error>> {
        let initial = self.actor(actor)?.word_c4 as i32;
        if request.target_state < initial {
            while (self.actor(actor)?.word_c4 as i32) > request.target_state {
                let state = (self.actor(actor)?.word_c4 as i32).wrapping_sub(1);
                self.actor_mut(actor)?.word_c4 = state as u32;
                match state {
                    0 => host
                        .effect(
                            self,
                            ActorStateChangeEffect::StateZero {
                                actor,
                                source: request.source,
                                argument: request.argument,
                            },
                        )
                        .map_err(ActorStateChangeError::Host)?,
                    1 => {
                        self.actor_mut(actor)?.health = 0;
                        host.effect(
                            self,
                            ActorStateChangeEffect::StateOne {
                                actor,
                                source: request.source,
                            },
                        )
                        .map_err(ActorStateChangeError::Host)?;
                    }
                    2 => {
                        if !host
                            .query(self, actor, ActorStateChangeQuery::ExcludesAttribution68)
                            .map_err(ActorStateChangeError::Host)?
                        {
                            let owner = host
                                .attribute(self, actor, request.source)
                                .map_err(ActorStateChangeError::Host)?;
                            // Retail rereads +B4 after attribution callbacks.
                            if self.actor(actor)?.word_b4 == 18 {
                                host.effect(
                                    self,
                                    ActorStateChangeEffect::IncrementSelector18Statistic { owner },
                                )
                                .map_err(ActorStateChangeError::Host)?;
                            }
                        }
                        if host
                            .query(self, actor, ActorStateChangeQuery::HasDerived58)
                            .map_err(ActorStateChangeError::Host)?
                        {
                            let index = self.actor(actor)?.byte_78 as i8;
                            host.effect(
                                self,
                                ActorStateChangeEffect::RecordPlacementDeath { index },
                            )
                            .map_err(ActorStateChangeError::Host)?;
                        }
                        let base = &mut self.owned_mut(actor)?.state.base;
                        mark_flag_change(base, base.flags | 0x8000);
                    }
                    _ => {}
                }
            }
        } else {
            while (self.actor(actor)?.word_c4 as i32) < request.target_state {
                let state = (self.actor(actor)?.word_c4 as i32).wrapping_add(1);
                self.actor_mut(actor)?.word_c4 = state as u32;
                if state == 2 {
                    let actor_state = self.actor_mut(actor)?;
                    if actor_state.health == 0 {
                        actor_state.health = 1;
                    }
                    let base = &mut self.owned_mut(actor)?.state.base;
                    mark_flag_change(base, base.flags & !0x8000);
                }
            }
        }
        Ok(())
    }

    /// 0047CE90 retires states 2/3 to state 1 with the process null handle
    /// and argument zero. Its explicit0054E894 input is required only after
    /// the state guard; the registry null value never substitutes for it.
    pub fn retire_actor<H: ActorStateChangeHost>(
        &mut self,
        actor: ObjectId,
        process_null_handle: Option<ActorHandle>,
        host: &mut H,
    ) -> Result<bool, ActorStateChangeError<H::Error>> {
        if !matches!(self.actor(actor)?.word_c4, 2 | 3) {
            return Ok(false);
        }
        let source =
            process_null_handle.ok_or(ActorStateChangeError::MissingProcessNullHandle(actor))?;
        self.change_actor_state(
            actor,
            ActorStateChangeRequest {
                target_state: 1,
                argument: 0,
                source,
            },
            host,
        )?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "state_change_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "state_change_statistics_tests.rs"]
mod statistics_tests;
