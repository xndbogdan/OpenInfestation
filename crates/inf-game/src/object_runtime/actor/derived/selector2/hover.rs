//! Selector 2's hovercraft, vehicle mode 3: the entry 004185D0 the mode
//! switch calls, and the drive 004191C0 its dispatch calls every tick. The
//! hull floats on the suspension 0041F3F0 like the wheeled modes, but it
//! sprays (00488E90) every tick, rights itself toward the ground normal
//! with full support, damps its spin and turns about its own Y axis.
//! Infest03 reaches the drive on 212 updates, on worlds 9 and 11.

use super::super::kinematics::{
    ftol, ActorAngularAxis, ActorAngularMotionRequest, ActorForwardMotionRequest,
};
use super::super::physics::ActorPhysicsRequest;
use super::super::DerivedActorError;
use super::drive::{
    analog_throttle, angular_table, forward_table, read_dword, VehicleDriveError,
    VehicleDriveRequest,
};
use super::update::VehicleUpdateHost;
use crate::object_runtime::ActorAngularResponseRequest;
use crate::object_runtime::{ObjectArena, ObjectId, PlayerControlBlock};

/// 0055F280: the CRT-initialized world up vector.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];
/// 004191D3: the spray's strength.
const SPRAY_STRENGTH: i32 = 1000;
/// 005205B0: 4096.0, the forward row's Y in 1/4096 units.
const PITCH_SCALE: f64 = 4096.0;

