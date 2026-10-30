//! Live47AAF0 factory services. The parent position stays an arena identity
//! through allocation; classification, source and basis belong to the caller's
//! normal-return suffix. Constructor carry uses the existing named policy.

use inf_formats::{
    native_basis::{NativeBasis, NativeBasisMath},
    FrontendExecutable,
};

use crate::{
    object_runtime::{
        construct_behavior_resource, construct_world_object_from_source,
        resident_constructor_selector, ActorChildFactoryCall, ActorChildHost, ActorChildResource,
        ActorFactoryInputs, BaseContext, CameraSettings, ConstructionFailurePolicy,
        ConstructionPositionSource, CreationListenerEffects, CreationListeners,
        DerivedActorFactoryInputs, ObjectArena, ObjectClass, ObjectConstructionOutcome,
        ObjectConstructionRequest, ObjectId, Selector36ChildServiceError, VehicleFactoryInputs,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::TerrainRuntime,
    world_start::{WorldStartContext, WorldStartState},
    ResidentRuntime,
};

pub(crate) struct WorldActorChildHost<'a, 'exe> {
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub process: &'a ParticleFactoryProcessState,
    pub terrain: &'a TerrainRuntime,
    pub world_flags: u32,
    pub clock: u32,
    pub sine: &'a [u16; 4096],
    pub math: &'a NativeBasisMath,
    /// The camera settings selector 16's constructor and the shot's
    /// camera read; a factory without them refuses both.
    pub camera_settings: Option<CameraSettings>,
    /// The world start selector 30's constructor reads; a factory without
    /// it (the lifecycle's) refuses that class with MissingWorldStart.
    pub world_start: Option<&'a mut WorldStartState>,
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
}

/// Position custody differs at the native callers:47AAF0 passes its parent's
/// live words; death/pickup producers pass their local stack words.
pub(crate) enum WorldFactoryPosition {
    Parent(ObjectId),
    Local([u16; 3]),
    /// A NULL position (a remote actor built from its resource): +32 keeps
    /// the allocation's residue, the fresh heap's words.
    Residue,
}

pub(crate) enum WorldFactoryInitialization<'a> {
    /// 0047AAF0 and particle/pickup producers supply NULL rotation/commands.
    Bare,
    /// 00480610 supplies captured H14 commands and late-read orientation;
    /// `0048E170` the placement's index, which 00480610 gives as -1.
    Placement {
        placement: u32,
        rotation: [u16; 3],
        suffix_offset: u32,
        instances: &'a crate::world_loader::WorldInstanceAllocation,
    },
}

pub(crate) struct WorldActorFactoryRequest<'a> {
    pub resource: ActorChildResource,
    pub position: WorldFactoryPosition,
    pub spawn_mode_1: bool,
    /// 00460310 mode 2 with this handle, refused unless 00462480 admits it.
    pub network_handle: Option<crate::object_runtime::ActorHandle>,
    pub initialization: WorldFactoryInitialization<'a>,
}

/// `0048E170`'s request for placement `index` of the current world: its
/// resource at its position raised to the ground and its angles, with its
/// commands (its `004EE1E0` asks for the world's leaf, resident since the
/// load). A `network_handle` builds in mode 2 for its machine, none in mode
/// 0. None past the count, where it builds nothing.
pub(crate) fn placement_request<'i>(
    instances: &'i crate::world_loader::WorldInstanceAllocation,
    terrain: &TerrainRuntime,
    index: u32,
    network_handle: Option<crate::object_runtime::ActorHandle>,
) -> Result<Option<WorldActorFactoryRequest<'i>>, String> {
    if index >= instances.placement_count()? {
        return Ok(None);
    }
    let placement = instances.placement_offset(index + 1)?;
    let allocation = instances.allocation.runtime_allocation();
    let word = |offset: u32| {
        allocation
            .word(placement + offset)
            .map_err(|error| error.to_string())
    };
    let mut position = [word(4)?, word(6)?, word(8)?];
    position[1] = position[1].wrapping_add(terrain.height_at([position[0], position[2]]) as u16);
    Ok(Some(WorldActorFactoryRequest {
        resource: ActorChildResource::Full(
            allocation
                .dword(placement)
                .map_err(|error| error.to_string())?,
        ),
        position: WorldFactoryPosition::Local(position),
        spawn_mode_1: false,
        network_handle,
        initialization: WorldFactoryInitialization::Placement {
            placement: index,
            rotation: [word(0xe)?, word(0xa)?, word(0xc)?],
            suffix_offset: placement + 0x10,
            instances,
        },
    }))
}

