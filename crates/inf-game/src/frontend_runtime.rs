//! Platform effects used by the executable's concrete screens.
//!
//! Specialized menus and phase factories fail at their actual boundary until
//! their owners are connected; no phase or profile success is manufactured.
//! Session-owned state (phase requests, the attract flag) is intercepted by
//! the session's linked routing before it reaches these effects.

use std::{cell::Cell, path::PathBuf};

use thiserror::Error;

use crate::{
    display_selection::{original_size, DisplayMode, DisplaySelection, MonitorModes},
    frontend_audio::{FrontendAudio, FrontendAudioError},
    frontend_bindings::FrontendDevice,
    frontend_dispatch::FrontendEffects,
    frontend_screen::FrontendFactoryHost,
    frontend_settings::{
        FrontendSettings, FrontendSettingsError, SidecarIdentityStore, SETTINGS_STATE_ADDRESS,
    },
    frontend_video::{FrontendVideoState, VideoDevice, VideoError, VideoResetRequest},
    input::{InputRuntime, InputStateError},
    menu::{MenuActionRequest, MenuCommand, MenuStack},
    name_entry::{IdentityWriteRequest, LoadedIdentity},
    platform_audio::{PlatformAudio, PlatformAudioError},
    platform_input::SdlInput,
    player_identity::{IdentityRead, PlayerIdentity},
    profile_menu::ProfileOperation,
    profile_store::{ProfileStore, ProfileStoreError},
    session::SessionPhase,
    session_platform::{linked::TextEntryPlatform, PortSessionServices},
    settings_row::{adjust_value, VolumeChannel},
    text_entry_screen::TextEntryFactoryHost,
    title_playback::TitleVideoState,
    title_screen::{TitleHost, TitleVideoHost},
    ResidentRuntime,
};

