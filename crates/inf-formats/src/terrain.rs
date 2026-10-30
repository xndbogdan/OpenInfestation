//! Handler-12 terrain resources.
//!
//! `Handler12_Load` (`FUN_004507D0`) copies one standard allocation, then
//! walks each entry's 512 sparse-record byte counts. It replaces the on-disk
//! pointer placeholders with runtime pointers and divides every byte count by
//! four. This parser preserves the disk representation while exposing those
//! four-byte records without performing either pointer fixup.

use serde::Serialize;
use thiserror::Error;

use crate::{ByteSpan, HandlerFraming, HandlerRegion};

pub const TERRAIN_HANDLER: u8 = 12;
pub const TERRAIN_GRID_SIDE: usize = 512;
pub const TERRAIN_CELL_COUNT: usize = TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE;
pub const TERRAIN_PREFIX_BYTES: usize = 0x14;
pub const TERRAIN_RECORD_BYTES: usize = 4;
pub const TERRAIN_ENTRY_FIXED_BYTES: usize = TERRAIN_PREFIX_BYTES
    + TERRAIN_CELL_BYTES
    + TERRAIN_BUCKET_SIZE_BYTES
    + TERRAIN_POINTER_PLACEHOLDER_BYTES;
pub const TERRAIN_BLEND_COUNT: usize = 55;
pub const TERRAIN_BLEND_TEXTURE_RESOURCE_BASE: u16 = 0x4fc;

const TERRAIN_CELL_BYTES: usize = TERRAIN_CELL_COUNT * 2;
const TERRAIN_BUCKET_SIZE_BYTES: usize = TERRAIN_GRID_SIDE * 2;
const TERRAIN_POINTER_PLACEHOLDER_BYTES: usize = TERRAIN_GRID_SIDE * 4;
const TERRAIN_BUCKET_SIZE_OFFSET: usize = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_BYTES;
const TERRAIN_POINTER_PLACEHOLDER_OFFSET: usize =
    TERRAIN_BUCKET_SIZE_OFFSET + TERRAIN_BUCKET_SIZE_BYTES;

/// The representatives assigned texture resources 0x4fc through 0x532 by
/// `FUN_0044B5C0`, in the executable's numeric assignment order.
pub const TERRAIN_CANONICAL_BLEND_COMBOS: [u8; TERRAIN_BLEND_COUNT] = [
    0, 1, 2, 3, 5, 6, 7, 10, 11, 15, 17, 18, 19, 21, 22, 23, 25, 26, 27, 29, 30, 31, 34, 35, 38,
    39, 42, 43, 46, 47, 51, 55, 59, 63, 85, 86, 87, 90, 91, 95, 102, 103, 106, 107, 110, 111, 119,
    123, 127, 170, 171, 175, 187, 191, 255,
];

