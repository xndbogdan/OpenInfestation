//! Linked-screen routing for the scheduled session.
//!
//! One scheduler iteration reaches the session's current screen through the
//! selected-input poll 004F0200 (first pass), linked +04 (second pass) and the
//! +08/+0C draw and presentation callbacks. This module owns the first two
//! and intercepts the effects the session owns: frontend phase requests
//! (00438380), the attract flag 0055F14C, and the text-entry completion's
//! offline game and network flag. Nothing here has a successful default for
//! an unconnected owner.

use thiserror::Error;

use super::{PortScreen, TEXT_ENTRY_CALLBACK_CONTEXT};
use crate::{
    frontend_assets::FrontendMenuCatalog,
    frontend_dispatch::{FrontendDispatch, FrontendEffects},
    frontend_renderer::FrontendRenderContext,
    frontend_screen::{FrontendProcessState, FrontendScreen, FrontendScreenError},
    input::{DeviceId, InputCallback, InputContextId, InputError, InputHost, InputRuntime},
    key_config_screen::{KeyConfigEffects, KeyConfigScreen},
    menu::{MenuActionRequest, MenuCommand, MenuStack, MenuVisibility},
    menu_basis::MenuBasisMath,
    name_entry::{IdentityWriteRequest, NameCompletion, NameEntryActions, NameEntryCompletion},
    offline_prepare::OfflineGame,
    platform_input::{InputNotification, SdlInput},
    process_particles::factories::ParticleFactoryProcessState,
    profile_menu::ProfileOperation,
    profile_store::{ProfileStore, ProfileStoreError},
    scheduler::SchedulerTick,
    screen_registry::{
        RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenRegistry,
    },
    session::{InvalidSessionPhase, SessionPhase, SessionState},
    settings_row::VolumeChannel,
    startup_screen::{StartupEffects, StartupScreen},
    text_entry_screen::{
        TextEntryProcessState, TextEntryScreen, TextEntryScreenError, TextEntryServices,
        TextEntryTick,
    },
    title_screen::{
        TitleScreen, TitleScreenError, TitleSessionHost, TitleTickOutcome, TitleVideoHost,
        TITLE_SKIP_CALLBACK,
    },
};
use inf_formats::{MissionSaveState, PlayerSaveState};
mod intro;
pub(in crate::session_platform) use intro::run_in_world_menu_commands;
mod map;

/// Alt+Esc in both keyboard tables: 00439260 calls 004F4F60 on press, which
/// clears the platform loop's running flag 00544D00. The iteration finishes.
pub const EXIT_CALLBACK: u32 = 0x0043_9260;
/// Alt+F1 in both keyboard tables: 004451D0 is `ret 8`.
pub const NOOP_CALLBACK: u32 = 0x0044_51D0;
/// Alt+Enter in both keyboard tables: 00437D30 toggles settings +40 and
/// resets the display through 00437DF0 on the callback object's +E8 context.
/// Its display owner is not connected here.
pub const FULLSCREEN_TOGGLE_CALLBACK: u32 = 0x0043_7D30;

#[derive(Debug, Error)]
pub enum LinkedScreenError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("the session context links no screen")]
    Unlinked,
    #[error("linked screen {0:?} is not owned by this port session")]
    Foreign(ScreenId),
}

#[derive(Debug, Error)]
pub enum LinkedActivityError {
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error(transparent)]
    Registry(RegistryError),
    #[error(transparent)]
    Input(crate::input::InputStateError),
}

/// Briefing linked-tick failures.
#[derive(Debug, Error)]
pub enum BriefingLinkedError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error("briefing tick failed: {0}")]
    Tick(String),
}

/// Gameplay linked-tick failures.
#[derive(Debug, Error)]
pub enum GameplayLinkedError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error("gameplay tick failed: {0}")]
    Tick(String),
}

/// Session-owned effect failures, beside the wrapped platform effect.
#[derive(Debug, Error)]
pub enum PortEffectError<E> {
    #[error("platform effect failed")]
    Host(E),
    #[error(transparent)]
    Phase(#[from] InvalidSessionPhase),
    #[error("0045AD30 serialization requires the 00487410 offline game, which is not published")]
    MissingOfflineGame,
}

#[derive(Debug, Error)]
pub enum PortLinkedError<F, T> {
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error("linked {screen:?} screen cannot use {services} services")]
    Services {
        screen: PortScreen,
        services: &'static str,
    },
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("input callback {0:#010x}: its owner is not connected")]
    SystemCallback(u32),
    #[error("keyboard notifications arrived without the attached keyboard adapter")]
    MissingKeyboard,
    #[error(transparent)]
    Input(#[from] crate::input::InputStateError),
    #[error(transparent)]
    Frontend(FrontendScreenError<PortEffectError<F>>),
    #[error(transparent)]
    Text(TextEntryScreenError<PortEffectError<T>>),
    #[error("text-entry platform effect failed")]
    TextPlatform(T),
    #[error("frontend platform effect failed")]
    FrontendPlatform(F),
}

/// Title poll/tick/draw failures beside the wrapped platform effect.
#[derive(Debug, Error)]
pub enum TitleLinkedError<H> {
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("keyboard notifications arrived without the attached keyboard adapter")]
    MissingKeyboard,
    #[error(transparent)]
    Input(#[from] crate::input::InputStateError),
    #[error(transparent)]
    Title(TitleScreenError<H>),
    #[error("input callback {0:#010x}: its owner is not connected")]
    SystemCallback(u32),
}

/// Startup poll/tick failures. A caught accept failure is not one of them:
/// the effects report it and the phase-1 request still runs.
#[derive(Debug, Error)]
pub enum StartupLinkedError {
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("keyboard notifications arrived without the attached keyboard adapter")]
    MissingKeyboard,
    #[error(transparent)]
    Input(#[from] crate::input::InputStateError),
    #[error("startup screen: {0}")]
    Startup(String),
    #[error("input callback {0:#010x}: its owner is not connected")]
    SystemCallback(u32),
}

