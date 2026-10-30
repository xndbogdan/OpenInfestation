use super::*;
use crate::{
    d3d_viewport::{clip_polygon, D3dViewportState},
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, MaterialValue, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry,
    SpriteFlags, SpriteModeSelectors, SpriteVariant,
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
                // Packet conversion does not read palette/texels.
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

fn packet(entry: &SpriteEntry, count: usize, mode: u32) -> PrimitiveCommand<'_> {
    let callback = match count {
        3 => PrimitiveCallback::TexturedUvTriangleNear,
        4 => PrimitiveCallback::TexturedUvQuadNear,
        _ => panic!("fixture shape"),
    };
    let mut command = PrimitiveCommand::new(callback, 0, count + 1);
    command.write_u32(count * 8, mode);
    command.write_texture(count * 8 + 4, Some(resource(entry)));
    command
}

fn complete_packet(entry: &SpriteEntry, count: usize, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = packet(entry, count, mode);
    for i in 0..count {
        command.write_u32(i * 8, 0x8000_7fff);
        command.write_u32(i * 8 + 4, i32::MIN as u32 + i as u32);
        command.write_u32(count * 8 + 8 + i * 8, u32::MAX);
        command.write_u32(count * 8 + 12 + i * 8, 65535);
    }
    command
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [u32::MAX; 3], // These consumers never use material shifts.
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn ff_selector_suppresses_before_geometry_uv_descriptor_and_states() {
    for count in [3, 4] {
        for (selectors, modes) in [
            ([255, 0], [0, 0x20, 0x4100, u32::MAX - 1]),
            ([0, 255], [1, 0x21, 0x4101, u32::MAX]),
        ] {
            let entry = sprite(selectors, &[0]);
            for mode in modes {
                assert!(read_textured_uv(&packet(&entry, count, mode))
                    .unwrap()
                    .is_none());
            }
        }
        let entry = sprite([255, 255], &[]);
        assert!(read_textured_uv(&packet(&entry, count, 0))
            .unwrap()
            .is_none());
        let entry = sprite([0, 1], &[0]);
        assert!(matches!(
            read_textured_uv(&packet(&entry, count, 1)),
            Err(TexturedPixelError::Selector(1))
        ));
        assert!(matches!(
            read_textured_uv(&packet(&entry, count, 0)),
            Err(TexturedPixelError::Unknown { offset: 0, .. })
        ));
    }
}

#[test]
fn callback_extent_relocation_and_every_consumed_byte_are_required() {
    let entry = sprite([0, 1], &[0x08, 0x10]);
    for count in [3, 4] {
        let complete = complete_packet(&entry, count, 1);
        let primitive = read_textured_uv(&complete).unwrap().unwrap();
        assert_eq!(primitive.flags, 0x10);
        let (vertices, uv) = match primitive.geometry {
            TexturedUvGeometry::Triangle { vertices, uv } => (vertices.to_vec(), uv.to_vec()),
            TexturedUvGeometry::Quad { vertices, uv } => (vertices.to_vec(), uv.to_vec()),
        };
        assert_eq!(vertices.len(), count);
        for i in 0..count {
            assert_eq!([vertices[i].x, vertices[i].y], [32767., -32768.]);
            assert_eq!(vertices[i].depth, i32::MIN + i as i32);
            assert_eq!(uv[i], [-1, 65535]);
        }
        let used_end = count * 16 + 8;
        assert!(complete.payload[used_end..].iter().all(Option::is_none));
        for byte in (0..count * 8 + 4).chain(count * 8 + 8..used_end) {
            let mut missing = complete.clone();
            missing.payload[byte] = None;
            assert!(matches!(
                read_textured_uv(&missing),
                Err(TexturedPixelError::Unknown { .. })
            ));
        }
        for callback in [
            PrimitiveCallback::SharedShadeUvTriangleNear,
            PrimitiveCallback::SharedShadeUvQuadNear,
            PrimitiveCallback::TexturedTriangleNear,
            PrimitiveCallback::TexturedQuadNear,
        ] {
            let mut wrong = complete.clone();
            wrong.callback = callback;
            assert!(matches!(
                read_textured_uv(&wrong),
                Err(TexturedPixelError::UvPacket)
            ));
        }
        let mut wrong = complete.clone();
        wrong.payload_blocks += 1;
        assert!(matches!(
            read_textured_uv(&wrong),
            Err(TexturedPixelError::UvPacket)
        ));
        wrong = complete.clone();
        wrong.payload.pop();
        assert!(matches!(
            read_textured_uv(&wrong),
            Err(TexturedPixelError::UvPacket)
        ));
        wrong = complete.clone();
        wrong.texture = None;
        assert!(
            matches!(read_textured_uv(&wrong), Err(TexturedPixelError::UvTexture(offset)) if offset == count * 8 + 4)
        );
        wrong.texture = Some((0, resource(&entry)));
        assert!(matches!(
            read_textured_uv(&wrong),
            Err(TexturedPixelError::UvTexture(_))
        ));
        let mut material = resource(&entry);
        material.data = RenderResourceData::Material(MaterialValue::from_raw(0));
        wrong.texture = Some((count * 8 + 4, material));
        assert!(matches!(
            read_textured_uv(&wrong),
            Err(TexturedPixelError::UvTexture(_))
        ));
    }
}

#[test]
fn secondary_texture_is_required_only_after_the_descriptor_requests_it() {
    let entry = sprite([0, 0], &[0]);
    for count in [3, 4] {
        let mut primitive = read_textured_uv(&complete_packet(&entry, count, 0))
            .unwrap()
            .unwrap();
        for flags in [0, 1, 8, 0x10, 0x20, 0x38, 0x200, 0x238] {
            primitive.flags = flags;
            let baseline = prepare_textured_uv(&primitive, renderer(), 8, false, [0., 0., 1., 1.]);
            assert_eq!(baseline.continuation, TexturedUvContinuation::Complete);
            let result = prepare_textured_uv(&primitive, renderer(), 8, true, [0., 0., 1., 1.]);
            assert_eq!(result.primary, baseline.primary);
            assert_eq!(
                result.continuation,
                if flags & 0x200 != 0 {
                    TexturedUvContinuation::SecondaryPassRequired
                } else {
                    TexturedUvContinuation::Complete
                }
            );
        }
    }
}

#[derive(Deserialize)]
struct NativeCase {
    count: usize,
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    depths: Vec<i32>,
    coords: Vec<[i16; 2]>,
    uv: Vec<[i32; 2]>,
    uv_bounds: [f32; 4],
    viewport: [i16; 4],
    vertices: Vec<[u32; 8]>,
    draws: Vec<Vec<[u32; 8]>>,
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

fn verify_submission<const V: usize, const T: usize>(
    result: D3dTexturedSubmission<V, T>,
    case: &NativeCase,
    index: usize,
) {
    assert_eq!(
        result.vertices.map(vertex_bits).to_vec(),
        case.vertices,
        "native pre-clip TL case {index}"
    );
    let mut draws = Vec::new();
    for indices in result.triangles {
        let triangle = indices.map(|i| result.vertices[i]);
        let polygon = if result.mode & 0x20 != 0 {
            triangle.to_vec()
        } else {
            clip_polygon(
                &triangle,
                D3dViewportState {
                    bounds: case.viewport,
                    near_depth: result.near_depth,
                    mode: result.mode,
                },
            )
            .unwrap()
        };
        if polygon.len() >= 3 {
            draws.push(polygon.into_iter().map(vertex_bits).collect::<Vec<_>>());
        }
    }
    assert_eq!(
        draws, case.draws,
        "native ordered fans and selected diagonal case {index}"
    );
    let state = |id| {
        case.states
            .iter()
            .rev()
            .find(|pair| pair[0] == id)
            .map(|pair| pair[1])
    };
    assert_eq!(result.policy.depth_enable, state(7), "ZENABLE case {index}");
    assert_eq!(
        result.policy.depth_write,
        state(14),
        "ZWRITEENABLE case {index}"
    );
    assert_eq!(
        Some(u32::from(result.policy.perspective)),
        state(4),
        "TEXTUREPERSPECTIVE case {index}"
    );
    assert_eq!(
        Some(u32::from(result.policy.fog.enabled())),
        state(28),
        "FOGENABLE case {index}"
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
        "blend case {index}"
    );
}

fn verify_native_cases(cases: &[NativeCase]) {
    for (index, case) in cases.iter().enumerate() {
        let entry = sprite([0, 0], &[case.flags]);
        let mut command = packet(&entry, case.count, case.mode);
        for i in 0..case.count {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_u32(case.count * 8 + 8 + i * 8, case.uv[i][0] as u32);
            command.write_u32(case.count * 8 + 12 + i * 8, case.uv[i][1] as u32);
        }
        let primitive = read_textured_uv(&command).unwrap().unwrap();
        let result = prepare_textured_uv(
            &primitive,
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
        );
        match result.primary {
            D3dTexturedUvSubmission::Triangle(result) => verify_submission(result, case, index),
            D3dTexturedUvSubmission::Quad(result) => verify_submission(result, case, index),
        }
    }
}

#[test]
fn original_uv_callbacks_match_tl_bits_states_clipping_and_diagonals() {
    // Literal retail callback004A3990→004C9A00 / 004A3A20→004DFC80 outputs,
    // FPCW027F. External COM/display/upload/error boundaries are intercepted;
    // allocation bounds are explicit and renderer+44 is zero. Actual native
    // clipping executes. This proves consumer arithmetic, not GPU precision
    // or execution of these cases in an observed retail scene.
    let cases: Vec<NativeCase> = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_textured_uv_native.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 24);
    verify_native_cases(&cases);
}

#[test]
#[ignore = "requires INF_TEXTURED_UV_ORACLE pointing to the local 2112-case native oracle JSON"]
fn full_local_native_uv_oracle_matches_rust_submission() {
    let path =
        std::env::var("INF_TEXTURED_UV_ORACLE").expect("explicit local oracle path required");
    let cases: Vec<NativeCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 2112);
    verify_native_cases(&cases);
}

#[path = "d3d_textured_uv_prefix_tests.rs"]
mod prefix;