impl WorldActorChildHost<'_, '_> {
    pub(crate) fn create_object(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldActorFactoryRequest<'_>,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Selector36ChildServiceError> {
        //47AB1D captures this before460310, including resource rejection.
        //Modes0/1 do not otherwise consume the captured argument in4600D0.
        self.process
            .process_null_handle
            .ok_or("47AAF0 child factory requires current process null handle0054E894")?;
        let slot = match request.resource {
            ActorChildResource::Full(value) => value as u16,
            ActorChildResource::StackPositionLowWord(value)
            | ActorChildResource::RegisterLowWord(value) => value,
        };
        let capacity = self.residency.resource_capacity(13).unwrap_or(0);
        if (slot as i16) < 0 || usize::from(slot) > capacity {
            return Ok(None);
        }
        if usize::from(slot) == capacity {
            return Err(
                format!("460310 child H13 one-past read for {:?}", request.resource).into(),
            );
        }
        if self.residency.resource(13, slot).is_none() {
            return Err(format!(
                "460310 child requires H13 lazy supplier for {:?}",
                request.resource,
            )
            .into());
        }
        if let Some(handle) = request.network_handle {
            if !arena
                .actor_handle_mode2_admissible(handle)
                .map_err(|error| error.to_string())?
            {
                return Ok(None);
            }
        }
        //The resident460120 route uses signedBX for its repeated H13 lookup;
        //virtual+C receives H13's model pointer/payload, not resource EDX.
        //4602D0->479E10 consumes only the LOWWORD at+30. Symbolic stack high
        //bits can therefore be lowered here, never at the lazy supplier above.
        let resource_argument = match request.resource {
            ActorChildResource::Full(value) => value,
            ActorChildResource::StackPositionLowWord(value)
            | ActorChildResource::RegisterLowWord(value) => u32::from(value),
        };
        let selector = resident_constructor_selector(arena, self.residency, resource_argument)
            .map_err(|error| error.to_string())?;
        // The Pursuit Ship (61) takes only the process RNG from these inputs.
        let derived_limits = if matches!(
            selector,
            2 | 7 | 10 | 11 | 12 | 33 | 34 | 35 | 36 | 60 | 61 | 77 | 80
        ) {
            let world = self
                .residency
                .resource(14, 0)
                .ok_or("child factory requires H14/0")?;
            let region = world
                .overlay
                .layout()
                .regions
                .get(world.registration.region_index)
                .filter(|region| region.handler.get() == 14)
                .ok_or("child factory H14 registration is inconsistent")?;
            let world_bank = inf_formats::parse_world_bank(world.overlay.bytes(), region)
                .map_err(|error| error.to_string())?;
            Some(
                world_bank
                    .entries
                    .get(usize::from(world.registration.local_index))
                    .ok_or("child factory H14 world entry is absent")?
                    .actor_mask_limits(),
            )
        } else {
            None
        };
        let grants = (selector == 12)
            .then(|| inf_formats::parse_grant_table(self.executable))
            .transpose()
            .map_err(|error| error.to_string())?;
        let vehicle = (selector == 2)
            .then(|| inf_formats::parse_vehicle_behavior_table(self.executable))
            .transpose()
            .map_err(|error| error.to_string())?;
        let first_grants = if selector == 12 {
            arena
                .first_player()
                .map(|player| arena.world_player(player).map(|player| player.grants_9c))
                .transpose()
                .map_err(|error| error.to_string())?
        } else {
            None
        };
        let allocation = super::fresh_world_allocation(
            self.terrain,
            self.clock,
            self.world_flags,
            self.sine,
            crate::object_runtime::ActorConstructorNull::Process(self.process),
        );
        let (placement_index, rotation_words, suffix, command_allocation) =
            match request.initialization {
                WorldFactoryInitialization::Bare => (u32::MAX, None, None, None),
                WorldFactoryInitialization::Placement {
                    placement,
                    rotation,
                    suffix_offset,
                    instances,
                } => (
                    placement,
                    Some(rotation),
                    Some(
                        instances
                            .allocation
                            .commands(instances.suffix(suffix_offset)?),
                    ),
                    Some(&instances.allocation),
                ),
            };
        let outcome = construct_world_object_from_source(
            arena,
            self.residency,
            ObjectConstructionRequest {
                resource_argument,
                context: BaseContext {
                    position_words: match request.position {
                        WorldFactoryPosition::Parent(_) => None,
                        WorldFactoryPosition::Local(position) => Some(position),
                        WorldFactoryPosition::Residue => {
                            Some(crate::fresh_heap::fresh_allocation_words())
                        }
                    },
                    placement_index,
                },
                suffix,
                command_allocation,
                // 00460288 reads 0055F448 for the completion's network tail.
                network_active: self.process.network_flag != 0,
                actor: Some(ActorFactoryInputs {
                    allocation,
                    rotation_words,
                    local_owner: self.process.local_slot,
                    first_player_grants: first_grants.as_ref(),
                    grant_table: grants.as_ref(),
                    math: self.math,
                    derived: derived_limits.map(|world_mask_limits| DerivedActorFactoryInputs {
                        world_mask_limits,
                        rng,
                    }),
                    listeners: self.listeners,
                    listener_effects: self.listener_effects,
                    network_world_active: self.process.host_flag != 0,
                    terrain: Some(self.terrain),
                    world_start: self
                        .world_start
                        .as_deref_mut()
                        .map(|state| WorldStartContext {
                            state,
                            runtime: self.residency,
                            terrain: self.terrain,
                        }),
                    camera_settings: self.camera_settings,
                    spawn_mode_1: request.spawn_mode_1,
                    network_handle: request.network_handle,
                    vehicle_table: vehicle.as_ref().map(|table| VehicleFactoryInputs {
                        table,
                        network_settings: (self.process.network_flag != 0)
                            .then_some(&self.process.settings),
                    }),
                }),
            },
            match request.position {
                WorldFactoryPosition::Parent(parent) => {
                    ConstructionPositionSource::ActorPosition(parent)
                }
                WorldFactoryPosition::Local(_) | WorldFactoryPosition::Residue => {
                    ConstructionPositionSource::Context
                }
            },
            ConstructionFailurePolicy::RetainOnError,
        )
        .map_err(|error| match error {
            crate::object_runtime::ObjectConstructionError::Retained { object, source } => {
                Selector36ChildServiceError::RetainedConstruction {
                    object,
                    reason: source.to_string(),
                }
            }
            other => Selector36ChildServiceError::Owner(other.to_string()),
        })?;
        Ok(match outcome {
            ObjectConstructionOutcome::Constructed(object) => {
                eprintln!(
                    "World dynamic object: object={object:?} resource={:?}",
                    request.resource
                );
                Some(object)
            }
            ObjectConstructionOutcome::Rejected { .. } => None,
        })
    }
}

