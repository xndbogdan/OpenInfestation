//! Complete Selector12 point update469210 -> final+148=467390.
//! Timer, captured target refresh and shared RNG precede the fresh148 kind gate.
//! This owner never initializes or recomputes retained actor14C.
use super::kinematics::divide;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorMotionError, DerivedActorError, NativeHeapRef, ObjectArena, ObjectError, ObjectId,
    },
};
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

#[derive(Debug)]
pub struct Selector12PointUpdateRequest<'a> {
    pub microseconds: u32,
    /// Existing canonical target refresh input. Represented targets do not
    /// mutate settings; arbitrary native callbacks remain outside this domain.
    pub settings_7c: u32,
    pub rng: &'a mut u32,
}

#[derive(Debug, Error)]
pub enum Selector12PointUpdateError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error("selector12 {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires cached+08 point at {address:08X}")]
    RequiredTargetFocus {
        reference: NativeHeapRef,
        address: u32,
    },
}

impl ObjectArena {
    pub fn update_selector12_point_target(
        &mut self,
        actor: ObjectId,
        request: Selector12PointUpdateRequest<'_>,
    ) -> Result<(), Selector12PointUpdateError> {
        self.selector12_actor(actor)?;
        let timer = self.family428650_actor(actor)?.words_188_19c[1].ok_or(
            Selector12PointUpdateError::RequiredField {
                actor,
                offset: 0x18c,
                address: 0x0046_739a,
            },
        )?;
        if (timer as i32) > (request.microseconds as i32) {
            self.family428650_actor_mut(actor)?.words_188_19c[1] =
                Some(timer.wrapping_sub(request.microseconds));
            return Ok(());
        }
        //1EC is tested before1DC. The NULL pair does not reset the timer.
        if self.family428650_actor(actor)?.targets_1d4_1fc[3].is_none()
            && self.family428650_actor(actor)?.targets_1d4_1fc[1].is_none()
        {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(1_000_000);
        let draw = name_entry_rng_next(request.rng) as u16;
        let timer = self.family428650_actor(actor)?.words_188_19c[1].ok_or(
            Selector12PointUpdateError::RequiredField {
                actor,
                offset: 0x18c,
                address: 0x0046_73dc,
            },
        )?;
        //Native captures fresh1EC before publishing the added timer.
        let right = self.family428650_actor(actor)?.targets_1d4_1fc[3];
        self.family428650_actor_mut(actor)?.words_188_19c[1] =
            Some(timer.wrapping_add(u32::from(draw) << 4));
        let selected = if let Some(right) = right {
            let left = self.family428650_actor(actor)?.targets_1d4_1fc[1];
            match left {
                Some(left) => {
                    let right_score = self.family428650_actor(actor)?.words_1d8_200[3];
                    let left_score = self.family428650_actor(actor)?.words_1d8_200[1];
                    if right_score > left_score {
                        Some(right)
                    } else {
                        Some(left)
                    }
                }
                None => Some(right),
            }
        } else {
            self.family428650_actor(actor)?.targets_1d4_1fc[1]
        }
        .ok_or(Selector12PointUpdateError::RequiredField {
            actor,
            offset: 0x1dc,
            address: 0x0046_7426,
        })?;
        let mut target = self.native_target(selected)?;
        let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
        //Write back to the captured allocation even if virtual18 stopped.
        self.write_native_target(selected, target)?;
        refreshed?; //Native ignores the normal return value.
        let position = self.object(actor)?.base.position_words.ok_or(
            Selector12PointUpdateError::RequiredField {
                actor,
                offset: 0x34,
                address: 0x0046_743d,
            },
        )?;
        let focus = self.native_target(selected)?.raw_focus_08_words().ok_or(
            Selector12PointUpdateError::RequiredTargetFocus {
                reference: selected,
                address: 0x0046_7445,
            },
        )?;
        let delta: [i16; 3] =
            std::array::from_fn(|axis| position[axis].wrapping_sub(focus[axis]) as i16);
        let squared = square(delta[0])
            .wrapping_add(square(delta[1]))
            .wrapping_add(square(delta[2]));
        let mut distance = distance(squared);
        let (mut x, mut z);
        if (distance as i32) > 256 {
            let shift = divide(14, distance as i32, 0x0046_74ac)?;
            x = word_shl(delta[0] as u16, shift);
            z = word_shl(delta[2] as u16, shift);
            let draw_x = name_entry_rng_next(request.rng) as u16;
            let product = u32::from(draw_x).wrapping_mul((x as i16 as i32) as u32);
            x = x.wrapping_add(((product as i32) >> 16) as u16);
            let draw_z = name_entry_rng_next(request.rng) as u16;
            let product = u32::from(draw_z).wrapping_mul((z as i16 as i32) as u32);
            z = z.wrapping_add(((product as i32) >> 16) as u16);
        } else {
            let odd = distance & 1 != 0;
            let draw_x = (name_entry_rng_next(request.rng) as u16) >> 3;
            x = if odd {
                draw_x.wrapping_add(128)
            } else {
                0xff80_u16.wrapping_sub(draw_x)
            };
            let draw_z = (name_entry_rng_next(request.rng) as u16) >> 3;
            z = if odd {
                draw_z.wrapping_add(128)
            } else {
                0xff80_u16.wrapping_sub(draw_z)
            };
            //The close branch retains originalY, then squares newZ and newX.
            distance = self::distance(
                square(delta[1])
                    .wrapping_add(square(z as i16))
                    .wrapping_add(square(x as i16)),
            );
            let shift = divide(14, distance as i32, 0x0046_7576)?;
            x = word_shl(x, shift);
            z = word_shl(z, shift);
        }
        //Fresh actual XYZ precedes190, which precedes fresh148 at467592.
        let position = self.object(actor)?.base.position_words.ok_or(
            Selector12PointUpdateError::RequiredField {
                actor,
                offset: 0x32,
                address: 0x0046_7580,
            },
        )?;
        self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(distance);
        let current = self.derived_actor(actor)?.target_148.ok_or(
            Selector12PointUpdateError::RequiredField {
                actor,
                offset: 0x148,
                address: 0x0046_75a3,
            },
        )?;
        let mut current_target = self.native_target(current)?;
        if current_target.kind_word() == 8 {
            current_target.set_focus_08_words([
                position[0].wrapping_add(x),
                position[1],
                position[2].wrapping_add(z),
            ]);
            self.write_native_target(current, current_target)?;
        }
        Ok(())
    }
}

fn square(word: i16) -> u32 {
    let value = i32::from(word);
    value.wrapping_mul(value) as u32
}
fn distance(squared: u32) -> u32 {
    if squared & 0x8000_0000 != 0 {
        isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
    } else {
        isqrt_48efc0(squared as i32)
    }
}
fn word_shl(word: u16, count: i32) -> u16 {
    //x86 masks a WORD SHL count to five bits, not the Rust u16 four-bit mask.
    (u32::from(word).wrapping_shl((count as u32) & 31)) as u16
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
