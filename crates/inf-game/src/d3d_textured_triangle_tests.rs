use super::*;
use crate::{
    d3d_viewport::{clip_polygon, D3dViewportState},
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};
use serde::Deserialize;

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 47,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 10,
};

fn sprite(selectors: [u8; 2], flags: &[u16]) -> SpriteEntry {
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: selectors[0],
            mode1: selectors[1],
            reserved: 0,
        },
        variants: flags
            .iter()
            .enumerate()
            .map(|(index, &flags)| SpriteVariant {
                physical_index: index as u8,
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
                // Packet conversion does not consume texels or palette bytes.
                encoding: SpriteEncoding::DirectRgb555 {
                    pixels: ByteSpan::new(0, 2),
                },
            })
            .collect(),
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
    let mut command = PrimitiveCommand::new(PrimitiveCallback::TexturedTriangleNear, 0, 5);
    command.write_u32(0x18, mode);
    command.write_texture(0x1c, Some(resource(entry)));
    command
}

/// Shared-shade record as the DE producer emits it: three corner
/// XY/depth pairs, mode, material relocation, one shade DWORD.
fn de_packet(entry: &SpriteEntry, mode: u32, shade: u32) -> PrimitiveCommand<'_> {
    use PrimitiveCallback::SharedShadeTriangleNear;
    let mut command = PrimitiveCommand::new(SharedShadeTriangleNear, 0, 3);
    for i in 0..3 {
        command.write_u32(i * 8, 0x8000_7fff);
        command.write_u32(i * 8 + 4, i32::MIN as u32 + i as u32);
    }
    command.write_u32(0x18, mode);
    command.write_texture(0x1c, Some(resource(entry)));
    command.write_u32(0x20, shade);
    command
}

fn complete_packet(entry: &SpriteEntry, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = packet(entry, mode);
    for i in 0..3 {
        command.write_u32(i * 8, 0x8000_7fff);
        command.write_u32(i * 8 + 4, i32::MIN as u32 + i as u32);
        command.write_rgb(0x20 + i * 4, [128 + i as u8, 64, 32]);
        command.write_u32(0x2c + i * 8, u32::MAX);
        command.write_u32(0x30 + i * 8, 65535);
    }
    command
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [u32::MAX; 3], // Not consumed by 004CE930.
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn ff_selector_suppresses_before_geometry_shade_uv_and_descriptor() {
    // Native read hooks confirm only +18/+1C packet reads on selector FF;
    // no display-begin, upload, state, clip or draw calls then occur.
    let entry = sprite([255, 0], &[0]);
    for mode in [0, 0x20, 0x100, 0x4000, u32::MAX - 1] {
        assert!(read_textured_triangle(&packet(&entry, mode))
            .unwrap()
            .is_none());
    }
    let entry = sprite([0, 255], &[0]);
    for mode in [1, 0x21, 0x101, 0x4001, u32::MAX] {
        assert!(read_textured_triangle(&packet(&entry, mode))
            .unwrap()
            .is_none());
    }
    let entry = sprite([255, 255], &[]);
    assert!(read_textured_triangle(&packet(&entry, 0))
        .unwrap()
        .is_none());
    let entry = sprite([0, 1], &[0]);
    assert!(matches!(
        read_textured_triangle(&packet(&entry, 1)),
        Err(TexturedPixelError::Selector(1))
    ));
    assert!(matches!(
        read_textured_triangle(&packet(&entry, 0)),
        Err(TexturedPixelError::Unknown { offset: 0, .. })
    ));
}

#[test]
fn packet_extent_callback_and_relocation_are_explicit() {
    let entry = sprite([0, 0], &[0]);
    let mut command = complete_packet(&entry, 0);
    command.callback = PrimitiveCallback::FlatTriangleFar;
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::TrianglePacket)
    ));
    command.callback = PrimitiveCallback::TexturedTriangleNear;
    command.payload_blocks = 4;
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::TrianglePacket)
    ));
    command.payload_blocks = 5;
    command.payload.pop();
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::TrianglePacket)
    ));
    command.payload.push(None);
    command.texture = Some((0x24, resource(&entry)));
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::TriangleTexture)
    ));
    command.texture = None;
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::TriangleTexture)
    ));
}

