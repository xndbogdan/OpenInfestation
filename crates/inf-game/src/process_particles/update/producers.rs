//! 00489D90 trail producers and 004893A0 splash, reached by debris callbacks.

use std::convert::Infallible;

use inf_formats::FrontendExecutable;

use crate::name_entry::name_entry_rng_next;
use crate::process_particles::{
    births::{ParticleBirthError, ParticleBirthProducer, ParticleBirthRequest},
    ParticleAllocationRequest, ParticleConstruction, ParticleId, ProcessParticleRuntime,
};

/// The shared output shape retains each actual constructor's source-read site.
struct AnimationBirthPlan {
    table: u32,
    scale: u32,
    vtable: u32,
    producer: ParticleBirthProducer,
}

impl ProcessParticleRuntime {
    /// All fourteen 00489D90 selectors retain their native table, RNG and
    /// vtable policy. Invalid unsigned selectors return null before RNG.
    pub fn birth_trail(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let (table, scale, vtable) = match request.particle_type {
            0 => (0x0053_d1d0, 0x400, 0x0052_88f4),
            1 => (0x0053_d1d0, 0x200, 0x0052_88f4),
            2 | 3 => {
                let index = name_entry_rng_next(rng) & 3;
                (
                    0x0053_d220 + index * 16,
                    if request.particle_type == 2 {
                        0x100
                    } else {
                        0x400
                    },
                    0x0052_88f4,
                )
            }
            4 => (0x0053_d260, 0x80, 0x0052_88f4),
            5 => (0x0053_d1e0, 0x200, 0x0052_88f4),
            6 => (0x0053_d1f0, 0x200, 0x0052_88f4),
            7 => (0x0053_d1f0, 0x400, 0x0052_88f4),
            8 => (0x0053_d200, 0x200, 0x0052_88f4),
            9 => (0x0053_d5f0, 0x400, 0x0052_88f4),
            10 => (0x0053_d5f0, 0x800, 0x0052_88f4),
            11 => (0x0053_d210, 0x200, 0x0052_88f4),
            12 => (0x0053_d290, 0x100, 0x0052_88d4),
            13 => (0x0053_d5f0, 0x200, 0x0052_88b4),
            _ => return Ok(None),
        };
        let producer = match request.particle_type {
            12 => ParticleBirthProducer::Trail12,
            13 => ParticleBirthProducer::Trail13,
            _ => ParticleBirthProducer::Trail,
        };
        self.birth_animation(
            request,
            AnimationBirthPlan {
                table,
                scale,
                vtable,
                producer,
            },
            executable,
            rng,
        )
        .map(Some)
    }

    /// 004893A0's null velocity intentionally preserves reached heap bytes.
    /// Splash +4=004027B0 does not consume those bytes.
    pub fn birth_splash(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<Infallible>> {
        let table = match request.particle_type {
            0 => 0x0053_c9d8,
            1 => 0x0053_ca68,
            _ => return Ok(None),
        };
        self.birth_animation(
            ParticleBirthRequest {
                velocity: None,
                ..request
            },
            AnimationBirthPlan {
                table,
                scale: 0x100,
                vtable: 0x0052_87b4,
                producer: ParticleBirthProducer::Splash,
            },
            executable,
            rng,
        )
        .map(Some)
    }

    fn birth_animation(
        &mut self,
        request: ParticleBirthRequest<'_>,
        plan: AnimationBirthPlan,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<ParticleId, ParticleBirthError<Infallible>> {
        let AnimationBirthPlan {
            table,
            scale,
            vtable,
            producer,
        } = plan;
        let id = self.allocate(ParticleAllocationRequest {
            payload_bytes: 0x34,
            last_reclaim_category: 1,
            network_active: request.inputs.network_active,
            phase: request.inputs.phase,
        })?;
        let source_handle = request.inputs.source_handle(producer)?;
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
        // 0048B090 (type13) publishes this vtable before +28; the other
        // constructors publish +28 before this vtable. No host call intervenes.
        record.write_dword(0x28, table)?;
        record.write_dword(0, 0x0052_6f40)?;
        let bytes = executable.mapped(table + 0x0c, 2)?.bytes;
        let lifetime = u16::from_le_bytes(bytes.try_into().expect("WORD"));
        record.write_word(0x10, lifetime.wrapping_sub(1))?;
        record.write_dword(0, 0x0052_87d4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0x30, 0)?;
        record.write_dword(0, vtable)?;
        record.write_dword(0x2c, scale.wrapping_add(random >> 9).wrapping_sub(64))?;
        Ok(id)
    }
}
