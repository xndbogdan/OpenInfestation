//! 00465C96 suffix: cached terrain floor, H7/32 and a projected thin quad.
use super::*;
use crate::object_runtime::native_trunc_low32;

pub(super) struct ShadowSubmission {
    pub table: u32,
    pub scale: i32,
    pub shadow_amount: i32,
    pub half_width: i32,
    pub depth: i32,
    pub delta: [i32; 3],
    pub projected: ModelVertexProjection,
}

impl ProcessParticleRuntime {
    pub(super) fn draw_shadow<'r, H: ParticleDrawHost<'r>>(
        &mut self,
        id: ParticleId,
        execution: ParticleDrawExecution<'_, '_>,
        submission: ShadowSubmission,
        host: &mut H,
        report: &mut ParticleDrawReport<H::Error>,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        let ParticleDrawExecution {
            request,
            executable,
            mut projector,
        } = execution;
        let ShadowSubmission {
            table,
            scale,
            shadow_amount,
            half_width,
            depth,
            mut delta,
            projected,
        } = submission;
        let floor = if self.record(id)?.read_byte(0x12)? & 0x20 != 0 {
            self.record(id)?.read_word(0x24)?
        } else {
            self.cached_terrain_floor(
                id,
                request.terrain.ok_or(ParticleDrawError::MissingTerrain)?,
            )? as u16
        };
        delta[1] = i32::from(floor.wrapping_sub(request.context.position[1] as u16) as i16);
        let mut width = half_width
            .wrapping_mul(shadow_amount)
            .wrapping_shl(1)
            .checked_div(scale)
            .ok_or(ParticleDrawError::ShadowDivision(scale))?;
        if width == 0 {
            width = 1;
        }
        // Native preloads H7/32 before the renderer's second sorting query,
        // then rereads the converted DWORD for the actual packet.
        host.shadow_material(32)
            .map_err(ParticleDrawError::Host)?
            .ok_or(ParticleDrawError::MissingShadowMaterial(32))?;
        let bias = i32::from(i16::from_le_bytes(
            executable
                .mapped(table.wrapping_add(8), 2)?
                .bytes
                .try_into()
                .unwrap(),
        ));
        let mut sort = depth.wrapping_add(bias).wrapping_add(1) as u32;
        if host.material_sorting().map_err(ParticleDrawError::Host)? {
            sort = (sort >> 1).wrapping_add(0xc000_0000);
        }
        let far = depth >= request.context.projection.fade[0];
        let mut command = PrimitiveCommand::new(
            if far {
                PrimitiveCallback::FlatQuadFar
            } else {
                PrimitiveCallback::FlatQuadNear
            },
            sort,
            3,
        );
        command.write_u32(0x20, 2);
        let material = host
            .shadow_material(32)
            .map_err(ParticleDrawError::Host)?
            .ok_or(ParticleDrawError::MissingShadowMaterial(32))?;
        command.write_u32(0x24, material);
        if far {
            command.write_u32(0x28, request.context.far_color);
            command.write_u32(
                0x2c,
                u32::from(projected.bytes[9].ok_or(ParticleDrawError::UnwrittenFog)?) * 0x0101_0101,
            );
        }
        let offset: [i32; 3] = std::array::from_fn(|axis| {
            let component = f32::from_bits(request.context.basis.words[1 + axis]);
            native_trunc_low32(f64::from(width) * f64::from(component)).wrapping_mul(depth) >> 9
        });
        let mut endpoints = [
            ModelVertexProjection::default(),
            ModelVertexProjection::default(),
        ];
        for (index, sign) in [-1_i32, 1].into_iter().enumerate() {
            let point = std::array::from_fn(|axis| {
                i32::from(delta[axis].wrapping_add(offset[axis].wrapping_mul(sign)) as i16)
            });
            match projector.project(ModelVertexInput::Local(point), &mut endpoints[index]) {
                Ok(_) => {}
                Err(error) => match error {},
            }
        }
        let xy: [u32; 2] = endpoints
            .each_ref()
            .map(|p| p.read_u32(0).expect("projection writesXY"));
        let points = xy.map(|v| [v as i16, (v >> 16) as i16]);
        let shift = [
            if points[1][1] > points[0][1] {
                1_i16
            } else {
                -1
            },
            if points[1][0] > points[0][0] {
                1_i16
            } else {
                -1
            },
        ];
        for (corner, endpoint) in [0, 1, 1, 0].into_iter().enumerate() {
            let mut point = points[endpoint];
            if corner >= 2 {
                for axis in 0..2 {
                    point[axis] = point[axis].wrapping_add(shift[axis]);
                }
            }
            command.write_u32(
                corner * 8,
                u32::from(point[0] as u16) | (u32::from(point[1] as u16) << 16),
            );
            command.write_u32(
                corner * 8 + 4,
                endpoints[endpoint]
                    .read_u32(4)
                    .expect("projection writesdepth"),
            );
        }
        host.submit(command).map_err(ParticleDrawError::Host)?;
        report.commands += 1;
        Ok(())
    }
}
