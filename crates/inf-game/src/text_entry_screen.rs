//! Phase 9/10/11 text-entry screen: factory 00459000, constructor 00459060,
//! vtable 00526028, destructor 00459290/004592B0.
//!
//! Composes the base screen owners (registry membership, input context and
//! analog references, authored bindings, held-bit input) with the name-entry
//! state and the 00445840 key translator. Identity reads, device enumeration,
//! settings/profile effects and phase requests are required host effects; the
//! draw 00459310 and the session host remain separate owners. Construction is
//! not transactional: completed acquisitions survive errors for explicit
//! teardown, as with the frontend screen.

use std::convert::Infallible;

use inf_formats::{KeyNameTable, MissionSaveState, PlayerSaveState};
use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::frontend_screen::FrontendProcessState;
use crate::input::{DeviceId, InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::key_text::{poll_stale_byte, KeyTextError, KeyTextLatch};
use crate::name_entry::{
    IdentityWriteRequest, KeyRead, LoadedIdentity, NameCompletion, NameEntryAction,
    NameEntryActions, NameEntryCompletion, NameEntryError, NameEntryServices, NameEntryState,
    NameEntryTick, NameEntryTickError,
};
use crate::profile_store::ProfileStore;
use crate::scheduler::SchedulerTick;
use crate::screen_input::{ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::SessionPhase;

/// Tab: 0045AE90 -> 0045AC00, confirm while the error byte is clear.
pub const CONFIRM_NAME_CALLBACK: u32 = 0x0045_AE90;
/// R: 0045AEA0 -> 0045AC10, rerun completion while the error byte is set.
pub const RETRY_NAME_CALLBACK: u32 = 0x0045_AEA0;

/// 00459270 reuse callback: the screen serves only requested phases 9..=11.
pub fn text_entry_reuses_phase(requested: SessionPhase) -> bool {
    matches!(
        requested,
        SessionPhase::Phase9 | SessionPhase::Phase10 | SessionPhase::Phase11
    )
}

/// Process globals shared with other owners: latch 0054766C (00445840) and
/// the effect bytes 00546938, which another screen also updates (0040EB1F)
/// and 00459060 never clears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEntryProcessState {
    pub key_latch: KeyTextLatch,
    pub effect_bytes: [u8; 16],
}

#[derive(Clone, Copy, Debug)]
pub struct TextEntryScreenConfig {
    pub attachment: ScreenAttachment,
    /// Logical callback identity for this screen, distinct from the frontend's.
    pub callback_context: u32,
}

#[derive(Debug, Error)]
pub enum TextEntryScreenError<E> {
    #[error("text-entry screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("00459000 serves requested phases 9..=11, not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    State(#[from] NameEntryError),
    #[error(transparent)]
    KeyText(#[from] KeyTextError),
    #[error(transparent)]
    ScreenInput(ScreenInputError<Infallible>),
    /// 00439610(0) reads the process-lifetime adapter cache 005474B0 by
    /// device type; retail faults only if no enumeration ever cached a
    /// type-0 adapter.
    #[error("no native type-0 keyboard adapter has been cached for 00445840")]
    MissingKeyboard,
    #[error("text-entry callback context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("base input dispatched {0:?}, which the 00439470 policy never produces")]
    UnexpectedAction(ScreenInputAction),
    #[error("required text-entry effect failed")]
    Host(E),
}

/// Constructor effects with no successful defaults.
pub trait TextEntryFactoryHost {
    type Error;
    /// 00463550 (phase 9) or 00463D10 (phases 10/11), before the editable
    /// copy. `slot` is 0055F2F4, the number of the `%dup` initials default.
    fn read_identity(
        &mut self,
        phase: SessionPhase,
        slot: u8,
    ) -> Result<LoadedIdentity, Self::Error>;
    /// 00439B30's eligible, calibrated devices in native order.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

pub struct TextEntryFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    pub process: &'a FrontendProcessState,
    /// 0055F2F4: 1 from boot, the slot the transport gave in a network game.
    pub local_slot: u8,
}

/// Scoped borrows for one tick or callback. `phase` is the session's current
/// phase (+64C). `keyboard_adapter` is the process-lifetime type-0 entry of
/// the adapter cache 005474B0, filled lazily by the first enumeration with a
/// type-0 table and device, and never cleared by screen teardown. `poll_micros` is the 004A4890 sample taken by
/// the latest selected-input poll (004F0200); bits 8..15 of its millisecond
/// word (`poll_micros / 1000`, the timeGetTime value pushed at 004A48A2) are
/// the idle key buffer's stale byte in steady-state ticks. Retail's first
/// tick after construction had another, unidentified writer, for which the
/// port substitutes the same poll byte. `rng` is the shared 0048F060 word
/// 0055F18C.
pub struct TextEntryServices<'a, E> {
    pub input: &'a mut InputRuntime,
    pub store: &'a mut ProfileStore,
    pub key_names: &'a KeyNameTable,
    pub keyboard_adapter: Option<DeviceId>,
    pub text_process: &'a mut TextEntryProcessState,
    pub frontend_process: &'a FrontendProcessState,
    pub rng: &'a mut u32,
    pub effects: &'a mut E,
    pub phase: u32,
    pub poll_micros: u64,
}

/// One 0045A8F0 call: the scheduler's capped and uncapped elapsed time and
/// the frontend attract flag 0055F14C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEntryTick {
    pub scheduler: SchedulerTick,
    pub attract: bool,
}

