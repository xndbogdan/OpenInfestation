use super::*;
use crate::{
    d3d_flat::D3dFlatState,
    draw_queue::{DrawQueue, DrawQueueError, DrawQueueFault, DrawSubmission},
    pixel_backend::flush_pixels,
};

#[path = "software_frame_effect_tests.rs"]
mod effect;

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
        viewport: [0, 0, 100, 100],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    }
}

fn fill(frame: &mut SoftwareFrame, triangle: &FlatTriangle) -> usize {
    frame
        .fill_flat_triangle(triangle, renderer(), &mut pipeline())
        .unwrap()
}

fn pack_xy(x: i16, y: i16) -> u32 {
    ((u32::from(y as u16)) << 16) | u32::from(x as u16)
}

fn flat_command(
    callback: PrimitiveCallback,
    blocks: usize,
    vertices: [(i16, i16, u32); 3],
    mode: u32,
    material: u32,
    rgb: [[u8; 3]; 3],
) -> PrimitiveCommand<'static> {
    let mut command = PrimitiveCommand::new(callback, vertices[0].2, blocks);
    for (index, (x, y, depth)) in vertices.iter().enumerate() {
        command.write_u32(index * 8, pack_xy(*x, *y));
        command.write_u32(index * 8 + 4, *depth);
    }
    command.write_u32(0x18, mode);
    command.write_u32(0x1c, material);
    for (index, corner) in rgb.iter().enumerate() {
        command.write_rgb(0x20 + index * 4, *corner);
    }
    command
}

fn near_flat(vertices: [(i16, i16, u32); 3], material: u32) -> PrimitiveCommand<'static> {
    flat_command(
        PrimitiveCallback::FlatTriangleNear,
        3,
        vertices,
        0x20,
        material,
        [[128; 3]; 3],
    )
}

#[test]
fn far_triangle_pixels_use_vertex_fog_or_rgb_fade_without_near_fallback() {
    use crate::{
        render_resources::{
            RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest,
            RenderSource,
        },
        ResidentRegistration,
    };
    use inf_formats::{
        ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
        SpriteModeSelectors, SpriteVariant,
    };
    let source = [0xff, 0x7f];
    let registration = ResidentRegistration {
        level: 0,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 0,
    };
    for callback in [
        PrimitiveCallback::SharedShadeTriangleFar,
        PrimitiveCallback::TexturedTriangleFar,
    ] {
        for (flags, fog, expected, writes_depth) in [
            (2u16, 0u8, [255, 0, 0, 255], true),
            (2, 255, [0, 0, 255, 255], true),
            (0x12, 255, [10, 20, 30, 255], false),
            (0x22, 255, [10, 20, 30, 0], false),
        ] {
            let entry = SpriteEntry {
                local_index: 0,
                span: ByteSpan::new(0, 0),
                selectors: SpriteModeSelectors {
                    mode0: 0,
                    mode1: 0,
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
            };
            let resource = RenderResource {
                request: RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: 0,
                },
                source: RenderSource {
                    registration: &registration,
                    bytes: &source,
                },
                data: RenderResourceData::Sprite(&entry),
            };
            let per_corner = callback == PrimitiveCallback::TexturedTriangleFar;
            let mut command = PrimitiveCommand::new(callback, 0, if per_corner { 5 } else { 3 });
            for (index, (x, y)) in [(0, 0), (4, 0), (4, 4)].into_iter().enumerate() {
                command.write_u32(index * 8, pack_xy(x, y));
                command.write_u32(index * 8 + 4, 64);
                command.write_byte(if per_corner { 0x48 } else { 0x28 } + index, fog);
                if per_corner {
                    command.write_rgb(0x20 + index * 4, [128, 0, 0]);
                    command.write_u32(0x2c + index * 8, [0, 65536, 65536][index]);
                    command.write_u32(0x30 + index * 8, [0, 0, 65536][index]);
                }
            }
            command.write_u32(0x18, 0);
            command.write_texture(0x1c, Some(resource));
            command.write_rgb(0x20, [128, 0, 0]);
            command.write_u32(if per_corner { 0x44 } else { 0x24 }, 0x1f);
            let mut frame = SoftwareFrame::new(6, 6, [10, 20, 30, 255]);
            let mut backend =
                SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
            backend
                .draw(
                    DrawInvocation {
                        command_offset: 0,
                        receiver_offset: 0,
                    },
                    &mut command,
                )
                .unwrap();
            assert_eq!(
                backend.frame.pixel_rgba(2, 1),
                Some(expected),
                "flags={flags:#x} fog={fog}"
            );
            assert_eq!(
                backend.frame.pixel_depth(2, 1),
                Some(if writes_depth { 1. - 1. / 64. } else { 1. })
            );
            assert_eq!(
                backend.stats.shared_shade_far_triangles,
                usize::from(!per_corner)
            );
            assert_eq!(backend.stats.textured_triangles, 1);
            assert!(backend.stats.fragments > 0);
            assert_eq!(backend.stats.unsupported, 0);
        }
    }
}

