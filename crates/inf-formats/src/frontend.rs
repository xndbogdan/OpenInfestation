//! Authored frontend records from the retail executable's mapped data section.
//!
//! This parser does not load a PE, execute callbacks, or apply runtime mutations.
//! Callers identify the executable and provide one contiguous, VA-tagged byte
//! range. Pointers are resolved only within that range. Coordinates, animation
//! controls, callback arguments, and original byte spans remain distinct.

use serde::Serialize;
use thiserror::Error;

use crate::ByteSpan;

pub const FRONTEND_MENU_DEFINITION_BYTES: usize = 20;
pub const FRONTEND_MENU_ITEM_BYTES: usize = 32;
pub const FRONTEND_MENU_LAYOUT_BYTES: usize = 16;
pub const FRONTEND_SCENE_MODEL_BYTES: usize = 24;
pub const FRONTEND_CAMERA_PRESET_BYTES: usize = 24;

/// Addresses in the supported, preferred-base Infest.exe image.
pub const FRONTEND_ROOT_MENU_ADDRESS: u32 = 0x0053_c088;

/// 0040A420 briefing menu: a header row plus three selectable rows, pushed
/// through 00453440 after 004137D0. Shares the generic menu decoder.
pub const BRIEFING_MENU_ADDRESS: u32 = 0x0052_f1b0;
/// Single-row custom menu built by 0040D560 over 0052F0D0.
pub const BRIEFING_DETAIL_MENU_ADDRESS: u32 = 0x0052_f0d0;
/// Four-row custom menu over the same single row record (0052F0E8).
pub const BRIEFING_LIST_MENU_ADDRESS: u32 = 0x0052_f0e8;
/// 00443190 in-world menu: a header row plus three selectable rows, built
/// into the phase-5 screen after the player lookup. Shares the generic menu
/// decoder.
pub const PHASE5_MENU_ADDRESS: u32 = 0x0053_5348;
/// 00444BD0 menu: a header row, seventeen indexed rows driving `00444B20`,
/// two child-push rows and a final `00444B50` row, built into the phase-12
/// screen with the previous session phase retained at `+0x12C`. Shares the
/// generic menu decoder.
pub const PHASE12_MENU_ADDRESS: u32 = 0x0053_5e40;
pub const FRONTEND_SCENE_MODELS_ADDRESS: u32 = 0x0053_bed8;
pub const FRONTEND_CAMERA_PRESETS_ADDRESS: u32 = 0x0053_c148;
pub const FRONTEND_SINE_TABLE_ADDRESS: u32 = 0x0053_df00;
pub const FRONTEND_SINE_TABLE_ENTRIES: usize = 4096;
pub const FRONTEND_INVERSE_SINE_TABLE_ADDRESS: u32 = 0x0053_ff00;
pub const FRONTEND_INVERSE_SINE_TABLE_ENTRIES: usize = 1024;

/// A bounded, contiguous virtual-address range; spans in decoded records are
/// relative to `bytes`, not executable file offsets or virtual addresses.
#[derive(Clone, Copy, Debug)]
pub struct FrontendData<'a> {
    pub bytes: &'a [u8],
    pub virtual_address: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FrontendMenu {
    pub virtual_address: u32,
    pub definition_span: ByteSpan,
    pub items_address: u32,
    pub layouts_address: u32,
    pub initial_selection: i32,
    pub flags: u32,
    pub items: Vec<FrontendMenuItem>,
    pub layouts: Vec<FrontendMenuLayout>,
    /// Retail stops on the first layout whose first byte is zero. The rest of
    /// this record need not be zero.
    pub layout_terminator: ByteSpan,
}

/// `00452310` installs these 32-byte records; `00452570` calls the draw target
/// with its two arguments, and `00452E20` calls the action target with an input
/// command and its two arguments. Callback-specific argument types are not
/// inferred here (the root text callbacks interpret draw argument 0 as i16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FrontendMenuItem {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub flags: u32,
    pub draw_callback: u32,
    pub draw_arguments: [u32; 2],
    pub action_callback: u32,
    pub action_arguments: [u32; 2],
    /// Mutable row-height result at +0x1C, in layout row-spacing units.
    pub row_height: u16,
    /// Mutable per-item age at +0x1E, reset by `00452440`.
    pub age: u16,
}

/// `00452570` uses signed x/y/width/height in a 1024-unit menu viewport and
/// unsigned row spacing. `00452310` clears the signed scroll animation word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FrontendMenuLayout {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub flags: u8,
    pub item_count: u8,
    pub visible_rows: u8,
    pub reserved: u8,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
    pub row_spacing: u16,
    pub scroll_animation: i16,
}

