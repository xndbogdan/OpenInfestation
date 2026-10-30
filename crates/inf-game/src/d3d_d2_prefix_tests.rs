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

const CALLBACKS: [PrimitiveCallback; 2] = [
    PrimitiveCallback::D2TriangleNear,
    PrimitiveCallback::D2TriangleFar,
];

#[derive(Deserialize)]
struct PrefixFixture {
    cases: Vec<PrefixCase>,
}

#[derive(Deserialize)]
struct PrefixCase {
    callback: String,
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    coords: [[i16; 2]; 3],
    depths: [i32; 3],
    shades: [[u8; 3]; 3],
    uv_bounds: [f32; 4],
    far_color: u32,
    fog: [u8; 3],
    shifts: [u32; 3],
    secondary_enabled: bool,
    secondary_reached: bool,
    selector_ff: bool,
    viewport: [i16; 4],
    vertices: Vec<[u32; 8]>,
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
    events: Vec<String>,
}

#[test]
fn original_near_far_secondary_receipts_keep_converted_primary_and_viewport_prefix() {
    // Unchanged native consumer and viewport instructions; supplied COM/
    // upload/display/error services. Secondary descriptor reads and GPU pixels
    // remain outside this receipt's authority.
    let fixture: PrefixFixture =
        serde_json::from_str(include_str!("../tests/fixtures/d2_pixels_native.json")).unwrap();
    assert_eq!(fixture.cases.len(), 48);
    for (index, case) in fixture.cases.into_iter().enumerate() {
        let callback = match case.callback.as_str() {
            "near" => CALLBACKS[0],
            "far" => CALLBACKS[1],
            _ => panic!("native consumer"),
        };
        let entry = flagged_sprite(
            if case.selector_ff { [255; 2] } else { [0; 2] },
            &[case.flags],
        );
        let mut command = prefix_packet(&entry, callback, case.mode);
        if case.selector_ff {
            assert!(read_d2(&command).unwrap().is_none());
            assert!(
                case.vertices.is_empty()
                    && case.states.is_empty()
                    && case.draws.is_empty()
                    && case.events.is_empty()
            );
            assert!(!case.secondary_reached);
            continue;
        }
        for i in 0..3 {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
            command.write_rgb(0x20 + i * 4, case.shades[i]);
        }
        if callback == CALLBACKS[1] {
            command.write_u32(0x2c, case.far_color);
            command.write_rgb(0x30, case.fog);
        }
        let triangle = read_d2(&command).unwrap().unwrap();
        let result = prepare_d2(
            &triangle,
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
            result.continuation == D2Continuation::SecondaryPassRequired,
            case.secondary_reached,
            "native continuation case{index}"
        );
        let primary = result.primary;
        assert_eq!(
            primary.vertices.map(vertex_bits).to_vec(),
            case.vertices,
            "native TL case{index}"
        );
        assert_eq!(primary.triangles, [[0, 1, 2]]);
        let polygon = if primary.mode & 0x20 != 0 {
            primary.vertices.to_vec()
        } else {
            clip_polygon(
                &primary.vertices,
                D3dViewportState {
                    bounds: case.viewport,
                    near_depth: primary.near_depth,
                    mode: primary.mode,
                },
            )
            .unwrap()
        };
        let draws = if polygon.len() < 3 {
            Vec::new()
        } else {
            vec![polygon.into_iter().map(vertex_bits).collect::<Vec<_>>()]
        };
        assert_eq!(draws, case.draws, "native fan case{index}");
        let mut events = vec!["converted"];
        events.extend(std::iter::repeat_n("primary_draw", draws.len()));
        if case.secondary_reached {
            events.push("secondary_required");
        }
        assert_eq!(events, case.events, "native event prefix case{index}");
        let mut states = if callback == CALLBACKS[1] {
            let rgb = crate::d3d_effect::far_rgb(case.far_color, case.shifts);
            vec![[34, u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]])]]
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
        assert_eq!(
            states, case.states,
            "native primary state order case{index}"
        );
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
    depth: u32,
    shade: [u8; 3],
) -> PrimitiveCommand<'_> {
    let mut command = prefix_packet(entry, callback, mode);
    command.depth = depth;
    let mut texture = resource(entry);
    texture.source.bytes = &[255, 127];
    command.write_texture(0x1c, Some(texture));
    for (i, [x, y]) in [[0u16, 0u16], [6, 0], [0, 6]].into_iter().enumerate() {
        command.write_u32(i * 8, u32::from(x) | u32::from(y) << 16);
        command.write_u32(i * 8 + 4, depth);
        command.write_rgb(0x20 + i * 4, shade);
    }
    if callback == CALLBACKS[1] {
        command.write_u32(0x2c, 0x7c1f);
        command.write_rgb(0x30, [0; 3]);
    }
    command
}

