//! Selector-12 virtual +118, 00467DF0. This complete motion consumer is
//! separate from the enclosing +24's water, behavior, targeting and weapon
//! owners. It never reads or supplies family allocation holes +230/+220.

use inf_formats::native_basis::{inverse_sine_q15, NativeBasisMath, RotationPlane};
use thiserror::Error;

use super::kinematics::ftol;
use super::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorForwardMotionRequest,
    ActorLateralMotionRequest, ActorMotionError, ActorPhysicsRequest,
};
use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    terrain_runtime::TerrainRuntime,
};

pub trait Selector12TerrainHost {
    type Error;
    /// Required 00405540(H12/0) → 00450A30 sample at the actual actor X/Z.
    /// No flat plane or sampled height is supplied as a default service.
    fn terrain_height(&mut self, xz_words: [u16; 2]) -> Result<i16, Self::Error>;
}

impl Selector12TerrainHost for &TerrainRuntime {
    type Error = std::convert::Infallible;
    fn terrain_height(&mut self, xz_words: [u16; 2]) -> Result<i16, Self::Error> {
        Ok(self.height_at(xz_words))
    }
}

pub struct Selector12MotionRequest<'a, H> {
    pub microseconds: u32,
    /// Native control DWORDs param3[0] / param3[1], respectively.
    pub turn_control: i32,
    pub forward_control: i32,
    /// Actual process byte 0053A128. Nonzero snaps live Y to the terrain.
    /// It is explicitly supplied, never inferred from the actor's position.
    pub surface_snap_53a128: u8,
    pub math: &'a NativeBasisMath,
    pub inverse_sine: &'a [i16; 1024],
    pub sine_quarter: &'a [u16; 4096],
    pub terrain: &'a mut H,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector12MotionError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error("actor {0:?} does not own selector-12 virtual +118")]
    UnsupportedClass(ObjectId),
    #[error("selector-12 native terrain sample failed: {0}")]
    Terrain(E),
}

impl ObjectArena {
    /// Complete 00467DF0 against the current actor and required terrain.
    /// Real field writes remain reached if a later table/division fails.
    /// Returning success establishes this consumer, not the enclosing +24.
    pub fn tick_selector12_motion<H: Selector12TerrainHost>(
        &mut self,
        actor: ObjectId,
        request: Selector12MotionRequest<'_, H>,
    ) -> Result<(), Selector12MotionError<H::Error>> {
        self.selector12_actor(actor)
            .map_err(|_| Selector12MotionError::UnsupportedClass(actor))?;
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ActorMotionError::MissingPosition(actor))?;
        let terrain = request
            .terrain
            .terrain_height([position[0], position[2]])
            .map_err(Selector12MotionError::Terrain)?;
        let separation = i32::from(position[1] as i16).wrapping_sub(i32::from(terrain));
        let step = request.microseconds >> 5;
        let mask;
        if (self.actor(actor)?.word_c4 as i32) >= 3 {
            if request.surface_snap_53a128 != 0 {
                // 00467E43's position write precedes the actual IDIV. A
                // delta below 32 therefore leaves snapped Y before failing.
                let base = &mut self.owned_mut(actor)?.state.base;
                let position = base.position_words.as_mut().expect("position already read");
                position[1] = position[1].wrapping_sub(separation as u16);
                let vertical = super::kinematics::divide(
                    separation.wrapping_neg().wrapping_shl(13),
                    step as i32,
                    0x0046_7e4d,
                )?;
                self.actor_mut(actor)?.words_8e_92[1] = vertical as u16;
            }
            if request.surface_snap_53a128 != 0 || separation == 0 {
                // The second sample uses the reached YZ-rotated X.y,
                // including a possible counter-triggered renormalization.
                let y = f32::from_bits(self.actor(actor)?.rotating.basis.words[8]);
                let angle = inverse_sine_q15(ftol(f64::from(y) * 32768.0), request.inverse_sine);
                self.rotate_visual(actor, RotationPlane::Yz, angle, request.math)?;
                let y = f32::from_bits(self.actor(actor)?.rotating.basis.words[2]);
                let angle = inverse_sine_q15(ftol(f64::from(y) * 32768.0), request.inverse_sine)
                    .wrapping_neg();
                self.rotate_visual(actor, RotationPlane::Xy, angle, request.math)?;
                let table = self.selector12_forward_table(actor)?;
                self.apply_actor_forward_motion(
                    actor,
                    ActorForwardMotionRequest {
                        power: 0xffff,
                        desired_velocity: request.forward_control,
                        step_units: step,
                        table,
                    },
                )?;
                let table = self.selector12_angular_table(actor)?;
                self.apply_actor_angular_motion(
                    actor,
                    ActorAngularMotionRequest {
                        power: 0xffff,
                        desired_turn: request.turn_control,
                        step_units: step,
                        table,
                        axis: ActorAngularAxis::LocalY,
                    },
                )?;
                let table = self.selector12_lateral_table(actor)?;
                self.apply_actor_lateral_motion(
                    actor,
                    ActorLateralMotionRequest {
                        power: 0xffff,
                        step_units: step,
                        table,
                    },
                )?;
            }
            mask = 0x1e;
        } else {
            let clearance = separation.wrapping_sub(10);
            if clearance > 0 {
                mask = 0x1f;
            } else {
                let base = &mut self.owned_mut(actor)?.state.base;
                let position = base.position_words.as_mut().expect("position already read");
                position[1] = position[1].wrapping_sub(clearance as u16);
                let velocity = &mut self.actor_mut(actor)?.words_8e_92;
                let vertical = i32::from(velocity[1] as i16);
                if vertical < 0 {
                    velocity[1] = (-(vertical / 2)) as u16;
                }
                for (axis, threshold) in [64, 24, 64].into_iter().enumerate() {
                    velocity[axis] = ground_velocity(velocity[axis], step, threshold);
                }
                mask = 0x1e;
            }
        }
        self.tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: request.microseconds,
                mask,
                sine_quarter: request.sine_quarter,
            },
        )?;
        Ok(())
    }
}

fn ground_velocity(word: u16, step: u32, threshold: i32) -> u16 {
    let velocity = i32::from(word as i16);
    if velocity == 0 {
        return word;
    }
    // 00467F53/F8A/FC1 use SHR, even for negative signed velocity.
    let decrement = ((velocity as u32).wrapping_mul(step) >> 14) as i32;
    let clear = if velocity > 0 {
        velocity <= threshold || velocity <= decrement
    } else {
        velocity >= -threshold || velocity >= decrement
    };
    if clear {
        0
    } else {
        word.wrapping_sub(decrement as u16)
    }
}

#[cfg(test)]
#[path = "selector12_motion_tests.rs"]
mod tests;
