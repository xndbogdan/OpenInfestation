//! Phase-0 startup (language) screen: factory 00433810, constructor 00432500,
//! vtable 00521DA0, destructor 00432720/00432740.
//!
//! Native lifecycle, recovered from the supported retail `Infest.exe` and the
//! bounded Infest01 visit from the Options menu (see the startup section of
//! [frontend runtime](../../../docs/re/FRONTEND_RUNTIME.md)):
//!
//! - Constructor `00432500` builds the `0x1B0`-byte screen over base
//!   `00437F90`, zeroes direction `+194`, angle `+198`, age `+19C`, idle
//!   `+1A0` and byte `+1AC`, copies settings `+64` into the selection `+190`
//!   and the H7 slot `0x20` word into the clear colour `+1A8`. It enumerates
//!   devices `00439B30` over its inline tables, attaches them `00439D00`,
//!   selects the context `004947E0`, saves and stops the music `00487230`
//!   and requests resource leaf 2 `004EE1E0(2)`. A set process byte
//!   `00546B24` starts the age at `00546B20 + 00532890 + 00532894` (the
//!   fades already complete); the byte is then set. Finally a set session
//!   byte `+65C` (settings loaded at startup) requests phase 1 through
//!   `00438380`, which the pending creating transition drops.
//! - Tick `00433640` runs base input `004392B0`, adds the scheduler delta to
//!   the age and, once the age passes `00546B20 + 00532890`, applies a
//!   pending direction: the selection wraps over 0..5 and the angle gains
//!   `(direction << 16) / 6`. The angle then decays to seven eighths. The
//!   idle time grows by the same delta; past `00532898` the screen accepts.
//! - Accept `00433730` acts only after that same age gate. A changed
//!   language is written to settings `+64` before `004EE160` reselects the
//!   resident language with settings `+0C` clamped to 4. Settings persist
//!   (`00463100`) only when session `+65C` is clear. It then requests phase 1.
//!   An exception in either call is caught, deleted, and still requests 1.
//! - Digital callbacks: accept `004327B0` on a press; `004327C0`/`004327E0`
//!   zero the idle time, then on a press call virtual `+30`/`+34`
//!   (`00432700`/`00432710`), which store direction -1/+1. Held-bit
//!   dispatch through `00439470` reaches the same two setters at `+30/+34`;
//!   the other slots are the empty `00401310`.
//! - Class query `00401300` returns 0; reuse `004326F0` accepts only phase 0.
//!   Teardown `00432740` releases the binding sets (`00439D50` at `+130`),
//!   the context object (`004F0D60` at `+EC`) and the base (`00438080`).
//!
//! The carousel draw `004330E0` and presentation `00433620` live in
//! [`crate::startup_draw`].

