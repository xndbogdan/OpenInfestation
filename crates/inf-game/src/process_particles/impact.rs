//! Shared projectile impact visual 00489600. Collision admission is separate:
//! 00465FA0 -> 00465570 -> actors, furniture, terrain, in that exact order.

use inf_formats::FrontendExecutable;

use super::{
    births::{ParticleBirthError, ParticleBirthProducer, ParticleBirthRequest},
    ParticleAllocationRequest, ParticleConstruction, ParticleId, ProcessParticleRuntime,
};
use crate::name_entry::name_entry_rng_next;

pub mod collision;
mod ground;

#[cfg(test)]
mod tests;

impl ProcessParticleRuntime {
    /// Native selector0 creates category1/00528814, including scratch writes
    /// and the sole RNG draw. Other selectors return NULL before allocation.
    pub fn birth_impact(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<std::convert::Infallible>> {
        if request.particle_type != 0 {
            return Ok(None);
        }
        let id = self.allocate(ParticleAllocationRequest {
            payload_bytes: 0x34,
            last_reclaim_category: 1,
            network_active: request.inputs.network_active,
            phase: request.inputs.phase,
        })?;
        let source_handle = request
            .inputs
            .source_handle(ParticleBirthProducer::Impact)?;
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
        record.write_dword(0x28, 0x0053_ca88)?;
        record.write_dword(0, 0x0052_6f40)?;
        let bytes = executable.mapped(0x0053_ca94, 4)?.bytes;
        let lifetime = u32::from_le_bytes(bytes.try_into().expect("DWORD")).wrapping_sub(1);
        record.write_word(0x10, lifetime as u16)?;
        record.write_dword(0, 0x0052_87d4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0x30, 0)?;
        record.write_dword(0, 0x0052_8814)?;
        record.write_dword(0x2c, (random >> 9) + 64)?;
        Ok(Some(id))
    }
}