#[test]
fn packet_reads_signed_inputs_and_only_significant_color_bytes() {
    let entry = sprite([0, 1], &[0x08, 0x10]);
    let command = complete_packet(&entry, 0x4101);
    let triangle = read_textured_triangle(&command).unwrap().unwrap();
    assert_eq!(triangle.flags, 0x10);
    assert_eq!(triangle.mode, 0x4101);
    for i in 0..3 {
        assert_eq!(
            [triangle.vertices[i].x, triangle.vertices[i].y],
            [32767., -32768.]
        );
        assert_eq!(triangle.vertices[i].depth, i32::MIN + i as i32);
        assert_eq!(triangle.shades[i], [128 + i as u8, 64, 32]);
        assert_eq!(triangle.uv[i], [-1, 65535]);
        assert_eq!(command.payload[0x23 + i * 4], None);
    }
    assert!(command.payload[0x44..].iter().all(Option::is_none));
    // Every significant shade byte must exist; an absent byte never becomes
    // a white/black default. Color high bytes and tail remain unknown.
    for corner in 0..3 {
        for channel in 0..3 {
            let offset = 0x20 + corner * 4 + channel;
            let mut missing = complete_packet(&entry, 0);
            missing.payload[offset] = None;
            assert!(matches!(
                read_textured_triangle(&missing),
                Err(TexturedPixelError::Unknown { field: "corner shade byte", offset: found }) if found == offset
            ));
        }
    }
    for offset in [0x2c, 0x30, 0x34, 0x38, 0x3c, 0x40] {
        let mut missing = complete_packet(&entry, 0);
        missing.payload[offset] = None;
        assert!(matches!(
            read_textured_triangle(&missing),
            Err(TexturedPixelError::Unknown { offset: found, .. }) if found == offset
        ));
    }
}

#[test]
fn far_tail_has_required_color_and_corner_fog_but_no_padding_default() {
    let entry = sprite([0, 0], &[0]);
    let mut command = complete_packet(&entry, 0);
    command.callback = PrimitiveCallback::TexturedTriangleFar;
    assert!(matches!(
        read_textured_triangle(&command),
        Err(TexturedPixelError::Unknown { offset: 0x44, .. })
    ));
    command.write_u32(0x44, 0x1234_5678);
    for index in 0..3 {
        assert!(matches!(
            read_textured_triangle(&command),
            Err(TexturedPixelError::Unknown { offset, .. }) if offset == 0x48 + index
        ));
        command.write_byte(0x48 + index, [0, 127, 255][index]);
    }
    let triangle = read_textured_triangle(&command).unwrap().unwrap();
    assert_eq!(
        triangle.shading,
        TexturedTriangleShading::Far {
            color: 0x1234_5678,
            fog: [0, 127, 255],
        }
    );
    assert!(command.payload[0x4b..].iter().all(Option::is_none));
    let entry = sprite([255, 0], &[]);
    let mut command = packet(&entry, 0);
    command.callback = PrimitiveCallback::TexturedTriangleFar;
    assert!(read_textured_triangle(&command).unwrap().is_none());
}

#[test]
fn shade_preserves_hue_and_uses_nearest_even_after_normalization() {
    assert_eq!(normalized_shade([0, 0, 0]), [0, 0, 0]);
    assert_eq!(normalized_shade([128, 64, 32]), [255, 128, 64]);
    assert_eq!(normalized_shade([130, 65, 0]), [255, 128, 0]);
    assert_eq!(normalized_shade([201, 134, 67]), [255, 170, 85]);
    assert_eq!(normalized_shade([255, 128, 64]), [255, 128, 64]);
}

