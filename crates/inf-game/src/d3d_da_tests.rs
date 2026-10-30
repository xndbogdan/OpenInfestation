//! Focused DA backend tests: record reads (corners, no shade,
//! texture), selector/mode/bl suppression, UV mapping, opaque
//! white diffuse, flags-selected blend. Packet shape follows
//! `d3d_d2_tests`.

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

fn sprite(selectors: [u8; 2], flags: u16) -> SpriteEntry {
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
            flags: SpriteFlags::new(flags),
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
    let mut command = PrimitiveCommand::new(PrimitiveCallback::DaTriangleNear, 0, 2);
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
    let entry = sprite([0, 0], 5);
    let mut far = packet(&entry, 0);
    far.callback = PrimitiveCallback::DaTriangleFar;
    // Far needs three blocks.
    assert!(matches!(read_da(&far), Err(DaPixelError::Packet)));
    let mut wrong = packet(&entry, 0);
    wrong.callback = PrimitiveCallback::TexturedTriangleNear;
    assert!(matches!(read_da(&wrong), Err(DaPixelError::Packet)));
}

#[test]
fn selector_ff_suppresses_before_vertices() {
    let entry = sprite([255, 255], 5);
    assert!(read_da(&packet(&entry, 0)).unwrap().is_none());
}

/// `004C2200`/`004C2DF0` never test the packet's mode word for the
/// second pass: a set bit 1 draws like any other triangle.
#[test]
fn packet_mode_bit1_does_not_select_the_second_pass() {
    let entry = sprite([0, 0], 5);
    let mut command = packet(&entry, 0x2);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    assert_eq!(triangle.mode, 0x2);
    assert!(!triangle.descriptor_0200);
}

/// `TEST AH, 2` at `004C266F`/`004C3383` reads the selected variant's
/// flags word: bit `0x200` marks the second pass, beside the low byte.
#[test]
fn variant_flag_0200_marks_the_second_pass() {
    let entry = sprite([0, 0], 0x205);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    assert!(triangle.descriptor_0200);
    assert_eq!(triangle.bl, 5);
}

#[test]
fn additive_variants_draw_with_white_near_corners() {
    // Sprite 577's frame-1 flags (0x14): additive, fog off, no depth
    // writes.
    let entry = sprite([0, 0], 0x14);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    let submission = prepare_da(&triangle, renderer(), 0, [0., 0., 1., 1.]).unwrap();
    assert_eq!(submission.vertices.map(|v| v.diffuse), [[255; 4]; 3]);
    assert_eq!(submission.policy.blend, D3dTextureBlend::Additive);
    assert!(!submission.policy.fog.enabled());
    assert_eq!(submission.policy.depth_write, Some(0));
}

#[test]
fn packet_reads_corners_mode_bl_and_texture() {
    let entry = sprite([0, 0], 5);
    let mut command = packet(&entry, 0x20);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    assert_eq!(triangle.mode, 0x20);
    assert_eq!(triangle.bl, 5);
    assert_eq!(
        triangle.vertices.map(|v| (v.x, v.y, v.depth)),
        [(-123., 234., 64), (32767., -32768., 256), (1., 2., 512)]
    );
}

#[test]
fn prepare_maps_uv_opaque_white_and_flags_blend() {
    let entry = sprite([0, 0], 5);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    let submission = prepare_da(&triangle, renderer(), 8, [0., 0., 1., 1.]).unwrap();
    assert_eq!(submission.triangles, [[0, 1, 2]]);
    assert_eq!(
        submission.vertices.map(|v| v.uv),
        [[0., 0.], [1., 0.], [1., 1.]]
    );
    assert_eq!(
        submission.vertices.map(|v| v.diffuse),
        [
            [255, 255, 255, 255],
            [255, 255, 255, 255],
            [255, 255, 255, 255]
        ]
    );
    assert_eq!(submission.vertices.map(|v| v.specular), [0xff00_0000; 3]);
    assert_eq!(submission.policy.blend, D3dTextureBlend::SourceAlpha);
    assert_eq!(submission.policy.perspective, true);
    assert!(!submission.policy.fog.enabled());
    assert_eq!(submission.policy.depth_enable, Some(1));
    // `bl&0x31` (here 0x01) stops depth writes.
    assert_eq!(submission.policy.depth_write, Some(0));
    assert_eq!(submission.vertices[0].rhw, 1.0 / 64.0);
}

