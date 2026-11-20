//! The pad behind the native joystick ([`crate::platform_input`]): the first
//! SDL game controller, or else the first joystick, read once a frame as
//! WinMM's `joyGetPosEx` reports it to retail (`004F54A0`).
//!
//! Windows' legacy joystick API numbers an XInput pad this way, so a mapped
//! controller gives retail's own view of it: the left stick on X and Y, the
//! triggers together on Z (the left one raising it), the right stick's
//! vertical on R and horizontal on U, and A, B, X, Y, the left and right
//! shoulders, Back, Start and the two stick clicks as buttons 1 to 10. The
//! d-pad is its point-of-view hat, which retail never reads. An unmapped
//! joystick reports its own axes and buttons in order.
//!
//! [`ControllerLayout::Console`] instead plays a mapped controller as the
//! PlayStation release's pad (SLES-01870), whose tables the PC executable
//! carries unused: a DualShock in analog mode, native type 9 (`8012D068`'s
//! fourth row, read by `800F4310`). Its codes 1 to 16 are the pad's bits
//! plus one (Select, L3, R3, Start, Up, Right, Down, Left, L2, R2, L1, R1,
//! Triangle, Circle, Cross, Square, active-low, `~(raw << 1) & 0x1FFFE`), and
//! its axes 0 to 3 the left stick's X and Y and the right stick's X and Y,
//! 0 to 255. The positional buttons stand in for the PlayStation's: Cross
//! is the bottom face button (Xbox A), Circle the right (B), Square the left
//! (X), Triangle the top (Y); the triggers, pushed past half, are L2 and R2.
//!
//! Port convenience, not retail: a pad plugged in after play starts is read
//! on the screen that is live, and a pad that goes away first sends one fully
//! released reading, so nothing it held stays held. Both follow the
//! notification stream ([`crate::platform_input`]), never SDL's hot-plug
//! events, so a recorded session replays identically. [`SdlPads::take_changes`]
//! reports the connects and removals the port's notice shows.

use sdl2::controller::{Axis, Button, GameController};
use sdl2::joystick::Joystick;
use sdl2::{GameControllerSubsystem, JoystickSubsystem};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::platform_input::{InputNotification, PadSample};

/// How the pad plays: as PC retail's joystick, or as the PlayStation
/// release's pad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerLayout {
    /// Retail's WinMM joystick, native type 2.
    #[default]
    Pc,
    /// The PlayStation pad, native type 9.
    Console,
}

impl ControllerLayout {
    pub fn native_type(self) -> u8 {
        match self {
            ControllerLayout::Pc => 2,
            ControllerLayout::Console => 9,
        }
    }
}

/// What a poll found the pad list do, for the port's notice: a pad joined or
/// left. The startup pad is not a connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PadChange {
    Connected(String),
    Removed(String),
}

/// Whether hint messages word their controls as PC retail does, or follow
/// the device the player used last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerHints {
    /// Retail's PC keyboard wording, on every layout and every device.
    #[default]
    Retail,
    /// The controller's wording after the player last used the pad.
    FollowLastInput,
}

impl ControllerHints {
    /// The controller wording applies only where it is true: the player chose
    /// to follow the last input, the pad plays the PlayStation layout whose
    /// texts the wording comes from, the player last used the pad, and the
    /// game is offline (network games play retail's game on every renderer).
    /// Every other case keeps the PC text byte for byte.
    pub fn uses_pad_wording(
        self,
        layout: ControllerLayout,
        last_input_is_pad: bool,
        network_game: bool,
    ) -> bool {
        self == Self::FollowLastInput
            && layout == ControllerLayout::Console
            && last_input_is_pad
            && !network_game
    }
}

/// The PlayStation pad's bits 0 to 15 (codes 1 to 16), minus L2 and R2,
/// which the triggers give.
const CONSOLE_BUTTONS: [(u32, Button); 14] = [
    (0, Button::Back),
    (1, Button::LeftStick),
    (2, Button::RightStick),
    (3, Button::Start),
    (4, Button::DPadUp),
    (5, Button::DPadRight),
    (6, Button::DPadDown),
    (7, Button::DPadLeft),
    (10, Button::LeftShoulder),
    (11, Button::RightShoulder),
    (12, Button::Y),
    (13, Button::B),
    (14, Button::A),
    (15, Button::X),
];
/// A trigger past half is the PlayStation's digital L2 or R2.
const TRIGGER_PRESSED: i16 = 0x4000;

