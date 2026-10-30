//! Embedded grant owners: 004750C0/00475350/00475380. Published dispatch and
//! an explicitly borrowed constructing player share the complete setter.
//! Normalized formats, saves and staged pickup policies remain separate; this
//! module neither publishes players nor attaches an enclosing production caller.

mod resources;
mod set;

pub(crate) use set::{set_live_grant, GrantSetTarget};

/// 00475320's record WORD +0 for `id`: the H0 string of its name, as a
/// gate's missing-grant notice reads it (0048B66D).
pub(crate) fn grant_record_text(
    context: &LiveGrantContext<'_>,
    id: u16,
) -> Result<u16, LiveGrantError> {
    resources::read_text(context, id)
}

use inf_formats::{FrontendExecutable, FrontendExecutableError, GRANT_MASK_WORDS};
use thiserror::Error;

use super::h0_text::H0TextCaptureError;
#[cfg(test)]
use super::h0_text::{capture_h0_text, H0TextCaptureRequest};
#[cfg(test)]
use super::{PlayerMessage88, PlayerMessageOperation, PlayerMessageRequest};

use super::{
    DerivedActorError, ObjectArena, ObjectId, PlayerAttachError, PlayerId, PlayerMessageError,
};
use crate::{
    grant_text::{GrantTextData, GrantTextError},
    process_particles::factories::ParticleFactoryProcessState,
    ResidentRuntime,
};

