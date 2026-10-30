use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const SINE: [u16; 4096] = [0; 4096];

pub(super) fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 4096;
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0;
    (arena, actor)
}

fn forward(desired_velocity: i32) -> ActorForwardMotionRequest {
    ActorForwardMotionRequest {
        power: 65535,
        desired_velocity,
        step_units: 1250,
        table: ActorForwardMotionTable {
            maximum: 0x3000,
            minimum: -0x1000,
            acceleration: 0x2000,
            deceleration: 0x2000,
        },
    }
}

fn angular(desired_turn: i32, axis: ActorAngularAxis) -> ActorAngularMotionRequest {
    ActorAngularMotionRequest {
        power: 65535,
        desired_turn,
        step_units: 1250,
        table: ActorAngularMotionTable {
            maximum: 0x10000,
            response: 0x20000,
        },
        axis,
    }
}

#[test]
fn forward_control_accelerates_and_zero_control_brakes_existing_motion() {
    let (mut arena, actor) = fixture();
    arena
        .apply_actor_forward_motion(actor, forward(3000))
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 623]);
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 1000];
    arena.apply_actor_forward_motion(actor, forward(0)).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 376]);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn forward_projection_truncates_and_velocity_addition_wraps_words() {
    let (mut arena, actor) = fixture();
    arena.actor_mut(actor).unwrap().rotating.basis.words[9] = 0.5_f32.to_bits();
    arena.actor_mut(actor).unwrap().words_8e_92[2] = 3;
    let mut request = forward(1);
    request.table.minimum = -100;
    arena.apply_actor_forward_motion(actor, request).unwrap();
    // 3*0.5 truncates to 1, matching requested normalized velocity.
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[2], 3);
    arena.actor_mut(actor).unwrap().rotating.basis.words[9] = 1.0_f32.to_bits();
    arena.actor_mut(actor).unwrap().words_8e_92[2] = 32760;
    let request = ActorForwardMotionRequest {
        desired_velocity: 32767,
        power: 0x20000,
        step_units: 32768,
        table: ActorForwardMotionTable {
            maximum: 32767,
            minimum: -32768,
            acceleration: 100,
            deceleration: 100,
        },
    };
    arena.apply_actor_forward_motion(actor, request).unwrap();
    assert_eq!(
        arena.actor(actor).unwrap().words_8e_92[2],
        (32774_i32 as u16)
    );
}

#[test]
fn forward_division_failure_does_not_publish_a_velocity() {
    let (mut arena, actor) = fixture();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 0;
    arena.actor_mut(actor).unwrap().words_8e_92 = [1, 2, 3];
    assert!(matches!(
        arena.apply_actor_forward_motion(actor, forward(0)),
        Err(ActorMotionError::Division {
            address: 0x0042_6874,
            denominator: 0,
            ..
        })
    ));
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [1, 2, 3]);
}

#[test]
fn lateral_correction_retains_full_small_projection_and_signed_limit_asymmetry() {
    let (mut arena, actor) = fixture();
    let request = ActorLateralMotionRequest {
        power: 65535,
        step_units: 1250,
        table: ActorLateralMotionTable {
            threshold: 10_000,
            rate: 1000,
        },
    };
    arena.actor_mut(actor).unwrap().words_8e_92 = [100, 7, 9];
    assert_eq!(
        arena.apply_actor_lateral_motion(actor, request).unwrap(),
        100
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 7, 9]);
    arena.actor_mut(actor).unwrap().words_8e_92[0] = 1000;
    assert_eq!(
        arena.apply_actor_lateral_motion(actor, request).unwrap(),
        37
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[0], 963);
    arena.actor_mut(actor).unwrap().words_8e_92[0] = (-1000_i16) as u16;
    assert_eq!(
        arena.apply_actor_lateral_motion(actor, request).unwrap(),
        -38
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[0] as i16, -962);
}

#[test]
fn angular_local_axis_rounds_nearest_even_while_world_axis_truncates() {
    let (mut arena, actor) = fixture();
    // Zero turn targets zero. Projecting existing +2 through 0.5 gives +1;
    // the local correction -0.5 rounds to even zero.
    arena.actor_mut(actor).unwrap().rotating.basis.words[5] = 0.5_f32.to_bits();
    arena.actor_mut(actor).unwrap().words_94_9c = [0, 2, 0];
    arena
        .apply_actor_angular_motion(actor, angular(0, ActorAngularAxis::LocalY))
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_94_9c[1] as i32, 2);
    arena
        .apply_actor_angular_motion(
            actor,
            angular(1000, ActorAngularAxis::WorldVector([0.0, 0.5, 0.0])),
        )
        .unwrap();
    // 00426B03 gives normalized turn 15; 00426B21 gives desired 1920.
    // Difference 1919 is below the native limit 4999. The explicit world
    // vector uses __ftol: 1919*0.5 becomes 959, whereas nearest-even is 960.
    assert_eq!(arena.actor(actor).unwrap().words_94_9c[1] as i32, 961);
}

#[test]
fn angular_zero_power_bypasses_bad_tables_and_local_current_rounds_ties_even() {
    let (mut arena, actor) = fixture();
    let mut request = angular(1, ActorAngularAxis::LocalY);
    request.power = 0;
    request.table.maximum = 0;
    request.table.response = 0;
    arena.apply_actor_angular_motion(actor, request).unwrap();
    request.power = 1;
    assert!(matches!(
        arena.apply_actor_angular_motion(actor, request),
        Err(ActorMotionError::Division {
            address: 0x0042_6b03,
            ..
        })
    ));
    arena.actor_mut(actor).unwrap().rotating.basis.words[5] = 0.5_f32.to_bits();
    arena.actor_mut(actor).unwrap().words_94_9c = [0, 1, 0];
    arena
        .apply_actor_angular_motion(
            actor,
            angular(0, ActorAngularAxis::WorldVector([0.0, 1.0, 0.0])),
        )
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_94_9c, [0, 1, 0]); // 0.5 → even 0
}

#[test]
fn motion_table_constants_match_retail_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let image = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    let words = |address: u32, count: u32| {
        (0..count)
            .map(|index| image.word(address + index * 4).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(words(0x0052_fe40, 2), vec![10_000, 1000]);
    assert_eq!(
        words(0x0053_a080, 4),
        vec![0x3000, (-0x1000_i32) as u32, 0x2000, 0x2000]
    );
    assert_eq!(words(0x0053_a090, 2), vec![0x10000, 0x20000]);
}
