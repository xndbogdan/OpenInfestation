//! Exact descriptor score callbacks feeding motion selection. Scalar owners
//! read actual retained allocation bytes; dependent callbacks stop at their
//! reached typed boundary. Descriptor changes and enclosing +24 stay separate.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{DerivedActorError, Family428650Class, Family429e20Class, FamilyBehaviorDescriptor};
use crate::object_runtime::{
    ActorConstructionError, ActorHandle, ObjectArena, ObjectError, ObjectId, RuntimeAllocation,
};

mod fc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyDescriptorScoreKind {
    Max1e0And1f0,
    Score1e8,
    HalfRadius164Common,
    HalfRadius164Selector12,
    /// 00425DF0 ignores all three stack arguments and the actor receiver.
    ConstantOne,
    /// 00469370 ignores all three stack arguments and the actor receiver.
    ConstantZero,
    /// Distinct wrappers of actual virtual +F0 (00405730).
    Max1d8And1f8Common,
    Max1d8And1f8Selector12,
    /// 00469130 -> Selector12 virtual +128 (004688F0).
    Selector12Score270,
    /// Distinct wrappers of actual direct 00428320.
    HomeDistanceCommon,
    HomeDistanceSelector12,
    VirtualFc,
    Virtual12c,
    ///00469190 -> actual Selector12 virtual+134 (00461C20), DWORD+278.
    Virtual134Selector12,
    ///004692B0 -> actual Selector12 virtual+140 (00467840), lazy current carry.
    Virtual140Selector12,
    Source43e360,
    Source428990,
    /// 004351E0: Selector34's 0043FCE0 twin reading +23C WORD +0E.
    Source428990Selector34,
    Source43d730,
    Source43d8f0,
}

impl FamilyDescriptorScoreKind {
    pub const fn address(self) -> u32 {
        match self {
            Self::Max1e0And1f0 => 0x0043_fb20,
            Self::Score1e8 => 0x0043_51b0,
            Self::HalfRadius164Common => 0x0043_fb60,
            Self::HalfRadius164Selector12 => 0x0046_9230,
            Self::ConstantOne => 0x0042_5df0,
            Self::ConstantZero => 0x0046_9370,
            Self::Max1d8And1f8Common => 0x0043_fac0,
            Self::Max1d8And1f8Selector12 => 0x0046_9330,
            Self::Selector12Score270 => 0x0046_9130,
            Self::HomeDistanceCommon => 0x0042_5db0,
            Self::HomeDistanceSelector12 => 0x0046_9270,
            Self::VirtualFc => 0x0043_fa40,
            Self::Virtual12c => 0x0042_5d30,
            Self::Virtual134Selector12 => 0x0046_9190,
            Self::Virtual140Selector12 => 0x0046_92b0,
            Self::Source43e360 => 0x0043_fc60,
            Self::Source428990 => 0x0043_fce0,
            Self::Source428990Selector34 => 0x0043_51e0,
            Self::Source43d730 => 0x0043_fd40,
            Self::Source43d8f0 => 0x0043_fdc0,
        }
    }

