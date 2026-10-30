//! Base screen input at 004392B0/00439430/00439470 and frontend 00484A80.
//!
//! This owns the screen's held mask, repeat countdown, age, and analog latch.
//! It consumes already-published input values; device translation and analog
//! binding/consumer lifetimes belong to the input owner. Dispatch is required
//! and occurs against live state, so one callback can suppress a later slot.

use crate::{input::InputCallback, scheduler::SchedulerTick};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenDispatchPolicy {
    /// 00439470: virtual slots +24 through +38, in that order.
    Base,
    /// 00484A80: the base pass followed by 004852A0/C0/B0.
    Frontend,
    /// 0042D460, the results screen's +20: its gate on +150 first, then the
    /// base pass and bits 0x40/0x80/0x100/0x200 (0042D3D0, 0042D440,
    /// 0042D3F0 and 0042D510).
    Results,
}

/// Bit identity is distinct from a menu command number or a device key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ScreenButton {
    Confirm = 0x01,
    Previous = 0x02,
    Next = 0x04,
    AdjustPrevious = 0x08,
    AdjustNext = 0x10,
    Back = 0x20,
    Auxiliary6 = 0x40,
    Auxiliary7 = 0x80,
    RequestAttract = 0x100,
    /// 0042AA30 writes this bit. Neither recovered dispatch policy consumes
    /// it, but a new press still dispatches other held bits and resets repeat.
    Bit200 = 0x200,
}

impl ScreenButton {
    pub const fn mask(self) -> u32 {
        self as u32
    }

    /// Direct screen-context callbacks only. 00443490..00443590 first resolve
    /// a different context to a screen and must be handled by that owner.
    pub const fn from_callback(address: u32) -> Option<Self> {
        match address {
            0x0044_5200 => Some(Self::Confirm),
            0x0044_5220 => Some(Self::Previous),
            0x0044_5240 => Some(Self::Next),
            0x0044_5260 => Some(Self::AdjustPrevious),
            0x0044_5280 => Some(Self::AdjustNext),
            0x0044_51e0 => Some(Self::Back),
            0x0048_5300 => Some(Self::Auxiliary6),
            0x0042_aa00 => Some(Self::Auxiliary7),
            0x0042_a9d0 => Some(Self::RequestAttract),
            0x0042_aa30 => Some(Self::Bit200),
            _ => None,
        }
    }
}

/// Names describe the frontend implementation of the base virtual slots.
/// Other screen classes must provide their own slot implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenInputAction {
    Confirm,
    Previous,
    Next,
    AdjustPrevious,
    AdjustNext,
    Back,
    Auxiliary6,
    Auxiliary7,
    RequestAttract,
    /// 0042D465, the results pass's head: a set +150 clears +C4 and ends the
    /// pass before any slot.
    ResultsGate,
    /// Bit 0x200 (0042AA30), which only the results pass dispatches
    /// (0042D510).
    Bit200,
}

impl ScreenInputAction {
    pub const fn virtual_slot(self) -> Option<u8> {
        match self {
            Self::Confirm => Some(0x24),
            Self::Previous => Some(0x28),
            Self::Next => Some(0x2c),
            Self::AdjustPrevious => Some(0x30),
            Self::AdjustNext => Some(0x34),
            Self::Back => Some(0x38),
            Self::Auxiliary6
            | Self::Auxiliary7
            | Self::RequestAttract
            | Self::ResultsGate
            | Self::Bit200 => None,
        }
    }
}

/// Values returned by 004F0140 from the two consumer references at screen+CC
/// and +D0. These are signed native words, not normalized floats or OS axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenAnalogValues {
    pub consumer_cc: i32,
    pub consumer_d0: i32,
}

/// Recovered direction after the strict, prioritized 004392DF..0043937F
/// reads. A live consumer may select it lazily without inventing unused axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenAnalogDirection {
    Neutral,
    Previous,
    Next,
    AdjustPrevious,
    AdjustNext,
}

