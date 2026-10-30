use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, ObjectClass,
    ObjectClassKind,
};

const SINE: [u16; 4096] = [0; 4096];

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena.allocate_selector93(allocation()).unwrap();
    (arena, actor)
}

fn request(gain: i32) -> ActorAngularResponseRequest {
    ActorAngularResponseRequest {
        current_axis: [1.0, 1.0, 1.0],
        desired_axis: [1.0, -1.0, 0.0],
        gain,
    }
}

fn recovery(gain: i32) -> ActorAngularResponseRequest {
    ActorAngularResponseRequest {
        current_axis: [0.0, 1.0, 0.0],
        desired_axis: [0.0, -1.0, 0.0],
        gain,
    }
}

fn angular(arena: &ObjectArena, actor: ObjectId) -> [i32; 3] {
    arena
        .actor(actor)
        .unwrap()
        .words_94_9c
        .map(|word| word as i32)
}

#[test]
fn exact_dot_thresholds_select_noop_cross_and_recovery() {
    let (mut arena, actor) = fixture();
    for (dot_word, expected_z) in [
        (0x3f7f_f000, 0), // Exact upper is the no-actor-read arm.
        (0x3f7f_efff, -2),
        (0xbefa_0000, -2), // Exact lower remains the cross response.
        (0xbefa_0001, -0x2000),
    ] {
        arena.actor_mut(actor).unwrap().words_94_9c = [0; 3];
        arena
            .apply_actor_angular_response(
                actor,
                ActorAngularResponseRequest {
                    current_axis: [0.0, 1.0, 0.0],
                    desired_axis: [1.0, f32::from_bits(dot_word), 0.0],
                    gain: 6,
                },
            )
            .unwrap();
        assert_eq!(angular(&arena, actor), [0, 0, expected_z]);
    }
}

#[test]
fn each_cross_axis_selects_its_own_full_or_signed_divided_gain() {
    let (mut arena, actor) = fixture();
    // Dot0/current x desired=[1,1,-2]. Later axes must reuse gain7,
    // even though an earlier zero/same-sign angular axis reduces it to2.
    for (initial, gain, expected) in [
        ([-4, 0, 3], 7, [3, 2, -11]),
        ([4, 3, -3], 7, [6, 5, -7]),
        ([-4, 0, 3], -7, [-11, -2, 17]),
    ] {
        arena.actor_mut(actor).unwrap().words_94_9c = initial.map(|word| word as u32);
        arena
            .apply_actor_angular_response(actor, request(gain))
            .unwrap();
        assert_eq!(angular(&arena, actor), expected);
    }
}

#[test]
fn cross_xz_retain_precision_while_y_spills_before_crt_truncation() {
    let (mut arena, actor) = fixture();
    let a = f32::from_bits(0x3f80_0003);
    arena.actor_mut(actor).unwrap().words_94_9c = [u32::MAX, u32::MAX, 1];
    arena
        .apply_actor_angular_response(
            actor,
            ActorAngularResponseRequest {
                current_axis: [a, a, a],
                desired_axis: [a, -a, 0.0],
                gain: i32::MAX,
            },
        )
        .unwrap();
    // X/Z retain a*a; Y reloads f32(a*a). CRT results are2147485183,
    //2147485182,-4294970366 before lowDWORD/addition. These are supplied
    // ordinary source discriminators, not an original CPU receipt claim.
    assert_eq!(
        arena.actor(actor).unwrap().words_94_9c,
        [0x8000_05fe, 0x8000_05fd, 0xffff_f403]
    );
}

#[test]
fn recovery_is_strict_at_both_local_z_bounds_and_ignores_zero_gain() {
    let (mut arena, actor) = fixture();
    for (initial_z, expected_z) in [
        (-0x4800, -0x4800),
        (-0x47ff, -0x67ff),
        (-1, -0x2001),
        (0, -0x2000),
        (1, 0x2001),
        (0x47ff, 0x67ff),
        (0x4800, 0x4800),
    ] {
        arena.actor_mut(actor).unwrap().words_94_9c = [0, 0, initial_z as u32];
        arena
            .apply_actor_angular_response(actor, recovery(0))
            .unwrap();
        assert_eq!(angular(&arena, actor), [0, 0, expected_z]);
    }
}

