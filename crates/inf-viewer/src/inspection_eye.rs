//! Explicit diagnostic eye controls, independent of the orthographic orbit.

use std::fmt;
use std::str::FromStr;

use crate::ui::{UiAction, UiLayout, UiRect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionEye(pub [i32; 3]);

impl FromStr for InspectionEye {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let values = text
            .split(',')
            .map(|part| part.trim().parse::<i32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Eye must contain three signed integers: X,Y,Z".to_owned())?;
        let values = values
            .try_into()
            .map_err(|_| "Eye must contain exactly three coordinates: X,Y,Z".to_owned())?;
        Ok(Self(values))
    }
}

impl fmt::Display for InspectionEye {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, y, z] = self.0;
        write!(f, "{x},{y},{z}")
    }
}

impl InspectionEye {
    pub fn adjust(&mut self, axis: usize, delta: i32) {
        if let Some(value) = self.0.get_mut(axis) {
            *value = value.saturating_add(delta);
        }
    }
}

pub(crate) struct EyeControls {
    pub edit: UiRect,
    pub adjustments: [UiRect; 6],
    pub shift: UiRect,
}

impl EyeControls {
    pub fn new(layout: &UiLayout) -> Self {
        let x = layout.viewport.x + 14;
        let y = layout.viewport.y + layout.viewport.height as i32 - 86;
        Self {
            edit: UiRect::new(x, y, layout.viewport.width.saturating_sub(28), 26),
            adjustments: std::array::from_fn(|index| {
                UiRect::new(x + index as i32 * 45, y + 31, 40, 26)
            }),
            shift: UiRect::new(x + 276, y + 31, 110, 26),
        }
    }

    pub fn action_at(&self, x: i32, y: i32) -> Option<UiAction> {
        if self.edit.contains(x, y) {
            return Some(UiAction::FocusInspectionEye);
        }
        for (index, rect) in self.adjustments.iter().enumerate() {
            if rect.contains(x, y) {
                return Some(UiAction::AdjustInspectionEye {
                    axis: index / 2,
                    delta: if index % 2 == 0 { -1024 } else { 1024 },
                });
            }
        }
        self.shift
            .contains(x, y)
            .then_some(UiAction::CycleInspectionShift)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_parser_is_exact_and_rejects_partial_or_noninteger_positions() {
        assert_eq!(
            "-128, 256,0".parse::<InspectionEye>().unwrap().0,
            [-128, 256, 0]
        );
        for bad in ["", "1,2", "1,2,3,4", "1.5,2,3", "2147483648,0,0"] {
            assert!(bad.parse::<InspectionEye>().is_err(), "{bad}");
        }
        let mut eye = InspectionEye([i32::MAX, i32::MIN, 0]);
        eye.adjust(0, 1024);
        eye.adjust(1, -1024);
        eye.adjust(2, -1024);
        assert_eq!(eye.0, [i32::MAX, i32::MIN, -1024]);
    }

    #[test]
    fn controls_fit_the_minimum_viewport_and_keep_axes_separate() {
        let layout = UiLayout::new(760, 520);
        let controls = EyeControls::new(&layout);
        for (index, rect) in controls.adjustments.iter().enumerate() {
            assert_eq!(
                controls.action_at(rect.x + 1, rect.y + 1),
                Some(UiAction::AdjustInspectionEye {
                    axis: index / 2,
                    delta: if index % 2 == 0 { -1024 } else { 1024 },
                })
            );
        }
        assert!(controls.shift.x + (controls.shift.width as i32) < layout.sidebar.x);
        assert!(controls.edit.y > layout.viewport.y + 100);
    }
}
