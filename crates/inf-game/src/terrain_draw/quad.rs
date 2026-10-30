//! 0044D6A0 textured terrain and 0044D030 far colored terrain packets.

use super::{
    TerrainClipContext, TerrainClipRequest, TerrainDrawError, TerrainDrawFault, TerrainDrawHost,
    TerrainStripMode, TerrainStripRequest, TerrainVertex,
};
use crate::{
    menu::MenuBasis,
    model_context::ModelContext,
    model_primitives::TextureSortIdentity,
    render_resources::{RenderResource, RenderResourceData},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainQuadCallback {
    TexturedNear,
    TexturedFar,
    ColoredNear,
    ColoredFar,
}

impl TerrainQuadCallback {
    pub const fn native_address(self) -> u32 {
        match self {
            Self::TexturedNear => 0x0044ea60,
            Self::TexturedFar => 0x0044ea70,
            Self::ColoredNear => 0x0044ea80,
            Self::ColoredFar => 0x0044ea90,
        }
    }
    pub const fn renderer_virtual_offset(self) -> u32 {
        match self {
            Self::TexturedNear => 0xac,
            Self::TexturedFar => 0xb0,
            Self::ColoredNear => 0x94,
            Self::ColoredFar => 0x98,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerrainQuadCommand<'a> {
    pub callback: TerrainQuadCallback,
    pub depth: u32,
    /// Native4A42B0 reserves four16-byte payload blocks, descriptor words0.
    pub payload: [Option<u8>; 64],
    /// Near-strip packets contain this relocation at+24. Far-strip packets
    /// contain the material DWORD there; their H3 lookup only affects sorting.
    pub texture: Option<RenderResource<'a>>,
}

impl TerrainQuadCommand<'_> {
    pub(super) fn write(&mut self, offset: usize, value: u32) {
        for (target, byte) in self.payload[offset..offset + 4]
            .iter_mut()
            .zip(value.to_le_bytes())
        {
            *target = Some(byte);
        }
    }
    pub fn read_u32(&self, offset: usize) -> Option<u32> {
        let b = self.payload.get(offset..offset + 4)?;
        Some(u32::from_le_bytes([b[0]?, b[1]?, b[2]?, b[3]?]))
    }
}

/// Current row metadata, independent of any stale vertex reached by an offset.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TerrainTileCoordinates {
    pub x: [u16; 2],
    pub z: [u16; 2],
}

pub(super) fn submit_tile<'a, H: TerrainDrawHost<'a>>(
    request: TerrainStripRequest<'_>,
    corners: [TerrainVertex; 4],
    coordinates: TerrainTileCoordinates,
    host: &mut H,
) -> Result<bool, TerrainDrawError<H::Error>> {
    let unknown = |field| TerrainDrawFault::Unwritten { field };
    let combined = corners.iter().try_fold(0, |bits, corner| {
        corner.projection.bytes[8]
            .map(|clip| bits | clip)
            .ok_or_else(|| unknown("clip byte"))
    })?;
    let direct = request.draw.tables.clip_acceptance[usize::from(combined)] != 0;
    if request.mode == TerrainStripMode::Far && !direct {
        return Ok(false);
    }
    if direct && culled(corners)? {
        return Ok(false);
    }
    let combo = corners
        .iter()
        .enumerate()
        .fold(0, |value, (index, corner)| {
            value | ((corner.cell_flags & 3) << (index * 2))
        });
    let blend = inf_formats::terrain_blend(combo);
    let texture = host
        .texture(blend.texture_resource_id as i16)
        .map_err(TerrainDrawError::Host)?;
    let adjust = i32::from(request.grid.spacing) * 0x1618 / 4000;
    let bias = request.sort_bias.wrapping_add(adjust) as i16;
    let bias = match request.mode {
        TerrainStripMode::Near => bias,
        TerrainStripMode::Far => bias.wrapping_mul(3),
    };
    if !direct {
        // E150 is reached even when the tile has no near-plane crossing; its
        // own gate then rejects it. Do not bypass texture acquisition.
        let eye = request.draw.view.eye.map(|v| v as u16);
        // E150 has deliberately asymmetric C/D X inputs: C uses old X plus
        // spacing, D uses new X plus spacing. Its original height callback
        // 0044DB70 resamples raw H12 at these CURRENT row coordinates.
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
        let eye_relative = xz.map(|[x, z]| {
            [
                x.wrapping_sub(eye[0]) as i16,
                (request.terrain.raw_height_at([x, z]) as u16).wrapping_sub(eye[1]) as i16,
                z.wrapping_sub(eye[2]) as i16,
            ]
        });
        let mut projection = request.draw.context_projection;
        projection.fade = request.fade;
        let count = host
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
                orientation: blend.orientation_word,
                sort_bias: request.sort_bias as i16,
                material: request.material,
            })
            .map_err(TerrainDrawError::Host)?;
        return Ok(count != 0);
    }
    let fog: [u8; 4] = super::all_written(corners.map(|v| v.projection.bytes[9]))
        .ok_or_else(|| unknown("fog byte"))?;
    let fogged = fog.iter().any(|v| *v != 0);
    let mut depth = corners[0]
        .projection
        .read_u32(4)
        .ok_or_else(|| unknown("depth"))?
        .wrapping_add(i32::from(bias) as u32);
    if host.material_sorting().map_err(TerrainDrawError::Host)? {
        if let Some(texture) = texture {
            depth = texture_sort(depth, texture, |texture| {
                host.texture_sort_identity(texture)
            })?;
        }
    }
    let callback = match (request.mode, fogged) {
        (TerrainStripMode::Near, false) => TerrainQuadCallback::TexturedNear,
        (TerrainStripMode::Near, true) => TerrainQuadCallback::TexturedFar,
        (TerrainStripMode::Far, false) => TerrainQuadCallback::ColoredNear,
        (TerrainStripMode::Far, true) => TerrainQuadCallback::ColoredFar,
    };
    let mut output = TerrainQuadCommand {
        callback,
        depth,
        payload: [None; 64],
        texture: if request.mode == TerrainStripMode::Near {
            texture
        } else {
            None
        },
    };
    let diagonal = (corners[1].cell_flags & 8) >> 3 == (!blend.orientation_word & 1) as u8;
    output.write(0x20, if diagonal { 0x4000 } else { 0 });
    if request.mode == TerrainStripMode::Far {
        output.write(0x24, request.material);
    } else if texture.is_none() {
        output.write(0x24, 0);
    }
    let orientation = blend.orientation_word;
    let indices: [usize; 4] =
        std::array::from_fn(|index| ((orientation >> (index * 8)) & 15) as usize);
    if indices.iter().any(|i| *i > 3) || (0..4).any(|i| indices[..i].contains(&indices[i])) {
        return Err(TerrainDrawFault::Orientation(orientation).into());
    }
    let fade_start = (((request.fade[1].wrapping_mul(2) / 3).wrapping_mul(55) / 100) + 256) as i16;
    for (source, target) in indices.into_iter().enumerate() {
        let corner = corners[source];
        output.write(
            target * 8,
            corner.projection.read_u32(0).ok_or_else(|| unknown("XY"))?,
        );
        let original_depth = corner
            .projection
            .read_u32(4)
            .ok_or_else(|| unknown("depth"))?;
        output.write(
            target * 8 + 4,
            original_depth.wrapping_add(if request.mode == TerrainStripMode::Far {
                0x400
            } else {
                0
            }),
        );
        let rgb = if request.mode == TerrainStripMode::Far {
            far_shade(corner.rgb, original_depth as i32, fade_start)?
        } else {
            corner.rgb
        };
        // A copied native RGB DWORD retains the terrain-cell flags high byte.
        let offset = 0x28 + target * 4;
        for (channel, byte) in rgb.into_iter().enumerate() {
            output.payload[offset + channel] = Some(byte);
        }
        // D030's negative-delta branch copies context+54 as a DWORD. BC30
        // wrote only its three128 bytes: the high byte remains unwritten.
        if request.mode == TerrainStripMode::Near
            || (original_depth as i32).wrapping_sub(i32::from(fade_start)) >= 0
        {
            output.payload[offset + 3] = Some(corner.cell_flags);
        }
        if fogged {
            output.payload[0x3c + target] = Some(fog[source]);
        }
    }
    if fogged {
        output.write(
            0x38,
            if request.mode == TerrainStripMode::Near {
                request.material
            } else {
                request.far_color
            },
        );
    }
    host.submit_quad(output).map_err(TerrainDrawError::Host)?;
    Ok(true)
}

