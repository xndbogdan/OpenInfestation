//! Helniazine's zone over the world's owners: its retirements (`0047CE90`),
//! of itself or of an actor inside it, run the lifecycle with the
//! missiles' states, which own the 00455560 family's deaths and give the
//! rest, the zone's own disposal among them, to the world's.

use crate::object_runtime::{ObjectArena, ObjectId, Selector74Host};

use super::{MissileStates, WorldFamilyUpdateHost};

pub(crate) struct WorldZoneHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
}

impl Selector74Host for WorldZoneHost<'_, '_> {
    type Error = String;

    fn retire(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        let family = &mut self.family;
        let null = family.owners.factory_process.process_null_handle;
        arena
            .retire_actor(
                actor,
                null,
                &mut MissileStates {
                    owners: &mut family.owners,
                    rng: &mut *family.rng,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0047CE90 zone retirement: {error:?}"))
    }
}

#[cfg(test)]
#[path = "zone_tests.rs"]
mod tests;
