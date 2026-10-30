//! Small software-canvas UI shared by all diagnostic views.

use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Models,
    Sprites,
    Terrain,
}

impl Tab {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Models => "MODELS",
            Self::Sprites => "SPRITES",
            Self::Terrain => "TERRAIN",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    SetTab(Tab),
    Previous,
    Next,
    FocusModelPosition,
    FocusSearch,
    CycleModelPresentation,
    FocusInspectionEye,
    AdjustInspectionEye { axis: usize, delta: i32 },
    CycleInspectionShift,
    ResetPosition,
    CycleModelVariant,
    ToggleModelTextures,
    CycleModelTextureDifficulty,
    CycleModelTextureMode,
    CycleModelQuadSplit,
    CycleSpriteMode,
    CycleSpriteVariant,
    CycleSpriteShade,
    CycleTerrainChannel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl UiRect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x + self.width as i32
            && y < self.y + self.height as i32
    }

    pub fn as_sdl(self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLayout {
    pub viewport: UiRect,
    pub sidebar: UiRect,
    pub status: UiRect,
    pub model_tab: UiRect,
    pub sprite_tab: UiRect,
    pub terrain_tab: UiRect,
    pub model_presentation: UiRect,
    pub previous: UiRect,
    pub next: UiRect,
    pub model_position: UiRect,
    pub search: UiRect,
    pub model_variant: UiRect,
    pub reset_position: UiRect,
    pub model_texture_difficulty: UiRect,
    pub model_texture_mode: UiRect,
    pub model_quad_split: UiRect,
    pub sprite_mode: UiRect,
    pub sprite_variant: UiRect,
    pub sprite_shade: UiRect,
    pub terrain_channel: UiRect,
    pub texture_checkbox: UiRect,
    pub shadow_checkbox: UiRect,
}

impl UiLayout {
    pub fn new(width: u32, height: u32) -> Self {
        const TOP: u32 = 78;
        const BOTTOM: u32 = 34;
        let sidebar_width = width.clamp(760, 1_600) / 4;
        let viewport_width = width.saturating_sub(sidebar_width);
        let content_height = height.saturating_sub(TOP + BOTTOM);
        let tab_width = 118;
        let control_y = 43;
        let sidebar_x = viewport_width as i32;
        let sidebar_inner_x = sidebar_x + 14;
        let sidebar_control_width = sidebar_width.saturating_sub(28);
        Self {
            viewport: UiRect::new(0, TOP as i32, viewport_width, content_height),
            sidebar: UiRect::new(sidebar_x, TOP as i32, sidebar_width, content_height),
            status: UiRect::new(0, (height.saturating_sub(BOTTOM)) as i32, width, BOTTOM),
            model_tab: UiRect::new(12, 8, tab_width, 28),
            sprite_tab: UiRect::new(136, 8, tab_width, 28),
            terrain_tab: UiRect::new(260, 8, tab_width, 28),
            model_presentation: UiRect::new(384, 8, 174, 28),
            previous: UiRect::new(12, control_y, 72, 26),
            next: UiRect::new(90, control_y, 72, 26),
            model_position: UiRect::new(174, control_y, 150, 26),
            search: UiRect::new(330, control_y, viewport_width.saturating_sub(342), 26),
            model_variant: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 230,
                sidebar_control_width,
                26,
            ),
            reset_position: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 264,
                sidebar_control_width,
                26,
            ),
            model_texture_difficulty: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 334,
                sidebar_control_width,
                26,
            ),
            model_texture_mode: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 368,
                sidebar_control_width,
                26,
            ),
            model_quad_split: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 402,
                sidebar_control_width,
                26,
            ),
            sprite_mode: UiRect::new(sidebar_inner_x, TOP as i32 + 184, sidebar_control_width, 26),
            sprite_variant: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 218,
                sidebar_control_width,
                26,
            ),
            sprite_shade: UiRect::new(sidebar_inner_x, TOP as i32 + 252, sidebar_control_width, 26),
            terrain_channel: UiRect::new(
                sidebar_inner_x,
                TOP as i32 + 184,
                sidebar_control_width,
                26,
            ),
            texture_checkbox: UiRect::new(sidebar_inner_x, TOP as i32 + 306, 16, 16),
            shadow_checkbox: UiRect::new(sidebar_inner_x, TOP as i32 + 438, 16, 16),
        }
    }

    pub fn action_at(
        &self,
        tab: Tab,
        reset_enabled: bool,
        model_variant_enabled: bool,
        x: i32,
        y: i32,
    ) -> Option<UiAction> {
        if self.model_tab.contains(x, y) {
            Some(UiAction::SetTab(Tab::Models))
        } else if self.sprite_tab.contains(x, y) {
            Some(UiAction::SetTab(Tab::Sprites))
        } else if self.terrain_tab.contains(x, y) {
            Some(UiAction::SetTab(Tab::Terrain))
        } else if tab == Tab::Models && self.model_presentation.contains(x, y) {
            Some(UiAction::CycleModelPresentation)
        } else if self.previous.contains(x, y) {
            Some(UiAction::Previous)
        } else if self.next.contains(x, y) {
            Some(UiAction::Next)
        } else if tab == Tab::Models && self.model_position.contains(x, y) {
            Some(UiAction::FocusModelPosition)
        } else if tab == Tab::Models && self.search.contains(x, y) {
            Some(UiAction::FocusSearch)
        } else {
            match tab {
                Tab::Models if model_variant_enabled && self.model_variant.contains(x, y) => {
                    Some(UiAction::CycleModelVariant)
                }
                Tab::Models if reset_enabled && self.reset_position.contains(x, y) => {
                    Some(UiAction::ResetPosition)
                }
                Tab::Models if self.texture_checkbox.contains(x, y) => {
                    Some(UiAction::ToggleModelTextures)
                }
                Tab::Models if self.model_texture_difficulty.contains(x, y) => {
                    Some(UiAction::CycleModelTextureDifficulty)
                }
                Tab::Models if self.model_texture_mode.contains(x, y) => {
                    Some(UiAction::CycleModelTextureMode)
                }
                Tab::Models if self.model_quad_split.contains(x, y) => {
                    Some(UiAction::CycleModelQuadSplit)
                }
                Tab::Sprites if self.sprite_mode.contains(x, y) => Some(UiAction::CycleSpriteMode),
                Tab::Sprites if self.sprite_variant.contains(x, y) => {
                    Some(UiAction::CycleSpriteVariant)
                }
                Tab::Sprites if self.sprite_shade.contains(x, y) => {
                    Some(UiAction::CycleSpriteShade)
                }
                Tab::Terrain if self.terrain_channel.contains(x, y) => {
                    Some(UiAction::CycleTerrainChannel)
                }
                _ => None,
            }
        }
    }
}

