//! Live shared family+24 services. These owners are independent of the intro
//! controller and retain synchronous damage, particle and lifecycle effects.
use crate::{
    object_runtime::{
        ActorDamagePacket, ActorTimedDamageRequest, FamilyUpdateEffect, FamilyUpdateHost,
        NativeFamilyUpdateError, NativeFamilyUpdateHost, ObjectArena, ObjectId,
    },
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest},
};

use super::particles::damage::{
    apply_timed_damage, birth_world_burst, retire_status_actor, WorldDamageOwners,
};
use super::particles::radius::{apply_radius_effect, WorldRadiusOrigin};
mod simple;
pub(crate) use simple::tick_simple_object_callback;
mod selector11;
pub(crate) use selector11::WorldSelector11Host;
mod selector30;
pub(crate) use selector30::{gate_actor_player, gate_grant, GateCamera, WorldSelector30Host};
mod selector86;
pub(crate) use selector86::WorldSelector86Host;
mod selector66;
pub(crate) use selector66::WorldSelector66Host;
mod selector60;
pub(crate) use selector60::WorldSelector60Host;
mod selector34;
pub(crate) use selector34::{
    tick_world_selector34_tail, WorldRibbonHost, WorldSelector34TailContext,
    WorldSelector34TailError,
};
mod vehicle;
pub(crate) use vehicle::{CarryEffects, DroppedTargetSearch, WorldVehicleHost};
mod respawn;
pub(crate) use respawn::WorldRespawnHost;
mod pad;
pub(crate) use pad::WorldPadHost;
mod patch;
pub(crate) use patch::WorldPatchHost;
mod message;
pub(crate) use message::{
    broadcast_world_actor_message, broadcast_world_event, call_world_mines,
    deliver_world_actor_message, send_world_actor_message,
};
mod course;
pub(crate) use course::{TargetDeathHost, WorldBridgeHost, WorldTargetHost};
mod barrier;
pub(crate) use barrier::WorldBarrierHost;
mod force_field;
pub(crate) use force_field::WorldForceFieldHost;
mod mining_rig;
pub(crate) use mining_rig::WorldRigHost;
mod siren;
pub(crate) use siren::WorldSirenHost;
mod stabiliser;
pub(crate) use stabiliser::{StabiliserDeathHost, WorldStabiliserHost};
mod selector64;
pub(crate) use selector64::WorldGateHost;
mod grenade26;
pub(crate) use grenade26::WorldGrenadeHost;
mod missile;
pub(crate) use missile::{
    inverse_sine_table, MissileStates, WorldImplantTake, WorldMissileLifecycle,
};
mod selector76;
pub(crate) use selector76::WorldFlameHost;
mod shot32;
#[cfg(test)]
mod weapon_test_support;
pub(crate) use shot32::WorldShotHost;
mod brain_attack;
mod ceiling;
mod director;
mod lifecycle_effects;
mod pursuit_ship;
mod rock;
mod selector15;
mod splash;
mod zone;
pub(crate) use brain_attack::WorldBrainAttackHost;
pub(crate) use ceiling::WorldCeilingHost;
pub(crate) use director::WorldDirectorHost;
pub(crate) use lifecycle_effects::{
    world_visual, EyeDeathWorldHost, PowerballDeathHost, PursuitShipDeathHost, RockDeathHost,
};
pub(crate) use pursuit_ship::WorldPursuitShipHost;
pub(crate) use rock::{WorldRockHost, WorldSpawnerHost};
pub(crate) use selector15::{
    drain_shop_menus, run_key_broadcasts, ShopServices, ShopWorld, WorldFactoryHost,
};
pub(crate) use splash::WorldSplashHost;
pub(crate) use zone::WorldZoneHost;
mod selector35;
pub(crate) use selector35::{
    tick_world_selector35_tail, WorldSelector35ClearanceHost, WorldSelector35TailContext,
    WorldSelector35TailError,
};

pub(crate) struct WorldFamilyUpdateHost<'a, 'exe> {
    pub owners: WorldDamageOwners<'a, 'exe>,
    pub rng: &'a mut u32,
    pub sine_quarter: &'a [u16; 4096],
}

