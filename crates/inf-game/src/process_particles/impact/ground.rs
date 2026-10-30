//! 00489390 material lookup, 00488E00 ground burst and 004894A0 sprite birth.
use crate::{
    name_entry::name_entry_rng_next,
    process_particles::{
        births::{
            ParticleBirthError, ParticleBirthInputs, ParticleBirthProducer, ParticleBirthRequest,
        },
        ParticleAllocationRequest, ParticleConstruction, ParticleId, ProcessParticleRuntime,
    },
};
use inf_formats::FrontendExecutable;
use std::convert::Infallible;

impl ProcessParticleRuntime {
    pub fn birth_ground_impact(
        &mut self,
        selector: u32,
        position: [u16; 3],
        strength: i32,
        load_factor: i32,
        inputs: ParticleBirthInputs<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleBirthError<Infallible>> {
        let bytes = executable
            .mapped(0x0053_d7c8_u32.wrapping_add(selector.wrapping_mul(4)), 4)?
            .bytes;
        let particle_type = u32::from_le_bytes(bytes.try_into().expect("DWORD"));
        let scaled = (strength >> 8).wrapping_mul(load_factor) >> 16;
        let count = if scaled == 0 { 1 } else { scaled };
        for _ in 0..count.max(0) {
            //00464A40 increments the retained process cursor before the read;
            //it consumes no RNG and only lookup is reduced modulo100.
            self.vector_cursor = self.vector_cursor.wrapping_add(1);
            let bytes = executable
                .mapped(0x0053_9960 + (self.vector_cursor % 100) * 6, 6)?
                .bytes;
            let mut velocity: [u16; 3] = std::array::from_fn(|axis| {
                (i16::from_le_bytes([bytes[axis * 2], bytes[axis * 2 + 1]]) >> 2) as u16
            });
            if (velocity[1] as i16) < 0 {
                velocity[1] = velocity[1].wrapping_neg();
            }
            //A NULL type result does not stop488E00's count loop.
            self.birth_material_sprite(
                ParticleBirthRequest {
                    particle_type,
                    position,
                    velocity: Some(velocity),
                    flags: 0,
                    inputs,
                },
                true,
                executable,
                rng,
            )?;
        }
        Ok(())
    }

    pub fn birth_material_sprite(
        &mut self,
        request: ParticleBirthRequest<'_>,
        shadow: bool,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let table = match request.particle_type {
            0 => 0x0053_c830,
            1 => 0x0053_c868,
            2 => 0x0053_c8a0,
            3 => 0x0053_c900,
            4 => 0x0053_c960,
            6 => 0x0053_ca38,
            7 => 0x0053_ca08,
            _ => return Ok(None),
        };
        let id = self.allocate(ParticleAllocationRequest {
            payload_bytes: 0x34,
            last_reclaim_category: 1,
            network_active: request.inputs.network_active,
            phase: request.inputs.phase,
        })?;
        let source_handle = request
            .inputs
            .source_handle(ParticleBirthProducer::GroundImpact)?;
        self.construct(
            id,
            ParticleConstruction {
                category: 1,
                position: request.position.into(),
                velocity: request.velocity.map(Into::into),
                source_handle,
                flags: request.flags,
            },
        )?;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, table)?;
        record.write_dword(0, 0x0052_6f40)?;
        let bytes = executable.mapped(table + 12, 2)?.bytes;
        let lifetime = u16::from_le_bytes(bytes.try_into().expect("WORD")).wrapping_sub(1);
        record.write_word(0x10, lifetime)?;
        record.write_dword(0, 0x0052_87d4)?;
        record.write_dword(0x2c, ((name_entry_rng_next(rng) & 0xffff) >> 9) + 192)?;
        record.write_dword(0x30, if shadow { 256 } else { 0 })?;
        let lifetime = record.read_word(0x10)?;
        record.write_dword(0, 0x0052_87f4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_word(
            0x10,
            lifetime.wrapping_add(((random >> 1) * u32::from(lifetime >> 1) >> 15) as u16),
        )?;
        Ok(Some(id))
    }
}
