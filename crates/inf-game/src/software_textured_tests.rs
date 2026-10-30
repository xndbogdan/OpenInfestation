use super::*;
use crate::{
    d3d_textured::{D3dTexturePolicy, D3dTexturedSubmission},
    d3d_viewport::D3dTlVertex,
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteRenderMode, SpriteVariant,
};

fn with_texture(pixels: [u16; 4], f: impl FnOnce(&crate::d3d_texture::CpuTexture)) {
    let source: Vec<u8> = pixels.into_iter().flat_map(u16::to_le_bytes).collect();
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
            width: 2,
            height: 2,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 8),
            },
        }],
    };
    let registration = ResidentRegistration {
        level: 0,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 0,
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
    let mut store = CpuTextureStore::new();
    f(store
        .resolve(
            resource,
            SpriteRenderMode::Mode0,
            tests::texture_state().upload,
        )
        .unwrap()
        .unwrap());
}

fn submission() -> D3dTexturedSubmission {
    D3dTexturedSubmission {
        vertices: [
            (0., 0., 1., 1.),
            (4., 0., 3., 1.),
            (4., 4., 3., 3.),
            (0., 4., 1., 3.),
        ]
        .map(|(x, y, u, v)| D3dTlVertex {
            x,
            y,
            depth: 0.5,
            rhw: 1. / 128.,
            diffuse: [255; 4],
            specular: 0xff00_0000,
            uv: [u / 256., v / 256.],
        }),
        triangles: [[1, 2, 3], [1, 3, 0]],
        policy: D3dTexturePolicy {
            depth_enable: Some(1),
            depth_write: Some(1),
            blend: D3dTextureBlend::Copy,
            perspective: true,
            fog: crate::d3d_textured::D3dTextureFog::Disabled,
        },
        mode: 0,
        near_depth: 64,
    }
}

fn pipeline() -> D3dRasterState {
    D3dRasterState {
        viewport: [0, 0, 5, 5],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    }
}

#[test]
fn textured_quad_uses_native_uv_corners_and_covers_each_shared_edge_once() {
    with_texture([0x7c00, 0x03e0, 0x001f, 0x7fff], |texture| {
        for reverse in [false, true] {
            for diagonal in [[[1, 2, 3], [1, 3, 0]], [[0, 1, 2], [0, 2, 3]]] {
                let mut quad = submission();
                quad.triangles = diagonal;
                if reverse {
                    for t in &mut quad.triangles {
                        t.swap(1, 2);
                    }
                }
                quad.policy.blend = D3dTextureBlend::Additive;
                let mut frame = SoftwareFrame::new(6, 6, [0; 4]);
                assert_eq!(
                    frame
                        .fill_textured_submission(
                            &quad,
                            texture,
                            &mut pipeline(),
                            tests::texture_state().sampling
                        )
                        .unwrap(),
                    16
                );
                for y in 0..6 {
                    for x in 0..6 {
                        let expected = if x < 4 && y < 4 {
                            texture.texel(1 + x / 2, 1 + y / 2).unwrap()
                        } else {
                            [0; 4]
                        };
                        assert_eq!(frame.pixel_rgba(x, y), Some(expected), "pixel {x},{y}");
                    }
                }
            }
        }
    });
}

#[test]
fn bilinear_sampling_uses_half_texel_centers_and_duplicated_borders() {
    with_texture([0x7c00, 0x03e0, 0x001f, 0x7fff], |texture| {
        let mut sampling = tests::texture_state().sampling;
        sampling.min_filter = 2;
        sampling.mag_filter = 2;
        let mut frame = SoftwareFrame::new(6, 6, [0; 4]);
        frame
            .fill_textured_submission(&submission(), texture, &mut pipeline(), sampling)
            .unwrap();
        assert_eq!(frame.pixel_rgba(0, 0), texture.texel(1, 1));
        assert_eq!(frame.pixel_rgba(1, 1), texture.texel(1, 1));
        let average = std::array::from_fn(|c| {
            let sum: u32 = [(1, 1), (2, 1), (1, 2), (2, 2)]
                .into_iter()
                .map(|(x, y)| u32::from(texture.texel(x, y).unwrap()[c]))
                .sum();
            (f64::from(sum) / 4.).round_ties_even() as u8
        });
        assert_eq!(frame.pixel_rgba(2, 2), Some(average));
        assert_eq!(frame.pixel_rgba(3, 3), texture.texel(2, 2));
    });
}

