//! The missiles' services (selectors 25, 28 and the implant, 68): the
//! heights, the exhaust ribbon (selector 67 through 00486600 and 004869B0),
//! the target refresh's settings, the inverse sine table, and the lifecycle
//! 0047D670 over the world's damage owners, whose states 1 and 0 come back
//! to the missile: the radius effect of its block (004887D0) and the
//! explosion (0048A0F0 type 2), or the implant's hand-over (0046AE90). The
//! steered flight adds the engine sound (00487F60, 00487FC0, and 00487FE0
//! when a released one updates), the world flash (00487D70) of a lost
//! launcher or a viewed missile's state 1, and the retirement 0047CE90.

use super::WorldGrenadeHost;
use crate::{
    object_runtime::{
        construct_selector67_ribbon, ActorChildResource, ActorHandle, ActorStateChangeEffect,
        ActorStateChangeHost, ActorStateChangeQuery, ActorStateChangeRequest, BombLifecycleHost,
        ImplantTakeHost, MissileHost, MissileLifecycleHost, MissileSteerHost, ObjectArena,
        ObjectId, PlayerId, Selector67FactoryRequest,
    },
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest, ParticleBurst},
    process_sound::ProcessParticleSoundQueue,
    process_sound::ProcessSoundRef,
    session_platform::particles::{
        damage::{world_retirement_host, WorldDamageOwners},
        radius::{apply_radius_effect, WorldRadiusOrigin},
    },
    session_platform::world_actor_factory::{WorldCameraFactoryContext, WorldCameraInputs},
    terrain_runtime::TerrainRuntime,
    world_radius_effect::WorldRadiusEffectRequest,
};

/// 004556B2: the explosion's type.
const EXPLOSION: u32 = 2;
/// 0053FF00: the inverse sine table 0048F520 reads.
const INVERSE_SINE: u32 = 0x0053_ff00;

/// The missile lifecycle over the world's damage owners.
pub(crate) struct WorldMissileLifecycle<'h, 'a, 'exe> {
    pub owners: &'h mut WorldDamageOwners<'a, 'exe>,
}

impl MissileLifecycleHost for WorldMissileLifecycle<'_, '_, '_> {
    type Error = String;

    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        arena
            .change_actor_state(
                missile,
                ActorStateChangeRequest {
                    target_state: 0,
                    argument: 0,
                    source,
                },
                &mut MissileStates {
                    owners: &mut *self.owners,
                    rng,
                },
            )
            .map_err(|error| format!("0047D670 missile detonation: {error:?}"))
    }

    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::ActorBlock {
                    actor: missile,
                    block,
                },
                damage_source: owner,
                replicate: true,
            },
            &mut *self.owners,
            rng,
        )
        .map_err(|error| format!("004887D0 missile radius effect: {error}"))
    }

    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        let owners = &mut *self.owners;
        let request = ParticleBirthRequest {
            particle_type: EXPLOSION,
            position,
            velocity: None,
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_central(
                request,
                owners.executable,
                rng,
                &mut ProcessParticleSoundQueue {
                    sounds: owners.sounds,
                    residency: owners.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0048A0F0 missile explosion: {error:?}"))
    }

    /// The world flash: a viewed missile's state 1 (00456B2E) or a steered
    /// missile's lost launcher.
    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        world_flash(self.owners, arena, position, radius, rng)
    }
}

/// 00487D70(position, radius, 0), the world flash, over the damage owners.
pub(crate) fn world_flash(
    owners: &mut WorldDamageOwners<'_, '_>,
    arena: &mut ObjectArena,
    position: [u16; 3],
    radius: u32,
    rng: &mut u32,
) -> Result<(), String> {
    let flash = owners
        .flash
        .as_deref_mut()
        .ok_or("00487D70 needs the world flash")?;
    let settings = owners
        .camera_settings
        .ok_or("the world flash camera needs the settings +68 and +6C")?;
    let loan = owners
        .behavior_factory
        .as_mut()
        .ok_or("the world flash camera needs the creation listeners")?;
    super::super::world_flash_host::trigger_world_flash(
        flash,
        arena,
        super::super::world_flash_host::point_flash(position, radius, 0),
        super::super::world_flash_host::WorldFlashCamera {
            residency: owners.residency,
            sine_quarter: loan.sine,
            settings,
            process: &*owners.factory_process,
            animation_clock: owners.animation_clock.animation_value(),
            terrain: &*owners.terrain,
            world_flags: loan.world_flags,
            listeners: &mut *loan.listeners,
        },
        rng,
    )
}

