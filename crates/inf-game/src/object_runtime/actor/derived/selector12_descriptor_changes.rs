//! Complete Selector12 descriptor changes through the actual retained target heap.
//! Wrappers469170/4691D0/4691F0/4692F0 preserve their distinct publication order.
//! The update callbacks have separate native owners.

use super::{FamilyBehaviorDescriptor, FamilyBehaviorProfile};
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, DerivedActorError,
    FixedPositionTarget, NativeTarget, NativeTargetClass, NativeTargetError, ObjectArena,
    ObjectError, ObjectId, WeightedTargetConstructorNull,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector12DescriptorChangeKind {
    ///469170 ->4672F0, captured source +26C and fresh29D spacing gate.
    Target26c,
    ///4691F0 ->4675E0, new kind8 target at the current actor position.
    PointAtCurrentPosition,
    ///4691D0 ->467340, captured source +274.
    Target274,
    ///4692F0 ->467760, unsigned scores280/288 select27C/284.
    PreferredTarget27c284,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector12DescriptorChangeRequest {
    pub kind: Selector12DescriptorChangeKind,
    pub entering: bool,
}
impl Selector12DescriptorChangeRequest {
    pub fn from_descriptor(
        profile: FamilyBehaviorProfile,
        descriptor: FamilyBehaviorDescriptor,
        entering: bool,
    ) -> Option<Self> {
        if !matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) {
            return None;
        }
        let kind = match descriptor.change_address {
            0x0046_9170 => Selector12DescriptorChangeKind::Target26c,
            0x0046_91f0 => Selector12DescriptorChangeKind::PointAtCurrentPosition,
            0x0046_91d0 => Selector12DescriptorChangeKind::Target274,
            0x0046_92f0 => Selector12DescriptorChangeKind::PreferredTarget27c284,
            _ => return None,
        };
        Some(Self { kind, entering })
    }
}

#[derive(Debug, Error)]
pub enum Selector12DescriptorChangeError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("selector12 {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
}

impl ObjectArena {
    /// These callbacks leave deletion to the dispatcher's previous +10C.
    /// Each read/store follows its reached native phase; no error rollback.
    pub fn change_selector12_descriptor_target(
        &mut self,
        actor: ObjectId,
        request: Selector12DescriptorChangeRequest,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), Selector12DescriptorChangeError> {
        match request.kind {
            Selector12DescriptorChangeKind::Target26c => {
                //4672F9: false bypasses even receiver custody.
                if !request.entering {
                    return Ok(());
                }
                let target = self.selector12_actor(actor)?.targets_26c_284[0];
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_a12c;
                let replacement = match target {
                    Some(reference) => self.clone_native_target(reference, constructor_null)?,
                    None => None,
                };
                self.derived_actor_mut(actor)?.target_148 = replacement;
                //46731E reads this byte only after normal pointer publication.
                if self.selector12_actor(actor)?.byte_29d == 0 {
                    self.family428650_actor_mut(actor)?.words_204_208[1] = Some(256);
                }
            }
            Selector12DescriptorChangeKind::PointAtCurrentPosition => {
                if !request.entering {
                    return Ok(());
                }
                self.selector12_actor(actor)?;
                //4675ED precedes the actual0x14 reservation, including failure.
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_a148;
                let reference = self
                    .allocate_actor_service(ActorServiceAllocationRequest {
                        kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
                    })
                    .map_err(NativeTargetError::from)?;
                let replacement = if let Some(reference) = reference {
                    //54E894 is sampled after successful allocation, never on NULL.
                    let handle = constructor_null.resolve(self)?;
                    let position = self.object(actor)?.base.position_words.ok_or(
                        Selector12DescriptorChangeError::RequiredField {
                            actor,
                            offset: 0x32,
                            address: 0x0046_7619,
                        },
                    )?;
                    //The current canonical DTO has whole-XYZ backing. An unknown
                    //point leaves a Pending reservation, rather than inventing
                    //the native partially written04/vtable/XY prefix.
                    self.construct_native_target(
                        reference,
                        NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                            handle,
                            Some(position),
                        )),
                    )?;
                    Some(reference)
                } else {
                    None
                };
                self.derived_actor_mut(actor)?.target_148 = replacement;
                self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(0);
            }
            Selector12DescriptorChangeKind::Target274 => {
                //467345/49: LOWBYTE false bypasses even actor custody.
                if !request.entering {
                    return Ok(());
                }
                //46734B captures source before467351 publishes status.
                let target = self.selector12_actor(actor)?.targets_26c_284[1];
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_a138;
                if let Some(reference) = target {
                    let cloned = self.clone_native_target(reference, constructor_null)?;
                    self.derived_actor_mut(actor)?.target_148 = cloned;
                    self.family428650_actor_mut(actor)?.words_204_208[1] = Some((-100_i32) as u32);
                } else {
                    //The NULL arm reverses the pointer/spacing store order.
                    self.family428650_actor_mut(actor)?.words_204_208[1] = Some((-100_i32) as u32);
                    self.derived_actor_mut(actor)?.target_148 = None;
                }
            }
            Selector12DescriptorChangeKind::PreferredTarget27c284 => {
                if !request.entering {
                    //467826: false still clears this actual class byte.
                    self.selector12_actor_mut(actor)?.byte_29c = 0;
                    return Ok(());
                }
                //467770/76: unsigned scores are captured before status.
                let left = self.selector12_actor(actor)?.word_280;
                let right = self.selector12_actor(actor)?.word_288;
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_a154;
                self.selector12_actor_mut(actor)?.byte_29c = 1;
                let target =
                    self.selector12_actor(actor)?.targets_26c_284[if left > right { 2 } else { 3 }];
                let Some(reference) = target else {
                    self.family428650_actor_mut(actor)?.word_22e |= 1;
                    self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(1);
                    return Ok(());
                };
                //4677AC resolves the original target's +10 before clone+04.
                //The captured actor identity survives that allocation phase.
                let target = self.native_target(reference)?;
                let referent = self.resolve_target_actor(&target)?;
                let cloned = self.clone_native_target(reference, constructor_null)?;
                self.derived_actor_mut(actor)?.target_148 = cloned;
                self.family428650_actor_mut(actor)?.words_204_208[1] = Some(128);
                self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(0);
                self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(4_000_000);
                self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(0);
                //4677EC is reached only after the successful clone/store suffix.
                let threshold = if let Some(referent) = referent {
                    u32::from(self.object(referent)?.base.word_28) + 288
                } else {
                    288
                };
                self.family428650_actor_mut(actor)?.words_188_19c[4] = Some(threshold);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector12_descriptor_changes/native_guards.rs"]
mod native_guards;
#[cfg(test)]
#[path = "selector12_descriptor_changes/tests.rs"]
mod tests;
