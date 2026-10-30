//! Concrete screen-input/menu integration for the frontend virtual callbacks.
//!
//! This resolves shared authored menus and executes native basis arithmetic.
//! Audio, settings, specialized menus, and session changes require their real
//! external owners. No external operation has a successful default.

use std::convert::Infallible;

use thiserror::Error;

use crate::frontend_assets::{FrontendAssetError, FrontendMenuCatalog};
use crate::frontend_control::{
    FrontendControl, FrontendControlError, FrontendHost, FrontendInput, FrontendProfileContext,
};
use crate::input::{InputCallback, InputRuntime};
use crate::menu::{MenuActionRequest, MenuBasis, MenuCommand, MenuData, MenuStack, MenuVisibility};
use crate::menu_basis::MenuBasisMath;
use crate::network::link::NetworkLink;
use crate::network::udp::{SearchTarget, SessionInfo};
use crate::network_menu::{
    host_menu_action, list_action, NetworkMenu, NetworkMenuError, NetworkMenuServices, HOST_MENU,
    LIST_ACTION,
};
use crate::profile_menu::{
    ProfileMenu, ProfileMenuError, ProfileMenuServices, ProfileOperation, ProfileTickOutcome,
};
use crate::profile_store::{ProfileStore, ProfileStoreError};
use crate::scheduler::SchedulerTick;
use crate::screen_input::{
    ScreenAnalogValues, ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError,
};
use crate::settings_row::VolumeChannel;

/// The external subset of FrontendHost. Generic authored menus and base input
/// are implemented by FrontendDispatch; derived menu owners remain explicit.
pub trait FrontendEffects {
    type Error;

    fn service_music(&mut self) -> Result<(), Self::Error>;
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error>;
    fn persist_settings(&mut self) -> Result<(), Self::Error>;
    /// Publish native 0054694C=1 at the actual profile action boundary.
    fn set_transition_flag(&mut self) -> Result<(), Self::Error>;
    /// Native catches save failures and displays a timed message. Preserve
    /// the concrete error for diagnostics while allowing that state to run.
    fn profile_operation_failed(
        &mut self,
        operation: ProfileOperation,
        error: &ProfileStoreError,
    ) -> Result<(), Self::Error>;
    fn open_network_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error>;
    /// 00453B80 value-row adjust. The effects owner resolves the absolute
    /// cell address to its live settings word; arithmetic stays in
    /// `settings_row`. Cells outside the settings state are an explicit
    /// boundary, never a neighboring word.
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error>;
    /// 0040F740 world refresh after a 00463B50 adjust. No-op without a loaded
    /// world, mirroring the native early return; world-attached backends must
    /// own the entity patching instead of succeeding silently.
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error>;
    /// 00463B70 Restore Defaults over the live settings words, including its
    /// display reinit and the reset-failure restore of the old +0C tier.
    fn restore_default_settings(&mut self) -> Result<(), Self::Error>;
    /// 00462F40 display-aware adjust with variant handling and the
    /// backend-reset/rollback contract. Shares the settings-word ownership
    /// with the plain adjust above.
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error>;
    /// 00463AC0/00463B00 audio tail after a Sound-row adjust. The effects
    /// owner reads the channel's own settings word (+00 or +04) and applies
    /// it to the live FX master or music/AUX consumers.
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error>;
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error>;
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error>;
    fn request_exit(&mut self) -> Result<(), Self::Error>;
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error>;
    /// The network link the menus drive (`0055F444`, `0055F3B0`). None where
    /// the platform has no network: its row then keeps its refusal.
    fn network_link(&mut self) -> Option<&mut NetworkLink> {
        None
    }
    /// The link with the settings and identity a protocol pass reads.
    fn network_platform(&mut self) -> Option<crate::network::frontend_host::NetworkPlatform<'_>> {
        None
    }
    /// Where `0045CCE0` reads the network message lines from: the game's
    /// folder and its H0 defaults. None where the platform has no network.
    fn network_line_source(&mut self) -> Option<crate::network::game_tick::LineGroupSource> {
        None
    }
    /// The settings DWORD at `offset` of `0054E8A8`.
    fn settings_word(&mut self, _offset: u32) -> Option<u32> {
        None
    }
    fn set_settings_word(&mut self, _offset: u32, _value: u32) -> Option<()> {
        None
    }
}

