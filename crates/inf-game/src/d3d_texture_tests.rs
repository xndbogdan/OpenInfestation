use super::*;
use inf_formats::{ByteSpan, SpriteAtlasOrigin, SpriteFlags, SpritePalette};

fn state() -> NativeTextureUploadState {
    NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    }
}

fn descriptor(
    width: u16,
    height: u16,
    flags: u16,
    pixels: &[u16],
    palette: &[u16],
) -> (SpriteVariant, Vec<u8>) {
    let mut source = Vec::new();
    let encoding = if flags & 2 != 0 {
        for pixel in pixels {
            source.extend_from_slice(&pixel.to_le_bytes());
        }
        SpriteEncoding::DirectRgb555 {
            pixels: ByteSpan::new(0, source.len()),
        }
    } else {
        source.extend(pixels.iter().map(|p| *p as u8));
        let indices = ByteSpan::new(0, source.len());
        let shade_start = source.len();
        source.resize(source.len() + palette.len() * 64, 0);
        let base_start = source.len();
        for color in palette {
            source.extend_from_slice(&color.to_le_bytes());
        }
        SpriteEncoding::Indexed {
            indices,
            palette: SpritePalette {
                color_count: palette.len() as u16,
                span: ByteSpan::new(shade_start, source.len()),
                shade_ramps: ByteSpan::new(shade_start, base_start),
                base_colors: ByteSpan::new(base_start, source.len()),
            },
        }
    };
    (
        SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(flags),
            color_count: palette.len() as u16,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width,
            height,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding,
        },
        source,
    )
}

#[test]
fn native_argb8888_upload_uses_live_source_shifts_and_no_bit_replication() {
    assert_eq!(convert_word(0x7fff, 0, state()), [255, 255, 248, 255]);
    assert_eq!(convert_word(0x4210, 0, state()), [132, 132, 128, 255]);
    assert_eq!(convert_word(0x4210, 8, state()), [8, 8, 0, 255]);
    assert_eq!(
        convert_word(
            0x4210,
            0,
            NativeTextureUploadState {
                source_rgb_shifts: [0, 5, 10],
                ..state()
            }
        ),
        [128, 132, 132, 255]
    );
}

#[test]
fn masked_index_zero_differs_from_black_palette_words_and_direct_high_bit() {
    let (v, source) = descriptor(3, 1, 1, &[0, 1, 2], &[0x7fff, 0, 0x8000]);
    let texture = upload(&v, &source, state(), TextureAllocationId(0)).unwrap();
    assert_eq!(texture.texel(1, 1), Some([0; 4]));
    assert_eq!(texture.texel(2, 1), Some([0, 0, 0, 255]));
    assert_eq!(texture.texel(3, 1), Some([0, 0, 0, 255]));
    let (v, source) = descriptor(3, 1, 3, &[0, 0x8000, 0x7fff], &[]);
    let texture = upload(&v, &source, state(), TextureAllocationId(1)).unwrap();
    assert_eq!(texture.texel(1, 1), Some([0; 4]));
    assert_eq!(texture.texel(2, 1), Some([0, 0, 0, 255]));
    assert_eq!(texture.texel(3, 1), Some([255, 255, 248, 255]));
}

#[test]
fn borders_uv_and_native_size_limit_match_first_allocation() {
    let (v, source) = descriptor(2, 2, 2, &[0x7c00, 0x03e0, 0x001f, 0x7fff], &[]);
    let texture = upload(&v, &source, state(), TextureAllocationId(0)).unwrap();
    assert_eq!(
        texture.uv_bounds,
        [1. / 256., 1. / 256., 3. / 256., 3. / 256.]
    );
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(
                texture.texel(x, y),
                texture.texel(x.clamp(1, 2), y.clamp(1, 2))
            );
        }
    }
    assert_eq!(texture.texel(4, 0), None);
    assert_eq!(texture.texel(0, 4), None);
    let (v, source) = descriptor(254, 254, 2, &vec![0x7fff; 254 * 254], &[]);
    let texture = upload(&v, &source, state(), TextureAllocationId(0)).unwrap();
    assert_eq!(
        texture.uv_bounds,
        [1. / 256., 1. / 256., 255. / 256., 255. / 256.]
    );
    assert!(texture.texel(255, 255).is_some());
    for dimensions in [[0, 1], [1, 0], [255, 1], [1, 255]] {
        let mut invalid = v.clone();
        invalid.width = dimensions[0];
        invalid.height = dimensions[1];
        assert!(matches!(
            upload(&invalid, &source, state(), TextureAllocationId(0)),
            Err(TextureUploadError::Dimensions { .. })
        ));
    }
}

