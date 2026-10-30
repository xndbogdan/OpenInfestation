//! Results-screen (phase-8) tick decisions: `0042CFC0` head, timer
//! reciprocal and `+0x148` tail exits.
//!
//! Native behavior, recovered from retail disassembly of the complete tick
//! (`0042CFC0..0042D31F`; see the block map in [results
//! runtime](../../../docs/re/RESULTS_RUNTIME.md#phase-8-results-screen)):
//!
//! - Head: world field `+0x250 == 0x16` or attract set requests phase 3
//!   with fall-through (verified live: caller `0042CFF5`).
//! - Both timer sites scale the scheduler timing word by the
//!   `0x10624DD3>>6` reciprocal (microsecond-to-millisecond scaling): the
//!   `+0x14C/+0xC0` accumulation uses the signed `imul/sar` form, the
//!   `+0x148` comparison the unsigned `mul/shr` form. A greater tail
//!   subtracts the signed scale with DWORD wrapping. The scalers agree on
//!   non-negative inputs; the one-scale tail helper is bounded to that
//!   agreement domain, not complete negative-argument native behavior.
//! - Tail: a zero `+0x148` skips to the epilogue; otherwise it counts down
//!   by the scaled timing, and at zero resets to 1 and requests phase 2
//!   when `+F5` is set, else phase 3.
//!
//! Pure decisions over explicit inputs. Save-owner calls (`00449C90`,
//! `00449C80`, `00449C10`, `0044A110`), the `00511C90` dispatch, object
//! ticks (`004554E0`, `0042B190/350/510`), the profile menu (`00449A00`)
//! retain separate required owners. The admitted fresh intro Tick submits
//! `00438380` and advances base input/age and the shared animation clock.

use crate::session::SessionPhase;

/// `0042CFC0` head: world complete or attract requests phase 3; otherwise
/// the tick continues without a request. The native code falls through
/// after the request instead of returning.
pub fn loop_head_request(world_status: u32, attract: bool) -> Option<SessionPhase> {
    if world_status == crate::gameplay_menu::WORLD_COMPLETE_STATUS || attract {
        Some(SessionPhase::Phase3)
    } else {
        None
    }
}

/// Reciprocal word shared by both timer sites: `0x10624DD3`.
pub const TIMER_RECIPROCAL: u32 = 0x1062_4DD3;

/// Signed `imul; sar 6; add (edx lsr 31)` scaling at `0042D06A`.
pub fn scale_tick_timing(timing: i32) -> i32 {
    let high = ((timing as i64).wrapping_mul(TIMER_RECIPROCAL as i64) >> 32) as i32;
    high.div_euclid(64).wrapping_add((high as u32 >> 31) as i32)
}

/// Unsigned `mul; shr 6` scaling at `0042D2AC`.
pub fn scale_tail_timing(timing: u32) -> u32 {
    ((((timing as u64) * (TIMER_RECIPROCAL as u64)) >> 32) as u32) >> 6
}

/// `+0x148` tail countdown: the new field value plus the tick exit, if any.
/// Zero skips; reaching the supplied scale resets to 1 and exits to the
/// frontend (`+F5`) or back to briefing. This helper assumes native signed
/// and unsigned scales agree, as they do for nonnegative scheduler inputs.
pub fn tail_countdown(
    field_148: u32,
    scaled: u32,
    return_to_frontend: bool,
) -> (u32, Option<SessionPhase>) {
    if field_148 == 0 {
        (0, None)
    } else if field_148 > scaled {
        (field_148 - scaled, None)
    } else {
        (
            1,
            Some(if return_to_frontend {
                SessionPhase::Frontend
            } else {
                SessionPhase::Phase3
            }),
        )
    }
}

#[cfg(test)]
#[path = "results_tick_tests.rs"]
mod tests;
