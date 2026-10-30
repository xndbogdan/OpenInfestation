//! Desired camera vectors selected by 0040FD10 before its smoothing pass.

use inf_formats::native_basis::{NativeBasis, NativeBasisMath};

use super::{
    CameraDerivedComponent, CameraDerivedTargetMode, CameraDerivedTargetRequest,
    CameraDesiredTargets, CameraHeightRequest, CameraPoseError, CameraPoseHost,
};
use crate::object_runtime::actor::selector16::Selector16State;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

impl ObjectArena {
    pub(super) fn update_camera_targets(
        &mut self,
        camera: ObjectId,
        state: &mut Selector16State,
        actor: Option<ObjectId>,
        derived: Option<CameraDerivedComponent>,
        basis: &mut NativeBasis,
        math: &NativeBasisMath,
        host: &mut dyn CameraPoseHost,
        settings_7c: u32,
    ) -> Result<(), CameraPoseError> {
        // Complete derived dispatch/publication precedes generic focus reads.
        // SpecialView consumes primary velocity only after its basis and eye.
        // The full pose reaches this phase only inside target_18c.is_some();
        // the private request retains optional custody for late read errors.
        if matches!(state.mode_188, 3 | 4) {
            if let Some(component) = derived {
                let result = host.update_derived_camera_targets(
                    self,
                    CameraDerivedTargetRequest {
                        component,
                        mode: if state.mode_188 == 4 {
                            CameraDerivedTargetMode::SpecialView
                        } else {
                            CameraDerivedTargetMode::Chase
                        },
                        eye: &mut state.eye_138,
                        look: &mut state.look_15c,
                        basis,
                        primary_target: state.target_18c,
                        settings_7c,
                        math,
                    },
                );
                self.actor_mut(camera)?.rotating.basis = *basis;
                return result;
            }
        }
        let primary = self.actor_follow_target(
            state
                .target_18c
                .ok_or(CameraPoseError::UnwrittenTarget("primary"))?,
        )?;
        let focus = primary
            .focus_08
            .ok_or(CameraPoseError::UnwrittenTarget("primary"))?;
        let velocity = primary
            .velocity_1a
            .ok_or(CameraPoseError::UnwrittenTarget("primary"))?;
        let targets = match state.mode_188 {
            0 => CameraDesiredTargets {
                eye_position: self
                    .object(camera)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(camera))?,
                eye_velocity: self.actor(camera)?.words_8e_92,
                look_position: focus,
                look_velocity: velocity,
            },
            1 => {
                let target_basis = self.follow_target_basis(&primary)?;
                let offset = state.offset_194.ok_or(CameraPoseError::UnwrittenOffset)?;
                let rotated = target_basis.transform_words(offset);
                CameraDesiredTargets {
                    eye_position: std::array::from_fn(|axis| {
                        focus[axis].wrapping_add(rotated[axis])
                    }),
                    eye_velocity: velocity,
                    look_position: focus,
                    look_velocity: velocity,
                }
            }
            3 | 4 => {
                state.mode_188 = 3;
                let actor = actor.ok_or(CameraPoseError::UnwrittenTarget("resolved actor"))?;
                let position = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let actor_state = self.actor(actor)?;
                let actor_basis = actor_state.rotating.basis;
                let actor_velocity = actor_state.words_8e_92;
                if state.byte_184 == 0 {
                    let forward = horizontal_forward(actor_basis);
                    // 00410293..2CD: Z*VZ + Y*VY + X*VX retained,
                    // __ftol truncation, then signed division by five.
                    let coefficients = vector(actor_basis, 7).map(f64::from);
                    let v = actor_velocity.map(|word| f64::from(word as i16));
                    let projected_speed = ftol(
                        (v[2] * coefficients[2] + v[1] * coefficients[1]) + v[0] * coefficients[0],
                    );
                    let distance =
                        (-400_i32).wrapping_sub(projected_speed / 5).max(-1000) as i16 as i32;
                    let mut eye = offset_position(position, forward, f64::from(distance));
                    eye[1] = eye[1].wrapping_add(actor_state.word_74).wrapping_add(150);
                    CameraDesiredTargets {
                        eye_position: eye,
                        eye_velocity: actor_velocity,
                        look_position: offset_position(position, forward, 400.0),
                        look_velocity: actor_velocity,
                    }
                } else {
                    let (look_position, look_velocity) = if let Some(reference) = state.target_190 {
                        // 00410161: the target's +18, its focus +08 and its
                        // velocity (+08), a node target's being 0055F0F8.
                        let mut target = self.native_target(reference)?;
                        let refreshed = self.refresh_native_target(&mut target, settings_7c);
                        self.write_native_target(reference, target)?;
                        refreshed?;
                        (
                            target
                                .raw_focus_08_words()
                                .ok_or(CameraPoseError::UnwrittenTarget("secondary"))?,
                            target
                                .velocity_words(Some(
                                    crate::object_runtime::PROCESS_VELOCITY_0055F0F8,
                                ))
                                .ok()
                                .flatten()
                                .ok_or(CameraPoseError::UnwrittenTarget("secondary"))?,
                        )
                    } else {
                        (
                            offset_position(position, vector(actor_basis, 7), 8192.0),
                            actor_velocity,
                        )
                    };
                    CameraDesiredTargets {
                        eye_position: position,
                        eye_velocity: actor_velocity,
                        look_position,
                        look_velocity,
                    }
                }
            }
            _ => {
                // Native mode two and every unsigned out-of-range mode share
                // this branch and rearm the snap on every call.
                state.mode_188 = 2;
                state.byte_187 = 1;
                let target_basis = self.follow_target_basis(&primary)?;
                let forward = horizontal_forward(target_basis);
                let eye_position = if derived.is_some() {
                    let mut offset =
                        vector(target_basis, 4).map(|value| ftol(f64::from(value) * 50.0) as u16);
                    if (offset[1] as i16) < 0 {
                        offset[1] = 0;
                    }
                    std::array::from_fn(|axis| focus[axis].wrapping_add(offset[axis]))
                } else {
                    focus
                };
                CameraDesiredTargets {
                    eye_position,
                    eye_velocity: velocity,
                    look_position: offset_position(focus, forward, 400.0),
                    look_velocity: velocity,
                }
            }
        };
        state
            .eye_138
            .set_target(targets.eye_position, targets.eye_velocity);
        state
            .look_15c
            .set_target(targets.look_position, targets.look_velocity);
        Ok(())
    }

    pub(super) fn camera_escape_velocity(
        &mut self,
        camera: ObjectId,
        state: &mut Selector16State,
        host: &mut dyn CameraPoseHost,
    ) -> Result<(), CameraPoseError> {
        let forward = vector(self.actor(camera)?.rotating.basis, 7);
        // 005205BC = -200.0, read by the three __ftol calls.
        let mut velocity = forward.map(|value| ftol(f64::from(value) * -200.0) as u16);
        velocity[1] = velocity[1].wrapping_add(50);
        self.actor_mut(camera)?.words_8e_92 = velocity;
        if state.mode_188 == 2 {
            let mut position = self
                .object(camera)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(camera))?;
            let floor = i32::from(host.query_height(CameraHeightRequest::InterpolatedTerrain {
                xz: [position[0], position[2]],
            })?) + 128;
            if (position[1] as i16 as i32) < floor {
                position[1] = floor as u16;
            }
            for axis in 0..3 {
                position[axis] =
                    position[axis].wrapping_add((i32::from(velocity[axis] as i16) / 4) as u16);
            }
            state.byte_187 = 1;
            self.owned_mut(camera)?.state.base.position_words = Some(position);
        }
        Ok(())
    }
}

fn horizontal_forward(basis: NativeBasis) -> [f32; 3] {
    // 00410208..293 and 004103F1..47C truncate forward X/Z*4096 to
    // DWORDs first, then retain the X register and spill Z and the length.
    let forward = vector(basis, 7);
    let x = f64::from(ftol(f64::from(forward[0]) * 4096.0));
    let z = ftol(f64::from(forward[2]) * 4096.0) as f32;
    let length = ((f64::from(z) * f64::from(z)) + x * x).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let divisor = f64::from(length as f32);
    [(x / divisor) as f32, 0.0, (f64::from(z) / divisor) as f32]
}

fn offset_position(position: [u16; 3], direction: [f32; 3], scale: f64) -> [u16; 3] {
    std::array::from_fn(|axis| {
        position[axis].wrapping_add(ftol(f64::from(direction[axis]) * scale) as u16)
    })
}

fn vector(basis: NativeBasis, first: usize) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from_bits(basis.words[first + axis]))
}

pub(super) fn ftol(value: f64) -> i32 {
    let integral = value.trunc();
    if integral.is_finite()
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&integral)
    {
        integral as i64 as i32
    } else {
        0
    }
}
