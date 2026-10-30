//! The terrain radar `00470170`, which the Radar upgrade (grant 0x12)
//! draws in place of the ordinary radar.
//!
//! It writes 16-bit pixels straight into the locked back buffer over a
//! square twice H10 global slot 0's dimensions, its top left the HUD's
//! centre less those dimensions. Nothing is drawn (and the contents walk is
//! skipped) when that corner lies left of or above the renderer's clip
//! rectangle; its right and bottom edges cut the rows short.
//!
//! - Slot 0 stores one quadrant of cells, four pairs of signed WORDs each;
//!   slot 1 a mask per cell. The other three quadrants reflect them: columns
//!   from the middle on read cell `2W - 1 - x` with each pair's first WORD
//!   negated, rows from the middle on read row `2W - 1 - y` (the width,
//!   twice: every retail grid is square) with the second negated.
//! - A pair (a, b) turns by the view's heading into an offset from the
//!   eye's X/Z WORDs, `X = eye + a*c + b*s`, `Z = eye + b*c - a*s` (each
//!   product shifted 15), whose high bytes address the 256-square map at
//!   `0054ED78`. Its byte picks one of `00471D40`'s 43 colours.
//! - Mask 0 draws the whole pixel: an RNG draw below the map's jitter puts
//!   H7 0x20 or 0x3C (by a second draw's parity) halved over the overlay
//!   colour, otherwise the four samples are averaged in pairs and added to
//!   it. The overlay is slot 2's index into H7 0x62..0x71, one per output
//!   pixel and never reflected.
//! - Mask 15 leaves the pixel. Any other mask makes a rim pixel: bits 1, 2,
//!   4 and 8 replace pairs 0..3 by the destination pixel itself, halved.

use thiserror::Error;

use crate::object_runtime::native_trunc_low32 as ftol;
use crate::terrain_map::TERRAIN_MAP_CELLS;

/// `00471D40`'s table: five materials in eight shades, then three extras.
pub const TERRAIN_COLOURS: usize = 43;

/// H7 slots `00471D40` reads: the five terrain materials, then the extras
/// stored at entries 40, 41 and 42.
pub const TERRAIN_MATERIAL_SLOTS: [i32; 5] = [0x75, 0x76, 0x77, 0x78, 0x79];
pub const TERRAIN_EXTRA_SLOTS: [i32; 3] = [0x1f, 0x22, 0x21];
/// `004702FD..0047033B`: the overlay colours.
pub const OVERLAY_SLOTS: std::ops::Range<i32> = 0x62..0x72;
/// `0047051B..0047056A`: the jamming noise for an even and an odd draw.
pub const NOISE_SLOTS: [i32; 2] = [0x20, 0x3c];
/// `0047035B..00470390`: the mask that halves every colour.
pub const MASK_SLOT: i32 = 0x0f;

/// `00471D40`: for shades 3..10, each of the five terrain materials with
/// every channel (read through the renderer's material shifts, red, green
/// and blue) scaled by the shade over 8, held to 0x7F and repacked to its
/// five bits. Blue is shifted within the low byte only. Entries 40..42 are
/// the extras whole.
pub fn terrain_colours(
    materials: [u32; 5],
    extras: [u32; 3],
    shifts: [u32; 3],
) -> [u32; TERRAIN_COLOURS] {
    let [red, green, blue] = shifts;
    // 00471DBA..00471DE0: a signed division by 8, then the 0x7F limit.
    let scale =
        |channel: u32, shade: u32| (channel.wrapping_mul(shade) as i32 / 8).min(0x7f) as u32;
    let mut table = [0; TERRAIN_COLOURS];
    for (shade, row) in (3..11).zip(table.chunks_exact_mut(5)) {
        for (colour, entry) in materials.iter().zip(row) {
            // 00471DB0: `shl al, cl` keeps the byte; a count of 8 or more
            // empties it.
            let count = blue.wrapping_sub(1) & 31;
            let byte = if count < 8 {
                (*colour as u8) << count
            } else {
                0
            };
            let blue_value = scale(u32::from(byte) & 0xf8, shade);
            let green_value = scale(colour.wrapping_shr(green.wrapping_add(1)) & 0xf8, shade);
            let red_value = scale(colour.wrapping_shr(red.wrapping_add(1)) & 0xf8, shade);
            *entry = (blue_value & 0xf8).wrapping_shr(blue.wrapping_sub(1))
                | (red_value & 0xf8).wrapping_shl(red.wrapping_add(1))
                | (green_value & 0xf8).wrapping_shl(green.wrapping_add(1));
        }
    }
    table[40..].copy_from_slice(&extras);
    table
}

