use super::*;

fn search(elevation: f32, clear_above: Option<f32>) -> (f32, usize) {
    let mut tests = 0;
    let raise = clear_raise::<()>(elevation, &mut |raise| {
        tests += 1;
        Ok(clear_above.is_none_or(|limit| raise < limit))
    })
    .unwrap();
    (raise, tests)
}

#[test]
fn a_clear_view_is_not_raised() {
    assert_eq!(search(0.2, Some(0.0)), (0.0, 1));
}

#[test]
fn a_blocked_view_rises_just_past_what_blocks_it() {
    // Clear from 0.3 radians of raise up.
    let (raise, tests) = search(0.2, Some(0.3));
    assert!(raise >= 0.3, "{raise}");
    assert!(
        raise <= 0.3 + STEP / (1 << REFINE) as f32 + 1.0e-6,
        "{raise}"
    );
    // The game's view, the steps up to the first clear one, the halvings.
    assert_eq!(tests, 1 + 3 + REFINE);
}

#[test]
fn a_view_nothing_clears_rises_to_the_highest() {
    let (raise, _) = search(0.2, None);
    assert!((raise - (HIGHEST - 0.2)).abs() < 1.0e-6, "{raise}");
    // Already at the highest: nowhere to rise.
    assert_eq!(search(HIGHEST + 0.1, None).0, 0.0);
}

#[test]
fn a_raised_point_keeps_its_distance_and_bearing() {
    let anchor = [65_500, 100, 10];
    let bearing = [0.6, -0.8];
    let raised = point(anchor, bearing, 0.5, 400.0);
    let offset: [f32; 3] =
        std::array::from_fn(|axis| f32::from(raised[axis].wrapping_sub(anchor[axis]) as i16));
    assert!((length(offset) - 400.0).abs() < 1.5, "{offset:?}");
    assert!((offset[1] - 400.0 * 0.5f32.sin()).abs() < 1.0, "{offset:?}");
    let flat = offset[0].hypot(offset[2]);
    assert!((offset[0] / flat - 0.6).abs() < 0.01 && (offset[2] / flat + 0.8).abs() < 0.01);
}

#[test]
fn the_remastered_view_sees_ten_degrees_wider() {
    let retail = [512, 512];
    assert_eq!(focal(CameraStyle::Original, false, retail), retail);
    // The scope's zoom keeps retail's.
    assert_eq!(focal(CameraStyle::Remastered, true, retail), retail);
    let wide = focal(CameraStyle::Remastered, false, retail);
    // Across 4:3 (1024 wide): 90 degrees for retail's, 100 for the remaster's.
    let across = |focal: i32| 2.0 * (512.0 / focal as f32).atan().to_degrees();
    assert!((across(retail[0]) - 90.0).abs() < 0.01);
    assert!((across(wide[0]) - 100.0).abs() < 0.1, "{wide:?}");
}

/// Degrees across a 4:3 view (1024 wide) for `focal`.
fn across(focal: i32) -> f32 {
    2.0 * (512.0 / focal as f32).atan().to_degrees()
}

#[test]
fn speed_and_a_boost_kick_the_view_wider() {
    let drive = |forward: f32, boost: bool| Drive {
        forward,
        boost,
        ..Drive::default()
    };
    assert_eq!(kick_target(drive(200.0, false)), 0.0);
    assert!((kick_target(drive(650.0, false)) - 3.0).abs() < 1.0e-4);
    assert_eq!(kick_target(drive(1500.0, false)), KICK_SPEED_DEGREES);
    assert_eq!(kick_target(drive(-900.0, true)), KICK_BOOST_DEGREES);
    let full = kick_target(drive(1000.0, true));
    assert!((across(widened([512, 512], REMASTERED_DEGREES + full)[0]) - 110.0).abs() < 0.1);
}

/// The kick through the game's own slew, as the world draw takes it: a
/// tick's drawn focal from the retail template `template`, the kick eased
/// toward `kick`.
fn drawn_focal(
    state: &mut CameraClearance,
    slewed: &mut [i32; 2],
    template: [i32; 2],
    kick: f32,
) -> i32 {
    let source = focal(CameraStyle::Remastered, false, template);
    *slewed = crate::world_scene::view_focals(*slewed, source, true, 0);
    let scale = kick_scale(state, kick, 0.016384);
    (slewed[0] as f32 * scale).round() as i32
}

