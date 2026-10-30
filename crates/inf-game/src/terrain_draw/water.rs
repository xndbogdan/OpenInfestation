//! Complete 0044C6A0 water strip: 0044C9D0/0044CCC0 row production, the
//! 0044EAA0 projection and 0044DBB0 tiles, with 0044DFE0's clip heights.
//! Ground and water share buffer custody, not projection or tile policy.

use super::{
    plan::ftol_low32,
    quad::{texture_sort, TerrainTileCoordinates},
    strip::{walk_strip, RowBuffer},
    TerrainClipContext, TerrainClipRequest, TerrainDrawError, TerrainDrawFault, TerrainDrawHost,
    TerrainDrawOutcome, TerrainQuadCallback, TerrainQuadCommand, TerrainStripRequest,
    TerrainVertex,
};
use crate::{
    menu::MenuBasis, model_context::ModelContext, model_vertices::ModelVertexProjection,
    terrain_runtime::TerrainWaterSurfaceRequest,
};

/// Native00537010 has sixteen8-byte entries. The intervening WORD is unused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainWaterTile {
    pub sprite_slot: i16,
    pub orientation: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerrainWaterTables {
    pub shoreline: [TerrainWaterTile; 16],
}

/// BC30 runs again before any H12 row read. Water is always the native near
/// palette, independent of a preceding strip's far flag. The B0C spacing bias
/// remains in its context for both direct quads and E150 clipping.
pub fn draw_water_strip<'a, H: TerrainDrawHost<'a>>(
    mut request: TerrainStripRequest<'_>,
    host: &mut H,
) -> Result<TerrainDrawOutcome, TerrainDrawError<H::Error>> {
    request.grid.validate()?;
    let material = host.material(0x74).map_err(TerrainDrawError::Host)?;
    request.material = request.draw.night.unwrap_or(material);
    request.sort_bias = request
        .sort_bias
        .wrapping_add(i32::from(request.grid.spacing) * 0xb0c / 4000);
    walk_strip(request, host, fill_row, submit_tile)
}

fn water_height(request: TerrainStripRequest<'_>, xz: [u16; 2]) -> i16 {
    request.terrain.water_surface_height(
        TerrainWaterSurfaceRequest {
            xz,
            wave_clock: request.draw.clock,
            world_flags: request.draw.world_flags,
            terrain_height: request.terrain.raw_height_at(xz),
        },
        &request.draw.tables.sine_table,
    )
}

fn fill_row(
    request: TerrainStripRequest<'_>,
    z: i16,
    left: i16,
    right: i16,
    buffer: &mut RowBuffer,
) -> Result<(), TerrainDrawFault> {
    let count = (i32::from(right) - i32::from(left) + 1).max(0) as usize;
    if count > 15 {
        return Err(TerrainDrawFault::RowCapacity { left, right });
    }
    buffer.left = left;
    buffer.initialized = buffer.initialized.max(count);
    let eye = request.draw.view.eye.map(|v| v as u16);
    let world_z = (z.wrapping_sub(1) as u16)
        .wrapping_mul(request.grid.spacing)
        .wrapping_add(eye[2])
        & !request.grid.mask;
    buffer.world_z = world_z;
    let mut world_x = (left as u16)
        .wrapping_mul(request.grid.spacing)
        .wrapping_add(eye[0])
        & !request.grid.mask;
    // C9D0 samples the preceding cell even when this row contains no vertices.
    let mut previous = water_height(
        request,
        [world_x.wrapping_sub(request.grid.spacing), world_z],
    );
    let plane_word = (request.terrain.plane_base() >> request.terrain.plane_shift()) as i16;
    for vertex in &mut buffer.vertices[..count] {
        let xz = [world_x, world_z];
        let height = water_height(request, xz);
        let shade = (((i32::from(height) - i32::from(previous)) >> 5) + 6).clamp(0, 15);
        vertex.rgb = request.draw.tint.map(|c| (i32::from(c) * shade / 15) as u8);
        vertex.cell_flags = u8::from(request.terrain.raw_height_at(xz) < plane_word);
        vertex.world = [world_x, height as u16, world_z];
        let delta =
            std::array::from_fn(
                |axis| i32::from(vertex.world[axis].wrapping_sub(eye[axis]) as i16),
            );
        let depth = project(request, delta, &mut vertex.projection);
        if (depth as i16) < 64 {
            let xy = vertex.projection.read_u32(0).unwrap();
            vertex.projection.write_u32(
                0,
                (xy & 0xffff0000) | u32::from(world_x.wrapping_sub(eye[0])),
            );
        }
        previous = height;
        world_x = world_x.wrapping_add(request.grid.spacing);
    }
    Ok(())
}