/// The Bomb's state 0 (00458DA0): its visual through the child factory and
/// its burst, over the same owners.
impl BombLifecycleHost for WorldMissileLifecycle<'_, '_, '_> {
    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        bomb: ObjectId,
        resource: ActorChildResource,
        rng: &mut u32,
    ) -> Result<(), String> {
        super::world_visual(self.owners, arena, bomb, resource, rng)
            .map_err(|error| format!("00458D36 Bomb {error}"))
    }

    fn particles(
        &mut self,
        _: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), String> {
        crate::session_platform::particles::damage::birth_world_burst(self.owners, burst, rng)
            .map_err(|error| format!("00458D87 Bomb burst: {error}"))
    }
}

/// 0047D670's arms for a missile or a Bomb: states 1 and 0 come back to it;
/// any other effect is the production lifecycle's.
pub(crate) struct MissileStates<'h, 'a, 'exe> {
    pub owners: &'h mut WorldDamageOwners<'a, 'exe>,
    pub rng: &'h mut u32,
}

impl ActorStateChangeHost for MissileStates<'_, '_, '_> {
    type Error = String;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        crate::session_platform::intro::retirement::lifecycle_query(arena, actor, query)
    }

    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        crate::session_platform::intro::retirement::lifecycle_attribute(
            self.owners.mission,
            arena,
            actor,
            source,
        )
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            // 00458E40: the implant hands its player what is near it.
            ActorStateChangeEffect::StateOne { actor, .. } if arena.is_implant(actor) => arena
                .implant_state_one(
                    actor,
                    &mut WorldImplantTake {
                        owners: &mut *self.owners,
                    },
                )
                .map_err(|error| format!("00458E40 implant state 1: {error}")),
            ActorStateChangeEffect::StateOne { actor, .. } if arena.is_missile(actor) => arena
                .missile_state_one(
                    actor,
                    self.rng,
                    &mut WorldMissileLifecycle {
                        owners: &mut *self.owners,
                    },
                )
                .map_err(|error| format!("00456B10 missile state 1: {error}")),
            // 00458F60 and 00458DA0, the Bomb's.
            ActorStateChangeEffect::StateOne { actor, .. } if arena.is_bomb29(actor) => arena
                .bomb29_state_one(
                    actor,
                    self.rng,
                    &mut WorldMissileLifecycle {
                        owners: &mut *self.owners,
                    },
                )
                .map_err(|error| format!("00458F60 Bomb state 1: {error}")),
            ActorStateChangeEffect::StateZero { actor, .. } if arena.is_bomb29(actor) => arena
                .bomb29_state_zero(
                    actor,
                    self.rng,
                    &mut WorldMissileLifecycle {
                        owners: &mut *self.owners,
                    },
                )
                .map_err(|error| format!("00458DA0 Bomb state 0: {error}")),
            ActorStateChangeEffect::StateZero { actor, .. } if arena.is_missile(actor) => arena
                .detonate_missile(
                    actor,
                    self.rng,
                    &mut WorldMissileLifecycle {
                        owners: &mut *self.owners,
                    },
                )
                .map_err(|error| format!("00455680 missile detonation: {error}")),
            other => world_retirement_host(self.owners, self.rng).effect(arena, other),
        }
    }
}

impl MissileLifecycleHost for WorldGrenadeHost<'_, '_> {
    type Error = String;

    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        WorldMissileLifecycle {
            owners: &mut self.owners,
        }
        .detonate(arena, missile, source, rng)
    }

    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        WorldMissileLifecycle {
            owners: &mut self.owners,
        }
        .burst(arena, missile, block, owner, rng)
    }

    fn explosion(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        WorldMissileLifecycle {
            owners: &mut self.owners,
        }
        .explosion(arena, position, rng)
    }

    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        WorldMissileLifecycle {
            owners: &mut self.owners,
        }
        .flash(arena, position, radius, rng)
    }
}

/// 0046AE90 for the implant over the world's damage owners: the hand-over
/// may build the camera (00478E00) and retargets it.
pub(crate) struct WorldImplantTake<'h, 'a, 'exe> {
    pub owners: &'h mut WorldDamageOwners<'a, 'exe>,
}

impl ImplantTakeHost for WorldImplantTake<'_, '_, '_> {
    type Error = String;

    fn take(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<bool, String> {
        let owners = &mut *self.owners;
        let settings = owners
            .camera_settings
            .ok_or("the hand-over camera needs the settings +68 and +6C")?;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the hand-over camera needs the creation listeners")?;
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: owners.residency,
            sine_quarter: loan.sine,
            settings,
            process: Some(&*owners.factory_process),
            animation_clock: owners.animation_clock.animation_value(),
        });
        let mut factory = camera.factory(&*owners.terrain, loan.world_flags, &mut *loan.listeners);
        arena
            .attach_player(player, handle, &mut factory)
            .map_err(|error| format!("0046AE90 implant hand-over: {error}"))
    }
}

fn position(arena: &ObjectArena, missile: ObjectId) -> Result<[u16; 3], String> {
    arena
        .object(missile)
        .map_err(|error| error.to_string())?
        .base
        .position_words
        .ok_or_else(|| "the missile position +32 is unwritten".to_string())
}

