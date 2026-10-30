//! Actual family +D8 service composition over the current world/process loans.
//! Arena custody is passed freshly into every callback. Target resources and
//! periodic temporaries remain in that arena's ActorServiceHeap; no shadow
//! allocation, row snapshot or copied animation clock is installed here.

use crate::{
    frontend_camera::AnimationClockSource,
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorHandle, BehaviorParticleAdapter, BehaviorPredicateContext, FamilyAttackHost,
        FamilyAttackUpdateHost, FamilyAttackUpdateRequest, FamilyBehaviorDescriptor,
        FamilyBehaviorHost, FamilyBehaviorProfile, FamilyDescriptorScoreError,
        FamilyDescriptorScoreRequest, FamilyDescriptorScoreRng, FamilyDormantRequest,
        FamilyHelperHistoryInputs, FamilyHelperRecordRequest, FamilyObjectTargetChangeRequest,
        FamilyPeriodicProbeRequest, FamilyPointTargetChangeRequest, FamilyRoamUpdateRequest,
        FamilySelectionRequest, FamilyTargetProducerHost, FamilyTargetProducerRequest,
        NativeHeapRef, NativeTargetError, ObjectArena, ObjectError, ObjectId,
        ProcessActorBehaviorTables, Selector12DescriptorChangeRequest, Selector12GrantGridRequest,
        TargetNullHandleSource, WeightedTargetConstructorNull,
    },
    process_particles::{factories::ParticleFactoryProcessState, ProcessParticleRuntime},
    process_sound::ProcessSoundRuntime,
    process_tasks::ProcessTaskRuntime,
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::{WorldActorTerrainError, WorldActorTerrainHost, WorldTerrainSource},
    ResidentRuntime,
};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use std::error::Error;
use thiserror::Error;

mod dying;
pub(in crate::session_platform) use dying::DyingStateZero;
mod family_dying;
pub(crate) use family_dying::{run_selector35_dormant_exit, WorldFamilyDyingContext};
mod grant_grid;
pub(crate) use grant_grid::WorldGrantGridHost;
pub(crate) mod particles;

#[derive(Debug, Error)]
pub enum WorldFamilyBehaviorError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error(transparent)]
    Descriptor(#[from] FamilyDescriptorScoreError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error("actor {actor:?} requires callback {address:08X} at {phase}")]
    RequiredCallback {
        actor: ObjectId,
        address: u32,
        phase: &'static str,
    },
    #[error("actor {actor:?} has no descriptor {index} in {profile:?}")]
    DescriptorIndex {
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
    },
    #[error("required H{handler}/{slot} backing at {phase}")]
    RequiredResource {
        handler: u8,
        slot: u16,
        phase: &'static str,
    },
    #[error("H{handler}/{slot} custody at {phase}: {reason}")]
    ResourceCustody {
        handler: u8,
        slot: u16,
        phase: &'static str,
        reason: &'static str,
    },
    #[error("actor factory 00460310 for behavior resource {resource} is not lent here")]
    RequiredActorResource460310 {
        resource: i16,
        scratch_bytes: [Option<u8>; 8],
    },
    #[error("actor factory 00460310 for behavior resource {resource}: {reason}")]
    BehaviorResourceFactory { resource: i16, reason: String },
    #[error("required component {component:?} packet source +{offset:X}")]
    RequiredPacketSource { component: ObjectId, offset: u16 },
    #[error("required particle effect {phase}")]
    RequiredParticleEffect { phase: &'static str },
    #[error("{phase}: {source}")]
    Owner {
        phase: &'static str,
        #[source]
        source: Box<dyn Error>,
    },
}

impl WorldFamilyBehaviorError {
    fn owner<E: Error + 'static>(phase: &'static str, error: E) -> Self {
        Self::Owner {
            phase,
            source: Box::new(error),
        }
    }
    fn required(actor: ObjectId, address: u32, phase: &'static str) -> Self {
        Self::RequiredCallback {
            actor,
            address,
            phase,
        }
    }
}

