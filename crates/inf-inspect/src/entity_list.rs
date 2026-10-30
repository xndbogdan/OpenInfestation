use std::collections::HashSet;

use serde::Serialize;

use crate::process::{plausible_heap_pointer, u16_at, u32_at, Process};

const WORLD_POINTER_ADDRESS: usize = 0x0055_F160;
const HANDLE_SENTINEL_ADDRESS: usize = 0x0054_E894;
const WORLD_HANDLE_LIST_OFFSET: usize = 0x60;
const LIST_NODE_BYTES: usize = 12;
const ENTITY_CAPTURE_BYTES: usize = 0xDC;
const MAX_ENTITY_COUNT: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityRecord {
    pub node_pointer: u32,
    pub next_node: u32,
    pub backlink: u32,
    pub entity_pointer: u32,
    pub vtable: u32,
    pub flags_raw: u32,
    pub position_raw: [i16; 3],
    pub status_raw: u8,
    pub model_global_id: i16,
    pub transform_raw: [u32; 10],
    pub handle_d4: u32,
    pub handle_d8: u32,
    pub effective_handle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityListSnapshot {
    pub record_kind: &'static str,
    pub stable: bool,
    pub world_pointer: u32,
    pub list_head: u32,
    pub handle_sentinel: u32,
    pub entities: Vec<EntityRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl EntityListSnapshot {
    fn error(
        world_pointer: u32,
        list_head: u32,
        handle_sentinel: u32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            record_kind: "entity_list_snapshot",
            stable: false,
            world_pointer,
            list_head,
            handle_sentinel,
            entities: Vec::new(),
            error: Some(message.into()),
        }
    }
}

pub fn capture(process: &Process) -> EntityListSnapshot {
    let handle_sentinel = match process.read_u32(HANDLE_SENTINEL_ADDRESS) {
        Ok(value) => value,
        Err(error) => return EntityListSnapshot::error(0, 0, 0, error),
    };
    let world_a = match process.read_u32(WORLD_POINTER_ADDRESS) {
        Ok(value) => value,
        Err(error) => return EntityListSnapshot::error(0, 0, handle_sentinel, error),
    };
    if !plausible_heap_pointer(world_a as usize) {
        return EntityListSnapshot::error(
            world_a,
            0,
            handle_sentinel,
            format!("world pointer is unavailable or implausible: {world_a:#010X}"),
        );
    }
    let head_a = match process.read_u32(world_a as usize + WORLD_HANDLE_LIST_OFFSET) {
        Ok(value) => value,
        Err(error) => return EntityListSnapshot::error(world_a, 0, handle_sentinel, error),
    };
    if !plausible_heap_pointer(head_a as usize) {
        return EntityListSnapshot::error(
            world_a,
            head_a,
            handle_sentinel,
            format!("handle-list head is implausible: {head_a:#010X}"),
        );
    }

    let first = match walk(process, head_a, handle_sentinel) {
        Ok(records) => records,
        Err(error) => {
            return EntityListSnapshot::error(world_a, head_a, handle_sentinel, error);
        }
    };
    let second = match walk(process, head_a, handle_sentinel) {
        Ok(records) => records,
        Err(error) => {
            return EntityListSnapshot::error(world_a, head_a, handle_sentinel, error);
        }
    };
    let world_b = match process.read_u32(WORLD_POINTER_ADDRESS) {
        Ok(value) => value,
        Err(error) => return EntityListSnapshot::error(world_a, head_a, handle_sentinel, error),
    };
    let head_b = if world_b == world_a {
        process
            .read_u32(world_b as usize + WORLD_HANDLE_LIST_OFFSET)
            .unwrap_or(0)
    } else {
        0
    };
    let stable = world_a == world_b && head_a == head_b && first == second;
    EntityListSnapshot {
        record_kind: "entity_list_snapshot",
        stable,
        world_pointer: world_b,
        list_head: head_b,
        handle_sentinel,
        entities: second,
        error: (!stable)
            .then(|| "duplicate entity-list reads did not match; sample retained".into()),
    }
}

fn walk(
    process: &Process,
    list_head: u32,
    handle_sentinel: u32,
) -> Result<Vec<EntityRecord>, String> {
    let mut records = Vec::new();
    let mut visited = HashSet::new();
    let mut node_pointer = list_head;
    loop {
        if records.len() >= MAX_ENTITY_COUNT {
            return Err(format!(
                "entity-list traversal exceeded {MAX_ENTITY_COUNT} active nodes"
            ));
        }
        if !plausible_heap_pointer(node_pointer as usize) {
            return Err(format!(
                "entity-list node is implausible: {node_pointer:#010X}"
            ));
        }
        if !visited.insert(node_pointer) {
            return Err(format!(
                "entity-list cycle reached node {node_pointer:#010X} before its terminator"
            ));
        }
        let node = process.read_bytes(node_pointer as usize, LIST_NODE_BYTES)?;
        let next_node = u32_at(&node, 0);
        if next_node == 0 {
            break;
        }
        let backlink = u32_at(&node, 4);
        let entity_pointer = u32_at(&node, 8);
        if !plausible_heap_pointer(backlink as usize) {
            return Err(format!(
                "entity-list backlink at {node_pointer:#010X} is implausible: {backlink:#010X}"
            ));
        }
        if !plausible_heap_pointer(entity_pointer as usize) {
            return Err(format!(
                "entity pointer at node {node_pointer:#010X} is implausible: {entity_pointer:#010X}"
            ));
        }
        let entity = process.read_bytes(entity_pointer as usize, ENTITY_CAPTURE_BYTES)?;
        let handle_d4 = u32_at(&entity, 0xD4);
        let handle_d8 = u32_at(&entity, 0xD8);
        records.push(EntityRecord {
            node_pointer,
            next_node,
            backlink,
            entity_pointer,
            vtable: u32_at(&entity, 0),
            flags_raw: u32_at(&entity, 0x2C),
            position_raw: [
                u16_at(&entity, 0x32) as i16,
                u16_at(&entity, 0x34) as i16,
                u16_at(&entity, 0x36) as i16,
            ],
            status_raw: entity[0x38],
            model_global_id: u16_at(&entity, 0x42) as i16,
            transform_raw: std::array::from_fn(|index| u32_at(&entity, 0x44 + index * 4)),
            handle_d4,
            handle_d8,
            effective_handle: if handle_d8 == handle_sentinel {
                handle_d4
            } else {
                handle_d8
            },
        });
        node_pointer = next_node;
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_entity_prefix_covers_both_handle_fields() {
        assert_eq!(ENTITY_CAPTURE_BYTES, 0xD8 + 4);
    }

    #[test]
    fn transform_capture_is_exactly_ten_dwords() {
        let offsets = std::array::from_fn::<_, 10, _>(|index| 0x44 + index * 4);
        assert_eq!(
            offsets,
            [0x44, 0x48, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60, 0x64, 0x68]
        );
    }
}
