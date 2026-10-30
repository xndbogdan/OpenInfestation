//! 00466A50 invisible emitter factory. Unlike the three neighboring factories,
//! both source handle and velocity are live process operands, not caller values.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleEmitterFactoryRequest {
    /// Exact unsigned native ECX: only10 and13 are admitted.
    pub selector: u32,
    pub flags: u32,
}

impl ProcessParticleRuntime {
    pub fn create_emitter_particle<H: ParticleFactoryHost>(
        &mut self,
        request: ParticleEmitterFactoryRequest,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
        host: &mut H,
    ) -> Result<Option<ParticleId>, ParticleFactoryError<H::Error>> {
        if !matches!(request.selector, 10 | 13) {
            return Ok(None);
        }
        if request.flags & 1 == 0 {
            // 466A87 captures0054E894 before466010 rereads the same source.
            // No synchronous service intervenes between those two reads.
            let null = context
                .process
                .process_null_handle
                .ok_or(ParticleFactoryError::MissingEmitterProcessNull { address: 0x466a87 })?;
            if context.process.network_flag != 0 {
                sources.separate_velocity = Some(self.emitter_velocity_54ecb8);
                host.network_notice(
                    ParticleCreationNotice {
                        event: 0x17,
                        code: 0x20000 | request.selector,
                        source_handle: None,
                        velocity_source: ParticleVelocitySource::Separate,
                    },
                    self,
                    context,
                    sources,
                )
                .map_err(ParticleFactoryError::Network)?;
            }
            // Retain the native read even on the offline492500 return path.
            let _ = null;
        }
        let network_active = context.process.network_flag != 0;
        let phase = if network_active {
            None
        } else {
            host.current_phase().map_err(ParticleFactoryError::Phase)?
        };
        let id = self.allocate_reclaiming(
            ParticleAllocationRequest {
                payload_bytes: 0x40,
                last_reclaim_category: 3,
                network_active,
                phase,
            },
            |particles, particle| {
                particles.delete_factory_particle(
                    ParticleDeleteRequest { particle, flags: 1 },
                    context,
                    host,
                )
            },
        )?;
        // 466ABE is a fresh read after allocation/reclamation. Missing input
        // retains that allocated prefix, exactly like other reached limits.
        let source = context
            .process
            .process_null_handle
            .ok_or(ParticleFactoryError::MissingEmitterProcessNull { address: 0x466abe })?;
        self.construct(
            id,
            ParticleConstruction {
                category: 3,
                position: sources.position,
                velocity: Some(self.emitter_velocity_54ecb8),
                source_handle: source.raw(),
                flags: request.flags | 2,
            },
        )?;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, 0x539f68)?;
        record.write_dword(0, 0x526f40)?;
        let lifetime = dword(context.executable, 0x539f74)?;
        record.write_dword(0x34, 0x539ea0)?;
        record.write_dword(0x38, 0)?;
        record.write_word(0x10, lifetime.wrapping_sub(1) as u16)?;
        record.write_dword(0x2c, 0x400)?;
        record.write_dword(0, 0x526f80)?;
        record.write_dword(0x3c, request.selector)?;
        Ok(Some(id))
    }
}

#[cfg(test)]
mod tests;
