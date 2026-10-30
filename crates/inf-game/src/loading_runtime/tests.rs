use super::*;
use std::{fs, path::Path};

fn state() -> LoadingProcessState {
    LoadingProcessState {
        counter_546918: 11,
        counter_54691c: 22,
        counter_546920: 33,
        previous_clock: 100,
        raw_progress: u32::MAX - 2,
        age_micros: u32::MAX - 3,
        pulse_direction: 1,
        selector: 4,
        pulse: 0xabcd_1000,
    }
}

struct Host {
    now: u64,
    available: bool,
    fail: Option<&'static str>,
    events: Vec<&'static str>,
    tick: Option<(i32, i32, LoadingProcessState)>,
    disposed: Vec<&'static str>,
}

impl Host {
    fn new(now: u64) -> Self {
        Self {
            now,
            available: true,
            fail: None,
            events: Vec::new(),
            tick: None,
            disposed: Vec::new(),
        }
    }
    fn reached(&mut self, phase: &'static str) -> Result<(), &'static str> {
        self.events.push(phase);
        if self.fail == Some(phase) {
            Err(phase)
        } else {
            Ok(())
        }
    }
}

impl LoadingCallbackHost for Host {
    type Error = &'static str;
    fn sample_clock(&mut self) -> u64 {
        self.events.push("clock");
        self.now
    }
    fn renderer_available(&mut self) -> Result<bool, Self::Error> {
        self.reached("renderer")?;
        Ok(self.available)
    }
    fn tick_loading(&mut self, request: LoadingTickRequest<'_>) -> Result<(), Self::Error> {
        self.tick = Some((
            request.delta_micros,
            request.caller_status,
            request.state.clone(),
        ));
        // These are declared reached host effects, not a dummy success Tick.
        request.state.counter_546918 = 77;
        request.effect_bytes[0] = 81;
        name_entry_rng_next(request.rng);
        self.reached("tick")
    }
    fn present_loading(&mut self) -> Result<(), Self::Error> {
        self.reached("present")
    }
    fn dispose_loading_error(&mut self, error: Self::Error) {
        self.events.push("dispose");
        self.disposed.push(error);
    }
}

fn callback(
    s: &mut LoadingProcessState,
    effects: &mut [u8; 16],
    rng: &mut u32,
    host: &mut Host,
    origin: LoadingCallbackOrigin,
) -> u32 {
    loading_callback_40edb0(
        LoadingCallbackRequest {
            state: s,
            effect_bytes: effects,
            rng,
            raw_progress: 9,
            origin,
        },
        host,
    )
}

#[test]
fn startup_reads_authored_current_fields_and_rejects_wrong_or_truncated_pe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut exe = fs::read(root.join("Infest.exe")).expect("retail executable required");
    let original = LoadingProcessState::native_startup(&exe).unwrap();
    assert_eq!(
        original,
        LoadingProcessState {
            counter_546918: 0,
            counter_54691c: 0,
            counter_546920: 0,
            previous_clock: 0,
            raw_progress: 0,
            age_micros: 0,
            pulse_direction: 0,
            selector: 0,
            pulse: 0,
        }
    );
    // Same-length supplied image values discriminate decoding from captured
    // zero constants. The original file is never modified.
    let begin = 0x146918;
    for (offset, value) in [
        (0, 11u32),
        (4, 22),
        (8, 33),
        (16, 0x81234567),
        (20, 0x76543210),
        (24, 99),
        (48, 123),
        (56, 0xfedc5678),
    ] {
        exe[begin + offset..begin + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    exe[begin + 53] = 7;
    exe[begin + 54..begin + 56].copy_from_slice(&0x9876u16.to_le_bytes());
    assert_eq!(
        LoadingProcessState::native_startup(&exe).unwrap(),
        LoadingProcessState {
            counter_546918: 11,
            counter_54691c: 22,
            counter_546920: 33,
            previous_clock: 0x7654321081234567,
            raw_progress: 99,
            age_micros: 123,
            pulse_direction: 7,
            selector: 0x9876,
            pulse: 0xfedc5678,
        }
    );
    assert!(LoadingProcessState::native_startup(&exe[..begin + 59]).is_err());
    exe[0] = 0;
    assert!(LoadingProcessState::native_startup(&exe).is_err());
}

#[test]
fn low_dword_signed_delta_clamps_and_actual_full_clock_commits_before_render() {
    for (previous, now, delta) in [
        (100u64, 101u64, 7800),
        (100, 125101, 125000),
        (0xffff_fff0, 0x1_0000_0010, 7800),
        (0, 0x8000_0000, 7800),
        (0x1234_0000_0064, 0x5678_0000_2774, 10000),
    ] {
        let mut s = state();
        s.previous_clock = previous;
        let mut effects = [6; 16];
        let mut rng = 29;
        let mut host = Host::new(now);
        callback(
            &mut s,
            &mut effects,
            &mut rng,
            &mut host,
            LoadingCallbackOrigin::ResourceRead,
        );
        let captured = &host.tick.as_ref().unwrap().2;
        assert_eq!(host.tick.as_ref().unwrap().0, delta);
        assert_eq!(captured.previous_clock, now);
        assert_eq!(captured.raw_progress, 6);
        assert_eq!(
            captured.age_micros,
            (u32::MAX - 3).wrapping_add(delta as u32)
        );
        assert_eq!(
            [
                captured.counter_546918,
                captured.counter_54691c,
                captured.counter_546920
            ],
            [11, 22, 33]
        );
        assert_eq!(captured.pulse >> 16, 0xabcd);
        assert_eq!(host.events, ["clock", "renderer", "tick", "present"]);
    }
}

#[test]
fn pulse_strict_boundaries_keep_upper_word_and_use_rng_only_on_zero_turn() {
    // delta7800: rise step487; fall step975. Equality saturates/turns.
    for (direction, low, expected_low, expected_direction, random) in [
        (1, 65047, 65534, 1, false),
        (7, 65048, 65535, 0, false),
        (1, 65535, 65535, 0, false),
        (0, 976, 1, 0, false),
        (0, 975, 0, 1, true),
        (0, 0, 0, 1, true),
    ] {
        let mut s = state();
        s.pulse_direction = direction;
        s.pulse = 0xbeef0000 | low;
        let mut rng = 1234;
        let mut expected_rng = rng;
        let expected_selector = if random {
            (name_entry_rng_next(&mut expected_rng) % 5) as u16
        } else {
            4
        };
        let mut effects = [17; 16];
        let mut host = Host::new(101);
        host.available = false;
        assert_eq!(
            callback(
                &mut s,
                &mut effects,
                &mut rng,
                &mut host,
                LoadingCallbackOrigin::ResourceRead
            ),
            0
        );
        assert_eq!(s.pulse, 0xbeef0000 | expected_low);
        assert_eq!(s.pulse_direction, expected_direction);
        assert_eq!(s.selector, expected_selector);
        assert_eq!(rng, expected_rng);
        assert_eq!(effects, [17; 16]);
        assert_eq!(host.events, ["clock", "renderer"]);
    }
}

#[test]
fn current_renderer_null_preserves_prefix_but_never_ticks_or_presents() {
    let mut s = state();
    s.pulse_direction = 0;
    s.pulse = 0xfeed0000;
    let mut effects = [31; 16];
    let mut rng = 0;
    let mut expected_rng = 0;
    let selection = name_entry_rng_next(&mut expected_rng) % 5;
    let mut host = Host::new(200);
    host.available = false;
    assert_eq!(
        callback(
            &mut s,
            &mut effects,
            &mut rng,
            &mut host,
            LoadingCallbackOrigin::AttractHandshake
        ),
        0
    );
    assert_eq!(s.selector, selection as u16);
    assert_eq!(rng, expected_rng);
    assert_eq!(s.previous_clock, 200);
    assert_eq!(s.raw_progress, 6);
    assert_eq!(s.pulse, 0xfeed0000);
    assert_eq!(s.pulse_direction, 1);
    assert_eq!(effects, [31; 16]);
    assert!(host.tick.is_none());
    assert!(host.disposed.is_empty());
}

#[test]
fn every_guarded_error_is_disposed_without_rollback_or_later_phase() {
    for (failure, events) in [
        ("renderer", vec!["clock", "renderer", "dispose"]),
        ("tick", vec!["clock", "renderer", "tick", "dispose"]),
        (
            "present",
            vec!["clock", "renderer", "tick", "present", "dispose"],
        ),
    ] {
        let mut s = state();
        let mut effects = [9; 16];
        let mut rng = 123;
        let mut host = Host::new(10_100);
        host.fail = Some(failure);
        assert_eq!(
            callback(
                &mut s,
                &mut effects,
                &mut rng,
                &mut host,
                LoadingCallbackOrigin::ResourceRead
            ),
            0
        );
        assert_eq!(host.events, events);
        assert_eq!(host.disposed, [failure]);
        assert_eq!(s.previous_clock, 10_100);
        assert_eq!(s.raw_progress, 6);
        if failure == "renderer" {
            assert_eq!(s.counter_546918, 11);
            assert_eq!(effects, [9; 16]);
            assert_eq!(rng, 123);
        } else {
            assert_eq!(s.counter_546918, 77);
            assert_eq!(effects[0], 81);
            assert_eq!(effects[1..], [9; 15]);
            assert_ne!(rng, 123);
        }
    }
}

#[test]
fn genuine_callback_origins_forward_only_their_actual_stack_status() {
    for (origin, status) in [
        (LoadingCallbackOrigin::ResourceRead, 1),
        (LoadingCallbackOrigin::AttractHandshake, 0),
    ] {
        let mut s = state();
        let mut effects = [0; 16];
        let mut rng = 9;
        let mut host = Host::new(101);
        callback(&mut s, &mut effects, &mut rng, &mut host, origin);
        assert_eq!(host.tick.as_ref().unwrap().1, status);
    }
}
