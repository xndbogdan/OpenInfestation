//! Selector 32, the cannon's shot: 00485F50's class with final table
//! 005285F0. It lives 600 ms. Its draw (+2C, 00485F90) keeps it at its
//! owner's muzzle, aimed at the owner's target; its update (+24, 00486250)
//! throws sparks (+3C, 00486100) and hands the target one damage packet per
//! pending shot (00488830). A repeat shot from the same weapon (+98,
//! 004860D0) adds one and restarts its life.
//!
//! Selector 31, the Lasers, derives from the same class (00485480, final
//! table 00528524) and keeps all of this but its update, +3C and draw: its
//! update (00485500) first adds the delta to its age +154, and its +3C
//! (004854C0) turns its angle +150 instead of throwing sparks. Its draw is
//! in [`super::laser31`].

use std::convert::Infallible;

use thiserror::Error;

use super::{BehaviorResourceClass, Family485f50State};
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    ActorBehaviorPositionError, ActorDamagePacket, ActorHandle, NativeHeapRef, ObjectArena,
    ObjectError, ObjectId,
};

/// The life a shot starts with and a repeat restores (004860E5).
pub const SHOT32_LIFE_MICROS: u32 = 600_000;
/// 004861BC's quantum (005205B0) and 004861F5's spark speed (00520A54).
const SPARK_QUANTUM: f64 = 4096.0;
const SPARK_SPEED: f64 = 3000.0;
/// 00486023's reach along the forward row without a target (005286BC).
const AIM_REACH: f64 = 2000.0;