#[test]
fn far_packet_reads_color_tail_and_per_corner_fog() {
    let entry = sprite([0, 0], 5);
    let mut command = PrimitiveCommand::new(PrimitiveCallback::DaTriangleFar, 0, 3);
    command.write_u32(0x18, 0);
    command.write_texture(0x1c, Some(resource(&entry)));
    write_inputs(&mut command);
    command.write_u32(0x20, 0x5284);
    command.write_byte(0x24, 0x75);
    command.write_byte(0x25, 0x6e);
    command.write_byte(0x26, 0x00);
    let triangle = read_da(&command).unwrap().unwrap();
    assert_eq!(triangle.fog, Some([0x75, 0x6e, 0x00]));
    let submission = prepare_da(&triangle, renderer(), 8, [0., 0., 1., 1.]).unwrap();
    assert_eq!(
        submission.vertices.map(|v| v.specular),
        [0x8a00_0000, 0x9100_0000, 0xff00_0000]
    );
}

#[test]
fn specular_derives_inverted_fog_alpha() {
    // Banked filled speculars (draw oracle): far 0x8A/0x91 from
    // record bytes ~0x75/0x6E; near fills 0xFF000000 (fog 0).
    assert_eq!(da_specular(0x00), 0xff00_0000);
    assert_eq!(da_specular(0x75), 0x8a00_0000);
    assert_eq!(da_specular(0x6e), 0x9100_0000);
    assert_eq!(da_specular(0xff), 0x0000_0000);
}

#[test]
fn blend_follows_traced_flag_rule() {
    assert_eq!(da_blend(0x10, 8), D3dTextureBlend::Additive);
    assert_eq!(da_blend(0x20, 8), D3dTextureBlend::InverseSourceColor);
    assert_eq!(da_blend(0x05, 8), D3dTextureBlend::SourceAlpha);
    assert_eq!(da_blend(0x08, 0), D3dTextureBlend::SourceAlpha);
    assert_eq!(da_blend(0x00, 8), D3dTextureBlend::SourceAlpha);
    assert_eq!(da_blend(0x00, 0), D3dTextureBlend::Copy);
}

fn two_pass_inputs() -> DaTwoPassInputs {
    DaTwoPassInputs {
        center: [320., 240.],
        focals: [512., 512.],
        du: 0.25,
        dv: 0.25,
        c14: 0.5,
        c18: 0.5,
    }
}

#[test]
fn second_pass_centers_midrange() {
    // X=P, Y=Q zeroes u/v, landing mid-scaled-range.
    assert_eq!(
        da_second_pass_uv(320., 240., 64, two_pass_inputs()),
        [0.75, 0.75]
    );
}

#[test]
fn second_pass_exact_power_of_two_step() {
    // su=sv=2^-17: X-P=2^17 hits u=1 exactly (saturate edge,
    // no clamp); Q-Y=2^16 gives v=1/2.
    assert_eq!(
        da_second_pass_uv(320. + 131072., 240. - 65536., 1, two_pass_inputs()),
        [1., 0.625]
    );
}

#[test]
fn second_pass_saturates_both_directions() {
    // Huge offsets clamp: u=+1 -> tu=2*DU+C14; u=-1 -> C14.
    // Note the asymmetric V form: v=-1 -> 2*DV+C18, v=+1 -> C18.
    assert_eq!(da_second_pass_uv(1e10, 240., 1, two_pass_inputs())[0], 1.);
    assert_eq!(da_second_pass_uv(-1e10, 240., 1, two_pass_inputs())[0], 0.5);
    assert_eq!(da_second_pass_uv(320., 1e10, 1, two_pass_inputs())[1], 1.);
    assert_eq!(da_second_pass_uv(320., -1e10, 1, two_pass_inputs())[1], 0.5);
}

#[test]
fn second_pass_keeps_first_pass_shape_with_rescaled_uvs() {
    let entry = sprite([0, 0], 5);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    let triangle = read_da(&command).unwrap().unwrap();
    let first = prepare_da(&triangle, renderer(), 8, [0., 0., 1., 1.]).unwrap();
    let second = prepare_da_second_pass(&triangle, renderer(), 8, two_pass_inputs()).unwrap();
    assert_eq!(second.triangles, first.triangles);
    assert_eq!(
        second.vertices.map(|v| (v.x, v.y, v.diffuse, v.specular)),
        first.vertices.map(|v| (v.x, v.y, v.diffuse, v.specular))
    );
    assert_eq!(second.policy.blend, first.policy.blend);
    assert_eq!(second.policy.perspective, first.policy.perspective);
    assert_eq!(second.mode, first.mode);
    assert_eq!(
        second.vertices.map(|v| v.uv),
        [
            da_second_pass_uv(-123., 234., 64, two_pass_inputs()),
            da_second_pass_uv(32767., -32768., 256, two_pass_inputs()),
            da_second_pass_uv(1., 2., 512, two_pass_inputs()),
        ]
    );
}

