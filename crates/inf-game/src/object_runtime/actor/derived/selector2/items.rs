//! The vehicle's hold and item controls in 00416170.
//!
//! Every update runs the hold (`00412380` from `004176FF`). A carried actor
//! whose capture (+70) still names the vehicle runs its +90 with the
//! vehicle and the delta (selector 12's `004688A0` repairs the vehicle),
//! then its status timers (`0047C990`), and takes the vehicle's position.
//! Any other is released from its slot, a dead handle is cleared, and a
//! change compacts the hold (`00411F60`). In a network game a slot holding
//! another machine's actor waits on it: taken (state 1), it is captured
//! again and message 8 tells its owner until a record names the vehicle as
//! its holder; let go (state 3), message 9 tells its owner while one still
//! does.
//!
//! The item controls are `00417731..004178ED`.
//! The drop counts the free slots (`004120E0`: those holding the process
//! NULL); with an actor carried, the slot array's +0 drops the last one
//! (`00412880`), then three one-shots of sound 2 at the vehicle close it.
//!
//! The pickup runs scanner B `00475F50` over the actor list with the
//! vehicle's filter `004155C0` (functor `00520A0C`), then `00421810` takes
//! the find into the hold.
//!
//! The scanner admits actors with flags 0x60 set and 0x80 clear, nearer
//! than the vehicle's +164 in X/Z (squared, unsigned), other than the
//! vehicle; it keeps the strictly best score. The filter refuses team 0x12,
//! anything 1500 or farther in X/Z (squared) and anything 1500 or more
//! away in height, and otherwise scores the nearest highest:
//! `0x7FFFFFFF - distance squared`.

use crate::network::protocol::GameSend;
use crate::object_runtime::{ActorHandle, FamilyUpdateHost, ObjectArena, ObjectError, ObjectId};

use super::carry::VehicleCarryContext;
use super::drive::VehicleDriveError;
use super::update::{status_error, StatusHost, VehicleUpdateHost};

/// `004688A0`: a carried scientist repairs the vehicle by the delta over
/// 2^14 µs, at least 1.
const REPAIR_SHIFT: u32 = 14;
/// The slot states: carried locally, and a remote actor's take and drop.
const SLOT_CARRIED: u32 = 2;
const SLOT_REMOTE_TAKE: u32 = 1;
const SLOT_REMOTE_DROP: u32 = 3;

/// `004177A2..004177EC`: sound 2 at gain 0x10000, falling in pitch.
const DROP_SOUND: u32 = 2;
const DROP_GAIN: u32 = 0x1_0000;
const DROP_PITCHES: [u32; 3] = [0x6aaa, 0x5555, 0x4000];

/// "Cargo hold full or beam unsuccessful", sent when `00421810` refuses.
pub const CARGO_FULL_TEXT: u16 = 0x16e;
/// The scanner's required and excluded flags.
const REQUIRED_FLAGS: u32 = 0x60;
const EXCLUDED_FLAGS: u32 = 0x80;
/// `004155C0`'s team, distance and height limits.
const ALIEN_TEAM: u32 = 0x12;
const FILTER_RANGE_SQUARED: u32 = 0x22_5510;
const FILTER_HEIGHT: i32 = 0x5dc;