#[derive(Debug, Error)]
pub enum FrontendRuntimeError {
    #[error(transparent)]
    Audio(#[from] FrontendAudioError<PlatformAudioError>),
    #[error(transparent)]
    Playback(#[from] PlatformAudioError),
    #[error(transparent)]
    Settings(#[from] FrontendSettingsError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error("frontend operation {0} requires its unconnected runtime owner")]
    Unsupported(&'static str),
    #[error("frontend action has no connected owner: {0:?}")]
    Action(MenuActionRequest),
    #[error("display adjust cell {address:#010x} is outside the 34-word settings state")]
    DisplayCell { address: u32 },
    #[error(transparent)]
    Video(#[from] VideoError),
    #[error("display reset rejected: {0}")]
    DisplayReset(String),
    #[error("display reset could not restore the previous window: {0}")]
    DisplayRollback(String),
}

pub struct FrontendPlatform {
    pub audio: PlatformAudio,
    pub music: FrontendAudio,
    pub settings: FrontendSettings,
    pub video: FrontendVideoState,
    /// Last successful display reset target, independent of a rolled-back row.
    pub display_variant: u32,
    /// 0048F060's process RNG starts in loader-zero BSS. Shared with all
    /// connected consumers; cue aliases do not own a private RNG stream.
    pub random_state: u32,
    /// Identity caches 0054E930/0054EBC8/0054EBC0 and loaded flag 0054EC59,
    /// shared by the player-submenu rows and the text-entry constructor.
    pub identity: PlayerIdentity,
    /// Native0054694C, shared by profile writes, results and loading draws.
    pub loading_mode: Cell<u8>,
    /// The platform loop's running flag 00544D00, cleared by 004F4F60. The
    /// requesting iteration still draws and presents.
    pub exit_requested: bool,
    /// The network session's owner, which outlives the frontend into play.
    pub network: crate::network::link::NetworkLink,
}

impl FrontendPlatform {
    /// Move the Ambience volume (the Sound menus' Ambience row) and give it
    /// to the mixer.
    pub fn adjust_ambience(&mut self, command: MenuCommand) {
        let mut modern = self.settings.modern();
        if crate::frontend_sound::adjust_ambience(&mut modern, command) {
            self.settings.set_modern(modern);
            self.audio.set_ambience_volume(self.settings.ambience());
        }
    }

    /// Turn Headphones (the Sound menus' Headphones row) and give it to the
    /// mixer.
    pub fn adjust_headphones(&mut self, command: MenuCommand) {
        let mut modern = self.settings.modern();
        if crate::frontend_sound::adjust_headphones(&mut modern, command) {
            self.settings.set_modern(modern);
            self.audio.set_headphones(self.settings.headphones());
        }
    }

    /// The network link with the settings and identity it reads.
    pub fn network_platform(&mut self) -> crate::network::frontend_host::NetworkPlatform<'_> {
        crate::network::frontend_host::NetworkPlatform {
            link: &mut self.network,
            settings: &mut self.settings,
            identity: &mut self.identity,
        }
    }

    pub fn new(
        audio: PlatformAudio,
        settings: FrontendSettings,
        devices: Vec<VideoDevice>,
    ) -> Result<Self, FrontendRuntimeError> {
        let display_variant = settings.words()[0x0C / 4];
        let video = FrontendVideoState::new(devices, settings.current_display())?;
        // Frontend music is selected by the session's 00487360 effect, which
        // runs before the frontend factory on every entry to phase 2.
        Ok(Self {
            audio,
            music: FrontendAudio::new(),
            settings,
            video,
            display_variant,
            random_state: 0,
            identity: PlayerIdentity::default(),
            loading_mode: Cell::new(0),
            exit_requested: false,
            network: crate::network::link::NetworkLink::default(),
        })
    }

    pub fn stop(
        &mut self,
        sounds: &mut crate::process_sound::ProcessSoundRuntime,
    ) -> Result<(), FrontendRuntimeError> {
        self.music.release(&mut self.audio)?;
        self.audio.shutdown_sounds(sounds)?;
        Ok(())
    }

    /// 00487B90 with the backend's track completions delivered first
    /// (004F7DE0): a finished or failed track plays again. The frontend
    /// tick (00484E23), the world update's tail (00487B3C), the briefing
    /// tick (0040CE25) and the paused tick (0043899D) all run it, so music
    /// loops on every screen.
    pub fn service_music(&mut self) -> Result<(), FrontendRuntimeError> {
        while let Some(code) = self.audio.poll_notification() {
            self.music.notify(code);
        }
        self.music
            .service(self.settings.music_mut(), &mut self.audio)?;
        Ok(())
    }
}

pub struct FrontendPlatformFactory<'a> {
    pub platform: &'a mut FrontendPlatform,
    pub keyboard: &'a mut Option<SdlInput>,
}

impl FrontendFactoryHost for FrontendPlatformFactory<'_> {
    type Error = FrontendRuntimeError;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.platform.music.pause(&mut self.platform.audio)?;
        Ok(())
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.platform
            .music
            .resume(self.platform.settings.music_mut(), &mut self.platform.audio)?;
        Ok(())
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        if self.keyboard.is_none() {
            *self.keyboard = Some(SdlInput::attach(input)?);
        }
        // 00438850 calibrates with the live Sensitivity (+54).
        let sensitivity = self.platform.settings.words()[0x54 / 4];
        Ok(self
            .keyboard
            .as_mut()
            .expect("attached keyboard")
            .frontend_devices(input, sensitivity)?)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.platform.settings.persist()?;
        Ok(())
    }
}

/// Synchronous display preparation. After success residency must expose the
/// new selection immediately to later callbacks, including navigation cues.
/// DisplayReset leaves the previous backend usable and is caught by the menu;
/// failure to restore the host is a fatal DisplayRollback instead.
pub trait FrontendDisplayHost {
    /// Rebuild the display for the native request and the port's Display
    /// and Resolution choice, whose tier is the request's variant.
    fn reset(
        &mut self,
        request: VideoResetRequest<'_>,
        display: DisplaySelection,
    ) -> Result<(), FrontendRuntimeError>;
    fn residency(&self) -> &ResidentRuntime;
    /// What the monitor behind SDL output `device` offers.
    fn monitor_modes(&self, device: i32) -> MonitorModes;
    /// The player's modern mode, for Vulkan's traced world.
    fn set_modern(&mut self, _modern: crate::modern_settings::ModernSettings) {}
}

/// The two Video rows the port extends: Resolution lists the monitor's
/// modes, and Display adds Borderless.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayRow {
    Resolution,
    Mode,
}

pub struct FrontendPlatformEffects<'a, D: FrontendDisplayHost> {
    pub platform: &'a mut FrontendPlatform,
    pub display: &'a mut D,
}

