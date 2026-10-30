//! Display modes and resolutions: how the game window occupies the monitor
//! chosen in the Video menu's Device row, and the sizes Resolution offers.
//!
//! Every size is in physical pixels. Native word `+40` (Display) keeps its
//! retail values, 0 for a window and 1 for full screen, and `+0C` keeps the
//! H5 display tier. Borderless and the exact size are port settings.

use serde::{Deserialize, Serialize};

use crate::menu::MenuCommand;

/// The H5 display records' sizes, tiers 0..4.
pub const ORIGINAL_SIZES: [[u32; 2]; 5] =
    [[320, 240], [512, 384], [640, 480], [800, 600], [1024, 768]];

/// The Display row's values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayMode {
    /// Retail `+40` = 0: a window whose client area is the resolution.
    #[default]
    Window,
    /// Retail `+40` = 1: the monitor switches to the resolution.
    FullScreen,
    /// Port: a borderless window over the desktop, without a mode change.
    /// Its resolution is the desktop's, which draws with the largest original
    /// inside it; the Resolution row has nothing else to offer.
    Borderless,
}

impl DisplayMode {
    pub const ALL: [Self; 3] = [Self::Window, Self::FullScreen, Self::Borderless];

    /// Display row value; 0 and 1 are retail's.
    pub const fn index(self) -> u32 {
        match self {
            Self::Window => 0,
            Self::FullScreen => 1,
            Self::Borderless => 2,
        }
    }

    pub const fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Window,
            1 => Self::FullScreen,
            _ => Self::Borderless,
        }
    }

    /// The retail word `+40`: Borderless also covers the display.
    pub const fn full_screen_word(self) -> u32 {
        match self {
            Self::Window => 0,
            Self::FullScreen | Self::Borderless => 1,
        }
    }

    /// The mode a retail word and the port's Borderless setting describe.
    pub const fn from_words(full_screen_word: u32, borderless: bool) -> Self {
        match (full_screen_word as u8, borderless) {
            (0, _) => Self::Window,
            (_, true) => Self::Borderless,
            (_, false) => Self::FullScreen,
        }
    }

    /// `00438AD0` increments `+40` and wraps after each pass.
    const fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len() as u32)
    }
}

/// The H5 tier drawing `size`: the largest original that fits it, which is
/// also the largest that fits its 4:3 area. Smaller sizes take tier 0.
pub fn tier_for(size: [u32; 2]) -> u32 {
    ORIGINAL_SIZES
        .iter()
        .rposition(|&[width, height]| width <= size[0] && height <= size[1])
        .unwrap_or(0) as u32
}

/// The original size of a tier word; values outside 0..4 use tier 4, as
/// `00438AD0` starts from `min(+0C, 4)`.
pub fn original_size(tier: u32) -> [u32; 2] {
    ORIGINAL_SIZES[(tier as usize).min(ORIGINAL_SIZES.len() - 1)]
}

/// The display a selection asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySelection {
    pub mode: DisplayMode,
    /// The selected resolution. Borderless covers the desktop and keeps this
    /// for the next window or mode.
    pub size: [u32; 2],
}

impl DisplaySelection {
    /// The Resolution row's value: the size, or in Borderless the `desktop`
    /// it covers (the size until the desktop is known).
    pub fn resolution(self, desktop: Option<[u32; 2]>) -> [u32; 2] {
        match (self.mode, desktop) {
            (DisplayMode::Borderless, Some(desktop)) => desktop,
            _ => self.size,
        }
    }

    /// The H5 tier this display draws with.
    pub fn tier(self, desktop: Option<[u32; 2]>) -> u32 {
        tier_for(self.resolution(desktop))
    }

    /// The Resolution row's text for a size without an authored string, in
    /// the authored strings' `640x480` form.
    pub fn resolution_text(self, desktop: Option<[u32; 2]>) -> Option<Vec<u8>> {
        let [width, height] = self.resolution(desktop);
        (!ORIGINAL_SIZES.contains(&[width, height]))
            .then(|| format!("{width}x{height}").into_bytes())
    }

    /// The Display row's text for Borderless, which has no authored string,
    /// in `language` (settings `+64`).
    pub fn mode_text(self, language: u8) -> Option<&'static [u8]> {
        (self.mode == DisplayMode::Borderless)
            .then(|| crate::port_text::PortText::Borderless.text(language))
    }

    /// Take `mode` with the row entry `size`. Borderless's only entry is the
    /// desktop, so it keeps the saved size, and leaving it restores that.
    pub fn adopt(&mut self, mode: DisplayMode, size: [u32; 2]) {
        self.mode = mode;
        if mode != DisplayMode::Borderless {
            self.size = size;
        }
    }
}

/// What a monitor offers, in physical pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorModes {
    /// The desktop's size, when known.
    pub desktop: Option<[u32; 2]>,
    /// Every distinct size the monitor reports, ascending.
    reported: Vec<[u32; 2]>,
}

impl MonitorModes {
    pub fn new(desktop: Option<[u32; 2]>, reported: impl IntoIterator<Item = [u32; 2]>) -> Self {
        let mut reported: Vec<_> = reported.into_iter().collect();
        reported.sort_unstable();
        reported.dedup();
        Self { desktop, reported }
    }

