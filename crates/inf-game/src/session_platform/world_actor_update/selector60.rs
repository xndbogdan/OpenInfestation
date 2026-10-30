//! Live 00480610 placement factory and selector60 update service composition.
use super::super::world_actor_factory::{
    WorldActorChildHost, WorldActorFactoryRequest, WorldFactoryInitialization, WorldFactoryPosition,
};
use crate::{
    object_runtime::{
        ActorChildHost, ActorChildResource, ActorHandle, ActorPhysicsRequest,
        ObjectAfterLoadInputs, ObjectArena, ObjectId, Selector60SpawnResult, Selector60SpawnRow,
        Selector60UpdateHost, TargetNullHandleSource,
    },
    world_actor_terrain::WorldTerrainSource,
    world_loader::WorldInstanceAllocation,
};

pub(crate) struct WorldSelector60Host<'a, 'exe> {
    pub factory: WorldActorChildHost<'a, 'exe>,
    pub terrain_source: &'a WorldTerrainSource,
    pub instances: &'a WorldInstanceAllocation,
    pub mission: &'a crate::world_mission::WorldMission,
}

impl TargetNullHandleSource for WorldSelector60Host<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.factory.process.process_null_handle
    }
}

impl Selector60UpdateHost for WorldSelector60Host<'_, '_> {
    type Error = String;

    fn delayed_physics(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
    ) -> Result<(), String> {
        arena
            .tick_actor_physics(
                actor,
                ActorPhysicsRequest {
                    microseconds,
                    mask: 0x1f,
                    sine_quarter: self.factory.sine,
                },
            )
            .map_err(|error| error.to_string())
    }

    fn spawn_placement(
        &mut self,
        arena: &mut ObjectArena,
        row: &Selector60SpawnRow,
        rng: &mut u32,
    ) -> Result<Selector60SpawnResult, String> {
        // H14 acquisition precedes even the zero-reference terminal branch.
        let source = self
            .factory
            .residency
            .resource(14, 0)
            .ok_or("selector60 requires current H14/0")?;
        if !self.instances.is_current(&source) {
            return Err("selector60 H14 allocation differs from the live command owner".into());
        }
        let reference = row.reference().map_err(|error| error.to_string())?;
        if reference == 0 {
            return Ok(Selector60SpawnResult::NoPlacement);
        }
        let allocation = self.instances.allocation.runtime_allocation();
        let placement = self.instances.placement_offset(reference)?;
        let at = |offset: u32| {
            placement
                .checked_add(offset)
                .ok_or("selector60 placement offset overflow")
        };
        let resource = allocation
            .dword(placement)
            .map_err(|error| error.to_string())?;
        let suffix_offset = at(0x10)?;
        let mut position = [0; 3];
        for (index, word) in position.iter_mut().enumerate() {
            *word = allocation
                .word(at(4 + index as u32 * 2)?)
                .map_err(|error| error.to_string())?;
        }
        let terrain = self
            .factory
            .residency
            .resource(12, 0)
            .ok_or("selector60 requires current H12/0")?;
        if !self.terrain_source.is_current(&terrain) {
            return Err("selector60 H12 allocation differs from prepared terrain".into());
        }
        position[1] = position[1]
            .wrapping_add(self.factory.terrain.height_at([position[0], position[2]]) as u16);
        // Native rereads the placement pointer for each orientation word after
        // the height callback, rather than reusing the captured XYZ record.
        let mut rotation = [0; 3];
        for (index, offset) in [0x0e, 0x0a, 0x0c].into_iter().enumerate() {
            let placement = self.instances.placement_offset(reference)?;
            let field = placement
                .checked_add(offset)
                .ok_or("selector60 orientation offset overflow")?;
            rotation[index] = allocation.word(field).map_err(|error| error.to_string())?;
        }
        self.factory
            .create_object(
                arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(resource),
                    position: WorldFactoryPosition::Local(position),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: WorldFactoryInitialization::Placement {
                        placement: u32::MAX,
                        rotation,
                        suffix_offset,
                        instances: self.instances,
                    },
                },
                rng,
            )
            .map(|object| match object {
                Some(object) => Selector60SpawnResult::Created(object),
                None => Selector60SpawnResult::Rejected,
            })
            .map_err(|error| error.to_string())
    }

    fn actor_component(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        self.factory
            .resolve_child_actor(arena, raw)
            .map_err(|error| error.to_string())
    }

    fn initialize_spawned(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
        _rng: &mut u32,
    ) -> Result<(), String> {
        let completion = if arena.selector30_actor(raw).is_ok() {
            Some(
                self.mission
                    .current_world_completion()
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        arena
            .initialize_after_load(
                raw,
                ObjectAfterLoadInputs {
                    world_completion: completion.as_ref(),
                },
            )
            .map_err(|error| error.to_string())
    }

    fn source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        // Selector60's exact final +48=004026B0 returns its own +44.
        arena
            .selector60_actor(parent)
            .map_err(|error| error.to_string())?;
        Ok(Some(parent))
    }
}