impl<D: FrontendDisplayHost> FrontendPlatformEffects<'_, D> {
    /// The Resolution and Display rows, over the port's lists instead of
    /// the rows' authored maximums. Each keeps `00462F40`'s contract: an
    /// unchanged choice returns, a changed one resets the display, and a
    /// failed reset restores the previous choice. As there, a change from
    /// tier 0 stores tier 1 without a reset. Borderless's Resolution row
    /// holds only the desktop, so it never changes.
    fn adjust_display_choice(
        &mut self,
        row: DisplayRow,
        command: MenuCommand,
    ) -> Result<(), FrontendRuntimeError> {
        let device = self.platform.video.published_device();
        let modes = self.display.monitor_modes(device);
        self.platform.settings.set_desktop(modes.desktop);
        let old = self.platform.settings.display_selection();
        let mut new = old;
        match row {
            DisplayRow::Resolution => {
                let current = old.resolution(modes.desktop);
                let Some(size) = modes.step(old.mode, current, command) else {
                    return self.finish_reset(Ok(()));
                };
                new.adopt(old.mode, size);
            }
            DisplayRow::Mode => {
                let mut index = old.mode.index();
                adjust_value(command, DisplayMode::ALL.len() as u32 - 1, &mut index);
                let mode = DisplayMode::from_index(index);
                if mode == old.mode {
                    return self.finish_reset(Ok(()));
                }
                // A tier-0 size only stores the new choice: no reset.
                if old.tier(modes.desktop) == 0 {
                    self.platform
                        .settings
                        .set_display_selection(modes.select_mode(old, mode));
                    return self.finish_reset(Ok(()));
                }
                new = modes.select_mode(old, mode);
            }
        }
        self.platform.settings.set_display_selection(new);
        let variant = new.tier(modes.desktop);
        let result = self.display.reset(
            VideoResetRequest {
                variant,
                words: self.platform.settings.words(),
                device,
            },
            new,
        );
        match &result {
            Ok(()) => self.platform.display_variant = variant,
            Err(FrontendRuntimeError::DisplayReset(_)) => {
                self.platform.settings.set_display_selection(old);
            }
            Err(_) => {}
        }
        self.finish_reset(result)
    }

    fn finish_reset(
        &mut self,
        result: Result<(), FrontendRuntimeError>,
    ) -> Result<(), FrontendRuntimeError> {
        match result {
            Ok(()) => {
                self.platform.video.last_reset_error = None;
                Ok(())
            }
            Err(FrontendRuntimeError::DisplayReset(message)) => {
                // 00462F40/00463000 dispose the caught reset error and return.
                self.platform.video.last_reset_error = Some(message);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl<D: FrontendDisplayHost> FrontendEffects for FrontendPlatformEffects<'_, D> {
    type Error = FrontendRuntimeError;
    fn network_link(&mut self) -> Option<&mut crate::network::link::NetworkLink> {
        Some(&mut self.platform.network)
    }
    fn network_platform(&mut self) -> Option<crate::network::frontend_host::NetworkPlatform<'_>> {
        Some(self.platform.network_platform())
    }
    fn network_line_source(&mut self) -> Option<crate::network::game_tick::LineGroupSource> {
        use crate::menu_text::MenuTextResources;
        let residency = self.display.residency();
        let strings = crate::render_resources::RenderResources::new(residency);
        let text = |id: i32| strings.string(id).ok().map(<[u8]>::to_vec);
        Some(crate::network::game_tick::LineGroupSource {
            folder: residency.installation().data_root().to_path_buf(),
            defaults: [text(0x5b), text(0x5a), text(0x5c)],
        })
    }
    fn settings_word(&mut self, offset: u32) -> Option<u32> {
        self.platform
            .settings
            .words()
            .get(offset as usize / 4)
            .copied()
    }
    fn set_settings_word(&mut self, offset: u32, value: u32) -> Option<()> {
        *self
            .platform
            .settings
            .words_mut()
            .get_mut(offset as usize / 4)? = value;
        Some(())
    }
    fn service_music(&mut self) -> Result<(), Self::Error> {
        self.platform.service_music()
    }
    fn play_sound(&mut self, resource: i16) -> Result<(), Self::Error> {
        self.platform.audio.play_resident_cue(
            self.display.residency(),
            resource,
            self.platform.settings.fx_volume(),
            &mut self.platform.random_state,
        )?;
        Ok(())
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.platform.settings.persist()?;
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.platform.loading_mode.set(1);
        Ok(())
    }
    fn profile_operation_failed(
        &mut self,
        operation: ProfileOperation,
        error: &ProfileStoreError,
    ) -> Result<(), Self::Error> {
        eprintln!("Profile {operation:?} failed: {error}");
        Ok(())
    }
    /// NetworkMenuRefusalApproximation: native 0045FD40 pushes the derived
    /// network menu 0045F9C0(false). Network play has no port owner, so the
    /// row refuses like a completed profile slot (004391D0(4)) and keeps the
    /// root menu; the dispatcher's confirmation sound follows as usual.
    fn open_network_menu(&mut self, _: &mut MenuStack) -> Result<(), Self::Error> {
        eprintln!(
            "NetworkMenuRefusalApproximation: 0045FD40's network menu is unported; \
             the row plays refusal sound 4 and keeps the root menu"
        );
        self.play_sound(4)
    }
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        if address == crate::frontend_sound::AMBIENCE_CELL {
            self.platform.adjust_ambience(command);
            return Ok(());
        }
        if address == crate::frontend_sound::HEADPHONES_CELL {
            self.platform.adjust_headphones(command);
            return Ok(());
        }
        let value = self.platform.settings.setting_word_mut(address).ok_or(
            FrontendRuntimeError::Unsupported("settings cell outside the 34-word state"),
        )?;
        crate::settings_row::adjust_value(
            command,
            crate::settings_row::row_most(address, max),
            value,
        );
        Ok(())
    }
    fn refresh_settings_world(&mut self) -> Result<(), Self::Error> {
        // The frontend owns no world entities: the 00478E00 lookup through the
        // 0055F160 gameplay subobject's +8 finds no player, so the native
        // refresh returns through its early-out branch. A host with a loaded
        // world must replace this with the entity patching (or an explicit
        // boundary) instead of inheriting the no-op.
        Ok(())
    }
    /// 00463B70 offline: authored defaults (keeping language and the cheat
    /// word), then 004391C0/00437DF0 reinitialize the display at the default
    /// +0C tier under setjmp, like 00462F40 but without its variant clamp.
    /// A caught reset failure writes the old +0C back and keeps the other
    /// defaults. No volume tail runs, so FX master and music keep their
    /// current device levels until their own rows adjust them.
    fn restore_default_settings(&mut self) -> Result<(), Self::Error> {
        let old_display = self.platform.settings.display_selection();
        self.platform.settings.restore_startup_defaults();
        let variant = self.platform.settings.words()[3];
        let device = self.platform.video.published_device();
        let display = self.platform.settings.display_selection();
        let result = self.display.reset(
            VideoResetRequest {
                variant,
                words: self.platform.settings.words(),
                device,
            },
            display,
        );
        match &result {
            Ok(()) => self.platform.display_variant = variant,
            // The old +0C returns with its resolution; the other defaults,
            // +40 included, stay.
            Err(FrontendRuntimeError::DisplayReset(_)) => {
                let mode = self.platform.settings.display_selection().mode;
                self.platform
                    .settings
                    .set_display_selection(DisplaySelection {
                        mode,
                        size: old_display.size,
                    });
            }
            Err(_) => {}
        }
        self.finish_reset(result)
    }
    fn apply_volume(&mut self, channel: VolumeChannel) -> Result<(), Self::Error> {
        let platform = &mut *self.platform;
        match channel {
            VolumeChannel::Effects => platform
                .audio
                .set_effects_volume(platform.settings.fx_volume())?,
            // 0055F160's +5D0 music selection is the shared FrontendAudio owner.
            VolumeChannel::Music => platform
                .music
                .apply_volume_setting(platform.settings.music_mut(), &mut platform.audio)?,
        }
        Ok(())
    }
    fn adjust_display_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error> {
        // The port's Vulkan rows take effect from the next frame, without
        // a reset.
        if let Some(row) = crate::frontend_video::ModernRow::from_address(address) {
            let mut modern = self.platform.settings.modern();
            let support = self.platform.settings.modern_support();
            if row.adjust(&mut modern, support, command) {
                self.platform.settings.set_modern(modern);
                self.display.set_modern(modern);
            }
            return Ok(());
        }
        // Filtering (+3C) goes past retail's Bilinear on Vulkan. A saved
        // value beyond this renderer's draws as its highest, and the row
        // moves on from there.
        let max = match address.wrapping_sub(SETTINGS_STATE_ADDRESS) {
            0x0c => return self.adjust_display_choice(DisplayRow::Resolution, command),
            0x40 => return self.adjust_display_choice(DisplayRow::Mode, command),
            0x3c => {
                let settings = &mut self.platform.settings;
                let drawn = settings.filtering();
                settings.words_mut()[crate::frontend_video::FILTERING_CELL] = drawn;
                settings.filtering_most()
            }
            _ => max,
        };
        let device = self.platform.video.published_device();
        // +0C is the display's tier, so a reset variant names its resolution.
        let display = self.platform.settings.display_selection();
        let result = crate::settings_row::adjust_display_setting(
            self.platform.settings.words_mut(),
            address,
            command,
            max,
            |variant, words| {
                self.display.reset(
                    VideoResetRequest {
                        variant,
                        words,
                        device,
                    },
                    display,
                )?;
                self.platform.display_variant = variant;
                Ok(())
            },
        )
        .map(|_| ())
        .map_err(|error| match error {
            crate::settings_row::DisplayAdjustError::Cell { address } => {
                FrontendRuntimeError::DisplayCell { address }
            }
            crate::settings_row::DisplayAdjustError::Host(error) => error,
        });
        // A tier 0 is stored as 1 without a reset: the window stays, and the
        // saved resolution becomes tier 1's.
        let tier = self.platform.settings.words()[3];
        if tier != display.tier(self.platform.settings.desktop()) {
            self.platform
                .settings
                .set_display_selection(DisplaySelection {
                    mode: display.mode,
                    size: original_size(tier),
                });
        }
        self.finish_reset(result)
    }
    /// Session-owned: `SessionFrontendEffects` routes requests to 00438380.
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(FrontendRuntimeError::Unsupported(
            "phase requests outside the session routing",
        ))
    }
    /// Session-owned 0055F14C; see `SessionFrontendEffects`.
    fn set_attract_flag(&mut self, _: bool) -> Result<(), Self::Error> {
        Err(FrontendRuntimeError::Unsupported(
            "the attract flag outside the session routing",
        ))
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        self.platform.exit_requested = true;
        Ok(())
    }
    fn external_action(
        &mut self,
        request: MenuActionRequest,
        _: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        if request.callback_address != 0x0046_3000 {
            return Err(FrontendRuntimeError::Action(request));
        }
        let settings = &mut self.platform.settings;
        let words = *settings.words();
        let old_display = settings.display_selection();
        let old_desktop = settings.desktop();
        let mut save_error = None;
        let result = self.platform.video.adjust_device(
            request.arguments[1],
            request.command,
            &words,
            |device, reset| {
                // RegCreateKey/RegSetValue failure does not bypass native reset.
                save_error = settings
                    .persist_current_display(&device.name)
                    .err()
                    .map(|error| error.to_string());
                // The choice must exist on the new monitor: an unlisted
                // resolution takes the largest listed one below it.
                let mut display = old_display;
                let modes = self.display.monitor_modes(device.id);
                if let Some(size) = modes.resolve(display.mode, display.size) {
                    display.adopt(display.mode, size);
                }
                settings.set_desktop(modes.desktop);
                settings.set_display_selection(display);
                // +0C is the display's tier, never above 4.
                let variant = display.tier(modes.desktop);
                let result = self.display.reset(
                    VideoResetRequest {
                        variant,
                        words: settings.words(),
                        device: reset.device,
                    },
                    display,
                );
                if result.is_err() {
                    settings.set_desktop(old_desktop);
                    settings.set_display_selection(old_display);
                }
                result?;
                self.platform.display_variant = variant;
                Ok(())
            },
        )?;
        self.platform.video.last_device_save_error = save_error;
        self.finish_reset(result)
    }
}

