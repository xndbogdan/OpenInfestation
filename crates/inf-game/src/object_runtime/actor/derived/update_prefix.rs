//! Bounded shared virtual +24 prefixes for admitted family actors. Retail
//! 00424830/004679F0/00481710/0043E6D0 deliberately order water, status,
//! activation delay and weapon timers differently. This owner stops before
//! virtual +D8; behavior dispatch and the later motion tail remain required.

use thiserror::Error;

use super::{Family4236d0Class, Family428650Class, Family429e20Class};
use crate::object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId};

mod status_visual;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyUpdateClass {
    /// The family's own class (selector 9, vtable 00520B84): selector 10's
    /// 00424830 with its own +118 motion 00424CF0.
    Family4236d0,
    Selector10,
    Selector12,
    Selector36,
    Selector35,
    /// 00434210: Selector35's prefix shape (no 00429D60 water tick); its
    /// class tail is controls 0043429C, motion +118 00434C10 and audio.
    Selector34,
}

/// Resolved H12 DWORD +00 and process 005483BC. The native consumer shifts
/// arithmetically with x86 masking, then sign-extends the resulting low WORD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorWaterPlane {
    pub height_fixed: i32,
    pub coordinate_shift: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyUpdateRequest {
    pub microseconds: u32,
    /// Required only by the classes whose actual +24 calls 00429D60.
    pub water_plane: Option<ActorWaterPlane>,
    /// Process DWORD0055F154, required only by a reached 0047DFD0 read.
    /// This is independent of the original raw microsecond argument.
    pub status_load_factor: Option<i32>,
}

/// The three authored bytes of 00429D60's temporary damage packet and its
/// explicit 0047C7C0 arguments. No uninitialized fourth byte is synthesized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorWaterDamage {
    pub amount: u16,
    pub channel: u8,
    pub source: ActorHandle,
    pub microseconds: u32,
    pub period_microseconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyUpdateEffect {
    /// 0047C7C0, including damage responses and actual virtual +B4.
    Drowning(ActorWaterDamage),
    /// The reached 0047DFD0 ->00489D90 packet. The backend must use the
    /// actual process particle owner, executable and current birth inputs.
    /// All status arithmetic/RNG before this boundary is already committed;
    /// status fields are reread when this synchronous callback returns.
    StatusTrail {
        particle_type: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        flags: u32,
    },
    /// 0047CE90, with the native registry null source. The existing lifecycle
    /// owner can service this callback; this prefix does not fake retirement.
    Retire,
    /// 0047B1E0(delta,0x1F), after the reached +144 write.
    DelayedPhysics { microseconds: u32, mask: u32 },
    /// Nonnull +140 owner, 0048D030(delta).
    WeaponTick { microseconds: u32 },
}

pub trait FamilyUpdateHost {
    type Error;

    /// Current process0054E894 at429DBF, read only when drowning reaches
    /// the actual damage call. Registry null is a different owner.
    fn process_null_handle(&self) -> Option<ActorHandle>;

    /// One draw from the actual 0048F060 process stream. No private actor
    /// RNG or successful default substitutes for this required service.
    fn random(&mut self) -> Result<u32, Self::Error>;

    /// Required native effects may mutate the arena synchronously. There is
    /// no default successful callback for an unowned effect.
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyUpdateProgress {
    /// Native +24 returns on a negative or positive initial delay. Positive
    /// delay has also executed the required physics callback before return.
    Returned,
    /// The next native call is virtual +D8, followed by class-specific target
    /// geometry, movement, collision, weapons and audio. This is not a tick
    /// completion result and must not be counted as a complete +24 callback.
    BeforeBehaviorDispatch { class: FamilyUpdateClass },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyUpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} has no proven family update prefix")]
    UnsupportedClass(ObjectId),
    #[error("actor {0:?} requires its resolved native water plane")]
    MissingWaterPlane(ObjectId),
    #[error("actor {actor:?} requires the unwritten native +{offset:03X} value")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {0:?} reaches native status division by zero at 0047CA68")]
    StatusDivisionByZero(ObjectId),
    #[error("actor {0:?} requires process DWORD0055F154 at0047DFE3")]
    MissingStatusLoadFactor(ObjectId),
    #[error("actor {0:?} drowning requires process0054E894 at00429DBF")]
    MissingProcessNullHandle(ObjectId),
    #[error("actor {actor:?} reaches native status visual division at {address:08X}: {numerator} / {denominator}")]
    StatusVisualDivision {
        actor: ObjectId,
        address: u32,
        numerator: i32,
        denominator: i32,
    },
    #[error("status visual RNG service failed at {address:08X}: {error}")]
    StatusVisualRandom { address: u32, error: E },
    #[error("family update host failed at {effect:?}: {error}")]
    Host {
        effect: FamilyUpdateEffect,
        error: E,
    },
}