#[test]
fn ordinary_far_quad_pixels_keep_literal_fade_and_vertex_fog_policies() {
    use crate::{
        render_resources::{
            RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest,
            RenderSource,
        },
        ResidentRegistration,
    };
    use inf_formats::{
        ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
        SpriteModeSelectors, SpriteVariant,
    };
    let source = [0xff, 0x7f];
    let registration = ResidentRegistration {
        level: 0,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 0,
    };
    for (flags, fog, expected, writes_depth) in [
        (2u16, 0u8, [255, 255, 248, 255], true),
        (2, 255, [0, 0, 255, 255], true),
        // Literal255-fog yields128, not the129 of the effect-RGB kernel.
        (0x12, 127, [138, 148, 154, 255], false),
        (0x12, 255, [10, 20, 30, 255], false),
        (0x22, 255, [10, 20, 30, 0], false),
        (0x0a, 255, [5, 10, 143, 191], false),
    ] {
        let entry = SpriteEntry {
            local_index: 0,
            span: ByteSpan::new(0, 0),
            selectors: SpriteModeSelectors {
                mode0: 0,
                mode1: 0,
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
        };
        let resource = RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: 0,
            },
            source: RenderSource {
                registration: &registration,
                bytes: &source,
            },
            data: RenderResourceData::Sprite(&entry),
        };
        let mut command = PrimitiveCommand::new(PrimitiveCallback::TexturedQuadFar, 0, 3);
        for (index, (x, y)) in [(0, 0), (4, 0), (4, 4), (0, 4)].into_iter().enumerate() {
            command.write_u32(index * 8, pack_xy(x, y));
            command.write_u32(index * 8 + 4, 64);
            command.write_byte(0x2c + index, fog);
        }
        command.write_u32(0x20, 0);
        command.write_texture(0x24, Some(resource));
        command.write_u32(0x28, 0x1f);
        let mut frame = SoftwareFrame::new(6, 6, [10, 20, 30, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
        backend
            .draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )
            .unwrap();
        assert_eq!(
            backend.frame.pixel_rgba(2, 1),
            Some(expected),
            "flags{flags:#x} fog{fog}"
        );
        assert_eq!(
            backend.frame.pixel_depth(2, 1),
            Some(if writes_depth { 1. - 1. / 64. } else { 1. })
        );
        assert_eq!(backend.stats.ordinary_far_quads, 1);
        assert_eq!(backend.stats.textured_quads, 1);
        assert!(backend.stats.fragments > 0);
        assert_eq!(backend.stats.unsupported, 0);
    }
}