/// The executable's single platform value behind both screen factories and
/// the session services: one keyboard adapter (the process cache 005474B0
/// entry both factories enumerate), the profile store, and audio/settings.
pub struct SdlPlatform {
    pub frontend: FrontendPlatform,
    pub keyboard: Option<SdlInput>,
    pub profiles: ProfileStore,
    /// Retail `Title.avi` source directory for the title-video backend.
    pub data_root: PathBuf,
    /// Title-video graph state behind `TitleVideoHost`.
    pub title_video: TitleVideoState,
}

impl SdlPlatform {
    fn factory(&mut self) -> FrontendPlatformFactory<'_> {
        FrontendPlatformFactory {
            platform: &mut self.frontend,
            keyboard: &mut self.keyboard,
        }
    }
}

impl crate::gameplay_screen::GameplayTerrainSettings for SdlPlatform {
    fn terrain_detail_setting(
        &mut self,
    ) -> Result<u32, crate::terrain_runtime::TerrainPolicyError> {
        Ok(self.frontend.settings.words()[crate::frontend_settings::DETAIL_CELL])
    }
}

impl FrontendFactoryHost for SdlPlatform {
    type Error = FrontendRuntimeError;
    fn pause_music(&mut self) -> Result<(), Self::Error> {
        self.factory().pause_music()
    }
    fn resume_music(&mut self) -> Result<(), Self::Error> {
        self.factory().resume_music()
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        FrontendFactoryHost::enumerate_calibrated_devices(&mut self.factory(), input)
    }
    fn persist_settings(&mut self) -> Result<(), Self::Error> {
        self.factory().persist_settings()
    }
}

