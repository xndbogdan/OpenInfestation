//! Live 0047C6E0 composition: task/statistics custody surrounds actual class
//! reactions, retaliation, selection and lifecycle. No owner is snapshotted.

use inf_formats::FrontendExecutable;
use thiserror::Error;

use crate::{
    frontend_camera::AnimationClockSource,
    grant_text::GrantTextData,
    object_runtime::{
        ActorDamageCallback, ActorDamagePacket, ActorDamageReactionEffect, ActorDamageReactionHost,
        ActorDamageRequest, ActorExtension, ActorFamilyDamageHost, ActorHandle,
        ActorStateChangeRequest, ActorTimedDamageHost, ActorTimedDamageRequest,
        BehaviorParticleAdapter, BehaviorPredicateContext, DerivedActorClass,
        FamilySelectionRequest, ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext,
        PlayerActorLookupRequest, ProcessActorBehaviorTables, VehicleDamageHost,
        VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    },
    process_particles::{
        audio::ParticleSoundRequest,
        births::{ParticleBirthInputs, ParticleBurst, ParticleBurstRequest},
        factories::ParticleFactoryProcessState,
        ProcessParticleRuntime,
    },
    process_sound::{ProcessParticleSoundQueue, ProcessSoundRuntime},
    process_tasks::{
        damage::{ProcessActorDamageHost, ProcessActorDamageServices, ProcessDamageContext},
        ProcessTaskRuntime,
    },
    session::SessionState,
    session_platform::{
        intro::retirement::{ProductionRetirementHost, RetirementOwners, RetirementWorldOwners},
        world_actor_behavior::particles::WorldSelectionServices,
    },
    terrain_runtime::TerrainRuntime,
    world_furniture_effect::FurnitureEffectQueue,
    world_mission::WorldMission,
    ResidentRuntime,
};

/// The world factory's owners a damage reaction lends, for a target
/// selection that is a behavior row's first evaluation (00460310).
pub(crate) struct BehaviorFactoryLoan<'a> {
    pub listeners: &'a mut crate::object_runtime::CreationListeners,
    pub sine: &'a [u16; 4096],
    pub world_flags: u32,
}

pub(crate) struct WorldDamageOwners<'a, 'exe> {
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
    pub particle_load_factor: u32,
    /// None where no world factory is at hand; a reached creation stops.
    pub behavior_factory: Option<BehaviorFactoryLoan<'a>>,
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a GrantTextData<'a>>,
    /// Settings +68/+6C for the camera a reached hand-over (0046AE90)
    /// retargets, the implant's; None stops it.
    pub camera_settings: Option<crate::object_runtime::CameraSettings>,
    /// The world flash (00487D70) a viewed missile's state 1 or a steered
    /// missile's lost launcher starts; None stops it.
    pub flash: Option<&'a mut crate::world_flash::WorldFlashState>,
    /// The world map whose radar a vehicle's message jams (00420BE0), as a
    /// dying Powerball's call reaches it; None stops it.
    pub map: Option<&'a mut crate::world_map::WorldMapState>,
}

