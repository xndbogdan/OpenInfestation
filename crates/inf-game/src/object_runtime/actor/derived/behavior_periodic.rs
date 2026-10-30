//! Periodic wrapper 00425C60. Its executable twelve-byte records keep callback,
//! byte offset within the actor and optional input factory separate. This
//! owns dispatch/lifetime policy, not collision or target-acquisition effects.

use super::{FamilyBehaviorError, FamilyBehaviorHost, FamilyBehaviorProfile};
use crate::object_runtime::{ObjectArena, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyPeriodicRecord {
    /// Executable callback identity, not a portable callable pointer.
    pub callback: u32,
    /// Native byte offset added to the actual actor address.
    pub actor_offset: u32,
    /// Executable factory identity. None represents the native null DWORD.
    pub input_factory: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyPeriodicProbeRequest {
    pub profile: FamilyBehaviorProfile,
    pub index: u16,
    pub record: FamilyPeriodicRecord,
}

const COMMON_RECORDS: [FamilyPeriodicRecord; 3] = [
    FamilyPeriodicRecord {
        callback: 0x0042_5d00,
        actor_offset: 0,
        input_factory: None,
    },
    FamilyPeriodicRecord {
        callback: 0x0042_5cf0,
        actor_offset: 0,
        input_factory: None,
    },
    FamilyPeriodicRecord {
        callback: 0x0042_7510,
        actor_offset: 0x1fc,
        input_factory: None,
    },
];

const SELECTOR12_RECORDS: [FamilyPeriodicRecord; 5] = [
    FamilyPeriodicRecord {
        callback: 0x0042_7490,
        actor_offset: 0x26c,
        input_factory: Some(0x0042_7640),
    },
    FamilyPeriodicRecord {
        callback: 0x0042_7510,
        actor_offset: 0x1fc,
        input_factory: Some(0x0042_7640),
    },
    COMMON_RECORDS[0],
    COMMON_RECORDS[1],
    FamilyPeriodicRecord {
        callback: 0x0046_8e30,
        actor_offset: 0x284,
        input_factory: None,
    },
];

// 00532A20: the two common records, then Selector12's +1FC record with
// input factory 00427640 (Selector35 keeps that factory DWORD null).
const SELECTOR34_RECORDS: [FamilyPeriodicRecord; 3] =
    [COMMON_RECORDS[0], COMMON_RECORDS[1], SELECTOR12_RECORDS[1]];

impl FamilyBehaviorProfile {
    /// Original literal records at005307B0/0053A0E0/00533D80/00532A20, pinned against
    /// the authenticated executable by the static/native comparison tests.
    pub fn periodic_record(self, index: u16) -> Option<FamilyPeriodicRecord> {
        let records = match self {
            Self::Selector12 { .. } => SELECTOR12_RECORDS.as_slice(),
            Self::Family4236d0 { .. } | Self::Selector35 => COMMON_RECORDS.as_slice(),
            Self::Selector34 => SELECTOR34_RECORDS.as_slice(),
        };
        records.get(usize::from(index)).copied()
    }
}

impl ObjectArena {
    pub(super) fn family_behavior_periodic_probe<H: FamilyBehaviorHost>(
        &mut self,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: u16,
        host: &mut H,
    ) -> Result<bool, FamilyBehaviorError<H::Error>> {
        let record = profile
            .periodic_record(index)
            .ok_or(FamilyBehaviorError::PeriodicIndex {
                actor,
                profile,
                index,
            })?;
        let input = if let Some(factory) = record.input_factory {
            let input = host
                .periodic_input_factory(self, actor, factory)
                .map_err(|error| FamilyBehaviorError::Host {
                    effect: "periodic input factory",
                    error,
                })?;
            //00425C87 returns false before callback/free on a null factory.
            let Some(input) = input else {
                return Ok(false);
            };
            Some(input)
        } else {
            None
        };
        let result = host
            .periodic_probe(
                self,
                actor,
                FamilyPeriodicProbeRequest {
                    profile,
                    index,
                    record,
                },
                input,
            )
            .map_err(|error| FamilyBehaviorError::Host {
                effect: "periodic probe",
                error,
            })?;
        if let Some(input) = input {
            host.release_periodic_input(self, input).map_err(|error| {
                FamilyBehaviorError::Host {
                    effect: "periodic input release",
                    error,
                }
            })?;
        }
        Ok(result as u8 != 0)
    }
}

#[cfg(test)]
#[path = "behavior_periodic_tests.rs"]
mod tests;

// Concrete producer integration lives below this wrapper's existing private
// boundary, so tests execute factory/null/probe/release policy directly.
#[cfg(test)]
#[path = "target_producers/tests.rs"]
mod target_producer_tests;
