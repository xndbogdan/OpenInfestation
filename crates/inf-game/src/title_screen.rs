//! Phase-1 title screen: factory 00404A10, constructor 00404620,
//! vtable 0051F600, destructor 00404760/00404780.
//!
//! Native lifecycle, recovered from the supported retail `Infest.exe` and
//! bounded TTD replay (see the title-handoff section of
//! [frontend runtime](../../../docs/re/FRONTEND_RUNTIME.md)):
//!
//! - Constructor `00404620` builds the `0xF4`-byte screen over base `00437F90`
//!   (scheduler member, input context, analog references `+CC/+D0`), zeroes
//!   `+EC/+F0`, publishes vtable `0051F600`, then enumerates devices
//!   `00439B30` over its inline twelve-slot digital/analog arrays (slot 0
//!   `0051F300`, slots 6/7 `0051F378`, slots 8/9 `0051F4A8`, every analog
//!   cell zero), attaches slots `00439D00` and selects the context `004947E0`.
//! - Factory `00404A10` allocates and constructs, then consults process-global
//!   `005468D8`: when set it finishes through `004049C0` immediately (request
//!   phase 2, set `+EC`); otherwise it opens `Title.avi` through `00404890`
//!   and sets the global. A failed open leaves `+F0` clear and makes no phase
//!   request; the tick then finishes on its first run.
//! - Tick `00404810` returns while the video handle `+F0` is present and its
//!   playback status `00494DB0` (handle `+18`) is not 2; otherwise it finishes
//!   through `004049C0`.
//! - Finish `004049C0` closes a present handle through `00494DC0`, restores
//!   the display setting through `00437DF0` from settings `+0C` (`00463AB0`),
//!   requests phase 2 through `00438380` (first-request-wins while a
//!   transition is pending) and sets `+EC` to 1.
//! - Draw `004047F0` runs the arena/batch preamble `00439270` on its draw
//!   argument, then presents one video frame through `00494F80` when a handle
//!   is present. Presentation `00404840` is an empty `ret 4`.
//! - Digital press callback `00404870`: on a press (`EDX != 0`) while `+EC`
//!   is clear, finish through `004049C0`; releases and post-finish presses
//!   return. Held-bit dispatch runs base `00439470` over empty `00401310`
//!   virtuals, so it is a no-op for this screen.
//! - Class query `00404740` returns 1; reuse `00404750` accepts only requested
//!   phase 1. Teardown `00404780` closes a present handle, releases binding
//!   sets `00439D50` and destroys the base `00438080`.
//!
//! The DirectShow video backend (`00494AE0` graph construction for
//! `Title.avi`, `00494F80` frame presentation, `00494DC0` close) has no
//! portable owner yet: `TitleVideoHost` reports the video absent, so the
//! ported factory takes the failed-open path and the first tick finishes to
//! phase 2. IV50 decoding and presentation stay explicit boundaries. The
//! linked poll/tick/draw/presentation callbacks are separate session owners.

use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::{SessionPhase, SessionState};

/// Vtable `0051F600` slot `+18`: the title class ID, not the session phase.
pub const TITLE_CLASS_ID: u8 = 1;

/// Digital press callback `00404870`: on a press (`EDX != 0`) while `+EC` is
/// clear, finish through `004049C0` (close, display restore, phase-2 request,
/// `+EC = 1`); releases and post-finish presses return without effect. The
/// authored tables bind it to Esc/Space/Return plus gamepad buttons; Alt+Esc
/// (`00439260`) and Alt+Enter (`00437D30`) keep the shared system descriptors.
pub const TITLE_SKIP_CALLBACK: u32 = 0x0040_4870;

/// Opaque callback identity of the portable title screen, distinct from the
/// frontend and text-entry contexts. It is never a native address.
pub const TITLE_CALLBACK_CONTEXT: u32 = 3;

/// `00404750` reuse rule: the title screen serves only requested phase 1.
pub fn title_reuses_phase(requested: SessionPhase) -> bool {
    requested == SessionPhase::Title
}

/// Process-global `005468D8`: once set, every later title factory finishes
/// immediately instead of opening the video again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TitleProcessState {
    pub title_seen: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TitleScreenConfig {
    pub attachment: ScreenAttachment,
    /// Logical callback identity, checked against dispatched input contexts.
    pub callback_context: u32,
}

/// The registry-bound title owner: base-screen membership, authored input
/// bindings and the `+EC/+F0` finish/video state. Construction is not
/// transactional: completed acquisitions survive errors for explicit
/// teardown, as with the frontend and text-entry screens.
#[derive(Debug)]
pub struct TitleScreen {
    config: TitleScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    finished: bool,
    video_present: bool,
}

