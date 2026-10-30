//! Portable persistence of the native frontend settings state.
//!
//! Startup initializer 00463A80 -> 00463A90 copies 34 DWORDs from 00538CD0
//! to 0054E8A8. This is not the conditional reset at 00463B70.
//! The initializer is registered at 0052E0D8 in the startup function-pointer
//! range 0052E000..0052E190 executed by 00512292 -> 0051237E.
//! 00463100 persists the name/offset records at 00526C50..00526D60; 00445570
//! persists 44 keyboard selections and 16 button actions. The native table
//! names +10 twice and does not persist +74; this sidecar retains all 34 words.
//!
//! A missing sidecar uses the original startup defaults, with the port's
//! fresh-install display, Filtering and Detail Level choices. An existing
//! invalid file is an error; the launcher's [`repair`] and [`reset`] fix
//! one. The port never reads or writes the retail
//! registry or Infest.sav. Keyboard/button selections follow 00445570/00445680:
//! each row stores its first code as a key name and each bound button its
//! action name; the load resolves names, and an unnamed button keeps its
//! previous value. The phase-12 screen edits them through
//! [`crate::key_bindings`]. Persisting
//! a word does not claim its renderer/gameplay effect is connected.
//!
//! Player identity (`00463550`/`00463D10` reads, `004633D0`/`00463490`
//! writes) lives in the Windows registry under
//! `HKCU\Software\Frontier Developments Ltd\Infestation` (`Player Name`,
//! `Net Player Name`, and `Player Initials` values, with a legacy HKLM key on
//! query failure). The port keeps the byte strings in this sidecar instead
//! and never touches the registry. Frontend constructor 004838A0 calls
//! 00463100 at 00483B41, creating the HKCU key before name entry. A missing sidecar
//! value therefore follows the opened-key default (`Jameson`, `1up` offline),
//! not the `Novice` fallback for a failed registry-key open. The display
//! caches start loader-zero and resolve through fallback chains: the net-name
//! cache reads the net name, then the name cache, then `Jameson`, while an
//! empty name cache reads back as `Jameson`. The retail `%dup` initials
//! default (`sprintf("%dup", 0055F2F4)`, 004636AE) is recovered: standard
//! CRT `%d` plus the literal `up`, with 0055F2F4 1 from boot, so a missing
//! or empty value reads back as `1up` after a successful key open.
//! The native cache trio, loaded flag, and exact read sequence live in
//! [`crate::player_identity`].