#[test]
fn the_kick_never_flickers_through_the_games_slew() {
    for template in [[512, 512], [1611, 1611]] {
        let mut state = CameraClearance::default();
        let mut slewed = [0, 0];
        let mut last = drawn_focal(&mut state, &mut slewed, template, 0.0);
        // Two seconds to full speed with a boost in the second, two at full
        // speed, then two slowing to a stop.
        let mut widest: f32 = 0.0;
        let mut turns = 0;
        let mut heading = 0;
        for tick in 0..480 {
            let seconds = tick as f32 * 0.016384;
            let forward =
                1000.0 * (seconds / 2.0).min(1.0) * (1.0 - (seconds - 4.0).max(0.0) / 2.0);
            let drive = Drive {
                forward: forward.max(0.0),
                boost: (1.0..2.0).contains(&seconds),
                ..Drive::default()
            };
            let next = drawn_focal(&mut state, &mut slewed, template, kick_target(drive));
            // A tick moves the drawn focal a little (half a percent, and the
            // rounding), one way: the first kick fed the slew, whose 0x50
            // step flashed the view wider and back.
            assert!(
                ((next - last).abs() - 1) * 200 <= last,
                "{template:?} tick {tick}: {last} -> {next}"
            );
            let way = (next - last).signum();
            if way != 0 && way != heading {
                turns += 1;
                heading = way;
            }
            last = next;
            widest = widest.max(state.kick);
        }
        // Wider up to full speed and the boost, back as the boost ends,
        // back again to a stop.
        assert!(turns <= 3, "{template:?}: {turns}");
        assert!(widest > 8.0, "{widest}");
        assert!(state.kick < 0.5, "{}", state.kick);
    }
    // The slew answers a source one unit lower with its whole step.
    assert_eq!(
        crate::world_scene::view_focals([430, 430], [429, 429], true, 0),
        [350, 350]
    );
}

/// A basis looking along `heading` (radians about the vertical), pitched and
/// rolled.
fn posed(heading: f32, pitch: f32, roll: f32) -> NativeBasis {
    let forward = [
        pitch.cos() * heading.sin(),
        pitch.sin(),
        pitch.cos() * heading.cos(),
    ];
    let (right, up) = level_frame(forward).unwrap();
    let up: [f32; 3] = std::array::from_fn(|axis| up[axis] * roll.cos() + right[axis] * roll.sin());
    let mut basis = NativeBasis::INITIAL;
    orient_from_forward(&mut basis, forward, up, false);
    basis
}

fn first_person(basis: NativeBasis, trembling: [f32; 3]) -> NativeBasis {
    let view = CameraDrawRequest {
        eye: [0, 0, 0],
        basis,
        coordinate_shift: 16,
    };
    first_person_view(view, trembling).basis
}

/// Pitch and roll of `basis`, radians.
fn attitude(basis: NativeBasis) -> (f32, f32) {
    let forward = row(basis, 2);
    let (right, up) = level_frame(forward).unwrap();
    let given_up = row(basis, 1);
    (
        forward[1].asin(),
        dot(given_up, right).atan2(dot(given_up, up)),
    )
}

#[test]
fn first_person_rolls_half_as_far_at_once() {
    // A hull pitched 0.2 and rolled 0.3: the drawn view keeps the pitch and
    // the heading and rolls 0.15, from its first frame, without lag.
    let drawn = first_person(posed(0.3, 0.2, 0.3), [0.0; 3]);
    let (pitch, roll) = attitude(drawn);
    assert!((pitch - 0.2).abs() < 1.0e-3, "{pitch}");
    assert!((roll - 0.15).abs() < 1.0e-3, "{roll}");
    let forward = row(drawn, 2);
    assert!((forward[0].atan2(forward[2]) - 0.3).abs() < 1.0e-3);
    // A level view stays level.
    let (pitch, roll) = attitude(first_person(posed(-1.0, 0.0, 0.0), [0.0; 3]));
    assert!(pitch.abs() < 1.0e-4 && roll.abs() < 1.0e-4);
    // The shake turns it a fraction of a degree.
    let (pitch, _) = attitude(first_person(posed(0.3, 0.2, 0.0), [0.0, 1.0, 0.0]));
    assert!(
        (pitch - 0.2 - SHAKE_DEGREES.to_radians()).abs() < 1.0e-3,
        "{pitch}"
    );
}

/// `frames` frames of `drive` through the shake, 16 ms apart: the largest
/// value it gives on any axis.
fn strongest(
    state: &mut CameraClearance,
    frames: usize,
    mut drive: impl FnMut(usize) -> Drive,
) -> f32 {
    (0..frames)
        .map(|frame| {
            let trembling = shake(state, Some(drive(frame)), false, 0.016);
            trembling.map(f32::abs).into_iter().fold(0.0, f32::max)
        })
        .fold(0.0, f32::max)
}

