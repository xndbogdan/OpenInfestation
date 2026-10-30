//! The barrier's update over the world's owners: `0047D670(0, +84, 1)` on
//! each actor it kills, through the production lifecycle, and `00460310`
//! with resource 0x36 at that actor's position, through the child factory.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    object_runtime::{
        ActorChildResource, ActorHandle, ActorStateChangeRequest, CameraSettings,
        CreationListenerEffects, ObjectArena, ObjectId, Selector69Host, BARRIER_TRACE_RESOURCE,
    },
    session_platform::{
        particles::damage::world_retirement_host,
        world_actor_factory::{
            WorldActorChildHost, WorldActorFactoryRequest, WorldFactoryInitialization,
            WorldFactoryPosition,
        },
    },
    world_start::WorldStartState,
};

use super::WorldFamilyUpdateHost;

pub(crate) struct WorldBarrierHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    /// The child factory's own inputs; it borrows the owners' terrain and
    /// listeners.
    pub world_flags: u32,
    pub clock: u32,
    pub math: &'a NativeBasisMath,
    pub camera_settings: CameraSettings,
    pub world_start: &'a mut WorldStartState,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
}

impl Selector69Host for WorldBarrierHost<'_, '_> {
    type Error = String;

    fn kill(
        &mut self,
        arena: &mut ObjectArena,
        victim: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        let family = &mut self.family;
        let mut host = world_retirement_host(&mut family.owners, &mut *family.rng);
        arena
            .change_actor_state(
                victim,
                ActorStateChangeRequest {
                    target_state: 1,
                    argument: 0,
                    source,
                },
                &mut host,
            )
            .map_err(|error| format!("0041152E 0047D670 on {victim:?}: {error:?}"))
    }

    fn trace(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the barrier's trace needs the world factory")?;
        WorldActorChildHost {
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            terrain: &*owners.terrain,
            world_flags: self.world_flags,
            clock: self.clock,
            sine: self.family.sine_quarter,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut *self.listener_effects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(BARRIER_TRACE_RESOURCE),
                position: WorldFactoryPosition::Local(position),
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            &mut *self.family.rng,
        )
        .map(|_| ())
        .map_err(|error| format!("00411548 00460310(0x36): {error}"))
    }
}
