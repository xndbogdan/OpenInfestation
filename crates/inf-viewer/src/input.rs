//! Input state machines without an SDL dependency in their timing or gesture logic.

use std::time::{Duration, Instant};

/// Delay before a held navigation key starts advancing through the catalog.
pub const NAVIGATION_REPEAT_DELAY: Duration = Duration::from_millis(300);

/// Catalog navigation cadence after the initial hold delay.
pub const NAVIGATION_REPEAT_INTERVAL: Duration = Duration::from_millis(65);

/// Prevent a long render or debugger pause from causing an unbounded burst.
const MAX_REPEAT_STEPS_PER_TICK: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationDirection {
    Previous,
    Next,
}

impl NavigationDirection {
    pub const fn delta(self) -> isize {
        match self {
            Self::Previous => -1,
            Self::Next => 1,
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::Previous => Self::Next,
            Self::Next => Self::Previous,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationSteps {
    pub direction: NavigationDirection,
    pub count: usize,
}

/// Tracks held previous/next keys using a clock-driven repeat schedule.
///
/// `press` reports whether the caller should perform the immediate first step.
/// Subsequent steps come from `take_due`, independent of SDL/OS key-repeat
/// settings and render frame count.
#[derive(Clone, Copy, Debug, Default)]
pub struct NavigationRepeat {
    previous_down: bool,
    next_down: bool,
    active: Option<NavigationDirection>,
    next_repeat_at: Option<Instant>,
}

impl NavigationRepeat {
    pub fn press(&mut self, direction: NavigationDirection, now: Instant) -> bool {
        let already_down = self.is_down(direction);
        self.set_down(direction, true);
        if already_down {
            return false;
        }

        self.active = Some(direction);
        self.next_repeat_at = Some(now + NAVIGATION_REPEAT_DELAY);
        true
    }

    pub fn release(&mut self, direction: NavigationDirection, now: Instant) {
        self.set_down(direction, false);
        if self.active != Some(direction) {
            return;
        }

        let resumed = direction.opposite();
        if self.is_down(resumed) {
            self.active = Some(resumed);
            self.next_repeat_at = Some(now + NAVIGATION_REPEAT_DELAY);
        } else {
            self.active = None;
            self.next_repeat_at = None;
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn take_due(&mut self, now: Instant) -> Option<NavigationSteps> {
        let direction = self.active?;
        let next_repeat_at = self.next_repeat_at?;
        if now < next_repeat_at {
            return None;
        }

        let elapsed_intervals =
            now.duration_since(next_repeat_at).as_nanos() / NAVIGATION_REPEAT_INTERVAL.as_nanos();
        let requested = 1usize.saturating_add(
            usize::try_from(elapsed_intervals).unwrap_or(MAX_REPEAT_STEPS_PER_TICK),
        );
        let count = requested.min(MAX_REPEAT_STEPS_PER_TICK);

        self.next_repeat_at = if requested > MAX_REPEAT_STEPS_PER_TICK {
            Some(now + NAVIGATION_REPEAT_INTERVAL)
        } else {
            Some(next_repeat_at + NAVIGATION_REPEAT_INTERVAL * count as u32)
        };

        Some(NavigationSteps { direction, count })
    }

    fn is_down(&self, direction: NavigationDirection) -> bool {
        match direction {
            NavigationDirection::Previous => self.previous_down,
            NavigationDirection::Next => self.next_down,
        }
    }

    fn set_down(&mut self, direction: NavigationDirection, down: bool) {
        match direction {
            NavigationDirection::Previous => self.previous_down = down,
            NavigationDirection::Next => self.next_down = down,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerInput {
    right_down: bool,
    middle_down: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    None,
    Orbit { dx: i32, dy: i32 },
    Pan { dx: i32, dy: i32 },
}

impl PointerInput {
    pub fn set_right(&mut self, down: bool) {
        self.right_down = down;
    }

    pub fn set_middle(&mut self, down: bool) {
        self.middle_down = down;
    }

    pub fn motion(&self, dx: i32, dy: i32) -> PointerAction {
        if self.middle_down {
            PointerAction::Pan { dx, dy }
        } else if self.right_down {
            PointerAction::Orbit { dx, dy }
        } else {
            PointerAction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestures_require_their_button_and_middle_takes_precedence() {
        let mut input = PointerInput::default();
        assert_eq!(input.motion(3, 4), PointerAction::None);
        input.set_right(true);
        assert_eq!(input.motion(3, 4), PointerAction::Orbit { dx: 3, dy: 4 });
        input.set_middle(true);
        assert_eq!(input.motion(3, 4), PointerAction::Pan { dx: 3, dy: 4 });
        input.set_middle(false);
        input.set_right(false);
        assert_eq!(input.motion(3, 4), PointerAction::None);
    }

    #[test]
    fn navigation_press_steps_immediately_then_waits_for_delay() {
        let start = Instant::now();
        let mut input = NavigationRepeat::default();

        assert!(input.press(NavigationDirection::Next, start));
        assert!(!input.press(NavigationDirection::Next, start));
        assert_eq!(
            input.take_due(start + NAVIGATION_REPEAT_DELAY - Duration::from_millis(1)),
            None
        );
        assert_eq!(
            input.take_due(start + NAVIGATION_REPEAT_DELAY),
            Some(NavigationSteps {
                direction: NavigationDirection::Next,
                count: 1,
            })
        );
    }

    #[test]
    fn repeat_count_is_elapsed_time_based_instead_of_frame_based() {
        let start = Instant::now();
        let final_tick = start + NAVIGATION_REPEAT_DELAY + NAVIGATION_REPEAT_INTERVAL * 3;
        let mut sampled = NavigationRepeat::default();
        let mut batched = NavigationRepeat::default();
        sampled.press(NavigationDirection::Previous, start);
        batched.press(NavigationDirection::Previous, start);

        let sampled_count = (0..=3)
            .map(|interval| {
                sampled
                    .take_due(
                        start + NAVIGATION_REPEAT_DELAY + NAVIGATION_REPEAT_INTERVAL * interval,
                    )
                    .map_or(0, |steps| steps.count)
            })
            .sum::<usize>();
        let batched_steps = batched.take_due(final_tick).unwrap();

        assert_eq!(sampled_count, 4);
        assert_eq!(batched_steps.count, sampled_count);
        assert_eq!(batched_steps.direction, NavigationDirection::Previous);
    }

    #[test]
    fn release_cancels_repeat_and_focus_style_clear_forgets_all_keys() {
        let start = Instant::now();
        let mut input = NavigationRepeat::default();
        input.press(NavigationDirection::Previous, start);
        input.release(
            NavigationDirection::Previous,
            start + Duration::from_millis(20),
        );
        assert_eq!(input.take_due(start + NAVIGATION_REPEAT_DELAY * 2), None);

        input.press(NavigationDirection::Next, start);
        input.clear();
        assert_eq!(input.take_due(start + NAVIGATION_REPEAT_DELAY * 2), None);
    }

    #[test]
    fn long_hitch_is_capped_and_repeat_schedule_resynchronizes() {
        let start = Instant::now();
        let mut input = NavigationRepeat::default();
        input.press(NavigationDirection::Next, start);
        let hitch_end = start + NAVIGATION_REPEAT_DELAY + NAVIGATION_REPEAT_INTERVAL * 100;

        assert_eq!(
            input.take_due(hitch_end),
            Some(NavigationSteps {
                direction: NavigationDirection::Next,
                count: MAX_REPEAT_STEPS_PER_TICK,
            })
        );
        assert_eq!(input.take_due(hitch_end), None);
        assert_eq!(
            input.take_due(hitch_end + NAVIGATION_REPEAT_INTERVAL),
            Some(NavigationSteps {
                direction: NavigationDirection::Next,
                count: 1,
            })
        );
    }

    #[test]
    fn most_recent_direction_resumes_the_other_held_key_after_release() {
        let start = Instant::now();
        let mut input = NavigationRepeat::default();
        assert!(input.press(NavigationDirection::Previous, start));
        assert!(input.press(NavigationDirection::Next, start + Duration::from_millis(10)));
        input.release(NavigationDirection::Next, start + Duration::from_millis(20));

        assert_eq!(
            input.take_due(start + Duration::from_millis(20) + NAVIGATION_REPEAT_DELAY),
            Some(NavigationSteps {
                direction: NavigationDirection::Previous,
                count: 1,
            })
        );
    }
}