#[derive(Debug, Error)]
pub enum FrontendDispatchError<E> {
    #[error(transparent)]
    Asset(#[from] FrontendAssetError),
    #[error("required frontend effect failed")]
    Effect(E),
    #[error(transparent)]
    Control(FrontendControlError<Infallible>),
    #[error(transparent)]
    ScreenInput(ScreenInputError<Infallible>),
    #[error("frontend dispatch requires the frontend screen policy, got {0:?}")]
    ScreenPolicy(ScreenDispatchPolicy),
    #[error("frontend control context {control} does not match screen input context {input}")]
    Context { control: u32, input: u32 },
    #[error("a digital action cannot recursively request a base-input tick")]
    NestedBaseTick,
    #[error("profile dispatch requires the process-owned profile store")]
    MissingProfileStore,
    #[error("profile rebuild requires the live input runtime during tick")]
    MissingProfileInput,
    #[error(transparent)]
    Profile(ProfileMenuError<Infallible>),
    #[error("the network menus need the platform's network link")]
    NetworkUnavailable,
    #[error("the network menus need settings word {0:#x}")]
    SettingsWord(u32),
    #[error("network menu: {0}")]
    Network(String),
}

/// A scoped borrow of the process-owned menu definitions, basis math and
/// required external effects. Screen and control state stay with their owner.
pub struct FrontendDispatch<'a, E: FrontendEffects> {
    pub catalog: &'a mut FrontendMenuCatalog,
    pub basis: &'a MenuBasisMath,
    pub visibility: MenuVisibility,
    pub effects: &'a mut E,
    pub profiles: Option<&'a mut ProfileStore>,
}

impl<E: FrontendEffects> FrontendDispatch<'_, E> {
    /// Supply values read from FrontendBindings::analog_values after input
    /// publication. The control calls base input before music/menu work.
    pub fn tick(
        &mut self,
        control: &mut FrontendControl,
        input: &mut ScreenInput,
        runtime_input: &mut InputRuntime,
        analog: ScreenAnalogValues,
        tick: SchedulerTick,
    ) -> Result<(), FrontendDispatchError<E::Error>> {
        validate_owners(control, input)?;
        let mut adapter = Adapter {
            dispatch: self,
            input,
            pass: Pass::Tick(analog),
            runtime_input: Some(runtime_input),
        };
        control.tick(tick, &mut adapter).map_err(control_error)
    }

    /// The ScreenInput owner validates address/context before mutating state.
    /// Callback dispatch uses its live mutable input, never a held-bit snapshot.
    pub fn digital_callback(
        &mut self,
        control: &mut FrontendControl,
        input: &mut ScreenInput,
        event: InputCallback,
    ) -> Result<(), FrontendDispatchError<E::Error>> {
        validate_owners(control, input)?;
        input
            .digital_callback(event, &mut |live, action| {
                self.action(control, live, action)
            })
            .map_err(screen_error)
    }

    fn action(
        &mut self,
        control: &mut FrontendControl,
        input: &mut ScreenInput,
        action: ScreenInputAction,
    ) -> Result<(), FrontendDispatchError<E::Error>> {
        let action = match action {
            ScreenInputAction::Confirm => FrontendInput::Confirm,
            ScreenInputAction::Previous => FrontendInput::Previous,
            ScreenInputAction::Next => FrontendInput::Next,
            ScreenInputAction::AdjustPrevious => FrontendInput::AdjustPrevious,
            ScreenInputAction::AdjustNext => FrontendInput::AdjustNext,
            ScreenInputAction::Back => FrontendInput::Back,
            ScreenInputAction::Auxiliary6 => FrontendInput::Auxiliary6,
            ScreenInputAction::Auxiliary7 => FrontendInput::Auxiliary7,
            ScreenInputAction::RequestAttract => FrontendInput::RequestAttract,
            ScreenInputAction::ResultsGate | ScreenInputAction::Bit200 => {
                unreachable!("only the results pass dispatches them")
            }
        };
        let visibility = self.visibility;
        let mut adapter = Adapter {
            dispatch: self,
            input,
            pass: Pass::DigitalAction,
            runtime_input: None,
        };
        control
            .input(action, visibility, &mut adapter)
            .map(|_| ())
            .map_err(control_error)
    }
}