/// 00460310 as 0048D8D3 calls it, lent to the behavior cache by owners
/// that hold the world factory.
pub(crate) trait BehaviorResourceFactory {
    /// The registry's signed resource WORD; None is the factory's NULL.
    fn create_behavior_resource(
        &mut self,
        arena: &mut ObjectArena,
        resource: i16,
    ) -> Result<Option<ObjectId>, String>;
}

/// The owners 00460310 reads for a registry resource. The basis math is
/// built only when a creation is reached (once per behavior row).
pub(crate) struct BehaviorResourceOwners<'a> {
    pub residency: &'a ResidentRuntime,
    /// The process null 0054E894 and network DWORD 0055F448, as current.
    pub process: ParticleFactoryProcessState,
    pub terrain: &'a TerrainRuntime,
    pub world_flags: u32,
    pub clock: u32,
    pub sine: &'a [u16; 4096],
    pub listeners: &'a mut CreationListeners,
}

impl BehaviorResourceFactory for BehaviorResourceOwners<'_> {
    /// ECX's low WORD is the resource (its high half is the process null's
    /// residue; every reader takes the WORD). EDX is the NULL command list;
    /// the stack passes the unwritten eight-byte position, rotation NULL,
    /// placement -1, factory mode 0 and the process null.
    fn create_behavior_resource(
        &mut self,
        arena: &mut ObjectArena,
        resource: i16,
    ) -> Result<Option<ObjectId>, String> {
        // 0046031B..0046034E: a negative or above-count WORD is NULL; the
        // count itself reads one past the table.
        if resource < 0 {
            return Ok(None);
        }
        let slot = resource as u16;
        let capacity = self.residency.resource_capacity(13).unwrap_or(0);
        if usize::from(slot) > capacity {
            return Ok(None);
        }
        if usize::from(slot) == capacity {
            return Err(format!(
                "460310 behavior resource H13 one-past read for {slot}"
            ));
        }
        if self.residency.resource(13, slot).is_none() {
            return Err(format!(
                "460310 behavior resource {slot} needs the H13 lazy supplier"
            ));
        }
        let math = NativeBasisMath::new(*self.sine);
        let allocation = super::fresh_world_allocation(
            self.terrain,
            self.clock,
            self.world_flags,
            self.sine,
            crate::object_runtime::ActorConstructorNull::Process(&self.process),
        );
        let outcome = construct_behavior_resource(
            arena,
            self.residency,
            ObjectConstructionRequest {
                resource_argument: u32::from(slot),
                context: BaseContext {
                    position_words: None,
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: self.process.network_flag != 0,
                actor: Some(ActorFactoryInputs {
                    allocation,
                    rotation_words: None,
                    local_owner: self.process.local_slot,
                    first_player_grants: None,
                    grant_table: None,
                    math: &math,
                    derived: None,
                    listeners: self.listeners,
                    listener_effects: &mut super::RequiredListenerEffects,
                    network_world_active: self.process.host_flag != 0,
                    terrain: Some(self.terrain),
                    world_start: None,
                    camera_settings: None,
                    spawn_mode_1: false,
                    network_handle: None,
                    vehicle_table: None,
                }),
            },
            ConstructionFailurePolicy::RetainOnError,
        )
        .map_err(|error| error.to_string())?;
        Ok(match outcome {
            ObjectConstructionOutcome::Constructed(object) => Some(object),
            ObjectConstructionOutcome::Rejected { .. } => None,
        })
    }
}

