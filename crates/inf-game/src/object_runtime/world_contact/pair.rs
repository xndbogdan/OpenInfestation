//! 0047B610 after its pair query: virtual +A0 on both actors, the momentum
//! exchange, effect 0048AB10, virtual +A4 on both, then damage both ways.
//! Position and velocity WORDs change without dirty bits.

use thiserror::Error;

use super::super::{
    native_trunc_low32 as ftol, ActorDamagePacket, ActorDamageRequest, CollisionContact,
    ObjectArena, ObjectError, ObjectId,
};

/// Services of one reached actor pair.
pub trait ActorPairHost {
    type Error;
    /// Virtual +A0(other). A nonzero AL from either actor ends the kernel
    /// after both calls, before any physics.
    fn accept_pair(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
    ) -> Result<bool, Self::Error>;
    /// Virtual +A4(other, &own_saved, &other_saved), with the velocities
    /// both actors had before the exchange.
    fn react_pair(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
        own_saved: [u16; 3],
        other_saved: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// 0048AB10 with ECX 0, the contact midpoint and the impact strength.
    fn impact_effect(&mut self, position: [u16; 3], strength: i32) -> Result<(), Self::Error>;
    /// 0047C6E0.
    fn actor_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ActorPairError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("pair contact left DWORD +{offset:02X} unwritten at 0047B610")]
    RequiredContact { offset: u8 },
    /// Native IDIV faults on a zero divisor.
    #[error("0047B610 divides by a zero {divisor} sum for {a:?}/{b:?}")]
    ZeroDivisor {
        a: ObjectId,
        b: ObjectId,
        divisor: &'static str,
    },
    #[error("actor pair service failed: {0}")]
    Host(E),
}

/// Which of 0047B610's three momentum arms the pair takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    /// B static: A rebounds from its own velocity.
    StaticB,
    /// Both dynamic: mass-weighted exchange along the center line.
    Exchange,
    /// A static, or A over 16 times heavier with +C4 >= 3: B rebounds,
    /// from the relative velocity only in the heavy case.
    BounceB { relative: bool },
}

