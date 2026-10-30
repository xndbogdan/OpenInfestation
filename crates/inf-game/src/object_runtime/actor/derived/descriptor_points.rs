//! Descriptor copied-point changes 00427990/00427A00/00427B40 and StayStill
//! update 00427BB0. Roam update and enclosing actor +24 remain independent.

use thiserror::Error;

use super::FamilyBehaviorDescriptor;
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, NativeTarget, NativeTargetClass,
    NativeTargetError, ObjectArena, ObjectError, ObjectId, WeightedTargetConstructorNull,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyPointTargetChangeKind {
    Roam,
    Home,
    StayStill,
}

impl FamilyPointTargetChangeKind {
    pub const fn callback_address(self) -> u32 {
        match self {
            Self::Roam => 0x0043_fba0,
            Self::Home => 0x0043_5190,
            Self::StayStill => 0x0042_5e00,
        }
    }

    pub const fn owner_address(self) -> u32 {
        match self {
            Self::Roam => 0x0042_7990,
            Self::Home => 0x0042_7a00,
            Self::StayStill => 0x0042_7b40,
        }
    }

    const fn status_address(self) -> u32 {
        match self {
            Self::Roam => 0x0053_11c8,
            Self::Home => 0x0053_11d0,
            Self::StayStill => 0x0053_11e4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyPointTargetChangeRequest {
    pub kind: FamilyPointTargetChangeKind,
    pub entering: bool,
}

impl FamilyPointTargetChangeRequest {
    pub fn from_descriptor(descriptor: FamilyBehaviorDescriptor, entering: bool) -> Option<Self> {
        let kind = match descriptor.change_address {
            0x0043_fba0 => FamilyPointTargetChangeKind::Roam,
            0x0043_5190 => FamilyPointTargetChangeKind::Home,
            0x0042_5e00 => FamilyPointTargetChangeKind::StayStill,
            _ => return None,
        };
        Some(Self { kind, entering })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyPointTargetChangeError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {0:?} requires its unwritten home position +222")]
    UninitializedHome(ObjectId),
}

impl ObjectArena {
    /// Low-byte entering false bypasses all actor reads. True writes status
    /// and Roam's timer before allocation; actual null allocation publishes
    /// null +148 before any process-handle/position read. These entries do not
    /// delete the previous +148: the dispatcher's prior +10C owns that clear.
    pub fn change_family_point_target(
        &mut self,
        actor: ObjectId,
        request: FamilyPointTargetChangeRequest,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), FamilyPointTargetChangeError> {
        if !request.entering {
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.word_228 = request.kind.status_address();
        if request.kind == FamilyPointTargetChangeKind::Roam {
            self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(0);
        }
        let target = if let Some(reference) = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(NativeTargetError::from)?
        {
            //0054E894 is read after allocation, before the actual point span.
            let handle = constructor_null.resolve(self)?;
            let point = match request.kind {
                FamilyPointTargetChangeKind::Home => {
                    self.family428650_actor(actor)?
                        .position_222
                        .ok_or(FamilyPointTargetChangeError::UninitializedHome(actor))?
                }
                FamilyPointTargetChangeKind::Roam | FamilyPointTargetChangeKind::StayStill => self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?,
            };
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(
                    crate::object_runtime::FixedPositionTarget::from_point(handle, Some(point)),
                ),
            )?;
            Some(reference)
        } else {
            None
        };
        self.derived_actor_mut(actor)?.target_148 = target;
        Ok(())
    }

    ///00427BB0 ignores delta. Target presence and actual kind+10 gate the
    /// reached XYZ read; no allocation, timer, dirty flag or RNG is changed.
    pub fn update_family_stay_still_target(&mut self, actor: ObjectId) -> Result<(), ObjectError> {
        let Some(reference) = self.derived_actor(actor)?.target_148 else {
            return Ok(());
        };
        let mut target = self.native_target(reference)?;
        if target.kind_word() != 8 {
            return Ok(());
        }
        let point = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        target.set_focus_08_words(point);
        self.write_native_target(reference, target)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "descriptor_points/tests.rs"]
mod tests;
