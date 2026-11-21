//! SDL physical keyboard and pad notifications in the native source-device
//! domain.
//!
//! 004F1080 constructs the keyboard (004F50F0) with native type 0, 256 buttons, no axes,
//! and both eligibility fields (+10/+14) zero. Its 004F1100 callback packs
//! nonzero DirectInput state bytes 1..255 into the corresponding button bits;
//! there is no key-to-menu translation. 00545488 supplies the 256-byte keyboard
//! format to the DirectInput worker at 0050FA40. The explicit mapping below
//! uses the DIK constants from the DirectInput SDK, not SDL's numeric values.
//!
//! The pad is retail's joystick: `004F55A0` creates one native type-2
//! device per WinMM joystick present at startup, and every 20 ms `004F54A0`
//! stores `joyGetPosEx`'s X, Y, Z, R, U and V (0 to 65535) and its buttons
//! doubled (button 1 is bit 1), or bit 31 alone when the read fails. Only
//! the first joystick drives input: `00439B30` makes one type-2 adapter, from
//! the first, and a second joystick never reaches a binding. The port gives
//! that one adapter the pad in use among the pads SDL lists
//! ([`crate::platform_pad`]), read once a frame. Retail binds no mouse table
//! on any screen, so its mouse device (type 1) never gets an adapter and has
//! no effect; the port creates none.
//!
//! The application owns event-window filtering, polling, callback dispatch,
//! screen repeat timing, and binding lifetimes. Text input and keyboard
//! scancodes without an established DIK counterpart are outside this
//! adapter. Losing focus explicitly releases the portable device state;
//! it does not invoke a menu action or reset the screen's repeat timer.

use sdl2::keyboard::Scancode;

use crate::frontend_bindings::FrontendDevice;
use crate::input::AxisTransform;
use crate::input::{
    AxisRest, DeviceButtons, DeviceId, DeviceSnapshot, InputError, InputHost, InputRuntime,
    InputStateError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputNotification {
    /// Use the event's physical scancode. Repeated key-down notifications are
    /// harmless: native binding evaluation emits only changed held states.
    Key { scancode: Scancode, pressed: bool },
    /// Release all source bits in one physical notification, preserving the
    /// authored binding order and ordinary release callbacks.
    FocusLost,
    /// One poll of the pad (`004F54A0`, or the PlayStation's `800F4310`),
    /// sent every frame while one is open; the first creates the native
    /// device: the joystick (type 2) or a PlayStation pad (types 8 and 9),
    /// with `axes` axes, which later ones keep. The pad in use's unplug sends
    /// one fully released reading first, then the failed ones; an idle pad's
    /// unplug sends nothing.
    Pad {
        native_type: u8,
        axes: u8,
        sample: PadSample,
    },
}

/// One `joyGetPosEx` read: its `dwButtons` (button 1 is bit 0) and its X, Y,
/// Z, R, U and V positions, or a failed read. The PlayStation pads' buttons
/// are their bits (code 1 is bit 0) and their axes 0 to 255: four for the
/// Console layout, six for Remastered (the two sticks, its thrust axis and
/// an idle axis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadSample {
    Read { buttons: u32, axes: [u32; 6] },
    Failed,
}

/// `00439AD0`'s calibration of a joystick axis: the bias of type 2 in
/// `00533764` (`-0x8000`), the multiplier `00438850` derives from the
/// Sensitivity setting (`+54 << 27` plus `0x7FFFFFF`), scale 2.
pub fn joystick_axis_transform(sensitivity_54: u32) -> AxisTransform {
    pad_axis_transform(2, sensitivity_54)
}

/// [`joystick_axis_transform`] for a native type: `00533764` biases type 2
/// by `-0x8000` and types 8 and 9 by `-0x80`.
pub fn pad_axis_transform(native_type: u8, sensitivity_54: u32) -> AxisTransform {
    AxisTransform {
        offset: if native_type == 2 { -0x8000 } else { -0x80 },
        q31_factor: (sensitivity_54 << 27).wrapping_add(0x07ff_ffff) as i32,
        integer_factor: 2,
    }
}

/// One successful `joyGetPosEx` read, for the player's input activity: a
/// button that newly goes down or an axis that newly leaves the dead zone.
#[derive(Clone, Copy, Debug)]
struct PadRead {
    buttons: u32,
    axes: [u32; 6],
}

/// Which device the player used last. It comes from the notifications alone,
/// so a recorded session and `--input-replay` replay it exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LastInput {
    #[default]
    Keyboard,
    Pad,
}

