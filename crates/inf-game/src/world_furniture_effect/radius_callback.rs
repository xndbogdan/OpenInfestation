//! Furniture callback 004375A0: capture process NULL before the lower world
//! radius service, retain the mutable stack XYZ identity and raw block VA,
//! then return AL1 only after the service returns normally.

use thiserror::Error;

use super::{FurnitureCallbackContext, FurnitureEffectQueue};
use crate::{
    object_runtime::{ActorHandle, TargetNullHandleSource},
    process_sound::ProcessSoundRuntime,
    terrain_runtime::TerrainRuntime,
};

/// Actual loans from the active executor invocation. Queue admission during
/// the lower radius call must mutate this same queue, not a detached copy.
pub struct FurnitureRadiusCallbackInputs<'a> {
    pub queue: &'a mut FurnitureEffectQueue,
    pub terrain: &'a mut TerrainRuntime,
    pub context: &'a mut FurnitureCallbackContext,
    pub block_address: u32,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub rng: &'a mut u32,
}

pub trait FurnitureRadiusCallbackHost: TargetNullHandleSource {
    type Error;

    /// 004887D0 on the current world. The block and position are consumed
    /// here, after the wrapper has captured the current process NULL handle.
    fn apply_radius(
        &mut self,
        inputs: FurnitureRadiusCallbackInputs<'_>,
        source: ActorHandle,
        replicate: bool,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FurnitureRadiusCallbackError<E> {
    #[error("furniture radius callback requires process NULL0054E894 at004375A0")]
    MissingProcessNull,
    #[error("furniture world-radius service failed: {0}")]
    Radius(E),
}

pub fn run_furniture_radius_callback<H: FurnitureRadiusCallbackHost + ?Sized>(
    host: &mut H,
    inputs: FurnitureRadiusCallbackInputs<'_>,
) -> Result<bool, FurnitureRadiusCallbackError<H::Error>> {
    // 4375A0 is the first instruction. In particular, do not parse the raw
    // block or read XYZ while preparing the request before this source read.
    let source = host
        .target_null_handle()
        .ok_or(FurnitureRadiusCallbackError::MissingProcessNull)?;
    host.apply_radius(inputs, source, true)
        .map_err(FurnitureRadiusCallbackError::Radius)?;
    // MOV AL,1 retains upper EAX; the executor consumes only this Boolean.
    Ok(true)
}

/// Existing resource/audio fixture hosts have no world radius services. This
/// explicit test boundary never substitutes successful radius behavior.
#[cfg(test)]
pub(crate) struct RequiredTestFurnitureRadius;

#[cfg(test)]
impl TargetNullHandleSource for RequiredTestFurnitureRadius {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        None
    }
}

#[cfg(test)]
impl FurnitureRadiusCallbackHost for RequiredTestFurnitureRadius {
    type Error = String;

    fn apply_radius(
        &mut self,
        _: FurnitureRadiusCallbackInputs<'_>,
        _: ActorHandle,
        _: bool,
    ) -> Result<(), String> {
        Err("required fixture world-radius service".into())
    }
}

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
impl super::FurnitureCameraCallbackHost for RequiredTestFurnitureRadius {
    type Error = String;

    fn camera_flash(
        &mut self,
        _: u32,
        _: super::FurnitureCameraCallbackInputs<'_>,
    ) -> Result<(), String> {
        Err("required fixture camera/world-flash service for 004375D0".into())
    }
}

#[cfg(test)]
impl super::FurniturePickupCallbackHost for RequiredTestFurnitureRadius {
    type Error = String;

    fn first_player_owns(&mut self, _: u16) -> Result<Option<bool>, String> {
        Err("required fixture first-player service for 00437880".into())
    }

    fn birth_pickup(
        &mut self,
        _: u16,
        _: u16,
        _: super::FurniturePickupCallbackInputs<'_>,
    ) -> Result<(), String> {
        Err("required fixture pickup birth for 00437880".into())
    }
}

#[cfg(test)]
impl super::FurnitureParticleCallbackHost for RequiredTestFurnitureRadius {
    type Error = String;

    fn emit(
        &mut self,
        _: super::FurnitureParticleEffect,
        _: super::FurnitureParticleCallbackInputs<'_>,
    ) -> Result<(), String> {
        Err("required fixture furniture particle service".into())
    }
}
