//! Production services for Selector11's update 004019E0: the family effects
//! and live damage, weapon, task and child-factory owners it reaches.

use inf_formats::native_basis::NativeBasisMath;

use super::{live_error, WorldFamilyUpdateHost};
use crate::{
    object_runtime::{
        ActorChildRequest, ActorChildResource, ActorHandle, ActorMessage, ActorTimedDamageRequest,
        ActorWeaponFireContext, CameraSettings, CreationListeners, FamilyUpdateEffect,
        FamilyUpdateHost, NativeFamilyUpdateError, ObjectArena, ObjectId,
        ProcessActorBehaviorTables, Selector11ScanHost, Selector11UpdateHost,
    },
    process_tasks::ProcessTaskRuntime,
    session_platform::{
        particles::damage::apply_timed_damage,
        world_actor_factory::{RequiredListenerEffects, WorldActorChildHost},
    },
    terrain_runtime::TerrainRuntime,
    world_start::WorldStartState,
};

pub(crate) struct WorldSelector11Host<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub world_start: &'a mut WorldStartState,
    pub listeners: &'a mut CreationListeners,
    pub world_flags: u32,
    pub camera_settings: CameraSettings,
}

impl FamilyUpdateHost for WorldSelector11Host<'_, '_> {
    type Error = NativeFamilyUpdateError;

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.family.process_null_handle()
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.family.random()
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.family.effect(arena, actor, effect)
    }
}

impl Selector11ScanHost for WorldSelector11Host<'_, '_> {
    fn scan_tasks(&mut self) -> Option<&mut ProcessTaskRuntime> {
        Some(&mut *self.family.owners.tasks)
    }

    fn scan_network_flag(&self) -> Option<u32> {
        Some(self.family.owners.factory_process.network_flag)
    }

    fn scan_terrain(&self) -> Option<&TerrainRuntime> {
        Some(&*self.family.owners.terrain)
    }
}

impl Selector11UpdateHost for WorldSelector11Host<'_, '_> {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables> {
        Some(&*self.family.owners.behavior_tables)
    }

    fn tasks(&self) -> Option<&ProcessTaskRuntime> {
        Some(&*self.family.owners.tasks)
    }

    fn player_damage(
        &mut self,
        arena: &mut ObjectArena,
        player: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error> {
        apply_timed_damage(
            arena,
            player,
            request,
            &mut self.family.owners,
            self.family.rng,
        )
        .map_err(|error| live_error("00401920 player damage", error))
    }

    fn player_notice(
        &mut self,
        arena: &mut ObjectArena,
        player: ObjectId,
        source: ObjectId,
        distance_squared: u32,
    ) -> Result<(), Self::Error> {
        // 004019C6: 0047DD80(player, 500, 0x17, 0).
        super::deliver_world_actor_message(
            arena,
            player,
            ActorMessage {
                sender: Some(source),
                distance_squared,
                code: 0x17,
                argument: 0,
            },
            &mut self.family.owners,
            self.family.rng,
        )
        .map_err(|error| live_error("0047DD80 virtual +98 code 0x17", error))
    }

    fn create_child(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), Self::Error> {
        let owners = &mut self.family.owners;
        let math = NativeBasisMath::new(*self.family.sine_quarter);
        let mut effects = RequiredListenerEffects;
        let mut factory = WorldActorChildHost {
            residency: owners.residency,
            executable: owners.executable,
            process: owners.factory_process,
            terrain: owners.terrain,
            world_flags: self.world_flags,
            clock: owners.animation_clock.animation_value(),
            sine: self.family.sine_quarter,
            math: &math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: self.listeners,
            listener_effects: &mut effects,
        };
        arena
            .create_actor_child(
                actor,
                ActorChildRequest {
                    resource: ActorChildResource::Full(resource),
                    position: Some(position),
                },
                None,
                &mut factory,
                self.family.rng,
            )
            .map(|_| ())
            .map_err(|error| live_error("0047AAF0 selector11 child", error))
    }

    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error> {
        arena
            .refresh_visual_model(actor, self.family.owners.residency)
            .map_err(|error| live_error("0047A110 selector11 wreck model", error))
    }

    fn retaliation(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        let null = self.family.owners.factory_process.process_null_handle;
        arena
            .propagate_actor_retaliation(actor, source, null, self.family.rng)
            .map_err(|error| live_error("0047C840 selector11 retaliation", error))
    }

    fn queue_shot(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        direction_bits: [u32; 3],
    ) -> Result<(), Self::Error> {
        arena
            .queue_actor_weapon_direction(
                actor,
                microseconds,
                direction_bits,
                0,
                &mut ActorWeaponFireContext {
                    tables: self.family.owners.behavior_tables,
                    rng: self.family.rng,
                    process_velocity: None,
                },
            )
            .map_err(|error| live_error("0048D0D0 selector11 shot", error))
    }
}