#[derive(Clone, Copy)]
enum Pass {
    Tick(ScreenAnalogValues),
    DigitalAction,
}

struct Adapter<'a, 'b, E: FrontendEffects> {
    dispatch: &'a mut FrontendDispatch<'b, E>,
    input: &'a mut ScreenInput,
    pass: Pass,
    runtime_input: Option<&'a mut InputRuntime>,
}

impl<E: FrontendEffects> FrontendHost for Adapter<'_, '_, E> {
    type Error = FrontendDispatchError<E::Error>;

    fn menu_data(&mut self, address: u32) -> Result<MenuData, Self::Error> {
        Ok(self.dispatch.catalog.menu_data(address)?)
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.dispatch.basis.rotate(basis, angle);
        Ok(())
    }

    fn tick_profile(
        &mut self,
        profile: &mut ProfileMenu,
        delta_micros: i32,
        context: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>> {
        let store = self
            .dispatch
            .profiles
            .as_deref_mut()
            .ok_or(FrontendControlError::Host(
                FrontendDispatchError::MissingProfileStore,
            ))?;
        let mut services = ProfileServices {
            basis: self.dispatch.basis,
            effects: self.dispatch.effects,
            screen_input: self.input,
            runtime_input: self.runtime_input.as_deref_mut(),
            context,
        };
        let outcome = profile
            .tick(
                delta_micros,
                self.dispatch.visibility == MenuVisibility::OwnerFlagSet,
                store,
                &mut services,
            )
            .map_err(profile_error)
            .map_err(FrontendControlError::Host)?;
        if let ProfileTickOutcome::OperationFailed { operation, error } = outcome {
            services
                .effects
                .profile_operation_failed(operation, &error)
                .map_err(FrontendDispatchError::Effect)
                .map_err(FrontendControlError::Host)?;
        }
        Ok(())
    }

    fn profile_action(
        &mut self,
        profile: &mut ProfileMenu,
        request: MenuActionRequest,
        context: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>> {
        let store = self
            .dispatch
            .profiles
            .as_deref_mut()
            .ok_or(FrontendControlError::Host(
                FrontendDispatchError::MissingProfileStore,
            ))?;
        let mut services = ProfileServices {
            basis: self.dispatch.basis,
            effects: self.dispatch.effects,
            screen_input: self.input,
            runtime_input: self.runtime_input.as_deref_mut(),
            context,
        };
        profile
            .execute_callback(
                request.callback_address,
                request.command,
                request.arguments,
                self.dispatch.visibility == MenuVisibility::OwnerFlagSet,
                store,
                &mut services,
            )
            .map_err(profile_error)
            .map_err(FrontendControlError::Host)
    }

    fn tick_base_input(
        &mut self,
        control: &mut FrontendControl,
        tick: SchedulerTick,
    ) -> Result<(), Self::Error> {
        let Pass::Tick(analog) = self.pass else {
            return Err(FrontendDispatchError::NestedBaseTick);
        };
        let dispatch = &mut *self.dispatch;
        self.input
            .tick(tick, analog, &mut |live, action| {
                dispatch.action(control, live, action)
            })
            .map_err(screen_error)
    }

    fn clear_held_input(&mut self) -> Result<(), Self::Error> {
        self.input.clear_held();
        Ok(())
    }

    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .service_music()
            .map_err(FrontendDispatchError::Effect)
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .play_sound(resource)
            .map_err(FrontendDispatchError::Effect)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .persist_settings()
            .map_err(FrontendDispatchError::Effect)
    }
    fn profile_first_byte_nonzero(&mut self) -> Result<bool, Self::Error> {
        Ok(self
            .dispatch
            .profiles
            .as_deref()
            .ok_or(FrontendDispatchError::MissingProfileStore)?
            .write_attempted())
    }
    fn open_start_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error> {
        self.dispatch
            .profiles
            .as_deref()
            .ok_or(FrontendDispatchError::MissingProfileStore)?;
        let templates = self.dispatch.catalog.profile_templates()?;
        stack.push(ProfileMenu::new(templates, MenuBasis::INITIAL, false));
        Ok(())
    }
    /// `0045FD40`: `0045F9C0(false)`, the connections. Without a network
    /// link the row keeps its refusal.
    fn open_network_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error> {
        if self.dispatch.effects.network_link().is_none() {
            return self
                .dispatch
                .effects
                .open_network_menu(stack)
                .map_err(FrontendDispatchError::Effect);
        }
        let templates = self.dispatch.catalog.network_templates()?;
        let layouts = self.dispatch.catalog.menu_data(HOST_MENU)?.layouts();
        let names = NetworkLink::connections().map(str::to_string).to_vec();
        let close = self
            .dispatch
            .effects
            .network_link()
            .ok_or(FrontendDispatchError::NetworkUnavailable)?
            .close_token();
        stack.push(NetworkMenu::connections(&templates, layouts, names).with_close(close));
        Ok(())
    }
    fn tick_network(
        &mut self,
        menu: &mut NetworkMenu,
        delta_micros: i32,
    ) -> Result<(), Self::Error> {
        let templates = self.dispatch.catalog.network_templates()?;
        menu.tick(
            delta_micros,
            &templates,
            &mut NetworkServices(&mut *self.dispatch),
        )
        .map_err(network_error)
    }
    fn network_action(
        &mut self,
        stack: &mut MenuStack,
        request: MenuActionRequest,
    ) -> Result<(), Self::Error> {
        let templates = self.dispatch.catalog.network_templates()?;
        let mut services = NetworkServices(&mut *self.dispatch);
        if request.callback_address == LIST_ACTION {
            list_action(stack, request, &templates, &mut services)
        } else {
            host_menu_action(stack, request, &mut services)
        }
        .map_err(network_error)
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .adjust_setting(address, command, max)
            .map_err(FrontendDispatchError::Effect)
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .refresh_settings_world()
            .map_err(FrontendDispatchError::Effect)
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .restore_default_settings()
            .map_err(FrontendDispatchError::Effect)
    }
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .adjust_display_setting(address, command, max)
            .map_err(FrontendDispatchError::Effect)
    }
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .apply_volume(channel)
            .map_err(FrontendDispatchError::Effect)
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .request_phase(phase)
            .map_err(FrontendDispatchError::Effect)
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .set_attract_flag(enabled)
            .map_err(FrontendDispatchError::Effect)
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .request_exit()
            .map_err(FrontendDispatchError::Effect)
    }
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        self.dispatch
            .effects
            .external_action(request, stack)
            .map_err(FrontendDispatchError::Effect)
    }
}