/// `00483C60` consumes a 24-byte record per model. The three signed rotation
/// controls at +4/+8/+12 feed `00491290`/`00491530`/`004917D0` respectively;
/// positive controls are fixed angles, negative controls select clock shifts.
/// Position is scaled by the separate `0053BFB4` value in the scene consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FrontendSceneModel {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub model_slot: i16,
    pub reserved_02: u16,
    pub rotation_controls: [i32; 3],
    pub position: [i16; 3],
    pub reserved_16: u16,
}

/// `00484AC0` reads two signed 3-vectors, scaling both by `0053BFB4` before
/// publishing camera targets. These are authored targets, not current camera
/// state and not an instruction to skip the retail spring interpolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FrontendCameraPreset {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub eye: [i32; 3],
    pub target: [i32; 3],
}

/// Original unsigned quarter-wave samples used by 00491530. Quadrant folding
/// and arithmetic policy belong to the runtime consumer, not this decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendSineTable {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub values: [u16; FRONTEND_SINE_TABLE_ENTRIES],
}

/// Original signed samples consumed by 0048F4D0. Its sign, saturation,
/// and five-bit index reduction belong to the runtime lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendInverseSineTable {
    pub virtual_address: u32,
    pub span: ByteSpan,
    pub values: [i16; FRONTEND_INVERSE_SINE_TABLE_ENTRIES],
}

/// Eleven raw mission-selection DWORDs consumed by 00488C90. Index wrapping
/// and unsigned comparison belong to the live game owner, not this decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendAttractSelectionTable {
    pub virtual_address: u32,
    /// Offset relative to the supplied FrontendData section.
    pub span: ByteSpan,
    pub values: [u32; FRONTEND_ATTRACT_SELECTION_ENTRIES],
}

pub const FRONTEND_ATTRACT_SELECTION_ADDRESS: u32 = 0x0053_c6ac;
pub const FRONTEND_ATTRACT_SELECTION_ENTRIES: usize = 11;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrontendError {
    #[error("frontend range {address:#010x} + {bytes} bytes is outside source {source_address:#010x} + {source_len} bytes")]
    AddressOutOfBounds {
        address: u32,
        bytes: usize,
        source_address: u32,
        source_len: usize,
    },
    #[error("frontend table size overflows for {count} records of {record_bytes} bytes")]
    TableSizeOverflow { count: u32, record_bytes: usize },
    #[error("menu {address:#010x} has negative item count {count}")]
    NegativeItemCount { address: u32, count: i32 },
    #[error(
        "menu layout at {address:#010x} has no complete terminator within the supplied source"
    )]
    MissingLayoutTerminator { address: u32 },
}

impl FrontendData<'_> {
    fn span(self, address: u32, bytes: usize) -> Result<ByteSpan, FrontendError> {
        let invalid = || FrontendError::AddressOutOfBounds {
            address,
            bytes,
            source_address: self.virtual_address,
            source_len: self.bytes.len(),
        };
        let start = address
            .checked_sub(self.virtual_address)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(invalid)?;
        let end = start.checked_add(bytes).ok_or_else(invalid)?;
        // The supplied range must not make a 32-bit pointer wrap, even on a
        // 64-bit host with a long input buffer.
        if end > self.bytes.len() || u64::from(address) + bytes as u64 > (1_u64 << 32) {
            return Err(invalid());
        }
        Ok(ByteSpan::new(start, end))
    }

    fn table(self, address: u32, count: u32, stride: usize) -> Result<ByteSpan, FrontendError> {
        let bytes = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(stride))
            .ok_or(FrontendError::TableSizeOverflow {
                count,
                record_bytes: stride,
            })?;
        self.span(address, bytes)
    }
}

/// Follow one authored definition and its item/layout pointers. Item counts
/// are bounded against the supplied range before allocation; layout walking
/// cannot leave that range. Runtime-owned bytes are read without resetting
/// them, so the same decoder can describe either static or sampled records.
pub fn parse_frontend_menu(
    source: FrontendData<'_>,
    address: u32,
) -> Result<FrontendMenu, FrontendError> {
    let definition_span = source.span(address, FRONTEND_MENU_DEFINITION_BYTES)?;
    let definition = &source.bytes[definition_span.start..definition_span.end];
    let items_address = word(definition, 0);
    let count = signed_word(definition, 4);
    if count < 0 {
        return Err(FrontendError::NegativeItemCount { address, count });
    }
    let layouts_address = word(definition, 8);
    let items = parse_frontend_menu_items(source, items_address, count as u32)?;
    let (layouts, layout_terminator) = parse_frontend_menu_layouts(source, layouts_address)?;
    Ok(FrontendMenu {
        virtual_address: address,
        definition_span,
        items_address,
        layouts_address,
        initial_selection: signed_word(definition, 12),
        flags: word(definition, 16),
        items,
        layouts,
        layout_terminator,
    })
}