pub(crate) struct WorldFamilyBehaviorHost<'a, 'exe> {
    /// The grant text a reached new grant formats (004750C0); None stops it.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub animation_clock: &'a dyn AnimationClockSource,
    pub factory_process: &'a mut ParticleFactoryProcessState,
    pub session: &'a mut crate::session::SessionState,
    pub behavior_tables: &'a mut ProcessActorBehaviorTables,
    pub selector35_process: &'a mut crate::object_runtime::Selector35RouteProcessState,
    pub tasks: &'a mut ProcessTaskRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub process_rng: &'a mut u32,
    pub terrain_source: &'a WorldTerrainSource,
    pub terrain: &'a mut TerrainRuntime,
    pub dying: WorldFamilyDyingContext<'a>,
    pub sine_quarter: &'a [u16; 4096],
    pub settings_7c: u32,
    pub particle_load_factor: u32,
}

impl TargetNullHandleSource for WorldFamilyBehaviorHost<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.factory_process.process_null_handle
    }
}

impl WorldFamilyBehaviorHost<'_, '_> {
    fn selector35_descriptor(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: crate::object_runtime::Selector35DescriptorRequest,
    ) -> Result<(), WorldFamilyBehaviorError> {
        let mut terrain = WorldActorTerrainHost {
            residency: self.residency,
            source: self.terrain_source,
            prepared: self.terrain,
        };
        arena
            .run_selector35_descriptor(
                actor,
                request,
                &mut crate::object_runtime::Selector35DescriptorContext {
                    rng: self.process_rng,
                    process: self.selector35_process,
                    constructor_null: WeightedTargetConstructorNull::Process(
                        &*self.factory_process,
                    ),
                    settings_7c: self.settings_7c,
                    live_grants: crate::object_runtime::LiveGrantContext {
                        process: Some(&*self.factory_process),
                        executable: Some(self.executable),
                        resident: Some(self.residency),
                        text_data: self.grant_text,
                    },
                },
                &mut terrain,
            )
            .map_err(|error| WorldFamilyBehaviorError::owner("Selector35 descriptor", error))
    }

    fn descriptor(
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
    ) -> Result<FamilyBehaviorDescriptor, WorldFamilyBehaviorError> {
        profile
            .descriptor(index)
            .ok_or(WorldFamilyBehaviorError::DescriptorIndex {
                actor,
                profile,
                index,
            })
    }

    fn select_targets(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        actor_list: bool,
    ) -> Result<bool, WorldFamilyBehaviorError> {
        // A behavior row's first evaluation may build its registry resource
        // (00460310), reading the process null and network DWORD as current.
        let mut resources = crate::session_platform::world_actor_factory::BehaviorResourceOwners {
            residency: self.residency,
            process: *self.factory_process,
            terrain: &*self.terrain,
            world_flags: self.dying.world_flags,
            clock: self.animation_clock.animation_value(),
            sine: self.sine_quarter,
            listeners: &mut *self.dying.listeners,
        };
        // These are disjoint live owner loans. No callback can observe a copy
        // of the process cache, mutable factory state, tasks, RNG or arena.
        let mut context = BehaviorPredicateContext {
            tables: self.behavior_tables,
            process: self.factory_process,
            rng: self.process_rng,
        };
        let mut services = particles::WorldSelectionServices {
            tasks: self.tasks,
            animation_clock: self.animation_clock,
            session: &mut *self.session,
            resources: Some(&mut resources),
        };
        let mut host = BehaviorParticleAdapter {
            particles: self.particles,
            executable: self.executable,
            services: &mut services,
        };
        let request = FamilySelectionRequest { actor };
        if actor_list {
            arena
                .select_family_actor_list_targets(request, &mut context, &mut host)
                .map_err(|e| WorldFamilyBehaviorError::owner("actual virtual+104 selection", e))
        } else {
            arena
                .select_family_targets(request, &mut context, &mut host)
                .map_err(|e| WorldFamilyBehaviorError::owner("actual virtual+100 selection", e))
        }
    }

