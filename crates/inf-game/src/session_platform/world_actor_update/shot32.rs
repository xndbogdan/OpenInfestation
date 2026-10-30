//! The cannon shot's services from the world tick: its spark (0048A0F0
//! type 5) and the delivery 00488830 of one packet to its owner's target.

use inf_formats::FrontendExecutable;

use crate::{
    frontend_camera::AnimationClockSource,
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorDamagePacket, ActorHandle, CreationListeners, NativeHeapRef, NativeTarget,
        ObjectArena, ProcessActorBehaviorTables, Shot32Host,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    process_sound::{ProcessParticleSoundQueue, ProcessSoundRuntime},
    process_tasks::ProcessTaskRuntime,
    session::SessionState,
    session_platform::particles::damage::{
        apply_world_actor_damage, BehaviorFactoryLoan, WorldDamageOwners,
    },
    terrain_runtime::{FurnitureDamageRequest, TerrainRuntime},
    world_furniture_effect::{FurnitureDamageRuntime, FurnitureEffectQueue},
    world_mission::WorldMission,
    world_radius_effect::ScaledRadiusEffect,
    world_statistics::{ActorRetirementAttribution, ActorStatisticCategory, WorldStatistics},
    ResidentRuntime,
};

/// 0048A0F0's spark type at 0048623C.
const SPARK: u32 = 5;
/// 00489D90's hit flash, type 2, with the zero velocity of 00488839.
const FLASH: u32 = 2;
/// The strength 00488E00 receives from 004888C4's push.
const GROUND_STRENGTH: i32 = 0x100;

pub(crate) struct WorldShotHost<'a, 'exe> {
    pub mission: &'a mut WorldMission,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub terrain: &'a mut TerrainRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub furniture: &'a mut FurnitureEffectQueue,
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub factory_process: &'a mut ParticleFactoryProcessState,
    pub behavior_tables: &'a mut ProcessActorBehaviorTables,
    pub animation_clock: &'a dyn AnimationClockSource,
    pub session: &'a SessionState,
    /// 0055F154, the particle load factor.
    pub particle_load_factor: u32,
    /// H15/0's DWORDs at 00548394: the four ground materials' selectors.
    pub impact_selectors: [u32; 5],
    pub listeners: &'a mut CreationListeners,
    pub sine: &'a [u16; 4096],
    pub world_flags: u32,
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
}

impl WorldShotHost<'_, '_> {
    fn flash(&mut self, position: [u16; 3], rng: &mut u32) -> Result<(), String> {
        let inputs = ParticleBirthInputs {
            network_active: self.factory_process.network_flag != 0,
            phase: Some(u32::from(self.session.current_phase().id())),
            process: self.factory_process,
        };
        self.particles
            .birth_trail(
                ParticleBirthRequest {
                    particle_type: FLASH,
                    position,
                    velocity: Some([0; 3]),
                    flags: 0,
                    inputs,
                },
                self.executable,
                rng,
            )
            .map(|_| ())
            .map_err(|error| format!("00489D90 hit flash: {error:?}"))
    }
}

impl Shot32Host for WorldShotHost<'_, '_> {
    type Error = String;

