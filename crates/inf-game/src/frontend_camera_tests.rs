use super::*;
use inf_formats::ByteSpan;

fn preset(eye: [i32; 3], target: [i32; 3]) -> FrontendCameraPreset {
    FrontendCameraPreset {
        virtual_address: 0x0053_c148,
        span: ByteSpan::new(0, 24),
        eye,
        target,
    }
}

fn camera() -> FrontendCamera {
    FrontendCamera::new(&preset([0, 0, -7000], [0; 3]), 10, FrontendEntry::First)
}

#[test]
fn first_entry_uses_entrance_pose_and_returning_entry_copies_both_targets() {
    let first = camera();
    assert_eq!(first.eye().position, [-12_000, 0, 70_000]);
    assert_eq!(first.look_at().position, [20_000, 0, 50_000]);
    assert_eq!(first.eye().target, [0, 0, -70_000]);
    assert_eq!(first.look_at().target, [0; 3]);
    assert_eq!(first.eye().velocity, [0; 3]);
    assert_eq!(first.look_at().velocity, [0; 3]);
    assert_eq!(first.selected_preset(), 0);
    assert!(!first.ready());

    let returning = FrontendCamera::new(
        &preset([1, -2, 3], [-4, 5, -6]),
        10,
        FrontendEntry::Returning,
    );
    assert_eq!(returning.eye().position, [10, -20, 30]);
    assert_eq!(returning.look_at().position, [-40, 50, -60]);
    assert_eq!(returning.eye().position, returning.eye().target);
    assert_eq!(returning.look_at().position, returning.look_at().target);
    assert_eq!(returning.eye().velocity, [0; 3]);
    assert!(returning.ready());
}

#[test]
fn position_scale_uses_wrapping_signed_multiplication() {
    let camera = FrontendCamera::new(
        &preset([i32::MAX, i32::MIN, -3], [i32::MIN, i32::MAX, 3]),
        2,
        FrontendEntry::Returning,
    );
    assert_eq!(camera.eye().target, [-2, 0, -6]);
    assert_eq!(camera.look_at().target, [0, -2, 6]);
    // -1200 * 0x08000000 produces MIN, including in a checked debug build.
    let entrance = FrontendCamera::new(&preset([0; 3], [0; 3]), 0x0800_0000, FrontendEntry::First);
    assert_eq!(entrance.eye().position[0], i32::MIN);
}

#[test]
fn preset_selection_preserves_pose_velocity_and_restarts_nine_on_every_call() {
    let mut camera = camera();
    camera.eye.velocity = [5, -6, 7];
    camera.look_at.velocity = [-8, 9, -10];
    let old_eye = camera.eye;
    let old_look = camera.look_at;
    let authored = preset([3, 2, 1], [-1, -2, -3]);

    for _ in 0..2 {
        assert!(camera.select_preset(9, &authored, 10).restart_timer_0120);
        assert_eq!(camera.selected_preset(), 9);
        assert_eq!(camera.eye.target, [30, 20, 10]);
        assert_eq!(camera.look_at.target, [-10, -20, -30]);
        assert_eq!(camera.eye.position, old_eye.position);
        assert_eq!(camera.look_at.position, old_look.position);
        assert_eq!(camera.eye.velocity, old_eye.velocity);
        assert_eq!(camera.look_at.velocity, old_look.velocity);
    }
    assert!(!camera.select_preset(0, &authored, -2).restart_timer_0120);
    assert_eq!(camera.selected_preset(), 0);
    assert_eq!(camera.eye.target, [-6, -4, -2]);
}

#[test]
fn gate_checks_only_eye_signed_wrapping_distance_strictly_below_one_hundred() {
    let mut camera = camera();
    camera.eye.position = [0; 3];
    camera.eye.target = [99, -99, 0];
    camera.eye.velocity = [i32::MAX; 3];
    camera.look_at.position = [i32::MAX; 3];
    camera.look_at.target = [0; 3];
    assert!(camera.ready());
    for axis in 0..3 {
        for outside in [100, -100] {
            camera.eye.target = [0; 3];
            camera.eye.target[axis] = outside;
            assert!(!camera.ready());
        }
    }
    camera.eye.target = [i32::MIN; 3];
    assert!(camera.ready(), "native signed abs(MIN) remains negative");
}

#[test]
fn motion_policy_uses_native_independent_eye_and_look_speeds() {
    assert_eq!(CameraMotion::FirstEntry.speeds(10), [150, 120]);
    assert_eq!(CameraMotion::Returning.speeds(10), [400, 250]);
    assert_eq!(CameraMotion::Accelerated.speeds(10), [2000, 1400]);
    assert_eq!(CameraMotion::FirstEntry.speeds(i32::MIN), [i32::MIN, 0]);
}

#[test]
fn integer_length_preserves_signed_inputs_and_scaling_rounding() {
    for (input, root) in [
        (i32::MIN, 0),
        (-1, 0),
        (0, 0),
        (1, 1),
        (4, 2),
        (15, 3),
        (16, 4),
        (i32::MAX, 46_340),
    ] {
        assert_eq!(integer_sqrt(input), root);
    }
    assert_eq!(vector_length([3, 4, 0]), 5);
    assert_eq!(vector_length([-3, -4, 0]), 5);
    assert_eq!(vector_length([0x6882, 0, 0]), 0x6882);
    assert_eq!(vector_length([0x6883, 0, 0]), 0x6882);
    assert_eq!(vector_length([-0x6883, 0, 0]), 0x6884);
    assert_eq!(vector_length([i32::MIN, 0, 0]), 0);
}

