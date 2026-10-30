//! Selector 27, the row-12 charge: 00455560's class with final table
//! 00525848, built from resource 121 (the ACAM's weapon row 12: a
//! 5000-strength channel-9 block, payload flag 8). It flies as a grenade
//! does until it meets an actor of another team. Its pair acceptance (+A0,
//! 00456010) then sticks it to that actor: the actor's handle goes to +154
//! and the charge's offset, in the actor's frame, to +158..+15C. Its update
//! (+24, 00455F30) holds the fuse at a second or more until something marks
//! it to detonate (+14E): the detonator's event 10, event 9, lethal damage
//! (state 1) or a lost target. The fuse then drops to 0 and the grenade's
//! 004555B0 sets it off. Stuck, it takes the actor's position and shows
//! while the actor does. Its draw (+2C, 00456180) puts it back on the
//! actor's surface, facing out, before the common 0047D910. Its destructor
//! (004562F0) tells its thrower 0x12. The rest of its table is the
//! grenade's.

use std::convert::Infallible;

use inf_formats::native_basis::{normalize_words, orient_from_forward, NativeLengthPrecision};
use thiserror::Error;

use super::grenade26::{Grenade26Error, Grenade26Host};
use super::Family455560Tail;
use crate::object_runtime::{
    mark_flag_change, ActorHandle, ActorMessage, ActorMessageReply, ObjectArena, ObjectError,
    ObjectId, PlayerActorLookupContext, VehicleMessageEffect,
};
use crate::process_particles::factories::ParticleFactoryProcessState;

/// 00455F3E: an unmarked charge's fuse never stays below a second.
const HELD_FUSE: u16 = 1000;
/// 00456017: the thrower is no contact for the charge's first 500 ms.
const OWNER_GRACE: u16 = 500;
/// 0045631B: the destructor's word to the thrower.
const OWNER_NOTICE: i32 = 0x12;

/// 00455F30's callees, the grenade's: the smoke 004558B0, the flight
/// 004559E0 and the fuse 004555B0.
pub(crate) trait Charge27Steps {
    type Error;
    fn smoke(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error>;
    fn flight(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error>;
    fn fuse(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Charge27Error<E = Infallible> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("{charge:?} is not a selector-27 charge")]
    NotCharge { charge: ObjectId },
    #[error("charge {charge:?} reads its unwritten {field}")]
    Unwritten {
        charge: ObjectId,
        field: &'static str,
    },
    #[error("charge {charge:?} resolving its target: {reason}")]
    Resolve { charge: ObjectId, reason: String },
    #[error("charge step: {0}")]
    Step(E),
}

type ChargeResult<T, E> = Result<T, Charge27Error<E>>;

/// The tail fields the charge adds to the grenade's.
struct ChargeTail<'a> {
    detonate: &'a mut u8,
    handle: &'a mut ActorHandle,
    offset: &'a mut Option<[u16; 3]>,
}

/// The grenade's own steps, with the world's services.
struct GrenadeSteps<'a, H> {
    delta: u32,
    sine_quarter: &'a [u16; 4096],
    rng: &'a mut u32,
    host: &'a mut H,
}

