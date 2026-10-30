//! Frontend control at 00483430/004838A0 and 00484E00..004852C0.
//!
//! Owns the base authored menu stack, camera, clock, and frontend timers. Scene
//! 47 residency, input registration, menu colors, drawing, profile/network menu
//! subclasses, settings, and sound remain required external owners. In
//! particular, Start Game opens 00448220; it does not jump to a gameplay phase.

use inf_formats::FrontendCameraPreset;
use std::{cell::Cell, rc::Rc};
use thiserror::Error;

use crate::frontend_camera::{AnimationClock, CameraMotion, FrontendCamera, FrontendEntry};
use crate::menu::{
    MenuActionRequest, MenuBasis, MenuCommand, MenuCommandOutcome, MenuControllerSpec, MenuData,
    MenuError, MenuHost, MenuInput, MenuOwner, MenuStack, MenuState, MenuStateError,
    MenuVisibility,
};
use crate::network_menu::NetworkMenu;
use crate::profile_menu::ProfileMenu;
use crate::profile_store::ProfileStoreError;
use crate::scheduler::SchedulerTick;
use crate::settings_row::VolumeChannel;

#[path = "frontend_draw.rs"]
mod draw;

pub const ROOT_MENU_ADDRESS: u32 = 0x0053_c088;
pub const QUIT_MENU_ADDRESS: u32 = 0x0053_c130;

