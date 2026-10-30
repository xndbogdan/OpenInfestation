//! World 004875A0 timing-ring head; its load factor feeds particle admission.

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldTimingRing {
    pub samples: [u32; 8],
    pub sum: u32,
    pub average: u32,
    pub cursor: u32,
    pub elapsed_milliseconds: u32,
    pub particle_load_factor: u32,
    pub frames_per_second: Option<u32>,
}

impl Default for WorldTimingRing {
    fn default() -> Self {
        // Actual 00487BE0 constructor tail, +588/+58C..+5A8/+5AC/+5B0.
        Self {
            samples: [125_000; 8],
            sum: 1_000_000,
            average: 125_000,
            cursor: 0,
            elapsed_milliseconds: 0,
            particle_load_factor: 0,
            frames_per_second: None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("native 004875A0 timing head divides by zero after its reached ring writes")]
pub struct TimingDivisionByZero;

impl WorldTimingRing {
    /// Signed parameter comparison, unsigned ring average and signed /1000.
    /// FPS publication fails only after all preceding native writes.
    pub fn tick(
        &mut self,
        microseconds: u32,
        alternate_microseconds: u32,
    ) -> Result<(), TimingDivisionByZero> {
        if (microseconds as i32) < alternate_microseconds as i32 {
            self.samples.fill(alternate_microseconds);
            self.average = alternate_microseconds;
            self.sum = alternate_microseconds.wrapping_mul(8);
            self.particle_load_factor = 0;
        } else {
            let index = (self.cursor & 7) as usize;
            self.sum = self.sum.wrapping_sub(self.samples[index]);
            self.samples[index] = microseconds;
            self.sum = self.sum.wrapping_add(microseconds);
            self.average = self.sum >> 3;
            self.particle_load_factor = if self.average < 34_464 {
                65_536
            } else {
                100_000 - self.average.min(100_000)
            };
        }
        self.elapsed_milliseconds = self
            .elapsed_milliseconds
            .wrapping_add(((microseconds as i32) / 1000) as u32);
        self.cursor = self.cursor.wrapping_add(1);
        if self.average == 0 {
            return Err(TimingDivisionByZero);
        }
        self.frames_per_second = Some(1_000_000 / self.average);
        Ok(())
    }
}
