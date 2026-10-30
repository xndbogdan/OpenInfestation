use std::collections::BTreeMap;

use inf_formats::{
    decode_instance_command, ByteSpan, InstanceCommandReadScope, InstanceCommandRequest,
    InstanceCommandTarget,
};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::ObjectClassKind;

fn decode(
    code: i32,
    payload: &[u8],
) -> Result<InstanceCommandView, inf_formats::InstanceAssetError> {
    let mut bytes = code.to_le_bytes().to_vec();
    bytes.extend_from_slice(payload);
    decode_instance_command(InstanceCommandRequest {
        source: &bytes,
        main_payload: ByteSpan::new(0, bytes.len()),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(87),
        read_scope: InstanceCommandReadScope::Dispatch,
    })
}

#[test]
fn original_constructor_commands_and_destruction_match_forward_list_receipt() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/map_annotations_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["constructor"], 0x48c710);
    assert_eq!(fixture["command"], 0x48c7a0);
    let mut arena = ObjectArena::new();
    let mut objects = BTreeMap::new();
    let mut live = Vec::new();
    for step in fixture["steps"].as_array().unwrap() {
        let index = step["index"].as_u64().unwrap() as usize;
        match step["op"].as_str().unwrap() {
            "construct" => {
                objects.insert(index, arena.allocate(ObjectClassKind::MapAnnotation));
                live.push(index);
            }
            "command" => {
                let payload: Vec<u8> =
                    serde_json::from_value(step["payload"].clone()).unwrap_or_default();
                let command = decode(step["code"].as_i64().unwrap() as i32, &payload).unwrap();
                assert_eq!(
                    arena
                        .apply_map_annotation_command(objects[&index], &command)
                        .unwrap(),
                    step["accepted"].as_bool().unwrap()
                );
            }
            "destroy" => {
                arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: objects[&index],
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
                live.retain(|id| *id != index);
                assert!(arena.object(objects[&index]).is_err());
            }
            _ => panic!("unknown original fixture operation"),
        }
        let order: Vec<_> = arena
            .map_annotation_list()
            .iter()
            .map(|object| *objects.iter().find(|(_, id)| *id == object).unwrap().0)
            .collect();
        let states: Vec<_> = live
            .iter()
            .map(|index| {
                let object = objects[index];
                let state = arena.map_annotation(object).unwrap();
                let base = &arena.object(object).unwrap().base;
                json!({"index":index,"label":state.label,"radius":state.radius,
                "rgb":state.rgb,"flags":base.flags,"dirty":base.dirty_word})
            })
            .collect();
        assert_eq!(
            json!({"order":order,"states":states}),
            step["state"],
            "{step}"
        );
    }
    assert_eq!(fixture["steps"].as_array().unwrap().len(), 25);
    assert!(arena.is_empty());
}

#[test]
fn missing_operand_does_not_mutate_annotation_or_move_its_node() {
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::MapAnnotation);
    let second = arena.allocate(ObjectClassKind::MapAnnotation);
    for (code, width) in [(2, 2), (3, 1), (4, 1), (5, 1), (6, 2)] {
        for length in 0..width {
            let before = arena.map_annotation(first).unwrap().clone();
            let order = arena.map_annotation_list().to_vec();
            assert!(decode(code, &vec![0xff; length]).is_err());
            assert_eq!(arena.map_annotation(first).unwrap(), &before);
            assert_eq!(arena.map_annotation_list(), order);
        }
    }
    arena.dispose(first).unwrap();
    // Queued disposal retains +40 until actual destruction, just as 48C780.
    assert_eq!(arena.map_annotation_list(), &[second, first]);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.map_annotation_list(), &[second]);
}
