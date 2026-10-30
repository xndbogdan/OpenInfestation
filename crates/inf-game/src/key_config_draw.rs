//! Phase-12 key configuration draw `004450B0` and its binding row `004452A0`.
//!
//! The draw runs the `00439270` preamble, unlocks the surface (`004957F0`)
//! and draws the menu controller over the session's caller context
//! (`00453610`). Presentation is the shared `0040D1D0` (H7 slot 32 clear).
//! Label rows are the generic `00453A80`. Each binding row copies its seven
//! descriptor words, narrows the copy to three quarters of the row width and
//! passes the action name (H0 label) left and the binding right to
//! `00453810`:
//!
//! - while its capture byte is set, H0 string `0x3A4` on odd 300 ms steps
//!   of the shared clock (`005471C0 / 15`), nothing on even ones;
//! - otherwise each of the record's four codes as `"%s "` of its key name,
//!   in H6 style `0x25` (or `0x26` on the selected row) when another action
//!   shares the first code (`00444E00`).
//!
//! A joystick button bound to the action's descriptor then adds
//! `"Joystick %d"` (button + 1) on the right, at the original width and
//! colour. The caller's descriptor is never written back, so each binding
//! row spans one row spacing.

use thiserror::Error;

use crate::key_bindings::{KeyBindingError, KeyBindingTables, KeyBindings, KEY_BINDING_ROWS};
use crate::menu::{MenuColor, MenuRowDescriptor, MenuRowDrawRequest};
use crate::menu_text::{MenuTextPlacement, MenuTextResources};

/// The binding row callback of all three pages.
pub const BINDING_ROW_CALLBACK: u32 = 0x0044_52a0;
/// H0 string blinking on a capturing row.
pub const CAPTURE_PROMPT_STRING: i32 = 0x3a4;
/// H6 styles of a duplicated binding: ordinary and selected rows.
pub const DUPLICATE_STYLES: [i32; 2] = [0x25, 0x26];

/// A binding row's `00453810` calls over one local descriptor copy: the
/// first with `local`, then the joystick text with the original colour and
/// width restored on whatever the first call left in the copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRowPlan {
    pub local: MenuRowDescriptor,
    pub left: Option<Vec<u8>>,
    pub right: Option<Vec<u8>>,
    pub joystick: Option<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum BindingRowError<R> {
    #[error("binding row {0} is outside the 44 authored rows")]
    Row(u32),
    #[error(transparent)]
    Bindings(#[from] KeyBindingError),
    #[error("binding code {0:#04x} is past the 0xDE key names")]
    KeyName(u8),
    #[error("binding row resource: {0}")]
    Resource(R),
}

/// The read-only state a binding row reads.
#[derive(Clone, Copy)]
pub struct BindingRowState<'a> {
    pub tables: &'a KeyBindingTables,
    pub bindings: &'a KeyBindings,
    pub capturing: bool,
    /// Shared clock `005471C0`.
    pub clock: u32,
}

/// `004452A0`: the texts and local descriptor of a binding row.
pub fn binding_row_plan<R: MenuTextResources>(
    request: &MenuRowDrawRequest,
    state: BindingRowState<'_>,
    resources: &R,
) -> Result<BindingRowPlan, BindingRowError<R::Error>> {
    let row = request.arguments[0];
    let entry = usize::try_from(row)
        .ok()
        .and_then(|row| state.tables.rows.get(row))
        .filter(|_| (row as usize) < KEY_BINDING_ROWS)
        .ok_or(BindingRowError::Row(row))?;
    let original = request.descriptor;
    let mut local = original;
    local.extent_pixels[0] = (i32::from(original.extent_pixels[0]) * 3 / 4) as i16;
    let label = || -> Result<Option<Vec<u8>>, BindingRowError<R::Error>> {
        // 004454A0: a negative WORD label resolves to NULL.
        if entry.label_resource < 0 {
            return Ok(None);
        }
        Ok(Some(
            resources
                .string(i32::from(entry.label_resource))
                .map_err(BindingRowError::Resource)?
                .to_vec(),
        ))
    };
    let (left, right);
    if state.capturing {
        // 0044530C: (clock / 15) & 0x80000001, signed: odd steps blink.
        let odd = (state.clock as i32 / 15) % 2 != 0;
        let prompt = if odd {
            Some(
                resources
                    .string(CAPTURE_PROMPT_STRING)
                    .map_err(BindingRowError::Resource)?
                    .to_vec(),
            )
        } else {
            None
        };
        left = label()?;
        right = prompt;
    } else {
        let record = state.bindings.record(entry.action_index)?;
        if state.bindings.duplicated(entry.action_index) {
            let style = DUPLICATE_STYLES[usize::from(local.flags & 1 != 0)];
            local.color =
                MenuColor::from_word(resources.style(style).map_err(BindingRowError::Resource)?);
        }
        let mut text = Vec::new();
        for code in record.codes {
            let name = state
                .tables
                .key_name(code)
                .ok_or(BindingRowError::KeyName(code))?;
            text.extend_from_slice(name);
            text.push(b' ');
        }
        left = label()?;
        right = Some(text);
    }
    let descriptor = state.tables.action_descriptors[usize::from(entry.action_index)];
    let joystick = state
        .bindings
        .button_for(descriptor)
        .map(|button| format!("Joystick {}", button + 1).into_bytes());
    Ok(BindingRowPlan {
        local,
        left,
        right,
        joystick,
    })
}

impl BindingRowPlan {
    /// The sides of the first `00453810` call, left first.
    pub fn first_sides(&self) -> impl Iterator<Item = (MenuTextPlacement, &[u8])> {
        [
            self.left
                .as_deref()
                .map(|text| (MenuTextPlacement::Left, text)),
            self.right
                .as_deref()
                .map(|text| (MenuTextPlacement::Right, text)),
        ]
        .into_iter()
        .flatten()
    }
}

pub(crate) mod software;

#[cfg(test)]
#[path = "key_config_draw_tests.rs"]
mod tests;
