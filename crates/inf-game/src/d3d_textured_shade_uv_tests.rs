use super::*;
use crate::{
    d3d_texture::NativeTextureUploadState,
    d3d_viewport::{clip_polygon, D3dViewportState},
    draw_queue::{DrawQueue, DrawQueueError, DrawSubmission},
    pixel_backend::flush_pixels,
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
const CALLBACKS: [PrimitiveCallback; 2] = [
    PrimitiveCallback::TexturedShadeUvQuadNear,
    PrimitiveCallback::TexturedShadeUvQuadFar,
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
fn packet(entry: &SpriteEntry, callback: PrimitiveCallback, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = PrimitiveCommand::new(callback, 20, 6);
    command.write_u32(0x20, mode);
    command.write_texture(0x24, Some(resource(entry)));
    command
}
fn complete(entry: &SpriteEntry, callback: PrimitiveCallback) -> PrimitiveCommand<'_> {
    let mut command = packet(entry, callback, 0x20);
    for (i, xy) in [[0i16, 0i16], [6, 0], [6, 6], [0, 6]]
        .into_iter()
        .enumerate()
    {
        command.write_u32(
            i * 8,
            u32::from(xy[0] as u16) | (u32::from(xy[1] as u16) << 16),
        );
        command.write_u32(i * 8 + 4, 20);
        command.write_rgb(0x28 + i * 4, [64, 32, 16]);
        command.write_u32(0x38 + i * 8, 0);
        command.write_u32(0x3c + i * 8, 0);
    }
    if callback == CALLBACKS[1] {
        command.write_u32(0x58, 0x7c1f);
        for i in 0..4 {
            command.write_byte(0x5c + i, 0);
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
fn selector_ff_skips_unknown_geometry_shades_uv_and_far_tail() {
    for callback in CALLBACKS {
        for (selectors, mode) in [([255, 0], 0), ([0, 255], 1)] {
            let entry = sprite(selectors, 0x238);
            assert!(read_shade_uv_quad(&packet(&entry, callback, mode))
                .unwrap()
                .is_none());
        }
    }
}

#[test]
fn exact_packet_fields_preserve_signed_uv_and_ignore_shade_high_and_near_tail() {
    let entry = sprite([0, 0], 2);
    for callback in CALLBACKS {
        let mut command = complete(&entry, callback);
        command.write_u32(0, 0x8000_7fff);
        command.write_u32(4, i32::MIN as u32);
        command.write_u32(0x38, u32::MAX);
        let quad = read_shade_uv_quad(&command).unwrap().unwrap();
        assert_eq!([quad.vertices[0].x, quad.vertices[0].y], [32767., -32768.]);
        assert_eq!(quad.vertices[0].depth, i32::MIN);
        assert_eq!(quad.uv[0][0], -1);
        assert!(command.payload[0x2b].is_none());
        let mut required = (0..0x24).chain(0x38..0x58).collect::<Vec<_>>();
        for i in 0..4 {
            required.extend(0x28 + i * 4..0x2b + i * 4);
            assert!(command.payload[0x2b + i * 4].is_none());
        }
        if callback == CALLBACKS[1] {
            required.extend(0x58..0x60);
        } else {
            assert!(command.payload[0x58..].iter().all(Option::is_none));
        }
        for byte in required {
            let mut missing = command.clone();
            missing.payload[byte] = None;
            assert!(
                matches!(
                    read_shade_uv_quad(&missing),
                    Err(TexturedShadeUvPixelError::Unknown { .. })
                ),
                "{callback:?} byte{byte:x}"
            );
        }
        let mut wrong = command.clone();
        wrong.texture = Some((0x1c, resource(&entry)));
        assert_eq!(
            read_shade_uv_quad(&wrong).unwrap_err(),
            TexturedShadeUvPixelError::Texture
        );
        wrong = command.clone();
        wrong.payload_blocks = 5;
        assert_eq!(
            read_shade_uv_quad(&wrong).unwrap_err(),
            TexturedShadeUvPixelError::Packet
        );
        wrong = command;
        wrong.callback = PrimitiveCallback::D6QuadNear;
        assert_eq!(
            read_shade_uv_quad(&wrong).unwrap_err(),
            TexturedShadeUvPixelError::Packet
        );
    }
    let entry = sprite([1, 0], 2);
    assert_eq!(
        read_shade_uv_quad(&packet(&entry, CALLBACKS[0], 0)).unwrap_err(),
        TexturedShadeUvPixelError::Selector(1)
    );
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
    depths: [i32; 4],
    coords: [[i16; 2]; 4],
    uv: [[i32; 2]; 4],
    uv_bounds: [f32; 4],
    shades: [[u8; 3]; 4],
    far_color: u32,
    shifts: [u32; 3],
    fog: [u8; 4],
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

#[test]
fn original_near_far_consumers_match_primary_tl_state_fans_and_secondary_prefix() {
    // Supplied COM/upload/display/error helpers, unchanged original conversion
    // and viewport instructions under FPCW027F. No GPU-pixel parity is claimed.
    let fixture: NativeFixture = serde_json::from_str(include_str!(
        "../tests/fixtures/textured_shade_uv_pixels_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 68);
    for (index, case) in fixture.cases.into_iter().enumerate() {
        let callback = match case.callback.as_str() {
            "quad_near" => CALLBACKS[0],
            "quad_far" => CALLBACKS[1],
            _ => panic!("native family"),
        };
        let entry = sprite(if case.selector_ff { [255; 2] } else { [0; 2] }, case.flags);
        let mut command = packet(&entry, callback, case.mode);
        if case.selector_ff {
            assert!(read_shade_uv_quad(&command).unwrap().is_none());
            assert!(case.states.is_empty() && case.draws.is_empty());
            continue;
        }
        for i in 0..4 {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_rgb(0x28 + i * 4, case.shades[i]);
            command.write_u32(0x38 + i * 8, case.uv[i][0] as u32);
            command.write_u32(0x3c + i * 8, case.uv[i][1] as u32);
        }
        if callback == CALLBACKS[1] {
            command.write_u32(0x58, case.far_color);
            for i in 0..4 {
                command.write_byte(0x5c + i, case.fog[i]);
            }
        }
        let quad = read_shade_uv_quad(&command).unwrap().unwrap();
        let result = prepare_shade_uv_quad(
            &quad,
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
            result.continuation == TexturedShadeUvContinuation::SecondaryPassRequired,
            case.secondary_reached
        );
        let primary = result.primary;
        assert_eq!(
            primary.vertices.map(vertex_bits).to_vec(),
            case.vertices,
            "native TL case{index}"
        );
        let mut draws = Vec::new();
        for indices in primary.triangles {
            let triangle = indices.map(|i| primary.vertices[i]);
            let polygon = if primary.mode & 0x20 != 0 {
                triangle.to_vec()
            } else {
                clip_polygon(
                    &triangle,
                    D3dViewportState {
                        bounds: case.viewport,
                        near_depth: primary.near_depth,
                        mode: primary.mode,
                    },
                )
                .unwrap()
            };
            if polygon.len() >= 3 {
                draws.push(polygon.into_iter().map(vertex_bits).collect::<Vec<_>>());
            }
        }
        assert_eq!(draws, case.draws, "native fans case{index}");
        let mut states = if callback == CALLBACKS[1] {
            let rgb = crate::d3d_effect::far_rgb(case.far_color, case.shifts);
            vec![[
                34,
                (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]),
            ]]
        } else {
            Vec::new()
        };
        if case.manages {
            states.extend([
                [7, primary.policy.depth_enable.unwrap()],
                [14, primary.policy.depth_write.unwrap()],
            ]);
        } else {
            assert_eq!(
                (primary.policy.depth_enable, primary.policy.depth_write),
                (None, None)
            );
        }
        states.push([4, u32::from(primary.policy.perspective)]);
        let blend = match primary.policy.blend {
            D3dTextureBlend::Copy => [2, 1],
            D3dTextureBlend::Additive => [2, 2],
            D3dTextureBlend::InverseSourceColor => [1, 4],
            D3dTextureBlend::SourceAlpha => [5, 6],
        };
        states.extend([[19, blend[0]], [20, blend[1]]]);
        if case.flags & 0x30 != 0 {
            states.extend([[28, 0], [28, 0]]);
        } else {
            states.push([28, u32::from(primary.policy.fog.enabled())]);
        }
        assert_eq!(states, case.states, "native states case{index}");
    }
}

#[test]
fn both_callbacks_reach_shaded_texture_pixels_and_keep_primary_on_secondary_failure() {
    for callback in CALLBACKS {
        for secondary in [false, true] {
            let entry = sprite([0; 2], 0x202);
            let command = complete(&entry, callback);
            let mut queue = DrawQueue::new(4096).unwrap();
            queue
                .submit(DrawSubmission {
                    depth: 20,
                    payload_blocks: 6,
                    receiver_offset: 0,
                    command,
                })
                .unwrap();
            let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
            let mut backend = SoftwarePixelBackend::new(
                &mut frame,
                renderer(),
                pipeline(),
                texture_state(secondary),
            );
            let result = flush_pixels(&mut queue, &mut backend);
            if secondary {
                assert_eq!(
                    result,
                    Err(DrawQueueError::Host(SoftwarePixelError::TexturedShadeUv(
                        TexturedShadeUvPixelError::SecondaryPass
                    )))
                );
                assert_eq!(queue.state().last_flush_completed, 0);
                assert!(queue.state().stopped);
            } else {
                assert!(result.is_ok());
                assert_eq!(queue.state().last_flush_completed, 1);
            }
            assert_eq!(backend.frame.pixel_rgba(3, 1), Some([128, 64, 31, 255]));
            assert_eq!(backend.stats.textured_shade_uv_quads, 1);
            assert!(backend.stats.fragments > 0);
            assert_eq!(
                [backend.stats.d6_quads, backend.stats.shared_shade_uv_quads],
                [0, 0]
            );
        }
    }
}
