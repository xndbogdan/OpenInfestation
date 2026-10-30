//! Executable singleton identity and callback selection for +D8 wrappers.
//! CRT initializer leaves write these distinct vtables before actor use.
//! Pointer-table entries are four bytes; periodic records are a separate owner.

use super::FamilyBehaviorProfile;

/// Addresses identify retail singleton objects and their original callbacks;
/// none is a portable arena pointer or a captured runtime allocation address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyBehaviorDescriptor {
    pub object_address: u32,
    pub vtable_address: u32,
    pub score_address: u32,
    pub change_address: u32,
    pub update_address: u32,
    /// Selector12's four-slot descriptor vtables have no duration method.
    /// Its supplied wrapper 004690F0 owns that policy independently.
    pub duration_address: Option<u32>,
}

impl FamilyBehaviorDescriptor {
    const fn new(object: u32, vtable: u32, callbacks: [u32; 3], duration: Option<u32>) -> Self {
        Self {
            object_address: object,
            vtable_address: vtable,
            score_address: callbacks[0],
            change_address: callbacks[1],
            update_address: callbacks[2],
            duration_address: duration,
        }
    }
}

const fn ordinary(
    object: u32,
    vtable: u32,
    callbacks: [u32; 3],
    duration: u32,
) -> FamilyBehaviorDescriptor {
    FamilyBehaviorDescriptor::new(object, vtable, callbacks, Some(duration))
}

// Original initializer leaves 00423390..00423690. The first nine follow
// mode2's pointer order; mode0 selects the distinct tenth descriptor at index2.
const FAMILY: [FamilyBehaviorDescriptor; 10] = [
    ordinary(0x546a70, 0x520abc, [0x469370, 0x469380, 0x4693a0], 0x43fa00),
    ordinary(0x546a78, 0x520ad0, [0x469370, 0x43fe40, 0x435240], 0x43fa00),
    ordinary(0x546a60, 0x520ae4, [0x43fac0, 0x43fb00, 0x435130], 0x43fa00),
    ordinary(0x546a74, 0x520af8, [0x43fb20, 0x435150, 0x435170], 0x43fa00),
    ordinary(0x546a6c, 0x520b0c, [0x43fb60, 0x43fba0, 0x43fbc0], 0x43fc40),
    ordinary(0x546a80, 0x520b20, [0x425db0, 0x435190, 0x4026d0], 0x43fbe0),
    ordinary(0x546a48, 0x520b34, [0x4351b0, 0x43fc00, 0x43fc20], 0x43fc40),
    ordinary(0x546a7c, 0x520b48, [0x43fa40, 0x43fa80, 0x43faa0], 0x43fa00),
    ordinary(0x546a5c, 0x520b5c, [0x425d30, 0x425d70, 0x425d90], 0x43fa00),
    ordinary(0x546a64, 0x520b70, [0x425df0, 0x425e00, 0x425e20], 0x43fa00),
];

// Original initializer leaves 0043CC90..0043D010 and table00533D38.
const SELECTOR35: [FamilyBehaviorDescriptor; 12] = [
    ordinary(0x547644, 0x52264c, [0x469370, 0x469380, 0x4693a0], 0x43fa00),
    ordinary(0x547650, 0x522660, [0x469370, 0x43fe40, 0x435240], 0x43fa00),
    ordinary(0x547634, 0x522674, [0x43fac0, 0x43fb00, 0x435130], 0x43fa00),
    ordinary(0x54764c, 0x522688, [0x43fb20, 0x435150, 0x435170], 0x43fa00),
    ordinary(0x547640, 0x52269c, [0x43fb60, 0x43fba0, 0x43fbc0], 0x43fc40),
    ordinary(0x54765c, 0x5226b0, [0x425db0, 0x435190, 0x4026d0], 0x43fbe0),
    ordinary(0x547628, 0x5226c4, [0x4351b0, 0x43fc00, 0x43fc20], 0x43fc40),
    ordinary(0x547630, 0x5226d8, [0x43fc60, 0x43fca0, 0x43fcc0], 0x43fa00),
    ordinary(0x547658, 0x5226ec, [0x43fa40, 0x43fa80, 0x43faa0], 0x43fa00),
    ordinary(0x54763c, 0x522700, [0x43fce0, 0x435220, 0x43fd20], 0x43fa00),
    ordinary(0x547648, 0x522714, [0x43fd40, 0x43fd80, 0x43fda0], 0x43fa00),
    ordinary(0x547654, 0x522728, [0x43fdc0, 0x43fe00, 0x43fe20], 0x43fa00),
];

