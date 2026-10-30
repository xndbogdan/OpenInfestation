//! Portable CPU allocations for the native Direct3D Handler-3 upload policy.
//!
//! Each descriptor owns a separate 256x256 ARGB8888 surface. This explicit
//! backend policy uses the native first-allocation UV and border layout; it
//! does not reproduce the retail shared-atlas repacking or eviction schedule.

use std::{
    cell::OnceCell,
    hash::{DefaultHasher, Hash, Hasher},
};

use inf_formats::{SpriteEncoding, SpriteError, SpriteRenderMode, SpriteVariant};
use thiserror::Error;

use crate::modern_tuning::{BandSearch, BandTuning};
use crate::render_resources::{RenderResource, RenderResourceData};

pub const TEXTURE_SURFACE_SIZE: u32 = 256;

/// Upload inputs from the live device owner. The output format is explicitly
/// ARGB8888 (masks 00FF0000, 0000FF00, 000000FF, FF000000).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTextureUploadState {
    /// 0065F118/11C/120: source channel shift fields, in R/G/B order.
    pub source_rgb_shifts: [u32; 3],
    /// 0065EF2C. Bit 08 selects the alpha-capable upload-kernel family.
    pub global_policy: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureAllocationId(usize);

#[derive(Debug)]
pub struct CpuTexture {
    pub allocation_id: TextureAllocationId,
    pub dimensions: [u16; 2],
    /// umin, vmin, umax, vmax, as consumed by 004D5410.
    pub uv_bounds: [f32; 4],
    pub flags: u16,
    rgba: Vec<[u8; 4]>,
    content_key: OnceCell<u64>,
    /// The OpenGL device's texture name, with the device serial and the
    /// presentation count it was looked up at (`gl_device`).
    pub(crate) device_name: std::cell::Cell<Option<(u64, u64, u32)>>,
}

impl CpuTexture {
    /// The colour a glow shines in from this texture: the mean of its
    /// drawn texels (keyed ones left out), each counting by how coloured it
    /// is, so a crystal white at its core and green at its edges shines
    /// green; brightest channel one.
    pub fn mean_colour(&self) -> [f32; 3] {
        let [width, height] = self.dimensions.map(u32::from);
        let mut sum = [0.0_f64; 3];
        for y in 1..=height {
            for x in 1..=width {
                if let Some([r, g, b, a]) = self.texel(x, y) {
                    if a > 0 {
                        let rgb = [r, g, b].map(f64::from);
                        let chroma = rgb.iter().copied().fold(0.0, f64::max)
                            - rgb.iter().copied().fold(255.0, f64::min);
                        let weight = chroma / 255.0 + 0.02;
                        for (total, value) in sum.iter_mut().zip(rgb) {
                            *total += value * weight;
                        }
                    }
                }
            }
        }
        let brightest = sum.iter().copied().fold(0.0, f64::max);
        if brightest <= 0.0 {
            return [1.0; 3];
        }
        sum.map(|total| (total / brightest) as f32)
    }