use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::scheduler::SchedulerTick;
use crate::screen_input::{ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::SessionPhase;
use crate::{ResidencyRequest, ResidentRuntime};

/// Vtable `00521DA0` slot `+18` (`00401300`): the startup class ID.
pub const STARTUP_CLASS_ID: u8 = 0;

/// Opaque callback identity of the portable startup screen, distinct from
/// every other screen context. It is never a native address.
pub const STARTUP_CALLBACK_CONTEXT: u32 = 7;

/// `004327B0`: accept on a press (Return without Alt, Esc, joystick buttons).
pub const STARTUP_ACCEPT_CALLBACK: u32 = 0x0043_27B0;
/// `004327C0`: Left; zeroes the idle time, then turns on a press.
pub const STARTUP_PREVIOUS_CALLBACK: u32 = 0x0043_27C0;
/// `004327E0`: Right; zeroes the idle time, then turns on a press.
pub const STARTUP_NEXT_CALLBACK: u32 = 0x0043_27E0;

/// `00546B20`: the authored delay before anything draws. Initialized data
/// zero; only the startup screen reads it and nothing writes it.
pub const STARTUP_DELAY_MICROS: i32 = 0;
/// `00532890`: the carousel fade-in and the input gate.
pub const STARTUP_FADE_IN_MICROS: i32 = 1_000_000;
/// `00532894`: the background fade that follows the fade-in.
pub const STARTUP_BACKGROUND_FADE_MICROS: i32 = 1_000_000;
/// `00532898`: idle time after which the tick accepts.
pub const STARTUP_IDLE_ACCEPT_MICROS: i32 = 30_000_000;
/// The selection wraps over the six carousel languages.
pub const STARTUP_LANGUAGES: i32 = 6;
/// Resource leaf requested by `00432684`.
pub const STARTUP_RESOURCE_LEAF: u8 = 2;
/// H7 slot read into `+1A8` by the constructor and short draws.
pub const STARTUP_CLEAR_MATERIAL: i32 = 0x20;

/// `004326F0` reuse rule: the startup screen serves only requested phase 0.
pub fn startup_reuses_phase(requested: SessionPhase) -> bool {
    requested == SessionPhase::Startup
}

/// Process byte `00546B24`: once a startup screen was constructed, later
/// constructions start with both fades complete.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupProcessState {
    pub constructed: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct StartupScreenConfig {
    pub attachment: ScreenAttachment,
    /// Logical callback identity, checked against dispatched input contexts.
    pub callback_context: u32,
}

/// The screen words `+190..+1A8`. `+1A4/+1A8` are rewritten by every draw
/// that passes the delay; the presentation clears with `+1A8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupState {
    /// `+190`: the language under the front flag, copied from settings `+64`.
    pub selection: i32,
    /// `+194`: pending turn, -1, 0 or +1.
    pub direction: i32,
    /// `+198`: carousel angle offset, 0x10000 per turn, decaying each tick.
    pub angle: i32,
    /// `+19C`: microseconds since construction, plus the process head start.
    pub age: i32,
    /// `+1A0`: microseconds since the last Left/Right callback.
    pub idle: i32,
    /// `+1A4`: shadow colour word, H7 slot `0x22` faded in by the draw.
    pub shadow_color: u32,
    /// `+1A8`: next-frame clear colour word.
    pub clear_color: u32,
}

impl StartupState {
    /// The age gate shared by the turn, the accept and the full colours.
    pub fn input_ready(&self) -> bool {
        self.age > STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS
    }
}

/// The registry-bound startup owner: base-screen membership, authored input
/// bindings, the base held/repeat input and the `+190..+1A8` words.
/// Construction is not transactional: completed acquisitions survive errors
/// for explicit teardown, as with the other screens.
#[derive(Debug)]
pub struct StartupScreen {
    config: StartupScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    input: ScreenInput,
    state: StartupState,
    /// Port fix: delta not yet given to the carousel's angle decay.
    angle_carry: u32,
}

pub struct StartupFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    pub residency: &'a mut ResidentRuntime,
    pub process: &'a mut StartupProcessState,
}

/// What the constructor reads before its device enumeration. The caller
/// resolves H7 slot `0x20` from the residency current at construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupConstruction {
    /// Settings `+64`, copied raw (the draw and the accept see it unclamped).
    pub settings_language: u32,
    /// The converted H7 slot `0x20` word.
    pub clear_material_word: u32,
    /// Session byte `+65C`.
    pub settings_loaded: bool,
}

/// Required constructor effects; none has a successful default.
pub trait StartupFactoryHost {
    type Error;
    /// `00487230` over the session's `+5D0` music owner: save and stop.
    fn pause_music(&mut self) -> Result<(), Self::Error>;
    /// `00439B30`'s eligible, calibrated devices in native order.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
    /// `00438380(1)`, first-request-wins. Inside the creating transition the
    /// request is dropped; the adapter must not fail the factory for that.
    fn request_title(&mut self);
}

/// Required effects of the tick and the input callbacks.
pub trait StartupEffects {
    type Error: std::fmt::Display;
    /// Settings `+64`, reread at the accept.
    fn settings_language(&self) -> u32;
    /// Session byte `+65C`.
    fn settings_loaded(&self) -> bool;
    /// `00433796..004337AF`: write settings `+64`, then `004EE160` with
    /// settings `+0C` clamped to 4. A failed reselect keeps the written word.
    fn select_language(&mut self, language: u32) -> Result<(), Self::Error>;
    /// `00463100`.
    fn persist_settings(&mut self) -> Result<(), Self::Error>;
    /// `00438380(1)`, first-request-wins.
    fn request_title(&mut self);
    /// The accept's caught exception (`004923C0` deletes it). Reported, never
    /// replayed or turned into success.
    fn report_accept_failure(&mut self, error: Self::Error);
    /// Port fix, not native: the quantum the carousel's angle decay runs in.
    /// None decays it every tick, as retail.
    fn tick_quantum(&self) -> Option<u32> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupAcceptOutcome {
    /// The age gate was not passed: `00433730` returns without effects.
    NotReady,
    /// Phase 1 was requested, after any language change and persistence.
    Accepted,
    /// A caught exception skipped the remaining calls; phase 1 was requested.
    Recovered,
}

impl StartupScreen {
    /// Metadata only; `initialize` performs the ordered native acquisitions.
    pub fn new(config: StartupScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            input: ScreenInput::new(ScreenDispatchPolicy::Base, config.callback_context),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            state: StartupState {
                selection: 0,
                direction: 0,
                angle: 0,
                age: 0,
                idle: 0,
                shadow_color: 0,
                clear_color: 0,
            },
            angle_carry: 0,
        }
    }