#[test]
fn perspective_and_affine_paths_diverge_with_sloping_reciprocal_depth() {
    with_texture([0x7c00, 0x001f, 0x7c00, 0x001f], |texture| {
        for perspective in [true, false] {
            let mut quad = submission();
            quad.policy.perspective = perspective;
            for i in [0, 3] {
                quad.vertices[i].rhw = 1. / 64.;
                quad.vertices[i].depth = 0.;
            }
            for i in [1, 2] {
                quad.vertices[i].rhw = 1. / 256.;
                quad.vertices[i].depth = 0.75;
            }
            let mut frame = SoftwareFrame::new(6, 6, [0; 4]);
            frame
                .fill_textured_submission(
                    &quad,
                    texture,
                    &mut pipeline(),
                    tests::texture_state().sampling,
                )
                .unwrap();
            assert_eq!(
                frame.pixel_rgba(2, 1),
                texture.texel(if perspective { 1 } else { 2 }, 1)
            );
            assert_eq!(frame.pixel_depth(2, 1), Some(0.375));
        }
    });
}

#[test]
fn blend_factors_respect_diffuse_alpha_and_inherited_alpha_enable() {
    with_texture([0x4210; 4], |texture| {
        let src = texture.texel(1, 1).unwrap();
        let dst = [50, 100, 150, 200];
        for blend in [
            D3dTextureBlend::Copy,
            D3dTextureBlend::Additive,
            D3dTextureBlend::InverseSourceColor,
            D3dTextureBlend::SourceAlpha,
        ] {
            for enabled in [true, false] {
                let mut quad = submission();
                quad.policy.blend = blend;
                quad.policy.depth_write = Some(0);
                for v in &mut quad.vertices {
                    v.diffuse[3] = 128;
                }
                let mut state = pipeline();
                state.alpha_blend_enabled = enabled;
                let mut frame = SoftwareFrame::new(6, 6, dst);
                frame
                    .fill_textured_submission(
                        &quad,
                        texture,
                        &mut state,
                        tests::texture_state().sampling,
                    )
                    .unwrap();
                let mut modulated = src;
                modulated[3] = 128;
                let expected = std::array::from_fn(|c| {
                    if !enabled {
                        return modulated[c];
                    }
                    let s = u32::from(modulated[c]);
                    let d = u32::from(dst[c]);
                    match blend {
                        D3dTextureBlend::Copy => s as u8,
                        D3dTextureBlend::Additive => (s + d).min(255) as u8,
                        D3dTextureBlend::InverseSourceColor => ((d * (255 - s) + 127) / 255) as u8,
                        D3dTextureBlend::SourceAlpha => ((s * 128 + d * 127 + 127) / 255) as u8,
                    }
                });
                assert_eq!(frame.pixel_rgba(1, 1), Some(expected));
                assert_eq!(frame.pixel_depth(1, 1), Some(1.));
            }
        }
    });
}

#[test]
fn vertex_fog_interpolates_specular_alpha_in_screen_space_across_clip_and_winding() {
    with_texture([0x7fff; 4], |texture| {
        for reverse in [false, true] {
            for diagonal in [[[1, 2, 3], [1, 3, 0]], [[0, 1, 2], [0, 2, 3]]] {
                for viewport in [[0, 0, 5, 5], [1, 1, 3, 3]] {
                    let mut quad = submission();
                    quad.triangles = diagonal;
                    quad.policy.fog =
                        crate::d3d_textured::D3dTextureFog::Vertex { color: [0, 0, 255] };
                    for (i, vertex) in quad.vertices.iter_mut().enumerate() {
                        let left = matches!(i, 0 | 3);
                        vertex.diffuse = [255, 0, 0, 128];
                        vertex.specular = if left { 0xff00_0000 } else { 0 };
                        vertex.rhw = if left { 1. / 64. } else { 1. / 256. };
                    }
                    if reverse {
                        for triangle in &mut quad.triangles {
                            triangle.swap(1, 2);
                        }
                    }
                    let mut state = pipeline();
                    state.viewport = viewport;
                    let mut frame = SoftwareFrame::new(6, 6, [0; 4]);
                    frame
                        .fill_textured_submission(
                            &quad,
                            texture,
                            &mut state,
                            tests::texture_state().sampling,
                        )
                        .unwrap();
                    // At X=2 visibility is one half regardless of RHW or winding.
                    assert_eq!(frame.pixel_rgba(2, 2), Some([128, 0, 128, 128]));
                    assert_eq!(frame.pixel_rgba(1, 1), Some([191, 0, 64, 128]));
                }
            }
        }
    });
}