#[test]
fn unsupported_global_policy_and_malformed_pixels_fail_before_allocation() {
    let (v, source) = descriptor(1, 1, 2, &[0x7fff], &[]);
    assert!(matches!(
        upload(
            &v,
            &source,
            NativeTextureUploadState {
                global_policy: 0,
                ..state()
            },
            TextureAllocationId(0)
        ),
        Err(TextureUploadError::GlobalPolicy(0))
    ));
    assert!(matches!(
        upload(&v, &source[..1], state(), TextureAllocationId(0)),
        Err(TextureUploadError::SourceSpan)
    ));
    let mut wrong = v.clone();
    wrong.flags = SpriteFlags::new(0);
    assert!(matches!(
        upload(&wrong, &source, state(), TextureAllocationId(0)),
        Err(TextureUploadError::Encoding)
    ));
}

#[test]
fn allocation_identity_tracks_physical_source_and_settings_and_ff_suppresses_upload() {
    use crate::{
        render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
        ResidentRegistration,
    };
    use inf_formats::{RequestKind, SpriteEntry, SpriteModeSelectors};
    let (variant, source) = descriptor(1, 1, 2, &[0x7fff], &[]);
    let entry = SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: 0,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![variant],
    };
    let copied_source = source.clone();
    let copied_entry = entry.clone();
    let suppressed = SpriteEntry {
        selectors: SpriteModeSelectors {
            mode0: 255,
            mode1: 255,
            reserved: 0,
        },
        variants: vec![],
        ..entry.clone()
    };
    let invalid = SpriteEntry {
        selectors: SpriteModeSelectors {
            mode0: 2,
            mode1: 2,
            reserved: 0,
        },
        ..entry.clone()
    };
    let registration = ResidentRegistration {
        level: 47,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 10,
    };
    let resource = RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 10,
        },
        source: RenderSource {
            registration: &registration,
            bytes: &source,
        },
        data: RenderResourceData::Sprite(&entry),
    };
    let mut store = CpuTextureStore::new();
    assert!(matches!(
        store.resolve(
            resource,
            SpriteRenderMode::Mode0,
            NativeTextureUploadState {
                global_policy: 0,
                ..state()
            }
        ),
        Err(TextureUploadError::GlobalPolicy(0))
    ));
    assert!(store.is_empty());
    let first = store
        .resolve(resource, SpriteRenderMode::Mode0, state())
        .unwrap()
        .unwrap()
        .allocation_id;
    assert_eq!(
        store
            .resolve(resource, SpriteRenderMode::Mode1, state())
            .unwrap()
            .unwrap()
            .allocation_id,
        first
    );
    let shifted = store
        .resolve(
            resource,
            SpriteRenderMode::Mode0,
            NativeTextureUploadState {
                source_rgb_shifts: [0, 5, 10],
                ..state()
            },
        )
        .unwrap()
        .unwrap()
        .allocation_id;
    assert_ne!(shifted, first);
    let different_bytes = RenderResource {
        source: RenderSource {
            bytes: &copied_source,
            ..resource.source
        },
        ..resource
    };
    assert_ne!(
        store
            .resolve(different_bytes, SpriteRenderMode::Mode0, state())
            .unwrap()
            .unwrap()
            .allocation_id,
        first
    );
    let different_descriptor = RenderResource {
        data: RenderResourceData::Sprite(&copied_entry),
        ..resource
    };
    assert_ne!(
        store
            .resolve(different_descriptor, SpriteRenderMode::Mode0, state())
            .unwrap()
            .unwrap()
            .allocation_id,
        first
    );
    assert_eq!(store.len(), 4);
    assert!(store
        .resolve(
            RenderResource {
                data: RenderResourceData::Sprite(&suppressed),
                ..resource
            },
            SpriteRenderMode::Mode0,
            NativeTextureUploadState {
                global_policy: 0,
                ..state()
            }
        )
        .unwrap()
        .is_none());
    assert!(matches!(
        store.resolve(
            RenderResource {
                data: RenderResourceData::Sprite(&invalid),
                ..resource
            },
            SpriteRenderMode::Mode0,
            state()
        ),
        Err(TextureUploadError::Selector { selector: 2 })
    ));
    assert_eq!(store.len(), 4);
}