/// `count` 32-byte item records at `address`, as `00452310` installs them.
pub fn parse_frontend_menu_items(
    source: FrontendData<'_>,
    address: u32,
    count: u32,
) -> Result<Vec<FrontendMenuItem>, FrontendError> {
    let item_span = source.table(address, count, FRONTEND_MENU_ITEM_BYTES)?;
    Ok(source.bytes[item_span.start..item_span.end]
        .chunks_exact(FRONTEND_MENU_ITEM_BYTES)
        .enumerate()
        .map(|(index, record)| FrontendMenuItem {
            virtual_address: address + (index * FRONTEND_MENU_ITEM_BYTES) as u32,
            span: record_span(item_span, index, FRONTEND_MENU_ITEM_BYTES),
            flags: word(record, 0),
            draw_callback: word(record, 4),
            draw_arguments: [word(record, 8), word(record, 12)],
            action_callback: word(record, 16),
            action_arguments: [word(record, 20), word(record, 24)],
            row_height: half(record, 28),
            age: half(record, 30),
        })
        .collect())
}

/// The layout chain at `address`, up to the first record whose first byte is
/// zero, and that terminator's span.
pub fn parse_frontend_menu_layouts(
    source: FrontendData<'_>,
    address: u32,
) -> Result<(Vec<FrontendMenuLayout>, ByteSpan), FrontendError> {
    let mut layouts = Vec::new();
    let mut layout_address = address;
    let terminator = loop {
        let span = source
            .span(layout_address, FRONTEND_MENU_LAYOUT_BYTES)
            .map_err(|_| FrontendError::MissingLayoutTerminator { address })?;
        let record = &source.bytes[span.start..span.end];
        if record[0] == 0 {
            break span;
        }
        layouts.push(FrontendMenuLayout {
            virtual_address: layout_address,
            span,
            flags: record[0],
            item_count: record[1],
            visible_rows: record[2],
            reserved: record[3],
            x: signed_half(record, 4),
            y: signed_half(record, 6),
            width: signed_half(record, 8),
            height: signed_half(record, 10),
            row_spacing: half(record, 12),
            scroll_animation: signed_half(record, 14),
        });
        layout_address = layout_address
            .checked_add(FRONTEND_MENU_LAYOUT_BYTES as u32)
            .ok_or(FrontendError::MissingLayoutTerminator { address })?;
    };
    Ok((layouts, terminator))
}

pub fn parse_frontend_scene_models(
    source: FrontendData<'_>,
    address: u32,
    count: u32,
) -> Result<Vec<FrontendSceneModel>, FrontendError> {
    let span = source.table(address, count, FRONTEND_SCENE_MODEL_BYTES)?;
    Ok(source.bytes[span.start..span.end]
        .chunks_exact(FRONTEND_SCENE_MODEL_BYTES)
        .enumerate()
        .map(|(index, record)| FrontendSceneModel {
            virtual_address: address + (index * FRONTEND_SCENE_MODEL_BYTES) as u32,
            span: record_span(span, index, FRONTEND_SCENE_MODEL_BYTES),
            model_slot: signed_half(record, 0),
            reserved_02: half(record, 2),
            rotation_controls: [
                signed_word(record, 4),
                signed_word(record, 8),
                signed_word(record, 12),
            ],
            position: [
                signed_half(record, 16),
                signed_half(record, 18),
                signed_half(record, 20),
            ],
            reserved_16: half(record, 22),
        })
        .collect())
}

pub fn parse_frontend_camera_presets(
    source: FrontendData<'_>,
    address: u32,
    count: u32,
) -> Result<Vec<FrontendCameraPreset>, FrontendError> {
    let span = source.table(address, count, FRONTEND_CAMERA_PRESET_BYTES)?;
    Ok(source.bytes[span.start..span.end]
        .chunks_exact(FRONTEND_CAMERA_PRESET_BYTES)
        .enumerate()
        .map(|(index, record)| FrontendCameraPreset {
            virtual_address: address + (index * FRONTEND_CAMERA_PRESET_BYTES) as u32,
            span: record_span(span, index, FRONTEND_CAMERA_PRESET_BYTES),
            eye: [
                signed_word(record, 0),
                signed_word(record, 4),
                signed_word(record, 8),
            ],
            target: [
                signed_word(record, 12),
                signed_word(record, 16),
                signed_word(record, 20),
            ],
        })
        .collect())
}

