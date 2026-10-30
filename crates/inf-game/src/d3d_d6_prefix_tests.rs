//! Direct D6 primary-prefix receipts and concrete shared CPU/queue custody.
//! Native conversion/viewport/fans are executed; external device helpers are
//! supplied. Secondary setup and retail GPU pixels remain separate owners.

use super::*;
use crate::{
    d3d_texture::{NativeTextureUploadState, TextureUploadError},
    d3d_viewport::{clip_polygon, D3dViewportState},
    draw_queue::{DrawInvocation, DrawQueue, DrawQueueError, DrawQueueFault, DrawSubmission},
    pixel_backend::{flush_pixels, PixelBackend},
    software_frame::{
        D3dRasterState, D3dTextureSampling, SoftwareFrame, SoftwarePixelBackend,
        SoftwarePixelError, SoftwareTextureState,
    },
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    callback: String,
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    coords: [[i16; 2]; 4],
    depths: [i32; 4],
    uv_bounds: [f32; 4],
    shades: [[u8; 3]; 4],
    far_color: u32,
    fog: [u8; 4],
    secondary_enabled: bool,
    secondary_reached: bool,
    selector_ff: bool,
    viewport: [i16; 4],
    vertices: Vec<[u32; 8]>,
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
    events: Vec<String>,
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

fn xy([x, y]: [i16; 2]) -> u32 {
    u32::from(x as u16) | (u32::from(y as u16) << 16)
}

const CALLBACKS: [PrimitiveCallback; 2] =
    [PrimitiveCallback::D6QuadNear, PrimitiveCallback::D6QuadFar];

#[test]
fn original_near_far_d6_primary_fans_precede_secondary_setup() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../tests/fixtures/d6_secondary_prefix_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 48);
    for (index, case) in fixture.cases.iter().enumerate() {
        let far = match case.callback.as_str() {
            "near" => false,
            "far" => true,
            other => panic!("unknown native callback {other}"),
        };
        let mut entry = sprite(if case.selector_ff { [255; 2] } else { [0; 2] });
        entry.variants[0].flags = SpriteFlags::new(case.flags);
        let mut command = packet(&entry, case.mode);
        command.callback = CALLBACKS[usize::from(far)];
        if case.selector_ff {
            assert!(read_d6(&command).unwrap().is_none());
            assert!(
                case.vertices.is_empty()
                    && case.draws.is_empty()
                    && case.states.is_empty()
                    && case.events.is_empty()
            );
            continue;
        }
        for i in 0..4 {
            command.write_u32(i * 8, xy(case.coords[i]));
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_rgb(0x28 + i * 4, case.shades[i]);
        }
        if far {
            command.write_u32(0x38, case.far_color);
            for (i, fog) in case.fog.into_iter().enumerate() {
                command.write_byte(0x3c + i, fog);
            }
        }
        let result = prepare_d6(
            &read_d6(&command).unwrap().unwrap(),
            D3dFlatState {
                material_shifts: [6, 1, 4],
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
            result.continuation == D6QuadContinuation::SecondaryPassRequired,
            case.secondary_reached,
            "continuation case{index}"
        );
        let primary = result.primary;
        assert_eq!(
            primary.vertices.map(vertex_bits).as_slice(),
            case.vertices.as_slice(),
            "native TL case{index}"
        );
        let mut draws = Vec::new();
        for indices in primary.triangles {
            let triangle = indices.map(|i| primary.vertices[i]);
            let polygon = if case.mode & 0x20 != 0 {
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
        assert_eq!(draws, case.draws, "native primary fans case{index}");
        let mut states = Vec::new();
        if far {
            let [r, g, b] = crate::d3d_effect::far_rgb(case.far_color, [6, 1, 4]);
            states.push([34, u32::from_be_bytes([0, r, g, b])]);
        }
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
        assert_eq!(states, case.states, "ordered native states case{index}");
        let mut events = vec!["converted"];
        events.extend(std::iter::repeat_n("primary_draw", draws.len()));
        if case.secondary_reached {
            events.push("secondary_required");
        }
        assert_eq!(events, case.events, "native reached order case{index}");
    }
}

fn pixel_renderer() -> D3dFlatState {
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

fn pixel_packet(
    entry: &SpriteEntry,
    callback: PrimitiveCallback,
    mode: u32,
) -> PrimitiveCommand<'_> {
    let mut command = PrimitiveCommand::new(callback, 20, 4);
    for (i, point) in [[0, 0], [6, 0], [6, 6], [0, 6]].into_iter().enumerate() {
        command.write_u32(i * 8, xy(point));
        command.write_u32(i * 8 + 4, 20);
        command.write_rgb(0x28 + i * 4, [64, 32, 16]);
    }
    command.write_u32(0x20, mode);
    let mut texture = resource(entry);
    texture.source.bytes = &[255, 127];
    command.write_texture(0x24, Some(texture));
    if callback == CALLBACKS[1] {
        command.write_u32(0x38, 0x7c1f);
        for i in 0..4 {
            command.write_byte(0x3c + i, 0);
        }
    }
    command
}

fn invocation() -> DrawInvocation {
    DrawInvocation {
        command_offset: 0,
        receiver_offset: 0,
    }
}

#[test]
fn both_d6_callbacks_retain_primary_pixels_counts_and_depth_before_secondary_error() {
    for callback in CALLBACKS {
        for mode in [0, 0x20, 0x4100, 0x4020] {
            for flags in [0x202, 0x20a, 0x212, 0x222] {
                let mut entry = sprite([0; 2]);
                entry.variants[0].flags = SpriteFlags::new(flags);
                let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let (expected_fragments, expected_pipeline) = {
                    let mut baseline = SoftwarePixelBackend::new(
                        &mut expected,
                        pixel_renderer(),
                        pipeline(),
                        texture_state(false),
                    );
                    baseline
                        .draw(invocation(), &mut pixel_packet(&entry, callback, mode))
                        .unwrap();
                    assert!(baseline.stats.fragments > 0);
                    (baseline.stats.fragments, baseline.pipeline)
                };
                let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let mut backend = SoftwarePixelBackend::new(
                    &mut frame,
                    pixel_renderer(),
                    pipeline(),
                    texture_state(true),
                );
                assert_eq!(
                    backend.draw(invocation(), &mut pixel_packet(&entry, callback, mode)),
                    Err(SoftwarePixelError::D6(D6PixelError::SecondaryPass))
                );
                assert_eq!(backend.frame.rgba(), expected.rgba());
                for y in 0..8 {
                    for x in 0..8 {
                        assert_eq!(backend.frame.pixel_depth(x, y), expected.pixel_depth(x, y));
                    }
                }
                assert_eq!(backend.stats.d6_quads, 1);
                assert_eq!(backend.stats.fragments, expected_fragments);
                assert_eq!(
                    backend.pipeline.depth_enabled,
                    expected_pipeline.depth_enabled
                );
                assert_eq!(backend.pipeline.depth_write, expected_pipeline.depth_write);
                if flags == 0x202 && mode == 0x20 {
                    assert_eq!(backend.frame.pixel_rgba(3, 2), Some([128, 64, 31, 255]));
                }
            }
        }
    }
}

fn flat(depth: u32, material: u32) -> PrimitiveCommand<'static> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatTriangleNear, depth, 3);
    for (i, point) in [[0, 0], [6, 0], [0, 6]].into_iter().enumerate() {
        command.write_u32(i * 8, xy(point));
        command.write_u32(i * 8 + 4, depth);
        command.write_rgb(0x20 + i * 4, [128; 3]);
    }
    command.write_u32(0x18, 0x20);
    command.write_u32(0x1c, material);
    command
}

#[test]
fn d6_queue_keeps_prior_and_current_primary_then_stops_later_packets() {
    for callback in CALLBACKS {
        let mut entry = sprite([0; 2]);
        entry.variants[0].flags = SpriteFlags::new(0x202);
        let prior = flat(30, 0x7fff);
        let current = pixel_packet(&entry, callback, 0x20);
        let later = flat(10, 0x03e0);
        let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let fragments = {
            let mut baseline = SoftwarePixelBackend::new(
                &mut expected,
                pixel_renderer(),
                pipeline(),
                texture_state(false),
            );
            for mut command in [prior.clone(), current.clone()] {
                baseline.draw(invocation(), &mut command).unwrap();
            }
            baseline.stats.fragments
        };
        let mut queue = DrawQueue::new(4096).unwrap();
        for command in [prior, current, later] {
            queue
                .submit(DrawSubmission {
                    depth: command.depth,
                    payload_blocks: command.payload_blocks,
                    receiver_offset: 0,
                    command,
                })
                .unwrap();
        }
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend = SoftwarePixelBackend::new(
            &mut frame,
            pixel_renderer(),
            pipeline(),
            texture_state(true),
        );
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Host(SoftwarePixelError::D6(
                D6PixelError::SecondaryPass
            )))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 1);
        assert_eq!(backend.stats.triangles, 1);
        assert_eq!(backend.stats.d6_quads, 1);
        assert_eq!(backend.stats.fragments, fragments);
        assert_eq!(backend.frame.rgba(), expected.rgba());
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
        );
        assert_eq!(backend.stats.d6_quads, 1);
        assert_eq!(backend.frame.rgba(), expected.rgba());
    }
}

