//! The vehicle's weapons (`004171A8..004174DF`, offline), run by the update
//! once the weapon switch has finished, and the fire direction `0041FF20`.
//!
//! The primary fires while control `+1C` holds its time and the selected
//! weapon's entry is ready. A live type-2 target takes a predictive shot
//! (`0048D340`) at its point; otherwise the shot is queued along the fire
//! direction (`0048D0D0`). The secondary either charges (`+20` adds to
//! `+218`), is thrown at once (`+35`) along the forward row, or releases
//! its charge past 200 ms as a lob. The network branch at `00416E34`
//! (network mode 4 and its launcher `00403940`) is not part of this offline
//! update.

use inf_formats::native_basis::{isqrt_48efc0, normalize_words, NativeLengthPrecision};

use super::drive::VehicleDriveError;
use super::update::{VehicleUpdateHost, VehicleUpdateRequest};
use crate::object_runtime::{
    ActorWeaponFireMode, ActorWeaponFireRequest, NativeHeapRef, ObjectArena, ObjectError, ObjectId,
    PlayerControlBlock, RACERS,
};

/// `004171D5`: an entry whose `+1C` reaches 6 s cannot fire.
const ENTRY_LIMIT: u32 = 6_000_000;
/// `005205B0`: direction words are the forward row times 4096.
const DIRECTION_SCALE: f64 = 4096.0;
/// `00417425`: a charge must pass 200 ms before it lobs.
const CHARGE_THRESHOLD: u32 = 200_000;
/// `00417437`: past 800 ms more the lob has no speed fraction.
const CHARGE_RANGE: u32 = 800_000;
/// `00417492`: the lob rises by 2000 except in mode 4.
const LOB_RISE: i16 = 2000;
/// `00417414`: the quick throw's speed fraction.
const QUICK_FRACTION: u32 = 0xffff_d000;

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

/// `00420000..0042002A` and `004172AA..004172DE`: the offset's length. A
/// square with its sign bit set is quartered before the root and the root
/// doubled.
fn fire_distance(delta: [i16; 3]) -> u32 {
    let [x, y, z] = delta.map(i32::from);
    let squared = z
        .wrapping_mul(z)
        .wrapping_add(x.wrapping_mul(x))
        .wrapping_add(y.wrapping_mul(y)) as u32;
    if squared & 0x8000_0000 != 0 {
        isqrt_48efc0((squared >> 2) as i32) << 1
    } else {
        isqrt_48efc0(squared as i32)
    }
}