impl ObjectArena {
    /// 004185D0, the mode switch's entry for mode 3: the hover tables, the
    /// turn rate and traction cleared, byte +160 set and no boost. The
    /// network branch (0055F448 with role 4) swaps in two other tables; the
    /// offline game never takes it.
    pub fn reset_vehicle_hover(&mut self, vehicle: ObjectId) -> Result<(), DerivedActorError> {
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.drive.dword_21c = Some(0);
            state.words_188_194 = [0x0053_0510, 0x0053_0518, 0x0053_0508, 0x0053_0560];
        }
        self.derived_actor_mut(vehicle)?.byte_160 = 1;
        let state = self.selector2_actor_mut(vehicle)?;
        state.drive.dword_57c = Some(0);
        state.dwords_220_224[1] = 0;
        Ok(())
    }

    /// 004191C0 for an offline hovercraft. `controls` is the driving
    /// player's block, which the drive only reads.
    pub fn tick_vehicle_hover<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let step = delta >> 5;
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };
        // 004191CE..004191E5: the spray at +32 along +8E and the +44 basis.
        host.hull_spray(self, vehicle, SPRAY_STRENGTH)
            .map_err(VehicleDriveError::Host)?;
        // 004191EA..00419315: the player lookup feeds only the network
        // branch, which needs 0055F448.

        // 00419315..00419395: the power, from thrust or a boost, then the
        // pitch keys, then the analog throttle on the +18C table.
        let boost = self.selector2_actor(vehicle)?.dwords_220_224[1];
        let power: i32 = if controls.thrust_1c != 0 || boost != 0 {
            0x7ffff
        } else if controls.pitch_18 != 0 {
            if controls.pitch_18 >= 0 {
                -0x7fff
            } else {
                0x7ffff
            }
        } else if controls.analog_04 != 0 {
            let throttle = analog_throttle(controls);
            let table = self.selector2_actor(vehicle)?.words_188_194[1];
            let radius = u32::from(self.object(vehicle)?.base.word_28);
            let raw = if throttle > 0 {
                read_dword(request.executable, table)?.wrapping_mul(radius) as i32
            } else {
                (read_dword(request.executable, table + 4)?.wrapping_mul(radius) as i32)
                    .wrapping_neg()
            };
            (raw >> 12).wrapping_mul(throttle) >> 16
        } else {
            0
        };
        // 0041939E: 00419050 with no speed: the turn rate at the step.
        self.vehicle_turn_rate(vehicle, step, controls, -1)?;
        let turn = self.selector2_actor(vehicle)?.dwords_220_224[0] as i32;
        // 004193C1: 0041F3F0 on the +194 table holds the hull up.
        let suspension_table = self.selector2_actor(vehicle)?.words_188_194[3];
        let contact = self.vehicle_suspension(vehicle, suspension_table, step, request)?;
        // 004193CC: 0047AD10(delta), unhalved even under water.
        {
            let velocity_y = &mut self.actor_mut(vehicle)?.words_8e_92[1];
            *velocity_y = velocity_y.wrapping_sub((delta >> 10) as u16);
        }
        // 004193D1..00419421: supported, the hull rights toward the ground
        // normal and grips with the +188 table; floating, toward world up.
        let normal = if contact.support != 0 {
            let normal = self
                .selector2_actor(vehicle)?
                .drive
                .normal_198
                .ok_or(unknown(0x198, 0x0041_93d8))?;
            let table = self.selector2_actor(vehicle)?.words_188_194[0];
            self.vehicle_grip(vehicle, contact.traction, step, table, false, request)?;
            normal.map(f32::from_bits)
        } else {
            WORLD_UP
        };
        // 00419424..004194A0: each angular DWORD loses
        // (w * -(0xFFFF * step >> 4)) >> 31 through 0047B470.
        let damping = (step.wrapping_shl(16).wrapping_sub(step) as i32).wrapping_neg() >> 4;
        let angular = self.actor(vehicle)?.words_94_9c;
        let loss = angular.map(|word| multiply_high31(word as i32, damping) as u32);
        self.add_actor_angular_velocity(vehicle, loss)?;
        // 004194B8: 00426730 toward the normal with full support.
        let basis = self.actor(vehicle)?.rotating.basis;
        self.apply_actor_angular_response(
            vehicle,
            ActorAngularResponseRequest {
                current_axis: [4, 5, 6].map(|index| f32::from_bits(basis.words[index])),
                desired_axis: normal,
                gain: (0xffff_u32.wrapping_mul(delta >> 4) >> 14) as i32,
            },
        )?;
        // 004194BD..00419501: under power, 00426860 on the +18C table; a
        // nose raised past 30 degrees (forward Y above 0x800) loses push.
        if power != 0 {
            let basis = self.actor(vehicle)?.rotating.basis;
            let pitch = ftol(f64::from(f32::from_bits(basis.words[8])) * PITCH_SCALE);
            let mut units = delta >> 3;
            if pitch > 0x800 {
                units = (0x1000_i32.wrapping_sub(pitch)).wrapping_mul(units as i32) as u32 >> 11;
            }
            let table = self.selector2_actor(vehicle)?.words_188_194[1];
            self.apply_actor_forward_motion(
                vehicle,
                ActorForwardMotionRequest {
                    power: 0xffff,
                    desired_velocity: power,
                    step_units: units,
                    table: forward_table(request.executable, table)?,
                },
            )?;
        }
        // 00419520: 00426AE0 about the hull's own Y on the +190 table.
        let table = self.selector2_actor(vehicle)?.words_188_194[2];
        self.apply_actor_angular_motion(
            vehicle,
            ActorAngularMotionRequest {
                power: 0xffff,
                desired_turn: turn,
                step_units: delta >> 3,
                table: angular_table(request.executable, table)?,
                axis: ActorAngularAxis::LocalY,
            },
        )?;
        // 0041952A: 0047B1E0 with angular and linear drag, the position
        // and the world-space spin.
        self.tick_actor_physics(
            vehicle,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 0x1e,
                sine_quarter: request.ground.sine_quarter,
            },
        )?;
        // 0041952F..0041957B: the engine load rises under power, else
        // falls to a quarter.
        let state = self.selector2_actor_mut(vehicle)?;
        let low = state.drive.dword_55c.ok_or(unknown(0x55c, 0x0041_9539))?;
        let high = state.drive.dword_560.ok_or(unknown(0x560, 0x0041_9533))?;
        if power != 0 {
            state.drive.dword_560 = Some(high.wrapping_add(100));
            state.drive.dword_55c = Some(low.wrapping_add(100));
        } else {
            state.drive.dword_55c = Some(((low as i32) >> 2) as u32);
            state.drive.dword_560 = Some(((high as i32) >> 2) as u32);
        }
        Ok(())
    }
}

/// Signed IMUL EDX:EAX, SHL EAX then RCL EDX: the high DWORD of the product
/// doubled.
fn multiply_high31(left: i32, right: i32) -> i32 {
    (i64::from(left) * i64::from(right) >> 31) as i32
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