use std::{
    convert::Infallible,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::display_selection::{original_size, tier_for, DisplayMode, DisplaySelection};
use crate::frontend_assets::FrontendAssets;
use crate::key_bindings::{
    KeyBindingError, KeyBindingTables, KeyBindings, JOYSTICK_BUTTONS, KEY_BINDING_ROWS,
};
use crate::model_vertices::MirrorCopySource;
use crate::name_entry::{IdentityInitialsWrite, IdentityTarget, IdentityWriteRequest};
use crate::object_runtime::VehiclePositionStep;
use crate::player_identity::{
    IdentityStore, NET_PLAYER_NAME_VALUE, PLAYER_INITIALS_VALUE, PLAYER_NAME_VALUE,
};
use crate::presentation::{ClassicScaling, RendererChoice};

pub const SETTINGS_WORDS: usize = 34;
/// Native base of the 34DWORD live state at 0054E8A8. Settings value rows
/// address their DWORD cells with absolute addresses into this state.
pub const SETTINGS_STATE_ADDRESS: u32 = 0x0054_E8A8;
/// The Video menu's Detail Level cell (`+70`): 0 draws the coarse ground,
/// 1 the fine one (004502C0; H14 flag `0x2000` forces coarse).
pub const DETAIL_CELL: usize = 0x70 / 4;
/// The port's Detail Level default (2026-10-27): High. Retail's
/// authored word at 00538CD0 is 0, the coarse ground; the port starts fresh
/// installs and Restore Defaults on the fine ground instead. A saved file
/// keeps whatever detail it holds, Low included.
pub const DETAIL_DEFAULT: u32 = 1;
const DEFAULTS_ADDRESS: u32 = 0x0053_8cd0;
const SCHEMA_VERSION: u32 = 11;

/// The settings words whose consumers bound them, and each one's highest
/// value: the two volumes, the display tier, Filtering (`+3C`, to Vulkan's
/// Anisotropic 16x), the language and Camera Style (`+6C`, to the port's
/// Remastered). Unconnected fields stay raw DWORDs rather than receiving
/// guessed boolean/ranges.
const WORD_LIMITS: [(u8, u32); 6] = [
    (0x00, 15),
    (0x04, 15),
    (0x0c, 4),
    (0x3c, crate::frontend_video::FILTERING_VULKAN_MOST),
    (0x64, 255),
    (0x6c, crate::camera_clearance::CAMERA_STYLE_REMASTERED),
];

#[derive(Debug, Error)]
pub enum FrontendSettingsError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("invalid original settings evidence at {address:#010x}: {reason}")]
    Evidence { address: u32, reason: &'static str },
    #[error("settings sidecar must have a .json filename: {0}")]
    Path(PathBuf),
    #[error("settings sidecar {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid settings sidecar {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported settings schema {0}")]
    Schema(u32),
    #[error("settings sidecar has {0} native words; expected 34")]
    WordCount(usize),
    #[error("settings word +{offset:02X} has unsupported value {value}")]
    Value { offset: u8, value: u32 },
    #[error(transparent)]
    KeyBindings(#[from] KeyBindingError),
    #[error("settings sidecar key row {label_resource}/{action_index} is not an authored row")]
    KeyRow {
        label_resource: i16,
        action_index: u16,
    },
    #[error("settings sidecar button {0} is outside 1..16")]
    Button(u8),
}

/// Native registry metadata. Names preserve original capitalization and order,
/// including the two differently capitalized labels for offset +10.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSettingDefinition {
    pub name: String,
    pub byte_offset: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeySelection {
    label_resource: i16,
    action_index: u16,
    native_code: u8,
    key_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ButtonAction {
    /// 00445570 formats its registry names with indices 1..16.
    button: u8,
    descriptor_address: u32,
    action_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    schema: u32,
    native_words: Vec<u32>,
    key_selections: Vec<KeySelection>,
    button_actions: Vec<ButtonAction>,
    /// Schema 2. Missing (older sidecars) loads as unset identity.
    #[serde(default)]
    player_name: Option<Vec<u8>>,
    /// Schema 2. Missing (older sidecars) loads as unset identity.
    #[serde(default)]
    player_initials: Option<Vec<u8>>,
    /// Schema 3. Missing (older sidecars) loads as unset identity.
    #[serde(default)]
    net_player_name: Option<Vec<u8>>,
    /// Schema 4: portable SDL output description replacing Current Display.
    #[serde(default)]
    current_display: Option<Vec<u8>>,
    /// Schema 5: the former port output size and scaling. Loading folds it
    /// into `display` and `scaling`; it is never written again.
    #[serde(default, skip_serializing)]
    output_presentation: Option<FormerOutputPresentation>,
    /// Schema 6: the exact resolution and Borderless, beside native `+0C`
    /// (its tier) and `+40` (Full Screen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display: Option<DisplayPreference>,
    /// Schema 6: completed-frame scaling (schema 5 kept it in
    /// `output_presentation`). Schema 8 adds Widescreen, the default.
    #[serde(default)]
    scaling: ClassicScaling,
    /// Schema 7: the port's fixes for retail bugs. Older sidecars load with
    /// every fix on.
    #[serde(default)]
    fixes: PortFixes,
    /// Schema 9: the port's renderer. Older sidecars load with Auto.
    #[serde(default)]
    renderer: RendererChoice,
    /// Schema 10: the pad's layout. Older sidecars load PC original.
    #[serde(default)]
    controller_layout: crate::platform_pad::ControllerLayout,
    /// Schema 11: the Vulkan renderer's modern mode. Older sidecars load
    /// path tracing on and no upscaler.
    #[serde(default)]
    modern: crate::modern_settings::ModernSettings,
}

impl SettingsFile {
    /// Bring an older sidecar to this schema's meaning: schema 5's output
    /// folds into the display, and before schema 8 `four-three` was the
    /// default fit, so it loads as today's default, Widescreen. Stretched
    /// and integer stay as chosen.
    fn migrate(&mut self) {
        self.fold_output_presentation();
        if self.schema < 8 && self.scaling == ClassicScaling::FourThree {
            self.scaling = ClassicScaling::Widescreen;
        }
    }

    /// Schema 5 output folds into the display: authored output keeps the
    /// `+0C` tier's own size, 720p and 1080p become exact resolutions.
    fn fold_output_presentation(&mut self) {
        let Some(former) = self.output_presentation.take() else {
            return;
        };
        if self.display.is_none() {
            let tier = self.native_words.get(0x0c / 4).copied().unwrap_or(0);
            let size = match former.resolution {
                FormerOutputResolution::Authored => original_size(tier),
                FormerOutputResolution::Hd720 => [1280, 720],
                FormerOutputResolution::Hd1080 => [1920, 1080],
            };
            self.display = Some(DisplayPreference {
                width: size[0],
                height: size[1],
                borderless: false,
            });
            self.scaling = former.scaling;
        }
    }
}

/// Schema 5's port output, read only to fold it into schema 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormerOutputPresentation {
    #[serde(default)]
    resolution: FormerOutputResolution,
    #[serde(default)]
    scaling: ClassicScaling,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FormerOutputResolution {
    #[default]
    Authored,
    #[serde(rename = "720p")]
    Hd720,
    #[serde(rename = "1080p")]
    Hd1080,
}

/// The port's fixes for retail bugs, beside the native words. Each is on by
/// default; turning one off restores retail's behavior. The launcher's
/// Options choose them, and the command line overrides them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortFixes {
    /// Carry the fractions the vehicle drive's position step drops, so the
    /// ACAM does not hang above the ground at high frame rates. See
    /// [`VehiclePositionStep`].
    #[serde(default = "fix_enabled")]
    pub vehicle_position_carry: bool,
    /// Join the ACAM's right-side gun mounts to the right of the hull, so no
    /// black band crosses its roof while it turns into or out of the
    /// hovercraft. See [`MirrorCopySource`].
    #[serde(default = "fix_enabled")]
    pub vehicle_gun_mounts: bool,
    /// Draw the settled hovercraft's rear fans the right way out. Its model
    /// (`hoverjet2`) mirrors the basis it saves for them (orientation
    /// `0x20`), so every face culls inside out: the fans' black insides
    /// cover them, unless the ACAM steers hard (another model). The fans
    /// turn with the steer as retail turns them, then take their X axis
    /// back. See [`crate::model_execution::hovercraft_fans_unmirrored`].
    #[serde(default = "fix_enabled")]
    pub hovercraft_fans: bool,
    /// Run the world in whole [`EXACT_WORLD_TICK_MICROS`] steps, carrying
    /// the rest of each frame's delta: the world's `delta >> n` steps
    /// (gravity's `>> 10`, timers' `>> 12` and `>> 13`) lose nothing, and
    /// its per-tick constants (the drive's drag, the VTOL's climb) run at
    /// most about 61 times a second, as retail's did at 60 fps. See
    /// [`crate::session_platform::WorldTickQuantum`].
    #[serde(default = "fix_enabled")]
    pub exact_world_ticks: bool,
    /// Draw a database entry's model from a world that loads it when the
    /// current world does not, instead of the entry's text alone. See
    /// [`crate::session_platform::DatabaseModels`].
    #[serde(default = "fix_enabled")]
    pub database_models: bool,
    /// Run the frontend screens' per-tick animation steps in whole
    /// [`EXACT_WORLD_TICK_MICROS`] steps: the name entry's row animation and
    /// glyph effects (a flashed glyph still fades, the flashes keep their
    /// rate) and the language carousel's easing.
    #[serde(default = "fix_enabled")]
    pub exact_frontend_ticks: bool,
    /// Animate the guided missiles' mounts through a change between the
    /// ground forms (buggy, truck) and the others (hovercraft, VTOL,
    /// skimmer): the roof launcher folds away with the old form and the
    /// side launchers come out with the new, where retail swaps them at
    /// once. Only the drawing changes. See
    /// [`crate::object_runtime::guided_mount_deploy`].
    #[serde(default = "fix_enabled")]
    pub guided_mounts: bool,
    /// Give every game the campaign's completion bonus, the Cranial
    /// Implanter at each respawn, while a save in the list has finished the
    /// campaign. Retail keeps the bonus only as settings +84 bit 0x40, which
    /// finishing sets: a save carried to another installation, or settings
    /// reset, leave the finished saves without it. See
    /// [`PortFixes::respawn_cheat_word`].
    #[serde(default = "fix_enabled")]
    pub completion_bonus: bool,
    /// Update the scientists, the alien ships and the creatures at retail's
    /// 30 fps step, every [`PACED_ACTOR_MICROS`] of world time: at 60 fps
    /// and above retail's scientists stand where they should walk and run,
    /// its ships fly at half speed, and its creatures walk slower and, in
    /// contact, call twice as often. See
    /// [`crate::session_platform::ActorPacing`].
    #[serde(default = "fix_enabled")]
    pub paced_actors: bool,
    /// Let at most [`SPLASH_VOICES`] of retail's 24 voices be water
    /// splashes. Retail's pool, full, silences the oldest one-shot for each
    /// new sound; hovering over water keeps about 20 splashes sounding (the
    /// spray's drop count is at its peak above 29 fps), so a generator's
    /// explosion stops halfway, in retail too. The pool stays retail's 24:
    /// raising it to 64 changed which sounds a busy fight let through (the
    /// user, 2026-10-26: "it creates unintended effects"). The pool decides
    /// only what is heard: no simulation reads it.
    #[serde(default = "fix_enabled")]
    pub splash_voice_cap: bool,
    /// Complete a rescue when its last scientist boards the transport
    /// (Pillier Stren's), granting the transport's `+7A` there. Retail
    /// grants it only as the transport leaves, which waits for the
    /// player's unit to come within the transport's reach `+240` after its
    /// last touch, so the message can come late or not at all. Applies on
    /// every renderer; off in a network game. See
    /// [`crate::object_runtime::ObjectArena::selector35_grant_rescue`].
    #[serde(default = "fix_enabled")]
    pub mission_completion: bool,
    /// Open every row of the computer's Cheats Menu, as if every name cheat
    /// that opens one had been entered, without changing the saved cheat
    /// word. Off by default: retail opens a row only for its name. See
    /// [`PortFixes::menu_cheat_word`].
    #[serde(default)]
    pub all_cheats: bool,
    /// Pelagia, the prototype world: the
    /// Command Centre's dead warpgate wakes for a scientist the player
    /// drops by it and leads to world 1, named, briefed and given a way
    /// back. Off, world 1 and the gate are retail's. See [`crate::pelagia`].
    #[serde(default = "fix_enabled")]
    pub prototype_world: bool,
    /// Pelagia's rework: world 1 gains its
    /// lagoon, egg crystals and tokens, return gate and factory stock.
    /// Off, world 1 is retail's, with only the port's briefing and name.
    /// See [`crate::pelagia`].
    #[serde(default = "fix_enabled")]
    pub pelagia_rework: bool,
}

/// Settings +84's bit for the campaign's completion bonus: while it is set,
/// each respawn grants the Cranial Implanter (0046ADF6).
pub const COMPLETION_BONUS: u32 = 0x40;

/// Settings +84's bits that open the Cheats Menu's rows (`00413410`):
/// `GIVE IT TO ME BABY` (1), `INVINCE ME` (2), `SEMPER UBI SUB UBI` (4) and
/// `I AM HUNGRY` (0x20).
pub const CHEATS_MENU_BITS: u32 = 0x27;

/// [`PortFixes::exact_world_ticks`]'s step: 2^14 microseconds, so every shift
/// of the delta up to 14 bits is exact and the world ticks at most about 61
/// times a second.
pub const EXACT_WORLD_TICK_MICROS: u32 = 16384;

/// [`PortFixes::splash_voice_cap`]' limit on splashes playing at once: past
/// it a new splash is dropped, and the playing ones finish.
pub const SPLASH_VOICES: usize = 12;

/// [`PortFixes::paced_actors`]'s step: two exact world ticks, 30.5 updates
/// a second.
pub const PACED_ACTOR_MICROS: u32 = 2 * EXACT_WORLD_TICK_MICROS;

fn fix_enabled() -> bool {
    true
}

impl Default for PortFixes {
    fn default() -> Self {
        Self {
            vehicle_position_carry: true,
            vehicle_gun_mounts: true,
            hovercraft_fans: true,
            exact_world_ticks: true,
            database_models: true,
            exact_frontend_ticks: true,
            guided_mounts: true,
            completion_bonus: true,
            paced_actors: true,
            splash_voice_cap: true,
            mission_completion: true,
            all_cheats: false,
            prototype_world: true,
            pelagia_rework: true,
        }
    }
}

impl PortFixes {
    /// The drive's position step these fixes select.
    pub fn vehicle_position_step(self) -> VehiclePositionStep {
        if self.vehicle_position_carry {
            VehiclePositionStep::CarryRemainder
        } else {
            VehiclePositionStep::Retail
        }
    }

    /// The world tick's quantum these fixes select.
    pub fn world_tick_quantum(self) -> crate::session_platform::WorldTickQuantum {
        if self.exact_world_ticks {
            crate::session_platform::WorldTickQuantum::Exact {
                micros: EXACT_WORLD_TICK_MICROS,
            }
        } else {
            crate::session_platform::WorldTickQuantum::Retail
        }
    }

    /// The live splash voices these fixes allow: retail has no such cap.
    pub fn splash_voice_limit(self) -> Option<usize> {
        self.splash_voice_cap.then_some(SPLASH_VOICES)
    }

    /// The scientists', the alien ships' and the creatures' update step these
    /// fixes select.
    pub fn actor_pacing(self) -> crate::session_platform::ActorPacing {
        if self.paced_actors {
            crate::session_platform::ActorPacing::Paced {
                micros: PACED_ACTOR_MICROS,
            }
        } else {
            crate::session_platform::ActorPacing::Retail
        }
    }

    /// Settings +84 as the respawn reads it. With `completion_bonus`, a save
    /// list holding `completed_campaigns` finished campaigns sets
    /// [`COMPLETION_BONUS`] as finishing one does.
    pub fn respawn_cheat_word(self, cheats_84: u32, completed_campaigns: usize) -> u32 {
        if self.completion_bonus && completed_campaigns > 0 {
            cheats_84 | COMPLETION_BONUS
        } else {
            cheats_84
        }
    }

    /// Settings +84 as Tab's computer and its Cheats Menu read it. With
    /// `all_cheats`, every [`CHEATS_MENU_BITS`] bit is set.
    pub fn menu_cheat_word(self, cheats_84: u32) -> u32 {
        if self.all_cheats {
            cheats_84 | CHEATS_MENU_BITS
        } else {
            cheats_84
        }
    }

    /// Opcode 2A's mirror copy these fixes select.
    pub fn model_mirror_copy(self) -> MirrorCopySource {
        if self.vehicle_gun_mounts {
            MirrorCopySource::MirrorSide
        } else {
            MirrorCopySource::Partner
        }
    }
}

/// The port's display choice beside the native words: the exact resolution
/// in physical pixels and whether a covering `+40` means Borderless.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplayPreference {
    width: u32,
    height: u32,
    #[serde(default)]
    borderless: bool,
}