/// 0044EAA0 uses CRT truncation; model/ground FISTP nearest-even differs.
/// Its X/Y dot order is (Z+Y)+X, while depth is (Y+Z)+X. E150 retains its
/// separate clipping projector and must not be changed to this order.
fn project(
    request: TerrainStripRequest<'_>,
    point: [i32; 3],
    output: &mut ModelVertexProjection,
) -> i32 {
    let transform = ModelContext::new(request.draw.view.eye, MenuBasis::INITIAL)
        .publish(request.draw.view, request.draw.coordinate_shift);
    let [x, y, z] = point.map(f64::from);
    let component = |axis: usize| {
        let [bx, by, bz] = std::array::from_fn(|source| {
            f64::from(f32::from_bits(
                transform.view_basis.words[1 + source * 3 + axis],
            ))
        });
        ftol_low32(if axis == 2 {
            (y * by + z * bz) + x * bx
        } else {
            (z * bz + y * by) + x * bx
        })
        .wrapping_add(transform.view_position[axis])
    };
    let [vx, vy, depth] = [component(0), component(1), component(2)];
    if depth < 64 {
        output.write_u32(0, u32::from(vx as u16) | (u32::from(vy as u16) << 16));
        output.write_u32(4, depth as u32);
        output.bytes[8] = Some(0xc0);
        return depth; // Fog is genuinely unwritten/retained on this branch.
    }
    let projection = request.draw.context_projection;
    let mut sx = ftol_low32(f64::from(projection.focal[0]) * f64::from(vx) / f64::from(depth))
        .wrapping_add(projection.center[0]);
    let mut sy = projection.center[1].wrapping_sub(ftol_low32(
        f64::from(projection.focal[1]) * f64::from(vy) / f64::from(depth),
    ));
    let mut magnitude = sx.wrapping_abs() | sy.wrapping_abs();
    while magnitude >= 32767 {
        magnitude >>= 1;
        sx >>= 1;
        sy >>= 1;
    }
    let factor = if request.fade[1] > request.fade[0] {
        0x1000000 / (request.fade[1] - request.fade[0])
    } else {
        0
    };
    output.bytes[9] = Some(if depth < request.fade[0] {
        0
    } else if depth >= request.fade[1] {
        255
    } else {
        (depth.wrapping_sub(request.fade[0]).wrapping_mul(factor) >> 16) as u8
    });
    let axis_clip = |v: i32, limit: i32, codes: [u8; 3]| {
        if (v as u32) < (limit as u32) {
            codes[1]
        } else if v < 0 {
            codes[0]
        } else {
            codes[2]
        }
    };
    output.write_u32(0, u32::from(sx as u16) | (u32::from(sy as u16) << 16));
    output.write_u32(4, depth as u32);
    output.bytes[8] = Some(
        axis_clip(sx, projection.viewport[0], [1, 2, 4])
            | axis_clip(sy, projection.viewport[1], [8, 16, 32]),
    );
    depth
}