impl TextEntryFactoryHost for SdlPlatform {
    type Error = FrontendRuntimeError;
    fn read_identity(
        &mut self,
        phase: SessionPhase,
        slot: u8,
    ) -> Result<LoadedIdentity, Self::Error> {
        read_identity(
            &mut self.frontend.identity,
            &self.frontend.settings,
            phase,
            slot,
        )
    }
    /// The same keyboard the frontend factory attached: the adapter cache
    /// entry is created once and never cleared by screen teardown.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        FrontendFactoryHost::enumerate_calibrated_devices(&mut self.factory(), input)
    }
}

impl PortSessionServices for SdlPlatform {
    type Error = FrontendRuntimeError;
    type IdentityError = FrontendRuntimeError;
    fn network_platform(&mut self) -> Option<crate::network::frontend_host::NetworkPlatform<'_>> {
        Some(self.frontend.network_platform())
    }
    fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError> {
        TextEntryPlatform::persist_identity(
            &mut SettingsTextEntryPlatform {
                settings: &mut self.frontend.settings,
                identity: &mut self.frontend.identity,
                exit_requested: &mut self.frontend.exit_requested,
            },
            request,
        )
    }
    fn profile_store(&mut self) -> Option<&mut ProfileStore> {
        Some(&mut self.profiles)
    }
    fn world_tick_inputs(&self) -> crate::session_platform::WorldTickPlatformInputs {
        let words = self.frontend.settings.words();
        crate::session_platform::WorldTickPlatformInputs {
            camera_settings: crate::object_runtime::CameraSettings::from_words(words),
            settings_20: words[0x20 / 4],
            settings_70: words[crate::frontend_settings::DETAIL_CELL],
            settings_78: words[0x78 / 4],
            settings_7c: words[0x7c / 4],
            input_mode_08: words[0x8 / 4],
            settings_84: self
                .frontend
                .settings
                .fixes()
                .respawn_cheat_word(words[0x84 / 4], self.profiles.completed_campaigns()),
            vehicle_position_step: self.frontend.settings.fixes().vehicle_position_step(),
        }
    }
    fn world_tick_quantum(&self) -> crate::session_platform::WorldTickQuantum {
        self.frontend.settings.fixes().world_tick_quantum()
    }
    fn ultra_ground(&self) -> Option<crate::terrain_runtime::ultra::UltraShape> {
        self.frontend.settings.ultra_ground()
    }
    fn puddle_wheels(&self) -> bool {
        self.frontend.settings.puddle_wheels()
    }
    fn actor_pacing(&self) -> crate::session_platform::ActorPacing {
        self.frontend.settings.fixes().actor_pacing()
    }
    fn mission_completion(&self) -> bool {
        self.frontend.settings.fixes().mission_completion
    }
    fn database_models(&self) -> crate::session_platform::DatabaseModels {
        if self.frontend.settings.fixes().database_models {
            crate::session_platform::DatabaseModels::EveryWorld
        } else {
            crate::session_platform::DatabaseModels::CurrentWorld
        }
    }
    fn prototype_world(&self) -> bool {
        self.frontend.settings.fixes().prototype_world
    }
    fn pelagia_rework(&self) -> bool {
        // The rework needs its gate: without the prototype world it is off.
        self.frontend.settings.fixes().prototype_world
            && self.frontend.settings.fixes().pelagia_rework
    }
    /// The in-world Options' plain rows (`00453B80`), volume sliders
    /// (`00463AC0`, `00463B00`) and the rows with a world refresh
    /// (`00463B50`: Detail Level, First Person, Camera Style and Dynamic
    /// Lighting), whose refresh the session then runs on the world. Of the
    /// rows that reset the display (`00462F40`), Filtering and the Vulkan
    /// rows move as the frontend's do and take effect from the next frame
    /// (the app applies them, user 2026-10-22); a resolution, display mode,
    /// colour depth or device (`00463000`, `00463B70`) still waits for the
    /// frontend.
    fn shown_menu_setting(
        &mut self,
        request: crate::menu::MenuActionRequest,
    ) -> Result<bool, String> {
        let platform = &mut self.frontend;
        let adjust = |platform: &mut FrontendPlatform| -> Result<(), String> {
            let value = platform
                .settings
                .setting_word_mut(request.arguments[1])
                .ok_or("settings cell outside the 34-word state")?;
            let most = crate::settings_row::row_most(request.arguments[1], request.arguments[0]);
            crate::settings_row::adjust_value(request.command, most, value);
            Ok(())
        };
        match request.callback_address {
            0x0045_3b80 | 0x0046_3b50 => adjust(platform)?,
            0x0046_3ac0 if request.arguments[1] == crate::frontend_sound::AMBIENCE_CELL => {
                platform.adjust_ambience(request.command);
            }
            0x0046_3ac0 if request.arguments[1] == crate::frontend_sound::HEADPHONES_CELL => {
                platform.adjust_headphones(request.command);
            }
            0x0046_3ac0 => {
                adjust(platform)?;
                platform
                    .audio
                    .set_effects_volume(platform.settings.fx_volume())
                    .map_err(|error| error.to_string())?;
            }
            0x0046_3b00 => {
                adjust(platform)?;
                platform
                    .music
                    .apply_volume_setting(platform.settings.music_mut(), &mut platform.audio)
                    .map_err(|error| error.to_string())?;
            }
            0x0046_2f40 => {
                use crate::frontend_video::PlayingDisplayRow;
                let settings = &mut platform.settings;
                match crate::frontend_video::playing_display_row(request.arguments[1]) {
                    PlayingDisplayRow::Modern(row) => {
                        let mut modern = settings.modern();
                        if row.adjust(&mut modern, settings.modern_support(), request.command) {
                            settings.set_modern(modern);
                        }
                    }
                    PlayingDisplayRow::Filtering => {
                        // From what it draws, as the frontend's row moves it.
                        let most = settings.filtering_most();
                        let cell = &mut settings.words_mut()[crate::frontend_video::FILTERING_CELL];
                        *cell = (*cell).min(most);
                        crate::settings_row::adjust_value(request.command, most, cell);
                    }
                    PlayingDisplayRow::Frontend => return Ok(false),
                }
            }
            _ => return Ok(false),
        }
        // Retail saves only on the frontend's Back; a change made while
        // playing is kept at once.
        platform
            .settings
            .persist()
            .map_err(|error| error.to_string())?;
        Ok(true)
    }
    fn grant_cheat_flag(&mut self, bits: u32) -> bool {
        self.frontend.settings.words_mut()[0x84 / 4] |= bits;
        true
    }
    fn cheats_menu_word(&self) -> u32 {
        let settings = &self.frontend.settings;
        settings.fixes().menu_cheat_word(settings.words()[0x84 / 4])
    }
    fn loading_platform(&mut self) -> crate::loading_integration::LoadingPlatformLoans<'_> {
        crate::loading_integration::LoadingPlatformLoans {
            mode: &self.frontend.loading_mode,
            rng: &mut self.frontend.random_state,
            voices: crate::process_sound::ProcessSoundTeardownLoan::Available(
                &mut self.frontend.audio,
            ),
        }
    }
    fn sound_voice_teardown(&mut self) -> crate::process_sound::ProcessSoundTeardownLoan<'_> {
        crate::process_sound::ProcessSoundTeardownLoan::Available(&mut self.frontend.audio)
    }
    fn process_rng(&mut self) -> &mut u32 {
        &mut self.frontend.random_state
    }
    fn session_music(
        &mut self,
        request: crate::session_platform::SessionMusicRequest,
    ) -> Result<(), Self::Error> {
        use crate::session_platform::SessionMusicRequest;
        match request {
            SessionMusicRequest::Select { native_argument } => self
                .frontend
                .music
                .select_track(native_argument, &mut self.frontend.audio)?,
            SessionMusicRequest::Resume => self
                .frontend
                .music
                .resume(self.frontend.settings.music_mut(), &mut self.frontend.audio)?,
            SessionMusicRequest::Service => self.frontend.service_music()?,
        }
        Ok(())
    }
    /// 004F0340.
    fn reset_input_baselines(&mut self, input: &mut InputRuntime) -> Result<(), Self::Error> {
        input.reset_baselines_and_discard();
        Ok(())
    }
    fn startup_settings(&self) -> crate::session_platform::StartupSettings {
        crate::session_platform::StartupSettings::from_settings(&self.frontend.settings)
    }
    fn key_bindings(&self) -> crate::key_bindings::KeyBindings {
        self.frontend.settings.key_bindings().clone()
    }
}