impl<'exe> WorldDamageOwners<'_, 'exe> {
    /// The same owners, lent for a nested service.
    pub(crate) fn reborrow(&mut self) -> WorldDamageOwners<'_, 'exe> {
        WorldDamageOwners {
            mission: &mut *self.mission,
            tasks: &mut *self.tasks,
            terrain: &mut *self.terrain,
            particles: &mut *self.particles,
            sounds: &mut *self.sounds,
            furniture: &mut *self.furniture,
            residency: self.residency,
            executable: self.executable,
            factory_process: &mut *self.factory_process,
            behavior_tables: &mut *self.behavior_tables,
            animation_clock: self.animation_clock,
            session: self.session,
            particle_load_factor: self.particle_load_factor,
            behavior_factory: self
                .behavior_factory
                .as_mut()
                .map(|loan| BehaviorFactoryLoan {
                    listeners: &mut *loan.listeners,
                    sine: loan.sine,
                    world_flags: loan.world_flags,
                }),
            grant_text: self.grant_text,
            camera_settings: self.camera_settings,
            flash: self.flash.as_deref_mut(),
            map: self.map.as_deref_mut(),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum WorldDamageError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {actor:?} requires its virtual+{slot:02X} damage owner")]
    RequiredVirtual { actor: ObjectId, slot: u16 },
    #[error("required damage service {0}")]
    Required(&'static str),
    #[error("{phase}: {reason}")]
    Owner { phase: &'static str, reason: String },
}

fn owner(phase: &'static str, error: impl std::fmt::Debug) -> WorldDamageError {
    WorldDamageError::Owner {
        phase,
        reason: format!("{error:?}"),
    }
}

/// The caller supplies its actual packet at the reached native damage boundary.
/// These admitted callbacks mutate arena/process owners, not its four bytes;
/// arbitrary packet aliases need an explicit future live packet carrier.
pub(crate) fn apply_world_actor_damage(
    arena: &mut ObjectArena,
    actor: ObjectId,
    packet: ActorDamagePacket,
    source: ActorHandle,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), WorldDamageError> {
    let (mut services, context) = damage_loans(owners, rng);
    arena
        .apply_actor_damage(
            actor,
            ActorDamageRequest { packet, source },
            &mut ProcessActorDamageHost::new(context, &mut services),
        )
        .map(|_| ())
        .map_err(|error| owner("0047C6E0 world actor damage", error))
}

/// Direct virtual+B4, as called by selector77 at00407CD2. This deliberately
/// omits the statistics/task wrapper 0047C6E0, which is absent at that call site.
pub(crate) fn react_world_actor_damage(
    arena: &mut ObjectArena,
    actor: ObjectId,
    callback: ActorDamageCallback,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), WorldDamageError> {
    let (mut services, context) = damage_loans(owners, rng);
    services.virtual_b4(arena, actor, callback, context)
}

pub(crate) fn apply_timed_damage(
    arena: &mut ObjectArena,
    actor: ObjectId,
    request: ActorTimedDamageRequest,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), WorldDamageError> {
    let (mut services, context) = damage_loans(owners, rng);
    arena
        .apply_actor_timed_damage(
            actor,
            request,
            &mut TimedDamageHost {
                services: &mut services,
                context,
            },
        )
        .map_err(|error| owner("0047C7C0 timed damage", error))
}

/// `0048A8B0` with the world's particle owners; its debris sounds queue
/// through the process sounds.
pub(crate) fn birth_world_burst(
    owners: &mut WorldDamageOwners<'_, '_>,
    burst: ParticleBurst,
    rng: &mut u32,
) -> Result<(), String> {
    owners
        .particles
        .birth_burst(
            ParticleBurstRequest {
                burst,
                load_factor: owners.particle_load_factor,
                inputs: ParticleBirthInputs {
                    network_active: owners.factory_process.network_flag != 0,
                    phase: Some(u32::from(owners.session.current_phase().id())),
                    process: owners.factory_process,
                },
            },
            owners.executable,
            rng,
            &mut ProcessParticleSoundQueue {
                sounds: &mut *owners.sounds,
                residency: owners.residency,
            },
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

struct TimedDamageHost<'a, 'world, 'exe> {
    services: &'a mut DamageServices<'world, 'exe>,
    context: ProcessDamageContext<'a>,
}
impl ActorTimedDamageHost for TimedDamageHost<'_, '_, '_> {
    type Error = WorldDamageError;
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error> {
        self.services.virtual_b4(
            arena,
            actor,
            callback,
            ProcessDamageContext {
                tasks: &mut *self.context.tasks,
                mission: &mut *self.context.mission,
            },
        )
    }
}

/// The lifecycle host `0047D670`'s arms reach, over the world's owners.
pub(crate) fn world_retirement_host<'b>(
    owners: &'b mut WorldDamageOwners<'_, '_>,
    rng: &'b mut u32,
) -> ProductionRetirementHost<'b> {
    ProductionRetirementHost {
        owners: world_retirement_owners(owners),
        rng,
    }
}

/// The retirement owners over the world's, with every damage owner a
/// retirement's blast needs.
pub(crate) fn world_retirement_owners<'b>(
    owners: &'b mut WorldDamageOwners<'_, '_>,
) -> RetirementOwners<'b> {
    RetirementOwners {
        mission: &mut *owners.mission,
        terrain: &mut *owners.terrain,
        tasks: &mut *owners.tasks,
        particles: &mut *owners.particles,
        sounds: &mut *owners.sounds,
        furniture: &mut *owners.furniture,
        residency: owners.residency,
        executable: owners.executable,
        phase: Some(u32::from(owners.session.current_phase().id())),
        particle_load_factor: owners.particle_load_factor,
        factory_process: &mut *owners.factory_process,
        grant_text: owners.grant_text,
        world: Some(RetirementWorldOwners {
            behavior_tables: &mut *owners.behavior_tables,
            animation_clock: owners.animation_clock,
            session: owners.session,
            behavior_factory: owners
                .behavior_factory
                .as_mut()
                .map(|loan| BehaviorFactoryLoan {
                    listeners: &mut *loan.listeners,
                    sine: loan.sine,
                    world_flags: loan.world_flags,
                }),
            map: owners.map.as_deref_mut(),
        }),
    }
}