/// Process values read by the text-entry draw. Screen state and age come from
/// the owner, never a separately maintained presentation copy.
#[derive(Clone, Copy, Debug)]
pub struct TextEntryScreenDraw {
    pub animation: crate::frontend_camera::AnimationClock,
    pub selector_one: u16,
}

#[derive(Debug)]
pub struct TextEntryScreen {
    config: TextEntryScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    input: ScreenInput,
    entry: Option<NameEntryState>,
}

impl TextEntryScreen {
    /// Metadata only; initialize performs the ordered native acquisitions.
    pub fn new(config: TextEntryScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            input: ScreenInput::new(ScreenDispatchPolicy::Base, config.callback_context),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            entry: None,
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
    pub fn entry(&self) -> Option<&NameEntryState> {
        self.entry.as_ref()
    }

    /// Virtual +08, full 00459310 draw. The linked +0C presentation/clear
    /// remains a separate scheduler operation. Every draw failure stops the
    /// screen; this class has no frontend-style native 0x301 catch.
    pub fn draw_frame<H: crate::name_entry_frame::NameEntryFrameHost>(
        &mut self,
        draw: TextEntryScreenDraw,
        host: &mut H,
    ) -> Result<(), crate::name_entry_frame::NameEntryFrameError<H::Error>> {
        use crate::name_entry_frame::{
            draw_name_entry_frame, NameEntryFrameError, NameEntryFrameInput,
        };
        if self.lifecycle != ScreenLifecycle::Ready {
            return Err(NameEntryFrameError::Lifecycle(self.lifecycle));
        }
        let result = draw_name_entry_frame(
            NameEntryFrameInput {
                state: self.entry.as_ref().expect("ready entry").into(),
                age_millis: self.input.age_millis(),
                animation: draw.animation,
                selector_one: draw.selector_one,
            },
            host,
        );
        if result.is_err() {
            self.lifecycle = ScreenLifecycle::Stopped;
        }
        result
    }

    /// 00459060 after its base 00437F90: identity read for the requested
    /// phase (+650), the editable copy, text-entry bindings, context
    /// selection and the shared animation reset. The +F0 player pointer
    /// (00488AA0) has no reader in this class and is not modelled. Only the
    /// session's text-entry factory reaches retail construction, so other
    /// phases are rejected before any acquisition.
    pub fn initialize<D, H: TextEntryFactoryHost>(
        &mut self,
        requested: SessionPhase,
        services: &mut TextEntryFactoryServices<'_, D>,
        host: &mut H,
    ) -> Result<(), TextEntryScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if !text_entry_reuses_phase(requested) {
            return Err(TextEntryScreenError::RequestedPhase(requested));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        let identity = host
            .read_identity(requested, services.local_slot)
            .map_err(TextEntryScreenError::Host)?;
        self.entry = Some(NameEntryState::new(u32::from(requested.id()), identity)?);
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(TextEntryScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.text_entry_input_tables(),
            &devices,
        )?;
        let context = self
            .bindings
            .context()
            .expect("installed text-entry context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => TextEntryScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => TextEntryScreenError::Input(error),
            })?;
        services.process.reset_animation();
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// 004592B0: release binding sets (00439D50), then base 00438080 unlinks
    /// membership, releases +D0/+CC and the context, and removes the scheduler
    /// member. No returning flag is published and the adapter cache survives.
    /// Also usable for retained partial acquisitions as explicit containment.
    pub fn destroy<D>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), TextEntryScreenError<Infallible>> {
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
        self.entry = None;
        self.lifecycle = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Virtual +04, 0045A8F0. Returns every completion in execution order.
    pub fn tick<E: NameEntryCompletion>(
        &mut self,
        tick: TextEntryTick,
        services: &mut TextEntryServices<'_, E>,
    ) -> Result<Vec<NameCompletion>, TextEntryScreenError<E::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        let phase = services.phase;
        let entry = self.entry.as_mut().expect("ready entry");
        let mut host = TickHost {
            screen_input: &mut self.input,
            bindings: &self.bindings,
            keyboard: services.keyboard_adapter,
            input: &mut *services.input,
            key_names: services.key_names,
            key_latch: &mut services.text_process.key_latch,
            frontend_process: services.frontend_process,
            effects: &mut *services.effects,
            poll_micros: services.poll_micros,
        };
        let result = entry
            .tick(
                NameEntryTick {
                    delta_micros: tick.scheduler.delta_micros,
                    elapsed_micros: tick.scheduler.elapsed_micros,
                    attract: tick.attract,
                    phase,
                },
                &mut *services.rng,
                &mut services.text_process.effect_bytes,
                &mut *services.store,
                &mut host,
            )
            .map_err(flatten);
        if result.is_err() {
            self.lifecycle = ScreenLifecycle::Stopped;
        }
        result
    }

