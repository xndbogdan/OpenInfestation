//! Shared instantaneous damage 0047C6E0. This owner keeps its unconditional
//! virtual +B4 and later task/source/statistics calls in native order. Class
//! damage responses and source +108 remain required synchronous callbacks.

use thiserror::Error;

mod timed;
pub use timed::{ActorTimedDamageError, ActorTimedDamageHost, ActorTimedDamageRequest};

use super::{ActorExtension, DerivedActorClass};
use crate::{
    object_runtime::{
        mark_flag_change, ActorConstructionError, ActorHandle, ObjectArena, ObjectError, ObjectId,
    },
    world_furniture_effect::{resisted_effect_damage, DamageResistance},
    world_radius_effect::ScaledRadiusEffect,
    world_statistics::ActorDamageStatistic,
};

/// The four-byte packet passed by address to 0047C6E0/virtual +B4. Only the
/// signed strength and channel are consumed by 0042A8F0. The callback also
/// receives byte +3 without inventing an allocator-carried value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDamagePacket {
    pub strength: i16,
    pub channel: u8,
    pub byte_03: Option<u8>,
}

impl From<ScaledRadiusEffect> for ActorDamagePacket {
    fn from(effect: ScaledRadiusEffect) -> Self {
        Self {
            strength: effect.strength,
            channel: effect.channel,
            byte_03: effect.byte_03,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDamageRequest {
    pub packet: ActorDamagePacket,
    pub source: ActorHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDamageCallback {
    /// Cached result of 0042A8F0 using the target's reached +A4 table.
    pub adjusted_damage: i32,
    pub packet: ActorDamagePacket,
    pub source: ActorHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorDamageOperation {
    VirtualB4,
    TaskOwner { classification: u32 },
    TaskSource,
    SetTaskSource,
    SourceComponent108,
    StatisticsOwner,
    RecordStatistics,
}

pub trait ActorDamageHost {
    type Error;
    type TaskOwner: Copy;
    type StatisticsOwner: Copy;

    /// Current0054E894 captured by482C60 only after the matching-task
    /// source gate. The captured value survives the following field read.
    fn process_null_handle(&self) -> Option<ActorHandle>;

    /// Actual target virtual +B4(damage, packet, source), including class
    /// reactions, health, task, lifecycle and network effects. Called even
    /// when adjusted damage is zero. Reached arena mutations survive errors.
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error>;

    /// Current process 0055F100[classification]. None means proven native
    /// null; unavailable portable task storage must instead return an error.
    fn task_owner(
        &mut self,
        arena: &mut ObjectArena,
        classification: u32,
    ) -> Result<Option<Self::TaskOwner>, Self::Error>;
    fn task_source(
        &mut self,
        arena: &mut ObjectArena,
        owner: Self::TaskOwner,
    ) -> Result<ActorHandle, Self::Error>;
    fn set_task_source(
        &mut self,
        arena: &mut ObjectArena,
        owner: Self::TaskOwner,
        source: ActorHandle,
    ) -> Result<(), Self::Error>;

    /// Actual nonnull source component's virtual +108(target), after source
    /// resolution and proven +58/+CC dispatch. It may mutate either actor.
    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
    ) -> Result<(), Self::Error>;

    /// Capture G+1BC/W+0C after source notification, before fresh source then
    /// target classification. record_statistics must use this exact owner.
    fn statistics_owner(
        &mut self,
        arena: &mut ObjectArena,
    ) -> Result<Self::StatisticsOwner, Self::Error>;
    fn record_statistics(
        &mut self,
        arena: &mut ObjectArena,
        owner: Self::StatisticsOwner,
        statistic: ActorDamageStatistic,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ActorDamageError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Actor(#[from] ActorConstructionError),
    #[error("actor {actor:?} damage channel {channel} lies outside its retained +A4 table")]
    UnsupportedChannel { actor: ObjectId, channel: u8 },
    #[error("actor {0:?} task-source clear requires process0054E894 at00482C60")]
    MissingProcessNullHandle(ObjectId),
    /// Native 0047C744 dereferences the selected task unconditionally. Do not
    /// turn an out-of-range classification or null task into a skipped call.
    #[error("native damage task owner is null for classification {classification}")]
    NullTaskOwner { classification: u32 },
    #[error("actor damage host failed at {operation:?}: {error}")]
    Host {
        operation: ActorDamageOperation,
        error: E,
    },
}

impl ObjectArena {
    /// Supported source +58 then +CC vtables: derived classes return self
    /// from +58; only the 00428650 family returns self from +CC. Existence
    /// of this component does not depend on its separate +22C active byte.
    pub fn actor_damage_source_component(
        &self,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Derived(state)
                if matches!(state.class, DerivedActorClass::Family428650(_)) =>
            {
                Some(actor)
            }
            _ => None,
        })
    }

    /// Complete outer 0047C6E0, with required class/task/source/statistics
    /// owners. Success requires every reached callback; it does not substitute
    /// a health-only or retirement-only implementation for virtual +B4.
    pub fn apply_actor_damage<H: ActorDamageHost>(
        &mut self,
        actor: ObjectId,
        request: ActorDamageRequest,
        host: &mut H,
    ) -> Result<(), ActorDamageError<H::Error>> {
        if crate::event_log::enabled() {
            let target = self
                .object(actor)
                .map_or(0, |object| object.base.resource_word);
            crate::event_log::damage(
                request.packet.strength,
                request.packet.channel,
                target,
                request.source.raw(),
            );
        }
        let responses = self.actor(actor)?.damage_responses();
        let &(threshold, multiplier) = responses.get(usize::from(request.packet.channel)).ok_or(
            ActorDamageError::UnsupportedChannel {
                actor,
                channel: request.packet.channel,
            },
        )?;
        let damage = resisted_effect_damage(
            request.packet.strength,
            DamageResistance {
                threshold,
                multiplier,
            },
            0,
        );
        host.virtual_b4(
            self,
            actor,
            ActorDamageCallback {
                adjusted_damage: damage,
                packet: request.packet,
                source: request.source,
            },
        )
        .map_err(|error| ActorDamageError::Host {
            operation: ActorDamageOperation::VirtualB4,
            error,
        })?;

        let base = &mut self.owned_mut(actor)?.state.base;
        if base.flags & 0x20_0000 != 0 {
            mark_flag_change(base, base.flags | 0x40_0000);
        }
        // 0047C72E caches classification once. The table slot is read a
        // second time on the equality branch, but actor +B4 is not reread.
        let classification = self.actor(actor)?.word_b4;
        let owner = damage_task_owner(self, host, classification)?;
        let source = host
            .task_source(self, owner)
            .map_err(|error| ActorDamageError::Host {
                operation: ActorDamageOperation::TaskSource,
                error,
            })?;
        if source == request.source {
            let owner = damage_task_owner(self, host, classification)?;
            // Exact 00482C60(source,0): a process-null source returns before
            // reading owner +8; otherwise reread it before clearing to null.
            let null = host
                .process_null_handle()
                .ok_or(ActorDamageError::MissingProcessNullHandle(actor))?;
            if request.source != null {
                let source =
                    host.task_source(self, owner)
                        .map_err(|error| ActorDamageError::Host {
                            operation: ActorDamageOperation::TaskSource,
                            error,
                        })?;
                if source == request.source {
                    host.set_task_source(self, owner, null).map_err(|error| {
                        ActorDamageError::Host {
                            operation: ActorDamageOperation::SetTaskSource,
                            error,
                        }
                    })?;
                }
            }
        }

        if damage != 0 {
            if let Some(source) = self.resolve_actor_handle(request.source)? {
                if let Some(component) = self.actor_damage_source_component(source)? {
                    host.source_component_108(self, component, actor)
                        .map_err(|error| ActorDamageError::Host {
                            operation: ActorDamageOperation::SourceComponent108,
                            error,
                        })?;
                }
            }
            let owner = host
                .statistics_owner(self)
                .map_err(|error| ActorDamageError::Host {
                    operation: ActorDamageOperation::StatisticsOwner,
                    error,
                })?;
            // Reuse exact 0047F750 → 0047F710 categorization, resolving the
            // original handle again after the component callback has returned.
            let categories = self.retirement_attribution(actor, request.source)?;
            host.record_statistics(
                self,
                owner,
                ActorDamageStatistic {
                    amount: damage,
                    categories,
                },
            )
            .map_err(|error| ActorDamageError::Host {
                operation: ActorDamageOperation::RecordStatistics,
                error,
            })?;
        }
        Ok(())
    }
}

fn damage_task_owner<H: ActorDamageHost>(
    arena: &mut ObjectArena,
    host: &mut H,
    classification: u32,
) -> Result<H::TaskOwner, ActorDamageError<H::Error>> {
    if classification >= 19 {
        return Err(ActorDamageError::NullTaskOwner { classification });
    }
    host.task_owner(arena, classification)
        .map_err(|error| ActorDamageError::Host {
            operation: ActorDamageOperation::TaskOwner { classification },
            error,
        })?
        .ok_or(ActorDamageError::NullTaskOwner { classification })
}

#[cfg(test)]
#[path = "damage_tests.rs"]
mod tests;