impl DisplayPreference {
    /// A window at the tier's own size, as an authored setting has.
    fn authored(tier: u32) -> Self {
        let [width, height] = original_size(tier);
        Self {
            width,
            height,
            borderless: false,
        }
    }
}

#[derive(Debug)]
pub struct FrontendSettings {
    path: PathBuf,
    words: [u32; SETTINGS_WORDS],
    /// 00538CD0's words with the port's startup choices: Detail Level High
    /// ([`DETAIL_DEFAULT`]), not retail's Low.
    startup_defaults: [u32; SETTINGS_WORDS],
    definitions: Vec<NativeSettingDefinition>,
    binding_tables: KeyBindingTables,
    bindings: KeyBindings,
    player_name: Option<Vec<u8>>,
    player_initials: Option<Vec<u8>>,
    net_player_name: Option<Vec<u8>>,
    current_display: Option<Vec<u8>>,
    display: DisplayPreference,
    scaling: ClassicScaling,
    fixes: PortFixes,
    renderer: RendererChoice,
    controller_layout: crate::platform_pad::ControllerLayout,
    modern: crate::modern_settings::ModernSettings,
    /// The desktop of the display's monitor, which Borderless covers. The
    /// display host sets it whenever it reads the monitor; never saved.
    desktop: Option<[u32; 2]>,
    /// The highest Filtering value (`+3C`) the running renderer offers;
    /// never saved. A higher saved value stays saved and draws as this.
    filtering_most: u32,
    /// What the renderer can run of the modern mode (everything until the
    /// device says).
    modern_support: crate::modern_settings::ModernSupport,
    /// Whether the port's sounds (`Remastered.pak`) were found; never saved.
    ambience_sounds: bool,
    /// The renderer running, which falls back from a Vulkan that fails to
    /// start; none before the window opens; never saved.
    running: Option<crate::presentation::RendererBackend>,
    /// The game the session plays, decided when its window opens; never
    /// saved.
    game: crate::presentation::SessionGame,
    /// A replay's recorded frame in place of this run's; never saved.
    frame_pin: Option<crate::presentation::FramePin>,
    /// Session byte +65C: 00438749 sets it only when the 004631E0 registry
    /// load returns, so here only when the sidecar existed at startup.
    /// Later persistence never sets it.
    loaded_at_startup: bool,
    /// A fresh install's words and display, checkpointed by a device or
    /// identity write when no file exists yet, so the first such write keeps
    /// Borderless and Anisotropic 8x. Live edits never leak into it; with a
    /// file the checkpoint comes from disk instead.
    unsaved_defaults: Option<([u32; SETTINGS_WORDS], DisplayPreference)>,
}