    /// Direct callbacks queued for this screen's context by the input poll.
    /// Held-bit callbacks go through base input 00439430; Tab and R are the
    /// screen's own callbacks and fire on press only. System callbacks and
    /// unknown addresses fail before mutation, as with the frontend screen.
    pub fn digital_callback<E: NameEntryCompletion>(
        &mut self,
        event: InputCallback,
        services: &mut TextEntryServices<'_, E>,
    ) -> Result<Option<NameCompletion>, TextEntryScreenError<E::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if event.context != self.config.callback_context {
            return Err(TextEntryScreenError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        let phase = services.phase;
        let entry = self.entry.as_mut().expect("ready entry");
        let effects = &mut *services.effects;
        let result = match event.address {
            CONFIRM_NAME_CALLBACK if event.value != 0 => entry
                .dispatch(NameEntryAction::ConfirmInput, phase, effects)
                .map(|_| None)
                .map_err(flatten_actions),
            RETRY_NAME_CALLBACK if event.value != 0 => entry
                .retry_after_error(phase, &mut *services.store, effects)
                .map_err(flatten_actions),
            CONFIRM_NAME_CALLBACK | RETRY_NAME_CALLBACK => Ok(None),
            _ => self
                .input
                .digital_callback(event, &mut |_, action| {
                    dispatch_slot(entry, action, phase, effects)
                })
                .map(|_| None)
                .map_err(screen_error),
        };
        if self.input.stopped() || matches!(result, Err(TextEntryScreenError::Host(_))) {
            self.lifecycle = ScreenLifecycle::Stopped;
        }
        result
    }

    fn require_lifecycle<E>(
        &self,
        expected: ScreenLifecycle,
    ) -> Result<(), TextEntryScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(TextEntryScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

/// 00439470 slot targets of vtable 00526028 (+24..+38).
fn dispatch_slot<E: NameEntryActions>(
    entry: &mut NameEntryState,
    action: ScreenInputAction,
    phase: u32,
    effects: &mut E,
) -> Result<(), TextEntryScreenError<E::Error>> {
    let action = match action {
        ScreenInputAction::Confirm => NameEntryAction::Activate,
        ScreenInputAction::Previous => NameEntryAction::Up,
        ScreenInputAction::Next => NameEntryAction::Down,
        ScreenInputAction::AdjustPrevious => NameEntryAction::Left,
        ScreenInputAction::AdjustNext => NameEntryAction::Right,
        ScreenInputAction::Back => NameEntryAction::Back,
        other => return Err(TextEntryScreenError::UnexpectedAction(other)),
    };
    entry
        .dispatch(action, phase, effects)
        .map_err(flatten_actions)
}

/// The tick's NameEntryServices: key translation, base input and animation
/// run here; completion effects forward to the required host effects.
struct TickHost<'a, E> {
    screen_input: &'a mut ScreenInput,
    bindings: &'a FrontendBindings,
    keyboard: Option<DeviceId>,
    input: &'a mut InputRuntime,
    key_names: &'a KeyNameTable,
    key_latch: &'a mut KeyTextLatch,
    frontend_process: &'a FrontendProcessState,
    effects: &'a mut E,
    poll_micros: u64,
}

impl<E: NameEntryCompletion> NameEntryActions for TickHost<'_, E> {
    type Error = TextEntryScreenError<E::Error>;
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error> {
        self.effects
            .grant_cheat_flag(bit)
            .map_err(TextEntryScreenError::Host)
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.effects
            .request_phase(phase)
            .map_err(TextEntryScreenError::Host)
    }
}

impl<E: NameEntryCompletion> NameEntryCompletion for TickHost<'_, E> {
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        self.effects
            .persist_identity(request)
            .map_err(TextEntryScreenError::Host)
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        self.effects
            .offline_game_state()
            .map_err(TextEntryScreenError::Host)
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        self.effects
            .network_session()
            .map_err(TextEntryScreenError::Host)
    }
    fn effect_quantum(&self) -> Option<u32> {
        self.effects.effect_quantum()
    }
}

