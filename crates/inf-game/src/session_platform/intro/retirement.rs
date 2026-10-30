//! Production lifecycle and selector11 world-effect composition.
//! All resources and mutable process owners come from the current Session.

use inf_formats::{FrontendExecutable, TerrainResourceLookup};

use crate::{
    object_runtime::{
        ActorDamageRequest, ActorHandle, ActorStateChangeEffect, ActorStateChangeHost,
        ActorStateChangeQuery, ObjectArena, ObjectId, Selector11RadiusOrigin,
        Selector11RetirementHost,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBurst, ParticleBurstRequest},
        ProcessParticleRuntime,
    },
    process_sound::{ProcessParticleSoundQueue, ProcessSoundRuntime},
    process_tasks::{
        damage::{
            services::NativeProcessActorDamageServices, ProcessActorDamageHost,
            ProcessDamageContext,
        },
        ProcessTaskRuntime,
    },
    terrain_runtime::{
        FurnitureDamageRequest, TerrainFlagScatterRequest, TerrainFurnitureEffectHost,
        TerrainRuntime,
    },
    world_actor_radius_effect::{
        apply_actor_radius_effect, ActorRadiusDamageRequest, ActorRadiusEffectHost,
        ActorRadiusEffectOutcome, ActorRadiusEffectRequest,
    },
    world_furniture_effect::{
        resident::{resident_furniture_model_radius, resident_placement_table},
        ExecutableFurnitureDamageResources, FurnitureDamageOutcome, FurnitureDamageRuntime,
        FurnitureEffectQueue,
    },
    world_mission::{PlacementDeathRequest, WorldMission},
    world_radius_effect::{
        apply_world_radius_effect, RadiusEffectBlock, WorldRadiusEffectHost,
        WorldRadiusEffectRequest,
    },
    world_statistics::WorldStatistics,
    ResidentRuntime,
};

pub(crate) struct RetirementOwners<'a> {
    pub mission: &'a mut WorldMission,
    pub terrain: &'a mut TerrainRuntime,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub furniture: &'a mut FurnitureEffectQueue,
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'a>,
    /// The session phase the particle births record.
    pub phase: Option<u32>,
    pub particle_load_factor: u32,
    pub factory_process: &'a mut crate::process_particles::factories::ParticleFactoryProcessState,
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    /// The world's other damage owners. With them a retirement's blast
    /// (`004887D0`) damages every actor it reaches through that actor's own
    /// +B4, as retail's does; without them (the intro's sequence) it owns
    /// only selector 11's.
    pub world: Option<RetirementWorldOwners<'a>>,
}

/// What [`RetirementOwners`] lacks of the world's damage owners.
pub(crate) struct RetirementWorldOwners<'a> {
    /// The world map a death's call jams the radar of (00420BE0).
    pub map: Option<&'a mut crate::world_map::WorldMapState>,
    pub behavior_tables: &'a mut crate::object_runtime::ProcessActorBehaviorTables,
    pub animation_clock: &'a dyn crate::frontend_camera::AnimationClockSource,
    pub session: &'a crate::session::SessionState,
    pub behavior_factory:
        Option<crate::session_platform::particles::damage::BehaviorFactoryLoan<'a>>,
}

pub(crate) struct ProductionRetirementHost<'a> {
    pub owners: RetirementOwners<'a>,
    pub rng: &'a mut u32,
}

impl<'a> RetirementOwners<'a> {
    /// The world's damage owners again, for a death that reaches the
    /// world's radius effect, bursts or child factory; None outside a world
    /// (the intro's sequence). This lifecycle lends no camera settings or
    /// world flash.
    pub(crate) fn world_damage_owners(
        &mut self,
    ) -> Option<crate::session_platform::particles::damage::WorldDamageOwners<'_, 'a>> {
        let world = self.world.as_mut()?;
        Some(
            crate::session_platform::particles::damage::WorldDamageOwners {
                mission: &mut *self.mission,
                tasks: &mut *self.tasks,
                terrain: &mut *self.terrain,
                particles: &mut *self.particles,
                sounds: &mut *self.sounds,
                furniture: &mut *self.furniture,
                residency: self.residency,
                executable: self.executable,
                factory_process: &mut *self.factory_process,
                behavior_tables: &mut *world.behavior_tables,
                animation_clock: world.animation_clock,
                session: world.session,
                particle_load_factor: self.particle_load_factor,
                behavior_factory: world.behavior_factory.as_mut().map(|loan| {
                    crate::session_platform::particles::damage::BehaviorFactoryLoan {
                        listeners: &mut *loan.listeners,
                        sine: loan.sine,
                        world_flags: loan.world_flags,
                    }
                }),
                grant_text: self.grant_text,
                camera_settings: None,
                flash: None,
                map: world.map.as_deref_mut(),
            },
        )
    }
}

