//! Canonical +98 event22/event10 and the zero-radius arm of 0047DAF0.
//!
//! Every represented receiver returns full DWORD0. Selector2 nevertheless
//! performs its real actor-handle/player lookup before either event suffix.
//! Selector12 consumes its live grant-difference phase only for event22;
//! the vehicle-cleanup event10 bypasses that phase and its +29D writes.

use thiserror::Error;

use super::{
    ActorExtension, ActorState, DerivedActorClass, Family4236d0Class, Family428650Class,
    Family429e20Class, Selector12GrantDifferenceError, Selector12GrantDifferenceRequest,
};
use crate::object_runtime::{
    ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext, PlayerActorLookupError,
    PlayerActorLookupRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorGrantDifferenceDelivery {
    Actor(ObjectId),
    /// 0047DAF0's zero-radius arm visits the actual G+48 actor FIFO.
    ActorList,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorGrantDifferenceNotificationRequest {
    pub delivery: ActorGrantDifferenceDelivery,
}

#[derive(Debug, Error)]
pub enum ActorGrantDifferenceNotificationError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    GrantDifference(#[from] Selector12GrantDifferenceError),
    #[error("0047DBC8's captured actor-list node {0:?} was removed")]
    RemovedCurrentNode(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorVehicleCleanupNotificationRequest {
    /// Captured source ECX from 00415BA0 -> 0047DD20. The represented
    /// event10 receivers do not dereference it or the formed source+32 XYZ.
    pub source: ObjectId,
}

#[derive(Debug, Error)]
pub enum ActorVehicleCleanupNotificationError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    /// The Bomb's 00458042 reads the network DWORD 0055F448.
    #[error("the Bomb's event 10 reads the process network flag, which the walk was not lent")]
    MissingProcess,
    #[error("0047DBC8's captured actor-list node {0:?} was removed")]
    RemovedCurrentNode(ObjectId),
}

#[derive(Clone, Copy)]
enum CanonicalBinding {
    NoOp,
    Selector2Lookup,
    Selector12,
    /// 004557F0 (selectors 26, 27 and 38): event 0xA from a sender whose
    /// +84 is this actor's +88 sets +14E and returns 1.
    OwnerEvent10,
    /// 00458010 (the Bomb, selector 29): event 0xA from its live owner arms
    /// it; from a dead sender it is disposed of in a network game. It
    /// returns 0.
    Bomb29,
}

/// Authenticate the represented final table through actual class custody.
/// Mutable B4, placement, scene and numeric model values never select a body.
fn canonical_binding(actor: &ActorState) -> CanonicalBinding {
    match &actor.extension {
        ActorExtension::Derived(derived) => match &derived.class {
            DerivedActorClass::Selector2(_) => CanonicalBinding::Selector2Lookup,
            DerivedActorClass::Family428650(family) => match &family.class {
                Family428650Class::Family429e20(probe) => match &probe.class {
                    Family429e20Class::Selector12(_) => CanonicalBinding::Selector12,
                    Family429e20Class::Family4236d0(family) => match &family.class {
                        Family4236d0Class::Base
                        | Family4236d0Class::Selector10(_)
                        | Family4236d0Class::Selector36(_) => CanonicalBinding::NoOp,
                    },
                    Family429e20Class::Base => CanonicalBinding::NoOp,
                },
                Family428650Class::Base
                | Family428650Class::Selector34(_)
                | Family428650Class::Selector35(_) => CanonicalBinding::NoOp,
            },
            DerivedActorClass::Base
            | DerivedActorClass::Selector11(_)
            | DerivedActorClass::Selector60(_)
            | DerivedActorClass::Selector77(_) => CanonicalBinding::NoOp,
        },
        ActorExtension::Shared
        | ActorExtension::Selector78(_)
        | ActorExtension::Selector61(_)
        | ActorExtension::Selector62(_)
        | ActorExtension::Selector16(_)
        | ActorExtension::Selector30(_)
        | ActorExtension::Selector15(_)
        | ActorExtension::Selector37(_)
        | ActorExtension::Selector47(_)
        | ActorExtension::Selector49(_)
        | ActorExtension::Selector39(_)
        | ActorExtension::Selector88(_)
        | ActorExtension::Selector66(_)
        | ActorExtension::Selector90(_)
        | ActorExtension::Selector53(_)
        | ActorExtension::Selector52(_)
        | ActorExtension::Selector73(_)
        | ActorExtension::Selector55(_)
        | ActorExtension::Selector17(_)
        | ActorExtension::Selector56
        | ActorExtension::Selector74(_)
        | ActorExtension::Selector84(_)
        | ActorExtension::Selector57
        | ActorExtension::Selector40(_)
        | ActorExtension::Selector41
        | ActorExtension::Selector44(_)
        | ActorExtension::Selector59(_)
        | ActorExtension::Selector18(_)
        | ActorExtension::Selector69(_)
        | ActorExtension::Selector93(_) => CanonicalBinding::NoOp,
        ActorExtension::BehaviorResource(state) => {
            if matches!(state.selector, 26 | 27 | 38) {
                CanonicalBinding::OwnerEvent10
            } else if state.selector == 29 {
                CanonicalBinding::Bomb29
            } else {
                CanonicalBinding::NoOp
            }
        }
    }
}

/// Private native policies, not an extensible callback/success host. Keeping
/// their error types separate prevents event10 from acquiring a grant phase.
trait NotificationMode: Copy {
    type Error: From<ObjectError> + From<PlayerActorLookupError>;

    fn receive(
        self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, Self::Error>;

    fn removed_current_node(actor: ObjectId) -> Self::Error;
}

#[derive(Clone, Copy)]
struct GrantDifferenceMode;

#[derive(Clone, Copy)]
struct VehicleCleanupMode {
    // Same captured operand throughout the fanout. Only 004557F0 reads it,
    // comparing the sender's +84 with its own +88.
    source: ObjectId,
}

struct NotificationRequest<M> {
    delivery: ActorGrantDifferenceDelivery,
    mode: M,
}

impl NotificationMode for GrantDifferenceMode {
    type Error = ActorGrantDifferenceNotificationError;

    fn receive(
        self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, Self::Error> {
        match arena.notification_receiver_prefix::<Self::Error>(actor, context)? {
            CanonicalBinding::Selector12 => Ok(arena
                .notify_selector12_grant_difference(Selector12GrantDifferenceRequest { actor })?),
            CanonicalBinding::NoOp
            | CanonicalBinding::Selector2Lookup
            | CanonicalBinding::OwnerEvent10
            | CanonicalBinding::Bomb29 => Ok(0),
        }
    }

    fn removed_current_node(actor: ObjectId) -> Self::Error {
        Self::Error::RemovedCurrentNode(actor)
    }
}

impl NotificationMode for VehicleCleanupMode {
    type Error = ActorVehicleCleanupNotificationError;

    fn receive(
        self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, Self::Error> {
        let binding = arena.notification_receiver_prefix::<Self::Error>(actor, context)?;
        // 00468BEF excludes event10 before the first-player/grant phase.
        // Other represented bodies exclude their non-10 arms; selector2's
        // event10 BYTE map 00420D8E=8 reaches 00420D5C after the real lookup.
        if let CanonicalBinding::OwnerEvent10 = binding {
            // 00455801: a nonnull sender other than the +88 owner is refused.
            if arena.actor(self.source)?.handle != arena.actor(actor)?.owner_handle {
                return Ok(0);
            }
            arena.mark_behavior_resource_14e(actor)?;
            return Ok(1);
        }
        if let CanonicalBinding::Bomb29 = binding {
            let network = context
                .process
                .ok_or(ActorVehicleCleanupNotificationError::MissingProcess)?
                .network_flag
                != 0;
            if arena.bomb29_message(actor, Some(self.source), 10, network)?
                == super::Bomb29Message::Dispose
            {
                arena.dispose(actor)?;
            }
        }
        Ok(0)
    }

    fn removed_current_node(actor: ObjectId) -> Self::Error {
        Self::Error::RemovedCurrentNode(actor)
    }
}

impl ObjectArena {
    /// Deliver canonical event22 to one actor or the zero-radius actor list.
    /// No source/XYZ/range input is consumed by this native broadcast arm.
    /// Only a reached selector2 receiver requires the borrowed process NULL.
    ///
    /// The same typed arena represents the captured native G/list owner.
    /// Its FIFO includes parked/disposed actors until actual destruction.
    /// Canonical receivers do not mutate this list; raw owner replacement,
    /// ObjectId ABA, arbitrary overrides and physical aliases are not admitted.
    /// This does not attach the enclosing capture reaction or actor update.
    pub fn notify_actor_grant_difference(
        &mut self,
        request: ActorGrantDifferenceNotificationRequest,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, ActorGrantDifferenceNotificationError> {
        self.notify_actor_zero_radius(
            NotificationRequest {
                delivery: request.delivery,
                mode: GrantDifferenceMode,
            },
            context,
        )
    }

    /// Complete the represented vehicle cleanup notification at 00415BA9:
    /// actual source, zero radius, event10, fourth DWORD0. This is only its
    /// notification phase; 00415BB0's common cleanup remains the caller's.
    ///
    /// The source is captured without dereferencing or validating it. Only
    /// reached FIFO receivers are authenticated; selector2 still requires the
    /// live process NULL before reading the event. No player/mask/29D phase
    /// occurs at selector12. Earlier callbacks and list/source custody survive
    /// a later error. The same stable arena/process domain as event22 applies.
    pub fn notify_actor_vehicle_cleanup(
        &mut self,
        request: ActorVehicleCleanupNotificationRequest,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, ActorVehicleCleanupNotificationError> {
        self.notify_actor_zero_radius(
            NotificationRequest {
                delivery: ActorGrantDifferenceDelivery::ActorList,
                mode: VehicleCleanupMode {
                    source: request.source,
                },
            },
            context,
        )
    }

    /// 0047DD20(radius, 10, 0) from `source`: 0047DAF0's radius arm sends
    /// event 10 to each listed actor whose squared distance from the
    /// source's live position (signed WORD deltas, wrapping sums) is at most
    /// radius² unsigned, through the vehicle cleanup's receivers: a grenade
    /// or mine the source owns marks itself to detonate. Radius zero is the
    /// cleanup's own arm.
    pub fn notify_actor_owner_event_10(
        &mut self,
        source: ObjectId,
        radius: u32,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, ActorVehicleCleanupNotificationError> {
        if radius == 0 {
            return self.notify_actor_vehicle_cleanup(
                ActorVehicleCleanupNotificationRequest { source },
                context,
            );
        }
        let limit = radius.wrapping_mul(radius);
        let mode = VehicleCleanupMode { source };
        let position = |arena: &ObjectArena, actor: ObjectId| {
            arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
        };
        let (mut index, mut sum) = (0, 0_u32);
        while let Some(&actor) = self.actor_list().get(index) {
            index += 1;
            if self.construction_is_stopped(actor)? || self.disposal_is_stopped(actor)? {
                continue;
            }
            let centre = position(self, source)?;
            let other = position(self, actor)?;
            let squares = (0..3).fold(0_i32, |squares, axis| {
                let delta = i32::from(centre[axis].wrapping_sub(other[axis]) as i16);
                squares.wrapping_add(delta.wrapping_mul(delta))
            }) as u32;
            if squares > limit {
                continue;
            }
            let result = mode.receive(self, actor, context)?;
            index = self
                .actor_list()
                .iter()
                .position(|&current| current == actor)
                .ok_or(ActorVehicleCleanupNotificationError::RemovedCurrentNode(
                    actor,
                ))?
                + 1;
            sum = sum.wrapping_add(result);
        }
        Ok(sum)
    }

    fn notify_actor_zero_radius<M: NotificationMode>(
        &mut self,
        request: NotificationRequest<M>,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<u32, M::Error> {
        match request.delivery {
            ActorGrantDifferenceDelivery::Actor(actor) => {
                request.mode.receive(self, actor, context)
            }
            ActorGrantDifferenceDelivery::ActorList => {
                let (mut index, mut sum) = (0, 0_u32);
                while let Some(&actor) = self.actor_list().get(index) {
                    // Portable construction/disposal containment leaves the
                    // FIFO intact and withholds callbacks from stopped owners.
                    if self.construction_is_stopped(actor)? || self.disposal_is_stopped(actor)? {
                        index += 1;
                        continue;
                    }
                    let result = request.mode.receive(self, actor, context)?;
                    // 0047DBC8 reloads the captured node's successor before
                    // 0047DBCA adds the completed callback's full EAX.
                    index = self
                        .actor_list()
                        .iter()
                        .position(|&current| current == actor)
                        .ok_or_else(|| M::removed_current_node(actor))?
                        + 1;
                    sum = sum.wrapping_add(result);
                }
                Ok(sum)
            }
        }
    }

    /// Complete the shared class admission and selector2's mandatory
    /// pre-event lookup. No mode may bypass or move that lookup to request
    /// entry. Each mode owns its distinct suffix after this complete phase.
    fn notification_receiver_prefix<E>(
        &mut self,
        actor: ObjectId,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<CanonicalBinding, E>
    where
        E: From<ObjectError> + From<PlayerActorLookupError>,
    {
        let binding = canonical_binding(self.actor(actor)?);
        if let CanonicalBinding::Selector2Lookup = binding {
            // 00420BE4/BEB precede even the event argument read. Raw +84
            // is authoritative here; no registry resolution is performed.
            let handle = self.actor(actor)?.handle;
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle }, context)?;
        }
        Ok(binding)
    }
}

#[cfg(test)]
mod native_evidence;
#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod tests;