    /// Surface pixels outside the uploaded one-pixel border are uninitialized
    /// in the native allocation, so consumers must not sample them as black.
    pub fn texel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= u32::from(self.dimensions[0]) + 2 || y >= u32::from(self.dimensions[1]) + 2 {
            return None;
        }
        self.rgba
            .get((y * TEXTURE_SURFACE_SIZE + x) as usize)
            .copied()
    }

    /// Every uploaded texel, the duplicated border included, has one color,
    /// so no texture coordinate changes what a fragment samples.
    pub fn single_color(&self) -> bool {
        let [width, height] = self.dimensions.map(|v| u32::from(v) + 2);
        let first = self.texel(0, 0);
        (0..height).all(|y| (0..width).all(|x| self.texel(x, y) == first))
    }

    /// Complete 256x256 RGBA storage. Unuploaded cells are storage padding,
    /// not proven native pixels; rendering should use `texel`.
    pub fn rgba(&self) -> &[[u8; 4]] {
        &self.rgba
    }

    /// Identity of the uploaded pixels, for a device cache that outlives
    /// this CPU store: equal keys name equal surfaces, wherever they were
    /// decoded. Hashes the bordered upload only; the padding is always zero.
    pub fn content_key(&self) -> u64 {
        *self.content_key.get_or_init(|| {
            let mut hasher = DefaultHasher::new();
            self.dimensions.hash(&mut hasher);
            let [width, height] = self.dimensions.map(|value| usize::from(value) + 2);
            for row in self
                .rgba
                .chunks_exact(TEXTURE_SURFACE_SIZE as usize)
                .take(height)
            {
                row[..width].hash(&mut hasher);
            }
            hasher.finish()
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextureUploadError {
    #[error("texture upload requires a Handler-3 render resource")]
    ResourceKind,
    #[error("texture selector {selector} is outside the physical descriptor table")]
    Selector { selector: usize },
    #[error("texture dimensions {width}x{height} do not fit a native bordered 256x256 surface")]
    Dimensions { width: u16, height: u16 },
    #[error("texture upload global policy {0:#010x} requires the unsupported color-key family")]
    GlobalPolicy(u32),
    #[error("texture source span is missing or has an incompatible length")]
    SourceSpan,
    #[error("texture descriptor encoding disagrees with flag02")]
    Encoding,
    #[error(transparent)]
    Sprite(#[from] SpriteError),
}

struct CachedTexture<'a> {
    source_bytes: &'a [u8],
    descriptor: &'a SpriteVariant,
    state: NativeTextureUploadState,
    /// The land band's cut ([`cut_band`]); none is the sprite as uploaded.
    cut: Option<BandTuning>,
    texture: CpuTexture,
}

/// The borrow retains exact source/descriptor allocation custody across draws
/// and prevents resource residency mutation while textures are cached. IDs
/// name CPU allocations; they are never substituted for native addresses.
#[derive(Default)]
pub struct CpuTextureStore<'a> {
    textures: Vec<CachedTexture<'a>>,
    /// The textures made from each source bytes and descriptor addresses.
    by_source: crate::fast_hash::FastMap<(usize, usize, usize), Vec<usize>>,
}

impl<'a> CpuTextureStore<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// A texture [`Self::resolve`] made, by its allocation.
    pub fn get(&self, id: TextureAllocationId) -> Option<&CpuTexture> {
        self.textures.get(id.0).map(|cached| &cached.texture)
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    pub fn resolve(
        &mut self,
        resource: RenderResource<'a>,
        mode: SpriteRenderMode,
        state: NativeTextureUploadState,
    ) -> Result<Option<&CpuTexture>, TextureUploadError> {
        self.resolve_cut(resource, mode, state, None)
    }

    /// [`Self::resolve`]'s texture as the modern sky's land band: a copy
    /// with its painted sky cut away ([`cut_band`]).
    pub fn resolve_band(
        &mut self,
        resource: RenderResource<'a>,
        mode: SpriteRenderMode,
        state: NativeTextureUploadState,
        band: BandTuning,
    ) -> Result<Option<&CpuTexture>, TextureUploadError> {
        self.resolve_cut(resource, mode, state, Some(band))
    }

    fn resolve_cut(
        &mut self,
        resource: RenderResource<'a>,
        mode: SpriteRenderMode,
        state: NativeTextureUploadState,
        band: Option<BandTuning>,
    ) -> Result<Option<&CpuTexture>, TextureUploadError> {
        let RenderResourceData::Sprite(entry) = resource.data else {
            return Err(TextureUploadError::ResourceKind);
        };
        let Some(selector) = entry.selectors.variant_index(mode) else {
            return Ok(None);
        };
        let descriptor = entry
            .variants
            .get(selector)
            .ok_or(TextureUploadError::Selector { selector })?;
        let key = (
            resource.source.bytes.as_ptr() as usize,
            resource.source.bytes.len(),
            std::ptr::from_ref(descriptor) as usize,
        );
        if let Some(&index) = self.by_source.get(&key).and_then(|made| {
            made.iter().find(|&&index| {
                let cached = &self.textures[index];
                std::ptr::eq(cached.source_bytes, resource.source.bytes)
                    && std::ptr::eq(cached.descriptor, descriptor)
                    && cached.state == state
                    && cached.cut == band
            })
        }) {
            return Ok(Some(&self.textures[index].texture));
        }
        let mut texture = upload(
            descriptor,
            resource.source.bytes,
            state,
            TextureAllocationId(self.textures.len()),
        )?;
        if let Some(band) = band {
            cut_band(
                &mut texture.rgba,
                usize::from(descriptor.width),
                usize::from(descriptor.height),
                band,
            );
        }
        self.by_source
            .entry(key)
            .or_default()
            .push(self.textures.len());
        self.textures.push(CachedTexture {
            source_bytes: resource.source.bytes,
            descriptor,
            state,
            cut: band,
            texture,
        });
        Ok(Some(
            &self.textures.last().expect("just inserted texture").texture,
        ))
    }
}

/// The modern sky's land band: the painted sky cut out of `rgba`, a bordered
/// upload `width` by `height` texels, as alpha 0. The sky's colour is the
/// top row's median, and a texel within the band's `cut` of it (0 to 1, in
/// the largest channel's difference), or at least its `bright` in
/// luminance, is sky. Each column's skyline lies
/// where, searching as the band's `from` says, two texels in a row turn:
/// from the bottom, to sky, so a cloud painted above the land goes too;
/// from the top, to land. The border is copied again from the cut texels.
pub fn cut_band(rgba: &mut [[u8; 4]], width: usize, height: usize, band: BandTuning) {
    let stride = TEXTURE_SURFACE_SIZE as usize;
    let at = |x: usize, y: usize| (y + 1) * stride + x + 1;
    let sky: [i32; 3] = std::array::from_fn(|channel| {
        let mut values: Vec<u8> = (0..width).map(|x| rgba[at(x, 0)][channel]).collect();
        values.sort_unstable();
        i32::from(values[values.len() / 2])
    });
    let limit = (band.cut.clamp(0.0, 1.0) * 255.0).round() as i32;
    let bright = band.bright.map(|bright| bright.clamp(0.0, 1.0) * 255.0);
    let near_sky = |texel: [u8; 4]| {
        let luminance =
            0.3 * f32::from(texel[0]) + 0.6 * f32::from(texel[1]) + 0.1 * f32::from(texel[2]);
        (0..3).all(|c| (i32::from(texel[c]) - sky[c]).abs() <= limit)
            || bright.is_some_and(|bright| luminance >= bright)
    };
    // Each column's land begins at its skyline, counting rows from the top.
    let skylines: Vec<usize> = (0..width)
        .map(|x| {
            let sky_at = |y: usize| near_sky(rgba[at(x, y)]);
            match band.from {
                BandSearch::Bottom => (1..height)
                    .rev()
                    .find(|&y| sky_at(y) && sky_at(y - 1))
                    .map_or(0, |y| y + 1),
                BandSearch::Top => (0..height.saturating_sub(1))
                    .find(|&y| !sky_at(y) && !sky_at(y + 1))
                    .unwrap_or(height),
            }
        })
        .collect();
    // The median of five neighbouring columns: a lone column the search
    // misread leaves no sliver.
    let medians: Vec<usize> = (0..width)
        .map(|x| {
            let mut near: Vec<usize> = skylines[x.saturating_sub(2)..(x + 3).min(width)].to_vec();
            near.sort_unstable();
            near[near.len() / 2]
        })
        .collect();
    for x in 0..width {
        // Then the mean of three, its fraction the share of the edge texel
        // that stays land: the skyline slopes between columns instead of
        // stepping a whole texel, which the texture's magnification over
        // the horizon showed as stairs (Exxodus from a high point).
        let near = &medians[x.saturating_sub(1)..(x + 2).min(width)];
        let skyline = near.iter().sum::<usize>() as f32 / near.len() as f32;
        for y in 0..skyline.ceil() as usize {
            let land = (y as f32 + 1.0 - skyline).clamp(0.0, 1.0);
            let texel = &mut rgba[at(x, y)];
            texel[3] = texel[3].min((land * 255.0).round() as u8);
        }
    }
    for y in 0..height + 2 {
        for x in 0..width + 2 {
            let from = at(
                x.saturating_sub(1).min(width - 1),
                y.saturating_sub(1).min(height - 1),
            );
            rgba[y * stride + x] = rgba[from];
        }
    }
}

/// `0050A3D0`'s kernels for a 32-bit surface in the alpha family
/// (`00544DF8` indices 8..11 and 20..23): indexed `004FB7E0`, `004FBCD0`,
/// `004FC270` and `004FC770`, direct RGB555 `004FF6A0`, `004FFB60`,
/// `00500110` and `005005C0`, each unmasked or masked, without and with
/// descriptor bit 08.
fn upload(
    descriptor: &SpriteVariant,
    source: &[u8],
    state: NativeTextureUploadState,
    allocation_id: TextureAllocationId,
) -> Result<CpuTexture, TextureUploadError> {
    if state.global_policy & 8 == 0 {
        return Err(TextureUploadError::GlobalPolicy(state.global_policy));
    }
    let width = usize::from(descriptor.width);
    let height = usize::from(descriptor.height);
    if width == 0 || height == 0 || width > 254 || height > 254 {
        return Err(TextureUploadError::Dimensions {
            width: descriptor.width,
            height: descriptor.height,
        });
    }
    let flags = descriptor.flags.raw();
    let masked = flags & 1 != 0;
    let colors: Vec<[u8; 4]> = match &descriptor.encoding {
        SpriteEncoding::Indexed { indices, palette } => {
            if flags & 2 != 0 {
                return Err(TextureUploadError::Encoding);
            }
            let pixels = source
                .get(indices.start..indices.end)
                .filter(|pixels| pixels.len() == width * height)
                .ok_or(TextureUploadError::SourceSpan)?;
            pixels
                .iter()
                .map(|&index| {
                    if masked && index == 0 {
                        return Ok([0; 4]);
                    }
                    let word = palette.base_rgb555(source, index)?;
                    Ok(convert_word(u32::from(word), flags, state))
                })
                .collect::<Result<_, SpriteError>>()?
        }
        SpriteEncoding::DirectRgb555 { pixels } => {
            if flags & 2 == 0 {
                return Err(TextureUploadError::Encoding);
            }
            let pixels = source
                .get(pixels.start..pixels.end)
                .filter(|pixels| pixels.len() == width * height * 2)
                .ok_or(TextureUploadError::SourceSpan)?;
            pixels
                .chunks_exact(2)
                .map(|bytes| {
                    let word = i16::from_le_bytes([bytes[0], bytes[1]]);
                    if masked && word == 0 {
                        [0; 4]
                    } else {
                        convert_word(word as i32 as u32, flags, state)
                    }
                })
                .collect()
        }
    };
    let mut rgba = vec![[0; 4]; (TEXTURE_SURFACE_SIZE * TEXTURE_SURFACE_SIZE) as usize];
    // Eight native 32-bit upload kernels duplicate first/last rows and columns
    // into a one-pixel border, including the four corners.
    for y in 0..height + 2 {
        for x in 0..width + 2 {
            let source_x = x.saturating_sub(1).min(width - 1);
            let source_y = y.saturating_sub(1).min(height - 1);
            rgba[y * TEXTURE_SURFACE_SIZE as usize + x] = colors[source_y * width + source_x];
        }
    }
    Ok(CpuTexture {
        allocation_id,
        dimensions: [descriptor.width, descriptor.height],
        uv_bounds: [
            1. / 256.,
            1. / 256.,
            (width + 1) as f32 / 256.,
            (height + 1) as f32 / 256.,
        ],
        flags,
        rgba,
        content_key: OnceCell::new(),
        device_name: Default::default(),
    })
}

/// A texture laid out as `upload` lays out decoded pixels, with the
/// duplicated one-pixel border, for device tests.
#[cfg(test)]
pub(crate) fn test_texture(
    dimensions: [u16; 2],
    texel: impl Fn(usize, usize) -> [u8; 4],
) -> CpuTexture {
    let [width, height] = dimensions.map(usize::from);
    let mut rgba = vec![[0; 4]; (TEXTURE_SURFACE_SIZE * TEXTURE_SURFACE_SIZE) as usize];
    for y in 0..height + 2 {
        for x in 0..width + 2 {
            rgba[y * TEXTURE_SURFACE_SIZE as usize + x] = texel(
                x.saturating_sub(1).min(width - 1),
                y.saturating_sub(1).min(height - 1),
            );
        }
    }
    CpuTexture {
        allocation_id: TextureAllocationId(0),
        dimensions,
        uv_bounds: [
            1. / 256.,
            1. / 256.,
            (width + 1) as f32 / 256.,
            (height + 1) as f32 / 256.,
        ],
        flags: 0,
        rgba,
        content_key: OnceCell::new(),
        device_name: Default::default(),
    }
}

fn convert_word(mut word: u32, flags: u16, state: NativeTextureUploadState) -> [u8; 4] {
    if flags & 8 != 0 {
        word = word.wrapping_shl(1);
    }
    let mut rgba = [0; 4];
    for (channel, shift) in state.source_rgb_shifts.into_iter().enumerate() {
        // The source shift aligns the channel to bits31..27, then signed SAR
        // places those bits in the selected target mask. ARGB8888 masks admit
        // all eight bits: no RGB555 bit replication or preview conversion.
        rgba[channel] = ((word.wrapping_shl(27u32.wrapping_sub(shift) & 31) as i32) >> 24) as u8;
    }
    rgba[3] = 255;
    rgba
}

#[cfg(test)]
#[path = "d3d_texture_tests.rs"]
mod tests;