impl TitleScreen {
    /// Metadata only; `initialize` performs the ordered native acquisitions.
    pub fn new(config: TitleScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            finished: false,
            video_present: false,
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

    /// Screen `+EC` as left by `004049C0` (1 once finished).
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Screen `+F0`: whether a video handle is currently held.
    pub fn video_present(&self) -> bool {
        self.video_present
    }

    /// Constructor `00404620` plus factory `00404A10` video selection, in
    /// native order: scheduler membership, base input, link, device
    /// enumeration, authored title tables, context selection, then the
    /// process-global video open-or-finish. Only requested phase 1 reaches
    /// retail construction.
    ///
    /// The skip-path finish requests phase 2 through `00438380`, which drops
    /// the request while the creating transition is still pending; the host
    /// adapter preserves that first-wins rule instead of failing the factory.
    pub fn initialize<D, H: TitleHost>(
        &mut self,
        services: &mut TitleFactoryServices<'_, D>,
        host: &mut H,
        state: &mut SessionState,
    ) -> Result<(), TitleScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if state.requested_phase() != SessionPhase::Title {
            return Err(TitleScreenError::RequestedPhase(state.requested_phase()));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(TitleScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.title_input_tables(),
            &devices,
        )?;
        let context = self.bindings.context().expect("installed title context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => TitleScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => TitleScreenError::Input(error),
            })?;
        let mut session_host = TitleSessionHost { video: host, state };
        self.factory(services.process, &mut session_host)?;
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Teardown `00404780`: close a present handle, release binding sets
    /// `00439D50`, then base `00438080` unlinks membership, releases
    /// `+D0/+CC` and the context, and removes the scheduler member. Unlike
    /// `finish`, this makes no phase request and writes no `+EC`. Also usable
    /// for retained partial acquisitions as explicit containment.
    pub fn destroy<D, H: TitleVideoHost>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
        video: &mut H,
    ) -> Result<(), TitleScreenError<H::Error>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        if self.video_present {
            video.close_title().map_err(TitleScreenError::Host)?;
            self.video_present = false;
        }
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

    /// Factory `00404A10` after construction: when the process global is set,
    /// finish immediately; otherwise open the video and set the global.
    ///
    /// A failed open (`None`) leaves the handle clear and makes no phase
    /// request, matching `00404890` returning with `+F0 == 0`. A host open
    /// error is reported without fabricating a phase request.
    pub fn factory<H: TitleEffects>(
        &mut self,
        process: &mut TitleProcessState,
        host: &mut H,
    ) -> Result<(), TitleScreenError<H::Error>> {
        if process.title_seen {
            self.finish(host)?;
            return Ok(());
        }
        if host.open_title().map_err(TitleScreenError::Host)?.is_some() {
            self.video_present = true;
            process.title_seen = true;
        }
        Ok(())
    }

