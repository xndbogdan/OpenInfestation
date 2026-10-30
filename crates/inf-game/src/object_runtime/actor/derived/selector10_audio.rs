//! Native424BC5..424C32 suffix of the family's 00424830, after normal +118:
//! selector 10's 00425400 or the family's own 00424CF0.
//! Its resource/status gates differ from class36 camera/smoothing audio.
use super::DerivedActorError;
use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    process_sound::{ProcessSoundConstructionError, ProcessSoundRecordError, ProcessSoundRuntime},
    ResidentRuntime,
};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum Selector10PersistentAudioError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
}
impl ObjectArena {
    pub fn tick_selector10_persistent_audio(
        &mut self,
        actor: ObjectId,
        residency: &ResidentRuntime,
        sounds: &mut ProcessSoundRuntime,
    ) -> Result<(), Selector10PersistentAudioError> {
        self.require_family_update_424830(actor)?;
        let resource = self.object(actor)?.base.resource_word;
        if !matches!(resource, 33 | 34 | 106) || (self.actor(actor)?.word_c4 as i32) <= 1 {
            return Ok(());
        }
        let captured = self.derived_actor(actor)?.sound_168;
        self.derived_actor_mut(actor)?.word_16c = Some(0xaaaa);
        self.derived_actor_mut(actor)?.word_170 = Some(0xaaaa);
        let result = if let Some(reference) = captured {
            sounds.records_mut().update(
                reference,
                0xaaaa,
                0xaaaa,
                Some(|| {
                    self.object(actor)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(actor))
                }),
            )
        } else {
            let result = sounds.create_persistent(residency, 20, 0xaaaa, 0xaaaa, 0, || {
                self.object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))
            });
            match result {
                Ok(reference) => {
                    self.derived_actor_mut(actor)?.sound_168 = Some(reference);
                    return Ok(());
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) => Ok(()),
            Err(ProcessSoundConstructionError::Record(error)) => Err(error.into()),
            Err(ProcessSoundConstructionError::Source(error)) => Err(error.into()),
        }
    }
}
#[cfg(test)]
mod tests;
