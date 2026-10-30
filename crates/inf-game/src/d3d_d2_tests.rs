//! Focused D2 backend tests: record reads (corners, per-corner
//! shades, texture), selector suppression, UV mapping, diffuse,
//! blend/policy. Packet shape follows `d3d_effect_tests`.

use super::*;
use crate::{
    model_primitives::PrimitiveCommand,
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 47,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 10,
};

fn sprite(selectors: [u8; 2]) -> SpriteEntry {
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: selectors[0],
            mode1: selectors[1],
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(0),
            color_count: 0,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width: 1,
            height: 1,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    }
}

fn resource(entry: &SpriteEntry) -> RenderResource<'_> {
    RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 10,
        },
        source: RenderSource {
            registration: &REGISTRATION,
            bytes: &[],
        },
        data: RenderResourceData::Sprite(entry),
    }
}

fn packet(entry: &SpriteEntry, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::D2TriangleNear, 0, 3);
    command.write_u32(0x18, mode);
    command.write_texture(0x1c, Some(resource(entry)));
    command
}

fn write_inputs(command: &mut PrimitiveCommand<'_>) {
    for (i, (xy, depth)) in [
        ([-123i16, 234i16], 64i32),
        ([32767, -32768], 256i32),
        ([1, 2], 512i32),
    ]
    .into_iter()
    .enumerate()
    {
        command.write_u32(
            i * 8,
            u32::from(xy[0] as u16) | (u32::from(xy[1] as u16) << 16),
        );
        command.write_u32(i * 8 + 4, depth as u32);
    }
    // Per-corner shade DWORDs (R/G/B low bytes); high bytes unconsumed.
    for (corner, shade) in [0x0001_0203u32, 0x0004_0506, 0x0007_0809]
        .into_iter()
        .enumerate()
    {
        command.write_u32(0x20 + corner * 4, shade);
    }
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [u32::MAX; 3],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn rejects_wrong_callback_and_blocks() {
    let entry = sprite([0, 0]);
    let mut far = packet(&entry, 0);
    far.callback = PrimitiveCallback::D2TriangleFar;
    // Far needs four blocks.
    assert!(matches!(read_d2(&far), Err(D2PixelError::Packet)));
    let mut wrong = packet(&entry, 0);
    wrong.callback = PrimitiveCallback::TexturedTriangleNear;
    assert!(matches!(read_d2(&wrong), Err(D2PixelError::Packet)));
}

#[test]
fn selector_ff_suppresses_before_vertices_and_shades() {
    let entry = sprite([255, 255]);
    assert!(read_d2(&packet(&entry, 0)).unwrap().is_none());
}

#[test]
fn packet_reads_corners_mode_shades_and_texture() {
    let entry = sprite([0, 0]);
    let mut command = packet(&entry, 0x20);
    write_inputs(&mut command);
    let triangle = read_d2(&command).unwrap().unwrap();
    assert_eq!(triangle.mode, 0x20);
    assert_eq!(
        triangle.vertices.map(|v| (v.x, v.y, v.depth)),
        [(-123., 234., 64), (32767., -32768., 256), (1., 2., 512)]
    );
    assert_eq!(
        triangle.shades,
        [[0x03, 0x02, 0x01], [0x06, 0x05, 0x04], [0x09, 0x08, 0x07]]
    );
}

#[test]
fn prepare_maps_uv_diffuse_and_sourcealpha_policy() {
    let entry = sprite([0, 0]);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    let triangle = read_d2(&command).unwrap().unwrap();
    let submission = prepare_d2(&triangle, renderer(), 8, false, [0., 0., 1., 1.]).primary;
    assert_eq!(submission.triangles, [[0, 1, 2]]);
    assert_eq!(
        submission.vertices.map(|v| v.uv),
        [[0., 0.], [1., 0.], [1., 1.]]
    );
    assert_eq!(
        submission.vertices.map(|v| v.diffuse),
        [[6, 4, 2, 255], [12, 10, 8, 255], [18, 16, 14, 255]]
    );
    assert_eq!(submission.policy.blend, D3dTextureBlend::SourceAlpha);
    assert_eq!(submission.policy.perspective, true);
    assert!(!submission.policy.fog.enabled());
    assert_eq!(submission.policy.depth_enable, Some(1));
    assert_eq!(submission.policy.depth_write, Some(1));
    // rhw/depth follow the textured-triangle kernel.
    assert_eq!(submission.vertices[0].rhw, 1.0 / 64.0);
}

#[path = "d3d_d2_prefix_tests.rs"]
mod prefix;