/// Buttons 1 to 10 of an XInput pad under WinMM.
const XINPUT_BUTTONS: [Button; 10] = [
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::LeftShoulder,
    Button::RightShoulder,
    Button::Back,
    Button::Start,
    Button::LeftStick,
    Button::RightStick,
];
/// X, Y, Z, R and U.
const XINPUT_AXES: u8 = 5;

enum Open {
    Controller(GameController),
    Joystick(Joystick),
}

impl Open {
    fn attached(&self) -> bool {
        match self {
            Open::Controller(controller) => controller.attached(),
            Open::Joystick(joystick) => joystick.attached(),
        }
    }

    /// The native type this pad plays as: an unmapped joystick has no
    /// PlayStation layout, so it stays retail's joystick.
    fn native_type(&self, layout: ControllerLayout) -> u8 {
        match self {
            Open::Controller(_) => layout.native_type(),
            Open::Joystick(_) => ControllerLayout::Pc.native_type(),
        }
    }

    fn axes(&self, layout: ControllerLayout) -> u8 {
        match self {
            Open::Controller(_) if self.native_type(layout) == 9 => 4,
            Open::Controller(_) => XINPUT_AXES,
            Open::Joystick(joystick) => joystick.num_axes().min(6) as u8,
        }
    }

    /// A reading with nothing pressed and every stick centred.
    fn released(&self, layout: ControllerLayout) -> PadSample {
        released_sample(self.native_type(layout), self.axes(layout))
    }

    fn sample(&self, layout: ControllerLayout) -> PadSample {
        let position = |value: i16| (i32::from(value) + 0x8000) as u32;
        if let (Open::Controller(controller), 9) = (self, self.native_type(layout)) {
            let mut buttons = CONSOLE_BUTTONS.iter().fold(0, |mask, &(bit, button)| {
                mask | (u32::from(controller.button(button)) << bit)
            });
            buttons |= u32::from(controller.axis(Axis::TriggerLeft) >= TRIGGER_PRESSED) << 8;
            buttons |= u32::from(controller.axis(Axis::TriggerRight) >= TRIGGER_PRESSED) << 9;
            let byte = |axis| position(controller.axis(axis)) >> 8;
            return PadSample::Read {
                buttons,
                axes: [
                    byte(Axis::LeftX),
                    byte(Axis::LeftY),
                    byte(Axis::RightX),
                    byte(Axis::RightY),
                    0,
                    0,
                ],
            };
        }
        match self {
            Open::Controller(controller) => {
                let buttons = XINPUT_BUTTONS
                    .iter()
                    .enumerate()
                    .fold(0, |mask, (bit, &button)| {
                        mask | (u32::from(controller.button(button)) << bit)
                    });
                let triggers = i32::from(controller.axis(Axis::TriggerLeft))
                    - i32::from(controller.axis(Axis::TriggerRight));
                PadSample::Read {
                    buttons,
                    axes: [
                        position(controller.axis(Axis::LeftX)),
                        position(controller.axis(Axis::LeftY)),
                        (0x8000 + triggers).clamp(0, 0xffff) as u32,
                        position(controller.axis(Axis::RightY)),
                        position(controller.axis(Axis::RightX)),
                        0,
                    ],
                }
            }
            Open::Joystick(joystick) => {
                let buttons = (0..joystick.num_buttons().min(32)).fold(0, |mask, index| {
                    mask | (u32::from(joystick.button(index).unwrap_or(false)) << index)
                });
                let mut axes = [0; 6];
                for (index, axis) in axes
                    .iter_mut()
                    .enumerate()
                    .take(usize::from(self.axes(layout)))
                {
                    *axis = position(joystick.axis(index as u32).unwrap_or(0));
                }
                PadSample::Read { buttons, axes }
            }
        }
    }
}

