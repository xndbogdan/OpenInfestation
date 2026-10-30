//! 0044C130/0044B8C0's alternating row buffers,0044BC30 palette/context,
//! and 0044BE90's original cell projection (always 004EB8E0 far-local).

use super::{
    quad, TerrainDrawError, TerrainDrawFault, TerrainDrawHost, TerrainDrawInputs,
    TerrainDrawOutcome, TerrainGrid, TerrainRow,
};
use crate::{
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch},
    model_projection::ModelProjectionBackend,
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
};

#[derive(Clone, Copy, Debug)]
pub enum TerrainLighting<'a> {
    Disabled,
    Dynamic {
        /// Native low WORD anchors0054E2F0/00548400.
        anchors: [u16; 2],
        /// Signed RGB bytes in X*90+Z order at00548404.
        cells: &'a [u8],
        /// Original526068 bytes, used outside the90-square grid.
        outside: [u8; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainStripMode {
    Near,
    Far,
}

#[derive(Clone, Copy)]
pub struct TerrainStripRequest<'a> {
    pub terrain: &'a crate::terrain_runtime::TerrainRuntime,
    pub draw: TerrainDrawInputs<'a>,
    pub rows: &'a [TerrainRow],
    pub grid: TerrainGrid,
    pub mode: TerrainStripMode,
    pub fade: [i32; 2],
    pub sort_bias: i32,
    pub material: u32,
    pub far_color: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainVertex {
    pub rgb: [u8; 3],
    /// Native byte+3, copied into the shade DWORD's high byte.
    pub cell_flags: u8,
    pub projection: ModelVertexProjection,
    pub world: [u16; 3],
}

#[derive(Default)]
pub(super) struct RowBuffer {
    pub(super) vertices: [TerrainVertex; 15],
    pub(super) initialized: usize,
    pub(super) left: i16,
    pub(super) offset: i16,
    pub(super) tiles: i16,
    /// Native buffer+F0, rewritten even when a shorter row leaves stale pixels.
    pub(super) world_z: u16,
}

pub(super) fn draw_strip<'a, H: TerrainDrawHost<'a>>(
    request: TerrainStripRequest<'_>,
    host: &mut H,
) -> Result<TerrainDrawOutcome, TerrainDrawError<H::Error>> {
    walk_strip(request, host, fill_row, quad::submit_tile)
}

/// C130 and C6A0 share the row-buffer lifetime and metadata traversal.
/// Their complete vertex producer and tile consumer remain distinct.
pub(super) fn walk_strip<'a, H, F, S>(
    request: TerrainStripRequest<'_>,
    host: &mut H,
    mut fill: F,
    mut submit: S,
) -> Result<TerrainDrawOutcome, TerrainDrawError<H::Error>>
where
    H: TerrainDrawHost<'a>,
    F: FnMut(
        TerrainStripRequest<'_>,
        i16,
        i16,
        i16,
        &mut RowBuffer,
    ) -> Result<(), TerrainDrawFault>,
    S: FnMut(
        TerrainStripRequest<'_>,
        [TerrainVertex; 4],
        quad::TerrainTileCoordinates,
        &mut H,
    ) -> Result<bool, TerrainDrawError<H::Error>>,
{
    let Some(first) = request.rows.first() else {
        return Ok(TerrainDrawOutcome::default());
    };
    let mut buffers = [RowBuffer::default(), RowBuffer::default()];
    fill(request, first.z, first.left, first.right, &mut buffers[0])?;
    buffers[0].tiles = first.right.wrapping_sub(first.left);
    let mut old_index = 0;
    let mut outcome = TerrainDrawOutcome::default();
    for index in 0..request.rows.len().saturating_sub(1) {
        let [row, next] = [request.rows[index], request.rows[index + 1]];
        let new_index = old_index ^ 1;
        let new = &mut buffers[new_index];
        fill(
            request,
            row.z,
            row.left.min(next.left),
            row.right.max(next.right),
            new,
        )?;
        new.offset = row.left.wrapping_sub(new.left);
        new.tiles = next.right.wrapping_sub(next.left);
        emit_row_pair(
            request,
            &buffers[old_index],
            &buffers[new_index],
            host,
            &mut outcome,
            &mut submit,
        )?;
        buffers[new_index].offset = next.left.wrapping_sub(buffers[new_index].left);
        old_index = new_index;
    }
    let last = request.rows.last().unwrap();
    let new_index = old_index ^ 1;
    fill(
        request,
        last.z,
        last.left,
        last.right,
        &mut buffers[new_index],
    )?;
    // The native final buffer has offset0 from constructor/earlier loop.
    // Preserve its actual offset when reusing a buffer: C130 never resets it.
    emit_row_pair(
        request,
        &buffers[old_index],
        &buffers[new_index],
        host,
        &mut outcome,
        &mut submit,
    )?;
    Ok(outcome)
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
    // The two stack buffers are reused without clearing. A shorter row
    // overwrites only its prefix; C130's final stale offset can still read
    // vertices retained from an earlier, longer row in this SAME buffer.
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
    let mut projection = request.draw.context_projection;
    projection.fade = request.fade;
    let mut projector = ModelProjectionBackend {
        transform: ModelContext::new(request.draw.view.eye, MenuBasis::INITIAL)
            .publish(request.draw.view, request.draw.coordinate_shift),
        view: request.draw.view,
        projection,
        dispatch: ModelDispatch::Far,
        coordinate_shift: request.draw.coordinate_shift,
        fog_factor: if request.fade[1] > request.fade[0] {
            0x1000000 / (request.fade[1] - request.fade[0])
        } else {
            0
        },
    };
    let curvature = request.fade[1] as i16;
    for vertex in &mut buffer.vertices[..count] {
        let grid_mask = !(request.grid.mask >> 7);
        let x = usize::from((world_x >> 7) & grid_mask);
        let z = usize::from((world_z >> 7) & grid_mask);
        let cell = request.terrain.cells()[x * 512 + z];
        let height = i32::from(cell.height_i8()).wrapping_shl(
            match request.terrain.sampling().height_scale {
                crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
                crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
            },
        ) as u16;
        let shade = i32::from(cell.shade4());
        let mut rgb = match request.mode {
            TerrainStripMode::Near => request.draw.tint.map(|c| (i32::from(c) * shade / 15) as u8),
            TerrainStripMode::Far => [(shade * 256 / 15) as u8; 3],
        };
        if request.mode == TerrainStripMode::Near {
            if let TerrainLighting::Dynamic {
                anchors,
                cells,
                outside,
            } = request.draw.lighting
            {
                if cells.len() < 90 * 90 * 3 {
                    return Err(TerrainDrawFault::LightGrid(cells.len()));
                }
                let index = [world_x, world_z].map(|v| i32::from(v));
                let indices: [usize; 2] = std::array::from_fn(|axis| {
                    ((index[axis].wrapping_sub(i32::from(anchors[axis])) >> 7) as u32
                        & 511
                        & u32::from(grid_mask)) as usize
                });
                let light = if indices[0] < 90 && indices[1] < 90 {
                    let index = (indices[0] * 90 + indices[1]) * 3;
                    [cells[index], cells[index + 1], cells[index + 2]]
                } else {
                    outside
                };
                rgb = std::array::from_fn(|channel| {
                    (i32::from(rgb[channel]) + i32::from(light[channel] as i8) * 2).clamp(0, 255)
                        as u8
                });
            }
        }
        vertex.rgb = rgb;
        vertex.cell_flags = (cell.raw() >> 8) as u8;
        vertex.world = [world_x, height, world_z];
        let delta =
            std::array::from_fn(
                |axis| i32::from(vertex.world[axis].wrapping_sub(eye[axis]) as i16),
            );
        let depth = projector
            .project(ModelVertexInput::Local(delta), &mut vertex.projection)
            .unwrap() as i16;
        if depth < 64 {
            let xy = vertex.projection.read_u32(0).unwrap();
            vertex.projection.write_u32(
                0,
                (xy & 0xffff0000) | u32::from(world_x.wrapping_sub(eye[0])),
            );
        }
        if request.mode == TerrainStripMode::Far {
            let curvature_delta = depth.wrapping_sub(curvature);
            if curvature_delta > 0 {
                let xy = vertex.projection.read_u32(0).unwrap();
                let y = (xy >> 16) as u16;
                let curve = i32::from(curvature_delta) * i32::from(curvature_delta) / 6000;
                vertex.projection.write_u32(
                    0,
                    (xy & 0xffff) | (u32::from(y.wrapping_add(curve as u16)) << 16),
                );
            }
        }
        world_x = world_x.wrapping_add(request.grid.spacing);
    }
    Ok(())
}

fn emit_row_pair<'a, H: TerrainDrawHost<'a>, S>(
    request: TerrainStripRequest<'_>,
    old: &RowBuffer,
    new: &RowBuffer,
    host: &mut H,
    outcome: &mut TerrainDrawOutcome,
    submit: &mut S,
) -> Result<(), TerrainDrawError<H::Error>>
where
    S: FnMut(
        TerrainStripRequest<'_>,
        [TerrainVertex; 4],
        quad::TerrainTileCoordinates,
        &mut H,
    ) -> Result<bool, TerrainDrawError<H::Error>>,
{
    for tile in 0..old.tiles.max(0) {
        let read = |buffer: &RowBuffer, offset: i16| {
            let index = i32::from(offset);
            if index < 0 || index as usize >= buffer.initialized {
                Err(TerrainDrawFault::Vertex {
                    index,
                    count: buffer.initialized,
                })
            } else {
                Ok(buffer.vertices[index as usize])
            }
        };
        let a = old.offset.wrapping_add(tile);
        let b = new.offset.wrapping_add(tile);
        let corners = [
            read(old, a)?,
            read(new, b)?,
            read(new, b.wrapping_add(1))?,
            read(old, a.wrapping_add(1))?,
        ];
        let eye_x = request.draw.view.eye[0] as u16;
        let coordinates = quad::TerrainTileCoordinates {
            x: [old, new].map(|buffer| {
                (buffer.left.wrapping_add(buffer.offset).wrapping_add(tile) as u16)
                    .wrapping_mul(request.grid.spacing)
                    .wrapping_add(eye_x)
                    & !request.grid.mask
            }),
            z: [old.world_z, new.world_z],
        };
        if submit(request, corners, coordinates, host)? {
            outcome.quads += 1;
        } else {
            outcome.rejected += 1;
        }
    }
    Ok(())
}