impl ScreenAnalogValues {
    fn direction(self) -> ScreenAnalogDirection {
        if self.consumer_d0 > 2500 {
            ScreenAnalogDirection::Previous
        } else if self.consumer_d0 < -2500 {
            ScreenAnalogDirection::Next
        } else if self.consumer_cc < -2500 {
            ScreenAnalogDirection::AdjustPrevious
        } else if self.consumer_cc > 2500 {
            ScreenAnalogDirection::AdjustNext
        } else {
            ScreenAnalogDirection::Neutral
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScreenInputError<E> {
    #[error("unknown direct screen input callback {0:#010x}")]
    UnknownCallback(u32),
    #[error("screen input context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("screen input callback failed")]
    Dispatch(E),
    #[error("screen input stopped after a partial dispatch")]
    Stopped,
}

#[derive(Debug)]
pub struct ScreenInput {
    policy: ScreenDispatchPolicy,
    context: u32,
    age_millis: i32,
    held_mask: u32,
    repeat_millis: i32,
    analog_latched: bool,
    stopped: bool,
}

impl ScreenInput {
    /// 00437F90 initializes +C0/+C4/+C8/+D4 to zero. Analog references are
    /// constructed separately and their published values are supplied to tick.
    pub fn new(policy: ScreenDispatchPolicy, context: u32) -> Self {
        Self {
            policy,
            context,
            age_millis: 0,
            held_mask: 0,
            repeat_millis: 0,
            analog_latched: false,
            stopped: false,
        }
    }

    pub fn policy(&self) -> ScreenDispatchPolicy {
        self.policy
    }
    pub fn context(&self) -> u32 {
        self.context
    }
    pub fn age_millis(&self) -> i32 {
        self.age_millis
    }
    pub fn held_mask(&self) -> u32 {
        self.held_mask
    }
    pub fn repeat_millis(&self) -> i32 {
        self.repeat_millis
    }
    pub fn analog_latched(&self) -> bool {
        self.analog_latched
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// 00485110 clears only +C4, leaving countdown and the analog latch intact.
    pub fn clear_held(&mut self) {
        self.held_mask = 0;
    }

    /// 004497A3 preserves the high WORD of screen+C4 during profile rebuild.
    /// Countdown and analog latch remain untouched, as with the native AND.
    pub fn clear_held_low_word(&mut self) {
        self.held_mask &= 0xffff_0000;
    }

    /// Native zero-value digital callbacks clear only their one held bit.
    pub fn release(&mut self, button: ScreenButton) {
        self.held_mask &= !button.mask();
    }

    /// The native callbacks ignore their timestamp words and test value != 0.
    /// Validation failures do not mutate state. A failed required dispatcher
    /// stops this owner because completed callback effects cannot be replayed.
    pub fn digital_callback<E, F>(
        &mut self,
        event: InputCallback,
        dispatch: &mut F,
    ) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        self.require_running()?;
        if event.context != self.context {
            return Err(ScreenInputError::Context {
                expected: self.context,
                actual: event.context,
            });
        }
        let button = ScreenButton::from_callback(event.address)
            .ok_or(ScreenInputError::UnknownCallback(event.address))?;
        if event.value == 0 {
            self.release(button);
            Ok(())
        } else {
            self.press(button, dispatch)
        }
    }

    /// 00439430 publishes the bit before dispatch, and writes 250 only AFTER
    /// that dispatch returns. Clearing held input during a callback does not
    /// suppress this post-callback countdown write. Already-held bits do none
    /// of those extra effects.
    pub fn press<E, F>(
        &mut self,
        button: ScreenButton,
        dispatch: &mut F,
    ) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        self.require_running()?;
        let old = self.held_mask;
        self.held_mask |= button.mask();
        if old & button.mask() == 0 {
            self.dispatch_held(dispatch)?;
            self.repeat_millis = 250;
        }
        Ok(())
    }

    /// 004392B0 advances age from raw elapsed time, applies analog priority,
    /// then consumes capped time from the shared repeat countdown. Per-call
    /// signed /1000 discards fractional milliseconds; no separate accumulator
    /// and no catch-up loop exist. All native word arithmetic wraps.
    pub fn tick<E, F>(
        &mut self,
        tick: SchedulerTick,
        analog: ScreenAnalogValues,
        dispatch: &mut F,
    ) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        self.require_running()?;
        self.add_age_millis(tick.elapsed_micros / 1000);
        self.tick_after_age(tick, analog.direction(), dispatch)
    }

    /// Both 004392D1's raw-age addition and 0042D096's capped-age addition
    /// target the same screen+C0. Callers retain native order around sources.
    pub(crate) fn add_age_millis(&mut self, elapsed: i32) {
        self.age_millis = self.age_millis.wrapping_add(elapsed);
    }

    /// A screen's own store to +C0, as the results screen's 0042D24C.
    pub(crate) fn set_age_millis(&mut self, age: i32) {
        self.age_millis = age;
    }

    /// Complete analog/repeat phase after the native raw-age store. The
    /// results owner can reach a missing live consumer after that store.
    pub(crate) fn tick_after_age<E, F>(
        &mut self,
        tick: SchedulerTick,
        direction: ScreenAnalogDirection,
        dispatch: &mut F,
    ) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        self.require_running()?;
        // +D0 wins over +CC. Both strict thresholds are 2500, and only one
        // branch runs. Opposite-bit clearing occurs AFTER press dispatch, so
        // reversing the analog direction can briefly dispatch both directions.
        let direction = match direction {
            ScreenAnalogDirection::Previous => Some((ScreenButton::Previous, ScreenButton::Next)),
            ScreenAnalogDirection::Next => Some((ScreenButton::Next, ScreenButton::Previous)),
            ScreenAnalogDirection::AdjustPrevious => {
                Some((ScreenButton::AdjustPrevious, ScreenButton::AdjustNext))
            }
            ScreenAnalogDirection::AdjustNext => {
                Some((ScreenButton::AdjustNext, ScreenButton::AdjustPrevious))
            }
            ScreenAnalogDirection::Neutral => None,
        };
        if let Some((pressed, opposite)) = direction {
            self.press(pressed, dispatch)?;
            self.analog_latched = true;
            self.release(opposite);
        } else if self.analog_latched {
            self.analog_latched = false;
            // This also clears digital direction bits: there is no per-source
            // mask at the screen level. Confirm/back/extension bits survive.
            self.held_mask &= !0x1e;
        }
        let elapsed = tick.delta_micros / 1000;
        if elapsed < self.repeat_millis {
            self.repeat_millis = self.repeat_millis.wrapping_sub(elapsed);
            return Ok(());
        }
        self.repeat_millis = if self.held_mask != 0 {
            self.repeat_millis.wrapping_sub(elapsed).wrapping_add(100)
        } else {
            0
        };
        self.dispatch_held(dispatch)
    }

