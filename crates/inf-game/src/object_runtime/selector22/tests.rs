use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList};
use serde_json::Value;

fn u(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn fading(fade: u32, limit: u32) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector22);
    assert_eq!(*arena.selector22(object).unwrap(), Selector22State::new());
    match &mut arena.owned_mut(object).unwrap().state.class {
        ObjectClass::Selector22(state) => {
            state.fade_44 = fade;
            state.limit_48 = limit;
        }
        _ => unreachable!(),
    }
    (arena, object)
}

fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}

/// 0042DC00 over clamp edges, signed thresholds, wrapped products and the
/// constructor's 0xFFFF start stepped at 30 fps until disposal.
#[test]
fn selector22_update_matches_native_rows_and_fade_history() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector22_update_native.json"
    ))
    .unwrap();
    let rows = fixture["rows"].as_array().unwrap();
    for row in rows.iter().chain(fixture["history"].as_array().unwrap()) {
        let (mut arena, object) = fading(u(&row["fade"]), u(&row["limit"]));
        arena.tick_selector22(object, u(&row["delta"])).unwrap();
        assert_eq!(
            disposed(&arena, object),
            row["disposed"].as_bool().unwrap(),
            "{row}"
        );
        let state = arena.selector22(object).unwrap();
        assert_eq!(state.fade_44, u(&row["after_fade"]), "{row}");
        assert_eq!(state.limit_48, u(&row["limit"]), "{row}");
    }
    assert_eq!(rows.len(), 1056);
    assert_eq!(fixture["history"].as_array().unwrap().len(), 30);
}
