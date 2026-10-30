//!Complete 0042DF40 retained emitter, with current process camera and terrain
//!services. Period and actor storage are reread after real trail births.
use super::*;
use crate::{name_entry::name_entry_rng_next, object_runtime::RuntimeAllocationError};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector86Trail {
    pub particle_type: u32,
    pub position: [u16; 3],
}

pub trait Selector86UpdateHost {
    type Error;
    ///00410F10 reads retained5469E8/EA/EC, not an acquired camera object's XYZ.
    fn cached_camera_position(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<[u16; 3], Self::Error>;
    ///Live450A30 and the inline terrain/water maximum. Coordinates are the
    ///captured native stack values; this service may mutate shared owners.
    fn surface_height(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        xz: [u16; 2],
    ) -> Result<i16, Self::Error>;
    ///489D90 receives velocity[0,100,0], flags0800 and the captured type/XYZ.
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        trail: Selector86Trail,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector86UpdateError<E> {
    #[error("selector86 {0:?} update previously stopped after reached writes")]
    PreviouslyStopped(ObjectId),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Allocation(#[from] RuntimeAllocationError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("selector86 {0:?} requires current retained +40 data")]
    MissingData(ObjectId),
    #[error("selector86 {object:?} service {phase} failed: {error}")]
    Host {
        object: ObjectId,
        phase: &'static str,
        error: E,
    },
    #[error("selector86 {0:?} reached native IDIV with radius zero after its first RNG draw")]
    ZeroRadius(ObjectId),
    #[error("selector86 {0:?} reached a zero-period repeated emission after its completed prefix")]
    NonProgressingZeroPeriod(ObjectId),
    #[error("selector86 {0:?} cannot sample a point inside its negative radius")]
    NonProgressingNegativeRadius(ObjectId),
}

impl ObjectArena {
    fn selector86_word<E>(
        &self,
        object: ObjectId,
        index: u32,
    ) -> Result<u16, Selector86UpdateError<E>> {
        let data = self
            .selector86(object)?
            .data_40
            .as_ref()
            .ok_or(Selector86UpdateError::MissingData(object))?;
        Ok(data.allocation.word(data.allocation_offset + index * 2)?)
    }

    pub fn tick_selector86<H: Selector86UpdateHost>(
        &mut self,
        object: ObjectId,
        microseconds: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector86UpdateError<H::Error>> {
        if self.selector86(object)?.update_stopped {
            return Err(Selector86UpdateError::PreviouslyStopped(object));
        }
        self.selector86_mut(object)?.update_stopped = true;
        let result = self.tick_selector86_inner(object, microseconds, executable, host, rng);
        if result.is_ok() {
            self.selector86_mut(object)?.update_stopped = false;
        }
        result
    }

    fn tick_selector86_inner<H: Selector86UpdateHost>(
        &mut self,
        object: ObjectId,
        microseconds: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector86UpdateError<H::Error>> {
        self.selector86(object)?;
        if self.object(object)?.base.flags & 0x0200_0000 == 0 {
            let point = self
                .object(object)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(object))?;
            let camera = host.cached_camera_position(self, object).map_err(|error| {
                Selector86UpdateError::Host {
                    object,
                    phase: "410F10 camera",
                    error,
                }
            })?;
            let distance = point.into_iter().zip(camera).fold(0u32, |sum, (a, b)| {
                let d = i32::from(a.wrapping_sub(b) as i16);
                sum.wrapping_add(d.wrapping_mul(d) as u32)
            });
            if distance >= self.selector86(object)?.distance_squared_48 {
                let interval = self.selector86_word(object, 3)? as i16 as i32 as u32;
                self.selector86_mut(object)?.elapsed_44 = interval;
                return Ok(());
            }
        }
        let elapsed = self
            .selector86(object)?
            .elapsed_44
            .wrapping_add(microseconds >> 10);
        self.selector86_mut(object)?.elapsed_44 = elapsed;
        let interval = self.selector86_word(object, 3)? as i16 as i32;
        if elapsed as i32 <= interval {
            return Ok(());
        }
        let radius = i32::from(self.selector86_word(object, 0)? as i16);
        let radius_squared = radius.wrapping_mul(radius);
        let selector = i32::from(self.selector86_word(object, 2)? as i16);
        let particle_type =
            executable.word(0x0053_1fb4u32.wrapping_add(selector.wrapping_mul(4) as u32))?;
        loop {
            let random = (name_entry_rng_next(rng) & 0xffff) as i32;
            let diameter = radius * 2;
            if diameter == 0 {
                return Err(Selector86UpdateError::ZeroRadius(object));
            }
            let dx = radius.wrapping_sub(random % diameter) as i16;
            let random = (name_entry_rng_next(rng) & 0xffff) as i32;
            let dz = radius.wrapping_sub(random % diameter) as i16;
            let squared = i32::from(dx)
                .wrapping_mul(i32::from(dx))
                .wrapping_add(i32::from(dz).wrapping_mul(i32::from(dz)));
            if squared < radius_squared {
                let interval = self.selector86_word(object, 3)? as i16 as i32 as u32;
                let point = self
                    .object(object)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(object))?;
                let xz = [
                    point[0].wrapping_add(dx as u16),
                    point[2].wrapping_add(dz as u16),
                ];
                self.selector86_mut(object)?.elapsed_44 =
                    self.selector86(object)?.elapsed_44.wrapping_sub(interval);
                let height = host.surface_height(self, object, xz).map_err(|error| {
                    Selector86UpdateError::Host {
                        object,
                        phase: "450A30 terrain/water",
                        error,
                    }
                })?;
                host.emit_trail(
                    self,
                    object,
                    Selector86Trail {
                        particle_type,
                        position: [xz[0], height as u16, xz[1]],
                    },
                    rng,
                )
                .map_err(|error| Selector86UpdateError::Host {
                    object,
                    phase: "489D90 trail",
                    error,
                })?;
            } else if (-10922..0).contains(&radius) {
                // In this range neither component can wrap and each squared
                // magnitude is >=radius². Native rejection cannot progress.
                return Err(Selector86UpdateError::NonProgressingNegativeRadius(object));
            }
            let interval = self.selector86_word(object, 3)? as i16 as i32;
            let elapsed = self.selector86(object)?.elapsed_44 as i32;
            if elapsed < interval {
                return Ok(());
            }
            //Portable containment of a malformed zero interval after retaining
            //the completed native RNG/callback prefix; no success or retry.
            if interval == 0 {
                return Err(Selector86UpdateError::NonProgressingZeroPeriod(object));
            }
        }
    }
}

#[cfg(test)]
mod tests;
