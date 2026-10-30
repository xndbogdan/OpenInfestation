//! The executable's pre-game window, after the V2000 port's launcher: the
//! installation and its file check, then Play, Options or Quit, all before
//! SDL starts. This is port setup policy, not a retail screen; the checks
//! and preferences belong to [`inf_game::launcher_setup`]. The controls keep
//! the classic Windows look and keyboard.

use std::path::PathBuf;
use std::time::Duration;

use inf_game::launcher_setup::LauncherPreferences;

#[derive(Debug)]
pub struct LauncherRequest {
    /// The installation the window opens with; it may be incomplete.
    pub root: PathBuf,
    pub preferences: LauncherPreferences,
    pub preferences_path: PathBuf,
    /// An integration smoke's timeout: the window then closes as Quit.
    pub smoke_timeout: Option<Duration>,
}

mod choices;
mod header_animation;
#[cfg(not(windows))]
mod linux;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod win32;

/// Play's installation, or None after Quit.
pub fn run(request: LauncherRequest) -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        native::run(request)
    }
    #[cfg(not(windows))]
    {
        linux::run(request)
    }
}

/// Use the caller's console for command-line output, without creating one;
/// redirected standard streams stay with the caller's pipes or files.
pub fn prepare_console() {
    #[cfg(windows)]
    native::attach_parent_console();
}

/// Make the process per-monitor DPI aware before the launcher's first
/// window, as SDL's hint would at its start: the game measures windows,
/// modes and the desktop in physical pixels.
pub fn declare_dpi_awareness() {
    #[cfg(windows)]
    native::declare_dpi_awareness();
}

/// A desktop start may have no console: show the error in a message box.
pub fn show_error(message: &str) {
    #[cfg(windows)]
    native::show_error(message);
    #[cfg(not(windows))]
    linux::show_error(message);
}