impl ActorStateChangeHost for ProductionRetirementHost<'_> {
    type Error = String;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        lifecycle_query(arena, actor, query)
    }
    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        lifecycle_attribute(self.owners.mission, arena, actor, source)
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            ActorStateChangeEffect::IncrementSelector18Statistic { owner: () } => {
                lifecycle_selector18_statistic(self.owners.mission)
            }
            ActorStateChangeEffect::RecordPlacementDeath { index } => {
                self.owners.placement_death(arena, index, 0)
            }
            ActorStateChangeEffect::StateOne { actor, source } => {
                use crate::object_runtime::{ActorExtension, DerivedActorClass};
                match &arena
                    .actor(actor)
                    .map_err(|error| error.to_string())?
                    .extension
                {
                    ActorExtension::Derived(derived)
                        if matches!(derived.class, DerivedActorClass::Family428650(_)) =>
                    {
                        arena
                            .retire_family_actor(actor, source, self)
                            .map_err(|error| format!("{error:?}"))
                    }
                    ActorExtension::Derived(derived)
                        if matches!(derived.class, DerivedActorClass::Selector11(_)) =>
                    {
                        arena
                            .retire_selector11(actor, source, &mut self.owners, &mut *self.rng)
                            .map_err(|error| format!("{error:?}"))
                    }
                    ActorExtension::Selector39(_) => arena
                        .retire_selector39(
                            actor,
                            &mut crate::session_platform::world_actor_update::TargetDeathHost {
                                host: self,
                            },
                        )
                        .map_err(|error| format!("0043B910 course target death: {error}")),
                    ActorExtension::BehaviorResource(_) if arena.is_thrown(actor) => arena
                        .grenade26_state_one(actor)
                        .map_err(|error| format!("00458F80 grenade state 1: {error}")),
                    ActorExtension::Derived(derived)
                        if matches!(derived.class, DerivedActorClass::Selector2(_)) =>
                    {
                        arena
                            .retire_vehicle(
                                actor,
                                &crate::object_runtime::PlayerCommandContext {
                                    process: Some(&*self.owners.factory_process),
                                    executable: Some(self.owners.executable),
                                    resident: Some(self.owners.residency),
                                },
                            )
                            .map_err(|error| format!("00420B90 vehicle death: {error}"))
                    }
                    // 00527C00, 00526714, 00526648, 00526DC8, 0051F900,
                    // 005279E0, 00521764, selector 60's 00527DE8, the
                    // barrier's 005205D8, and the Remote Beamer's and its
                    // receiver's 005274C4/00526530 bind +AC = 00404600, which only
                    // disposes the actor (00479FA0).
                    ActorExtension::Derived(derived)
                        if matches!(derived.class, DerivedActorClass::Selector60(_)) =>
                    {
                        arena
                            .dispose(actor)
                            .map_err(|error| format!("00404600 shared death: {error}"))
                    }
                    // The camera's 005204D0, the banner's 005260A0 and the
                    // base's 005267E0 too.
                    ActorExtension::Shared
                    | ActorExtension::Selector16(_)
                    | ActorExtension::Selector40(_)
                    | ActorExtension::Selector41
                    | ActorExtension::Selector78(_)
                    | ActorExtension::Selector69(_)
                    | ActorExtension::Selector52(_)
                    | ActorExtension::Selector74(_)
                    | ActorExtension::Selector62(_)
                    | ActorExtension::Selector84(_)
                    | ActorExtension::Selector53(_)
                    | ActorExtension::Selector59(_)
                    | ActorExtension::Selector18(_)
                    | ActorExtension::Selector55(_)
                    | ActorExtension::Selector56
                    | ActorExtension::Selector17(_) => arena
                        .dispose(actor)
                        .map_err(|error| format!("00404600 shared death: {error}")),
                    // 005262CC+AC = 0042DA90: the ECM Unit's holder loses
                    // the ECM, then the same disposal.
                    ActorExtension::Selector73(_) => arena
                        .retire_selector73(actor)
                        .map_err(|error| format!("0042DA90 ECM Unit death: {error}")),
                    // 00526464+AC = 00486420: the Powerball's visual 0x36
                    // (+4C), burst, message 0xC to the actors within 2000
                    // and disposal.
                    ActorExtension::Selector57 => arena
                        .retire_selector57(
                            actor,
                            &mut crate::session_platform::world_actor_update::PowerballDeathHost {
                                host: self,
                            },
                        )
                        .map_err(|error| format!("00486420 Powerball death: {error}")),
                    // 005259E8+AC and 00525B88+AC = 00456B10, the missiles',
                    // and 00526398+AC = 00458E40, the implant's: the
                    // missiles' lifecycle, which runs their state 0
                    // (00455680) too. Another machine's end messages reach
                    // them here.
                    ActorExtension::BehaviorResource(_) if arena.is_missile(actor) => {
                        let ProductionRetirementHost { owners, rng } = self;
                        let mut owners = owners
                            .world_damage_owners()
                            .ok_or("00456B10 missile death needs the world's damage owners")?;
                        crate::session_platform::world_actor_update::MissileStates {
                            owners: &mut owners,
                            rng,
                        }
                        .effect(arena, ActorStateChangeEffect::StateOne { actor, source })
                    }
                    // 00525AB4+AC = 00458F60: the Bomb goes to state 0 with
                    // its own handle, whose +B0 (00458DA0) the missiles'
                    // lifecycle runs.
                    ActorExtension::BehaviorResource(_) if arena.is_bomb29(actor) => {
                        let ProductionRetirementHost { owners, rng } = self;
                        let mut owners = owners
                            .world_damage_owners()
                            .ok_or("00458F60 Bomb death needs the world's damage owners")?;
                        arena
                            .bomb29_state_one(
                                actor,
                                rng,
                                &mut crate::session_platform::world_actor_update::WorldMissileLifecycle {
                                    owners: &mut owners,
                                },
                            )
                            .map_err(|error| format!("00458F60 Bomb death: {error}"))
                    }
                    // 00520700+AC = 00412FA0: the Pursuit Ship's visual 0x37,
                    // burst, blast, explosion and disposal.
                    ActorExtension::Selector61(_) => arena
                        .retire_selector61(
                            actor,
                            &mut crate::session_platform::world_actor_update::PursuitShipDeathHost {
                                host: self,
                            },
                        )
                        .map_err(|error| format!("00412FA0 Pursuit Ship death: {error}")),
                    // 00525648+AC = 00453F30: the rock's blasts, burst,
                    // visuals 0x36 and 0x37 and disposal.
                    ActorExtension::Selector44(_) => arena
                        .retire_selector44(
                            actor,
                            &mut crate::session_platform::world_actor_update::RockDeathHost {
                                host: self,
                            },
                        )
                        .map_err(|error| format!("00453F30 rock death: {error}")),
                    // 0051FBB0+AC and 0051FC8C+AC = 00408560: an eye's bursts
                    // down its stalk, its sound and child at the tip.
                    ActorExtension::Derived(derived)
                        if matches!(derived.class, DerivedActorClass::Selector77(_)) =>
                    {
                        let sine = self
                            .owners
                            .world
                            .as_ref()
                            .and_then(|world| world.behavior_factory.as_ref())
                            .map(|loan| *loan.sine)
                            .ok_or("00408560 eye death needs the world's sine table")?;
                        arena
                            .retire_selector77(
                                actor,
                                &mut crate::session_platform::world_actor_update::EyeDeathWorldHost {
                                    host: self,
                                    math: inf_formats::native_basis::NativeBasisMath::new(sine),
                                },
                            )
                            .map_err(|error| format!("00408560 eye death: {error}"))
                    }
                    // 005289C0+AC = 0048C630: the gate shuts to model 0x11E.
                    ActorExtension::Selector30(_) => arena
                        .retire_selector30(actor, self.owners.residency)
                        .map_err(|error| format!("0048C630 gate death: {error}")),
                    // 00527CD4+AC = 0047FAB0: the Siren starts its 2 s death.
                    ActorExtension::Selector47(_) => arena
                        .retire_selector47(actor)
                        .map_err(|error| format!("0047FAB0 Siren death: {error}")),
                    // 005203C0+AC = 0040DE20 on the factory (00521940) and the
                    // terrain bridge (0051FF54).
                    ActorExtension::Selector15(_) | ActorExtension::Selector37(_) => arena
                        .retire_family40d800(actor, self)
                        .map_err(|error| format!("{error}")),
                    // 00527908+AC = 00476520, the stabiliser's death.
                    ActorExtension::Selector49(_) => arena
                        .retire_selector49(
                            actor,
                            &mut crate::session_platform::world_actor_update::StabiliserDeathHost {
                                host: self,
                            },
                        )
                        .map_err(|error| format!("00476520 stabiliser death: {error}")),
                    _ => Err(format!(
                        "required actor {actor:?} virtual+AC retirement owner"
                    )),
                }
            }
            // 00455680 and 00458DA0, the missiles' and the Bomb's +B0, for one
            // already at state 1.
            ActorStateChangeEffect::StateZero {
                actor,
                source,
                argument,
            } if arena.is_missile(actor) || arena.is_bomb29(actor) => {
                let ProductionRetirementHost { owners, rng } = self;
                let mut owners = owners
                    .world_damage_owners()
                    .ok_or("00455680 missile detonation needs the world's damage owners")?;
                crate::session_platform::world_actor_update::MissileStates {
                    owners: &mut owners,
                    rng,
                }
                .effect(
                    arena,
                    ActorStateChangeEffect::StateZero {
                        actor,
                        source,
                        argument,
                    },
                )
            }
            other => Err(format!(
                "native lifecycle effect remains unowned: {other:?}"
            )),
        }
    }
}