#[derive(serde::Deserialize)]
struct NativeFan {
    kind: u32,
    vertex_type: u32,
    count: u32,
    flags: u32,
    vertices: Vec<[u32; 8]>,
}

#[derive(serde::Deserialize)]
struct NativeCase {
    flags: u8,
    mode: u32,
    shifts: [u32; 3],
    near: u32,
    manages: u8,
    global_policy: u32,
    depths: [i32; 3],
    fans: Vec<NativeFan>,
    states: Vec<[u32; 2]>,
    far_color: Option<u32>,
    fog: Option<[u8; 3]>,
}

fn bits(v: D3dTlVertex) -> [u32; 8] {
    let [r, g, b, a] = v.diffuse;
    [
        v.x.to_bits(),
        v.y.to_bits(),
        v.depth.to_bits(),
        v.rhw.to_bits(),
        u32::from_be_bytes([a, r, g, b]),
        v.specular,
        v.uv[0].to_bits(),
        v.uv[1].to_bits(),
    ]
}

/// 336 unmodified 004C2200/004C2DF0 executions (check_da_consumers.py),
/// x87 CW 027F, renderer +3C/+40 = 2/7: every low-byte flag combination,
/// near and far, both global policies. Mode 20 bypasses clipping, so the
/// one drawn fan is the prepared vertices.
#[test]
fn original_da_tl_records_and_state_receipts_match() {
    let receipt: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/d3d_da_native.json")).unwrap();
    let uv: [f32; 4] = serde_json::from_value(receipt["uv"].clone()).unwrap();
    let coords: [[i16; 2]; 3] = serde_json::from_value(receipt["coords"].clone()).unwrap();
    let cases: Vec<NativeCase> = serde_json::from_value(receipt["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 336);
    for case in &cases {
        let entry = sprite([0, 0], u16::from(case.flags));
        let triangle = DaTriangle {
            vertices: std::array::from_fn(|i| FlatVertex {
                x: coords[i][0].into(),
                y: coords[i][1].into(),
                depth: case.depths[i],
            }),
            mode: case.mode,
            resource: resource(&entry),
            bl: case.flags,
            fog: case.fog,
            far_color: case.far_color,
            descriptor_0200: false,
        };
        let state = D3dFlatState {
            material_shifts: case.shifts,
            near_depth: case.near,
            manages_depth: case.manages != 0,
            depth_enable: 2,
            depth_write: 7,
        };
        let submission = prepare_da(&triangle, state, case.global_policy, uv).unwrap();
        let [fan] = &case.fans[..] else {
            panic!("one fan per case")
        };
        assert_eq!(
            (fan.kind, fan.vertex_type, fan.count, fan.flags),
            (6, 3, 3, 0x1c)
        );
        assert_eq!(submission.vertices.map(bits).to_vec(), fan.vertices);
        let state = |id| case.states.iter().rev().find(|v| v[0] == id).map(|v| v[1]);
        assert_eq!(submission.policy.depth_enable, state(7));
        assert_eq!(submission.policy.depth_write, state(14));
        assert_eq!(state(4), Some(u32::from(submission.policy.perspective)));
        assert_eq!(Some(u32::from(submission.policy.fog.enabled())), state(28));
        // Far sets FOGCOLOR from the far color first, whether or not it
        // then enables fog; the port carries it only with enabled fog.
        let far_color = case.far_color.map(|color| {
            let [r, g, b] = crate::d3d_effect::far_rgb(color, case.shifts);
            u32::from_be_bytes([0, r, g, b])
        });
        assert_eq!(far_color, state(34));
        if let crate::d3d_textured::D3dTextureFog::Vertex { color: [r, g, b] } =
            submission.policy.fog
        {
            assert_eq!(Some(u32::from_be_bytes([0, r, g, b])), state(34));
        }
        let factors = match submission.policy.blend {
            D3dTextureBlend::Copy => [2, 1],
            D3dTextureBlend::SourceAlpha => [5, 6],
            D3dTextureBlend::Additive => [2, 2],
            D3dTextureBlend::InverseSourceColor => [1, 4],
        };
        assert_eq!([state(19), state(20)], factors.map(Some));
    }
}
