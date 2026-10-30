//! The 0040D800 intermediate actor, shared by selectors 15, 23, 30, 37, 39,
//! 47 and 49. This owns its constructor prefix, the 0040D950 payload copy,
//! 0040D9C0 commands with the 0040D8D0 refill, 0040E000 postinitialization,
//! the 0040DFC0 after-load, the 0040DE70 link messages 7 and 8 and the
//! 0040DE20 death.

use inf_formats::{native_basis::NativeBasisMath, Family40d950Payload, InstanceCommandPayload};
use thiserror::Error;

use super::{ActorExtension, ActorInitializationRequest, ActorStateChangeRequest};
use crate::object_runtime::{world_load::world_completion_bit, ObjectArena, ObjectError, ObjectId};
use crate::ResidentRuntime;

/// 0040D800's written prefix. Fresh supported paths keep the owned sound
/// pointer +154 null; no public API fabricates a later sound resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor40d800State {
    pub byte_138: u8,
    /// Maximum stored count.
    pub word_13c: i32,
    /// Current stored count, kept in 0..=+13C by the signed tests below.
    pub word_140: i32,
    pub word_144: u16,
    /// Spawn offset +146 (DWORD) and +14A (WORD).
    pub words_146_14a: Option<[u16; 3]>,
    /// 0040DE70's link count: message 7 adds one, message 8 removes one.
    pub byte_14c: u8,
    /// Set by the first link message 7.
    pub byte_14d: u8,
    pub byte_14e: u8,
    pub word_150: u32,
    pub word_158: Option<u32>,
    /// The persistent sound record, null at construction; 0048B3F0 creates
    /// and releases it and the destructor 0040D870 releases a live one.
    pub sound_154: Option<crate::process_sound::ProcessSoundRef>,
}

impl Actor40d800State {
    pub(super) fn new() -> Self {
        Self {
            byte_138: 1,
            word_13c: 0,
            word_140: 0,
            word_144: u16::MAX,
            words_146_14a: None,
            byte_14c: 0,
            byte_14d: 0,
            byte_14e: 0,
            word_150: 0,
            word_158: None,
            sound_154: None,
        }
    }

    /// The signed lower test runs first; a negative maximum can still be
    /// assigned by the later upper test. Do not replace this with clamp().
    pub(super) fn clamp_current(&mut self) {
        if self.word_140 < 1 {
            self.word_140 = 0;
        } else if self.word_140 > self.word_13c {
            self.word_140 = self.word_13c;
        }
    }

    /// 0040D8D0: a nonzero delta sets dirty bit 0x0200 (`+39 |= 2`) before
    /// the signed clamp. Returns whether that bit is set.
    pub(super) fn refill(&mut self, delta: i32) -> bool {
        self.word_140 = self.word_140.wrapping_add(delta);
        self.clamp_current();
        delta != 0
    }

    /// 0040D9C0 codes 16..20. `None` means the code is not this method's.
    /// `Some(dirty_0200)` reports the refill's dirty bit.
    pub(super) fn apply_command(
        &mut self,
        code: i32,
        payload: &InstanceCommandPayload,
    ) -> Option<Result<bool, i32>> {
        use InstanceCommandPayload::*;
        if !(16..=20).contains(&code) {
            return None;
        }
        Some(match (code, payload) {
            (16, Actor40d800Maximum(value)) => {
                self.word_13c = i32::from(value.value);
                self.clamp_current();
                Ok(false)
            }
            (17, Actor40d800Flag14e(value)) => {
                self.byte_14e = u8::from(value.value != 0);
                Ok(false)
            }
            (18, Actor40d800Refill(value)) => Ok(value.value != 0 && self.refill(self.word_13c)),
            (19, Actor40d800Word144(value)) => {
                self.word_144 = value.value;
                Ok(false)
            }
            (20, Actor40d800Words146(value)) => {
                self.words_146_14a = Some(value.value);
                Ok(false)
            }
            _ => Err(code),
        })
    }
}