const TERRAIN_ORIENTATION_WORDS: [u32; 8] = [
    0x3322_1100,
    0x1122_3300,
    0x2233_0011,
    0x0231_2013,
    0x3300_1122,
    0x1100_3322,
    0x2013_0231,
    0x0011_2233,
];
const TERRAIN_ORIENTATION_REMAP: [usize; 8] = [7, 6, 1, 0, 2, 3, 4, 5];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainBank {
    pub payload: ByteSpan,
    pub entries: Vec<TerrainEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainEntry {
    pub span: ByteSpan,
    pub prefix_span: ByteSpan,
    pub prefix: TerrainPrefix,
    pub cells_span: ByteSpan,
    cells: Box<[TerrainCell]>,
    pub bucket_sizes_span: ByteSpan,
    pub pointer_placeholders_span: ByteSpan,
    pointer_placeholders: Box<[u32]>,
    pub sparse_records_span: ByteSpan,
    buckets: Vec<TerrainBucket>,
}

impl TerrainEntry {
    /// Return the cell stored at the executable's `x * 512 + y` index.
    pub fn cell(&self, x: usize, y: usize) -> Option<TerrainCell> {
        terrain_cell_index(x, y).map(|index| self.cells[index])
    }

    pub fn cells(&self) -> &[TerrainCell] {
        &self.cells
    }

    pub fn bucket(&self, y: usize) -> Option<&TerrainBucket> {
        self.buckets.get(y)
    }

    pub fn buckets(&self) -> &[TerrainBucket] {
        &self.buckets
    }

    /// Values ignored and overwritten by `Handler12_Load` on disk load.
    pub fn pointer_placeholders(&self) -> &[u32] {
        &self.pointer_placeholders
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainPrefix {
    /// Signed fixed-point plane threshold. Scene state supplies its shift.
    pub plane_height_fixed: i32,
    /// Signed vector normalized by `FUN_0044FDD0` for terrain lighting.
    pub light_direction_raw: [i32; 3],
    /// Preserved because its runtime meaning has not yet been recovered.
    pub unknown_10: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TerrainCell(u16);

impl TerrainCell {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn height_i8(self) -> i8 {
        self.0 as u8 as i8
    }

    pub const fn material(self) -> u8 {
        ((self.0 >> 8) & 0x03) as u8
    }

    pub const fn has_sparse_record(self) -> bool {
        self.0 & 0x0400 != 0
    }

    /// Bit 11 is zero in retail files and filled from blend orientation at
    /// runtime by `FUN_004506A0`.
    pub const fn stored_alternate_diagonal(self) -> bool {
        self.0 & 0x0800 != 0
    }

    pub const fn shade4(self) -> u8 {
        (self.0 >> 12) as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainBucket {
    pub y: u16,
    /// Original on-disk byte count. The loader replaces it with `byte_size / 4`.
    pub byte_size: u16,
    pub records_span: ByteSpan,
    pub records: Vec<TerrainRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainRecord {
    pub x: u16,
    pub state: u8,
    pub resource_lookup: TerrainResourceLookup,
}

impl TerrainRecord {
    pub const fn state_low_nibble(self) -> u8 {
        self.state & 0x0f
    }

    pub const fn state_high_nibble(self) -> u8 {
        self.state >> 4
    }
}

/// Raw index into the separately loaded lookup used by sparse terrain state.
/// Values 0xfd, 0xfe, and 0xff have special runtime handling, so this is not a
/// direct resource identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TerrainResourceLookup(u8);

impl TerrainResourceLookup {
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn is_special(self) -> bool {
        self.0 >= 0xfd
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TerrainError {
    #[error("terrain parser requires handler 12, received handler {0}")]
    WrongHandler(u8),

    #[error("handler 12 requires standard framing")]
    WrongFraming,

    #[error("handler 12 payload span {span:?} is outside the {input_len}-byte OVL")]
    PayloadOutOfBounds { span: ByteSpan, input_len: usize },

    #[error("handler 12 entry {entry} fixed data needs {needed} bytes, only {available} remain")]
    FixedDataTruncated {
        entry: u32,
        needed: usize,
        available: usize,
    },

    #[error("handler 12 entry {entry} bucket {y} byte size {byte_size} is not divisible by four")]
    MisalignedBucketSize { entry: u32, y: u16, byte_size: u16 },

    #[error(
        "handler 12 entry {entry} sparse records need {needed} bytes, only {available} remain"
    )]
    SparseDataTruncated {
        entry: u32,
        needed: usize,
        available: usize,
    },

    #[error(
        "handler 12 entry {entry} bucket {y} record {record} x={x} is outside the 512-cell grid"
    )]
    RecordXOutOfRange {
        entry: u32,
        y: u16,
        record: usize,
        x: u16,
    },

    #[error(
        "handler 12 entry {entry} bucket {y} record {record} x={x} is not strictly after x={previous_x}"
    )]
    RecordsNotStrictlySorted {
        entry: u32,
        y: u16,
        record: usize,
        x: u16,
        previous_x: u16,
    },

    #[error(
        "handler 12 entry {entry} sparse record ({x},{y}) has no corresponding cell bit 10 marker"
    )]
    RecordMissingCellMarker { entry: u32, x: u16, y: u16 },

    #[error(
        "handler 12 entry {entry} cell ({x},{y}) has bit 10 set but no corresponding sparse record"
    )]
    CellMarkerMissingRecord { entry: u32, x: u16, y: u16 },

    #[error("handler 12 has {remaining} trailing payload bytes after byte {offset:#x}")]
    TrailingPayloadBytes { offset: usize, remaining: usize },

    #[error("integer overflow while parsing handler 12 entry {entry}")]
    IntegerOverflow { entry: u32 },
}