/// `0047D670`'s +68 and +58 queries. Shared, derived and `00428650`-family
/// admitted tables use the real null-returning `00401300` at +68, the
/// grenade `00404610`, which returns itself; other extensions need their
/// actual interface owner before lifecycle admission.
pub(crate) fn lifecycle_query(
    arena: &mut ObjectArena,
    actor: ObjectId,
    query: ActorStateChangeQuery,
) -> Result<bool, String> {
    match query {
        ActorStateChangeQuery::HasDerived58 => arena
            .actor_derived_component(actor)
            .map(|value| value.is_some())
            .map_err(|error| error.to_string()),
        ActorStateChangeQuery::ExcludesAttribution68 => {
            // The grenade's, the mine's, the missiles' and the Bomb's +68
            // (00404610) answers itself.
            if arena.is_thrown(actor) || arena.is_missile(actor) || arena.is_bomb29(actor) {
                return Ok(true);
            }
            match &arena
                .actor(actor)
                .map_err(|error| error.to_string())?
                .extension
            {
                // These tables bind the null 00401300 at +68 (the camera's
                // 005204D0 too).
                crate::object_runtime::ActorExtension::Shared
                | crate::object_runtime::ActorExtension::Selector16(_)
                | crate::object_runtime::ActorExtension::Selector78(_)
                | crate::object_runtime::ActorExtension::Selector15(_)
                | crate::object_runtime::ActorExtension::Selector30(_)
                | crate::object_runtime::ActorExtension::Selector37(_)
                | crate::object_runtime::ActorExtension::Selector47(_)
                | crate::object_runtime::ActorExtension::Selector49(_)
                | crate::object_runtime::ActorExtension::Selector39(_)
                | crate::object_runtime::ActorExtension::Selector53(_)
                | crate::object_runtime::ActorExtension::Selector52(_)
                | crate::object_runtime::ActorExtension::Selector73(_)
                | crate::object_runtime::ActorExtension::Selector74(_)
                | crate::object_runtime::ActorExtension::Selector61(_)
                | crate::object_runtime::ActorExtension::Selector62(_)
                | crate::object_runtime::ActorExtension::Selector84(_)
                | crate::object_runtime::ActorExtension::Selector57
                | crate::object_runtime::ActorExtension::Selector40(_)
                | crate::object_runtime::ActorExtension::Selector41
                | crate::object_runtime::ActorExtension::Selector44(_)
                | crate::object_runtime::ActorExtension::Selector59(_)
                | crate::object_runtime::ActorExtension::Selector18(_)
                | crate::object_runtime::ActorExtension::Selector69(_)
                | crate::object_runtime::ActorExtension::Derived(_) => {}
                _ => return Err("required actor virtual+68 attribution policy".into()),
            }
            Ok(false)
        }
    }
}