/// The update's services.
pub trait Shot32Host {
    type Error;
    /// 0048A0F0(5, position, velocity, 0).
    fn spark(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 00488830(packet, target, owner).
    fn deliver(
        &mut self,
        arena: &mut ObjectArena,
        packet: ActorDamagePacket,
        target: NativeHeapRef,
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Shot32Error<E = Infallible> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Position(#[from] ActorBehaviorPositionError),
    #[error("shot {shot:?} is not a selector-32 shot")]
    NotShot { shot: ObjectId },
    #[error("shot {shot:?} requires its {field} at {address:08X}")]
    Unknown {
        shot: ObjectId,
        field: &'static str,
        address: u32,
    },
    /// The owner's +58 decides who holds the target. The vehicle answers
    /// itself (00404610); other owners are not ported.
    #[error("shot {shot:?}'s owner {owner:?} has no ported virtual +58")]
    Owner { shot: ObjectId, owner: ObjectId },
    #[error("shot service failed")]
    Host(E),
}

impl ObjectArena {
    /// The cannon's shot or a laser: 00485F50's class, selectors 32 and 31.
    pub(crate) fn shot32(&self, shot: ObjectId) -> Result<&Family485f50State, ObjectError> {
        let state = self.behavior_resource(shot)?;
        match &state.class {
            BehaviorResourceClass::Family485f50(family) if matches!(state.selector, 31 | 32) => {
                Ok(family)
            }
            _ => Err(ObjectError::NotBehaviorResource(shot)),
        }
    }

    pub(super) fn shot32_mut(
        &mut self,
        shot: ObjectId,
    ) -> Result<&mut Family485f50State, ObjectError> {
        match &mut self.actor_mut(shot)?.extension {
            crate::object_runtime::ActorExtension::BehaviorResource(state)
                if matches!(state.selector, 31 | 32) =>
            {
                match &mut state.class {
                    BehaviorResourceClass::Family485f50(family) => Ok(family),
                    BehaviorResourceClass::Family455560(_) => {
                        Err(ObjectError::NotBehaviorResource(shot))
                    }
                }
            }
            _ => Err(ObjectError::NotBehaviorResource(shot)),
        }
    }

    pub fn is_shot32(&self, object: ObjectId) -> bool {
        self.shot32(object).is_ok()
    }

    /// The owner's +58: the vehicle's 00404610 answers the vehicle, which
    /// holds the target (+D0, 00402790: +148) and the behavior (+7C,
    /// 004027A0: +140).
    fn shot32_holder<E>(
        &self,
        shot: ObjectId,
        owner: ObjectId,
    ) -> Result<ObjectId, Shot32Error<E>> {
        if self.selector2_actor(owner).is_ok() {
            Ok(owner)
        } else {
            Err(Shot32Error::Owner { shot, owner })
        }
    }

    /// 00486250, the shot's +24; a laser's 00485500 first adds the delta to
    /// its age +154.
    pub fn tick_shot32<H: Shot32Host>(
        &mut self,
        shot: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Shot32Error<H::Error>> {
        if let Some((_, age)) = &mut self.shot32_mut(shot)?.selector31 {
            *age = age.wrapping_add(delta);
        }
        let life = self.shot32(shot)?.dword_140;
        // 00486260: JBE, unsigned.
        if life <= delta {
            self.shot32_mut(shot)?.dword_140 = 0;
            self.dispose(shot)?;
            return Ok(());
        }
        let state = self.shot32_mut(shot)?;
        state.dword_140 = life - delta;
        // 004854C0, a laser's +3C: the angle turns by a tenth of the delta,
        // backwards in weapon group 1.
        let group = state.word_144;
        if let Some((angle, _)) = &mut state.selector31 {
            let turn = (delta / 10) as u16;
            *angle = if group != 0 {
                angle.wrapping_sub(turn)
            } else {
                angle.wrapping_add(turn)
            };
        } else {
            self.shot32_sparks(shot, rng, host)?;
        }
        let owner_handle = self.actor(shot)?.owner_handle;
        let owner = self
            .resolve_actor_handle(owner_handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let Some(owner) = owner else {
            self.dispose(shot)?;
            return Ok(());
        };
        if self.shot32(shot)?.dword_14c == 0 {
            return Ok(());
        }
        let holder = self.shot32_holder(shot, owner)?;
        let Some(target) = self.derived_actor(holder)?.target_148 else {
            return Ok(());
        };
        let state = self.shot32(shot)?;
        let strength = state.word_146.ok_or(Shot32Error::Unknown {
            shot,
            field: "strength +146",
            address: 0x0048_62ad,
        })?;
        // 004862BA..004862C7 write the WORD and the channel BYTE of a stack
        // DWORD; its fourth byte is residue.
        let packet = ActorDamagePacket {
            strength: strength as i16,
            channel: state.byte_148,
            byte_03: None,
        };
        while self.shot32(shot)?.dword_14c != 0 {
            let owner_handle = self.actor(shot)?.owner_handle;
            host.deliver(self, packet, target, owner_handle, rng)
                .map_err(Shot32Error::Host)?;
            let state = self.shot32_mut(shot)?;
            state.dword_14c = state.dword_14c.wrapping_sub(1);
        }
        Ok(())
    }

    /// The `+3C` of a shot another machine fired, which its owner ends: the
    /// cannon shot's sparks (00486100), or the laser's turn (004854C0).
    pub fn tick_remote_shot32<H: Shot32Host>(
        &mut self,
        shot: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Shot32Error<H::Error>> {
        let state = self.shot32_mut(shot)?;
        let group = state.word_144;
        if let Some((angle, _)) = &mut state.selector31 {
            let turn = (delta / 10) as u16;
            *angle = if group != 0 {
                angle.wrapping_sub(turn)
            } else {
                angle.wrapping_add(turn)
            };
            return Ok(());
        }
        // A shot built from a record can throw before a record's sync writes
        // its aim +138: retail's allocator residue, the fresh heap's zero.
        let state = self.shot32_mut(shot)?;
        if state.words_138.is_none() {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("RemoteShotAimApproximation: a shot another machine fired throws a spark before a record's sync writes its aim +138; retail reads allocator residue, the port the fresh heap's zero (fresh_heap.rs)");
            }
            state.words_138 = Some([0; 3]);
        }
        self.shot32_sparks(shot, rng, host)
    }

    /// 00486100, the shot's +3C: one throw in three, a spark along its aim.
    fn shot32_sparks<H: Shot32Host>(
        &mut self,
        shot: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Shot32Error<H::Error>> {
        if name_entry_rng_next(rng) % 3 != 0 {
            return Ok(());
        }
        let aim = self.shot32(shot)?.words_138.ok_or(Shot32Error::Unknown {
            shot,
            field: "aim +138",
            address: 0x0048_6120,
        })?;
        let position = self
            .object(shot)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(shot))?;
        host.spark(self, position, spark_velocity(aim), rng)
            .map_err(Shot32Error::Host)
    }

    /// 004860D0, the shot's +98: mode 2 adds a pending packet, and 1 or 2
    /// restarts the shot's life. Answers whether it took the shot.
    pub fn retrigger_shot32(&mut self, shot: ObjectId, mode: u32) -> Result<bool, ObjectError> {
        if !matches!(mode, 1 | 2) {
            return Ok(false);
        }
        let state = self.shot32_mut(shot)?;
        if mode == 2 {
            state.dword_14c = state.dword_14c.wrapping_add(1);
        }
        state.dword_140 = SHOT32_LIFE_MICROS;
        Ok(true)
    }

    /// 0048CA90..0048CAC3, the emitter through the new shot's +6C (the
    /// shot): its weapon group at +144, and with registry flag 0x40 a
    /// positive strength becomes -strength / 10.
    pub fn set_shot32_group(
        &mut self,
        shot: ObjectId,
        group: u16,
        negate_tenth: bool,
    ) -> Result<(), ObjectError> {
        let selector = self.behavior_resource(shot)?.selector;
        let state = self.shot32_mut(shot)?;
        state.word_144 = group;
        if negate_tenth {
            let strength = state.word_146.ok_or(ObjectError::BehaviorResourcePayload {
                object: shot,
                selector,
            })? as i16;
            if strength > 0 {
                // IMUL by 99999999h, SAR EDX 2, then the sign bit added.
                let high = ((i64::from(strength) * -1_717_986_919) >> 32) as i32 >> 2;
                state.word_146 = Some(high.wrapping_add((high as u32 >> 31) as i32) as u16);
            }
        }
        Ok(())
    }

    /// 00485F90 (the draw +2C, through 004860C0): the shot moves to its
    /// owner's muzzle and aims at the owner's target, or along the owner's
    /// forward row without one. It draws nothing.
    pub fn pose_shot32(
        &mut self,
        shot: ObjectId,
        network_active: bool,
        settings_7c: u32,
    ) -> Result<(), Shot32Error> {
        if network_active && self.object(shot)?.base.flags & 0x8000_0000 != 0 {
            return Ok(());
        }
        let owner = self
            .resolve_actor_handle(self.actor(shot)?.owner_handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let Some(owner) = owner else {
            return Ok(());
        };
        let holder = self.shot32_holder(shot, owner)?;
        let target = self.derived_actor(holder)?.target_148;
        if self.derived_actor(holder)?.behavior_140.is_some() {
            // 0048D730(1, +144, owner).
            let slot = u32::from(self.shot32(shot)?.word_144);
            let muzzle = self.actor_behavior_group_position(owner, slot)?;
            self.owned_mut(shot)?.state.base.position_words = Some(muzzle);
        }
        let aim = match target {
            None => {
                // +48: the owner's basis, its forward row at +1C.
                let basis = self.actor(holder)?.rotating.basis;
                std::array::from_fn(|axis| {
                    ftol(f64::from(f32::from_bits(basis.words[7 + axis])) * AIM_REACH) as u16
                })
            }
            Some(reference) => {
                // +18 refreshes the target, then each axis reads its +1C.
                let mut native = self.native_target(reference)?;
                let refreshed = self.refresh_native_target(&mut native, settings_7c);
                self.write_native_target(reference, native)?;
                refreshed?;
                let point =
                    self.native_target(reference)?
                        .point_words()
                        .ok_or(Shot32Error::Unknown {
                            shot,
                            field: "target point +1C",
                            address: 0x0048_6073,
                        })?;
                let position = self
                    .object(shot)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(shot))?;
                std::array::from_fn(|axis| point[axis].wrapping_sub(position[axis]))
            }
        };
        self.shot32_mut(shot)?.words_138 = Some(aim);
        Ok(())
    }
}

/// MSVCRT `__ftol` (`00511BA8`): truncation to a QWORD, of which callers
/// keep the low DWORD.
fn ftol(value: f64) -> i32 {
    let value = value.trunc();
    if value.is_finite()
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        value as i64 as i32
    } else {
        0
    }
}

/// 00486120..00486228: the aim's direction (the x87 at double precision,
/// each store an f32), cut to 1/4096, normalized again by 00417DA0 and
/// scaled to the spark speed, each step truncated.
fn spark_velocity(aim: [u16; 3]) -> [u16; 3] {
    let [x, y, z] = aim.map(|word| f64::from(word as i16));
    let length = ((z * z + y * y) + x * x).sqrt();
    let direction = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        // 0048617C divides by the f32 spill of the length.
        let length = f64::from(length as f32);
        [x, y, z].map(|v| (v / length) as f32)
    };
    let quantized = direction.map(|v| ftol(f64::from(v) * SPARK_QUANTUM));
    // 00417DA0 reads each pushed DWORD's low WORD and divides by the
    // unspilled length.
    let [x, y, z] = quantized.map(|v| f64::from(v as i16));
    let length = ((z * z + y * y) + x * x).sqrt();
    let unit = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [x, y, z].map(|v| (v / length) as f32)
    };
    unit.map(|v| ftol(f64::from(v) * SPARK_SPEED) as u16)
}

#[cfg(test)]
#[path = "shot32_tests.rs"]
mod tests;
