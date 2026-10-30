//! Selector 21's swings (00405615..004056F2) against an independent model of
//! the disassembly: no recording reaches its worlds (1, 4, 15 and 21).

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const SINE: [u16; 4096] = [0; 4096];

fn construction() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([100, 200, 300]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn nemian() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_selector21_actor(construction(), &mut 0)
        .unwrap();
    (arena, actor)
}

fn swing(arena: &ObjectArena, actor: ObjectId) -> Selector21Swing {
    arena.selector10_actor(actor).unwrap().swing.unwrap()
}

#[test]
fn case_15_builds_selector_10_with_two_swings_at_0x2001() {
    let (arena, actor) = nemian();
    let state = arena.selector10_actor(actor).unwrap();
    assert_eq!((state.word_268, state.helper_26c.is_none()), (None, true));
    assert_eq!(state.swing, Some(Selector21Swing::INITIAL));
    assert_eq!(
        (
            Selector21Swing::INITIAL.swing_270,
            Selector21Swing::INITIAL.swing_274
        ),
        (0x2001, 0x2001)
    );
}

/// Four hundred updates of 125,000 us from the process RNG 0x12345678: past
/// 0x1000 a swing draws a speed toward the other side, and every update adds
/// the high DWORD of `delta * speed` shifted left once.
#[test]
fn the_swings_turn_back_at_0x1000_with_drawn_speeds() {
    let (mut arena, actor) = nemian();
    arena.actor_mut(actor).unwrap().word_c4 = 3;
    let mut rng = 0x1234_5678_u32;
    // (update, +270, +274, +278, +27C, RNG after)
    let checkpoints = [
        (1, 4842, 6492, -29_220_864, -57_567_232, 0xbde2_5da2_u32),
        (2, 2611, 4947, -26_533_888, -38_322_176, 0x94e2_eddc),
        (10, 6247, 2272, 36_679_680, 53_946_368, 0x1fb4_4b21),
        (50, -6303, -5205, -33_304_576, -42_743_808, 0xfc36_957a),
        (400, -2322, 2539, -30_582_784, -59_084_800, 0xd2af_abd5),
    ];
    let mut next = 0;
    for update in 1..=400 {
        arena
            .tick_selector21_swing(actor, 125_000, &mut rng)
            .unwrap();
        if checkpoints[next].0 == update {
            let (_, a, b, speed_b, speed_a, after) = checkpoints[next];
            let swing = swing(&arena, actor);
            assert_eq!(
                (
                    swing.swing_270,
                    swing.swing_274,
                    swing.speed_278,
                    swing.speed_27c,
                    rng
                ),
                (a, b, speed_b, speed_a, after),
                "update {update}"
            );
            next += 1;
        }
    }
    assert_eq!(next, checkpoints.len());
}

/// 00405615: only state 3 swings; other states draw nothing.
#[test]
fn only_a_live_nemian_swings() {
    let (mut arena, actor) = nemian();
    for state in [0, 1, 2, 4] {
        arena.actor_mut(actor).unwrap().word_c4 = state;
        let mut rng = 7;
        arena
            .tick_selector21_swing(actor, 125_000, &mut rng)
            .unwrap();
        assert_eq!((swing(&arena, actor), rng), (Selector21Swing::INITIAL, 7));
    }
}

/// 00405700: selectors 1 and 2 are the swings' low WORDs; the others reach
/// the derived 004260A0.
#[test]
fn model_values_one_and_two_are_the_swings() {
    let (mut arena, actor) = nemian();
    if let Some(swing) = &mut arena.selector10_actor_mut(actor).unwrap().swing {
        swing.swing_270 = -2;
        swing.swing_274 = 0x0001_2345;
    }
    let state = arena.selector10_actor(actor).unwrap();
    let values: Vec<_> = (0..4)
        .map(|selector| selector21_value(state, selector))
        .collect();
    assert_eq!(values, [None, Some(0xfffe), Some(0x2345), None]);
    let (arena, plain) = {
        let (mut arena, _) = nemian();
        let plain = arena
            .allocate_selector10_actor(construction(), &mut 0)
            .unwrap();
        (arena, plain)
    };
    assert_eq!(
        selector21_value(arena.selector10_actor(plain).unwrap(), 1),
        None
    );
}
