use super::*;
use crate::{
    d3d_effect::EffectPixelError,
    d3d_texture::TextureUploadError,
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 0,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 0,
};

fn sprite(flags: u16, selectors: [u8; 2]) -> SpriteEntry {
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

fn effect<'a>(
    entry: &'a SpriteEntry,
    callback: PrimitiveCallback,
    mode: u32,
) -> PrimitiveCommand<'a> {
    let far = callback == PrimitiveCallback::EffectQuadFar;
    let mut command = PrimitiveCommand::new(callback, 20, if far { 4 } else { 3 });
    for (i, [x, y]) in [[0, 0], [6, 0], [6, 6], [0, 6]].into_iter().enumerate() {
        command.write_u32(i * 8, pack_xy(x, y));
        command.write_u32(i * 8 + 4, 20);
    }
    command.write_u32(0x20, mode);
    command.write_texture(
        0x24,
        Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: 0,
            },
            source: RenderSource {
                registration: &REGISTRATION,
                bytes: &[255, 127],
            },
            data: RenderResourceData::Sprite(entry),
        }),
    );
    command.write_rgb(0x28, [64, 32, 16]);
    if far {
        command.write_u32(0x2c, 0x7c1f);
        for i in 0..4 {
            command.write_byte(0x30 + i, 0);
        }
    }
    command
}

/// An 8x8 screen quad at one depth over `bytes`, the world flash's shape,
/// with packet+20 `mode` or left unknown.
fn screen_effect<'a>(
    entry: &'a SpriteEntry,
    bytes: &'a [u8],
    mode: Option<u32>,
) -> PrimitiveCommand<'a> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadNear, 20, 3);
    for (i, [x, y]) in [[0, 0], [8, 0], [8, 8], [0, 8]].into_iter().enumerate() {
        command.write_u32(i * 8, pack_xy(x, y));
        command.write_u32(i * 8 + 4, 0x40);
    }
    if let Some(mode) = mode {
        command.write_u32(0x20, mode);
    }
    command.write_texture(
        0x24,
        Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: 0,
            },
            source: RenderSource {
                registration: &REGISTRATION,
                bytes,
            },
            data: RenderResourceData::Sprite(entry),
        }),
    );
    command.write_rgb(0x28, [64, 32, 16]);
    command
}

type Drawn = (usize, Vec<u8>, Vec<f32>);

fn draw_screen(
    entry: &SpriteEntry,
    bytes: &[u8],
    viewport: [i16; 4],
    mode: Option<u32>,
) -> Result<Drawn, SoftwarePixelError> {
    let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
    let mut backend = SoftwarePixelBackend::new(
        &mut frame,
        renderer(),
        D3dRasterState {
            viewport,
            ..pipeline()
        },
        texture_state(),
    );
    backend.draw(
        DrawInvocation {
            command_offset: 0,
            receiver_offset: 0,
        },
        &mut screen_effect(entry, bytes, mode),
    )?;
    let fragments = backend.stats.fragments;
    Ok((fragments, frame.rgba().to_vec(), frame.depth.clone()))
}

#[test]
fn unknown_mode_screen_quad_draws_as_every_mode_word_draws_it() {
    let white: &[u8] = &[255, 127];
    for flags in [0x02, 0x12] {
        let entry = sprite(flags, [0, 0]);
        let unknown = draw_screen(&entry, white, [0, 0, 8, 8], None).unwrap();
        assert_eq!(unknown.0, 64, "flags{flags:x}");
        // 004D78A0 reads bits 1, 20, 100 and 4000 of +20.
        for bits in 0..16u32 {
            let mode = [1, 0x20, 0x100, 0x4000]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| bits >> i & 1 != 0)
                .map(|(_, bit)| bit)
                .sum();
            assert_eq!(
                draw_screen(&entry, white, [0, 0, 8, 8], Some(mode)).unwrap(),
                unknown,
                "flags{flags:x} mode{mode:x}"
            );
        }
    }
    let refused = Err(SoftwarePixelError::Effect(EffectPixelError::Unknown {
        field: "mode",
        offset: 0x20,
    }));
    // The clipper would cut a corner past the bounds unless bit 20 skips it.
    let entry = sprite(0x12, [0, 0]);
    assert_eq!(draw_screen(&entry, white, [0, 0, 7, 8], None), refused);
    // Two texel colors: the diagonal and perspective move the coordinates.
    let mut two = sprite(0x12, [0, 0]);
    two.variants[0].width = 2;
    two.variants[0].encoding = SpriteEncoding::DirectRgb555 {
        pixels: ByteSpan::new(0, 4),
    };
    assert_eq!(
        draw_screen(&two, &[255, 127, 0, 0], [0, 0, 8, 8], None),
        refused
    );
}

