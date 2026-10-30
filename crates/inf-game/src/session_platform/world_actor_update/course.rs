//! The course with the world's services. The targets' death `0043B910`
//! runs inside the lifecycle and their update `0043BB80` on the update
//! list; both deliver link messages (the bridge's lifecycle change), the
//! fuse sound 0x32, and the death's mission writes, commands and grants.
//! A generator's death scorches the ground and its update burns, then
//! blows it up with the world's child factory, particles and blast.
//! The bridge's update `00409A20` raises the world's terrain.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    object_runtime::{
        ActorChildRequest, ActorChildResource, ActorMessage, ActorTimedDamageRequest,
        BridgeTerrain, CameraSettings, CreationListenerEffects, FamilyLinkChange, GrantOwner,
        LiveGrantContext, LiveGrantOperation, LiveGrantRequest, ObjectArena, ObjectId,
        PlayerCommandContext, PlayerCommandOperation, PlayerCommandRequest, PlayerId,
        Selector11RetirementHost, Selector37Host, Selector39DeathHost, Selector39Host,
        Selector39LinkDelivery, Selector39MissionWrite, Selector39UpdateHost, BRIDGE_SOUND,
        FUSE_SOUND,
    },
    process_particles::{
        audio::ParticleSoundRequest,
        births::{ParticleBirthInputs, ParticleBirthRequest, ParticleBurst},
    },
    process_sound::ProcessSoundRuntime,
    session_platform::{
        intro::retirement::ProductionRetirementHost,
        particles::{
            damage::{apply_timed_damage, birth_world_burst, world_retirement_host},
            radius::{apply_radius_effect, WorldRadiusOrigin},
        },
        world_actor_factory::WorldActorChildHost,
    },
    terrain_runtime::ProcessTerrainPolicy,
    world_radius_effect::WorldRadiusEffectRequest,
    world_start::WorldStartState,
    ResidentRuntime,
};

/// `0040DC0C`: the wreck's blast, ten bytes in the executable.
const WRECK_BLAST: u32 = 0x0053_1250;

use super::{deliver_world_actor_message, WorldFamilyUpdateHost};

/// `00487EC0(0x32, position, 0x10000, 0x18000, 0)`.
fn fuse_sound(
    sounds: &mut ProcessSoundRuntime,
    residency: &ResidentRuntime,
    position: [u16; 3],
) -> Result<(), String> {
    sounds
        .enqueue(
            residency,
            ParticleSoundRequest {
                sound: FUSE_SOUND,
                position,
                volume: 0x10000,
                pitch: 0x18000,
                flags: 0,
            },
        )
        .map(|_| ())
        .map_err(|error| format!("00487EC0 fuse sound: {error:?}"))
}

/// The receiver's `0047D670` inside its `+98`.
fn link_change(
    arena: &mut ObjectArena,
    delivery: Selector39LinkDelivery,
    host: &mut ProductionRetirementHost<'_>,
) -> Result<(), String> {
    let request = match delivery.change {
        FamilyLinkChange::Lock(request) | FamilyLinkChange::Unlock(request) => request,
        FamilyLinkChange::None => return Ok(()),
        FamilyLinkChange::Release { grant } => {
            let owners = &host.owners;
            return super::message::release_world_barrier(
                arena,
                delivery.target,
                grant,
                &LiveGrantContext {
                    process: Some(owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                    text_data: owners.grant_text,
                },
            );
        }
    };
    arena
        .change_actor_state(delivery.target, request, host)
        .map_err(|error| format!("0047D670 on link message {}: {error:?}", delivery.code))
}

/// The death, inside the lifecycle's own host.
pub(crate) struct TargetDeathHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
}

impl Selector39Host for TargetDeathHost<'_, '_> {
    type Error = String;
    fn link_change(
        &mut self,
        arena: &mut ObjectArena,
        delivery: Selector39LinkDelivery,
    ) -> Result<(), String> {
        link_change(arena, delivery, self.host)
    }
    fn fuse_sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.host.owners;
        fuse_sound(owners.sounds, owners.residency, position)
    }
}