/// The actual embedded six-DWORD owners, not detached masks or native pointers.
/// Bare PlayerId retains its existing arena-local lookup semantics; the real
/// message token selected from that owner is independently arena-qualified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantOwner {
    Selector12Actor24c(ObjectId),
    Player9c(PlayerId),
    /// The factory's mask `+198`, which docking merges into the player's
    /// and then overwrites with it.
    Selector15Factory198(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveGrantOperation {
    /// 004750C0 consumes LOWWORD but retains the original DWORD at its boundary.
    SetRaw { id: u32 },
    /// 00475350 queries source IDs0..174, completing each setter in order.
    Merge { source: GrantOwner },
    /// 00475380 performs six source-read then target-write pairs, including
    /// high bits. Same-owner copies still reach their actual storage.
    Copy { source: GrantOwner },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveGrantRequest {
    pub target: GrantOwner,
    pub operation: LiveGrantOperation,
}

/// Current owners borrowed through the complete operation. Missing fields fail
/// only at their reached read. No canonical lower callback mutates these owners
/// synchronously; reborrow between requests for process/residency/CRT changes.
/// Phase, process NULL, RNG, first-player selection and native +B8 retargeting
/// are not inputs to this fixed-normal binding domain.
#[derive(Clone, Copy, Debug)]
pub struct LiveGrantContext<'a> {
    pub process: Option<&'a ParticleFactoryProcessState>,
    pub executable: Option<FrontendExecutable<'a>>,
    pub resident: Option<&'a ResidentRuntime>,
    pub text_data: Option<&'a GrantTextData<'a>>,
}

#[derive(Debug, Error)]
pub enum LiveGrantError {
    #[error(transparent)]
    Object(#[from] super::ObjectError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("grant owner {owner:?} has no represented mask DWORD {word_index}")]
    MaskBoundary {
        owner: GrantOwner,
        word_index: usize,
    },
    #[error("new grant on {owner:?} requires the current full 0055F448 network field")]
    MissingProcess { owner: GrantOwner },
    #[error("grant {id:#06x} requires executable bytes {address:#010x}+{bytes}")]
    MissingExecutable { id: u16, address: u32, bytes: usize },
    #[error("grant {id:#06x} executable read {address:#010x}+{bytes}: {source}")]
    Executable {
        id: u16,
        address: u32,
        bytes: usize,
        #[source]
        source: FrontendExecutableError,
    },
    #[error("grant {id:#06x} reaches unowned adjacent record bytes {address:#010x}+{bytes}")]
    AdjacentRecord { id: u16, address: u32, bytes: usize },
    #[error("selector12 {actor:?} has unowned raw grant notification binding {raw:#010x}")]
    NotificationBinding { actor: ObjectId, raw: u32 },
    #[error("grant string {index:#06x} requires the current H0 pool")]
    MissingResidency { index: u16 },
    #[error("grant string {index:#06x} reaches the one-past H0 storage at count {count}")]
    H0OnePast { index: u16, count: usize },
    #[error("grant string {index:#06x} has unavailable resident registration/record backing")]
    StringRegistration { index: u16 },
    #[error(transparent)]
    Text(#[from] GrantTextError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
}

impl From<H0TextCaptureError> for LiveGrantError {
    fn from(error: H0TextCaptureError) -> Self {
        match error {
            H0TextCaptureError::MissingResidency { index } => Self::MissingResidency { index },
            H0TextCaptureError::OnePast { index, count } => Self::H0OnePast { index, count },
            H0TextCaptureError::InvalidRegistration { index } => Self::StringRegistration { index },
        }
    }
}

impl ObjectArena {
    /// Execute an explicit live grant body. All three original bodies return
    /// void; this adds no changed-bit result or event22 broadcast. Earlier mask
    /// writes survive reached lower errors, with no transactional rollback.
    ///
    /// Complete six-word backing, one captured arena and fixed-normal Player9c
    /// message identity are admitted. Partial native masks, raw aliases,
    /// replaced H0 tables, arbitrary +B8 binding changes and arena/ID ABA remain
    /// outside this policy. Selector12's actual raw NULL binding is supported;
    /// its nonzero unrepresented binding stops only when reached offline.
    pub fn mutate_live_grants(
        &mut self,
        request: LiveGrantRequest,
        context: &LiveGrantContext<'_>,
    ) -> Result<(), LiveGrantError> {
        match request.operation {
            LiveGrantOperation::SetRaw { id } => set_live_grant(
                &mut GrantSetTarget::Published {
                    arena: self,
                    owner: request.target,
                },
                id,
                context,
            ),
            LiveGrantOperation::Merge { source } => {
                for id in 0..0xaf {
                    if self.live_grant_has(source, id)? {
                        set_live_grant(
                            &mut GrantSetTarget::Published {
                                arena: self,
                                owner: request.target,
                            },
                            u32::from(id),
                            context,
                        )?;
                    }
                }
                Ok(())
            }
            LiveGrantOperation::Copy { source } => {
                for word_index in 0..GRANT_MASK_WORDS {
                    // 0047538F source precedes 00475392 target, even equal.
                    let value = self.live_grant_word(source, word_index)?;
                    self.store_live_grant_word(request.target, word_index, value)?;
                }
                Ok(())
            }
        }
    }

    fn live_grant_word(&self, owner: GrantOwner, word_index: usize) -> Result<u32, LiveGrantError> {
        // Authenticate the actual reached owner before its storage bound.
        let mask = match owner {
            GrantOwner::Selector12Actor24c(actor) => {
                &self.selector12_actor(actor)?.grants_24c.mask_04
            }
            GrantOwner::Player9c(player) => &self.world_player(player)?.grants_9c,
            GrantOwner::Selector15Factory198(factory) => {
                &self.selector15(factory)?.grants_198.mask_04
            }
        };
        mask.get(word_index)
            .copied()
            .ok_or(LiveGrantError::MaskBoundary { owner, word_index })
    }

    fn store_live_grant_word(
        &mut self,
        owner: GrantOwner,
        word_index: usize,
        value: u32,
    ) -> Result<(), LiveGrantError> {
        let mask = match owner {
            GrantOwner::Selector12Actor24c(actor) => {
                &mut self.selector12_actor_mut(actor)?.grants_24c.mask_04
            }
            GrantOwner::Player9c(player) => &mut self.world_player_mut(player)?.grants_9c,
            GrantOwner::Selector15Factory198(factory) => {
                &mut self.selector15_mut(factory)?.grants_198.mask_04
            }
        };
        *mask
            .get_mut(word_index)
            .ok_or(LiveGrantError::MaskBoundary { owner, word_index })? = value;
        Ok(())
    }

    /// The raw query's LOWWORD0 return precedes even target authentication.
    /// Other IDs read one actual DWORD; adjacent storage is not clamped.
    pub(in crate::object_runtime) fn live_grant_has(
        &self,
        owner: GrantOwner,
        id: u16,
    ) -> Result<bool, LiveGrantError> {
        if id == 0 {
            return Ok(true);
        }
        Ok(self.live_grant_word(owner, usize::from(id >> 5))? & (1_u32 << (id & 31)) != 0)
    }
}

#[cfg(test)]
mod native_evidence;
#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