impl<H: Grenade26Host> Charge27Steps for GrenadeSteps<'_, H> {
    type Error = Grenade26Error<H::Error>;
    fn smoke(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error> {
        arena.grenade26_smoke(charge, self.rng, self.host)
    }
    fn flight(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error> {
        arena.grenade26_flight(charge, self.delta, self.sine_quarter, self.host)
    }
    fn fuse(&mut self, arena: &mut ObjectArena, charge: ObjectId) -> Result<(), Self::Error> {
        arena.grenade26_fuse(charge, self.delta, self.rng, self.host)
    }
}

impl ObjectArena {
    pub fn is_charge27(&self, object: ObjectId) -> bool {
        self.behavior_resource(object)
            .is_ok_and(|state| state.selector == 27)
            && self.thrown(object).is_ok()
    }

    fn charge27_tail<E>(&mut self, charge: ObjectId) -> ChargeResult<ChargeTail<'_>, E> {
        if !self.is_charge27(charge) {
            return Err(Charge27Error::NotCharge { charge });
        }
        match &mut self.thrown_mut(charge)?.tail {
            Family455560Tail::Flags14c {
                byte_14e,
                handle_154: Some(handle),
                offset_158,
                ..
            } => Ok(ChargeTail {
                detonate: byte_14e,
                handle,
                offset: offset_158,
            }),
            _ => Err(Charge27Error::NotCharge { charge }),
        }
    }

    fn charge27_resolve<E>(
        &mut self,
        charge: ObjectId,
        handle: ActorHandle,
    ) -> ChargeResult<Option<ObjectId>, E> {
        self.resolve_actor_handle(handle)
            .map_err(|error| Charge27Error::Resolve {
                charge,
                reason: error.to_string(),
            })
    }

    fn charge27_position<E>(&self, charge: ObjectId, actor: ObjectId) -> ChargeResult<[u16; 3], E> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(Charge27Error::Unwritten {
                charge,
                field: "position",
            })
    }

    /// 00455F30, the charge's +24, with the live process null `null`
    /// (0054E894).
    pub fn tick_charge27<H: Grenade26Host>(
        &mut self,
        charge: ObjectId,
        delta: u32,
        null: ActorHandle,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> ChargeResult<(), Grenade26Error<H::Error>> {
        self.update_charge27(
            charge,
            null,
            &mut GrenadeSteps {
                delta,
                sine_quarter,
                rng,
                host,
            },
        )
    }

    /// 00455F30 over its callees.
    pub(crate) fn update_charge27<S: Charge27Steps>(
        &mut self,
        charge: ObjectId,
        null: ActorHandle,
        steps: &mut S,
    ) -> ChargeResult<(), S::Error> {
        // 00455F34..00455F55: unmarked, the fuse holds at a second or
        // more; marked, it drops to 0.
        let marked = *self.charge27_tail::<S::Error>(charge)?.detonate != 0;
        let state = self.thrown_mut(charge)?;
        if marked {
            state.word_13a = 0;
        } else if state.word_13a < HELD_FUSE {
            state.word_13a = HELD_FUSE;
        }
        steps.smoke(self, charge).map_err(Charge27Error::Step)?;
        let target = *self.charge27_tail::<S::Error>(charge)?.handle;
        if target == null {
            // 00455F79: unstuck, it flies.
            steps.flight(self, charge).map_err(Charge27Error::Step)?;
            return steps.fuse(self, charge).map_err(Charge27Error::Step);
        }
        match self.charge27_resolve::<S::Error>(charge, target)? {
            // 00455FF1: its target is gone, so it is marked.
            None => *self.charge27_tail::<S::Error>(charge)?.detonate = 1,
            Some(actor) => {
                // 00455F9E..00455FDF: it takes its target's position, and
                // leaves the draw list (00479E90) while the target's flag
                // 0x80 is set, rejoining it once the flag clears.
                let position = self.charge27_position::<S::Error>(charge, actor)?;
                self.owned_mut(charge)?.state.base.position_words = Some(position);
                let hidden = self.object(actor)?.base.flags & 0x80 != 0;
                let shown = self.object(charge)?.base.flags & 1 != 0;
                if hidden == shown {
                    self.set_flag0(charge, !hidden)?;
                }
            }
        }
        steps.fuse(self, charge).map_err(Charge27Error::Step)
    }

    /// 00456010, the charge's +A0(other): AL 1 skips the pair. For its
    /// first 500 ms its thrower is no contact; an actor of its own team
    /// (+B4) is an ordinary one. Any other actor takes it: +154 gets the
    /// actor's handle, +158..+15C the charge's offset from the actor, taken
    /// into the actor's frame (00490A80 over its +48 basis, 004026B0's
    /// +44), and the charge's flags lose 0x104.
    pub fn charge27_accepts(
        &mut self,
        charge: ObjectId,
        other: ObjectId,
    ) -> ChargeResult<bool, Infallible> {
        self.charge27_tail::<Infallible>(charge)?;
        let age = self.thrown(charge)?.word_13c;
        let (owner, team) = {
            let actor = self.actor(charge)?;
            (actor.owner_handle, actor.word_b4)
        };
        let (handle, other_team, basis) = {
            let actor = self.actor(other)?;
            (actor.handle, actor.word_b4, actor.rotating.basis)
        };
        if age < OWNER_GRACE && handle == owner {
            return Ok(true);
        }
        if other_team == team {
            return Ok(false);
        }
        let own = self.charge27_position::<Infallible>(charge, charge)?;
        let theirs = self.charge27_position::<Infallible>(charge, other)?;
        let delta: [i16; 3] =
            std::array::from_fn(|axis| own[axis].wrapping_sub(theirs[axis]) as i16);
        let offset = basis.inverse_words(delta).map(|word| word as u16);
        let tail = self.charge27_tail::<Infallible>(charge)?;
        *tail.handle = handle;
        *tail.offset = Some(offset);
        // 00456091..004560B5: a change in the low nine bits marks +38, and
        // a live charge (bit 31 clear) marks +39.
        let base = &mut self.owned_mut(charge)?.state.base;
        let flags = base.flags & 0xffff_fefb;
        mark_flag_change(base, flags);
        if flags & 0x8000_0000 == 0 {
            base.dirty_word |= 0x0200;
        }
        Ok(true)
    }

    /// 00456180, the charge's +2C, before its common 0047D910, with the
    /// live process null `null`. A stuck charge returns to its target: its
    /// position is the target's plus the offset out of the target's frame
    /// (004909E0), and its basis faces along that offset (00417DA0), turned
    /// about it by the target's Z row (X when the offset has no X).
    pub fn pose_charge27(
        &mut self,
        charge: ObjectId,
        null: ActorHandle,
    ) -> ChargeResult<(), Infallible> {
        let target = *self.charge27_tail::<Infallible>(charge)?.handle;
        if target == null {
            return Ok(());
        }
        let Some(actor) = self.charge27_resolve::<Infallible>(charge, target)? else {
            // 004562C9: its target is gone, so it is marked.
            *self.charge27_tail::<Infallible>(charge)?.detonate = 1;
            return Ok(());
        };
        let offset =
            self.charge27_tail::<Infallible>(charge)?
                .offset
                .ok_or(Charge27Error::Unwritten {
                    charge,
                    field: "offset +158",
                })?;
        let basis = self.actor(actor)?.rotating.basis;
        let out = basis.transform_words(offset.map(|word| word as i16));
        let theirs = self.charge27_position::<Infallible>(charge, actor)?;
        let position = std::array::from_fn(|axis| theirs[axis].wrapping_add(out[axis]));
        self.owned_mut(charge)?.state.base.position_words = Some(position);
        let normal = normalize_words(out.map(|word| word as i16), NativeLengthPrecision::Retained);
        // 004561FF: the row is +60 (Z) when the offset's X is nonzero, +48
        // (X) otherwise; side = row cross normal, each component rounded
        // to f32.
        let row = if offset[0] != 0 { 7 } else { 1 };
        let r: [f64; 3] =
            std::array::from_fn(|axis| f64::from(f32::from_bits(basis.words[row + axis])));
        let [nx, ny, nz] = normal.map(f64::from);
        let side = [
            (nz * r[1] - ny * r[2]) as f32,
            (nx * r[2] - nz * r[0]) as f32,
            (ny * r[0] - nx * r[1]) as f32,
        ];
        orient_from_forward(
            &mut self.actor_mut(charge)?.rotating.basis,
            side,
            normal,
            false,
        );
        Ok(())
    }

    /// 004562F0's 0047DD40(+88, 0, 0x12, 0): its thrower, if its handle
    /// resolves, hears 0x12 through 0047DD80 with the squared distance
    /// between them. The vehicle's 00420BE0 answers it only online, with a
    /// grant; any reply the destruction cannot apply is a boundary.
    pub(super) fn charge27_owner_notice(
        &mut self,
        charge: ObjectId,
        process: Option<&ParticleFactoryProcessState>,
    ) -> Result<(), ObjectError> {
        let boundary = ObjectError::BehaviorResourceTeardown(charge);
        let owner = self.actor(charge)?.owner_handle;
        let Some(owner) = self.resolve_actor_handle(owner).map_err(|_| boundary)? else {
            return Ok(());
        };
        let process = process.ok_or(boundary)?;
        let position = |arena: &Self, actor| {
            arena
                .object(actor)
                .ok()
                .and_then(|object| object.base.position_words)
                .ok_or(ObjectError::BehaviorResourceTeardown(charge))
        };
        let (from, to) = (position(self, charge)?, position(self, owner)?);
        let distance_squared = (0..3).fold(0_i32, |squares, axis| {
            let delta = i32::from(from[axis].wrapping_sub(to[axis]) as i16);
            squares.wrapping_add(delta.wrapping_mul(delta))
        }) as u32;
        let reply = self
            .receive_actor_message(
                owner,
                ActorMessage {
                    sender: Some(charge),
                    distance_squared,
                    code: OWNER_NOTICE,
                    argument: 0,
                },
                PlayerActorLookupContext {
                    process: Some(process),
                },
                process.network_flag != 0,
            )
            .map_err(|_| boundary)?;
        match reply {
            ActorMessageReply::Done(_) | ActorMessageReply::Vehicle(VehicleMessageEffect::None) => {
                Ok(())
            }
            _ => Err(boundary),
        }
    }
}

#[cfg(test)]
#[path = "charge27_tests.rs"]
mod tests;
