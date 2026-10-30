//! Reached487F40/487F60/487FC0/487FE0 operations retain the actor's actual
//! +168 record. Allocation/list writes precede the deferred actor+32 read.
use super::*;
use crate::{
    platform_audio::ResidentSoundResource,
    process_sound::{
        ProcessSoundConstructionError, ProcessSoundRecordConstructionRequest,
        ProcessSoundRecordError, ProcessSoundRecordKind, ProcessSoundRuntime,
        ProcessSoundVoiceTeardown,
    },
    ResidentRuntime,
};

impl ObjectArena {
    pub fn run_family_dying_audio<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        actor: ObjectId,
        operation: FamilyDyingAudio,
        residency: &ResidentRuntime,
        sounds: &mut ProcessSoundRuntime,
        voices: &mut V,
    ) -> Result<(), FamilyDyingError<ProcessSoundRecordError>> {
        self.family4236d0_actor(actor)?;
        let position = |arena: &ObjectArena| {
            arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
        };
        let record_error = |error| match error {
            ProcessSoundConstructionError::Record(error) => service(0x0047_fff0, error),
            ProcessSoundConstructionError::Source(error) => FamilyDyingError::Object(error),
        };
        match operation {
            FamilyDyingAudio::OneShot {
                sound,
                volume,
                pitch,
            } => {
                sounds
                    .records_mut()
                    .construct(ProcessSoundRecordConstructionRequest {
                        resource: ResidentSoundResource::capture(residency, sound as i16),
                        kind: ProcessSoundRecordKind::OneShot,
                        volume,
                        pitch,
                        control_flag: 0,
                        position: || position(self),
                    })
                    .map_err(record_error)?;
            }
            FamilyDyingAudio::CreatePersistent {
                sound,
                volume,
                pitch,
            } => {
                let reference = sounds
                    .create_persistent(residency, sound as i16, volume, pitch, 0, || position(self))
                    .map_err(record_error)?;
                self.derived_actor_mut(actor)?.sound_168 = Some(reference);
            }
            FamilyDyingAudio::UpdatePersistent { volume, pitch } => {
                let reference = self.derived_actor(actor)?.sound_168.ok_or(
                    FamilyDyingError::RequiredField {
                        actor,
                        offset: 0x168,
                        address: 0x0042_472d,
                    },
                )?;
                sounds
                    .records_mut()
                    .update(reference, volume, pitch, Some(|| position(self)))
                    .map_err(record_error)?;
            }
            FamilyDyingAudio::StopPersistent => {
                let reference = self.derived_actor(actor)?.sound_168.ok_or(
                    FamilyDyingError::RequiredField {
                        actor,
                        offset: 0x168,
                        address: 0x0042_4663,
                    },
                )?;
                sounds
                    .delete_record(reference, voices)
                    .map_err(|e| service(0x0047_ffe0, e))?;
                // The outer native caller owns its normal-return NULL store.
            }
        }
        Ok(())
    }
}
