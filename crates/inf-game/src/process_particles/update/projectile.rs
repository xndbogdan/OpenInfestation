//! Shared projectile movement 00466250 and impact bubble 004896E0.

use super::*;

impl ProcessParticleRuntime {
    pub(super) fn update_projectile(
        &mut self,
        id: ParticleId,
        delta: i32,
    ) -> Result<(), ParticleError> {
        if self.record(id)?.read_byte(0x12)? & 0x0c == 0 {
            let life = i32::from(self.record(id)?.read_word(0x10)?);
            let next = life.wrapping_sub(delta >> 5).max(0);
            self.record_mut(id)?.write_word(0x10, next as u16)?;
        }
        self.integrate_velocity(id, delta, false)
    }

    /// Unlike465110, this consumer does not publish water-classification bits
    /// and never queries ground collision. A crossing births one splash before
    /// returning true to the manager's retirement path.
    pub(super) fn update_impact_bubble<E>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<bool, ParticleUpdateError<E>> {
        let terrain = request
            .terrain
            .as_ref()
            .ok_or(ParticleUpdateError::MissingTerrain { id })?;
        // The same water geometry as465110, without its classification writer.
        let water = self.measure_particle_water(id, 10, terrain)?;
        if water.classification >= 0 {
            self.record_mut(id)?.write_word(6, water.height as u16)?;
            let position = self.record(id)?.position()?;
            self.birth_splash(
                super::super::births::ParticleBirthRequest {
                    particle_type: 0,
                    position,
                    velocity: None,
                    flags: 0,
                    inputs: request.birth_inputs.borrowed(),
                },
                executable,
                rng,
            )?;
            return Ok(true);
        }
        let delta = request.delta_microseconds;
        let y_velocity = self.record(id)?.read_word(0x0c)?;
        self.record_mut(id)?
            .write_word(0x0c, y_velocity.wrapping_add((delta >> 10) as u16))?;
        for offset in [0x0a, 0x0e] {
            let velocity = self.record(id)?.read_word(offset)?;
            let loss = i32::from(velocity as i16).wrapping_mul(delta >> 2) >> 15;
            self.record_mut(id)?
                .write_word(offset, velocity.wrapping_sub(loss as u16))?;
        }
        self.integrate_velocity(id, delta, false)?;
        Ok(false)
    }
}