/// Selector 82's burst and sound (004098C0), over the world's particles
/// and sounds.
impl crate::object_runtime::Selector82Host for WorldFamilyUpdateHost<'_, '_> {
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        burst: crate::process_particles::births::ParticleBurst,
    ) -> Result<(), NativeFamilyUpdateError> {
        birth_world_burst(&mut self.owners, burst, self.rng)
            .map_err(|error| live_error("0048A8B0 remains burst", error))
    }

    fn sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), NativeFamilyUpdateError> {
        let owners = &mut self.owners;
        owners
            .sounds
            .enqueue(
                owners.residency,
                crate::process_particles::audio::ParticleSoundRequest {
                    sound,
                    position,
                    volume: 0x10000,
                    pitch: 0x10000,
                    flags: 0,
                },
            )
            .map(|_| ())
            .map_err(|error| live_error("00487F10 remains sound", error))
    }
}

/// Powerball's ball and goals (00403300, 00402DD0, 00403F40) over the
/// world's sounds, terrain, H0 text and the first player's messages.
impl crate::object_runtime::BallHost for WorldFamilyUpdateHost<'_, '_> {
    fn ball_sound(
        &mut self,
        sound: u32,
        position: [u16; 3],
    ) -> Result<(), NativeFamilyUpdateError> {
        crate::object_runtime::Selector82Host::sound(self, sound, position)
    }

    fn ball_terrain(&self) -> &crate::terrain_runtime::TerrainRuntime {
        self.owners.terrain
    }

    fn ball_text(&self, index: u16) -> Result<Option<Vec<u8>>, NativeFamilyUpdateError> {
        use crate::object_runtime::h0_text::{capture_h0_text, H0TextCaptureRequest};
        let source = capture_h0_text(H0TextCaptureRequest {
            index_word: index,
            resident: Some(self.owners.residency),
        })
        .map_err(|error| live_error("Powerball H0 line", error))?;
        Ok(match source {
            crate::grant_text::GrantTextSource::Record(Some(text)) => {
                Some(text.iter().copied().take_while(|&byte| byte != 0).collect())
            }
            _ => None,
        })
    }

    fn ball_runtime(&self) -> &crate::ResidentRuntime {
        self.owners.residency
    }

    fn ball_line(
        &mut self,
        arena: &mut ObjectArena,
        text: Vec<u8>,
    ) -> Result<(), NativeFamilyUpdateError> {
        let player = arena
            .first_player()
            .ok_or_else(|| live_error("004135D0 Powerball line", "no first player"))?;
        let recipient = arena
            .player_message(player)
            .map_err(|error| live_error("004135D0 Powerball line", error))?
            .identity();
        arena
            .apply_player_message(crate::object_runtime::PlayerMessageRequest {
                recipient,
                operation: crate::object_runtime::PlayerMessageOperation::Send {
                    text: Some(&text),
                    flag: 0,
                },
            })
            .map(|_| ())
            .map_err(|error| live_error("004135D0 Powerball line", error))
    }
}

fn live_error(phase: &'static str, error: impl std::fmt::Debug) -> NativeFamilyUpdateError {
    NativeFamilyUpdateError::LiveService {
        phase,
        reason: format!("{error:?}"),
    }
}

/// The Prison Unit's take (004122C0 on its hold, with the slot flash) and
/// its team's task source (00482C60), over the world's owners.
impl crate::object_runtime::PrisonHost for WorldFamilyUpdateHost<'_, '_> {
    fn take_prisoner(
        &mut self,
        arena: &mut ObjectArena,
        prison: ObjectId,
        person: ObjectId,
    ) -> Result<bool, NativeFamilyUpdateError> {
        let owners = &mut self.owners;
        let context = crate::object_runtime::VehicleCarryContext {
            process: &*owners.factory_process,
            executable: owners.executable,
            resident: owners.residency,
            text_data: owners.grant_text,
        };
        let mut effects = CarryEffects {
            particles: &mut *owners.particles,
            sounds: &mut *owners.sounds,
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            phase: u32::from(owners.session.current_phase().id()),
        };
        arena
            .take_into_hold(prison, person, context, &mut effects, self.rng)
            .map_err(|error| live_error("004122C0 Prison Unit take", error))
    }

