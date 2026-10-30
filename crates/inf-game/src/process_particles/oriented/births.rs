//! Authored animation effects reached by pickups and attached smoke helpers.
use super::*;
use crate::process_particles::{
    births::ParticleBirthProducer, ParticleAllocationRequest, ParticleConstruction, ParticleId,
};

impl ProcessParticleRuntime {
    pub fn birth_falling_drop(
        &mut self,
        mut request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        // Native requires the velocity pointer before allocation. These three
        // random draws survive an allocation/source failure.
        let old = request
            .velocity
            .ok_or(super::super::ParticleError::RequiredBirthVelocity {
                address: 0x0048_9920,
            })?;
        request.velocity = Some(std::array::from_fn(|i| {
            old[i]
                .wrapping_add((((name_entry_rng_next(rng) & 0xffff) * 100) >> 15) as u16)
                .wrapping_sub(50)
        }));
        let id = self.birth_effect_animation(
            request,
            0x0053_d7b8,
            1,
            0x34,
            ParticleBirthProducer::FallingDrop,
            executable,
        )?;
        let record = self.record_mut(id)?;
        let scale = ((name_entry_rng_next(rng) & 0xffff) >> 9) + 192;
        record.write_dword(0x30, 0)?;
        record.write_dword(0x2c, scale)?;
        let life = record.read_word(0x10)?;
        record.write_dword(0, 0x0052_87f4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0, 0x0052_8834)?;
        record.write_word(
            0x10,
            life.wrapping_add(((random >> 1) * u32::from(life >> 1) >> 15) as u16),
        )?;
        Ok(Some(id))
    }
    /// `00489CB0`: the ground contact's dust, its animation table chosen by
    /// the terrain material's selector (`0053D840`).
    pub fn birth_material_dust(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let bytes = executable
            .mapped(
                0x0053_d840u32.wrapping_add(request.particle_type.wrapping_mul(4)),
                4,
            )?
            .bytes;
        let table = u32::from_le_bytes(bytes.try_into().expect("DWORD"));
        let id = self.birth_effect_animation(
            request,
            table,
            0,
            0x34,
            ParticleBirthProducer::MaterialDust,
            executable,
        )?;
        let record = self.record_mut(id)?;
        let scale = ((name_entry_rng_next(rng) & 0xffff) >> 9) + 192;
        record.write_dword(0x30, 0)?;
        record.write_dword(0x2c, scale)?;
        record.write_dword(0, 0x0052_8894)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        let life = record.read_word(0x10)?;
        record.write_word(0x10, ((random >> 1) * u32::from(life >> 2) >> 15) as u16)?;
        Ok(Some(id))
    }
    pub fn birth_pickup_flash(
        &mut self,
        mut request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let (table, base) = match request.particle_type {
            0 => (0x0053_d4e0, 0x400),
            2 => (0x0053_d6b8, 0x400),
            3 => (0x0053_d6b8, 0x200),
            4 => (0x0053_ca68, 0x400),
            5 => (0x0053_d650, 0x400),
            6 => (0x0053_d660, 0x400),
            _ => return Ok(None),
        };
        request.velocity = None;
        let id = self.birth_effect_animation(
            request,
            table,
            1,
            0x34,
            ParticleBirthProducer::PickupFlash,
            executable,
        )?;
        let record = self.record_mut(id)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0x30, 0)?;
        record.write_dword(0, 0x0052_8854)?;
        record.write_dword(0x2c, (random >> 9) + base - 64)?;
        Ok(Some(id))
    }
    /// 00489BC0. This class survives one update and retires on its second;
    /// the native +34 counter is independent of the animation lifetime.
    pub fn birth_short_smoke(
        &mut self,
        mut request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let table = match request.particle_type {
            0 => 0x0053_d830,
            1 => 0x0053_d820,
            _ => return Ok(None),
        };
        request.velocity = None;
        let id = self.birth_effect_animation(
            request,
            table,
            1,
            0x38,
            ParticleBirthProducer::ShortSmoke,
            executable,
        )?;
        let random = name_entry_rng_next(rng) & 0xffff;
        let record = self.record_mut(id)?;
        record.write_dword(0x30, 0)?;
        record.write_dword(0x34, 1)?;
        record.write_dword(0x2c, (random >> 9) + 0x3c0)?;
        record.write_dword(0, 0x0052_8874)?;
        Ok(Some(id))
    }

    /// 0048AD70. Both authored tables produce terrain lighting only; +08 is
    /// 0048AE60 rather than the ordinary sprite draw wrapper.
    pub fn birth_terrain_flash(
        &mut self,
        mut request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let table = match request.particle_type {
            0 => 0x0053_d610,
            1 => 0x0053_d620,
            _ => return Ok(None),
        };
        request.velocity = None;
        let id = self.birth_effect_animation(
            request,
            table,
            1,
            0x34,
            ParticleBirthProducer::TerrainFlash,
            executable,
        )?;
        let random = name_entry_rng_next(rng) & 0xffff;
        let record = self.record_mut(id)?;
        record.write_dword(0x30, 0)?;
        record.write_dword(0, 0x0052_8974)?;
        record.write_dword(0x2c, (random >> 9) + 0x1c0)?;
        Ok(Some(id))
    }

    fn birth_effect_animation(
        &mut self,
        request: ParticleBirthRequest<'_>,
        table: u32,
        category: usize,
        payload_bytes: u32,
        producer: ParticleBirthProducer,
        executable: FrontendExecutable<'_>,
    ) -> Result<ParticleId, ParticleBirthError<Infallible>> {
        let id = self.allocate(ParticleAllocationRequest {
            payload_bytes,
            last_reclaim_category: 1,
            network_active: request.inputs.network_active,
            phase: request.inputs.phase,
        })?;
        let source_handle = request.inputs.source_handle(producer)?;
        self.construct(
            id,
            ParticleConstruction {
                category,
                position: request.position.into(),
                velocity: request.velocity.map(Into::into),
                source_handle,
                flags: request.flags,
            },
        )?;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, table)?;
        record.write_dword(0, 0x0052_6f40)?;
        let bytes = executable.mapped(table.wrapping_add(12), 2)?.bytes;
        record.write_word(
            0x10,
            u16::from_le_bytes(bytes.try_into().expect("WORD")).wrapping_sub(1),
        )?;
        record.write_dword(0, 0x0052_87d4)?;
        Ok(id)
    }
}
