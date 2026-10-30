//! 00466D20/00466D50 and the reached 00466280 impact branch.

use super::{
    dword, ParticleDeleteRequest, ParticleFactoryContext, ParticleFactoryError,
    ParticleFactoryHost, ParticleFactoryImpactRequest,
};
use crate::{object_runtime::native_trunc_low32, process_particles::ProcessParticleRuntime};

fn scalar(
    context: &ParticleFactoryContext<'_, '_>,
    address: u32,
) -> Result<f64, inf_formats::FrontendExecutableError> {
    Ok(f64::from(f32::from_bits(dword(
        context.executable,
        address,
    )?)))
}

impl ProcessParticleRuntime {
    /// Captured wrapper flags survive callbacks. Normal classes share the
    /// existing base unlink/release policy; EFC owns its additional branch.
    pub fn delete_factory_particle<H: ParticleFactoryHost>(
        &mut self,
        request: ParticleDeleteRequest,
        context: &mut ParticleFactoryContext<'_, '_>,
        host: &mut H,
    ) -> Result<(), ParticleFactoryError<H::Error>> {
        let id = request.particle;
        if self.prepare_destroy(id)? {
            // Native WORD reads are Y, X, Z before their FP conversion.
            let y = f64::from(self.record(id)?.read_word(0x0c)? as i16);
            let x = f64::from(self.record(id)?.read_word(0x0a)? as i16);
            let z = f64::from(self.record(id)?.read_word(0x0e)? as i16);
            // CW027F retains 53-bit arithmetic. sqrt is compared before its
            // f32 denominator spill is consumed by the three divisions.
            let length = ((z * z + y * y) + x * x).sqrt();
            let denominator = f64::from(length as f32);
            let (nx, ny, nz) = if length == 0.0 {
                (scalar(context, 0x520a70)?, 0.0, 0.0)
            } else {
                // X stays retained; Y/Z alone are spilled to f32 locals.
                (
                    x / denominator,
                    f64::from((y / denominator) as f32),
                    f64::from((z / denominator) as f32),
                )
            };
            let steps = [
                native_trunc_low32(nx * scalar(context, 0x526ef8)?) as u16,
                native_trunc_low32(ny * scalar(context, 0x526ef8)?) as u16,
                native_trunc_low32(nz * scalar(context, 0x526ef8)?) as u16,
            ];
            for _ in 0..5 {
                host.impact_489600(
                    ParticleFactoryImpactRequest {
                        particle: id,
                        selector: 0,
                        flags: 0,
                    },
                    self,
                    context,
                )
                .map_err(ParticleFactoryError::Impact)?;
                // Callback changes are observed by each reached native ADD.
                for (axis, step) in steps.into_iter().enumerate() {
                    let offset = 4 + axis * 2;
                    let value = self.record(id)?.read_word(offset)?;
                    self.record_mut(id)?
                        .write_word(offset, value.wrapping_add(step))?;
                }
            }
        }
        self.finish_destroy(id, request.flags)?;
        Ok(())
    }
}