/// Key configuration poll/tick failures.
#[derive(Debug, Error)]
pub enum KeyConfigLinkedError {
    #[error(transparent)]
    Screen(#[from] LinkedScreenError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("keyboard notifications arrived without the attached keyboard adapter")]
    MissingKeyboard,
    #[error(transparent)]
    Input(#[from] crate::input::InputStateError),
    #[error("key configuration screen: {0}")]
    KeyConfig(String),
    #[error("input callback {0:#010x}: its owner is not connected")]
    SystemCallback(u32),
}

/// Process effects the text-entry screen reaches outside the session: the
/// settings sidecar's identity writes (004633D0/00463490, never the registry),
/// the cheat word settings+0x84 (00463AB0 then `or`), and the loop's running
/// flag for Alt+Esc.
pub trait TextEntryPlatform {
    type Error;
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error>;
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error>;
    fn request_exit(&mut self) -> Result<(), Self::Error>;
    /// Port fix, not native: the name entry's effect quantum, None for
    /// retail's every-tick steps.
    fn name_entry_effect_quantum(&self) -> Option<u32> {
        None
    }
}

/// The caller's services for the screen it resolved as linked. A mismatch is
/// rejected before anything runs.
pub enum LinkedServices<'a, E, N> {
    Frontend {
        effects: &'a mut E,
        profiles: &'a mut ProfileStore,
        basis: &'a MenuBasisMath,
    },
    TextEntry {
        platform: &'a mut N,
        profiles: &'a mut ProfileStore,
        /// 0048F060's process word 0055F18C, shared with sound cues.
        rng: &'a mut u32,
        /// Entry 0 of the process adapter cache 005474B0: the keyboard.
        keyboard: Option<DeviceId>,
    },
}

impl<E, N> LinkedServices<'_, E, N> {
    fn name(&self) -> &'static str {
        match self {
            Self::Frontend { .. } => "frontend",
            Self::TextEntry { .. } => "text-entry",
        }
    }
}

/// Physical notifications for one selected-input poll.
pub struct PortInputPass<'a> {
    pub keyboard: Option<&'a mut SdlInput>,
    /// Drained in arrival order before 004F0200 dispatches the queue.
    pub notifications: &'a mut Vec<InputNotification>,
    /// The poll's 004A4890 sample, also used for notification timestamps.
    pub now_micros: u64,
}

/// Disjoint session state lent to one linked callback; see
/// [`super::PortSession::split`].
pub struct PortLinked<'a> {
    pub registry: &'a mut ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub catalog: &'a mut FrontendMenuCatalog,
    pub process: &'a FrontendProcessState,
    pub text_process: &'a mut TextEntryProcessState,
    pub attachment: ScreenAttachment,
    pub startup: Option<&'a mut StartupScreen>,
    pub key_config: Option<&'a mut KeyConfigScreen>,
    pub title: Option<&'a mut TitleScreen>,
    pub frontend: Option<&'a mut FrontendScreen>,
    pub text: Option<&'a mut TextEntryScreen>,
    pub briefing: Option<&'a mut crate::briefing_screen::BriefingScreen>,
    pub gameplay: Option<&'a mut crate::gameplay_screen::GameplayScreen>,
    pub intro_results: Option<&'a mut crate::intro_results_screen::IntroResultsScreen>,
    pub map: Option<&'a mut crate::map_screen::MapScreen>,
    pub attract: &'a mut bool,
    pub factory_process: &'a ParticleFactoryProcessState,
    /// The terrain spacing Alt+F3 reselects (004502C0).
    pub terrain_policy: &'a mut crate::terrain_runtime::ProcessTerrainPolicy,
    pub owner_byte: bool,
    pub offline_game: Option<&'a OfflineGame>,
    /// The CRT name classification the results screen's save formats with.
    pub(crate) profile_name_classification:
        &'a crate::profile_name_format::ProfileNameClassification,
}

