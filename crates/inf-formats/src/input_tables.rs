//! Authored screen input records consumed by 00439B30 and 004F0470/004F0A40.
//!
//! Each screen constructor builds its own twelve digital table pointers and
//! twelve analog pointer cells on the stack: the frontend in 004838A0, the
//! phase 9/10/11 text-entry screen in 00459060, the phase-1 title screen in
//! 00404620 (inline 0x60-byte struct: twelve digital addresses at +0x00,
//! twelve analog cells at +0x30, all zero for the title) and the phase-3
//! briefing screen in 0040A420 (same inline shape; analog slot 7's DWORD is
//! never written and stays empty).
//!
//! Sources remain bounded original .data/.rdata bytes. Spans here are executable
//! FILE spans, unlike the section-relative spans in the older frontend tables.
//! Device slots and byte codes retain their native identities; no platform key
//! mapping, callback execution, consumer allocation, or timer is manufactured.

use serde::Serialize;
use thiserror::Error;

use crate::frontend_executable::FrontendExecutableRange;
use crate::{ByteSpan, FrontendExecutable, FrontendExecutableError};

pub const INPUT_DEVICE_SLOTS: usize = 12;
pub const DIGITAL_INPUT_RECORD_BYTES: usize = 20;
pub const ANALOG_INPUT_RECORD_BYTES: usize = 24;
pub const DIGITAL_ACTION_DESCRIPTOR_BYTES: usize = 8;
pub const ANALOG_ACTION_DESCRIPTOR_BYTES: usize = 12;

/// 004838A0 constructs these twelve pointers on its stack, followed by the
/// twelve analog pointers. Equal roots still produce distinct per-device sets.
pub const FRONTEND_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_80b0,
    0,
    0x0052_8218,
    0,
    0,
    0,
    0x0052_8370,
    0x0052_8370,
    0x0052_8410,
    0x0052_8410,
    0,
    0,
];

/// Nonzero entries name pointer CELLS read by 004838A0, not table addresses.
pub const FRONTEND_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [
    0,
    0,
    0x0053_3398,
    0,
    0,
    0,
    0,
    0,
    0x0053_33e8,
    0x0053_33e8,
    0,
    0,
];

/// 00459060 stores these twelve pointers between 00459093 and 0045914B
/// (slots 6..9 at 0045909A/9E/D8/DC, the rest from 004590FF). Slot 2's
/// table has no terminator of its own and runs on into the slots 6/7 table.
pub const TEXT_ENTRY_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_5c58,
    0,
    0x0052_5d48,
    0,
    0,
    0,
    0x0052_5e88,
    0x0052_5e88,
    0x0052_5f40,
    0x0052_5f40,
    0,
    0,
];

/// 00459060 reads the same two analog pointer cells as the frontend
/// (004590D2 for slot 2, 004590E0 for slots 8/9) into its own stack array
/// (stores 0045912F and 004590E5/E9).
pub const TEXT_ENTRY_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [
    0,
    0,
    0x0053_3398,
    0,
    0,
    0,
    0,
    0,
    0x0053_33e8,
    0x0053_33e8,
    0,
    0,
];

/// 0040A420 stores its inline twelve-slot arrays on the stack: digital roots
/// slot 0 00520028, slot 2 005200E0, slots 6/7 00520238 and slots 8/9
/// 005202A0, with the shared analog pointer cells 00533398 (slot 2) and
/// 005333E8 (slots 8/9). Analog slot 7's stack DWORD is never written by the
/// constructor, so it is encoded empty: no authored device type consumes it.
pub const PHASE3_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_0028,
    0,
    0x0052_00e0,
    0,
    0,
    0,
    0x0052_0238,
    0x0052_0238,
    0x0052_02a0,
    0x0052_02a0,
    0,
    0,
];

/// The shared cells match the frontend/text-entry layout; slot 7 is zero
/// because the constructor leaves that stack DWORD unwritten (see above),
/// consistently with the frontend's six null digital slots.
pub const PHASE3_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [
    0,
    0,
    0x0053_3398,
    0,
    0,
    0,
    0,
    0,
    0x0053_33e8,
    0x0053_33e8,
    0,
    0,
];