#[test]
fn state_priority_secondary_gate_and_triangle_order_are_explicit() {
    let entry = sprite([0, 0], &[0]);
    let mut triangle = read_textured_triangle(&complete_packet(&entry, 0))
        .unwrap()
        .unwrap();
    for (flags, global, blend, alpha, depth_write) in [
        (0, 0, D3dTextureBlend::Copy, 255, 1),
        (1, 0, D3dTextureBlend::Copy, 255, 0),
        (0, 8, D3dTextureBlend::SourceAlpha, 255, 1),
        (8, 8, D3dTextureBlend::SourceAlpha, 128, 0),
        (0x28, 8, D3dTextureBlend::InverseSourceColor, 255, 0),
        (0x38, 8, D3dTextureBlend::Additive, 255, 0),
        (0x200, 0, D3dTextureBlend::Copy, 255, 1),
    ] {
        triangle.flags = flags;
        let result =
            prepare_textured_triangle(&triangle, renderer(), global, false, [0., 0., 1., 1.])
                .primary;
        assert_eq!(
            result.policy,
            D3dTexturePolicy {
                depth_enable: Some(1),
                depth_write: Some(depth_write),
                blend,
                perspective: true,
                fog: crate::d3d_textured::D3dTextureFog::Disabled,
            }
        );
        assert!(result.vertices.iter().all(|v| v.diffuse[3] == alpha));
    }
    let primary = prepare_textured_triangle(&triangle, renderer(), 0, false, [0.; 4]);
    let secondary = prepare_textured_triangle(&triangle, renderer(), 0, true, [0.; 4]);
    assert_eq!(secondary.primary, primary.primary);
    assert_eq!(primary.continuation, TexturedTriangleContinuation::Complete);
    assert_eq!(
        secondary.continuation,
        TexturedTriangleContinuation::SecondaryPassRequired
    );
    triangle.flags = 0;
    for mode in [0, 0x20, 0x100, 0x4000, u32::MAX] {
        triangle.mode = mode;
        let result = prepare_textured_triangle(
            &triangle,
            D3dFlatState {
                manages_depth: false,
                ..renderer()
            },
            0,
            true,
            [0.; 4],
        )
        .primary;
        assert_eq!(result.policy.depth_enable, None);
        assert_eq!(result.policy.depth_write, None);
        assert_eq!(result.policy.perspective, mode & 0x100 == 0);
        assert_eq!(result.triangles, [[0, 1, 2]]);
        assert_eq!(result.mode, mode);
    }
}

#[derive(Deserialize)]
struct NativeFar {
    color: u32,
    fog: [u8; 3],
    shifts: [u32; 3],
}

#[derive(Deserialize)]
struct NativeCase {
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    depths: [i32; 3],
    coords: [[i16; 2]; 3],
    shades: [[u8; 3]; 3],
    uv: [[i32; 2]; 3],
    uv_bounds: [f32; 4],
    viewport: [i16; 4],
    vertices: [[u32; 8]; 3],
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
    #[serde(default)]
    far: Option<NativeFar>,
}

fn vertex_bits(vertex: D3dTlVertex) -> [u32; 8] {
    let [r, g, b, a] = vertex.diffuse;
    [
        vertex.x.to_bits(),
        vertex.y.to_bits(),
        vertex.depth.to_bits(),
        vertex.rhw.to_bits(),
        u32::from_be_bytes([a, r, g, b]),
        vertex.specular,
        vertex.uv[0].to_bits(),
        vertex.uv[1].to_bits(),
    ]
}

