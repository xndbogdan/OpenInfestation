//! The Tractor Beam (upgrade 0x36), the vehicle's tow `00417E40`, run while
//! byte +1BC is set. With no tow (+1B4 the process NULL) and the beam owned,
//! the pickup key (block +37) looks for a Powerball (resource 0x27, selector
//! 57) within 600 across X/Z (scanner B, flags 0x20 set and 0x80 clear, the
//! filter `00418410`, the nearest winning) and holds it: +1B4 its handle,
//! +1B8 its distance, the tow's length. The drop key (block +38) lets go.
//!
//! While it holds the ball, the length eases to 400 by `(delta >> 5) * 400
//! >> 15` a tick and the ball leaves this pass of the update list
//! (`00479040`). When the line is longer than the length or shorter than
//! half of it, both ends move onto it about their centre of mass, each
//! weighed by the other's mass (the ball's share is its mass plus a third
//! of the speed between them), and each takes the velocity of that move
//! (`0047DDF0`). Then a line of short smoke (`00489BC0`, type 1 while it
//! pulls, 0 while slack) runs from the ball to the vehicle, one particle
//! every 30 or so.

use super::drive::VehicleDriveError;
use crate::object_runtime::actor::behavior::normalize_417da0;
use crate::object_runtime::actor::derived::kinematics::ftol;
use crate::object_runtime::actor::derived::DerivedActorError;
use crate::object_runtime::{
    ActorHandle, ObjectArena, ObjectError, ObjectId, PlayerControlBlock, PlayerId,
};
use crate::world_radius_effect::radius_word_distance;

/// `00417E7A`: the Tractor Beam's upgrade.
pub const TRACTOR_UPGRADE: u32 = 0x36;
/// `00418414`: the filter scores only the Powerball's resource.
const TOW_RESOURCE: u16 = 0x27;
/// `00417EA5..00417EB5`: scanner B's flags and its squared X/Z range, 600.
const TOW_REQUIRED: u32 = 0x20;
const TOW_EXCLUDED: u32 = 0x80;
const TOW_RANGE_SQUARED: u32 = 360_000;
/// `00417FFC`: the length the tow eases to.
const TOW_LENGTH: i32 = 400;
/// `0041833E`: one beam particle every thirtieth of the line.
const BEAM_SPACING: i32 = 30;
/// Block +37 (pickup) and +38 (drop), as the player's bytes +3B and +3C.
const PICKUP_KEY: usize = 0x3b;
const DROP_KEY: usize = 0x3c;