impl ActorChildHost for WorldActorChildHost<'_, '_> {
    type Error = Selector36ChildServiceError;

    fn create_child_object(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorChildFactoryCall,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.create_object(
            arena,
            WorldActorFactoryRequest {
                resource: request.resource,
                position: WorldFactoryPosition::Parent(request.parent),
                spawn_mode_1: request.spawn_mode_1,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            rng,
        )
    }

    fn resolve_child_actor(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        //Actual+18 returns self only for actors; typed nonactors bind401300.
        Ok(
            match &arena.object(raw).map_err(|error| error.to_string())?.class {
                ObjectClass::Actor(_) => Some(raw),
                ObjectClass::Base
                | ObjectClass::Selector81(_)
                | ObjectClass::Selector94(_)
                | ObjectClass::Selector75(_)
                | ObjectClass::Selector86(_)
                | ObjectClass::Selector67(_)
                | ObjectClass::MapAnnotation(_)
                | ObjectClass::Visual(_)
                | ObjectClass::Selector22(_)
                | ObjectClass::Selector65(_)
                | ObjectClass::Selector50(_)
                | ObjectClass::Selector48(_)
                | ObjectClass::Selector43(_)
                | ObjectClass::Selector42(_)
                | ObjectClass::Selector64(_)
                | ObjectClass::RotatingVisual(_)
                | ObjectClass::Selector45(_)
                | ObjectClass::Selector46(_)
                | ObjectClass::Race(_)
                | ObjectClass::Selector58(_)
                | ObjectClass::Selector83(_)
                | ObjectClass::Selector76(_)
                | ObjectClass::TimedPickup(_)
                | ObjectClass::TerrainLight { .. }
                | ObjectClass::RotatingTerrainLight { .. }
                | ObjectClass::RouteNode { .. } => None,
            },
        )
    }

    fn child_source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<NativeBasis>, Self::Error> {
        //The shooters' final tables bind +48 to 4026B0 (`lea eax,[ecx+0x44]`):
        //selectors 2 (520860), 10 and its 21 (520CC8, 51F7A8), 11 (51F080),
        //12 (527058), 35 (52273C) and 36 (527EE0), and so do the eyes, 77
        //(51FBB0) and 79 (51FC8C), whose death leaves a child (00407E70),
        //and the factory 15 (521940), whose bought items are its children.
        //Other parent tables must establish their own virtual policy first.
        let admitted = arena.selector2_actor(parent).is_ok()
            || arena.selector15(parent).is_ok()
            || arena.selector10_actor(parent).is_ok()
            || arena.selector11_actor(parent).is_ok()
            || arena.selector12_actor(parent).is_ok()
            || arena.selector35_actor(parent).is_ok()
            || arena.selector36_actor(parent).is_ok()
            || arena.selector77_actor(parent).is_ok();
        if !admitted {
            return Err(
                format!("parent {parent:?}'s final table has no admitted virtual +48").into(),
            );
        }
        Ok(Some(
            arena
                .actor(parent)
                .map_err(|error| error.to_string())?
                .rotating
                .basis,
        ))
    }

    fn refresh_child_parent_model(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<(), Self::Error> {
        arena
            .refresh_visual_model(parent, self.residency)
            .map_err(|error| error.to_string().into())
    }

    fn create_registry_object(
        &mut self,
        arena: &mut ObjectArena,
        resource: u16,
    ) -> Result<Option<ObjectId>, Self::Error> {
        BehaviorResourceOwners {
            residency: self.residency,
            process: *self.process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            clock: self.clock,
            sine: self.sine,
            listeners: &mut *self.listeners,
        }
        .create_behavior_resource(arena, resource as i16)
        .map_err(Into::into)
    }

    fn take_shot(
        &mut self,
        arena: &mut ObjectArena,
        player: crate::object_runtime::PlayerId,
        shot: crate::object_runtime::ActorHandle,
    ) -> Result<bool, Self::Error> {
        let mut camera = super::WorldCameraFactoryContext::new(super::WorldCameraInputs {
            residency: self.residency,
            sine_quarter: self.sine,
            settings: self
                .camera_settings
                .ok_or("0048CAF9 0046AE90 needs the camera settings")?,
            process: Some(&*self.process),
            animation_clock: self.clock,
        });
        let mut factory = camera.factory(self.terrain, self.world_flags, &mut *self.listeners);
        arena
            .attach_player(player, shot, &mut factory)
            .map_err(|error| format!("0048CAF9 0046AE90: {error}").into())
    }
}