pub fn parse_frontend_sine_table(
    source: FrontendData<'_>,
    address: u32,
) -> Result<FrontendSineTable, FrontendError> {
    let span = source.span(address, FRONTEND_SINE_TABLE_ENTRIES * 2)?;
    let bytes = &source.bytes[span.start..span.end];
    Ok(FrontendSineTable {
        virtual_address: address,
        span,
        values: std::array::from_fn(|index| half(bytes, index * 2)),
    })
}

pub fn parse_frontend_inverse_sine_table(
    source: FrontendData<'_>,
    address: u32,
) -> Result<FrontendInverseSineTable, FrontendError> {
    let span = source.span(address, FRONTEND_INVERSE_SINE_TABLE_ENTRIES * 2)?;
    let bytes = &source.bytes[span.start..span.end];
    Ok(FrontendInverseSineTable {
        virtual_address: address,
        span,
        values: std::array::from_fn(|index| signed_half(bytes, index * 2)),
    })
}

pub fn parse_frontend_attract_selection_table(
    source: FrontendData<'_>,
    address: u32,
) -> Result<FrontendAttractSelectionTable, FrontendError> {
    let span = source.span(address, FRONTEND_ATTRACT_SELECTION_ENTRIES * 4)?;
    let bytes = &source.bytes[span.start..span.end];
    Ok(FrontendAttractSelectionTable {
        virtual_address: address,
        span,
        values: std::array::from_fn(|index| word(bytes, index * 4)),
    })
}

fn record_span(table: ByteSpan, index: usize, stride: usize) -> ByteSpan {
    let start = table.start + index * stride;
    ByteSpan::new(start, start + stride)
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded record"),
    )
}

fn signed_word(bytes: &[u8], offset: usize) -> i32 {
    word(bytes, offset) as i32
}

fn half(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("bounded record"),
    )
}