impl PortLinked<'_> {
    /// Deliver physical notifications, then run the selected-input poll
    /// 004F0200. Notifications are delivered immediately before the poll, as
    /// the standalone frontend did; the native device-notification trigger
    /// relative to 004943A0 is not established. No keyboard descriptor is
    /// immediate (flag 0x01), so callbacks run only inside the poll.
    pub fn poll<E: FrontendEffects, N: TextEntryPlatform>(
        &mut self,
        state: &mut SessionState,
        pass: PortInputPass<'_>,
        services: LinkedServices<'_, E, N>,
        completions: &mut Vec<NameCompletion>,
    ) -> Result<(), PortLinkedError<E::Error, N::Error>> {
        let screen = self.checked_screen(&services)?;
        if let Some(frontend) = self.frontend.as_deref_mut() {
            frontend.take_typed_keys(pass.notifications);
        }
        let input = &mut *self.input;
        let mut router = Router {
            now_micros: pass.now_micros,
            catalog: &mut *self.catalog,
            process: self.process,
            text_process: &mut *self.text_process,
            frontend: self.frontend.as_deref_mut(),
            text: self.text.as_deref_mut(),
            attract: &mut *self.attract,
            network_active: self.factory_process.network_flag != 0,
            owner_byte: self.owner_byte,
            offline_game: self.offline_game,
            state,
            services,
            completions,
            screen,
        };
        if !pass.notifications.is_empty() {
            let keyboard = pass.keyboard.ok_or(PortLinkedError::MissingKeyboard)?;
            for notification in pass.notifications.drain(..) {
                keyboard
                    .update(notification, input, &mut router)
                    .map_err(flatten_input)?;
            }
        }
        input.poll(&mut router).map_err(flatten_input)
    }

    /// Linked +04 under the context guard: frontend 00484E00 or text-entry
    /// 0045A8F0 with the scheduler's capped/uncapped pair. Text-entry
    /// completions are appended in execution order.
    pub fn tick<E: FrontendEffects, N: TextEntryPlatform>(
        &mut self,
        state: &mut SessionState,
        tick: SchedulerTick,
        services: LinkedServices<'_, E, N>,
        completions: &mut Vec<NameCompletion>,
    ) -> Result<(), PortLinkedError<E::Error, N::Error>> {
        let screen = self.checked_screen(&services)?;
        let owner = self.attachment.context_owner;
        let PortLinked {
            registry,
            input,
            catalog,
            process,
            text_process,
            title,
            frontend,
            text,
            attract,
            factory_process,
            owner_byte,
            offline_game,
            ..
        } = self;
        let result = registry.with_context_dispatch(owner, |registry| {
            require_linked(registry, owner, screen, title, frontend, text)?;
            match services {
                LinkedServices::Frontend {
                    effects,
                    profiles,
                    basis,
                } => {
                    let screen = frontend.as_deref_mut().expect("checked frontend");
                    let mut effects = SessionFrontendEffects {
                        inner: effects,
                        state,
                        attract,
                    };
                    screen
                        .tick(
                            input,
                            tick,
                            &mut FrontendDispatch {
                                catalog,
                                basis,
                                visibility: MenuVisibility::from_owner_flag(*owner_byte),
                                effects: &mut effects,
                                profiles: Some(profiles),
                            },
                        )
                        .map_err(PortLinkedError::Frontend)
                }
                LinkedServices::TextEntry {
                    platform,
                    profiles,
                    rng,
                    keyboard,
                } => {
                    let screen = text.as_deref_mut().expect("checked text entry");
                    let phase = u32::from(state.current_phase().id());
                    let attract = **attract;
                    let poll_micros = input.clock_micros();
                    let mut effects = SessionNameEntryEffects {
                        platform,
                        state,
                        offline_game: *offline_game,
                        network_active: factory_process.network_flag != 0,
                    };
                    let done = screen
                        .tick(
                            TextEntryTick {
                                scheduler: tick,
                                attract,
                            },
                            &mut TextEntryServices {
                                input,
                                store: profiles,
                                key_names: catalog.assets().key_names(),
                                keyboard_adapter: keyboard,
                                text_process,
                                frontend_process: process,
                                rng,
                                effects: &mut effects,
                                phase,
                                poll_micros,
                            },
                        )
                        .map_err(PortLinkedError::Text)?;
                    completions.extend(done);
                    Ok(())
                }
            }
        });
        match result {
            Ok(()) => Ok(()),
            Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }

    /// Title linked +04 `00404810` under the context guard: while the video
    /// plays this requests nothing; when the handle is absent or finished it
    /// runs `004049C0` and requests phase 2 through the session state.
    pub fn tick_title<V: TitleVideoHost>(
        &mut self,
        state: &mut SessionState,
        _tick: SchedulerTick,
        video: &mut V,
    ) -> Result<TitleTickOutcome, TitleLinkedError<V::Error>> {
        let owner = self.attachment.context_owner;
        let PortLinked {
            registry, title, ..
        } = self;
        let result = registry.with_context_dispatch(owner, |registry| {
            let actual = registry.linked_screen(owner)?;
            let id = title.as_deref().and_then(TitleScreen::id);
            if id.is_none() || id != actual {
                return Err(match actual {
                    Some(actual) => TitleLinkedError::Screen(LinkedScreenError::Foreign(actual)),
                    None => TitleLinkedError::Screen(LinkedScreenError::Unlinked),
                });
            }
            let screen = title.as_deref_mut().expect("checked title");
            let mut effects = TitleSessionHost { video, state };
            screen.tick(&mut effects).map_err(TitleLinkedError::Title)
        });
        match result {
            Ok(outcome) => Ok(outcome),
            Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }

    /// Title selected-input poll `004F0200`: physical notifications drain in
    /// arrival order, then the queue dispatches. System callbacks behave as on
    /// every screen; the skip callback `00404870` finishes an unfinished title
    /// on presses. The title runs no held-bit dispatch: its `00439470`
    /// targets are empty `00401310` virtuals.
    pub fn poll_title<V: TitleVideoHost>(
        &mut self,
        state: &mut SessionState,
        pass: PortInputPass<'_>,
        video: &mut V,
        exit_requested: &mut bool,
    ) -> Result<(), TitleLinkedError<V::Error>> {
        let owner = self.attachment.context_owner;
        let actual = self
            .registry
            .linked_screen(owner)
            .map_err(TitleLinkedError::Registry)?;
        let id = self.title.as_deref().and_then(TitleScreen::id);
        if id.is_none() || id != actual {
            return Err(match actual {
                Some(actual) => TitleLinkedError::Screen(LinkedScreenError::Foreign(actual)),
                None => TitleLinkedError::Screen(LinkedScreenError::Unlinked),
            });
        }
        let input = &mut *self.input;
        let mut router = TitleRouter {
            now_micros: pass.now_micros,
            screen: self.title.as_deref_mut().expect("checked title"),
            video,
            state,
            exit_requested,
        };
        if !pass.notifications.is_empty() {
            let keyboard = pass.keyboard.ok_or(TitleLinkedError::MissingKeyboard)?;
            for notification in pass.notifications.drain(..) {
                keyboard
                    .update(notification, input, &mut router)
                    .map_err(flatten_title_input)?;
            }
        }
        input.poll(&mut router).map_err(flatten_title_input)
    }

    /// Startup selected-input poll `004F0200`: notifications drain in arrival
    /// order, then the queue dispatches. Alt+Enter keeps the shared toggle
    /// boundary; every other callback reaches the startup screen, whose own
    /// context check rejects foreign callbacks before mutation.
    pub fn poll_startup<E: StartupEffects>(
        &mut self,
        pass: PortInputPass<'_>,
        effects: &mut E,
    ) -> Result<(), StartupLinkedError> {
        let actual = self.registry.linked_screen(self.attachment.context_owner)?;
        let id = self.startup.as_deref().and_then(StartupScreen::id);
        if id.is_none() || id != actual {
            return Err(StartupLinkedError::Screen(match actual {
                Some(actual) => LinkedScreenError::Foreign(actual),
                None => LinkedScreenError::Unlinked,
            }));
        }
        let input = &mut *self.input;
        let mut router = StartupRouter {
            now_micros: pass.now_micros,
            screen: self.startup.as_deref_mut().expect("checked startup"),
            effects,
        };
        let flatten = |error: InputError<StartupLinkedError>| match error {
            InputError::Host(error) => error,
            InputError::State(error) => StartupLinkedError::Input(error),
        };
        if !pass.notifications.is_empty() {
            let keyboard = pass.keyboard.ok_or(StartupLinkedError::MissingKeyboard)?;
            for notification in pass.notifications.drain(..) {
                keyboard
                    .update(notification, input, &mut router)
                    .map_err(flatten)?;
            }
        }
        input.poll(&mut router).map_err(flatten)
    }

    /// Startup linked +04 `00433640` under the context guard.
    pub fn tick_startup<E: StartupEffects>(
        &mut self,
        tick: SchedulerTick,
        effects: &mut E,
    ) -> Result<(), StartupLinkedError> {
        let owner = self.attachment.context_owner;
        let PortLinked {
            registry,
            input,
            startup,
            ..
        } = self;
        let result = registry.with_context_dispatch(owner, |registry| {
            let actual = registry.linked_screen(owner)?;
            let id = startup.as_deref().and_then(StartupScreen::id);
            if id.is_none() || id != actual {
                return Err(StartupLinkedError::Screen(match actual {
                    Some(actual) => LinkedScreenError::Foreign(actual),
                    None => LinkedScreenError::Unlinked,
                }));
            }
            let screen = startup.as_deref_mut().expect("checked startup");
            screen
                .tick(input, tick, effects)
                .map(|_| ())
                .map_err(|error| StartupLinkedError::Startup(error.to_string()))
        });
        match result {
            Ok(()) => Ok(()),
            Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }

    /// Key configuration selected-input poll `004F0200`. The keyboard binds
    /// Alt+Enter, Alt+F1 and Alt+Esc beside the screen's held-bit rows.
    pub fn poll_key_config<E: KeyConfigEffects>(
        &mut self,
        pass: PortInputPass<'_>,
        effects: &mut E,
        exit_requested: &mut bool,
    ) -> Result<(), KeyConfigLinkedError> {
        let actual = self.registry.linked_screen(self.attachment.context_owner)?;
        let id = self.key_config.as_deref().and_then(KeyConfigScreen::id);
        if id.is_none() || id != actual {
            return Err(KeyConfigLinkedError::Screen(match actual {
                Some(actual) => LinkedScreenError::Foreign(actual),
                None => LinkedScreenError::Unlinked,
            }));
        }
        let input = &mut *self.input;
        let mut router = KeyConfigRouter {
            now_micros: pass.now_micros,
            screen: self.key_config.as_deref_mut().expect("checked key config"),
            effects,
            exit_requested,
        };
        let flatten = |error: InputError<KeyConfigLinkedError>| match error {
            InputError::Host(error) => error,
            InputError::State(error) => KeyConfigLinkedError::Input(error),
        };
        if !pass.notifications.is_empty() {
            let keyboard = pass.keyboard.ok_or(KeyConfigLinkedError::MissingKeyboard)?;
            for notification in pass.notifications.drain(..) {
                keyboard
                    .update(notification, input, &mut router)
                    .map_err(flatten)?;
            }
        }
        input.poll(&mut router).map_err(flatten)
    }

    /// Key configuration linked +04 `00444E80` under the context guard.
    /// `keyboard` is the process adapter in cache slot 0.
    pub fn tick_key_config<E: KeyConfigEffects>(
        &mut self,
        tick: SchedulerTick,
        keyboard: Option<DeviceId>,
        effects: &mut E,
    ) -> Result<(), KeyConfigLinkedError> {
        let owner = self.attachment.context_owner;
        let PortLinked {
            registry,
            input,
            key_config,
            ..
        } = self;
        let result = registry.with_context_dispatch(owner, |registry| {
            let actual = registry.linked_screen(owner)?;
            let id = key_config.as_deref().and_then(KeyConfigScreen::id);
            if id.is_none() || id != actual {
                return Err(KeyConfigLinkedError::Screen(match actual {
                    Some(actual) => LinkedScreenError::Foreign(actual),
                    None => LinkedScreenError::Unlinked,
                }));
            }
            let screen = key_config.as_deref_mut().expect("checked key config");
            screen
                .tick(input, tick, keyboard, effects)
                .map_err(|error| KeyConfigLinkedError::KeyConfig(error.to_string()))
        });
        match result {
            Ok(()) => Ok(()),
            Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }

    /// The owned screen matching the caller's services, which must also be
    /// the one the registry links (the session serves one screen at a time).
    fn checked_screen<E, N, F, T>(
        &self,
        services: &LinkedServices<'_, E, N>,
    ) -> Result<PortScreen, PortLinkedError<F, T>> {
        let screen = linked_screen(
            self.registry,
            self.attachment,
            self.startup.as_deref(),
            self.key_config.as_deref(),
            self.title.as_deref(),
            self.frontend.as_deref(),
            self.text.as_deref(),
            self.briefing.as_deref(),
            self.gameplay.as_deref(),
            self.intro_results.as_deref(),
            self.map.as_deref(),
        )?;
        match (screen, services) {
            (PortScreen::Frontend, LinkedServices::Frontend { .. })
            | (PortScreen::TextEntry, LinkedServices::TextEntry { .. }) => Ok(screen),
            _ => Err(PortLinkedError::Services {
                screen,
                services: services.name(),
            }),
        }
    }

    /// Phase-3 uses its authored digital tables and the same keyboard queue
    /// as the frontend/name entry. Native callbacks return ordered cue IDs.
    pub fn poll_briefing(
        &mut self,
        pass: PortInputPass<'_>,
        exit_requested: &mut bool,
    ) -> Result<Vec<u32>, BriefingLinkedError> {
        let actual = self.registry.linked_screen(self.attachment.context_owner)?;
        let id = self
            .briefing
            .as_deref()
            .and_then(crate::briefing_screen::BriefingScreen::id);
        if id.is_none() || id != actual {
            return Err(BriefingLinkedError::Screen(match actual {
                Some(actual) => LinkedScreenError::Foreign(actual),
                None => LinkedScreenError::Unlinked,
            }));
        }
        let mut router = BriefingRouter {
            now_micros: pass.now_micros,
            screen: self.briefing.as_deref_mut().expect("checked briefing"),
            exit_requested,
            cues: Vec::new(),
        };
        let flatten = |error: InputError<BriefingLinkedError>| match error {
            InputError::Host(error) => error,
            InputError::State(error) => BriefingLinkedError::Tick(error.to_string()),
        };
        if !pass.notifications.is_empty() {
            let keyboard = pass
                .keyboard
                .ok_or_else(|| BriefingLinkedError::Tick("missing keyboard adapter".into()))?;
            for notification in pass.notifications.drain(..) {
                keyboard
                    .update(notification, self.input, &mut router)
                    .map_err(flatten)?;
            }
        }
        self.input.poll(&mut router).map_err(flatten)?;
        Ok(router.cues)
    }

    /// Linked +04 for the current world's authored briefing (`0040C6A0`).
    /// Runs under the context guard and routes the phase-4 request through the
    /// session state (first-request-wins).
    pub fn tick_briefing(
        &mut self,
        state: &mut SessionState,
        tick: SchedulerTick,
        world: &mut crate::world_backend::WorldSession,
        audio: &mut impl FnMut(
            crate::briefing_screen::BriefingAudioRequest,
            &mut u32,
        ) -> Result<(), String>,
    ) -> Result<crate::briefing_runtime::BriefingTickOutcome, BriefingLinkedError> {
        let owner = self.attachment.context_owner;
        // 0040C6A0 reads the status only for a world with briefing records;
        // the arenas have none.
        let no_records = self
            .briefing
            .as_deref()
            .and_then(crate::briefing_screen::BriefingScreen::runtime)
            .is_some_and(|runtime| runtime.program().records.is_empty());
        let mission_status = match world.mission.status_byte(world.mission.current_world_a0) {
            Ok(status) => u32::from(status),
            Err(_) if no_records => 0,
            Err(error) => return Err(BriefingLinkedError::Tick(error.to_string())),
        };
        let gates = crate::briefing_runtime::BriefingGates {
            mission_status,
            attract_mode: *self.attract,
        };
        let result = self.registry.with_context_dispatch(owner, |registry| {
            let actual = registry
                .linked_screen(owner)
                .map_err(BriefingLinkedError::Registry)?;
            let id = self
                .briefing
                .as_deref()
                .and_then(crate::briefing_screen::BriefingScreen::id);
            if id.is_none() || id != actual {
                let screen_error = match actual {
                    Some(actual) => LinkedScreenError::Foreign(actual),
                    None => LinkedScreenError::Unlinked,
                };
                return Err(BriefingLinkedError::Screen(screen_error));
            }
            let screen = self.briefing.as_deref_mut().expect("checked briefing");
            let mut host = BriefingSessionHost { state, audio };
            let outcome = screen
                .tick(
                    tick,
                    gates,
                    &mut world.rng,
                    self.input,
                    Some(crate::briefing_camera::BriefingCameraWorld {
                        arena: &world.arena,
                        terrain: &world.terrain,
                    }),
                    &mut host,
                )
                .map_err(|error| BriefingLinkedError::Tick(error.to_string()))?;
            for effect in &outcome.effects {
                match *effect {
                    // 0040D510's finish calls 0048E7F0, whose 00439530 resets
                    // the shared animation clock where the finish runs: before
                    // the item tick's own 00439560 advance (0040C770) after it.
                    crate::briefing_runtime::BriefingTickEffect::Complete(_) => {
                        self.process.reset_animation();
                    }
                    crate::briefing_runtime::BriefingTickEffect::AdvanceSharedAnimation {
                        delta_micros,
                    } => self.process.advance_animation(delta_micros),
                    _ => {}
                }
            }
            Ok(outcome)
        });
        match result {
            Ok(outcome) => Ok(outcome),
            Err(RegistryCallbackError::Registry(error)) => {
                Err(BriefingLinkedError::Registry(error))
            }
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }

    /// Linked +04 for the gameplay screen (`00442B50`'s inner worker
    /// `00440D80`): its base input, whose held slots' codes then reach the
    /// first player's message.
    pub fn tick_gameplay(
        &mut self,
        state: &mut SessionState,
        tick: SchedulerTick,
        world: Option<&mut crate::world_backend::WorldSession>,
    ) -> Result<(), GameplayLinkedError> {
        let input = &*self.input;
        let owner = self.attachment.context_owner;
        let result = self.registry.with_context_dispatch(owner, |registry| {
            let actual = registry
                .linked_screen(owner)
                .map_err(GameplayLinkedError::Registry)?;
            let id = self
                .gameplay
                .as_deref()
                .and_then(crate::gameplay_screen::GameplayScreen::id);
            if id.is_none() || id != actual {
                let screen_error = match actual {
                    Some(actual) => LinkedScreenError::Foreign(actual),
                    None => LinkedScreenError::Unlinked,
                };
                return Err(GameplayLinkedError::Screen(screen_error));
            }
            let screen = self.gameplay.as_deref_mut().expect("checked gameplay");
            let mut host = GameplaySessionHost;
            screen
                .tick(tick, input, &mut host)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))
        });
        match result {
            Ok(()) => {
                if let (Some(screen), Some(world)) = (self.gameplay.as_deref_mut(), world) {
                    run_in_world_menu_commands(screen, world, state)?;
                }
                Ok(())
            }
            Err(RegistryCallbackError::Registry(error)) => {
                Err(GameplayLinkedError::Registry(error))
            }
            Err(RegistryCallbackError::Callback(error)) => Err(error),
        }
    }
}

struct BriefingSessionHost<'a> {
    state: &'a mut SessionState,
    audio: &'a mut dyn FnMut(
        crate::briefing_screen::BriefingAudioRequest,
        &mut u32,
    ) -> Result<(), String>,
}