impl FrontendSettings {
    /// The settings the game reads and writes unless `--settings` names
    /// another: `settings.json` beside the game's executable (since 2026-10-27), or in the working folder when it cannot say where it is.
    pub fn default_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|folder| folder.join("settings.json")))
            .unwrap_or_else(|| PathBuf::from("settings.json"))
    }

    pub fn load_or_defaults(
        path: impl Into<PathBuf>,
        assets: &FrontendAssets,
    ) -> Result<Self, FrontendSettingsError> {
        let mut settings = Self::defaults(path.into(), assets)?;
        match fs::read(&settings.path) {
            Ok(bytes) => settings.apply_saved(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // 2026-10-27: a fresh install opens Borderless over
                // the desktop, not retail's authored Full Screen. A saved file
                // keeps its own word, retail's Full Screen included.
                settings.start_borderless();
                // And it filters Anisotropic 8x once Vulkan runs, not
                // retail's Bilinear; elsewhere it still draws Bilinear.
                settings.words[crate::frontend_video::FILTERING_CELL] =
                    crate::frontend_video::FILTERING_FRESH_DEFAULT;
                settings.unsaved_defaults = Some((settings.words, settings.display));
            }
            Err(source) => return Err(settings.io_error(settings.path.clone(), source)),
        }
        settings.validate_values()?;
        Ok(settings)
    }

    /// The settings before any saved file: the executable's authored words,
    /// the port's starting choices and the original key bindings.
    fn defaults(path: PathBuf, assets: &FrontendAssets) -> Result<Self, FrontendSettingsError> {
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            return Err(FrontendSettingsError::Path(path));
        }
        let source = FrontendExecutable::parse(assets.executable_bytes())?;
        let mut words = [0; SETTINGS_WORDS];
        for (index, value) in words.iter_mut().enumerate() {
            *value = source.word(DEFAULTS_ADDRESS + index as u32 * 4)?;
        }
        // The port starts on the fine ground (2026-10-27), where
        // retail's authored word is Low. A saved file keeps its own word.
        words[DETAIL_CELL] = DETAIL_DEFAULT;
        let binding_tables = KeyBindingTables::from_executable(source, assets.key_names())?;
        let settings = Self {
            path,
            words,
            startup_defaults: words,
            definitions: setting_definitions(source)?,
            bindings: KeyBindings::initial(&binding_tables),
            binding_tables,
            player_name: None,
            player_initials: None,
            net_player_name: None,
            current_display: None,
            display: DisplayPreference::authored(words[0x0c / 4]),
            scaling: ClassicScaling::default(),
            fixes: PortFixes::default(),
            renderer: RendererChoice::default(),
            controller_layout: Default::default(),
            modern: Default::default(),
            desktop: None,
            filtering_most: crate::frontend_video::FILTERING_RETAIL_MOST,
            modern_support: crate::modern_settings::ModernSupport::default(),
            ambience_sounds: false,
            running: None,
            game: Default::default(),
            frame_pin: None,
            loaded_at_startup: false,
            unsaved_defaults: None,
        };
        Ok(settings)
    }

    /// A saved file's settings over the defaults, exactly: an unknown field,
    /// a schema this build does not read or a binding row it does not know
    /// is an error.
    fn apply_saved(&mut self, bytes: &[u8]) -> Result<(), FrontendSettingsError> {
        let mut saved: SettingsFile =
            serde_json::from_slice(bytes).map_err(|source| FrontendSettingsError::Json {
                path: self.path.clone(),
                source,
            })?;
        saved.migrate();
        // Older sidecars predate player-identity custody and load with
        // unset identity; anything else must match exactly.
        if !(1..=SCHEMA_VERSION).contains(&saved.schema) {
            return Err(FrontendSettingsError::Schema(saved.schema));
        }
        self.load_bindings(&saved.key_selections, &saved.button_actions)?;
        self.words = saved
            .native_words
            .try_into()
            .map_err(|words: Vec<u32>| FrontendSettingsError::WordCount(words.len()))?;
        self.player_name = saved.player_name;
        self.player_initials = saved.player_initials;
        self.net_player_name = saved.net_player_name;
        self.current_display = saved.current_display;
        // A sidecar without a display had the tier's own size.
        self.display = saved
            .display
            .unwrap_or_else(|| DisplayPreference::authored(self.words[0x0c / 4]));
        self.scaling = saved.scaling;
        self.fixes = saved.fixes;
        self.renderer = saved.renderer;
        self.controller_layout = saved.controller_layout;
        self.modern = saved.modern;
        // The retail tier word follows the exact resolution.
        self.words[0x0c / 4] = tier_for([self.display.width, self.display.height]);
        self.loaded_at_startup = true;
        Ok(())
    }

    pub fn words(&self) -> &[u32; SETTINGS_WORDS] {
        &self.words
    }

    pub fn key_binding_tables(&self) -> &KeyBindingTables {
        &self.binding_tables
    }

    pub fn key_bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    /// The live records the phase-12 screen edits.
    pub fn key_bindings_mut(&mut self) -> (&KeyBindingTables, &mut KeyBindings) {
        (&self.binding_tables, &mut self.bindings)
    }

    /// 00445680: a key name rewrites its row's first code with the first
    /// code of that name; a button name its descriptor. Unmatched names
    /// keep the startup records, as the native search falls through.
    fn load_bindings(
        &mut self,
        keys: &[KeySelection],
        buttons: &[ButtonAction],
    ) -> Result<(), FrontendSettingsError> {
        for key in keys {
            let authored = self.binding_tables.rows.iter().any(|row| {
                row.label_resource == key.label_resource && row.action_index == key.action_index
            });
            if !authored {
                return Err(FrontendSettingsError::KeyRow {
                    label_resource: key.label_resource,
                    action_index: key.action_index,
                });
            }
            if let Some(code) = self.binding_tables.code_for_name(key.key_name.as_bytes()) {
                self.bindings.set_first_code(key.action_index, code)?;
            }
        }
        for button in buttons {
            let index = usize::from(button.button)
                .checked_sub(1)
                .filter(|index| *index < JOYSTICK_BUTTONS)
                .ok_or(FrontendSettingsError::Button(button.button))?;
            if let Some(descriptor) = self
                .binding_tables
                .descriptor_for_name(button.action_name.as_bytes())
            {
                self.bindings.set_button(index, descriptor);
            }
        }
        Ok(())
    }

    /// 00445570's 44 key values: the name of each row's first code.
    fn stored_keys(&self) -> Vec<KeySelection> {
        (0..KEY_BINDING_ROWS)
            .map(|index| {
                let row = self.binding_tables.rows[index];
                let code = self
                    .bindings
                    .record(row.action_index)
                    .expect("authored action")
                    .codes[0];
                let name = self.binding_tables.key_name(code).unwrap_or_default();
                KeySelection {
                    label_resource: row.label_resource,
                    action_index: row.action_index,
                    native_code: code,
                    key_name: String::from_utf8_lossy(name).into_owned(),
                }
            })
            .collect()
    }

    /// 00445570's button values: a named descriptor is written; an unnamed
    /// one skips RegSetValueExA, so the previous value stays.
    fn stored_buttons(&self, previous: &[ButtonAction]) -> Vec<ButtonAction> {
        let live = self.bindings.buttons();
        (0..JOYSTICK_BUTTONS)
            .filter_map(|index| {
                let button = index as u8 + 1;
                match self.binding_tables.action_name(live[index]) {
                    Some(name) => Some(ButtonAction {
                        button,
                        descriptor_address: live[index],
                        action_name: String::from_utf8_lossy(name).into_owned(),
                    }),
                    None => previous
                        .iter()
                        .find(|entry| entry.button == button)
                        .cloned(),
                }
            })
            .collect()
    }

    /// Session byte +65C, the startup screen's readiness test.
    pub fn loaded_at_startup(&self) -> bool {
        self.loaded_at_startup
    }

    /// Settings +64, the raw language word the startup screen selects.
    pub fn language(&self) -> u32 {
        self.words[0x64 / 4]
    }

    /// 004389B0: settings +64 above 5 selects language 0.
    pub fn residency_language(&self) -> u8 {
        match self.language() {
            language @ 0..=5 => language as u8,
            _ => 0,
        }
    }

    /// 00433796..004337AF: the accepted language goes to +64 before the
    /// 004EE160 reselect, which receives +0C clamped to 4.
    pub fn set_language(&mut self, language: u32) {
        self.words[0x64 / 4] = language;
    }

    /// A settings file that does not exist yet: retail's authored words, with
    /// the Display row on Borderless (port word `+40` 1, which retail's own 1
    /// already covers). Nothing is persisted here; the first change owns the
    /// file.
    fn start_borderless(&mut self) {
        self.words[0x40 / 4] = DisplayMode::Borderless.full_screen_word();
        self.display.borderless = true;
    }

    /// 00463B70's offline branch: the 00538CD0 defaults replace the live
    /// words, except that 00463C7B saves +64 (language) and +84 (cheat word)
    /// and writes both back after its 34-DWORD copy. The port's Detail Level
    /// default is High ([`DETAIL_DEFAULT`]), where retail's authored +70 is
    /// Low; this is the port's one-word departure from 00463B70's copy. Its
    /// network-session branch copies a fixed subset, which the offline port
    /// never reaches. Nothing is persisted here.
    pub fn restore_startup_defaults(&mut self) {
        let kept = [0x64 / 4, 0x84 / 4].map(|index| (index, self.words[index]));
        self.words = self.startup_defaults;
        for (index, value) in kept {
            self.words[index] = value;
        }
        // 00463B70's own default: the authored tier's size and Full Screen,
        // not the port's first-run Borderless.
        self.display = DisplayPreference::authored(self.words[0x0c / 4]);
    }

    pub fn words_mut(&mut self) -> &mut [u32; SETTINGS_WORDS] {
        &mut self.words
    }

    /// Live settings word for an absolute value-row cell address. Aligned
    /// addresses inside the 34DWORD state resolve; anything else (video-mode
    /// cells and other owners' state) is an explicit boundary, never a
    /// neighboring word.
    pub fn setting_word(&self, address: u32) -> Option<u32> {
        setting_word_index(address).map(|index| self.words[index])
    }

    /// See [`FrontendSettings::setting_word`].
    pub fn setting_word_mut(&mut self, address: u32) -> Option<&mut u32> {
        setting_word_index(address).map(|index| &mut self.words[index])
    }

    pub fn definitions(&self) -> &[NativeSettingDefinition] {
        &self.definitions
    }

    pub fn current_display(&self) -> Option<&[u8]> {
        self.current_display.as_deref()
    }

    /// The Display and Resolution choice: native `+40` with the port's
    /// Borderless flag, and the exact resolution.
    pub fn display_selection(&self) -> DisplaySelection {
        DisplaySelection {
            mode: DisplayMode::from_words(self.words[0x40 / 4], self.display.borderless),
            size: [self.display.width, self.display.height],
        }
    }

    /// Choose a display: `+40` keeps retail's 0/1, `+0C` the tier it draws
    /// with (the desktop's in Borderless), and Borderless and the exact size
    /// stay port settings.
    pub fn set_display_selection(&mut self, selection: DisplaySelection) {
        self.words[0x40 / 4] = selection.mode.full_screen_word();
        self.words[0x0c / 4] = selection.tier(self.desktop);
        self.display = DisplayPreference {
            width: selection.size[0],
            height: selection.size[1],
            borderless: selection.mode == DisplayMode::Borderless,
        };
    }

    /// The desktop Borderless covers, as the display host last read it.
    pub fn desktop(&self) -> Option<[u32; 2]> {
        self.desktop
    }

    pub fn set_desktop(&mut self, desktop: Option<[u32; 2]>) {
        self.desktop = desktop;
    }

    /// The highest Filtering value the running renderer offers.
    pub fn filtering_most(&self) -> u32 {
        self.filtering_most
    }

    pub fn set_filtering_most(&mut self, most: u32) {
        self.filtering_most = most;
    }

    /// The Filtering the running renderer draws: the saved value, or its
    /// highest where the saved one is beyond it (a Vulkan choice under
    /// OpenGL draws as Bilinear, and stays saved for Vulkan).
    pub fn filtering(&self) -> u32 {
        self.words[crate::frontend_video::FILTERING_CELL].min(self.filtering_most)
    }

    /// Output scaling: widescreen, four-three, stretched or integer.
    pub fn scaling(&self) -> ClassicScaling {
        self.scaling
    }

    pub fn set_scaling(&mut self, scaling: ClassicScaling) {
        self.scaling = scaling;
    }

    /// The port's fixes for retail bugs.
    pub fn fixes(&self) -> PortFixes {
        self.fixes
    }

    pub fn set_fixes(&mut self, fixes: PortFixes) {
        self.fixes = fixes;
    }

    /// The port's renderer: OpenGL with the CPU raster as fallback, or one
    /// of them.
    pub fn renderer(&self) -> RendererChoice {
        self.renderer
    }

    pub fn set_renderer(&mut self, renderer: RendererChoice) {
        self.renderer = renderer;
    }

    /// How a game controller plays: as PC retail's joystick, or as the
    /// PlayStation release's pad.
    pub fn controller_layout(&self) -> crate::platform_pad::ControllerLayout {
        self.controller_layout
    }

    pub fn set_controller_layout(&mut self, layout: crate::platform_pad::ControllerLayout) {
        self.controller_layout = layout;
    }

    /// The camera the Camera Style row (`+6C`) draws: retail's, or the
    /// remastered one ([`crate::camera_clearance`]).
    pub fn camera_style(&self) -> crate::camera_clearance::CameraStyle {
        crate::camera_clearance::CameraStyle::from_word(
            self.words[crate::camera_clearance::CAMERA_STYLE_CELL],
        )
    }

    /// The Vulkan renderer's modern mode: path tracing, the upscaler and
    /// anti-aliasing.
    /// What the renderer can run of the modern mode, for its rows.
    pub fn modern_support(&self) -> crate::modern_settings::ModernSupport {
        self.modern_support
    }

    pub fn set_modern_support(&mut self, support: crate::modern_settings::ModernSupport) {
        self.modern_support = support;
    }

    /// Whether the port's sounds were found, for the Ambience row.
    pub fn ambience_sounds(&self) -> bool {
        self.ambience_sounds
    }

    pub fn set_ambience_sounds(&mut self, found: bool) {
        self.ambience_sounds = found;
    }

    pub fn set_running(&mut self, backend: crate::presentation::RendererBackend) {
        self.running = Some(backend);
    }

    pub fn modern(&self) -> crate::modern_settings::ModernSettings {
        self.modern
    }

    pub fn set_modern(&mut self, modern: crate::modern_settings::ModernSettings) {
        self.modern = modern;
    }

    /// Ultra ground detail's shape: none. The setting is retired (the
    /// user, 2026-10-26: its subdivided ground raised spikes on flat
    /// stretches) until a finer ground, tessellated from the terrain's own
    /// detail, replaces it (the port\'s plan, item 25). A saved Ultra Ground
    /// stays in the settings file and does nothing.
    pub fn ultra_ground(&self) -> Option<crate::terrain_runtime::ultra::UltraShape> {
        None
    }

    /// Wheels sink into puddles in effect: the saved choice in the
    /// remaster's game of a session that started path traced, never in
    /// retail's (OpenGL, the software renderer). The session also leaves it
    /// off in a network game, whose weather is retail's.
    pub fn puddle_wheels(&self) -> bool {
        self.modern.puddle_wheels
            && self.game.view == crate::presentation::GameView::Remaster { traced: true }
    }

    /// The game the session plays ([`crate::presentation::GameView`]).
    pub fn game_view(&self) -> crate::presentation::GameView {
        self.game.view
    }

    pub fn set_game_view(&mut self, view: crate::presentation::GameView) {
        self.game.view = view;
    }

    /// The game the session plays and what it reads of the machine.
    pub fn game(&self) -> &crate::presentation::SessionGame {
        &self.game
    }

    pub fn set_game(&mut self, game: crate::presentation::SessionGame) {
        self.game = game;
    }

    /// The `[far, past]` the traced remaster's game sees world `world` by,
    /// where the session keeps them (none: the build's tuning).
    pub fn game_far(&self, world: u32) -> Option<[f32; 2]> {
        self.game.far.as_ref().map(|far| far.of(world))
    }

    pub fn frame_pin(&self) -> Option<crate::presentation::FramePin> {
        self.frame_pin
    }

    pub fn set_frame_pin(&mut self, pin: Option<crate::presentation::FramePin>) {
        self.frame_pin = pin;
    }

    /// The modern sky in effect, so its rain and its lightning are drawn
    /// (and heard): Vulkan, traced or not, on a device that draws it.
    pub fn draws_modern_sky(&self) -> bool {
        self.renderer == crate::presentation::RendererChoice::Vulkan
            && self.modern_support.modern_sky
    }

    /// Headphones in effect: the saved choice under Vulkan, where the port's
    /// placing runs; never under OpenGL and the software renderer.
    pub fn headphones(&self) -> bool {
        self.modern.headphones && self.renderer == crate::presentation::RendererChoice::Vulkan
    }

    /// The Ambience volume in effect: the saved one under Vulkan, as chosen
    /// and running; none under OpenGL and the software renderer, which keep
    /// retail's sound, also when they stand in
    /// for a Vulkan that failed to start.
    pub fn ambience(&self) -> u8 {
        let running = self
            .running
            .is_none_or(|backend| backend == crate::presentation::RendererBackend::Vulkan);
        if self.renderer == crate::presentation::RendererChoice::Vulkan && running {
            self.modern
                .ambience
                .min(crate::modern_settings::MOST_AMBIENCE)
        } else {
            0
        }
    }

    /// 00463000 writes only Current Display immediately. Other live settings
    /// still wait for Back/00463100; preserve their last persisted values.
    /// Keep only a successful preference in memory: Back does not retry this
    /// separate native registry write after a device-selection save failed.
    pub fn persist_current_display(&mut self, name: &[u8]) -> Result<(), FrontendSettingsError> {
        let mut saved = self.persisted_checkpoint()?;
        saved.current_display = Some(up_to_nul(name).to_vec());
        self.write_file(&saved)?;
        self.current_display = saved.current_display;
        Ok(())
    }

    /// 004633D0/00463490 write the required selected name and, only for a
    /// nonNULL pointer, initials. Keep preserves saved and live initials
    /// independently. A name-entry cheat's live +84 word is not saved here.
    /// The sidecar replaces its checkpoint atomically; failures leave live
    /// identity unchanged, without claiming native per-value partial writes.
    pub fn persist_identity(
        &mut self,
        request: IdentityWriteRequest<'_>,
    ) -> Result<(), FrontendSettingsError> {
        let mut saved = self.persisted_checkpoint()?;
        let name = Some(up_to_nul(request.name).to_vec());
        match request.target {
            IdentityTarget::PlayerName => saved.player_name = name.clone(),
            IdentityTarget::NetPlayerName => saved.net_player_name = name.clone(),
        }
        if let IdentityInitialsWrite::Write(initials) = request.initials {
            saved.player_initials = Some(up_to_nul(initials).to_vec());
        }
        self.write_file(&saved)?;
        match request.target {
            IdentityTarget::PlayerName => self.player_name = name,
            IdentityTarget::NetPlayerName => self.net_player_name = name,
        }
        if let IdentityInitialsWrite::Write(_) = request.initials {
            self.player_initials = saved.player_initials;
        }
        Ok(())
    }

    /// Base for native writes that do not also save ordinary settings.
    fn persisted_checkpoint(&self) -> Result<SettingsFile, FrontendSettingsError> {
        let mut saved = match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<SettingsFile>(&bytes).map_err(|source| {
                FrontendSettingsError::Json {
                    path: self.path.clone(),
                    source,
                }
            })?,
            // Without a file there is nothing persisted yet: the fresh
            // install's own words and display, not live edits of them.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.default_file(),
            Err(source) => return Err(self.io_error(self.path.clone(), source)),
        };
        if !(1..=SCHEMA_VERSION).contains(&saved.schema) {
            return Err(FrontendSettingsError::Schema(saved.schema));
        }
        // A rewritten checkpoint keeps an older file's output as its display
        // and its scaling in today's meaning.
        saved.migrate();
        saved.schema = SCHEMA_VERSION;
        Ok(saved)
    }

    /// Raw +00 setting, before the native sound owner's packed-gain conversion.
    pub fn fx_volume(&self) -> u32 {
        self.words[0]
    }

    /// FrontendAudio may clear +04 when opening the real music transport fails.
    /// Persistence revalidates this exposed native word before touching disk.
    pub fn music_mut(&mut self) -> &mut u32 {
        &mut self.words[1]
    }

    /// Select an OVL display tier at its own size, keeping the Display mode,
    /// and the language word. The application applies the physical display
    /// and renderer settings through its separate host.
    pub fn select_presentation(
        &mut self,
        variant: u8,
        language: u8,
    ) -> Result<(), FrontendSettingsError> {
        if variant > 4 {
            return Err(FrontendSettingsError::Value {
                offset: 0x0c,
                value: u32::from(variant),
            });
        }
        let mut selection = self.display_selection();
        selection.size = original_size(u32::from(variant));
        self.set_display_selection(selection);
        self.select_language(language);
        Ok(())
    }

    /// The language word `+64`.
    pub fn select_language(&mut self, language: u8) {
        self.words[0x64 / 4] = u32::from(language);
    }

    /// Stored `Player Name` bytes, without any read-time fallback. `None`
    /// means no name value has been saved, not that the store cannot open.
    pub fn stored_player_name(&self) -> Option<&[u8]> {
        self.player_name.as_deref()
    }

    /// Stored `Player Initials` bytes, without any read-time fallback.
    pub fn stored_player_initials(&self) -> Option<&[u8]> {
        self.player_initials.as_deref()
    }

    /// `004633D0`/`00463490` write contract: retain bytes up to the first
    /// NUL, matching the native `strlen` extent. The name-entry screen
    /// enforces its own 18/4-byte limits before calling.
    pub fn set_player_name(&mut self, name: &[u8]) {
        self.player_name = Some(up_to_nul(name).to_vec());
    }

    /// See [`FrontendSettings::set_player_name`].
    pub fn set_player_initials(&mut self, initials: &[u8]) {
        self.player_initials = Some(up_to_nul(initials).to_vec());
    }

    /// Stored `Net Player Name` bytes, without any read-time fallback.
    pub fn stored_net_player_name(&self) -> Option<&[u8]> {
        self.net_player_name.as_deref()
    }

    /// Update the remembered net name using the other setters' `strlen`
    /// extent. Immediate identity publication uses [`Self::persist_identity`].
    pub fn set_net_player_name(&mut self, name: &[u8]) {
        self.net_player_name = Some(up_to_nul(name).to_vec());
    }

    /// Persist the actual owned values. Write and sync a new adjacent file,
    /// then replace the sidecar; failed writes preserve the existing file.
    pub fn persist(&self) -> Result<(), FrontendSettingsError> {
        self.validate_values()?;
        let previous_buttons = match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<SettingsFile>(&bytes)
                .map(|saved| saved.button_actions)
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let saved = SettingsFile {
            schema: SCHEMA_VERSION,
            native_words: self.words.to_vec(),
            key_selections: self.stored_keys(),
            button_actions: self.stored_buttons(&previous_buttons),
            player_name: self.player_name.clone(),
            player_initials: self.player_initials.clone(),
            net_player_name: self.net_player_name.clone(),
            current_display: self.current_display.clone(),
            output_presentation: None,
            display: Some(self.display),
            scaling: self.scaling,
            fixes: self.fixes,
            renderer: self.renderer,
            controller_layout: self.controller_layout,
            modern: self.modern,
        };
        self.write_file(&saved)
    }

    /// The file a fresh install would write: its own words and display, no
    /// binding values, every port choice at its default.
    fn default_file(&self) -> SettingsFile {
        let (native_words, display) = self.unsaved_defaults.unwrap_or((
            self.startup_defaults,
            DisplayPreference::authored(self.startup_defaults[0x0c / 4]),
        ));
        SettingsFile {
            schema: SCHEMA_VERSION,
            native_words: native_words.to_vec(),
            // An identity write creates no binding values.
            key_selections: Vec::new(),
            button_actions: Vec::new(),
            player_name: None,
            player_initials: None,
            net_player_name: None,
            current_display: None,
            output_presentation: None,
            display: Some(display),
            scaling: ClassicScaling::default(),
            fixes: PortFixes::default(),
            renderer: RendererChoice::default(),
            controller_layout: Default::default(),
            modern: Default::default(),
        }
    }

    fn write_file(&self, saved: &SettingsFile) -> Result<(), FrontendSettingsError> {
        let bytes =
            serde_json::to_vec_pretty(saved).map_err(|source| FrontendSettingsError::Json {
                path: self.path.clone(),
                source,
            })?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|source| self.io_error(parent.to_owned(), source))?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| self.io_error(temporary.clone(), source))?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()
        })();
        drop(file);
        if let Err(source) = result {
            let _ = fs::remove_file(&temporary);
            return Err(self.io_error(temporary, source));
        }
        if let Err(source) = fs::rename(&temporary, &self.path) {
            let _ = fs::remove_file(&temporary);
            return Err(self.io_error(self.path.clone(), source));
        }
        Ok(())
    }

    fn validate_values(&self) -> Result<(), FrontendSettingsError> {
        for (offset, maximum) in WORD_LIMITS {
            let value = self.words[usize::from(offset) / 4];
            if value > maximum {
                return Err(FrontendSettingsError::Value { offset, value });
            }
        }
        Ok(())
    }

    fn io_error(&self, path: PathBuf, source: std::io::Error) -> FrontendSettingsError {
        FrontendSettingsError::Io { path, source }
    }
}