#[test]
fn vertex_fog_preserves_alpha_and_precedes_destination_blending() {
    with_texture([0x7fff; 4], |texture| {
        for (blend, expected) in [
            (D3dTextureBlend::Copy, [0, 0, 255, 128]),
            (D3dTextureBlend::SourceAlpha, [0, 50, 128, 164]),
        ] {
            let mut quad = submission();
            quad.policy.blend = blend;
            quad.policy.fog = crate::d3d_textured::D3dTextureFog::Vertex { color: [0, 0, 255] };
            for vertex in &mut quad.vertices {
                vertex.diffuse = [255, 0, 0, 128];
                vertex.specular = 0;
            }
            let mut frame = SoftwareFrame::new(6, 6, [0, 100, 0, 200]);
            frame
                .fill_textured_submission(
                    &quad,
                    texture,
                    &mut pipeline(),
                    tests::texture_state().sampling,
                )
                .unwrap();
            assert_eq!(frame.pixel_rgba(1, 1), Some(expected));
        }
    });
}

#[test]
fn textured_clipping_preserves_interior_and_unsupported_state_writes_no_pixels() {
    with_texture([0x7c00, 0x03e0, 0x001f, 0x7fff], |texture| {
        let mut whole = SoftwareFrame::new(6, 6, [0; 4]);
        whole
            .fill_textured_submission(
                &submission(),
                texture,
                &mut pipeline(),
                tests::texture_state().sampling,
            )
            .unwrap();
        let mut clipped = SoftwareFrame::new(6, 6, [0; 4]);
        let mut state = pipeline();
        state.viewport = [1, 1, 3, 3];
        assert_eq!(
            clipped
                .fill_textured_submission(
                    &submission(),
                    texture,
                    &mut state,
                    tests::texture_state().sampling
                )
                .unwrap(),
            4
        );
        for y in 1..3 {
            for x in 1..3 {
                assert_eq!(clipped.pixel_rgba(x, y), whole.pixel_rgba(x, y));
            }
        }
        for field in 0..6 {
            let mut sampling = tests::texture_state().sampling;
            match field {
                0 => sampling.min_filter = 3,
                1 => sampling.mag_filter = 2,
                2 => sampling.texture_blend = 2,
                3 => sampling.address = 2,
                4 => sampling.color_key_enabled = true,
                _ => sampling.alpha_test_enabled = true,
            }
            let mut frame = SoftwareFrame::new(6, 6, [10; 4]);
            assert!(matches!(
                frame.fill_textured_submission(&submission(), texture, &mut pipeline(), sampling),
                Err(SoftwarePixelError::Texture(
                    TexturedPixelError::State { .. }
                ))
            ));
            assert!(frame.rgba().iter().all(|v| *v == 10));
        }
    });
}

/// Corners of both signs (a far sprite across the world's wrap) draw
/// nothing and leave the frame to go on, whether or not clipping would hide
/// the negative ones.
#[test]
fn perspective_depths_of_both_signs_draw_nothing() {
    with_texture([0x7fff; 4], |texture| {
        for viewport in [[0, 0, 5, 5], [0, 0, 4, 1]] {
            for bypass in [false, true] {
                let mut quad = submission();
                quad.vertices[2].rhw = -quad.vertices[2].rhw;
                quad.vertices[3].rhw = -quad.vertices[3].rhw;
                if bypass {
                    quad.mode |= 0x20;
                }
                let mut state = pipeline();
                state.viewport = viewport;
                let mut frame = SoftwareFrame::new(6, 6, [10; 4]);
                assert_eq!(
                    frame.fill_textured_submission(
                        &quad,
                        texture,
                        &mut state,
                        tests::texture_state().sampling
                    ),
                    Ok(0)
                );
                assert!(frame.rgba().iter().all(|v| *v == 10));
            }
        }
    });
}
