//! The Sound menus' Ambience and Headphones rows: the volume of the sound the port adds, 0 to 15 as retail's Sound
//! and Music, and whether its placed sounds reach each ear apart, under
//! Vulkan only. Copies of retail's Sound slider on cells of the port's own,
//! as the Video menus' Vulkan rows are copies of its Filtering row.

use crate::menu::MenuCommand;
use crate::modern_settings::{ModernSettings, MOST_AMBIENCE};
use crate::port_text::PortText;

/// Retail's Sound menu, and the in-world Options' (the same rows under the
/// in-world chain).
pub const SOUND_MENU: u32 = 0x0053_8e68;
pub const IN_WORLD_SOUND_MENU: u32 = 0x0053_8e80;

/// The Ambience row's cell: an address no retail record has, past the
/// Vulkan rows'.
pub const AMBIENCE_CELL: u32 = crate::frontend_video::MODERN_CELLS + 0x100;
/// The Headphones row's, the next.
pub const HEADPHONES_CELL: u32 = AMBIENCE_CELL + 4;

/// Add the Ambience and Headphones rows to a Sound menu's definition,
/// before Back: copies of the Sound slider (`+00`) on their cells. The last
/// layout counts them too.
pub fn append_ambience_row(menu: &mut inf_formats::FrontendMenu) {
    let sound = crate::frontend_settings::SETTINGS_STATE_ADDRESS;
    let Some(template) = menu
        .items
        .iter()
        .find(|row| row.draw_arguments[1] == sound)
        .copied()
    else {
        return;
    };
    let back = menu.items.pop();
    for (address, cell) in [(0xfff1_0000, AMBIENCE_CELL), (0xfff1_0004, HEADPHONES_CELL)] {
        menu.items.push(inf_formats::FrontendMenuItem {
            virtual_address: address,
            span: inf_formats::ByteSpan { start: 0, end: 0 },
            draw_arguments: [template.draw_arguments[0], cell],
            action_arguments: [template.action_arguments[0], cell],
            ..template
        });
    }
    menu.items.extend(back);
    if let Some(layout) = menu.layouts.last_mut() {
        layout.item_count += 2;
    }
}

/// The Headphones row's two sides in `language`: its label, and On or Off,
/// or Unavailable without the remastered sounds.
pub fn headphones_texts(on: bool, available: bool, language: u8) -> (Vec<u8>, Vec<u8>) {
    let label = PortText::Headphones.text(language).to_vec();
    let value = match (available, on) {
        (false, _) => PortText::Unavailable,
        (true, true) => PortText::On,
        (true, false) => PortText::Off,
    };
    (label, value.text(language).to_vec())
}

/// Turn Headphones by `command` as retail's value rows turn (sideways
/// clamped, Return wrapping). Whether it changed.
pub fn adjust_headphones(modern: &mut ModernSettings, command: MenuCommand) -> bool {
    let mut on = u32::from(modern.headphones);
    crate::settings_row::adjust_value(command, 1, &mut on);
    let changed = (on != 0) != modern.headphones;
    modern.headphones = on != 0;
    changed
}

/// The row's two sides in `language`: its label, and the slider retail's
/// Sound row draws for the volume (`argument` its width, Off at 0), or
/// Unavailable without the remastered sounds.
pub fn ambience_texts(
    argument: u32,
    level: u8,
    available: bool,
    language: u8,
) -> (Vec<u8>, Vec<u8>) {
    let label = PortText::Ambience.text(language).to_vec();
    let value = if !available {
        PortText::Unavailable.text(language).to_vec()
    } else if level == 0 {
        PortText::Off.text(language).to_vec()
    } else {
        crate::settings_row::volume_slider(argument, u32::from(level)).unwrap_or_default()
    };
    (label, value)
}

/// Move the Ambience volume by `command` as retail's sliders move
/// (`00453B80`): down to 0, up to 15. Whether it moved.
pub fn adjust_ambience(modern: &mut ModernSettings, command: MenuCommand) -> bool {
    let mut level = u32::from(modern.ambience.min(MOST_AMBIENCE));
    crate::settings_row::adjust_value(command, u32::from(MOST_AMBIENCE), &mut level);
    let moved = level != u32::from(modern.ambience);
    modern.ambience = level as u8;
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row moves as retail's sliders do and draws as its Sound row
    /// does: Off at 0, a bar above, Unavailable without the sounds.
    #[test]
    fn the_ambience_row_moves_and_draws_as_retails_sliders() {
        let mut modern = ModernSettings::default();
        assert_eq!(modern.ambience, 4);
        assert!(adjust_ambience(&mut modern, MenuCommand::AdjustNext));
        assert_eq!(modern.ambience, 5);
        modern.ambience = 15;
        assert!(!adjust_ambience(&mut modern, MenuCommand::AdjustNext));
        modern.ambience = 0;
        assert!(!adjust_ambience(&mut modern, MenuCommand::AdjustPrevious));
        let width = 0x000f_008c;
        assert_eq!(ambience_texts(width, 0, true, 0).1, b"Off");
        assert_eq!(ambience_texts(width, 5, false, 0).1, b"Unavailable");
        let bar = ambience_texts(width, 5, true, 0).1;
        assert_eq!(bar.len(), 15);
        assert_eq!(bar.iter().filter(|&&glyph| glyph == 0x1e).count(), 5);
        assert_eq!(ambience_texts(width, 5, true, 1).0, b"Ambiance");
    }

    /// Headphones turn on and off as retail's value rows do, and read On,
    /// Off or Unavailable; the Sound menus gain both rows before Back.
    #[test]
    fn the_headphones_row_turns_and_draws() {
        let mut modern = ModernSettings::default();
        assert!(!modern.headphones);
        assert!(adjust_headphones(&mut modern, MenuCommand::AdjustNext));
        assert!(modern.headphones);
        assert!(!adjust_headphones(&mut modern, MenuCommand::AdjustNext));
        assert!(adjust_headphones(&mut modern, MenuCommand::AdjustPrevious));
        assert_eq!(
            headphones_texts(true, true, 0),
            (b"Headphones".to_vec(), b"On".to_vec())
        );
        assert_eq!(headphones_texts(false, true, 0).1, b"Off");
        assert_eq!(headphones_texts(true, false, 0).1, b"Unavailable");
        assert_eq!(headphones_texts(true, true, 2).0, b"Kopfh\xf6rer");
    }
}