fn verify_native_cases(cases: &[NativeCase]) {
    for (index, case) in cases.iter().enumerate() {
        let entry = sprite([0, 0], &[case.flags]);
        let mut command = packet(&entry, case.mode);
        if let Some(far) = &case.far {
            command.callback = PrimitiveCallback::TexturedTriangleFar;
            command.write_u32(0x44, far.color);
            command.write_rgb(0x48, far.fog);
        }
        for i in 0..3 {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_rgb(0x20 + i * 4, case.shades[i]);
            command.write_u32(0x2c + i * 8, case.uv[i][0] as u32);
            command.write_u32(0x30 + i * 8, case.uv[i][1] as u32);
        }
        let triangle = read_textured_triangle(&command).unwrap().unwrap();
        let result = prepare_textured_triangle(
            &triangle,
            D3dFlatState {
                near_depth: case.near,
                manages_depth: case.manages,
                depth_enable: case.depth_enable,
                depth_write: case.depth_write,
                material_shifts: case.far.as_ref().map_or([u32::MAX; 3], |far| far.shifts),
                ..renderer()
            },
            case.policy,
            false,
            case.uv_bounds,
        )
        .primary;
        assert_eq!(
            result.vertices.map(vertex_bits),
            case.vertices,
            "native pre-clip TL records case {index}"
        );
        assert_eq!(result.triangles, [[0, 1, 2]]);
        let polygon = if case.mode & 0x20 != 0 {
            result.vertices.to_vec()
        } else {
            clip_polygon(
                &result.vertices,
                D3dViewportState {
                    bounds: case.viewport,
                    near_depth: result.near_depth,
                    mode: result.mode,
                },
            )
            .unwrap()
        };
        let draws: Vec<_> = if polygon.len() >= 3 {
            vec![polygon.into_iter().map(vertex_bits).collect::<Vec<_>>()]
        } else {
            vec![]
        };
        assert_eq!(
            draws, case.draws,
            "native ordered triangle fan case {index}"
        );
        let state = |id| {
            case.states
                .iter()
                .rev()
                .find(|pair| pair[0] == id)
                .map(|pair| pair[1])
        };
        assert_eq!(
            result.policy.depth_enable,
            state(7),
            "native ZENABLE case {index}"
        );
        assert_eq!(
            result.policy.depth_write,
            state(14),
            "native ZWRITEENABLE case {index}"
        );
        assert_eq!(
            Some(u32::from(result.policy.perspective)),
            state(4),
            "native TEXTUREPERSPECTIVE case {index}"
        );
        assert_eq!(
            Some(u32::from(result.policy.fog.enabled())),
            state(28),
            "native FOGENABLE case {index}"
        );
        if let D3dTextureFog::Vertex { color } = result.policy.fog {
            assert_eq!(
                Some(u32::from_be_bytes([0, color[0], color[1], color[2]])),
                state(34),
                "native FOGCOLOR case {index}"
            );
        }
        let factors = match result.policy.blend {
            D3dTextureBlend::Copy => (2, 1),
            D3dTextureBlend::Additive => (2, 2),
            D3dTextureBlend::InverseSourceColor => (1, 4),
            D3dTextureBlend::SourceAlpha => (5, 6),
        };
        assert_eq!(
            (Some(factors.0), Some(factors.1)),
            (state(19), state(20)),
            "native blend factors case {index}"
        );
    }
}

#[test]
fn original_callback_tl_records_clipping_and_states_match() {
    // Literal output from callback 004A38F0 -> 004CE930 in the retail PE,
    // x87 FPCW027F. COM/display/upload/error boundaries are stubbed and the
    // selected descriptor already has an allocation; renderer+44 is zero.
    // Cases include signed32 UV/depth, unsigned near, normalization, every
    // blend priority, bypass and actual native clip 004BCCE0. This establishes
    // CPU submission behavior, not original GPU raster precision or live use.
    let cases: Vec<NativeCase> = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_textured_triangle_native.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 18);
    verify_native_cases(&cases);
}

