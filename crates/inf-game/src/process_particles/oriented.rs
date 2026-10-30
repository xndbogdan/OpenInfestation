//! 00488E90 oriented emission. The terrain owner supplies its current signed
//! low-WORD plane after the native getter; each lower birth keeps the live RNG.
use super::{
    births::{ParticleBirthError, ParticleBirthInputs, ParticleBirthRequest},
    ProcessParticleRuntime,
};
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::native_trunc_low32;
use inf_formats::{
    native_basis::{sine_q15, NativeBasis},
    FrontendExecutable,
};
use std::convert::Infallible;

#[cfg(test)]
mod birth_tests;
mod births;
#[cfg(test)]
mod tests;

pub struct OrientedParticleRequest<'a> {
    pub position: &'a [u16; 3],
    pub velocity: &'a [u16; 3],
    pub basis: &'a NativeBasis,
    pub strength: i32,
    pub load_factor: i32,
    pub plane: i16,
    pub sine_quarter: &'a [u16; 4096],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emission {
    Bubble,
    FallingDrop,
}

impl ProcessParticleRuntime {
    pub fn emit_oriented_particles(
        &mut self,
        request: OrientedParticleRequest<'_>,
        inputs: ParticleBirthInputs<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleBirthError<Infallible>> {
        // Native float multiplier 00520A64 is read only by the bubble arm.
        emit(
            &request,
            || {
                let bytes = executable.mapped(0x0052_0a64, 4)?.bytes;
                Ok::<_, ParticleBirthError<Infallible>>(f32::from_le_bytes(
                    bytes.try_into().expect("float"),
                ))
            },
            rng,
            |kind, position, velocity, rng| {
                let birth = ParticleBirthRequest {
                    particle_type: 0,
                    position,
                    velocity: Some(velocity),
                    flags: 0,
                    inputs,
                };
                match kind {
                    Emission::Bubble => {
                        self.birth_impact(birth, executable, rng)?;
                    }
                    Emission::FallingDrop => {
                        self.birth_falling_drop(birth, executable, rng)?;
                    }
                }
                Ok(())
            },
        )
    }
}

fn emit<E>(
    request: &OrientedParticleRequest<'_>,
    scale: impl FnOnce() -> Result<f32, E>,
    rng: &mut u32,
    mut birth: impl FnMut(Emission, [u16; 3], [u16; 3], &mut u32) -> Result<(), E>,
) -> Result<(), E> {
    let scaled = (request.strength >> 7).wrapping_mul(request.load_factor);
    let axis = |word: usize| f64::from(f32::from_bits(request.basis.words[word]));
    if (request.position[1] as i16) > request.plane {
        let masked = scaled & !0x7fff;
        let count = if masked > 0x40000 {
            8
        } else if masked < 0x8000 {
            1
        } else {
            scaled >> 15
        };
        let magnitude = i32::from((request.strength / 4).wrapping_neg() as i16);
        let velocity =
            std::array::from_fn(|i| native_trunc_low32(f64::from(magnitude) * axis(4 + i)) as u16);
        for _ in 0..count.max(0) {
            birth(Emission::FallingDrop, *request.position, velocity, rng)?;
        }
    } else {
        let mut count = scaled >> 16;
        let sign = if count < 0 { -1i32 } else { 1 };
        if count == 0 {
            count = 1;
        } else if count < 0 {
            count = count.wrapping_neg();
        }
        let y = request.position[1].wrapping_sub(50);
        let factor = f64::from(scale()?);
        let base: [u16; 3] = std::array::from_fn(|i| {
            request.velocity[i].wrapping_sub(
                (native_trunc_low32(axis(4 + i) * factor) as i32).wrapping_mul(if i == 1 {
                    sign
                } else {
                    1
                }) as u16,
            )
        });
        let z_x = native_trunc_low32(axis(7) * factor) as i32;
        let x_x = native_trunc_low32(axis(1) * factor) as i32;
        let z_z = native_trunc_low32(axis(9) * factor) as i32;
        let x_z = native_trunc_low32(axis(3) * factor) as i32;
        for _ in 0..count as u32 {
            let angle = (name_entry_rng_next(rng) & 0xffff) as i32;
            let sine = sine_q15(angle, request.sine_quarter);
            let cosine = sine_q15(angle.wrapping_add(0x4000), request.sine_quarter);
            let x = request.position[0]
                .wrapping_add(((name_entry_rng_next(rng) & 0xffff) >> 9) as u16)
                .wrapping_sub(64);
            let z = request.position[2]
                .wrapping_add(((name_entry_rng_next(rng) & 0xffff) >> 9) as u16)
                .wrapping_sub(64);
            let vy = base[1]
                .wrapping_add(((name_entry_rng_next(rng) & 0xffff) >> 7) as u16)
                .wrapping_sub(256);
            let vx = base[0]
                .wrapping_add((sine.wrapping_mul(x_x) >> 15) as u16)
                .wrapping_add((sine.wrapping_mul(z_x) >> 15) as u16);
            let vz = base[2]
                .wrapping_add((cosine.wrapping_mul(x_z) >> 15) as u16)
                .wrapping_add((cosine.wrapping_mul(z_z) >> 15) as u16);
            birth(Emission::Bubble, [x, y, z], [vx, vy, vz], rng)?;
        }
    }
    Ok(())
}
