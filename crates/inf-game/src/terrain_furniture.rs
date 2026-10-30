//! Furniture-item enumeration for the terrain detail path (`0044CD00`).
//!
//! The terrain parent (`0044AA10`) computes a row domain — origin words,
//! the `005372F0` scale / `005372F4` mask pair, and coordinate words —
//! and this owner walks the H12 sparse buckets, selecting the in-range
//! records exactly the `0044CD00` filter does. `setup_furniture_item` owns
//! the selected record's `0044CEF0` position/light/ambient inputs. Model and
//! effect composition in `004359A0` is owned by the draw child through required
//! resource, particle and model services.
//!
//! The label-soup in the decompiled `0044CD00` (advance/consume/skip
//! loops at `0044CE14/4CE57/4CE74`) is one forward scan per bucket:
//! each record fires if and only if it is in the wrapping x-range AND
//! passes the odd-x flag test. The transcription below is that scan
//! directly, with each clause mapped to its native expression.

use inf_formats::{
    MissionPlacementTable, TerrainBucket, TerrainCell, TerrainRecord, TerrainResourceLookup,
};
use thiserror::Error;

pub mod draw;

/// Native `0044CD00` inputs. `coords` keeps the native word layout:
/// word 0 seeds the z-accumulator, then each row reads the pair at
/// `3*i+1, 3*i+2` (lo, hi); the words at `3*i+3` are skipped exactly
/// like the native `param_2 + 3` advance (stale per-row z words — the
/// accumulator decrements by `scale` instead). `rows` (`param_3`)
/// needs `coords.len() >= 3 * rows`; short arrays fail closed
/// (retail would read out of bounds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FurnitureDomain {
    /// `*(param_1 + 0x4)`: x origin word.
    pub origin_x: i16,
    /// `*(param_1 + 0xC)`: z origin word.
    pub origin_z: i16,
    /// `DAT_005372F0`: coordinate scale (spacing units).
    pub scale: u16,
    /// `DAT_005372F4`: coordinate mask; `0x7F` also bypasses the
    /// odd-x flag test below.
    pub mask: u16,
    pub coords: Vec<u16>,
    pub rows: usize,
}

/// One selected sparse record: its bucket (y cell) and index inside
/// the bucket's decoded record vec, with the record itself and the
/// walk accumulator low word at selection time (the `0044CEF0`
/// z input, `00546B54`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureHit {
    pub bucket: usize,
    pub index: usize,
    pub record: TerrainRecord,
    pub z: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FurnitureWalkError {
    #[error("furniture domain needs {needed} coordinate words for {rows} rows, has {have}")]
    ShortCoords {
        needed: usize,
        have: usize,
        rows: usize,
    },
    #[error("furniture walk selected bucket {bucket} beyond {count} decoded buckets")]
    Bucket { bucket: usize, count: usize },
}

/// `0045B180` anchor pair: caller args minus `0x1680`. The retained process
/// owner is `WorldLightGrid`; this supplied-input helper does not publish it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureAnchors {
    pub x: i32,
    pub z: i32,
}

pub fn furniture_anchors(x: i16, z: i16) -> FurnitureAnchors {
    FurnitureAnchors {
        x: i32::from(x) - 0x1680,
        z: i32::from(z) - 0x1680,
    }
}

/// `00490130` clamps the packed ambient RGB only. Its separate signed
/// component DWORDs retain the unclamped arguments, and the packed high byte
/// survives. `0044CEF0` supplies all three tint/16 + signed-light components.
pub fn clamp_ambient_rgb(values: [i32; 3]) -> [u8; 3] {
    let clamp = |value: i32| value.clamp(0, 0xff) as u8;
    if values[2] < 0 {
        [clamp(values[0]), clamp(values[1]), 0]
    } else {
        [clamp(values[0]), clamp(values[1]), clamp(values[2])]
    }
}