    fn spark(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        let inputs = ParticleBirthInputs {
            network_active: self.factory_process.network_flag != 0,
            phase: Some(u32::from(self.session.current_phase().id())),
            process: self.factory_process,
        };
        self.particles
            .birth_central(
                ParticleBirthRequest {
                    particle_type: SPARK,
                    position,
                    velocity: Some(velocity),
                    flags: 0,
                    inputs,
                },
                self.executable,
                rng,
                &mut ProcessParticleSoundQueue {
                    sounds: self.sounds,
                    residency: self.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0048A0F0 shot spark: {error:?}"))
    }

    /// 00488830.
    fn deliver(
        &mut self,
        arena: &mut ObjectArena,
        packet: ActorDamagePacket,
        target: NativeHeapRef,
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), String> {
        // 0048885A: every delivery counts a category-0 firing.
        if let Some(record) = self.mission.record_0c.as_mut() {
            WorldStatistics::from_record(record)
                .record_particle_firing(ActorStatisticCategory::Flag10);
        }
        let native = arena.native_target(target).map_err(|e| e.to_string())?;
        let point = native.point_words();
        match native {
            NativeTarget::ActorFollow(_) => {
                // 0048896E: +10 resolves the actor; a lost one ends here.
                let Some(actor) = arena
                    .resolve_target_actor(&native)
                    .map_err(|e| e.to_string())?
                else {
                    return Ok(());
                };
                self.flash(point.ok_or("the type-2 target's point +1C")?, rng)?;
                apply_world_actor_damage(
                    arena,
                    actor,
                    packet,
                    owner,
                    &mut WorldDamageOwners {
                        grant_text: self.grant_text,
                        camera_settings: None,
                        flash: None,
                        mission: self.mission,
                        tasks: self.tasks,
                        terrain: self.terrain,
                        particles: self.particles,
                        sounds: self.sounds,
                        furniture: self.furniture,
                        residency: self.residency,
                        executable: self.executable,
                        factory_process: self.factory_process,
                        behavior_tables: self.behavior_tables,
                        animation_clock: self.animation_clock,
                        session: self.session,
                        particle_load_factor: self.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *self.listeners,
                            sine: self.sine,
                            world_flags: self.world_flags,
                        }),
                        map: None,
                    },
                    rng,
                )
                .map_err(|error| format!("{error:?}"))?;
                // 004889A6: the target's category after the damage, and the
                // literal 0 0047F710 leaves on the stack as the source.
                if let Some(record) = self.mission.record_0c.as_mut() {
                    let base = &arena.object(actor).map_err(|e| e.to_string())?.base;
                    let classification = arena.actor(actor).map_err(|e| e.to_string())?.word_b4;
                    WorldStatistics::from_record(record).record_particle_hit(
                        ActorRetirementAttribution {
                            target: ActorStatisticCategory::from_actor(base.flags, classification),
                            source: ActorStatisticCategory::Flag10,
                        },
                    );
                }
                Ok(())
            }
            NativeTarget::FurniturePosition(furniture) => {
                // 00488939: the flash, then 00436020 at its +08's X and Z.
                self.flash(point.ok_or("the type-4 target's point +1C")?, rng)?;
                FurnitureDamageRuntime {
                    network_active: self.factory_process.network_flag != 0,
                    resources: &mut crate::session_platform::particles::radius::ProjectileFurnitureResources::new(
                        self.residency,
                        self.executable,
                    ),
                    queue: self.furniture,
                }
                .apply(
                    self.terrain,
                    FurnitureDamageRequest {
                        xz: [furniture.focus_08[0], furniture.focus_08[2]],
                        effect: ScaledRadiusEffect {
                            strength: packet.strength,
                            channel: packet.channel,
                            byte_03: packet.byte_03,
                            impulse_magnitude: 0,
                            impulse_vector: None,
                        },
                    },
                    rng,
                )
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
            }
            NativeTarget::FixedPosition(_) | NativeTarget::ObjectPosition(_) => {
                // 00488878: the material of the point's cell picks the burst.
                let point = point.ok_or("the target's point +1C")?;
                let material = (self.terrain.cell_at([point[0], point[2]]).raw() >> 8) & 3;
                let inputs = ParticleBirthInputs {
                    network_active: self.factory_process.network_flag != 0,
                    phase: Some(u32::from(self.session.current_phase().id())),
                    process: self.factory_process,
                };
                self.particles
                    .birth_ground_impact(
                        self.impact_selectors[usize::from(material)],
                        point,
                        GROUND_STRENGTH,
                        self.particle_load_factor as i32,
                        inputs,
                        self.executable,
                        rng,
                    )
                    .map_err(|error| format!("00488E00 ground burst: {error:?}"))?;
                if name_entry_rng_next(rng) & 3 == 0 {
                    self.flash(point, rng)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "shot32_tests.rs"]
mod tests;