#[test]
fn a_boost_shakes_the_truck_briefly_and_speed_alone_never_shakes() {
    let truck = |boost: bool| Drive {
        forward: 1500.0,
        boost,
        form: 2,
        ..Drive::default()
    };
    let mut state = CameraClearance::default();
    // Full speed alone rests.
    assert_eq!(strongest(&mut state, 60, |_| truck(false)), 0.0);
    // The boost's first frame starts the burst.
    let burst = strongest(&mut state, 20, |_| truck(true));
    assert!(burst > 0.05 && burst <= 1.0, "{burst}");
    // Past the burst a boost alone rests.
    assert_eq!(shake(&mut state, Some(truck(true)), false, 0.2), [0.0; 3]);
    // A fresh boost under retail's shake adds nothing.
    let mut state = CameraClearance::default();
    assert_eq!(shake(&mut state, Some(truck(true)), true, 0.016), [0.0; 3]);
}

#[test]
fn the_skimmer_pushed_on_rumbles_a_little_harder() {
    let skimmer = |pushed: bool| Drive {
        forward: 1500.0,
        pushed,
        form: 5,
        ..Drive::default()
    };
    let mut state = CameraClearance::default();
    assert_eq!(strongest(&mut state, 60, |_| skimmer(false)), 0.0);
    // It comes in gently, and reaches past the rumble's share, never past
    // 1.12 times it.
    let first = shake(&mut state, Some(skimmer(true)), false, 0.016)[0].abs();
    assert!(first < 0.03, "{first}");
    let rumble = strongest(&mut state, 2000, |_| skimmer(true));
    assert!(
        rumble > SKIM_SHARE && rumble <= SKIM_SHARE * SKIMMER_STRENGTH,
        "{rumble}"
    );
}

#[test]
fn the_buggy_shakes_harder_as_it_gains_speed_and_while_it_boosts() {
    let buggy = |forward: f32, boost: bool| Drive {
        forward,
        boost,
        pushed: true,
        form: 1,
    };
    let truck = |forward: f32, boost: bool| Drive {
        form: 2,
        ..buggy(forward, boost)
    };
    // Standing and cruising rest, and so does rolling faster down a slope
    // with nothing held.
    for cruise in [0.0, 1084.0] {
        let mut state = CameraClearance::default();
        assert_eq!(strongest(&mut state, 60, |_| buggy(cruise, false)), 0.0);
    }
    let mut state = CameraClearance::default();
    let rolling = |frame: usize| Drive {
        pushed: false,
        ..buggy(frame as f32 * 600.0 * 0.016, false)
    };
    assert_eq!(strongest(&mut state, 60, rolling), 0.0);
    // Picking up speed on the throttle, as the buggy does from a standstill
    // (600 a second each second), rumbles it gently.
    let mut state = CameraClearance::default();
    let throttle = |frame: usize| buggy(frame as f32 * 600.0 * 0.016, false);
    let gentle = strongest(&mut state, 60, throttle);
    assert!(gentle > 0.05 && gentle < 0.3, "{gentle}");
    // A boost from a standstill: 1731 in about a second, held two more. The
    // burst shakes the buggy harder than the truck, and the buggy rumbles on
    // while the boost runs, where the truck rests.
    let speed = |frame: usize| (frame as f32 * 1731.0 / 60.0).min(1731.0);
    let mut buggy_state = CameraClearance::default();
    let mut truck_state = CameraClearance::default();
    let buggy_burst = strongest(&mut buggy_state, 25, |frame| buggy(speed(frame), true));
    let truck_burst = strongest(&mut truck_state, 25, |frame| truck(speed(frame), true));
    assert!(
        buggy_burst > 1.2 * truck_burst && buggy_burst <= BUGGY_STRENGTH,
        "{buggy_burst} {truck_burst}"
    );
    let buggy_on = strongest(&mut buggy_state, 160, |frame| {
        buggy(speed(frame + 25), true)
    });
    let truck_on = strongest(&mut truck_state, 160, |frame| {
        truck(speed(frame + 25), true)
    });
    assert!(buggy_on > 0.2 && buggy_on <= BUGGY_STRENGTH, "{buggy_on}");
    assert_eq!(truck_on, 0.0);
    // The boost over, it settles at speed.
    strongest(&mut buggy_state, 120, |_| buggy(1731.0, false));
    assert_eq!(
        shake(&mut buggy_state, Some(buggy(1731.0, false)), false, 0.016),
        [0.0; 3]
    );
}