/// 00404620 stores these twelve pointers in its inline stack struct
/// (slots 6/7 share 0x51F378, slots 8/9 share 0x51F4A8). Slot 0 is the
/// keyboard: skip-video `00404870` on Esc/Space/Return, then the shared Alt
/// system records; the remaining tables are all skip-video records.
pub const TITLE_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0051_f300,
    0,
    0,
    0,
    0,
    0,
    0x0051_f378,
    0x0051_f378,
    0x0051_f4a8,
    0x0051_f4a8,
    0,
    0,
];

/// The title constructor zeroes every analog cell: the title has no analog
/// input, so every slot stays empty.
pub const TITLE_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// 00440990 builds two inline twelve-slot digital arrays. The screen struct
/// holds slot 0 `00534290` (keyboard: gameplay actions plus Alt+Enter),
/// slots 6/7 `00522A78` and slots 8/9 `00522AE0`. A second enumeration for
/// the `+0xFC` sub-object (constructed after the `00488AA0` player lookup)
/// holds slot 0 `00534570` (the large `00469xxx` vehicle/state family),
/// slot 2 `00534CC0`, slots 6/7 `00522D98` and slots 8/9 `00522FB8`.
/// Slots 6/7 and 8/9 share one table each; `00522A78` repeats the exact rows
/// of `00522AE0`, and `00522D98` repeats `00522FB8`, from distinct tables.
///
/// Analog input is not encoded here: both constructors store table addresses
/// (some snapshotted from the shared cells, some immediate) whose
/// descriptors live in runtime-written memory. The screen struct reads the
/// current contents of `00533398`/`005333E8` into analog slots 2 and 8/9
/// ([`GAMEPLAY_ANALOG_TABLE_POINTERS`]); the sub-object struct carries the
/// tables themselves ([`GAMEPLAY_SUB_ANALOG_TABLE_ADDRESSES`]).
pub const GAMEPLAY_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0053_4290,
    0,
    0,
    0,
    0,
    0,
    0x0052_2a78,
    0x0052_2a78,
    0x0052_2ae0,
    0x0052_2ae0,
    0,
    0,
];

/// The gameplay screen's analog cells (`004409D1`, `004409EC`): slot 2 the
/// joystick's menu axes (`00533350`), slots 8/9 the pad's (`005333A0`).
pub const GAMEPLAY_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [
    0,
    0,
    0x0053_3398,
    0,
    0,
    0,
    0,
    0,
    0x0053_33e8,
    0x0053_33e8,
    0,
    0,
];

/// The `+0xFC` sub-object's analog tables, stored directly (`00440AE7`,
/// `00440AEF`): slot 2 `005231D8`, the joystick's X to the player's `+5C`
/// (scale 4), Y to `+60` and Z to `+64` (both -4); slots 8/9 `00523238`,
/// the pad's axes 0, 1 and 3 to the same three at 1024, -1024 and -1024.
/// Their descriptors lie in `.data`'s zero fill.
pub const GAMEPLAY_SUB_ANALOG_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0,
    0,
    0x0052_31d8,
    0,
    0,
    0,
    0,
    0,
    0x0052_3238,
    0x0052_3238,
    0,
    0,
];

/// Digital roots of the `+0xFC` sub-object enumeration in `00440990`.
pub const GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0053_4570,
    0,
    0x0053_4cc0,
    0,
    0,
    0,
    0x0052_2d98,
    0x0052_2d98,
    0x0052_2fb8,
    0x0052_2fb8,
    0,
    0,
];

/// 0042AB00 builds one inline twelve-slot digital array: slot 0 `005212A8`
/// (17-row keyboard over results callbacks plus Alt+Enter/Alt+Esc; no
/// Alt+Enter-noop), slot 2 `00521410` (16 joystick buttons on one callback),
/// slots 6/7 `00521568` and slots 8/9 `00521648` (10 rows each, identical
/// content from distinct tables). Every analog cell is zero: the results
/// screen has no analog input and reads no shared pointer cells.
pub const PHASE8_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_12a8,
    0,
    0x0052_1410,
    0,
    0,
    0,
    0x0052_1568,
    0x0052_1568,
    0x0052_1648,
    0x0052_1648,
    0,
    0,
];

