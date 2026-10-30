//! The Pursuit Ship's update services from the world tick (selector 61's
//! 00412A10): the ground (00450A30), its retirement (0047CE90) and its
//! shots' owners (0048D340).

use super::{MissileStates, WorldFamilyUpdateHost};
use crate::object_runtime::{
    ActorWeaponFireContext, ObjectArena, ObjectId, Selector61Host, PROCESS_VELOCITY_0055F0F8,
};

pub(crate) struct WorldPursuitShipHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
}

impl Selector61Host for WorldPursuitShipHost<'_, '_> {
    type Error = String;

    fn ground_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<i16, String> {
        Ok(self.family.owners.terrain.height_at(xz))
    }

    fn retire(&mut self, arena: &mut ObjectArena, ship: ObjectId) -> Result<(), String> {
        let family = &mut self.family;
        let null = family.owners.factory_process.process_null_handle;
        arena
            .retire_actor(
                ship,
                null,
                &mut MissileStates {
                    owners: &mut family.owners,
                    rng: &mut *family.rng,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0047CE90 Pursuit Ship retirement: {error:?}"))
    }

    fn firing(&mut self) -> ActorWeaponFireContext<'_> {
        ActorWeaponFireContext {
            tables: &mut *self.family.owners.behavior_tables,
            rng: &mut *self.family.rng,
            process_velocity: Some(PROCESS_VELOCITY_0055F0F8),
        }
    }
}