/// The services the family's death `0040DE20` reaches.
pub trait FamilyDeathHost {
    type Error;
    /// `0048EAD0(index, 0)`: the mission's record of the placement's death.
    fn mark_death(&mut self, arena: &mut ObjectArena, index: i8) -> Result<(), Self::Error>;
    /// `0047E190(argument)`: the ground under the actor scorched.
    fn scorch(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        argument: i32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum FamilyDeathError<E: std::fmt::Debug> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("0040DE20 stopped at {address:#010x}: {error:?}")]
    Host { address: u32, error: E },
}

/// `0040DE20`'s fuse `+150`: 1,000 microseconds with `+14E` set, else 2 s.
const DEATH_FUSE: u32 = 2_000_000;
const DEATH_FUSE_14E: u32 = 1_000;

/// 0040DE70's lifecycle consequence, executed by the caller through 0047D670.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyLinkChange {
    None,
    /// Message 7 on an actor in state 2: `0047D670(0, +84, 3)`.
    Lock(ActorStateChangeRequest),
    /// The last message 8 on a linked actor in state 3: `0047D670(0, +84, 2)`.
    Unlock(ActorStateChangeRequest),
    /// The barrier's last message 8 (`00411CE0`): a nonzero `+7A` to the
    /// first player's grants, then `00479FA0` on the barrier.
    Release {
        grant: u16,
    },
}

impl ObjectArena {
    /// `0040E090`, the `+CC` of the factory, the bridge and the course
    /// target: the free staff (`+13C` less `+140`), but none for a source
    /// of another team when the actor has a team.
    pub fn free_staff(
        &self,
        actor: ObjectId,
        source: Option<ObjectId>,
    ) -> Result<u32, ObjectError> {
        let team = self.actor(actor)?.word_b4;
        if team != 0 {
            if let Some(source) = source {
                if self.actor(source)?.word_b4 != team {
                    return Ok(0);
                }
            }
        }
        let staff = self.family40d800(actor)?;
        Ok((staff.word_13c as u32).wrapping_sub(staff.word_140 as u32))
    }

