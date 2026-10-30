//! Direct3D DA textured triangle: kinds 004A3A00/004A3A40,
//! renderer virtuals 005290E8+54/+58 (`004C2200`/`004C2DF0`).
//! The near record carries corners, mode, material (no shades —
//! entry fills are opaque white); far adds far color and fog.
//! UVs come from the upload allocation bounds with the same
//! per-corner assignment as D2 (`(b14,b18)`, `(b1C,b18)`,
//! `(b1C,b20)`), proven by the DA-bounds oracle end to end
//! (bounds floats → fill → draw block).
//!
//! The second pass (`rep movs` + refill + UV-scale loop + second
//! draw) runs only when the selected variant's flags have `0x200`
//! (`TEST AH, 2` on the flags word at `004C266F`/`004C3383`) and
//! renderer `+44` holds a texture (`004C2696`/`004C338C`); the
//! packet's mode word is never tested for it. The port's device
//! has no `+44` texture, so DA draws single-pass, and a
//! descriptor-`0x200` triangle on a device with one reports the
//! unported second pass after its primary draw.
//! The selected variant's flags low byte `bl` picks the corners, the
//! blend and the fog (`check_da_consumers.py`'s receipts). Without
//! `bl&0x30` the corners are white with alpha 0x80 for `bl&8`
//! (`004C2349`/`004C2FA6`) or 0xFF (`004C238D`/`004C2FEA`), and Far
//! enables device fog with the far color and each corner's `255-fog`
//! specular alpha. With `bl&0x30` (additive or inverse source color)
//! fog stays off: Near corners are white, Far corners `255-fog` grey.
//! Depth writes stop for `bl&0x39`, as for the textured quads.

use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_textured::{sprite_mode, D3dTextureBlend, D3dTexturePolicy, D3dTexturedSubmission},
    d3d_viewport::D3dTlVertex,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{RenderResource, RenderResourceData},
    software_frame::FlatVertex,
};

#[derive(Clone, Copy, Debug)]
pub struct DaTriangle<'a> {
    pub vertices: [FlatVertex; 3],
    pub mode: u32,
    pub resource: RenderResource<'a>,
    /// Mode-selected variant flags low byte: entry-fill variant
    /// (`bl&0x30`) and blend select. Proven equal to the material
    /// `bl` for both frame-1 materials (sprite 633 → `0x05`,
    /// sprite 577 → `0x14`).
    pub bl: u8,
    /// Far fog bytes for the specular fog factor; `None` on near
    /// packets (which carry no fog tail).
    pub fog: Option<[u8; 3]>,
    /// The far packet's color at `+20`, the device fog color.
    pub far_color: Option<u32>,
    /// The selected variant's flags bit `0x200`: with a renderer
    /// `+44` texture the native consumer runs its second pass.
    pub descriptor_0200: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DaPixelError {
    #[error("DA triangle requires callback 004A3A00/two blocks or 004A3A40/three blocks")]
    Packet,
    #[error("DA triangle is missing {field} at payload offset {offset:#x}")]
    Unknown { field: &'static str, offset: usize },
    #[error("DA triangle requires a Handler-3 relocation at payload+1C")]
    Texture,
    #[error("DA sprite selector {0} does not identify a physical descriptor")]
    Selector(usize),
    #[error(
        "DA primary pass completed; descriptor 0x200 with renderer +44 requires the second pass"
    )]
    SecondaryPass,
}