struct BriefingRouter<'a> {
    now_micros: u64,
    screen: &'a mut crate::briefing_screen::BriefingScreen,
    exit_requested: &'a mut bool,
    cues: Vec<u32>,
}

impl InputHost for BriefingRouter<'_> {
    type Error = BriefingLinkedError;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }
    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        match event.address {
            EXIT_CALLBACK => {
                if event.value != 0 {
                    *self.exit_requested = true;
                }
            }
            NOOP_CALLBACK => {}
            FULLSCREEN_TOGGLE_CALLBACK if event.value == 0 => {}
            FULLSCREEN_TOGGLE_CALLBACK => {
                return Err(BriefingLinkedError::Tick(
                    "fullscreen callback owner is unconnected".into(),
                ))
            }
            _ => self.cues.extend(
                self.screen
                    .digital_callback(event)
                    .map_err(|error| BriefingLinkedError::Tick(error.to_string()))?,
            ),
        }
        Ok(())
    }
}

impl crate::briefing_screen::BriefingTickHost for BriefingSessionHost<'_> {
    type Error = String;
    fn audio(
        &mut self,
        request: crate::briefing_screen::BriefingAudioRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        (self.audio)(request, rng)
    }
    fn request_phase(
        &mut self,
        _previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error> {
        let _ = self.state.request_phase(requested);
        Ok(())
    }
}

