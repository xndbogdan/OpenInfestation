use super::*;
use crate::{
    d3d_texture::{NativeTextureUploadState, TextureUploadError},
    draw_queue::{DrawInvocation, DrawQueue, DrawQueueError, DrawQueueFault, DrawSubmission},
    pixel_backend::{flush_pixels, PixelBackend},
    software_frame::{
        D3dRasterState, D3dTextureSampling, SoftwareFrame, SoftwarePixelBackend,
        SoftwarePixelError, SoftwareTextureState,
    },
};

const CALLBACKS: [PrimitiveCallback; 4] = [
    PrimitiveCallback::TexturedUvTriangleNear,
    PrimitiveCallback::TexturedUvTriangleFar,
    PrimitiveCallback::TexturedUvQuadNear,
    PrimitiveCallback::TexturedUvQuadFar,
];

fn shape(callback: PrimitiveCallback) -> (usize, bool) {
    match callback {
        PrimitiveCallback::TexturedUvTriangleNear => (3, false),
        PrimitiveCallback::TexturedUvTriangleFar => (3, true),
        PrimitiveCallback::TexturedUvQuadNear => (4, false),
        PrimitiveCallback::TexturedUvQuadFar => (4, true),
        _ => panic!("UV-only callback"),
    }
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<ConsumerCase>,
}

#[derive(Deserialize)]
struct ConsumerCase {
    #[serde(flatten)]
    native: NativeCase,
    callback: String,
    far_color: u32,
    fog: Vec<u8>,
    shifts: [u32; 3],
    secondary_enabled: bool,
    secondary_reached: bool,
    selector_ff: bool,
    events: Vec<String>,
}

#[test]
fn four_original_consumers_match_white_shading_fog_states_and_primary_fans() {
    // Original consumer/viewport instructions; hosted COM/upload/display/error
    // boundaries. No secondary pixels, GPU precision or scene execution claim.
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../tests/fixtures/textured_uv_consumers_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 112);
    for (index, case) in fixture.cases.into_iter().enumerate() {
        let callback = match case.callback.as_str() {
            "triangle_near" => CALLBACKS[0],
            "triangle_far" => CALLBACKS[1],
            "quad_near" => CALLBACKS[2],
            "quad_far" => CALLBACKS[3],
            _ => panic!("native callback"),
        };
        let n = &case.native;
        let (count, far) = shape(callback);
        assert_eq!(count, n.count);
        let entry = sprite(if case.selector_ff { [255; 2] } else { [0; 2] }, &[n.flags]);
        let mut command = packet(&entry, count, n.mode);
        command.callback = callback;
        if case.selector_ff {
            assert!(read_textured_uv(&command).unwrap().is_none());
            assert!(
                n.vertices.is_empty()
                    && n.states.is_empty()
                    && n.draws.is_empty()
                    && case.events.is_empty()
            );
            assert!(!case.secondary_reached);
            continue;
        }
        for i in 0..count {
            command.write_u32(
                i * 8,
                u32::from(n.coords[i][0] as u16) | (u32::from(n.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, n.depths[i] as u32);
            command.write_u32(count * 8 + 8 + i * 8, n.uv[i][0] as u32);
            command.write_u32(count * 8 + 12 + i * 8, n.uv[i][1] as u32);
        }
        if far {
            command.write_u32(count * 16 + 8, case.far_color);
            for i in 0..count {
                command.write_byte(count * 16 + 12 + i, case.fog[i]);
            }
        }
        let primitive = read_textured_uv(&command).unwrap().unwrap();
        let result = prepare_textured_uv(
            &primitive,
            D3dFlatState {
                material_shifts: case.shifts,
                near_depth: n.near,
                manages_depth: n.manages,
                depth_enable: n.depth_enable,
                depth_write: n.depth_write,
            },
            n.policy,
            case.secondary_enabled,
            n.uv_bounds,
        );
        assert_eq!(
            result.continuation == TexturedUvContinuation::SecondaryPassRequired,
            case.secondary_reached,
            "continuation case{index}"
        );
        let policy = match &result.primary {
            D3dTexturedUvSubmission::Triangle(p) => p.policy,
            D3dTexturedUvSubmission::Quad(p) => p.policy,
        };
        match result.primary {
            D3dTexturedUvSubmission::Triangle(p) => verify_submission(p, n, index),
            D3dTexturedUvSubmission::Quad(p) => verify_submission(p, n, index),
        }
        let mut events = vec!["converted"];
        events.extend(std::iter::repeat_n("primary_draw", n.draws.len()));
        if case.secondary_reached {
            events.push("secondary_required");
        }
        assert_eq!(events, case.events, "event prefix case{index}");
        let mut states = if far {
            let rgb = crate::d3d_effect::far_rgb(case.far_color, case.shifts);
            vec![[34, u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]])]]
        } else {
            Vec::new()
        };
        if n.manages {
            states.extend([
                [7, policy.depth_enable.unwrap()],
                [14, policy.depth_write.unwrap()],
            ]);
        }
        states.push([4, u32::from(policy.perspective)]);
        let blend = match policy.blend {
            D3dTextureBlend::Copy => [2, 1],
            D3dTextureBlend::Additive => [2, 2],
            D3dTextureBlend::InverseSourceColor => [1, 4],
            D3dTextureBlend::SourceAlpha => [5, 6],
        };
        states.extend([[19, blend[0]], [20, blend[1]]]);
        if n.flags & 0x30 != 0 {
            states.extend([[28, 0], [28, 0]]);
        } else {
            states.push([28, u32::from(policy.fog.enabled())]);
        }
        assert_eq!(
            states, n.states,
            "ordered primary device states case{index}"
        );
    }
}

