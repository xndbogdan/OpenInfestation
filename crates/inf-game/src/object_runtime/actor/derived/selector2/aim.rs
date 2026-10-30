//! The aim `0041F9C0`, run from the vehicle update every tick after the
//! behavior's: it finds what the vehicle aims at and keeps it as the `+148`
//! target.
//!
//! The vehicle's flag 4 is cleared for the duration so that neither the scan
//! nor the sight test meets the vehicle itself. The scan (`00475EC0`) scores
//! the actors with flags `0x24` within `+164` in XZ (`004153A0`). A scored
//! actor becomes the type-2 target and the sight test (`0043AD90`) runs
//! toward it. Without one the vehicle looks 3000 units ahead along its
//! forward row, spread by the selected weapon, and the sight test decides the
//! target alone. A hit replaces the target with the terrain point (type 8),
//! the furniture (type 4) or the actor (type 2) it met.

use super::drive::VehicleDriveError;
use super::update::{VehicleUpdateHost, VehicleUpdateRequest};
use crate::object_runtime::actor::behavior::{read_vehicle_row_dword, BehaviorRowSelection};
use crate::object_runtime::{
    mark_flag_change, ActorHandle, ActorServiceAllocationRequest, ActorServiceResourceKind,
    FixedPositionTarget, FollowTargetPoint, FollowTargetRequest, FurniturePositionTarget,
    NativeHeapRef, NativeTarget, ObjectArena, ObjectError, ObjectId,
};
use crate::world_sight::{Sight, SightHit};

/// `00520A54`: the look-ahead's reach along the vehicle's forward row.
const LOOK_AHEAD: f64 = 3000.0;
/// `00520A58`: the free look's reach along the special view's forward row.
const FREE_LOOK_REACH: f64 = 7500.0;
/// `0041FB18`: the flags a scanned actor needs.
const SCAN_FLAGS: u32 = 0x24;
/// `004154CC`: the accuracy factor's base.
const ACCURACY_BASE: u32 = 0x7fff;
/// `0041552C`: wheeled modes accept a slightly wider cone without accuracy.
const WHEELED_CONE: i64 = 0x7c00_0000;

/// `00415310`'s evaluator: the vehicle's forward row (virtual `+48`, record
/// `+1C..+24`), the current target's handle and whether it is armed.
struct AimEvaluator {
    forward: [f64; 3],
    handle: ActorHandle,
    armed: bool,
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

impl ObjectArena {
    /// `0041F9C0`.
    pub(super) fn vehicle_aim<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let null = request.drive.process_null;
        let evaluator = self.aim_evaluator(vehicle, null)?;
        self.set_vehicle_flag4(vehicle, false)?;
        if self.selector2_actor(vehicle)?.byte_1c0 != 0 {
            // 0041FCE8..0041FD6B: the free look aims from 00421D50's eye along
            // its forward row, through the look-ahead's sight test and tail.
            let math =
                inf_formats::native_basis::NativeBasisMath::new(*request.drive.ground.sine_quarter);
            let mut eye = [None; 3];
            // 00490370 writes every word: the stack basis's residue is not read.
            let mut basis = self.actor(vehicle)?.rotating.basis;
            self.special_view_421d50(vehicle, &mut eye, &mut basis, &math)
                .map_err(|error| match error {
                    crate::object_runtime::CameraPoseError::UninitializedSpecialViewStorage {
                        offset,
                        ..
                    } => VehicleDriveError::UnknownField {
                        vehicle,
                        offset,
                        address: 0x0041_fcf4,
                    },
                    error => VehicleDriveError::Status(error.to_string()),
                })?;
            let eye = eye.map(|word| word.expect("special view eye written"));
            let target = std::array::from_fn(|axis| {
                let forward = f64::from(f32::from_bits(basis.words[7 + axis]));
                (ftol(forward * FREE_LOOK_REACH) as u16).wrapping_add(eye[axis])
            });
            let sight = host
                .sight(self, eye, target)
                .map_err(VehicleDriveError::Host)?;
            self.replace_aim_target(vehicle, sight, null)?;
            self.set_vehicle_flag4(vehicle, true)?;
            return Ok(());
        }
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let sight = match self.aim_scan(vehicle, &evaluator, request.settings_7c, host)? {
            Some(best) => {
                let reference = self.lock_aim_target(vehicle, best, request.settings_7c)?;
                let target = self.native_target(reference)?.raw_focus_08_words().ok_or(
                    VehicleDriveError::UnknownField {
                        vehicle,
                        offset: 0x148,
                        address: 0x0041_fc46,
                    },
                )?;
                let sight = host
                    .sight(self, position, target)
                    .map_err(VehicleDriveError::Host)?;
                // 0041FC6E: a clear line keeps the scanned target.
                if sight.hit.is_none() {
                    self.set_vehicle_flag4(vehicle, true)?;
                    return Ok(());
                }
                sight
            }
            None => {
                let target = self.look_ahead(vehicle, position, host)?;
                host.sight(self, position, target)
                    .map_err(VehicleDriveError::Host)?
            }
        };
        self.replace_aim_target(vehicle, sight, null)?;
        self.set_vehicle_flag4(vehicle, true)?;
        Ok(())
    }