/// Stateless absent title-video backend behind the linked title callbacks.
/// The AppFrame borrows the SDL keyboard adapter for notifications and the
/// exit flag beside it, so the linked poll/tick cannot also borrow the whole
/// platform for video: this unit backend carries the absent behavior instead.
/// The factory/teardown path borrows no poll state and uses the platform's
/// own implementation below, which shares these exact semantics.
pub struct AbsentTitleVideo;

impl TitleVideoHost for AbsentTitleVideo {
    type Error = FrontendRuntimeError;

    /// The native failed open: `+F0` stays clear with no phase request.
    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(None)
    }
    /// Only read with a handle held; unreachable while the video is absent.
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        Err(FrontendRuntimeError::Unsupported(
            "title video presentation",
        ))
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        Err(FrontendRuntimeError::Unsupported("title video close"))
    }
    /// Native `00437DF0` with settings `+0C`. The absent open changes no
    /// display tier, so the restore is the identity.
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    /// The SDL flow runs the draw preamble through the software renderer,
    /// not here; direct uses have no renderer to drive.
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// The title video backend: the transcoded frame player behind the native
/// lifecycle. `open_title` takes the native failed-open path (`+F0` stays
/// clear, no phase request) whenever `Title.avi` or the transcoded `video/`
/// store is missing or malformed, so the first title tick still finishes
/// to phase 2 exactly as before.
impl TitleVideoHost for SdlPlatform {
    type Error = FrontendRuntimeError;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(self.title_video.open_title(&self.data_root))
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(self.title_video.playback_status())
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.title_video.present_frame(&mut self.frontend.audio);
        Ok(())
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.title_video.close_title(&mut self.frontend.audio);
        Ok(())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        AbsentTitleVideo.restore_display()
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        AbsentTitleVideo.draw_preamble()
    }
}

