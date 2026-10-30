//! Session control at 00438380/004383B0/004394E0.
//!
//! Screens, world callbacks, audio and rendering belong to the required host.
//! This module implements dispatch and the native empty idle callback.
//! A host error stops this owner;
//! already executed callbacks cannot safely be replayed as a transaction.

use serde::Serialize;
use thiserror::Error;

/// Session IDs are independent of OVL leaf IDs and campaign/world selection.
/// Numeric names retain uncertainty about the role of the corresponding UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum SessionPhase {
    Startup = 0,
    Title = 1,
    Frontend = 2,
    Phase3 = 3,
    Gameplay = 4,
    Phase5 = 5,
    Phase6 = 6,
    Phase7 = 7,
    Phase8 = 8,
    Phase9 = 9,
    Phase10 = 10,
    Phase11 = 11,
    Phase12 = 12,
}

impl SessionPhase {
    pub const ALL: [Self; 13] = [
        Self::Startup,
        Self::Title,
        Self::Frontend,
        Self::Phase3,
        Self::Gameplay,
        Self::Phase5,
        Self::Phase6,
        Self::Phase7,
        Self::Phase8,
        Self::Phase9,
        Self::Phase10,
        Self::Phase11,
        Self::Phase12,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("unsupported session phase {0}; retail factories cover 0 through 12")]
pub struct InvalidSessionPhase(pub u32);

impl TryFrom<u32> for SessionPhase {
    type Error = InvalidSessionPhase;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::ALL
            .get(value as usize)
            .copied()
            .ok_or(InvalidSessionPhase(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TickCallback {
    /// LAB_004A4AF0: an actual empty retail callback.
    Idle,
    /// 004388D0: world update, followed by gameplay elapsed-time accounting.
    Gameplay,
    /// 00438950: player/selected-object work followed by 00487B90.
    Player,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ScreenFactory {
    Startup,
    Title,
    Frontend,
    Phase3,
    Gameplay,
    Phase5,
    Phase7,
    Phase8,
    TextEntry,
    Phase12,
}

/// Virtual `+18` returns a screen class, not the owning session phase. In
/// particular, phase 8 returns class 4 and phases 4/6 share class 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ScreenKind {
    Startup = 0,
    Title = 1,
    Frontend = 2,
    Phase3 = 3,
    Phase8 = 4,
    Gameplay = 5,
    Phase5 = 6,
    Phase7 = 7,
    TextEntry = 8,
    Phase12 = 9,
}

impl ScreenKind {
    pub const fn for_factory(factory: ScreenFactory) -> Self {
        match factory {
            ScreenFactory::Startup => Self::Startup,
            ScreenFactory::Title => Self::Title,
            ScreenFactory::Frontend => Self::Frontend,
            ScreenFactory::Phase3 => Self::Phase3,
            ScreenFactory::Gameplay => Self::Gameplay,
            ScreenFactory::Phase5 => Self::Phase5,
            ScreenFactory::Phase7 => Self::Phase7,
            ScreenFactory::Phase8 => Self::Phase8,
            ScreenFactory::TextEntry => Self::TextEntry,
            ScreenFactory::Phase12 => Self::Phase12,
        }
    }

    pub const fn id(self) -> u8 {
        self as u8
    }
}

impl ScreenFactory {
    pub const fn for_phase(phase: SessionPhase) -> Self {
        match phase {
            SessionPhase::Startup => Self::Startup,
            SessionPhase::Title => Self::Title,
            SessionPhase::Frontend => Self::Frontend,
            SessionPhase::Phase3 => Self::Phase3,
            SessionPhase::Gameplay | SessionPhase::Phase6 => Self::Gameplay,
            SessionPhase::Phase5 => Self::Phase5,
            SessionPhase::Phase7 => Self::Phase7,
            SessionPhase::Phase8 => Self::Phase8,
            SessionPhase::Phase9 | SessionPhase::Phase10 | SessionPhase::Phase11 => Self::TextEntry,
            SessionPhase::Phase12 => Self::Phase12,
        }
    }

    pub const fn retail_address(self) -> u32 {
        match self {
            Self::Startup => 0x00433810,
            Self::Title => 0x00404a10,
            Self::Frontend => 0x00483430,
            Self::Phase3 => 0x0040a2e0,
            Self::Gameplay => 0x00440c60,
            Self::Phase5 => 0x00440d20,
            Self::Phase7 => 0x00450fc0,
            Self::Phase8 => 0x0042a970,
            Self::TextEntry => 0x00459000,
            Self::Phase12 => 0x00444ac0,
        }
    }
}

/// Native side-effect boundaries, kept separate until their owners are ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionEffect {
    LeaveWorld,                           // 004871C0
    SelectFrontendMusic,                  // 00487360
    PrepareFromFrontend,                  // 00487380
    PreparePhase9,                        // 00487410
    LoadWorld,                            // 004874D0
    SelectWorldMusic,                     // 00487250
    ResumeWorldMusic,                     // 004872A0
    PrepareResults,                       // 00487540
    SetPlayerTimer { microseconds: i32 }, // first world object +F4
    ResetInputBaselines,                  // 004F0340
}

/// The original counters at session +658 and +248. The comparison is strictly
/// greater than one second; exactly 1,000,000 remains in the remainder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GameplayTime {
    pub seconds: u32,
    pub remainder_micros: i32,
}

impl GameplayTime {
    pub fn advance(&mut self, delta_micros: i32) {
        self.remainder_micros = self.remainder_micros.wrapping_add(delta_micros);
        while self.remainder_micros > 1_000_000 {
            self.seconds = self.seconds.wrapping_add(1);
            self.remainder_micros -= 1_000_000;
        }
    }
}

/// Callback-visible state. Requests made while transitioning obey the same
/// first-request-wins rule as outside a callback; fields cannot be rewritten by
/// a host accidentally. The phase-9 redirect is internal to the dispatcher.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionState {
    current: SessionPhase,
    requested: SessionPhase,
    pending: bool,
    tick_callback: TickCallback,
    gameplay_time: GameplayTime,
    last_delta_micros: i32,
    suspended: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            current: SessionPhase::Title,
            requested: SessionPhase::Title,
            pending: false,
            tick_callback: TickCallback::Idle,
            gameplay_time: GameplayTime::default(),
            last_delta_micros: 0,
            suspended: false,
        }
    }
}

