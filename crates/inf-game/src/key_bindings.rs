//! Keyboard and joystick bindings behind the phase-12 key configuration
//! screen and the registry load/save pair `00445680`/`00445570`.
//!
//! Each of the 44 rows at `00535658` (six bytes: H0 label, action index,
//! capture byte) names a gameplay digital record through `00444E50`: action
//! indices below 38 live at `[00534CA0]` (`00534570`), the rest at
//! `[00534410]` (`00534290`). A capture writes the held scan codes into the
//! record's first required list (`+4..+7`). Restore Defaults (`00444B50`)
//! copies whole records from `[00534CA4]`/`[00534414]`, whose first table
//! also forbids Alt (`+0C = 38`) where the startup image does not. The
//! sixteen joystick records at `[00534F6C]` (`00534CC0`) bind a button to an
//! action descriptor; their defaults sit at `[00534F70]` (`00534E18`).
//!
//! The registry stores each row's first code as its key name and each bound
//! button as its action name. The load resolves a key name to the first
//! code with that name, so unnamed codes reload as code 0.

use inf_formats::{
    FrontendExecutable, FrontendExecutableError, InputTableError, KeyNameTable, ScreenInputTables,
};
use thiserror::Error;

/// Rows at `00535658`, across the three key configuration pages.
pub const KEY_BINDING_ROWS: usize = 44;
/// Joystick records at `[00534F6C]`.
pub const JOYSTICK_BUTTONS: usize = 16;
/// `00444E50` splits the action indices at 38.
pub const FIRST_TABLE_ACTIONS: u16 = 38;
/// The no-op descriptor `00444FE6` stores on a button that loses its action.
pub const UNBOUND_BUTTON_DESCRIPTOR: u32 = 0x0053_5650;
/// The Fire descriptor whose button code `0044501A` mirrors.
pub const FIRE_DESCRIPTOR: u32 = 0x0053_a388;

const ROW_TABLE: u32 = 0x0053_5658;
const LIVE_FIRST: u32 = 0x0053_4ca0;
const DEFAULT_FIRST: u32 = 0x0053_4ca4;
const LIVE_SECOND: u32 = 0x0053_4410;
const DEFAULT_SECOND: u32 = 0x0053_4414;
const LIVE_BUTTONS: u32 = 0x0053_4f6c;
const DEFAULT_BUTTONS: u32 = 0x0053_4f70;
const ACTION_NAMES: u32 = 0x0053_61d0;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyBindingError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("key binding row {row} names action {action}, outside the two authored tables")]
    Action { row: usize, action: u16 },
    #[error("key binding action {0} is outside the 44 authored actions")]
    UnknownAction(u16),
    #[error("action name at {0:#010x} has no terminator within 64 bytes")]
    Name(u32),
    #[error("authored joystick record at {0:#010x} binds a descriptor with no action name")]
    UnnamedButton(u32),
    #[error(transparent)]
    Table(#[from] InputTableError),
}

/// One authored row: its H0 label and the action record it edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBindingRow {
    pub label_resource: i16,
    pub action_index: u16,
}

/// The mutable part of one keyboard record (`+4..+7` and `+0C..+0F`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyRecord {
    pub codes: [u8; 4],
    pub forbidden: [u8; 4],
}

/// Authored tables read once from the supported executable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBindingTables {
    pub rows: [KeyBindingRow; KEY_BINDING_ROWS],
    /// Record `+0` per action index; restores never change it.
    pub action_descriptors: [u32; KEY_BINDING_ROWS],
    pub initial: [KeyRecord; KEY_BINDING_ROWS],
    pub defaults: [KeyRecord; KEY_BINDING_ROWS],
    pub initial_buttons: [u32; JOYSTICK_BUTTONS],
    pub default_buttons: [u32; JOYSTICK_BUTTONS],
    /// `+144` of the live joystick table: the terminator record's `+4` byte.
    pub initial_button_tail: u8,
    /// `005361D0` pairs: action name and descriptor.
    pub action_names: Vec<(Vec<u8>, u32)>,
    pub key_names: Vec<Vec<u8>>,
}

