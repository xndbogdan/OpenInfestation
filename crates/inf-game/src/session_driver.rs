//! Joins session dispatch to the two native scheduler passes.

use crate::scheduler::{Scheduler, SchedulerActivity, SchedulerHost, SchedulerTick};

use super::{
    PhaseTransition, SessionController, SessionError, SessionHost, SessionPhase, SessionState,
};

/// Port-only admission before the native transition touches the current screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionTransitionAdmission {
    Proceed,
    /// Keep the current screen and cancel only the pending request. The host
    /// must name and report the deferred feature; this is not native success.
    Deferred,
}

/// Callbacks of the scheduler-linked screen/context and selected input owner.
/// The host retains their display context (scheduler context +4) and current
/// screen link (+18). These callbacks remain separate from world simulation.
/// All three linked callbacks must reject screen attachment/replacement while
/// running (004946F0 checks the linked owner's busy flag). The host owns that
/// registry invariant; this adapter does not implement native registration.
pub trait SessionFrameHost: SessionHost {
    fn input_is_selected(&self) -> bool;
    fn poll_input(&mut self, state: &mut SessionState) -> Result<(), Self::Error>;
    /// Port-only, with no native counterpart: called immediately before
    /// session +08 while a request is pending. An error stops the session
    /// before 004383B0 queries or deletes the current screen, so a port that
    /// has not implemented the requested phase's effects/factory keeps the
    /// current screen intact. Proceed promises the effects/factory exist;
    /// Deferred keeps the current screen and continues its scheduled callbacks.
    /// Errors retain the pending request and stop, as for other host failures.
    fn admit_transition(
        &mut self,
        state: &SessionState,
    ) -> Result<SessionTransitionAdmission, Self::Error>;
    fn linked_tick(
        &mut self,
        tick: SchedulerTick,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
    fn linked_callback_08(&mut self, state: &mut SessionState) -> Result<(), Self::Error>;
    fn linked_callback_0c(&mut self, state: &mut SessionState) -> Result<(), Self::Error>;
    fn linked_activity(
        &mut self,
        activity: SchedulerActivity,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
}

/// Progress through one scheduler iteration's linked callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkedStage {
    /// The last first pass was gated (or none ran): the second pass is empty.
    Ineligible,
    Eligible,
    /// Linked +04 ran; +08/+0C must follow before the next first pass.
    Updated,
    /// The iteration's linked callbacks are complete.
    Presented,
}

/// One session's controller and timing with its host borrowed per call.
/// Hosts that borrow per-iteration state (a renderer bound to a residency
/// snapshot, a window) cannot be owned by the driver; [`ScheduledSession`]
/// wraps this for owned hosts. With multiple sessions, execute every first
/// pass before any second pass, each native list newest first.
#[derive(Debug)]
pub struct SessionDriver {
    controller: SessionController,
    scheduler: Scheduler,
    stage: LinkedStage,
    transitions: Vec<PhaseTransition>,
}

impl SessionDriver {
    /// Supply the clock sample corresponding to construction at `004944D0`.
    pub fn new(initial_clock_micros: u64) -> Self {
        Self {
            controller: SessionController::default(),
            scheduler: Scheduler::new(initial_clock_micros),
            stage: LinkedStage::Ineligible,
            transitions: Vec::new(),
        }
    }

    pub fn state(&self) -> &SessionState {
        self.controller.state()
    }
    pub fn request_phase(&mut self, phase: SessionPhase) -> bool {
        self.controller.request_phase(phase)
    }
    /// Completed phase transitions in execution order, including direct entry.
    pub fn transitions(&self) -> &[PhaseTransition] {
        &self.transitions
    }

    pub fn create_startup_screen<H: SessionHost>(
        &mut self,
        host: &mut H,
        context: H::ScreenContext,
    ) -> Result<(), SessionError<H::Error>> {
        self.controller.create_startup_screen(host, context)
    }

    /// See [`SessionController::enter_frontend_directly`].
    pub fn enter_frontend_directly<H: SessionHost>(
        &mut self,
        host: &mut H,
        context: H::ScreenContext,
    ) -> Result<PhaseTransition, SessionError<H::Error>> {
        let transition = self.controller.enter_frontend_directly(host, context)?;
        self.transitions.push(transition);
        Ok(transition)
    }

    pub fn first_pass<H: SessionFrameHost>(
        &mut self,
        clock_micros: u64,
        host: &mut H,
    ) -> Result<Option<SchedulerTick>, SessionError<H::Error>> {
        self.controller.ensure_running()?;
        if matches!(self.stage, LinkedStage::Eligible | LinkedStage::Updated) {
            return Err(SessionError::LinkedPassOrder);
        }
        self.stage = LinkedStage::Ineligible;
        let tick = self.scheduler.first_pass(
            clock_micros,
            &mut Callbacks {
                controller: &mut self.controller,
                host,
                transitions: &mut self.transitions,
            },
        )?;
        if tick.is_some() {
            self.stage = LinkedStage::Eligible;
        }
        Ok(tick)
    }

    /// The whole second pass: [`Self::linked_update`] then [`Self::linked_present`].
    pub fn second_pass<H: SessionFrameHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), SessionError<H::Error>> {
        self.linked_update(host)?;
        self.linked_present(host)
    }