/// Explosion debris (`0053D4F0`) draws 250 nearer than it lies, so a piece
/// 250 from the eye reaches packet depth 0, and retail's conversion divides
/// one by it (`004D5603`). On Stendahl Alpha a piece with a zero-scale frame
/// left four equal corners at (752, 390): D3D draws no pixel of that, so the
/// flush goes on, whatever its corners' depths. A sized piece draws nothing
/// too, as an approximation. A sized quad with a single corner at depth 0
/// still stops the queue.
#[test]
fn queued_quads_at_packet_depth_zero_draw_nothing_and_keep_the_queue() {
    use crate::{
        render_resources::{
            RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest,
            RenderSource,
        },
        ResidentRegistration,
    };
    use inf_formats::{
        ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
        SpriteModeSelectors, SpriteVariant,
    };
    let source = [0xff, 0x7f];
    let registration = ResidentRegistration {
        level: 0,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 0,
    };
    let entry = SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: 0,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(2),
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
    };
    let resource = RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 0,
        },
        source: RenderSource {
            registration: &registration,
            bytes: &source,
        },
        data: RenderResourceData::Sprite(&entry),
    };
    let point = [(3, 2); 4];
    let sized = [(0, 0), (4, 0), (4, 4), (0, 4)];
    for (corners, depths, drawn) in [
        (point, [0; 4], true),
        (point, [0, 64, 64, 64], true),
        (sized, [0; 4], true),
        (sized, [0, 64, 64, 64], false),
    ] {
        let mut command =
            PrimitiveCommand::new(PrimitiveCallback::TexturedQuadNear, 0xc000_0000, 3);
        for (index, (x, y)) in corners.into_iter().enumerate() {
            command.write_u32(index * 8, pack_xy(x, y));
            command.write_u32(index * 8 + 4, depths[index]);
        }
        command.write_u32(0x20, 0);
        command.write_texture(0x24, Some(resource));
        let mut queue = DrawQueue::new(4096).unwrap();
        queue
            .submit(DrawSubmission {
                depth: 0xc000_0000,
                payload_blocks: 3,
                receiver_offset: 0,
                command,
            })
            .unwrap();
        queue.end_group().unwrap();
        let mut frame = SoftwareFrame::new(6, 6, [10, 20, 30, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
        let result = flush_pixels(&mut queue, &mut backend);
        if drawn {
            assert_eq!(result, Ok(1), "corners {corners:?}");
            assert!(!queue.state().stopped);
            assert_eq!(backend.stats.textured_quads, 1);
        } else {
            assert_eq!(
                result,
                Err(DrawQueueError::Host(SoftwarePixelError::NonFinite))
            );
            assert!(queue.state().stopped);
        }
        assert_eq!(backend.stats.fragments, 0);
        assert!(backend
            .frame
            .rgba()
            .chunks(4)
            .all(|pixel| pixel == [10, 20, 30, 255]));
    }
}

#[test]
fn flat_packet_reads_exact_geometry_material_and_rgb() {
    let command = flat_command(
        PrimitiveCallback::FlatTriangleNear,
        3,
        [(10, 20, 100), (30, 40, 200), (50, 60, u32::MAX)],
        7,
        0x0421,
        [[10, 20, 30], [40, 50, 60], [70, 80, 90]],
    );
    let triangle = read_flat_triangle(&command).unwrap();
    assert_eq!(
        triangle.vertices,
        [
            FlatVertex {
                x: 10.0,
                y: 20.0,
                depth: 100
            },
            FlatVertex {
                x: 30.0,
                y: 40.0,
                depth: 200
            },
            FlatVertex {
                x: 50.0,
                y: 60.0,
                depth: -1
            },
        ]
    );
    assert_eq!(triangle.mode, 7);
    assert_eq!(triangle.material, MaterialValue::from_raw(0x0421));
    assert_eq!(triangle.rgb, [[10, 20, 30], [40, 50, 60], [70, 80, 90]]);
}

#[test]
fn flat_packet_with_unknown_byte_or_wrong_shape_fails_closed() {
    let mut command = near_flat([(0, 0, 10), (4, 0, 10), (0, 4, 10)], 0x7fff);
    command.payload[0x21] = None;
    assert!(matches!(
        read_flat_triangle(&command),
        Err(SoftwarePixelError::Unknown { .. })
    ));

    let wide = flat_command(
        PrimitiveCallback::FlatTriangleNear,
        5,
        [(0, 0, 10), (4, 0, 10), (0, 4, 10)],
        0,
        0,
        [[0; 3]; 3],
    );
    assert!(matches!(
        read_flat_triangle(&wide),
        Err(SoftwarePixelError::Blocks { .. })
    ));

    let far = flat_command(
        PrimitiveCallback::FlatTriangleFar,
        4,
        [(0, 0, 10), (4, 0, 10), (0, 4, 10)],
        0,
        0,
        [[0; 3]; 3],
    );
    assert!(matches!(
        read_flat_triangle(&far),
        Err(SoftwarePixelError::Unsupported { .. })
    ));
}

#[test]
fn flat_packet_rejects_every_truncated_length_and_extra_bytes() {
    let command = near_flat([(0, 0, 10), (4, 0, 10), (0, 4, 10)], 0x7fff);
    for actual in 0..48 {
        let mut truncated = command.clone();
        truncated.payload.truncate(actual);
        assert_eq!(
            read_flat_triangle(&truncated),
            Err(SoftwarePixelError::PayloadLength {
                expected: 48,
                actual,
            }),
            "payload length {actual}"
        );
    }
    let mut extra = command;
    extra.payload.push(None);
    assert_eq!(
        read_flat_triangle(&extra),
        Err(SoftwarePixelError::PayloadLength {
            expected: 48,
            actual: 49,
        })
    );
}

#[test]
fn flat_packet_ignores_unwritten_rgb_high_bytes_and_padding() {
    let mut command = near_flat([(0, 0, 10), (4, 0, 10), (0, 4, 10)], 0x7fff);
    let expected = read_flat_triangle(&command).unwrap();
    // Each RGB high byte and the final four padding bytes remain unwritten
    // by the producer. Neither unknown nor arbitrary values are pixel inputs.
    for offset in [0x23, 0x27, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f] {
        assert_eq!(command.payload[offset], None);
        command.payload[offset] = Some(offset as u8);
    }
    assert_eq!(read_flat_triangle(&command).unwrap(), expected);
}

#[test]
fn right_triangle_covers_exact_pixels_with_material_color() {
    let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
    let command = near_flat([(0, 0, 50), (4, 0, 50), (0, 4, 50)], 0x7fff);
    let triangle = read_flat_triangle(&command).unwrap();
    let fragments = fill(&mut frame, &triangle);
    assert!(fragments > 0);
    // Integer pixel centers are covered with the native shifted material
    // white, modulated by the explicit neutral shade 128.
    assert_eq!(frame.pixel_rgba(0, 0), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(1, 1), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_depth(0, 0), Some(0.98));
    // The top-left rule excludes pixel centers on the hypotenuse.
    assert_eq!(frame.pixel_rgba(3, 0), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(3, 1), Some([0, 0, 0, 255]));
    assert_eq!(frame.pixel_rgba(3, 3), Some([0, 0, 0, 255]));
    assert_eq!(frame.pixel_rgba(7, 7), Some([0, 0, 0, 255]));
}

#[test]
fn explicit_material_shifts_widen_five_bit_channels_without_bit_replication() {
    // The explicit [6,1,4] display shifts widen low-word (1,1,1) to 8.
    let mut frame = SoftwareFrame::new(4, 4, [0, 0, 0, 255]);
    let command = near_flat([(0, 0, 5), (3, 0, 5), (0, 3, 5)], 0x0421);
    let triangle = read_flat_triangle(&command).unwrap();
    fill(&mut frame, &triangle);
    assert_eq!(frame.pixel_rgba(0, 0), Some([8, 8, 8, 255]));
}

#[test]
fn lequal_keeps_nearer_and_coplanar_overwrites_but_rejects_farther() {
    let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
    let white = near_flat([(0, 0, 50), (4, 0, 50), (0, 4, 50)], 0x7fff);
    let grey = near_flat([(0, 0, 50), (4, 0, 50), (0, 4, 50)], 0x0421);
    let far = near_flat([(0, 0, 60), (4, 0, 60), (0, 4, 60)], 0x7fff);
    let near = near_flat([(0, 0, 40), (4, 0, 40), (0, 4, 40)], 0x7c00);

    fill(&mut frame, &read_flat_triangle(&white).unwrap());
    assert_eq!(frame.pixel_rgba(0, 0), Some([248, 248, 248, 255]));
    // A coplanar later command decorates the earlier base polygon.
    fill(&mut frame, &read_flat_triangle(&grey).unwrap());
    assert_eq!(frame.pixel_rgba(0, 0), Some([8, 8, 8, 255]));
    // A farther command cannot overwrite the nearer stored depth.
    fill(&mut frame, &read_flat_triangle(&far).unwrap());
    assert_eq!(frame.pixel_rgba(0, 0), Some([8, 8, 8, 255]));
    assert_eq!(frame.pixel_depth(0, 0), Some(0.98));
    // A nearer command replaces both color and transformed depth.
    fill(&mut frame, &read_flat_triangle(&near).unwrap());
    assert_eq!(frame.pixel_rgba(0, 0), Some([248, 0, 0, 255]));
    assert_eq!(frame.pixel_depth(0, 0), Some(0.975));
}

#[test]
fn interpolated_depths_stay_within_vertex_range() {
    let mut frame = SoftwareFrame::new(16, 16, [0, 0, 0, 255]);
    let command = near_flat([(0, 0, 100), (15, 0, 200), (0, 15, 300)], 0x7fff);
    let triangle = read_flat_triangle(&command).unwrap();
    let fragments = fill(&mut frame, &triangle);
    assert!(fragments > 0);
    for y in 0..16 {
        for x in 0..16 {
            if frame.pixel_rgba(x, y) != Some([0, 0, 0, 255]) {
                let depth = frame.pixel_depth(x, y).unwrap();
                assert!(
                    (0.99..=1.0 - 1.0 / 300.0).contains(&depth),
                    "{x},{y} depth {depth}"
                );
            }
        }
    }
}

#[test]
fn out_of_bounds_triangles_clip_to_the_viewport() {
    let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
    let command = near_flat([(-10, -10, 9), (10, 0, 9), (0, 10, 9)], 0x7fff);
    let triangle = read_flat_triangle(&command).unwrap();
    let fragments = fill(&mut frame, &triangle);
    assert!(fragments > 0);
    assert_eq!(frame.pixel_depth(0, 0), Some(1.0 - 1.0 / 9.0));

    let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
    let outside = near_flat([(100, 100, 9), (104, 100, 9), (100, 104, 9)], 0x7fff);
    let triangle = read_flat_triangle(&outside).unwrap();
    assert_eq!(fill(&mut frame, &triangle), 0);
    assert_eq!(frame.pixel_rgba(7, 7), Some([1, 2, 3, 255]));
}

#[test]
fn backend_rejects_malformed_far_effect_packets_without_touching_pixels() {
    let mut frame = SoftwareFrame::new(4, 4, [9, 9, 9, 255]);
    let mut backend =
        SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
    let mut effect = PrimitiveCommand::new(PrimitiveCallback::EffectQuadFar, 10, 3);
    let error = backend
        .draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0,
            },
            &mut effect,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        SoftwarePixelError::Effect(crate::d3d_effect::EffectPixelError::Packet)
    ));
    assert_eq!(backend.stats.unsupported, 0);
    assert_eq!(backend.stats.triangles, 0);
    assert_eq!(backend.frame.pixel_rgba(0, 0), Some([9, 9, 9, 255]));
}