impl MissileSteerHost for WorldGrenadeHost<'_, '_> {
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        missile: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
        flag: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.owners
            .sounds
            .create_persistent(
                self.owners.residency,
                sound as i16,
                gain,
                pitch,
                flag,
                || position(arena, missile),
            )
            .map_err(|error| format!("00487F60 sound {sound:#x}: {error:?}"))
    }

    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        missile: ObjectId,
        reference: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.owners
            .sounds
            .records_mut()
            .update(reference, gain, pitch, Some(|| position(arena, missile)))
            .map_err(|error| format!("00487FC0: {error:?}"))
    }

    fn retire(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        let null = self.owners.factory_process.process_null_handle;
        arena
            .retire_actor(
                missile,
                null,
                &mut MissileStates {
                    owners: &mut self.owners,
                    rng,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0047CE90 missile retirement: {error:?}"))
    }

    fn world_flags(&self) -> u32 {
        self.world_flags
    }
}

impl MissileHost for WorldGrenadeHost<'_, '_> {
    fn terrain(&mut self, _: &mut ObjectArena) -> Result<&TerrainRuntime, String> {
        Ok(&*self.owners.terrain)
    }

    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, String> {
        construct_selector67_ribbon(arena, &self.ribbon_factory(), source)
            .map_err(|error| format!("00486600 missile ribbon: {error}"))
    }

    fn extend_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), String> {
        let executable = self.owners.executable;
        let mut host = super::selector34::WorldRibbonHost {
            factory: self.ribbon_factory(),
            executable,
        };
        arena
            .update_selector67_from_ship(ribbon, delta, point, executable, &mut host)
            .map_err(|error| format!("004869B0 missile ribbon: {error}"))
    }

    fn settings_7c(&self) -> u32 {
        self.settings_7c
    }

    fn inverse_sine(&self) -> &[i16; 1024] {
        &self.inverse_sine
    }

    fn process_null(&self) -> Option<ActorHandle> {
        self.owners.factory_process.process_null_handle
    }

    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        let voices = self
            .voices
            .as_deref_mut()
            .ok_or("00487FE0 needs the sound voices")?;
        self.owners
            .sounds
            .delete_record(reference, voices)
            .map_err(|error| format!("00487FE0: {error:?}"))
    }
}

impl WorldGrenadeHost<'_, '_> {
    fn ribbon_factory(&self) -> Selector67FactoryRequest<'_> {
        Selector67FactoryRequest {
            runtime: self.owners.residency,
            process: &*self.owners.factory_process,
            network_active: self.owners.factory_process.network_flag != 0,
        }
    }
}

/// The inverse sine table at 0053FF00.
pub(crate) fn inverse_sine_table(
    executable: inf_formats::FrontendExecutable<'_>,
) -> Result<[i16; 1024], String> {
    let bytes = executable
        .mapped(INVERSE_SINE, 2048)
        .map_err(|error| error.to_string())?
        .bytes;
    Ok(std::array::from_fn(|index| {
        i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
    }))
}

#[cfg(test)]
#[path = "missile_tests.rs"]
mod tests;

/// `00458A50`'s resource: selector 25, the launcher's missile.
const LAUNCHER_MISSILE: u32 = 0x6e;

impl crate::object_runtime::Selector94Host for WorldGrenadeHost<'_, '_> {
    type Error = String;

    /// `00458A50`: H13 0x6E (selector 25) through `004600F0` (mode 0, no
    /// position, so +32 keeps the allocation's words), placed at `position`
    /// and turned on `target`, then its `+24` with 0. A refused one is
    /// nothing.
    fn fire(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        target: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        use crate::session_platform::world_actor_factory::{
            WorldActorChildHost, WorldActorFactoryRequest, WorldFactoryInitialization,
            WorldFactoryPosition,
        };
        let loan = self
            .owners
            .behavior_factory
            .as_mut()
            .ok_or("the launcher's missile needs the world factory")?;
        let missile = WorldActorChildHost {
            residency: self.owners.residency,
            executable: self.owners.executable,
            process: &*self.owners.factory_process,
            terrain: &*self.owners.terrain,
            world_flags: self.world_flags,
            clock: self.clock,
            sine: self.sine,
            math: self.math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut *self.listener_effects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(LAUNCHER_MISSILE),
                position: WorldFactoryPosition::Residue,
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            rng,
        )
        .map_err(|error| format!("00458AB7 004600F0(0x6E): {error}"))?;
        let Some(missile) = missile else {
            return Ok(());
        };
        arena
            .place_missile25(missile, position, target, self.settings_7c)
            .map_err(|error| error.to_string())?;
        let sine = self.sine;
        arena
            .tick_missile25(missile, 0, sine, rng, self)
            .map_err(|error| format!("00458C0A the missile's +24: {error}"))
    }
}