#[test]
fn the_spring_starts_and_ends_gently_within_its_speed() {
    let tick = 0.016384;
    let mut value = 0.0;
    let mut speed = 0.0;
    let mut steps = Vec::new();
    for _ in 0..200 {
        let next = smooth_damp(value, 1.0, &mut speed, 0.25, 1.0, tick);
        assert!(next >= value && next <= 1.0, "{value} -> {next}");
        steps.push(next - value);
        value = next;
    }
    assert!(value > 0.999, "{value}");
    // It never outruns its speed, and gathers speed rather than leaping.
    assert!(steps.iter().all(|&step| step <= tick * 1.05), "{steps:?}");
    assert!(steps[0] < steps[5] && steps[5] < steps[20], "{steps:?}");
    // A paused frame moves nothing.
    assert_eq!(smooth_damp(0.5, 1.0, &mut speed, 0.25, 1.0, 0.0), 0.5);
}

#[test]
fn a_passing_block_never_lifts_a_lasting_one_rises_holds_and_settles() {
    let tick = 0.016384;
    let mut state = CameraClearance::default();
    // Something passes between the camera and the craft for 0.15 s.
    for _ in 0..9 {
        step_lift(&mut state, 0.5, tick);
    }
    for _ in 0..120 {
        step_lift(&mut state, 0.0, tick);
        assert_eq!(state.lift, 0.0);
    }
    // A block that lasts: the lift rises within its speed, never past what
    // is needed.
    let mut last = 0.0;
    for _ in 0..90 {
        step_lift(&mut state, 0.5, tick);
        assert!(
            state.lift - last <= LIFT_SPEED * tick * 1.05,
            "{last} -> {}",
            state.lift
        );
        assert!(state.lift <= 0.5);
        last = state.lift;
    }
    assert!(state.lift > 0.49, "{}", state.lift);
    // Clear again: it holds its height a while, then settles slowly.
    let held = (HOLD_SECONDS / tick) as usize - 2;
    for _ in 0..held {
        step_lift(&mut state, 0.0, tick);
        assert!(state.lift > 0.49);
    }
    while state.lift_target > 0.0 {
        step_lift(&mut state, 0.0, tick);
    }
    let mut last = state.lift;
    let mut ticks = 0;
    while state.lift > 1.0e-3 {
        step_lift(&mut state, 0.0, tick);
        assert!(state.lift <= last && last - state.lift <= LIFT_SPEED * tick * 1.05);
        last = state.lift;
        ticks += 1;
        assert!(ticks < 300, "{}", state.lift);
    }
    assert!(ticks > 30, "{ticks}");
}

#[test]
fn a_jump_of_the_games_eye_is_a_cut() {
    let tick = 0.016384;
    assert!(!is_cut([0, 0, 0], [100, -200, 50], tick));
    assert!(is_cut([0, 0, 0], [0, 500, 0], tick));
    // Across the world's wrap the eye moved 136.
    assert!(!is_cut([32_700, 0, 0], [-32_700, 0, 0], tick));
    // A long frame at speed is the camera's own motion; a respawn is not.
    assert!(!is_cut([0, 0, 0], [0, 600, 0], 0.1));
    assert!(is_cut([0, 0, 0], [0, 2000, 0], 0.1));
}

#[test]
fn the_noise_is_smooth_and_bounded() {
    let mut previous = noise(1, 0.0);
    for step in 1..2000 {
        let value = noise(1, step as f32 * 0.01);
        assert!((-1.0..=1.0).contains(&value));
        assert!((value - previous).abs() < 0.06, "{previous} -> {value}");
        previous = value;
    }
    assert_ne!(noise(0, 3.5), noise(1, 3.5));
}

#[test]
fn the_leash_holds_a_lag_past_its_reach_and_never_further() {
    let kept = |far: f32| {
        let lag = [far * 0.6, 0.0, -far * 0.8];
        let pull = leash_pull(lag, 200.0, &mut 0.0);
        // The pull runs straight back along the lag.
        assert!(dot(pull, lag) <= 0.0, "{pull:?}");
        length(std::array::from_fn(|axis| lag[axis] + pull[axis]))
    };
    // Within its reach the game's own lag stands.
    assert_eq!(leash_pull([120.0, 0.0, -160.0], 200.0, &mut 0.0), [0.0; 3]);
    let mut previous = 0.0;
    for step in 0..400 {
        let far = step as f32 * 5.0;
        let now = kept(far);
        // As the game's lag grows, the drawn one grows less, up to the slack.
        assert!(now >= previous - 1.0e-3 && now <= 200.0 * LEASH_SLACK + 1.0e-3);
        assert!(now <= far + 1.0e-3);
        previous = now;
    }
    // No step where it takes hold.
    assert!((kept(201.0) - 201.0).abs() < 0.1);
}

