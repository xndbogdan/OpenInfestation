//! Current world construction inputs shared by intro helpers and the camera.
//! Pre-context allocation WORDs retain the named fresh-heap approximation.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    fresh_heap::fresh_allocation_words,
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, CameraFactoryInputs, CameraSettings,
        CreationListener, CreationListenerEffects, CreationListenerError, CreationListeners,
        ObjectArena, ObjectId,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::TerrainRuntime,
    ResidentRuntime,
};

mod child;
pub(crate) use child::{
    placement_request, BehaviorResourceFactory, BehaviorResourceOwners, WorldActorChildHost,
    WorldActorFactoryRequest, WorldFactoryInitialization, WorldFactoryPosition,
};
mod pickup;
pub(crate) use pickup::WorldPickupHost;

/// Live owners for one reached 00478E00 call. The renderer's material snapshot
/// is separate from this current factory residency. The resource clock is
/// captured once by the caller; acquisition never advances it.
#[derive(Clone, Copy)]
pub struct WorldCameraInputs<'a> {
    pub residency: &'a ResidentRuntime,
    pub sine_quarter: &'a [u16; 4096],
    pub settings: CameraSettings,
    pub process: Option<&'a ParticleFactoryProcessState>,
    pub animation_clock: u32,
}

/// Scoped preparation shared by Tick and predraw. It owns the math/listener
/// adapters while the existing factory borrows actual terrain and listeners.
pub(crate) struct WorldCameraFactoryContext<'a> {
    inputs: WorldCameraInputs<'a>,
    math: NativeBasisMath,
    listener_effects: RequiredListenerEffects,
}

impl<'a> WorldCameraFactoryContext<'a> {
    pub(crate) fn new(inputs: WorldCameraInputs<'a>) -> Self {
        Self {
            math: NativeBasisMath::new(*inputs.sine_quarter),
            inputs,
            listener_effects: RequiredListenerEffects,
        }
    }

    pub(crate) fn factory<'b>(
        &'b mut self,
        terrain: &'b TerrainRuntime,
        world_flags: u32,
        listeners: &'b mut CreationListeners,
    ) -> CameraFactoryInputs<'b> {
        CameraFactoryInputs {
            process: self.inputs.process,
            runtime: self.inputs.residency,
            math: &self.math,
            allocation: fresh_world_allocation(
                terrain,
                self.inputs.animation_clock,
                world_flags,
                self.inputs.sine_quarter,
                match self.inputs.process {
                    Some(process) => ActorConstructorNull::Process(process),
                    None => ActorConstructorNull::RegistrySnapshotApproximation,
                },
            ),
            local_owner: self.inputs.process.map_or(1, |process| process.local_slot),
            settings: self.inputs.settings,
            listeners,
            listener_effects: &mut self.listener_effects,
            rng: None,
        }
    }
}

/// Factory admission borrows the current terrain only while the reached
/// factory executes. Retirement may mutate it before a later helper/camera.
pub(super) fn fresh_world_allocation<'a>(
    terrain: &'a TerrainRuntime,
    clock: u32,
    world_flags: u32,
    sine: &'a [u16; 4096],
    constructor_null: ActorConstructorNull<'a>,
) -> ActorConstructionRequest<'a> {
    ActorConstructionRequest {
        constructor_null,
        allocation_position: Some(fresh_allocation_words()),
        // 0055F2F4, the slot new handles take.
        owner: match constructor_null {
            ActorConstructorNull::Process(process) => process.local_slot,
            ActorConstructorNull::RegistrySnapshotApproximation => 1,
        },
        terrain: crate::object_runtime::ActorTerrainInputs {
            terrain: Some(crate::object_runtime::ActorTerrainSurface {
                terrain,
                plane_height_fixed: terrain.plane_base(),
            }),
            coordinate_shift: 8,
            clock,
            world_flags: Some(world_flags),
            sine_quarter: sine,
        },
    }
}

pub(crate) struct RequiredListenerEffects;
impl CreationListenerEffects for RequiredListenerEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        _: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        match listener.interface {
            // 00477350: the Force Field marks the new actor.
            crate::object_runtime::CreationListenerInterface::Selector18 => {
                Ok(arena.selector18_listener(listener.owner, actor)?)
            }
            // 00407350: the ending director watches for its bug.
            crate::object_runtime::CreationListenerInterface::Selector78 => {
                Ok(arena.selector78_listener(listener.owner, actor)?)
            }
        }
    }
}