struct ProfileServices<'a, 'c, E: FrontendEffects> {
    basis: &'a MenuBasisMath,
    effects: &'a mut E,
    screen_input: &'a mut ScreenInput,
    runtime_input: Option<&'a mut InputRuntime>,
    context: &'a mut FrontendProfileContext<'c>,
}

impl<E: FrontendEffects> ProfileMenuServices for ProfileServices<'_, '_, E> {
    type Error = FrontendDispatchError<E::Error>;
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.basis.rotate(basis, angle);
        Ok(())
    }
    fn reset_profile_input(&mut self) -> Result<(), Self::Error> {
        let runtime = self
            .runtime_input
            .as_deref_mut()
            .ok_or(FrontendDispatchError::MissingProfileInput)?;
        self.screen_input.clear_held_low_word();
        runtime.reset_baselines_and_discard();
        Ok(())
    }
    fn select_camera(&mut self, preset: i32) -> Result<(), Self::Error> {
        self.context
            .select_camera(preset)
            .map_err(FrontendDispatchError::Control)
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.effects
            .set_transition_flag()
            .map_err(FrontendDispatchError::Effect)
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.effects
            .request_phase(phase)
            .map_err(FrontendDispatchError::Effect)
    }
    fn play_sound(&mut self, slot: i16) -> Result<(), Self::Error> {
        self.effects
            .play_sound(slot)
            .map_err(FrontendDispatchError::Effect)
    }
    fn begin_controller_fade(&mut self) -> Result<(), Self::Error> {
        self.context.begin_controller_fade();
        Ok(())
    }
}