impl ObjectArena {
    /// `004171A8..004174DF`.
    pub(super) fn vehicle_fire<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        // 004171E4 and 00417382: in a race the weapons wait for this
        // machine's racer to run, the primary for its first 5 seconds, the
        // secondary for the first 5 of each minute.
        let race = match self.race_rank(&request.drive)? {
            Some(_) => {
                let race = self.race().expect("a ranked race");
                let slot = request
                    .drive
                    .network
                    .map_or(1, |network| network.local_slot);
                let record = self.race_state(race)?.records[usize::from(slot) % RACERS];
                Some((record.flags_20 & 1 != 0, record.total_06))
            }
            None => None,
        };
        if controls.fire_20 != 0 {
            let primary = self.vehicle_behavior_ref(vehicle)?.byte_10;
            let held = race.is_some_and(|(racing, total)| !racing || total < 100);
            if self.vehicle_weapon_ready(vehicle, primary)? && !held {
                self.fire_primary(vehicle, controls.fire_20 as u32, request, host)?;
            }
        }
        // 0041732E: the secondary charges, throws or lobs.
        if controls.grenade_24 != 0 {
            let state = self.selector2_actor_mut(vehicle)?;
            state.dwords_210_218[2] =
                state.dwords_210_218[2].wrapping_add(controls.grenade_24 as u32);
            return Ok(());
        }
        let quick = controls.byte(0x39) != 0;
        if !quick && self.selector2_actor(vehicle)?.dwords_210_218[2] == 0 {
            return Ok(());
        }
        let secondary = self.vehicle_behavior_ref(vehicle)?.byte_11;
        let held = race.is_some_and(|(racing, total)| !racing || (i32::from(total) / 20) % 60 < 5);
        if self.vehicle_weapon_ready(vehicle, secondary)? && !held {
            // +12 holds the primary while the secondary fires as +10.
            let primary = self.vehicle_behavior_ref(vehicle)?.byte_10;
            self.vehicle_behavior_slot(vehicle)?.byte_10 = secondary;
            let fired = self.fire_secondary(vehicle, quick, host);
            self.vehicle_behavior_slot(vehicle)?.byte_10 = primary;
            fired?;
        }
        self.selector2_actor_mut(vehicle)?.dwords_210_218[2] = 0;
        controls.set_byte(0x39, 0);
        Ok(())
    }

    /// `004171B7..004171E2`: an entry fires with no `+14` cooldown, a
    /// nonzero `+00` count and `+1C` below 6 s.
    fn vehicle_weapon_ready(&self, vehicle: ObjectId, index: u8) -> Result<bool, ObjectError> {
        let behavior = self.vehicle_behavior_ref(vehicle)?;
        let entry = behavior
            .entries_18
            .get(index as i8 as usize)
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        Ok(entry[5] == 0 && entry[0] != 0 && entry[7] < ENTRY_LIMIT)
    }

    /// `0041723B..0041732E`.
    fn fire_primary<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        time: u32,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let direction =
            self.vehicle_fire_direction(vehicle, request.special_view, request.settings_7c)?;
        if let Some((target, delta, distance)) =
            self.locked_fire_target(vehicle, request.settings_7c)?
        {
            return host
                .fire_weapon(
                    self,
                    vehicle,
                    ActorWeaponFireRequest {
                        target: Some(target),
                        distance,
                        world: delta,
                        microseconds: time,
                        mode: ActorWeaponFireMode::Predictive { publish_lead: true },
                    },
                )
                .map_err(VehicleDriveError::Host);
        }
        // Under the special view 0041FF20 writes nothing, and 00417312 would
        // normalize the stack's residue.
        let direction = direction.ok_or(VehicleDriveError::UnknownField {
            vehicle,
            offset: 0x148,
            address: 0x0041_7312,
        })?;
        let normal = normalize_words(direction, NativeLengthPrecision::Retained);
        host.queue_weapon(self, vehicle, time, normal.map(f32::to_bits), 0)
            .map_err(VehicleDriveError::Host)
    }

    /// `00417247..004172DE`: a type-2 target whose refresh (virtual `+18`)
    /// answers above 1 yields its point's offset (virtual `+1C`) and length.
    fn locked_fire_target<E>(
        &mut self,
        vehicle: ObjectId,
        settings_7c: u32,
    ) -> Result<Option<(NativeHeapRef, [i16; 3], u32)>, VehicleDriveError<E>> {
        let Some(reference) = self.derived_actor(vehicle)?.target_148 else {
            return Ok(None);
        };
        if self.native_target(reference)?.kind_word() != 2 {
            return Ok(None);
        }
        let mut target = self.native_target(reference)?;
        let refreshed = self.refresh_native_target(&mut target, settings_7c);
        self.write_native_target(reference, target)?;
        if (refreshed? as i32) <= 1 {
            return Ok(None);
        }
        let delta = self.offset_to_target_point(vehicle, reference, 0x0041_7275)?;
        Ok(Some((reference, delta, fire_distance(delta))))
    }

    /// The target's point (virtual `+1C`) less the vehicle's position.
    fn offset_to_target_point<E>(
        &self,
        vehicle: ObjectId,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<[i16; 3], VehicleDriveError<E>> {
        let point = self.native_target(reference)?.point_words().ok_or(
            VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x148,
                address,
            },
        )?;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        Ok(std::array::from_fn(|axis| {
            point[axis].wrapping_sub(position[axis]) as i16
        }))
    }

    /// `0041FF20`: the fire direction and the behavior's `+20` lead. Under
    /// the special view (`G+5C5`) the target is freed, the lead cleared and
    /// no direction written. Without a target the direction is the forward
    /// row times 4096 (truncated). Otherwise the target is refreshed and the
    /// direction is its point less the hull; a type-2 target whose refresh
    /// answers above 1 leads by `(distance << 10) / speed` (the selected
    /// weapon's `+02`), at most `0xFFF`, times 5/4.
    pub(super) fn vehicle_fire_direction<E>(
        &mut self,
        vehicle: ObjectId,
        special_view: bool,
        settings_7c: u32,
    ) -> Result<Option<[i16; 3]>, VehicleDriveError<E>> {
        if special_view {
            if let Some(reference) = self.derived_actor_mut(vehicle)?.target_148.take() {
                self.release_actor_service(reference)?;
            }
            self.vehicle_behavior_slot(vehicle)?.word_20 = 0;
            return Ok(None);
        }
        let Some(reference) = self.derived_actor(vehicle)?.target_148 else {
            let basis = self.actor(vehicle)?.rotating.basis;
            let direction = std::array::from_fn(|axis| {
                ftol(f64::from(f32::from_bits(basis.words[7 + axis])) * DIRECTION_SCALE) as i16
            });
            self.vehicle_behavior_slot(vehicle)?.word_20 = 0;
            return Ok(Some(direction));
        };
        let mut target = self.native_target(reference)?;
        let refreshed = self.refresh_native_target(&mut target, settings_7c);
        self.write_native_target(reference, target)?;
        let refreshed = refreshed? as i32;
        let delta = self.offset_to_target_point(vehicle, reference, 0x0041_ffc3)?;
        let distance = fire_distance(delta);
        let mut lead = 0;
        if refreshed > 1 && self.native_target(reference)?.kind_word() == 2 {
            let speed = i32::from((self.vehicle_weapon_dword(vehicle, 0)? >> 16) as i16);
            let quotient = (distance.wrapping_shl(10) as i32)
                .checked_div(speed)
                .ok_or(VehicleDriveError::Division {
                    address: 0x0042_0057,
                })?;
            lead = quotient.min(0xfff).wrapping_mul(5) >> 2;
        }
        self.vehicle_behavior_slot(vehicle)?.word_20 = lead as u32;
        Ok(Some(delta))
    }

    /// `0041FF20` as the HUD's sight calls it (`0041CE7A`), with no host.
    pub(crate) fn vehicle_sight_direction(
        &mut self,
        vehicle: ObjectId,
        special_view: bool,
        settings_7c: u32,
    ) -> Result<Option<[i16; 3]>, VehicleDriveError<std::convert::Infallible>> {
        self.vehicle_fire_direction(vehicle, special_view, settings_7c)
    }

    /// `004173F8..004174C5`, with the secondary selected as `+10`.
    fn fire_secondary<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        quick: bool,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let basis = self.actor(vehicle)?.rotating.basis;
        if quick {
            let forward = [basis.words[7], basis.words[8], basis.words[9]];
            return host
                .queue_weapon(self, vehicle, 1, forward, QUICK_FRACTION)
                .map_err(VehicleDriveError::Host);
        }
        let charge = self.selector2_actor(vehicle)?.dwords_210_218[2];
        if charge <= CHARGE_THRESHOLD {
            return Ok(());
        }
        let left = charge - CHARGE_THRESHOLD;
        self.selector2_actor_mut(vehicle)?.dwords_210_218[2] = left;
        let fraction = if left < CHARGE_RANGE {
            (left >> 4) + 0x4000
        } else {
            0
        };
        let mut direction: [i16; 3] = std::array::from_fn(|axis| {
            ftol(f64::from(f32::from_bits(basis.words[7 + axis])) * DIRECTION_SCALE) as i16
        });
        if self.selector2_actor(vehicle)?.dwords_1e8_1f4[0] != 4 {
            direction[1] = direction[1].wrapping_add(LOB_RISE);
        }
        let normal = normalize_words(direction, NativeLengthPrecision::Retained);
        host.queue_weapon(self, vehicle, 1, normal.map(f32::to_bits), fraction)
            .map_err(VehicleDriveError::Host)
    }

    fn vehicle_behavior_ref(
        &self,
        vehicle: ObjectId,
    ) -> Result<&crate::object_runtime::actor::behavior::ActorBehaviorState, ObjectError> {
        self.derived_actor(vehicle)?
            .behavior_140
            .as_ref()
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))
    }

    fn vehicle_behavior_slot(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<&mut crate::object_runtime::actor::behavior::ActorBehaviorState, ObjectError> {
        self.derived_actor_mut(vehicle)?
            .behavior_140
            .as_mut()
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))
    }
}

#[cfg(test)]
#[path = "fire_tests.rs"]
mod tests;
