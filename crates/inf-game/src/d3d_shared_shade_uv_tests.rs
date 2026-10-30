use super::*;
use crate::{
    d3d_texture::NativeTextureUploadState,
    d3d_viewport::{clip_polygon, D3dViewportState},
    draw_queue::{DrawInvocation, DrawQueue, DrawQueueError, DrawSubmission},
    pixel_backend::{flush_pixels, PixelBackend},
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
    software_frame::{
        D3dRasterState, D3dTextureSampling, SoftwareFrame, SoftwarePixelBackend,
        SoftwarePixelError, SoftwareTextureState,
    },
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

const CALLBACKS: [PrimitiveCallback; 4] = [
    PrimitiveCallback::SharedShadeUvTriangleNear,
    PrimitiveCallback::SharedShadeUvTriangleFar,
    PrimitiveCallback::SharedShadeUvQuadNear,
    PrimitiveCallback::SharedShadeUvQuadFar,
];

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
            bytes: &[255, 127],
        },
        data: RenderResourceData::Sprite(entry),
    }
}

fn shape(callback: PrimitiveCallback) -> (usize, bool) {
    match callback {
        PrimitiveCallback::SharedShadeUvTriangleNear => (3, false),
        PrimitiveCallback::SharedShadeUvTriangleFar => (3, true),
        PrimitiveCallback::SharedShadeUvQuadNear => (4, false),
        PrimitiveCallback::SharedShadeUvQuadFar => (4, true),
        _ => panic!("explicit fixture shape"),
    }
}

fn packet(entry: &SpriteEntry, callback: PrimitiveCallback, mode: u32) -> PrimitiveCommand<'_> {
    let (count, far) = shape(callback);
    let mut command = PrimitiveCommand::new(callback, 20, count + 1 + usize::from(far));
    command.write_u32(count * 8, mode);
    command.write_texture(count * 8 + 4, Some(resource(entry)));
    command
}

fn complete(entry: &SpriteEntry, callback: PrimitiveCallback, mode: u32) -> PrimitiveCommand<'_> {
    let (count, far) = shape(callback);
    let mut command = packet(entry, callback, mode);
    for (i, xy) in [[0i16, 0i16], [6, 0], [6, 6], [0, 6]]
        .into_iter()
        .take(count)
        .enumerate()
    {
        command.write_u32(
            i * 8,
            u32::from(xy[0] as u16) | (u32::from(xy[1] as u16) << 16),
        );
        command.write_u32(i * 8 + 4, 20);
        command.write_u32(count * 8 + 12 + i * 8, 0);
        command.write_u32(count * 8 + 16 + i * 8, 0);
    }
    command.write_rgb(count * 8 + 8, [64, 32, 16]);
    if far {
        command.write_u32(count * 16 + 12, 0x7c1f);
        for i in 0..count {
            command.write_byte(count * 16 + 16 + i, 0);
        }
    }
    command
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 1,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn selector_ff_precedes_all_color_geometry_uv_and_far_reads() {
    for callback in CALLBACKS {
        for (selectors, mode) in [([255, 0], 0x4020), ([0, 255], 0x4121)] {
            let entry = sprite(selectors, 0x238);
            let command = packet(&entry, callback, mode);
            assert!(read_shared_shade_uv(&command).unwrap().is_none());
        }
        let entry = sprite([0, 1], 0);
        assert!(matches!(
            read_shared_shade_uv(&packet(&entry, callback, 1)),
            Err(SharedShadeUvPixelError::Selector(1))
        ));
    }
}

