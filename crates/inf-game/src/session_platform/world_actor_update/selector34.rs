//! Production selector34 post-D8 continuation of 00434210: controls
//! 0043429C..004349D3, motion +118 00434C10, then the engine audio suffix.
//! Controls share Selector35's clearance adapter over the retained world.

use super::selector35::{WorldSelector35ClearanceError, WorldSelector35ClearanceHost};
use crate::{
    object_runtime::{
        construct_selector67_ribbon, ActorConstructionError, ActorHandle, ActorWeaponFireContext,
        CameraFactoryInputs, ObjectArena, ObjectError, ObjectId, PlayerAttachError,
        Selector34ControlsError, Selector34ControlsRequest, Selector34MotionError,
        Selector34MotionHost, Selector34MotionRequest, Selector34PersistentAudioError,
        Selector67Error, Selector67FactoryError, Selector67FactoryRequest, Selector67Host,
    },
    process_sound::{ProcessSoundRuntime, ProcessSoundVoiceTeardown},
};
use inf_formats::FrontendExecutable;
use thiserror::Error;

/// The selector67 ribbon 005286D8 behind Selector34 motion: 00486600
/// creates it into +25C, then 004869B0 drives it every frame. The world
/// pass's own +24 004866A0 uses the same services.
pub(crate) struct WorldRibbonHost<'a> {
    pub factory: Selector67FactoryRequest<'a>,
    pub executable: FrontendExecutable<'a>,
}

#[derive(Debug, Error)]
pub enum WorldRibbonServiceError {
    #[error(transparent)]
    Factory(#[from] Selector67FactoryError),
    #[error(transparent)]
    Handle(#[from] ActorConstructionError),
}

#[derive(Debug, Error)]
pub enum WorldSelector34RibbonError {
    #[error(transparent)]
    Create(#[from] Selector67FactoryError),
    #[error(transparent)]
    Ribbon(#[from] Selector67Error<WorldRibbonServiceError>),
    #[error("selector34 ship {0:?} has no +32 position for 004869B0")]
    ShipPosition(ObjectId),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

impl Selector67Host for WorldRibbonHost<'_> {
    type Error = WorldRibbonServiceError;

    fn split_ribbon(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        let source = self
            .factory
            .process
            .process_null_handle
            .ok_or(Selector67FactoryError::MissingProcessNull)?;
        Ok(construct_selector67_ribbon(arena, &self.factory, source)?)
    }

    fn owner_resolves(
        &mut self,
        arena: &mut ObjectArena,
        owner: ActorHandle,
    ) -> Result<bool, Self::Error> {
        Ok(arena.resolve_actor_handle(owner)?.is_some())
    }
}

impl Selector34MotionHost for WorldRibbonHost<'_> {
    type Error = WorldSelector34RibbonError;

    fn create_trail(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Ok(construct_selector67_ribbon(arena, &self.factory, source)?)
    }

    /// 00434EBC passes the ship's live +32.
    fn update_trail(
        &mut self,
        arena: &mut ObjectArena,
        trail: ObjectId,
        microseconds: u32,
        position_owner: ObjectId,
    ) -> Result<(), Self::Error> {
        let point = arena
            .object(position_owner)?
            .base
            .position_words
            .ok_or(WorldSelector34RibbonError::ShipPosition(position_owner))?;
        let executable = self.executable;
        arena.update_selector67_from_ship(trail, microseconds, point, executable, self)?;
        Ok(())
    }
}

pub(crate) struct WorldSelector34TailContext<'a, 'exe, 'camera> {
    pub microseconds: u32,
    pub settings_7c: u32,
    pub executable: FrontendExecutable<'exe>,
    pub sine: &'a [u16; 4096],
    pub inverse_sine: &'a [i16; 1024],
    pub clearance: WorldSelector35ClearanceHost<'a>,
    pub firing: ActorWeaponFireContext<'a>,
    pub camera: &'a mut CameraFactoryInputs<'camera>,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub ribbon: Selector67FactoryRequest<'a>,
}

#[derive(Debug, Error)]
pub enum WorldSelector34TailError {
    #[error("selector34 target/clearance/firing controls failed: {0}")]
    Controls(Selector34ControlsError<WorldSelector35ClearanceError>),
    #[error("selector34 motion failed: {0}")]
    Motion(Selector34MotionError<WorldSelector34RibbonError>),
    #[error("selector34 engine audio suffix failed: {0}")]
    Audio(Selector34PersistentAudioError<PlayerAttachError>),
}

pub(crate) fn tick_world_selector34_tail(
    arena: &mut ObjectArena,
    actor: ObjectId,
    mut context: WorldSelector34TailContext<'_, '_, '_>,
) -> Result<(), WorldSelector34TailError> {
    let mut controls = arena
        .tick_selector34_controls(
            actor,
            Selector34ControlsRequest {
                microseconds: context.microseconds,
                settings_7c: context.settings_7c,
                firing: context.firing,
                clearance: &mut context.clearance,
            },
        )
        .map_err(WorldSelector34TailError::Controls)?;
    let mut ribbon = WorldRibbonHost {
        factory: context.ribbon,
        executable: context.executable,
    };
    arena
        .tick_selector34_motion(
            actor,
            Selector34MotionRequest {
                microseconds: context.microseconds,
                controls: &mut controls,
                executable: context.executable,
                sine_quarter: context.sine,
                inverse_sine: context.inverse_sine,
            },
            &mut ribbon,
        )
        .map_err(WorldSelector34TailError::Motion)?;
    arena
        .tick_selector34_persistent_audio(
            actor,
            context.camera,
            context.clearance.residency,
            context.sounds,
            context.voices,
        )
        .map_err(WorldSelector34TailError::Audio)
}
