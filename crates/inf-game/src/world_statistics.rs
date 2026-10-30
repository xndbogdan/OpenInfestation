//! Mission statistics W+0C, aliased by world G+1BC.
//!
//! Constructor 0048D9F0 allocates0x158 bytes and 00480FC0/00480FD0 clear
//! them. The loader clears the same allocation after player setup. This view
//! owns the recovered damage/retirement matrices and selector18 counter; other
//! bytes stay in the retained mission record for their separate consumers.

use crate::world_mission::RECORD_0C_BYTES;

/// Five-way classification returned by 0047F710/0047F750. These +B4 values
/// are runtime classification IDs, independent from constructor selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ActorStatisticCategory {
    Flag10 = 0,
    B4Eighteen = 1,
    OtherB4 = 2,
    B4One = 3,
    ///0047F710 returns4 for +B4 zero;0047F750 also returns4 when a source
    /// handle does not resolve. A null source never borrows the target category.
    B4ZeroOrUnresolved = 4,
}

impl ActorStatisticCategory {
    ///0047F710 tests flags bit10 before any +B4 comparison.
    pub const fn from_actor(flags: u32, classification_b4: u32) -> Self {
        if flags & 0x10 != 0 {
            Self::Flag10
        } else {
            match classification_b4 {
                0 => Self::B4ZeroOrUnresolved,
                1 => Self::B4One,
                18 => Self::B4Eighteen,
                _ => Self::OtherB4,
            }
        }
    }
}

/// Native 00481010 arguments: target category first, source category second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRetirementAttribution {
    pub target: ActorStatisticCategory,
    pub source: ActorStatisticCategory,
}

impl ActorRetirementAttribution {
    ///00481010 DWORD index `(target +7)*5 +source`, offsets8C..EC.
    pub const fn byte_offset(self) -> usize {
        ((self.target as usize + 7) * 5 + self.source as usize) * 4
    }
}

/// Native 00480FE0 arguments: signed damage amount, target category, source
/// category. The same 0047F710/0047F750 categories also index retirements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDamageStatistic {
    pub amount: i32,
    pub categories: ActorRetirementAttribution,
}

impl ActorDamageStatistic {
    /// 00480FE0 DWORD index `(target + 12)*5 + source`, offsets F0..150.
    pub const fn byte_offset(self) -> usize {
        damage_byte_offset(self.categories)
    }
}

const fn damage_byte_offset(categories: ActorRetirementAttribution) -> usize {
    ((categories.target as usize + 12) * 5 + categories.source as usize) * 4
}

/// A mutable view of the existing W+0C allocation, not a second statistics
/// copy. Its borrow also identifies the owner captured before attribution.
pub struct WorldStatistics<'a> {
    record: &'a mut [u8; RECORD_0C_BYTES],
}

impl<'a> WorldStatistics<'a> {
    pub fn from_record(record: &'a mut [u8; RECORD_0C_BYTES]) -> Self {
        Self { record }
    }

    pub fn attribution_count(&self, attribution: ActorRetirementAttribution) -> u32 {
        self.read_word(attribution.byte_offset())
    }

    ///00481010 increments one DWORD with x86 wrapping; it neither clears
    /// other counters nor applies the extra selector18 increment itself.
    pub fn record_attribution(&mut self, attribution: ActorRetirementAttribution) {
        self.increment(attribution.byte_offset());
    }

    pub fn damage_total(&self, categories: ActorRetirementAttribution) -> u32 {
        self.read_word(damage_byte_offset(categories))
    }

    ///00481050 counts a completed projectile actor-impact call, independently
    ///of adjusted damage. Target-major matrix DWORDs occupy28..88.
    pub fn record_particle_hit(&mut self, categories: ActorRetirementAttribution) {
        self.increment(((categories.target as usize + 2) * 5 + categories.source as usize) * 4);
    }

    pub fn particle_hit_count(&self, categories: ActorRetirementAttribution) -> u32 {
        self.read_word(((categories.target as usize + 2) * 5 + categories.source as usize) * 4)
    }

    /// 00481030 increments the source actor's category after a firing particle
    /// factory returns. The emitter captures this mission owner before calling
    /// the factory and classifies the source actor freshly after the call.
    pub fn record_particle_firing(&mut self, category: ActorStatisticCategory) {
        self.increment(0x14 + category as usize * 4);
    }

    pub fn particle_firing_count(&self, category: ActorStatisticCategory) -> u32 {
        self.read_word(0x14 + category as usize * 4)
    }

    /// The separate actor-resource arm reaches 00481040 before construction.
    pub fn record_actor_firing(&mut self, category: ActorStatisticCategory) {
        self.increment(category as usize * 4);
    }

    /// 00480FE0 adds the signed amount's DWORD bit pattern with wrapping.
    /// Negative damage can decrease a total or wrap it through zero; the
    /// consumer neither clamps it nor substitutes a retirement increment.
    pub fn record_damage(&mut self, statistic: ActorDamageStatistic) {
        let offset = statistic.byte_offset();
        let value = self.read_word(offset).wrapping_add(statistic.amount as u32);
        self.record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub fn selector18_retirements(&self) -> u32 {
        self.read_word(0x154)
    }

    ///0047D6E5 increments captured owner's +154 only after rereading target
    /// +B4 and finding18. Call-site ordering belongs to 0047D670.
    pub fn increment_selector18_retirements(&mut self) {
        self.increment(0x154);
    }

    fn read_word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.record[offset..offset + 4].try_into().unwrap())
    }

    fn increment(&mut self, offset: usize) {
        let value = self.read_word(offset).wrapping_add(1);
        self.record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
#[path = "world_statistics_tests.rs"]
mod tests;