struct GameplaySessionHost;
impl crate::gameplay_screen::GameplayTickHost for GameplaySessionHost {
    type Error = std::convert::Infallible;
}

#[allow(clippy::too_many_arguments)]
fn linked_screen(
    registry: &ScreenRegistry<FrontendRenderContext, InputContextId>,
    attachment: ScreenAttachment,
    startup: Option<&StartupScreen>,
    key_config: Option<&KeyConfigScreen>,
    title: Option<&TitleScreen>,
    frontend: Option<&FrontendScreen>,
    text: Option<&TextEntryScreen>,
    briefing: Option<&crate::briefing_screen::BriefingScreen>,
    gameplay: Option<&crate::gameplay_screen::GameplayScreen>,
    intro_results: Option<&crate::intro_results_screen::IntroResultsScreen>,
    map: Option<&crate::map_screen::MapScreen>,
) -> Result<PortScreen, LinkedScreenError> {
    let id = registry
        .linked_screen(attachment.context_owner)?
        .ok_or(LinkedScreenError::Unlinked)?;
    if title.and_then(TitleScreen::id) == Some(id) {
        Ok(PortScreen::Title)
    } else if frontend.and_then(FrontendScreen::id) == Some(id) {
        Ok(PortScreen::Frontend)
    } else if text.and_then(TextEntryScreen::id) == Some(id) {
        Ok(PortScreen::TextEntry)
    } else if briefing.and_then(crate::briefing_screen::BriefingScreen::id) == Some(id) {
        Ok(PortScreen::Briefing)
    } else if gameplay.and_then(crate::gameplay_screen::GameplayScreen::id) == Some(id) {
        Ok(PortScreen::Gameplay)
    } else if intro_results.and_then(crate::intro_results_screen::IntroResultsScreen::id)
        == Some(id)
    {
        Ok(PortScreen::IntroResults)
    } else if map.and_then(crate::map_screen::MapScreen::id) == Some(id) {
        Ok(PortScreen::Map)
    } else if startup.and_then(StartupScreen::id) == Some(id) {
        Ok(PortScreen::Startup)
    } else if key_config.and_then(KeyConfigScreen::id) == Some(id) {
        Ok(PortScreen::KeyConfig)
    } else {
        Err(LinkedScreenError::Foreign(id))
    }
}