impl<E: NameEntryCompletion> NameEntryServices for TickHost<'_, E> {
    fn read_key(&mut self) -> Result<KeyRead, Self::Error> {
        let keyboard = self.keyboard.ok_or(TextEntryScreenError::MissingKeyboard)?;
        let buttons = self.input.device_buttons(keyboard)?;
        Ok(self
            .key_latch
            .read(buttons, self.key_names, poll_stale_byte(self.poll_micros))?)
    }
    fn tick_base_input(
        &mut self,
        entry: &mut NameEntryState,
        tick: &NameEntryTick,
    ) -> Result<(), Self::Error> {
        let analog = self.bindings.analog_values(self.input)?;
        let effects = &mut *self.effects;
        self.screen_input
            .tick(
                SchedulerTick {
                    delta_micros: tick.delta_micros,
                    elapsed_micros: tick.elapsed_micros,
                },
                analog,
                &mut |_, action| dispatch_slot(entry, action, tick.phase, effects),
            )
            .map_err(screen_error)
    }
    fn tick_animation(&mut self, delta_micros: i32) -> Result<(), Self::Error> {
        self.frontend_process.advance_animation(delta_micros as u32);
        Ok(())
    }
}

fn flatten_actions<E>(error: NameEntryTickError<E>) -> TextEntryScreenError<E> {
    match error {
        NameEntryTickError::State(error) => TextEntryScreenError::State(error),
        NameEntryTickError::Host(error) => TextEntryScreenError::Host(error),
    }
}

fn flatten<E>(error: NameEntryTickError<TextEntryScreenError<E>>) -> TextEntryScreenError<E> {
    match error {
        NameEntryTickError::State(error) => TextEntryScreenError::State(error),
        NameEntryTickError::Host(error) => error,
    }
}

fn screen_error<E>(error: ScreenInputError<TextEntryScreenError<E>>) -> TextEntryScreenError<E> {
    TextEntryScreenError::ScreenInput(match error {
        ScreenInputError::Dispatch(error) => return error,
        ScreenInputError::UnknownCallback(address) => ScreenInputError::UnknownCallback(address),
        ScreenInputError::Context { expected, actual } => {
            ScreenInputError::Context { expected, actual }
        }
        ScreenInputError::Stopped => ScreenInputError::Stopped,
    })
}

#[cfg(test)]
#[path = "text_entry_screen_tests.rs"]
mod tests;