impl ObjectArena {
    /// `00417E40(delta, block)`; `beam` births each particle of the line.
    pub(super) fn vehicle_tow<E>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        player: Option<PlayerId>,
        null: ActorHandle,
        delta: u32,
        beam: &mut impl FnMut(u32, [u16; 3]) -> Result<(), E>,
    ) -> Result<(), VehicleDriveError<E>> {
        let towed = self.vehicle_towed(vehicle)?;
        if towed == null.raw() {
            let Some(player) = player else {
                return Ok(());
            };
            if !self.player_upgrade_owned(player, TRACTOR_UPGRADE)?
                || controls.byte(PICKUP_KEY) == 0
            {
                return Ok(());
            }
            let Some(ball) = self.tow_scan(vehicle)? else {
                return Ok(());
            };
            let handle = self.actor(ball)?.handle.raw();
            let length = radius_word_distance(self.tow_line(vehicle, ball)?);
            self.set_vehicle_tow(vehicle, handle, length)?;
            controls.set_byte(PICKUP_KEY, 0);
            return Ok(());
        }
        if controls.byte(DROP_KEY) != 0 {
            let length = self.vehicle_tow_length(vehicle)?;
            self.set_vehicle_tow(vehicle, null.raw(), length)?;
            controls.set_byte(DROP_KEY, 0);
            return Ok(());
        }
        let Some(ball) = self.resolve_actor_handle(ActorHandle::from_raw(towed))? else {
            let length = self.vehicle_tow_length(vehicle)?;
            self.set_vehicle_tow(vehicle, null.raw(), length)?;
            return Ok(());
        };
        if self.object(ball)?.base.flags & 2 == 0 {
            return Ok(());
        }
        let step = (delta >> 5) as i32;
        let vehicle_mass = i32::from(self.actor(vehicle)?.word_8c);
        let speed: [i16; 3] = {
            let (own, theirs) = (
                self.actor(vehicle)?.words_8e_92,
                self.actor(ball)?.words_8e_92,
            );
            std::array::from_fn(|axis| own[axis].wrapping_sub(theirs[axis]) as i16)
        };
        let share = (radius_word_distance(speed) as i32 / 3)
            .wrapping_add(i32::from(self.actor(ball)?.word_8c));
        let total = vehicle_mass.wrapping_add(share);
        let mut length = self.vehicle_tow_length(vehicle)? as i32;
        if length != TOW_LENGTH {
            let ease = (step.wrapping_mul(TOW_LENGTH) >> 15) as i16 as i32;
            length = if length < TOW_LENGTH {
                length.wrapping_add(ease).min(TOW_LENGTH)
            } else {
                length.wrapping_sub(ease).max(TOW_LENGTH)
            };
            self.set_vehicle_tow(vehicle, towed, length as u32)?;
        }
        self.append_flag1_alternate(ball)?;
        let line = self.tow_line(vehicle, ball)?;
        let distance = radius_word_distance(line) as i32;
        let direction = normalize_417da0(line);
        let (reach, pulling) = if distance > length {
            (length, true)
        } else if distance >= length / 2 {
            (distance, false)
        } else {
            (length / 2, true)
        };
        if pulling {
            let divide = |value: i32, divisor: i32| {
                value
                    .checked_div(divisor)
                    .ok_or(VehicleDriveError::Unported {
                        vehicle,
                        address: 0x0041_8135,
                    })
            };
            let own = self.vehicle_tow_position(vehicle)?;
            let mut centre = [0_u16; 3];
            for axis in 0..3 {
                let moved = divide(i32::from(line[axis]).wrapping_mul(share), total)?;
                centre[axis] = own[axis].wrapping_sub(moved as u16);
            }
            let along = |amount: i32| -> [u16; 3] {
                direction.map(|unit| ftol(f64::from(amount) * f64::from(unit)) as u16)
            };
            let own_reach = divide(reach.wrapping_mul(share), total)? as i16 as i32;
            let offset = along(own_reach);
            let own_point: [u16; 3] =
                std::array::from_fn(|axis| centre[axis].wrapping_add(offset[axis]));
            self.tow_move(vehicle, own_point, step)?;
            let ball_reach = divide(reach.wrapping_mul(vehicle_mass), total)? as i16 as i32;
            let offset = along(ball_reach);
            let ball_point: [u16; 3] =
                std::array::from_fn(|axis| centre[axis].wrapping_sub(offset[axis]));
            self.tow_move(ball, ball_point, step)?;
        }
        let count = reach / BEAM_SPACING + 1;
        if count < 0 {
            return Ok(());
        }
        let origin = self.vehicle_tow_position(ball)?;
        let mut along = 0_i32;
        for _ in 0..=count {
            let t = along
                .checked_div(count)
                .ok_or(VehicleDriveError::Unported {
                    vehicle,
                    address: 0x0041_8371,
                })? as i16;
            let point: [u16; 3] = std::array::from_fn(|axis| {
                origin[axis].wrapping_add(ftol(f64::from(t) * f64::from(direction[axis])) as u16)
            });
            beam(u32::from(pulling), point).map_err(VehicleDriveError::Host)?;
            along = along.wrapping_add(reach);
        }
        Ok(())
    }

    /// Scanner B `00475F50` from the vehicle with the tow's filter `00418410`:
    /// a Powerball scores `-1 - n` for its squared X/Z distance `n`.
    fn tow_scan(&self, vehicle: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        let origin = self.vehicle_tow_position(vehicle)?;
        let mut best: Option<(ObjectId, u32)> = None;
        for &candidate in self.actor_list() {
            let base = &self.object(candidate)?.base;
            if base.flags & TOW_EXCLUDED != 0 || base.flags & TOW_REQUIRED != TOW_REQUIRED {
                continue;
            }
            let other = self.vehicle_tow_position(candidate)?;
            let dx = i32::from(origin[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(origin[2].wrapping_sub(other[2]) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= TOW_RANGE_SQUARED || candidate == vehicle {
                continue;
            }
            let score = if base.resource_word == TOW_RESOURCE {
                !squared
            } else {
                0
            };
            if score > best.map_or(0, |(_, best)| best) {
                best = Some((candidate, score));
            }
        }
        Ok(best.map(|(actor, _)| actor))
    }

    /// The vehicle's position less the ball's, in WORDs.
    fn tow_line(&self, vehicle: ObjectId, ball: ObjectId) -> Result<[i16; 3], ObjectError> {
        let (own, theirs) = (
            self.vehicle_tow_position(vehicle)?,
            self.vehicle_tow_position(ball)?,
        );
        Ok(std::array::from_fn(|axis| {
            own[axis].wrapping_sub(theirs[axis]) as i16
        }))
    }

    fn vehicle_tow_position(&self, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }

    /// `0047DDF0`: the actor takes `point` and the velocity that moves it
    /// there in `step` (`(move << 15) / step` added to its own).
    fn tow_move<E>(
        &mut self,
        actor: ObjectId,
        point: [u16; 3],
        step: i32,
    ) -> Result<(), VehicleDriveError<E>> {
        let from = self.vehicle_tow_position(actor)?;
        let mut velocity = self.actor(actor)?.words_8e_92;
        for axis in 0..3 {
            let moved = i32::from(point[axis].wrapping_sub(from[axis]) as i16) << 15;
            let added = moved.checked_div(step).ok_or(VehicleDriveError::Unported {
                vehicle: actor,
                address: 0x0041_8208,
            })?;
            velocity[axis] = velocity[axis].wrapping_add(added as u16);
        }
        self.owned_mut(actor)?.state.base.position_words = Some(point);
        self.actor_mut(actor)?.words_8e_92 = velocity;
        Ok(())
    }

    /// DWORD +1B4, the towed actor's handle.
    fn vehicle_towed(&self, vehicle: ObjectId) -> Result<u32, DerivedActorError> {
        let bytes = &self.selector2_actor(vehicle)?.bytes_1b4_1bf;
        Ok(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
    }

    /// DWORD +1B8, the tow's length.
    fn vehicle_tow_length(&self, vehicle: ObjectId) -> Result<u32, DerivedActorError> {
        let bytes = &self.selector2_actor(vehicle)?.bytes_1b4_1bf;
        Ok(u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
    }

    fn set_vehicle_tow(
        &mut self,
        vehicle: ObjectId,
        handle: u32,
        length: u32,
    ) -> Result<(), DerivedActorError> {
        let bytes = &mut self.selector2_actor_mut(vehicle)?.bytes_1b4_1bf;
        bytes[..4].copy_from_slice(&handle.to_le_bytes());
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        Ok(())
    }
}

#[cfg(test)]
#[path = "tow_tests.rs"]
mod tests;
