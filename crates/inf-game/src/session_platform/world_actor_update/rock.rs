//! The timed spawner and its falling rocks over the world's owners: the
//! eruption's event (`0047DAF0`'s zero-radius arm with no sender) through
//! the world's message delivery, each rock through the child factory
//! (`00460310`), and a rock's smoke (`0047DFD0`). Its death runs inside the
//! lifecycle ([`super::lifecycle_effects`]). World 19's spawner (selector
//! 81) raises its members through the same factory.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    object_runtime::{
        ActorChildResource, CameraSettings, CreationListenerEffects, ObjectArena, ObjectId,
        Selector43Host, Selector44Host, Selector81Host, ERUPTION_EVENT, MEMBER_RESOURCE,
        ROCK_RESOURCE,
    },
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest},
    session_platform::{
        particles::damage::WorldDamageOwners,
        world_actor_factory::{
            WorldActorChildHost, WorldActorFactoryRequest, WorldFactoryInitialization,
            WorldFactoryPosition,
        },
    },
    terrain_runtime::TerrainRuntime,
    world_start::WorldStartState,
};

use super::{broadcast_world_event, WorldFamilyUpdateHost};

/// The spawner's update. The world's RNG is the update's own argument.
pub(crate) struct WorldSpawnerHost<'a, 'exe> {
    pub owners: WorldDamageOwners<'a, 'exe>,
    pub sine_quarter: &'a [u16; 4096],
    /// The child factory's own inputs; it borrows the owners' terrain and
    /// listeners.
    pub world_flags: u32,
    pub clock: u32,
    pub math: &'a NativeBasisMath,
    pub camera_settings: CameraSettings,
    pub world_start: &'a mut WorldStartState,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
}

impl Selector43Host for WorldSpawnerHost<'_, '_> {
    type Error = String;

    fn terrain(&self) -> &TerrainRuntime {
        &*self.owners.terrain
    }

    fn eruption(
        &mut self,
        arena: &mut ObjectArena,
        argument: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        broadcast_world_event(arena, ERUPTION_EVENT, argument, &mut self.owners, None, rng)
            .map(|_| ())
            .map_err(|error| format!("0047DAF0 eruption: {error}"))
    }

    fn launch(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.spawn_at(arena, ROCK_RESOURCE, position, rng)
            .map_err(|error| format!("004545F8 00460310(0x18): {error}"))
    }
}

impl Selector81Host for WorldSpawnerHost<'_, '_> {
    type Error = String;

    fn terrain(&self) -> &TerrainRuntime {
        &*self.owners.terrain
    }

    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.spawn_at(arena, MEMBER_RESOURCE, position, rng)
            .map_err(|error| format!("004097EA 00460310(0x86): {error}"))
    }
}

impl WorldSpawnerHost<'_, '_> {
    /// `00460310(resource, position, NULL, -1, NULL, 0054E894)`.
    fn spawn_at(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        let owners = &mut self.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the spawner's spawns need the world factory")?;
        WorldActorChildHost {
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            terrain: &*owners.terrain,
            world_flags: self.world_flags,
            clock: self.clock,
            sine: self.sine_quarter,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut *self.listener_effects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(resource),
                position: WorldFactoryPosition::Local(position),
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            rng,
        )
        .map_err(|error| error.to_string())
    }
}

/// A rock's update on the update list.
pub(crate) struct WorldRockHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
}

impl Selector44Host for WorldRockHost<'_, '_> {
    type Error = String;

    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let rng = &mut *self.family.rng;
        let load_factor = Some(owners.particle_load_factor as i32);
        let trail = arena
            .status_visual_trail(rock, delta, load_factor, || {
                Ok::<_, std::convert::Infallible>(crate::name_entry::name_entry_rng_next(rng))
            })
            .map_err(|error| format!("0047DFD0 rock smoke: {error:?}"))?;
        let Some(trail) = trail else {
            return Ok(());
        };
        let request = ParticleBirthRequest {
            particle_type: trail.particle_type,
            position: trail.position,
            velocity: Some(trail.velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 rock smoke: {error:?}"))
    }
}