/// A reading with nothing pressed and every stick centred, for a native type
/// (the PlayStation pad's axes are bytes, retail's joystick's words).
fn released_sample(native_type: u8, axes: u8) -> PadSample {
    let centre = if native_type == 9 { 0x80 } else { 0x8000 };
    let mut positions = [0; 6];
    positions[..usize::from(axes)].fill(centre);
    PadSample::Read {
        buttons: 0,
        axes: positions,
    }
}

/// The open pad and the name SDL gave it at open, kept for the removal report
/// after the device is gone.
struct OpenPad {
    handle: Open,
    name: String,
}

/// One poll's decision for [`SdlPads`], over SDL's device indices and names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadPoll {
    /// The open pad still answers: a pad attached beside it changes nothing,
    /// and it is read again as it is.
    Keep,
    /// The open pad is gone, or none was open: drop it and open the first
    /// listed device index, falling back to the ones after it.
    Replace {
        /// The detached pad's fully released reading, sent before its failed
        /// reads and before any replacement.
        released: Option<PadSample>,
        /// The first device index to try, when a pad is listed.
        index: Option<u32>,
        /// This is the startup poll: the pad it opens is the startup pad,
        /// not a hot-plug, so its connect is not reported.
        startup: bool,
    },
}

/// The pad bookkeeping behind [`SdlPads`]: whether a connect is a hot-plug,
/// and the changes the notice reads. [`SdlPads`] only opens, reads and
/// pushes, so the decisions stay testable without SDL devices.
#[derive(Debug, Default)]
struct PadBook {
    /// A poll has run: a pad a later poll opens is a hot-plug.
    polled: bool,
    changes: Vec<PadChange>,
}

impl PadBook {
    /// What one poll does, from SDL's list of attached device indices and the
    /// open pad's own attachment answer.
    fn poll(
        &mut self,
        attached: bool,
        open_name: Option<&str>,
        native_type: u8,
        axes: u8,
        pads: &[u32],
    ) -> PadPoll {
        let startup = !self.polled;
        self.polled = true;
        if open_name.is_some() && attached {
            return PadPoll::Keep;
        }
        let released = open_name.map(|name| {
            self.changes.push(PadChange::Removed(name.to_owned()));
            released_sample(native_type, axes)
        });
        PadPoll::Replace {
            released,
            index: pads.first().copied(),
            startup,
        }
    }

    /// The shell opened the pad named `name`: a hot-plug unless this is the
    /// startup poll.
    fn opened(&mut self, name: String, startup: bool) {
        if !startup {
            self.changes.push(PadChange::Connected(name));
        }
    }

    fn take_changes(&mut self) -> Vec<PadChange> {
        std::mem::take(&mut self.changes)
    }
}

/// The open pad, if any, the native type and axis count its first reading
/// reported, and the pad changes the notice reads.
pub struct SdlPads {
    controllers: GameControllerSubsystem,
    joysticks: JoystickSubsystem,
    layout: ControllerLayout,
    open: Option<OpenPad>,
    device: Option<(u8, u8)>,
    book: PadBook,
}

impl SdlPads {
    pub fn new(sdl: &sdl2::Sdl, layout: ControllerLayout) -> Result<Self, String> {
        Ok(Self {
            controllers: sdl.game_controller()?,
            joysticks: sdl.joystick()?,
            layout,
            open: None,
            device: None,
            book: PadBook::default(),
        })
    }

    /// SDL's device indices this poll, in the order a fresh open takes them.
    /// SDL renumbers them as pads come and go, so the first listed one is the
    /// next available pad. The app pumps SDL's events before every poll
    /// (`events.poll_iter`), which is what updates this list; the pad's own
    /// `attached()` answer is current for the same reason, so no device-event
    /// handling is needed.
    fn enumerate(&self) -> Vec<u32> {
        self.joysticks
            .num_joysticks()
            .map(|count| (0..count).collect())
            .unwrap_or_default()
    }