#[test]
fn each_packet_requires_only_its_native_fields_and_exact_relocation() {
    let entry = sprite([0, 0], 8);
    for callback in CALLBACKS {
        let (count, far) = shape(callback);
        let mut command = complete(&entry, callback, 0x4120);
        // Consumers read signed WORD XY and signed DWORD depth/UV.
        command.write_u32(0, 0x8000_7fff);
        command.write_u32(4, i32::MIN as u32);
        command.write_u32(count * 8 + 12, u32::MAX);
        let primitive = read_shared_shade_uv(&command).unwrap().unwrap();
        let (vertices, uv) = match primitive.geometry {
            SharedShadeUvGeometry::Triangle { vertices, uv, .. } => {
                (vertices.to_vec(), uv.to_vec())
            }
            SharedShadeUvGeometry::Quad { vertices, uv, .. } => (vertices.to_vec(), uv.to_vec()),
        };
        assert_eq!([vertices[0].x, vertices[0].y], [32767., -32768.]);
        assert_eq!(vertices[0].depth, i32::MIN);
        assert_eq!(uv[0][0], -1);
        assert_eq!(primitive.rgb, [64, 32, 16]);
        assert!(command.payload[count * 8 + 11].is_none()); // shared DWORD high byte
        let end = count * 16 + 12 + if far { 4 + count } else { 0 };
        assert!(command.payload[end..].iter().all(Option::is_none));
        let required = (0..count * 8 + 4)
            .chain(count * 8 + 8..count * 8 + 11)
            .chain(count * 8 + 12..end);
        for byte in required {
            let mut missing = command.clone();
            missing.payload[byte] = None;
            assert!(
                matches!(
                    read_shared_shade_uv(&missing),
                    Err(SharedShadeUvPixelError::Unknown { .. })
                ),
                "callback={callback:?} byte={byte:x}"
            );
        }
        for offset in [0, count * 8, count * 8 + 8] {
            let mut wrong = command.clone();
            wrong.texture = Some((offset, resource(&entry)));
            assert_eq!(
                read_shared_shade_uv(&wrong).unwrap_err(),
                SharedShadeUvPixelError::Texture(count * 8 + 4)
            );
        }
        let mut wrong = command.clone();
        wrong.payload_blocks += 1;
        assert_eq!(
            read_shared_shade_uv(&wrong).unwrap_err(),
            SharedShadeUvPixelError::Packet
        );
        wrong = command.clone();
        wrong.payload.pop();
        assert_eq!(
            read_shared_shade_uv(&wrong).unwrap_err(),
            SharedShadeUvPixelError::Packet
        );
        wrong = command;
        wrong.callback = PrimitiveCallback::TexturedUvQuadNear;
        assert_eq!(
            read_shared_shade_uv(&wrong).unwrap_err(),
            SharedShadeUvPixelError::Packet
        );
    }
}

#[derive(Deserialize)]
struct NativeFixture {
    cases: Vec<NativeCase>,
}

#[derive(Deserialize)]
struct NativeCase {
    callback: String,
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
    rgb: [u8; 3],
    far_color: u32,
    shifts: [u32; 3],
    fog: Vec<u8>,
    secondary_enabled: bool,
    secondary_reached: bool,
    selector_ff: bool,
    viewport: [i16; 4],
    vertices: Vec<[u32; 8]>,
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
}