pub const BACKGROUND: Color = Color::RGB(20, 23, 29);
pub const VIEWPORT_BACKGROUND: Color = Color::RGB(10, 14, 19);
pub const PANEL: Color = Color::RGB(31, 36, 44);
pub const PANEL_ALT: Color = Color::RGB(39, 45, 55);
pub const BORDER: Color = Color::RGB(77, 89, 106);
pub const TEXT: Color = Color::RGB(224, 230, 237);
pub const MUTED: Color = Color::RGB(145, 155, 168);
pub const DISABLED: Color = Color::RGB(91, 99, 110);
pub const ACCENT: Color = Color::RGB(74, 175, 222);
pub const WARNING: Color = Color::RGB(245, 179, 66);

pub fn fill(canvas: &mut Canvas<Window>, rect: UiRect, color: Color) -> Result<(), String> {
    canvas.set_draw_color(color);
    canvas.fill_rect(rect.as_sdl())
}

pub fn outline(canvas: &mut Canvas<Window>, rect: UiRect, color: Color) -> Result<(), String> {
    canvas.set_draw_color(color);
    canvas.draw_rect(rect.as_sdl())
}

pub fn button(
    canvas: &mut Canvas<Window>,
    rect: UiRect,
    label: &str,
    selected: bool,
    enabled: bool,
) -> Result<(), String> {
    let background = if selected {
        Color::RGB(42, 93, 116)
    } else if enabled {
        PANEL_ALT
    } else {
        Color::RGB(29, 33, 40)
    };
    fill(canvas, rect, background)?;
    outline(canvas, rect, if selected { ACCENT } else { BORDER })?;
    let color = if enabled { TEXT } else { DISABLED };
    let text_width = text_width(label, 1);
    let x = rect.x + ((rect.width as i32 - text_width as i32) / 2).max(3);
    let y = rect.y + ((rect.height as i32 - 7) / 2);
    draw_text(canvas, x, y, label, color, 1)
}

pub fn checkbox(
    canvas: &mut Canvas<Window>,
    rect: UiRect,
    label: &str,
    checked: bool,
    enabled: bool,
) -> Result<(), String> {
    fill(canvas, rect, Color::RGB(18, 21, 26))?;
    outline(canvas, rect, if enabled { BORDER } else { DISABLED })?;
    if checked {
        canvas.set_draw_color(if enabled { ACCENT } else { DISABLED });
        canvas.draw_line(
            Point::new(rect.x + 3, rect.y + rect.height as i32 / 2),
            Point::new(rect.x + 7, rect.y + rect.height as i32 - 4),
        )?;
        canvas.draw_line(
            Point::new(rect.x + 7, rect.y + rect.height as i32 - 4),
            Point::new(rect.x + rect.width as i32 - 3, rect.y + 3),
        )?;
    }
    draw_text(
        canvas,
        rect.x + rect.width as i32 + 7,
        rect.y + 4,
        label,
        if enabled { TEXT } else { DISABLED },
        1,
    )
}

