//! Session custody of the phase-12 key configuration screen.
//!
//! Options > Controls row 3 (`00453C70(0x0C)`) requests phase 12. The screen
//! returns to the phase it was built from (`+12C`); the rebuilt frontend's
//! `00463100` then persists the edited bindings with the settings.

use super::draw::PortLinkedDrawError;
use super::{PortPlatform, PortScreen, PortSession, PortSessionError, PortSessionErrorOf};
use crate::frontend_renderer::FrontendRenderContext;
use crate::frontend_screen::{FrontendFactoryHost, FrontendProcessState};
use crate::frontend_settings::FrontendSettings;
use crate::input::InputContextId;
use crate::input::InputRuntime;
use crate::key_bindings::{KeyBindingTables, KeyBindings};
use crate::key_config_screen::{
    KeyConfigEffects, KeyConfigFactoryHost, KeyConfigFactoryServices, KeyConfigScreen,
    KeyConfigScreenConfig, KEY_CONFIG_CALLBACK_CONTEXT,
};
use crate::menu::MenuVisibility;
use crate::menu_basis::MenuBasisMath;
use crate::screen_registry::{RegistryCallbackError, ScreenAttachment, ScreenRegistry};
use crate::session::{SessionPhase, SessionState};

struct KeyConfigFactoryAdapter<'a, P> {
    platform: &'a mut P,
}

impl<P: FrontendFactoryHost> KeyConfigFactoryHost for KeyConfigFactoryAdapter<'_, P> {
    type Error = P::Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<crate::frontend_bindings::FrontendDevice>, Self::Error> {
        self.platform.enumerate_calibrated_devices(input)
    }
}

/// Tick and callback effects over the actual owners: the settings' binding
/// records, the session's first-request-wins phase request, the shared
/// animation clock, the gameplay owner byte and the menu basis arithmetic.
pub struct KeyConfigSessionEffects<'a> {
    pub settings: &'a mut FrontendSettings,
    pub state: &'a mut SessionState,
    pub process: &'a FrontendProcessState,
    pub owner_byte: bool,
    pub basis: &'a MenuBasisMath,
}

impl KeyConfigEffects for KeyConfigSessionEffects<'_> {
    fn bindings(&mut self) -> (&KeyBindingTables, &mut KeyBindings) {
        self.settings.key_bindings_mut()
    }
    fn request_phase(&mut self, phase: SessionPhase) {
        let _ = self.state.request_phase(phase);
    }
    fn advance_animation(&mut self, delta_micros: u32) {
        self.process.advance_animation(delta_micros);
    }
    fn visibility(&self) -> MenuVisibility {
        MenuVisibility::from_owner_flag(self.owner_byte)
    }
    fn basis(&self) -> &MenuBasisMath {
        self.basis
    }
}

/// Disjoint borrows for the key configuration's +08 draw `004450B0`.
pub struct PortKeyConfigDraw<'a, P> {
    pub screen: &'a mut KeyConfigScreen,
    pub registry: &'a mut ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub process: &'a FrontendProcessState,
    pub owner_byte: bool,
    pub platform: &'a mut P,
}

impl<P> PortSession<P> {
    pub fn key_config_screen(&self) -> Option<&KeyConfigScreen> {
        self.key_config.as_ref()
    }

    /// Dispatch the key configuration draw under its context guard.
    pub fn with_key_config_draw<T, E>(
        &mut self,
        callback: impl FnOnce(PortKeyConfigDraw<'_, P>) -> Result<T, E>,
    ) -> Result<Option<T>, PortLinkedDrawError<E>> {
        let Some(screen) = self.key_config.as_mut() else {
            return Ok(None);
        };
        let attachment = self.attachment;
        let process = &self.process;
        let owner_byte = self.owner_byte;
        let platform = &mut self.platform;
        let result = self
            .registry
            .with_context_dispatch(attachment.context_owner, |registry| {
                let actual = registry.linked_screen(attachment.context_owner)?;
                let expected = screen.id();
                if expected.is_none() || expected != actual {
                    return Err(PortLinkedDrawError::LinkedScreen { expected, actual });
                }
                callback(PortKeyConfigDraw {
                    screen,
                    registry,
                    attachment,
                    process,
                    owner_byte,
                    platform,
                })
                .map(Some)
                .map_err(PortLinkedDrawError::Callback)
            });
        match result {
            Ok(value) => Ok(value),
            Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// Factory `00444AC0`.
    pub(super) fn construct_key_config(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        let mut screen = KeyConfigScreen::new(
            KeyConfigScreenConfig {
                attachment: self.attachment,
                callback_context: KEY_CONFIG_CALLBACK_CONTEXT,
            },
            self.catalog.assets(),
        );
        let result = screen.initialize(
            &mut KeyConfigFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets: self.catalog.assets(),
            },
            &mut KeyConfigFactoryAdapter {
                platform: &mut self.platform,
            },
            state,
        );
        // A failed factory keeps its partial acquisitions for explicit
        // teardown, as the construction contract requires.
        self.key_config = Some(screen);
        result.map_err(|error| PortSessionError::KeyConfig(error.to_string()))
    }

    pub(super) fn destroy_key_config(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        let mut owner = self
            .key_config
            .take()
            .ok_or(PortSessionError::MissingScreen(PortScreen::KeyConfig))?;
        owner
            .destroy(&mut self.registry, &mut self.input)
            .map_err(|error| PortSessionError::KeyConfig(error.to_string()))
    }
}
