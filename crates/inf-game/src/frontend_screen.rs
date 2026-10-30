//! 004838A0 frontend construction and 00483BD0/00438080 destruction (through
//! the scalar deleting destructor 00483BB0).
//!
//! Composes the actual resource, registry, input and menu owners. Platform
//! device enumeration/calibration, settings persistence and music transport
//! remain required effects. Construction is deliberately not transactional:
//! completed audio, residency, input and global publications survive errors.

use std::{cell::Cell, convert::Infallible, rc::Rc};

use thiserror::Error;

use crate::frontend_assets::{FrontendAssetError, FrontendAssets, FrontendMenuCatalog};
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::frontend_camera::{AnimationClock, AnimationClockSource, FrontendEntry};
use crate::frontend_control::{
    FrontendControl, FrontendControlConfig, FrontendControlError, ROOT_MENU_ADDRESS,
};
use crate::frontend_dispatch::{FrontendDispatch, FrontendDispatchError, FrontendEffects};
use crate::frontend_frame::{
    draw_frontend_frame, FrontendFrameError, FrontendFrameHost, FrontendFrameInput,
    FrontendFrameOutcome,
};
use crate::frontend_scene::FrontendScene;
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::menu::{
    MenuBasis, MenuColor, MenuDrawColors, MenuDrawHost, MenuDrawInput, MenuDrawOutcome, MenuState,
    MenuVisibility,
};
use crate::scheduler::SchedulerTick;
use crate::screen_input::{ScreenDispatchPolicy, ScreenInput};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::{HandlerResources, ResidencyRequest, ResidentRuntime};

/// Native process globals, shared across frontend instances and row callbacks.
/// Cell permits a render host to read live colors while a screen publishes its
/// returning bit. No platform pointers or thread-sharing assumptions are used.
#[derive(Debug)]
pub struct FrontendProcessState {
    returning: Rc<Cell<bool>>,
    animation: Rc<Cell<AnimationClock>>,
    colors: Cell<Option<MenuDrawColors>>,
    tint: Cell<Option<MenuColor>>,
    selector_one: Cell<u16>,
}

/// Word `00533760`'s `.data` value.
const INITIAL_SELECTOR_ONE: u16 = 1;

impl Default for FrontendProcessState {
    fn default() -> Self {
        Self {
            returning: Rc::new(Cell::new(false)),
            animation: Rc::new(Cell::new(AnimationClock::default())),
            colors: Cell::new(None),
            tint: Cell::new(None),
            selector_one: Cell::new(INITIAL_SELECTOR_ONE),
        }
    }
}

impl AnimationClockSource for FrontendProcessState {
    fn animation_value(&self) -> u32 {
        self.animation().value()
    }
    fn animation_micros(&self) -> u64 {
        self.animation().accumulated_micros()
    }
}

