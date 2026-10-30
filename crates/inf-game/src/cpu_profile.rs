//! Port-only and opt-in (`INF_CPU_PROFILE=1`): the CPU's time per frame in
//! named parts, a table when the Vulkan device closes. A part is timed from
//! its [`scope`] to the end of the scope's guard; parts nest, so a part's
//! time includes the parts within it. The first frames, which load the
//! world, are left out. Off, a scope costs one relaxed load.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicU8, Ordering},
        OnceLock,
    },
    time::Instant,
};

/// The frames left out at the start.
const WARMUP_FRAMES: u64 = 30;

static ENABLED: OnceLock<bool> = OnceLock::new();
static REPORTED: AtomicU8 = AtomicU8::new(0);

fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("INF_CPU_PROFILE").is_some_and(|value| value != "0"))
}

#[derive(Default)]
struct Part {
    /// This frame's time and calls.
    frame: f64,
    frame_calls: u32,
    /// Over the measured frames: the time, the most in one frame, the calls.
    sum: f64,
    most: f64,
    calls: u64,
}

#[derive(Default)]
struct Profile {
    frames: u64,
    measured: u64,
    parts: HashMap<&'static str, Part>,
}

thread_local! {
    static PROFILE: RefCell<Profile> = RefCell::new(Profile::default());
}

/// A part being timed; its time is added when it drops.
pub struct Scope {
    name: &'static str,
    began: Instant,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let span = self.began.elapsed().as_secs_f64() * 1000.0;
        PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            let part = profile.parts.entry(self.name).or_default();
            part.frame += span;
            part.frame_calls += 1;
        });
    }
}

/// Time the part `name` until the guard drops; none while profiling is off.
#[must_use]
pub fn scope(name: &'static str) -> Option<Scope> {
    enabled().then(|| Scope {
        name,
        began: Instant::now(),
    })
}

/// A frame ends: its parts count, after the warm-up.
pub fn frame_end() {
    if !enabled() {
        return;
    }
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        profile.frames += 1;
        let measured = profile.frames > WARMUP_FRAMES;
        if measured {
            profile.measured += 1;
        }
        for part in profile.parts.values_mut() {
            if measured {
                part.sum += part.frame;
                part.most = part.most.max(part.frame);
                part.calls += u64::from(part.frame_calls);
            }
            part.frame = 0.0;
            part.frame_calls = 0;
        }
    });
}

/// The table, once: each part's mean and most milliseconds a frame and its
/// calls a frame, the costliest first.
pub fn report() {
    if !enabled() || REPORTED.swap(1, Ordering::Relaxed) != 0 {
        return;
    }
    PROFILE.with(|profile| {
        let profile = profile.borrow();
        if profile.measured == 0 {
            return;
        }
        let frames = profile.measured as f64;
        let mut parts: Vec<_> = profile.parts.iter().collect();
        parts.sort_by(|a, b| b.1.sum.total_cmp(&a.1.sum));
        eprintln!(
            "CPU profile: {} frames after {WARMUP_FRAMES}, ms a frame (mean, most, calls a frame):",
            profile.measured
        );
        for (name, part) in parts {
            eprintln!(
                "  {name:<40} {:>7.3} {:>8.3} {:>7.1}",
                part.sum / frames,
                part.most,
                part.calls as f64 / frames
            );
        }
    });
}
