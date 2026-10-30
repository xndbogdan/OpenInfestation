//! 004349E7..00434B0F: camera-gated persistent ship engine sound. Its
//! velocity-dependent volume/pitch are distinct from selector35/36 policies.
use super::super::kinematics::ftol;
use crate::{
    object_runtime::{ActorAudioCameraHost, DerivedActorError, ObjectArena, ObjectError, ObjectId},
    process_sound::{
        ProcessSoundConstructionError, ProcessSoundRecordError, ProcessSoundRuntime,
        ProcessSoundVoiceTeardown,
    },
    ResidentRuntime,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Selector34PersistentAudioError<C> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error("actual 00478E00 camera failed")]
    Camera(C),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
}

impl ObjectArena {
    pub fn tick_selector34_persistent_audio<
        C: ActorAudioCameraHost,
        V: ProcessSoundVoiceTeardown + ?Sized,
    >(
        &mut self,
        actor: ObjectId,
        camera: &mut C,
        residency: &ResidentRuntime,
        sounds: &mut ProcessSoundRuntime,
        voices: &mut V,
    ) -> Result<(), Selector34PersistentAudioError<C::Error>> {
        self.selector34_actor(actor)?;
        let Some(listener) = camera
            .camera(self)
            .map_err(Selector34PersistentAudioError::Camera)?
        else {
            return Ok(());
        };
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let camera = self
            .object(listener)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(listener))?;
        let squared = position.into_iter().zip(camera).fold(0u32, |sum, (p, c)| {
            let d = i32::from(p.wrapping_sub(c) as i16);
            sum.wrapping_add(d.wrapping_mul(d) as u32)
        });
        if squared > 0x0384_0000 {
            if let Some(reference) = self.derived_actor(actor)?.sound_168 {
                sounds.delete_record(reference, voices)?;
                self.derived_actor_mut(actor)?.sound_168 = None;
            }
            return Ok(());
        }
        if let Some(reference) = self.derived_actor(actor)?.sound_168 {
            let (volume, pitch) = engine_parameters(
                self.actor(actor)?.words_8e_92,
                self.actor(actor)?.rotating.basis.words,
            );
            match sounds.records_mut().update(
                reference,
                volume,
                pitch,
                Some(|| {
                    self.object(actor)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(actor))
                }),
            ) {
                Ok(()) => {}
                Err(ProcessSoundConstructionError::Record(e)) => return Err(e.into()),
                Err(ProcessSoundConstructionError::Source(e)) => return Err(e.into()),
            }
        } else {
            let reference = match sounds.create_persistent(residency, 21, 0x1000, 0x800, 0, || {
                self.object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))
            }) {
                Ok(reference) => reference,
                Err(ProcessSoundConstructionError::Record(e)) => return Err(e.into()),
                Err(ProcessSoundConstructionError::Source(e)) => return Err(e.into()),
            };
            self.derived_actor_mut(actor)?.sound_168 = Some(reference);
        }
        Ok(())
    }
}

fn engine_parameters(velocity: [u16; 3], basis: [u32; 10]) -> (u32, u32) {
    let [x, y, z] = velocity.map(|v| f64::from(v as i16));
    // Native x87 PC=53 retains this order and truncates the same dot twice.
    let dot = (y * f64::from(f32::from_bits(basis[8])) + z * f64::from(f32::from_bits(basis[9])))
        + x * f64::from(f32::from_bits(basis[7]));
    let pitch = ftol(dot).wrapping_mul(8).wrapping_add(2048) as u32;
    let volume = ftol(dot).wrapping_add(1024).wrapping_shl(4) as u32;
    (volume, pitch)
}