pub fn draw_text(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    text: &str,
    color: Color,
    scale: u32,
) -> Result<(), String> {
    canvas.set_draw_color(color);
    let scale = scale.max(1);
    let mut cursor = x;
    for character in text.chars() {
        let glyph = glyph(character.to_ascii_uppercase());
        for (row, bits) in glyph.into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    canvas.fill_rect(Rect::new(
                        cursor + column * scale as i32,
                        y + row as i32 * scale as i32,
                        scale,
                        scale,
                    ))?;
                }
            }
        }
        cursor += 6 * scale as i32;
    }
    Ok(())
}

pub fn text_width(text: &str, scale: u32) -> u32 {
    text.chars().count() as u32 * 6 * scale.max(1)
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0x1f],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '\\' => [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01],
        '.' => [0, 0, 0, 0, 0, 0x06, 0x06],
        ',' => [0, 0, 0, 0, 0x06, 0x04, 0x08],
        '@' => [0x0e, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0e],
        ':' => [0, 0x06, 0x06, 0, 0x06, 0x06, 0],
        '#' => [0x0a, 0x1f, 0x0a, 0x0a, 0x1f, 0x0a, 0],
        '+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '[' => [0x0e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0e],
        ']' => [0x0e, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0e],
        '<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        '=' => [0, 0, 0x1f, 0, 0x1f, 0, 0],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        ' ' => [0; 7],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_reset_has_no_action() {
        let layout = UiLayout::new(1_280, 800);
        let x = layout.reset_position.x + 2;
        let y = layout.reset_position.y + 2;
        assert_eq!(layout.action_at(Tab::Models, false, false, x, y), None);
        assert_eq!(
            layout.action_at(Tab::Models, true, false, x, y),
            Some(UiAction::ResetPosition)
        );
    }

    #[test]
    fn mode_specific_controls_do_not_overlap_actions() {
        let layout = UiLayout::new(1_280, 800);
        let x = layout.sprite_variant.x + 2;
        let y = layout.sprite_variant.y + 2;
        assert_eq!(layout.action_at(Tab::Models, true, false, x, y), None);
        assert_eq!(
            layout.action_at(Tab::Sprites, false, false, x, y),
            Some(UiAction::CycleSpriteVariant)
        );
    }

    #[test]
    fn model_position_field_is_clickable_only_on_models_tab() {
        let layout = UiLayout::new(1_280, 800);
        let x = layout.model_position.x + 2;
        let y = layout.model_position.y + 2;

        assert_eq!(
            layout.action_at(Tab::Models, false, false, x, y),
            Some(UiAction::FocusModelPosition)
        );
        assert_eq!(layout.action_at(Tab::Sprites, false, false, x, y), None);
        assert_eq!(layout.action_at(Tab::Terrain, false, false, x, y), None);
        assert!(!layout.search.contains(x, y));
    }

    #[test]
    fn model_variant_button_requires_a_real_variant_choice() {
        let layout = UiLayout::new(1_280, 800);
        let x = layout.model_variant.x + 2;
        let y = layout.model_variant.y + 2;

        assert_eq!(layout.action_at(Tab::Models, false, false, x, y), None);
        assert_eq!(
            layout.action_at(Tab::Models, false, true, x, y),
            Some(UiAction::CycleModelVariant)
        );
        assert_eq!(
            layout.action_at(Tab::Sprites, false, true, x, y),
            Some(UiAction::CycleSpriteVariant)
        );
    }

    #[test]
    fn model_surface_controls_are_clickable_only_on_models_tab() {
        let layout = UiLayout::new(1_280, 800);
        for (rect, action) in [
            (layout.texture_checkbox, UiAction::ToggleModelTextures),
            (
                layout.model_texture_difficulty,
                UiAction::CycleModelTextureDifficulty,
            ),
            (layout.model_texture_mode, UiAction::CycleModelTextureMode),
            (layout.model_quad_split, UiAction::CycleModelQuadSplit),
        ] {
            let x = rect.x + 2;
            let y = rect.y + 2;
            assert_eq!(
                layout.action_at(Tab::Models, false, false, x, y),
                Some(action)
            );
            assert_eq!(layout.action_at(Tab::Sprites, false, false, x, y), None);
            assert_eq!(layout.action_at(Tab::Terrain, false, false, x, y), None);
        }
    }

    #[test]
    fn presentation_control_is_a_models_mode_not_a_tab() {
        let layout = UiLayout::new(1_280, 800);
        let x = layout.model_presentation.x + 2;
        let y = layout.model_presentation.y + 2;
        assert_eq!(
            layout.action_at(Tab::Models, false, false, x, y),
            Some(UiAction::CycleModelPresentation)
        );
        assert_eq!(layout.action_at(Tab::Sprites, false, false, x, y), None);
    }

    #[test]
    fn all_three_tabs_have_distinct_mouse_actions() {
        let layout = UiLayout::new(1_280, 800);
        for (rect, tab) in [
            (layout.model_tab, Tab::Models),
            (layout.sprite_tab, Tab::Sprites),
            (layout.terrain_tab, Tab::Terrain),
        ] {
            assert_eq!(
                layout.action_at(Tab::Models, false, false, rect.x + 2, rect.y + 2),
                Some(UiAction::SetTab(tab))
            );
        }
    }
}
