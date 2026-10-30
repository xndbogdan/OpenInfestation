//! 0044C4A0's shared near-strip terrain particle births. The process event
//! gate belongs to 0044AA10; this producer knows no mission or screen IDs.

use crate::{
    name_entry::name_entry_rng_next,
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest},
    terrain_runtime::TerrainRuntime,
};

use super::TerrainRow;

#[derive(Clone, Copy)]
pub struct TerrainParticleEmissionRequest<'a> {
    pub terrain: &'a TerrainRuntime,
    /// Native context+04/+0C origins; only their low WORDs are consumed.
    pub context_position: [i32; 3],
    pub rows: &'a [TerrainRow],
}

/// Preserve row/birth order and the reached RNG prefix when a required birth
/// fails. The birth consumes its own RNG through the supplied same process
/// word; collecting packets before calling it would change every later draw.
pub fn emit_terrain_particles<'a, E>(
    request: TerrainParticleEmissionRequest<'_>,
    inputs: ParticleBirthInputs<'a>,
    rng: &mut u32,
    mut birth: impl FnMut(ParticleBirthRequest<'a>, &mut u32) -> Result<(), E>,
) -> Result<usize, E> {
    let spacing = request.terrain.sampling().spacing.units();
    let origin_x = request.context_position[0] as u16;
    let origin_z = request.context_position[2] as u16;
    let mut births = 0;
    for row in request.rows {
        if name_entry_rng_next(rng) & 0xffff >= u32::from(spacing) << 4 {
            continue;
        }
        let left = (row.left as u16)
            .wrapping_mul(spacing)
            .wrapping_add(origin_x);
        let span = (row.right as u16)
            .wrapping_mul(spacing)
            .wrapping_sub(left)
            .wrapping_add(origin_x) as i16;
        let z = (row.z.wrapping_sub(2) as u16)
            .wrapping_mul(spacing)
            .wrapping_add(origin_z);
        let count = (i32::from(span).abs() >> 12) + 2;
        for _ in 0..count {
            // The decompiler loses ECX's count of trailing one bits; the PE
            // at44C56F..57F selects the literal stack table6/9/10/11.
            let selector = (name_entry_rng_next(rng) & 0xffff).trailing_ones() & 3;
            let particle_type = [6, 9, 10, 11][selector as usize];
            let x = left.wrapping_add(
                ((name_entry_rng_next(rng) as i32).wrapping_mul(i32::from(span)) >> 16) as u16,
            );
            let z = z.wrapping_add(
                ((name_entry_rng_next(rng) as i32).wrapping_mul(i32::from(spacing)) >> 15) as u16,
            );
            //44C602 reads the signed cell byte directly, without triangle
            // interpolation or water-plane clamping.
            let y = request.terrain.raw_height_at([x, z]) as u16;
            let velocity = [
                32u16.wrapping_sub((name_entry_rng_next(rng) as u16) >> 11),
                ((name_entry_rng_next(rng) as u16) >> 9).wrapping_add(64),
                32u16.wrapping_sub((name_entry_rng_next(rng) as u16) >> 11),
            ];
            birth(
                ParticleBirthRequest {
                    particle_type,
                    position: [x, y, z],
                    velocity: Some(velocity),
                    flags: 0x800,
                    inputs,
                },
                rng,
            )?;
            births += 1;
        }
    }
    Ok(births)
}

#[cfg(test)]
mod tests;
