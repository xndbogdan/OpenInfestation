//! The animated panel object 0040D6A0 (vtables 00520384, then 00520378):
//! the five briefing panels and the player message's line and hint boxes.
//! `004554E0` steps its stage toward the requested one, one stage per
//! duration, and eases its scale WORD toward the requested scale.

/// Fields consumed by `004554E0`, preserving the native three-stage policy.
/// A draw uses both stages and the countdown when growing or shrinking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelAnimation {
    pub current_stage: u8,
    pub requested_stage: u8,
    pub remaining_micros: i32,
    pub duration_micros: i32,
    pub current_scale_word: u16,
    pub requested_scale_word: u16,
}

impl PanelAnimation {
    /// The briefing's panels: a 500 ms stage at the given scale.
    pub(crate) fn new(scale: u16) -> Self {
        Self::with_duration(scale, 500_000)
    }

    pub(crate) fn with_duration(scale: u16, duration_micros: i32) -> Self {
        Self {
            current_stage: 0,
            requested_stage: 0,
            remaining_micros: 0,
            duration_micros,
            current_scale_word: scale,
            requested_scale_word: scale,
        }
    }

    /// Refills only an exhausted timer, as each native request does.
    pub(crate) fn request(&mut self, stage: u8) {
        if self.remaining_micros == 0 {
            self.remaining_micros = self.duration_micros;
        }
        self.requested_stage = stage;
    }

    /// `004554E0`.
    pub(crate) fn tick(&mut self, delta_micros: i32) {
        if delta_micros < self.remaining_micros {
            self.remaining_micros = self.remaining_micros.wrapping_sub(delta_micros);
        } else if self.current_stage == self.requested_stage {
            self.remaining_micros = 0;
        } else {
            if self.current_stage < self.requested_stage {
                self.current_stage = self.current_stage.wrapping_add(1);
            } else {
                self.current_stage = self.current_stage.wrapping_sub(1);
            }
            if self.current_stage != self.requested_stage {
                self.remaining_micros = self.duration_micros;
            }
        }
        if self.current_scale_word != self.requested_scale_word {
            let difference = u32::from(self.requested_scale_word)
                .wrapping_sub(u32::from(self.current_scale_word));
            let amount = difference.wrapping_mul((delta_micros as u32) >> 2) as i32 >> 15;
            self.current_scale_word = self.current_scale_word.wrapping_add(amount as u16);
        }
    }
}
