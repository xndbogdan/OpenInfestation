//! The Alien Brain's attack from the world tick (selector 62's 00464050):
//! the ribbons (selector 67 through 00486600 and 004869B0), the resources
//! 0x36 and 0x38 through the child factory (00460310), the radius blocks
//! at 00539820 (004887D0) and the world flash (00487D70). The brain itself
//! (selector 85's 00405FC0) adds its debris sparks (004665D0), the liquid
//! plane, the Invulnerability it grants (0046B2B0), its lifecycle (0047D670)
//! and the camera shake (005483B0).

use inf_formats::native_basis::NativeBasisMath;

use super::{grenade26::spark, missile::world_flash, WorldFamilyUpdateHost, WorldRibbonHost};
use crate::{
    grant_text::PickupTextData,
    object_runtime::{
        construct_selector67_ribbon, ActorChildResource, ActorHandle, ActorStateChangeRequest,
        ObjectArena, ObjectId, PlayerId, Selector62Host, Selector67FactoryRequest, Selector85Host,
    },
    session_platform::{
        cheats::{pickup, GrantLoan, GrantTables},
        particles::{
            damage::world_retirement_host,
            radius::{apply_radius_effect, WorldRadiusOrigin},
        },
        world_actor_factory::{
            RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
            WorldFactoryInitialization, WorldFactoryPosition,
        },
    },
    world_radius_effect::WorldRadiusEffectRequest,
};

pub(crate) struct WorldBrainAttackHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    /// Settings +7C, which the follow target's refresh reads.
    pub settings_7c: u32,
    /// The profile's pickup text, which the brain's grant formats.
    pub pickup_text: Option<PickupTextData<'a>>,
}

impl WorldBrainAttackHost<'_, '_> {
    fn ribbon_factory(&self) -> Selector67FactoryRequest<'_> {
        let owners = &self.family.owners;
        Selector67FactoryRequest {
            runtime: owners.residency,
            process: &*owners.factory_process,
            network_active: owners.factory_process.network_flag != 0,
        }
    }
}

impl Selector62Host for WorldBrainAttackHost<'_, '_> {
    type Error = String;

    fn process_null(&self) -> Option<ActorHandle> {
        self.family.owners.factory_process.process_null_handle
    }

    fn settings_7c(&self) -> u32 {
        self.settings_7c
    }

    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, String> {
        construct_selector67_ribbon(arena, &self.ribbon_factory(), source)
            .map_err(|error| format!("00486600 attack ribbon: {error}"))
    }

    fn extend_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), String> {
        let executable = self.family.owners.executable;
        let mut host = WorldRibbonHost {
            factory: self.ribbon_factory(),
            executable,
        };
        arena
            .update_selector67_from_ship(ribbon, delta, point, executable, &mut host)
            .map_err(|error| format!("004869B0 attack ribbon: {error}"))
    }

    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let clock = owners.animation_clock.animation_value();
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the attack's spawn needs the world's creation listeners")?;
        let math = NativeBasisMath::new(*loan.sine);
        WorldActorChildHost {
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            terrain: &*owners.terrain,
            world_flags: loan.world_flags,
            clock,
            sine: loan.sine,
            math: &math,
            camera_settings: owners.camera_settings,
            world_start: None,
            listeners: &mut *loan.listeners,
            listener_effects: &mut RequiredListenerEffects,
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
            &mut *self.family.rng,
        )
        .map(|_| ())
        .map_err(|error| format!("00460310({resource:#x}): {error}"))
    }

    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        block_address: u32,
        position: [u16; 3],
        source: ActorHandle,
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let bytes = owners
            .executable
            .mapped(block_address, 10)
            .map_err(|error| error.to_string())?
            .bytes;
        let block = std::array::from_fn(|index| bytes[index]);
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::PointBlock { position, block },
                damage_source: source,
                replicate: true,
            },
            owners,
            &mut *self.family.rng,
        )
        .map_err(|error| format!("004887D0 attack blast {block_address:08X}: {error}"))
    }

    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        radius: u32,
    ) -> Result<(), String> {
        let position = arena
            .object(actor)
            .map_err(|error| error.to_string())?
            .base
            .position_words
            .ok_or("the attack's flash reads its unwritten +32")?;
        world_flash(
            &mut self.family.owners,
            arena,
            position,
            radius,
            &mut *self.family.rng,
        )
    }
}

impl Selector85Host for WorldBrainAttackHost<'_, '_> {
    fn rng(&mut self) -> &mut u32 {
        &mut *self.family.rng
    }

    fn spark(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), String> {
        let family = &mut self.family;
        spark(&mut family.owners, point, velocity, owner, &mut *family.rng)
            .map_err(|error| format!("brain debris {error}"))
    }

    fn plane(&mut self) -> (i32, u32) {
        let terrain = &*self.family.owners.terrain;
        (terrain.plane_base(), terrain.plane_shift())
    }

    fn set_plane(&mut self, plane: i32) {
        self.family.owners.terrain.set_plane_base(plane);
    }

    fn pickup(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), String> {
        let owners = &self.family.owners;
        let text = self
            .pickup_text
            .as_ref()
            .ok_or("the brain's grant formats the profile's pickup text")?;
        let tables = GrantTables::parse(owners.executable)?;
        let loan = GrantLoan {
            executable: owners.executable,
            residency: owners.residency,
            process: &*owners.factory_process,
            text,
            clock: owners.animation_clock.animation_value(),
        };
        pickup(arena, player, id, amount, &tables, loan).map(|_| ())
    }

    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
    ) -> Result<(), String> {
        let family = &mut self.family;
        let mut host = world_retirement_host(&mut family.owners, &mut *family.rng);
        arena
            .change_actor_state(actor, request, &mut host)
            .map_err(|error| format!("0047D670 on the brain: {error:?}"))
    }

    fn shake(&mut self) {
        self.family.owners.mission.terrain_event_5483b0 = true;
    }
}