pub const PHASE8_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// 00432500 builds one inline twelve-slot digital array: slot 0 `00521AE0`
/// (keyboard: Return without Alt and Esc accept through `004327B0`, Left
/// `004327C0` and Right `004327E0` turn the carousel, plus Alt+Enter), slot
/// 2 `00521B58` (16 joystick buttons on the accept callback), slots 6/7
/// `00521CB0` and slots 8/9 `00521D28` (5 rows each, identical content from
/// distinct tables). Every stack DWORD is written before the `push esi`
/// that precedes `00439B30`; the base is the slot-0 cell, as on the title.
/// Analog slot 2 snapshots the current contents of the shared cell
/// `00533398` (a direct table address, same calibration boundary as the
/// gameplay screen); every other analog cell is zero, so no analog
/// pointer constants are encoded. The keyboard has no Alt+Esc exit row.
pub const STARTUP_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_1ae0,
    0,
    0x0052_1b58,
    0,
    0,
    0,
    0x0052_1cb0,
    0x0052_1cb0,
    0x0052_1d28,
    0x0052_1d28,
    0,
    0,
];

/// 00451020 builds one inline twelve-slot digital array: slot 0 `00525120`
/// (13-row keyboard over map callbacks plus Alt+Enter/Alt+Esc; no noop),
/// slot 2 `00525238` (16 joystick buttons on one callback), slots 6/7
/// `00525390` and slots 8/9 `00525498` (12 rows each, identical content
/// from distinct tables). Analog slot 2 snapshots the current contents of
/// the shared cell `00533398` and slots 8/9 the contents of `005333E8`
/// (direct table addresses, same calibration boundary as the gameplay
/// screen); every other analog cell is zero, so no analog pointer
/// constants are encoded. Unlike the briefing constructor, every analog
/// DWORD is written: there is no unwritten slot here.
pub const PHASE7_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_5120,
    0,
    0x0052_5238,
    0,
    0,
    0,
    0x0052_5390,
    0x0052_5390,
    0x0052_5498,
    0x0052_5498,
    0,
    0,
];

/// No analog table is encoded for the 00443190 in-world menu screen.
pub const PHASE5_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// Resolve the 00443190 in-world menu constructor policy. Its keyboard
/// rows bind the context-resolving wrappers `004434A0`/`004434D0`/`00443500`/
/// `00443530` on the arrows, `00443560` on Return and `00443590` on Escape
/// and Tab, each pressing a held bit of the current screen (`00439430`),
/// and the menu key `00443700` on Shift+Esc and P.
pub fn parse_phase5_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        PHASE5_DIGITAL_TABLE_ADDRESSES,
        PHASE5_ANALOG_TABLE_POINTERS,
    )
}

/// 00444BD0 builds one inline twelve-slot digital array: slot 0 `00524B90`
/// (10-row keyboard: back `004451E0` on Esc and G, Return without Alt
/// `00445200`, the arrows `00445220..00445280`, plus Alt+Enter, Alt+F1 and
/// Alt+Esc), slot 2 `00524C70` (16 joystick buttons on `00445200`). Analog
/// slot 2 snapshots the current contents of the shared cell `00533398` (a
/// direct table address, same calibration boundary as the gameplay
/// screen); every other analog cell is zero, so no analog pointer
/// constants are encoded.
pub const PHASE12_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] =
    [0x0052_4b90, 0, 0x0052_4c70, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// No analog table is encoded for the 00444BD0 key configuration screen.
pub const PHASE12_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// Resolve the 00444BD0 key configuration constructor policy.
pub fn parse_phase12_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        PHASE12_DIGITAL_TABLE_ADDRESSES,
        PHASE12_ANALOG_TABLE_POINTERS,
    )
}