/// Selector FF suppresses the draw before coordinates, upload or
/// renderer states are consumed. The triangle consumes +00..1F; the
/// far tail adds the fog color at +20 and the corners' fog bytes.
pub fn read_da<'a>(command: &PrimitiveCommand<'a>) -> Result<Option<DaTriangle<'a>>, DaPixelError> {
    let far = match command.callback {
        PrimitiveCallback::DaTriangleNear => false,
        PrimitiveCallback::DaTriangleFar => true,
        _ => return Err(DaPixelError::Packet),
    };
    let blocks = if far { 3 } else { 2 };
    if command.payload_blocks != blocks || command.payload.len() != blocks * 16 {
        return Err(DaPixelError::Packet);
    }
    let read = |offset, field| {
        command
            .read_u32(offset)
            .ok_or(DaPixelError::Unknown { field, offset })
    };
    let mode = read(0x18, "mode")?;
    let Some((0x1c, resource)) = command.texture else {
        return Err(DaPixelError::Texture);
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return Err(DaPixelError::Texture);
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return Ok(None);
    };
    let variant = entry
        .variants
        .get(selector)
        .ok_or(DaPixelError::Selector(selector))?;
    let bl = (variant.flags.raw() & 0xff) as u8;
    let descriptor_0200 = variant.flags.raw() & 0x200 != 0;
    let mut vertices = [FlatVertex {
        x: 0.,
        y: 0.,
        depth: 0,
    }; 3];
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let xy = read(i * 8, "projected XY")?;
        *vertex = FlatVertex {
            x: command.corner(xy)[0],
            y: command.corner(xy)[1],
            depth: read(i * 8 + 4, "signed depth")? as i32,
        };
    }
    // The far color at +0x20 is the device fog color; three fog
    // bytes follow at +0x24. Near packets have no tail.
    let far_color = if far {
        Some(read(0x20, "far color")?)
    } else {
        None
    };
    let fog = if far {
        Some([
            command.payload[0x24].ok_or(DaPixelError::Unknown {
                field: "fog",
                offset: 0x24,
            })?,
            command.payload[0x25].ok_or(DaPixelError::Unknown {
                field: "fog",
                offset: 0x25,
            })?,
            command.payload[0x26].ok_or(DaPixelError::Unknown {
                field: "fog",
                offset: 0x26,
            })?,
        ])
    } else {
        None
    };
    Ok(Some(DaTriangle {
        vertices,
        mode,
        resource,
        bl,
        fog,
        far_color,
        descriptor_0200,
    }))
}

/// Traced blend rule over the variant-flags low byte:
/// `bl&0x30` selects additive (`bl&0x10`) vs inverse-source,
/// otherwise SourceAlpha, or Copy when both `bl&8` and the
/// global `0x65EF2C` bit 3 are clear.
pub fn da_blend(bl: u8, global_policy: u32) -> D3dTextureBlend {
    if bl & 0x30 != 0 {
        if bl & 0x10 != 0 {
            D3dTextureBlend::Additive
        } else {
            D3dTextureBlend::InverseSourceColor
        }
    } else if bl & 8 != 0 || global_policy & 8 != 0 {
        D3dTextureBlend::SourceAlpha
    } else {
        D3dTextureBlend::Copy
    }
}

/// DA submission: opaque white corners over the bound texture's
/// allocation rect (entry-fill color `0xFFFFFFFF`; the fog-derived
/// CF84 path stays skipped). UV assignment mirrors the native
/// fill (`(b14,b18)`, `(b1C,b18)`, `(b1C,b20)`); `uv_bounds` is
/// that allocation's +1C/+20/+24/+28, not parser-inferred bounds.
pub fn prepare_da(
    triangle: &DaTriangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    uv_bounds: [f32; 4],
) -> Result<D3dTexturedSubmission<3, 1>, DaPixelError> {
    let [u0, v0, u1, v1] = uv_bounds;
    submit_da(
        triangle,
        renderer,
        global_policy,
        [[u0, v0], [u1, v0], [u1, v1]],
    )
}

/// Second-pass inputs for descriptor-`0x200` draws. `center` is the
/// viewport center (`P`/`Q`), `focals` the projection focals;
/// `du`/`dv`/`c14`/`c18` come from the upload allocation and
/// record. The port does NOT assume `c14=u0`/`c18=v0`: that
/// span fit is unproven, so they travel explicitly. The backend
/// carries no focal/center or `+44` texture state, so nothing live
/// supplies these; a descriptor-`0x200` triangle on a device with a
/// `+44` texture reports [`DaPixelError::SecondaryPass`].
#[derive(Clone, Copy, Debug)]
pub struct DaTwoPassInputs {
    pub center: [f32; 2],
    pub focals: [f32; 2],
    pub du: f32,
    pub dv: f32,
    pub c14: f32,
    pub c18: f32,
}

