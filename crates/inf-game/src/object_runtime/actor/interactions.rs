//! Canonical actor +50/+54, Selector30 numeric +CC and void +9C callbacks.
//!
//! The actor-bearing APIs authenticate the actual represented class before
//! dispatch. Direct constant-body calls with stale pointers, arbitrary virtual
//! overrides and raw aliases are outside that custody. These owners do not add
//! the selector12/13 00468350 walk or production actor dispatch.

use thiserror::Error;

use super::{ActorExtension, ActorState, DerivedActorClass, Family428650Class};
use crate::{
    frontend_camera::AnimationClockSource,
    object_runtime::{ObjectArena, ObjectError, ObjectId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorComponentQueryMode {
    /// Every represented canonical +50 table selects the null 00401300 body.
    Self50,
    /// Selector30 selects 00404610 (self); other represented +54s are null.
    Interaction54,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorComponentQueryRequest {
    pub actor: ObjectId,
    pub mode: ActorComponentQueryMode,
}

/// 0048BE30's numeric +CC on the actual component returned by Selector30 +54.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorInteractionRelationRequest {
    pub component: ObjectId,
    pub source: Option<ObjectId>,
}

/// Actual +9C arguments, retained even though the admitted bodies ignore them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorInteractionNotificationRequest {
    pub candidate: ObjectId,
    pub source: Option<ObjectId>,
    pub distance_squared: u32,
}

/// Borrow the existing process clock. Its value is read only on the reached
/// Selector35 nonzero +242 arm, with no callback or independent scalar copy.
#[derive(Clone, Copy)]
pub struct ActorInteractionNotificationContext<'a> {
    pub animation_clock: Option<&'a dyn AnimationClockSource>,
}

impl std::fmt::Debug for ActorInteractionNotificationContext<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug inspection is not a native clock read.
        formatter
            .debug_struct("ActorInteractionNotificationContext")
            .field("animation_clock_available", &self.animation_clock.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorInteractionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("interaction component {0:?} answers +CC with a relation the port does not own")]
    NotSelector30(ObjectId),
    #[error("actor {actor:?} has no known interaction byte at +{offset:03X}")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("0043F86A requires the actual 005471C0 animation clock")]
    MissingAnimationClock,
}

#[derive(Clone, Copy)]
enum CanonicalBinding {
    NullComponentNoOp,
    /// +54 = 00404610 and the RET8 +9C: the 0040D800 family's tables.
    Family40d800,
    /// The factory's 00521940 also answers +50 with itself.
    Factory,
    Selector35,
}

/// The same concrete final-table custody selects all three callback policies.
fn canonical_binding(actor: &ActorState) -> CanonicalBinding {
    match &actor.extension {
        ActorExtension::Derived(state) => match &state.class {
            DerivedActorClass::Family428650(family) => match &family.class {
                Family428650Class::Selector35(_) => CanonicalBinding::Selector35,
                Family428650Class::Base
                | Family428650Class::Family429e20(_)
                | Family428650Class::Selector34(_) => CanonicalBinding::NullComponentNoOp,
            },
            DerivedActorClass::Base
            | DerivedActorClass::Selector2(_)
            | DerivedActorClass::Selector11(_)
            | DerivedActorClass::Selector60(_)
            | DerivedActorClass::Selector77(_) => CanonicalBinding::NullComponentNoOp,
        },
        ActorExtension::Selector15(_) => CanonicalBinding::Factory,
        ActorExtension::Selector30(_)
        | ActorExtension::Selector37(_)
        | ActorExtension::Selector47(_)
        | ActorExtension::Selector49(_)
        | ActorExtension::Selector39(_) => CanonicalBinding::Family40d800,
        ActorExtension::Shared
        | ActorExtension::Selector78(_)
        | ActorExtension::Selector61(_)
        | ActorExtension::Selector62(_)
        | ActorExtension::Selector16(_)
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
        | ActorExtension::Selector93(_)
        | ActorExtension::BehaviorResource(_) => CanonicalBinding::NullComponentNoOp,
    }
}

impl ObjectArena {
    /// Canonical pure +50/+54 selection. 00401300 reads no payload;
    /// 00404610 returns the same authenticated actor, not a fabricated pointer.
    pub fn query_actor_component(
        &self,
        request: ActorComponentQueryRequest,
    ) -> Result<Option<ObjectId>, ObjectError> {
        let binding = canonical_binding(self.actor(request.actor)?);
        Ok(match (request.mode, binding) {
            (
                ActorComponentQueryMode::Interaction54,
                CanonicalBinding::Family40d800 | CanonicalBinding::Factory,
            )
            | (ActorComponentQueryMode::Self50, CanonicalBinding::Factory) => Some(request.actor),
            (ActorComponentQueryMode::Self50, _)
            | (ActorComponentQueryMode::Interaction54, CanonicalBinding::NullComponentNoOp)
            | (ActorComponentQueryMode::Interaction54, CanonicalBinding::Selector35) => None,
        })
    }

    /// Pure numeric 0048BE30. +18E precedes source admission, then the source
    /// classification precedes the component classification. Return the full
    /// DWORD 0/1 used by the caller as a divisor; this is not the damage +CC.
    pub fn query_actor_interaction_relation(
        &self,
        request: ActorInteractionRelationRequest,
    ) -> Result<u32, ActorInteractionError> {
        // The component's +CC with the source. The factory, the bridge and
        // the course target answer with 0040E090, their free staff.
        let state = match &self.actor(request.component)?.extension {
            ActorExtension::Selector30(state) => state,
            ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_) => {
                return Ok(self.free_staff(request.component, request.source)?);
            }
            _ => return Err(ActorInteractionError::NotSelector30(request.component)),
        };
        if state.byte_18e != 0 {
            return Ok(1);
        }
        let Some(source) = request.source else {
            return Ok(0);
        };
        // The port's Pelagia gate, while locked, answers a scientist of the
        // player's side ([`crate::pelagia`]).
        if state.pelagia && state.word_164 as i16 > 0 {
            return Ok(u32::from(self.actor(source)?.word_b4 != 0x12));
        }
        let source_class = self.actor(source)?.word_b4;
        let component_class = self.actor(request.component)?.word_b4;
        Ok(u32::from(source_class != component_class))
    }

    /// Canonical void +9C: 004026D0 is RET8 without a promised scalar result.
    /// Selector35's 0043F860 alone mutates +244, using the actual 50Hz clock.
    /// Neither admitted body consumes source or distance_squared.
    pub fn notify_actor_interaction(
        &mut self,
        request: ActorInteractionNotificationRequest,
        context: ActorInteractionNotificationContext<'_>,
    ) -> Result<(), ActorInteractionError> {
        match canonical_binding(self.actor(request.candidate)?) {
            CanonicalBinding::NullComponentNoOp
            | CanonicalBinding::Family40d800
            | CanonicalBinding::Factory => Ok(()),
            CanonicalBinding::Selector35 => {
                let Family428650Class::Selector35(state) =
                    &mut self.family428650_actor_mut(request.candidate)?.class
                else {
                    unreachable!("authenticated canonical selector35")
                };
                let byte = state.bytes_240[2].ok_or(ActorInteractionError::Uninitialized {
                    actor: request.candidate,
                    offset: 0x242,
                })?;
                if byte == 0 {
                    return Ok(());
                }
                let clock = context
                    .animation_clock
                    .ok_or(ActorInteractionError::MissingAnimationClock)?;
                state.word_244 = clock.animation_value().wrapping_add(200);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