/// 00443190 builds one inline twelve-slot digital array: slot 0 `00522B48`
/// (10-row keyboard over gameplay action callbacks plus Alt+Enter; no
/// Alt+Esc exit), slot 2 `00534418` (16 rows), slots 6/7 `00522C28` and
/// slots 8/9 `00522CE0` (8 rows each, identical content from distinct
/// tables). Analog input is not encoded: the constructor snapshots the
/// current contents of the shared cells `00533398`/`005333E8`, whose
/// descriptors live in runtime-written memory (same calibration boundary
/// as the gameplay screen).
pub const PHASE5_DIGITAL_TABLE_ADDRESSES: [u32; INPUT_DEVICE_SLOTS] = [
    0x0052_2b48,
    0,
    0x0053_4418,
    0,
    0,
    0,
    0x0052_2c28,
    0x0052_2c28,
    0x0052_2ce0,
    0x0052_2ce0,
    0,
    0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DigitalActionDescriptor {
    pub identity: u32,
    pub file_span: ByteSpan,
    /// Zero means no callback; the descriptor can still own a published value.
    pub callback_address: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AnalogActionDescriptor {
    pub identity: u32,
    /// None for a descriptor in the loader's zero fill
    /// ([`parse_direct_analog_input_table`]): its words are zero.
    pub file_span: Option<ByteSpan>,
    pub callback_address: u32,
    pub flags: u32,
    /// +8 is the raw microsecond interval passed to 004A48C0; zero is untimed.
    pub period_micros: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DigitalInputRecord {
    pub virtual_address: u32,
    pub file_span: ByteSpan,
    pub descriptor: DigitalActionDescriptor,
    /// Source A at +4; source B at +8. Each list stops at its first zero byte.
    pub required: [[u8; 4]; 2],
    /// Source A at +12; source B at +16. Bytes after zero are preserved.
    pub forbidden: [[u8; 4]; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AnalogInputRecord {
    pub virtual_address: u32,
    pub file_span: ByteSpan,
    pub descriptor: AnalogActionDescriptor,
    /// +4 indexes native device words; this decoder has no device array bounds.
    pub axis: u32,
    /// 004F0C66 reads the FIRST record's +8 policy for every record in the set.
    /// Keep every original word; do not reinterpret this as per-row policy.
    pub flags: u32,
    pub offset: i32,
    pub scale_q31: i32,
    pub multiplier: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DigitalInputTable {
    pub virtual_address: u32,
    pub records: Vec<DigitalInputRecord>,
    /// Only the descriptor word must be zero; retail does not read the tail.
    pub terminator: ByteSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnalogInputTable {
    pub virtual_address: u32,
    pub records: Vec<AnalogInputRecord>,
    pub terminator: ByteSpan,
}

/// One screen constructor's resolved per-device-slot tables.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScreenInputTables {
    pub digital: [Option<DigitalInputTable>; INPUT_DEVICE_SLOTS],
    pub analog: [Option<AnalogInputTable>; INPUT_DEVICE_SLOTS],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InputTableError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(
        "input table at {address:#010x} has no descriptor terminator within its mapped source"
    )]
    MissingTerminator { address: u32 },
}

pub fn parse_digital_action_descriptor(
    source: FrontendExecutable<'_>,
    address: u32,
) -> Result<DigitalActionDescriptor, InputTableError> {
    let range = source.mapped(address, DIGITAL_ACTION_DESCRIPTOR_BYTES)?;
    Ok(DigitalActionDescriptor {
        identity: address,
        file_span: range.file_span,
        callback_address: word(range.bytes, 0),
        flags: word(range.bytes, 4),
    })
}

pub fn parse_analog_action_descriptor(
    source: FrontendExecutable<'_>,
    address: u32,
) -> Result<AnalogActionDescriptor, InputTableError> {
    let range = source.mapped(address, ANALOG_ACTION_DESCRIPTOR_BYTES)?;
    Ok(AnalogActionDescriptor {
        identity: address,
        file_span: Some(range.file_span),
        callback_address: word(range.bytes, 0),
        flags: word(range.bytes, 4),
        period_micros: word(range.bytes, 8),
    })
}

pub fn parse_digital_input_table(
    source: FrontendExecutable<'_>,
    address: u32,
) -> Result<DigitalInputTable, InputTableError> {
    let (ranges, terminator) = table_ranges(source, address, DIGITAL_INPUT_RECORD_BYTES)?;
    let records = ranges
        .into_iter()
        .map(|range| {
            let bytes = range.bytes;
            Ok(DigitalInputRecord {
                virtual_address: range.virtual_address,
                file_span: range.file_span,
                descriptor: parse_digital_action_descriptor(source, word(bytes, 0))?,
                required: [codes(bytes, 4), codes(bytes, 8)],
                forbidden: [codes(bytes, 12), codes(bytes, 16)],
            })
        })
        .collect::<Result<_, InputTableError>>()?;
    Ok(DigitalInputTable {
        virtual_address: address,
        records,
        terminator,
    })
}

pub fn parse_analog_input_table(
    source: FrontendExecutable<'_>,
    address: u32,
) -> Result<AnalogInputTable, InputTableError> {
    parse_direct_analog_input_table(source, address, &[])
}

/// An analog table whose records may name descriptors in the loader's zero
/// fill, beyond the file: `zero_fill` lists those identities, which the
/// caller has proven lie there (the player's `0054ECF8`, `0054ECE8` and
/// `0054ED08`). Their callback, flags and period read zero; any other
/// descriptor must be file-backed.
pub fn parse_direct_analog_input_table(
    source: FrontendExecutable<'_>,
    address: u32,
    zero_fill: &[u32],
) -> Result<AnalogInputTable, InputTableError> {
    let descriptor = |identity: u32| {
        if zero_fill.contains(&identity) {
            Ok(AnalogActionDescriptor {
                identity,
                file_span: None,
                callback_address: 0,
                flags: 0,
                period_micros: 0,
            })
        } else {
            parse_analog_action_descriptor(source, identity)
        }
    };
    let (ranges, terminator) = table_ranges(source, address, ANALOG_INPUT_RECORD_BYTES)?;
    let records = ranges
        .into_iter()
        .map(|range| {
            let bytes = range.bytes;
            Ok(AnalogInputRecord {
                virtual_address: range.virtual_address,
                file_span: range.file_span,
                descriptor: descriptor(word(bytes, 0))?,
                axis: word(bytes, 4),
                flags: word(bytes, 8),
                offset: word(bytes, 12) as i32,
                scale_q31: word(bytes, 16) as i32,
                multiplier: word(bytes, 20) as i32,
            })
        })
        .collect::<Result<_, InputTableError>>()?;
    Ok(AnalogInputTable {
        virtual_address: address,
        records,
        terminator,
    })
}

/// Resolve the exact 004838A0 constructor policy. Device enumeration still
/// decides which slots instantiate sets; this function merely decodes data.
pub fn parse_frontend_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        FRONTEND_DIGITAL_TABLE_ADDRESSES,
        FRONTEND_ANALOG_TABLE_POINTERS,
    )
}

/// Resolve the 00459060 text-entry constructor policy.
pub fn parse_text_entry_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        TEXT_ENTRY_DIGITAL_TABLE_ADDRESSES,
        TEXT_ENTRY_ANALOG_TABLE_POINTERS,
    )
}

/// Resolve the 0040A420 briefing constructor policy. Analog slot 7 has no
/// authored pointer cell (the constructor leaves that stack DWORD unwritten)
/// and stays empty.
pub fn parse_phase3_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        PHASE3_DIGITAL_TABLE_ADDRESSES,
        PHASE3_ANALOG_TABLE_POINTERS,
    )
}