fn vertex_bits(v: D3dTlVertex) -> [u32; 8] {
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

fn verify_submission<const V: usize, const T: usize>(
    result: D3dTexturedSubmission<V, T>,
    case: &NativeCase,
    index: usize,
) {
    assert_eq!(
        result.vertices.map(vertex_bits).to_vec(),
        case.vertices,
        "native TL case {index}"
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
    assert_eq!(draws, case.draws, "native primary fans case {index}");
    let packed_far = (u32::from(far_rgb(case.far_color, case.shifts)[0]) << 16)
        | (u32::from(far_rgb(case.far_color, case.shifts)[1]) << 8)
        | u32::from(far_rgb(case.far_color, case.shifts)[2]);
    let mut states = if case.callback.ends_with("far") {
        vec![[34, packed_far]]
    } else {
        Vec::new()
    };
    if case.manages {
        states.extend([
            [7, result.policy.depth_enable.unwrap()],
            [14, result.policy.depth_write.unwrap()],
        ]);
    } else {
        assert_eq!(
            (result.policy.depth_enable, result.policy.depth_write),
            (None, None)
        );
    }
    states.push([4, u32::from(result.policy.perspective)]);
    let blend = match result.policy.blend {
        D3dTextureBlend::Copy => [2, 1],
        D3dTextureBlend::Additive => [2, 2],
        D3dTextureBlend::InverseSourceColor => [1, 4],
        D3dTextureBlend::SourceAlpha => [5, 6],
    };
    states.extend([[19, blend[0]], [20, blend[1]]]);
    if case.flags & 0x30 != 0 {
        states.extend([[28, 0], [28, 0]]);
    } else {
        states.push([28, u32::from(result.policy.fog.enabled())]);
    }
    assert_eq!(
        states, case.states,
        "native ordered primary states case {index}"
    );
}

#[test]
fn original_four_consumers_match_tl_state_fans_and_secondary_prefix() {
    // 136 unchanged-PE receipts, x87 FPCW027F. Real native viewport clipping;
    // hosted COM/upload/display/error helpers. Secondary setup is excluded.
    // No GPU precision or observed retail scene acceptance is inferred.
    let fixture: NativeFixture = serde_json::from_str(include_str!(
        "../tests/fixtures/shared_shade_uv_pixels_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 136);
    for (index, case) in fixture.cases.iter().enumerate() {
        let callback = match case.callback.as_str() {
            "triangle_near" => CALLBACKS[0],
            "triangle_far" => CALLBACKS[1],
            "quad_near" => CALLBACKS[2],
            "quad_far" => CALLBACKS[3],
            _ => panic!("native family"),
        };
        let (count, far) = shape(callback);
        let entry = sprite(
            if case.selector_ff { [255, 255] } else { [0, 0] },
            case.flags,
        );
        let mut command = packet(&entry, callback, case.mode);
        if case.selector_ff {
            assert!(read_shared_shade_uv(&command).unwrap().is_none());
            assert!(case.states.is_empty() && case.draws.is_empty());
            continue;
        }
        for i in 0..count {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_u32(count * 8 + 12 + i * 8, case.uv[i][0] as u32);
            command.write_u32(count * 8 + 16 + i * 8, case.uv[i][1] as u32);
        }
        command.write_rgb(count * 8 + 8, case.rgb);
        if far {
            command.write_u32(count * 16 + 12, case.far_color);
            for i in 0..count {
                command.write_byte(count * 16 + 16 + i, case.fog[i]);
            }
        }
        let primitive = read_shared_shade_uv(&command).unwrap().unwrap();
        let result = prepare_shared_shade_uv(
            &primitive,
            D3dFlatState {
                material_shifts: case.shifts,
                near_depth: case.near,
                manages_depth: case.manages,
                depth_enable: case.depth_enable,
                depth_write: case.depth_write,
            },
            case.policy,
            case.secondary_enabled,
            case.uv_bounds,
        );
        assert_eq!(
            result.continuation == SharedShadeUvContinuation::SecondaryPassRequired,
            case.secondary_reached
        );
        match result.primary {
            D3dTexturedUvSubmission::Triangle(primary) => verify_submission(primary, case, index),
            D3dTexturedUvSubmission::Quad(primary) => verify_submission(primary, case, index),
        }
    }
}

fn pipeline() -> D3dRasterState {
    D3dRasterState {
        viewport: [0, 0, 7, 7],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    }
}

fn texture_state(secondary_texture: bool) -> SoftwareTextureState {
    SoftwareTextureState {
        upload: NativeTextureUploadState {
            source_rgb_shifts: [10, 5, 0],
            global_policy: 8,
        },
        sampling: D3dTextureSampling {
            min_filter: 1,
            mag_filter: 1,
            texture_blend: 4,
            address: 1,
            color_key_enabled: false,
            alpha_test_enabled: false,
        },
        secondary_texture,
    }
}

#[test]
fn all_four_callbacks_submit_real_nonwhite_texture_pixels() {
    for callback in CALLBACKS {
        let entry = sprite([0, 0], 2); // native direct RGB555 upload
        let mut command = complete(&entry, callback, 0x20);
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state(false));
        backend
            .draw(
                DrawInvocation {
                    receiver_offset: 0,
                    command_offset: 0,
                },
                &mut command,
            )
            .unwrap();
        assert!(backend.stats.fragments > 0);
        assert_eq!(backend.frame.pixel_rgba(3, 1), Some([128, 64, 31, 255]));
        let (count, _) = shape(callback);
        assert_eq!(
            [
                backend.stats.shared_shade_uv_triangles,
                backend.stats.shared_shade_uv_quads
            ],
            if count == 3 { [1, 0] } else { [0, 1] }
        );
        assert_eq!(
            [
                backend.stats.textured_uv_triangles,
                backend.stats.textured_uv_quads,
                backend.stats.effect_quads
            ],
            [0, 0, 0]
        );
    }
}

#[test]
fn secondary_boundary_retains_primary_pixels_and_failed_queue_packet_prefix() {
    for callback in CALLBACKS {
        let entry = sprite([0, 0], 0x202);
        let command = complete(&entry, callback, 0x20);
        let mut queue = DrawQueue::new(4096).unwrap();
        queue
            .submit(DrawSubmission {
                depth: 20,
                payload_blocks: command.payload_blocks,
                receiver_offset: 0,
                command,
            })
            .unwrap();
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state(true));
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Host(SoftwarePixelError::SharedShadeUv(
                SharedShadeUvPixelError::SecondaryPass
            )))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 0);
        assert!(backend.stats.fragments > 0);
        assert_eq!(backend.frame.pixel_rgba(3, 1), Some([128, 64, 31, 255]));
        assert_eq!(
            backend.stats.shared_shade_uv_triangles + backend.stats.shared_shade_uv_quads,
            1
        );
    }
}