/// `00470239..004702F2`: the view's heading as `ftol(cos * 32768)` and
/// `ftol(sin * -32768)`, from its X axis in XZ (scaled to 4096 and narrowed
/// to signed WORDs). Unlike the contents' (`0047242C`), both quotients are
/// stored as floats first.
pub fn terrain_heading_rotation(axis_x: [f32; 3]) -> [i32; 2] {
    let a = f64::from(ftol(f64::from(axis_x[0]) * 4096.0) as i16);
    let b = f64::from(ftol(f64::from(axis_x[2]) * 4096.0) as i16);
    let length = (b * b + a * a).sqrt();
    let (cosine, sine) = if length == 0.0 {
        (1.0f32, 0.0f32)
    } else {
        let stored = f64::from(length as f32);
        ((a / stored) as f32, (b / stored) as f32)
    };
    [
        ftol(f64::from(cosine) * 32768.0),
        ftol(f64::from(sine) * -32768.0),
    ]
}

/// `004701A2..004701C3`: the centre WORDs less half of twice the
/// dimensions, the radar's top left.
pub fn terrain_radar_origin(centre: [i32; 2], size: [i32; 2]) -> [i16; 2] {
    [0, 1].map(|axis| {
        let half = size[axis].wrapping_shl(1).wrapping_div(2);
        (centre[axis] as u16).wrapping_add(half.wrapping_neg() as u16) as i16
    })
}

/// H10 global slots 0..2.
#[derive(Clone, Copy, Debug)]
pub struct TerrainRadarGrids<'a> {
    /// Slot 0's dimensions, the radar's half width and height.
    pub size: [i32; 2],
    /// Slot 0: four pairs per stored cell, rows of `W` cells.
    pub samples: &'a [[i16; 8]],
    /// Slot 1: each stored cell's mask.
    pub masks: &'a [i8],
    /// Slot 2: each output pixel's overlay index, rows of `2W`.
    pub overlay: &'a [i8],
}