/// 004943A0 rereads context +18 for every callback: inside the guard the
/// linked screen must still be the one resolved for these services.
fn require_linked<F, T>(
    registry: &ScreenRegistry<FrontendRenderContext, InputContextId>,
    owner: crate::screen_registry::ContextOwnerId,
    expected: PortScreen,
    title: &Option<&mut TitleScreen>,
    frontend: &Option<&mut FrontendScreen>,
    text: &Option<&mut TextEntryScreen>,
) -> Result<(), PortLinkedError<F, T>> {
    let actual = registry.linked_screen(owner)?;
    let id = match expected {
        PortScreen::Title => title.as_deref().and_then(TitleScreen::id),
        PortScreen::Frontend => frontend.as_deref().and_then(FrontendScreen::id),
        PortScreen::TextEntry => text.as_deref().and_then(TextEntryScreen::id),
        PortScreen::Startup
        | PortScreen::KeyConfig
        | PortScreen::Briefing
        | PortScreen::Gameplay
        | PortScreen::IntroResults
        | PortScreen::Map => {
            // Briefing/gameplay ticks use their own `tick_briefing`/`tick_gameplay`
            // paths, not the frontend/text-entry `tick` dispatcher.
            return Err(PortLinkedError::Services {
                screen: expected,
                services: "frontend/text-entry",
            });
        }
    };
    if id.is_none() || id != actual {
        return Err(match actual {
            Some(actual) => LinkedScreenError::Foreign(actual),
            None => LinkedScreenError::Unlinked,
        }
        .into());
    }
    Ok(())
}

fn flatten_input<F, T>(error: InputError<PortLinkedError<F, T>>) -> PortLinkedError<F, T> {
    match error {
        InputError::State(error) => PortLinkedError::Input(error),
        InputError::Host(error) => error,
    }
}

fn flatten_title_input<H>(error: InputError<TitleLinkedError<H>>) -> TitleLinkedError<H> {
    match error {
        InputError::State(error) => TitleLinkedError::Input(error),
        InputError::Host(error) => error,
    }
}