/// Parse every terrain entry in a Handler-12 standard allocation.
pub fn parse_terrain_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<TerrainBank, TerrainError> {
    if region.handler.get() != TERRAIN_HANDLER {
        return Err(TerrainError::WrongHandler(region.handler.get()));
    }
    let HandlerFraming::Standard { payload } = region.framing else {
        return Err(TerrainError::WrongFraming);
    };
    let bytes = payload
        .get(source)
        .ok_or(TerrainError::PayloadOutOfBounds {
            span: payload,
            input_len: source.len(),
        })?;

    let mut relative_offset = 0usize;
    let mut entries = Vec::with_capacity(region.requested_count as usize);
    for entry_index in 0..region.requested_count {
        let (entry, entry_bytes) =
            parse_terrain_entry(bytes, payload.start, relative_offset, entry_index)?;
        entries.push(entry);
        relative_offset += entry_bytes;
    }

    if relative_offset != bytes.len() {
        return Err(TerrainError::TrailingPayloadBytes {
            offset: payload.start + relative_offset,
            remaining: bytes.len() - relative_offset,
        });
    }

    Ok(TerrainBank { payload, entries })
}

fn parse_terrain_entry(
    payload: &[u8],
    payload_start: usize,
    relative_offset: usize,
    entry_index: u32,
) -> Result<(TerrainEntry, usize), TerrainError> {
    let available = payload.len() - relative_offset;
    if available < TERRAIN_ENTRY_FIXED_BYTES {
        return Err(TerrainError::FixedDataTruncated {
            entry: entry_index,
            needed: TERRAIN_ENTRY_FIXED_BYTES,
            available,
        });
    }

    let fixed = &payload[relative_offset..relative_offset + TERRAIN_ENTRY_FIXED_BYTES];
    let absolute_start = payload_start
        .checked_add(relative_offset)
        .ok_or(TerrainError::IntegerOverflow { entry: entry_index })?;
    let prefix_span = ByteSpan::new(absolute_start, absolute_start + TERRAIN_PREFIX_BYTES);
    let prefix = TerrainPrefix {
        plane_height_fixed: i32_at(fixed, 0x00),
        light_direction_raw: [
            i32_at(fixed, 0x04),
            i32_at(fixed, 0x08),
            i32_at(fixed, 0x0c),
        ],
        unknown_10: u32_at(fixed, 0x10),
    };

    let cells_start = TERRAIN_PREFIX_BYTES;
    let cells_end = cells_start + TERRAIN_CELL_BYTES;
    let cells = fixed[cells_start..cells_end]
        .chunks_exact(2)
        .map(|cell| TerrainCell::from_raw(u16::from_le_bytes([cell[0], cell[1]])))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let cells_span = ByteSpan::new(absolute_start + cells_start, absolute_start + cells_end);

    let bucket_sizes_span = ByteSpan::new(
        absolute_start + TERRAIN_BUCKET_SIZE_OFFSET,
        absolute_start + TERRAIN_POINTER_PLACEHOLDER_OFFSET,
    );
    let bucket_sizes = fixed[TERRAIN_BUCKET_SIZE_OFFSET..TERRAIN_POINTER_PLACEHOLDER_OFFSET]
        .chunks_exact(2)
        .map(|size| u16::from_le_bytes([size[0], size[1]]))
        .collect::<Vec<_>>();
    let pointer_placeholders_span = ByteSpan::new(
        absolute_start + TERRAIN_POINTER_PLACEHOLDER_OFFSET,
        absolute_start + TERRAIN_ENTRY_FIXED_BYTES,
    );
    let pointer_placeholders = fixed[TERRAIN_POINTER_PLACEHOLDER_OFFSET..TERRAIN_ENTRY_FIXED_BYTES]
        .chunks_exact(4)
        .map(|pointer| u32::from_le_bytes(pointer.try_into().expect("four-byte chunk")))
        .collect::<Vec<_>>()
        .into_boxed_slice();

    let mut sparse_bytes = 0usize;
    for (y, &byte_size) in bucket_sizes.iter().enumerate() {
        if usize::from(byte_size) % TERRAIN_RECORD_BYTES != 0 {
            return Err(TerrainError::MisalignedBucketSize {
                entry: entry_index,
                y: y as u16,
                byte_size,
            });
        }
        sparse_bytes = sparse_bytes
            .checked_add(usize::from(byte_size))
            .ok_or(TerrainError::IntegerOverflow { entry: entry_index })?;
    }

    let sparse_available = available - TERRAIN_ENTRY_FIXED_BYTES;
    if sparse_bytes > sparse_available {
        return Err(TerrainError::SparseDataTruncated {
            entry: entry_index,
            needed: sparse_bytes,
            available: sparse_available,
        });
    }
    let entry_bytes = TERRAIN_ENTRY_FIXED_BYTES
        .checked_add(sparse_bytes)
        .ok_or(TerrainError::IntegerOverflow { entry: entry_index })?;
    let sparse_records_start = absolute_start + TERRAIN_ENTRY_FIXED_BYTES;
    let sparse_records_span =
        ByteSpan::new(sparse_records_start, sparse_records_start + sparse_bytes);
    let sparse_source =
        &payload[relative_offset + TERRAIN_ENTRY_FIXED_BYTES..relative_offset + entry_bytes];
    let mut sparse_offset = 0usize;
    let mut marked_records = vec![false; TERRAIN_CELL_COUNT];
    let mut buckets = Vec::with_capacity(TERRAIN_GRID_SIDE);

    for (y, &byte_size) in bucket_sizes.iter().enumerate() {
        let bucket_end = sparse_offset + usize::from(byte_size);
        let bucket_source = &sparse_source[sparse_offset..bucket_end];
        let records_span = ByteSpan::new(
            sparse_records_start + sparse_offset,
            sparse_records_start + bucket_end,
        );
        let mut records = Vec::with_capacity(usize::from(byte_size) / TERRAIN_RECORD_BYTES);
        let mut previous_x = None;
        for (record_index, record) in bucket_source.chunks_exact(TERRAIN_RECORD_BYTES).enumerate() {
            let x = u16::from_le_bytes([record[0], record[1]]);
            if usize::from(x) >= TERRAIN_GRID_SIDE {
                return Err(TerrainError::RecordXOutOfRange {
                    entry: entry_index,
                    y: y as u16,
                    record: record_index,
                    x,
                });
            }
            if let Some(previous_x) = previous_x {
                if x <= previous_x {
                    return Err(TerrainError::RecordsNotStrictlySorted {
                        entry: entry_index,
                        y: y as u16,
                        record: record_index,
                        x,
                        previous_x,
                    });
                }
            }
            let cell_index = usize::from(x) * TERRAIN_GRID_SIDE + y;
            if !cells[cell_index].has_sparse_record() {
                return Err(TerrainError::RecordMissingCellMarker {
                    entry: entry_index,
                    x,
                    y: y as u16,
                });
            }
            marked_records[cell_index] = true;
            previous_x = Some(x);
            records.push(TerrainRecord {
                x,
                state: record[2],
                resource_lookup: TerrainResourceLookup::from_raw(record[3]),
            });
        }
        buckets.push(TerrainBucket {
            y: y as u16,
            byte_size,
            records_span,
            records,
        });
        sparse_offset = bucket_end;
    }

    for (index, cell) in cells.iter().enumerate() {
        if cell.has_sparse_record() && !marked_records[index] {
            return Err(TerrainError::CellMarkerMissingRecord {
                entry: entry_index,
                x: (index / TERRAIN_GRID_SIDE) as u16,
                y: (index % TERRAIN_GRID_SIDE) as u16,
            });
        }
    }

    let absolute_end = absolute_start + entry_bytes;
    Ok((
        TerrainEntry {
            span: ByteSpan::new(absolute_start, absolute_end),
            prefix_span,
            prefix,
            cells_span,
            cells,
            bucket_sizes_span,
            pointer_placeholders_span,
            pointer_placeholders,
            sparse_records_span,
            buckets,
        },
        entry_bytes,
    ))
}

