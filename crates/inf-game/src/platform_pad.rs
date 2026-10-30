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
        let centre = if self.native_type(layout) == 9 {
            0x80
        } else {
            0x8000
        };
        let mut axes = [0; 6];
        axes[..usize::from(self.axes(layout))].fill(centre);
        PadSample::Read { buttons: 0, axes }
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

/// The open pad, if any, and the native type and axis count its first
/// reading reported.
pub struct SdlPads {
    controllers: GameControllerSubsystem,
    joysticks: JoystickSubsystem,
    layout: ControllerLayout,
    open: Option<Open>,
    device: Option<(u8, u8)>,
}

impl SdlPads {
    pub fn new(sdl: &sdl2::Sdl, layout: ControllerLayout) -> Result<Self, String> {
        Ok(Self {
            controllers: sdl.game_controller()?,
            joysticks: sdl.joystick()?,
            layout,
            open: None,
            device: None,
        })
    }

    fn first(&self) -> Option<Open> {
        let count = self.joysticks.num_joysticks().ok()?;
        (0..count).find_map(|index| {
            if self.controllers.is_game_controller(index) {
                self.controllers.open(index).ok().map(Open::Controller)
            } else {
                self.joysticks.open(index).ok().map(Open::Joystick)
            }
        })
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
            sample: open.released(self.layout),
        });
    }

    /// The frame's reading, once a pad has been open: a failed read while
    /// it is unplugged, as `joyGetPosEx` fails. A pad plugged in later is
    /// read as the same native joystick.
    pub fn poll(&mut self, out: &mut Vec<InputNotification>) {
        if self.open.as_ref().is_some_and(|open| !open.attached()) {
            self.open = None;
        }
        if self.open.is_none() {
            self.open = self.first();
        }
        if self.device.is_none() {
            self.device = self
                .open
                .as_ref()
                .map(|open| (open.native_type(self.layout), open.axes(self.layout)));
        }
        let Some((native_type, axes)) = self.device else {
            return;
        };
        let sample = self
            .open
            .as_ref()
            .map_or(PadSample::Failed, |open| open.sample(self.layout));
        out.push(InputNotification::Pad {
            native_type,
            axes,
            sample,
        });
    }
}