    /// Constants have no retained read, rather than a sentinel WORD offset.
    pub const fn coefficient_offset(self) -> Option<u32> {
        Some(match self {
            Self::ConstantOne | Self::ConstantZero => return None,
            Self::Max1d8And1f8Common | Self::Virtual12c | Self::Virtual140Selector12 => 0,
            Self::Max1e0And1f0 => 2,
            Self::Selector12Score270 | Self::HomeDistanceCommon => 4,
            Self::Score1e8 => 6,
            Self::HalfRadius164Common | Self::Virtual134Selector12 => 8,
            Self::HomeDistanceSelector12 | Self::VirtualFc => 0x0a,
            Self::HalfRadius164Selector12 | Self::Source43e360 => 0x0c,
            Self::Max1d8And1f8Selector12 | Self::Source428990Selector34 => 0x0e,
            Self::Source428990 => 0x10,
            Self::Source43d730 | Self::Source43d8f0 => 0x12,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyDescriptorScoreRequest {
    pub kind: FamilyDescriptorScoreKind,
    /// Native dispatcher supplies whether this descriptor is currently selected.
    pub current: bool,
    /// Original third stack argument. Addition and multiplication wrap DWORDs.
    pub bias: i32,
}

impl FamilyDescriptorScoreRequest {
    /// Preserve the genuine current flag independently of the score bias.
    /// Coefficient zero still bypasses every source-specific current gate.
    pub fn from_descriptor(
        descriptor: FamilyBehaviorDescriptor,
        current: bool,
        bias: i32,
    ) -> Option<Self> {
        let kind = match descriptor.score_address {
            0x0043_fb20 => FamilyDescriptorScoreKind::Max1e0And1f0,
            0x0043_51b0 => FamilyDescriptorScoreKind::Score1e8,
            0x0043_fb60 => FamilyDescriptorScoreKind::HalfRadius164Common,
            0x0046_9230 => FamilyDescriptorScoreKind::HalfRadius164Selector12,
            0x0042_5df0 => FamilyDescriptorScoreKind::ConstantOne,
            0x0046_9370 => FamilyDescriptorScoreKind::ConstantZero,
            0x0043_fac0 => FamilyDescriptorScoreKind::Max1d8And1f8Common,
            0x0046_9330 => FamilyDescriptorScoreKind::Max1d8And1f8Selector12,
            0x0046_9130 => FamilyDescriptorScoreKind::Selector12Score270,
            0x0042_5db0 => FamilyDescriptorScoreKind::HomeDistanceCommon,
            0x0046_9270 => FamilyDescriptorScoreKind::HomeDistanceSelector12,
            0x0043_fa40 => FamilyDescriptorScoreKind::VirtualFc,
            0x0042_5d30 => FamilyDescriptorScoreKind::Virtual12c,
            0x0046_9190 => FamilyDescriptorScoreKind::Virtual134Selector12,
            0x0046_92b0 => FamilyDescriptorScoreKind::Virtual140Selector12,
            0x0043_fc60 => FamilyDescriptorScoreKind::Source43e360,
            0x0043_fce0 => FamilyDescriptorScoreKind::Source428990,
            0x0043_51e0 => FamilyDescriptorScoreKind::Source428990Selector34,
            0x0043_fd40 => FamilyDescriptorScoreKind::Source43d730,
            0x0043_fdc0 => FamilyDescriptorScoreKind::Source43d8f0,
            _ => return None,
        };
        Some(Self {
            kind,
            current,
            bias,
        })
    }
}

/// The one process RNG is needed only at common424F77 after byte260==0.
/// Pure score callers and strict fixtures explicitly keep this loan unavailable.
#[derive(Debug)]
pub enum FamilyDescriptorScoreRng<'a> {
    Available(&'a mut u32),
    RequiredAtReach,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyDescriptorScoreError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("actor {0:?} has no represented native +23C retained binding")]
    UnsupportedRetainedBinding(ObjectId),
    #[error("actor {actor:?} requires unwritten +{offset:03X} at this score callback")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {actor:?} requires the live RNG at {address:08X}")]
    RequiredRandom { actor: ObjectId, address: u32 },
    #[error("actor {actor:?} has NULL current target at {address:08X}")]
    MissingCurrentTarget { actor: ObjectId, address: u32 },
    #[error("object {object:?} requires native raw DWORD+44 at {address:08X}")]
    RequiredReferent44 { object: ObjectId, address: u32 },
    #[error("score {callback:08X} requires actor {actor:?} virtual +{offset:03X}")]
    RequiredActorVirtual {
        actor: ObjectId,
        callback: u32,
        offset: u16,
    },
    #[error("score {callback:08X} requires actor {actor:?} source {address:08X}")]
    RequiredNativeSource {
        actor: ObjectId,
        callback: u32,
        address: u32,
    },
    #[error("score {callback:08X} requires current process NULL 0054E894 at {address:08X}")]
    MissingProcessNull { callback: u32, address: u32 },
    #[error("score {callback:08X} requires the animation clock 005471C0 at {address:08X}")]
    MissingProcessClock { callback: u32, address: u32 },
    /// 0043D929 reads the first player's unit (0046B030) without a NULL test.
    #[error("actor {actor:?} score reaches a NULL player unit at {address:08X}")]
    MissingPlayerUnit { actor: ObjectId, address: u32 },
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
}

impl ObjectArena {
    /// Scores without a process NULL or clock; a reached `00428990`,
    /// `0043D730` or `0043D8F0` source stops.
    pub fn score_family_descriptor(
        &mut self,
        actor: ObjectId,
        request: FamilyDescriptorScoreRequest,
        rng: FamilyDescriptorScoreRng<'_>,
    ) -> Result<i32, FamilyDescriptorScoreError> {
        self.score_family_descriptor_in_process(actor, request, rng, None, None)
    }

    /// `process_null` is the live `0054E894` value that `00428990` reads
    /// before the escort handle; `clock` the live `005471C0` that
    /// Selector35's `0043D730` and `0043D8F0` compare with `+244`.
    pub fn score_family_descriptor_in_process(
        &mut self,
        actor: ObjectId,
        request: FamilyDescriptorScoreRequest,
        mut rng: FamilyDescriptorScoreRng<'_>,
        process_null: Option<ActorHandle>,
        clock: Option<u32>,
    ) -> Result<i32, FamilyDescriptorScoreError> {
        use FamilyDescriptorScoreKind as Kind;
        let offset = match request.kind.coefficient_offset() {
            Some(offset) => offset,
            None => return Ok(i32::from(request.kind == Kind::ConstantOne)),
        };
        // The zero test precedes source/current-flag/bias reads in each wrapper.
        let mut coefficient = self.descriptor_score_coefficient(actor, offset)?;
        if coefficient == 0 {
            return Ok(0);
        }
        let source = match request.kind {
            Kind::Max1e0And1f0 => {
                let words = &self.family428650_actor(actor)?.words_1d8_200;
                words[1].max(words[3]) as i32
            }
            Kind::Score1e8 => self.family428650_actor(actor)?.words_1d8_200[2] as i32,
            Kind::HalfRadius164Common | Kind::HalfRadius164Selector12 => {
                self.derived_actor(actor)?.word_164 / 2
            }
            Kind::Max1d8And1f8Common | Kind::Max1d8And1f8Selector12 => {
                // Actual final vtable +F0 is 00405730 for each represented
                // retained class. Its first DWORD is +1F8, then unsigned +1D8.
                let words = &self.family428650_actor(actor)?.words_1d8_200;
                words[4].max(words[0]) as i32
            }
            Kind::Selector12Score270 => {
                // Non-Selector12 callers have a different actual +128. Do not
                // reinterpret their adjacent fields as the Selector12 owner.
                let state = self.selector12_actor(actor).map_err(|_| {
                    FamilyDescriptorScoreError::RequiredActorVirtual {
                        actor,
                        callback: request.kind.address(),
                        offset: 0x128,
                    }
                })?;
                if self.actor(actor)?.word_b4 == 18 {
                    0
                } else if state.byte_29d != 0 {
                    state.word_270.wrapping_shl(2) as i32
                } else {
                    state.word_270 as i32
                }
            }
            Kind::HomeDistanceCommon | Kind::HomeDistanceSelector12 => {
                self.descriptor_home_distance(actor)?
            }
            Kind::Virtual134Selector12 => {
                // Final527058+134 is 00461C20: only DWORD+278, with no target,
                // weapon, position, current flag or passed argument read.
                let state = self.selector12_actor(actor).map_err(|_| {
                    FamilyDescriptorScoreError::RequiredActorVirtual {
                        actor,
                        callback: request.kind.address(),
                        offset: 0x134,
                    }
                })?;
                state.word_278 as i32
            }
            Kind::VirtualFc => self.descriptor_fc_source(actor, request.current, &mut rng)?,
            Kind::Virtual140Selector12 => {
                let state = self.selector12_actor(actor).map_err(|_| {
                    FamilyDescriptorScoreError::RequiredActorVirtual {
                        actor,
                        callback: request.kind.address(),
                        offset: 0x140,
                    }
                })?;
                //00467840 reads unsigned+280 then+288 before current's BYTE.
                let mut source = state.word_280.max(state.word_288);
                if request.current {
                    let family = self.family428650_actor(actor)?;
                    //Native rawNULL checks are1D4,1DC,1F4,1EC in that order.
                    //Some Released is still nonNULL; no heap dereference occurs.
                    if [0usize, 1, 4, 3]
                        .into_iter()
                        .all(|index| family.targets_1d4_1fc[index].is_none())
                    {
                        let carry = family.words_188_19c[2].ok_or(
                            FamilyDescriptorScoreError::Uninitialized {
                                actor,
                                offset: 0x190,
                            },
                        )?;
                        if carry != 0 {
                            source = source.wrapping_shl(2);
                        }
                    }
                }
                source as i32
            }
            Kind::Virtual12c => {
                // Only the 004236D0 family's mode-2 table holds 00425D30;
                // its four final tables share +12C = 00423B40.
                if !self.follows_player_as_family4236d0(actor)? {
                    return Err(FamilyDescriptorScoreError::RequiredActorVirtual {
                        actor,
                        callback: request.kind.address(),
                        offset: 0x12c,
                    });
                }
                self.follow_player_source(actor)?
            }
            Kind::Source428990 | Kind::Source428990Selector34 => {
                let null = process_null.ok_or(FamilyDescriptorScoreError::MissingProcessNull {
                    callback: request.kind.address(),
                    address: 0x0042_8990,
                })?;
                self.descriptor_escort_source(actor, null)?
            }
            // 0043E360, Selector35's: nothing while byte +26C is set, else
            // +164, halved (toward zero) unless the descriptor is current.
            Kind::Source43e360 if self.selector35_actor(actor).is_ok() => {
                if self.selector35_actor(actor)?.byte_26c != 0 {
                    0
                } else {
                    let radius = self.derived_actor(actor)?.word_164;
                    if request.current {
                        radius
                    } else {
                        radius / 2
                    }
                }
            }
            // 0043D730, Selector35's Rescue: nothing without a count +242,
            // nor while another descriptor runs (+184 not -1) unless this
            // one is current; then 0x7FFF until the clock passes +244 (the
            // last touch's 0043F860 deadline), and 1 after.
            Kind::Source43d730 if self.selector35_actor(actor).is_ok() => {
                let count = self.selector35_rescue_byte(actor, 2)?;
                if count == 0
                    || (!request.current && self.family428650_actor(actor)?.word_184 != -1)
                {
                    0
                } else {
                    let clock = clock.ok_or(FamilyDescriptorScoreError::MissingProcessClock {
                        callback: request.kind.address(),
                        address: 0x0043_d751,
                    })?;
                    if self.selector35_actor(actor)?.word_244 as i32 >= clock as i32 {
                        0x7fff
                    } else {
                        1
                    }
                }
            }
            // 0043D8F0, Selector35's Go to space: nothing while fewer than
            // +242 are aboard (+243), 0xFFFF once it has gone (0xFF); then
            // 0xFFFF only after the deadline +244 with the first player's
            // unit (0046B030) nearer than WORD +240.
            Kind::Source43d8f0 if self.selector35_actor(actor).is_ok() => {
                let count = self.selector35_rescue_byte(actor, 2)?;
                let aboard = self.selector35_rescue_byte(actor, 3)?;
                if count > aboard {
                    0
                } else if aboard == 0xff {
                    0xffff
                } else {
                    let unit = self.first_player_primary_actor()?;
                    let clock = clock.ok_or(FamilyDescriptorScoreError::MissingProcessClock {
                        callback: request.kind.address(),
                        address: 0x0043_d91f,
                    })?;
                    if self.selector35_actor(actor)?.word_244 as i32 >= clock as i32 {
                        0
                    } else {
                        let unit = unit.ok_or(FamilyDescriptorScoreError::MissingPlayerUnit {
                            actor,
                            address: 0x0043_d929,
                        })?;
                        let there = self
                            .object(unit)?
                            .base
                            .position_words
                            .ok_or(ObjectError::ActorPositionUnknown(unit))?;
                        let here = self
                            .object(actor)?
                            .base
                            .position_words
                            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                        let square = [0, 1, 2].into_iter().fold(0u32, |sum, axis| {
                            let delta = i32::from(there[axis].wrapping_sub(here[axis]) as i16);
                            sum.wrapping_add(delta.wrapping_mul(delta) as u32)
                        });
                        let distance = if square & 0x8000_0000 != 0 {
                            isqrt_48efc0((square >> 2) as i32).wrapping_shl(1)
                        } else {
                            isqrt_48efc0(square as i32)
                        };
                        let bytes = self.selector35_actor(actor)?.bytes_240;
                        let reach = match (bytes[0], bytes[1]) {
                            (Some(low), Some(high)) => i32::from(u16::from_le_bytes([low, high])),
                            _ => {
                                return Err(FamilyDescriptorScoreError::Uninitialized {
                                    actor,
                                    offset: 0x240,
                                })
                            }
                        };
                        if (distance as i32) < reach {
                            0xffff
                        } else {
                            0
                        }
                    }
                }
            }
            Kind::Source43e360 | Kind::Source43d730 | Kind::Source43d8f0 => {
                let address = match request.kind {
                    Kind::Source43e360 => 0x0043_e360,
                    Kind::Source43d730 => 0x0043_d730,
                    Kind::Source43d8f0 => 0x0043_d8f0,
                    _ => unreachable!(),
                };
                return Err(FamilyDescriptorScoreError::RequiredNativeSource {
                    actor,
                    callback: request.kind.address(),
                    address,
                });
            }
            Kind::ConstantOne | Kind::ConstantZero => unreachable!(),
        };
        if !matches!(
            request.kind,
            Kind::Max1e0And1f0
                | Kind::Score1e8
                | Kind::HalfRadius164Common
                | Kind::HalfRadius164Selector12
        ) {
            // These wrappers reacquire +23C and its unsigned WORD after the
            // source call, including common FC byte260/RNG writes; retain the real read.
            coefficient = self.descriptor_score_coefficient(actor, offset)?;
        }
        Ok(i32::from(coefficient)
            .wrapping_add(request.bias)
            .wrapping_mul(source))
    }

    /// Selector35's rescue bytes `+240..+243`, by index.
    fn selector35_rescue_byte(
        &self,
        actor: ObjectId,
        index: usize,
    ) -> Result<u8, FamilyDescriptorScoreError> {
        self.selector35_actor(actor)?.bytes_240[index].ok_or(
            FamilyDescriptorScoreError::Uninitialized {
                actor,
                offset: 0x240 + index as u16,
            },
        )
    }

    fn descriptor_score_coefficient(
        &self,
        actor: ObjectId,
        coefficient_offset: u32,
    ) -> Result<u16, FamilyDescriptorScoreError> {
        let (allocation, base) = self.descriptor_score_retained(actor)?;
        let offset = base.checked_add(coefficient_offset).ok_or(
            FamilyDescriptorScoreError::UnsupportedRetainedBinding(actor),
        )?;
        Ok(allocation.word(offset).map_err(DerivedActorError::from)?)
    }

    /// 00428320 wraps each displacement as a signed WORD, then squares and
    /// sums as DWORDs. The bit31 repair is not CPU signed-overflow recovery.
    /// 00428990 escort source over handle `+20C`. A live referent (signed
    /// `+C4 >= 1`) yields its wrapped distance plus 0x100, or 0xFFFF beyond
    /// signed `+164`. A missing or dead referent clears `+210`, then stores
    /// the NULL reread at 00428A20 into `+20C`, and scores zero.
    fn descriptor_escort_source(
        &mut self,
        actor: ObjectId,
        null: ActorHandle,
    ) -> Result<i32, FamilyDescriptorScoreError> {
        let handle = self.family428650_actor(actor)?.word_20c;
        if handle == null.raw() {
            return Ok(0);
        }
        if let Some(referent) = self.resolve_actor_handle(ActorHandle::from_raw(handle))? {
            if (self.actor(referent)?.word_c4 as i32) >= 1 {
                let own = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let other = self
                    .object(referent)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(referent))?;
                // 004289B9..EA: signed WORD deltas, DWORD-wrapping squares.
                let squared = (0..3).fold(0u32, |sum, axis| {
                    let delta = i32::from(own[axis].wrapping_sub(other[axis]) as i16);
                    sum.wrapping_add(delta.wrapping_mul(delta) as u32)
                });
                let distance = if squared & 0x8000_0000 != 0 {
                    isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1) as i32
                } else {
                    isqrt_48efc0(squared as i32) as i32
                };
                return Ok(if distance > self.derived_actor(actor)?.word_164 {
                    0xffff
                } else {
                    distance.wrapping_add(0x100)
                });
            }
        }
        let family = self.family428650_actor_mut(actor)?;
        family.word_210 = 0;
        family.word_20c = null.raw();
        Ok(0)
    }

    fn descriptor_home_distance(&self, actor: ObjectId) -> Result<i32, FamilyDescriptorScoreError> {
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let home = self.family428650_actor(actor)?.position_222.ok_or(
            FamilyDescriptorScoreError::Uninitialized {
                actor,
                offset: 0x224,
            },
        )?;
        let squared = [1, 0, 2].into_iter().fold(0u32, |sum, index| {
            let delta = i32::from(position[index].wrapping_sub(home[index]) as i16);
            sum.wrapping_add(delta.wrapping_mul(delta) as u32)
        });
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1) as i32
        } else {
            isqrt_48efc0(squared as i32) as i32
        };
        let radius = self.derived_actor(actor)?.word_164;
        Ok(if distance < radius {
            distance
        } else {
            radius / 2
        })
    }

    // Portable admission retains the actual class-owned +23C allocation.
    // Native wrappers do not inspect a constructor/model/mission whitelist.
    fn descriptor_score_retained(
        &self,
        actor: ObjectId,
    ) -> Result<(&RuntimeAllocation, u32), FamilyDescriptorScoreError> {
        let (allocation, offset) = match &self.family428650_actor(actor)?.class {
            Family428650Class::Family429e20(state) => match &state.class {
                Family429e20Class::Family4236d0(state) => {
                    let retained = state
                        .retained_23c
                        .as_ref()
                        .ok_or(DerivedActorError::MissingFamily4236d0Payload)?;
                    (retained.allocation(), retained.allocation_offset)
                }
                Family429e20Class::Selector12(state) => {
                    let retained = state
                        .retained_23c
                        .as_ref()
                        .ok_or(DerivedActorError::MissingSelector12Payload)?;
                    (retained.allocation(), retained.allocation_offset)
                }
                _ => {
                    return Err(FamilyDescriptorScoreError::UnsupportedRetainedBinding(
                        actor,
                    ))
                }
            },
            Family428650Class::Selector34(state) => {
                let retained = state
                    .retained_23c
                    .as_ref()
                    .ok_or(DerivedActorError::MissingSelector34Payload)?;
                (retained.allocation(), retained.allocation_offset)
            }
            Family428650Class::Selector35(state) => {
                let retained = state
                    .retained_23c
                    .as_ref()
                    .ok_or(DerivedActorError::MissingSelector35Payload)?;
                (retained.allocation(), retained.allocation_offset)
            }
            _ => {
                return Err(FamilyDescriptorScoreError::UnsupportedRetainedBinding(
                    actor,
                ))
            }
        };
        Ok((allocation, offset))
    }
}

#[cfg(test)]
#[path = "descriptor_motion/tests.rs"]
mod tests;