pub(crate) fn retire_status_actor(
    arena: &mut ObjectArena,
    actor: ObjectId,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), WorldDamageError> {
    let mut host = world_retirement_host(owners, rng);
    arena
        .retire_actor(
            actor,
            host.owners.factory_process.process_null_handle,
            &mut host,
        )
        .map(|_| ())
        .map_err(|error| owner("0047CE90 status retirement", error))
}

fn damage_loans<'a, 'exe>(
    owners: &'a mut WorldDamageOwners<'_, 'exe>,
    rng: &'a mut u32,
) -> (DamageServices<'a, 'exe>, ProcessDamageContext<'a>) {
    let services = DamageServices {
        core: DamageCore {
            terrain: &mut *owners.terrain,
            particles: &mut *owners.particles,
            sounds: &mut *owners.sounds,
            furniture: &mut *owners.furniture,
            residency: owners.residency,
            executable: owners.executable,
            factory_process: &mut *owners.factory_process,
            behavior_tables: &mut *owners.behavior_tables,
            animation_clock: owners.animation_clock,
            session: owners.session,
            particle_load_factor: owners.particle_load_factor,
            behavior_factory: owners
                .behavior_factory
                .as_mut()
                .map(|loan| BehaviorFactoryLoan {
                    listeners: &mut *loan.listeners,
                    sine: loan.sine,
                    world_flags: loan.world_flags,
                }),
            grant_text: owners.grant_text,
            flash: owners.flash.as_deref_mut(),
            map: owners.map.as_deref_mut(),
            camera_settings: owners.camera_settings,
        },
        rng,
    };
    (
        services,
        ProcessDamageContext {
            tasks: &mut *owners.tasks,
            mission: &mut *owners.mission,
        },
    )
}

/// Task/mission loans stay in ProcessActorDamageHost until each callback;
/// this separate context prevents a second mutable alias to either owner.
struct DamageCore<'a, 'exe> {
    terrain: &'a mut TerrainRuntime,
    particles: &'a mut ProcessParticleRuntime,
    sounds: &'a mut ProcessSoundRuntime,
    furniture: &'a mut FurnitureEffectQueue,
    residency: &'a ResidentRuntime,
    executable: FrontendExecutable<'exe>,
    factory_process: &'a mut ParticleFactoryProcessState,
    behavior_tables: &'a mut ProcessActorBehaviorTables,
    animation_clock: &'a dyn AnimationClockSource,
    session: &'a SessionState,
    particle_load_factor: u32,
    behavior_factory: Option<BehaviorFactoryLoan<'a>>,
    grant_text: Option<&'a GrantTextData<'a>>,
    flash: Option<&'a mut crate::world_flash::WorldFlashState>,
    map: Option<&'a mut crate::world_map::WorldMapState>,
    camera_settings: Option<crate::object_runtime::CameraSettings>,
}
struct DamageServices<'a, 'exe> {
    core: DamageCore<'a, 'exe>,
    rng: &'a mut u32,
}