    pub fn lifecycle(&self) -> ScreenLifecycle {
        self.lifecycle
    }
    pub fn id(&self) -> Option<ScreenId> {
        self.id
    }
    pub fn bindings(&self) -> &FrontendBindings {
        &self.bindings
    }
    pub fn screen_input(&self) -> &ScreenInput {
        &self.input
    }
    pub fn state(&self) -> &StartupState {
        &self.state
    }
    /// The draw rewrites `+1A4/+1A8`; nothing else may use this.
    pub(crate) fn state_mut(&mut self) -> &mut StartupState {
        &mut self.state
    }

    /// Constructor `00432500` in native order. Application startup also
    /// reaches it with requested phase 1, so no requested phase is checked.
    pub fn initialize<D, H: StartupFactoryHost>(
        &mut self,
        services: &mut StartupFactoryServices<'_, D>,
        construction: StartupConstruction,
        host: &mut H,
    ) -> Result<(), StartupScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        self.state.selection = construction.settings_language as i32;
        self.state.clear_color = construction.clear_material_word;
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(StartupScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.startup_input_tables(),
            &devices,
        )?;
        let context = self.bindings.context().expect("installed startup context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => StartupScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => StartupScreenError::Input(error),
            })?;
        host.pause_music().map_err(StartupScreenError::Host)?;
        services.residency.apply(ResidencyRequest::Activate {
            level: STARTUP_RESOURCE_LEAF,
        })?;
        if services.process.constructed {
            self.state.age =
                STARTUP_BACKGROUND_FADE_MICROS + STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS;
        }
        services.process.constructed = true;
        if construction.settings_loaded {
            host.request_title();
        }
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Teardown `00432740`: release binding sets `00439D50`, then base
    /// `00438080` unlinks membership, releases `+D0/+CC` and the context,
    /// and removes the scheduler member. Residency and music are untouched.
    pub fn destroy<D>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), StartupScreenError<std::convert::Infallible>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        if let Some(id) = self.id {
            if registry.screen(id)?.membership == ScreenMembership::Linked {
                registry.unlink_screen_membership(id)?;
            }
        }
        self.bindings.release_base(input)?;
        if let Some(id) = self.id {
            registry.remove_screen(id)?;
            self.id = None;
        }
        self.lifecycle = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Tick `00433640` after the base input `004392B0`.
    pub fn tick<H: StartupEffects>(
        &mut self,
        input: &InputRuntime,
        tick: SchedulerTick,
        effects: &mut H,
    ) -> Result<StartupAcceptOutcome, StartupScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        let analog = match self.bindings.analog_values(input) {
            Ok(values) => values,
            Err(error) => {
                self.lifecycle = ScreenLifecycle::Stopped;
                return Err(error.into());
            }
        };
        let state = &mut self.state;
        let result = self.input.tick(tick, analog, &mut |_, action| {
            held_action(state, action);
            Ok::<(), std::convert::Infallible>(())
        });
        if let Err(error) = result {
            self.lifecycle = ScreenLifecycle::Stopped;
            return Err(StartupScreenError::ScreenInput(error.to_string()));
        }
        let delta = tick.delta_micros;
        let state = &mut self.state;
        state.age = state.age.wrapping_add(delta);
        if state.input_ready() && state.direction != 0 {
            let direction = state.direction;
            state.selection = state.selection.wrapping_add(direction);
            if state.selection >= STARTUP_LANGUAGES {
                state.selection = 0;
            }
            if state.selection < 0 {
                state.selection = STARTUP_LANGUAGES - 1;
            }
            state.direction = 0;
            state.angle = state
                .angle
                .wrapping_add(direction.wrapping_shl(16) / STARTUP_LANGUAGES);
        }
        // Port fix: retail eases the angle by 7/8 a tick, so the carousel
        // settles faster at a high frame rate; the fix eases it once a whole
        // quantum, carrying the rest.
        let ease = match effects.tick_quantum() {
            Some(quantum) if delta > 0 => {
                let total = self.angle_carry + delta as u32;
                let whole = total - total % quantum;
                self.angle_carry = total - whole;
                whole != 0
            }
            _ => true,
        };
        let state = &mut self.state;
        if ease {
            state.angle = state.angle.wrapping_mul(7) / 8;
        }
        state.idle = state.idle.wrapping_add(delta);
        if state.idle > STARTUP_IDLE_ACCEPT_MICROS {
            return Ok(self.accept(effects));
        }
        Ok(StartupAcceptOutcome::NotReady)
    }

    /// Direct callbacks queued for this screen's context by the input poll.
    /// System callbacks never reach this owner; unknown addresses and
    /// foreign contexts fail before mutation.
    pub fn digital_callback<H: StartupEffects>(
        &mut self,
        event: InputCallback,
        effects: &mut H,
    ) -> Result<StartupAcceptOutcome, StartupScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if event.context != self.config.callback_context {
            return Err(StartupScreenError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        let pressed = event.value != 0;
        match event.address {
            STARTUP_ACCEPT_CALLBACK => {
                if pressed {
                    return Ok(self.accept(effects));
                }
            }
            STARTUP_PREVIOUS_CALLBACK => {
                self.state.idle = 0;
                if pressed {
                    self.state.direction = -1;
                }
            }
            STARTUP_NEXT_CALLBACK => {
                self.state.idle = 0;
                if pressed {
                    self.state.direction = 1;
                }
            }
            address => return Err(StartupScreenError::UnknownCallback(address)),
        }
        Ok(StartupAcceptOutcome::NotReady)
    }

    /// Accept `00433730`. Its setjmp scope catches a failed reselect or
    /// persist; the phase-1 request runs on both paths.
    pub fn accept<H: StartupEffects>(&mut self, effects: &mut H) -> StartupAcceptOutcome {
        if !self.state.input_ready() {
            return StartupAcceptOutcome::NotReady;
        }
        let selection = self.state.selection as u32;
        let result = (|| {
            if effects.settings_language() != selection {
                effects.select_language(selection)?;
            }
            if effects.settings_language() != selection || !effects.settings_loaded() {
                effects.persist_settings()?;
            }
            Ok(())
        })();
        let outcome = match result {
            Ok(()) => StartupAcceptOutcome::Accepted,
            Err(error) => {
                effects.report_accept_failure(error);
                StartupAcceptOutcome::Recovered
            }
        };
        effects.request_title();
        outcome
    }

    fn require_lifecycle<E>(&self, expected: ScreenLifecycle) -> Result<(), StartupScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(StartupScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

/// `00439470` over vtable `00521DA0`: `+30`/`+34` store the direction; the
/// confirm, previous, next and back slots are the empty `00401310`.
fn held_action(state: &mut StartupState, action: ScreenInputAction) {
    match action {
        ScreenInputAction::AdjustPrevious => state.direction = -1,
        ScreenInputAction::AdjustNext => state.direction = 1,
        _ => {}
    }
}

#[derive(Debug, Error)]
pub enum StartupScreenError<E> {
    #[error("startup screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    Residency(#[from] crate::Error),
    #[error("startup base input stopped: {0}")]
    ScreenInput(String),
    #[error("startup callback context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("startup input callback {0:#010x} is not 004327B0/C0/E0")]
    UnknownCallback(u32),
    #[error("required startup effect failed")]
    Host(E),
}

impl<E> From<ScreenInputError<std::convert::Infallible>> for StartupScreenError<E> {
    fn from(error: ScreenInputError<std::convert::Infallible>) -> Self {
        Self::ScreenInput(error.to_string())
    }
}

#[cfg(test)]
#[path = "startup_screen_tests.rs"]
mod tests;
