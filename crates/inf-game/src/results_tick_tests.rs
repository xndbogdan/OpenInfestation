use super::{loop_head_request, scale_tail_timing, scale_tick_timing, tail_countdown};
use crate::gameplay_menu::WORLD_COMPLETE_STATUS;
use crate::session::SessionPhase;

#[test]
fn head_requests_briefing_on_complete_world_or_attract() {
    assert_eq!(
        loop_head_request(WORLD_COMPLETE_STATUS, false),
        Some(SessionPhase::Phase3)
    );
    assert_eq!(loop_head_request(0, true), Some(SessionPhase::Phase3));
    assert_eq!(loop_head_request(0, false), None);
}

#[test]
fn signed_scaler_floors_microseconds_to_milliseconds() {
    let vectors = [
        (0, 0),
        (1, 0),
        (999, 0),
        (1000, 1),
        (8000, 8),
        (125000, 125),
        (1000000, 1000),
        (0x7FFF_FFFF, 2147483),
        (-1000, -1),
    ];
    for (timing, expected) in vectors {
        assert_eq!(scale_tick_timing(timing), expected, "timing {timing}");
    }
}

#[test]
fn unsigned_scaler_matches_on_non_negative_inputs() {
    for timing in [0u32, 1, 999, 1000, 8000, 125000, 1000000, 0x7FFF_FFFF] {
        assert_eq!(
            scale_tail_timing(timing),
            scale_tick_timing(timing as i32) as u32,
            "timing {timing}"
        );
    }
}

#[test]
fn tail_skips_on_zero_and_counts_down_otherwise() {
    assert_eq!(tail_countdown(0, 8, false), (0, None));
    assert_eq!(tail_countdown(100, 8, false), (92, None));
    assert_eq!(tail_countdown(8, 8, false), (1, Some(SessionPhase::Phase3)));
}

#[test]
fn tail_exhaustion_selects_frontend_only_with_f5() {
    assert_eq!(
        tail_countdown(5, 8, true),
        (1, Some(SessionPhase::Frontend))
    );
    assert_eq!(tail_countdown(5, 8, false), (1, Some(SessionPhase::Phase3)));
}