impl KeyBindingTables {
    pub fn from_executable(
        source: FrontendExecutable<'_>,
        key_names: &KeyNameTable,
    ) -> Result<Self, KeyBindingError> {
        let mut rows = [KeyBindingRow {
            label_resource: 0,
            action_index: 0,
        }; KEY_BINDING_ROWS];
        for (index, row) in rows.iter_mut().enumerate() {
            let bytes = source.mapped(ROW_TABLE + index as u32 * 6, 6)?.bytes;
            let action_index = u16::from_le_bytes([bytes[2], bytes[3]]);
            if usize::from(action_index) >= KEY_BINDING_ROWS {
                return Err(KeyBindingError::Action {
                    row: index,
                    action: action_index,
                });
            }
            *row = KeyBindingRow {
                label_resource: i16::from_le_bytes([bytes[0], bytes[1]]),
                action_index,
            };
        }
        let record = |live: bool, action: u16| -> Result<(u32, KeyRecord), KeyBindingError> {
            let (pointer, index) = match (action < FIRST_TABLE_ACTIONS, live) {
                (true, true) => (LIVE_FIRST, action),
                (true, false) => (DEFAULT_FIRST, action),
                (false, true) => (LIVE_SECOND, action - FIRST_TABLE_ACTIONS),
                (false, false) => (DEFAULT_SECOND, action - FIRST_TABLE_ACTIONS),
            };
            let address = source.word(pointer)? + u32::from(index) * 20;
            let bytes = source.mapped(address, 20)?.bytes;
            let word =
                |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            Ok((
                word(0),
                KeyRecord {
                    codes: bytes[4..8].try_into().unwrap(),
                    forbidden: bytes[12..16].try_into().unwrap(),
                },
            ))
        };
        let mut action_descriptors = [0; KEY_BINDING_ROWS];
        let mut initial = [KeyRecord::default(); KEY_BINDING_ROWS];
        let mut defaults = [KeyRecord::default(); KEY_BINDING_ROWS];
        for action in 0..KEY_BINDING_ROWS as u16 {
            let (descriptor, live) = record(true, action)?;
            action_descriptors[usize::from(action)] = descriptor;
            initial[usize::from(action)] = live;
            defaults[usize::from(action)] = record(false, action)?.1;
        }
        let buttons = |pointer: u32| -> Result<[u32; JOYSTICK_BUTTONS], KeyBindingError> {
            let base = source.word(pointer)?;
            let mut out = [0; JOYSTICK_BUTTONS];
            for (index, value) in out.iter_mut().enumerate() {
                *value = source.word(base + index as u32 * 20)?;
            }
            Ok(out)
        };
        let live_buttons = source.word(LIVE_BUTTONS)?;
        let mut action_names = Vec::new();
        let mut address = ACTION_NAMES;
        loop {
            let name = source.word(address)?;
            if name == 0 {
                break;
            }
            action_names.push((c_string(source, name)?, source.word(address + 4)?));
            address += 8;
        }
        let initial_buttons = buttons(LIVE_BUTTONS)?;
        let default_buttons = buttons(DEFAULT_BUTTONS)?;
        // Every authored button names its action, so 00445570 stores all 16.
        for (pointer, values) in [
            (LIVE_BUTTONS, initial_buttons),
            (DEFAULT_BUTTONS, default_buttons),
        ] {
            for (index, descriptor) in values.into_iter().enumerate() {
                if !action_names
                    .iter()
                    .any(|(_, candidate)| *candidate == descriptor)
                {
                    return Err(KeyBindingError::UnnamedButton(
                        source.word(pointer)? + index as u32 * 20,
                    ));
                }
            }
        }
        Ok(Self {
            rows,
            action_descriptors,
            initial,
            defaults,
            initial_buttons,
            default_buttons,
            initial_button_tail: source.mapped(live_buttons + 0x144, 1)?.bytes[0],
            action_names,
            key_names: key_names
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
        })
    }

    /// The name `00445570` stores for a code, or `None` past the 0xDE names.
    pub fn key_name(&self, code: u8) -> Option<&[u8]> {
        self.key_names.get(usize::from(code)).map(Vec::as_slice)
    }

    /// `00445680`: the first code whose name matches.
    pub fn code_for_name(&self, name: &[u8]) -> Option<u8> {
        self.key_names
            .iter()
            .position(|candidate| candidate == name)
            .map(|code| code as u8)
    }

    /// `00445570`: the action name a button's descriptor stores, if any.
    pub fn action_name(&self, descriptor: u32) -> Option<&[u8]> {
        self.action_names
            .iter()
            .find(|(_, candidate)| *candidate == descriptor)
            .map(|(name, _)| name.as_slice())
    }

    /// `00445680`: the descriptor of the first pair with this name.
    pub fn descriptor_for_name(&self, name: &[u8]) -> Option<u32> {
        self.action_names
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, descriptor)| *descriptor)
    }
}

/// The live records the key configuration screen edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBindings {
    records: [KeyRecord; KEY_BINDING_ROWS],
    buttons: [u32; JOYSTICK_BUTTONS],
    button_tail: u8,
}

