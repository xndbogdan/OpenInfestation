//! Retained process WORD005483C0, published by 004508E0 on each world load.
//!
//! Loader-zero proof applies once at process startup. Later loads borrow this
//! same WORD and retain reached reset/max writes on failure; there is no implicit
//! per-world reset, snapshot, allocation-history seed, or collision-query owner.

use inf_formats::{FrontendExecutable, FrontendExecutableError};

#[derive(Debug)]
pub struct ProcessFurnitureRadius {
    word: u16,
}

impl ProcessFurnitureRadius {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        executable.loader_zero_data(0x0054_83c0, 2)?;
        Ok(Self { word: 0 })
    }

    pub fn word(&self) -> u16 {
        self.word
    }

    pub(crate) fn word_mut(&mut self) -> &mut u16 {
        &mut self.word
    }
}