#[test]
fn queued_flat_triangles_flush_into_the_frame_in_depth_order() {
    let mut queue = DrawQueue::new(4096).unwrap();
    // Farther white triangle submitted first, nearer green second; the
    // sorted replay draws white before green so green wins the overlap.
    for (depth, material) in [(60u32, 0x7fff), (20u32, 0x03e0)] {
        let command = near_flat([(0, 0, depth), (6, 0, depth), (0, 6, depth)], material);
        queue
            .submit(DrawSubmission {
                depth,
                payload_blocks: 3,
                receiver_offset: 0,
                command,
            })
            .unwrap();
    }
    queue.end_group().unwrap();

    let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
    let mut backend =
        SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
    let completed = flush_pixels(&mut queue, &mut backend).unwrap();
    assert_eq!(completed, 2);
    assert_eq!(backend.stats.triangles, 2);
    assert!(backend.stats.fragments > 0);
    // The explicit material shifts map 0x03E0 to green 248. Depth is
    // transformed to 1 - near/depth before barycentric interpolation.
    assert_eq!(backend.frame.pixel_rgba(0, 0), Some([0, 248, 0, 255]));
    let depth = backend.frame.pixel_depth(0, 0).unwrap();
    assert!((depth - 0.95).abs() < 1e-3, "overlap depth {depth}");
}