    fn require_running<E>(&self) -> Result<(), ScreenInputError<E>> {
        if self.stopped {
            Err(ScreenInputError::Stopped)
        } else {
            Ok(())
        }
    }

    fn dispatch_held<E, F>(&mut self, dispatch: &mut F) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        if self.policy == ScreenDispatchPolicy::Results {
            self.require_running()?;
            if let Err(error) = dispatch(self, ScreenInputAction::ResultsGate) {
                self.stopped = true;
                return Err(ScreenInputError::Dispatch(error));
            }
        }
        // 00439470 re-reads +C4 before every slot. Do not collect a snapshot.
        for (button, action) in [
            (ScreenButton::Confirm, ScreenInputAction::Confirm),
            (ScreenButton::Previous, ScreenInputAction::Previous),
            (ScreenButton::Next, ScreenInputAction::Next),
            (
                ScreenButton::AdjustPrevious,
                ScreenInputAction::AdjustPrevious,
            ),
            (ScreenButton::AdjustNext, ScreenInputAction::AdjustNext),
            (ScreenButton::Back, ScreenInputAction::Back),
        ] {
            self.dispatch_if_held(button, action, dispatch)?;
        }
        if self.policy != ScreenDispatchPolicy::Base {
            for (button, action) in [
                (ScreenButton::Auxiliary6, ScreenInputAction::Auxiliary6),
                (ScreenButton::Auxiliary7, ScreenInputAction::Auxiliary7),
                (
                    ScreenButton::RequestAttract,
                    ScreenInputAction::RequestAttract,
                ),
            ] {
                self.dispatch_if_held(button, action, dispatch)?;
            }
        }
        if self.policy == ScreenDispatchPolicy::Results {
            self.dispatch_if_held(ScreenButton::Bit200, ScreenInputAction::Bit200, dispatch)?;
        }
        Ok(())
    }

    fn dispatch_if_held<E, F>(
        &mut self,
        button: ScreenButton,
        action: ScreenInputAction,
        dispatch: &mut F,
    ) -> Result<(), ScreenInputError<E>>
    where
        F: FnMut(&mut Self, ScreenInputAction) -> Result<(), E>,
    {
        self.require_running()?;
        if self.held_mask & button.mask() != 0 {
            if let Err(error) = dispatch(self, action) {
                self.stopped = true;
                return Err(ScreenInputError::Dispatch(error));
            }
            // A nested failed dispatch may have been swallowed by the host.
            self.require_running()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "screen_input_tests.rs"]
mod tests;