    /// `00415310`. The vehicle's virtual `+64` is itself and `+7C` its
    /// behavior, so it is armed with a behavior. An armed evaluator drops a
    /// current target whose actor no longer resolves or whose `+C4` is at
    /// most 1, using the process NULL instead.
    fn aim_evaluator(
        &mut self,
        vehicle: ObjectId,
        null: ActorHandle,
    ) -> Result<AimEvaluator, ObjectError> {
        let mut handle = match self.derived_actor(vehicle)?.target_148 {
            Some(reference) => self.native_target(reference)?.handle_04(),
            None => null,
        };
        let armed = self.derived_actor(vehicle)?.behavior_140.is_some();
        if armed {
            let live = self
                .resolve_actor_handle(handle)
                .map_err(|_| ObjectError::MissingActorRegistry)?;
            let alive = match live {
                Some(actor) => (self.actor(actor)?.word_c4 as i32) > 1,
                None => false,
            };
            if !alive {
                handle = null;
            }
        }
        let basis = self.actor(vehicle)?.rotating.basis;
        Ok(AimEvaluator {
            forward: std::array::from_fn(|axis| f64::from(f32::from_bits(basis.words[7 + axis]))),
            handle,
            armed,
        })
    }

    /// `0041FB27` and `0041FEF0`: flag 4 off for the aim and back on after;
    /// a change in the low nine bits marks the dirty word.
    fn set_vehicle_flag4(&mut self, vehicle: ObjectId, on: bool) -> Result<(), ObjectError> {
        let base = &mut self.owned_mut(vehicle)?.state.base;
        let flags = if on { base.flags | 4 } else { base.flags & !4 };
        mark_flag_change(base, flags);
        Ok(())
    }

