use serde::Serialize;

use crate::process::{plausible_heap_pointer, u16_at, u32_at, Process};

const HANDLER_COUNT_ADDRESS: usize = 0x0053_AA10;
const HANDLER_TABLE_ADDRESS: usize = 0x0053_AA14;
const MODEL_HANDLER_INDEX: u32 = 8;
const MODEL_HANDLER_ADDRESS: usize = HANDLER_TABLE_ADDRESS + MODEL_HANDLER_INDEX as usize * 4;
const MODEL_HANDLER_OBJECT: u32 = 0x0055_EEE0;
const MODEL_HANDLER_VTABLE: u32 = 0x0052_76D8;
const EXPECTED_HANDLER_COUNT: u32 = 16;
const EXPECTED_SLOT_CAPACITY: u32 = 760;
const MAX_SLOT_CAPACITY: u32 = 8192;
const MODEL_HEADER_BYTES: usize = 0x20;
const MAX_MODEL_NAME_BYTES: usize = 256;
const MAX_MODEL_ENTRY_BYTES: u32 = u16::MAX as u32;
const MAX_ALIGNED_MODEL_ENTRY_BYTES: u32 = MAX_MODEL_ENTRY_BYTES & !3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandlerState {
    pub pointer: u32,
    pub vtable: u32,
    pub slot_table: u32,
    pub slot_capacity: u32,
    pub handler_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelHeader {
    pub visibility_extent: u32,
    pub command_word_count: u16,
    pub auxiliary_count: u8,
    pub flags: u8,
    pub vertex_slot_count: u16,
    pub raw_vertex_count: u16,
    pub shade_slot_count: u16,
    pub raw_0e: u16,
    pub reserved_10: u16,
    pub vertex_ranges: [u16; 4],
    pub reserved_1a: u16,
    pub shade_ranges: [u16; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelEntryDescriptor {
    pub header: ModelHeader,
    pub content_bytes: u32,
    pub aligned_bytes: u32,
    pub name: Option<String>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotState {
    Empty,
    Stable,
    Unstable,
    Unreadable,
    ImplausiblePointer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelSlot {
    pub global_id: u32,
    pub pointer: u32,
    pub state: SlotState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<ModelEntryDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelTableSnapshot {
    pub record_kind: &'static str,
    pub stable: bool,
    pub handler_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<HandlerState>,
    pub active_slots: usize,
    pub slots: Vec<ModelSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ModelTableSnapshot {
    fn error(handler_count: u32, message: impl Into<String>) -> Self {
        Self {
            record_kind: "model_table_snapshot",
            stable: false,
            handler_count,
            handler: None,
            active_slots: 0,
            slots: Vec::new(),
            error: Some(message.into()),
        }
    }

    pub fn pointer_hash(&self) -> String {
        let mut bytes = Vec::with_capacity(self.slots.len() * 4);
        for slot in &self.slots {
            bytes.extend_from_slice(&slot.pointer.to_le_bytes());
        }
        format!("{:016X}", fnv1a64(&bytes))
    }
}

pub fn capture(process: &Process) -> ModelTableSnapshot {
    capture_with_entries(process, true)
}

pub fn capture_pointers(process: &Process) -> ModelTableSnapshot {
    capture_with_entries(process, false)
}

fn capture_with_entries(process: &Process, include_entries: bool) -> ModelTableSnapshot {
    let handler_count = match process.read_u32(HANDLER_COUNT_ADDRESS) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(0, error),
    };
    if handler_count != EXPECTED_HANDLER_COUNT {
        return ModelTableSnapshot::error(
            handler_count,
            format!(
                "handler count changed: expected {EXPECTED_HANDLER_COUNT}, found {handler_count}"
            ),
        );
    }

    let first = match read_handler(process) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    if let Err(error) = validate_handler(&first) {
        return ModelTableSnapshot::error(handler_count, error);
    }
    let table_size = first.slot_capacity as usize * 4;
    let table_a = match process.read_bytes(first.slot_table as usize, table_size) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    let table_b = match process.read_bytes(first.slot_table as usize, table_size) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    let second = match read_handler(process) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    if first != second || table_a != table_b {
        return ModelTableSnapshot {
            record_kind: "model_table_snapshot",
            stable: false,
            handler_count,
            handler: Some(second),
            active_slots: 0,
            slots: Vec::new(),
            error: Some("duplicate Handler-8 table reads did not match".into()),
        };
    }

    let mut slots = Vec::with_capacity(first.slot_capacity as usize);
    for global_id in 0..first.slot_capacity {
        let offset = global_id as usize * 4;
        let pointer = u32_at(&table_a, offset);
        slots.push(if include_entries {
            read_slot(process, global_id, pointer)
        } else {
            pointer_only_slot(global_id, pointer)
        });
    }
    let active_slots = slots.iter().filter(|slot| slot.pointer != 0).count();
    let final_handler = match read_handler(process) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    if let Err(error) = validate_handler(&final_handler) {
        return ModelTableSnapshot::error(handler_count, error);
    }
    let final_table = match process.read_bytes(final_handler.slot_table as usize, table_size) {
        Ok(value) => value,
        Err(error) => return ModelTableSnapshot::error(handler_count, error),
    };
    let bracket_stable = first == final_handler && table_a == final_table;
    let stable = bracket_stable
        && slots
            .iter()
            .all(|slot| matches!(slot.state, SlotState::Empty | SlotState::Stable));
    ModelTableSnapshot {
        record_kind: "model_table_snapshot",
        stable,
        handler_count,
        handler: Some(final_handler),
        active_slots,
        slots,
        error: (!bracket_stable)
            .then(|| "Handler-8 object or table changed while entries were sampled".into()),
    }
}

fn read_handler(process: &Process) -> Result<HandlerState, String> {
    let pointer = process.read_u32(MODEL_HANDLER_ADDRESS)?;
    if !plausible_heap_pointer(pointer as usize) {
        return Err(format!("implausible Handler-8 pointer {pointer:#010X}"));
    }
    let bytes = process.read_bytes(pointer as usize, 16)?;
    Ok(HandlerState {
        pointer,
        vtable: u32_at(&bytes, 0),
        slot_table: u32_at(&bytes, 4),
        slot_capacity: u32_at(&bytes, 8),
        handler_index: u32_at(&bytes, 12),
    })
}

fn validate_handler(handler: &HandlerState) -> Result<(), String> {
    if handler.pointer != MODEL_HANDLER_OBJECT {
        return Err(format!(
            "Handler-8 object changed: expected {MODEL_HANDLER_OBJECT:#010X}, found {:#010X}",
            handler.pointer
        ));
    }
    if handler.vtable != MODEL_HANDLER_VTABLE {
        return Err(format!(
            "Handler-8 vtable changed: expected {MODEL_HANDLER_VTABLE:#010X}, found {:#010X}",
            handler.vtable
        ));
    }
    if handler.handler_index != MODEL_HANDLER_INDEX {
        return Err(format!(
            "Handler-8 object reports index {}, expected {MODEL_HANDLER_INDEX}",
            handler.handler_index
        ));
    }
    if handler.slot_capacity != EXPECTED_SLOT_CAPACITY {
        return Err(format!(
            "Handler-8 slot capacity changed: expected {EXPECTED_SLOT_CAPACITY}, found {}",
            handler.slot_capacity
        ));
    }
    if handler.slot_capacity > MAX_SLOT_CAPACITY {
        return Err(format!(
            "Handler-8 slot capacity {} exceeds safety bound {MAX_SLOT_CAPACITY}",
            handler.slot_capacity
        ));
    }
    if !plausible_heap_pointer(handler.slot_table as usize) {
        return Err(format!(
            "implausible Handler-8 slot table {:#010X}",
            handler.slot_table
        ));
    }
    Ok(())
}

fn pointer_only_slot(global_id: u32, pointer: u32) -> ModelSlot {
    let (state, error) = if pointer == 0 {
        (SlotState::Empty, None)
    } else if plausible_heap_pointer(pointer as usize) {
        (SlotState::Stable, None)
    } else {
        (
            SlotState::ImplausiblePointer,
            Some(format!("implausible model pointer {pointer:#010X}")),
        )
    };
    ModelSlot {
        global_id,
        pointer,
        state,
        descriptor: None,
        error,
    }
}

fn read_slot(process: &Process, global_id: u32, pointer: u32) -> ModelSlot {
    if pointer == 0 {
        return ModelSlot {
            global_id,
            pointer,
            state: SlotState::Empty,
            descriptor: None,
            error: None,
        };
    }
    if !plausible_heap_pointer(pointer as usize) {
        return ModelSlot {
            global_id,
            pointer,
            state: SlotState::ImplausiblePointer,
            descriptor: None,
            error: Some(format!("implausible model pointer {pointer:#010X}")),
        };
    }
    match read_entry(process, pointer as usize) {
        Ok((first_bytes, descriptor)) => match read_entry(process, pointer as usize) {
            Ok((second_bytes, second_descriptor))
                if first_bytes == second_bytes && descriptor == second_descriptor =>
            {
                ModelSlot {
                    global_id,
                    pointer,
                    state: SlotState::Stable,
                    descriptor: Some(descriptor),
                    error: None,
                }
            }
            Ok(_) => ModelSlot {
                global_id,
                pointer,
                state: SlotState::Unstable,
                descriptor: None,
                error: Some("duplicate model-entry reads did not match".into()),
            },
            Err(error) => ModelSlot {
                global_id,
                pointer,
                state: SlotState::Unstable,
                descriptor: None,
                error: Some(format!("second model-entry read failed: {error}")),
            },
        },
        Err(error) => ModelSlot {
            global_id,
            pointer,
            state: SlotState::Unreadable,
            descriptor: None,
            error: Some(error),
        },
    }
}

fn read_entry(
    process: &Process,
    pointer: usize,
) -> Result<(Vec<u8>, ModelEntryDescriptor), String> {
    let header_bytes = process.read_bytes(pointer, MODEL_HEADER_BYTES)?;
    let header = decode_header(&header_bytes);
    validate_model_header(&header, pointer)?;
    let raw_normal_count = header
        .shade_slot_count
        .checked_sub(2)
        .filter(|value| value % 2 == 0)
        .ok_or_else(|| {
            format!(
                "invalid shade-slot count {} at {pointer:08X}",
                header.shade_slot_count
            )
        })?
        / 2;
    let vector_count = u32::from(header.raw_vertex_count) + u32::from(raw_normal_count);
    let base_bytes = MODEL_HEADER_BYTES as u32
        + u32::from(header.auxiliary_count) * 4
        + vector_count * 6
        + u32::from(header.command_word_count) * 2;
    let remaining_entry_bytes = remaining_bounded_entry_bytes(base_bytes, pointer)?;
    let mut content_bytes = base_bytes;
    let mut name = None;
    if header.flags & 0x40 == 0 {
        let mut name_bytes = Vec::new();
        let scan_limit = MAX_MODEL_NAME_BYTES.min(remaining_entry_bytes as usize);
        let mut terminated = false;
        for index in 0..scan_limit {
            let byte = process.read_bytes(pointer + base_bytes as usize + index, 1)?[0];
            content_bytes = content_bytes
                .checked_add(1)
                .ok_or_else(|| "model content length overflow".to_string())?;
            if byte == 0 {
                terminated = true;
                break;
            }
            name_bytes.push(byte);
        }
        if !terminated {
            return Err(format!(
                "model name at {:#010X} has no terminator within its bounded {}-byte scan",
                pointer + base_bytes as usize,
                scan_limit
            ));
        }
        name = Some(if name_bytes.is_ascii() {
            String::from_utf8(name_bytes).expect("ASCII is UTF-8")
        } else {
            name_bytes
                .iter()
                .map(|byte| format!("\\x{byte:02X}"))
                .collect()
        });
    }
    let aligned_bytes = content_bytes
        .checked_add(3)
        .ok_or_else(|| "model aligned length overflow".to_string())?
        & !3;
    if aligned_bytes > MAX_MODEL_ENTRY_BYTES {
        return Err(format!(
            "model entry at {pointer:08X} claims {aligned_bytes:#x} bytes, exceeding the {MAX_MODEL_ENTRY_BYTES:#x} safety bound"
        ));
    }
    let bytes = process.read_bytes(pointer, aligned_bytes as usize)?;
    let descriptor = ModelEntryDescriptor {
        header,
        content_bytes,
        aligned_bytes,
        name,
        fnv1a64: format!("{:016X}", fnv1a64(&bytes)),
    };
    Ok((bytes, descriptor))
}

fn remaining_bounded_entry_bytes(base_bytes: u32, pointer: usize) -> Result<u32, String> {
    MAX_ALIGNED_MODEL_ENTRY_BYTES
        .checked_sub(base_bytes)
        .ok_or_else(|| {
            format!(
                "model entry at {pointer:08X} claims at least {base_bytes:#x} bytes, exceeding the {MAX_ALIGNED_MODEL_ENTRY_BYTES:#x} aligned safety bound"
            )
        })
}

fn validate_model_header(header: &ModelHeader, pointer: usize) -> Result<(), String> {
    if header.shade_slot_count < 2 || header.shade_slot_count % 2 != 0 {
        return Err(format!(
            "invalid shade-slot count {} at {pointer:08X}",
            header.shade_slot_count
        ));
    }
    let [a, b, c, d] = header.vertex_ranges;
    if header.vertex_slot_count % 2 != 0
        || a % 2 != 0
        || b % 2 != 0
        || c % 2 != 0
        || d % 2 != 0
        || !(a <= b && b <= c && c <= d && d <= header.vertex_slot_count)
    {
        return Err(format!(
            "invalid vertex ranges {:?} for {} slots at {pointer:08X}",
            header.vertex_ranges, header.vertex_slot_count
        ));
    }
    let [a, b] = header.shade_ranges;
    if a % 2 != 0 || b % 2 != 0 || !(a <= b && b <= header.shade_slot_count) {
        return Err(format!(
            "invalid shade ranges {:?} for {} slots at {pointer:08X}",
            header.shade_ranges, header.shade_slot_count
        ));
    }
    Ok(())
}

fn decode_header(bytes: &[u8]) -> ModelHeader {
    ModelHeader {
        visibility_extent: u32_at(bytes, 0),
        command_word_count: u16_at(bytes, 4),
        auxiliary_count: bytes[6],
        flags: bytes[7],
        vertex_slot_count: u16_at(bytes, 8),
        raw_vertex_count: u16_at(bytes, 10),
        shade_slot_count: u16_at(bytes, 12),
        raw_0e: u16_at(bytes, 14),
        reserved_10: u16_at(bytes, 16),
        vertex_ranges: [
            u16_at(bytes, 18),
            u16_at(bytes, 20),
            u16_at(bytes, 22),
            u16_at(bytes, 24),
        ],
        reserved_1a: u16_at(bytes, 26),
        shade_ranges: [u16_at(bytes, 28), u16_at(bytes, 30)],
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_slot_is_eighth_pointer_in_global_table() {
        assert_eq!(MODEL_HANDLER_ADDRESS, 0x0053_AA34);
    }

    #[test]
    fn model_header_matches_retail_layout() {
        let mut bytes = [0u8; MODEL_HEADER_BYTES];
        bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        bytes[4..6].copy_from_slice(&7u16.to_le_bytes());
        bytes[6] = 3;
        bytes[7] = 0x40;
        bytes[8..10].copy_from_slice(&9u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&11u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&14u16.to_le_bytes());
        let header = decode_header(&bytes);
        assert_eq!(header.visibility_extent, 0x1122_3344);
        assert_eq!(header.command_word_count, 7);
        assert_eq!(header.auxiliary_count, 3);
        assert_eq!(header.flags, 0x40);
        assert_eq!(header.vertex_slot_count, 9);
        assert_eq!(header.raw_vertex_count, 11);
        assert_eq!(header.shade_slot_count, 14);
    }

    #[test]
    fn pointer_hash_is_stable_and_order_sensitive() {
        let snapshot = ModelTableSnapshot {
            record_kind: "model_table_snapshot",
            stable: true,
            handler_count: 16,
            handler: None,
            active_slots: 2,
            slots: vec![
                ModelSlot {
                    global_id: 0,
                    pointer: 1,
                    state: SlotState::Stable,
                    descriptor: None,
                    error: None,
                },
                ModelSlot {
                    global_id: 1,
                    pointer: 2,
                    state: SlotState::Stable,
                    descriptor: None,
                    error: None,
                },
            ],
            error: None,
        };
        assert_eq!(snapshot.pointer_hash(), "C9C28939C99668C6");
    }

    #[test]
    fn optional_name_scan_never_crosses_the_entry_safety_envelope() {
        assert_eq!(
            remaining_bounded_entry_bytes(MAX_ALIGNED_MODEL_ENTRY_BYTES, 0x1234).unwrap(),
            0
        );
        assert!(remaining_bounded_entry_bytes(MAX_ALIGNED_MODEL_ENTRY_BYTES + 1, 0x1234).is_err());
    }
}
