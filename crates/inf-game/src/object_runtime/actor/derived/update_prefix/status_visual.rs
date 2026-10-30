//! Complete 0047DFD0 phase reached synchronously by 0047C990. Actual trail
//! allocation/publication stays at the existing required family host boundary.
//! A burning grenade (payload flag 4) reaches the same visual from its own
//! update, through [`ObjectArena::status_visual_trail`].

use super::{FamilyUpdateEffect, FamilyUpdateError, FamilyUpdateHost};
use crate::object_runtime::{ObjectArena, ObjectId};

/// The 00489D90 packet 0047DFD0 births, with flags 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusTrail {
    pub particle_type: u32,
    pub position: [u16; 3],
    pub velocity: [u16; 3],
}

impl ObjectArena {
    pub fn tick_status_visual<H: FamilyUpdateHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), FamilyUpdateError<H::Error>> {
        let Some(trail) =
            self.status_visual_trail(actor, microseconds, load_factor, || host.random())?
        else {
            return Ok(());
        };
        self.family_update_effect(
            actor,
            FamilyUpdateEffect::StatusTrail {
                particle_type: trail.particle_type,
                position: trail.position,
                velocity: trail.velocity,
                flags: 0,
            },
            host,
        )
    }

    /// 0047DFD0 up to its trail birth, drawing from `random` (0048F060):
    /// the packet, or none when the load factor's quota passes it over.
    pub fn status_visual_trail<E>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        load_factor: Option<i32>,
        mut random: impl FnMut() -> Result<u32, E>,
    ) -> Result<Option<StatusTrail>, FamilyUpdateError<E>> {
        let mut visual_random = |address: u32| {
            random().map_err(|error| FamilyUpdateError::StatusVisualRandom { address, error })
        };
        //0047DFD9 captures this size before marking dirty and reading the
        // process operand. Do not preflight position, RNG or particle service.
        let size = self.object(actor)?.base.word_28;
        self.owned_mut(actor)?.state.base.dirty_word |= 0x400;
        let load_factor = load_factor.ok_or(FamilyUpdateError::MissingStatusLoadFactor(actor))?;
        let probability = visual_divide(
            actor,
            load_factor.wrapping_mul(100),
            i32::from(size).wrapping_shl(14),
            0x0047_dff5,
        )?;
        if probability >= 2 {
            let draw = visual_random(0x0047_dffe)? & 0xffff;
            if (draw as i32) % probability != 0 {
                return Ok(None);
            }
        }

        //0047E013..31: only after admission are XYZ, raw delta>>5 and a
        // newly read half-size captured. The birth callback consumes copies.
        let mut position =
            self.object(actor)?
                .base
                .position_words
                .ok_or(FamilyUpdateError::Uninitialized {
                    actor,
                    offset: 0x32,
                })?;
        let half_size = i32::from(self.object(actor)?.base.word_28 >> 1);
        let step = microseconds >> 5;
        let draw = visual_random(0x0047_e036)? & 0xffff;
        if draw & 0x0f == 0 {
            let state = self.actor_mut(actor)?;
            state.word_c8 = 15_625;
            state.word_ca = Some(55);
        }

        let particle_type = 9 + u32::from(self.object(actor)?.base.word_28 > 200);
        if i32::from(self.actor(actor)?.words_8e_92[0] as i16).abs() < 300
            && i32::from(self.actor(actor)?.words_8e_92[2] as i16).abs() < 300
        {
            let center = half_size / 2;
            for (axis, address, division) in
                [(0, 0x0047_e0a2, 0x0047_e0ad), (2, 0x0047_e0b6, 0x0047_e0c1)]
            {
                let draw = (visual_random(address)? & 0xffff) as i32;
                // IDIV produces a remainder with the dividend's sign. The
                // actual unsigned WORD draw is nonnegative here.
                visual_divide::<E>(actor, draw, half_size, division)?;
                let jitter = (draw % half_size).wrapping_sub(center);
                position[axis] = position[axis].wrapping_add(jitter as u16);
            }
        }

        //0047E0CE..ED uses current signed X/Z velocity, with wrapping IMUL,
        // arithmetic SAR15 and low-WORD subtraction. Captured Y is unchanged.
        for axis in [0, 2] {
            let velocity = i32::from(self.actor(actor)?.words_8e_92[axis] as i16);
            let movement = velocity.wrapping_mul(step as i32) >> 15;
            position[axis] = position[axis].wrapping_sub(movement as u16);
        }

        let x_draw = (visual_random(0x0047_e0f2)? & 0x7f) as i32;
        let x = i32::from(self.actor(actor)?.words_8e_92[0] as i16).wrapping_mul(3) / 4;
        let y = i32::from(self.actor(actor)?.words_8e_92[1] as i16).wrapping_mul(3) / 4;
        let z_draw = (visual_random(0x0047_e13f)? & 0x7f) as i32;
        let z = i32::from(self.actor(actor)?.words_8e_92[2] as i16).wrapping_mul(3) / 4;
        Ok(Some(StatusTrail {
            particle_type,
            position,
            velocity: [
                x.wrapping_add(x_draw).wrapping_sub(64) as u16,
                y as u16,
                z.wrapping_add(z_draw).wrapping_sub(64) as u16,
            ],
        }))
    }
}

fn visual_divide<E>(
    actor: ObjectId,
    numerator: i32,
    denominator: i32,
    address: u32,
) -> Result<i32, FamilyUpdateError<E>> {
    numerator
        .checked_div(denominator)
        .ok_or(FamilyUpdateError::StatusVisualDivision {
            actor,
            address,
            numerator,
            denominator,
        })
}