fn submit_tile<'a, H: TerrainDrawHost<'a>>(
    request: TerrainStripRequest<'_>,
    corners: [TerrainVertex; 4],
    coordinates: TerrainTileCoordinates,
    host: &mut H,
) -> Result<bool, TerrainDrawError<H::Error>> {
    let combo = corners
        .iter()
        .enumerate()
        .fold(0usize, |value, (i, corner)| {
            value | (usize::from(corner.cell_flags) << i)
        });
    if combo == 0 {
        return Ok(false);
    }
    let tile = request.draw.tables.water.shoreline[combo];
    let unknown = |field| TerrainDrawFault::Unwritten { field };
    let combined = corners.iter().try_fold(0, |bits, corner| {
        corner.projection.bytes[8]
            .map(|clip| bits | clip)
            .ok_or_else(|| unknown("clip byte"))
    })?;
    if request.draw.tables.clip_acceptance[usize::from(combined)] == 0 {
        let texture = host
            .texture(tile.sprite_slot)
            .map_err(TerrainDrawError::Host)?;
        let eye = request.draw.view.eye.map(|v| v as u16);
        let xz = [
            [coordinates.x[0], coordinates.z[0]],
            [coordinates.x[1], coordinates.z[1]],
            [
                coordinates.x[0].wrapping_add(request.grid.spacing),
                coordinates.z[1],
            ],
            [
                coordinates.x[1].wrapping_add(request.grid.spacing),
                coordinates.z[0],
            ],
        ];
        // 0044DFE0 recomputes each height from the row metadata, with the
        // asymmetric C/D X inputs, rather than reusing the vertices'.
        let eye_relative = xz.map(|[x, z]| {
            [
                x.wrapping_sub(eye[0]) as i16,
                (water_height(request, [x, z]) as u16).wrapping_sub(eye[1]) as i16,
                z.wrapping_sub(eye[2]) as i16,
            ]
        });
        let mut projection = request.draw.context_projection;
        projection.fade = request.fade;
        return host
            .clipped_tile(TerrainClipRequest {
                corners,
                eye_relative,
                context: TerrainClipContext {
                    transform: ModelContext::new(request.draw.view.eye, MenuBasis::INITIAL)
                        .publish(request.draw.view, request.draw.coordinate_shift),
                    projection,
                    fog_factor: if request.fade[1] > request.fade[0] {
                        0x1000000 / (request.fade[1] - request.fade[0])
                    } else {
                        0
                    },
                },
                texture,
                orientation: tile.orientation,
                sort_bias: request.sort_bias as i16,
                material: request.material,
            })
            .map(|count| count != 0)
            .map_err(TerrainDrawError::Host);
    }
    let fog: [Option<u8>; 4] = std::array::from_fn(|i| corners[i].projection.bytes[9]);
    let fog: [u8; 4] = super::all_written(fog).ok_or_else(|| unknown("fog byte"))?;
    if fog.iter().all(|value| *value == 255) {
        return Ok(false);
    }
    let fogged = fog.iter().any(|value| *value != 0);
    let texture = host
        .texture(tile.sprite_slot)
        .map_err(TerrainDrawError::Host)?;
    let mut depth = corners[0]
        .projection
        .read_u32(4)
        .ok_or_else(|| unknown("depth"))?
        .wrapping_add(i32::from(request.sort_bias as i16) as u32);
    if host.material_sorting().map_err(TerrainDrawError::Host)? {
        if let Some(texture) = texture {
            depth = texture_sort(depth, texture, |texture| {
                host.texture_sort_identity(texture)
            })?;
        }
    }
    let mut output = TerrainQuadCommand {
        callback: if fogged {
            TerrainQuadCallback::TexturedFar
        } else {
            TerrainQuadCallback::TexturedNear
        },
        depth,
        payload: [None; 64],
        texture,
    };
    output.write(0x20, 0);
    if texture.is_none() {
        output.write(0x24, 0);
    }
    let indices: [usize; 4] =
        std::array::from_fn(|i| ((tile.orientation >> (i * 8)) & 15) as usize);
    if indices.iter().any(|i| *i > 3) || (0..4).any(|i| indices[..i].contains(&indices[i])) {
        return Err(TerrainDrawFault::Orientation(tile.orientation).into());
    }
    for (source, target) in indices.into_iter().enumerate() {
        let corner = corners[source];
        output.write(
            target * 8,
            corner.projection.read_u32(0).ok_or_else(|| unknown("XY"))?,
        );
        output.write(
            target * 8 + 4,
            corner
                .projection
                .read_u32(4)
                .ok_or_else(|| unknown("depth"))?,
        );
        output.write(
            0x28 + target * 4,
            u32::from_le_bytes([
                corner.rgb[0],
                corner.rgb[1],
                corner.rgb[2],
                corner.cell_flags,
            ]),
        );
        if fogged {
            output.payload[0x3c + target] = Some(fog[source]);
        }
    }
    if fogged {
        output.write(0x38, request.material);
    }
    host.submit_quad(output).map_err(TerrainDrawError::Host)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