impl ProcessActorDamageServices for DamageServices<'_, '_> {
    type Error = WorldDamageError;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.core.factory_process.process_null_handle
    }
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
        context: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error> {
        let mut host = ReactionHost {
            core: &mut self.core,
            context,
        };
        // Actual final-table bindings, not actor+B4 classification or scene.
        match &arena.actor(actor)?.extension {
            ActorExtension::Derived(derived) => match &derived.class {
                DerivedActorClass::Family428650(_) => arena
                    .react_family_actor_damage(actor, callback, self.rng, &mut host)
                    .map_err(|error| owner("00428040 family reaction", error)),
                DerivedActorClass::Base
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector77(_) => arena
                    .react_derived_actor_damage(actor, callback, self.rng, &mut host)
                    .map_err(|error| owner("004283E0 derived reaction", error)),
                DerivedActorClass::Selector2(_) => arena
                    .react_vehicle_damage(actor, callback, self.rng, &mut host)
                    .map_err(|error| owner("004212F0 vehicle damage", error)),
                // 00527DE8+B4 = 00480C80 gates 004283E0 on its spawn list.
                DerivedActorClass::Selector60(_) => {
                    if arena
                        .selector60_reacts_to_damage(actor)
                        .map_err(|error| owner("00480C80 selector 60 reaction", error))?
                    {
                        arena
                            .react_derived_actor_damage(actor, callback, self.rng, &mut host)
                            .map_err(|error| owner("004283E0 derived reaction", error))
                    } else {
                        Ok(())
                    }
                }
            },
            // 005268AC+B4, 00525914+B4, 00525B88+B4, 005259E8+B4 and
            // 00525AB4+B4 are 0047CBC0 itself: the grenade, the mine, the
            // missiles and the Bomb (selectors 26, 38, 25, 28 and 29).
            ActorExtension::BehaviorResource(_)
                if arena.is_thrown(actor) || arena.is_missile(actor) || arena.is_bomb29(actor) =>
            {
                arena
                    .react_actor_damage(actor, callback, self.rng, &mut host)
                    .map_err(|error| owner("0047CBC0 grenade reaction", error))
            }
            // 0047CBC0 over class-specific +B0/+C0/+C4 bodies.
            ActorExtension::BehaviorResource(_) => {
                Err(WorldDamageError::RequiredVirtual { actor, slot: 0xb4 })
            }
            // 005228F0+B4 is 0047CBC0 itself.
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_) => arena
                .react_actor_damage(actor, callback, self.rng, &mut host)
                .map_err(|error| owner("0047CBC0 shared reaction", error)),
            // 00521940, 005289C0 and 0051FF54 share +B4=0040E030.
            ActorExtension::Selector15(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_) => arena
                .react_family40d800_damage(actor, callback, self.rng, &mut host)
                .map_err(|error| owner("0040E030 family reaction", error)),
            // 00527908+B4=00476720 admits the shared 0047CBC0 only at the
            // stabiliser's full height.
            ActorExtension::Selector49(_) => {
                if arena
                    .selector49_admits_damage(actor)
                    .map_err(|error| owner("00476720 stabiliser gate", error))?
                {
                    arena
                        .react_actor_damage(actor, callback, self.rng, &mut host)
                        .map_err(|error| owner("0047CBC0 stabiliser reaction", error))
                } else {
                    Ok(())
                }
            }
            // 00522398+B4=0043BD90 guards the family's 0040E030.
            ActorExtension::Selector39(_) => arena
                .react_selector39_damage(actor, callback, self.rng, &mut host)
                .map_err(|error| owner("0043BD90 course target reaction", error)),
        }
    }
    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
        _: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error> {
        arena
            .source_component_108(component, target)
            .map_err(Into::into)
    }
}

struct ReactionHost<'a, 'world, 'exe> {
    core: &'a mut DamageCore<'world, 'exe>,
    context: ProcessDamageContext<'a>,
}

