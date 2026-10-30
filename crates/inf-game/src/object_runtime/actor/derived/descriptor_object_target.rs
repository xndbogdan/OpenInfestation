//! Descriptor 0043FA80 -> actual family virtual+E8,004288E0.
//! It follows a common-object target, separately from copied-point changes and
//! the RNG/weapon-dependent attack virtual+F8. Target identities stay retained.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{Family428650Class, Family429e20Class};
use crate::object_runtime::{
    NativeTargetError, ObjectArena, ObjectError, ObjectId, WeightedTargetConstructorNull,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyObjectTargetChangeRequest {
    pub entering: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyObjectTargetChangeError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {0:?} requires a different actual virtual+E8")]
    RequiredActorVirtual(ObjectId),
    #[error("actor {0:?} has null +1FC at the reached clone dereference")]
    MissingSource(ObjectId),
}

impl ObjectArena {
    /// LOWBYTE entering false bypasses the receiver. The admitted final classes
    /// share4288E0; Selector35's43F700 remains a separate required owner.
    /// Normal NULL clone still publishes148 and completes14C/208. Neither this
    /// method nor its clone releases the old148: the preceding dispatcher+10C
    /// owns that phase. Whole XYZ Option custody cannot represent native faults
    /// between individual Y/X/Z reads; no distance store precedes those reads.
    pub fn change_family_object_target(
        &mut self,
        actor: ObjectId,
        request: FamilyObjectTargetChangeRequest,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), FamilyObjectTargetChangeError> {
        if !request.entering {
            return Ok(());
        }
        let family = self.family428650_actor(actor)?;
        let owned = matches!(
            &family.class,
            Family428650Class::Family429e20(state)
                if matches!(
                    &state.class,
                    Family429e20Class::Family4236d0(_) | Family429e20Class::Selector12(_)
                )
        );
        if !owned {
            return Err(FamilyObjectTargetChangeError::RequiredActorVirtual(actor));
        }
        //4288F0 captures the raw pointer before4288F6 publishes status. The
        //nonnull branch dereferences that captured allocation only afterward.
        let source_reference = family.targets_1d4_1fc[5];
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_11f0;
        let Some(reference) = source_reference else {
            return Ok(());
        };
        //Actual target virtual+14, not the actor resolver at virtual+10.
        let Some(source) = self.native_target(reference)?.object_referent() else {
            return Ok(());
        };
        //Native42890F rereads1FC after+14. Preserve that custody seam rather
        //than silently using the earlier reference for the clone operation.
        let reference = self.family428650_actor(actor)?.targets_1d4_1fc[5]
            .ok_or(FamilyObjectTargetChangeError::MissingSource(actor))?;
        let clone = self.clone_native_target(reference, constructor_null)?;
        self.derived_actor_mut(actor)?.target_148 = clone;

        let source_words = self
            .object(source)?
            .base
            .position_words
            .ok_or(ObjectError::TargetObjectPositionUnknown(source))?;
        let actor_words = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let [dx, dy, dz] = std::array::from_fn::<_, 3, _>(|axis| {
            i32::from(source_words[axis].wrapping_sub(actor_words[axis]) as i16)
        });
        let squared = (dx.wrapping_mul(dx) as u32)
            .wrapping_add(dy.wrapping_mul(dy) as u32)
            .wrapping_add(dz.wrapping_mul(dz) as u32);
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        self.derived_actor_mut(actor)?.word_14c = Some(distance);
        let size = u32::from(self.object(actor)?.base.word_28);
        self.family428650_actor_mut(actor)?.words_204_208[1] =
            Some(size.wrapping_neg().wrapping_shl(2));
        Ok(())
    }
}

/// Actual common actor virtual+EC 00428830; raw wrapper delta is unused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyObjectTargetUpdateError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires a different actual virtual+EC")]
    RequiredActorVirtual(ObjectId),
    #[error("actor {0:?} has null +148 at the reached target virtual+14 dereference")]
    MissingCurrentTarget(ObjectId),
    #[error("actor {0:?} requires current DWORD +14C for object-target update")]
    UninitializedDistance(ObjectId),
}