pub(super) fn texture_sort<'a, E>(
    depth: u32,
    texture: RenderResource<'a>,
    identity: impl FnOnce(RenderResource<'a>) -> Result<TextureSortIdentity, E>,
) -> Result<u32, TerrainDrawError<E>> {
    let RenderResourceData::Sprite(entry) = texture.data else {
        return Err(TerrainDrawFault::Texture.into());
    };
    // Native terrain uses the first selector byte, irrespective of model mode.
    let selector = entry.selectors.mode0;
    if selector == 255 {
        return Ok((depth >> 1).wrapping_add(0xc0000000));
    }
    let variant = entry
        .variants
        .get(usize::from(selector))
        .ok_or(TerrainDrawFault::Texture)?;
    if variant.flags.raw() & 0x39 != 0 {
        return Ok((depth >> 1).wrapping_add(0xc0000000));
    }
    let identity = identity(texture).map_err(TerrainDrawError::Host)?;
    identity
        .sort_word(selector)
        .map_err(|_| TerrainDrawFault::Texture.into())
}

fn far_shade(rgb: [u8; 3], depth: i32, fade_start: i16) -> Result<[u8; 3], TerrainDrawFault> {
    let delta = depth.wrapping_sub(i32::from(fade_start));
    if delta < 0 {
        return Ok([128; 3]);
    }
    let span = i32::from(fade_start) + 0x800;
    if delta >= span {
        return Ok(rgb);
    }
    let mut out = [0; 3];
    for channel in 0..3 {
        out[channel] = (i32::from(rgb[channel])
            .wrapping_sub(128)
            .wrapping_mul(delta)
            .checked_div(span)
            .ok_or(TerrainDrawFault::Division)?
            + 128) as u8;
    }
    Ok(out)
}

/// 0048FAA0's two signed-DWORD cross products. The diagonal byte chooses
/// [D,A,B,C] versus[A,B,C,D] before the same four-corner test.
fn culled(corners: [TerrainVertex; 4]) -> Result<bool, TerrainDrawFault> {
    let mut xy = [[0; 2]; 4];
    for (target, vertex) in xy.iter_mut().zip(corners) {
        let word = vertex
            .projection
            .read_u32(0)
            .ok_or(TerrainDrawFault::Unwritten { field: "XY" })?;
        *target = [i32::from(word as i16), i32::from((word >> 16) as i16)];
    }
    if corners[1].cell_flags & 8 == 0 {
        xy.rotate_right(1);
    }
    let [a, b, c, d] = xy;
    let [bx, by] = [b[0] - a[0], b[1] - a[1]];
    let [cx, cy] = [c[0] - a[0], c[1] - a[1]];
    let first = bx.wrapping_mul(cy).wrapping_sub(by.wrapping_mul(cx));
    if first <= 0 {
        return Ok(false);
    }
    let second = (d[1] - a[1])
        .wrapping_mul(cx)
        .wrapping_sub((d[0] - a[0]).wrapping_mul(cy));
    Ok(second > 0)
}