#[test]
fn original_far_callback_corner_shades_uv_fog_and_clipping_match() {
    // check_far_textured_triangle_consumer.py --samples: supported retail
    // 004D0170, unmodified arithmetic and 004BCCE0, x87 027F. Eight bounded
    // receipts include signed UV/depth, direct diffuse fading, vertex fog,
    // allocation bounds, managed depth, native clipping and bypass fans.
    let cases: Vec<NativeCase> = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_far_textured_triangle_native.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 8);
    assert!(cases.iter().all(|case| case.far.is_some()));
    verify_native_cases(&cases);
}

#[test]
#[ignore = "requires INF_FAR_TEXTURED_TRIANGLE_ORACLE from check_far_textured_triangle_consumer.py --output"]
fn full_local_far_native_oracle_matches_rust_submission() {
    let path = std::env::var_os("INF_FAR_TEXTURED_TRIANGLE_ORACLE").unwrap();
    let cases: Vec<NativeCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 1024);
    assert!(cases.iter().all(|case| case.far.is_some()));
    verify_native_cases(&cases);
}

#[test]
#[ignore = "requires INF_TEXTURED_TRIANGLE_ORACLE pointing to the local 544-case native oracle JSON"]
fn full_local_native_oracle_matches_rust_submission() {
    let path =
        std::env::var("INF_TEXTURED_TRIANGLE_ORACLE").expect("explicit local oracle path required");
    let cases: Vec<NativeCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 544);
    verify_native_cases(&cases);
}

#[test]
fn de_shared_shade_uses_native_allocation_corners() {
    //004C3B60 reads allocation TL/TR/BR, independently of packet shade.
    let entry = sprite([0, 0], &[0]);
    let triangle = read_de_triangle(&de_packet(&entry, 0, 0xccbb_aa99))
        .unwrap()
        .unwrap();
    assert_eq!(triangle.shared_rgb, [0x99, 0xaa, 0xbb]);
    let prepared =
        prepare_de_triangle(&triangle, renderer(), 0, false, [0.1, 0.2, 0.8, 0.9]).unwrap();
    assert_eq!(
        prepared.vertices.map(|vertex| vertex.uv),
        [[0.1, 0.2], [0.8, 0.2], [0.8, 0.9]]
    );
    assert_eq!(triangle.mode, 0);
    assert_eq!(triangle.vertices[0].x, 32767.0);
    assert_eq!(triangle.vertices[0].depth, i32::MIN);
}

fn far_de_packet(entry: &SpriteEntry, mode: u32, shade: u32) -> PrimitiveCommand<'_> {
    let mut command = de_packet(entry, mode, shade);
    command.callback = PrimitiveCallback::SharedShadeTriangleFar;
    command.write_u32(0x24, 0x001f);
    for (index, fog) in [0, 127, 255].into_iter().enumerate() {
        command.write_byte(0x28 + index, fog);
    }
    command
}

#[test]
fn de_far_tail_requires_exact_inputs_and_ignores_padding_after_selector() {
    let entry = sprite([0, 0], &[0]);
    let mut command = far_de_packet(&entry, 0, 0xccbb_aa99);
    command.payload[0x23] = None;
    let triangle = read_de_triangle(&command).unwrap().unwrap();
    assert_eq!(triangle.shared_rgb, [0x99, 0xaa, 0xbb]);
    assert_eq!(
        triangle.shading,
        TexturedTriangleShading::Far {
            color: 0x1f,
            fog: [0, 127, 255]
        }
    );
    for offset in 0x24..0x2b {
        let mut missing = command.clone();
        missing.payload[offset] = None;
        assert!(matches!(
            read_de_triangle(&missing),
            Err(TexturedPixelError::Unknown { .. })
        ));
    }
    command.payload[0x2b..].fill(Some(0xff));
    let expected = prepare_de_triangle(&triangle, renderer(), 0, false, [0., 0., 1., 1.]).unwrap();
    assert_eq!(
        prepare_de_triangle(
            &read_de_triangle(&command).unwrap().unwrap(),
            renderer(),
            0,
            false,
            [0., 0., 1., 1.]
        )
        .unwrap(),
        expected
    );
    for (blocks, length) in [(4, 64), (3, 47), (3, 49)] {
        let mut invalid = command.clone();
        invalid.payload_blocks = blocks;
        invalid.payload.resize(length, None);
        assert!(matches!(
            read_de_triangle(&invalid),
            Err(TexturedPixelError::DePacket)
        ));
    }
    let entry = sprite([255, 255], &[]);
    for mode in [0, 1] {
        let mut incomplete = PrimitiveCommand::new(PrimitiveCallback::SharedShadeTriangleFar, 0, 3);
        incomplete.write_u32(0x18, mode);
        incomplete.write_texture(0x1c, Some(resource(&entry)));
        assert!(read_de_triangle(&incomplete).unwrap().is_none());
    }
}