/// A bordered 6x8 band: blue sky over brown land from row 4 down, with a
/// white cloud painted at the top of column 1, a sky-blue fleck inside the
/// land of column 3, and the sky reaching two rows deeper in column 5 alone;
/// the border copied as an upload makes it.
fn band_rgba() -> Vec<[u8; 4]> {
    let (width, height) = (6, 8);
    let stride = TEXTURE_SURFACE_SIZE as usize;
    let mut rgba = vec![[0; 4]; stride * stride];
    let texel = |x: usize, y: usize| match (x, y) {
        (1, 1) => [250, 250, 250, 255],
        (3, 6) => [40, 80, 200, 255],
        (5, y) if y < 6 => [40, 80, 200, 255],
        (_, y) if y < 4 => [40, 80, 200, 255],
        _ => [120, 80, 40, 255],
    };
    for y in 0..height + 2 {
        for x in 0..width + 2 {
            rgba[y * stride + x] = texel(
                x.saturating_sub(1).min(width - 1),
                y.saturating_sub(1).min(height - 1),
            );
        }
    }
    rgba
}

fn land_rows(rgba: &[[u8; 4]], x: usize) -> Vec<bool> {
    let stride = TEXTURE_SURFACE_SIZE as usize;
    (0..8)
        .map(|y| rgba[(y + 1) * stride + x + 1][3] != 0)
        .collect()
}

#[test]
fn the_land_band_keeps_its_land_and_cuts_its_painted_sky_and_clouds() {
    let band = |from| BandTuning {
        sprites: None,
        cut: 0.16,
        bright: None,
        from,
    };
    let expected: Vec<bool> = (0..8).map(|y| y >= 4).collect();
    // Up from the bottom the cloud above column 1's sky goes too, and the
    // fleck in column 3's land is one texel, not two, so the land holds.
    // Column 5 alone reaches deeper: its neighbours' median overrules it.
    let mut rgba = band_rgba();
    cut_band(&mut rgba, 6, 8, band(BandSearch::Bottom));
    for x in 0..6 {
        assert_eq!(land_rows(&rgba, x), expected, "from the bottom, column {x}");
    }
    // Down from the top the cloud is one texel, not two, so column 1's land
    // still begins at row 4.
    let mut rgba = band_rgba();
    cut_band(&mut rgba, 6, 8, band(BandSearch::Top));
    for x in 0..6 {
        assert_eq!(land_rows(&rgba, x), expected, "from the top, column {x}");
    }
    // The border repeats the cut edge texels.
    let stride = TEXTURE_SURFACE_SIZE as usize;
    assert_eq!(rgba[0][3], 0);
    assert_eq!(rgba[9 * stride][3], 255);
    // A glow painted in the sky below its top row, far from the sky's
    // colour, is land by colour alone, and sky once brightness counts.
    let glowing = || {
        let mut rgba = band_rgba();
        for y in 1..4 {
            for x in 0..6 {
                rgba[(y + 1) * stride + x + 1] = [250, 230, 160, 255];
            }
        }
        rgba
    };
    let mut rgba = glowing();
    cut_band(&mut rgba, 6, 8, band(BandSearch::Bottom));
    assert!(land_rows(&rgba, 2)[1], "by colour alone");
    let mut rgba = glowing();
    let bright = BandTuning {
        bright: Some(0.5),
        ..band(BandSearch::Bottom)
    };
    cut_band(&mut rgba, 6, 8, bright);
    for x in 0..6 {
        assert_eq!(land_rows(&rgba, x), expected, "with brightness, column {x}");
    }
}

#[test]
fn the_land_bands_skyline_slopes_between_columns() {
    // Sky down to row 2 in columns 0 to 2 and to row 4 in 3 to 5: the edge
    // texels either side of the step keep part of their land.
    let stride = TEXTURE_SURFACE_SIZE as usize;
    let mut rgba = vec![[0; 4]; stride * stride];
    for y in 0..10 {
        for x in 0..8 {
            let (cx, cy) = (x.max(1).min(6) - 1, y.max(1).min(8) - 1);
            let sky = cy < if cx < 3 { 2 } else { 4 };
            rgba[y * stride + x] = if sky {
                [40, 80, 200, 255]
            } else {
                [120, 80, 40, 255]
            };
        }
    }
    let band = BandTuning {
        sprites: None,
        cut: 0.16,
        bright: None,
        from: BandSearch::Bottom,
    };
    cut_band(&mut rgba, 6, 8, band);
    let alpha = |x: usize, y: usize| rgba[(y + 1) * stride + x + 1][3];
    // Column 2's skyline is (2 + 2 + 4) / 3 rows down, column 3's
    // (2 + 4 + 4) / 3.
    assert_eq!([alpha(2, 1), alpha(2, 2), alpha(2, 3)], [0, 85, 255]);
    assert_eq!([alpha(3, 2), alpha(3, 3), alpha(3, 4)], [0, 170, 255]);
    assert_eq!([alpha(0, 1), alpha(0, 2)], [0, 255]);
}

#[path = "d3d_texture_native_tests.rs"]
mod native;

#[path = "d3d_texture_corpus_tests.rs"]
mod corpus;
