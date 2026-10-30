//! The session's microsecond clock and two scheduler passes.
//!
//! `004944D0` samples the initial clock; `004945A0` admits an update only after
//! 8,000 microseconds and clamps its first argument to 125,000. These are the
//! arguments supplied by the session constructor `004389C0`. `004943A0` runs
//! every session's first pass before any linked object's second pass. The
//! session list `0055F4D0` and the linked list `0055F4B8` start empty
//! (`004942F0`, `00494320`).
//!
//! The platform loop runs `004943A0` once an iteration through `00438E30`,
//! inside a `setjmp` error frame whose error screens, message boxes and
//! display recovery the port's typed errors and crash log replace.
//!
//! Time is supplied explicitly. The retail `004A4890` clock is `timeGetTime`
//! multiplied by 1,000, but its operating-system resolution and message pump
//! are not simulated here. Nor does this module implement input, screen,
//! gameplay, or presentation callbacks: the host must supply their owners.

use serde::Serialize;

pub const MIN_TICK_MICROS: i32 = 8_000;
pub const MAX_TICK_MICROS: i32 = 125_000;

/// The two arguments to both session and linked-object virtual `+0x04`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SchedulerTick {
    /// Elapsed time clamped to the session's maximum update interval.
    pub delta_micros: i32,
    /// Unclamped elapsed time, from a signed low-word subtraction.
    pub elapsed_micros: i32,
}

/// Window/menu-loop notifications, independent of scheduler eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerActivity {
    /// `00494430`: session `+0x0C`, then linked object `+0x14`.
    Suspend,
    /// `00494480`: session `+0x10`, then linked object `+0x10`.
    Resume,
}

/// The callback owners for one session and its linked second-pass object.
///
/// There are deliberately no default callbacks. `004946F0` prohibits replacing
/// the linked object during session `+0x04` or the second pass; its replacement
/// is allowed during `transition`, before the native busy flag is set. Hosts
/// must retain that ownership boundary. Each linked method must resolve the
/// current object when called, as `004943A0` rereads the owner between calls.
pub trait SchedulerHost {
    type Error;

    /// Whether `0055F4B0` identifies an input object belonging to this session.
    fn input_is_selected(&self) -> bool;

    /// `004F0200`, invoked before the session transition only when selected.
    fn poll_input(&mut self) -> Result<(), Self::Error>;

    /// Session virtual `+0x08`; this can replace the current screen/tick owner.
    fn transition(&mut self) -> Result<(), Self::Error>;

    /// Session virtual `+0x04`, dispatching the newly selected tick owner.
    fn session_tick(&mut self, tick: SchedulerTick) -> Result<(), Self::Error>;

    /// Linked object virtual `+0x04`, receiving the same two time arguments.
    fn linked_tick(&mut self, tick: SchedulerTick) -> Result<(), Self::Error>;

    /// Linked object virtual `+0x08`, with the second-pass owner's context.
    fn linked_callback_08(&mut self) -> Result<(), Self::Error>;

    /// Linked object virtual `+0x0C`, with that same context.
    /// The base implementation `00494830` calls `00495890` on the context.
    fn linked_callback_0c(&mut self) -> Result<(), Self::Error>;

    /// Session notification: `00438890` writes suspension flag `+0x60 = 1`;
    /// `004388B0` writes zero. Neither changes the scheduler clock.
    fn session_activity(&mut self, activity: SchedulerActivity) -> Result<(), Self::Error>;

    /// Owner adapters `004946D0/004946E0` invoke linked `+0x14/+0x10`.
    /// Base callbacks `00494810/004947F0` deselect/select the input object.
    fn linked_activity(&mut self, activity: SchedulerActivity) -> Result<(), Self::Error>;
}

/// Timing state for the retail session's fixed minimum/maximum intervals.
///
/// This contains neither a pause gate nor a catch-up accumulator. Subthreshold
/// elapsed time accumulates because the timestamp is retained. An admitted
/// update consumes all elapsed time even when its simulation delta was capped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheduler {
    last_clock_micros: u64,
    second_pass_tick: Option<SchedulerTick>,
}

