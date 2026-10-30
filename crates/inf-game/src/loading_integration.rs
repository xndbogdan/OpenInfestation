//! Consumed ordinary-world loading loans across both 0048DC30 activations.
//!
//! WholeFileReadCadenceApproximation groups one physical OVL into one native
//! adaptive read request. TransactionalResidencyViewApproximation supplies the
//! actual committed table during staged reads, then the newly committed table
//! during the next activation. Native per-handler reads/partial publication
//! remain separate owners; neither approximation admits attract handshakes.

use std::cell::Cell;

use crate::{
    frontend_renderer::FrontendRenderContext,
    input::InputContextId,
    loading_read::{ResourceReadCallback, ResourceReadState},
    loading_runtime::{
        loading_callback_40edb0, LoadingCallbackHost, LoadingCallbackOrigin,
        LoadingCallbackRequest, LoadingProcessState, LoadingTickRequest,
    },
    residency::{ResidencyApplyRequest, ResidencyReads, ResidentResourceReadHost},
    screen_registry::{ContextOwnerId, ScreenRegistry},
    ResidencyRequest, ResidentRuntime,
};

/// Disjoint actual platform fields; no second loading mode or RNG snapshot.
pub struct LoadingPlatformLoans<'a> {
    pub mode: &'a Cell<u8>,
    pub rng: &'a mut u32,
    pub voices: crate::process_sound::ProcessSoundTeardownLoan<'a>,
}

/// Fresh session+648 lookup is reached only inside the callback render guard.
#[derive(Clone, Copy)]
pub struct LoadingContextSource<'a> {
    pub registry: &'a ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub owner: ContextOwnerId,
}

pub struct LoadingDrawInputs<'a> {
    pub tick: LoadingTickRequest<'a>,
    pub residency: &'a ResidentRuntime,
    pub context: &'a FrontendRenderContext,
    pub current_attract: &'a bool,
    pub current_mode: &'a Cell<u8>,
}

/// The same app clock and renderer lent synchronously to a world load.
/// These are required effects; passive tooling uses an explicit other policy.
pub trait LoadingPresentationHost {
    fn sample_clock(&mut self) -> u64;
    fn renderer_available(&mut self) -> Result<bool, String>;
    fn tick_loading(&mut self, request: LoadingDrawInputs<'_>) -> Result<(), String>;
    fn present_loading(&mut self, residency: &ResidentRuntime) -> Result<(), String>;
    fn dispose_loading_error(&mut self, error: String);
}

pub struct WorldLoadingInputs<'a> {
    pub reads: &'a mut ResourceReadState,
    pub process: &'a mut LoadingProcessState,
    pub effects: &'a mut [u8; 16],
    pub context: LoadingContextSource<'a>,
    pub current_attract: &'a bool,
    pub current_mode: &'a Cell<u8>,
    pub presentation: &'a mut dyn LoadingPresentationHost,
}

pub enum WorldLoadReads<'a> {
    /// Passive preparation/session fixtures have no app clock/renderer loan.
    NoCallbacks,
    Interactive(WorldLoadingInputs<'a>),
}

pub(crate) fn activate_world_resources(
    residency: &mut ResidentRuntime,
    selected_leaf: u8,
    rng: &mut u32,
    reads: &mut WorldLoadReads<'_>,
) -> crate::Result<()> {
    match reads {
        WorldLoadReads::NoCallbacks => {
            residency.apply(ResidencyRequest::Activate { level: 48 })?;
            residency.apply(ResidencyRequest::Activate {
                level: selected_leaf,
            })?;
        }
        WorldLoadReads::Interactive(loading) => {
            // 0048DC99 precedes installation/reads; ordinary mode does not
            // discharge this counter. No per-load clock/effect/RNG reset.
            loading.process.counter_546920 = 1;
            loading
                .reads
                .exchange_callback(ResourceReadCallback::LoadingCallback);
            for level in [48, selected_leaf] {
                // Borrow the retained read state separately from the same
                // callback/process loans so the callback can exchange it.
                let state = &mut *loading.reads;
                let mut callback = WorldReadCallback {
                    process: &mut *loading.process,
                    effects: &mut *loading.effects,
                    context: loading.context,
                    current_attract: loading.current_attract,
                    current_mode: loading.current_mode,
                    presentation: &mut *loading.presentation,
                    rng: &mut *rng,
                };
                residency.apply(ResidencyApplyRequest {
                    operation: ResidencyRequest::Activate { level },
                    reads: ResidencyReads::Loading {
                        state,
                        host: &mut callback,
                    },
                })?;
            }
            // 0048DD11 exchanges literal NULL, not the previous pointer.
            // An earlier error retains the installed callback and prefix;
            // native exceptional removal has not been established.
            loading.reads.exchange_callback(ResourceReadCallback::None);
        }
    }
    Ok(())
}

struct WorldReadCallback<'a> {
    process: &'a mut LoadingProcessState,
    effects: &'a mut [u8; 16],
    context: LoadingContextSource<'a>,
    current_attract: &'a bool,
    current_mode: &'a Cell<u8>,
    presentation: &'a mut dyn LoadingPresentationHost,
    rng: &'a mut u32,
}

impl ResidentResourceReadHost for WorldReadCallback<'_> {
    fn sample_clock(&mut self) -> u64 {
        self.presentation.sample_clock()
    }

    fn invoke_loading_callback(
        &mut self,
        residency: &ResidentRuntime,
        _reads: &mut ResourceReadState,
        raw_progress: u32,
    ) -> u32 {
        let mut host = CallbackPresentation {
            residency,
            context: self.context,
            current_attract: self.current_attract,
            current_mode: self.current_mode,
            presentation: &mut *self.presentation,
        };
        loading_callback_40edb0(
            LoadingCallbackRequest {
                state: &mut *self.process,
                effect_bytes: &mut *self.effects,
                rng: &mut *self.rng,
                raw_progress,
                origin: LoadingCallbackOrigin::ResourceRead,
            },
            &mut host,
        )
    }
}

struct CallbackPresentation<'a> {
    residency: &'a ResidentRuntime,
    context: LoadingContextSource<'a>,
    current_attract: &'a bool,
    current_mode: &'a Cell<u8>,
    presentation: &'a mut dyn LoadingPresentationHost,
}

impl LoadingCallbackHost for CallbackPresentation<'_> {
    type Error = String;
    fn sample_clock(&mut self) -> u64 {
        self.presentation.sample_clock()
    }
    fn renderer_available(&mut self) -> Result<bool, String> {
        self.presentation.renderer_available()
    }
    fn tick_loading(&mut self, tick: LoadingTickRequest<'_>) -> Result<(), String> {
        let context = self
            .context
            .registry
            .backend(self.context.owner)
            .map_err(|error| format!("loading session context: {error}"))?;
        self.presentation.tick_loading(LoadingDrawInputs {
            tick,
            residency: self.residency,
            context,
            current_attract: self.current_attract,
            current_mode: self.current_mode,
        })
    }
    fn present_loading(&mut self) -> Result<(), String> {
        self.presentation.present_loading(self.residency)
    }
    fn dispose_loading_error(&mut self, error: String) {
        self.presentation.dispose_loading_error(error)
    }
}

#[cfg(test)]
mod tests;