    pub fn family40d800(&self, object: ObjectId) -> Result<&Actor40d800State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector15(state) => Ok(&state.intermediate),
            ActorExtension::Selector30(state) => Ok(&state.intermediate),
            ActorExtension::Selector37(state) => Ok(&state.intermediate),
            ActorExtension::Selector39(state) => Ok(&state.intermediate),
            ActorExtension::Selector49(state) => Ok(&state.intermediate),
            ActorExtension::Selector47(state) => Ok(&state.intermediate),
            _ => Err(ObjectError::NotFamily40d800(object)),
        }
    }

    pub(in crate::object_runtime) fn family40d800_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Actor40d800State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector15(state) => Ok(&mut state.intermediate),
            ActorExtension::Selector30(state) => Ok(&mut state.intermediate),
            ActorExtension::Selector37(state) => Ok(&mut state.intermediate),
            ActorExtension::Selector39(state) => Ok(&mut state.intermediate),
            ActorExtension::Selector49(state) => Ok(&mut state.intermediate),
            ActorExtension::Selector47(state) => Ok(&mut state.intermediate),
            _ => Err(ObjectError::NotFamily40d800(object)),
        }
    }

    /// 0040D920: +08 (0047A610) context, then 0040D950 over the payload,
    /// which first runs the shared 0047AC10.
    pub(crate) fn initialize_family40d800_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.family40d800(object)?;
        let payload = request
            .payload
            .family40d950
            .clone()
            .ok_or(ObjectError::MissingFamily40d950Payload(object))?;
        self.initialize_actor(object, request, runtime, math)?;
        self.apply_family40d950_payload(object, &payload)
    }

    /// 0040D950 after 0047AC10: maximum +13C with the signed clamp, then a
    /// nonzero maximum with no stored count clears +B4 and ORs dirty 2.
    /// +146/+14A copy the payload's spawn offset.
    pub(super) fn apply_family40d950_payload(
        &mut self,
        object: ObjectId,
        payload: &Family40d950Payload,
    ) -> Result<(), ObjectError> {
        let state = self.family40d800_mut(object)?;
        state.word_13c = payload.signed_dword_44.value;
        state.clamp_current();
        let empty = payload.signed_dword_44.value != 0 && state.word_140 == 0;
        state.words_146_14a = Some(payload.words_48_4c.value);
        if empty {
            self.owned_mut(object)?.state.base.dirty_word |= 2;
            self.actor_mut(object)?.word_b4 = 0;
        }
        Ok(())
    }

    /// 0040E000, the family +1C: the same empty-store test as 0040D950, then
    /// the shared 0047AAD0.
    pub(crate) fn finish_family40d800_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let state = self.family40d800(object)?;
        if state.word_13c != 0 && state.word_140 == 0 {
            self.owned_mut(object)?.state.base.dirty_word |= 2;
            self.actor_mut(object)?.word_b4 = 0;
        }
        self.finish_actor_initialization(object)
    }

    /// 0040DFC0, the family +34. A completed placement (0048EA80 over its
    /// signed +78) fills the store through 0040D8D0, ORs dirty 2 and joins
    /// the player's side (+B4 = 2).
    pub(crate) fn family40d800_after_load(
        &mut self,
        object: ObjectId,
        world_completion: &[u32; 4],
    ) -> Result<(), ObjectError> {
        let index = self.actor(object)?.byte_78 as i8;
        if !world_completion_bit(world_completion, index) {
            return Ok(());
        }
        let state = self.family40d800_mut(object)?;
        let dirty_0200 = state.refill(state.word_13c);
        let base = &mut self.owned_mut(object)?.state.base;
        if dirty_0200 {
            base.dirty_word |= 0x0200;
        }
        base.dirty_word |= 2;
        self.actor_mut(object)?.word_b4 = 2;
        Ok(())
    }

    /// 0040DE70, the family +98 for link messages. Message 7 counts a link
    /// and sets +14D; on an actor in state 2 with a positive count it locks
    /// the actor (state 3). Message 8 uncounts; the last one on a linked
    /// actor in state 3 unlocks it (state 2). Other codes return zero.
    pub(super) fn family40d800_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        let lifecycle = self.actor(object)?.word_c4 as i32;
        let source = self.actor(object)?.handle;
        let state = self.family40d800_mut(object)?;
        let request = |target_state| ActorStateChangeRequest {
            target_state,
            argument: 0,
            source,
        };
        match code {
            7 => {
                state.byte_14c = state.byte_14c.wrapping_add(1);
                state.byte_14d = 1;
                if lifecycle == 2 && (state.byte_14c as i8) > 0 {
                    return Ok(FamilyLinkChange::Lock(request(3)));
                }
            }
            8 => {
                state.byte_14c = state.byte_14c.wrapping_sub(1);
                if (state.byte_14c as i8) < 1 && state.byte_14d != 0 && lifecycle == 3 {
                    return Ok(FamilyLinkChange::Unlock(request(2)));
                }
            }
            _ => {}
        }
        Ok(FamilyLinkChange::None)
    }

    /// `0040DE20`, the `+AC` of the family's base table and so of the
    /// factory (selector 15) and the terrain bridge (selector 37): the
    /// placement's death (`+78`), the ground under it scorched
    /// (`0047E190(5)`), then the fuse `+150`, which neither class's own
    /// update reads.
    pub fn retire_family40d800<H: FamilyDeathHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), FamilyDeathError<H::Error>>
    where
        H::Error: std::fmt::Debug,
    {
        let index = self.actor(actor)?.byte_78 as i8;
        host.mark_death(self, index)
            .map_err(|error| FamilyDeathError::Host {
                address: 0x0040_de36,
                error,
            })?;
        host.scorch(self, actor, 5)
            .map_err(|error| FamilyDeathError::Host {
                address: 0x0040_de3f,
                error,
            })?;
        let state = self.family40d800_mut(actor)?;
        state.word_150 = if state.byte_14e != 0 {
            DEATH_FUSE_14E
        } else {
            DEATH_FUSE
        };
        Ok(())
    }
}