#[test]
fn slowing_down_keeps_the_leash_until_the_games_lag_closes() {
    let drawn = |lag: [f32; 3], reach: f32, held: &mut f32| {
        let pull = leash_pull(lag, reach, held);
        length(std::array::from_fn(|axis| lag[axis] + pull[axis]))
    };
    let mut held = 0.0;
    // At speed the game's eye lags 1200; the drawn one is held near 200.
    let fast = drawn([1200.0, 0.0, 0.0], 200.0, &mut held);
    assert!(fast <= 200.0 * LEASH_SLACK + 1.0e-3, "{fast}");
    // A crash: the speed's reach is slack again, but the lag stands, so the
    // view stays where it was drawn rather than swinging back out to it.
    assert!((drawn([1150.0, 0.0, 0.0], 3000.0, &mut held) - fast).abs() < 1.0);
    // The game closes its lag within the leash: it lets go there, and stays
    // slack while slow.
    assert_eq!(drawn([150.0, 0.0, 0.0], 3000.0, &mut held), 150.0);
    assert_eq!(drawn([1000.0, 0.0, 0.0], 3000.0, &mut held), 1000.0);
}

/// The drawn view's angle off the craft at `anchor` (from the eye at the
/// origin, the game's view along +Z) after one frame of the framing.
fn framing(state: &mut CameraClearance, anchor: [i16; 3], mode: u32, seconds: f32) -> f32 {
    let view = CameraDrawRequest {
        eye: [0, 0, 0],
        basis: posed(0.0, 0.0, 0.0),
        coordinate_shift: 16,
    };
    let anchor_words = anchor.map(|word| word as u16);
    let forward = row(framed(state, view, anchor_words, mode, seconds).basis, 2);
    dot(forward, normalized(anchor.map(f32::from)))
        .acos()
        .to_degrees()
}

#[test]
fn the_view_swings_onto_a_craft_it_lost_and_lets_go_gently() {
    let tick = 0.016384;
    let mut state = CameraClearance::default();
    // A craft within the frame never turns the view.
    let view = CameraDrawRequest {
        eye: [0, 0, 0],
        basis: posed(0.0, 0.0, 0.0),
        coordinate_shift: 16,
    };
    assert_eq!(framed(&mut state, view, [300, 0, 1000], 3, tick), view);
    // The craft 60 degrees off to the side of the view.
    let side = [866, 0, 500];
    let mut angles = vec![framing(&mut state, side, 3, tick)];
    for _ in 0..60 {
        angles.push(framing(&mut state, side, 3, tick));
    }
    // It gathers speed, never turns faster than its rate, and ends with the
    // craft within the frame's reach.
    let steps: Vec<f32> = angles.windows(2).map(|pair| pair[0] - pair[1]).collect();
    assert!(60.0 - angles[0] < steps[3], "{angles:?}");
    assert!(steps
        .iter()
        .all(|&step| step <= FRAME_RATE.to_degrees() * tick * 1.05));
    let last = *angles.last().unwrap();
    assert!(
        last > FRAME_DEGREES[0] && last <= FRAME_DEGREES[1],
        "{last}"
    );
    // Out of the chase (the death camera) it lets go the same way.
    let first = framing(&mut state, side, 0, tick);
    assert!(first < last + 1.0, "{first}");
    for _ in 0..120 {
        framing(&mut state, side, 0, tick);
    }
    assert!((framing(&mut state, side, 0, tick) - 60.0).abs() < 0.01);
    // A craft 120 degrees off needs a turn its rate holds back.
    let mut state = CameraClearance::default();
    let behind = [866, 0, -500];
    let mut previous = framing(&mut state, behind, 3, tick);
    let mut fastest = 0.0f32;
    for _ in 0..60 {
        let now = framing(&mut state, behind, 3, tick);
        fastest = fastest.max(previous - now);
        previous = now;
    }
    let most = FRAME_RATE.to_degrees() * tick;
    assert!(fastest <= most * 1.01 && fastest > most * 0.8, "{fastest}");
}