#[test]
fn recovery_uses_full_angular_dwords_and_actual_basis_nearest_even_transform() {
    let (mut arena, actor) = fixture();
    let state = arena.actor_mut(actor).unwrap();
    state.rotating.basis.words[9] = 0.25_f32.to_bits();
    state.words_94_9c = [0, 0, 0x0001_0000];
    arena
        .apply_actor_angular_response(actor, recovery(i32::MIN))
        .unwrap();
    // Full DWORD65536 projects to localZ16384; LOWWORD narrowing would
    // incorrectly choose the zero/negative arm. World response is2048.
    assert_eq!(angular(&arena, actor), [0, 0, 67584]);

    let state = arena.actor_mut(actor).unwrap();
    state.rotating.basis.words[7] = (1.0_f32 / 16384.0).to_bits();
    state.rotating.basis.words[8] = (3.0_f32 / 16384.0).to_bits();
    state.rotating.basis.words[9] = 0.5_f32.to_bits();
    state.words_94_9c = [0, 0, 2];
    arena
        .apply_actor_angular_response(actor, recovery(0))
        .unwrap();
    // LocalZ1 selects8192; nearest-even transform rounds0.5→0,1.5→2.
    assert_eq!(angular(&arena, actor), [0, 2, 4098]);
}

#[test]
fn unordered_dot_reaches_recovery_and_invalid_cross_conversion_stays_masked() {
    let (mut arena, actor) = fixture();
    let mut input = recovery(0);
    input.current_axis[0] = f32::NAN;
    arena.apply_actor_angular_response(actor, input).unwrap();
    assert_eq!(angular(&arena, actor), [0, 0, -0x2000]);

    arena.actor_mut(actor).unwrap().words_94_9c = [0; 3];
    // Finite operands with a product beyond QWORD range use the actual
    // CRT lowDWORD policy, not saturating Rust float-to-i32 conversion.
    let large = f32::MAX;
    arena
        .apply_actor_angular_response(
            actor,
            ActorAngularResponseRequest {
                current_axis: [large, large, 0.0],
                desired_axis: [large, -large, 0.0],
                gain: 3,
            },
        )
        .unwrap();
    assert_eq!(angular(&arena, actor), [0; 3]);
}

#[test]
fn only_world_angular_words_change_with_wrapping_and_whole_arena_custody() {
    let (mut arena, actor) = fixture();
    let other = arena.allocate_actor(allocation()).unwrap();
    arena
        .copy_actor_position(other, [0x8000, 0xffff, 9])
        .unwrap();
    let state = arena.actor_mut(actor).unwrap();
    state.words_94_9c = [0x7fff_fffe, 0xffff_fffe, 1];
    state.words_8e_92 = [0x8000, 0xffff, 0x7fff];
    state.rotating.basis.words[0] = u32::MAX;
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0x1234;
    let before = format!("{arena:?}");
    let mut expected = arena.object(actor).unwrap().clone();
    let initial = arena.actor(actor).unwrap().words_94_9c;
    let input = request(9);
    let input_before = input;
    arena.apply_actor_angular_response(actor, input).unwrap();
    let changed = arena.actor(actor).unwrap().words_94_9c;
    assert_eq!(changed, [0x8000_0001, 7, (-17_i32) as u32]);
    let ObjectClass::Actor(expected_actor) = &mut expected.class else {
        panic!("genuine actor constructor");
    };
    expected_actor.words_94_9c = changed;
    assert_eq!(arena.object(actor).unwrap(), &expected);
    assert_eq!(input, input_before);
    // Normalize only the documented angular outputs for whole-arena custody.
    arena.actor_mut(actor).unwrap().words_94_9c = initial;
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn upper_dot_bypasses_identity_storage_but_reached_errors_preserve_custody() {
    let (mut arena, actor) = fixture();
    let nonactor = arena.allocate(ObjectClassKind::Base);
    let (other_arena, foreign) = fixture();
    assert!(other_arena.actor(foreign).is_ok());
    let high = ActorAngularResponseRequest {
        current_axis: [0.0, 1.0, 0.0],
        desired_axis: [0.0, 1.0, 0.0],
        gain: i32::MAX,
    };
    let before = format!("{arena:?}");
    arena.apply_actor_angular_response(nonactor, high).unwrap();
    arena.apply_actor_angular_response(foreign, high).unwrap();
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(
        arena.apply_actor_angular_response(nonactor, request(7)),
        Err(ObjectError::NotActor(nonactor))
    );
    assert_eq!(
        arena.apply_actor_angular_response(foreign, recovery(0)),
        Err(ObjectError::UnknownObject(foreign))
    );
    assert_eq!(format!("{arena:?}"), before);

    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let retired = format!("{arena:?}");
    arena.apply_actor_angular_response(actor, high).unwrap();
    assert_eq!(
        arena.apply_actor_angular_response(actor, recovery(0)),
        Err(ObjectError::UnknownObject(actor))
    );
    assert_eq!(format!("{arena:?}"), retired);
}