    pub fn reported(&self) -> &[[u32; 2]] {
        &self.reported
    }

    /// Every size `mode` can start with, ascending: the originals and the
    /// reported modes that fit the desktop for a window, the reported modes
    /// for full screen, and the desktop alone for Borderless.
    pub fn sizes(&self, mode: DisplayMode) -> Vec<[u32; 2]> {
        let mut sizes: Vec<_> = match mode {
            DisplayMode::Borderless => self.desktop.into_iter().collect(),
            DisplayMode::FullScreen => self.reported.clone(),
            DisplayMode::Window => ORIGINAL_SIZES
                .iter()
                .chain(&self.reported)
                .copied()
                .filter(|&[width, height]| {
                    self.desktop
                        .is_none_or(|desktop| width <= desktop[0] && height <= desktop[1])
                })
                .collect(),
        };
        sizes.retain(|&size| ORIGINAL_SIZES[0][0] <= size[0] && ORIGINAL_SIZES[0][1] <= size[1]);
        sizes.sort_unstable();
        sizes.dedup();
        sizes
    }

    /// The Resolution row: `sizes` without tier-0 sizes, which the row's
    /// `00462F40` never keeps (a nonpositive tier is stored as 1).
    pub fn menu_sizes(&self, mode: DisplayMode) -> Vec<[u32; 2]> {
        let mut sizes = self.sizes(mode);
        sizes.retain(|&size| tier_for(size) > 0);
        sizes
    }

    /// The row entry `size` selects: itself when listed, else the largest
    /// listed size below it, as `00438AD0` descends, else the smallest; in
    /// Borderless always the desktop. `None` for an empty row.
    pub fn resolve(&self, mode: DisplayMode, size: [u32; 2]) -> Option<[u32; 2]> {
        let sizes = self.menu_sizes(mode);
        sizes
            .iter()
            .rev()
            .copied()
            .find(|&listed| listed <= size)
            .or_else(|| sizes.first().copied())
    }

    /// The Resolution row entry after `command` from `current`, with
    /// `00453B80`'s arithmetic over the row: previous stops at the first
    /// entry, next at the last, and activate wraps to the first. A current
    /// size between entries sits between them, as tier 0 sits before tier 1.
    /// `None` when the command changes nothing.
    pub fn step(
        &self,
        mode: DisplayMode,
        current: [u32; 2],
        command: MenuCommand,
    ) -> Option<[u32; 2]> {
        let sizes = self.menu_sizes(mode);
        let last = sizes.len().checked_sub(1)?;
        // Entries below `current`; a listed `current` is entry `below`.
        let below = sizes.partition_point(|&listed| listed < current);
        let index = match (command, sizes.get(below) == Some(&current)) {
            (MenuCommand::AdjustPrevious, _) => below.checked_sub(1)?,
            (MenuCommand::AdjustNext, true) => (below + 1).min(last),
            (MenuCommand::AdjustNext, false) => below.min(last),
            (MenuCommand::Activate, true) if below < last => below + 1,
            (MenuCommand::Activate, false) if below <= last => below,
            (MenuCommand::Activate, _) => 0,
            _ => return None,
        };
        Some(sizes[index]).filter(|&size| size != current)
    }

    /// The selection after the Display row changes from `old` to `mode`:
    /// a tier-0 size takes tier 1's original size; choosing Full Screen takes
    /// the monitor's highest reported mode (2026-10-27), otherwise
    /// the entry [`Self::resolve`] gives for the old size, or the old size
    /// when the new mode's row is empty.
    pub fn select_mode(&self, old: DisplaySelection, mode: DisplayMode) -> DisplaySelection {
        if old.tier(self.desktop) == 0 {
            return DisplaySelection {
                mode,
                size: original_size(1),
            };
        }
        let mut new = old;
        if mode == DisplayMode::FullScreen {
            match self.menu_sizes(mode).into_iter().max() {
                Some(size) => new.adopt(mode, size),
                None => new.mode = mode,
            }
            return new;
        }
        match self.resolve(mode, old.size) {
            Some(size) => new.adopt(mode, size),
            None => new.mode = mode,
        }
        new
    }

    /// `00438AD0`'s order from the saved choice: the saved size and each
    /// smaller size of the saved mode, largest first, then the same from the
    /// saved size for each following mode; Borderless has only the desktop
    /// and keeps the saved size. Retail then tries the other renderer and
    /// Bilinear value; the port's renderer falls back before its display
    /// starts, and Bilinear never changes the display.
    pub fn startup_order(&self, selection: DisplaySelection) -> Vec<DisplaySelection> {
        let mut order = Vec::new();
        let mut mode = selection.mode;
        for _ in DisplayMode::ALL {
            let sizes = self.sizes(mode);
            if mode == DisplayMode::Borderless {
                if !sizes.is_empty() {
                    order.push(DisplaySelection {
                        mode,
                        size: selection.size,
                    });
                }
            } else {
                for &size in sizes.iter().rev().filter(|&&size| size <= selection.size) {
                    order.push(DisplaySelection { mode, size });
                }
            }
            mode = mode.next();
        }
        order
    }
}

#[cfg(test)]
#[path = "display_selection_tests.rs"]
mod tests;
