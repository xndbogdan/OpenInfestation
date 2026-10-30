//! Session custody of the phase-0 startup (language) screen.
//!
//! The Options Language row requests phase 0. The transition creates the
//! screen through `00433810`; its constructor's phase-1 request is dropped
//! while that transition is pending, so the carousel stays until an accept
//! requests phase 1. The title factory then finishes at once (process byte
//! `005468D8`) or on its first tick, and phase 2 rebuilds the frontend.

use thiserror::Error;

use super::{PortPlatform, PortScreen, PortSession, PortSessionError, PortSessionErrorOf};
use crate::frontend_display::resident_material_word;
use crate::frontend_screen::FrontendFactoryHost;
use crate::frontend_settings::{FrontendSettings, FrontendSettingsError};
use crate::input::InputRuntime;
use crate::render_resources::RenderResources;
use crate::residency::ResourceSelection;
use crate::session::{SessionPhase, SessionState};
use crate::startup_screen::{
    StartupConstruction, StartupEffects, StartupFactoryHost, StartupFactoryServices, StartupScreen,
    StartupScreenConfig, StartupState, STARTUP_CALLBACK_CONTEXT, STARTUP_CLEAR_MATERIAL,
};
use crate::{ResidencyRequest, ResidentRuntime};

/// Settings `+64` and session byte `+65C`, read by the constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupSettings {
    pub language: u32,
    pub loaded: bool,
}

impl StartupSettings {
    pub fn from_settings(settings: &FrontendSettings) -> Self {
        Self {
            language: settings.language(),
            loaded: settings.loaded_at_startup(),
        }
    }
}

/// The constructor's effects over the session platform. The phase-1 request
/// reaches `00438380`, which the creating transition's pending flag drops.
struct StartupFactoryAdapter<'a, P> {
    platform: &'a mut P,
    state: &'a mut SessionState,
}

impl<P: FrontendFactoryHost> StartupFactoryHost for StartupFactoryAdapter<'_, P> {
    type Error = P::Error;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.platform.pause_music()
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<crate::frontend_bindings::FrontendDevice>, Self::Error> {
        self.platform.enumerate_calibrated_devices(input)
    }
    fn request_title(&mut self) {
        let _ = self.state.request_phase(SessionPhase::Title);
    }
}

#[derive(Debug, Error)]
pub enum StartupEffectError {
    #[error(transparent)]
    Settings(#[from] FrontendSettingsError),
    #[error("004EE160 language reselect: {0}")]
    Residency(#[from] crate::Error),
}

/// Accept effects over the actual owners: the settings words and sidecar,
/// the live residency and the session's first-request-wins phase request.
pub struct StartupSessionEffects<'a> {
    pub settings: &'a mut FrontendSettings,
    pub residency: &'a mut ResidentRuntime,
    pub state: &'a mut SessionState,
}

impl StartupEffects for StartupSessionEffects<'_> {
    type Error = StartupEffectError;
    fn settings_language(&self) -> u32 {
        self.settings.language()
    }
    fn settings_loaded(&self) -> bool {
        self.settings.loaded_at_startup()
    }
    fn select_language(&mut self, language: u32) -> Result<(), Self::Error> {
        // 00433796..0043379E: +0C, signed, clamped to 4 for the reselect.
        let variant = self.settings.words()[0x0c / 4] as i32;
        let difficulty = if variant < 5 { variant } else { 4 };
        self.settings.set_language(language);
        self.residency.apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: difficulty as u8,
                language: language as u8,
            },
        })?;
        Ok(())
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        Ok(self.settings.persist()?)
    }
    fn request_title(&mut self) {
        let _ = self.state.request_phase(SessionPhase::Title);
    }
    fn tick_quantum(&self) -> Option<u32> {
        self.settings
            .fixes()
            .exact_frontend_ticks
            .then_some(crate::frontend_settings::EXACT_WORLD_TICK_MICROS)
    }
    fn report_accept_failure(&mut self, error: Self::Error) {
        eprintln!(
            "Infestation: language accept caught {error}; requesting phase 1 as 00433730 does"
        );
    }
}

impl<P> PortSession<P> {
    pub fn startup_screen(&self) -> Option<&StartupScreen> {
        self.startup.as_ref()
    }

    /// The words the startup draw rewrites (`+1A4/+1A8`).
    pub(crate) fn startup_state_mut(&mut self) -> Option<&mut StartupState> {
        self.startup.as_mut().map(StartupScreen::state_mut)
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// Factory `00433810`. The H7 slot `0x20` word comes from the residency
    /// current before the screen requests leaf 2, as `004325F6` reads it.
    pub(super) fn construct_startup(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        let settings = self.platform.startup_settings();
        let clear_material_word = resident_material_word(
            RenderResources::new(&self.residency),
            STARTUP_CLEAR_MATERIAL,
        )
        .map_err(|error| PortSessionError::Startup(error.to_string()))?;
        let mut screen = StartupScreen::new(
            StartupScreenConfig {
                attachment: self.attachment,
                callback_context: STARTUP_CALLBACK_CONTEXT,
            },
            self.catalog.assets(),
        );
        let result = screen.initialize(
            &mut StartupFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets: self.catalog.assets(),
                residency: &mut self.residency,
                process: &mut self.startup_process,
            },
            StartupConstruction {
                settings_language: settings.language,
                clear_material_word,
                settings_loaded: settings.loaded,
            },
            &mut StartupFactoryAdapter {
                platform: &mut self.platform,
                state,
            },
        );
        // A failed factory keeps its partial acquisitions for explicit
        // teardown, as the construction contract requires.
        self.startup = Some(screen);
        result.map_err(|error| PortSessionError::Startup(error.to_string()))
    }

    pub(super) fn destroy_startup(&mut self) -> Result<(), PortSessionErrorOf<P>> {
        let mut owner = self
            .startup
            .take()
            .ok_or(PortSessionError::MissingScreen(PortScreen::Startup))?;
        owner
            .destroy(&mut self.registry, &mut self.input)
            .map_err(|error| PortSessionError::Startup(error.to_string()))
    }
}
