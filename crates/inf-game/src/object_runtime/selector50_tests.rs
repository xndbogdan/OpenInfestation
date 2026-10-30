use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList};
use inf_formats::{ByteSpan, InstanceField};

fn field<T>(value: T) -> InstanceField<T> {
    InstanceField {
        span: ByteSpan::new(0, 0),
        value,
    }
}

fn trigger() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector50);
    assert_eq!(*arena.selector50(object).unwrap(), Selector50State::new());
    (arena, object)
}

fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}

/// 0043FF40: code 3 squares the WORD radius, code 4 scales WORD seconds by
/// 1,000,000 in 32 bits, code 6 keeps DWORD != 0, other payloads reject.
#[test]
fn commands_store_the_native_fields() {
    let (mut arena, object) = trigger();
    let apply = |arena: &mut ObjectArena, payload| {
        arena.apply_selector50_command(object, &payload).unwrap()
    };
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Radius(field(300))
    ));
    assert_eq!(
        arena.selector50(object).unwrap().radius_squared_44,
        Some(90_000)
    );
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Radius(field(0xffff))
    ));
    assert_eq!(
        arena.selector50(object).unwrap().radius_squared_44,
        Some(0xfffe_0001)
    );
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Seconds(field(2))
    ));
    assert_eq!(arena.selector50(object).unwrap().delay_micros_48, 2_000_000);
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Seconds(field(0xffff))
    ));
    assert_eq!(
        arena.selector50(object).unwrap().delay_micros_48,
        0xffff_u32.wrapping_mul(1_000_000)
    );
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Silent(field(0x100))
    ));
    assert_eq!(arena.selector50(object).unwrap().silent_50, 1);
    assert!(apply(
        &mut arena,
        InstanceCommandPayload::Selector50Silent(field(0))
    ));
    assert_eq!(arena.selector50(object).unwrap().silent_50, 0);
    assert!(!apply(&mut arena, InstanceCommandPayload::NoPayloadRead));
}

/// Code 5 stores the command, then queries the first player (0046C820):
/// an owned command disposes the trigger (00479FA0) and still accepts.
#[test]
fn command_disposes_the_trigger_when_the_first_player_owns_it() {
    let (mut arena, object) = trigger();
    assert!(matches!(
        arena.apply_selector50_command(
            object,
            &InstanceCommandPayload::Selector50Command(field(0x25))
        ),
        Err(Selector50Error::MissingFirstPlayer(missing)) if missing == object
    ));
    let player = arena.add_world_player([0; 6]);
    assert!(arena
        .apply_selector50_command(
            object,
            &InstanceCommandPayload::Selector50Command(field(0x25))
        )
        .unwrap());
    assert_eq!(arena.selector50(object).unwrap().command_4c, Some(0x25));
    assert!(!disposed(&arena, object));
    arena.world_player_mut(player).unwrap().command_mask_130[1] |= 1 << 5;
    assert!(arena
        .apply_selector50_command(
            object,
            &InstanceCommandPayload::Selector50Command(field(0x25))
        )
        .unwrap());
    assert!(disposed(&arena, object));
}

/// 0043FEC8..0043FF24: WORD-wrapped differences, an unsigned 32-bit sum
/// compared unsigned with +44, and an unsigned timer compared with +48.
#[test]
fn step_follows_the_native_range_and_timer_tests() {
    let state = Selector50State {
        radius_squared_44: Some(100 * 100),
        delay_micros_48: 1_000_000,
        command_4c: Some(0x25),
        timer_54: 900_000,
        ..Selector50State::new()
    };
    let at = |x: u16| [x, 0, 0];
    // Exactly on the radius stays in range; one past restarts the timer.
    assert_eq!(
        selector50_step(&state, at(100), at(0), 100_000),
        Some(Selector50Step {
            timer: 1_000_000,
            fire: false
        })
    );
    assert_eq!(
        selector50_step(&state, at(101), at(0), 100_000),
        Some(Selector50Step {
            timer: 0,
            fire: false
        })
    );
    assert_eq!(
        selector50_step(&state, at(0), at(0), 100_001),
        Some(Selector50Step {
            timer: 1_000_001,
            fire: true
        })
    );
    // 0x0005 - 0xFFFE wraps to 7 in a WORD.
    assert_eq!(
        selector50_step(&state, at(5), at(0xfffe), 0).map(|step| step.timer),
        Some(900_000)
    );
    // Three 0x8000 differences sum past i32 but stay below 0xFFFF squared.
    let far = Selector50State {
        radius_squared_44: Some(0xfffe_0001),
        ..state
    };
    assert_eq!(
        selector50_step(&far, [0x8000; 3], [0; 3], 0).map(|step| step.timer),
        Some(900_000)
    );
    // The timer wraps in 32 bits.
    let wrapped = Selector50State {
        timer_54: u32::MAX,
        ..state
    };
    assert_eq!(
        selector50_step(&wrapped, at(0), at(0), 2),
        Some(Selector50Step {
            timer: 1,
            fire: false
        })
    );
    let unwritten = Selector50State {
        radius_squared_44: None,
        ..state
    };
    assert_eq!(selector50_step(&unwritten, at(0), at(0), 0), None);
}

/// Infest01's Level 1 trigger updates (every 0043FEB0 call from AF0B4B):
/// each trigger's first update and every update that is in range, resets
/// or sets the command. The step matches the timer stored at return and
/// whether 0046C760 ran.
#[test]
fn step_matches_infest01_level1_updates() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/selector50_triggers_native.json"
    ))
    .unwrap();
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1172);
    let mut fired = 0;
    for row in rows {
        let n = |index: usize| row[index].as_u64().unwrap() as u32;
        let state = Selector50State {
            radius_squared_44: Some(n(4)),
            delay_micros_48: n(5),
            command_4c: Some(n(6)),
            silent_50: n(7) as u8,
            timer_54: n(8),
            ..Selector50State::new()
        };
        let here = [n(1) as u16, n(2) as u16, n(3) as u16];
        let there = [n(9) as u16, n(10) as u16, n(11) as u16];
        let step = selector50_step(&state, here, there, n(12)).unwrap();
        let set = row[14].as_bool().unwrap();
        assert_eq!(
            (step.timer, step.fire),
            (n(13), set),
            "{}",
            row[0].as_str().unwrap()
        );
        fired += usize::from(set);
    }
    assert_eq!(fired, 831);
}
