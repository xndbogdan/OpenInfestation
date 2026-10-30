//! Selector34 dying: virtual +120 entry 00434130 and +11C update 004341A0,
//! reached through the shared descriptor wrappers 0043FE40/00435240. Unlike
//! Selector35's 0043DEB0, the entry draws no RNG, and the update runs the
//! shared status visual 0047DFD0 instead of its own particles.

use thiserror::Error;

use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, ActorStateChangeRequest,
    DerivedActorError, FixedPositionTarget, NativeTarget, NativeTargetClass, NativeTargetError,
    ObjectArena, ObjectError, ObjectId, TargetNullHandleSource,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector34DyingRequest {
    /// 00434130 tests only the low byte of its argument.
    Change {
        entering: u32,
    },
    Update {
        microseconds: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector34DyingEffect {
    /// 0047DFD0 with the update's delta.
    StatusVisual { microseconds: u32 },
    /// 0047D670(0, process NULL, 0) once the descriptor has run 15 s.
    ChangeState(ActorStateChangeRequest),
}

pub trait Selector34DyingHost: TargetNullHandleSource {
    type Error;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector34DyingEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector34DyingError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("selector34 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector34 requires current process NULL at {address:08X}")]
    MissingProcessNull { address: u32 },
    #[error("selector34 dying service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}

/// The descriptor's elapsed time +188 above which the actor leaves state 1.
const DYING_MICROSECONDS: i32 = 15_000_000;
/// 004341E7: the dive point's Y WORD (-5000).
const DIVE_Y: u16 = 0xec78;

impl ObjectArena {
    pub fn run_selector34_dying<H: Selector34DyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        request: Selector34DyingRequest,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector34DyingError<H::Error>> {
        self.selector34_actor(actor)?;
        match request {
            Selector34DyingRequest::Change { entering } => {
                if entering as u8 == 0 {
                    return Ok(());
                }
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_0d3c;
                let target = match self
                    .allocate_actor_service(ActorServiceAllocationRequest {
                        kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
                    })
                    .map_err(NativeTargetError::from)?
                {
                    Some(reference) => {
                        // 00434153 reads 0054E894 only after the allocation.
                        let source = host.target_null_handle().ok_or(
                            Selector34DyingError::MissingProcessNull { address: 0x43_4153 },
                        )?;
                        let position = self.selector34_dying_position(actor)?;
                        self.construct_native_target(
                            reference,
                            NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                                source,
                                Some(position),
                            )),
                        )?;
                        Some(reference)
                    }
                    None => None,
                };
                // 00434181 overwrites +148 without destroying its old target.
                self.derived_actor_mut(actor)?.target_148 = target;
                let state = self.selector34_actor_mut(actor)?;
                state.byte_238 = 0;
                state.word_230 = 0;
                Ok(())
            }
            Selector34DyingRequest::Update { microseconds } => {
                let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(
                    Selector34DyingError::RequiredField {
                        actor,
                        offset: 0x188,
                        address: 0x43_41a6,
                    },
                )?;
                if elapsed as i32 > DYING_MICROSECONDS {
                    let source = host
                        .target_null_handle()
                        .ok_or(Selector34DyingError::MissingProcessNull { address: 0x43_41b2 })?;
                    return self.selector34_dying_effect(
                        actor,
                        Selector34DyingEffect::ChangeState(ActorStateChangeRequest {
                            target_state: 0,
                            argument: 0,
                            source,
                        }),
                        0x43_41bc,
                        host,
                        rng,
                    );
                }
                self.selector34_dying_effect(
                    actor,
                    Selector34DyingEffect::StatusVisual { microseconds },
                    0x43_41cf,
                    host,
                    rng,
                )?;
                // X/Z are read after the visual; a NULL +148 faults natively.
                let [x, _, z] = self.selector34_dying_position(actor)?;
                let reference = self.derived_actor(actor)?.target_148.ok_or(
                    Selector34DyingError::RequiredField {
                        actor,
                        offset: 0x148,
                        address: 0x43_41ee,
                    },
                )?;
                let mut target = self.native_target(reference)?;
                if target.kind_word() == 8 {
                    target.set_focus_08_words([x, DIVE_Y, z]);
                    self.write_native_target(reference, target)?;
                }
                Ok(())
            }
        }
    }

    fn selector34_dying_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector34DyingError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor).into())
    }

    fn selector34_dying_effect<H: Selector34DyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        effect: Selector34DyingEffect,
        address: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector34DyingError<H::Error>> {
        host.effect(self, actor, effect, rng)
            .map_err(|error| Selector34DyingError::Service { address, error })
    }
}