fn signed_half(bytes: &[u8], offset: usize) -> i16 {
    half(bytes, offset) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x0053_c008;

    #[test]
    fn inverse_sine_table_preserves_signed_words_and_requires_full_bounded_span() {
        let mut bytes = vec![0_u8; FRONTEND_INVERSE_SINE_TABLE_ENTRIES * 2];
        for (index, value) in [(0, i16::MIN), (1, -1), (1023, i16::MAX)] {
            bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        let source = FrontendData {
            bytes: &bytes,
            virtual_address: FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        };
        let table =
            parse_frontend_inverse_sine_table(source, FRONTEND_INVERSE_SINE_TABLE_ADDRESS).unwrap();
        assert_eq!(table.values[0], i16::MIN);
        assert_eq!(table.values[1], -1);
        assert_eq!(table.values[1023], i16::MAX);
        assert_eq!(table.span, ByteSpan::new(0, 2048));
        assert!(
            parse_frontend_inverse_sine_table(source, FRONTEND_INVERSE_SINE_TABLE_ADDRESS - 1,)
                .is_err()
        );
        assert!(
            parse_frontend_inverse_sine_table(source, FRONTEND_INVERSE_SINE_TABLE_ADDRESS + 1,)
                .is_err()
        );
        assert!(parse_frontend_inverse_sine_table(
            FrontendData {
                bytes: &bytes[..2047],
                ..source
            },
            FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )
        .is_err());
    }

    #[test]
    fn sine_table_preserves_unsigned_samples_and_requires_complete_bounded_source() {
        let bytes: Vec<_> = (0..4096_u16)
            .flat_map(|value| (value | 0x8000).to_le_bytes())
            .collect();
        let source = FrontendData {
            bytes: &bytes,
            virtual_address: FRONTEND_SINE_TABLE_ADDRESS,
        };
        let table = parse_frontend_sine_table(source, FRONTEND_SINE_TABLE_ADDRESS).unwrap();
        assert_eq!(table.span, ByteSpan::new(0, 8192));
        assert_eq!(table.values[0], 0x8000);
        assert_eq!(table.values[4095], 0x8fff);
        assert!(parse_frontend_sine_table(source, FRONTEND_SINE_TABLE_ADDRESS - 1).is_err());
        assert!(parse_frontend_sine_table(source, FRONTEND_SINE_TABLE_ADDRESS + 1).is_err());
        assert!(parse_frontend_sine_table(
            FrontendData {
                bytes: &bytes[..8191],
                ..source
            },
            FRONTEND_SINE_TABLE_ADDRESS
        )
        .is_err());
    }

    #[test]
    fn attract_selection_table_preserves_supplied_dwords_and_section_origin() {
        let values: [u32; FRONTEND_ATTRACT_SELECTION_ENTRIES] =
            std::array::from_fn(|index| 0x8000_0100 + index as u32);
        let mut bytes = vec![0x59; 8];
        bytes.extend(values.into_iter().flat_map(u32::to_le_bytes));
        let source = FrontendData {
            bytes: &bytes,
            virtual_address: FRONTEND_ATTRACT_SELECTION_ADDRESS - 8,
        };
        let table =
            parse_frontend_attract_selection_table(source, FRONTEND_ATTRACT_SELECTION_ADDRESS)
                .unwrap();
        assert_eq!(table.virtual_address, FRONTEND_ATTRACT_SELECTION_ADDRESS);
        assert_eq!(table.span, ByteSpan::new(8, 52));
        assert_eq!(table.values, values);
        assert_eq!(&bytes[..8], &[0x59; 8]);
        for short in 0..44 {
            assert!(parse_frontend_attract_selection_table(
                FrontendData {
                    bytes: &bytes[..8 + short],
                    ..source
                },
                FRONTEND_ATTRACT_SELECTION_ADDRESS,
            )
            .is_err());
        }
        assert!(
            parse_frontend_attract_selection_table(source, source.virtual_address - 1,).is_err()
        );
        assert!(parse_frontend_attract_selection_table(
            source,
            FRONTEND_ATTRACT_SELECTION_ADDRESS + 1,
        )
        .is_err());
    }

    // The retail root definition, three authored callback records, and one
    // layout plus sentinel. All pointers use their original preferred VAs.
    fn root_fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        for record in [
            [4_u32, 0x0048_35e0, 204, 0, 0x0048_3490, 0, 0, 0],
            [4, 0x0048_34f0, 158, 0, 0x0048_34b0, 0, 0x0053_9378, 0],
            [4, 0x0048_34f0, 210, 0, 0x0045_fd40, 0, 0, 0],
        ] {
            for word in record {
                bytes.extend(word.to_le_bytes());
            }
        }
        bytes.extend([2, 3, 3, 0, 0xf4, 1, 0x80, 2, 0xc2, 1, 0x84, 3, 50, 0, 0, 0]);
        bytes.extend([0; 16]);
        for word in [BASE, 3, BASE + 96, 0, 0] {
            bytes.extend(word.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn root_menu_preserves_callback_arguments_and_layout_provenance() {
        let bytes = root_fixture();
        let menu = parse_frontend_menu(
            FrontendData {
                bytes: &bytes,
                virtual_address: BASE,
            },
            FRONTEND_ROOT_MENU_ADDRESS,
        )
        .unwrap();
        assert_eq!(
            menu.items
                .iter()
                .map(|i| i.draw_arguments[0])
                .collect::<Vec<_>>(),
            [204, 158, 210]
        );
        assert_eq!(menu.items[0].action_callback, 0x0048_3490);
        assert_eq!(menu.items[1].action_arguments, [0, 0x0053_9378]);
        assert_eq!(menu.items[2].span, ByteSpan::new(64, 96));
        assert_eq!(menu.layout_terminator, ByteSpan::new(112, 128));
        let layout = menu.layouts[0];
        assert_eq!(
            (layout.x, layout.y, layout.width, layout.height),
            (500, 640, 450, 900)
        );
        assert_eq!(
            (layout.item_count, layout.visible_rows, layout.row_spacing),
            (3, 3, 50)
        );
    }

    #[test]
    fn runtime_fields_and_signed_coordinates_are_read_without_mutation() {
        let mut bytes = root_fixture();
        bytes[28..32].copy_from_slice(&[2, 0, 0xff, 0xff]);
        bytes[100..102].copy_from_slice(&(-123_i16).to_le_bytes());
        bytes[108..110].copy_from_slice(&60000_u16.to_le_bytes());
        bytes[110..112].copy_from_slice(&(-199_i16).to_le_bytes());
        bytes[113] = 0xff; // Only the first terminator byte has sentinel meaning.
        bytes[140..144].copy_from_slice(&(-1_i32).to_le_bytes());
        let original = bytes.clone();
        let menu = parse_frontend_menu(
            FrontendData {
                bytes: &bytes,
                virtual_address: BASE,
            },
            FRONTEND_ROOT_MENU_ADDRESS,
        )
        .unwrap();
        assert_eq!(menu.items[0].row_height, 2);
        assert_eq!(menu.items[0].age, 65535);
        assert_eq!(menu.layouts[0].x, -123);
        assert_eq!(menu.layouts[0].row_spacing, 60000);
        assert_eq!(menu.layouts[0].scroll_animation, -199);
        assert_eq!(menu.initial_selection, -1);
        assert_eq!(bytes, original);
    }

    #[test]
    fn rejects_external_pointers_negative_counts_and_missing_terminators() {
        let mut bytes = root_fixture();
        bytes[128..132].copy_from_slice(&(BASE - 4).to_le_bytes());
        assert!(matches!(
            parse_frontend_menu(
                FrontendData {
                    bytes: &bytes,
                    virtual_address: BASE
                },
                FRONTEND_ROOT_MENU_ADDRESS
            ),
            Err(FrontendError::AddressOutOfBounds { .. })
        ));
        bytes[132..136].copy_from_slice(&(-1_i32).to_le_bytes());
        assert!(matches!(
            parse_frontend_menu(
                FrontendData {
                    bytes: &bytes,
                    virtual_address: BASE
                },
                FRONTEND_ROOT_MENU_ADDRESS
            ),
            Err(FrontendError::NegativeItemCount { .. })
        ));
        let mut bytes = root_fixture();
        bytes[136..140].copy_from_slice(&(BASE + 140).to_le_bytes());
        assert!(matches!(
            parse_frontend_menu(
                FrontendData {
                    bytes: &bytes,
                    virtual_address: BASE
                },
                FRONTEND_ROOT_MENU_ADDRESS
            ),
            Err(FrontendError::MissingLayoutTerminator { .. })
        ));
    }

    #[test]
    fn scene_preserves_dynamic_rotation_controls_and_signed_position() {
        let mut bytes = Vec::new();
        bytes.extend(754_i16.to_le_bytes());
        bytes.extend(0xabcd_u16.to_le_bytes());
        for value in [-1_i32, -5, 49152] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [2000_i16, -2000, 8000] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(0x9876_u16.to_le_bytes());
        let records = parse_frontend_scene_models(
            FrontendData {
                bytes: &bytes,
                virtual_address: FRONTEND_SCENE_MODELS_ADDRESS,
            },
            FRONTEND_SCENE_MODELS_ADDRESS,
            1,
        )
        .unwrap();
        assert_eq!(records[0].model_slot, 754);
        assert_eq!(records[0].rotation_controls, [-1, -5, 49152]);
        assert_eq!(records[0].position, [2000, -2000, 8000]);
        assert_eq!(
            (records[0].reserved_02, records[0].reserved_16),
            (0xabcd, 0x9876)
        );
        assert_eq!(records[0].span, ByteSpan::new(0, 24));
    }

    #[test]
    fn camera_targets_remain_in_authored_units() {
        let bytes: Vec<_> = [-2200_i32, 800, -1800, 750, 800, -600]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect();
        let presets = parse_frontend_camera_presets(
            FrontendData {
                bytes: &bytes,
                virtual_address: FRONTEND_CAMERA_PRESETS_ADDRESS,
            },
            FRONTEND_CAMERA_PRESETS_ADDRESS,
            1,
        )
        .unwrap();
        assert_eq!(presets[0].eye, [-2200, 800, -1800]);
        assert_eq!(presets[0].target, [750, 800, -600]);
    }

    #[test]
    fn table_bounds_are_checked_before_count_allocation_or_address_wrap() {
        let bytes = [0; 24];
        for length in 0..24 {
            let source = FrontendData {
                bytes: &bytes[..length],
                virtual_address: 0x1000,
            };
            assert!(parse_frontend_scene_models(source, 0x1000, 1).is_err());
            assert!(parse_frontend_camera_presets(source, 0x1000, 1).is_err());
        }
        let source = FrontendData {
            bytes: &bytes,
            virtual_address: 0x1000,
        };
        assert!(parse_frontend_scene_models(source, 0x1000, u32::MAX).is_err());
        let source = FrontendData {
            bytes: &bytes,
            virtual_address: u32::MAX - 10,
        };
        assert!(parse_frontend_scene_models(source, u32::MAX - 10, 1).is_err());
    }
}