impl Scheduler {
    /// Supply the clock sample corresponding to construction at `004944D0`.
    pub fn new(initial_clock_micros: u64) -> Self {
        Self {
            last_clock_micros: initial_clock_micros,
            second_pass_tick: None,
        }
    }

    pub fn last_clock_micros(&self) -> u64 {
        self.last_clock_micros
    }

    /// Run `004945A0` for one session. Run all first passes in native
    /// newest-first list order before any second pass for multiple sessions.
    ///
    /// The native subtraction at `004945C0` ignores both clock high words and
    /// compares the low-word result as signed (`jl` at `004945C7`). This matters
    /// at wraparound and for intervals beyond `i32::MAX` microseconds; using a
    /// saturating `Duration` subtraction would implement a different clock.
    ///
    /// The pre-callback sample is committed only after a successful session
    /// tick (`00494605..00494617`). Host errors stop dispatch and suppress the
    /// second pass. This is Rust failure containment, not a model of the
    /// retail nonlocal-jump error recovery or a rollback of host side effects.
    pub fn first_pass<H: SchedulerHost>(
        &mut self,
        now_micros: u64,
        host: &mut H,
    ) -> Result<Option<SchedulerTick>, H::Error> {
        self.second_pass_tick = None;
        let elapsed_micros = (now_micros as u32).wrapping_sub(self.last_clock_micros as u32) as i32;
        if elapsed_micros < MIN_TICK_MICROS {
            return Ok(None);
        }
        let tick = SchedulerTick {
            delta_micros: elapsed_micros.min(MAX_TICK_MICROS),
            elapsed_micros,
        };
        if host.input_is_selected() {
            host.poll_input()?;
        }
        host.transition()?;
        host.session_tick(tick)?;
        self.last_clock_micros = now_micros;
        self.second_pass_tick = Some(tick);
        Ok(Some(tick))
    }

    /// Run the linked-object part of `004943A0`, after every session first pass.
    /// The second pass is gated by that session's nonzero elapsed field, not by
    /// input selection or suspension. Every method obtains the current linked
    /// object from the host; no screen handle is cached across callbacks.
    ///
    /// Eligibility remains until the next first pass, just as the native raw
    /// elapsed field remains unchanged. The caller runs this once per scheduler
    /// iteration; retrying after a host error can repeat its earlier effects.
    pub fn second_pass<H: SchedulerHost>(&self, host: &mut H) -> Result<(), H::Error> {
        if self.linked_update(host)? {
            self.linked_present(host)?;
        }
        Ok(())
    }

    /// The linked `+04` part of the second pass (`004943F2`). Returns whether
    /// the linked object was eligible. A host that must replace borrowed
    /// presentation state between the tick and the draw splits the pass here;
    /// it must then call [`Self::linked_present`] before any other scheduler
    /// work, since native `004943A0` runs the three callbacks back to back.
    pub fn linked_update<H: SchedulerHost>(&self, host: &mut H) -> Result<bool, H::Error> {
        let Some(tick) = self.second_pass_tick else {
            return Ok(false);
        };
        host.linked_tick(tick)?;
        Ok(true)
    }

    /// The linked `+08` draw and `+0C` presentation (`004943FE`, `0049440A`).
    pub fn linked_present<H: SchedulerHost>(&self, host: &mut H) -> Result<(), H::Error> {
        if self.second_pass_tick.is_some() {
            host.linked_callback_08()?;
            host.linked_callback_0c()?;
        }
        Ok(())
    }

    /// Broadcast activity to this session and its linked object in native
    /// order. The owner of multiple sessions must broadcast to all sessions
    /// before all linked objects, each list newest first, using the
    /// corresponding host methods (004944D0/00494630 prepend registrations).
    ///
    /// `004F4980` can notify directly from the window procedure, or defer to
    /// `004F4DF0` after a scheduler iteration. Message coalescing and the native
    /// one-iteration throttle belong to that platform owner. No clock reset,
    /// elapsed-time discard, or implicit first-pass suppression happens here.
    pub fn notify_activity<H: SchedulerHost>(
        &self,
        activity: SchedulerActivity,
        host: &mut H,
    ) -> Result<(), H::Error> {
        host.session_activity(activity)?;
        host.linked_activity(activity)
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