impl SessionState {
    pub fn current_phase(&self) -> SessionPhase {
        self.current
    }
    pub fn requested_phase(&self) -> SessionPhase {
        self.requested
    }
    pub fn transition_pending(&self) -> bool {
        self.pending
    }
    pub fn tick_callback(&self) -> TickCallback {
        self.tick_callback
    }
    pub fn gameplay_time(&self) -> GameplayTime {
        self.gameplay_time
    }
    /// Mission W+34 aliases session+248: G starts at session+64 and W at
    /// G+1B0. Restoring/resetting the mission changes seconds only; the
    /// scheduler remainder at session+658 survives mission loads.
    pub(crate) fn restore_mission_elapsed_seconds(&mut self, seconds: u32) {
        self.gameplay_time.seconds = seconds;
    }
    pub fn last_delta_micros(&self) -> i32 {
        self.last_delta_micros
    }
    /// Session +60 is a consumer flag, not a scheduler clock gate.
    pub fn suspended(&self) -> bool {
        self.suspended
    }

    /// A mid-transition state for screen factory tests: `current` is the
    /// phase being left and `requested` the pending one.
    #[cfg(test)]
    pub(crate) fn pending_for_tests(current: SessionPhase, requested: SessionPhase) -> Self {
        Self {
            current,
            requested,
            pending: true,
            ..Self::default()
        }
    }

    /// 00438380 accepts even a request for the current phase when not pending.
    pub fn request_phase(&mut self, phase: SessionPhase) -> bool {
        if self.pending {
            return false;
        }
        self.requested = phase;
        self.pending = true;
        true
    }
}

/// Host custody is explicit: retain the captured screen handle across kind and
/// reuse queries, and preserve its opaque +E8 constructor context across deletion.
/// Screen context is not interpreted as an OVL, phase, or portable pointer.
pub trait SessionHost {
    type Screen: Copy;
    type ScreenContext;
    type Error;

