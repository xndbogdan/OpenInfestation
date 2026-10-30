//! Native kind8/0051F154 copied-point resources and shared target queries.
//! The 0x14-byte allocation never writes padding +0E/+0F. Process velocity
//! and basis are required query inputs, not constructor defaults.

use inf_formats::native_basis::NativeBasis;
use thiserror::Error;

use super::{ActorHandle, NativeTarget, ObjectArena, ObjectError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedPositionTarget {
    pub handle_04: ActorHandle,
    /// Unknown copied words remain unknown; this does not mean a null target.
    pub focus_08: Option<[u16; 3]>,
    pub word_10: u32,
}

impl FixedPositionTarget {
    pub(crate) fn from_point(handle: ActorHandle, point: Option<[u16; 3]>) -> Self {
        Self {
            handle_04: handle,
            focus_08: point,
            word_10: 8,
        }
    }
}

/// Native kind4/00528060 (004827D0, 0x1C bytes): the aim's furniture target.
/// +08 is the furniture's position and +14 the sight's point, or the
/// position again for a NULL point pointer. Its virtuals are kind8's but for
/// its clone (00482890) and +1C (00482940), which selects +14 as kind2 does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurniturePositionTarget {
    pub handle_04: ActorHandle,
    pub focus_08: [u16; 3],
    pub word_10: u32,
    pub focus_14: [u16; 3],
}

impl FurniturePositionTarget {
    pub(crate) fn new(handle: ActorHandle, position: [u16; 3], point: Option<[u16; 3]>) -> Self {
        Self {
            handle_04: handle,
            focus_08: position,
            word_10: 4,
            focus_14: point.unwrap_or(position),
        }
    }
}

/// Process vector 0055F0F8, returned by a fixed/object target's virtual +08
/// (004026F0, its only reference). It lies in zero-filled .bss, and Infest01
/// never writes it: 0 writes and 16548 reads over the whole trace.
pub const PROCESS_VELOCITY_0055F0F8: [u16; 3] = [0; 3];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeTargetQueryError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("target velocity query requires process vector 0055F0F8")]
    MissingProcessVelocity,
    #[error("target basis query requires process basis 00547590")]
    MissingProcessBasis,
}

impl NativeTarget {
    /// Actual DWORD +10, independent from the allocation's vtable class.
    pub fn kind_word(self) -> u32 {
        match self {
            Self::ActorFollow(target) => target.word_10,
            Self::FurniturePosition(target) => target.word_10,
            Self::FixedPosition(target) => target.word_10,
            Self::ObjectPosition(target) => target.word_10,
        }
    }

    /// Virtual +1C: types2/4 select +14, while types8/16 select +08
    /// (00402710).
    pub fn point_words(self) -> Option<[u16; 3]> {
        match self {
            Self::ActorFollow(target) => target.focus_14,
            Self::FurniturePosition(target) => Some(target.focus_14),
            Self::FixedPosition(target) => target.focus_08,
            Self::ObjectPosition(target) => Some(target.focus_08),
        }
    }

    /// Direct six-byte +08 read, independent from virtual +1C. Attack's
    /// distance and helper displacement use the live focus cache even when
    /// a type2 target retains a distinct explicit point at +14.
    pub fn raw_focus_08_words(self) -> Option<[u16; 3]> {
        match self {
            Self::ActorFollow(target) => target.focus_08,
            Self::FurniturePosition(target) => Some(target.focus_08),
            Self::FixedPosition(target) => target.focus_08,
            Self::ObjectPosition(target) => Some(target.focus_08),
        }
    }

    /// Six-byte raw +08 destination shared by explicit kind-gated writers;
    /// callers retain their own gates and ordering before this copy.
    pub(crate) fn set_focus_08_words(&mut self, point: [u16; 3]) {
        match self {
            Self::ActorFollow(target) => target.focus_08 = Some(point),
            Self::FurniturePosition(target) => target.focus_08 = point,
            Self::FixedPosition(target) => target.focus_08 = Some(point),
            Self::ObjectPosition(target) => target.focus_08 = point,
        }
    }

    /// Virtual +08: type2 exposes retained +1A (004828C0), types8/16 expose the actual
    /// process vector. An unavailable process source never becomes zeroes.
    pub fn velocity_words(
        self,
        process_velocity: Option<[u16; 3]>,
    ) -> Result<Option<[u16; 3]>, NativeTargetQueryError> {
        match self {
            Self::ActorFollow(target) => Ok(target.velocity_1a),
            Self::FurniturePosition(_) | Self::FixedPosition(_) | Self::ObjectPosition(_) => {
                process_velocity
                    .map(Some)
                    .ok_or(NativeTargetQueryError::MissingProcessVelocity)
            }
        }
    }
}

impl ObjectArena {
    /// Shared inline kind8 construction, including clone 00402730. The point
    /// span is copied; neither actor nor object referents are resolved. Native
    /// allocation must succeed before a caller reaches this constructor.
    pub fn fixed_position_target(
        &self,
        point: Option<[u16; 3]>,
    ) -> Result<FixedPositionTarget, ObjectError> {
        let handle = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        Ok(FixedPositionTarget::from_point(handle, point))
    }

    /// Virtual +0C dispatch. A resolving type2 queries its live basis; a
    /// stale handle and types8/16 require the supplied process basis.
    pub fn native_target_basis(
        &mut self,
        target: &NativeTarget,
        process_basis: Option<NativeBasis>,
    ) -> Result<NativeBasis, NativeTargetQueryError> {
        if let NativeTarget::ActorFollow(target) = target {
            if let Some(actor) = self
                .resolve_actor_handle(target.handle_04)
                .map_err(|_| ObjectError::MissingActorRegistry)?
            {
                return Ok(self.actor(actor)?.rotating.basis);
            }
        }
        process_basis.ok_or(NativeTargetQueryError::MissingProcessBasis)
    }

    /// Virtual +18: types4/8/16 return4 without reading or changing fields.
    pub fn refresh_native_target(
        &mut self,
        target: &mut NativeTarget,
        settings_7c: u32,
    ) -> Result<u32, ObjectError> {
        match target {
            NativeTarget::ActorFollow(target) => self.refresh_follow_target(target, settings_7c),
            NativeTarget::FurniturePosition(_)
            | NativeTarget::FixedPosition(_)
            | NativeTarget::ObjectPosition(_) => Ok(4),
        }
    }
}

#[cfg(test)]
#[path = "fixed/native_receipts.rs"]
mod native_receipts;
#[cfg(test)]
#[path = "fixed/tests.rs"]
mod tests;
