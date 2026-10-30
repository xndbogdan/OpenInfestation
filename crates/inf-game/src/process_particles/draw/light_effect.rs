//! 0048AE60: authored light-only animations. The fading arm calls 0045B1A0
//! directly; only the sine arm passes through 00465720's flag/floor policy.
use super::*;

impl ProcessParticleRuntime {
    pub(super) fn draw_terrain_flash<H: ParticleDrawLighting>(
        &mut self,
        id: ParticleId,
        execution: ParticleDrawExecution<'_, '_>,
        host: &mut H,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        let record = self.record(id)?;
        let table = record.read_dword(0x28)?;
        let lifetime = record.read_word(0x10)?;
        let executable = execution.executable;
        // Even an unrecognized table reaches this frame read before returning.
        let frame = animation::ParticleAnimationFrame::read(
            executable,
            animation::frame_address(executable, table, lifetime)?,
        )?;
        if !matches!(table, 0x0053_d610 | 0x0053_d620) {
            return Ok(());
        }
        let total = i32::from(u16::from_le_bytes(
            executable
                .mapped(table.wrapping_add(12), 2)?
                .bytes
                .try_into()
                .expect("WORD"),
        ));
        let divide = |value: i32| {
            value
                .checked_div(total)
                .ok_or(ParticleDrawError::LightDivision { table, lifetime })
        };
        let life = i32::from(lifetime);
        if table == 0x0053_d620 {
            let mut rgb = [0; 3];
            for (out, channel) in rgb.iter_mut().zip(frame.light_rgb) {
                *out = divide(i32::from(channel).wrapping_mul(life))? as i8;
            }
            let radius =
                divide(total.wrapping_sub(life).wrapping_mul(3).wrapping_shl(8))?.wrapping_add(256);
            let position = self
                .record(id)?
                .position()?
                .map(|word| i32::from(word as i16));
            return host
                .terrain_light(
                    TerrainLightRequest {
                        world_xz: [position[0], position[2]],
                        radius,
                        rgb,
                    },
                    position,
                )
                .map_err(ParticleDrawError::Host);
        }

        let angle = divide(life.wrapping_shl(14))? as u32;
        let mut quarter = angle & 0x3ffc;
        if angle & 0x4000 != 0 {
            quarter = 0x3ffc - quarter;
        }
        let word = u32::from(u16::from_le_bytes(
            executable
                .mapped(0x0053_df00 + (quarter >> 2) * 2, 2)?
                .bytes
                .try_into()
                .expect("WORD"),
        ));
        // Native duplicates the table WORD into both halves; this is not the
        // ordinary signed-Q15 sine lookup used by other consumers.
        let mut sine = (word | (word << 16)) as i32;
        if angle & 0x8000 != 0 {
            sine = sine.wrapping_neg();
        }
        let factor = (sine >> 15).wrapping_add(0x10000);
        let amount = divide(life.wrapping_shl(9))?.wrapping_mul(factor) >> 15;
        self.draw_light(
            id,
            ParticleLightParameters {
                amount: amount as i16,
                rgb: frame.light_rgb,
            },
            execution.request.terrain,
            host,
        )
    }
}