#[derive(Clone, Debug)]
pub struct FrontendControlConfig {
    pub camera_presets: Vec<FrontendCameraPreset>,
    pub position_scale: i32,
    pub motion_scale: i32,
    /// Supplied by the persistent session/game owner. Frontend destruction at
    /// 00483BD0 sets global 0055F14D = 1 after destroying binding sets and before
    /// destroying menus/base screen; later factories therefore use Returning.
    /// Successful ready-menu drawing also sets the bit at 004849F8. Menu depth
    /// or camera readiness alone cannot derive this lifetime bit.
    pub entry: FrontendEntry,
    /// Logical callback context for the owning screen; never a host pointer.
    pub context: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendInput {
    Previous,
    Next,
    AdjustPrevious,
    AdjustNext,
    Confirm,
    Back,
    Auxiliary6,
    Auxiliary7,
    /// 004852B0 sets the idle timer to 60,001; transition happens in tick.
    RequestAttract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendInputOutcome {
    CameraBlocked,
    Menu(MenuCommandOutcome),
    CancelledEditing,
    PoppedMenu,
    AttractArmed,
}

#[derive(Debug, Error)]
pub enum FrontendControlError<E> {
    #[error("frontend camera preset {index} is outside {count} records")]
    CameraPreset { index: u32, count: usize },
    #[error(transparent)]
    Menu(#[from] MenuStateError),
    #[error(transparent)]
    ProfileStore(#[from] ProfileStoreError),
    #[error("frontend host operation failed")]
    Host(E),
    #[error("frontend control stopped after an earlier partial operation")]
    Stopped,
    #[error("frontend tick cannot recursively enter itself")]
    ReentrantTick,
    #[error("frontend drawing cannot recursively enter itself")]
    ReentrantDraw,
}

/// Every operation is required. A backend that does not own the native effect
/// must fail, rather than manufacture successful profile/network menus or draws.
pub trait FrontendHost {
    type Error;

    /// Return the shared live table owner, not a fresh executable decode. Its
    /// lifetime includes screen re-entry, preserving authored row-height writes.
    fn menu_data(&mut self, address: u32) -> Result<MenuData, Self::Error>;
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error>;
    /// Execute the derived owner while borrowing the actual camera/timer and
    /// controller fade flag. These effects must occur within the callback.
    fn tick_profile(
        &mut self,
        profile: &mut ProfileMenu,
        delta_micros: i32,
        context: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>>;
    fn profile_action(
        &mut self,
        profile: &mut ProfileMenu,
        request: MenuActionRequest,
        context: &mut FrontendProfileContext<'_>,
    ) -> Result<(), FrontendControlError<Self::Error>>;
    /// Owns 004392B0's analog values, held-mask dispatch, and repeat countdown.
    /// It may dispatch input through `control` before the frontend menu tick.
    fn tick_base_input(
        &mut self,
        control: &mut FrontendControl,
        tick: SchedulerTick,
    ) -> Result<(), Self::Error>;
    /// 00485110 clears screen+C4 before checking camera readiness.
    fn clear_held_input(&mut self) -> Result<(), Self::Error>;
    /// 00487B90: CD/music service after base input, before frontend menu work.
    fn service_music(&mut self) -> Result<(), Self::Error>;
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error>;
    fn persist_settings(&mut self) -> Result<(), Self::Error>;
    /// The first byte of the profile owner at session+660, read after saving.
    fn profile_first_byte_nonzero(&mut self) -> Result<bool, Self::Error>;
    /// 00449A00(false) constructs the derived 00448220 profile menu. A generic
    /// authored MenuState cannot stand in for its methods and owned profile.
    fn open_start_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error>;
    /// 0045FD40 constructs 0045F9C0(false), vtable 0052621C, then pushes it.
    fn open_network_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error>;
    /// The network menus' ticks (`0045FDE0`, `0045FF80`).
    fn tick_network(
        &mut self,
        menu: &mut NetworkMenu,
        delta_micros: i32,
    ) -> Result<(), Self::Error>;
    /// A network list row's command (`0045F740`) or the host menu's
    /// (`0045F560`, `0045F440`, `0045F480`, `0045F320`).
    fn network_action(
        &mut self,
        stack: &mut MenuStack,
        request: MenuActionRequest,
    ) -> Result<(), Self::Error>;
    /// 00462F40 display-aware adjust of the settings DWORD at the row's
    /// address with the row's maximum, including variant handling and the
    /// backend-reset/rollback contract.
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error>;
    /// 0040F740 world refresh after a 00463B50 adjust. With no loaded world
    /// it returns immediately, mirroring the native null-world early return;
    /// a world-attached implementation must own the entity patching instead.
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error>;
    /// Options Restore Defaults 00463B70. It takes no command, so every
    /// action command reaching the row runs it: the 34 authored defaults,
    /// then the display reinit at their +0C tier with the 00462F40 rollback.
    fn restore_default_settings(&mut self) -> Result<(), Self::Error>;
    /// 00453B80 adjusts the settings DWORD cell at the row's address with the
    /// row's maximum. Command 2 decrements, 3 increments clamped, and 4
    /// increments wrapping to zero; other commands are ignored.
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error>;
    /// 00463AC0/00463B00 tail after the row's 00453B80 adjust, run for every
    /// action command: apply the channel's fixed settings word to its live
    /// audio consumer.
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error>;
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error>;
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error>;
    fn request_exit(&mut self) -> Result<(), Self::Error>;
    /// Settings and other derived callbacks retain their complete native
    /// request. This is a required dispatcher, never a success default.
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Default)]
struct FrontendState {
    returning: Rc<Cell<bool>>,
    accelerated: bool,
    /// Screen BYTE +124, which 00484A70 sets.
    start_pending: bool,
    timer_0120_millis: i32,
    persist_delay_millis: i32,
    close_delay_millis: i32,
    idle_millis: i32,
}

/// Scoped access to the live frontend effects used by profile callbacks.
/// No copied camera or deferred approximation of controller fade is involved.
pub struct FrontendProfileContext<'a> {
    config: &'a FrontendControlConfig,
    camera: &'a mut FrontendCamera,
    state: &'a mut FrontendState,
    active: &'a mut bool,
}

impl FrontendProfileContext<'_> {
    pub fn select_camera(
        &mut self,
        preset: i32,
    ) -> Result<(), FrontendControlError<std::convert::Infallible>> {
        select_camera(self.config, self.camera, self.state, preset as u32)
    }
    pub fn begin_controller_fade(&mut self) {
        *self.active = false;
    }
}

#[derive(Debug)]
pub struct FrontendControl {
    config: FrontendControlConfig,
    menus: MenuStack,
    camera: FrontendCamera,
    animation: Rc<Cell<AnimationClock>>,
    state: FrontendState,
    stopped: bool,
    ticking: bool,
    drawing: bool,
}

impl FrontendControl {
    /// 004838A0 initializes the empty controller and camera before music and
    /// scene residency. The screen factory pushes the root only after binding
    /// installation/selection, then persists settings and resets the clock.
    pub fn new(
        config: FrontendControlConfig,
    ) -> Result<Self, FrontendControlError<std::convert::Infallible>> {
        let preset = config
            .camera_presets
            .first()
            .ok_or(FrontendControlError::CameraPreset { index: 0, count: 0 })?;
        let camera = FrontendCamera::new(preset, config.position_scale, config.entry);
        let menus = MenuStack::new(MenuControllerSpec {
            fade_rate: 65535,
            viewport: [0, 0, 1024, 1024],
        });
        let state = FrontendState {
            returning: Rc::new(Cell::new(config.entry == FrontendEntry::Returning)),
            ..FrontendState::default()
        };
        Ok(Self {
            config,
            menus,
            camera,
            animation: Rc::new(Cell::new(AnimationClock::default())),
            state,
            stopped: false,
            ticking: false,
            drawing: false,
        })
    }

    pub fn menus(&self) -> &MenuStack {
        &self.menus
    }
    /// Live menu state and renderer feedback. Complete frontend controller
    /// drawing through `draw_menu` so the entry/acceleration state is
    /// updated together with MenuStack's drawn flag.
    pub fn menus_mut(&mut self) -> &mut MenuStack {
        &mut self.menus
    }
    pub fn camera(&self) -> &FrontendCamera {
        &self.camera
    }
    pub fn animation(&self) -> AnimationClock {
        self.animation.get()
    }
    /// 00439530 occurs after the factory's settings persistence.
    pub(crate) fn reset_animation(&mut self) {
        self.animation.set(AnimationClock::default());
    }
    /// Bind the live process global, including changes from nested callbacks.
    pub(crate) fn share_process_state(
        &mut self,
        returning: Rc<Cell<bool>>,
        animation: Rc<Cell<AnimationClock>>,
    ) {
        self.state.returning = returning;
        self.animation = animation;
    }
    pub(crate) fn callback_context(&self) -> u32 {
        self.config.context
    }
    pub(crate) fn publish_returning(&self) {
        self.state.returning.set(true);
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }
    pub fn idle_millis(&self) -> i32 {
        self.state.idle_millis
    }
    /// Port-only AttractDemoDeferredApproximation: restart one eligible idle
    /// period after the actual native request was deferred before transition.
    pub(crate) fn reset_idle_after_attract_deferral(&mut self) {
        self.state.idle_millis = 0;
    }
    pub fn timer_0120_millis(&self) -> i32 {
        self.state.timer_0120_millis
    }
    pub fn accelerated(&self) -> bool {
        self.state.accelerated
    }
    pub fn entry_state(&self) -> FrontendEntry {
        if self.state.returning.get() {
            FrontendEntry::Returning
        } else {
            FrontendEntry::First
        }
    }

    /// 004849DC..00484A05 checks readiness before the real controller draw.
    /// Only its successful completion sets 0055F14D and clears +11C, even if a
    /// row callback changed the camera target while drawing. The closure owns
    /// actual controller submission and frontend camera wrappers for its rows.
    /// Scene/footer drawing occurs before this boundary and is external.
    fn draw_menu_if_ready<R, E>(
        &mut self,
        native_error_code: impl Fn(&E) -> Option<u32>,
        draw: impl FnOnce(&mut Self) -> Result<R, FrontendControlError<E>>,
    ) -> Result<Option<R>, FrontendControlError<E>> {
        if self.stopped {
            return Err(FrontendControlError::Stopped);
        }
        if self.drawing {
            self.stopped = true;
            return Err(FrontendControlError::ReentrantDraw);
        }
        if !self.camera.ready() {
            return Ok(None);
        }
        self.drawing = true;
        let result = draw(self);
        self.drawing = false;
        match result {
            Ok(value) if !self.stopped => {
                self.menus.mark_drawn();
                self.state.returning.set(true);
                self.state.accelerated = false;
                Ok(Some(value))
            }
            Ok(_) => Err(FrontendControlError::Stopped),
            Err(error) => {
                // Only the enclosing 00484940 catch consumes native 0x301.
                // Preserve partial row writes, but never undo an independently
                // stopped owner or treat state/reentrancy failures as native.
                if !matches!(&error, FrontendControlError::Host(error) if native_error_code(error) == Some(0x301))
                {
                    self.stopped = true;
                }
                Err(error)
            }
        }
    }

    /// 00483B90 is idempotent while the delayed save/pop is already armed.
    pub fn arm_settings_close(&mut self) {
        if self.state.persist_delay_millis == 0 {
            self.state.persist_delay_millis = 1;
        }
    }

    /// Row draw wrappers have camera side effects BEFORE the underlying draw.
    /// The closure receives the proven delegated callback address and performs
    /// the actual draw with its original geometry/arguments. No draw is claimed
    /// merely because this method was called.
    #[cfg(test)]
    fn with_row_draw<R, E>(
        &mut self,
        callback: u32,
        arguments: [u32; 2],
        draw: impl FnOnce(u32) -> Result<R, E>,
    ) -> Result<R, FrontendControlError<E>> {
        if self.stopped {
            return Err(FrontendControlError::Stopped);
        }
        let result = (|| {
            let delegated = draw::prepare_row(
                &self.config,
                &mut self.camera,
                &mut self.state,
                callback,
                arguments,
            )?;
            draw(delegated).map_err(FrontendControlError::Host)
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    pub fn input<H: FrontendHost>(
        &mut self,
        input: FrontendInput,
        visibility: MenuVisibility,
        host: &mut H,
    ) -> Result<FrontendInputOutcome, FrontendControlError<H::Error>> {
        if self.stopped {
            return Err(FrontendControlError::Stopped);
        }
        let result = self.input_inner(input, visibility, host);
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    fn input_inner<H: FrontendHost>(
        &mut self,
        input: FrontendInput,
        visibility: MenuVisibility,
        host: &mut H,
    ) -> Result<FrontendInputOutcome, FrontendControlError<H::Error>> {
        if input == FrontendInput::RequestAttract {
            self.state.idle_millis = 60001;
            return Ok(FrontendInputOutcome::AttractArmed);
        }
        if input == FrontendInput::Back {
            host.clear_held_input()
                .map_err(FrontendControlError::Host)?;
            let outcome = if self.camera.ready() {
                if let Some(menu) = self
                    .menus
                    .current_mut()
                    .filter(|menu| menu.navigation_locked())
                {
                    menu.set_navigation_locked(false);
                    FrontendInputOutcome::CancelledEditing
                } else {
                    host.persist_settings()
                        .map_err(FrontendControlError::Host)?;
                    self.menus.pop(1);
                    if self.menus.is_empty() {
                        let data = host
                            .menu_data(QUIT_MENU_ADDRESS)
                            .map_err(FrontendControlError::Host)?;
                        self.menus.push(MenuState::new(data, MenuBasis::INITIAL));
                    }
                    FrontendInputOutcome::PoppedMenu
                }
            } else {
                self.accelerate_returning_camera();
                FrontendInputOutcome::CameraBlocked
            };
            self.state.idle_millis = 0;
            return Ok(outcome);
        }
        // The screen's +28..+34 (004851E0, 00485210, 00485240, 00485270) send
        // commands 0 to 3 to the controller +EC (004537E0) and Confirm's
        // +24 (004850C0) Activate, each once the camera agrees (00483750),
        // then play the cue (004391D0).
        let (command, sound, gated) = match input {
            FrontendInput::Previous => (MenuCommand::Previous, Some(2), true),
            FrontendInput::Next => (MenuCommand::Next, Some(2), true),
            FrontendInput::AdjustPrevious => (MenuCommand::AdjustPrevious, Some(3), true),
            FrontendInput::AdjustNext => (MenuCommand::AdjustNext, Some(3), true),
            FrontendInput::Confirm => (MenuCommand::Activate, Some(5), true),
            FrontendInput::Auxiliary6 => (MenuCommand::Auxiliary6, None, false),
            FrontendInput::Auxiliary7 => (MenuCommand::Auxiliary7, None, false),
            FrontendInput::Back | FrontendInput::RequestAttract => unreachable!(),
        };
        let outcome = if gated && !self.camera.ready() {
            if input == FrontendInput::Confirm {
                self.accelerate_returning_camera();
            }
            FrontendInputOutcome::CameraBlocked
        } else {
            let mut adapter = ActionAdapter {
                config: &self.config,
                camera: &mut self.camera,
                state: &mut self.state,
                host,
            };
            let outcome = self
                .menus
                .command(
                    MenuInput {
                        command,
                        visibility,
                        context: self.config.context,
                    },
                    &mut adapter,
                )
                .map_err(flatten_menu_error)?;
            if let Some(resource) = sound {
                adapter
                    .host
                    .play_sound(resource)
                    .map_err(FrontendControlError::Host)?;
            }
            FrontendInputOutcome::Menu(outcome)
        };
        // 004852A0/C0 do not reset the idle timer; all five gated methods do.
        if gated {
            self.state.idle_millis = 0;
        }
        Ok(outcome)
    }

    fn accelerate_returning_camera(&mut self) {
        if self.state.returning.get() {
            self.state.accelerated = true;
        }
    }

    pub fn tick<H: FrontendHost>(
        &mut self,
        tick: SchedulerTick,
        host: &mut H,
    ) -> Result<(), FrontendControlError<H::Error>> {
        if self.stopped {
            return Err(FrontendControlError::Stopped);
        }
        if self.ticking {
            self.stopped = true;
            return Err(FrontendControlError::ReentrantTick);
        }
        self.ticking = true;
        let result = self.tick_inner(tick, host);
        self.ticking = false;
        if result.is_err() {
            self.stopped = true;
        }
        result
    }

    fn tick_inner<H: FrontendHost>(
        &mut self,
        tick: SchedulerTick,
        host: &mut H,
    ) -> Result<(), FrontendControlError<H::Error>> {
        let mut animation = self.animation.get();
        animation.advance(tick.delta_micros as u32);
        self.animation.set(animation);
        host.tick_base_input(self, tick)
            .map_err(FrontendControlError::Host)?;
        if self.stopped {
            return Err(FrontendControlError::Stopped);
        }
        host.service_music().map_err(FrontendControlError::Host)?;
        let delta_millis = tick.delta_micros / 1000;
        if self.state.timer_0120_millis != 0 {
            self.state.timer_0120_millis = self.state.timer_0120_millis.wrapping_add(delta_millis);
        }
        if self.camera.ready() {
            let mut adapter = ActionAdapter {
                config: &self.config,
                camera: &mut self.camera,
                state: &mut self.state,
                host,
            };
            self.menus
                .tick(tick.delta_micros, &mut adapter)
                .map_err(flatten_menu_error)?;
        } else {
            self.menus.reset_current_ages();
        }
        if self.state.persist_delay_millis != 0 {
            if self.state.persist_delay_millis > 1000 {
                self.state.persist_delay_millis = 0;
                host.persist_settings()
                    .map_err(FrontendControlError::Host)?;
                if host
                    .profile_first_byte_nonzero()
                    .map_err(FrontendControlError::Host)?
                {
                    self.menus.pop(1);
                } else {
                    self.state.close_delay_millis = 3000;
                }
            } else {
                self.state.persist_delay_millis =
                    self.state.persist_delay_millis.wrapping_add(delta_millis);
            }
        }
        if self.state.close_delay_millis != 0 {
            self.state.close_delay_millis =
                self.state.close_delay_millis.wrapping_sub(delta_millis);
            if self.state.close_delay_millis < 1 {
                self.state.close_delay_millis = 0;
                self.menus.pop(1);
            }
        }
        if self.state.start_pending {
            host.open_start_menu(&mut self.menus)
                .map_err(FrontendControlError::Host)?;
            self.state.start_pending = false;
        }
        if self.menus.len() == 1 {
            self.state.idle_millis = self
                .state
                .idle_millis
                .wrapping_add(tick.elapsed_micros / 1000);
            let attract = self.state.idle_millis > 60000;
            host.set_attract_flag(attract)
                .map_err(FrontendControlError::Host)?;
            if attract {
                host.request_phase(9).map_err(FrontendControlError::Host)?;
            }
        } else {
            self.state.idle_millis = 0;
        }
        let motion = match (self.state.returning.get(), self.state.accelerated) {
            (false, _) => CameraMotion::FirstEntry,
            (true, false) => CameraMotion::Returning,
            (true, true) => CameraMotion::Accelerated,
        };
        self.camera
            .advance(tick.delta_micros, motion, self.config.motion_scale);
        Ok(())
    }
}

fn select_camera<E>(
    config: &FrontendControlConfig,
    camera: &mut FrontendCamera,
    state: &mut FrontendState,
    index: u32,
) -> Result<(), FrontendControlError<E>> {
    let preset =
        config
            .camera_presets
            .get(index as usize)
            .ok_or(FrontendControlError::CameraPreset {
                index,
                count: config.camera_presets.len(),
            })?;
    if camera
        .select_preset(index, preset, config.position_scale)
        .restart_timer_0120
    {
        state.timer_0120_millis = 1;
    }
    Ok(())
}

fn flatten_menu_error<E>(error: MenuError<FrontendControlError<E>>) -> FrontendControlError<E> {
    match error {
        MenuError::State(error) => FrontendControlError::Menu(error),
        MenuError::ProfileStore(error) => FrontendControlError::ProfileStore(error),
        MenuError::Host(error) => error,
    }
}

struct ActionAdapter<'a, H> {
    config: &'a FrontendControlConfig,
    camera: &'a mut FrontendCamera,
    state: &'a mut FrontendState,
    host: &'a mut H,
}

impl<H: FrontendHost> MenuHost for ActionAdapter<'_, H> {
    type Error = FrontendControlError<H::Error>;

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.host
            .rotate_menu_basis(basis, angle)
            .map_err(FrontendControlError::Host)
    }

    fn tick_profile(
        &mut self,
        profile: &mut ProfileMenu,
        delta_micros: i32,
        active: &mut bool,
    ) -> Result<(), MenuError<Self::Error>> {
        self.host
            .tick_profile(
                profile,
                delta_micros,
                &mut FrontendProfileContext {
                    config: self.config,
                    camera: self.camera,
                    state: self.state,
                    active,
                },
            )
            .map_err(MenuError::Host)
    }

    fn tick_network(
        &mut self,
        network: &mut NetworkMenu,
        delta_micros: i32,
    ) -> Result<(), MenuError<Self::Error>> {
        self.host
            .tick_network(network, delta_micros)
            .map_err(FrontendControlError::Host)
            .map_err(MenuError::Host)
    }

    fn action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        let network_row = matches!(stack.current_owner(), Some(MenuOwner::Network(_)))
            || matches!(
                request.callback_address,
                crate::network_menu::ARENA_ACTION
                    | crate::network_menu::VALUE_ACTION
                    | crate::network_menu::LIMIT_ACTION
                    | crate::network_menu::START_ACTION
            );
        if network_row {
            return self
                .host
                .network_action(stack, request)
                .map_err(FrontendControlError::Host);
        }
        if let Some((MenuOwner::Profile(profile), active)) = stack.current_owner_and_active_mut() {
            return self.host.profile_action(
                profile,
                request,
                &mut FrontendProfileContext {
                    config: self.config,
                    camera: self.camera,
                    state: self.state,
                    active,
                },
            );
        }
        let activate = request.command == MenuCommand::Activate;
        match request.callback_address {
            0x0048_3490 => {
                if activate {
                    self.state.start_pending = true;
                }
            }
            0x0048_34b0 | 0x0045_3be0 => {
                if activate {
                    if request.callback_address == 0x0048_34b0 {
                        select_camera(self.config, self.camera, self.state, 2)?;
                    }
                    let data = self
                        .host
                        .menu_data(request.arguments[1])
                        .map_err(FrontendControlError::Host)?;
                    stack.push(MenuState::new(data, MenuBasis::INITIAL));
                }
            }
            0x0045_3b80 => {
                self.host
                    .adjust_setting(request.arguments[1], request.command, request.arguments[0])
                    .map_err(FrontendControlError::Host)?;
            }
            0x0046_3b50 => {
                self.host
                    .adjust_setting(request.arguments[1], request.command, request.arguments[0])
                    .map_err(FrontendControlError::Host)?;
                self.host
                    .refresh_settings_world()
                    .map_err(FrontendControlError::Host)?;
            }
            0x0046_3ac0 | 0x0046_3b00 => {
                self.host
                    .adjust_setting(request.arguments[1], request.command, request.arguments[0])
                    .map_err(FrontendControlError::Host)?;
                let channel = if request.callback_address == 0x0046_3ac0 {
                    VolumeChannel::Effects
                } else {
                    VolumeChannel::Music
                };
                self.host
                    .apply_volume(channel)
                    .map_err(FrontendControlError::Host)?;
            }
            0x0046_2f40 => {
                self.host
                    .adjust_display_setting(
                        request.arguments[1],
                        request.command,
                        request.arguments[0],
                    )
                    .map_err(FrontendControlError::Host)?;
            }
            0x0046_3b70 => {
                self.host
                    .restore_default_settings()
                    .map_err(FrontendControlError::Host)?;
            }
            0x0045_3c50 => {
                if activate {
                    stack.pop(1);
                }
            }
            0x0045_3c70 => {
                if activate {
                    self.host
                        .request_phase(request.arguments[0])
                        .map_err(FrontendControlError::Host)?;
                }
            }
            0x0045_fd40 => {
                if activate {
                    self.host
                        .open_network_menu(stack)
                        .map_err(FrontendControlError::Host)?;
                }
            }
            0x0048_3620 | 0x0048_36c0 => {
                let yes_row = request.callback_address == 0x0048_36c0;
                let exit = request.command == MenuCommand::Auxiliary6 || (yes_row && activate);
                let restore = request.command == MenuCommand::Auxiliary7 || (!yes_row && activate);
                if exit {
                    self.host
                        .request_exit()
                        .map_err(FrontendControlError::Host)?;
                } else if restore {
                    let data = self
                        .host
                        .menu_data(ROOT_MENU_ADDRESS)
                        .map_err(FrontendControlError::Host)?;
                    stack.replace(MenuState::new(data, MenuBasis::INITIAL));
                }
            }
            _ => self
                .host
                .external_action(request, stack)
                .map_err(FrontendControlError::Host)?,
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "frontend_control_tests.rs"]
mod tests;