/// The lifecycle's attribution into the mission's W+0C statistics.
pub(crate) fn lifecycle_attribute(
    mission: &mut WorldMission,
    arena: &mut ObjectArena,
    actor: ObjectId,
    source: ActorHandle,
) -> Result<(), String> {
    let record = mission
        .record_0c
        .as_mut()
        .ok_or("native retirement statistics allocation W+0C is absent")?;
    let attribution = arena
        .retirement_attribution(actor, source)
        .map_err(|error| error.to_string())?;
    WorldStatistics::from_record(record).record_attribution(attribution);
    Ok(())
}

/// The lifecycle's selector-18 retirement count in W+0C.
pub(crate) fn lifecycle_selector18_statistic(mission: &mut WorldMission) -> Result<(), String> {
    let record = mission
        .record_0c
        .as_mut()
        .ok_or("captured retirement statistics allocation W+0C is absent")?;
    WorldStatistics::from_record(record).increment_selector18_retirements();
    Ok(())
}

impl Selector11RetirementHost for RetirementOwners<'_> {
    type Error = String;
    fn world_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Selector11RadiusOrigin>,
        rng: &mut u32,
    ) -> Result<(), String> {
        apply_world_radius_effect(arena, request, self, rng)
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
    fn birth_burst(
        &mut self,
        _: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), String> {
        let inputs = ParticleBirthInputs {
            network_active: self.factory_process.network_flag != 0,
            phase: self.phase,
            process: &*self.factory_process,
        };
        self.particles
            .birth_burst(
                ParticleBurstRequest {
                    burst,
                    load_factor: self.particle_load_factor,
                    inputs,
                },
                self.executable,
                rng,
                &mut ProcessParticleSoundQueue {
                    sounds: &mut *self.sounds,
                    residency: self.residency,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
    fn scatter_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        argument: i32,
        rng: &mut u32,
    ) -> Result<(), String> {
        let position_words = arena
            .object(actor)
            .map_err(|error| error.to_string())?
            .base
            .position_words
            .ok_or("retirement scatter requires current actor XYZ")?;
        self.terrain.scatter_cell_flags(
            TerrainFlagScatterRequest {
                position_words,
                count_minus_one: argument,
            },
            rng,
        );
        Ok(())
    }
    fn placement_death(
        &mut self,
        _: &mut ObjectArena,
        index: i8,
        force: i32,
    ) -> Result<(), String> {
        let force = u8::try_from(force)
            .map_err(|_| "placement-death force is outside its native byte input")?;
        self.mission
            .record_placement_death(PlacementDeathRequest {
                index: i32::from(index),
                inhibit_byte_08: self.mission.inhibit_byte_08,
                force,
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl crate::object_runtime::FamilyDeathHost for ProductionRetirementHost<'_> {
    type Error = String;
    fn mark_death(&mut self, arena: &mut ObjectArena, index: i8) -> Result<(), String> {
        self.owners.placement_death(arena, index, 0)
    }
    fn scorch(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        argument: i32,
    ) -> Result<(), String> {
        let ProductionRetirementHost { owners, rng } = self;
        owners.scatter_terrain(arena, actor, argument, &mut **rng)
    }
}

impl WorldRadiusEffectHost for RetirementOwners<'_> {
    type Error = String;
    type Source = Selector11RadiusOrigin;
    fn furniture_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<u32, String> {
        self.terrain
            .apply_furniture_radius_effect(
                request.origin,
                &mut RetirementFurnitureHost {
                    arena,
                    residency: self.residency,
                    executable: self.executable,
                    queue: &mut *self.furniture,
                    network_active: self.factory_process.network_flag != 0,
                },
                rng,
            )
            .map_err(|error| format!("{error:?}"))
    }
    fn actor_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        rng: &mut u32,
    ) -> Result<ActorRadiusEffectOutcome, String> {
        apply_actor_radius_effect(
            arena,
            ActorRadiusEffectRequest {
                origin: request.origin,
                damage_source: request.damage_source,
            },
            &mut RetirementActorRadiusHost { owners: self },
            rng,
        )
        .map_err(|error| format!("{error:?}"))
    }
    /// Reached only after a nonzero alternate-hit count.
    fn replicate_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Self::Source>,
        _: &mut u32,
    ) -> Result<(), String> {
        let position = request
            .origin
            .position_words(arena)
            .map_err(|error| error.to_string())?;
        let block = request
            .origin
            .effect_block(arena, self.executable)
            .map_err(|error| error.to_string())?;
        let send =
            crate::world_radius_effect::radius_effect_send(request.damage_source, position, block)?;
        arena.queue_network_send(19, send);
        Ok(())
    }
}

struct RetirementFurnitureHost<'a> {
    arena: &'a ObjectArena,
    residency: &'a ResidentRuntime,
    executable: FrontendExecutable<'a>,
    queue: &'a mut FurnitureEffectQueue,
    network_active: bool,
}
impl TerrainFurnitureEffectHost for RetirementFurnitureHost<'_> {
    type Error = String;
    type Source = Selector11RadiusOrigin;
    fn source_block(&mut self, origin: Self::Source) -> Result<RadiusEffectBlock, String> {
        origin
            .effect_block(self.arena, self.executable)
            .map_err(|error| error.to_string())
    }
    fn source_position_words(&mut self, origin: Self::Source) -> Result<[u16; 3], String> {
        origin
            .position_words(self.arena)
            .map_err(|error| error.to_string())
    }
    fn furniture_model_radius(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<u16>, String> {
        resident_furniture_model_radius(self.residency, lookup).map_err(|error| error.to_string())
    }
    fn apply_furniture_damage(
        &mut self,
        terrain: &mut TerrainRuntime,
        request: FurnitureDamageRequest,
        rng: &mut u32,
    ) -> Result<bool, String> {
        let table = resident_placement_table(self.residency).map_err(|error| error.to_string())?;
        let mut resources = ExecutableFurnitureDamageResources {
            placement_table: table,
            executable: self.executable,
        };
        let outcome = FurnitureDamageRuntime {
            network_active: self.network_active,
            resources: &mut resources,
            queue: &mut *self.queue,
        }
        .apply(terrain, request, rng)
        .map_err(|error| format!("{error:?}"))?;
        Ok(matches!(outcome, FurnitureDamageOutcome::Accepted { .. }))
    }
}

struct RetirementActorRadiusHost<'h, 'a> {
    owners: &'h mut RetirementOwners<'a>,
}
impl ActorRadiusEffectHost for RetirementActorRadiusHost<'_, '_> {
    type Error = String;
    type Source = Selector11RadiusOrigin;
    fn source_block(
        &mut self,
        arena: &ObjectArena,
        origin: Self::Source,
    ) -> Result<RadiusEffectBlock, String> {
        origin
            .effect_block(arena, self.owners.executable)
            .map_err(|error| error.to_string())
    }
    fn source_position_words(
        &mut self,
        arena: &ObjectArena,
        origin: Self::Source,
    ) -> Result<[u16; 3], String> {
        origin
            .position_words(arena)
            .map_err(|error| error.to_string())
    }
    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorRadiusDamageRequest,
        rng: &mut u32,
    ) -> Result<(), String> {
        let RetirementOwners {
            mission,
            terrain,
            tasks,
            particles,
            sounds,
            furniture,
            residency,
            executable,
            particle_load_factor,
            factory_process,
            grant_text,
            world,
            ..
        } = &mut *self.owners;
        // 004887D0 reaches each actor in range through its own +B4.
        if let Some(world) = world {
            let mut owners = crate::session_platform::particles::damage::WorldDamageOwners {
                mission: &mut **mission,
                tasks: &mut **tasks,
                terrain: &mut **terrain,
                particles: &mut **particles,
                sounds: &mut **sounds,
                furniture: &mut **furniture,
                residency,
                executable: *executable,
                factory_process: &mut **factory_process,
                behavior_tables: &mut *world.behavior_tables,
                animation_clock: world.animation_clock,
                session: world.session,
                particle_load_factor: *particle_load_factor,
                behavior_factory: world.behavior_factory.as_mut().map(|loan| {
                    crate::session_platform::particles::damage::BehaviorFactoryLoan {
                        listeners: &mut *loan.listeners,
                        sine: loan.sine,
                        world_flags: loan.world_flags,
                    }
                }),
                grant_text: *grant_text,
                camera_settings: None,
                flash: None,
                map: None,
            };
            return crate::session_platform::particles::damage::apply_world_actor_damage(
                arena,
                request.actor,
                request.effect.into(),
                request.source,
                &mut owners,
                rng,
            )
            .map_err(|error| format!("{error:?}"));
        }
        arena
            .apply_actor_damage(
                request.actor,
                ActorDamageRequest {
                    packet: request.effect.into(),
                    source: request.source,
                },
                &mut ProcessActorDamageHost::new(
                    ProcessDamageContext {
                        tasks: &mut **tasks,
                        mission: &mut **mission,
                    },
                    &mut NativeProcessActorDamageServices::new(rng, factory_process),
                ),
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
    fn network_active(&mut self) -> Result<bool, String> {
        Ok(self.owners.factory_process.network_flag != 0)
    }
    /// Settings `+10` (`00463AB0`), the game's mode.
    fn network_role(&mut self) -> Result<i32, String> {
        Ok(self.owners.factory_process.settings[0x10 / 4] as i32)
    }
}
