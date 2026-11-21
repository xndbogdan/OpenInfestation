//! The Remastered pad's per-screen binding tables for native type 8.
//!
//! This module is port-authored binding data, not original bytes. Every row it
//! installs is a retail row re-coded: the descriptor (callback, flags and
//! original file span) is taken from the screen's own parsed tables, and only
//! the device codes change. Rows retail's pad never had (cargo rotation, the
//! primary steps, the gameplay screen's map/first-person/pause keys) reuse the
//! descriptor their keyboard row already binds. No action is invented.
//!
//! Slot 8 is retail's analog PlayStation pad that does not answer the
//! extended-mode query. The port never creates a type-8 device for the PC or
//! Console layouts, so only a Remastered pad selects these tables; slots 0..7
//! and 9 stay byte-identical to the parser's output.
//!
//! Synthetic row addresses start at [`SYNTHETIC_BASE`], outside the loaded
//! image. A row's virtual address and file span are self-consistent (the
//! provenance shape `FrontendBindings` validates), while its descriptor keeps
//! the real retail identity and span. The descriptors are always looked up in
//! the retail tables passed in; no callback address is baked into a row.
//!
//! The play sub-object intentionally leaves cargo rotation unbound: the
//! agreed Remastered gesture has no button left for it, and the final mapping
//! assigns Y to beam down.
//!
//! See `docs/re/REMASTERED_PAD.md` for the retail evidence behind each action.

use inf_formats::{
    AnalogInputRecord, AnalogInputTable, ByteSpan, DigitalActionDescriptor, DigitalInputRecord,
    DigitalInputTable, ScreenInputTables, ANALOG_INPUT_RECORD_BYTES, DIGITAL_INPUT_RECORD_BYTES,
    INPUT_DEVICE_SLOTS,
};
use thiserror::Error;

/// SDL native type the Remastered layout reports (`docs/re/REMASTERED_PAD.md`).
pub const NATIVE_TYPE: usize = 8;

/// A reserved virtual/file range outside the retail image. Row bytes are
/// port-authored; the descriptors inside them are retail's.
pub const SYNTHETIC_BASE: u32 = 0x7f00_0000;

/// The button codes the Remastered sample produces (1-based, as retail).
pub mod codes {
    pub const VIEW: u8 = 1;
    pub const L3: u8 = 2;
    pub const R3: u8 = 3;
    pub const START: u8 = 4;
    pub const UP: u8 = 5;
    pub const RIGHT: u8 = 6;
    pub const DOWN: u8 = 7;
    pub const LEFT: u8 = 8;
    pub const LT: u8 = 9;
    pub const RT: u8 = 10;
    pub const LB: u8 = 11;
    pub const RB: u8 = 12;
    pub const Y: u8 = 13;
    pub const B: u8 = 14;
    pub const A: u8 = 15;
    pub const X: u8 = 16;
}

/// The axis the Remastered sample reports for the combined thrust trigger.
pub const THRUST_AXIS: u32 = 4;

/// `0054ED08` is the player's `+64` analog throttle row in `00523238`.
const THRUST_ANALOG_DESCRIPTOR: u32 = 0x0054_ed08;
/// `0053A378` is the digital thrust consumer (`+78`); it has no callback.
const DIGITAL_THRUST_DESCRIPTOR: u32 = 0x0053_a378;
/// `0053A388` is the primary-fire consumer (`+80`); it has no callback.
const PRIMARY_FIRE_DESCRIPTOR: u32 = 0x0053_a388;

/// One screen constructor's tables, named by the screen that installs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemasteredScreen {
    /// `004838A0`'s frontend, all its menu families.
    Frontend,
    /// `0040A420`'s phase-3 briefing.
    Briefing,
    /// `0042AB00`'s phase-8 results.
    Results,
    /// `00459060`'s phase 9/10/11 text and name entry.
    TextEntry,
    /// `00443190`'s in-world pause menu.
    InWorldPause,
    /// `00451020`'s phase-7 map.
    Map,
    /// `00432500`'s startup/language carousel.
    Startup,
    /// `00444BD0`'s key configuration.
    KeyConfig,
    /// `00404620`'s title screen, its intro and its title video.
    Title,
    /// `00440990`'s gameplay screen object.
    Gameplay,
    /// `00440990`'s `+0xFC` gameplay sub-object.
    GameplaySub,
}

impl RemasteredScreen {
    fn index(self) -> u32 {
        self as u32
    }

    fn digital_base(self) -> u32 {
        SYNTHETIC_BASE + self.index() * 0x1000
    }