impl ObjectArena {
    ///00428830 retains the same target allocation and only updates its +14/+08.
    ///NULL target148 is a required boundary; a valid target returning NULL from
    ///virtual14 instead sets only low BYTE22E mask0x04. Selector35 has a distinct EC.
    ///The signed14C near branch never reads route44 or actor XYZ. The far branch
    ///uses WORD-wrapped deltas, DWORD-wrapped squares and an unsigned comparison;
    ///it neither takes a square root nor repairs bit31. No allocation/RNG occurs.
    ///Whole XYZ Option custody cannot express native faults between individual
    ///axes. A changed target identity is committed even when its later XYZ read
    ///fails; unknown whole XYZ never becomes an invented position.
    pub fn update_family_object_target(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), FamilyObjectTargetUpdateError> {
        let family = self.family428650_actor(actor)?;
        let owned = matches!(
            &family.class,
            Family428650Class::Family429e20(state)
                if matches!(
                    &state.class,
                    Family429e20Class::Family4236d0(_) | Family429e20Class::Selector12(_)
                )
        );
        if !owned {
            return Err(FamilyObjectTargetUpdateError::RequiredActorVirtual(actor));
        }
        let captured = self
            .derived_actor(actor)?
            .target_148
            .ok_or(FamilyObjectTargetUpdateError::MissingCurrentTarget(actor))?;
        //Native428834 dereferences148 before the virtual14 result NULL check.
        let Some(source) = self.native_target(captured)?.object_referent() else {
            self.family428650_actor_mut(actor)?.word_22e |= 4;
            return Ok(());
        };
        let distance = self
            .derived_actor(actor)?
            .word_14c
            .ok_or(FamilyObjectTargetUpdateError::UninitializedDistance(actor))?
            as i32;
        let next = if distance < 256 {
            let Some(next) = self.route_next(source)? else {
                self.family428650_actor_mut(actor)?.word_22e |= 4;
                return Ok(());
            };
            next
        } else {
            //This required RouteNode view preserves the existing typed boundary
            //for other common-object44/50 layouts rather than treating them as routes.
            if self.route_node(source)?.distance_squared == 0 {
                return Ok(());
            }
            let Some(next) = self.route_next(source)? else {
                return Ok(());
            };
            let next_words = self
                .object(next)?
                .base
                .position_words
                .ok_or(ObjectError::TargetObjectPositionUnknown(next))?;
            let actor_words = self
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
            let [dx, dy, dz] = std::array::from_fn::<_, 3, _>(|axis| {
                i32::from(next_words[axis].wrapping_sub(actor_words[axis]) as i16)
            });
            let squared = (dx.wrapping_mul(dx) as u32)
                .wrapping_add(dy.wrapping_mul(dy) as u32)
                .wrapping_add(dz.wrapping_mul(dz) as u32);
            //4288BA rereads the source's current squared-segment DWORD.
            if squared >= self.route_node(source)?.distance_squared {
                return Ok(());
            }
            next
        };
        //The native suffix rereads148 after route selection. Virtual24 stores
        //changed referent identity before XYZ; commit its reached prefix on Err.
        let reference = self
            .derived_actor(actor)?
            .target_148
            .ok_or(FamilyObjectTargetUpdateError::MissingCurrentTarget(actor))?;
        let mut target = self.object_position_target_data(reference)?;
        let replacement = self.replace_object_position_target(&mut target, next);
        self.write_native_target(
            reference,
            crate::object_runtime::NativeTarget::ObjectPosition(target),
        )?;
        replacement?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "descriptor_object_target/native_guards.rs"]
mod native_guards;
#[cfg(test)]
#[path = "descriptor_object_target/tests.rs"]
mod tests;