impl ObjectArena {
    pub fn family_update_class(
        &self,
        actor: ObjectId,
    ) -> Result<FamilyUpdateClass, FamilyUpdateError<std::convert::Infallible>> {
        let class = match &self.family428650_actor(actor)?.class {
            Family428650Class::Family429e20(state) => match &state.class {
                Family429e20Class::Family4236d0(state) => match state.class {
                    Family4236d0Class::Base => FamilyUpdateClass::Family4236d0,
                    Family4236d0Class::Selector10(_) => FamilyUpdateClass::Selector10,
                    Family4236d0Class::Selector36(_) => FamilyUpdateClass::Selector36,
                },
                Family429e20Class::Selector12(_) => FamilyUpdateClass::Selector12,
                _ => return Err(FamilyUpdateError::UnsupportedClass(actor)),
            },
            Family428650Class::Selector35(_) => FamilyUpdateClass::Selector35,
            Family428650Class::Selector34(_) => FamilyUpdateClass::Selector34,
            _ => return Err(FamilyUpdateError::UnsupportedClass(actor)),
        };
        Ok(class)
    }

    /// 00429D60. Every geometry branch requires the allocator-carried WORD
    /// +230. Unknown state fails exactly at that read, before damage or writes.
    pub fn tick_actor_water<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        plane: ActorWaterPlane,
        host: &mut H,
    ) -> Result<(), FamilyUpdateError<H::Error>> {
        self.family429e20_actor(actor)?;
        let base = &self.object(actor)?.base;
        let position = base
            .position_words
            .ok_or(FamilyUpdateError::Uninitialized {
                actor,
                offset: 0x32,
            })?;
        let plane_y = (plane.height_fixed >> (plane.coordinate_shift & 31)) as i16;
        let separation = i32::from(position[1] as i16) - i32::from(plane_y);
        let submerged = separation < 0 && separation < -(i32::from(base.word_28) / 2);
        let previous =
            self.family429e20_actor(actor)?
                .word_230
                .ok_or(FamilyUpdateError::Uninitialized {
                    actor,
                    offset: 0x230,
                })?;
        if submerged {
            if previous > 4000 {
                let amount =
                    self.actor(actor)?
                        .maximum_health
                        .ok_or(FamilyUpdateError::Uninitialized {
                            actor,
                            offset: 0xa2,
                        })?;
                let source = host
                    .process_null_handle()
                    .ok_or(FamilyUpdateError::MissingProcessNullHandle(actor))?;
                self.family_update_effect(
                    actor,
                    FamilyUpdateEffect::Drowning(ActorWaterDamage {
                        amount,
                        channel: 5,
                        source,
                        microseconds,
                        period_microseconds: 4_000_000,
                    }),
                    host,
                )?;
            } else {
                self.family429e20_actor_mut(actor)?.word_230 =
                    Some(previous.wrapping_add((microseconds >> 10) as u16));
            }
        } else if previous != 0 {
            self.family429e20_actor_mut(actor)?.word_230 =
                Some(previous.wrapping_sub((microseconds >> 9) as u16));
        }
        Ok(())
    }

    /// 0047C990's shared status arithmetic. Native visual and retirement
    /// effects remain required; all fields consumed after them are reread.
    pub fn tick_actor_status<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        status_load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), FamilyUpdateError<H::Error>> {
        let timer = self.actor(actor)?.word_c8;
        if timer != 0 {
            self.actor_mut(actor)?.word_c8 =
                u32::from(timer).saturating_sub(microseconds >> 5) as u16;
        }
        if self.actor(actor)?.word_ce == 0 && self.actor(actor)?.word_cc == 0 {
            return Ok(());
        }
        self.tick_status_visual(actor, microseconds, status_load_factor, host)?;
        if self.actor(actor)?.word_ce != 0 {
            let strength = self.actor(actor)?.word_cc;
            // 0047CA09 wraps the DWORD product, arithmetic-shifts it, then
            // sign-extends the low WORD. A negative result is not clamped.
            let product = (microseconds >> 5).wrapping_mul(u32::from(strength));
            let mut amount = ((product as i32 >> 15) as i16) as i32;
            if amount == 0 {
                amount = 1;
            } else if amount > i32::from(self.actor(actor)?.word_ce) {
                amount = i32::from(self.actor(actor)?.word_ce);
            }
            let remaining = self.actor(actor)?.word_ce.wrapping_sub(amount as u16);
            self.actor_mut(actor)?.word_ce = remaining;
            if remaining == 0 {
                let maximum =
                    self.actor(actor)?
                        .maximum_health
                        .ok_or(FamilyUpdateError::Uninitialized {
                            actor,
                            offset: 0xa2,
                        })?;
                self.actor_mut(actor)?.word_cc = if strength > maximum {
                    2000
                } else {
                    if maximum == 0 {
                        return Err(FamilyUpdateError::StatusDivisionByZero(actor));
                    }
                    (u32::from(strength) * 2000 / u32::from(maximum)) as u16
                };
            }
            let health = self.actor(actor)?.health;
            if amount < i32::from(health) {
                self.actor_mut(actor)?.health = health.wrapping_sub(amount as u16);
            } else {
                self.family_update_effect(actor, FamilyUpdateEffect::Retire, host)?;
            }
        } else {
            let timer = self.actor(actor)?.word_cc;
            self.actor_mut(actor)?.word_cc =
                u32::from(timer).saturating_sub(microseconds >> 10) as u16;
        }
        Ok(())
    }

    /// Execute only the proven class prefix. No dispatcher or motion callback
    /// is silently skipped or accepted as a complete class update.
    pub fn update_family_prefix<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyUpdateRequest,
        host: &mut H,
    ) -> Result<FamilyUpdateProgress, FamilyUpdateError<H::Error>> {
        // The concrete vtable selects the enclosing method once; callbacks
        // may mutate fields but cannot silently select a different policy.
        let class = self
            .family_update_class(actor)
            .map_err(|error| match error {
                FamilyUpdateError::Object(error) => FamilyUpdateError::Object(error),
                _ => FamilyUpdateError::UnsupportedClass(actor),
            })?;
        let microseconds = request.microseconds;
        // 00434210 (selector34) also begins directly at the +144 delay test.
        if !matches!(
            class,
            FamilyUpdateClass::Selector35 | FamilyUpdateClass::Selector34
        ) {
            let plane = request
                .water_plane
                .ok_or(FamilyUpdateError::MissingWaterPlane(actor))?;
            self.tick_actor_water(actor, microseconds, plane, host)?;
        }
        // 00424830, the family's own +24 and selector 10's.
        let shared_424830 = matches!(
            class,
            FamilyUpdateClass::Family4236d0 | FamilyUpdateClass::Selector10
        );
        if shared_424830 {
            self.tick_actor_status(actor, microseconds, request.status_load_factor, host)?;
        }
        let delay = self.derived_actor(actor)?.word_144;
        if delay < 0 {
            return Ok(FamilyUpdateProgress::Returned);
        }
        if delay != 0 {
            self.derived_actor_mut(actor)?.word_144 = if delay > microseconds as i32 {
                delay.wrapping_sub(microseconds as i32)
            } else {
                0
            };
            self.family_update_effect(
                actor,
                FamilyUpdateEffect::DelayedPhysics {
                    microseconds,
                    mask: 0x1f,
                },
                host,
            )?;
            return Ok(FamilyUpdateProgress::Returned);
        }
        if !shared_424830 {
            self.tick_actor_status(actor, microseconds, request.status_load_factor, host)?;
        }
        if shared_424830 {
            let timer = self.derived_actor(actor)?.word_17a;
            if timer != 0 {
                let next = timer.wrapping_sub((microseconds >> 10) as u16);
                self.derived_actor_mut(actor)?.word_17a = if (next as i16) < 0 { 0 } else { next };
            }
        }
        if class != FamilyUpdateClass::Selector36
            && self.derived_actor(actor)?.behavior_140.is_some()
        {
            self.family_update_effect(
                actor,
                FamilyUpdateEffect::WeaponTick { microseconds },
                host,
            )?;
        }
        Ok(FamilyUpdateProgress::BeforeBehaviorDispatch { class })
    }

    fn family_update_effect<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
        host: &mut H,
    ) -> Result<(), FamilyUpdateError<H::Error>> {
        host.effect(self, actor, effect)
            .map_err(|error| FamilyUpdateError::Host { effect, error })
    }
}

#[cfg(test)]
#[path = "update_prefix_tests.rs"]
mod tests;