    fn task_source(
        &mut self,
        team: u32,
        source: crate::object_runtime::ActorHandle,
        enable: bool,
    ) -> Result<(), NativeFamilyUpdateError> {
        let null = self
            .owners
            .factory_process
            .process_null_handle
            .ok_or_else(|| live_error("00482C60 Prison Unit task", "the process NULL 0054E894"))?;
        let tasks = &mut *self.owners.tasks;
        let owner = tasks
            .owner(team)
            .map_err(|error| live_error("00482C60 Prison Unit task", error))?
            .ok_or_else(|| live_error("00482C60 Prison Unit task", format!("NULL slot {team}")))?;
        tasks
            .set_source(owner, source, enable, null)
            .map_err(|error| live_error("00482C60 Prison Unit task", error))
    }
}

impl FamilyUpdateHost for WorldFamilyUpdateHost<'_, '_> {
    type Error = NativeFamilyUpdateError;

    fn process_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        self.owners.factory_process.process_null_handle
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        NativeFamilyUpdateHost {
            process_rng: self.rng,
            sine_quarter: self.sine_quarter,
        }
        .random()
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        match effect {
            FamilyUpdateEffect::DelayedPhysics { .. } | FamilyUpdateEffect::WeaponTick { .. } => {
                NativeFamilyUpdateHost {
                    process_rng: self.rng,
                    sine_quarter: self.sine_quarter,
                }
                .effect(arena, actor, effect)
            }
            FamilyUpdateEffect::Drowning(request) => apply_timed_damage(
                arena,
                actor,
                ActorTimedDamageRequest {
                    packet: ActorDamagePacket {
                        strength: request.amount as i16,
                        channel: request.channel,
                        byte_03: None,
                    },
                    source: request.source,
                    microseconds: request.microseconds as i32,
                    period_microseconds: request.period_microseconds as i32,
                },
                &mut self.owners,
                self.rng,
            )
            .map_err(|error| live_error("0047C7C0 drowning", error)),
            FamilyUpdateEffect::StatusTrail {
                particle_type,
                position,
                velocity,
                flags,
            } => self
                .owners
                .particles
                .birth_trail(
                    ParticleBirthRequest {
                        particle_type,
                        position,
                        velocity: Some(velocity),
                        flags,
                        inputs: ParticleBirthInputs {
                            network_active: self.owners.factory_process.network_flag != 0,
                            phase: Some(u32::from(self.owners.session.current_phase().id())),
                            process: self.owners.factory_process,
                        },
                    },
                    self.owners.executable,
                    self.rng,
                )
                .map(|_| ())
                .map_err(|error| live_error("00489D90 status trail", error)),
            FamilyUpdateEffect::Retire => {
                retire_status_actor(arena, actor, &mut self.owners, self.rng)
                    .map_err(|error| live_error("0047CE90 status retirement", error))
            }
        }
    }
}

/// The wreck's dying update `00416090`: the family services' RNG, status
/// trail and timed damage, and the wreck's burst and radius effect.
impl crate::object_runtime::VehicleDyingHost for WorldFamilyUpdateHost<'_, '_> {
    type Error = NativeFamilyUpdateError;

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        FamilyUpdateHost::random(self)
    }

    fn rng_word(&self) -> Option<u32> {
        Some(*self.rng)
    }

    fn status_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        FamilyUpdateHost::effect(self, arena, vehicle, effect)
    }

    fn wreck_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: crate::object_runtime::TerrainDestructionEffect,
    ) -> Result<(), Self::Error> {
        use crate::object_runtime::TerrainDestructionEffect;
        match effect {
            TerrainDestructionEffect::Burst(burst) => {
                birth_world_burst(&mut self.owners, burst, self.rng)
                    .map_err(|error| live_error("0048A8B0 wreck burst", error))
            }
            TerrainDestructionEffect::Radius {
                block_address,
                source,
            } => apply_radius_effect(
                arena,
                crate::world_radius_effect::WorldRadiusEffectRequest {
                    origin: WorldRadiusOrigin::Actor {
                        actor: vehicle,
                        block_address,
                    },
                    damage_source: source,
                    replicate: true,
                },
                &mut self.owners,
                self.rng,
            )
            .map_err(|error| live_error("004887D0 wreck radius effect", error)),
            other => Err(live_error("the wreck reaches no other effect", other)),
        }
    }

    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error> {
        apply_timed_damage(arena, vehicle, request, &mut self.owners, self.rng)
            .map_err(|error| live_error("0047C7C0 wreck deep-water hit", error))
    }

    /// The family owners lend no voice teardown: a loop still voiced stops
    /// here as a required service. The world's wreck runs through
    /// [`WorldWreckHost`], which lends it.
    fn release_sound(
        &mut self,
        reference: crate::process_sound::ProcessSoundRef,
    ) -> Result<(), Self::Error> {
        self.owners
            .sounds
            .delete_record(
                reference,
                &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            )
            .map_err(|error| live_error("00487FE0 wreck weapon sound", error))
    }
}

