use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const SINE: [u16; 4096] = [0; 4096];

fn fixture(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
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
            allocation_position: position,
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
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0;
    (arena, actor)
}

fn request(microseconds: u32, mask: u8) -> ActorPhysicsRequest<'static> {
    ActorPhysicsRequest {
        microseconds,
        mask,
        sine_quarter: &SINE,
    }
}

#[test]
fn angular_short_vector_drag_has_native_zero_delta_floor_and_no_position_read() {
    let (mut arena, actor) = fixture(None);
    arena.actor_mut(actor).unwrap().words_94_9c = [3, 4, 0];
    arena.tick_actor_physics(actor, request(0, 2)).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_94_9c, [1, 2, 0]);
    arena.actor_mut(actor).unwrap().words_94_9c = [2, 0, 0];
    arena.tick_actor_physics(actor, request(0, 2)).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_94_9c, [0; 3]);
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn angular_overflow_square_uses_signed_product_carry_without_short_vector_floor() {
    let (mut arena, actor) = fixture(None);
    arena.actor_mut(actor).unwrap().words_94_9c = [50_000, (-50_000_i32) as u32, 0];
    arena.tick_actor_physics(actor, request(32_000, 2)).unwrap();
    assert_eq!(
        arena.actor(actor).unwrap().words_94_9c.map(|v| v as i32),
        [49_238, -49_237, 0]
    );
    assert_eq!(multiply_high31(-1, 1), -1);
    assert_eq!(multiply_high31(1, 1), 0);
}

#[test]
fn linear_drag_retains_negative_remaining_and_wrapped_square_input() {
    let (mut arena, actor) = fixture(None);
    arena.actor_mut(actor).unwrap().words_8e_92 = [1000, 0, 0];
    arena
        .tick_actor_physics(actor, request(5_000_000, 4))
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[0] as i16, -164);
    arena.actor_mut(actor).unwrap().words_8e_92 = [(-32768_i16) as u16; 3];
    arena.tick_actor_physics(actor, request(0, 4)).unwrap();
    // Square sum C0000000 reaches the actual signed-input 48EFC0 zero path.
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn gravity_precedes_linear_drag_and_position_integration() {
    let (mut arena, actor) = fixture(Some([65535, 0, 0]));
    arena.actor_mut(actor).unwrap().words_8e_92 = [32767, 0, 0];
    arena
        .tick_actor_physics(actor, request(32768, 1 | 8))
        .unwrap();
    assert_eq!(
        arena.actor(actor).unwrap().words_8e_92.map(|v| v as i16),
        [32767, -32, 0]
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([1022, 65535, 0])
    );
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0xc0);
    let (mut combined, other) = fixture(Some([0; 3]));
    combined.actor_mut(other).unwrap().words_8e_92 = [3, 4, 0];
    combined
        .tick_actor_physics(other, request(1024, 1 | 4 | 8))
        .unwrap();
    // Gravity changes 4→3, then sqrt(18)=4 and remaining3 yields [2,2,0].
    assert_eq!(combined.actor(other).unwrap().words_8e_92, [2, 2, 0]);
}

#[test]
fn missing_position_keeps_reached_gravity_and_drag_writes() {
    let (mut arena, actor) = fixture(None);
    arena.actor_mut(actor).unwrap().words_8e_92 = [3, 4, 0];
    arena.actor_mut(actor).unwrap().words_94_9c = [3, 4, 0];
    assert_eq!(
        arena.tick_actor_physics(actor, request(1024, 0x1f)),
        Err(ActorMotionError::MissingPosition(actor))
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [2, 2, 0]);
    assert_eq!(arena.actor(actor).unwrap().words_94_9c, [1, 2, 0]);
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 0);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn world_rotation_bypasses_local_basis_transform_and_preserves_zero_angle_call() {
    let (mut arena, actor) = fixture(None);
    let mut table = [0; 4096];
    table[4095] = 32767;
    // Rotate the actor's local Y onto world Z. Physics angular +98 still
    // represents world Y, and must not be transformed through this basis.
    let basis = &mut arena.actor_mut(actor).unwrap().rotating.basis;
    basis.words[5] = 0;
    basis.words[6] = 1.0_f32.to_bits();
    basis.words[8] = (-1.0_f32).to_bits();
    basis.words[9] = 0;
    arena.actor_mut(actor).unwrap().words_94_9c = [0, 0x4000, 0];
    arena
        .tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: 1_048_576,
                mask: 0x10,
                sine_quarter: &table,
            },
        )
        .unwrap();
    let result = arena.actor(actor).unwrap().rotating.basis;
    let unit = (f64::from(0x7fff_7fff_i32) / 2_147_483_648.0) as f32;
    assert_eq!(f32::from_bits(result.words[3]), -unit); // X rotates towards -Z
    assert_eq!(result.words[0], 3);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0x10);
    // Nonzero angular DWORD triggers the basis call even when step is zero.
    arena
        .tick_actor_physics(
            actor,
            ActorPhysicsRequest {
                microseconds: 0,
                mask: 0x10,
                sine_quarter: &table,
            },
        )
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 6);
    arena.actor_mut(actor).unwrap().words_94_9c = [0; 3];
    arena
        .tick_actor_physics(actor, request(1000, 0x10))
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 6);
}

/// One second of gravity at whole-millisecond frames (retail's clock), as
/// the gameplay tick hands it to 0047AD10: retail's per-frame `delta >> 10`
/// pulls less the shorter the frame (none at all at 1 ms), while the port's
/// exact world ticks (16384 µs quanta, the rest carried) pull the elapsed
/// time's exact `elapsed / 1024`, less at most the one quantum still carried.
#[test]
fn exact_world_ticks_keep_gravity_at_every_frame_rate() {
    use crate::session_platform::{WorldTickCarry, WorldTickQuantum};
    let fall = |frame_ms: u32, quantum: WorldTickQuantum| {
        let (mut arena, actor) = fixture(None);
        let mut carry = WorldTickCarry::default();
        let mut elapsed = 0;
        while elapsed < 1_000_000 {
            elapsed += frame_ms * 1000;
            let delta = carry.world_delta(quantum, (frame_ms * 1000) as i32);
            if delta > 0 {
                arena
                    .tick_actor_physics(actor, request(delta as u32, 1))
                    .unwrap();
            }
        }
        let drop = 0u16.wrapping_sub(arena.actor(actor).unwrap().words_8e_92[1]);
        (u32::from(drop), elapsed / 1024)
    };
    let rates = [33, 16, 7, 4, 1];
    let retail = rates.map(|ms| fall(ms, WorldTickQuantum::Retail).0);
    assert_eq!(retail, [992, 945, 858, 750, 0]);
    for ms in rates {
        let (drop, exact) = fall(ms, WorldTickQuantum::Exact { micros: 16384 });
        assert!(exact - drop < 16, "{ms} ms frames: {drop} of {exact}");
    }
}
