//! The pad behind the native joystick ([`crate::platform_input`]): every
//! attached SDL pad, opened as a game controller when SDL maps it and
//! otherwise as a joystick, read once a frame, with one of them — the pad in
//! use — giving the single reading the native stream sees, as WinMM's
//! `joyGetPosEx` reports it to retail (`004F54A0`).
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
//! Port convenience, not retail: every attached pad is read, but the native
//! stream follows the pad in use. At the first poll that is the first listed
//! pad; any other pad takes the stream the moment it newly presses a button
//! or newly takes an axis out of its dead zone, or when it is plugged in
//! while no pad is in use. The pad in use that goes away first sends one
//! fully released reading, so nothing it held stays held, and then failed
//! reads until another pad takes the stream; an idle pad's departure is only
//! a notice. All of it follows the notification stream
//! ([`crate::platform_input`]), never SDL's hot-plug events, so a recorded
//! session replays identically. [`SdlPads::take_changes`] reports the connects
//! and removals the port's notice shows.

use sdl2::controller::{Axis, Button, GameController};
use sdl2::joystick::Joystick;
use sdl2::{GameControllerSubsystem, JoystickSubsystem};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::control_hints::{PadFamily, PadWording};
use crate::platform_input::{axis_off_centre, InputNotification, PadSample};

/// How the pad plays: as PC retail's joystick, as the PlayStation release's
/// pad, or as the Remastered layout's modern pad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerLayout {
    /// Retail's WinMM joystick, native type 2.
    #[default]
    Pc,
    /// The PlayStation pad, native type 9.
    Console,
    /// The Remastered layout, native type 8. Its device sample, axes and
    /// per-screen tables are the layout change's; this variant lets the hint
    /// wording name the layout meanwhile.
    Remastered,
}