/// The world's wreck: the family host and the platform's voice teardown, so
/// `00487FE0` stops a weapon loop's live voice as retail does.
pub(crate) struct WorldWreckHost<'h, 'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub voices: &'h mut dyn crate::process_sound::ProcessSoundVoiceTeardown,
}

impl crate::object_runtime::VehicleDyingHost for WorldWreckHost<'_, '_, '_> {
    type Error = NativeFamilyUpdateError;

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        crate::object_runtime::VehicleDyingHost::random(&mut self.family)
    }

    fn rng_word(&self) -> Option<u32> {
        crate::object_runtime::VehicleDyingHost::rng_word(&self.family)
    }

    fn status_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        crate::object_runtime::VehicleDyingHost::status_effect(
            &mut self.family,
            arena,
            vehicle,
            effect,
        )
    }

    fn wreck_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: crate::object_runtime::TerrainDestructionEffect,
    ) -> Result<(), Self::Error> {
        crate::object_runtime::VehicleDyingHost::wreck_effect(
            &mut self.family,
            arena,
            vehicle,
            effect,
        )
    }

    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error> {
        crate::object_runtime::VehicleDyingHost::timed_damage(
            &mut self.family,
            arena,
            vehicle,
            request,
        )
    }

    fn release_sound(
        &mut self,
        reference: crate::process_sound::ProcessSoundRef,
    ) -> Result<(), Self::Error> {
        self.family
            .owners
            .sounds
            .delete_record(reference, &mut *self.voices)
            .map_err(|error| live_error("00487FE0 wreck weapon sound", error))
    }
}

impl crate::object_runtime::Selector77UpdateHost for WorldFamilyUpdateHost<'_, '_> {
    fn behavior_tables(&self) -> Option<&crate::object_runtime::ProcessActorBehaviorTables> {
        Some(self.owners.behavior_tables)
    }

    fn liquid_level(&self) -> Option<i16> {
        let terrain = &*self.owners.terrain;
        Some((terrain.plane_base() >> terrain.plane_shift()) as i16)
    }

    fn ground_height(&self, at: [u16; 2]) -> Option<i16> {
        Some(self.owners.terrain.height_at(at))
    }

    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: crate::object_runtime::ActorDamageCallback,
    ) -> Result<(), crate::object_runtime::Selector77UpdateError<Self::Error>> {
        super::particles::damage::react_world_actor_damage(
            arena,
            actor,
            callback,
            &mut self.owners,
            self.rng,
        )
        .map_err(
            |error| crate::object_runtime::Selector77UpdateError::Service {
                address: 0x0040_7cd2,
                error: live_error("00407CD2 direct damage reaction", error),
            },
        )
    }

    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        packet: crate::object_runtime::ActorDamagePacket,
        source: crate::object_runtime::ActorHandle,
    ) -> Result<(), crate::object_runtime::Selector77UpdateError<Self::Error>> {
        super::particles::damage::apply_world_actor_damage(
            arena,
            actor,
            packet,
            source,
            &mut self.owners,
            self.rng,
        )
        .map_err(
            |error| crate::object_runtime::Selector77UpdateError::Service {
                address: 0x0040_8faf,
                error: live_error("00408FAF eye strike damage", error),
            },
        )
    }

    fn queue_direction(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        shot: crate::object_runtime::Selector77Shot,
    ) -> Result<(), crate::object_runtime::Selector77UpdateError<Self::Error>> {
        arena
            .queue_actor_weapon_direction(
                actor,
                shot.microseconds,
                shot.direction_bits,
                shot.speed_scale,
                &mut crate::object_runtime::ActorWeaponFireContext {
                    tables: self.owners.behavior_tables,
                    rng: self.rng,
                    //0048D0D0 consumes the supplied direction and never queries
                    //target velocity. The producer owns that separate loan.
                    process_velocity: None,
                },
            )
            .map_err(Into::into)
    }
}
