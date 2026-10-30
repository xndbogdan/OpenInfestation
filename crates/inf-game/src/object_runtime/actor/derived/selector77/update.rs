//! Selector77 virtual+24 (00407CA0), idle 00407B00, target controls 00407760,
//! steering 004081B0 and extension virtual+D4 (004080C0). The timer-expiry
//! prefix 00407600 resolves the actual first player's primary actor. Required
//! target/fire services retain their actual live owners. Retained fallible
//! target allocation and explicit release preserve their native prefixes.
//!
//! Complete original bodies are guarded in the owning source test. Native
//! ctor 00407410 leaves19C/1A0 unwritten; this kernel does not initialize them.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::Selector77State;
use crate::object_runtime::{
    ActorDamageCallback, ActorDamagePacket, ActorHandle, ActorWeaponFireError,
    BehaviorCacheStorageError, DerivedActorClass, DerivedActorError, FamilyUpdateEffect,
    FamilyUpdateError, FamilyUpdateHost, NativeHeapRef, NativeTarget, NativeTargetClass,
    NativeTargetQueryError, ObjectArena, ObjectError, ObjectId, ProcessActorBehaviorTables,
};
mod member;
mod pair;
mod target;

#[derive(Clone, Copy, Debug)]
pub struct Selector77TickRequest<'a> {
    /// The actual unsigned raw microsecond argument. Native callers do not
    /// rescale it through the process status load factor.
    pub microseconds: u32,
    /// Current process0054E894, independent of the registry's constructor NULL.
    pub process_null_handle: Option<ActorHandle>,
    /// Current0055F154, read only by reached shared status visual arithmetic.
    pub status_load_factor: Option<i32>,
    pub sine_quarter: &'a [u16; 4096],
    pub inverse_sine: &'a [i16; 1024],
    pub settings_7c: u32,
    /// Actual0055F0F8, reached only by fixed/object target virtual08.
    pub process_velocity: Option<[u16; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector77Shot {
    pub microseconds: u32,
    pub direction_bits: [u32; 3],
    pub speed_scale: u32,
}

pub trait Selector77UpdateHost: FamilyUpdateHost {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables> {
        None
    }
    fn virtual_b4(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        Err(Selector77UpdateError::RequiredDamage { actor, callback })
    }
    fn queue_direction(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
        _shot: Selector77Shot,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        Err(Selector77UpdateError::RequiredWeaponQueue(actor))
    }
    /// 0047C6E0 on `actor` from `source`, the eye's strike (00408FAF).
    fn apply_damage(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
        _packet: ActorDamagePacket,
        _source: ActorHandle,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        Err(Selector77UpdateError::RequiredStrikeDamage(actor))
    }
    /// The liquid's height a member (selector 80) reads: H12 slot 0's first
    /// DWORD (0055EE3C) shifted by 005483BC.
    fn liquid_level(&self) -> Option<i16> {
        None
    }
    /// 00450A30 on H12 slot 0: the ground's height under `at`.
    fn ground_height(&self, _at: [u16; 2]) -> Option<i16> {
        None
    }
}
impl Selector77UpdateHost for crate::object_runtime::NativeFamilyUpdateHost<'_> {}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector77UpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error(transparent)]
    TargetQuery(#[from] NativeTargetQueryError),
    #[error(transparent)]
    Firing(#[from] ActorWeaponFireError),
    #[error(transparent)]
    Division(#[from] super::super::kinematics::ActorMotionError),
    #[error(transparent)]
    InverseSine(#[from] inf_formats::native_basis::InverseSineQ31Error),
    #[error(transparent)]
    Handles(#[from] crate::object_runtime::ActorConstructionError),
    #[error(transparent)]
    Target(#[from] crate::object_runtime::NativeTargetError),
    #[error("selector 79 strike {0:?} requires direct0047C6E0 damage service")]
    RequiredStrikeDamage(ObjectId),
    #[error(transparent)]
    Status(FamilyUpdateError<E>),
    #[error("selector77 {actor:?} requires native +{offset:03X} allocation/history backing")]
    RequiredField { actor: ObjectId, offset: u16 },
    #[error("selector77 requires actual process NULL handle0054E894")]
    MissingProcessNullHandle,
    #[error("selector77 {actor:?} requires virtual+B4: {callback:?}")]
    RequiredDamage {
        actor: ObjectId,
        callback: ActorDamageCallback,
    },
    #[error("selector77 target{reference:?} requires its {field} words")]
    RequiredTargetWords {
        reference: NativeHeapRef,
        field: &'static str,
    },
    #[error("selector77 requires live process behavior tables")]
    RequiredBehaviorTables,
    #[error("selector77 {0:?} requires direct0048D0D0 queue service")]
    RequiredWeaponQueue(ObjectId),
    #[error(
        "selector 80 {0:?} reads H12 slot 0 (0055EE3C), which is vacant; retail dereferences NULL"
    )]
    VacantTerrain(ObjectId),
    #[error("selector77 live service failed at{address:08X}: {error}")]
    Service { address: u32, error: E },
    #[error("selector77 process RNG failed at {address:08X}: {error}")]
    Random { address: u32, error: E },
    #[error("selector77 host failed at {effect:?}: {error}")]
    Host {
        effect: FamilyUpdateEffect,
        error: E,
    },
}

impl ObjectArena {
    /// 00407CA0. Status precedes the signed+C4 gate; equality in unsigned
    /// +198 comparison subtracts to zero without reacquisition. Each mutation
    /// before a reached error remains in the actual arena/process owners.
    pub fn tick_selector77<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: Selector77TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        self.selector77_actor(actor)?;
        let null = request
            .process_null_handle
            .ok_or(Selector77UpdateError::MissingProcessNullHandle)?;
        let source = self.selector77_actor(actor)?.handle_1b0;
        if source != null {
            host.virtual_b4(
                self,
                actor,
                ActorDamageCallback {
                    adjusted_damage: 10_000,
                    packet: ActorDamagePacket {
                        strength: 10_000,
                        channel: 9,
                        byte_03: None,
                    },
                    source,
                },
            )?;
        }
        self.tick_actor_status(
            actor,
            request.microseconds,
            request.status_load_factor,
            host,
        )
        .map_err(Selector77UpdateError::Status)?;
        if self.actor(actor)?.word_c4 as i32 <= 2 {
            return Ok(());
        }
        let timer = self.selector77_actor(actor)?.word_198;
        if timer < request.microseconds {
            self.selector77_reacquire(actor, host)?;
        } else {
            self.selector77_update_state_mut(actor)?.word_198 =
                timer.wrapping_sub(request.microseconds);
        }
        if self.derived_actor(actor)?.behavior_140.is_some() {
            let effect = FamilyUpdateEffect::WeaponTick {
                microseconds: request.microseconds,
            };
            host.effect(self, actor, effect)
                .map_err(|error| Selector77UpdateError::Host { effect, error })?;
        }
        // 00407D1C captures target before00407D29 publishes the control limit.
        // Even a released pointer is nonnull; do not reinterpret it as idle.
        let target = self.selector77_update_target(actor);
        self.selector77_update_state_mut(actor)?.word_1ac = Some(request.microseconds >> 6);
        let target = target?;
        if let Some(mut target) = target {
            let reference = self
                .derived_actor(actor)?
                .target_148
                .expect("captured nonnull target");
            let result = self.refresh_native_target(&mut target, request.settings_7c);
            self.write_native_target(reference, target)?;
            if result? == 0 {
                let timer = self.selector77_actor(actor)?.word_198;
                self.selector77_update_state_mut(actor)?.word_198 =
                    timer.wrapping_sub(request.microseconds);
            }
            return self.selector77_target_update(actor, &request, host);
        }
        self.selector77_idle(actor, request.microseconds, request.sine_quarter, host)
    }

    fn selector77_update_state_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector77State, DerivedActorError> {
        match &mut self.derived_actor_mut(actor)?.class {
            DerivedActorClass::Selector77(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector77(actor)),
        }
    }

    fn selector77_update_target(
        &self,
        actor: ObjectId,
    ) -> Result<Option<NativeTarget>, ObjectError> {
        self.derived_actor(actor)?
            .target_148
            .map(|reference| self.native_target(reference))
            .transpose()
    }

    /// 00407600 through the no-player/far-player normal returns. A genuine
    /// existing target must be released before lookup; near-player allocation
    /// must occur before primary+84 or RNG reads. Neither service is simulated.
    fn selector77_reacquire<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            self.release_actor_service(reference)?;
        }
        self.derived_actor_mut(actor)?.target_148 = None;
        let Some(primary) = self.first_player_primary_actor()? else {
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_7737,
                    error,
                })?;
            self.selector77_update_state_mut(actor)?.word_198 = (random & 0xffff) << 4;
            return Ok(());
        };
        // The portable position owner admits complete raw XYZ. Within this
        // typed domain the native WORD-Y/X/Z differences and wrapping squares
        // are preserved; arbitrary partial mapped source axes remain excluded.
        let eye = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let player = self
            .object(primary)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(primary))?;
        let y = i32::from(eye[1].wrapping_sub(player[1]) as i16);
        let x = i32::from(eye[0].wrapping_sub(player[0]) as i16);
        let z = i32::from(eye[2].wrapping_sub(player[2]) as i16);
        let squared = (x.wrapping_mul(x) as u32)
            .wrapping_add(y.wrapping_mul(y) as u32)
            .wrapping_add(z.wrapping_mul(z) as u32);
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        let range = self.derived_actor(actor)?.word_164;
        if distance as i32 <= range {
            let allocation = self
                .allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                    kind: crate::object_runtime::ActorServiceResourceKind::Target(
                        NativeTargetClass::ActorFollow,
                    ),
                })
                .map_err(ObjectError::from)?;
            if let Some(reference) = allocation {
                // 004076B1 reads primary+84 only after malloc succeeds.
                let handle = self.actor(primary)?.handle;
                let data = self.follow_target(
                    crate::object_runtime::FollowTargetRequest::resolve_actor(handle),
                )?;
                self.construct_native_target(reference, NativeTarget::ActorFollow(data))?;
            }
            self.derived_actor_mut(actor)?.target_148 = allocation;
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_76c0,
                    error,
                })?;
            self.selector77_update_state_mut(actor)?.word_198 =
                (random & 0xffff).wrapping_add(62500) << 4;
            return Ok(());
        }
        // These unusual shifts are literal0040769C/004076A3, not a distance
        // division. The first store and unsigned clamp precede RNG failure.
        let candidate = (range as u32)
            .wrapping_shl(23)
            .wrapping_sub(range as u32)
            .wrapping_add(distance)
            .wrapping_shl(9);
        self.selector77_update_state_mut(actor)?.word_198 = candidate;
        if candidate > 3_000_000 {
            self.selector77_update_state_mut(actor)?.word_198 = 3_000_000;
        }
        let random = host
            .random()
            .map_err(|error| Selector77UpdateError::Random {
                address: 0x0040_76be,
                error,
            })?;
        let timer = self.selector77_actor(actor)?.word_198;
        self.selector77_update_state_mut(actor)?.word_198 =
            timer.wrapping_add((random & 0xffff) << 3);
        Ok(())
    }

    /// 00407B00 always invokes 004081B0 then+D4, including a zero delta and
    /// a false steering return. It never reads19E or a target-position fallback.
    fn selector77_idle<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        sine: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        let phase = self
            .selector77_actor(actor)?
            .word_1a0
            .ok_or(Selector77UpdateError::RequiredField {
                actor,
                offset: 0x1a0,
            })?
            .wrapping_add(microseconds);
        self.selector77_update_state_mut(actor)?.word_1a0 = Some(phase);
        let pitch = i32::from(self.selector77_actor(actor)?.words_184_190[1] as i16);
        let correction = (pitch.wrapping_mul((microseconds >> 6) as i32) >> 15) as i16;
        let first = i32::from(correction).wrapping_add(selector77_sine(phase >> 5, sine) >> 6);
        let second = selector77_sine(phase >> 7, sine) >> 6;
        self.selector77_steering::<H::Error>(actor, first, second)?;
        self.selector77_extension(actor, microseconds, sine, host)
    }

    /// 004081B0. The microsecond argument is unused by this intrinsic body;
    /// only its already-published signed1AC limit participates in steering.
    fn selector77_steering<E>(
        &mut self,
        actor: ObjectId,
        first: i32,
        second: i32,
    ) -> Result<bool, Selector77UpdateError<E>> {
        let mut accepted = true;
        let mut clamp = |arena: &Self, value: i32| -> Result<i32, Selector77UpdateError<E>> {
            if wrapping_abs(value) < 0x78 {
                return Ok(0);
            }
            let limit = arena.selector77_actor(actor)?.word_1ac.ok_or(
                Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x1ac,
                },
            )? as i32;
            if value > limit {
                accepted = false;
                return Ok(limit);
            }
            let negative = limit.wrapping_neg();
            if value < negative {
                accepted = false;
                return Ok(negative);
            }
            Ok(value)
        };
        let mut first = clamp(self, first)?;
        let mut second = clamp(self, second)?;
        if wrapping_abs(first) > wrapping_abs(second) {
            second /= 2;
        } else {
            first /= 2;
        }
        let pitch = i32::from(self.selector77_actor(actor)?.words_184_190[1] as i16)
            .wrapping_sub(first)
            .clamp(-0x4000, 0x4000);
        let roll =
            i32::from(self.selector77_actor(actor)?.words_184_190[5] as i16).wrapping_add(second);
        self.selector77_update_state_mut(actor)?.words_184_190[1] = pitch as u16;
        let roll = if wrapping_abs(roll) > 0x2000 {
            let limit = self.selector77_actor(actor)?.word_1ac.ok_or(
                Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x1ac,
                },
            )? as i32;
            let yaw = if roll > 0 {
                limit / 2
            } else {
                (limit / 2).wrapping_neg()
            };
            self.selector77_update_state_mut(actor)?.words_184_190[3] =
                self.selector77_actor(actor)?.words_184_190[3].wrapping_add(yaw as u16);
            roll.clamp(-0x3000, 0x3000)
        } else {
            if roll > 0x800 || roll < -0x800 {
                let limit = self.selector77_actor(actor)?.word_1ac.ok_or(
                    Selector77UpdateError::RequiredField {
                        actor,
                        offset: 0x1ac,
                    },
                )? as i32;
                let yaw = if roll > 0x800 {
                    limit / 4
                } else {
                    (limit / 4).wrapping_neg()
                };
                self.selector77_update_state_mut(actor)?.words_184_190[3] =
                    self.selector77_actor(actor)?.words_184_190[3].wrapping_add(yaw as u16);
            }
            roll
        };
        self.selector77_update_state_mut(actor)?.words_184_190[5] = roll as u16;
        Ok(accepted)
    }

    /// 004080C0. First visit selects a target/speed using two real RNG draws;
    /// only later visits move190. Writes/RNG before a later failure survive.
    /// A member's +D4 (selector 80, 00409010) first picks a farther target
    /// when it has none: half a draw less the unsigned extension, plus 0x2000.
    fn selector77_extension<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        sine: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        if self.selector77_actor(actor)?.member.is_some()
            && self.selector77_actor(actor)?.word_1a4 == 0
        {
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_9027,
                    error,
                })?;
            let extension = u32::from(self.selector77_actor(actor)?.words_184_190[6]);
            self.selector77_update_state_mut(actor)?.word_1a4 = ((random & 0xffff) >> 1)
                .wrapping_sub(extension)
                .wrapping_add(0x2000);
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_9040,
                    error,
                })?;
            self.selector77_update_state_mut(actor)?.word_1a8 = ((random & 0xffff) >> 2) + 0x1800;
        }
        let phase = self
            .selector77_actor(actor)?
            .word_19c
            .ok_or(Selector77UpdateError::RequiredField {
                actor,
                offset: 0x19c,
            })?
            .wrapping_add((microseconds >> 5) as u16);
        self.selector77_update_state_mut(actor)?.word_19c = Some(phase);
        let remaining = self.selector77_actor(actor)?.word_1a4;
        self.selector77_update_state_mut(actor)?.words_184_190[2] =
            (0x0c00 + (selector77_sine(u32::from(phase), sine) >> 4)) as u16;
        if remaining == 0 {
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_8156,
                    error,
                })?;
            let extension = i32::from(self.selector77_actor(actor)?.words_184_190[6] as i16);
            self.selector77_update_state_mut(actor)?.word_1a4 =
                ((random & 0xffff) / 3 + 0x800).wrapping_sub(extension as u32);
            let random = host
                .random()
                .map_err(|error| Selector77UpdateError::Random {
                    address: 0x0040_8186,
                    error,
                })?;
            self.selector77_update_state_mut(actor)?.word_1a8 = ((random & 0xffff) >> 2) + 0x1800;
            return Ok(());
        }
        let speed = self.selector77_actor(actor)?.word_1a8;
        let mut step = ((speed.wrapping_mul(microseconds >> 5) as i32 >> 15) as i16) as i32;
        let remaining = remaining as i32;
        if remaining > 0 {
            if step > remaining {
                step = remaining;
            }
        } else {
            step = step.wrapping_neg();
            if step < remaining {
                step = remaining;
            }
        }
        let extension = self.selector77_actor(actor)?.words_184_190[6];
        self.selector77_update_state_mut(actor)?.words_184_190[6] =
            extension.wrapping_add(step as u16);
        self.selector77_update_state_mut(actor)?.word_1a4 = remaining.wrapping_sub(step) as u32;
        Ok(())
    }
}

fn wrapping_abs(value: i32) -> i32 {
    let sign = value >> 31;
    (value ^ sign).wrapping_sub(sign)
}

fn selector77_sine(phase: u32, sine: &[u16; 4096]) -> i32 {
    let low = phase & 0x3ffc;
    let index = if phase & 0x4000 != 0 {
        0x3ffc - low
    } else {
        low
    } >> 2;
    let value = i32::from(sine[index as usize]);
    if phase & 0x8000 != 0 {
        -value
    } else {
        value
    }
}

#[cfg(test)]
mod tests;