    fn sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), WorldFamilyBehaviorError> {
        //0042966B/004296BC and00429C18 test actual H9 registration/getter
        //before the XYZ pointer reaches487F10. Canonical missing resident
        //H9 returnsNULL; it bypasses enqueue and the later position read.
        if self.residency.resource(9, slot).is_none() {
            return Ok(());
        }
        let position = arena
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        self.sounds
            .enqueue(
                self.residency,
                crate::process_particles::audio::ParticleSoundRequest {
                    sound: u32::from(slot),
                    position,
                    volume: 65_536,
                    pitch: 65_536,
                    flags: 0,
                },
            )
            .map_err(|error| WorldFamilyBehaviorError::owner("47FF60 sound record", error))?;
        //Fallible allocation retains current H9; alias/RNG/PCM is later.
        Ok(())
    }
}

impl FamilyBehaviorHost for WorldFamilyBehaviorHost<'_, '_> {
    type Error = WorldFamilyBehaviorError;
    type PeriodicInput = NativeHeapRef;

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(self.process_rng))
    }

    fn periodic_input_factory(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        factory: u32,
    ) -> Result<Option<NativeHeapRef>, Self::Error> {
        if factory != 0x0042_7640 {
            return Err(WorldFamilyBehaviorError::required(
                actor,
                factory,
                "periodic input factory",
            ));
        }
        //Reuse the complete actual427640 allocation/constructor owner. It
        //writes only vtable and captured actor identity, without dereferencing
        //that actor or consuming a process RNG draw.
        Ok(arena.allocate_family_radius_score_input(actor)?)
    }

    fn periodic_probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyPeriodicProbeRequest,
        input: Option<NativeHeapRef>,
    ) -> Result<u32, Self::Error> {
        if let Some(producer) = FamilyTargetProducerRequest::from_periodic(request) {
            //Pass the real token unchanged.427510 never reads it;427490
            //dereferences it only after first-player/distance admission.
            return arena
                .produce_family_periodic_target(actor, producer, input, self)
                .map(u32::from)
                .map_err(|e| WorldFamilyBehaviorError::owner("periodic target producer", e));
        }
        if let Some(grid) = Selector12GrantGridRequest::from_periodic(request) {
            let mut host = grant_grid::WorldGrantGridHost {
                residency: self.residency,
                executable: self.executable,
                source: self.terrain_source,
                prepared: self.terrain,
                process: self.factory_process,
            };
            return arena
                .produce_selector12_grant_grid(actor, grid, &mut host)
                .map(u32::from)
                .map_err(|e| WorldFamilyBehaviorError::owner("00468E30 grant grid", e));
        }
        if request.profile.periodic_record(request.index) == Some(request.record) {
            match (request.record.callback, request.record.actor_offset) {
                (0x0042_5cf0, 0) => return self.select_targets(arena, actor, false).map(u32::from),
                (0x0042_5d00, 0) => return self.select_targets(arena, actor, true).map(u32::from),
                _ => {}
            }
        }
        Err(WorldFamilyBehaviorError::required(
            actor,
            request.record.callback,
            "periodic callback",
        ))
    }

    fn release_periodic_input(
        &mut self,
        arena: &mut ObjectArena,
        input: NativeHeapRef,
    ) -> Result<(), Self::Error> {
        arena.release_actor_service(input)?;
        Ok(())
    }

    fn score(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        current: bool,
        bias: i32,
    ) -> Result<i32, Self::Error> {
        let descriptor = Self::descriptor(actor, profile, index)?;
        let request = FamilyDescriptorScoreRequest::from_descriptor(descriptor, current, bias)
            .ok_or_else(|| {
                WorldFamilyBehaviorError::required(
                    actor,
                    descriptor.score_address,
                    "descriptor score",
                )
            })?;
        //Each source receives the actual selected-descriptor flag. Required
        //nonzero dependencies stop before their unowned callback executes.
        //Escort source 00428990 reads the live process NULL 0054E894, and
        //Selector35's 0043D730/0043D8F0 the live clock 005471C0.
        Ok(arena.score_family_descriptor_in_process(
            actor,
            request,
            FamilyDescriptorScoreRng::Available(&mut *self.process_rng),
            self.factory_process.process_null_handle,
            Some(self.animation_clock.animation_value()),
        )?)
    }

    fn candidate_bias(
        &mut self,
        _arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<i32, Self::Error> {
        //The dispatcher itself owns actual10/12/34/35/36 +110. This fallback
        //is never a success for a different unrepresented virtual.
        Err(WorldFamilyBehaviorError::required(
            actor,
            0x110,
            "candidate virtual+110",
        ))
    }

    fn change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        entering: bool,
    ) -> Result<(), Self::Error> {
        let descriptor = Self::descriptor(actor, profile, index)?;
        if let Some(request) =
            Selector12DescriptorChangeRequest::from_descriptor(profile, descriptor, entering)
        {
            return arena
                .change_selector12_descriptor_target(
                    actor,
                    request,
                    WeightedTargetConstructorNull::Process(&*self.factory_process),
                )
                .map_err(|e| {
                    WorldFamilyBehaviorError::owner("Selector12 descriptor target change", e)
                });
        }
        if let Some(request) = FamilyPointTargetChangeRequest::from_descriptor(descriptor, entering)
        {
            return arena
                .change_family_point_target(
                    actor,
                    request,
                    WeightedTargetConstructorNull::Process(&*self.factory_process),
                )
                .map_err(|e| WorldFamilyBehaviorError::owner("descriptor point change", e));
        }
        match descriptor.change_address {
            0x0043_5150
                if matches!(
                    profile,
                    FamilyBehaviorProfile::Family4236d0 { .. } | FamilyBehaviorProfile::Selector35
                ) =>
            {
                arena
                    .change_family_evade(
                        actor,
                        crate::object_runtime::FamilyEvadeEntryRequest {
                            kind: if matches!(profile, FamilyBehaviorProfile::Selector35) {
                                crate::object_runtime::FamilyEvadeEntryKind::Selector35128
                            } else {
                                crate::object_runtime::FamilyEvadeEntryKind::Common128
                            },
                            entering: u32::from(entering),
                            constructor_null: WeightedTargetConstructorNull::Process(
                                &*self.factory_process,
                            ),
                        },
                        self.process_rng,
                    )
                    .map_err(|e| {
                        WorldFamilyBehaviorError::owner("family evade descriptor change", e)
                    })
            }
            0x0043_fa80 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::ChangeObject { entering },
                ),
            0x0043_fb00 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::ChangeAttack { entering },
                ),
            0x0043_fca0 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::ChangeLanding { entering },
                ),
            0x0043_fd80 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::ChangeRescue { entering },
                ),
            0x0043_fe00 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::ChangeSpace { entering },
                ),
            0x0043_fe40 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector35(
                        crate::object_runtime::Selector35LifecycleRequest::DyingChange { entering },
                    ),
                ),
            0x0046_9380 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector35(
                        crate::object_runtime::Selector35LifecycleRequest::DormantChange {
                            entering,
                        },
                    ),
                ),
            0x0043_fe40 if matches!(profile, FamilyBehaviorProfile::Family4236d0 { .. }) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Family(
                        crate::object_runtime::FamilyDyingRequest::Change {
                            entering: u32::from(entering),
                        },
                    ),
                ),
            0x0043_fe40 if matches!(profile, FamilyBehaviorProfile::Selector34) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector34(
                        crate::object_runtime::Selector34DyingRequest::Change {
                            entering: u32::from(entering),
                        },
                    ),
                ),
            0x0043_fe40 if matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) => self
                .selector12_dying(
                    arena,
                    actor,
                    crate::object_runtime::Selector12DyingRequest::Change {
                        entering: u32::from(entering),
                    },
                ),
            0x0046_9380 => {
                let mut terrain = WorldActorTerrainHost {
                    residency: self.residency,
                    source: self.terrain_source,
                    prepared: self.terrain,
                };
                arena
                    .dispatch_family_dormant(
                        actor,
                        FamilyDormantRequest::Change { entering },
                        &mut terrain,
                    )
                    .map_err(|e| WorldFamilyBehaviorError::owner("dormant descriptor change", e))
            }
            0x0043_fa80 if !matches!(profile, FamilyBehaviorProfile::Selector35) => arena
                .change_family_object_target(
                    actor,
                    FamilyObjectTargetChangeRequest { entering },
                    WeightedTargetConstructorNull::Process(&*self.factory_process),
                )
                .map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual virtual+E8 object-target change", e)
                }),
            //Selector34's actual +F8 is 00433FB0, not common 00429B00.
            0x0043_fb00 if matches!(profile, FamilyBehaviorProfile::Selector34) => arena
                .change_selector34_attack(
                    actor,
                    entering,
                    &mut crate::object_runtime::Selector34AttackContext {
                        rng: self.process_rng,
                        constructor_null: WeightedTargetConstructorNull::Process(
                            &*self.factory_process,
                        ),
                        settings_7c: self.settings_7c,
                    },
                )
                .map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual Selector34 +F8 attack change", e)
                }),
            0x0043_fb00 if !matches!(profile, FamilyBehaviorProfile::Selector35) => arena
                .change_family_attack(actor, entering, self)
                .map_err(|e| WorldFamilyBehaviorError::owner("actual virtual+F8 attack change", e)),
            // Follow Player 00425D70 calls actual +134, 00423A80 for the
            // 004236D0 family, whose mode-2 table alone holds it.
            0x0042_5d70 if matches!(profile, FamilyBehaviorProfile::Family4236d0 { .. }) => arena
                .change_family_follow_player(actor, entering)
                .map_err(|e| WorldFamilyBehaviorError::owner("Follow Player descriptor change", e)),
            // 00435220 runs the actor's +E4, 00428AB0 on both escort tables.
            0x0043_5220
                if matches!(
                    profile,
                    FamilyBehaviorProfile::Selector34 | FamilyBehaviorProfile::Selector35
                ) =>
            {
                arena.change_flying_escort(actor, entering).map_err(|e| {
                    WorldFamilyBehaviorError::owner("Flying Escort descriptor change", e)
                })
            }
            //Herding 0043FC00 calls 00427A70 directly in every listing profile.
            0x0043_fc00 => arena
                .change_family_herding(
                    actor,
                    entering,
                    WeightedTargetConstructorNull::Process(&*self.factory_process),
                )
                .map_err(|e| WorldFamilyBehaviorError::owner("Herding descriptor change", e)),
            //Selector35's actual F8 is43D620, not common429B00. The other
            //literal descriptors forward to separate source virtuals.
            address => Err(WorldFamilyBehaviorError::required(
                actor,
                address,
                "descriptor change",
            )),
        }
    }

    fn update(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        microseconds: u32,
    ) -> Result<(), Self::Error> {
        let descriptor = Self::descriptor(actor, profile, index)?;
        match descriptor.update_address {
            // 0043FD20 runs 00428B80, the Flying Escort's update.
            0x0043_fd20
                if matches!(
                    profile,
                    FamilyBehaviorProfile::Selector34 | FamilyBehaviorProfile::Selector35
                ) =>
            {
                arena
                    .update_flying_escort(actor)
                    .map_err(|e| WorldFamilyBehaviorError::owner("Flying Escort update", e))
            }
            //Herding 0043FC20 calls 00427AA0 directly in every listing profile.
            0x0043_fc20 => arena
                .update_family_herding(
                    actor,
                    microseconds,
                    self.process_rng,
                    WeightedTargetConstructorNull::Process(&*self.factory_process),
                )
                .map_err(|e| WorldFamilyBehaviorError::owner("Herding descriptor update", e)),
            0x0043_faa0 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateObject {
                        microseconds,
                    },
                ),
            0x0043_fcc0 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateLanding {
                        microseconds,
                    },
                ),
            0x0043_fda0 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateRescue {
                        microseconds,
                    },
                ),
            0x0043_fe20 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateSpace {
                        microseconds,
                    },
                ),
            //Selector34's actual +F4 is 00433CC0, not common 004298A0.
            0x0043_5130 if matches!(profile, FamilyBehaviorProfile::Selector34) => arena
                .update_selector34_attack(
                    actor,
                    microseconds,
                    &mut crate::object_runtime::Selector34AttackContext {
                        rng: self.process_rng,
                        constructor_null: WeightedTargetConstructorNull::Process(
                            &*self.factory_process,
                        ),
                        settings_7c: self.settings_7c,
                    },
                )
                .map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual Selector34 +F4 attack update", e)
                }),
            0x0043_5130 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateAttack {
                        microseconds,
                    },
                ),
            0x0043_5170 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .selector35_descriptor(
                    arena,
                    actor,
                    crate::object_runtime::Selector35DescriptorRequest::UpdateEvade {
                        microseconds,
                    },
                ),
            address @ (0x0043_5170 | 0x0042_5d90)
                if matches!(profile, FamilyBehaviorProfile::Family4236d0 { .. }) =>
            {
                arena
                    .update_family_timed_descriptor(
                        actor,
                        crate::object_runtime::FamilyTimedDescriptorRequest {
                            kind: if address == 0x0043_5170 {
                                crate::object_runtime::FamilyTimedDescriptorKind::Point124
                            } else {
                                crate::object_runtime::FamilyTimedDescriptorKind::Retry130
                            },
                            microseconds,
                            settings_7c: self.settings_7c,
                        },
                        self.process_rng,
                    )
                    .map_err(|error| {
                        WorldFamilyBehaviorError::owner("common timed descriptor update", error)
                    })
            }
            0x0043_5240 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector35(
                        crate::object_runtime::Selector35LifecycleRequest::DyingUpdate,
                    ),
                ),
            0x0046_93a0 if matches!(profile, FamilyBehaviorProfile::Selector35) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector35(
                        crate::object_runtime::Selector35LifecycleRequest::DormantUpdate,
                    ),
                ),
            0x0043_5240 if matches!(profile, FamilyBehaviorProfile::Family4236d0 { .. }) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Family(
                        crate::object_runtime::FamilyDyingRequest::Update { microseconds },
                    ),
                ),
            0x0043_5240 if matches!(profile, FamilyBehaviorProfile::Selector34) => self
                .run_dying_descriptor(
                    arena,
                    actor,
                    family_dying::DyingDescriptor::Selector34(
                        crate::object_runtime::Selector34DyingRequest::Update { microseconds },
                    ),
                ),
            0x0043_5240 if matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) => self
                .selector12_dying(
                    arena,
                    actor,
                    crate::object_runtime::Selector12DyingRequest::Update,
                ),
            0x0043_5170 | 0x0042_5d90
                if matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) =>
            {
                //435170/425D90 call actual+124/+130. Final527058 selects404840
                //RET4: authenticate class custody, then read no scalar/service.
                arena.selector12_actor(actor).map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual Selector12+124 RET4 update", e)
                })?;
                Ok(())
            }
            0x0046_93a0 => {
                let mut terrain = WorldActorTerrainHost {
                    residency: self.residency,
                    source: self.terrain_source,
                    prepared: self.terrain,
                };
                arena
                    .dispatch_family_dormant(
                        actor,
                        FamilyDormantRequest::Update { microseconds },
                        &mut terrain,
                    )
                    .map_err(|e| WorldFamilyBehaviorError::owner("dormant descriptor update", e))
            }
            0x0046_9210 if matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) => arena
                .update_selector12_point_target(
                    actor,
                    crate::object_runtime::Selector12PointUpdateRequest {
                        microseconds,
                        settings_7c: self.settings_7c,
                        rng: &mut *self.process_rng,
                    },
                )
                .map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual Selector12+148 point update", e)
                }),
            0x0046_9310 if matches!(profile, FamilyBehaviorProfile::Selector12 { .. }) => {
                //The actual update owns fresh target/grant gates before14C.
                //Constructor-NULL24C notification binding bypasses H0/formatter;
                //missing later sources retain canonical live-grant error prefixes.
                let process = &*self.factory_process;
                let grants = crate::object_runtime::LiveGrantContext {
                    process: Some(process),
                    executable: Some(self.executable),
                    resident: Some(self.residency),
                    text_data: None,
                };
                let mut terrain = grant_grid::WorldGrantGridHost {
                    residency: self.residency,
                    executable: self.executable,
                    source: self.terrain_source,
                    prepared: self.terrain,
                    process,
                };
                arena
                    .update_selector12_preferred_target(
                        actor,
                        crate::object_runtime::Selector12PreferredUpdateRequest {
                            microseconds,
                            live_grants: &grants,
                        },
                        &mut terrain,
                    )
                    .map_err(|e| {
                        WorldFamilyBehaviorError::owner("actual Selector12+13C preferred update", e)
                    })
            }
            0x0040_26d0 => Ok(()), //Complete RET8 no-op; receiver/arguments unused.
            0x0042_5e20 => {
                arena.update_family_stay_still_target(actor)?;
                Ok(())
            }
            0x0043_fbc0 => {
                let mut terrain = WorldActorTerrainHost {
                    residency: self.residency,
                    source: self.terrain_source,
                    prepared: self.terrain,
                };
                arena
                    .update_family_roam_target(
                        actor,
                        FamilyRoamUpdateRequest {
                            microseconds,
                            sine_quarter: self.sine_quarter,
                        },
                        self.process_rng,
                        &mut terrain,
                    )
                    .map_err(|e| WorldFamilyBehaviorError::owner("00427800 Roam update", e))
            }
            0x0043_faa0 if !matches!(profile, FamilyBehaviorProfile::Selector35) => {
                arena.update_family_object_target(actor).map_err(|e| {
                    WorldFamilyBehaviorError::owner("actual virtual+EC object-target update", e)
                })
            }
            0x0043_5130 if !matches!(profile, FamilyBehaviorProfile::Selector35) => {
                let mut host = AttackUpdateBridge {
                    process: self.factory_process,
                };
                arena
                    .update_family_attack(
                        actor,
                        FamilyAttackUpdateRequest {
                            microseconds,
                            settings_7c: self.settings_7c,
                        },
                        self.process_rng,
                        &mut host,
                    )
                    .map_err(|e| {
                        WorldFamilyBehaviorError::owner("actual virtual+F4 attack update", e)
                    })
            }
            address => Err(WorldFamilyBehaviorError::required(
                actor,
                address,
                "descriptor update",
            )),
        }
    }

    fn occasional_sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error> {
        self.sound(arena, actor, slot)
    }
}

impl FamilyTargetProducerHost for WorldFamilyBehaviorHost<'_, '_> {
    type Error = WorldFamilyBehaviorError;
    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(self.process_rng))
    }
}

impl FamilyAttackHost for WorldFamilyBehaviorHost<'_, '_> {
    type Error = WorldFamilyBehaviorError;
    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(self.process_rng))
    }
    fn attack_sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error> {
        self.sound(arena, actor, slot)
    }
}

//Attack update owns RNG directly; this short bridge lends only the actual
//process getter and reached helper service, preventing a second RNG loan.
struct AttackUpdateBridge<'a> {
    process: &'a ParticleFactoryProcessState,
}
impl TargetNullHandleSource for AttackUpdateBridge<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.process.process_null_handle
    }
}
impl FamilyAttackUpdateHost for AttackUpdateBridge<'_> {
    type Error = WorldFamilyBehaviorError;
    fn record_attack_helper(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyHelperRecordRequest,
    ) -> Result<(), Self::Error> {
        arena
            .record_family_helper_history(
                actor,
                request,
                FamilyHelperHistoryInputs {
                    process_null_handle: self.process.process_null_handle,
                },
            )
            .map_err(|e| WorldFamilyBehaviorError::owner("004830B0 helper history", e))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_guards;