    /// Tick `00404810`: while a handle is present and its status is not 2,
    /// do nothing; otherwise finish.
    pub fn tick<H: TitleEffects>(
        &mut self,
        host: &mut H,
    ) -> Result<TitleTickOutcome, TitleScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if self.video_present {
            let status = host.playback_status().map_err(TitleScreenError::Host)?;
            if status != PLAYBACK_FINISHED {
                return Ok(TitleTickOutcome::Playing);
            }
        }
        self.finish(host)?;
        Ok(TitleTickOutcome::Finished)
    }

    /// Finish `004049C0`: close a present handle, restore the display
    /// setting, request phase 2 and set `+EC`.
    pub fn finish<H: TitleEffects>(
        &mut self,
        host: &mut H,
    ) -> Result<(), TitleScreenError<H::Error>> {
        if self.video_present {
            host.close_title().map_err(TitleScreenError::Host)?;
            self.video_present = false;
        }
        host.restore_display().map_err(TitleScreenError::Host)?;
        host.request_phase(SessionPhase::Title, SessionPhase::Frontend)
            .map_err(TitleScreenError::Host)?;
        self.finished = true;
        Ok(())
    }

    /// Draw `004047F0`: the arena/batch preamble always runs; one video
    /// frame is presented only when a handle is present.
    pub fn draw<H: TitleEffects>(
        &self,
        host: &mut H,
    ) -> Result<TitleDrawOutcome, TitleScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        host.draw_preamble().map_err(TitleScreenError::Host)?;
        if self.video_present {
            host.present_frame().map_err(TitleScreenError::Host)?;
            Ok(TitleDrawOutcome::Presented)
        } else {
            Ok(TitleDrawOutcome::NoVideo)
        }
    }

    /// Direct callbacks queued for this screen's context by the input poll:
    /// the skip callback `00404870` finishes on presses while `+EC` is clear.
    /// Releases, post-finish presses and system callbacks never reach this
    /// owner; unknown addresses fail before mutation.
    pub fn digital_callback<H: TitleEffects>(
        &mut self,
        event: InputCallback,
        effects: &mut H,
    ) -> Result<(), TitleScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if event.context != self.config.callback_context {
            return Err(TitleScreenError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        if event.address != TITLE_SKIP_CALLBACK {
            return Err(TitleScreenError::UnknownCallback(event.address));
        }
        if event.value == 0 || self.finished {
            return Ok(());
        }
        self.finish(effects)
    }

    fn require_lifecycle<E>(&self, expected: ScreenLifecycle) -> Result<(), TitleScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(TitleScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

/// `00494DB0` value that ends the tick wait: playback status 2.
pub const PLAYBACK_FINISHED: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleTickOutcome {
    /// A handle is present and its status is not 2: `00404810` returns.
    Playing,
    /// `004049C0` ran: the handle was absent or its status reached 2.
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleDrawOutcome {
    /// `00439270` ran and `00494F80` presented one frame.
    Presented,
    /// `00439270` ran with no handle to present.
    NoVideo,
}

#[derive(Debug, Error)]
pub enum TitleScreenError<E> {
    #[error("title screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("00404A10 serves requested phase 1 (Title), not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error("title callback context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("title input callback {0:#010x} is not the skip callback")]
    UnknownCallback(u32),
    #[error("required title effect failed")]
    Host(E),
}

pub struct TitleFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    pub process: &'a mut TitleProcessState,
}

/// Required title effects for the pure state machine. Every method names its
/// native owner; none has a successful default. `request_phase` routes to
/// `00438380` and preserves its first-request-wins rule: a rejection while a
/// transition is pending is reported, never silently treated as a completed
/// transition. Session adapters that run inside the creating transition drop
/// the rejection instead, matching the native discarded result.
pub trait TitleEffects {
    type Error;
    /// `00404890`: construct the DirectShow graph for `Title.avi`. `Some`
    /// stores a handle at `+F0`; `None` is the failed open that leaves `+F0`
    /// clear without a phase request.
    fn open_title(&mut self) -> Result<Option<()>, Self::Error>;
    /// `00494DB0`: the playback status at handle `+18`.
    fn playback_status(&mut self) -> Result<u32, Self::Error>;
    /// `00494F80`: present one video frame; called only with a handle held.
    fn present_frame(&mut self) -> Result<(), Self::Error>;
    /// `00494DC0`: close the held handle and clear `+F0`.
    fn close_title(&mut self) -> Result<(), Self::Error>;
    /// `00439270`: replace the command arena, reset the batch and publish
    /// the renderer. Runs on every draw, with or without video.
    fn draw_preamble(&mut self) -> Result<(), Self::Error>;
    /// `00463AB0` then `00437DF0` with settings `+0C`: restore the display.
    fn restore_display(&mut self) -> Result<(), Self::Error>;
    /// `00438380` from `004049C0` for phase 2. Arguments name the retail
    /// previous/requested pair for audit; the host preserves first-wins.
    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error>;
}

/// Platform video backend behind the title state machine. The port has no
/// IV50 decoder: the production implementation reports the video absent, so
/// the factory takes the failed-open path and the first tick finishes.
pub trait TitleVideoHost {
    type Error;
    fn open_title(&mut self) -> Result<Option<()>, Self::Error>;
    fn playback_status(&mut self) -> Result<u32, Self::Error>;
    fn present_frame(&mut self) -> Result<(), Self::Error>;
    fn close_title(&mut self) -> Result<(), Self::Error>;
    fn restore_display(&mut self) -> Result<(), Self::Error>;
    fn draw_preamble(&mut self) -> Result<(), Self::Error>;
}

/// Platform title host: device enumeration plus the video backend, behind
/// one error type. Used only on the factory/teardown path, where no input
/// poll borrows compete for the keyboard adapter.
pub trait TitleHost: TitleVideoHost {
    /// `00439B30`'s eligible, calibrated devices in native order. The adapter
    /// cache entry is process-lifetime: the title shares the keyboard the
    /// first screen factory attached.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

/// Session adapter over a platform video backend: video and display effects
/// run on the platform, while the phase-2 request reaches `00438380` through
/// the session state. A rejection while a transition is pending is dropped,
/// matching the native factory's discarded result.
pub struct TitleSessionHost<'a, H> {
    pub video: &'a mut H,
    pub state: &'a mut SessionState,
}

impl<H: TitleVideoHost> TitleEffects for TitleSessionHost<'_, H> {
    type Error = H::Error;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        self.video.open_title()
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        self.video.playback_status()
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.video.present_frame()
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.video.close_title()
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        self.video.draw_preamble()
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        self.video.restore_display()
    }
    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        let _ = previous;
        let _ = self.state.request_phase(requested);
        Ok(())
    }
}

#[cfg(test)]
#[path = "title_screen_tests.rs"]
mod tests;
