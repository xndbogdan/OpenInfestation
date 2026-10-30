//! The vehicle another machine drives (`+2C` bit 31): its `+3C`, `00420F40`.
//! The record's bits `+58C` stand in for the controls: bits 0 and 1 a mode
//! that idles (2000) or runs at the speed (300 more with bit 1), bit 2 the
//! throttle engaged and bit 3 forward. From them the engine's words `+22C`
//! come as the local update's do, then the shared `0047EBF0` moves it on.

use std::sync::atomic::{AtomicBool, Ordering};

use super::super::kinematics::ftol;
use super::drive::VehicleDriveError;
use super::update::{i16_of, magnitude, squares, VehicleUpdateHost};
use crate::object_runtime::actor::common_update::{RemoteActorError, RemoteActorHost};
use crate::object_runtime::{ActorHandle, FamilyUpdateEffect, ObjectArena, ObjectId};

/// 004211C8: the hover spray's strength.
const SPRAY_STRENGTH: i32 = 1000;

static RESIDUE_BITS_LOGGED: AtomicBool = AtomicBool::new(false);

/// `004211B4..00421244`: a new wake ribbon's first points, a quarter and
/// an eighth of the velocity behind the skimmer and (to the eighth's
/// rounding) where it is.
pub(super) fn wake_ribbon_seed(position: [u16; 3], velocity: [u16; 3]) -> [[u16; 3]; 3] {
    let mut point: [u16; 3] = std::array::from_fn(|axis| {
        let behind = (i32::from(velocity[axis] as i16) << 1) >> 3;
        position[axis].wrapping_sub(behind as u16)
    });
    let mut points = [point; 3];
    for later in &mut points[1..] {
        for (word, velocity) in point.iter_mut().zip(velocity) {
            *word = word.wrapping_add(((velocity as i16) >> 3) as u16);
        }
        *later = point;
    }
    points
}

