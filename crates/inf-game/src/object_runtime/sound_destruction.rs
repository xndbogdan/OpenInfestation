//! Reached425F70 sound loan. No pool is installed on the arena or cloned.
use super::ObjectId;
use crate::process_sound::{
    ProcessSoundRecordError, ProcessSoundRef, ProcessSoundRuntime, ProcessSoundVoiceTeardown,
};

pub struct ObjectDestructionRequest<'a> {
    pub object: ObjectId,
    pub sound: SoundTeardownService<'a>,
}
pub enum SoundTeardownService<'a> {
    Available {
        sounds: &'a mut ProcessSoundRuntime,
        voices: &'a mut dyn ProcessSoundVoiceTeardown,
    },
    ///Explicit absence: inspected only at nonNULL168 after148/154/140.
    RequiredAtReach,
}
impl SoundTeardownService<'_> {
    pub(crate) fn reborrow(&mut self) -> SoundTeardownService<'_> {
        match self {
            Self::Available { sounds, voices } => SoundTeardownService::Available {
                sounds: &mut **sounds,
                voices: &mut **voices,
            },
            Self::RequiredAtReach => SoundTeardownService::RequiredAtReach,
        }
    }
    pub(crate) fn delete(
        &mut self,
        reference: ProcessSoundRef,
    ) -> Result<(), ProcessSoundRecordError> {
        match self {
            Self::Available { sounds, voices } => sounds.delete_record(reference, &mut **voices),
            Self::RequiredAtReach => Err(ProcessSoundRecordError::RequiredTeardown(reference)),
        }
    }
}