impl ObjectArena {
    /// `00412380` on the vehicle's hold.
    pub(super) fn tick_vehicle_hold<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        null: ActorHandle,
        status_load_factor: Option<i32>,
        carry: Option<VehicleCarryContext<'_>>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        self.tick_hold(
            vehicle,
            delta,
            null,
            status_load_factor,
            carry,
            &mut StatusHost { host, null },
        )
    }

    /// `00412380` on `vehicle`'s hold (a vehicle's +1A4 or a Prison Unit's
    /// +144); `status` runs the carried actors' status timers, and `carry`
    /// another machine's actor's capture (none stops there).
    pub(in crate::object_runtime) fn tick_hold<S: FamilyUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        null: ActorHandle,
        status_load_factor: Option<i32>,
        carry: Option<VehicleCarryContext<'_>>,
        status: &mut S,
    ) -> Result<(), VehicleDriveError<S::Error>> {
        let source = self.actor(vehicle)?.handle;
        let count = self.carry_slots(vehicle)?.handles.len();
        let mut changed = false;
        for index in 0..count {
            let slots = self.carry_slots(vehicle)?;
            let (handle, state) = (slots.handles[index], slots.states[index]);
            if handle == null {
                continue;
            }
            let Some(actor) = self.resolve_actor_handle(handle)? else {
                self.carry_slots_mut(vehicle)?.handles[index] = null;
                changed = true;
                continue;
            };
            let captor = |arena: &Self| -> Result<ActorHandle, VehicleDriveError<S::Error>> {
                arena
                    .actor(actor)?
                    .capture_handle_70
                    .ok_or(VehicleDriveError::UnknownField {
                        vehicle: actor,
                        offset: 0x70,
                        address: 0x0041_23e9,
                    })
            };
            let kept = match state {
                Some(SLOT_CARRIED) => {
                    captor(self)? == source
                        && self.carried_actor_tick(
                            vehicle,
                            actor,
                            delta,
                            null,
                            status_load_factor,
                            carry,
                            status,
                        )?
                }
                Some(SLOT_REMOTE_TAKE) => {
                    let captor = captor(self)?;
                    if captor == null {
                        // 00412468: the actor's +8C(1, the vehicle, the
                        // slot's +8) and, taken, message 8 to its owner.
                        let context = carry.ok_or(VehicleDriveError::Unported {
                            vehicle,
                            address: 0x0041_2468,
                        })?;
                        let mut saved = self.carry_slots(vehicle)?.saved_flags[index];
                        let taken = self.capture_carried_actor::<std::convert::Infallible>(
                            actor, source, &mut saved, context,
                        );
                        self.carry_slots_mut(vehicle)?.saved_flags[index] = saved;
                        let taken = taken.map_err(|error| {
                            VehicleDriveError::Status(format!("00412479 take: {error}"))
                        })?;
                        if taken {
                            self.queue_network_send(
                                8,
                                GameSend::Pair([source.raw(), handle.raw()]),
                            );
                        }
                        taken
                    } else {
                        if captor == source {
                            self.carry_slots_mut(vehicle)?.states[index] = Some(SLOT_CARRIED);
                        }
                        captor == source
                    }
                }
                Some(SLOT_REMOTE_DROP) => {
                    // 004123F4: message 9 to its owner while it still names
                    // the vehicle; it rides on meanwhile.
                    let held = captor(self)? == source;
                    if held {
                        self.queue_network_send(9, GameSend::Pair([source.raw(), handle.raw()]));
                    }
                    held
                }
                Some(_) => false,
                None => {
                    return Err(VehicleDriveError::UnknownField {
                        vehicle,
                        offset: 0x1a4,
                        address: 0x0041_23cf,
                    })
                }
            };
            if !kept {
                let slots = self.carry_slots_mut(vehicle)?;
                slots.handles[index] = null;
                slots.states[index] = Some(0);
                changed = true;
                continue;
            }
            self.ride_with_holder::<S::Error>(vehicle, actor)?;
        }
        if changed {
            self.compact_vehicle_slots::<VehicleDriveError<S::Error>>(vehicle, null)?;
        }
        Ok(())
    }

    /// `004124C4` (and `00412618` on another machine's vehicle): with the
    /// hold's byte +0C set, each slot's +0 places its actor and the
    /// holder's basis (+48, 004026B0) is copied whole; clear, the actor
    /// takes the holder's +32. A plain slot's +0, `00412720` (table
    /// `005206C0`, every hold's slots), copies the holder's +32 too, but only
    /// a set byte calls it, and the one hold that sets it replaces its slots'
    /// table. Only the Prison Unit's hold sets it: its
    /// slot's 00412740 turns the offset 0054ED4C/50 (zero BSS nothing
    /// writes) by the holder's basis (0047A660), which leaves the holder's
    /// own position.
    pub(super) fn ride_with_holder<E>(
        &mut self,
        holder: ObjectId,
        actor: ObjectId,
    ) -> Result<(), VehicleDriveError<E>> {
        let position = self.item_position(holder)?;
        self.owned_mut(actor)?.state.base.position_words = Some(position);
        if self.carry_slots(holder)?.byte_0c != 0 {
            let basis = self.actor(holder)?.rotating.basis;
            self.actor_mut(actor)?.rotating.basis = basis;
        }
        Ok(())
    }

    /// `00412421..00412459`: a local carried actor's +90 keeps it while
    /// true; its status timers then run.
    #[allow(clippy::too_many_arguments)]
    fn carried_actor_tick<S: FamilyUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        actor: ObjectId,
        delta: u32,
        null: ActorHandle,
        status_load_factor: Option<i32>,
        carry: Option<VehicleCarryContext<'_>>,
        status: &mut S,
    ) -> Result<bool, VehicleDriveError<S::Error>> {
        if self.object(actor)?.base.flags & 0x8000_0000 == 0
            && !self.carried_actor_virtual_90(
                vehicle,
                actor,
                delta,
                null,
                status_load_factor,
                carry,
                status,
            )?
        {
            return Ok(false);
        }
        self.tick_actor_status(actor, delta, status_load_factor, status)
            .map_err(status_error)?;
        Ok(true)
    }

    /// The carried actor's +90 with the vehicle and the delta. Selector 12
    /// binds `004688A0`: the vehicle's health +A0 rises by the delta over
    /// 2^14, at least 1, as a WORD, and no higher than its +A2. The ECM
    /// Unit binds `0042DA60`: the vehicle takes the ECM flag 0x40000. The
    /// mining rig (`00526714`), the Remote Beamer and its receiver
    /// (`005274C4`, `00526530`) and the registry's classes keep the shared
    /// table's `004026C0` (AL 1): the Bomb (`00525AB4`), the grenades,
    /// mines, missiles and shots (25..28, 31, 32, 38, 68). Of the other
    /// tables only the Alien Brain's attack (`00464930`), selector 85's and
    /// the Prison Unit's (`0046E640`) bind their own; the port keeps the
    /// stop for every class whose table it has not read.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn carried_actor_virtual_90<S: FamilyUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        actor: ObjectId,
        delta: u32,
        null: ActorHandle,
        status_load_factor: Option<i32>,
        carry: Option<VehicleCarryContext<'_>>,
        status: &mut S,
    ) -> Result<bool, VehicleDriveError<S::Error>> {
        if self.selector73(actor).is_ok() {
            return Ok(self.hold_selector73(vehicle)?);
        }
        // 0046E640, the Prison Unit's: its own hold runs (00412380), and a
        // unit that held a prisoner its hold has lost lets go of the team's
        // task source, as itself and as the holder (00482C60 twice), which
        // this path cannot reach. It stays in the hold.
        if self.selector17(actor).is_ok() {
            self.tick_hold(actor, delta, null, status_load_factor, carry, status)?;
            let prison = self.selector17(actor)?;
            if prison.holding_15e != 0 && prison.hold_144.handles.contains(&null) {
                return Err(VehicleDriveError::Unported {
                    vehicle: actor,
                    address: 0x0046_e66c,
                });
            }
            return Ok(true);
        }
        if self.selector52(actor).is_ok()
            || self.selector55(actor).is_ok()
            || self.is_selector56(actor)
            || matches!(
                self.actor(actor)?.extension,
                crate::object_runtime::ActorExtension::BehaviorResource(_)
            )
        {
            return Ok(true);
        }
        if self.selector12_actor(actor).is_err() {
            return Err(VehicleDriveError::Unported {
                vehicle,
                address: 0x0041_243f,
            });
        }
        let repair = if delta < 1 << REPAIR_SHIFT {
            1
        } else {
            (delta >> REPAIR_SHIFT) as u16
        };
        let state = self.actor_mut(vehicle)?;
        state.health = state.health.wrapping_add(repair);
        let maximum = state
            .maximum_health
            .ok_or(VehicleDriveError::UnknownField {
                vehicle,
                offset: 0xa2,
                address: 0x0046_88ce,
            })?;
        if state.health > maximum {
            state.health = maximum;
        }
        Ok(true)
    }

    /// `00417777..004177EC`: the drop.
    pub(super) fn vehicle_drop<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        null: ActorHandle,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let slots = &self.carry_slots(vehicle)?.handles;
        let free = slots.iter().filter(|&&handle| handle == null).count();
        let Some(index) = slots.len().checked_sub(free + 1) else {
            return Ok(());
        };
        host.drop_item(self, vehicle, index)
            .map_err(VehicleDriveError::Host)?;
        for pitch in DROP_PITCHES {
            host.play_sound(self, vehicle, DROP_SOUND, DROP_GAIN, pitch)
                .map_err(VehicleDriveError::Sound)?;
        }
        Ok(())
    }

    fn item_position<E>(&self, actor: ObjectId) -> Result<[u16; 3], VehicleDriveError<E>> {
        Ok(self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }

    /// `00475F50` with the vehicle's scanner and filter: the strictly best
    /// candidate, if any scored.
    pub(super) fn vehicle_pickup_scan<E>(
        &self,
        vehicle: ObjectId,
    ) -> Result<Option<ObjectId>, VehicleDriveError<E>> {
        let range = self.derived_actor(vehicle)?.word_164 as i16 as i32;
        let range_squared = range.wrapping_mul(range) as u32;
        let origin = self.item_position(vehicle)?;
        let mut best: Option<(ObjectId, u32)> = None;
        for candidate in self.actor_list().to_vec() {
            let flags = self.object(candidate)?.base.flags;
            if flags & EXCLUDED_FLAGS != 0 || flags & REQUIRED_FLAGS != REQUIRED_FLAGS {
                continue;
            }
            let other = self.item_position(candidate)?;
            let dx = i32::from(origin[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(origin[2].wrapping_sub(other[2]) as i16);
            let squared = (dz.wrapping_mul(dz) as u32).wrapping_add(dx.wrapping_mul(dx) as u32);
            if squared >= range_squared || candidate == vehicle {
                continue;
            }
            let score = self.vehicle_pickup_score(origin, candidate, squared)?;
            if score > best.map_or(0, |(_, best)| best) {
                best = Some((candidate, score));
            }
        }
        Ok(best.map(|(actor, _)| actor))
    }

    /// `004155C0(candidate, distance squared)`.
    fn vehicle_pickup_score<E>(
        &self,
        origin: [u16; 3],
        candidate: ObjectId,
        squared: u32,
    ) -> Result<u32, VehicleDriveError<E>> {
        if self.actor(candidate)?.word_b4 == ALIEN_TEAM || squared >= FILTER_RANGE_SQUARED {
            return Ok(0);
        }
        let height = i32::from(origin[1] as i16)
            .wrapping_sub(i32::from(self.item_position(candidate)?[1] as i16));
        if height.wrapping_abs() >= FILTER_HEIGHT {
            return Ok(0);
        }
        Ok(0x7fff_ffff - squared)
    }
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
