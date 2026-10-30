//! Supplied fixture manager state, independent of native process DWORDs.
//! Cloning is deliberately unavailable; hooks mutate this owner in place.
use std::cell::Cell;
#[derive(Debug)]
pub(crate) struct FixtureSessionPhase {
    value: Cell<Option<u32>>,
    reads: Cell<usize>,
}
impl FixtureSessionPhase {
    pub(crate) fn supplied(value: Option<u32>) -> Self {
        Self {
            value: Cell::new(value),
            reads: Cell::new(0),
        }
    }
    pub(crate) fn current(&self) -> Option<u32> {
        self.reads.set(self.reads.get() + 1);
        self.value.get()
    }
    pub(crate) fn snapshot(&self) -> Option<u32> {
        self.value.get()
    }
    pub(crate) fn set(&self, value: Option<u32>) {
        self.value.set(value)
    }
    pub(crate) fn reads(&self) -> usize {
        self.reads.get()
    }
}
impl Default for FixtureSessionPhase {
    fn default() -> Self {
        Self::supplied(Some(4))
    } // Existing explicit gameplay-fixture domain.
}
