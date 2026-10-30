//! The Siren's update over the world's owners: the shared status
//! (`0047C990`, `0047DFD0`) through the family services, its alarm sound
//! (`00487F60`/`00487FC0`/`00487FE0`), its alert (`0047C840`) and the world
//! flash (`00487D70`).

use crate::{
    object_runtime::{
        ActorHandle, CameraSettings, FamilyUpdateEffect, FamilyUpdateHost, NativeFamilyUpdateError,
        ObjectArena, ObjectError, ObjectId, Selector47Host, SIREN_SOUND,
    },
    process_sound::{ProcessSoundRef, ProcessSoundVoiceTeardown},
    session_platform::world_actor_factory::{WorldCameraFactoryContext, WorldCameraInputs},
    world_flash::{WorldFlashPosition, WorldFlashRequest, WorldFlashState},
};

use super::WorldFamilyUpdateHost;

/// 0047FC5B: the alarm starts at full volume and pitch.
const FULL: u32 = 0x1_0000;

pub(crate) struct WorldSirenHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub status_load_factor: i32,
    pub attract: bool,
    pub world_flags: u32,
    pub flash: &'a mut WorldFlashState,
    pub camera_settings: CameraSettings,
    pub animation_clock: u32,
}

fn live(phase: &'static str, reason: impl std::fmt::Debug) -> NativeFamilyUpdateError {
    NativeFamilyUpdateError::LiveService {
        phase,
        reason: format!("{reason:?}"),
    }
}

fn position(arena: &ObjectArena, siren: ObjectId) -> Result<[u16; 3], ObjectError> {
    arena
        .object(siren)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(siren))
}

impl FamilyUpdateHost for WorldSirenHost<'_, '_> {
    type Error = NativeFamilyUpdateError;

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.family.process_null_handle()
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.family.random()
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.family.effect(arena, actor, effect)
    }
}

impl Selector47Host for WorldSirenHost<'_, '_> {
    fn status_load_factor(&self) -> Option<i32> {
        Some(self.status_load_factor)
    }

    fn attract(&self) -> bool {
        self.attract
    }

    fn create_alarm(
        &mut self,
        arena: &ObjectArena,
        siren: ObjectId,
    ) -> Result<Option<ProcessSoundRef>, Self::Error> {
        let owners = &mut self.family.owners;
        owners
            .sounds
            .create_persistent(owners.residency, SIREN_SOUND as i16, FULL, FULL, 0, || {
                position(arena, siren)
            })
            .map(Some)
            .map_err(|error| live("0047FC6F 00487F60", error))
    }

    fn update_alarm(
        &mut self,
        arena: &ObjectArena,
        siren: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), Self::Error> {
        self.family
            .owners
            .sounds
            .records_mut()
            .update(reference, volume, pitch, Some(|| position(arena, siren)))
            .map_err(|error| live("00487FC0", error))
    }

    fn release_alarm(&mut self, reference: ProcessSoundRef) -> Result<(), Self::Error> {
        self.family
            .owners
            .sounds
            .delete_record(reference, &mut *self.voices)
            .map_err(|error| live("00487FE0", error))
    }

    fn alert(
        &mut self,
        arena: &mut ObjectArena,
        siren: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        let null = self.family.process_null_handle();
        arena
            .propagate_actor_retaliation(siren, source, null, &mut *self.family.rng)
            .map_err(|error| live("0047C840 Siren alert", error))
    }

    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
    ) -> Result<(), Self::Error> {
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or_else(|| live("0047FB58 00487D70", "the flash needs the world's listeners"))?;
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: owners.residency,
            sine_quarter: self.family.sine_quarter,
            settings: self.camera_settings,
            process: Some(&*owners.factory_process),
            animation_clock: self.animation_clock,
        });
        let mut host = super::super::world_flash_host::WorldFlashCameraHost {
            camera: camera.factory(&*owners.terrain, self.world_flags, &mut *loan.listeners),
            residency: owners.residency,
        };
        self.flash
            .trigger(
                arena,
                WorldFlashRequest {
                    position: WorldFlashPosition::Words(position),
                    radius,
                    style_argument: 0,
                },
                &mut host,
                &mut *self.family.rng,
            )
            .map(|_| ())
            .map_err(|error| live("0047FB58 00487D70", error))
    }
}
