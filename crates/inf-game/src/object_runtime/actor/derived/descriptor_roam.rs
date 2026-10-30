//! Roam descriptor update 0043FBC0 -> 00427800. This owner does not attach
//! the enclosing actor +24 or supply allocation-carried timers +220/+230.

use inf_formats::native_basis::sine_q15;
use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{ActorTerrainHost, ObjectArena, ObjectError, ObjectId},
};

pub struct FamilyRoamUpdateRequest<'a> {
    /// Native delta DWORD; the timer comparison interprets it as signed.
    pub microseconds: u32,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyRoamUpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its unwritten Roam timer +18C")]
    MissingTimer(ObjectId),
    #[error("actor {0:?} requires its position at 004278E7")]
    MissingPosition(ObjectId),
    #[error("actor {0:?} requires its current target at 0042795B")]
    MissingTarget(ObjectId),
    #[error("Roam H12 terrain acquisition failed: {0}")]
    Terrain(E),
}

impl ObjectArena {
    /// Timer decay bypasses every other source. Expiry adds the period to
    /// the old timer, consumes two shared process draws, and samples raw H12
    /// height before testing actual target kind +10. Errors retain prefixes.
    pub fn update_family_roam_target<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyRoamUpdateRequest<'_>,
        process_rng: &mut u32,
        host: &mut H,
    ) -> Result<(), FamilyRoamUpdateError<H::Error>> {
        let timer = self.family428650_actor(actor)?.words_188_19c[1]
            .ok_or(FamilyRoamUpdateError::MissingTimer(actor))?;
        if (timer as i32) > (request.microseconds as i32) {
            self.family428650_actor_mut(actor)?.words_188_19c[1] =
                Some(timer.wrapping_sub(request.microseconds));
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(2_000_000));
        let first = name_entry_rng_next(process_rng) & 0xffff;
        let timer = self.family428650_actor(actor)?.words_188_19c[1]
            .expect("period store precedes the pure process RNG draw");
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(first << 6));
        let second = name_entry_rng_next(process_rng) & 0xffff;
        let quarter_angle = (0x8000i32 - second as i32) >> 2;
        let angle = if quarter_angle > 0 {
            quarter_angle + 4096
        } else {
            quarter_angle - 4096
        };
        let local = [
            (sine_q15(angle, request.sine_quarter) >> 2) as u16,
            0,
            (sine_q15(angle + 16384, request.sine_quarter) >> 2) as u16,
        ];
        let transformed = self.transform_visual_vector_words(actor, local)?;
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(FamilyRoamUpdateError::MissingPosition(actor))?;
        let xz = [
            transformed[0].wrapping_add(position[0]),
            transformed[2].wrapping_add(position[2]),
        ];
        // X/Z are captured before the mutable H12 acquisition; the current
        // +148 target is captured only after it returns (00427937).
        let terrain = host
            .terrain(self, actor)
            .map_err(FamilyRoamUpdateError::Terrain)?;
        // 00427937 captures the then-current target before reading the cell.
        // A null pointer still reaches height sampling before its kind read.
        let target = self.derived_actor(actor)?.target_148;
        let height = terrain.raw_height_at(xz) as u16;
        let reference = target.ok_or(FamilyRoamUpdateError::MissingTarget(actor))?;
        let mut target = self.native_target(reference)?;
        if target.kind_word() == 8 {
            target.set_focus_08_words([xz[0], height, xz[1]]);
            self.write_native_target(reference, target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
