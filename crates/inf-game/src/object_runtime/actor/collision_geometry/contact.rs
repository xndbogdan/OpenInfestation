//! The five DWORD fields in the native 20-byte contact result.

/// Every DWORD retains its exact bits or remains unwritten. Native rejection
/// does not initialize any field, and a flag alone does not own its normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionContact {
    words: [Option<u32>; 5],
}

impl CollisionContact {
    /// Fresh allocation custody without invented flags, normal or displacement.
    pub const fn unwritten() -> Self {
        Self { words: [None; 5] }
    }

    /// Actual caller writes or explicitly supplied native allocation state.
    /// Offsets are +00 flag, +04/+08/+0C f32 normal bits, +10 displacement.
    pub const fn from_native_words(words: [Option<u32>; 5]) -> Self {
        Self { words }
    }

    pub const fn native_words(self) -> [Option<u32>; 5] {
        self.words
    }

    pub const fn flag(self) -> Option<u32> {
        self.words[0]
    }

    pub const fn normal_bits(self) -> [Option<u32>; 3] {
        [self.words[1], self.words[2], self.words[3]]
    }

    /// Signed interpretation belongs to the reached +10 consumer; storage
    /// itself preserves the entire DWORD, including native wrapping results.
    pub fn displacement(self) -> Option<i32> {
        self.words[4].map(|word| word as i32)
    }

    pub(super) fn write_flag(&mut self, flag: u32) {
        self.words[0] = Some(flag);
    }

    pub(super) fn write_normal(&mut self, component: usize, normal: f32) {
        self.words[component + 1] = Some(normal.to_bits());
    }

    pub(super) fn write_displacement(&mut self, displacement: i32) {
        self.words[4] = Some(displacement as u32);
    }
}