impl Selector39DeathHost for TargetDeathHost<'_, '_> {
    fn mission_write(&mut self, write: Selector39MissionWrite) -> Result<(), String> {
        let mission = &mut *self.host.owners.mission;
        match write {
            Selector39MissionWrite::StopCountdown => {
                mission.dword_24 = Some(0);
                mission.byte_17 = Some(0);
            }
            Selector39MissionWrite::AddCountdown { seconds, quadruple } => mission
                .add_target_countdown(seconds, quadruple)
                .map_err(|error| format!("0043B987 countdown: {error}"))?,
            Selector39MissionWrite::SetByte13 => mission.byte_13 = Some(1),
        }
        Ok(())
    }
    fn mark_death(&mut self, arena: &mut ObjectArena, index: i8) -> Result<(), String> {
        self.host.owners.placement_death(arena, index, 0)
    }
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u8,
    ) -> Result<(), String> {
        let owners = &self.host.owners;
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id,
                        silent: u32::from(silent),
                    },
                },
                &PlayerCommandContext {
                    process: Some(owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0046C760({raw_id:#x}, {silent}): {error}"))
    }
    fn grant(&mut self, arena: &mut ObjectArena, player: PlayerId, id: u16) -> Result<(), String> {
        let owners = &self.host.owners;
        arena
            .mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw { id: u32::from(id) },
                },
                &LiveGrantContext {
                    process: Some(owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                    text_data: owners.grant_text,
                },
            )
            .map_err(|error| format!("004750C0({id:#x}): {error}"))
    }
    fn scorch(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        argument: i32,
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = &mut *self.host;
        owners.scatter_terrain(arena, target, argument, &mut **rng)
    }
}

/// The update on the update list.
pub(crate) struct WorldTargetHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub status_load_factor: i32,
    /// The child factory's own inputs, for the wreck's visuals; it borrows
    /// the owners' terrain and listeners.
    pub world_flags: u32,
    pub clock: u32,
    pub math: &'a NativeBasisMath,
    pub camera_settings: CameraSettings,
    pub world_start: &'a mut WorldStartState,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
}

impl Selector39Host for WorldTargetHost<'_, '_> {
    type Error = String;
    fn link_change(
        &mut self,
        arena: &mut ObjectArena,
        delivery: Selector39LinkDelivery,
    ) -> Result<(), String> {
        let mut host = world_retirement_host(&mut self.family.owners, self.family.rng);
        link_change(arena, delivery, &mut host)
    }
    fn fuse_sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.family.owners;
        fuse_sound(owners.sounds, owners.residency, position)
    }
}

impl Selector39UpdateHost for WorldTargetHost<'_, '_> {
    fn status(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        arena
            .tick_actor_status(
                target,
                delta,
                Some(self.status_load_factor),
                &mut self.family,
            )
            .map_err(|error| format!("0047C990: {error}"))
    }
    fn radiation(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), String> {
        apply_timed_damage(
            arena,
            actor,
            request,
            &mut self.family.owners,
            self.family.rng,
        )
        .map_err(|error| format!("0047C7C0 radiation: {error}"))
    }
    fn radiation_notice(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        actor: ObjectId,
        distance_squared: u32,
    ) -> Result<(), String> {
        deliver_world_actor_message(
            arena,
            actor,
            ActorMessage {
                sender: Some(target),
                distance_squared,
                code: 0x17,
                argument: 0,
            },
            &mut self.family.owners,
            self.family.rng,
        )
    }
    fn retire(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), String> {
        let null = self.family.owners.factory_process.process_null_handle;
        let mut host = world_retirement_host(&mut self.family.owners, self.family.rng);
        arena
            .retire_actor(target, null, &mut host)
            .map(|_| ())
            .map_err(|error| format!("0047CE90 retirement: {error:?}"))
    }
    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let rng = &mut *self.family.rng;
        let load_factor = Some(owners.particle_load_factor as i32);
        let trail = arena
            .status_visual_trail(target, delta, load_factor, || {
                Ok::<_, std::convert::Infallible>(crate::name_entry::name_entry_rng_next(rng))
            })
            .map_err(|error| format!("0047DFD0 wreck smoke: {error:?}"))?;
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
            .map_err(|error| format!("00489D90 wreck smoke: {error:?}"))
    }
    fn child_visual(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        resource: u32,
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the wreck's visuals need the world factory")?;
        let mut factory = WorldActorChildHost {
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
        };
        arena
            .create_actor_child(
                target,
                ActorChildRequest {
                    resource: ActorChildResource::Full(resource),
                    position: None,
                },
                None,
                &mut factory,
                self.family.rng,
            )
            .map(|_| ())
            .map_err(|error| format!("0047AAF0 wreck visual {resource:#x}: {error}"))
    }
    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        birth_world_burst(&mut self.family.owners, burst, self.family.rng)
            .map_err(|error| format!("0048A8B0 wreck burst: {error}"))
    }
    fn blast(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), String> {
        let source = arena
            .actor(target)
            .map_err(|error| error.to_string())?
            .handle;
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::Actor {
                    actor: target,
                    block_address: WRECK_BLAST,
                },
                damage_source: source,
                replicate: true,
            },
            &mut self.family.owners,
            self.family.rng,
        )
        .map_err(|error| format!("004887D0 wreck blast: {error}"))
    }
    fn refresh_model(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), String> {
        arena
            .refresh_visual_model(target, self.family.owners.residency)
            .map_err(|error| format!("0047A110 wreck model: {error}"))
    }
    #[track_caller]
    fn random(&mut self) -> u32 {
        crate::name_entry::name_entry_rng_next(self.family.rng)
    }
    fn basis_math(&self) -> &NativeBasisMath {
        self.math
    }
    fn countdown(&mut self) -> Result<crate::object_runtime::Selector39Countdown, String> {
        let mission = &*self.family.owners.mission;
        Ok(crate::object_runtime::Selector39Countdown {
            held: mission.byte_18.ok_or("G+1C8 is unwritten")?,
            remaining: mission.dword_24.ok_or("G+1D4 is unwritten")?,
        })
    }
}