// Original initializer leaves 00433880..00433AE0 and table005329F8. Indices
// 0-6 share Selector35's callbacks; index7 scores through 004351E0 (+23C+0E).
const SELECTOR34: [FamilyBehaviorDescriptor; 8] = [
    ordinary(0x546b38, 0x521e0c, [0x469370, 0x469380, 0x4693a0], 0x43fa00),
    ordinary(0x546b40, 0x521e20, [0x469370, 0x43fe40, 0x435240], 0x43fa00),
    ordinary(0x546b2c, 0x521e34, [0x43fac0, 0x43fb00, 0x435130], 0x43fa00),
    ordinary(0x546b3c, 0x521e48, [0x43fb20, 0x435150, 0x435170], 0x43fa00),
    ordinary(0x546b34, 0x521e5c, [0x43fb60, 0x43fba0, 0x43fbc0], 0x43fc40),
    ordinary(0x546b44, 0x521e70, [0x425db0, 0x435190, 0x4026d0], 0x43fbe0),
    ordinary(0x546b28, 0x521e84, [0x4351b0, 0x43fc00, 0x43fc20], 0x43fc40),
    ordinary(0x546b30, 0x521e98, [0x4351e0, 0x435220, 0x43fd20], 0x43fa00),
];

// Original initializer leaves 00466D80..00467020. Present+140 adds the
// ninth singleton at index4; it does not replace later objects or their vtables.
const SELECTOR12: [FamilyBehaviorDescriptor; 9] = [
    FamilyBehaviorDescriptor::new(0x54ecd0, 0x526fc8, [0x469370, 0x469380, 0x4693a0], None),
    FamilyBehaviorDescriptor::new(0x54ecd8, 0x526fd8, [0x469370, 0x43fe40, 0x435240], None),
    FamilyBehaviorDescriptor::new(0x54ecc0, 0x526fe8, [0x469130, 0x469170, 0x435170], None),
    FamilyBehaviorDescriptor::new(0x54ecd4, 0x526ff8, [0x43fb20, 0x4691f0, 0x469210], None),
    FamilyBehaviorDescriptor::new(0x54ece0, 0x527008, [0x469190, 0x4691d0, 0x425d90], None),
    FamilyBehaviorDescriptor::new(0x54eccc, 0x527018, [0x469230, 0x43fba0, 0x43fbc0], None),
    FamilyBehaviorDescriptor::new(0x54ecdc, 0x527028, [0x469270, 0x435190, 0x4026d0], None),
    FamilyBehaviorDescriptor::new(0x54ecc8, 0x527038, [0x4692b0, 0x4692f0, 0x469310], None),
    FamilyBehaviorDescriptor::new(0x54ecc4, 0x527048, [0x469330, 0x43fb00, 0x435130], None),
];

impl FamilyBehaviorProfile {
    /// Resolve the actual four-byte pointer-table entry and its CRT-initialized
    /// singleton vtable. Unsupported mode or index does not alias another entry.
    pub fn descriptor(self, index: i32) -> Option<FamilyBehaviorDescriptor> {
        let (_, count) = self.descriptor_table()?;
        if index < 0 || index >= count {
            return None;
        }
        let index = index as usize;
        Some(match self {
            Self::Family4236d0 { mode: 0 } if index == 2 => FAMILY[9],
            Self::Family4236d0 { .. } => FAMILY[index],
            Self::Selector35 => SELECTOR35[index],
            Self::Selector34 => SELECTOR34[index],
            Self::Selector12 {
                behavior_present: false,
            } => SELECTOR12[index],
            Self::Selector12 {
                behavior_present: true,
            } => {
                SELECTOR12[match index {
                    4 => 8,
                    5.. => index - 1,
                    _ => index,
                }]
            }
        })
    }

    pub(super) fn duration_kernel(self, index: i32) -> Option<(u32, u32)> {
        if matches!(self, Self::Selector12 { .. }) {
            // Wrapper 004690F0 ignores index, including out-of-table values.
            return Some((125_000, 5));
        }
        if index < 0 {
            // 0043F9E6 never dereferences the descriptor pointer table.
            return Some((46_875, 6));
        }
        match self.descriptor(index)?.duration_address? {
            0x0043_fa00 => Some((46_875, 6)),
            0x0043_fbe0 => Some((93_750, 4)),
            0x0043_fc40 => Some((31_250, 4)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "behavior_descriptors/initializer_tests.rs"]
mod initializer_tests;
