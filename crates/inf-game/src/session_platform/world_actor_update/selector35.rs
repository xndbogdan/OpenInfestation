//! Production selector35 post-D8 continuation over the retained world owners.
//! The intrinsic owns terrain sampling and sparse-cell geometry; this adapter
//! lends the current H12 and resolves actual H11/H8 resources only when reached.

use crate::{
    object_runtime::{
        ActorTerrainHost, ActorWeaponFireContext, CameraFactoryInputs, ObjectArena, ObjectId,
        PlayerAttachError, Selector35ClearanceHost, Selector35MotionError, Selector35MotionRequest,
        Selector35PersistentAudioError, Selector35TailError, Selector35TailRequest,
    },
    process_sound::{ProcessSoundRuntime, ProcessSoundVoiceTeardown},
    terrain_runtime::{TerrainRuntime, TerrainSpacing},
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    world_furniture_effect::{
        resident::{resident_furniture_model_radius, ResidentFurnitureResourceError},
        ProcessFurnitureQueryState,
    },
    ResidentRuntime,
};
use inf_formats::{FrontendExecutable, TerrainResourceLookup};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldSelector35ClearanceError {
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error(transparent)]
    Furniture(#[from] ResidentFurnitureResourceError),
}

pub(crate) struct WorldSelector35ClearanceHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub source: &'a WorldTerrainSource,
    pub terrain: &'a TerrainRuntime,
    pub furniture: &'a ProcessFurnitureQueryState,
}

impl ActorTerrainHost for WorldSelector35ClearanceHost<'_> {
    type Error = WorldSelector35ClearanceError;

    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        let current = self
            .residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        Ok(self.terrain)
    }
}

impl Selector35ClearanceHost for WorldSelector35ClearanceHost<'_> {
    fn furniture_model_radius(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        placement: u8,
    ) -> Result<Option<u16>, Self::Error> {
        resident_furniture_model_radius(self.residency, TerrainResourceLookup::from_raw(placement))
            .map_err(Into::into)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.terrain.sampling().spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.terrain.sampling().spacing
    }
    fn height_shift(&mut self) -> u32 {
        self.terrain.sampling().height_scale.shift()
    }
    fn fallback_placement(&self) -> u8 {
        self.furniture.fallback_payload().resource_lookup.raw()
    }
}

pub(crate) struct WorldSelector35TailContext<'a, 'exe, 'camera> {
    pub microseconds: u32,
    pub settings_7c: u32,
    pub executable: FrontendExecutable<'exe>,
    pub sine: &'a [u16; 4096],
    pub clearance: WorldSelector35ClearanceHost<'a>,
    pub firing: ActorWeaponFireContext<'a>,
    pub camera: &'a mut CameraFactoryInputs<'camera>,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
}

#[derive(Debug, Error)]
pub enum WorldSelector35TailError {
    #[error("selector35 target/clearance/firing prefix failed: {0}")]
    Tail(Selector35TailError<WorldSelector35ClearanceError>),
    #[error(transparent)]
    Motion(#[from] Selector35MotionError),
    #[error("selector35 persistent audio suffix failed: {0}")]
    Audio(Selector35PersistentAudioError<PlayerAttachError>),
}

pub(crate) fn tick_world_selector35_tail(
    arena: &mut ObjectArena,
    actor: ObjectId,
    mut context: WorldSelector35TailContext<'_, '_, '_>,
) -> Result<(), WorldSelector35TailError> {
    let mut controls = arena
        .tick_selector35_tail(
            actor,
            Selector35TailRequest {
                microseconds: context.microseconds,
                settings_7c: context.settings_7c,
                firing: context.firing,
                clearance: &mut context.clearance,
            },
        )
        .map_err(WorldSelector35TailError::Tail)?;
    arena.tick_selector35_motion(
        actor,
        Selector35MotionRequest {
            microseconds: context.microseconds,
            controls: &mut controls,
            executable: context.executable,
            sine_quarter: context.sine,
        },
    )?;
    arena
        .tick_selector35_persistent_audio(
            actor,
            context.camera,
            context.clearance.residency,
            context.sounds,
            context.voices,
        )
        .map_err(WorldSelector35TailError::Audio)
}