    /// `00475EC0`: the actor list from `G+48`, each actor with both scan
    /// flags nearer than `+164` in XZ (unsigned squares) and not the vehicle,
    /// keeping the strictly best score. A zero best finds nothing.
    fn aim_scan<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        evaluator: &AimEvaluator,
        settings_7c: u32,
        host: &mut H,
    ) -> Result<Option<ObjectId>, VehicleDriveError<H::Error>> {
        let reach = i32::from(self.derived_actor(vehicle)?.word_164 as i16);
        let limit = reach.wrapping_mul(reach) as u32;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let mut best = None;
        let mut best_score = 0u32;
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & SCAN_FLAGS != SCAN_FLAGS {
                continue;
            }
            let other = self
                .object(candidate)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(candidate))?;
            let dx = i32::from(position[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(position[2].wrapping_sub(other[2]) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= limit || candidate == vehicle {
                continue;
            }
            let score = self.aim_score(vehicle, evaluator, candidate, settings_7c, host)?;
            if score > best_score {
                best_score = score;
                best = Some(candidate);
            }
        }
        Ok(best)
    }

    /// `004153A0`: nothing for an unarmed evaluator, a candidate whose `+C4`
    /// is not positive, or one on the vehicle's side or side 1. The offset is
    /// projected on the forward row (truncated) and must be ahead. Wheeled
    /// modes 1 to 3 measure it in the vehicle's frame (`0047A660`), its
    /// height a sixteenth. The square of the projection must exceed a share
    /// of the offset's: with settings `+7C` the selected weapon's accuracy
    /// (none while `+200` is set), 15/16 of it for the current target;
    /// otherwise 31/32 when wheeled, else all of it. The score is the largest
    /// value less the lateral square, which the current target has a
    /// quarter off. A candidate the weapon cannot damage (`0048D820`) scores
    /// nothing if its virtual `+54` is itself, else a sixteenth, and so does
    /// one below lifecycle 3 or on side 2.
    fn aim_score<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        evaluator: &AimEvaluator,
        candidate: ObjectId,
        settings_7c: u32,
        host: &mut H,
    ) -> Result<u32, VehicleDriveError<H::Error>> {
        if !evaluator.armed {
            return Ok(0);
        }
        let (side, lifecycle, handle) = {
            let actor = self.actor(candidate)?;
            (actor.word_b4, actor.word_c4 as i32, actor.handle)
        };
        if lifecycle <= 0 || side == self.actor(vehicle)?.word_b4 || side == 1 {
            return Ok(0);
        }
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let other = self
            .object(candidate)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(candidate))?;
        let mut delta: [i16; 3] =
            std::array::from_fn(|axis| other[axis].wrapping_sub(position[axis]) as i16);
        let [dx, dy, dz] = delta.map(f64::from);
        let [fx, fy, fz] = evaluator.forward;
        let projection = ftol((dz * fz + dy * fy) + dx * fx);
        if projection <= 0 {
            return Ok(0);
        }
        let mode = self.selector2_actor(vehicle)?.dwords_1e8_1f4[0];
        let wheeled = matches!(mode, 1..=3);
        if wheeled {
            delta = self.actor(vehicle)?.rotating.basis.inverse_words(delta);
            // CDQ, AND 0Fh, ADD, SAR 4: division by 16 toward zero.
            delta[1] = (i32::from(delta[1]) / 16) as i16;
        }
        let [x, y, z] = delta.map(i32::from);
        let squared = x
            .wrapping_mul(x)
            .wrapping_add(y.wrapping_mul(y))
            .wrapping_add(z.wrapping_mul(z));
        let current = handle == evaluator.handle;
        let threshold = if settings_7c != 0 {
            let accuracy = self.vehicle_weapon_dword(vehicle, 0x18)?;
            let factor = if self.selector2_actor(vehicle)?.dwords_200_204[0] != 0 {
                0
            } else {
                ACCURACY_BASE.wrapping_sub(accuracy) << 16
            };
            let threshold = ((i64::from(squared) * i64::from(factor as i32)) >> 31) as u32;
            if current {
                threshold.wrapping_mul(15) >> 4
            } else {
                threshold
            }
        } else if wheeled {
            ((i64::from(squared) * WHEELED_CONE) >> 31) as u32
        } else {
            squared as u32
        };
        let projected = projection.wrapping_mul(projection) as u32;
        if projected <= threshold {
            return Ok(0);
        }
        let mut lateral = (squared as u32).wrapping_sub(projected);
        if current {
            lateral -= lateral >> 2;
        }
        let mut score = 0x7fff_ffffu32.wrapping_sub(lateral);
        if !host
            .damage_predicate(self, vehicle, candidate)
            .map_err(VehicleDriveError::Host)?
        {
            score = if self.actor_completion_kept(candidate)? {
                0
            } else {
                score >> 4
            };
        }
        // 0041558B rereads both after the predicate.
        let actor = self.actor(candidate)?;
        if (actor.word_c4 as i32) < 3 || actor.word_b4 == 2 {
            score >>= 4;
        }
        Ok(score)
    }

    /// A DWORD of the selected weapon's row: the vehicle's behavior `+14`
    /// is always its own `+26C` copy.
    pub(super) fn vehicle_weapon_dword<E>(
        &self,
        vehicle: ObjectId,
        offset: u32,
    ) -> Result<u32, VehicleDriveError<E>> {
        let behavior = self.derived_actor(vehicle)?.behavior_140.as_ref().ok_or(
            VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x140,
                address: 0x0041_fdb1,
            },
        )?;
        let BehaviorRowSelection::VehicleCopy { byte_offset, .. } =
            BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, vehicle)
        else {
            return Err(VehicleDriveError::Status(
                "the vehicle's behavior +14 is not its +26C copy".into(),
            ));
        };
        read_vehicle_row_dword(self, vehicle, byte_offset, offset)
            .map_err(|error| VehicleDriveError::Status(error.to_string()))
    }

    /// `0041FB79..0041FC35`: the scanned actor becomes the type-2 target.
    /// A current type-2 target is retargeted in place (virtual `+20`,
    /// `00482760`); any other is freed for a new one (`004825D0` resolving
    /// the actor). The target is then refreshed (virtual `+18`).
    fn lock_aim_target<E>(
        &mut self,
        vehicle: ObjectId,
        best: ObjectId,
        settings_7c: u32,
    ) -> Result<NativeHeapRef, VehicleDriveError<E>> {
        let mut kept = None;
        if let Some(reference) = self.derived_actor(vehicle)?.target_148 {
            let mut target = self.native_target(reference)?;
            let retargeted = match &mut target {
                NativeTarget::ActorFollow(follow) => {
                    self.replace_follow_target(follow, Some(best))?
                }
                // 00402720 answers false for types 4, 8 and 16.
                _ => false,
            };
            if retargeted {
                self.write_native_target(reference, target)?;
                kept = Some(reference);
            } else {
                self.release_actor_service(reference)?;
                self.derived_actor_mut(vehicle)?.target_148 = None;
            }
        }
        let reference = match kept {
            Some(reference) => Some(reference),
            None => {
                let handle = self.actor(best)?.handle;
                let reference =
                    self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
                self.derived_actor_mut(vehicle)?.target_148 = reference;
                reference
            }
        };
        // 0041FC35 calls through a NULL allocation.
        let reference = reference.ok_or(VehicleDriveError::Unported {
            vehicle,
            address: 0x0041_fc35,
        })?;
        let mut target = self.native_target(reference)?;
        let refreshed = self.refresh_native_target(&mut target, settings_7c);
        self.write_native_target(reference, target)?;
        refreshed?;
        Ok(reference)
    }

    /// `0041FD70..0041FE54`: the vehicle's position plus its forward row
    /// times 3000 (each axis truncated), and with the selected weapon's
    /// spread `+14` three draws of `0048F060` add
    /// `((draw >> 5) - 1024) * (spread / 2) >> 15` to x, y and z.
    fn look_ahead<H: VehicleUpdateHost>(
        &self,
        vehicle: ObjectId,
        position: [u16; 3],
        host: &mut H,
    ) -> Result<[u16; 3], VehicleDriveError<H::Error>> {
        let basis = self.actor(vehicle)?.rotating.basis;
        let mut target: [u16; 3] = std::array::from_fn(|axis| {
            let forward = f64::from(f32::from_bits(basis.words[7 + axis]));
            (ftol(forward * LOOK_AHEAD) as u16).wrapping_add(position[axis])
        });
        let spread = self.vehicle_weapon_dword(vehicle, 0x14)? as i32;
        if spread != 0 {
            let half = spread / 2;
            for word in &mut target {
                let draw = host.random().map_err(VehicleDriveError::Host)?;
                let offset = ((((draw & 0xffff) >> 5) as i32) - 0x400).wrapping_mul(half) >> 15;
                *word = word.wrapping_add(offset as u16);
            }
        }
        Ok(target)
    }

    /// `0041FC74..0041FEEA`: a hit frees the target and allocates its
    /// replacement by type: the terrain point as type 8 (`0051F154`, the
    /// process NULL at `+04`), the furniture as type 4 (`004827D0`) and the
    /// actor as type 2 with the point (`004825D0`). The look-ahead's clear
    /// line frees the target alone.
    fn replace_aim_target<E>(
        &mut self,
        vehicle: ObjectId,
        sight: Sight,
        null: ActorHandle,
    ) -> Result<(), VehicleDriveError<E>> {
        if let Some(reference) = self.derived_actor_mut(vehicle)?.target_148.take() {
            self.release_actor_service(reference)?;
        }
        let Some(hit) = sight.hit else {
            return Ok(());
        };
        let data = match hit {
            SightHit::Terrain { point } => {
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(null, Some(point)))
            }
            SightHit::Furniture { point, position } => NativeTarget::FurniturePosition(
                FurniturePositionTarget::new(null, position, Some(point)),
            ),
            SightHit::Actor { point, actor } => {
                NativeTarget::ActorFollow(self.follow_target(FollowTargetRequest {
                    handle: self.actor(actor)?.handle,
                    point: FollowTargetPoint::ExplicitPoint(Some(point)),
                })?)
            }
        };
        let reference = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(data.class()),
            })
            .map_err(ObjectError::from)?;
        if let Some(reference) = reference {
            self.construct_native_target(reference, data)?;
        }
        self.derived_actor_mut(vehicle)?.target_148 = reference;
        Ok(())
    }
}

#[cfg(test)]
#[path = "aim_tests.rs"]
mod tests;