#[test]
fn effect_secondary_boundary_retains_near_far_primary_pixels_and_device_policy() {
    for callback in [
        PrimitiveCallback::EffectQuadNear,
        PrimitiveCallback::EffectQuadFar,
    ] {
        for mode in [0, 0x20, 0x4100, 0x4020] {
            for flags in [0x202, 0x20a, 0x212, 0x222] {
                let entry = sprite(flags, [0, 0]);
                let mut expected_frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let (expected_fragments, expected_pipeline) = {
                    let mut baseline = SoftwarePixelBackend::new(
                        &mut expected_frame,
                        renderer(),
                        pipeline(),
                        texture_state(),
                    );
                    baseline
                        .draw(
                            DrawInvocation {
                                command_offset: 0,
                                receiver_offset: 0,
                            },
                            &mut effect(&entry, callback, mode),
                        )
                        .unwrap();
                    assert_eq!(baseline.stats.effect_quads, 1);
                    assert!(baseline.stats.fragments > 0);
                    (baseline.stats.fragments, baseline.pipeline)
                };
                let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
                let mut state = texture_state();
                state.secondary_texture = true;
                let mut backend =
                    SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), state);
                assert_eq!(
                    backend.draw(
                        DrawInvocation {
                            command_offset: 0,
                            receiver_offset: 0
                        },
                        &mut effect(&entry, callback, mode)
                    ),
                    Err(SoftwarePixelError::Effect(EffectPixelError::SecondaryPass))
                );
                assert_eq!(
                    backend.frame.rgba(),
                    expected_frame.rgba(),
                    "{callback:?} mode{mode:x} flags{flags:x}"
                );
                assert_eq!(backend.frame.depth, expected_frame.depth);
                assert_eq!(backend.stats.effect_quads, 1);
                assert_eq!(backend.stats.fragments, expected_fragments);
                assert_eq!(
                    backend.pipeline.depth_enabled,
                    expected_pipeline.depth_enabled
                );
                assert_eq!(backend.pipeline.depth_write, expected_pipeline.depth_write);
                assert_eq!(backend.stats.skipped, 0);
                if flags == 0x202 && mode == 0x20 {
                    assert_eq!(backend.frame.pixel_rgba(3, 2), Some([128, 64, 31, 255]));
                }
            }
        }
    }
}

#[test]
fn effect_queue_failure_keeps_prior_and_current_primary_then_stops_later_draws() {
    for callback in [
        PrimitiveCallback::EffectQuadNear,
        PrimitiveCallback::EffectQuadFar,
    ] {
        let entry = sprite(0x202, [0, 0]);
        let earlier = near_flat([(0, 0, 30), (6, 0, 30), (0, 6, 30)], 0x7fff);
        let current = effect(&entry, callback, 0x20);
        let later = near_flat([(0, 0, 10), (6, 0, 10), (0, 6, 10)], 0x03e0);
        let mut expected = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let expected_fragments = {
            let mut baseline =
                SoftwarePixelBackend::new(&mut expected, renderer(), pipeline(), texture_state());
            for mut command in [earlier.clone(), current.clone()] {
                baseline
                    .draw(
                        DrawInvocation {
                            command_offset: 0,
                            receiver_offset: 0,
                        },
                        &mut command,
                    )
                    .unwrap();
            }
            baseline.stats.fragments
        };
        let mut queue = DrawQueue::new(4096).unwrap();
        for command in [earlier, current, later] {
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
        let mut state = texture_state();
        state.secondary_texture = true;
        let mut backend = SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), state);
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Host(SoftwarePixelError::Effect(
                EffectPixelError::SecondaryPass
            )))
        );
        assert!(queue.state().stopped);
        assert_eq!(queue.state().last_flush_completed, 1);
        assert_eq!(backend.stats.triangles, 1);
        assert_eq!(backend.stats.effect_quads, 1);
        assert_eq!(backend.stats.fragments, expected_fragments);
        assert_eq!(backend.frame.rgba(), expected.rgba());
        assert_eq!(backend.frame.depth, expected.depth);
        assert_eq!(
            flush_pixels(&mut queue, &mut backend),
            Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
        );
        assert_eq!(backend.stats.effect_quads, 1);
        assert_eq!(backend.frame.rgba(), expected.rgba());
    }
}

#[test]
fn effect_primary_admission_precedes_secondary_and_ff_still_suppresses_all_work() {
    for callback in [
        PrimitiveCallback::EffectQuadNear,
        PrimitiveCallback::EffectQuadFar,
    ] {
        let entry = sprite(0x202, [0, 0]);
        for invalid_upload in [false, true] {
            let mut command = effect(&entry, callback, 0x20);
            let mut state = texture_state();
            state.secondary_texture = true;
            let expected = if invalid_upload {
                state.upload.global_policy = 0;
                SoftwarePixelError::Upload(TextureUploadError::GlobalPolicy(0))
            } else {
                command.write_u32(4, 0);
                SoftwarePixelError::NonFinite
            };
            let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
            let mut backend = SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), state);
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
            assert_eq!(backend.stats.effect_quads, 0);
            assert_eq!(backend.stats.fragments, 0);
            assert_eq!(backend.frame.pixel_rgba(3, 2), Some([1, 2, 3, 255]));
        }
        let entry = sprite(0x202, [255, 255]);
        let mut command = effect(&entry, callback, 0x20);
        for offset in (0..0x20).chain(0x28..command.payload.len()) {
            command.payload[offset] = None;
        }
        let mut state = texture_state();
        state.secondary_texture = true;
        state.upload.global_policy = 0;
        let mut frame = SoftwareFrame::new(8, 8, [1, 2, 3, 255]);
        let mut backend = SoftwarePixelBackend::new(&mut frame, renderer(), pipeline(), state);
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
        assert_eq!(backend.stats.effect_quads, 0);
        assert!(backend.textures.is_empty());
        assert_eq!(backend.frame.pixel_rgba(3, 2), Some([1, 2, 3, 255]));
    }
}