impl ActorDamageReactionHost for ReactionHost<'_, '_, '_> {
    type Error = WorldDamageError;
    type RecentSource = ObjectId;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.core.factory_process.process_null_handle
    }
    fn network_active(&mut self) -> Result<bool, Self::Error> {
        Ok(self.core.factory_process.network_flag != 0)
    }
    fn difficulty_multiplier(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<i16, Self::Error> {
        //0048E7C0 is MOV AX,[ECX+10]; RET, read after the offline gate.
        self.context
            .mission
            .word_10
            .map(|value| value as i16)
            .ok_or(WorldDamageError::Required("0048E7C0 mission+10"))
    }
    fn source_bonuses(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error> {
        let player = arena
            .find_player_for_actor_handle(
                PlayerActorLookupRequest { handle: source },
                PlayerActorLookupContext {
                    process: Some(self.core.factory_process),
                },
            )
            .map_err(|error| owner("0046B090 source player", error))?;
        player
            .map(|player| {
                arena
                    .world_player(player)
                    .map(|state| std::array::from_fn(|index| state.bytes_13c_143[index] as i8))
                    .map_err(|error| owner("source player+13C bonuses", error))
            })
            .transpose()
    }
    fn animation_tick(&mut self) -> Result<u32, Self::Error> {
        Ok(self.core.animation_clock.animation_value())
    }
    /// Settings `+10` (`00463AB0`), the game's mode.
    fn network_role(&mut self) -> Result<i32, Self::Error> {
        Ok(self.core.factory_process.settings[0x10 / 4] as i32)
    }
    fn local_player_byte(&mut self) -> Result<u8, Self::Error> {
        Ok(self.core.factory_process.local_slot)
    }
    fn excludes_status(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<bool, Self::Error> {
        // Every class admitted by this B4 dispatcher has+68=00401300, except
        // the grenade's, the mine's, the missiles' and the Bomb's 00404610,
        // which answers itself.
        arena.actor(actor)?;
        Ok(arena.is_thrown(actor) || arena.is_missile(actor) || arena.is_bomb29(actor))
    }
    fn recent_damage_source(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        // The vehicle's +64 (00520860) is 00404610, itself; every other
        // admitted final table binds +64=00401300.
        let vehicle = matches!(
            &arena.actor(actor)?.extension,
            ActorExtension::Derived(derived)
                if matches!(derived.class, DerivedActorClass::Selector2(_))
        );
        Ok(vehicle.then_some(actor))
    }
    /// The vehicle's `+5A8`, its last marked hit's clock.
    fn recent_source_tick(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
    ) -> Result<u32, Self::Error> {
        let state = arena
            .selector2_actor(vehicle)
            .map_err(|error| owner("vehicle+5A8", error))?;
        state.attacked_5a8.ok_or(WorldDamageError::Required(
            "vehicle+5A8 before its first network hit",
        ))
    }
    /// The high WORD (`+5A6`) of its last marked attacker `+5A4`.
    fn recent_source_owner_word(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
    ) -> Result<u16, Self::Error> {
        let state = arena
            .selector2_actor(vehicle)
            .map_err(|error| owner("vehicle+5A4", error))?;
        Ok((state.attacker_5a4 >> 16) as u16)
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: ActorDamageReactionEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        match effect {
            ActorDamageReactionEffect::Retaliation { source } => arena
                .propagate_actor_retaliation(
                    actor,
                    source,
                    self.core.factory_process.process_null_handle,
                    rng,
                )
                .map_err(|error| owner("0047C840 retaliation", error)),
            ActorDamageReactionEffect::Lethal { callback }
                if arena.is_missile(actor) || arena.is_bomb29(actor) =>
            {
                // The missiles' state 1 (00456B10) and the Bomb's (00458F60)
                // go on to state 0.
                let mut owners = WorldDamageOwners {
                    mission: &mut *self.context.mission,
                    tasks: &mut *self.context.tasks,
                    terrain: &mut *self.core.terrain,
                    particles: &mut *self.core.particles,
                    sounds: &mut *self.core.sounds,
                    furniture: &mut *self.core.furniture,
                    residency: self.core.residency,
                    executable: self.core.executable,
                    factory_process: &mut *self.core.factory_process,
                    behavior_tables: &mut *self.core.behavior_tables,
                    animation_clock: self.core.animation_clock,
                    session: self.core.session,
                    particle_load_factor: self.core.particle_load_factor,
                    behavior_factory: self.core.behavior_factory.as_mut().map(|loan| {
                        BehaviorFactoryLoan {
                            listeners: &mut *loan.listeners,
                            sine: loan.sine,
                            world_flags: loan.world_flags,
                        }
                    }),
                    grant_text: self.core.grant_text,
                    camera_settings: None,
                    flash: None,
                    map: self.core.map.as_deref_mut(),
                };
                arena
                    .change_actor_state(
                        actor,
                        ActorStateChangeRequest {
                            target_state: 1,
                            argument: 0,
                            source: callback.source,
                        },
                        &mut crate::session_platform::world_actor_update::MissileStates {
                            owners: &mut owners,
                            rng,
                        },
                    )
                    .map_err(|error| owner("0047D670 missile lethal lifecycle", error))
            }
            ActorDamageReactionEffect::Lethal { callback } => {
                let mut host = ProductionRetirementHost {
                    owners: RetirementOwners {
                        mission: &mut *self.context.mission,
                        terrain: &mut *self.core.terrain,
                        tasks: &mut *self.context.tasks,
                        particles: &mut *self.core.particles,
                        sounds: &mut *self.core.sounds,
                        furniture: &mut *self.core.furniture,
                        residency: self.core.residency,
                        executable: self.core.executable,
                        phase: Some(u32::from(self.core.session.current_phase().id())),
                        particle_load_factor: self.core.particle_load_factor,
                        factory_process: &mut *self.core.factory_process,
                        grant_text: self.core.grant_text,
                        world: Some(RetirementWorldOwners {
                            behavior_tables: &mut *self.core.behavior_tables,
                            animation_clock: self.core.animation_clock,
                            session: self.core.session,
                            behavior_factory: self.core.behavior_factory.as_mut().map(|loan| {
                                BehaviorFactoryLoan {
                                    listeners: &mut *loan.listeners,
                                    sine: loan.sine,
                                    world_flags: loan.world_flags,
                                }
                            }),
                            map: self.core.map.as_deref_mut(),
                        }),
                    },
                    rng,
                };
                // Target state1 does not reach+B0, the only consumer of the
                // packet-address argument. Zero here is explicitly unread.
                arena
                    .change_actor_state(
                        actor,
                        ActorStateChangeRequest {
                            target_state: 1,
                            argument: 0,
                            source: callback.source,
                        },
                        &mut host,
                    )
                    .map_err(|error| owner("0047D670 lethal lifecycle", error))
            }
            ActorDamageReactionEffect::StatusRetirement => Err(WorldDamageError::Required(
                "0047CE90 nonnull+68 status retirement",
            )),
            // 0047CBF6: the packet's DWORD, the target and the source go to
            // the machine that owns the target (message 18).
            ActorDamageReactionEffect::RemoteDamage { callback, target } => {
                let packet = callback.packet;
                let byte_03 = packet
                    .byte_03
                    .ok_or(WorldDamageError::Required("remote damage packet byte +3"))?;
                let dword = u32::from(packet.strength as u16)
                    | u32::from(packet.channel) << 16
                    | u32::from(byte_03) << 24;
                arena.queue_network_send(
                    18,
                    crate::network::protocol::GameSend::Damage([
                        dword,
                        target.raw(),
                        callback.source.raw(),
                    ]),
                );
                Ok(())
            }
            ActorDamageReactionEffect::NetworkDamageNotice {
                kind,
                local_player,
                source_owner,
            } => {
                let network = arena
                    .network_game_mut()
                    .ok_or(WorldDamageError::Required("0045DA50 network game state"))?;
                let [low, high] = kind.to_le_bytes();
                if let Some((id, send)) =
                    network
                        .loan()
                        .damage_notice([low, high, local_player, source_owner])
                {
                    arena.queue_network_send(id, send);
                }
                Ok(())
            }
            ActorDamageReactionEffect::NetworkDeathNotice {
                incoming_damage_low_word,
                local_player,
                source_owner,
            } => {
                let network = arena
                    .network_game_mut()
                    .ok_or(WorldDamageError::Required("0045D8F0 network game state"))?;
                let [low, high] = incoming_damage_low_word.to_le_bytes();
                let notice = network
                    .loan()
                    .death_notice([low, high, local_player, source_owner], rng)
                    .map_err(|error| owner("0045D8F0", error))?;
                for (id, send) in notice.sends {
                    arena.queue_network_send(id, send);
                }
                // 0045DA2D: the line to the first player's +88.
                if let (Some(line), Some(player)) = (notice.line, arena.first_player()) {
                    let mut text = line;
                    text.push(0);
                    let recipient = arena
                        .player_message(player)
                        .map_err(|error| owner("0045DA34 first player+88", error))?
                        .identity();
                    arena
                        .apply_player_message(crate::object_runtime::PlayerMessageRequest {
                            recipient,
                            operation: crate::object_runtime::PlayerMessageOperation::Send {
                                text: Some(&text),
                                flag: 0,
                            },
                        })
                        .map_err(|error| owner("0045DA34 004135D0", error))?;
                }
                Ok(())
            }
        }
    }
}

impl VehicleDamageHost for ReactionHost<'_, '_, '_> {
    fn player_process(&self) -> &ParticleFactoryProcessState {
        self.core.factory_process
    }
    fn collision_sound(&mut self, position: [u16; 3], volume: u32) -> Result<(), Self::Error> {
        self.core
            .sounds
            .enqueue(
                self.core.residency,
                ParticleSoundRequest {
                    sound: VEHICLE_COLLISION_SOUND,
                    position,
                    volume,
                    pitch: VEHICLE_COLLISION_PITCH,
                    flags: 0,
                },
            )
            .map_err(|error| owner("00487EC0 vehicle collision sound", error))
    }
    /// 0054ED70 holds the race.
    fn selector70_alive(&mut self, arena: &ObjectArena) -> Result<bool, Self::Error> {
        Ok(arena.race().is_some())
    }
    fn hit_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        style: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        // A hit on a vehicle viewed from inside (byte +D0, the first-person
        // view) flashes at its nose.
        let core = &mut *self.core;
        let flash = core
            .flash
            .as_deref_mut()
            .ok_or(WorldDamageError::Required("00487D70 vehicle hit flash"))?;
        let settings = core.camera_settings.ok_or(WorldDamageError::Required(
            "00487D70 camera settings +68/+6C",
        ))?;
        let loan = core
            .behavior_factory
            .as_mut()
            .ok_or(WorldDamageError::Required(
                "00487D70 camera's creation listeners",
            ))?;
        crate::session_platform::world_flash_host::trigger_world_flash(
            flash,
            arena,
            crate::session_platform::world_flash_host::point_flash(
                position,
                crate::object_runtime::VEHICLE_HIT_FLASH_RADIUS,
                style,
            ),
            crate::session_platform::world_flash_host::WorldFlashCamera {
                residency: core.residency,
                sine_quarter: loan.sine,
                settings,
                process: &*core.factory_process,
                animation_clock: core.animation_clock.animation_value(),
                terrain: &*core.terrain,
                world_flags: loan.world_flags,
                listeners: &mut *loan.listeners,
            },
            rng,
        )
        .map_err(|error| owner("00487D70 vehicle hit flash", error))
    }
}