/// The H7 colours the pass reads, already converted for the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainRadarColours {
    /// [`terrain_colours`], indexed by the map's bytes.
    pub terrain: [u32; TERRAIN_COLOURS],
    /// [`OVERLAY_SLOTS`].
    pub overlay: [u32; 16],
    /// [`NOISE_SLOTS`].
    pub noise: [u32; 2],
    /// [`MASK_SLOT`].
    pub mask: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainRadarPass<'a> {
    pub grids: TerrainRadarGrids<'a>,
    pub colours: TerrainRadarColours,
    /// The process map `0054ED78`, `x + z * 256`.
    pub map: &'a [u8],
    /// The HUD's centre WORDs.
    pub centre: [i32; 2],
    /// The renderer's clip rectangle `+48..+4E`: left, top, right, bottom.
    pub viewport: [i16; 4],
    /// The eye's X and Z WORDs.
    pub eye_words: [u16; 2],
    /// [`terrain_heading_rotation`].
    pub rotation: [i32; 2],
    /// The map's jitter, `+24`.
    pub jitter: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainRadarPixel {
    /// The WORD stored.
    Word(u16),
    /// A rim pixel's quarter terms for pairs 0..3 (mask bits 1, 2, 4 and
    /// 8): its sample's colour halved, or `None` for the destination's own
    /// pixel halved. See [`TerrainRadar::rim`].
    Rim([Option<u32>; 4]),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainRadar {
    /// The top left, signed WORDs.
    pub origin: [i32; 2],
    /// In the order written: rows from the top, each row's left half then
    /// its right half.
    pub pixels: Vec<([i32; 2], TerrainRadarPixel)>,
    /// [`MASK_SLOT`]'s colour, which also combines the rim terms.
    pub mask: u32,
    /// `00471989`: the contents `004723F0` follow while the jitter is below
    /// 0x4000.
    pub contents: bool,
}

impl TerrainRadar {
    /// `00470758..00470950`: a rim pixel over the destination WORD.
    pub fn rim(&self, terms: [Option<u32>; 4], destination: u16) -> u16 {
        let own = (u32::from(destination) >> 1) & self.mask;
        let [h1, h2, h4, h8] = terms.map(|term| term.unwrap_or(own));
        let high = (h8.wrapping_add(h4) >> 1) & self.mask;
        let low = (h2.wrapping_add(h1) >> 1) & self.mask;
        high.wrapping_add(low) as u16
    }

    /// Port approximation for a frame that keeps no CPU pixels (the OpenGL
    /// device), where retail reads a rim pixel's destination back: the rim
    /// over black with each five-bit channel (placed by the material
    /// shifts) scaled by four over its sample count, and that share of four
    /// as the blend's alpha, so the destination keeps the share its own
    /// terms hold. None when every term is the destination's.
    pub fn rim_blend(&self, terms: [Option<u32>; 4], shifts: [u32; 3]) -> Option<(u16, u8)> {
        let samples = terms.iter().flatten().count() as u32;
        if samples == 0 {
            return None;
        }
        let word = u32::from(self.rim(terms, 0));
        let [red, green, blue] = shifts;
        let mut scaled = 0;
        for position in [
            red.wrapping_add(4),
            green.wrapping_add(4),
            4u32.wrapping_sub(blue),
        ] {
            let channel = word.wrapping_shr(position) & 31;
            scaled |= (channel * 4 / samples).min(31).wrapping_shl(position);
        }
        Some((scaled as u16, (samples * 255 / 4) as u8))
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TerrainRadarError {
    #[error("the radar's map holds {0} bytes, not 65536")]
    MapSize(usize),
    /// Retail reads past its 43-entry table into the overlay colours and
    /// other locals; the map's constructor writes 0..39 only.
    #[error("map byte {value} at {index:#06x} lies past 00471D40's 43 colours")]
    MapColour { index: usize, value: u8 },
    #[error("overlay index {0} lies outside H7 0x62..0x71")]
    OverlayIndex(i8),
    /// The bottom half reflects rows by twice the width, which leaves a
    /// grid taller or wider than it is the other way.
    #[error("H10 slot {slot} has no entry {index} ({stored} stored)")]
    Outside { slot: u8, index: i64, stored: usize },
}

/// One output row's reflection.
#[derive(Clone, Copy)]
struct Quadrant {
    /// The stored row, `y` or `2W - 1 - y`.
    cell_row: i32,
    negate_b: bool,
}

/// The pass. `next` is the process RNG `0048F060`, drawn once per mask-0
/// pixel and once more when that draw falls below the jitter.
pub fn terrain_radar(
    pass: &TerrainRadarPass<'_>,
    rng: &mut u32,
    mut next: impl FnMut(&mut u32) -> u32,
) -> Result<Option<TerrainRadar>, TerrainRadarError> {
    if pass.map.len() != TERRAIN_MAP_CELLS {
        return Err(TerrainRadarError::MapSize(pass.map.len()));
    }
    let [width, height] = pass.grids.size;
    let double = [width.wrapping_shl(1), height.wrapping_shl(1)];
    let origin = terrain_radar_origin(pass.centre, pass.grids.size);
    let [left, top, right, bottom] = pass.viewport;
    // 00470383..004703A4
    if origin[0] < left || origin[1] < top {
        return Ok(None);
    }
    let [origin_x, origin_y] = origin.map(i32::from);
    // 004703F8..0047045E: the stored quadrant's width, the halves' columns
    // and the rows, each cut by the clip rectangle.
    let stored_width = double[0].wrapping_add(1) / 2;
    let available = [i32::from(right) - origin_x, i32::from(bottom) - origin_y];
    let left_columns = stored_width.min(available[0]);
    let top_rows = (double[1].wrapping_add(1) / 2).min(available[1]);
    let right_end = double[0].min(available[0]);
    let bottom_end = double[1].min(available[1]);
    let mut radar = TerrainRadar {
        origin: [origin_x, origin_y],
        pixels: Vec::new(),
        mask: pass.colours.mask,
        contents: pass.jitter < 0x4000,
    };
    let rows = (0..top_rows.max(0))
        .map(|y| {
            (
                y,
                Quadrant {
                    cell_row: y,
                    negate_b: false,
                },
            )
        })
        .chain((top_rows.max(0)..bottom_end).map(|y| {
            (
                y,
                Quadrant {
                    cell_row: double[0] - 1 - y,
                    negate_b: true,
                },
            )
        }));
    for (y, quadrant) in rows {
        let columns = (0..left_columns.max(0))
            .map(|x| (x, x, false))
            .chain((left_columns.max(0)..right_end).map(|x| (x, double[0] - 1 - x, true)));
        for (x, cell_column, negate_a) in columns {
            let cell =
                i64::from(quadrant.cell_row) * i64::from(stored_width) + i64::from(cell_column);
            let sampler = Sampler {
                pass,
                cell,
                negate_a,
                negate_b: quadrant.negate_b,
            };
            let overlay = || {
                let index = i64::from(y) * i64::from(double[0]) + i64::from(x);
                let value = entry(pass.grids.overlay, 2, index)?;
                pass.colours
                    .overlay
                    .get(value as usize)
                    .filter(|_| value >= 0)
                    .copied()
                    .ok_or(TerrainRadarError::OverlayIndex(value))
            };
            let mask = entry(pass.grids.masks, 1, cell)?;
            let pixel = match mask {
                0 => {
                    let colour = pass.colours.mask;
                    let value = if ((next(rng) & 0xffff) as i32) < pass.jitter {
                        let noise = pass.colours.noise[(next(rng) & 1) as usize];
                        overlay()?.wrapping_add((noise >> 1) & colour)
                    } else {
                        let high =
                            (sampler.colour(3)?.wrapping_add(sampler.colour(2)?) >> 1) & colour;
                        let low =
                            (sampler.colour(1)?.wrapping_add(sampler.colour(0)?) >> 1) & colour;
                        ((high.wrapping_add(low) >> 1) & colour).wrapping_add(overlay()?)
                    };
                    TerrainRadarPixel::Word(value as u16)
                }
                15 => continue,
                _ => {
                    let mut terms = [None; 4];
                    for (pair, term) in terms.iter_mut().enumerate() {
                        if mask & (1 << pair) == 0 {
                            *term = Some((sampler.colour(pair)? >> 1) & pass.colours.mask);
                        }
                    }
                    TerrainRadarPixel::Rim(terms)
                }
            };
            radar.pixels.push(([origin_x + x, origin_y + y], pixel));
        }
    }
    Ok(Some(radar))
}

fn entry<T: Copy>(values: &[T], slot: u8, index: i64) -> Result<T, TerrainRadarError> {
    usize::try_from(index)
        .ok()
        .and_then(|at| values.get(at).copied())
        .ok_or(TerrainRadarError::Outside {
            slot,
            index,
            stored: values.len(),
        })
}

/// One output pixel's stored cell and reflection.
struct Sampler<'p, 'a> {
    pass: &'p TerrainRadarPass<'a>,
    cell: i64,
    negate_a: bool,
    negate_b: bool,
}

impl Sampler<'_, '_> {
    /// Pair `pair`'s map colour (`004705B7..00470647` and its reflections).
    fn colour(&self, pair: usize) -> Result<u32, TerrainRadarError> {
        let words = entry(self.pass.grids.samples, 0, self.cell)?;
        let [c, s] = self.pass.rotation;
        let sign = |negate: bool, word: i16| {
            let value = i32::from(word);
            if negate {
                -value
            } else {
                value
            }
        };
        let a = sign(self.negate_a, words[pair * 2]);
        let b = sign(self.negate_b, words[pair * 2 + 1]);
        let [eye_x, eye_z] = self.pass.eye_words.map(i32::from);
        let x = eye_x
            .wrapping_add(a.wrapping_mul(c) >> 15)
            .wrapping_add(b.wrapping_mul(s) >> 15);
        let z = eye_z
            .wrapping_add(b.wrapping_mul(c) >> 15)
            .wrapping_sub(a.wrapping_mul(s) >> 15);
        let index = (z as u32 & 0xff00) as usize + ((x as u32 & 0xffff) >> 8) as usize;
        let value = self.pass.map[index];
        self.pass
            .colours
            .terrain
            .get(usize::from(value))
            .copied()
            .ok_or(TerrainRadarError::MapColour { index, value })
    }
}

#[cfg(test)]
#[path = "terrain_radar_tests.rs"]
mod tests;