#[test]
fn de_far_descriptor_branches_preserve_fog_alpha_depth_and_secondary_boundary() {
    let entry = sprite([0, 0], &[0]);
    let mut triangle = read_de_triangle(&far_de_packet(&entry, 0x100, 0x0080_8080))
        .unwrap()
        .unwrap();
    let renderer = D3dFlatState {
        material_shifts: [6, 1, 4],
        ..renderer()
    };
    for (flags, global, blend, alpha, write) in [
        (0, 0, D3dTextureBlend::Copy, 255, 1),
        (0, 8, D3dTextureBlend::SourceAlpha, 255, 1),
        (1, 0, D3dTextureBlend::Copy, 255, 0),
        (8, 0, D3dTextureBlend::SourceAlpha, 128, 0),
        (0x10, 0, D3dTextureBlend::Additive, 255, 0),
        (0x20, 0, D3dTextureBlend::InverseSourceColor, 255, 0),
    ] {
        triangle.flags = flags;
        let result = prepare_de_triangle(&triangle, renderer, global, false, [0.; 4]).unwrap();
        assert_eq!(result.policy.blend, blend);
        assert_eq!(result.policy.depth_enable, Some(1));
        assert_eq!(result.policy.depth_write, Some(write));
        assert!(!result.policy.perspective);
        assert!(result
            .vertices
            .iter()
            .all(|vertex| vertex.diffuse[3] == alpha));
        if flags & 0x30 == 0 {
            assert_eq!(
                result.policy.fog,
                D3dTextureFog::Vertex { color: [0, 0, 255] }
            );
            assert_eq!(
                result.vertices.map(|vertex| vertex.specular),
                [0xff00_0000, 0x8000_0000, 0]
            );
            assert!(result
                .vertices
                .iter()
                .all(|vertex| vertex.diffuse[..3] == [255; 3]));
        } else {
            assert_eq!(result.policy.fog, D3dTextureFog::Disabled);
            assert_eq!(
                result.vertices.map(|vertex| vertex.diffuse),
                [[255; 4], [129, 129, 129, 255], [0, 0, 0, 255]]
            );
        }
    }
    triangle.flags = 0x200;
    assert!(matches!(
        prepare_de_triangle(&triangle, renderer, 0, true, [0.; 4]),
        Err(TexturedPixelError::SecondaryPass)
    ));
    let unmanaged = prepare_de_triangle(
        &triangle,
        D3dFlatState {
            manages_depth: false,
            ..renderer
        },
        0,
        false,
        [0.; 4],
    )
    .unwrap();
    assert_eq!(
        (unmanaged.policy.depth_enable, unmanaged.policy.depth_write),
        (None, None)
    );
}