impl KeyBindings {
    /// The live records the gameplay constructor's 00439B30 reads. The
    /// sub-object's keyboard rows are the 38 records below `[00534CA0]`, the
    /// screen's first six rows the records below `[00534410]`, and the
    /// sub-object's joystick rows take each button's bound descriptor. Only
    /// the first source's required and forbidden lists are editable.
    pub fn apply_to_gameplay_tables(
        &self,
        screen: &mut ScreenInputTables,
        sub_object: &mut ScreenInputTables,
        executable: FrontendExecutable<'_>,
    ) -> Result<(), KeyBindingError> {
        let first = usize::from(FIRST_TABLE_ACTIONS);
        if let Some(table) = sub_object.digital[0].as_mut() {
            for (record, live) in table.records.iter_mut().zip(&self.records[..first]) {
                record.required[0] = live.codes;
                record.forbidden[0] = live.forbidden;
            }
        }
        if let Some(table) = screen.digital[0].as_mut() {
            for (record, live) in table.records.iter_mut().zip(&self.records[first..]) {
                record.required[0] = live.codes;
                record.forbidden[0] = live.forbidden;
            }
        }
        if let Some(table) = sub_object.digital[2].as_mut() {
            for (record, &descriptor) in table.records.iter_mut().zip(&self.buttons) {
                if record.descriptor.identity != descriptor {
                    record.descriptor =
                        inf_formats::parse_digital_action_descriptor(executable, descriptor)?;
                }
            }
        }
        Ok(())
    }

    /// The retail startup records, for test platforms without settings.
    #[cfg(test)]
    pub(crate) fn retail_initial() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root)
            .expect("retail Infest.exe is required");
        let tables = KeyBindingTables::from_executable(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
            assets.key_names(),
        )
        .unwrap();
        Self::initial(&tables)
    }

    /// The startup image of the live tables.
    pub fn initial(tables: &KeyBindingTables) -> Self {
        Self {
            records: tables.initial,
            buttons: tables.initial_buttons,
            button_tail: tables.initial_button_tail,
        }
    }

    pub fn record(&self, action: u16) -> Result<KeyRecord, KeyBindingError> {
        self.records
            .get(usize::from(action))
            .copied()
            .ok_or(KeyBindingError::UnknownAction(action))
    }

    pub fn buttons(&self) -> [u32; JOYSTICK_BUTTONS] {
        self.buttons
    }

    /// The terminator `+4` byte `0044501D` writes; nothing reads it.
    pub fn button_tail(&self) -> u8 {
        self.button_tail
    }

    /// `00444F33`: the held codes, ascending, zero-filled to four.
    pub fn set_codes(&mut self, action: u16, codes: [u8; 4]) -> Result<(), KeyBindingError> {
        let record = self
            .records
            .get_mut(usize::from(action))
            .ok_or(KeyBindingError::UnknownAction(action))?;
        record.codes = codes;
        Ok(())
    }

    /// `00445752`: a reloaded key name rewrites only the first code byte.
    pub fn set_first_code(&mut self, action: u16, code: u8) -> Result<(), KeyBindingError> {
        let record = self
            .records
            .get_mut(usize::from(action))
            .ok_or(KeyBindingError::UnknownAction(action))?;
        record.codes[0] = code;
        Ok(())
    }

    /// `00444F93..0044501D`: bind a joystick button to the action's
    /// descriptor, unbind every other button holding it, and mirror the
    /// button code into the terminator byte for Fire.
    pub fn bind_button(&mut self, button: usize, descriptor: u32, button_code: u8) -> Option<()> {
        *self.buttons.get_mut(button)? = descriptor;
        for (index, other) in self.buttons.iter_mut().enumerate() {
            if index != button && *other == descriptor {
                *other = UNBOUND_BUTTON_DESCRIPTOR;
            }
        }
        if descriptor == FIRE_DESCRIPTOR {
            self.button_tail = button_code;
        }
        Some(())
    }

    /// `00445680`: a reloaded button name rewrites the descriptor only.
    pub fn set_button(&mut self, button: usize, descriptor: u32) -> Option<()> {
        *self.buttons.get_mut(button)? = descriptor;
        Some(())
    }

    /// `00444B50`: copy every default record and button record. The
    /// terminator byte is outside the sixteen copied records.
    pub fn restore_defaults(&mut self, tables: &KeyBindingTables) {
        self.records = tables.defaults;
        self.buttons = tables.default_buttons;
    }

    /// `00444E00`: another row's action shares this action's first code.
    /// Actions 0x20..0x24 neither collide nor are collided with.
    pub fn duplicated(&self, action: u16) -> bool {
        let exempt = |action: u16| (0x20..=0x24).contains(&action);
        if exempt(action) {
            return false;
        }
        let Some(first) = self.records.get(usize::from(action)).map(|r| r.codes[0]) else {
            return false;
        };
        (0..KEY_BINDING_ROWS as u16).any(|other| {
            other != action && !exempt(other) && self.records[usize::from(other)].codes[0] == first
        })
    }

    /// `00445503`: the first button bound to the action's descriptor.
    pub fn button_for(&self, descriptor: u32) -> Option<usize> {
        self.buttons.iter().position(|button| *button == descriptor)
    }
}

fn c_string(source: FrontendExecutable<'_>, address: u32) -> Result<Vec<u8>, KeyBindingError> {
    let mut bytes = Vec::new();
    for offset in 0..64 {
        let byte = source.mapped(address + offset, 1)?.bytes[0];
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
    }
    Err(KeyBindingError::Name(address))
}

#[cfg(test)]
#[path = "key_bindings_tests.rs"]
mod tests;