/// The bridge on the update list, with the world's terrain and the spacing
/// owner its fine scopes borrow.
pub(crate) struct WorldBridgeHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub status_load_factor: i32,
    pub policy: &'a mut ProcessTerrainPolicy,
    /// Settings +70, which `00450370` rereads.
    pub detail: u32,
    pub world_flags: u32,
}

impl Selector37Host for WorldBridgeHost<'_, '_> {
    type Error = String;
    fn terrain(&mut self) -> BridgeTerrain<'_> {
        BridgeTerrain {
            terrain: &mut *self.family.owners.terrain,
            policy: &mut *self.policy,
            detail: self.detail,
            world_flags: Some(self.world_flags),
        }
    }
    fn sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.family.owners;
        owners
            .sounds
            .enqueue(
                owners.residency,
                ParticleSoundRequest {
                    sound: BRIDGE_SOUND,
                    position,
                    volume: 0x10000,
                    pitch: 0x4000,
                    flags: 0,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("00487F10 bridge sound: {error:?}"))
    }
    fn start_doom(&mut self, micros: u32) {
        self.family.owners.mission.start_doom_countdown(micros);
    }
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u8,
    ) -> Result<(), String> {
        let owners = &self.family.owners;
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id,
                        silent: u32::from(silent),
                    },
                },
                &PlayerCommandContext {
                    process: Some(&*owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0046C760({raw_id:#x}, {silent}): {error}"))
    }
    fn status(
        &mut self,
        arena: &mut ObjectArena,
        bridge: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        arena
            .tick_actor_status(
                bridge,
                delta,
                Some(self.status_load_factor),
                &mut self.family,
            )
            .map_err(|error| format!("0047C990: {error}"))
    }
    fn create_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        use crate::session_platform::world_actor_factory::{
            RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
            WorldFactoryInitialization, WorldFactoryPosition,
        };
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the bridge's evacuee needs the world factory")?;
        let math = NativeBasisMath::new(*self.family.sine_quarter);
        WorldActorChildHost {
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            terrain: &*owners.terrain,
            world_flags: self.world_flags,
            clock: owners.animation_clock.animation_value(),
            sine: self.family.sine_quarter,
            math: &math,
            camera_settings: owners.camera_settings,
            world_start: None,
            listeners: &mut *loan.listeners,
            listener_effects: &mut RequiredListenerEffects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: crate::object_runtime::ActorChildResource::Full(
                    super::selector15::SCIENTIST_RESOURCE,
                ),
                position: WorldFactoryPosition::Local(position),
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            self.family.rng,
        )
        .map_err(|error| format!("00460310 evacuee: {error}"))
    }
    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        use crate::object_runtime::{VehicleCarryEffect, VehicleCarryHost};
        let owners = &mut self.family.owners;
        super::CarryEffects {
            particles: &mut *owners.particles,
            sounds: &mut *owners.sounds,
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            phase: u32::from(owners.session.current_phase().id()),
        }
        .effect(
            arena,
            VehicleCarryEffect::Particle {
                selector: super::selector15::FLASH_SELECTOR,
                position,
                flags: 0,
            },
            self.family.rng,
        )
    }
}