/// Resolve the 0042AB00 results constructor policy. Every analog cell is
/// zero, so no pointer dereference can fail here.
pub fn parse_phase8_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        PHASE8_DIGITAL_TABLE_ADDRESSES,
        PHASE8_ANALOG_TABLE_POINTERS,
    )
}

/// Analog slot 2 of the 00432500 startup screen snapshots the runtime cell
/// `00533398`, the same calibration boundary as the gameplay screen, so no
/// analog table is encoded; every other analog cell is zero.
pub const STARTUP_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// The 00451020 map screen's analog slots 2 and 8/9 snapshot the runtime
/// cells `00533398` and `005333E8`, the same calibration boundary as the
/// gameplay screen, so no analog table is encoded; every other analog cell
/// is zero.
pub const PHASE7_ANALOG_TABLE_POINTERS: [u32; INPUT_DEVICE_SLOTS] = [0; INPUT_DEVICE_SLOTS];

/// Resolve the 00451020 map screen constructor policy.
pub fn parse_phase7_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        PHASE7_DIGITAL_TABLE_ADDRESSES,
        PHASE7_ANALOG_TABLE_POINTERS,
    )
}

/// Resolve the 00432500 startup (language) constructor policy.
pub fn parse_startup_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        STARTUP_DIGITAL_TABLE_ADDRESSES,
        STARTUP_ANALOG_TABLE_POINTERS,
    )
}

