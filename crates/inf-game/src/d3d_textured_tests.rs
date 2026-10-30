use super::*;
use crate::{
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
                // Packet/TL preparation must not read texel or palette bytes.
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
    let mut command = PrimitiveCommand::new(PrimitiveCallback::TexturedQuadNear, 0, 3);
    command.write_u32(0x20, mode);
    command.write_texture(0x24, Some(resource(entry)));
    command
}

fn write_vertices(command: &mut PrimitiveCommand<'_>) {
    for (i, (xy, depth)) in [
        ([-123i16, 234i16], 64i32),
        ([32767, -32768], i32::MIN),
        ([1, 2], i32::MAX),
        ([-456, -100], -12345),
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
        material_shifts: [u32::MAX; 3], // This callback never reads RGB shifts.
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn ff_selector_suppresses_before_vertices_and_only_the_mode_low_bit_selects() {
    // 004D541D..004D5433 reads mode/pointer/selectors before any XY/depth.
    let entry = sprite([255, 0], &[0x38]);
    for mode in [0, 0x20, 0x100, 0x4000, 0xffff_fffe] {
        assert!(read_textured_quad(&packet(&entry, mode)).unwrap().is_none());
    }
    assert!(matches!(
        read_textured_quad(&packet(&entry, 1)),
        Err(TexturedPixelError::Unknown { offset: 0, .. })
    ));
    let entry = sprite([0, 255], &[0]);
    for mode in [1, 0x21, 0x101, 0x4001, u32::MAX] {
        assert!(read_textured_quad(&packet(&entry, mode)).unwrap().is_none());
    }
    let absent = sprite([255, 255], &[]);
    assert!(read_textured_quad(&packet(&absent, 0)).unwrap().is_none());
    let invalid = sprite([0, 3], &[0]);
    assert!(matches!(
        read_textured_quad(&packet(&invalid, 1)),
        Err(TexturedPixelError::Selector(3))
    ));
}

#[test]
fn packet_keeps_signed_words_and_depth_without_consuming_shared_shade_or_padding() {
    let entry = sprite([1, 0], &[8, 0x30]);
    let mut command = packet(&entry, 0x4120);
    write_vertices(&mut command);
    assert!(command.payload[0x28..].iter().all(Option::is_none));
    let quad = read_textured_quad(&command).unwrap().unwrap();
    assert_eq!(quad.flags, 0x30);
    assert_eq!(quad.mode, 0x4120);
    assert_eq!(
        quad.vertices.map(|v| [v.x, v.y]),
        [[-123., 234.], [32767., -32768.], [1., 2.], [-456., -100.]]
    );
    assert_eq!(
        quad.vertices.map(|v| v.depth),
        [64, i32::MIN, i32::MAX, -12345]
    );
    let original = prepare_textured_quad(&quad, renderer(), 8, false, [0., 0., 1., 1.]).unwrap();
    command.payload[0x28..].fill(Some(0x13));
    let filled = read_textured_quad(&command).unwrap().unwrap();
    assert_eq!(
        prepare_textured_quad(&filled, renderer(), 8, false, [0., 0., 1., 1.]).unwrap(),
        original
    );
    assert_eq!(original.vertices.map(|v| v.diffuse), [[255; 4]; 4]);
}

#[test]
fn packet_requires_exact_callback_extent_and_borrowed_h3_relocation() {
    let entry = sprite([0, 0], &[0]);
    let mut command = packet(&entry, 0x20);
    write_vertices(&mut command);
    for byte in 0..0x24 {
        let mut missing = command.clone();
        missing.payload[byte] = None;
        assert!(matches!(
            read_textured_quad(&missing),
            Err(TexturedPixelError::Unknown { .. })
        ));
    }
    for callback in [
        PrimitiveCallback::FlatQuadNear,
        PrimitiveCallback::TexturedUvQuadNear,
        PrimitiveCallback::EffectQuadNear,
    ] {
        let mut wrong = command.clone();
        wrong.callback = callback;
        assert!(matches!(
            read_textured_quad(&wrong),
            Err(TexturedPixelError::Packet)
        ));
    }
    for length in [47, 49] {
        let mut wrong = command.clone();
        wrong.payload.resize(length, None);
        assert!(matches!(
            read_textured_quad(&wrong),
            Err(TexturedPixelError::Packet)
        ));
    }
    let mut wrong = command.clone();
    wrong.payload_blocks = 2;
    assert!(matches!(
        read_textured_quad(&wrong),
        Err(TexturedPixelError::Packet)
    ));
    for texture in [
        None,
        Some((0x28, resource(&entry))),
        Some((
            0x24,
            RenderResource {
                data: RenderResourceData::Style(0),
                ..resource(&entry)
            },
        )),
    ] {
        let mut wrong = command.clone();
        wrong.texture = texture;
        assert!(matches!(
            read_textured_quad(&wrong),
            Err(TexturedPixelError::Texture)
        ));
    }
}

#[test]
fn flags_choose_native_blend_precedence_and_depth_ownership_independently() {
    let entry = sprite([0, 0], &[0]);
    let mut command = packet(&entry, 0x20);
    write_vertices(&mut command);
    let mut quad = read_textured_quad(&command).unwrap().unwrap();
    // Native 004D54E2..004D5599: 10 before 20 before 08 before global08.
    for (flags, global, blend, alpha, depth_write) in [
        (0, 0, D3dTextureBlend::Copy, 255, 7),
        (0, 8, D3dTextureBlend::SourceAlpha, 255, 7),
        (1, 0, D3dTextureBlend::Copy, 255, 0),
        (8, 8, D3dTextureBlend::SourceAlpha, 128, 0),
        (0x28, 8, D3dTextureBlend::InverseSourceColor, 255, 0),
        (0x38, 8, D3dTextureBlend::Additive, 255, 0),
        (0x200, 0, D3dTextureBlend::Copy, 255, 7),
    ] {
        quad.flags = flags;
        let renderer = D3dFlatState {
            depth_enable: 3,
            depth_write: 7,
            ..renderer()
        };
        let result = prepare_textured_quad(&quad, renderer, global, false, [0.; 4]).unwrap();
        assert_eq!(
            result.policy,
            D3dTexturePolicy {
                depth_enable: Some(3),
                depth_write: Some(depth_write),
                blend,
                perspective: true,
                fog: D3dTextureFog::Disabled,
            }
        );
        assert_eq!(result.vertices.map(|v| v.diffuse[3]), [alpha; 4]);
        let inherited = prepare_textured_quad(
            &quad,
            D3dFlatState {
                manages_depth: false,
                ..renderer
            },
            global,
            false,
            [0.; 4],
        )
        .unwrap();
        assert_eq!(inherited.policy.depth_enable, None);
        assert_eq!(inherited.policy.depth_write, None);
    }
    for mode in [0, 0x100, 0x4000, 0x4100] {
        quad.mode = mode;
        quad.flags = 0;
        let result = prepare_textured_quad(&quad, renderer(), 0, true, [0.; 4]).unwrap();
        assert_eq!(result.policy.perspective, mode & 0x100 == 0);
        assert_eq!(
            result.triangles,
            if mode & 0x4000 == 0 {
                [[1, 2, 3], [1, 3, 0]]
            } else {
                [[0, 1, 2], [0, 2, 3]]
            }
        );
    }
    quad.flags = 0x200;
    assert_eq!(
        prepare_textured_quad(&quad, renderer(), 0, true, [0.; 4]),
        Err(TexturedPixelError::SecondaryPass)
    );
}

fn far_packet(entry: &SpriteEntry, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = packet(entry, mode);
    command.callback = PrimitiveCallback::TexturedQuadFar;
    write_vertices(&mut command);
    command.write_u32(0x28, 0x1f);
    for (index, fog) in [0, 127, 128, 255].into_iter().enumerate() {
        command.write_byte(0x2c + index, fog);
    }
    command
}

#[test]
fn far_quad_requires_color_and_four_fog_bytes_after_selector() {
    let entry = sprite([0, 0], &[0]);
    let command = far_packet(&entry, 0x4100);
    let quad = read_textured_quad(&command).unwrap().unwrap();
    assert_eq!(
        quad.shading,
        TexturedQuadShading::Far {
            color: 0x1f,
            fog: [0, 127, 128, 255],
        }
    );
    assert_eq!(
        quad.vertices.map(|vertex| vertex.depth),
        [64, i32::MIN, i32::MAX, -12345]
    );
    for offset in 0x28..0x30 {
        let mut missing = command.clone();
        missing.payload[offset] = None;
        assert!(matches!(read_textured_quad(&missing),
            Err(TexturedPixelError::Unknown { offset: found, .. }) if found == offset || found == 0x28));
    }
    for (blocks, length) in [(4, 64), (3, 47), (3, 49)] {
        let mut wrong = command.clone();
        wrong.payload_blocks = blocks;
        wrong.payload.resize(length, None);
        assert!(matches!(
            read_textured_quad(&wrong),
            Err(TexturedPixelError::Packet)
        ));
    }
    let absent = sprite([255, 255], &[]);
    for mode in [0, 1, 0x20, 0x101, 0x4100, u32::MAX] {
        let mut incomplete = packet(&absent, mode);
        incomplete.callback = PrimitiveCallback::TexturedQuadFar;
        assert!(read_textured_quad(&incomplete).unwrap().is_none());
    }
}

#[test]
fn far_quad_keeps_literal_rgb_fade_distinct_from_vertex_fog() {
    let entry = sprite([0, 0], &[0]);
    let mut quad = read_textured_quad(&far_packet(&entry, 0x100))
        .unwrap()
        .unwrap();
    let state = D3dFlatState {
        material_shifts: [6, 1, 4],
        ..renderer()
    };
    for (flags, global, blend, alpha, depth_write) in [
        (0, 0, D3dTextureBlend::Copy, 255, 1),
        (0, 8, D3dTextureBlend::SourceAlpha, 255, 1),
        (1, 0, D3dTextureBlend::Copy, 255, 0),
        (8, 8, D3dTextureBlend::SourceAlpha, 128, 0),
        (0x28, 8, D3dTextureBlend::InverseSourceColor, 255, 0),
        (0x38, 8, D3dTextureBlend::Additive, 255, 0),
    ] {
        quad.flags = flags;
        let result =
            prepare_textured_quad(&quad, state, global, false, [0.1, 0.2, 0.8, 0.9]).unwrap();
        assert_eq!(result.policy.depth_enable, Some(1));
        assert_eq!(result.policy.depth_write, Some(depth_write));
        assert_eq!(result.policy.blend, blend);
        assert!(!result.policy.perspective);
        assert_eq!(
            result.vertices.map(|vertex| vertex.uv),
            [[0.1, 0.2], [0.8, 0.2], [0.8, 0.9], [0.1, 0.9]]
        );
        assert_eq!(result.vertices.map(|vertex| vertex.diffuse[3]), [alpha; 4]);
        if flags & 0x30 != 0 {
            assert_eq!(result.policy.fog, D3dTextureFog::Disabled);
            assert_eq!(
                result.vertices.map(|vertex| vertex.diffuse),
                [
                    [255, 255, 255, 255],
                    [128, 128, 128, 255],
                    [127, 127, 127, 255],
                    [0, 0, 0, 255],
                ]
            );
            assert_eq!(
                result.vertices.map(|vertex| vertex.specular),
                [0xff00_0000; 4]
            );
        } else {
            assert_eq!(
                result.policy.fog,
                D3dTextureFog::Vertex { color: [0, 0, 255] }
            );
            assert_eq!(
                result.vertices.map(|vertex| vertex.diffuse),
                [[255, 255, 255, alpha]; 4]
            );
            assert_eq!(
                result.vertices.map(|vertex| vertex.specular),
                [0xff00_0000, 0x8000_0000, 0x7f00_0000, 0]
            );
        }
        let inherited = prepare_textured_quad(
            &quad,
            D3dFlatState {
                manages_depth: false,
                ..state
            },
            global,
            false,
            [0.; 4],
        )
        .unwrap();
        assert_eq!(inherited.policy.depth_enable, None);
        assert_eq!(inherited.policy.depth_write, None);
    }
    quad.flags = 0x200;
    assert!(prepare_textured_quad(&quad, state, 0, false, [0.; 4]).is_ok());
    assert_eq!(
        prepare_textured_quad(&quad, state, 0, true, [0.; 4]),
        Err(TexturedPixelError::SecondaryPass)
    );
    quad.flags = 0;
    for mode in [0, 0x20, 0x100, 0x4000, 0x4101] {
        quad.mode = mode;
        let result = prepare_textured_quad(&quad, state, 0, true, [0.; 4]).unwrap();
        assert_eq!(result.mode, mode);
        assert_eq!(result.policy.perspective, mode & 0x100 == 0);
        assert_eq!(
            result.triangles,
            if mode & 0x4000 == 0 {
                [[1, 2, 3], [1, 3, 0]]
            } else {
                [[0, 1, 2], [0, 2, 3]]
            }
        );
    }
}

#[test]
fn far_quad_tl_words_match_unmodified_native_primary_consumer() {
    // Cases0/16 from tracked check_ordinary_far_quad_consumer.py, original
    // 004D6570 under x87CW027F. These are synthetic inputs/native TL outputs;
    // COM/upload/display are intercepted, renderer+44 is zero, no GPU claim.
    let entry = sprite([0, 0], &[0]);
    for (flags, mode, depths, bounds, near, color, shifts, expected) in [
        (
            0,
            16673,
            [2147483647, 64, 16777217, -12345],
            [
                0.5555418133735657,
                0.7522083520889282,
                0.8827810883522034,
                0.08326584100723267,
            ],
            64,
            2182341133,
            [38, 33, 36],
            [
                [
                    3270901760, 1131020288, 1065353216, 805306368, 4294967295, 4278190080,
                    1057896445, 1061195962,
                ],
                [
                    1191181824, 3338665984, 0, 1015021568, 4294967295, 2147483648, 1063386609,
                    1061195962,
                ],
                [
                    1065353216, 1073741824, 1065353152, 864026623, 4294967295, 2130706432,
                    1063386609, 1034585928,
                ],
                [
                    3286499328, 3267887104, 1065396705, 3098140912, 4294967295, 0, 1057896445,
                    1034585928,
                ],
            ],
        ),
        (
            16,
            288,
            [16777217, 2147483647, -2147483648, 12345],
            [
                0.22547033429145813,
                0.5456882119178772,
                0.27300384640693665,
                0.18256793916225433,
            ],
            u32::MAX,
            2154163899,
            [u32::MAX, 0, u32::MAX],
            [
                [
                    3270901760, 1131020288, 3279880191, 864026623, 4294967295, 4278190080,
                    1046929842, 1057731129,
                ],
                [
                    1191181824, 3338665984, 3212836864, 805306368, 4286611584, 4278190080,
                    1049347881, 1057731129,
                ],
                [
                    1065353216, 1073741824, 1077936128, 2952790016, 4286545791, 4278190080,
                    1049347881, 1044050711,
                ],
                [
                    3286499328, 3267887104, 3366576336, 950657264, 4278190080, 4278190080,
                    1046929842, 1044050711,
                ],
            ],
        ),
    ] {
        let mut command = far_packet(&entry, mode);
        command.write_u32(0x28, color);
        for (index, depth) in depths.into_iter().enumerate() {
            command.write_u32(index * 8 + 4, depth as u32);
        }
        let mut quad = read_textured_quad(&command).unwrap().unwrap();
        quad.flags = flags;
        let result = prepare_textured_quad(
            &quad,
            D3dFlatState {
                near_depth: near,
                material_shifts: shifts,
                manages_depth: false,
                ..renderer()
            },
            0,
            false,
            bounds,
        )
        .unwrap();
        assert_eq!(
            result.vertices.map(vertex_bits),
            expected,
            "native004D6570 flags{flags:#x}"
        );
        if flags == 0 {
            assert_eq!(
                result.policy.fog,
                D3dTextureFog::Vertex {
                    color: [198, 132, 107]
                }
            );
        }
    }
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
    depths: [i32; 4],
    coords: [[i16; 2]; 4],
    uv_bounds: [f32; 4],
    vertices: [[u32; 8]; 4],
    draws: [[[u32; 8]; 3]; 2],
    states: Vec<[u32; 2]>,
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
    let entry = sprite([0, 0], &[0]);
    for (index, case) in cases.iter().enumerate() {
        let quad = TexturedQuad {
            vertices: std::array::from_fn(|i| FlatVertex {
                x: f32::from(case.coords[i][0]),
                y: f32::from(case.coords[i][1]),
                depth: case.depths[i],
            }),
            mode: case.mode,
            resource: resource(&entry),
            flags: case.flags,
            shading: TexturedQuadShading::Near,
        };
        let result = prepare_textured_quad(
            &quad,
            D3dFlatState {
                near_depth: case.near,
                manages_depth: case.manages,
                depth_enable: case.depth_enable,
                depth_write: case.depth_write,
                ..renderer()
            },
            case.policy,
            false,
            case.uv_bounds,
        )
        .unwrap();
        let vertices = result.vertices.map(vertex_bits);
        assert_eq!(vertices, case.vertices, "native TL records case {index}");
        assert_eq!(
            result
                .triangles
                .map(|triangle| triangle.map(|i| vertices[i])),
            case.draws,
            "native ordered draw records case {index}"
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
fn native_tl_records_draw_order_and_render_state_match_original_executable() {
    // Selected cases 0,1,3,4,5,7,17,18,22,30 from 512 executions of retail
    // 004D5410, x87 FPCW=027F. Full TL DWORDs and SetRenderState receipts
    // are literal native outputs, not expectations recomputed in Rust.
    // External COM/display/upload/error functions were stubbed, descriptors
    // supplied an existing allocation, mode20 bypassed viewport clipping,
    // and renderer+44 was zero. This does not establish GPU pixel precision.
    let cases: Vec<NativeCase> =
        serde_json::from_str(include_str!("../tests/fixtures/d3d_textured_native.json")).unwrap();
    assert_eq!(cases.len(), 10);
    verify_native_cases(&cases);
}

#[test]
#[ignore = "requires INF_TEXTURED_QUAD_ORACLE pointing to the local 512-case native oracle JSON"]
fn full_local_native_oracle_matches_rust_submission() {
    let path =
        std::env::var("INF_TEXTURED_QUAD_ORACLE").expect("explicit local oracle path required");
    let cases: Vec<NativeCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 512);
    verify_native_cases(&cases);
}
