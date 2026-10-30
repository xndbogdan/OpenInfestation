//! Exact481B10. No delta/clock read occurs in this callback. Camera factory
//!comes first and can mutate the live arena; H9 is lazy and only used at birth.
use super::{DerivedActorError, Family4236d0Class};
use crate::{
    object_runtime::{CameraFactoryInputs, ObjectArena, ObjectError, ObjectId, PlayerAttachError},
    process_sound::{
        ProcessSoundConstructionError, ProcessSoundRecordError, ProcessSoundRuntime,
        ProcessSoundVoiceTeardown,
    },
    ResidentRuntime,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug)]
pub struct Selector36PersistentAudioRequest {
    pub controls: [i32; 2],
}
///One mandatory actual478E00 service. A NULL result is genuine factory output,
///not missing service, processNULL, first-player or renderer-eye substitution.
pub trait ActorAudioCameraHost {
    type Error;
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error>;
}
impl ActorAudioCameraHost for CameraFactoryInputs<'_> {
    type Error = PlayerAttachError;
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        arena.world_camera(self)
    }
}
#[derive(Debug, Error)]
pub enum Selector36PersistentAudioError<C> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("actual478E00 camera failed")]
    Camera(C),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
}
impl ObjectArena {
    pub fn tick_selector36_persistent_audio<
        C: ActorAudioCameraHost,
        V: ProcessSoundVoiceTeardown + ?Sized,
    >(
        &mut self,
        actor: ObjectId,
        request: Selector36PersistentAudioRequest,
        camera: &mut C,
        residency: &ResidentRuntime,
        sounds: &mut ProcessSoundRuntime,
        voices: &mut V,
    ) -> Result<(), Selector36PersistentAudioError<C::Error>> {
        self.selector36_actor(actor)?; //actual class policy, no model/placement whitelist.
        let Some(listener) = camera
            .camera(self)
            .map_err(Selector36PersistentAudioError::Camera)?
        else {
            return Ok(());
        };
        //The port owns wholeXYZ custody; first-X failure is represented. Native
        //reads actorXY, cameraXY, actorZ, cameraZ. No callback lies inside it.
        let xyz = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let listener_xyz = self
            .object(listener)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(listener))?;
        let delta: [i32; 3] =
            std::array::from_fn(
                |axis| i32::from(xyz[axis].wrapping_sub(listener_xyz[axis]) as i16),
            );
        let squared = delta
            .into_iter()
            .fold(0u32, |sum, d| sum.wrapping_add(d.wrapping_mul(d) as u32));
        if squared > 0x0384_0000 {
            if let Some(reference) = self.derived_actor(actor)?.sound_168 {
                sounds.delete_record(reference, voices)?;
                self.derived_actor_mut(actor)?.sound_168 = None; //normal-onlyNULL.
            }
            return Ok(());
        }
        //CDQ/XOR/SUB is wrapping absolute, and comparison is signed. MIN does
        //not saturate toMAX: its multiply wraps back to the native32768 target.
        let mut magnitude = request.controls[1].wrapping_abs();
        if magnitude > 1024 {
            magnitude = 1024;
        }
        let target = magnitude.wrapping_mul(8).wrapping_add(32768);
        let old = self.selector36_actor(actor)?;
        let volume = smooth(old.word_280, target);
        let old_pitch = old.word_284; //481BBE read before481BC6 volume store.
        let Family4236d0Class::Selector36(state) = &mut self.family4236d0_actor_mut(actor)?.class
        else {
            unreachable!("class checked")
        };
        state.word_280 = volume;
        let pitch = smooth(old_pitch, target);
        state.word_284 = pitch;
        //Fresh168 after280/284 stores. Existing records never reread H9.
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
            let reference = match sounds.create_persistent(residency, 64, volume, pitch, 0, || {
                //47FFF0 current source read occurs AFTER allocation/list/resource8.
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
fn smooth(old: u32, target: i32) -> u32 {
    old.wrapping_add((target.wrapping_sub(old as i32) >> 2) as u32)
}
#[cfg(test)]
mod tests;