/// D8's two-block packet reaches the solid triangle consumer: the material
/// alone colors it, with the solid callbacks' low-bit replication (green
/// 0x03E0 is 255 here, 248 through the shaded flat triangle).
#[test]
fn a_queued_solid_triangle_draws_its_material_color() {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::SolidTriangleNear, 30, 2);
    for (corner, (x, y)) in [(0i16, 0i16), (6, 0), (0, 6)].into_iter().enumerate() {
        command.write_u32(corner * 8, u32::from(x as u16) | u32::from(y as u16) << 16);
        command.write_u32(corner * 8 + 4, 30);
    }
    command.write_u32(0x18, 0x20);
    command.write_u32(0x1c, 0x03e0);
    let mut queue = DrawQueue::new(4096).unwrap();
    queue
        .submit(DrawSubmission {
            depth: 30,
            payload_blocks: 2,
            receiver_offset: 0,
            command,
        })
        .unwrap();
    queue.end_group().unwrap();
    let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
    let mut backend =
        SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
    assert_eq!(flush_pixels(&mut queue, &mut backend).unwrap(), 1);
    assert_eq!(backend.stats.solid_triangles, 1);
    assert!(backend.stats.fragments > 0);
    assert_eq!(backend.frame.pixel_rgba(0, 0), Some([0, 255, 0, 255]));
}

