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
//! the first, and a second joystick never reaches a binding. The port reads
//! its pad once a frame. Retail binds no mouse table on any screen, so its
//! mouse device (type 1) never gets an adapter and has no effect; the port
//! creates none.
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
    DeviceButtons, DeviceId, DeviceSnapshot, InputError, InputHost, InputRuntime, InputStateError,
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
    /// device: the joystick (type 2) or the PlayStation pad (type 9), with
    /// `axes` axes, which later ones keep.
    Pad {
        native_type: u8,
        axes: u8,
        sample: PadSample,
    },
}

/// One `joyGetPosEx` read: its `dwButtons` (button 1 is bit 0) and its X, Y,
/// Z, R, U and V positions, or a failed read. The PlayStation pad's buttons
/// are its bits (code 1 is bit 0) and its four axes 0 to 255.
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
    /// A screen's sets have made its type-2 adapter (`005474B8`).
    adapter: bool,
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
}

impl SdlInput {
    /// The native constructor clears its buttons at 004F5128. This explicitly
    /// attaches a platform keyboard; table presence alone never creates one.
    /// Native axis count is zero, so 00439AD0 performs no calibration steps.
    pub fn attach(input: &mut InputRuntime) -> Result<Self, InputStateError> {
        Ok(Self {
            source: input.create_device(DeviceButtons::default())?,
            pad: None,
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
        if let Some(pad) = &mut self.pad {
            for axis in 0..u32::from(pad.axes) {
                input.set_axis_transform(
                    pad.source,
                    axis,
                    pad_axis_transform(pad.native_type, sensitivity_54),
                )?;
            }
            pad.adapter = true;
            devices.push(FrontendDevice {
                native_type: u32::from(pad.native_type),
                source: pad.source,
            });
        }
        Ok(devices)
    }

    /// `00439A20`'s pad: its adapter exists and, for the PlayStation pad,
    /// its status input (`005222B0` binds code 0x1F, the failed read, for
    /// types 1 and 6 to 9) is not held. Type 2 binds none, so an unplugged
    /// joystick still counts.
    pub fn joystick_live(&self) -> bool {
        self.pad
            .as_ref()
            .is_some_and(|pad| pad.adapter && !(pad.native_type == 9 && pad.absent))
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
                    self.pad = Some(NativePad {
                        source: input.create_device(DeviceButtons::default())?,
                        native_type,
                        absent: false,
                        axes: axes.min(6),
                        stored: [0; 8],
                        adapter: false,
                    });
                }
                let pad = self.pad.as_mut().expect("created above");
                pad.absent = sample == PadSample::Failed;
                let mut buttons = DeviceButtons::default();
                match sample {
                    PadSample::Read {
                        buttons: read,
                        axes,
                    } => {
                        buttons.0[0] = read.wrapping_mul(2);
                        for (stored, axis) in pad.stored.iter_mut().zip(axes) {
                            *stored = axis as i32;
                        }
                    }
                    // 004F548C: the axes keep their last read.
                    PadSample::Failed => buttons.0[0] = 0x8000_0000,
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
        match notification {
            InputNotification::Key { scancode, pressed } => {
                let Some(code) = native_keyboard_code(scancode) else {
                    return Ok(KeyboardUpdate::UnsupportedScancode);
                };
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
        Ok(if buttons == previous {
            KeyboardUpdate::Unchanged
        } else {
            KeyboardUpdate::Changed
        })
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
