//! 0043EE75..0043EF55 persistent engine sound after normal 0043EF60. The
//! unsigned threshold reads actor WORD+8C, not model radius/size WORD+28.
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
pub enum Selector35PersistentAudioError<C> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error("actual 00478E00 camera failed")]
    Camera(C),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
}

#[cfg(test)]
mod tests;
impl ObjectArena {
    pub fn tick_selector35_persistent_audio<
        C: ActorAudioCameraHost,
        V: ProcessSoundVoiceTeardown + ?Sized,
    >(
        &mut self,
        actor: ObjectId,
        camera: &mut C,
        residency: &ResidentRuntime,
        sounds: &mut ProcessSoundRuntime,
        voices: &mut V,
    ) -> Result<(), Selector35PersistentAudioError<C::Error>> {
        self.selector35_actor(actor)?;
        let Some(listener) = camera
            .camera(self)
            .map_err(Selector35PersistentAudioError::Camera)?
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
        let large = self.actor(actor)?.word_8c > 400;
        let (slot, volume, pitch) = if large {
            (64, 65536, 8192)
        } else {
            (16, 16384, 65536)
        };
        if let Some(reference) = self.derived_actor(actor)?.sound_168 {
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
            let reference =
                match sounds.create_persistent(residency, slot, volume, pitch, 0, || {
                    self.object(actor)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(actor))
                }) {
                    Ok(r) => r,
                    Err(ProcessSoundConstructionError::Record(e)) => return Err(e.into()),
                    Err(ProcessSoundConstructionError::Source(e)) => return Err(e.into()),
                };
            self.derived_actor_mut(actor)?.sound_168 = Some(reference);
        }
        Ok(())
    }
}