pub(crate) fn setting_word_index(address: u32) -> Option<usize> {
    let offset = address.wrapping_sub(SETTINGS_STATE_ADDRESS);
    if offset % 4 != 0 {
        return None;
    }
    let index = (offset / 4) as usize;
    (index < SETTINGS_WORDS).then_some(index)
}

fn up_to_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn evidence(address: u32, reason: &'static str) -> FrontendSettingsError {
    FrontendSettingsError::Evidence { address, reason }
}

fn original_string(
    source: FrontendExecutable<'_>,
    address: u32,
) -> Result<String, FrontendSettingsError> {
    let mut bytes = Vec::new();
    for offset in 0..128 {
        let byte = source.mapped(address + offset, 1)?.bytes[0];
        if byte == 0 {
            return String::from_utf8(bytes)
                .map_err(|_| evidence(address, "non-UTF8 source label"));
        }
        bytes.push(byte);
    }
    Err(evidence(address, "unterminated source label"))
}

fn setting_definitions(
    source: FrontendExecutable<'_>,
) -> Result<Vec<NativeSettingDefinition>, FrontendSettingsError> {
    (0..34)
        .map(|index| {
            let address = 0x0052_6c50 + index * 8;
            let offset = source.word(address + 4)?;
            if offset % 4 != 0 || offset >= SETTINGS_WORDS as u32 * 4 {
                return Err(evidence(
                    address,
                    "settings offset is outside the native 0x88-byte state",
                ));
            }
            Ok(NativeSettingDefinition {
                name: original_string(source, source.word(address)?)?,
                byte_offset: offset as u8,
            })
        })
        .collect()
}