#[test]
fn invalid_queued_packet_stops_replay_and_retains_prior_pixels() {
    let cases = [
        (
            32,
            0,
            SoftwarePixelError::PayloadLength {
                expected: 48,
                actual: 32,
            },
        ),
        (
            49,
            0,
            SoftwarePixelError::PayloadLength {
                expected: 48,
                actual: 49,
            },
        ),
        (48, 4, SoftwarePixelError::Receiver { offset: 4 }),
        (
            48,
            u32::MAX,
            SoftwarePixelError::Receiver { offset: u32::MAX },
        ),
    ];
    for (payload_length, receiver_offset, expected_error) in cases {
        let valid = near_flat([(0, 0, 30), (6, 0, 30), (0, 6, 30)], 0x7fff);
        let mut invalid = near_flat([(0, 0, 20), (6, 0, 20), (0, 6, 20)], 0x03e0);
        invalid.payload.resize(payload_length, None);
        let skipped = near_flat([(0, 0, 10), (6, 0, 10), (0, 6, 10)], 0x7c00);
        let mut expected_frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let expected_fragments = fill(&mut expected_frame, &read_flat_triangle(&valid).unwrap());
        assert!(expected_fragments > 0);
        let mut queue = DrawQueue::new(4096).unwrap();
        for (command, receiver_offset) in [(valid, 0), (invalid, receiver_offset), (skipped, 0)] {
            queue
                .submit(DrawSubmission {
                    depth: command.depth,
                    payload_blocks: command.payload_blocks,
                    receiver_offset,
                    command,
                })
                .unwrap();
        }
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend =
            SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Host(expected_error))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 1);
        assert_eq!(backend.stats.triangles, 1);
        assert_eq!(backend.stats.fragments, expected_fragments);
        assert_eq!(backend.frame.rgba, expected_frame.rgba);
        assert_eq!(backend.frame.depth, expected_frame.depth);
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
        );
        assert_eq!(backend.stats.triangles, 1);
        assert_eq!(backend.frame.rgba, expected_frame.rgba);
        assert_eq!(backend.frame.depth, expected_frame.depth);
    }
}

