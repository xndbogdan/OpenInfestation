//! `0046D870`'s services in a live world: the pickup through the actor
//! factory, then the ground under it from the prepared H12. The crashing
//! ship's pickup and the pickup spawner's drops share them.

use super::{
    WorldActorChildHost, WorldActorFactoryRequest, WorldFactoryInitialization, WorldFactoryPosition,
};
use crate::{
    object_runtime::{
        ActorChildResource, ActorHandle, ObjectArena, ObjectId, PickupBirthHost,
        TargetNullHandleSource,
    },
    world_actor_terrain::WorldTerrainSource,
};

pub(crate) struct WorldPickupHost<'a, 'exe> {
    pub factory: WorldActorChildHost<'a, 'exe>,
    pub terrain_source: &'a WorldTerrainSource,
}

impl TargetNullHandleSource for WorldPickupHost<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.factory.process.process_null_handle
    }
}

impl PickupBirthHost for WorldPickupHost<'_, '_> {
    type Error = String;

    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        _source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.factory
            .create_object(
                arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(resource),
                    position: WorldFactoryPosition::Local(*position),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: WorldFactoryInitialization::Bare,
                },
                rng,
            )
            .map_err(|error| error.to_string())
    }

    fn refresh_model(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), String> {
        arena
            .refresh_visual_model(object, self.factory.residency)
            .map_err(|error| error.to_string())
    }

    fn ground_height(&mut self, _arena: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, String> {
        let current = self
            .factory
            .residency
            .resource(12, 0)
            .ok_or("pickup requires current H12/0")?;
        if !self.terrain_source.is_current(&current) {
            return Err("pickup H12 allocation replaced the prepared terrain".into());
        }
        // 450A30 samples terrain only; the native third argument is zero.
        Ok(self.factory.terrain.height_at(xz) as u16)
    }
}
