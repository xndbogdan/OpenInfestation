use super::*;
use crate::object_runtime::{
    BaseContext, ObjectClassKind, ObjectDestructionRequest, ObjectDrawProcessState, ObjectList,
    SoundTeardownService,
};
use inf_formats::{native_basis::NativeBasisMath, FrontendExecutable};
use serde_json::Value;
mod behavior;
mod native_birth;
mod native_draw;

fn raw() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
#[test]
fn pickup_basis_startup_writer_and_timer_match_original_instructions() {
    let rows: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/pickup_lifecycle_native.json"
    ))
    .unwrap();
    let bytes = raw();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let sine = exe.mapped(0x53df00, 8192).unwrap().bytes;
    let math = NativeBasisMath::new(std::array::from_fn(|i| {
        u16::from_le_bytes([sine[i * 2], sine[i * 2 + 1]])
    }));
    let startup = ObjectDrawProcessState::native_startup(exe).unwrap();
    assert_eq!(
        serde_json::to_value(startup.pickup_basis.words).unwrap(),
        rows["leaves"]["startup"]
    );
    for row in rows["leaves"]["rotations"].as_array().unwrap() {
        let mut process = startup.clone();
        process.advance_pickup_rotation(row["input"].as_u64().unwrap() as i32, &math);
        assert_eq!(
            serde_json::to_value(process.pickup_basis.words).unwrap(),
            row["basis"]
        );
    }
    for row in rows["leaves"]["timers"].as_array().unwrap() {
        let mut arena = ObjectArena::default();
        let object = arena.allocate(ObjectClassKind::TimedPickup);
        arena.timed_pickup_mut(object).unwrap().remaining_58 =
            Some(row["life"].as_u64().unwrap() as u32);
        arena
            .tick_timed_pickup(object, row["delta"].as_u64().unwrap() as u32)
            .unwrap();
        assert_eq!(
            arena.timed_pickup(object).unwrap().remaining_58,
            Some(row["remaining"].as_u64().unwrap() as u32)
        );
        for (name, list) in [
            ("flag0", ObjectList::Flag0Primary),
            ("flag1", ObjectList::Flag1Primary),
            ("parking", ObjectList::Parking),
            ("disposal", ObjectList::Disposal),
        ] {
            assert_eq!(
                arena.list(list).len(),
                row["lists"][name].as_array().unwrap().len(),
                "{row}"
            );
        }
    }
}

#[test]
fn pickup_retirement_releases_generation_and_generic_update_cannot_disable_it() {
    let mut arena = ObjectArena::default();
    let object = arena.allocate(ObjectClassKind::TimedPickup);
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some([1, 2, 3]),
                placement_index: u32::MAX,
            },
        )
        .unwrap();
    arena.timed_pickup_mut(object).unwrap().kind_44 = Some(0x83);
    let before = arena.object(object).unwrap().clone();
    assert!(
        matches!(arena.update(object),Err(ObjectError::TimedPickupUpdateBoundary(id)) if id==object)
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    arena.tick_timed_pickup(object, 10_000_001).unwrap();
    arena
        .destroy(ObjectDestructionRequest {
            object,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.object(object).is_err());
    let next = arena.allocate(ObjectClassKind::TimedPickup);
    assert_ne!(next, object);
    assert_eq!(
        arena.timed_pickup(next).unwrap().remaining_58,
        Some(10_000_000)
    );
    assert_eq!(arena.timed_pickup(next).unwrap().kind_44, None);
}

/// 0046D7D0, the placed (selector 14) +1C: each placement takes world pickup
/// serial 0054ED48 and post-increments it. A serial the mission marks as
/// collected (0048E930 over W+B0) disposes through 00479FA0; an uncollected
/// one leaves the update list (00479F20(0)). Placement -1 assigns nothing.
#[test]
fn placed_pickups_take_world_serials_and_dispose_when_collected() {
    let mut arena = ObjectArena::default();
    let mut words = [0; 200];
    // Current world 2's first W+B0 word marks serial 1 collected.
    words[2 * 4] = 1 << 1;
    arena.set_world_pickup_words(crate::object_runtime::PickupCompletionWords {
        current_world: 2,
        words_b0_3cc: words,
    });
    arena.begin_world_placements();
    let first = arena.allocate(ObjectClassKind::PlacedPickup);
    let second = arena.allocate(ObjectClassKind::PlacedPickup);
    let dynamic = arena.allocate(ObjectClassKind::PlacedPickup);
    for (object, word) in [(first, 0), (second, 1), (dynamic, 0xffff)] {
        arena.owned_mut(object).unwrap().state.base.placement_word = word;
        arena.set_flag1(object, true).unwrap();
        arena.activate_placed_pickup(object, false, false).unwrap();
    }
    assert_eq!(arena.timed_pickup(first).unwrap().save_index_48, 0);
    assert_eq!(arena.timed_pickup(second).unwrap().save_index_48, 1);
    assert_eq!(arena.timed_pickup(dynamic).unwrap().save_index_48, u32::MAX);
    assert_eq!(arena.world_load_state().pickup_serial_54ed48, 2);
    let listed = |arena: &ObjectArena, list| -> Vec<ObjectId> {
        arena.list(list).iter().map(|node| node.object).collect()
    };
    assert_eq!(listed(&arena, ObjectList::Disposal), [second]);
    // Only the dynamic (placement -1) pickup keeps its update membership.
    assert_eq!(listed(&arena, ObjectList::Flag1Primary), [dynamic]);
    let state = arena.timed_pickup(first).unwrap();
    assert_eq!(
        (state.class, state.remaining_58),
        (crate::object_runtime::PickupClass::Placed, None)
    );
    // The timed 0046E0D0 countdown never runs for a placed pickup, and a
    // network session's 00479F20(1) branch is an explicit stop.
    assert_eq!(
        arena.tick_timed_pickup(first, 1),
        Err(ObjectError::PlacedPickupNetworkUpdate(first))
    );
    let networked = arena.allocate(ObjectClassKind::PlacedPickup);
    arena
        .owned_mut(networked)
        .unwrap()
        .state
        .base
        .placement_word = 2;
    // The network host lists it for 0046D950; a guest leaves it off.
    arena.activate_placed_pickup(networked, true, true).unwrap();
    assert!(arena.object(networked).unwrap().base.flags & 2 != 0);
    arena
        .activate_placed_pickup(networked, true, false)
        .unwrap();
    assert!(arena.object(networked).unwrap().base.flags & 2 == 0);
}

/// The shared destructor 0046E0A0 adds one to gameplay G+5B4 for each kind-7
/// pickup it destroys, placed or timed, before the common 00479D00 release.
/// 00487540 zeroes the count at the phase-8 transition.
#[test]
fn kind7_pickup_destruction_counts_into_g5b4() {
    let mut arena = ObjectArena::default();
    for (class, kind) in [
        (ObjectClassKind::PlacedPickup, 7),
        (ObjectClassKind::TimedPickup, 7),
        (ObjectClassKind::PlacedPickup, 3),
    ] {
        let object = arena.allocate(class);
        arena.timed_pickup_mut(object).unwrap().kind_44 = Some(kind);
        arena.dispose(object).unwrap();
        arena
            .destroy(ObjectDestructionRequest {
                object,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert!(arena.object(object).is_err());
    }
    assert_eq!(arena.pickup_kind7_count(), 2);
    arena.reset_pickup_kind7_count();
    assert_eq!(arena.pickup_kind7_count(), 0);
}