/// The input host for one title poll. System callbacks behave as on every
/// screen; the skip callback reaches the title screen, whose own context
/// check rejects callbacks of any other owner before mutation. There is no
/// held-bit dispatch: the title's `00439470` targets are empty.
struct TitleRouter<'r, V> {
    now_micros: u64,
    screen: &'r mut TitleScreen,
    video: &'r mut V,
    state: &'r mut SessionState,
    exit_requested: &'r mut bool,
}

impl<V: TitleVideoHost> TitleRouter<'_, V> {
    fn system_callback(
        &mut self,
        event: InputCallback,
    ) -> Option<Result<(), TitleLinkedError<V::Error>>> {
        match event.address {
            EXIT_CALLBACK => Some(if event.value == 0 {
                Ok(())
            } else {
                *self.exit_requested = true;
                Ok(())
            }),
            NOOP_CALLBACK => Some(Ok(())),
            // 00437D30 acts on press only; its release is a no-op.
            FULLSCREEN_TOGGLE_CALLBACK => Some(if event.value == 0 {
                Ok(())
            } else {
                Err(TitleLinkedError::SystemCallback(event.address))
            }),
            _ => None,
        }
    }
}

impl<V: TitleVideoHost> InputHost for TitleRouter<'_, V> {
    type Error = TitleLinkedError<V::Error>;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        if let Some(result) = self.system_callback(event) {
            return result;
        }
        if event.address != TITLE_SKIP_CALLBACK {
            return Err(TitleLinkedError::SystemCallback(event.address));
        }
        let mut effects = TitleSessionHost {
            video: &mut *self.video,
            state: &mut *self.state,
        };
        self.screen
            .digital_callback(event, &mut effects)
            .map_err(TitleLinkedError::Title)
    }
}

/// The input host for one startup poll. The keyboard table binds Alt+Enter
/// but no Alt+Esc or no-op rows; everything else is the screen's.
struct StartupRouter<'r, E> {
    now_micros: u64,
    screen: &'r mut StartupScreen,
    effects: &'r mut E,
}

impl<E: StartupEffects> InputHost for StartupRouter<'_, E> {
    type Error = StartupLinkedError;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        match event.address {
            // 00437D30 acts on press only; its release is a no-op.
            FULLSCREEN_TOGGLE_CALLBACK if event.value == 0 => Ok(()),
            FULLSCREEN_TOGGLE_CALLBACK => Err(StartupLinkedError::SystemCallback(event.address)),
            _ => self
                .screen
                .digital_callback(event, self.effects)
                .map(|_| ())
                .map_err(|error| StartupLinkedError::Startup(error.to_string())),
        }
    }
}

/// The input host for one key configuration poll.
struct KeyConfigRouter<'r, E> {
    now_micros: u64,
    screen: &'r mut KeyConfigScreen,
    effects: &'r mut E,
    exit_requested: &'r mut bool,
}

impl<E: KeyConfigEffects> InputHost for KeyConfigRouter<'_, E> {
    type Error = KeyConfigLinkedError;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        match event.address {
            EXIT_CALLBACK => {
                if event.value != 0 {
                    *self.exit_requested = true;
                }
                Ok(())
            }
            NOOP_CALLBACK => Ok(()),
            // 00437D30 acts on press only; its release is a no-op.
            FULLSCREEN_TOGGLE_CALLBACK if event.value == 0 => Ok(()),
            FULLSCREEN_TOGGLE_CALLBACK => Err(KeyConfigLinkedError::SystemCallback(event.address)),
            _ => self
                .screen
                .digital_callback(event, self.effects)
                .map_err(|error| KeyConfigLinkedError::KeyConfig(error.to_string())),
        }
    }
}

/// The input host for one poll. System callbacks are routed by address
/// first, then everything else goes to the linked screen, whose own context
/// check rejects callbacks of any other owner before mutation.
struct Router<'r, 'a, E, N> {
    now_micros: u64,
    catalog: &'r mut FrontendMenuCatalog,
    process: &'r FrontendProcessState,
    text_process: &'r mut TextEntryProcessState,
    frontend: Option<&'r mut FrontendScreen>,
    text: Option<&'r mut TextEntryScreen>,
    attract: &'r mut bool,
    network_active: bool,
    owner_byte: bool,
    offline_game: Option<&'r OfflineGame>,
    state: &'r mut SessionState,
    services: LinkedServices<'a, E, N>,
    completions: &'r mut Vec<NameCompletion>,
    screen: PortScreen,
}

impl<E: FrontendEffects, N: TextEntryPlatform> Router<'_, '_, E, N> {
    fn system_callback(
        &mut self,
        event: InputCallback,
    ) -> Option<Result<(), PortLinkedError<E::Error, N::Error>>> {
        match event.address {
            EXIT_CALLBACK => Some(if event.value == 0 {
                Ok(())
            } else {
                match &mut self.services {
                    LinkedServices::Frontend { effects, .. } => effects
                        .request_exit()
                        .map_err(PortLinkedError::FrontendPlatform),
                    LinkedServices::TextEntry { platform, .. } => platform
                        .request_exit()
                        .map_err(PortLinkedError::TextPlatform),
                }
            }),
            NOOP_CALLBACK => Some(Ok(())),
            // 00437D30 acts on press only; its release is a no-op.
            FULLSCREEN_TOGGLE_CALLBACK => Some(if event.value == 0 {
                Ok(())
            } else {
                Err(PortLinkedError::SystemCallback(event.address))
            }),
            _ => None,
        }
    }
}

