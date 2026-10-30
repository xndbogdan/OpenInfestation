//! 00466B30. The invisible parent keeps its own coordinates and emits trail10
//! through 00489D90; the manager alone owns parent lifetime and retirement.

use super::*;
use crate::process_particles::births::ParticleBirthRequest;

impl ProcessParticleRuntime {
    pub(super) fn update_emitter<E>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<bool, ParticleUpdateError<E>> {
        let spread = if self.record(id)?.read_dword(0x3c)? == 13 {
            50
        } else {
            400
        };
        let quotient = request.load_factor.wrapping_mul(100) / (spread << 14);
        if quotient >= 2 && (name_entry_rng_next(rng) & 0xffff) % quotient as u32 != 0 {
            return Ok(false);
        }
        let mut position = self.record(id)?.position()?;
        let delta = request.delta_microseconds >> 5;
        let vx = i32::from(self.record(id)?.read_word(0x0a)? as i16);
        let vz = i32::from(self.record(id)?.read_word(0x0e)? as i16);
        if vx.abs() < 300 && vz.abs() < 300 {
            position[0] = position[0].wrapping_add(
                ((name_entry_rng_next(rng) & 0xffff) as i32 % spread - spread / 2) as u16,
            );
            position[2] = position[2].wrapping_add(
                ((name_entry_rng_next(rng) & 0xffff) as i32 % spread - spread / 2) as u16,
            );
        }
        position[0] = position[0].wrapping_sub((vx.wrapping_mul(delta) >> 15) as u16);
        if self.record(id)?.read_dword(0x3c)? == 13 {
            position[1] = position[1].wrapping_add((delta.wrapping_shl(9) >> 15) as u16);
        }
        position[2] = position[2].wrapping_sub((vz.wrapping_mul(delta) >> 15) as u16);
        let mut velocity = [0u16; 3];
        for (axis, word) in velocity.iter_mut().enumerate() {
            let jitter = if axis != 1 {
                (name_entry_rng_next(rng) & 0xffff) as i32 % 128 - 64
            } else {
                0
            };
            let old = i32::from(self.record(id)?.read_word(0x0a + axis * 2)? as i16);
            *word = (old * 3 / 4 + jitter) as u16;
        }
        self.birth_trail(
            ParticleBirthRequest {
                particle_type: 10,
                position,
                velocity: Some(velocity),
                flags: 0,
                inputs: request.birth_inputs.borrowed(),
            },
            executable,
            rng,
        )?;
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