/// Linked title callbacks borrow the player state and the mixer disjointly:
/// the keyboard adapter stays out, exactly as with the absent backend. The
/// factory/teardown path keeps using the platform implementation above;
/// opening happens only there, never from a linked callback.
pub struct LinkedTitleVideo<'a> {
    pub state: &'a mut TitleVideoState,
    pub audio: &'a mut PlatformAudio,
}

impl TitleVideoHost for LinkedTitleVideo<'_> {
    type Error = FrontendRuntimeError;

    fn open_title(&mut self) -> Result<Option<()>, Self::Error> {
        Err(FrontendRuntimeError::Unsupported(
            "title video open outside the factory path",
        ))
    }
    fn playback_status(&mut self) -> Result<u32, Self::Error> {
        Ok(self.state.playback_status())
    }
    fn present_frame(&mut self) -> Result<(), Self::Error> {
        self.state.present_frame(self.audio);
        Ok(())
    }
    fn close_title(&mut self) -> Result<(), Self::Error> {
        self.state.close_title(self.audio);
        Ok(())
    }
    fn restore_display(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn draw_preamble(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TitleHost for SdlPlatform {
    /// The same keyboard the first screen factory attached: the adapter
    /// cache entry is created once and never cleared by screen teardown.
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        FrontendFactoryHost::enumerate_calibrated_devices(&mut self.factory(), input)
    }
}

/// The text-entry constructor's identity read (00459171): 00463550 for
/// phase 9, 00463D10 for phases 10/11, through the process caches. A cached
/// read never consults the sidecar: after the player submenu's 00463D10 set
/// the loaded flag, phase 9 copies the still-empty name cache. `slot` is
/// 0055F2F4, which numbers the `%dup` initials default: 1 from boot, and in a
/// network game's phase 9 the slot the transport gave.
pub fn read_identity(
    identity: &mut PlayerIdentity,
    settings: &FrontendSettings,
    phase: SessionPhase,
    slot: u8,
) -> Result<LoadedIdentity, FrontendRuntimeError> {
    let kind = IdentityRead::for_phase(phase).ok_or(FrontendRuntimeError::Unsupported(
        "identity reads outside the text-entry phases",
    ))?;
    identity
        .read(kind, &mut SidecarIdentityStore(settings), u32::from(slot))
        .map_err(|never| match never {})
}

/// Text-entry process effects over the settings sidecar, the identity caches
/// and the loop flag.
pub struct SettingsTextEntryPlatform<'a> {
    pub settings: &'a mut FrontendSettings,
    pub identity: &'a mut PlayerIdentity,
    pub exit_requested: &'a mut bool,
}

impl TextEntryPlatform for SettingsTextEntryPlatform<'_> {
    type Error = FrontendRuntimeError;
    fn name_entry_effect_quantum(&self) -> Option<u32> {
        self.settings
            .fixes()
            .exact_frontend_ticks
            .then_some(crate::frontend_settings::EXACT_WORLD_TICK_MICROS)
    }
    /// 004633D0/00463490 write these identity values, not ordinary settings.
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        self.settings.persist_identity(request)?;
        // 00463478: only a successful write clears 0054EC59; the writers
        // throw before it on failure and never update the caches.
        self.identity.clear_loaded();
        Ok(())
    }
    /// 0045AF02..0045AF12: `or [00463AB0()+0x84], bit`, without persisting.
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error> {
        self.settings.words_mut()[0x84 / 4] |= bit;
        Ok(())
    }
    fn request_exit(&mut self) -> Result<(), Self::Error> {
        *self.exit_requested = true;
        Ok(())
    }
}