    fn current_screen(&self) -> Option<Self::Screen>;
    /// Virtual `+18` is a class query; `004383B0` discards its result. Concrete
    /// implementations return constants and do not perform screen teardown.
    fn screen_kind(
        &mut self,
        screen: Self::Screen,
        state: &mut SessionState,
    ) -> Result<ScreenKind, Self::Error>;
    fn screen_context(&self, screen: Self::Screen) -> Self::ScreenContext;
    fn reuse_screen(
        &mut self,
        screen: Self::Screen,
        requested: SessionPhase,
        state: &mut SessionState,
    ) -> Result<bool, Self::Error>;
    fn destroy_screen(
        &mut self,
        screen: Self::Screen,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
    /// Read DAT_0055F448 at the point where the native branch consumes it.
    /// 00493300 sets it after network-worker creation; 00493650 clears it.
    fn network_session_active(&self) -> bool;
    fn transition_effect(
        &mut self,
        effect: TransitionEffect,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
    fn create_screen(
        &mut self,
        factory: ScreenFactory,
        context: Self::ScreenContext,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
    /// The host must reject screen attachment/replacement during this call:
    /// 004946F0 checks the session busy flag. Registry enforcement is host-owned.
    fn tick(
        &mut self,
        callback: TickCallback,
        delta_micros: i32,
        elapsed_micros: i32,
        state: &mut SessionState,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum SessionError<E> {
    #[error("session has no current screen; startup must construct its screen before transitions")]
    MissingScreen,
    #[error("application startup requires an empty screen collection")]
    ScreenAlreadyPresent,
    #[error("session stopped after a callback failure; partial callbacks cannot be replayed")]
    Stopped,
    #[error("session callback failed: {0}")]
    Host(E),
    #[error("direct frontend entry requires the initial title state without a pending request")]
    DirectEntry,
    #[error("linked presentation must follow exactly one linked update in the same iteration")]
    LinkedPassOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PhaseTransition {
    pub previous: SessionPhase,
    pub requested: SessionPhase,
    pub effective: SessionPhase,
    pub reuse_accepted: bool,
    pub created: Option<ScreenFactory>,
}

#[derive(Debug, Default)]
pub struct SessionController {
    state: SessionState,
    stopped: bool,
}

impl SessionController {
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn request_phase(&mut self, phase: SessionPhase) -> bool {
        !self.stopped && self.state.request_phase(phase)
    }

    /// Application startup (004389C0) calls factory 0 specially; current and
    /// requested remain 1. A normal transition requires an existing screen.
    /// Its context is the scheduler-context object 004382F0 builds (vtable
    /// 005221A8, registered through 00494630).
    pub fn create_startup_screen<H: SessionHost>(
        &mut self,
        host: &mut H,
        context: H::ScreenContext,
    ) -> Result<(), SessionError<H::Error>> {
        self.ensure_running()?;
        if host.current_screen().is_some() {
            return Err(SessionError::ScreenAlreadyPresent);
        }
        let result = host.create_screen(ScreenFactory::Startup, context, &mut self.state);
        self.finish_callback(result)
    }

    pub fn transition<H: SessionHost>(
        &mut self,
        host: &mut H,
    ) -> Result<Option<PhaseTransition>, SessionError<H::Error>> {
        self.ensure_running()?;
        if !self.state.pending {
            return Ok(None);
        }
        let Some(screen) = host.current_screen() else {
            self.stopped = true;
            return Err(SessionError::MissingScreen);
        };
        let result = self.perform_transition(host, screen);
        self.finish_callback(result).map(Some)
    }

    fn perform_transition<H: SessionHost>(
        &mut self,
        host: &mut H,
        screen: H::Screen,
    ) -> Result<PhaseTransition, H::Error> {
        let previous = self.state.current;
        let requested = self.state.requested;
        let _kind = host.screen_kind(screen, &mut self.state)?;
        // 004383B0 captures +E8 AFTER the kind query but BEFORE reuse/deletion.
        let context = host.screen_context(screen);
        let reuse_accepted = host.reuse_screen(screen, requested, &mut self.state)?;
        if !reuse_accepted {
            // 00438110: the screen's +0 with 1, its deleting destructor.
            host.destroy_screen(screen, &mut self.state)?;
        }
        self.enter_requested_phase(host, previous, reuse_accepted, context)
    }

    /// Port launch standing in for the unported startup/title screens
    /// (factories 0/1). Runs the 004383B0 body for previous Title ->
    /// Frontend after the title screen's own +18/+1C/delete steps: frontend
    /// music 00487360, the frontend factory, then 004F0340, publishing phase 2
    /// with the idle callback. Retail reaches this through the title screen's
    /// request; no title screen or its effects are implied.
    pub fn enter_frontend_directly<H: SessionHost>(
        &mut self,
        host: &mut H,
        context: H::ScreenContext,
    ) -> Result<PhaseTransition, SessionError<H::Error>> {
        self.ensure_running()?;
        if self.state.pending || self.state.current != SessionPhase::Title {
            return Err(SessionError::DirectEntry);
        }
        if host.current_screen().is_some() {
            return Err(SessionError::ScreenAlreadyPresent);
        }
        self.state.requested = SessionPhase::Frontend;
        self.state.pending = true;
        let result = self.enter_requested_phase(host, SessionPhase::Title, false, context);
        self.finish_callback(result)
    }

    fn enter_requested_phase<H: SessionHost>(
        &mut self,
        host: &mut H,
        previous: SessionPhase,
        reuse_accepted: bool,
        context: H::ScreenContext,
    ) -> Result<PhaseTransition, H::Error> {
        let requested = self.state.requested;
        use SessionPhase as P;
        use TickCallback as T;
        use TransitionEffect as E;
        match requested {
            P::Startup => {} // No native callback-selection case.
            P::Title => self.state.tick_callback = T::Idle,
            P::Frontend => {
                self.state.tick_callback = T::Idle;
                if !matches!(previous, P::Title | P::Phase12) {
                    host.transition_effect(E::LeaveWorld, &mut self.state)?;
                }
                host.transition_effect(E::SelectFrontendMusic, &mut self.state)?;
            }
            P::Phase3 => {
                if previous == P::Frontend {
                    host.transition_effect(E::PrepareFromFrontend, &mut self.state)?;
                }
                // Only the port's Teleport to world loads from gameplay: the
                // world it leaves ends first, as 00487540 ends one before its
                // results, while its terrain is still the world's (a furniture
                // marker's cleanup 00435360 looks there for its fence).
                if previous == P::Gameplay {
                    host.transition_effect(E::PrepareResults, &mut self.state)?;
                }
                host.transition_effect(E::LoadWorld, &mut self.state)?;
                self.state.tick_callback = T::Idle;
            }
            P::Gameplay => {
                match previous {
                    P::Phase3 | P::Phase8 => {
                        host.transition_effect(E::SelectWorldMusic, &mut self.state)?
                    }
                    P::Phase5 | P::Phase6 => {
                        host.transition_effect(E::ResumeWorldMusic, &mut self.state)?
                    }
                    _ => {}
                }
                host.transition_effect(
                    E::SetPlayerTimer {
                        microseconds: 500_000,
                    },
                    &mut self.state,
                )?;
                self.state.tick_callback = T::Gameplay;
            }
            P::Phase5 | P::Phase6 | P::Phase7 => {
                if !host.network_session_active() {
                    self.state.tick_callback = T::Player;
                }
            }
            P::Phase8 => {
                host.transition_effect(E::PrepareResults, &mut self.state)?;
                self.state.tick_callback = T::Idle;
            }
            P::Phase9 => {
                if previous == P::Frontend {
                    host.transition_effect(E::PreparePhase9, &mut self.state)?;
                }
                self.state.tick_callback = T::Idle;
                if host.network_session_active() {
                    self.state.requested = P::Phase3;
                    host.transition_effect(E::LoadWorld, &mut self.state)?;
                }
            }
            P::Phase10 | P::Phase11 => self.state.tick_callback = T::Player,
            P::Phase12 => self.state.tick_callback = T::Idle,
        }
        let effective = self.state.requested;
        let created = if host.current_screen().is_none() {
            let factory = ScreenFactory::for_phase(effective);
            host.create_screen(factory, context, &mut self.state)?;
            Some(factory)
        } else {
            None
        };
        host.transition_effect(E::ResetInputBaselines, &mut self.state)?;
        self.state.current = effective;
        self.state.pending = false;
        Ok(PhaseTransition {
            previous,
            requested,
            effective,
            reuse_accepted,
            created,
        })
    }

    /// The scheduler calls this after transition with capped and uncapped
    /// elapsed microseconds. A tick may enqueue the next transition.
    pub fn tick<H: SessionHost>(
        &mut self,
        host: &mut H,
        delta_micros: i32,
        elapsed_micros: i32,
    ) -> Result<(), SessionError<H::Error>> {
        self.ensure_running()?;
        self.state.last_delta_micros = delta_micros; // DAT_00533348
        let callback = self.state.tick_callback;
        if callback == TickCallback::Idle {
            return Ok(());
        }
        let result = host.tick(callback, delta_micros, elapsed_micros, &mut self.state);
        self.finish_callback(result)?;
        if callback == TickCallback::Gameplay {
            self.state.gameplay_time.advance(delta_micros);
        }
        Ok(())
    }

    fn ensure_running<E>(&self) -> Result<(), SessionError<E>> {
        if self.stopped {
            Err(SessionError::Stopped)
        } else {
            Ok(())
        }
    }

    fn finish_callback<T, E>(&mut self, result: Result<T, E>) -> Result<T, SessionError<E>> {
        result.map_err(|error| {
            self.stopped = true;
            SessionError::Host(error)
        })
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[path = "session_driver.rs"]
mod driver;
pub use driver::{ScheduledSession, SessionDriver, SessionFrameHost, SessionTransitionAdmission};