impl ActorFamilyDamageHost for ReactionHost<'_, '_, '_> {
    fn packet_channel(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<u8, Self::Error> {
        Ok(callback.packet.channel)
    }
    fn select_target_100(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        let process = *self.core.factory_process;
        let clock = self.core.animation_clock.animation_value();
        let mut resources = self.core.behavior_factory.as_mut().map(|loan| {
            crate::session_platform::world_actor_factory::BehaviorResourceOwners {
                residency: self.core.residency,
                process,
                terrain: &*self.core.terrain,
                world_flags: loan.world_flags,
                clock,
                sine: loan.sine,
                listeners: &mut *loan.listeners,
            }
        });
        let mut context = BehaviorPredicateContext {
            tables: &mut *self.core.behavior_tables,
            process: &mut *self.core.factory_process,
            rng,
        };
        let mut services = WorldSelectionServices {
            tasks: &mut *self.context.tasks,
            animation_clock: self.core.animation_clock,
            session: self.core.session,
            resources: resources
                .as_mut()
                .map(|owners| owners as &mut dyn crate::session_platform::world_actor_factory::BehaviorResourceFactory),
        };
        let mut host = BehaviorParticleAdapter {
            particles: &mut *self.core.particles,
            executable: self.core.executable,
            services: &mut services,
        };
        arena
            .select_family_targets(FamilySelectionRequest { actor }, &mut context, &mut host)
            .map(|_| ())
            .map_err(|error| owner("actual virtual+100 damage target selection", error))
    }
}