const fn terrain_cell_index(x: usize, y: usize) -> Option<usize> {
    if x < TERRAIN_GRID_SIDE && y < TERRAIN_GRID_SIDE {
        Some(x * TERRAIN_GRID_SIDE + y)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum TerrainBlendSymmetry {
    /// `[a,b,c,d]`
    Identity = 0,
    /// `[c,b,a,d]`
    SwapAc = 1,
    /// `[b,c,d,a]`
    RotateLeft = 2,
    /// `[d,c,b,a]`
    Reverse = 3,
    /// `[c,d,a,b]`
    RotateByTwo = 4,
    /// `[a,d,c,b]`
    SwapBd = 5,
    /// `[d,a,b,c]`
    RotateRight = 6,
    /// `[b,a,d,c]`
    SwapAdjacentPairs = 7,
}

impl TerrainBlendSymmetry {
    pub const ALL: [Self; 8] = [
        Self::Identity,
        Self::SwapAc,
        Self::RotateLeft,
        Self::Reverse,
        Self::RotateByTwo,
        Self::SwapBd,
        Self::RotateRight,
        Self::SwapAdjacentPairs,
    ];

    /// Apply the executable's permutation to base-four digits low to high.
    pub const fn apply(self, combo: u8) -> u8 {
        let [a, b, c, d] = unpack_combo(combo);
        match self {
            Self::Identity => pack_combo([a, b, c, d]),
            Self::SwapAc => pack_combo([c, b, a, d]),
            Self::RotateLeft => pack_combo([b, c, d, a]),
            Self::Reverse => pack_combo([d, c, b, a]),
            Self::RotateByTwo => pack_combo([c, d, a, b]),
            Self::SwapBd => pack_combo([a, d, c, b]),
            Self::RotateRight => pack_combo([d, a, b, c]),
            Self::SwapAdjacentPairs => pack_combo([b, a, d, c]),
        }
    }

    /// Orientation word selected through the executable's table at 0x536fd0
    /// and remap table at 0x536ff0.
    pub const fn orientation_word(self) -> u32 {
        TERRAIN_ORIENTATION_WORDS[TERRAIN_ORIENTATION_REMAP[self as usize]]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainBlend {
    pub source_combo: u8,
    pub canonical_combo: u8,
    pub texture_rank: u8,
    pub texture_resource_id: u16,
    pub symmetry: TerrainBlendSymmetry,
    pub orientation_word: u32,
}

impl TerrainBlend {
    /// Whether `FUN_004506A0` sets cell bit 11 for this orientation.
    pub const fn uses_alternate_diagonal(self) -> bool {
        !matches!(self.orientation_word & 0x0f, 0 | 2)
    }
}

/// Reproduce one entry of the 256-element table built by `FUN_0044B5C0`.
pub fn terrain_blend(combo: u8) -> TerrainBlend {
    let mut canonical_combo = combo;
    let mut symmetry = TerrainBlendSymmetry::Identity;
    for candidate_symmetry in TerrainBlendSymmetry::ALL.into_iter().skip(1) {
        let candidate = candidate_symmetry.apply(combo);
        if candidate < canonical_combo {
            canonical_combo = candidate;
            symmetry = candidate_symmetry;
        }
    }
    let texture_rank = TERRAIN_CANONICAL_BLEND_COMBOS
        .iter()
        .position(|&canonical| canonical == canonical_combo)
        .expect("the eight terrain symmetries have exactly 55 representatives")
        as u8;
    let orientation_word = symmetry.orientation_word();
    TerrainBlend {
        source_combo: combo,
        canonical_combo,
        texture_rank,
        texture_resource_id: TERRAIN_BLEND_TEXTURE_RESOURCE_BASE + u16::from(texture_rank),
        symmetry,
        orientation_word,
    }
}

/// Build the complete immutable lookup table without renderer or scene state.
pub fn terrain_blend_table() -> [TerrainBlend; 256] {
    std::array::from_fn(|combo| terrain_blend(combo as u8))
}

const fn unpack_combo(combo: u8) -> [u8; 4] {
    [combo & 3, (combo >> 2) & 3, (combo >> 4) & 3, combo >> 6]
}

const fn pack_combo([a, b, c, d]: [u8; 4]) -> u8 {
    a | (b << 2) | (c << 4) | (d << 6)
}

fn i32_at(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed terrain field"),
    )
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed terrain field"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HandlerId;

    fn region(payload: ByteSpan, requested_count: u32) -> HandlerRegion {
        HandlerRegion {
            handler: HandlerId::new(TERRAIN_HANDLER).unwrap(),
            requested_count,
            span: payload,
            framing: HandlerFraming::Standard { payload },
        }
    }

    fn entry_with_records(records: &[(u16, u16, u8, u8)]) -> Vec<u8> {
        let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
        bytes[0..4].copy_from_slice(&(-0x1234i32).to_le_bytes());
        bytes[4..8].copy_from_slice(&3i32.to_le_bytes());
        bytes[8..12].copy_from_slice(&(-4i32).to_le_bytes());
        bytes[12..16].copy_from_slice(&5i32.to_le_bytes());
        bytes[16..20].copy_from_slice(&0x89ab_cdefu32.to_le_bytes());

        let mut by_y = vec![Vec::new(); TERRAIN_GRID_SIDE];
        for &(x, y, state, lookup) in records {
            let cell_index = usize::from(x) * TERRAIN_GRID_SIDE + usize::from(y);
            let cell_offset = TERRAIN_PREFIX_BYTES + cell_index * 2;
            let raw = 0x0400 | 0x3200 | (u16::from((x & 3) as u8) << 8) | 0x00fe;
            bytes[cell_offset..cell_offset + 2].copy_from_slice(&raw.to_le_bytes());
            by_y[usize::from(y)].push((x, state, lookup));
        }
        for records in &mut by_y {
            records.sort_by_key(|record| record.0);
        }
        for (y, records) in by_y.iter().enumerate() {
            let size = (records.len() * TERRAIN_RECORD_BYTES) as u16;
            let size_offset = TERRAIN_BUCKET_SIZE_OFFSET + y * 2;
            bytes[size_offset..size_offset + 2].copy_from_slice(&size.to_le_bytes());
            for &(x, state, lookup) in records {
                bytes.extend_from_slice(&x.to_le_bytes());
                bytes.push(state);
                bytes.push(lookup);
            }
        }
        bytes
    }

    #[test]
    fn parses_prefix_cells_pointer_placeholders_and_sparse_buckets() {
        let mut source = vec![0xaa; 7];
        let mut entry = entry_with_records(&[(3, 2, 0xab, 0xfd), (7, 2, 0x21, 9)]);
        entry[TERRAIN_POINTER_PLACEHOLDER_OFFSET..TERRAIN_POINTER_PLACEHOLDER_OFFSET + 4]
            .copy_from_slice(&0x1234_5678u32.to_le_bytes());
        source.extend_from_slice(&entry);
        let payload = ByteSpan::new(7, source.len());

        let bank = parse_terrain_bank(&source, &region(payload, 1)).unwrap();
        assert_eq!(TERRAIN_ENTRY_FIXED_BYTES, 0x80c14);
        assert_eq!(bank.entries.len(), 1);
        let terrain = &bank.entries[0];
        assert_eq!(terrain.span, payload);
        assert_eq!(terrain.prefix.plane_height_fixed, -0x1234);
        assert_eq!(terrain.prefix.light_direction_raw, [3, -4, 5]);
        assert_eq!(terrain.prefix.unknown_10, 0x89ab_cdef);
        assert_eq!(terrain.pointer_placeholders()[0], 0x1234_5678);
        assert_eq!(terrain.pointer_placeholders().len(), TERRAIN_GRID_SIDE);
        assert_eq!(terrain.cells().len(), TERRAIN_CELL_COUNT);

        let cell = terrain.cell(3, 2).unwrap();
        assert_eq!(cell.height_i8(), -2);
        assert_eq!(cell.material(), 3);
        assert!(cell.has_sparse_record());
        assert!(!cell.stored_alternate_diagonal());
        assert_eq!(cell.shade4(), 3);
        assert!(terrain.cell(TERRAIN_GRID_SIDE, 0).is_none());

        let bucket = terrain.bucket(2).unwrap();
        assert_eq!(bucket.byte_size, 8);
        assert_eq!(bucket.records.len(), 2);
        assert_eq!(bucket.records[0].x, 3);
        assert_eq!(bucket.records[0].state_low_nibble(), 0x0b);
        assert_eq!(bucket.records[0].state_high_nibble(), 0x0a);
        assert_eq!(bucket.records[0].resource_lookup.raw(), 0xfd);
        assert!(bucket.records[0].resource_lookup.is_special());
    }

    #[test]
    fn parses_multiple_variable_length_entries_without_sentinels() {
        let first = entry_with_records(&[(1, 9, 1, 2)]);
        let second = entry_with_records(&[(2, 10, 3, 4), (5, 11, 6, 7)]);
        let mut source = first;
        source.extend_from_slice(&second);
        let payload = ByteSpan::new(0, source.len());

        let bank = parse_terrain_bank(&source, &region(payload, 2)).unwrap();
        assert_eq!(bank.entries.len(), 2);
        assert_eq!(bank.entries[0].sparse_records_span.len(), 4);
        assert_eq!(bank.entries[1].sparse_records_span.len(), 8);
        assert_eq!(bank.entries[1].bucket(11).unwrap().records[0].x, 5);
    }

    #[test]
    fn rejects_misaligned_unsorted_and_unpaired_sparse_records() {
        let mut misaligned = entry_with_records(&[]);
        misaligned[TERRAIN_BUCKET_SIZE_OFFSET..TERRAIN_BUCKET_SIZE_OFFSET + 2]
            .copy_from_slice(&3u16.to_le_bytes());
        assert!(matches!(
            parse_terrain_bank(&misaligned, &region(ByteSpan::new(0, misaligned.len()), 1)),
            Err(TerrainError::MisalignedBucketSize {
                y: 0,
                byte_size: 3,
                ..
            })
        ));

        let mut unsorted = entry_with_records(&[(1, 2, 0, 0), (3, 2, 0, 0)]);
        let records = TERRAIN_ENTRY_FIXED_BYTES;
        unsorted[records..records + 2].copy_from_slice(&3u16.to_le_bytes());
        unsorted[records + 4..records + 6].copy_from_slice(&1u16.to_le_bytes());
        assert!(matches!(
            parse_terrain_bank(&unsorted, &region(ByteSpan::new(0, unsorted.len()), 1)),
            Err(TerrainError::RecordsNotStrictlySorted { y: 2, .. })
        ));

        let mut missing_marker = entry_with_records(&[(4, 8, 0, 0)]);
        let cell = TERRAIN_PREFIX_BYTES + (4 * TERRAIN_GRID_SIDE + 8) * 2;
        missing_marker[cell + 1] &= !0x04;
        assert!(matches!(
            parse_terrain_bank(
                &missing_marker,
                &region(ByteSpan::new(0, missing_marker.len()), 1)
            ),
            Err(TerrainError::RecordMissingCellMarker { x: 4, y: 8, .. })
        ));
    }

    #[test]
    fn rejects_truncated_out_of_range_or_trailing_entry_data() {
        let truncated = vec![0; TERRAIN_ENTRY_FIXED_BYTES - 1];
        assert!(matches!(
            parse_terrain_bank(&truncated, &region(ByteSpan::new(0, truncated.len()), 1)),
            Err(TerrainError::FixedDataTruncated { available, .. })
                if available == TERRAIN_ENTRY_FIXED_BYTES - 1
        ));

        let mut sparse_truncated = entry_with_records(&[]);
        sparse_truncated[TERRAIN_BUCKET_SIZE_OFFSET..TERRAIN_BUCKET_SIZE_OFFSET + 2]
            .copy_from_slice(&4u16.to_le_bytes());
        assert!(matches!(
            parse_terrain_bank(
                &sparse_truncated,
                &region(ByteSpan::new(0, sparse_truncated.len()), 1)
            ),
            Err(TerrainError::SparseDataTruncated {
                needed: 4,
                available: 0,
                ..
            })
        ));

        let mut out_of_range = entry_with_records(&[]);
        out_of_range[TERRAIN_BUCKET_SIZE_OFFSET..TERRAIN_BUCKET_SIZE_OFFSET + 2]
            .copy_from_slice(&4u16.to_le_bytes());
        out_of_range.extend_from_slice(&512u16.to_le_bytes());
        out_of_range.extend_from_slice(&[0, 0]);
        assert!(matches!(
            parse_terrain_bank(
                &out_of_range,
                &region(ByteSpan::new(0, out_of_range.len()), 1)
            ),
            Err(TerrainError::RecordXOutOfRange { x: 512, .. })
        ));

        let mut orphan_marker = entry_with_records(&[]);
        let cell_offset = TERRAIN_PREFIX_BYTES + (4 * TERRAIN_GRID_SIDE + 8) * 2;
        orphan_marker[cell_offset..cell_offset + 2].copy_from_slice(&0x0400u16.to_le_bytes());
        assert!(matches!(
            parse_terrain_bank(
                &orphan_marker,
                &region(ByteSpan::new(0, orphan_marker.len()), 1)
            ),
            Err(TerrainError::CellMarkerMissingRecord { x: 4, y: 8, .. })
        ));

        let mut trailing = entry_with_records(&[]);
        trailing.push(0);
        assert!(matches!(
            parse_terrain_bank(&trailing, &region(ByteSpan::new(0, trailing.len()), 1)),
            Err(TerrainError::TrailingPayloadBytes { remaining: 1, .. })
        ));
    }

    #[test]
    fn reproduces_all_256_executable_blend_entries() {
        let table = terrain_blend_table();
        let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;
        for blend in &table {
            for byte in [
                blend.canonical_combo,
                blend.texture_rank,
                blend.symmetry as u8,
            ]
            .into_iter()
            .chain(blend.texture_resource_id.to_le_bytes())
            .chain(blend.orientation_word.to_le_bytes())
            {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
            }
        }
        assert_eq!(fingerprint, 0xf711_8524_743e_3e4e);
        let representatives = table
            .iter()
            .filter(|blend| blend.source_combo == blend.canonical_combo)
            .map(|blend| blend.source_combo)
            .collect::<Vec<_>>();
        assert_eq!(representatives, TERRAIN_CANONICAL_BLEND_COMBOS);
        assert_eq!(
            table.iter().map(|blend| blend.texture_rank).max().unwrap(),
            54
        );
        assert!(table.iter().all(|blend| {
            blend.texture_resource_id
                == TERRAIN_BLEND_TEXTURE_RESOURCE_BASE + u16::from(blend.texture_rank)
        }));

        for (combo, texture, orientation) in [
            (0, 0x4fc, 0x0011_2233),
            (85, 0x51e, 0x0011_2233),
            (170, 0x52d, 0x0011_2233),
            (255, 0x532, 0x0011_2233),
        ] {
            assert_eq!(table[combo].texture_resource_id, texture);
            assert_eq!(table[combo].orientation_word, orientation);
            assert_eq!(table[combo].symmetry, TerrainBlendSymmetry::Identity);
        }

        let mut orbit_sizes = [0usize; 256];
        for blend in table {
            orbit_sizes[usize::from(blend.canonical_combo)] += 1;
        }
        let mut distribution = [0usize; 9];
        for representative in TERRAIN_CANONICAL_BLEND_COMBOS {
            distribution[orbit_sizes[usize::from(representative)]] += 1;
        }
        assert_eq!(distribution[1], 4);
        assert_eq!(distribution[2], 6);
        assert_eq!(distribution[4], 30);
        assert_eq!(distribution[8], 15);
    }

    #[test]
    fn applies_the_eight_low_to_high_digit_permutations_exactly() {
        let combo = pack_combo([0, 1, 2, 3]);
        let actual = TerrainBlendSymmetry::ALL.map(|symmetry| unpack_combo(symmetry.apply(combo)));
        assert_eq!(
            actual,
            [
                [0, 1, 2, 3],
                [2, 1, 0, 3],
                [1, 2, 3, 0],
                [3, 2, 1, 0],
                [2, 3, 0, 1],
                [0, 3, 2, 1],
                [3, 0, 1, 2],
                [1, 0, 3, 2],
            ]
        );
        assert_eq!(
            TerrainBlendSymmetry::ALL.map(TerrainBlendSymmetry::orientation_word),
            [
                0x0011_2233,
                0x2013_0231,
                0x1122_3300,
                0x3322_1100,
                0x2233_0011,
                0x0231_2013,
                0x3300_1122,
                0x1100_3322,
            ]
        );
    }
}