/// The network menus' services over the frontend's owners.
struct NetworkServices<'a, 'b, E: FrontendEffects>(&'a mut FrontendDispatch<'b, E>);

impl<E: FrontendEffects> NetworkServices<'_, '_, E> {
    fn link(&mut self) -> Result<&mut NetworkLink, FrontendDispatchError<E::Error>> {
        self.0
            .effects
            .network_link()
            .ok_or(FrontendDispatchError::NetworkUnavailable)
    }
}

impl<E: FrontendEffects> NetworkMenuServices for NetworkServices<'_, '_, E> {
    type Error = FrontendDispatchError<E::Error>;
    fn close_token(&mut self) -> Result<crate::network_menu::CloseToken, Self::Error> {
        Ok(self.link()?.close_token())
    }
    fn network_game_ready(&mut self) -> bool {
        true
    }
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.0.basis.rotate(basis, angle);
        Ok(())
    }
    fn menu_data(&mut self, address: u32) -> Result<MenuData, Self::Error> {
        Ok(self.0.catalog.menu_data(address)?)
    }
    fn connections(&mut self) -> Result<Vec<String>, Self::Error> {
        Ok(NetworkLink::connections().map(str::to_string).to_vec())
    }
    fn initialize_connection(&mut self, target: SearchTarget) -> Result<(), Self::Error> {
        self.link()?.initialize_connection(target);
        Ok(())
    }
    /// `004940A0`'s result goes unread: a search that cannot start lists
    /// nothing.
    fn enumerate_sessions(&mut self) -> Result<Vec<SessionInfo>, Self::Error> {
        match self.link()?.enumerate_sessions(std::time::Instant::now()) {
            Ok(sessions) => Ok(sessions),
            Err(error) => {
                eprintln!("Network: the session search failed: {error}");
                Ok(Vec::new())
            }
        }
    }
    fn join(&mut self, session: &SessionInfo) -> Result<bool, Self::Error> {
        match self.link()?.join(session) {
            Ok(()) => Ok(true),
            Err(error) => {
                eprintln!("Network: joining {:?} failed: {error}", session.name);
                Ok(false)
            }
        }
    }
    fn host(&mut self) -> Result<bool, Self::Error> {
        match self.link()?.host(std::time::Instant::now()) {
            Ok(()) => Ok(true),
            Err(error) => {
                eprintln!("Network: hosting failed: {error}");
                Ok(false)
            }
        }
    }
    fn host_slot(&mut self) -> Result<bool, Self::Error> {
        self.link()?
            .first_heartbeat()
            .map_err(|error| FrontendDispatchError::Network(error.to_string()))
    }
    fn wait_receive(&mut self, clear_world: bool) -> Result<(u8, i32), Self::Error> {
        self.0
            .effects
            .network_platform()
            .ok_or(FrontendDispatchError::NetworkUnavailable)?
            .wait_receive(clear_world)
            .map_err(FrontendDispatchError::Network)
    }
    fn reset_net_game(&mut self) -> Result<(), Self::Error> {
        let lines = self
            .0
            .effects
            .network_line_source()
            .ok_or(FrontendDispatchError::NetworkUnavailable)?;
        self.0
            .effects
            .network_platform()
            .ok_or(FrontendDispatchError::NetworkUnavailable)?
            .reset_game(&lines);
        Ok(())
    }
    fn close_session(&mut self) -> Result<(), Self::Error> {
        self.link()?.close();
        Ok(())
    }
    fn play_cue(&mut self, cue: i16) -> Result<(), Self::Error> {
        self.0
            .effects
            .play_sound(cue)
            .map_err(FrontendDispatchError::Effect)
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.0
            .effects
            .request_phase(phase)
            .map_err(FrontendDispatchError::Effect)
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.0
            .effects
            .set_transition_flag()
            .map_err(FrontendDispatchError::Effect)
    }
    fn setting(&mut self, offset: u32) -> Result<u32, Self::Error> {
        self.0
            .effects
            .settings_word(offset)
            .ok_or(FrontendDispatchError::SettingsWord(offset))
    }
    fn set_setting(&mut self, offset: u32, value: u32) -> Result<(), Self::Error> {
        self.0
            .effects
            .set_settings_word(offset, value)
            .ok_or(FrontendDispatchError::SettingsWord(offset))
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.0
            .effects
            .adjust_setting(address, command, max)
            .map_err(FrontendDispatchError::Effect)
    }
    fn report(&mut self, message: &str) -> Result<(), Self::Error> {
        eprintln!("Network: {message}");
        Ok(())
    }
}