#[test]
fn far_tail_reads_are_required_after_ff_and_before_geometry_with_unread_padding() {
    let entry = sprite([0; 2], &[0]);
    for callback in [CALLBACKS[1], CALLBACKS[3]] {
        let (count, _) = shape(callback);
        let mut command = complete_packet(&entry, count, 0);
        command.callback = callback;
        let tail = count * 16 + 8;
        assert!(
            matches!(read_textured_uv(&command), Err(TexturedPixelError::Unknown {
            field: "far color", offset }) if offset == tail)
        );
        command.write_u32(tail, 0x7c1f);
        for i in 0..count {
            command.write_byte(tail + 4 + i, i as u8);
        }
        assert!(read_textured_uv(&command).unwrap().is_some());
        assert!(command.payload[tail + 4 + count..]
            .iter()
            .all(Option::is_none));
        for offset in tail..tail + 4 + count {
            let mut missing = command.clone();
            missing.payload[offset] = None;
            // A required DWORD reports its starting offset even when one of
            // its later bytes is absent; fog bytes are consumed individually.
            let expected_offset = if offset < tail + 4 { tail } else { offset };
            assert!(
                matches!(read_textured_uv(&missing), Err(TexturedPixelError::Unknown { offset: at, .. }) if at == expected_offset)
            );
        }
        let mut missing = command.clone();
        missing.payload[0] = None;
        missing.payload[tail] = None;
        assert!(
            matches!(read_textured_uv(&missing), Err(TexturedPixelError::Unknown {
            field: "far color", offset }) if offset == tail)
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
    fog: u8,
) -> PrimitiveCommand<'_> {
    let (count, far) = shape(callback);
    let mut command = packet(entry, count, mode);
    command.callback = callback;
    command.depth = depth;
    let mut texture = resource(entry);
    texture.source.bytes = &[255, 127];
    command.write_texture(count * 8 + 4, Some(texture));
    let coords = if count == 3 {
        vec![[0u16, 0u16], [6, 0], [0, 6]]
    } else {
        vec![[0u16, 0u16], [6, 0], [6, 6], [0, 6]]
    };
    for (i, [x, y]) in coords.into_iter().enumerate() {
        command.write_u32(i * 8, u32::from(x) | u32::from(y) << 16);
        command.write_u32(i * 8 + 4, depth);
        command.write_u32(count * 8 + 8 + i * 8, 0);
        command.write_u32(count * 8 + 12 + i * 8, 0);
    }
    if far {
        command.write_u32(count * 16 + 8, 0x7c1f);
        for i in 0..count {
            command.write_byte(count * 16 + 12 + i, fog);
        }
    }
    command
}
fn primitive_count(backend: &SoftwarePixelBackend<'_, '_>, count: usize) -> usize {
    if count == 3 {
        backend.stats.textured_uv_triangles
    } else {
        backend.stats.textured_uv_quads
    }
}
fn invocation() -> DrawInvocation {
    DrawInvocation {
        command_offset: 0,
        receiver_offset: 0,
    }
}

#[test]
fn secondary_boundary_keeps_four_primary_pixel_paths_and_clipped_away_prefix() {
    for callback in CALLBACKS {
        let (count, _) = shape(callback);
        for mode in [0, 0x20, 0x4100, 0x4020] {
            for flags in [0x202, 0x20a, 0x212, 0x222] {
                let entry = sprite([0; 2], &[flags]);
                let command = pixel_packet(&entry, callback, mode, 20, 127);
                let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let (fragments, depth_policy) = {
                    let mut backend = SoftwarePixelBackend::new(
                        &mut expected,
                        pixel_renderer(),
                        pipeline(),
                        texture_state(false),
                    );
                    backend.draw(invocation(), &mut command.clone()).unwrap();
                    assert_eq!(primitive_count(&backend, count), 1);
                    assert!(backend.stats.fragments > 0);
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
                    backend.draw(invocation(), &mut command.clone()),
                    Err(SoftwarePixelError::Texture(
                        TexturedPixelError::SecondaryPass
                    ))
                );
                assert_eq!(backend.frame.rgba(), expected.rgba());
                for y in 0..8 {
                    for x in 0..8 {
                        assert_eq!(backend.frame.pixel_depth(x, y), expected.pixel_depth(x, y));
                    }
                }
                assert_eq!(primitive_count(&backend, count), 1);
                assert_eq!(backend.stats.fragments, fragments);
                assert_eq!(
                    [backend.pipeline.depth_enabled, backend.pipeline.depth_write],
                    depth_policy
                );
            }
        }
        let entry = sprite([0; 2], &[0x202]);
        let mut command = pixel_packet(&entry, callback, 0, 20, 255);
        for i in 0..count {
            command.write_u32(
                i * 8,
                u32::from(-50i16 as u16) | (u32::from(-50i16 as u16) << 16),
            );
        }
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend = SoftwarePixelBackend::new(
            &mut frame,
            pixel_renderer(),
            pipeline(),
            texture_state(true),
        );
        assert_eq!(
            backend.draw(invocation(), &mut command),
            Err(SoftwarePixelError::Texture(
                TexturedPixelError::SecondaryPass
            ))
        );
        assert_eq!(primitive_count(&backend, count), 1);
        assert_eq!(backend.stats.fragments, 0);
        assert_eq!(backend.frame.pixel_rgba(3, 1), Some([1, 2, 3, 255]));
    }
}

#[test]
fn far_white_fade_and_vertex_fog_make_distinct_cpu_pixels() {
    // The native upload of RGB555 0x7fff is [255, 255, 248], without
    // low-bit replication (d3d_texture's native upload regression). White
    // diffuse preserves that texel; fog127 either modulates by 128/255 or
    // blends toward the decoded magenta fog color [255, 0, 255].
    for callback in [CALLBACKS[1], CALLBACKS[3]] {
        for (flags, fog, expected) in [
            (2, 0, [255, 255, 248, 255]),
            (0x12, 0, [255, 255, 248, 255]),
            (0x12, 127, [128, 128, 124, 255]),
            (2, 127, [255, 128, 251, 255]),
        ] {
            let entry = sprite([0; 2], &[flags]);
            let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
            let mut backend = SoftwarePixelBackend::new(
                &mut frame,
                pixel_renderer(),
                pipeline(),
                texture_state(false),
            );
            backend
                .draw(
                    invocation(),
                    &mut pixel_packet(&entry, callback, 0x20, 20, fog),
                )
                .unwrap();
            assert_eq!(
                backend.frame.pixel_rgba(3, 1),
                Some(expected),
                "{callback:?} flags{flags:x} fog{fog}"
            );
        }
    }
}

#[test]
fn uv_queue_failure_keeps_prior_and_current_primary_and_stops_later_draws() {
    for callback in CALLBACKS {
        let (count, _) = shape(callback);
        let ordinary = sprite([0; 2], &[2]);
        let secondary = sprite([0; 2], &[0x202]);
        let commands = [
            pixel_packet(&ordinary, callback, 0x20, 30, 0),
            pixel_packet(&secondary, callback, 0x20, 20, 127),
            pixel_packet(&ordinary, callback, 0x20, 10, 255),
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
                backend.draw(invocation(), &mut command.clone()).unwrap();
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
            Err(DrawQueueError::Host(SoftwarePixelError::Texture(
                TexturedPixelError::SecondaryPass
            )))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 1);
        assert_eq!(primitive_count(&backend, count), 2);
        assert_eq!(backend.stats.fragments, fragments);
        assert_eq!(backend.frame.rgba(), expected.rgba());
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(backend.frame.pixel_depth(x, y), expected.pixel_depth(x, y));
            }
        }
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
        );
        assert_eq!(backend.frame.rgba(), expected.rgba());
    }
}

#[test]
fn uv_admission_failures_precede_secondary_and_ff_suppresses_unknown_tail() {
    for callback in CALLBACKS {
        let (count, _) = shape(callback);
        let entry = sprite([0; 2], &[0x202]);
        for invalid_upload in [false, true] {
            let mut command = pixel_packet(&entry, callback, 0x20, 20, 0);
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
            assert_eq!(backend.draw(invocation(), &mut command), Err(expected));
            assert_eq!(primitive_count(&backend, count), 0);
            assert_eq!(backend.stats.fragments, 0);
            assert_eq!(backend.frame.pixel_rgba(3, 1), Some([1, 2, 3, 255]));
        }
        let entry = sprite([255; 2], &[0x202]);
        let mut command = pixel_packet(&entry, callback, 0x20, 20, 0);
        for offset in (0..count * 8).chain(count * 8 + 8..command.payload.len()) {
            command.payload[offset] = None;
        }
        let mut state = texture_state(true);
        state.upload.global_policy = 0;
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, pixel_renderer(), pipeline(), state);
        backend.draw(invocation(), &mut command).unwrap();
        assert_eq!(backend.stats.skipped, 1);
        assert_eq!(primitive_count(&backend, count), 0);
        assert!(backend.textures.is_empty());
    }
}