impl ObjectArena {
    /// 00420F40: 00422300's hold check, the moving timer `+5A0`, the engine
    /// from `+58C`, 0047EBF0, the hold's carry (004125C0), the skimmer's
    /// wake ribbon and the hovercraft's spray.
    pub fn tick_remote_vehicle<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        null: ActorHandle,
        sine_quarter: &[u16; 4096],
        load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>>
    where
        H::Error: std::fmt::Display,
    {
        self.vehicle_target_check(vehicle, delta, null)?;
        let velocity = self.actor(vehicle)?.words_8e_92.map(i16_of);
        let sum = squares(velocity);
        let state = self.selector2_actor_mut(vehicle)?;
        let timer = &mut state.dwords_59c_5a0[1];
        *timer = if sum as u32 > 0xf424 {
            2_000_000
        } else if *timer >= delta {
            *timer - delta
        } else {
            0
        };
        // A vehicle built from a record reads +58C before the next record's
        // sync writes it: retail's allocator residue, the fresh heap's zero.
        let bits = state.drive.word_58c.unwrap_or_else(|| {
            if !RESIDUE_BITS_LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!("RemoteVehicleBitsApproximation: a vehicle another machine drives reads its +58C before a record's sync writes it; retail reads allocator residue, the port the fresh heap's zero (fresh_heap.rs)");
            }
            0
        });
        let (boosting, intensity) = if bits & 3 == 0 {
            (1, 2000)
        } else {
            let speed = magnitude(sum);
            (
                0,
                if bits & 2 != 0 {
                    speed.wrapping_add(300)
                } else {
                    speed
                },
            )
        };
        state.dwords_220_224[1] = boosting;
        state.dword_228 = Some(intensity);
        if intensity as i32 > 100 {
            self.remote_vehicle_engine(vehicle, intensity as i32, bits, velocity, delta)?;
        }
        self.tick_remote_shared(vehicle, delta, sine_quarter, load_factor, host)?;
        // 004125C0: each held actor whose +70 is this vehicle rides with
        // it, and one this machine owns runs its +90 with the vehicle, its
        // answer unread.
        let own = self.actor(vehicle)?.handle;
        let count = self.carry_slots(vehicle)?.handles.len();
        for index in 0..count {
            let handle = self.carry_slots(vehicle)?.handles[index];
            if handle == null {
                continue;
            }
            let Some(held) = self.resolve_actor_handle(handle)? else {
                continue;
            };
            let captor =
                self.actor(held)?
                    .capture_handle_70
                    .ok_or(VehicleDriveError::UnknownField {
                        vehicle: held,
                        offset: 0x70,
                        address: 0x0041_260d,
                    })?;
            if captor != own {
                continue;
            }
            self.ride_with_holder::<H::Error>(vehicle, held)?;
            if self.object(held)?.base.flags & 0x8000_0000 == 0 {
                self.carried_actor_virtual_90(
                    vehicle,
                    held,
                    delta,
                    null,
                    load_factor,
                    None,
                    &mut super::update::StatusHost {
                        host: &mut *host,
                        null,
                    },
                )?;
            }
        }
        let state = self.selector2_actor(vehicle)?;
        let mode = state.dwords_1e8_1f4[0];
        // 00421189..00421267: a skimmer leaves a wake ribbon. A new one
        // (00486600 with its handle) starts with three points, a quarter
        // and an eighth of its velocity behind it and where it is; one it
        // has gains where it is (004869B0). In any other mode its ribbon is
        // let go (00486B20: the owner +50 the null) and forgotten.
        match (mode == 5, state.ribbon_264) {
            (true, None) => {
                let handle = self.actor(vehicle)?.handle;
                if let Some(ribbon) = host
                    .create_ribbon(self, handle)
                    .map_err(VehicleDriveError::Host)?
                {
                    self.selector2_actor_mut(vehicle)?.ribbon_264 = Some(ribbon);
                    let position = self.position(vehicle)?;
                    let velocity = self.actor(vehicle)?.words_8e_92;
                    for point in wake_ribbon_seed(position, velocity) {
                        host.extend_ribbon(self, ribbon, delta, point)
                            .map_err(VehicleDriveError::Host)?;
                    }
                }
            }
            (true, Some(ribbon)) => {
                let position = self.position(vehicle)?;
                host.extend_ribbon(self, ribbon, delta, position)
                    .map_err(VehicleDriveError::Host)?;
            }
            (false, Some(ribbon)) => {
                self.set_selector67_owner(ribbon, null)?;
                self.selector2_actor_mut(vehicle)?.ribbon_264 = None;
            }
            (false, None) => {}
        }
        if mode == 3 {
            host.hull_spray(self, vehicle, SPRAY_STRENGTH)
                .map_err(VehicleDriveError::Host)?;
        }
        // 00421289: a live vehicle claims its race record (0046F760).
        if self.race().is_some() && self.actor(vehicle)?.word_c4 == 3 {
            self.claim_race_record(vehicle, null)
                .map_err(|error| VehicleDriveError::Status(format!("race record: {error}")))?;
        }
        Ok(())
    }

    /// 0047EBF0, the shared `+3C` (the camera's too), with the vehicle
    /// update's services.
    pub fn tick_remote_shared<H: VehicleUpdateHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>>
    where
        H::Error: std::fmt::Display,
    {
        self.tick_remote_actor(actor, delta, sine_quarter, load_factor, &mut Remote(host))
            .map_err(|error| match error {
                RemoteActorError::Object(error) => VehicleDriveError::Object(error),
                RemoteActorError::Motion(error) => VehicleDriveError::Motion(error),
                RemoteActorError::Status(error) => VehicleDriveError::Status(error),
                RemoteActorError::Host(error) => VehicleDriveError::Host(error),
            })
    }

    /// `00421050..00421161`: the engine words from the intensity `+228`, as
    /// the local `0041A0C0`'s, but turned by the record's bits and with
    /// `+232..+236` the velocity scaled by `-(delta >> 6)`.
    fn remote_vehicle_engine<E>(
        &mut self,
        vehicle: ObjectId,
        intensity: i32,
        bits: u16,
        velocity: [i32; 3],
        delta: u32,
    ) -> Result<(), VehicleDriveError<E>> {
        let basis = self.actor(vehicle)?.rotating.basis;
        let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let base = (intensity / 10).wrapping_add(100);
        let rise = f64::from(base as i16);
        let mut words = [0u16; 6];
        for (axis, word) in words.iter_mut().take(3).enumerate() {
            *word = ftol(rise * m(4 + axis)) as u16;
        }
        let boosting = self.selector2_actor(vehicle)?.dwords_220_224[1] != 0;
        let push = if boosting || (bits & 4 != 0 && bits & 8 != 0) {
            Some(base / 4)
        } else if bits & 4 != 0 {
            Some((base / 4).wrapping_neg())
        } else {
            None
        };
        if let Some(push) = push {
            let push = f64::from(push);
            for (axis, word) in words.iter_mut().take(3).enumerate() {
                *word = word.wrapping_add(ftol(push * m(7 + axis)) as u16);
            }
        }
        if (words[1] as i16) < 0 {
            words[1] = words[1].wrapping_neg();
        }
        let scale = ((delta >> 6) as i32).wrapping_neg();
        for axis in 0..3 {
            words[3 + axis] = (velocity[axis].wrapping_mul(scale) >> 15) as u16;
        }
        self.selector2_actor_mut(vehicle)?.drive.words_22c_236 = Some(words);
        Ok(())
    }
}

/// The vehicle update's services as 0047EBF0's.
struct Remote<'h, H>(&'h mut H);

impl<H: VehicleUpdateHost> RemoteActorHost for Remote<'_, H>
where
    H::Error: std::fmt::Display,
{
    type Error = H::Error;
    fn random(&mut self) -> Result<u32, H::Error> {
        self.0.random()
    }
    fn status_trail(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), H::Error> {
        self.0.status_effect(arena, actor, effect)
    }
}