    /// Opens the pad at a device index, as the game controller when SDL maps
    /// it and otherwise as a raw joystick.
    fn open_at(&self, index: u32) -> Option<OpenPad> {
        if self.controllers.is_game_controller(index) {
            self.controllers.open(index).ok().map(|controller| OpenPad {
                name: controller.name(),
                handle: Open::Controller(controller),
            })
        } else {
            self.joysticks.open(index).ok().map(|joystick| OpenPad {
                name: joystick.name(),
                handle: Open::Joystick(joystick),
            })
        }
    }

    /// The window lost focus: the pad lets go, one reading with nothing
    /// held. It is not read again until focus returns.
    pub fn release(&mut self, out: &mut Vec<InputNotification>) {
        let (Some(open), Some((native_type, axes))) = (self.open.as_ref(), self.device) else {
            return;
        };
        out.push(InputNotification::Pad {
            native_type,
            axes,
            sample: open.handle.released(self.layout),
        });
    }

    /// The pad changes since the last call, for the port's notice: a pad
    /// plugged in after startup, and one that went away.
    pub fn take_changes(&mut self) -> Vec<PadChange> {
        self.book.take_changes()
    }

    /// The frame's reading, once a pad has been open: a failed read while
    /// it is unplugged, as `joyGetPosEx` fails. An unplug first sends one
    /// fully released reading, so no button or axis it held survives. A pad
    /// plugged in later is read as the same native joystick.
    pub fn poll(&mut self, out: &mut Vec<InputNotification>) {
        let attached = self.open.as_ref().is_some_and(|pad| pad.handle.attached());
        let pads = self.enumerate();
        // Before the first open there is no device to release.
        let (native_type, axes) = self.device.unwrap_or((0, 0));
        let name = self.open.as_ref().map(|pad| pad.name.as_str());
        match self.book.poll(attached, name, native_type, axes, &pads) {
            PadPoll::Keep => {}
            PadPoll::Replace {
                released,
                index,
                startup,
            } => {
                if let (Some(sample), Some((native_type, axes))) = (released, self.device) {
                    out.push(InputNotification::Pad {
                        native_type,
                        axes,
                        sample,
                    });
                }
                self.open = None;
                if let Some(index) = index {
                    let first = pads
                        .iter()
                        .position(|candidate| *candidate == index)
                        .unwrap_or(0);
                    let opened = pads[first..].iter().find_map(|&index| self.open_at(index));
                    if let Some(pad) = opened {
                        self.book.opened(pad.name.clone(), startup);
                        self.open = Some(pad);
                    }
                }
            }
        }
        if self.device.is_none() {
            self.device = self.open.as_ref().map(|pad| {
                (
                    pad.handle.native_type(self.layout),
                    pad.handle.axes(self.layout),
                )
            });
        }
        let Some((native_type, axes)) = self.device else {
            return;
        };
        let sample = self
            .open
            .as_ref()
            .map_or(PadSample::Failed, |pad| pad.handle.sample(self.layout));
        out.push(InputNotification::Pad {
            native_type,
            axes,
            sample,
        });
    }
}

#[cfg(test)]
#[path = "platform_pad_tests.rs"]
mod tests;

#[cfg(test)]
mod control_hints_tests {
    use super::*;

    /// Controller wording needs all four conditions; any one missing keeps
    /// the PC text.
    #[test]
    fn pad_wording_is_the_follow_setting_on_a_console_pad_offline() {
        for setting in [ControllerHints::Retail, ControllerHints::FollowLastInput] {
            for layout in [ControllerLayout::Pc, ControllerLayout::Console] {
                for last_input_is_pad in [false, true] {
                    for network_game in [false, true] {
                        let expected = setting == ControllerHints::FollowLastInput
                            && layout == ControllerLayout::Console
                            && last_input_is_pad
                            && !network_game;
                        assert_eq!(
                            setting.uses_pad_wording(layout, last_input_is_pad, network_game),
                            expected,
                            "{setting:?} {layout:?} pad={last_input_is_pad} network={network_game}"
                        );
                    }
                }
            }
        }
        assert_eq!(ControllerHints::default(), ControllerHints::Retail);
    }
}