#[test]
fn d6_primary_admission_errors_precede_secondary_and_ff_skips_unknown_inputs() {
    for callback in CALLBACKS {
        let mut entry = sprite([0; 2]);
        entry.variants[0].flags = SpriteFlags::new(0x202);
        for bad_upload in [false, true] {
            let mut command = pixel_packet(&entry, callback, 0x20);
            let mut state = texture_state(true);
            let error = if bad_upload {
                state.upload.global_policy = 0;
                SoftwarePixelError::Upload(TextureUploadError::GlobalPolicy(0))
            } else {
                command.write_u32(4, 0);
                SoftwarePixelError::NonFinite
            };
            let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
            let mut backend =
                SoftwarePixelBackend::new(&mut frame, pixel_renderer(), pipeline(), state);
            assert_eq!(backend.draw(invocation(), &mut command), Err(error));
            assert_eq!(backend.stats.d6_quads, 0);
            assert_eq!(backend.stats.fragments, 0);
            assert_eq!(backend.frame.pixel_rgba(3, 2), Some([1, 2, 3, 255]));
        }
        let entry = sprite([255; 2]);
        let mut command = packet(&entry, 0x20);
        command.callback = callback;
        let mut state = texture_state(true);
        state.upload.global_policy = 0;
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, pixel_renderer(), pipeline(), state);
        backend.draw(invocation(), &mut command).unwrap();
        assert_eq!(backend.stats.skipped, 1);
        assert_eq!(backend.stats.d6_quads, 0);
        assert!(backend.textures.is_empty());
        assert_eq!(backend.frame.pixel_rgba(3, 2), Some([1, 2, 3, 255]));
    }
}