impl ObjectArena {
    /// 0047B610 from 0047B7F7, for a nonzero pair contact. Returns AL 1.
    pub fn respond_actor_pair<H: ActorPairHost>(
        &mut self,
        a: ObjectId,
        b: ObjectId,
        contact: CollisionContact,
        host: &mut H,
    ) -> Result<u8, ActorPairError<H::Error>> {
        let accept_a = host.accept_pair(self, a, b).map_err(ActorPairError::Host)?;
        let accept_b = host.accept_pair(self, b, a).map_err(ActorPairError::Host)?;
        if accept_a || accept_b {
            return Ok(1);
        }
        let mut normal = [0.0; 3];
        for (component, bits) in contact.normal_bits().into_iter().enumerate() {
            let bits = bits.ok_or(ActorPairError::RequiredContact {
                offset: 4 + 4 * component as u8,
            })?;
            normal[component] = f64::from(f32::from_bits(bits));
        }
        let displacement = contact
            .displacement()
            .ok_or(ActorPairError::RequiredContact { offset: 0x10 })?;
        let mass_a = i32::from(self.actor(a)?.word_8c);
        let mass_b = i32::from(self.actor(b)?.word_8c);
        let saved_a = self.actor(a)?.words_8e_92;
        let saved_b = self.actor(b)?.words_8e_92;
        let static_a = self.object(a)?.base.flags & 8 != 0;
        let static_b = self.object(b)?.base.flags & 8 != 0;
        let arm = if static_a {
            Arm::BounceB { relative: false }
        } else if static_b {
            Arm::StaticB
        } else if mass_a * 16 < mass_b
            || mass_b * 16 >= mass_a
            // 0047B8A6: a signed JL.
            || (self.actor(a)?.word_c4 as i32) < 3
        {
            Arm::Exchange
        } else {
            Arm::BounceB { relative: true }
        };
        let mut energy_along = 0;
        match arm {
            Arm::StaticB => {
                let v = saved_a.map(|word| f64::from(word as i16));
                let along = ftol((v[0] * normal[0] + v[2] * normal[2]) + v[1] * normal[1]);
                if along < 0 {
                    let rebound = f64::from(along.wrapping_add(along >> 3) as i16);
                    let impulse = normal.map(|n| ftol(rebound * n) as u16);
                    let velocity = &mut self.actor_mut(a)?.words_8e_92;
                    for axis in 0..3 {
                        velocity[axis] = saved_a[axis].wrapping_sub(impulse[axis]);
                    }
                }
                let push = f64::from(displacement as i16);
                self.offset_position(a, normal.map(|n| ftol(push * n) as u16))?;
            }
            Arm::Exchange => {
                self.exchange_actor_pair(a, b, normal, displacement, [mass_a, mass_b])?
            }
            Arm::BounceB { relative } => {
                let own = self.actor(b)?.words_8e_92;
                let other = self.actor(a)?.words_8e_92;
                let v: [f64; 3] = std::array::from_fn(|axis| {
                    let word = if relative {
                        own[axis].wrapping_sub(other[axis])
                    } else {
                        own[axis]
                    };
                    f64::from(word as i16)
                });
                let along = ftol((v[0] * normal[0] + v[2] * normal[2]) + v[1] * normal[1]);
                energy_along = along;
                if along > 0 {
                    let rebound = f64::from(along.wrapping_add(along >> 3) as i16);
                    let impulse = normal.map(|n| ftol(rebound * n) as u16);
                    let velocity = &mut self.actor_mut(b)?.words_8e_92;
                    for axis in 0..3 {
                        velocity[axis] = own[axis].wrapping_sub(impulse[axis]);
                    }
                }
                let push = f64::from(displacement as i16);
                self.offset_position(b, normal.map(|n| (ftol(push * n) as u16).wrapping_neg()))?;
            }
        }
        // 0047BE21: the impact effect uses the bounce arm's closing term.
        let energy = (mass_a + mass_b).wrapping_mul(energy_along).wrapping_abs();
        if energy > 80_000 {
            let radius = |actor| -> Result<i32, ObjectError> {
                Ok(i32::from(self.actor(actor)?.rotating.visual.radius_word))
            };
            let (radius_a, radius_b) = (radius(a)?, radius(b)?);
            let sum = radius_a + radius_b;
            if sum == 0 {
                return Err(ActorPairError::ZeroDivisor {
                    a,
                    b,
                    divisor: "radius",
                });
            }
            let pa = self.position(a)?;
            let pb = self.position(b)?;
            let middle = std::array::from_fn(|axis| {
                let span = i32::from(pb[axis] as i16) - i32::from(pa[axis] as i16);
                ((span.wrapping_mul(radius_a) / sum) as u16).wrapping_add(pa[axis])
            });
            // 0047BEB7..CB: division by 13333 through 13A94A67 and SAR 10.
            let strength = ((i64::from(energy) * 0x13a9_4a67) >> 42) as i32;
            host.impact_effect(middle, strength)
                .map_err(ActorPairError::Host)?;
        }
        host.react_pair(self, a, b, saved_a, saved_b)
            .map_err(ActorPairError::Host)?;
        host.react_pair(self, b, a, saved_b, saved_a)
            .map_err(ActorPairError::Host)?;
        for (actor, other, saved, own_static, other_static) in [
            (a, b, saved_a, static_a, static_b),
            (b, a, saved_b, static_b, static_a),
        ] {
            if self.object(actor)?.base.flags & 0x8000_0000 != 0 || own_static {
                continue;
            }
            // Byte 3 is whatever the reused stack DWORD held; left unknown.
            let packet = ActorDamagePacket {
                strength: self.velocity_change_strength(actor, saved)?,
                channel: 1,
                byte_03: None,
            };
            let source = self.actor(other)?.handle;
            host.actor_damage(self, actor, ActorDamageRequest { packet, source })
                .map_err(ActorPairError::Host)?;
            if other_static {
                let source = self.actor(actor)?.handle;
                host.actor_damage(self, other, ActorDamageRequest { packet, source })
                    .map_err(ActorPairError::Host)?;
            }
        }
        Ok(1)
    }

    /// 0047B9BB..0047BCE0: center-of-mass velocity exchange while closing
    /// along the center line, then separation by each partner's mass share.
    fn exchange_actor_pair<E>(
        &mut self,
        a: ObjectId,
        b: ObjectId,
        normal: [f64; 3],
        displacement: i32,
        [mass_a, mass_b]: [i32; 2],
    ) -> Result<(), ActorPairError<E>> {
        let sum = mass_a + mass_b;
        let zero = || ActorPairError::ZeroDivisor {
            a,
            b,
            divisor: "mass",
        };
        if sum == 0 {
            return Err(zero());
        }
        let va = self.actor(a)?.words_8e_92;
        let vb = self.actor(b)?.words_8e_92;
        let pa = self.position(a)?;
        let pb = self.position(b)?;
        let relative: [f64; 3] =
            std::array::from_fn(|axis| f64::from(va[axis].wrapping_sub(vb[axis]) as i16));
        let offset: [i32; 3] =
            std::array::from_fn(|axis| i32::from(pa[axis].wrapping_sub(pb[axis]) as i16));
        // 0047BA21..55: Y and Z spill through binary32; X stays in x87.
        let x = f64::from(offset[0]);
        let y = f64::from(offset[1] as f32);
        let z = f64::from(offset[2] as f32);
        let length = ((z * z + y * y) + x * x).sqrt();
        let spilled = f64::from(length as f32);
        let unit = if length == 0.0 {
            [1.0, 0.0, 0.0]
        } else {
            [x / spilled, y / spilled, z / spilled]
        };
        let closing = ftol((relative[2] * unit[2] + relative[1] * unit[1]) + relative[0] * unit[0]);
        if closing < 0 {
            for axis in 0..3 {
                let own = i32::from(self.actor(a)?.words_8e_92[axis] as i16);
                let theirs = i32::from(self.actor(b)?.words_8e_92[axis] as i16);
                let center = own
                    .wrapping_mul(mass_a)
                    .wrapping_add(theirs.wrapping_mul(mass_b))
                    / sum;
                let half = own.wrapping_sub(theirs) >> 1;
                let (next_a, next_b) = if half == 0 {
                    (center, center)
                } else {
                    (
                        center.wrapping_sub(half.wrapping_mul(mass_b) / sum),
                        (half.wrapping_mul(mass_a) / sum).wrapping_add(center),
                    )
                };
                self.actor_mut(a)?.words_8e_92[axis] = next_a as u16;
                self.actor_mut(b)?.words_8e_92[axis] = next_b as u16;
            }
        }
        // 0047BC1D..33: each actor moves by its partner's mass share plus one.
        for (actor, share, sign) in [
            (a, displacement.wrapping_mul(mass_b) / sum, 1u16),
            (b, displacement.wrapping_mul(mass_a) / sum, u16::MAX),
        ] {
            let step = f64::from(share.wrapping_add(1) as i16);
            let offset = normal.map(|n| (ftol(step * n) as u16).wrapping_mul(sign));
            self.offset_position(actor, offset)?;
            let velocity = &mut self.actor_mut(actor)?.words_8e_92;
            for axis in 0..3 {
                velocity[axis] = velocity[axis].wrapping_add(offset[axis]);
            }
        }
        Ok(())
    }