    /// Linked +04 only. A gated iteration has no linked callbacks; repeating
    /// the update within one iteration is rejected rather than replayed.
    pub fn linked_update<H: SessionFrameHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), SessionError<H::Error>> {
        self.controller.ensure_running()?;
        match self.stage {
            LinkedStage::Ineligible => return Ok(()),
            LinkedStage::Eligible => {}
            LinkedStage::Updated | LinkedStage::Presented => {
                return Err(SessionError::LinkedPassOrder)
            }
        }
        self.stage = LinkedStage::Updated;
        self.scheduler.linked_update(&mut Callbacks {
            controller: &mut self.controller,
            host,
            transitions: &mut self.transitions,
        })?;
        Ok(())
    }

    /// Linked +08 draw and +0C presentation, after this iteration's update.
    pub fn linked_present<H: SessionFrameHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), SessionError<H::Error>> {
        self.controller.ensure_running()?;
        match self.stage {
            LinkedStage::Ineligible => return Ok(()),
            LinkedStage::Updated => {}
            LinkedStage::Eligible | LinkedStage::Presented => {
                return Err(SessionError::LinkedPassOrder)
            }
        }
        self.stage = LinkedStage::Presented;
        self.scheduler.linked_present(&mut Callbacks {
            controller: &mut self.controller,
            host,
            transitions: &mut self.transitions,
        })
    }

    pub fn activity<H: SessionFrameHost>(
        &mut self,
        activity: SchedulerActivity,
        host: &mut H,
    ) -> Result<(), SessionError<H::Error>> {
        self.controller.ensure_running()?;
        self.scheduler.notify_activity(
            activity,
            &mut Callbacks {
                controller: &mut self.controller,
                host,
                transitions: &mut self.transitions,
            },
        )
    }
}

/// One session and its linked screen/context, owning its host.
/// The host supplies the screen, audio, and world implementations.
pub struct ScheduledSession<H> {
    driver: SessionDriver,
    host: H,
}

impl<H: SessionFrameHost> ScheduledSession<H> {
    pub fn new(host: H, initial_clock_micros: u64) -> Self {
        Self {
            driver: SessionDriver::new(initial_clock_micros),
            host,
        }
    }

    pub fn state(&self) -> &SessionState {
        self.driver.state()
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
    pub fn request_phase(&mut self, phase: SessionPhase) -> bool {
        self.driver.request_phase(phase)
    }

    pub fn create_startup_screen(
        &mut self,
        context: H::ScreenContext,
    ) -> Result<(), SessionError<H::Error>> {
        self.driver.create_startup_screen(&mut self.host, context)
    }

    pub fn first_pass(
        &mut self,
        clock_micros: u64,
    ) -> Result<Option<SchedulerTick>, SessionError<H::Error>> {
        self.driver.first_pass(clock_micros, &mut self.host)
    }

    pub fn second_pass(&mut self) -> Result<(), SessionError<H::Error>> {
        self.driver.second_pass(&mut self.host)
    }

    pub fn activity(&mut self, activity: SchedulerActivity) -> Result<(), SessionError<H::Error>> {
        self.driver.activity(activity, &mut self.host)
    }
}

struct Callbacks<'a, H> {
    controller: &'a mut SessionController,
    host: &'a mut H,
    transitions: &'a mut Vec<PhaseTransition>,
}

impl<H> Callbacks<'_, H> {
    /// Only the explicit admission result reaches this port-only cancellation.
    /// No kind/reuse/delete/effect/factory/input-reset operation has run.
    fn defer_pending_transition(&mut self) {
        debug_assert!(self.controller.state.pending);
        self.controller.state.pending = false;
        self.controller.state.requested = self.controller.state.current;
    }
}

impl<H: SessionFrameHost> SchedulerHost for Callbacks<'_, H> {
    type Error = SessionError<H::Error>;

    fn input_is_selected(&self) -> bool {
        self.host.input_is_selected()
    }

    fn poll_input(&mut self) -> Result<(), Self::Error> {
        let result = self.host.poll_input(&mut self.controller.state);
        self.controller.finish_callback(result)
    }

    fn transition(&mut self) -> Result<(), Self::Error> {
        self.controller.ensure_running()?;
        let words = |state: &SessionState| {
            [
                u32::from(state.current_phase().id()),
                u32::from(state.requested_phase().id()),
                u32::from(state.transition_pending()),
            ]
        };
        let entry = words(&self.controller.state);
        if self.controller.state.pending {
            let result = self.host.admit_transition(&self.controller.state);
            if self.controller.finish_callback(result)? == SessionTransitionAdmission::Deferred {
                self.defer_pending_transition();
                crate::event_log::phase(entry, words(&self.controller.state));
                return Ok(());
            }
        }
        if let Some(transition) = self.controller.transition(self.host)? {
            self.transitions.push(transition);
        }
        crate::event_log::phase(entry, words(&self.controller.state));
        Ok(())
    }

    fn session_tick(&mut self, tick: SchedulerTick) -> Result<(), Self::Error> {
        self.controller
            .tick(self.host, tick.delta_micros, tick.elapsed_micros)
    }

    fn linked_tick(&mut self, tick: SchedulerTick) -> Result<(), Self::Error> {
        let result = self.host.linked_tick(tick, &mut self.controller.state);
        self.controller.finish_callback(result)
    }

    fn linked_callback_08(&mut self) -> Result<(), Self::Error> {
        let result = self.host.linked_callback_08(&mut self.controller.state);
        self.controller.finish_callback(result)
    }

    fn linked_callback_0c(&mut self) -> Result<(), Self::Error> {
        let result = self.host.linked_callback_0c(&mut self.controller.state);
        self.controller.finish_callback(result)
    }

    fn session_activity(&mut self, activity: SchedulerActivity) -> Result<(), Self::Error> {
        self.controller.state.suspended = activity == SchedulerActivity::Suspend;
        Ok(())
    }

    fn linked_activity(&mut self, activity: SchedulerActivity) -> Result<(), Self::Error> {
        let result = self
            .host
            .linked_activity(activity, &mut self.controller.state);
        self.controller.finish_callback(result)
    }
}