#[test]
fn secondary_boundary_retains_near_far_primary_pixels_and_policy() {
    for callback in CALLBACKS {
        for mode in [0, 0x20, 0x4100, 0x4020] {
            for flags in [0x202, 0x20a, 0x212, 0x222] {
                let entry = flagged_sprite([0; 2], &[flags]);
                let command = pixel_packet(&entry, callback, mode, 20, [64, 32, 16]);
                let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let (fragments, depth_policy) = {
                    let mut backend = SoftwarePixelBackend::new(
                        &mut expected,
                        pixel_renderer(),
                        pipeline(),
                        texture_state(false),
                    );
                    backend
                        .draw(
                            DrawInvocation {
                                command_offset: 0,
                                receiver_offset: 0,
                            },
                            &mut command.clone(),
                        )
                        .unwrap();
                    assert_eq!(backend.stats.d2_triangles, 1);
                    assert!(backend.stats.fragments > 0);
                    if mode == 0x20 && flags == 0x202 {
                        assert_eq!(backend.frame.pixel_rgba(3, 1), Some([128, 64, 31, 255]));
                    }
                    (
                        backend.stats.fragments,
                        [backend.pipeline.depth_enabled, backend.pipeline.depth_write],
                    )
                };
                let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let mut backend = SoftwarePixelBackend::new(
                    &mut frame,
                    pixel_renderer(),
                    pipeline(),
                    texture_state(true),
                );
                assert_eq!(
                    backend.draw(
                        DrawInvocation {
                            command_offset: 0,
                            receiver_offset: 0
                        },
                        &mut command.clone()
                    ),
                    Err(SoftwarePixelError::D2(D2PixelError::SecondaryPass))
                );
                assert_eq!(backend.frame.rgba(), expected.rgba());
                assert_eq!(backend.stats.d2_triangles, 1);
                assert_eq!(backend.stats.fragments, fragments);
                assert_eq!(
                    [backend.pipeline.depth_enabled, backend.pipeline.depth_write],
                    depth_policy
                );
            }
        }
    }
}

#[test]
fn queue_failure_keeps_prior_and_current_primary_and_stops_later_triangles() {
    for callback in CALLBACKS {
        let ordinary = flagged_sprite([0; 2], &[2]);
        let secondary = flagged_sprite([0; 2], &[0x202]);
        let commands = [
            pixel_packet(&ordinary, callback, 0x20, 30, [128; 3]),
            pixel_packet(&secondary, callback, 0x20, 20, [64, 32, 16]),
            pixel_packet(&ordinary, callback, 0x20, 10, [0, 128, 0]),
        ];
        let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let fragments = {
            let mut backend = SoftwarePixelBackend::new(
                &mut expected,
                pixel_renderer(),
                pipeline(),
                texture_state(false),
            );
            for command in &commands[..2] {
                backend
                    .draw(
                        DrawInvocation {
                            command_offset: 0,
                            receiver_offset: 0,
                        },
                        &mut command.clone(),
                    )
                    .unwrap();
            }
            backend.stats.fragments
        };
        let mut queue = DrawQueue::new(4096).unwrap();
        for command in commands {
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
            Err(DrawQueueError::Host(SoftwarePixelError::D2(
                D2PixelError::SecondaryPass
            )))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 1);
        assert_eq!(backend.stats.d2_triangles, 2);
        assert_eq!(backend.stats.fragments, fragments);
        assert_eq!(backend.frame.rgba(), expected.rgba());
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
        );
        assert_eq!(backend.frame.rgba(), expected.rgba());
    }
}