    pub(in crate::object_runtime) fn position(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }

    fn offset_position(&mut self, actor: ObjectId, offset: [u16; 3]) -> Result<(), ObjectError> {
        let base = &mut self.owned_mut(actor)?.state.base;
        let mut position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        for (word, offset) in position.iter_mut().zip(offset) {
            *word = word.wrapping_add(offset);
        }
        base.position_words = Some(position);
        Ok(())
    }
}

/// 0047B610's admission and pair query, with the kernel's own services.
pub trait WorldPairHost: ActorPairHost {
    /// 0047B66F..0047B7EC: both H8 headers +0E nonzero, then 004F2BE0 over
    /// A's context at `delta` and B's at zero. None is a zero header or a
    /// zero contact flag; both return AL 0.
    fn pair_contact(
        &mut self,
        arena: &ObjectArena,
        a: ObjectId,
        b: ObjectId,
        delta: [i16; 3],
    ) -> Result<Option<CollisionContact>, Self::Error>;
}

impl ObjectArena {
    /// 004790C0 over the G+48 list. A failed pair is reported and skipped;
    /// the walk continues with the next partner.
    pub fn run_world_pair_pass<H: WorldPairHost>(
        &mut self,
        host: &mut H,
        mut admitted: impl FnMut(ObjectId, ObjectId) -> bool,
        mut stopped: impl FnMut(ObjectId, ObjectId, ActorPairError<H::Error>),
    ) {
        let actors = self.actor_list().to_vec();
        for (index, &a) in actors.iter().enumerate() {
            let Ok(object) = self.object(a) else { continue };
            let mut flags_a = object.base.flags;
            if flags_a & 4 == 0 {
                continue;
            }
            for &b in &actors[index + 1..] {
                let Ok(object) = self.object(b) else { continue };
                let flags_b = object.base.flags;
                if flags_b & 4 == 0 || flags_b & flags_a & 8 != 0 || !admitted(a, b) {
                    continue;
                }
                match self.pair_contact_step(a, b, host) {
                    Ok(0) => {}
                    Ok(_) => {
                        // 004791A6 rereads A; losing flag 4 ends its partners.
                        flags_a = self.object(a).map_or(0, |object| object.base.flags);
                        if flags_a & 4 == 0 {
                            break;
                        }
                    }
                    Err(error) => stopped(a, b, error),
                }
            }
        }
    }

    /// One 004790C0 candidate: the broadphase, then 0047B610.
    fn pair_contact_step<H: WorldPairHost>(
        &mut self,
        a: ObjectId,
        b: ObjectId,
        host: &mut H,
    ) -> Result<u8, ActorPairError<H::Error>> {
        let pa = self.position(a)?;
        let pb = self.position(b)?;
        let delta: [i16; 3] = std::array::from_fn(|axis| pa[axis].wrapping_sub(pb[axis]) as i16);
        let [x, y, z] = delta.map(i32::from);
        // 00479153..64: an unsigned compare of the wrapped i32 square sum.
        let square = z.wrapping_mul(z).wrapping_add(x * x).wrapping_add(y * y) as u32;
        let radius = u32::from(self.actor(a)?.rotating.visual.radius_word)
            + u32::from(self.actor(b)?.rotating.visual.radius_word);
        if square >= radius.wrapping_mul(radius) {
            return Ok(0);
        }
        let Some(contact) = host
            .pair_contact(self, a, b, delta)
            .map_err(ActorPairError::Host)?
        else {
            return Ok(0);
        };
        self.respond_actor_pair(a, b, contact, host)
    }
}
