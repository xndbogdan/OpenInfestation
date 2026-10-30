//! The shared 004663E0 projectile streak. Ten ordinary animation submissions
//! use the same live payload; only the normal-return suffix restores XYZ.

use super::*;
use crate::object_runtime::native_trunc_low32;

impl ProcessParticleRuntime {
    pub(super) fn draw_projectile<'r, H: ParticleDrawHost<'r>>(
        &mut self,
        id: ParticleId,
        execution: ParticleDrawExecution<'_, '_>,
        host: &mut H,
        report: &mut ParticleDrawReport<H::Error>,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        self.projectile_streak(id, execution.executable, |particles, submission| {
            particles.draw_animation(id, execution, submission, host, report)
        })
    }

    /// The native wrapper owns position mutation around a reached shared draw.
    /// Keeping this phase separate also authenticates explicit stopped/returned
    /// lower-service segments without substituting the renderer implementation.
    pub(super) fn projectile_streak<E>(
        &mut self,
        id: ParticleId,
        executable: FrontendExecutable<'_>,
        mut draw: impl FnMut(&mut Self, ParticleAnimationSubmission) -> Result<(), E>,
    ) -> Result<(), E>
    where
        E: From<ParticleError> + From<FrontendExecutableError>,
    {
        let mut velocity = [0_f64; 3];
        for (axis, value) in velocity.iter_mut().enumerate() {
            *value = f64::from(self.record(id)?.read_word(0x0a + axis * 2)? as i16);
        }
        let [x, y, z] = velocity;
        let length = ((z * z + y * y) + x * x).sqrt();
        let divisor = f64::from(length as f32);
        let normal = if length == 0.0 {
            let bytes = executable.mapped(0x0052_0a70, 4)?.bytes;
            [
                f64::from(f32::from_bits(u32::from_le_bytes(
                    bytes.try_into().unwrap(),
                ))),
                0.0,
                0.0,
            ]
        } else {
            // X stays on the x87 stack; Y and Z alone spill to float32.
            [
                x / divisor,
                f64::from((y / divisor) as f32),
                f64::from((z / divisor) as f32),
            ]
        };
        let bytes = executable.mapped(0x0052_6f1c, 4)?.bytes;
        let scale = f64::from(f32::from_bits(u32::from_le_bytes(
            bytes.try_into().unwrap(),
        )));
        let step = normal.map(|axis| native_trunc_low32(axis * scale) as u16);
        let original = self.record(id)?.position()?;
        for index in 0..10 {
            let (table, scale) = if index != 0 {
                // 4664E1/E8/F3 writes Y,Z,X in that order, rereading actual
                // live position after the preceding draw callback returns.
                let y = self.record(id)?.read_word(6)?;
                self.record_mut(id)?
                    .write_word(6, y.wrapping_add(step[1]))?;
                let scale = self.record(id)?.read_dword(0x2c)? as i32;
                let z = self.record(id)?.read_word(8)?;
                self.record_mut(id)?
                    .write_word(8, z.wrapping_add(step[2]))?;
                let table = self.record(id)?.read_dword(0x28)?;
                let x = self.record(id)?.read_word(4)?;
                self.record_mut(id)?
                    .write_word(4, x.wrapping_add(step[0]))?;
                (table, scale)
            } else {
                (
                    self.record(id)?.read_dword(0x28)?,
                    self.record(id)?.read_dword(0x2c)? as i32,
                )
            };
            draw(
                self,
                ParticleAnimationSubmission {
                    table,
                    scale,
                    shadow_amount: 0,
                },
            )?;
        }
        self.record_mut(id)?.write_position(original)?;
        Ok(())
    }
}