#[test]
fn admission_failures_precede_secondary_and_ff_skips_every_other_field() {
    for callback in CALLBACKS {
        let entry = flagged_sprite([0; 2], &[0x202]);
        for invalid_upload in [false, true] {
            let mut command = pixel_packet(&entry, callback, 0x20, 20, [64, 32, 16]);
            let mut state = texture_state(true);
            let expected = if invalid_upload {
                state.upload.global_policy = 0;
                SoftwarePixelError::Upload(TextureUploadError::GlobalPolicy(0))
            } else {
                command.write_u32(4, 0);
                SoftwarePixelError::NonFinite
            };
            let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
            let mut backend =
                SoftwarePixelBackend::new(&mut frame, pixel_renderer(), pipeline(), state);
            assert_eq!(
                backend.draw(
                    DrawInvocation {
                        command_offset: 0,
                        receiver_offset: 0
                    },
                    &mut command
                ),
                Err(expected)
            );
            assert_eq!(
                (backend.stats.d2_triangles, backend.stats.fragments),
                (0, 0)
            );
            assert_eq!(backend.frame.pixel_rgba(3, 1), Some([1, 2, 3, 255]));
        }
        let entry = flagged_sprite([255; 2], &[0x202]);
        let mut command = pixel_packet(&entry, callback, 0x20, 20, [64, 32, 16]);
        for offset in (0..0x18).chain(0x20..command.payload.len()) {
            command.payload[offset] = None;
        }
        let mut state = texture_state(true);
        state.upload.global_policy = 0;
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, pixel_renderer(), pipeline(), state);
        backend
            .draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )
            .unwrap();
        assert_eq!(backend.stats.skipped, 1);
        assert_eq!(backend.stats.d2_triangles, 0);
        assert!(backend.textures.is_empty());
    }
}

fn flagged_sprite(selectors: [u8; 2], flags: &[u16]) -> SpriteEntry {
    assert_eq!(flags.len(), 1);
    let mut entry = sprite(selectors);
    entry.variants[0].flags = SpriteFlags::new(flags[0]);
    entry
}

fn prefix_packet(
    entry: &SpriteEntry,
    callback: PrimitiveCallback,
    mode: u32,
) -> PrimitiveCommand<'_> {
    let mut command =
        PrimitiveCommand::new(callback, 0, if callback == CALLBACKS[1] { 4 } else { 3 });
    command.write_u32(0x18, mode);
    command.write_texture(0x1c, Some(resource(entry)));
    command
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

#[test]
fn only_native_consumed_rgb_and_far_tail_bytes_are_required() {
    for callback in CALLBACKS {
        let entry = flagged_sprite([0; 2], &[0]);
        let mut command = pixel_packet(&entry, callback, 0x20, 20, [64, 32, 16]);
        // High bytes and padding are unknown, not invented shade/initialization.
        for offset in [0x23, 0x27, 0x2b] {
            command.payload[offset] = None;
        }
        let padding = if callback == CALLBACKS[1] { 0x33 } else { 0x2c };
        for byte in &mut command.payload[padding..] {
            *byte = None;
        }
        assert!(read_d2(&command).unwrap().is_some());
        command.payload[0x25] = None;
        assert_eq!(
            read_d2(&command).unwrap_err(),
            D2PixelError::Unknown {
                field: "per-corner RGB",
                offset: 0x25
            }
        );
    }
    let entry = flagged_sprite([0; 2], &[0]);
    let mut command = pixel_packet(&entry, CALLBACKS[1], 0x20, 20, [64; 3]);
    command.payload[0x31] = None;
    assert_eq!(
        read_d2(&command).unwrap_err(),
        D2PixelError::Unknown {
            field: "fog byte",
            offset: 0x31
        }
    );
}

#[test]
fn ordinary_far_fog_reaches_cpu_primary_before_secondary_boundary() {
    let entry = flagged_sprite([0; 2], &[0x202]);
    let mut command = pixel_packet(&entry, CALLBACKS[1], 0x20, 20, [128; 3]);
    command.write_rgb(0x30, [255; 3]);
    let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
    let mut backend = SoftwarePixelBackend::new(
        &mut frame,
        pixel_renderer(),
        pipeline(),
        texture_state(true),
    );
    assert_eq!(
        backend.draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0
            },
            &mut command
        ),
        Err(SoftwarePixelError::D2(D2PixelError::SecondaryPass))
    );
    assert_eq!(backend.stats.d2_triangles, 1);
    assert!(backend.stats.fragments > 0);
    assert_eq!(backend.frame.pixel_rgba(3, 1), Some([255, 0, 255, 255]));
}