/// The native pad: its device, type, axis count and last stored axes.
#[derive(Debug)]
struct NativePad {
    source: DeviceId,
    native_type: u8,
    axes: u8,
    /// Its last read failed: the PlayStation pad's status input (code 0x1F)
    /// is held.
    absent: bool,
    stored: [i32; 8],
    /// The last successful read's raw state. None until the first read and
    /// after a failed one, so neither establishes a new press or deflection.
    last_read: Option<PadRead>,
    /// A screen's sets have made its type-2 adapter (`005474B8`).
    adapter: bool,
    /// A Pad notification created it after the live screen was built: its
    /// bindings are not in that screen's context yet.
    arrived: bool,
}

/// A quarter of the axis range from the layout's centre: the joystick (type
/// 2) spans 0..0xFFFF around 0x8000, the PlayStation pads (types 8 and 9)
/// 0..255 around 0x80, as [`pad_axis_transform`] biases them. Inside the
/// dead zone a resting stick is neutral. The pad choice
/// ([`crate::platform_pad`]) uses the same test to see which pad the player
/// picked up.
pub(crate) fn axis_off_centre(value: u32, native_type: u8) -> bool {
    pad_axis_rest(native_type).off_centre(value as i32)
}

/// [`axis_off_centre`]'s raw envelope, for the input owner's capture release.
fn pad_axis_rest(native_type: u8) -> AxisRest {
    if native_type == 2 {
        AxisRest {
            centre: 0x8000,
            dead_zone: 0x4000,
        }
    } else {
        AxisRest {
            centre: 0x80,
            dead_zone: 0x40,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardUpdate {
    Changed,
    Unchanged,
    UnsupportedScancode,
}

/// One attached keyboard and the pad, if any, retained for the lifetime of
/// its InputRuntime. These sources must not be independently mutated or
/// reused in another arena.
#[derive(Debug)]
pub struct SdlInput {
    source: DeviceId,
    pad: Option<NativePad>,
    /// A pad that was read and then failed: it was just removed. Taken once
    /// by the gameplay input pass, which pauses as on the PlayStation release.
    pad_removed: bool,
    last_input: LastInput,
    /// Whether the first player's message was shown at the last gameplay poll
    /// (the factory's shop, the computer, a hint). Its first frame releases
    /// the pad's held controls
    /// ([`PortFixes::factory_dock_releases_pad`](crate::frontend_settings::PortFixes::factory_dock_releases_pad)).
    message_shown: bool,
}

impl SdlInput {
    /// The native constructor clears its buttons at 004F5128. This explicitly
    /// attaches a platform keyboard; table presence alone never creates one.
    /// Native axis count is zero, so 00439AD0 performs no calibration steps.
    pub fn attach(input: &mut InputRuntime) -> Result<Self, InputStateError> {
        Ok(Self {
            source: input.create_device(DeviceButtons::default())?,
            pad: None,
            pad_removed: false,
            last_input: LastInput::Keyboard,
            message_shown: false,
        })
    }

    /// Every device a screen's sets bind, in native enumeration order: the
    /// keyboard, then the joystick, its axes calibrated for `sensitivity_54`
    /// (`00439AA0` runs `00439AD0` on every adapter; axes past its count keep
    /// `004F0EE0`'s defaults). A
    /// screen with a type-2 table then holds the joystick's adapter; the
    /// port treats every enumeration as one, as only the title has none and
    /// play is never reached from it directly.
    pub fn frontend_devices(
        &mut self,
        input: &mut InputRuntime,
        sensitivity_54: u32,
    ) -> Result<Vec<FrontendDevice>, InputStateError> {
        let mut devices = vec![self.frontend_device()];
        if self.pad.is_some() {
            devices.push(self.calibrate_pad(input, sensitivity_54)?);
        }
        Ok(devices)
    }

    /// The pad a notification first created after the live screen was built:
    /// its axes calibrated for `sensitivity_54` and its adapter made, as a
    /// screen's own enumeration does, so the screen's live bindings can take
    /// it. Reported once; the next screen's enumeration binds it as usual.
    pub fn take_arrived_pad(
        &mut self,
        input: &mut InputRuntime,
        sensitivity_54: u32,
    ) -> Result<Option<FrontendDevice>, InputStateError> {
        match self.pad.as_ref() {
            Some(pad) if pad.arrived => self.calibrate_pad(input, sensitivity_54).map(Some),
            _ => Ok(None),
        }
    }

    /// `00439AD0`'s calibration of the pad's axes, with the adapter a
    /// screen's sets give it (`005474B8`).
    fn calibrate_pad(
        &mut self,
        input: &mut InputRuntime,
        sensitivity_54: u32,
    ) -> Result<FrontendDevice, InputStateError> {
        let pad = self.pad.as_mut().expect("an attached pad");
        for axis in 0..u32::from(pad.axes) {
            input.set_axis_transform(
                pad.source,
                axis,
                pad_axis_transform(pad.native_type, sensitivity_54),
            )?;
        }
        pad.adapter = true;
        pad.arrived = false;
        Ok(FrontendDevice {
            native_type: u32::from(pad.native_type),
            source: pad.source,
        })
    }

    /// `00439A20`'s pad: its adapter exists and, for a PlayStation pad, its
    /// status input (`005222B0` binds code 0x1F, the failed read, for types 1
    /// and 6 to 9) is not held. Type 2 binds none, so an unplugged joystick
    /// still counts.
    pub fn joystick_live(&self) -> bool {
        self.pad
            .as_ref()
            .is_some_and(|pad| pad.adapter && !(matches!(pad.native_type, 8 | 9) && pad.absent))
    }

    /// Port fix [`PortFixes::factory_dock_releases_pad`](crate::frontend_settings::PortFixes):
    /// the first frame a player message is shown (a factory's shop after it
    /// docks the vehicle) releases the pad's held buttons and deflected axes,
    /// as the message brake releases the keyboard's steering (`0046A719`).
    /// Each stays released until the pad reports it up (or the stick back in
    /// its dead zone). Every later frame while the message shows does
    /// nothing, so a control pressed after the release counts normally.
    pub fn note_message_shown(
        &mut self,
        shown: bool,
        input: &mut InputRuntime,
    ) -> Result<(), InputStateError> {
        if shown && !self.message_shown {
            if let Some(pad) = self.pad.as_ref() {
                input.release_held_controls(pad.source)?;
            }
        }
        self.message_shown = shown;
        Ok(())
    }

    /// Whether the player's last input was the pad: a button newly went down
    /// or an axis newly left its dead zone. A keyboard press takes it back.
    /// Every-frame repeated pad readings, releases, failed reads and key
    /// repeats do not count, so the last device stays as the player left it.
    pub fn last_input_is_pad(&self) -> bool {
        self.last_input == LastInput::Pad
    }

    /// Eligible native type 0, proven by the physical keyboard constructor.
    /// Binding installation and input-context selection remain caller-owned.
    pub fn frontend_device(&self) -> FrontendDevice {
        FrontendDevice {
            native_type: 0,
            source: self.source,
        }
    }

    /// Preserve each event in order; do not collapse press/release pairs into
    /// an end-of-frame snapshot. Native InputRuntime owns callback queueing,
    /// timestamps, shared-binding held counts, and failure propagation.
    pub fn update<H: InputHost>(
        &mut self,
        notification: InputNotification,
        input: &mut InputRuntime,
        host: &mut H,
    ) -> Result<KeyboardUpdate, InputError<H::Error>> {
        match notification {
            InputNotification::Pad {
                native_type,
                axes,
                sample,
            } => {
                if self.pad.is_none() {
                    let source = input.create_device(DeviceButtons::default())?;
                    input.set_axis_rest(source, pad_axis_rest(native_type))?;
                    self.pad = Some(NativePad {
                        source,
                        native_type,
                        absent: sample == PadSample::Failed,
                        axes: axes.min(6),
                        stored: [0; 8],
                        last_read: None,
                        adapter: false,
                        arrived: true,
                    });
                }
                let pad = self.pad.as_mut().expect("created above");
                // A read that now fails is the pad's removal. A pad that was
                // already failing is not removed again, and a focus-loss
                // release is another reading, not a removal.
                if !pad.absent && sample == PadSample::Failed {
                    self.pad_removed = true;
                }
                pad.absent = sample == PadSample::Failed;
                let mut buttons = DeviceButtons::default();
                let mut used_pad = false;
                match sample {
                    PadSample::Read {
                        buttons: read,
                        axes,
                    } => {
                        if let Some(previous) = pad.last_read {
                            let newly_down = read & !previous.buttons != 0;
                            let newly_off_centre = (0..usize::from(pad.axes)).any(|index| {
                                axis_off_centre(axes[index], pad.native_type)
                                    && !axis_off_centre(previous.axes[index], pad.native_type)
                            });
                            used_pad = newly_down || newly_off_centre;
                        }
                        pad.last_read = Some(PadRead {
                            buttons: read,
                            axes,
                        });
                        buttons.0[0] = read.wrapping_mul(2);
                        for (stored, axis) in pad.stored.iter_mut().zip(axes) {
                            *stored = axis as i32;
                        }
                    }
                    // 004F548C: the axes keep their last read.
                    PadSample::Failed => {
                        buttons.0[0] = 0x8000_0000;
                        pad.last_read = None;
                    }
                }
                if used_pad {
                    self.last_input = LastInput::Pad;
                }
                let previous = input.device_buttons(pad.source)?;
                input.update_device(
                    pad.source,
                    DeviceSnapshot {
                        buttons,
                        axes: pad.stored,
                    },
                    host,
                )?;
                return Ok(if buttons == previous {
                    KeyboardUpdate::Unchanged
                } else {
                    KeyboardUpdate::Changed
                });
            }
            _ => {}
        }
        let previous = input.device_buttons(self.source)?;
        let mut buttons = previous;
        let mut key_pressed = false;
        match notification {
            InputNotification::Key { scancode, pressed } => {
                let Some(code) = native_keyboard_code(scancode) else {
                    return Ok(KeyboardUpdate::UnsupportedScancode);
                };
                // A repeat is not a new press: the key was already held.
                key_pressed = pressed && !previous.pressed(code);
                buttons.set(code, pressed);
            }
            InputNotification::FocusLost => buttons = DeviceButtons::default(),
            InputNotification::Pad { .. } => unreachable!("handled above"),
        }
        input.update_device(
            self.source,
            DeviceSnapshot {
                buttons,
                axes: [0; 8],
            },
            host,
        )?;
        if key_pressed {
            self.last_input = LastInput::Keyboard;
        }
        Ok(if buttons == previous {
            KeyboardUpdate::Unchanged
        } else {
            KeyboardUpdate::Changed
        })
    }

    /// The live pad's removal since the last call: a reading followed by a
    /// failed one. A pad that was already failing does not report again, and
    /// the focus-loss release (a reading) is not a removal. Taking it clears
    /// the one-shot flag; a pad re-added and removed again reports again.
    pub fn take_pad_removed(&mut self) -> bool {
        std::mem::take(&mut self.pad_removed)
    }
}

/// Standard PC keyboard physical keys to DirectInput DIK byte offsets.
/// Return/keypad Enter, cursor/keypad keys, and left/right modifiers stay
/// distinct. Locale-specific, media, and extended keys are not guessed.
pub fn native_keyboard_code(scancode: Scancode) -> Option<u8> {
    use Scancode::*;
    Some(match scancode {
        Escape => 0x01,
        Num1 => 0x02,
        Num2 => 0x03,
        Num3 => 0x04,
        Num4 => 0x05,
        Num5 => 0x06,
        Num6 => 0x07,
        Num7 => 0x08,
        Num8 => 0x09,
        Num9 => 0x0a,
        Num0 => 0x0b,
        Minus => 0x0c,
        Equals => 0x0d,
        Backspace => 0x0e,
        Tab => 0x0f,
        Q => 0x10,
        W => 0x11,
        E => 0x12,
        R => 0x13,
        T => 0x14,
        Y => 0x15,
        U => 0x16,
        I => 0x17,
        O => 0x18,
        P => 0x19,
        LeftBracket => 0x1a,
        RightBracket => 0x1b,
        Return => 0x1c,
        LCtrl => 0x1d,
        A => 0x1e,
        S => 0x1f,
        D => 0x20,
        F => 0x21,
        G => 0x22,
        H => 0x23,
        J => 0x24,
        K => 0x25,
        L => 0x26,
        Semicolon => 0x27,
        Apostrophe => 0x28,
        Grave => 0x29,
        LShift => 0x2a,
        Backslash => 0x2b,
        Z => 0x2c,
        X => 0x2d,
        C => 0x2e,
        V => 0x2f,
        B => 0x30,
        N => 0x31,
        M => 0x32,
        Comma => 0x33,
        Period => 0x34,
        Slash => 0x35,
        RShift => 0x36,
        KpMultiply => 0x37,
        LAlt => 0x38,
        Space => 0x39,
        CapsLock => 0x3a,
        F1 => 0x3b,
        F2 => 0x3c,
        F3 => 0x3d,
        F4 => 0x3e,
        F5 => 0x3f,
        F6 => 0x40,
        F7 => 0x41,
        F8 => 0x42,
        F9 => 0x43,
        F10 => 0x44,
        NumLockClear => 0x45,
        ScrollLock => 0x46,
        Kp7 => 0x47,
        Kp8 => 0x48,
        Kp9 => 0x49,
        KpMinus => 0x4a,
        Kp4 => 0x4b,
        Kp5 => 0x4c,
        Kp6 => 0x4d,
        KpPlus => 0x4e,
        Kp1 => 0x4f,
        Kp2 => 0x50,
        Kp3 => 0x51,
        Kp0 => 0x52,
        KpPeriod => 0x53,
        NonUsBackslash => 0x56,
        F11 => 0x57,
        F12 => 0x58,
        KpEnter => 0x9c,
        RCtrl => 0x9d,
        KpDivide => 0xb5,
        PrintScreen => 0xb7,
        RAlt => 0xb8,
        Pause => 0xc5,
        Home => 0xc7,
        Up => 0xc8,
        PageUp => 0xc9,
        Left => 0xcb,
        Right => 0xcd,
        End => 0xcf,
        Down => 0xd0,
        PageDown => 0xd1,
        Insert => 0xd2,
        Delete => 0xd3,
        LGui => 0xdb,
        RGui => 0xdc,
        Application => 0xdd,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "platform_input_tests.rs"]
mod tests;