impl FrontendProcessState {
    pub fn entry(&self) -> FrontendEntry {
        if self.returning.get() {
            FrontendEntry::Returning
        } else {
            FrontendEntry::First
        }
    }
    pub fn colors(&self) -> Option<MenuDrawColors> {
        self.colors.get()
    }
    pub fn animation(&self) -> AnimationClock {
        self.animation.get()
    }
    /// 0043A370 writes RGB 128 while the high byte is transferred unchanged
    /// from the factory's raw context argument. It is not established alpha.
    pub fn tint(&self) -> Option<MenuColor> {
        self.tint.get()
    }
    pub fn publish_colors(&self, colors: MenuDrawColors) {
        self.colors.set(Some(colors));
    }
    /// Word `00533760`, which object callback `0043A270` returns for
    /// selector 1: the vehicle form a model of the ACAM draws. It starts at
    /// 1; the shop's preview (`0042FD70`) and the message's model notice
    /// (`00414DD0`, not ported) write the previewed form.
    pub fn object_selector_one(&self) -> u16 {
        self.selector_one.get()
    }
    pub fn set_object_selector_one(&self, value: u16) {
        self.selector_one.set(value);
    }
    /// 00439530: reset the shared animation clock 005471C0 and its
    /// accumulator. Screens other than the frontend call it directly.
    pub fn reset_animation(&self) {
        self.animation.set(AnimationClock::default());
    }
    /// 00439560 with the caller's capped delta, as the text-entry tick does.
    pub fn advance_animation(&self, delta_micros: u32) {
        let mut clock = self.animation.get();
        clock.advance(delta_micros);
        self.animation.set(clock);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrontendScreenConfig {
    pub attachment: ScreenAttachment,
    /// The logical screen callback identity, distinct from context_owner.
    pub callback_context: u32,
    /// Provenance for 0043A370's untouched fourth byte. None for a portable
    /// allocation with no native address receipt; RGB is still fully known.
    pub native_context_high_byte: Option<u8>,
}

#[derive(Debug, Error)]
pub enum FrontendScreenError<E> {
    #[error("frontend screen requires {expected:?}, currently {actual:?}")]
    Phase {
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
    Assets(#[from] FrontendAssetError),
    #[error(transparent)]
    Residency(#[from] crate::Error),
    #[error("frontend controller initialization failed: {0}")]
    Control(FrontendControlError<Infallible>),
    #[error("required Handler-6 color slot {0} is unavailable or is not a word resource")]
    ColorResource(u16),
    #[error("required frontend factory effect failed")]
    Host(E),
    #[error(transparent)]
    Dispatch(#[from] FrontendDispatchError<E>),
}

/// Required effects have no successful defaults. An unavailable adapter must
/// fail; an empty eligible-device list is valid only after real enumeration.
pub trait FrontendFactoryHost {
    type Error;
    fn pause_music(&mut self) -> Result<(), Self::Error>;
    fn resume_music(&mut self) -> Result<(), Self::Error>;
    /// Enumerate in native order, apply +10/+14 eligibility and 00439AD0 axis
    /// calibration, retaining process-owned device adapters in InputRuntime.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
    fn persist_settings(&mut self) -> Result<(), Self::Error>;
}

pub struct FrontendFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub residency: &'a mut ResidentRuntime,
    pub catalog: &'a mut FrontendMenuCatalog,
    pub process: &'a FrontendProcessState,
}

pub struct FrontendScreenDraw<'a> {
    pub scene: &'a FrontendScene,
    pub visibility: MenuVisibility,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Debug)]
pub struct FrontendScreen {
    config: FrontendScreenConfig,
    phase: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    input: ScreenInput,
    control: Option<FrontendControl>,
    construction_completed: bool,
}

impl FrontendScreen {
    /// Metadata only; initialize performs ordered native acquisitions.
    pub fn new(config: FrontendScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            input: ScreenInput::new(ScreenDispatchPolicy::Frontend, config.callback_context),
            config,
            phase: ScreenLifecycle::New,
            id: None,
            control: None,
            construction_completed: false,
        }
    }

    pub fn phase(&self) -> ScreenLifecycle {
        self.phase
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
    pub fn control(&self) -> Option<&FrontendControl> {
        self.control.as_ref()
    }

    /// While the network address prompt shows (DirectPlay's TCP/IP dialog),
    /// its typed keys go to it instead of the bindings.
    pub(crate) fn take_typed_keys(
        &mut self,
        notifications: &mut Vec<crate::platform_input::InputNotification>,
    ) {
        let Some(control) = self.control.as_mut() else {
            return;
        };
        let Some(crate::menu::MenuOwner::Network(menu)) = control.menus_mut().current_owner_mut()
        else {
            return;
        };
        if !menu.is_address_prompt() {
            return;
        }
        notifications.retain(|notification| match notification {
            crate::platform_input::InputNotification::Key { scancode, pressed } => {
                !menu.key(scancode.name(), *pressed)
            }
            _ => true,
        });
    }

    /// The port's labelled demo deferral only operates on a live Ready screen.
    pub(crate) fn reset_idle_after_attract_deferral(&mut self) -> bool {
        if self.phase != ScreenLifecycle::Ready {
            return false;
        }
        let Some(control) = self.control.as_mut() else {
            return false;
        };
        control.reset_idle_after_attract_deferral();
        true
    }

    pub fn tick<E: FrontendEffects>(
        &mut self,
        input: &mut InputRuntime,
        tick: SchedulerTick,
        dispatch: &mut FrontendDispatch<'_, E>,
    ) -> Result<(), FrontendScreenError<E::Error>> {
        self.require_phase(ScreenLifecycle::Ready)?;
        let analog = match self.bindings.analog_values(input) {
            Ok(values) => values,
            Err(error) => {
                self.phase = ScreenLifecycle::Stopped;
                return Err(error.into());
            }
        };
        let control = self.control.as_mut().expect("ready controller");
        let result = dispatch.tick(control, &mut self.input, input, analog, tick);
        if result.is_err() {
            self.phase = ScreenLifecycle::Stopped;
        }
        result.map_err(FrontendScreenError::Dispatch)
    }

    /// Route only direct screen callbacks here. Session/device callbacks need
    /// their separate owners; unknown addresses and wrong contexts fail before
    /// mutation without preventing a later correctly routed event.
    pub fn digital_callback<E: FrontendEffects>(
        &mut self,
        event: InputCallback,
        dispatch: &mut FrontendDispatch<'_, E>,
    ) -> Result<(), FrontendScreenError<E::Error>> {
        self.require_phase(ScreenLifecycle::Ready)?;
        let control = self.control.as_mut().expect("ready controller");
        let result = dispatch.digital_callback(control, &mut self.input, event);
        if self.input.stopped() || control.stopped() {
            self.phase = ScreenLifecycle::Stopped;
        }
        result.map_err(FrontendScreenError::Dispatch)
    }

    pub fn initialize<D, H: FrontendFactoryHost>(
        &mut self,
        services: &mut FrontendFactoryServices<'_, D>,
        host: &mut H,
    ) -> Result<(), FrontendScreenError<H::Error>> {
        self.require_phase(ScreenLifecycle::New)?;
        self.phase = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        let assets = services.catalog.assets();
        self.control = Some(
            FrontendControl::new(FrontendControlConfig {
                camera_presets: assets.camera_presets().to_vec(),
                position_scale: assets.position_scale(),
                motion_scale: assets.motion_scale(),
                entry: services.process.entry(),
                context: self.config.callback_context,
            })
            .map_err(FrontendScreenError::Control)?,
        );
        self.control
            .as_mut()
            .expect("constructed controller")
            .share_process_state(
                Rc::clone(&services.process.returning),
                Rc::clone(&services.process.animation),
            );

        host.pause_music().map_err(FrontendScreenError::Host)?;
        services
            .residency
            .apply(ResidencyRequest::Activate { level: 47 })?;
        host.resume_music().map_err(FrontendScreenError::Host)?;
        // Lookups at 004839B6..004839FD, then 00453E10(word3, word2, word2).
        let color = |slot| {
            resident_menu_color(services.residency, slot)
                .ok_or(FrontendScreenError::ColorResource(slot))
        };
        let selected_end = color(2)?;
        let ordinary = color(2)?;
        let selected_start = color(3)?;
        services.process.publish_colors(MenuDrawColors {
            selected_start,
            ordinary,
            selected_end,
        });
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(FrontendScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.catalog.assets().frontend_input_tables(),
            &devices,
        )?;
        let context = self.bindings.context().expect("installed frontend context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => FrontendScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => FrontendScreenError::Input(error),
            })?;
        let root = services.catalog.menu_data(ROOT_MENU_ADDRESS)?;
        let control = self.control.as_mut().expect("constructed controller");
        control
            .menus_mut()
            .push(MenuState::new(root, MenuBasis::INITIAL));
        host.persist_settings().map_err(FrontendScreenError::Host)?;
        control.reset_animation();
        services.process.tint.set(Some(MenuColor {
            rgb: [128; 3],
            high_byte: self.config.native_context_high_byte,
        }));
        self.construction_completed = true;
        self.phase = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Native ordered teardown, also usable for retained partial acquisitions.
    /// Failed-constructor cleanup is Rust containment, not retail SEH/longjmp
    /// reconstruction. It does not publish the fully constructed destructor's
    /// returning flag (native base/controller unwind: 0051DE00/0051DE08).
    /// Failed cleanup keeps remaining handles; it never restores residency,
    /// music, global colors, queued targets, or the stale context-owner link.
    pub fn destroy<D>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), FrontendScreenError<Infallible>> {
        if self.phase == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.phase = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        if let Some(control) = &self.control {
            if self.construction_completed {
                control.publish_returning();
            }
            self.control = None;
        }
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
        self.phase = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Complete menu drawing through this owner so 0055F14D remains live across
    /// screens. The renderer reads process colors per row through its host.
    pub fn draw_menu<H: MenuDrawHost>(
        &mut self,
        input: MenuDrawInput<'_>,
        host: &mut H,
    ) -> Result<Option<MenuDrawOutcome>, FrontendControlError<H::Error>> {
        if self.phase != ScreenLifecycle::Ready {
            return Err(FrontendControlError::Stopped);
        }
        let control = self.control.as_mut().expect("ready controller");
        let outcome = control.draw_menu(input, host);
        if outcome.is_err() {
            self.phase = ScreenLifecycle::Stopped;
        }
        outcome
    }

    /// Full 00484940 frame, deriving base age and control state from this screen.
    /// Only the proven native 0x301 catch permits continued execution after a
    /// backend error; missing implementations are ordinary stopping errors.
    pub fn draw_frame<H: FrontendFrameHost>(
        &mut self,
        draw: FrontendScreenDraw<'_>,
        host: &mut H,
    ) -> Result<FrontendFrameOutcome, FrontendFrameError<H::Error>> {
        if self.phase != ScreenLifecycle::Ready {
            return Err(FrontendFrameError::Control(FrontendControlError::Stopped));
        }
        let result = draw_frontend_frame(
            self.control.as_mut().expect("ready controller"),
            draw.scene,
            FrontendFrameInput {
                age_millis: self.input.age_millis(),
                visibility: draw.visibility,
                sine_quarter: draw.sine_quarter,
            },
            host,
        );
        if result.is_err() {
            self.phase = ScreenLifecycle::Stopped;
        }
        result
    }

    fn require_phase<E>(&self, expected: ScreenLifecycle) -> Result<(), FrontendScreenError<E>> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(FrontendScreenError::Phase {
                expected,
                actual: self.phase,
            })
        }
    }
}

/// Shared intrinsic H6 lookup; each screen explicitly chooses its style slots.
pub(crate) fn resident_menu_color(runtime: &ResidentRuntime, slot: u16) -> Option<u32> {
    let resource = runtime.resource(6, slot)?;
    let Some((_, HandlerResources::Words(bank))) = resource.overlay.handler(6) else {
        return None;
    };
    let entry = bank
        .entries
        .get(usize::from(resource.registration.local_index))?;
    Some(entry.raw)
}
