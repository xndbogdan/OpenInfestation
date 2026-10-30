//! Owned helper +26C selected by 004252B0. Construction/destruction and the
//! common445CD0 and walker 004476D0 callbacks retain actual child backing and
//! error order.
mod legs;
mod tick;
pub use legs::{
    segment_probe_43a380, selector10_custom_425870, SegmentProbe, Selector10CustomInputs,
    Selector10CustomServices, Selector10LegError, Selector10LegGround, WheelGround,
};
pub(crate) use tick::unit_48f080;
pub use tick::{Selector10HelperTickError, Selector10HelperTickRequest};

/// Exact constructor branches, including the extra DWORD present only in the
/// 0x38-byte helper, whose low WORD is the walker's stride phase (004476D0)
/// and whose high WORD nothing reads. Addresses name native families, not
/// executable host pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector10HelperClass {
    Family445ae0,
    Family446bc0 { phase_34: Option<u16> },
}

/// The extended 0x74-byte child changes its vtable after running 00445A70.
/// Its additional bytes are untouched, just like the base child's holes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector10HelperChildClass {
    Base445a70,
    Extended446bc0 { unknown_50_73: [Option<u8>; 36] },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector10HelperChild {
    pub vtable_00: u32,
    pub unknown_04_13: [Option<u8>; 16],
    pub word_14: u32,
    pub unknown_18_35: [Option<u8>; 30],
    pub word_36: u16,
    pub unknown_38_4f: [Option<u8>; 24],
    pub class: Selector10HelperChildClass,
}

impl Selector10HelperChild {
    fn new(class: Selector10HelperClass) -> Self {
        let (vtable_00, class) = match class {
            Selector10HelperClass::Family445ae0 => {
                (0x0052_4e88, Selector10HelperChildClass::Base445a70)
            }
            Selector10HelperClass::Family446bc0 { .. } => (
                0x0052_4eb8,
                Selector10HelperChildClass::Extended446bc0 {
                    unknown_50_73: [None; 36],
                },
            ),
        };
        Self {
            vtable_00,
            unknown_04_13: [None; 16],
            word_14: 0,
            unknown_18_35: [None; 30],
            word_36: 0,
            unknown_38_4f: [None; 24],
            class,
        }
    }

    /// Every byte no writer has reached takes `byte`: the fresh-heap
    /// policy's start for a mode-4 leg (a fresh 0x74-byte block).
    pub(in crate::object_runtime) fn fill_unknown(&mut self, byte: u8) {
        let extended = match &mut self.class {
            Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } => &mut unknown_50_73[..],
            Selector10HelperChildClass::Base445a70 => &mut [],
        };
        for slot in self
            .unknown_04_13
            .iter_mut()
            .chain(self.unknown_18_35.iter_mut())
            .chain(self.unknown_38_4f.iter_mut())
            .chain(extended.iter_mut())
        {
            slot.get_or_insert(byte);
        }
    }

    /// Native allocation extent, independent of Rust's explicit unknown state.
    pub fn allocation_bytes(&self) -> usize {
        match self.class {
            Selector10HelperChildClass::Base445a70 => 0x50,
            Selector10HelperChildClass::Extended446bc0 { .. } => 0x74,
        }
    }
}

/// 00445A90 initializes the common header and all eight pointer slots before
/// 00445AE0 or 00446BC0 allocates the descriptor's unsigned-WORD child count.
/// Only the five immutable executable descriptors selected by 004252B0 are
/// supported here; arbitrary descriptor pointers are not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector10Helper {
    pub vtable_00: u32,
    pub word_04: Option<u32>,
    pub word_08: u32,
    pub byte_0c: u8,
    pub byte_0d: u8,
    pub word_0e: Option<u16>,
    pub table_10: u32,
    pub children_14: [Option<Box<Selector10HelperChild>>; 8],
    pub class: Selector10HelperClass,
}

impl Selector10Helper {
    /// A positive-allocation result for 004252B0's exact selector switch.
    /// None means the switch has no allocation branch: callers must preserve
    /// an existing +26C. Native selected branches overwrite a previous +26C
    /// without freeing it; callers must handle that separate lifetime boundary.
    pub fn for_selector(selector: u32) -> Option<Box<Self>> {
        // Native descriptor WORDs at 005307F8/00530900/00530A08/00530B10/
        // 00530C18 are 6/4/4/2/2. These are executable data, not H13 fields.
        let (table_10, count, class) = match selector {
            0 => (0x0053_07f8, 6, Selector10HelperClass::Family445ae0),
            1 => (0x0053_0900, 4, Selector10HelperClass::Family445ae0),
            2 => (0x0053_0a08, 4, Selector10HelperClass::Family445ae0),
            3 => (0x0053_0b10, 2, Selector10HelperClass::Family445ae0),
            4 => (
                0x0053_0c18,
                2,
                Selector10HelperClass::Family446bc0 { phase_34: None },
            ),
            _ => return None,
        };
        Some(Box::new(Self {
            vtable_00: match class {
                Selector10HelperClass::Family445ae0 => 0x0052_4ea8,
                Selector10HelperClass::Family446bc0 { .. } => 0x0052_4ec8,
            },
            word_04: None,
            word_08: 0,
            byte_0c: 1,
            byte_0d: 0,
            word_0e: None,
            table_10,
            children_14: std::array::from_fn(|index| {
                (index < count).then(|| Box::new(Selector10HelperChild::new(class)))
            }),
            class,
        }))
    }

    /// Native allocation extent, independent of Rust's explicit unknown state.
    pub fn allocation_bytes(&self) -> usize {
        match self.class {
            Selector10HelperClass::Family445ae0 => 0x34,
            Selector10HelperClass::Family446bc0 { .. } => 0x38,
        }
    }
}

impl Drop for Selector10Helper {
    fn drop(&mut self) {
        // Both final vtables use deleting destructor 00446C60, which calls
        // 00445B90. It restores the common vtable and directly frees every
        // nonnull child in ascending slot order; no child virtual is invoked.
        // The surrounding Box is then released, matching deleting flag bit 0.
        self.vtable_00 = 0x0052_4e98;
        for child in &mut self.children_14 {
            drop(child.take());
        }
    }
}
