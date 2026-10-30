//! Shared 004875A0 camera phase after objects/collision and before particles.
//! Live draw custody survives a pose error; it never reuses a cached pose.

use super::{
    PortPlatform, PortSession, WorldCameraFactoryContext, WorldCameraInputs,
    WorldTickPlatformInputs,
};
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        CameraFactoryInputs, CameraGlobalState, CameraPoseError, CameraPoseTickRequest,
        NativeCameraHost, NativeCameraInputs, NativeCameraServices, ObjectArena, ObjectId,
        PlayerId,
    },
    process_particles::factories::ParticleFactoryProcessState,
    world_backend::WorldSession,
    ResidentRuntime,
};

pub(super) struct WorldCameraTickContext<'a> {
    pub residency: &'a ResidentRuntime,
    pub assets: &'a FrontendAssets,
    pub clock: u32,
    pub inputs: WorldTickPlatformInputs,
    pub microseconds: u32,
    pub process: &'a ParticleFactoryProcessState,
    pub globals: &'a mut CameraGlobalState,
    pub inverse_sine: &'a [i16; 1024],
    pub up: [f32; 3],
    pub shake_enabled: u32,
}

impl<P: PortPlatform> PortSession<P> {
    pub(super) fn tick_world_camera(&mut self, microseconds: u32) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        tick_world_camera(
            world,
            WorldCameraTickContext {
                residency: &self.residency,
                assets: self.catalog.assets(),
                clock: self.process.animation().value(),
                inputs: self.platform.world_tick_inputs(),
                microseconds,
                process: &self.factory_process,
                globals: &mut self.camera_globals,
                inverse_sine: &self.camera_inverse_sine,
                up: self.process_up,
                shake_enabled: self.camera_shake_enabled,
            },
        );
    }
}

pub(super) fn tick_world_camera(world: &mut WorldSession, context: WorldCameraTickContext<'_>) {
    let sine = &context.assets.sine_table().values;
    let mut preparation = WorldCameraFactoryContext::new(WorldCameraInputs {
        residency: context.residency,
        sine_quarter: sine,
        settings: context.inputs.camera_settings,
        process: Some(context.process),
        animation_clock: context.clock,
    });
    let mut services = ProductionCameraServices {
        factory: preparation.factory(&world.terrain, world.world_flags, &mut world.listeners),
    };
    let math = services.factory.math;
    let result: Result<(), CameraPoseError> = (|| {
        //00487758 captures this actual object before00487768. Null skips
        // only the camera call; subsequent particle work still executes.
        let camera = world
            .arena
            .world_camera_with_rng(&mut services.factory, Some(&mut world.rng))
            .map_err(|error| CameraPoseError::Host(error.to_string()))?;
        world.native_camera = camera;
        let Some(camera) = camera else {
            return Ok(());
        };
        let mut host = NativeCameraHost::new(
            NativeCameraInputs {
                terrain: &world.terrain,
                world_flags: world.world_flags,
                sine_quarter: sine,
                inverse_sine: context.inverse_sine,
                process: Some(context.process),
            },
            &mut services,
        );
        world.arena.tick_camera_pose(
            camera,
            CameraPoseTickRequest {
                microseconds: context.microseconds,
                settings: context.inputs.camera_settings,
                settings_7c: context.inputs.settings_7c,
                up: context.up,
                shake_enabled: context.shake_enabled,
                globals: context.globals,
                rng: &mut world.rng,
                math,
                host: &mut host,
            },
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => world.camera_failure = None,
        Err(error) => {
            let reason = error.to_string();
            if world.camera_failure.as_ref() != Some(&reason) {
                eprintln!("Native camera feature skipped after reached writes: {error}");
            }
            world.camera_failure = Some(reason);
        }
    }
}

struct ProductionCameraServices<'a> {
    factory: CameraFactoryInputs<'a>,
}

impl NativeCameraServices for ProductionCameraServices<'_> {
    fn attach_player_to_camera(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        camera: ObjectId,
    ) -> Result<bool, CameraPoseError> {
        let handle = arena.actor(camera)?.handle;
        arena
            .attach_player(player, handle, &mut self.factory)
            .map_err(|error| CameraPoseError::Host(error.to_string()))
    }
}

#[cfg(test)]
mod tests;