impl<E: FrontendEffects, N: TextEntryPlatform> InputHost for Router<'_, '_, E, N> {
    type Error = PortLinkedError<E::Error, N::Error>;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        if let Some(result) = self.system_callback(event) {
            return result;
        }
        match (&mut self.services, self.screen) {
            (
                LinkedServices::Frontend {
                    effects,
                    profiles,
                    basis,
                },
                PortScreen::Frontend,
            ) => {
                let screen = self.frontend.as_deref_mut().expect("checked frontend");
                let mut effects = SessionFrontendEffects {
                    inner: &mut **effects,
                    state: &mut *self.state,
                    attract: &mut *self.attract,
                };
                screen
                    .digital_callback(
                        event,
                        &mut FrontendDispatch {
                            catalog: &mut *self.catalog,
                            basis,
                            visibility: MenuVisibility::from_owner_flag(self.owner_byte),
                            effects: &mut effects,
                            profiles: Some(&mut **profiles),
                        },
                    )
                    .map_err(PortLinkedError::Frontend)
            }
            (
                LinkedServices::TextEntry {
                    platform,
                    profiles,
                    rng,
                    keyboard,
                },
                PortScreen::TextEntry,
            ) => {
                debug_assert_eq!(TEXT_ENTRY_CALLBACK_CONTEXT, 2);
                let screen = self.text.as_deref_mut().expect("checked text entry");
                let phase = u32::from(self.state.current_phase().id());
                let poll_micros = input.clock_micros();
                let mut effects = SessionNameEntryEffects {
                    platform: &mut **platform,
                    state: &mut *self.state,
                    offline_game: self.offline_game,
                    network_active: self.network_active,
                };
                let completion = screen
                    .digital_callback(
                        event,
                        &mut TextEntryServices {
                            input,
                            store: &mut **profiles,
                            key_names: self.catalog.assets().key_names(),
                            keyboard_adapter: *keyboard,
                            text_process: &mut *self.text_process,
                            frontend_process: self.process,
                            rng: &mut **rng,
                            effects: &mut effects,
                            phase,
                            poll_micros,
                        },
                    )
                    .map_err(PortLinkedError::Text)?;
                self.completions.extend(completion);
                Ok(())
            }
            _ => unreachable!("services were checked against the linked screen"),
        }
    }
}

/// The frontend's effects with the session-owned requests intercepted.
pub struct SessionFrontendEffects<'a, E> {
    pub inner: &'a mut E,
    pub state: &'a mut SessionState,
    pub attract: &'a mut bool,
}

impl<E: FrontendEffects> FrontendEffects for SessionFrontendEffects<'_, E> {
    type Error = PortEffectError<E::Error>;

    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.inner.service_music().map_err(PortEffectError::Host)
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.inner
            .play_sound(resource)
            .map_err(PortEffectError::Host)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.inner.persist_settings().map_err(PortEffectError::Host)
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.inner
            .set_transition_flag()
            .map_err(PortEffectError::Host)
    }
    fn profile_operation_failed(
        &mut self,
        operation: ProfileOperation,
        error: &ProfileStoreError,
    ) -> Result<(), Self::Error> {
        self.inner
            .profile_operation_failed(operation, error)
            .map_err(PortEffectError::Host)
    }
    fn open_network_menu(&mut self, stack: &mut MenuStack) -> Result<(), Self::Error> {
        self.inner
            .open_network_menu(stack)
            .map_err(PortEffectError::Host)
    }
    fn network_link(&mut self) -> Option<&mut crate::network::link::NetworkLink> {
        self.inner.network_link()
    }
    fn network_platform(&mut self) -> Option<crate::network::frontend_host::NetworkPlatform<'_>> {
        self.inner.network_platform()
    }
    fn network_line_source(&mut self) -> Option<crate::network::game_tick::LineGroupSource> {
        self.inner.network_line_source()
    }
    fn settings_word(&mut self, offset: u32) -> Option<u32> {
        self.inner.settings_word(offset)
    }
    fn set_settings_word(&mut self, offset: u32, value: u32) -> Option<()> {
        self.inner.set_settings_word(offset, value)
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .adjust_setting(address, command, max)
            .map_err(PortEffectError::Host)
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        self.inner
            .refresh_settings_world()
            .map_err(PortEffectError::Host)
    }
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        self.inner
            .restore_default_settings()
            .map_err(PortEffectError::Host)
    }
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .adjust_display_setting(address, command, max)
            .map_err(PortEffectError::Host)
    }
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error> {
        self.inner
            .apply_volume(channel)
            .map_err(PortEffectError::Host)
    }
    /// 00438380: first request wins while one is pending; callers discard
    /// the result, so a rejected request is silently dropped.
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        let phase = SessionPhase::try_from(phase)?;
        let _ = self.state.request_phase(phase);
        Ok(())
    }
    fn set_attract_flag(&mut self, enabled: bool) -> Result<(), Self::Error> {
        *self.attract = enabled;
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.inner.request_exit().map_err(PortEffectError::Host)
    }
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        self.inner
            .external_action(request, stack)
            .map_err(PortEffectError::Host)
    }
}

/// The text-entry completion's session effects: phase requests, the offline
/// game 00487410 published and the network flag 0055F448.
pub struct SessionNameEntryEffects<'a, N> {
    pub platform: &'a mut N,
    pub state: &'a mut SessionState,
    pub offline_game: Option<&'a OfflineGame>,
    pub network_active: bool,
}

impl<N: TextEntryPlatform> NameEntryActions for SessionNameEntryEffects<'_, N> {
    type Error = PortEffectError<N::Error>;
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error> {
        self.platform
            .grant_cheat_flag(bit)
            .map_err(PortEffectError::Host)
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        let phase = SessionPhase::try_from(phase)?;
        let _ = self.state.request_phase(phase);
        Ok(())
    }
}

impl<N: TextEntryPlatform> NameEntryCompletion for SessionNameEntryEffects<'_, N> {
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        self.platform
            .persist_identity(request)
            .map_err(PortEffectError::Host)
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        let game = self
            .offline_game
            .ok_or(PortEffectError::MissingOfflineGame)?;
        Ok((game.mission.clone(), game.player.clone()))
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        Ok(self.network_active)
    }
    fn effect_quantum(&self) -> Option<u32> {
        self.platform.name_entry_effect_quantum()
    }
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
