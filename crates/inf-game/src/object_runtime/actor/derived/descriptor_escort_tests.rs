//! The Flying Escort over Helniazine's small alien ships (selector 35,
//! resource 81), which choose it when they wake: the port left them
//! idle, stopped at the change's 00435220.

use super::*;
use crate::object_runtime::NativeTarget;
use crate::terrain_runtime::TerrainSpacing;

#[test]
fn helniazines_ships_enter_and_run_the_flying_escort() {
    let (_runtime, _assets, mut world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let ships: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            arena.selector35_actor(actor).is_ok()
                && arena.object(actor).unwrap().base.resource_word == 81
        })
        .collect();
    assert!(!ships.is_empty(), "Helniazine places the small ships");
    let ship = ships[0];
    // The ship escorts another actor: here the first other ship.
    let escorted = ships.iter().copied().find(|&other| other != ship).unwrap();
    let handle = arena.actor(escorted).unwrap().handle;
    arena.family428650_actor_mut(ship).unwrap().word_20c = handle.raw();
    arena.change_flying_escort(ship, false).unwrap();
    assert_ne!(
        arena.family428650_actor(ship).unwrap().word_228,
        FLYING_ESCORT_STATUS
    );
    arena.change_flying_escort(ship, true).unwrap();
    assert_eq!(
        arena.family428650_actor(ship).unwrap().word_228,
        FLYING_ESCORT_STATUS
    );
    let target = arena.derived_actor(ship).unwrap().target_148.unwrap();
    match arena.native_target(target).unwrap() {
        NativeTarget::ActorFollow(follow) => assert_eq!(follow.handle_04, handle),
        other => panic!("{other:?}"),
    }
    let reach = i32::from(arena.object(escorted).unwrap().base.word_28).max(0xa4) as u32;
    let own = u32::from(arena.object(ship).unwrap().base.word_28);
    let state = arena.selector35_actor(ship).unwrap();
    assert_eq!(state.word_234, reach);
    assert_eq!(state.word_230, (reach + own * 2).max(0xfa));

    // The update asks for a new choice past 0.5 s within 500 of its target.
    let mut asks = |elapsed: u32, distance: u32| {
        arena.family428650_actor_mut(ship).unwrap().word_22e &= !1;
        arena.family428650_actor_mut(ship).unwrap().words_188_19c[0] = Some(elapsed);
        arena.derived_actor_mut(ship).unwrap().word_14c = Some(distance);
        arena.update_flying_escort(ship).unwrap();
        arena.family428650_actor(ship).unwrap().word_22e & 1 != 0
    };
    assert!(!asks(500_000, 100));
    assert!(asks(500_001, 499));
    assert!(!asks(500_001, 500));
    assert!(
        !asks(u32::MAX, 100),
        "a negative clock is below half a second"
    );
    assert!(asks(600_000, u32::MAX), "a negative distance is within 500");
}