/// Resolve the 00404620 title constructor policy.
pub fn parse_title_input_tables(
    source: FrontendExecutable<'_>,
) -> Result<ScreenInputTables, InputTableError> {
    parse_screen_input_tables(
        source,
        TITLE_DIGITAL_TABLE_ADDRESSES,
        TITLE_ANALOG_TABLE_POINTERS,
    )
}

/// Digital roots are table addresses; nonzero analog entries are pointer
/// cells read at construction, whose zero value leaves the slot empty.
pub fn parse_screen_input_tables(
    source: FrontendExecutable<'_>,
    digital_tables: [u32; INPUT_DEVICE_SLOTS],
    analog_pointers: [u32; INPUT_DEVICE_SLOTS],
) -> Result<ScreenInputTables, InputTableError> {
    let mut digital = std::array::from_fn(|_| None);
    let mut analog = std::array::from_fn(|_| None);
    for (slot, address) in digital_tables.into_iter().enumerate() {
        if address != 0 {
            digital[slot] = Some(parse_digital_input_table(source, address)?);
        }
    }
    for (slot, pointer) in analog_pointers.into_iter().enumerate() {
        if pointer != 0 {
            let address = source.word(pointer)?;
            if address != 0 {
                analog[slot] = Some(parse_analog_input_table(source, address)?);
            }
        }
    }
    Ok(ScreenInputTables { digital, analog })
}

/// [`parse_screen_input_tables`] for a constructor that stores analog table
/// addresses directly, whose descriptors may lie in `zero_fill`
/// ([`parse_direct_analog_input_table`]).
pub fn parse_screen_input_tables_direct(
    source: FrontendExecutable<'_>,
    digital_tables: [u32; INPUT_DEVICE_SLOTS],
    analog_tables: [u32; INPUT_DEVICE_SLOTS],
    zero_fill: &[u32],
) -> Result<ScreenInputTables, InputTableError> {
    let mut tables = parse_screen_input_tables(source, digital_tables, [0; INPUT_DEVICE_SLOTS])?;
    for (slot, address) in analog_tables.into_iter().enumerate() {
        if address != 0 {
            tables.analog[slot] =
                Some(parse_direct_analog_input_table(source, address, zero_fill)?);
        }
    }
    Ok(tables)
}

fn table_ranges(
    source: FrontendExecutable<'_>,
    address: u32,
    stride: usize,
) -> Result<(Vec<FrontendExecutableRange<'_>>, ByteSpan), InputTableError> {
    let mut ranges = Vec::new();
    let mut current = address;
    loop {
        // A native table belongs to one original section. Mapping only each
        // successive row would allow a missing sentinel to cross into an
        // adjacent section and accidentally consume unrelated data there.
        let offset = (current - address) as usize;
        source
            .mapped(address, offset + 4)
            .map_err(|_| InputTableError::MissingTerminator { address })?;
        let head = source
            .mapped(current, 4)
            .map_err(|_| InputTableError::MissingTerminator { address })?;
        if word(head.bytes, 0) == 0 {
            return Ok((ranges, head.file_span));
        }
        source.mapped(address, offset + stride)?;
        ranges.push(source.mapped(current, stride)?);
        current = current
            .checked_add(stride as u32)
            .ok_or(InputTableError::MissingTerminator { address })?;
    }
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded input record"),
    )
}

fn codes(bytes: &[u8], offset: usize) -> [u8; 4] {
    bytes[offset..offset + 4]
        .try_into()
        .expect("bounded input record")
}

#[cfg(test)]
#[path = "input_tables_tests.rs"]
mod tests;
