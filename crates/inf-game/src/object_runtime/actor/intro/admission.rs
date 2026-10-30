//! Native intro factory services, 00462750 and the shared 00478E00 camera.
//! No allocation bytes, settings, clock or actor callbacks have defaults.

use inf_formats::native_basis::NativeBasisMath;
use thiserror::Error;

use super::ArenaIntroServices;
use crate::object_runtime::{
    construct_world_object, ActorConstructionRequest, ActorFactoryInputs, BaseContext,
    CameraFactoryInputs, CameraSettings, CreationListenerEffects, CreationListeners, ObjectArena,
    ObjectConstructionError, ObjectConstructionOutcome, ObjectConstructionRequest, ObjectId,
    PlayerAttachError,
};
use crate::ResidentRuntime;

/// Recovered admission dependencies. In particular, +32 allocation bytes and
/// the actor terrain clock precede payload/context writes and stay explicit.
pub struct IntroAdmissionInputs<'a> {
    pub process: Option<&'a crate::process_particles::factories::ParticleFactoryProcessState>,
    pub runtime: &'a ResidentRuntime,
    pub math: &'a NativeBasisMath,
    pub helper_allocation: ActorConstructionRequest<'a>,
    pub camera_allocation: ActorConstructionRequest<'a>,
    pub local_owner: u8,
    pub camera_settings: CameraSettings,
    pub network_active: bool,
    pub network_world_active: bool,
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
}

#[derive(Debug, Error)]
pub enum IntroAdmissionError {
    #[error(transparent)]
    Construction(#[from] ObjectConstructionError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
}

impl IntroAdmissionInputs<'_> {
    /// Complete 00462750 factory phase. Keeping this borrow scoped to one
    /// admission lets later intro callbacks mutate the retained world terrain.
    pub fn create_helper(
        &mut self,
        arena: &mut ObjectArena,
    ) -> Result<Option<ObjectId>, IntroAdmissionError> {
        // Shared commands resolve the current first player at their native
        // point. Copy the live grant value solely to end the arena read borrow
        // before admission; the factory itself owns its mutation/listener walk.
        let grants = arena
            .first_player()
            .map(|player| arena.world_player(player).map(|player| player.grants_9c))
            .transpose()?;
        let outcome = construct_world_object(
            arena,
            self.runtime,
            ObjectConstructionRequest {
                // Retail 00462750 loads EDX=8E before 004600F0 mode zero.
                resource_argument: 142,
                context: BaseContext {
                    position_words: None,
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: self.network_active,
                actor: Some(ActorFactoryInputs {
                    allocation: self.helper_allocation,
                    rotation_words: None,
                    local_owner: self.local_owner,
                    first_player_grants: grants.as_ref(),
                    grant_table: None,
                    math: self.math,
                    derived: None,
                    listeners: self.listeners,
                    listener_effects: self.listener_effects,
                    network_world_active: self.network_world_active,
                    terrain: None,
                    world_start: None,
                    camera_settings: None,
                    spawn_mode_1: false,
                    network_handle: None,
                    vehicle_table: None,
                }),
            },
        )?;
        Ok(match outcome {
            ObjectConstructionOutcome::Constructed(helper) => Some(helper),
            ObjectConstructionOutcome::Rejected { .. } => None,
        })
    }

    /// Complete lazy 00478E00 camera phase. An existing live camera is reused
    /// before any allocation/listener admission is required by world_camera.
    pub fn acquire_camera(
        &mut self,
        arena: &mut ObjectArena,
    ) -> Result<Option<ObjectId>, IntroAdmissionError> {
        Ok(arena.world_camera(&mut self.camera_inputs())?)
    }

    /// 0046AE90: `player` takes the actor whose handle this is, the camera
    /// built on demand.
    pub fn take_player(
        &mut self,
        arena: &mut ObjectArena,
        player: crate::object_runtime::PlayerId,
        handle: crate::object_runtime::ActorHandle,
    ) -> Result<bool, PlayerAttachError> {
        arena.attach_player(player, handle, &mut self.camera_inputs())
    }

    fn camera_inputs(&mut self) -> CameraFactoryInputs<'_> {
        CameraFactoryInputs {
            process: self.process,
            runtime: self.runtime,
            math: self.math,
            allocation: self.camera_allocation,
            local_owner: self.local_owner,
            settings: self.camera_settings,
            listeners: self.listeners,
            listener_effects: self.listener_effects,
            rng: None,
        }
    }
}

/// Callbacks outside factory admission. Implementors must execute the native
/// operation or fail at that feature; missing tasks never succeed by default.
pub trait IntroCallbackHost {
    type Error;

    fn retire_actor(&mut self, arena: &mut ObjectArena, actor: ObjectId)
        -> Result<(), Self::Error>;
    fn world_present(&mut self) -> Result<bool, Self::Error>;
    fn complete_mission(
        &mut self,
        arena: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), Self::Error>;
}

pub struct NativeIntroServices<'a, H: IntroCallbackHost> {
    pub admission: IntroAdmissionInputs<'a>,
    pub callbacks: &'a mut H,
}

#[derive(Debug, Error)]
pub enum NativeIntroServicesError<E> {
    #[error(transparent)]
    Admission(#[from] IntroAdmissionError),
    #[error("native intro callback remains unavailable")]
    Callback(E),
}

impl<H: IntroCallbackHost> crate::object_runtime::TargetNullHandleSource
    for NativeIntroServices<'_, H>
{
    fn target_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        self.admission
            .process
            .and_then(|process| process.process_null_handle)
    }
}

impl<H: IntroCallbackHost> ArenaIntroServices for NativeIntroServices<'_, H> {
    type Error = NativeIntroServicesError<H::Error>;

    fn create_helper(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        self.admission
            .create_helper(arena)
            .map_err(NativeIntroServicesError::Admission)
    }

    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        self.admission
            .acquire_camera(arena)
            .map_err(NativeIntroServicesError::Admission)
    }

    fn retire_actor(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error> {
        self.callbacks
            .retire_actor(arena, actor)
            .map_err(NativeIntroServicesError::Callback)
    }

    fn world_present(&mut self) -> Result<bool, Self::Error> {
        self.callbacks
            .world_present()
            .map_err(NativeIntroServicesError::Callback)
    }

    fn complete_mission(
        &mut self,
        arena: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), Self::Error> {
        self.callbacks
            .complete_mission(arena, selection, start_ordinal, argument)
            .map_err(NativeIntroServicesError::Callback)
    }
}