/// Second-pass UV rescale (`0x4C2B89..0x4C2CBA`, FPU-stack
/// traced): `u=((X-P)*su*D)` / `v=((Q-Y)*sv*D)` with
/// `su=(1/fx)/256` / `sv=(1/fy)/256`, `D` the signed depth,
/// saturated to `[-1,1]`, then `tu=(u+1)*DU+C14` /
/// `tv=(1-v)*DV+C18` (note the asymmetric V form, verbatim).
/// The out-of-block scratch first iteration has no pixel effect
/// and is dropped. Inputs are finite screen words and depths,
/// like the backend's own finite-vertex contract.
pub fn da_second_pass_uv(x: f32, y: f32, depth: i32, inputs: DaTwoPassInputs) -> [f32; 2] {
    let su = (1.0 / inputs.focals[0]) / 256.0;
    let sv = (1.0 / inputs.focals[1]) / 256.0;
    let d = depth as f32;
    let u = ((x - inputs.center[0]) * su * d).clamp(-1.0, 1.0);
    let v = ((inputs.center[1] - y) * sv * d).clamp(-1.0, 1.0);
    [
        (u + 1.0) * inputs.du + inputs.c14,
        (1.0 - v) * inputs.dv + inputs.c18,
    ]
}

/// Second-pass submission: same corners, opaque whites, blend
/// and policy as the first pass, rescaled UVs. Emits the second
/// `+0x74` draw once the backend can supply [`DaTwoPassInputs`].
pub fn prepare_da_second_pass(
    triangle: &DaTriangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    inputs: DaTwoPassInputs,
) -> Result<D3dTexturedSubmission<3, 1>, DaPixelError> {
    let uv = triangle
        .vertices
        .map(|vertex| da_second_pass_uv(vertex.x, vertex.y, vertex.depth, inputs));
    submit_da(triangle, renderer, global_policy, uv)
}

/// Corner-block specular fog factor (`0x4C3215..0x4C3260`,
/// objdump): the fill loads the record fog byte, computes
/// `((0xFF - fog) << 24)` (`inc`/`neg`/`shl 0x18`) with black
/// specular color -- D3D fog alpha, 255 unfogged. Near packets
/// carry no fog tail and fill `0xFF000000` (fog 0).
pub fn da_specular(fog: u8) -> u32 {
    u32::from(0xff - fog) << 24
}

fn submit_da(
    triangle: &DaTriangle<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    uv: [[f32; 2]; 3],
) -> Result<D3dTexturedSubmission<3, 1>, DaPixelError> {
    let blend = da_blend(triangle.bl, global_policy);
    let lit = triangle.bl & 0x30 == 0;
    let alpha = if triangle.bl & 8 != 0 { 0x80 } else { 0xff };
    let fog = match triangle.far_color {
        Some(color) if lit => crate::d3d_textured::D3dTextureFog::Vertex {
            color: crate::d3d_effect::far_rgb(color, renderer.material_shifts),
        },
        _ => crate::d3d_textured::D3dTextureFog::Disabled,
    };
    Ok(D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let vertex = triangle.vertices[i];
            let rhw = (1.0 / f64::from(vertex.depth)) as f32;
            let (diffuse, specular) = match triangle.fog {
                Some(fog) if lit => ([255, 255, 255, alpha], da_specular(fog[i])),
                Some(fog) => {
                    let grey = 0xff - fog[i];
                    ([grey, grey, grey, 0xff], da_specular(0))
                }
                None if lit => ([255, 255, 255, alpha], da_specular(0)),
                None => ([255; 4], da_specular(0)),
            };
            D3dTlVertex {
                x: vertex.x,
                y: vertex.y,
                depth: (1.0 - f64::from(renderer.near_depth) * f64::from(rhw)) as f32,
                rhw,
                diffuse,
                specular,
                uv: uv[i],
            }
        }),
        triangles: [[0, 1, 2]],
        policy: D3dTexturePolicy {
            depth_enable: renderer.manages_depth.then_some(renderer.depth_enable),
            depth_write: renderer
                .manages_depth
                .then_some(if triangle.bl & 0x39 != 0 {
                    0
                } else {
                    renderer.depth_write
                }),
            blend,
            perspective: triangle.mode & 0x100 == 0,
            fog,
        },
        mode: triangle.mode,
        near_depth: renderer.near_depth,
    })
}

#[cfg(test)]
#[path = "d3d_da_tests.rs"]
mod tests;