#[test]
fn q31_extracts_signed_product_bits_and_wraps_the_result() {
    assert_eq!(multiply_q31(1000, 0x4000_0000), 500);
    assert_eq!(multiply_q31(-1, 0x4000_0000), -1);
    assert_eq!(multiply_q31(1, 0x4000_0000), 0);
    assert_eq!(multiply_q31(i32::MIN, i32::MIN), i32::MIN);
}

#[test]
fn normalization_preserves_zero_fallback_float_spills_and_truncation() {
    assert_eq!(normalized_scaled([0; 3], 7), [7, 0, 0]);
    assert_eq!(normalized_scaled([3, 4, 0], 10), [6, 8, 0]);
    assert_eq!(normalized_scaled([-3, -4, 0], 10), [-6, -8, 0]);
    assert_eq!(normalized_scaled([3, 4, 0], 9), [5, 7, 0]);

    // X remains exact above the f32 integer range; Y and Z are spilled, and
    // their scale is spilled too. An ordinary symmetric normalize fails this.
    assert_eq!(
        normalized_scaled([16_777_217, 0, 0], 16_777_217),
        [16_777_218, 0, 0]
    );
    assert_eq!(
        normalized_scaled([0, 16_777_217, 0], 16_777_217),
        [0, 16_777_216, 0]
    );
    assert_eq!(
        normalized_scaled([0, 0, 16_777_217], 16_777_217),
        [0, 0, 16_777_216]
    );
}

#[test]
fn spring_keeps_acceleration_clamp_velocity_and_target_snap_order() {
    let mut spring = CameraSpring {
        position: [0; 3],
        target: [10_000, 0, 0],
        velocity: [0; 3],
    };
    // 524288 << 11 gives exactly half a Q31 unit. The four native steps
    // exercise acceleration limiting, deceleration, and a snap with velocity.
    for (position, velocity) in [(2500, 5000), (6800, 8600), (9600, 5600), (10_000, 2000)] {
        spring.advance(100, 524_288);
        assert_eq!(spring.position, [position, 0, 0]);
        assert_eq!(spring.velocity, [velocity, 0, 0]);
    }
    spring.advance(100, 524_288);
    assert_eq!(spring.position, spring.target);
    assert_eq!(spring.velocity, [0; 3]);
}

#[test]
fn spring_uses_negative_q31_rounding_and_capped_frame_delta_without_seconds_conversion() {
    let mut positive = CameraSpring {
        position: [0; 3],
        target: [10_000, 0, 0],
        velocity: [0; 3],
    };
    let mut negative = CameraSpring {
        target: [-10_000, 0, 0],
        ..positive
    };
    positive.advance(100, 125_000);
    negative.advance(100, 125_000);
    assert_eq!(positive.velocity, [1192, 0, 0]);
    assert_eq!(positive.position, [142, 0, 0]);
    assert_eq!(negative.velocity, [-1192, 0, 0]);
    assert_eq!(negative.position, [-143, 0, 0]);
}

#[test]
fn overshoot_comparison_is_strict_and_uses_magnitude_without_direction() {
    let mut equal = CameraSpring {
        position: [0; 3],
        target: [100, 0, 0],
        velocity: [-200, 0, 0],
    };
    let mut larger = CameraSpring {
        velocity: [-202, 0, 0],
        ..equal
    };
    // A zero acceleration limit preserves the existing velocity. Equal step
    // magnitude takes the add branch; larger magnitude snaps even facing away.
    equal.advance(0, 524_288);
    larger.advance(0, 524_288);
    assert_eq!(equal.position, [-100, 0, 0]);
    assert_eq!(larger.position, [100, 0, 0]);
    assert_eq!(larger.velocity, [-202, 0, 0]);
}

#[test]
fn animation_clock_carries_fractional_time_and_treats_frame_argument_as_unsigned() {
    let mut clock = AnimationClock::default();
    clock.advance(19_999);
    assert_eq!(clock.value(), 0);
    clock.advance(1);
    assert_eq!(clock.value(), 1);
    clock.advance(19_999);
    assert_eq!(clock.value(), 1);
    assert_eq!(clock.accumulated_micros(), 39_999);
    clock.reset();
    clock.advance(u32::MAX);
    assert_eq!(clock.accumulated_micros(), 4_294_967_295);
    assert_eq!(clock.value(), 214_748);
    clock.advance(1);
    assert_eq!(clock.accumulated_micros(), 4_294_967_296);
    assert_eq!(clock.value(), 214_748);
}

#[test]
fn animation_clock_uses_signed_division_and_wraps_the_owned_u64_accumulator() {
    let mut clock = AnimationClock {
        accumulated_micros: 1u64 << 63,
        value: 0,
    };
    clock.advance(0);
    assert_eq!(clock.value(), 3_511_565_262);
    clock.accumulated_micros = u64::MAX - 19_999;
    clock.advance(0);
    assert_eq!(clock.value(), u32::MAX, "signed -20000 / 20000 is -1");
    clock.advance(20_000);
    assert_eq!(clock.accumulated_micros(), 0);
    assert_eq!(clock.value(), 0);
}

#[test]
fn animation_callback_accepts_only_zero_selector_and_returns_low_sixteen_bits() {
    let mut clock = AnimationClock::default();
    clock.advance(20_000 * 0x12345);
    assert_eq!(clock.value(), 0x12345);
    assert_eq!(clock.callback_value(0), 0x2345);
    for selector in [1, 0x40, 0x80, 0xc0, 0x1_0000, u32::MAX] {
        assert_eq!(clock.callback_value(selector), 0);
    }
    // Masking belongs to the model operand decoder before its callback.
    assert_eq!(clock.callback_value(0xc0 & 0xff3f), 0x2345);
    clock.reset();
    assert_eq!(clock.accumulated_micros(), 0);
    assert_eq!(clock.value(), 0);
    assert_eq!(clock.callback_value(0), 0);
}
