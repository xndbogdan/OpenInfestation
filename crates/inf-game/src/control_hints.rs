//! The controller wording of Handler-0 hints. See
//! `docs/re/CONTROL_HINTS_PS1.md`: the PlayStation release replaces six
//! level-1 hint records (globals 865..870) and the four shell prompts
//! (globals 37..40) at the PC's own IDs.

/// The controller wording for H0 global `global_id` in residency language `language` (0..5; others read as English), or None when the PC text stands.
pub fn pad_hint(global_id: u32, language: u8) -> Option<&'static [u8]> {
    // shortcut: stub until the table lands; the lead replaces this file.
    let _ = (global_id, language);
    None
}