/// `0044CEF0` per-item inputs beyond the record and walk z. Cells are
/// the H12 array (the height read takes the low byte signed, so the
/// runtime diagonal bit is irrelevant); the light table is the
/// `0x5A`-stride signed RGB triples at `00548404`. Off-grid reads the low
/// three signed bytes of the data DWORD at `00526068`, not address bytes.
pub struct FurnitureSetupInputs<'a> {
    pub cells: &'a [TerrainCell],
    /// `(byte)005372F8 & 31`: signed-height scale.
    pub height_shift: u32,
    /// `DAT_005372F4`: coordinate mask (also the odd-x bypass when
    /// exactly `0x7F`, as in the walk).
    pub mask: u16,
    pub anchors: FurnitureAnchors,
    pub light_cells: &'a [u8],
    pub outside_light_word: u32,
    /// `005474E0`: each unsigned tint byte contributes `byte >> 4` to ambient.
    pub tint_word: u32,
}

/// `0044CEF0` outputs for one drawn item: world position, anchor
/// cell pair, light-grid membership, signed light RGB and all three
/// `00490130` arguments. H1 submission itself (`004359A0`) stays open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureItem {
    pub xyz: [i32; 3],
    pub anchor_cell: [u32; 2],
    pub in_light_grid: bool,
    pub light_rgb: [i8; 3],
    /// Raw signed components: normal shading consumes these before clamping.
    pub ambient_rgb: [i32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FurnitureSetupError {
    #[error("furniture height cell {index} beyond {count} H12 cells")]
    HeightCell { index: usize, count: usize },
    #[error("furniture light table needs {needed} bytes, has {have}")]
    LightTable { needed: usize, have: usize },
}

/// `0044CEF0` data transcription. `None` when the record lookup byte
/// is zero (the entry gate — never an H1). Everything else is
/// computed unconditionally, exactly the native order: X/Z words,
/// even-aligned height-cell read, anchor-relative cells, light
/// select, ambient arguments. Original `0044CFA7..0044D00B` reads all RGB
/// bytes, then sign-extends and adds each independently; none is a leftover.
pub fn setup_furniture_item(
    record: TerrainRecord,
    walk_z: u16,
    inputs: &FurnitureSetupInputs<'_>,
) -> Result<Option<FurnitureItem>, FurnitureSetupError> {
    if record.resource_lookup.raw() == 0 {
        return Ok(None);
    }
    // `(int)~F4 >> 7` as u32: `0xFFFFFFFE` under `0xFF` (even-align),
    // `0xFFFFFFFF` under `0x7F` (exact).
    let align = ((!(inputs.mask as i32)) >> 7) as u32;
    let x = (u32::from(record.x)) << 7;
    let z = u32::from(walk_z);
    let height_index =
        ((((u32::from(record.x)) & 0x1ff) & align) * 0x200 + ((z >> 7) & align)) as usize;
    let cell = inputs
        .cells
        .get(height_index)
        .ok_or(FurnitureSetupError::HeightCell {
            index: height_index,
            count: inputs.cells.len(),
        })?;
    let y = (i32::from(cell.height_i8())).wrapping_shl(inputs.height_shift);
    // Anchor words take the low 16 bits; the height carry rides the
    // high word (`(uint)Y >> 16`, logical).
    let carry = (y as u32) >> 16;
    let anchor_x = (carry << 16) | (inputs.anchors.x as u16 as u32);
    let anchor_z = (carry << 16) | (inputs.anchors.z as u16 as u32);
    let cell_u = x.wrapping_sub(anchor_x) >> 7 & 0x1ff & align;
    let cell_v = z.wrapping_sub(anchor_z) >> 7 & 0x1ff & align;
    let in_light_grid = cell_u < 0x5a && cell_v < 0x5a;
    let light_rgb = if in_light_grid {
        let at = ((cell_v + cell_u * 0x5a) * 3) as usize;
        let bytes = inputs
            .light_cells
            .get(at..at + 3)
            .ok_or(FurnitureSetupError::LightTable {
                needed: at + 3,
                have: inputs.light_cells.len(),
            })?;
        [bytes[0] as i8, bytes[1] as i8, bytes[2] as i8]
    } else {
        let bytes = inputs.outside_light_word.to_le_bytes();
        [bytes[0] as i8, bytes[1] as i8, bytes[2] as i8]
    };
    let tint = inputs.tint_word.to_le_bytes();
    let ambient_rgb = std::array::from_fn(|channel| {
        i32::from(tint[channel] >> 4) + i32::from(light_rgb[channel])
    });
    Ok(Some(FurnitureItem {
        xyz: [x as i32, y, z as i32],
        anchor_cell: [cell_u, cell_v],
        in_light_grid,
        light_rgb,
        ambient_rgb,
    }))
}

/// `004359A0` lookup bind: the H11 table's first word at
/// `base + lookup * 4` is the H1 slot. The runtime table is a
/// copy of the mission table bytes (`6.ovl` region 0: header
/// DWORD plus 256 `{slot, X}` DWORDs), so the port reads the
/// parsed [`MissionPlacementTable`] entry's first word
/// (`resource_word`) at the lookup index. Original `004359AC/004359D5`
/// gates only zero, then zero-extends all other bytes including `FD..FF`.
/// Their special policies in other consumers do not apply here. Anything past the table
/// fails closed. The entry high word (`parameter_word`), the
/// `00437A70` constructor, the RNG gate and H1 itself stay
/// with the submission owner.
pub fn bind_furniture_lookup(
    table: &MissionPlacementTable,
    lookup: TerrainResourceLookup,
) -> Option<u16> {
    if lookup.raw() == 0 {
        return None;
    }
    table
        .slots
        .get(usize::from(lookup.raw()))
        .map(|entry| entry.resource_word)
}

/// `0044CD00` record selection over decoded H12 buckets in native y
/// order. Counts are the decoded record vecs (the loader's
/// `byte_size >> 2` fixup). Deterministic: same grid + domain yields
/// the same hits in order.
pub fn walk_furniture_records(
    buckets: &[TerrainBucket],
    domain: &FurnitureDomain,
) -> Result<Vec<FurnitureHit>, FurnitureWalkError> {
    let needed = domain.rows.checked_mul(3).unwrap_or(usize::MAX);
    if domain.coords.len() < needed {
        return Err(FurnitureWalkError::ShortCoords {
            needed,
            have: domain.coords.len(),
            rows: domain.rows,
        });
    }
    let scale = domain.scale;
    let mask = domain.mask;
    let origin_x = domain.origin_x as u16;
    let origin_z = domain.origin_z as u16;
    // `uVar5`: only the low word is ever consumed (`>> 7 & 0x1FF`
    // for the cell, `& 0xFFFF` by `0044CEF0`), so the native
    // pointer-high word in `CONCAT22` is dead and untracked.
    let mut z = domain.coords[0].wrapping_mul(scale).wrapping_add(origin_z) & !mask;
    let mut hits = Vec::new();
    for row in 0..domain.rows {
        let cell = usize::from((z >> 7) & 0x1ff);
        let records = &buckets
            .get(cell)
            .ok_or(FurnitureWalkError::Bucket {
                bucket: cell,
                count: buckets.len(),
            })?
            .records;
        // `uVar1/uVar2`: x-range ends in 9-bit cell space.
        let hi = (domain.coords[3 * row + 2]
            .wrapping_mul(scale)
            .wrapping_add(origin_x)
            & !mask)
            >> 7;
        let lo = (domain.coords[3 * row + 1]
            .wrapping_mul(scale)
            .wrapping_add(origin_x)
            & !mask)
            >> 7;
        let width = hi.wrapping_sub(lo) & 0x1ff;
        for (index, record) in records.iter().enumerate() {
            // In-range test `(x - lo & 0x1FF) <= width`, wrapping.
            if record.x.wrapping_sub(lo) & 0x1ff <= width && (mask == 0x7f || record.x & 1 == 0) {
                hits.push(FurnitureHit {
                    bucket: cell,
                    index,
                    record: *record,
                    z,
                });
            }
        }
        z = z.wrapping_sub(scale);
    }
    Ok(hits)
}

#[cfg(test)]
#[path = "terrain_furniture_tests.rs"]
mod tests;