pub(super) fn texture_state() -> SoftwareTextureState {
    SoftwareTextureState {
        upload: crate::d3d_texture::NativeTextureUploadState {
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
        secondary_texture: false,
    }
}

#[test]
fn replace_surface_copies_matching_rgba_and_rejects_mismatch() {
    let mut frame = SoftwareFrame::new(2, 1, [0, 0, 0, 255]);
    assert!(frame.replace_surface(&[1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(frame.rgba(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(!frame.replace_surface(&[1, 2, 3]));
    assert_eq!(frame.rgba(), &[1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn fit_surface_keeps_the_source_aspect_and_rejects_bad_input() {
    // 2x1 red,green fitted onto 4x2, the same aspect: each pixel covers 2x2.
    let src = [255, 0, 0, 255, 0, 255, 0, 255];
    let mut frame = SoftwareFrame::new(4, 2, [0, 0, 0, 255]);
    assert!(frame.fit_surface(2, 1, &src));
    assert_eq!(
        frame.rgba(),
        &[
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, //
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
        ]
    );
    // Same-size fit matches a replace.
    let mut frame = SoftwareFrame::new(2, 1, [0, 0, 0, 255]);
    assert!(frame.fit_surface(2, 1, &src));
    assert_eq!(frame.rgba(), &src);
    // A 1x1 source in a 3x1 frame keeps its square: black sides.
    let mut frame = SoftwareFrame::new(3, 1, [9, 9, 9, 255]);
    assert!(frame.fit_surface(1, 1, &[7, 8, 9, 255]));
    assert_eq!(frame.rgba(), &[0, 0, 0, 255, 7, 8, 9, 255, 0, 0, 0, 255]);
    // A 4:3 640x480 picture in a 1365x768 widescreen frame: 1024 wide, centred.
    let picture = [5u8, 6, 7, 255].repeat(640 * 480);
    let mut frame = SoftwareFrame::new(1365, 768, [9, 9, 9, 255]);
    assert!(frame.fit_surface(640, 480, &picture));
    assert_eq!(frame.pixel_rgba(169, 0), Some([0, 0, 0, 255]));
    assert_eq!(frame.pixel_rgba(170, 0), Some([5, 6, 7, 255]));
    assert_eq!(frame.pixel_rgba(1193, 767), Some([5, 6, 7, 255]));
    assert_eq!(frame.pixel_rgba(1194, 767), Some([0, 0, 0, 255]));
    // Zero dimensions and short input fail closed without touching pixels.
    let mut frame = SoftwareFrame::new(2, 1, [9, 9, 9, 255]);
    assert!(!frame.fit_surface(0, 1, &src));
    assert!(!frame.fit_surface(2, 1, &[1, 2, 3]));
    assert_eq!(frame.rgba(), &[9, 9, 9, 255, 9, 9, 9, 255]);
}

#[test]
fn far_flat_packet_reads_its_fourth_block_and_rejects_the_near_shape() {
    let mut command = flat_command(
        PrimitiveCallback::FlatTriangleFar,
        4,
        [(1, 2, 64), (3, 4, 65), (5, 6, 66)],
        0x22,
        0x7fff,
        [[10, 20, 30], [40, 50, 60], [70, 80, 90]],
    );
    command.write_u32(0x2c, 0x1234_5678);
    for (index, fog) in [9u8, 128, 255].into_iter().enumerate() {
        command.write_byte(0x30 + index, fog);
    }
    let (triangle, fog) = read_flat_triangle_far(&command).unwrap();
    assert_eq!(
        fog,
        FlatFog {
            color: 0x1234_5678,
            fog: [9, 128, 255]
        }
    );
    assert_eq!(triangle.mode, 0x22);
    assert_eq!(triangle.rgb[2], [70, 80, 90]);
    assert_eq!(triangle.vertices[1].depth, 65);
    // The near reader keeps rejecting the far callback, and vice versa.
    assert!(matches!(
        read_flat_triangle(&command),
        Err(SoftwarePixelError::Unsupported { .. })
    ));
    let mut missing = command.clone();
    missing.payload[0x32] = None;
    assert!(matches!(
        read_flat_triangle_far(&missing),
        Err(SoftwarePixelError::Unknown { offset: 0x32, .. })
    ));
}

fn render_one(command: PrimitiveCommand<'static>) -> (Vec<u8>, usize) {
    let mut queue = DrawQueue::new(4096).unwrap();
    queue
        .submit(DrawSubmission {
            depth: command.depth,
            payload_blocks: command.payload_blocks,
            receiver_offset: 0,
            command,
        })
        .unwrap();
    queue.end_group().unwrap();
    let mut frame = SoftwareFrame::new(8, 8, [0, 0, 0, 255]);
    let mut backend =
        SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), texture_state());
    assert_eq!(flush_pixels(&mut queue, &mut backend).unwrap(), 1);
    let triangles = backend.stats.triangles;
    (frame.rgba().to_vec(), triangles)
}

#[test]
fn dc_packets_render_as_the_flat_triangle_with_one_shade() {
    let corners = [(0, 0, 40), (7, 0, 41), (0, 7, 42)];
    let shade = [10, 120, 230];
    let dc = |callback| {
        let mut command = flat_command(callback, 3, corners, 0x20, 0x7fff, [[0; 3]; 3]);
        // DC stores one shade DWORD at +20; +24.. belong to Far only.
        for offset in 0x24..0x30 {
            command.payload[offset] = None;
        }
        command.write_rgb(0x20, shade);
        command
    };
    let near = dc(PrimitiveCallback::DcTriangleNear);
    let (triangle, fog) = read_dc_triangle(&near).unwrap();
    assert_eq!((triangle.rgb, fog), ([shade; 3], None));
    let flat = flat_command(
        PrimitiveCallback::FlatTriangleNear,
        3,
        corners,
        0x20,
        0x7fff,
        [shade; 3],
    );
    let (pixels, triangles) = render_one(near.clone());
    assert_eq!(pixels, render_one(flat).0);
    assert!(pixels.chunks(4).any(|pixel| pixel != [0, 0, 0, 255]));
    assert_eq!(triangles, 1);

    let mut far = dc(PrimitiveCallback::DcTriangleFar);
    far.write_u32(0x24, 0x0040_6080);
    for (index, value) in [0u8, 128, 255].into_iter().enumerate() {
        far.write_byte(0x28 + index, value);
    }
    let mut flat_far = flat_command(
        PrimitiveCallback::FlatTriangleFar,
        4,
        corners,
        0x20,
        0x7fff,
        [shade; 3],
    );
    flat_far.write_u32(0x2c, 0x0040_6080);
    for (index, value) in [0u8, 128, 255].into_iter().enumerate() {
        flat_far.write_byte(0x30 + index, value);
    }
    let (_, far_fog) = read_dc_triangle(&far).unwrap();
    assert_eq!(
        far_fog,
        Some(FlatFog {
            color: 0x0040_6080,
            fog: [0, 128, 255]
        })
    );
    assert_eq!(render_one(far.clone()).0, render_one(flat_far).0);

    // Unknown required bytes and foreign shapes fail closed.
    let mut missing = near.clone();
    missing.payload[0x22] = None;
    assert!(matches!(
        read_dc_triangle(&missing),
        Err(SoftwarePixelError::Unknown { offset: 0x22, .. })
    ));
    let mut fogless = far.clone();
    fogless.payload[0x2a] = None;
    assert!(matches!(
        read_dc_triangle(&fogless),
        Err(SoftwarePixelError::Unknown { offset: 0x2a, .. })
    ));
    let mut blocks = near.clone();
    blocks.payload_blocks = 4;
    assert!(matches!(
        read_dc_triangle(&blocks),
        Err(SoftwarePixelError::Blocks { .. })
    ));
    assert!(matches!(
        read_dc_triangle(&flat_command(
            PrimitiveCallback::FlatTriangleNear,
            3,
            corners,
            0x20,
            0x7fff,
            [shade; 3],
        )),
        Err(SoftwarePixelError::Unsupported { .. })
    ));
}

#[test]
fn a_canvas_lays_out_four_three_and_draws_at_its_origin() {
    // A 16x6 frame is wider than 4:3: its canvas is 8x6, from x = 4.
    let mut frame = SoftwareFrame::new(16, 6, [0, 0, 0, 255]);
    let canvas = frame.four_three_canvas().unwrap();
    assert_eq!(
        canvas,
        FrameCanvas {
            origin: [4, 0],
            size: [8, 6]
        }
    );
    frame.set_canvas(Some(canvas));
    assert_eq!([frame.width(), frame.height()], [8, 6]);
    assert_eq!(frame.buffer_size(), [16, 6]);
    let command = near_flat([(0, 0, 50), (4, 0, 50), (0, 4, 50)], 0x7fff);
    fill(&mut frame, &read_flat_triangle(&command).unwrap());
    // Canvas pixel (0, 0) is frame pixel (4, 0); the frame's own left edge
    // stays clear.
    assert_eq!(frame.pixel_rgba(4, 0), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(5, 1), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(0, 0), Some([0, 0, 0, 255]));
    assert_eq!(frame.pixel_rgba(3, 0), Some([0, 0, 0, 255]));
    frame.set_canvas(None);
    assert_eq!([frame.width(), frame.height()], [16, 6]);
    // The same draw without a canvas lands at the frame's own origin.
    fill(&mut frame, &read_flat_triangle(&command).unwrap());
    assert_eq!(frame.pixel_rgba(0, 0), Some([248, 248, 248, 255]));
    // A 4:3 frame has no canvas to draw on.
    assert_eq!(SoftwareFrame::new(8, 6, [0; 4]).four_three_canvas(), None);
}

#[test]
fn a_canvas_clips_to_its_own_width() {
    let mut frame = SoftwareFrame::new(16, 6, [0, 0, 0, 255]);
    frame.set_canvas(frame.four_three_canvas());
    // From canvas x = 6 to 12: the canvas ends at 8, frame x = 12.
    let mut command = near_flat([(6, 0, 50), (12, 0, 50), (6, 4, 50)], 0x7fff);
    // Mode 0 clips against the viewport (0x20 would bypass it).
    command.write_u32(0x18, 0);
    fill(&mut frame, &read_flat_triangle(&command).unwrap());
    assert_eq!(frame.pixel_rgba(10, 0), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(11, 0), Some([248, 248, 248, 255]));
    assert_eq!(frame.pixel_rgba(12, 0), Some([0, 0, 0, 255]));
    assert_eq!(frame.pixel_rgba(13, 0), Some([0, 0, 0, 255]));
}

/// Port option: the world's wobble under the water is V2000's (OpenV2K's
/// `retail_underwater_offset` test vectors), from Infestation's sine table.
#[test]
fn the_underwater_wobble_matches_v2000s_cardinal_phases() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root)
        .expect("retail Infest.exe is required; copy the game into retail/");
    let sine = &assets.sine_table().values;
    let wet = WetFrame::new(0, sine, 480, [1.0; 3]);
    assert_eq!(wet.offsets[0], [1, 0]);
    assert_eq!(wet.offsets[16], [0, 1]);
    assert_eq!(wet.offsets[32], [-2, 0]);
    assert_eq!(wet.offsets[48], [0, -2]);
    // The phase wraps every 128 ticks, as retail's 16-bit angle does.
    assert_eq!(WetFrame::new(128, sine, 480, [1.0; 3]).offsets, wet.offsets);
    for tick in [1, 63, 127, u32::MAX] {
        for [x, y] in WetFrame::new(tick, sine, 480, [1.0; 3]).offsets {
            assert!((-2..=1).contains(&x) && (-2..=1).contains(&y));
        }
    }
    // A 4K frame moves its vertices in 480-line pixels, and dims them.
    let wet = WetFrame::new(0, sine, 2160, [0.5, 0.5, 1.0]);
    let vertex = D3dTlVertex {
        x: 0.0,
        y: 0.0,
        depth: 0.5,
        rhw: 0.01,
        diffuse: [200, 100, 50, 255],
        specular: 0,
        uv: [0.0; 2],
    };
    let moved = wet.apply(vertex);
    assert_eq!((moved.x, moved.y), (4.5, 0.0));
    assert_eq!(moved.diffuse, [100, 50, 50, 255]);
    assert_eq!(moved.depth, 0.5);
}