#[test]
fn de_matches_native_near_and_far_primary_tl_bit_pins() {
    // Synthetic cases0/16 from the tracked unmodified-machine-code checker,
    // x87CW027F. These are arithmetic checks, not sampled briefing behavior.
    let cases = [
        (
            false,
            0,
            32,
            8,
            2277003346,
            3859801898,
            [u32::MAX, 0, u32::MAX],
            [-12345, i32::MIN, 16777217],
            [1051161142, 1059417271, 1056152152, 1047780041],
            [
                [
                    3270901760, 1131020288, 1065396705, 3098140912, 4285689855, 4278190080,
                    1051161142, 1059417271,
                ],
                [
                    1191181824, 3338665984, 1065353216, 2952790016, 4285689855, 4278190080,
                    1056152152, 1059417271,
                ],
                [
                    1065353216, 1073741824, 1065353152, 864026623, 4285689855, 4278190080,
                    1056152152, 1047780041,
                ],
            ],
        ),
        (
            true,
            0,
            16416,
            8,
            1651101111,
            2400899860,
            [6, 1, 4],
            [-12345, i32::MIN, 16777217],
            [1049874121, 1032189529, 1048111437, 1057416643],
            [
                [
                    3270901760, 1131020288, 1065396705, 3098140912, 4293197699, 4278190080,
                    1049874121, 1032189529,
                ],
                [
                    1191181824, 3338665984, 1065353216, 2952790016, 4293197699, 2147483648,
                    1048111437, 1032189529,
                ],
                [
                    1065353216, 1073741824, 1065353152, 864026623, 4293197699, 0, 1048111437,
                    1057416643,
                ],
            ],
        ),
        (
            true,
            16,
            32,
            8,
            3934316946,
            1686532132,
            [10, 5, 1],
            [i32::MIN, -12345, 16777217],
            [1060000029, 1058059797, 1055258125, 1054961500],
            [
                [
                    3270901760, 1131020288, 1065353216, 2952790016, 4288741260, 4278190080,
                    1060000029, 1058059797,
                ],
                [
                    1191181824, 3338665984, 1065396705, 3098140912, 4287884161, 4278190080,
                    1055258125, 1058059797,
                ],
                [
                    1065353216, 1073741824, 1065353152, 864026623, 4278190080, 4278190080,
                    1055258125, 1054961500,
                ],
            ],
        ),
    ];
    for (far, flags, mode, global, color, far_color, shifts, depths, bounds, expected) in cases {
        let entry = sprite([0, 0], &[flags]);
        let mut command = de_packet(&entry, mode, color);
        if far {
            command.callback = PrimitiveCallback::SharedShadeTriangleFar;
            command.write_u32(0x24, far_color);
            for (index, value) in [0, 127, 255].into_iter().enumerate() {
                command.write_byte(0x28 + index, value);
            }
        }
        for (index, [x, y]) in [[-123i16, 234i16], [32767, -32768], [1, 2]]
            .into_iter()
            .enumerate()
        {
            command.write_u32(index * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
            command.write_u32(index * 8 + 4, depths[index] as u32);
        }
        let triangle = read_de_triangle(&command).unwrap().unwrap();
        let prepared = prepare_de_triangle(
            &triangle,
            D3dFlatState {
                material_shifts: shifts,
                near_depth: 64,
                manages_depth: false,
                ..renderer()
            },
            global,
            false,
            bounds.map(f32::from_bits),
        )
        .unwrap();
        assert_eq!(
            prepared.vertices.map(vertex_bits),
            expected,
            "far={far} flags={flags:#x}"
        );
    }
}

#[test]
fn de_packet_shape_texture_and_ff_are_checked() {
    let entry = sprite([0, 0], &[0]);
    let mut command = de_packet(&entry, 0, 0);
    command.callback = PrimitiveCallback::TexturedTriangleNear;
    assert!(matches!(
        read_de_triangle(&command),
        Err(TexturedPixelError::DePacket)
    ));
    let mut command = de_packet(&entry, 0, 0);
    command.texture = None;
    assert!(matches!(
        read_de_triangle(&command),
        Err(TexturedPixelError::DeTexture)
    ));
    let entry = sprite([255, 0], &[0]);
    assert!(read_de_triangle(&de_packet(&entry, 0, 0))
        .unwrap()
        .is_none());
}

#[path = "d3d_textured_triangle_prefix_tests.rs"]
mod prefix;