fn network_error<E>(error: NetworkMenuError<FrontendDispatchError<E>>) -> FrontendDispatchError<E> {
    match error {
        NetworkMenuError::Host(error) => error,
        other => FrontendDispatchError::Network(other.to_string()),
    }
}

fn profile_error<E>(error: ProfileMenuError<FrontendDispatchError<E>>) -> FrontendDispatchError<E> {
    FrontendDispatchError::Profile(match error {
        ProfileMenuError::Host(error) => return error,
        ProfileMenuError::State(error) => ProfileMenuError::State(error),
        ProfileMenuError::Store(error) => ProfileMenuError::Store(error),
        ProfileMenuError::UnsupportedCallback(address) => {
            ProfileMenuError::UnsupportedCallback(address)
        }
        ProfileMenuError::InvalidSlot(index) => ProfileMenuError::InvalidSlot(index),
        ProfileMenuError::MissingPendingIndex(operation) => {
            ProfileMenuError::MissingPendingIndex(operation)
        }
    })
}

fn validate_owners<E>(
    control: &FrontendControl,
    input: &ScreenInput,
) -> Result<(), FrontendDispatchError<E>> {
    if input.policy() != ScreenDispatchPolicy::Frontend {
        return Err(FrontendDispatchError::ScreenPolicy(input.policy()));
    }
    if control.callback_context() != input.context() {
        return Err(FrontendDispatchError::Context {
            control: control.callback_context(),
            input: input.context(),
        });
    }
    Ok(())
}

// A nested input operation already owns the original error. Flatten only the
// adapter's Host/Dispatch wrappers; preserve every native state error exactly.
fn control_error<E>(
    error: FrontendControlError<FrontendDispatchError<E>>,
) -> FrontendDispatchError<E> {
    FrontendDispatchError::Control(match error {
        FrontendControlError::Host(error) => return error,
        FrontendControlError::CameraPreset { index, count } => {
            FrontendControlError::CameraPreset { index, count }
        }
        FrontendControlError::Menu(error) => FrontendControlError::Menu(error),
        FrontendControlError::ProfileStore(error) => FrontendControlError::ProfileStore(error),
        FrontendControlError::Stopped => FrontendControlError::Stopped,
        FrontendControlError::ReentrantTick => FrontendControlError::ReentrantTick,
        FrontendControlError::ReentrantDraw => FrontendControlError::ReentrantDraw,
    })
}

fn screen_error<E>(error: ScreenInputError<FrontendDispatchError<E>>) -> FrontendDispatchError<E> {
    FrontendDispatchError::ScreenInput(match error {
        ScreenInputError::Dispatch(error) => return error,
        ScreenInputError::UnknownCallback(address) => ScreenInputError::UnknownCallback(address),
        ScreenInputError::Context { expected, actual } => {
            ScreenInputError::Context { expected, actual }
        }
        ScreenInputError::Stopped => ScreenInputError::Stopped,
    })
}

#[cfg(test)]
#[path = "frontend_dispatch_tests.rs"]
mod tests;