impl ControllerLayout {
    pub fn native_type(self) -> u8 {
        match self {
            ControllerLayout::Pc => 2,
            ControllerLayout::Console => 9,
            ControllerLayout::Remastered => 8,
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
    /// The wording hints and shell prompts use. Retail's PC text stands
    /// unless the player chose to follow the last input, the player last used
    /// the pad, the game is offline (network games play retail's game on
    /// every renderer), and the layout is a pad layout; then the layout picks
    /// the Console or Remastered sentences and the family names the buttons.
    pub fn wording(
        self,
        layout: ControllerLayout,
        last_input_is_pad: bool,
        network_game: bool,
        family: PadFamily,
    ) -> PadWording {
        if self != Self::FollowLastInput || !last_input_is_pad || network_game {
            return PadWording::Retail;
        }
        match layout {
            ControllerLayout::Pc => PadWording::Retail,
            ControllerLayout::Console => PadWording::Console(family),
            ControllerLayout::Remastered => PadWording::Remastered(family),
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

    /// SDL's id for this connection: one per physical device, shared by the
    /// controller and joystick views of it, and new after a re-plug.
    fn instance_id(&self) -> u32 {
        match self {
            Open::Controller(controller) => controller.instance_id(),
            Open::Joystick(joystick) => joystick.instance_id(),
        }
    }

    fn name(&self) -> String {
        match self {
            Open::Controller(controller) => controller.name(),
            Open::Joystick(joystick) => joystick.name(),
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

/// An open pad: SDL's handle, the name it gave the device at open (kept for
/// the removal report after the device is gone), its device instance id, its
/// button family for hint wording, and the last reading for the new-activity
/// test.
struct OpenPad {
    handle: Open,
    id: u32,
    name: String,
    family: PadFamily,
    last: Option<PadSample>,
}

impl OpenPad {
    fn new(handle: Open, family: PadFamily) -> Self {
        Self {
            id: handle.instance_id(),
            name: handle.name(),
            handle,
            family,
            last: None,
        }
    }
}

/// SDL's game-controller type for a device, as the hint wording's family:
/// its PS3, PS4 and PS5 types are Sony pads, whose buttons are named Cross,
/// Circle, Square and Triangle; every other type (Xbox, Switch, generic,
/// unknown) names Microsoft's positional A, B, X and Y.
fn pad_family(kind: sdl2::sys::SDL_GameControllerType) -> PadFamily {
    use sdl2::sys::SDL_GameControllerType;
    match kind {
        SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS3
        | SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS4
        | SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS5 => PadFamily::PlayStation,
        _ => PadFamily::Xbox,
    }
}

/// [`pad_family`] for a device index SDL lists. The call needs SDL's joystick
/// subsystem initialized, which [`SdlPads::new`] receives.
fn controller_family(index: u32) -> PadFamily {
    // SAFETY: `index` is a device SDL just listed, and SDL is initialized by
    // the `Sdl` the caller holds for the whole lifetime of `SdlPads`.
    pad_family(unsafe { sdl2::sys::SDL_GameControllerTypeForIndex(index as i32) })
}

/// One listed pad as a poll sees it: SDL's device instance id, the name to
/// report, and whether this reading newly pressed a button or newly left an
/// axis's dead zone.
#[derive(Clone, Copy, Debug)]
struct PadSeen<'a> {
    id: u32,
    name: &'a str,
    activity: bool,
}

/// One poll's decision for [`SdlPads`], over the pads SDL lists and each
/// pad's activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PadPoll {
    /// The pad in use: its device instance id, whose reading this poll sends.
    active: Option<u32>,
    /// The pad in use just left: its fully released reading goes out before
    /// this poll's failed one.
    released: Option<PadSample>,
    /// A pad was in use and none is now: every poll reads
    /// [`PadSample::Failed`] until another pad is used or connected.
    failed: bool,
}

/// The pad bookkeeping behind [`SdlPads`]: the pads the last poll saw, the
/// pad in use, whether the stream is failing, and the changes the notice
/// reads. [`SdlPads`] only opens, reads and pushes, so the decisions stay
/// testable without SDL devices.
#[derive(Debug, Default)]
struct PadBook {
    /// A poll has run: a pad a later poll finds is a hot-plug, and the pads
    /// present at the first poll are silent.
    polled: bool,
    /// The pad in use, by device instance id.
    active: Option<u32>,
    /// The pad in use is gone and no other has taken the stream: every poll
    /// reads Failed until one does.
    failed: bool,
    /// The pads the last poll listed, with the names to report when they go.
    known: Vec<(u32, String)>,
    changes: Vec<PadChange>,
}

impl PadBook {
    /// What one poll does, from the pads SDL lists, their activity, and the
    /// native type and axis count the first pad latched.
    fn poll(&mut self, pads: &[PadSeen<'_>], native_type: u8, axes: u8) -> PadPoll {
        let startup = !self.polled;
        self.polled = true;

        // Every pad known last poll and no longer listed has left. Only the
        // pad in use releases and fails; an idle pad is a notice alone.
        let mut released = None;
        for (id, name) in &self.known {
            if pads.iter().any(|pad| pad.id == *id) {
                continue;
            }
            self.changes.push(PadChange::Removed(name.clone()));
            if self.active == Some(*id) {
                released = Some(released_sample(native_type, axes));
                self.active = None;
                self.failed = true;
            }
        }

        // A pad SDL lists for the first time joins. The startup pads are not
        // connects, and a pad found at startup is not a hot-plug.
        let mut connected = Vec::new();
        for pad in pads {
            if self.known.iter().any(|(id, _)| *id == pad.id) {
                continue;
            }
            if !startup {
                self.changes.push(PadChange::Connected(pad.name.to_owned()));
            }
            connected.push(pad.id);
        }
        self.known = pads
            .iter()
            .map(|pad| (pad.id, pad.name.to_owned()))
            .collect();

        if startup {
            // The first listed pad is the startup pad, as a lone one always
            // was; every later pad beside it starts idle.
            self.active = pads.first().map(|pad| pad.id);
            self.failed = false;
        } else if let Some(pad) = pads.iter().find(|pad| pad.activity) {
            // Any pad the player uses takes the stream, even from the one
            // already read, and that same poll reads the new pad.
            self.active = Some(pad.id);
            self.failed = false;
        } else if self.active.is_none() {
            // Nothing is in use: a pad that just connected takes the stream
            // without needing a press, as a lone startup pad does.
            if let Some(id) = connected.first() {
                self.active = Some(*id);
                self.failed = false;
            }
        }

        PadPoll {
            active: self.active,
            released,
            failed: self.failed,
        }
    }

    /// The pad the native stream reads, if any.
    fn active(&self) -> Option<u32> {
        self.active
    }

    fn take_changes(&mut self) -> Vec<PadChange> {
        std::mem::take(&mut self.changes)
    }
}

/// Whether a reading shows what [`SdlInput::last_input_is_pad`] counts as
/// the pad being used: a button newly down, or an axis newly past the dead
/// zone its native type's centre gives ([`axis_off_centre`]). A first reading
/// has nothing to compare with, so it establishes the baseline instead.
///
/// [`SdlInput::last_input_is_pad`]: crate::platform_input::SdlInput::last_input_is_pad
fn newly_used(previous: Option<PadSample>, now: PadSample, native_type: u8, axes: u8) -> bool {
    let (
        PadSample::Read {
            buttons,
            axes: now_axes,
        },
        Some(PadSample::Read {
            buttons: previous_buttons,
            axes: previous_axes,
        }),
    ) = (now, previous)
    else {
        return false;
    };
    let newly_down = buttons & !previous_buttons != 0;
    let newly_off_centre = (0..usize::from(axes)).any(|index| {
        axis_off_centre(now_axes[index], native_type)
            && !axis_off_centre(previous_axes[index], native_type)
    });
    newly_down || newly_off_centre
}

/// Every open pad, the native type and axis count the first pad latched, and
/// the pad changes the notice reads.
pub struct SdlPads {
    controllers: GameControllerSubsystem,
    joysticks: JoystickSubsystem,
    layout: ControllerLayout,
    pads: Vec<OpenPad>,
    device: Option<(u8, u8)>,
    book: PadBook,
}

impl SdlPads {
    pub fn new(sdl: &sdl2::Sdl, layout: ControllerLayout) -> Result<Self, String> {
        Ok(Self {
            controllers: sdl.game_controller()?,
            joysticks: sdl.joystick()?,
            layout,
            pads: Vec::new(),
            device: None,
            book: PadBook::default(),
        })
    }

    /// How many pads SDL lists this poll. SDL renumbers its device indices as
    /// pads come and go, so an index only serves to open a device; open pads
    /// are tracked by the instance id SDL gives each connection. The app
    /// pumps SDL's events before every poll (`events.poll_iter`), which is
    /// what updates this list; the pad's own `attached()` answer is current
    /// for the same reason, so no device-event handling is needed.
    fn listed(&self) -> u32 {
        self.joysticks.num_joysticks().unwrap_or(0)
    }

    /// Opens a device index, as the game controller when SDL maps it and
    /// otherwise as a raw joystick. The family is SDL's game-controller type
    /// for the same index, whether or not SDL maps the device: a generic
    /// joystick still answers (or reports unknown, which is Xbox's names).
    fn open_at(&self, index: u32) -> Option<OpenPad> {
        let family = controller_family(index);
        if self.controllers.is_game_controller(index) {
            self.controllers
                .open(index)
                .ok()
                .map(|controller| OpenPad::new(Open::Controller(controller), family))
        } else {
            self.joysticks
                .open(index)
                .ok()
                .map(|joystick| OpenPad::new(Open::Joystick(joystick), family))
        }
    }

    /// Opens the listed devices not held yet, one per attached pad. A held
    /// pad is never reopened: the count of open pads against SDL's list says
    /// whether anything is missing, and an opened handle's instance id is the
    /// dedupe key, so the second handle SDL can hand out for an already-open
    /// device is dropped at once.
    fn open_new(&mut self) {
        let listed = self.listed();
        let mut missing = listed.saturating_sub(self.pads.len() as u32);
        for index in 0..listed {
            if missing == 0 {
                break;
            }
            let Some(pad) = self.open_at(index) else {
                continue;
            };
            if self.pads.iter().any(|open| open.id == pad.id) {
                continue;
            }
            self.pads.push(pad);
            missing -= 1;
        }
    }

    /// The window lost focus: the pad in use lets go, one reading with
    /// nothing held. It is not read again until focus returns.
    pub fn release(&mut self, out: &mut Vec<InputNotification>) {
        let (Some((native_type, axes)), Some(_)) = (self.device, self.book.active()) else {
            return;
        };
        out.push(InputNotification::Pad {
            native_type,
            axes,
            sample: released_sample(native_type, axes),
        });
    }

    /// The pad changes since the last call, for the port's notice: every pad
    /// that joined after the first poll, and one that went away.
    pub fn take_changes(&mut self) -> Vec<PadChange> {
        self.book.take_changes()
    }

    /// The family of the pad in use, for the hint wording. No pad in use
    /// reads as Xbox; the PC text stands then anyway.
    pub fn family(&self) -> PadFamily {
        self.book
            .active()
            .and_then(|id| self.pads.iter().find(|pad| pad.id == id))
            .map_or(PadFamily::Xbox, |pad| pad.family)
    }

    /// The frame's reading, once a pad has been open: the pad in use's
    /// reading, or a failed read while it is unplugged, as `joyGetPosEx`
    /// fails. The pad in use's unplug first sends one fully released reading,
    /// so no button or axis it held survives; an idle pad's unplug sends
    /// nothing. A pad plugged in later is read as the same native joystick.
    pub fn poll(&mut self, out: &mut Vec<InputNotification>) {
        // A handle answers for itself, so an unplugged pad is dropped before
        // it is read. SDL's list still renumbers around it.
        self.pads.retain(|pad| pad.handle.attached());
        self.open_new();

        // The native stream's shape comes from the first pad ever opened and
        // never changes, so a recorded session replays the same device.
        if self.device.is_none() {
            self.device = self.pads.first().map(|pad| {
                (
                    pad.handle.native_type(self.layout),
                    pad.handle.axes(self.layout),
                )
            });
        }

        let mut seen = Vec::with_capacity(self.pads.len());
        let mut samples = Vec::with_capacity(self.pads.len());
        for pad in &mut self.pads {
            let sample = pad.handle.sample(self.layout);
            let activity = self
                .device
                .is_some_and(|(native_type, axes)| newly_used(pad.last, sample, native_type, axes));
            pad.last = matches!(sample, PadSample::Read { .. }).then_some(sample);
            seen.push(PadSeen {
                id: pad.id,
                name: &pad.name,
                activity,
            });
            samples.push((pad.id, sample));
        }

        let (native_type, axes) = self.device.unwrap_or((0, 0));
        let pad_poll = self.book.poll(&seen, native_type, axes);
        // Before the first open there is no native stream and no device to
        // report; the book opened nothing, so nothing is pushed.
        let Some((native_type, axes)) = self.device else {
            return;
        };
        if let Some(sample) = pad_poll.released {
            out.push(InputNotification::Pad {
                native_type,
                axes,
                sample,
            });
        }
        let sample = match pad_poll.active {
            Some(id) => samples
                .iter()
                .find_map(|&(candidate, sample)| (candidate == id).then_some(sample))
                .unwrap_or(PadSample::Failed),
            None if pad_poll.failed => PadSample::Failed,
            None => return,
        };
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

    /// The wording needs the follow setting, the pad's last use and offline
    /// play; then only a pad layout changes the text, and its family names
    /// the buttons. Any failed condition keeps retail's PC text.
    #[test]
    fn pad_wording_is_the_follow_setting_on_a_pad_layout_offline() {
        use crate::control_hints::{PadFamily, PadWording};
        for setting in [ControllerHints::Retail, ControllerHints::FollowLastInput] {
            for layout in [
                ControllerLayout::Pc,
                ControllerLayout::Console,
                ControllerLayout::Remastered,
            ] {
                for family in [PadFamily::Xbox, PadFamily::PlayStation] {
                    for last_input_is_pad in [false, true] {
                        for network_game in [false, true] {
                            let expected = if setting == ControllerHints::FollowLastInput
                                && last_input_is_pad
                                && !network_game
                            {
                                match layout {
                                    ControllerLayout::Pc => PadWording::Retail,
                                    ControllerLayout::Console => PadWording::Console(family),
                                    ControllerLayout::Remastered => PadWording::Remastered(family),
                                }
                            } else {
                                PadWording::Retail
                            };
                            assert_eq!(
                                setting.wording(layout, last_input_is_pad, network_game, family),
                                expected,
                                "{setting:?} {layout:?} {family:?} pad={last_input_is_pad} \
                                 network={network_game}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(ControllerHints::default(), ControllerHints::Retail);
    }

    /// SDL's three PlayStation controller types are Sony's; everything else,
    /// including the Switch types and an unknown device, names Xbox buttons.
    #[test]
    fn only_sdls_playstation_types_are_the_playstation_family() {
        use sdl2::sys::SDL_GameControllerType;
        for kind in [
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_UNKNOWN,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_XBOX360,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_XBOXONE,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_NINTENDO_SWITCH_PRO,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_VIRTUAL,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_AMAZON_LUNA,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_GOOGLE_STADIA,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_NVIDIA_SHIELD,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_NINTENDO_SWITCH_JOYCON_LEFT,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_NINTENDO_SWITCH_JOYCON_RIGHT,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_NINTENDO_SWITCH_JOYCON_PAIR,
        ] {
            assert_eq!(pad_family(kind), PadFamily::Xbox, "{}", kind as u32);
        }
        for kind in [
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS3,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS4,
            SDL_GameControllerType::SDL_CONTROLLER_TYPE_PS5,
        ] {
            assert_eq!(pad_family(kind), PadFamily::PlayStation, "{}", kind as u32);
        }
    }
}