    fn analog_base(self) -> u32 {
        self.digital_base() + 0x800
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RemasteredBindingError {
    #[error("the {screen:?} retail tables have no row for callback {callback:#010x}")]
    MissingCallback {
        screen: RemasteredScreen,
        callback: u32,
    },
    #[error("the {screen:?} retail tables have no row for descriptor {identity:#010x}")]
    MissingDescriptor {
        screen: RemasteredScreen,
        identity: u32,
    },
    #[error("the {screen:?} retail throttle table has no {identity:#010x} row")]
    MissingAnalogDescriptor {
        screen: RemasteredScreen,
        identity: u32,
    },
}

/// Build `screen`'s slot-8 digital and analog tables from its retail tables.
///
/// Every other slot is returned exactly as parsed.
pub fn remastered_tables(
    screen: RemasteredScreen,
    retail: &ScreenInputTables,
) -> Result<ScreenInputTables, RemasteredBindingError> {
    let mut tables = retail.clone();
    tables.digital[NATIVE_TYPE] = Some(screen.digital(retail)?);
    tables.analog[NATIVE_TYPE] = screen.analog(retail)?;
    Ok(tables)
}

impl RemasteredScreen {
    fn digital(
        self,
        retail: &ScreenInputTables,
    ) -> Result<DigitalInputTable, RemasteredBindingError> {
        use codes::*;
        let rows = match self {
            Self::Frontend => {
                let confirm = self.action(retail, 0x0044_5200)?;
                let back = self.action(retail, 0x0044_51e0)?;
                let previous = self.action(retail, 0x0044_5220)?;
                let next = self.action(retail, 0x0044_5240)?;
                let adjust_previous = self.action(retail, 0x0044_5260)?;
                let adjust_next = self.action(retail, 0x0044_5280)?;
                vec![
                    row(confirm, &[A]),
                    row(confirm, &[START]),
                    row(back, &[B]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(adjust_previous, &[LEFT]),
                    row(adjust_next, &[RIGHT]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::Briefing => {
                let confirm = self.action(retail, 0x0040_d670)?;
                let back = self.action(retail, 0x0040_d660)?;
                let previous = self.action(retail, 0x0040_d680)?;
                let next = self.action(retail, 0x0040_d690)?;
                vec![
                    row(confirm, &[A]),
                    row(back, &[B]),
                    // Retail's type-9 Start row is the briefing's back key.
                    row(back, &[START]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::Results => {
                let back = self.action(retail, 0x0042_aa30)?;
                let continue_ = self.action(retail, 0x0042_aa60)?;
                let save = self.action(retail, 0x0042_aaa0)?;
                let confirm = self.action(retail, 0x0044_5200)?;
                let previous = self.action(retail, 0x0044_5220)?;
                let next = self.action(retail, 0x0044_5240)?;
                let adjust_previous = self.action(retail, 0x0044_5260)?;
                let adjust_next = self.action(retail, 0x0044_5280)?;
                vec![
                    // The PS1 binds Cross to both continue and held confirm.
                    row(continue_, &[A]),
                    row(confirm, &[A]),
                    row(continue_, &[START]),
                    row(confirm, &[START]),
                    row(back, &[B]),
                    row(save, &[X]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(adjust_previous, &[LEFT]),
                    row(adjust_next, &[RIGHT]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::TextEntry => {
                let back = self.action(retail, 0x0044_51e0)?;
                let confirm_tab = self.action(retail, 0x0045_ae90)?;
                let confirm = self.action(retail, 0x0044_5200)?;
                let retry = self.action(retail, 0x0045_aea0)?;
                let previous = self.action(retail, 0x0044_5220)?;
                let next = self.action(retail, 0x0044_5240)?;
                let adjust_previous = self.action(retail, 0x0044_5260)?;
                let adjust_next = self.action(retail, 0x0044_5280)?;
                vec![
                    row(confirm, &[A]),
                    row(confirm_tab, &[START]),
                    row(back, &[B]),
                    row(retry, &[X]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(adjust_previous, &[LEFT]),
                    row(adjust_next, &[RIGHT]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::InWorldPause => {
                let close = self.action(retail, 0x0044_3700)?;
                let back = self.action(retail, 0x0044_3590)?;
                let previous = self.action(retail, 0x0044_34a0)?;
                let next = self.action(retail, 0x0044_34d0)?;
                let adjust_previous = self.action(retail, 0x0044_3500)?;
                let adjust_next = self.action(retail, 0x0044_3530)?;
                let confirm = self.action(retail, 0x0044_3560)?;
                vec![
                    row(confirm, &[A]),
                    row(back, &[B]),
                    // Retail's close row forbids Select, leaving 4+1 to quit.
                    row(close, &[START]).forbidding(&[VIEW]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(adjust_previous, &[LEFT]),
                    row(adjust_next, &[RIGHT]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::Map => {
                let close = self.action(retail, 0x0045_20a0)?;
                let up = self.action(retail, 0x0045_20c0)?;
                let down = self.action(retail, 0x0045_20f0)?;
                let left = self.action(retail, 0x0045_2120)?;
                let right = self.action(retail, 0x0045_2150)?;
                let zoom_out = self.action(retail, 0x0045_2180)?;
                let zoom_in = self.action(retail, 0x0045_21b0)?;
                vec![
                    // R3 and View both toggle the map, so both close it here.
                    row(close, &[A]),
                    row(close, &[B]),
                    row(close, &[START]),
                    row(close, &[VIEW]),
                    row(close, &[R3]),
                    row(up, &[UP]),
                    row(down, &[DOWN]),
                    row(left, &[LEFT]),
                    row(right, &[RIGHT]),
                    row(zoom_out, &[LB]),
                    row(zoom_in, &[RB]),
                ]
            }
            Self::Startup => {
                let accept = self.action(retail, 0x0043_27b0)?;
                let left = self.action(retail, 0x0043_27c0)?;
                let right = self.action(retail, 0x0043_27e0)?;
                vec![
                    row(accept, &[A]),
                    row(accept, &[START]),
                    row(left, &[LEFT]),
                    row(right, &[RIGHT]),
                    row(left, &[LB]),
                    row(right, &[RB]),
                ]
            }
            Self::KeyConfig => {
                // 00444BD0 authors no PlayStation rows; these descriptors are
                // its own keyboard rows, re-coded.
                let back = self.action(retail, 0x0044_51e0)?;
                let confirm = self.action(retail, 0x0044_5200)?;
                let previous = self.action(retail, 0x0044_5220)?;
                let next = self.action(retail, 0x0044_5240)?;
                let adjust_previous = self.action(retail, 0x0044_5260)?;
                let adjust_next = self.action(retail, 0x0044_5280)?;
                vec![
                    row(confirm, &[A]),
                    row(back, &[B]),
                    row(previous, &[UP]),
                    row(next, &[DOWN]),
                    row(adjust_previous, &[LEFT]),
                    row(adjust_next, &[RIGHT]),
                    row(previous, &[LB]),
                    row(next, &[RB]),
                ]
            }
            Self::Title => {
                let skip = self.action(retail, 0x0040_4870)?;
                (1..=16).map(|code| row(skip, &[code])).collect()
            }
            Self::Gameplay => {
                let map = self.action(retail, 0x0044_36d0)?;
                let first_person = self.action(retail, 0x0044_3650)?;
                let pause = self.action(retail, 0x0044_3700)?;
                vec![
                    // Retail's pause row forbids Select, leaving 4+1 to quit.
                    row(pause, &[START]).forbidding(&[VIEW]),
                    row(first_person, &[VIEW]),
                    row(map, &[R3]),
                ]
            }
            Self::GameplaySub => {
                let digital_thrust = self.descriptor(retail, DIGITAL_THRUST_DESCRIPTOR)?;
                let message_select = self.action(retail, 0x0046_97f0)?;
                let mode = self.action(retail, 0x0046_9580)?;
                let beam_down = self.action(retail, 0x0046_9670)?;
                let beam_up = self.action(retail, 0x0046_9660)?;
                let fire = self.descriptor(retail, PRIMARY_FIRE_DESCRIPTOR)?;
                let grenade = self.action(retail, 0x0046_94d0)?;
                let primary_step_down = self.action(retail, 0x0046_96e0)?;
                let grenade_step_up = self.action(retail, 0x0046_9700)?;
                // The detonator is a primary weapon slot the d-pad left steps
                // to; d-pad down rotates the carried cargo instead (user,
                // 2026-11-21), its `+30 = 1` step `00469680`.
                let cargo_rotate = self.action(retail, 0x0046_9680)?;
                let lights = self.action(retail, 0x0046_9740)?;
                let computer = self.action(retail, 0x0046_9450)?;
                vec![
                    // A is Cross: the digital thrust consumer also feeds the
                    // turbo request, and message select is Cross's CPU action.
                    row(digital_thrust, &[A]),
                    row(message_select, &[A]),
                    row(mode, &[B]),
                    row(beam_down, &[Y]),
                    row(beam_up, &[X]),
                    row(fire, &[LB]),
                    row(grenade, &[RB]),
                    // D-pad left/right are one-shot ring steps, not the held
                    // L2/R2 modifiers retail's PS1 gestures use.
                    row(primary_step_down, &[LEFT]),
                    row(grenade_step_up, &[RIGHT]),
                    row(cargo_rotate, &[DOWN]),
                    row(lights, &[UP]),
                    row(computer, &[L3]),
                ]
            }
        };
        Ok(digital_table(self.digital_base(), rows))
    }

    fn analog(
        self,
        retail: &ScreenInputTables,
    ) -> Result<Option<AnalogInputTable>, RemasteredBindingError> {
        // A screen with no authored type-9 analog rows keeps none: the port
        // does not add the runtime-snapshot cells retail's constructors use
        // for the pause and map screens, nor analog to menus that never had
        // it (results, startup, key configuration, title).
        let Some(source) = retail.analog[9].as_ref() else {
            return Ok(None);
        };
        let base = self.analog_base();
        if self == Self::GameplaySub {
            let mut records = source.records.clone();
            let thrust = records
                .iter_mut()
                .find(|record| record.descriptor.identity == THRUST_ANALOG_DESCRIPTOR)
                .ok_or(RemasteredBindingError::MissingAnalogDescriptor {
                    screen: self,
                    identity: THRUST_ANALOG_DESCRIPTOR,
                })?;
            // The PS1's right-stick Y falls upward and is negated; the trigger
            // rises with pressure, so the multiplier's sign flips. The Q31
            // half-scale and the zero offset stay retail's.
            thrust.axis = THRUST_AXIS;
            thrust.multiplier = 1024;
            return Ok(Some(analog_table(base, records)));
        }
        Ok(Some(analog_table(base, source.records.clone())))
    }

    /// A descriptor from any of this screen's retail digital rows.
    fn action(
        self,
        retail: &ScreenInputTables,
        callback: u32,
    ) -> Result<DigitalActionDescriptor, RemasteredBindingError> {
        self.find(retail, |descriptor| descriptor.callback_address == callback)
            .ok_or(RemasteredBindingError::MissingCallback {
                screen: self,
                callback,
            })
    }

    /// A descriptor whose callback is zero (`0053A378`, `0053A388`): its
    /// identity is the consumer record the gameplay code reads.
    fn descriptor(
        self,
        retail: &ScreenInputTables,
        identity: u32,
    ) -> Result<DigitalActionDescriptor, RemasteredBindingError> {
        self.find(retail, |descriptor| descriptor.identity == identity)
            .ok_or(RemasteredBindingError::MissingDescriptor {
                screen: self,
                identity,
            })
    }

    fn find(
        self,
        retail: &ScreenInputTables,
        predicate: impl Fn(&DigitalActionDescriptor) -> bool,
    ) -> Option<DigitalActionDescriptor> {
        // Prefer the console slot the screen authored, then the other
        // device tables, so a screen's own descriptor wins over a shared one.
        [9, 8, 6, 7, 2]
            .into_iter()
            .chain(0..INPUT_DEVICE_SLOTS)
            .filter_map(|slot| retail.digital[slot].as_ref())
            .flat_map(|table| table.records.iter())
            .map(|record| &record.descriptor)
            .find(|descriptor| predicate(descriptor))
            .copied()
    }
}

#[derive(Clone, Copy)]
struct Row {
    descriptor: DigitalActionDescriptor,
    required: [u8; 4],
    forbidden: [u8; 4],
}

impl Row {
    fn forbidding(mut self, codes: &[u8]) -> Self {
        self.forbidden = list(codes);
        self
    }
}

fn row(descriptor: DigitalActionDescriptor, required: &[u8]) -> Row {
    Row {
        descriptor,
        required: list(required),
        forbidden: [0; 4],
    }
}

fn list(codes: &[u8]) -> [u8; 4] {
    let mut result = [0; 4];
    for (slot, &code) in result.iter_mut().zip(codes) {
        *slot = code;
    }
    result
}

fn digital_table(base: u32, rows: Vec<Row>) -> DigitalInputTable {
    let stride = DIGITAL_INPUT_RECORD_BYTES as u32;
    let count = rows.len() as u32;
    DigitalInputTable {
        virtual_address: base,
        records: rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let start = base + index as u32 * stride;
                DigitalInputRecord {
                    virtual_address: start,
                    file_span: ByteSpan::new(start as usize, (start + stride) as usize),
                    descriptor: row.descriptor,
                    required: [row.required, [0; 4]],
                    forbidden: [row.forbidden, [0; 4]],
                }
            })
            .collect(),
        terminator: ByteSpan::new(
            (base + count * stride) as usize,
            (base + count * stride + 4) as usize,
        ),
    }
}

fn analog_table(base: u32, records: Vec<AnalogInputRecord>) -> AnalogInputTable {
    let stride = ANALOG_INPUT_RECORD_BYTES as u32;
    let count = records.len() as u32;
    AnalogInputTable {
        virtual_address: base,
        records: records
            .into_iter()
            .enumerate()
            .map(|(index, mut record)| {
                let start = base + index as u32 * stride;
                record.virtual_address = start;
                record.file_span = ByteSpan::new(start as usize, (start + stride) as usize);
                record
            })
            .collect(),
        terminator: ByteSpan::new(
            (base + count * stride) as usize,
            (base + count * stride + 4) as usize,
        ),
    }
}

#[cfg(test)]
#[path = "remastered_bindings_tests.rs"]
mod tests;