#[path = "frontend_settings_repair.rs"]
mod repair;
pub use repair::{broken_copy, diagnose, repair, reset, SettingsHealth};

#[cfg(test)]
#[path = "frontend_settings_tests.rs"]
mod tests;

/// The sidecar as the registry key that 00463550/00463D10 read through the
/// process caches in [`crate::player_identity::PlayerIdentity`]. Frontend
/// construction's 00463100 creates the HKCU key (0046312D) before any identity
/// read, so the key always opens; a missing value is a failed query, and the
/// portable store has no legacy HKLM key. 004633D0/00463490 store each value
/// with its terminator (`strlen + 1`), so a value fits a query cap only when
/// shorter than it, as RegQueryValueExA's ERROR_MORE_DATA requires.
pub struct SidecarIdentityStore<'a>(pub &'a FrontendSettings);

impl IdentityStore for SidecarIdentityStore<'_> {
    type Key = ();
    type Error = Infallible;

    fn open_user_key(&mut self) -> Result<Option<()>, Infallible> {
        Ok(Some(()))
    }
    fn open_machine_key(&mut self) -> Result<Option<()>, Infallible> {
        Ok(None)
    }
    fn query_value(
        &mut self,
        _: &(),
        value: &'static str,
        capacity: u32,
    ) -> Result<Option<Vec<u8>>, Infallible> {
        let stored = match value {
            PLAYER_NAME_VALUE => &self.0.player_name,
            NET_PLAYER_NAME_VALUE => &self.0.net_player_name,
            PLAYER_INITIALS_VALUE => &self.0.player_initials,
            _ => return Ok(None),
        };
        Ok(stored
            .as_ref()
            .filter(|bytes| bytes.len() < capacity as usize)
            .cloned())
    }
    fn close_key(&mut self, _: ()) {}
}
