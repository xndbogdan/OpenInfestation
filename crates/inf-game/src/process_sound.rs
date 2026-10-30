//! Shared process40-byte sound records,47FF60/47FFF0 and world service480110.
//! Stable native voice operations are explicit backend loans; nonNULL modulation1C remains required.

use inf_formats::{
    native_basis::{isqrt_48efc0, q31_divide},
    FrontendExecutable, FrontendExecutableError,
};

use crate::{
    frontend_scene::SceneView, platform_audio::ResidentSoundResource,
    process_particles::audio::ParticleSoundRequest, ResidentRuntime,
};

#[derive(Clone, Copy, Debug)]
pub struct ProcessSoundListener {
    /// The actual 00488020 render context, copied by 00410D90.
    pub view: SceneView,
    /// Current G+64C. Native phase5 suppresses positional gain.
    pub phase: u32,
    /// Process 0055F0F4. Its original virtual-zero initial value is false.
    pub suppressed: bool,
    /// The port's own: the water's plane (world units) when the world has
    /// one, for Remastered's muffling of a sound under it.
    pub water: Option<i32>,
}

/// The port's own, beside retail's DWORDs: where a positional sound is from
/// the camera, for Remastered's placement of retail's sounds (Ambience above
/// 0): the listener-relative vector 00490A80 rotated through the camera's
/// basis (X right, Y up, Z ahead), and whether it is under the water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundPlacement {
    pub rotated: [i16; 3],
    pub under_water: bool,
}

/// Four actual DWORDs passed by 00480110 to 004F46E0, and a positional
/// sound's placement (the port's own; none for the others).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessSoundParameters {
    pub volume: u32,
    pub pitch: u32,
    pub pan: i32,
    pub control_flag: bool,
    pub placement: Option<SoundPlacement>,
}

impl ProcessSoundParameters {
    pub(crate) fn words(self) -> [u32; 4] {
        [
            self.volume,
            self.pitch,
            self.pan as u32,
            u32::from(self.control_flag),
        ]
    }
}

/// One explicit native voice operation. Create output publication is separate
/// from Start so a later failure retains the real record18 store.
pub enum ProcessSoundVoiceRequest<'a> {
    OneShot {
        resource: &'a ResidentSoundResource,
        parameters: ProcessSoundParameters,
        effects_volume: u32,
        rng: &'a mut u32,
    },
    Create {
        resource: &'a ResidentSoundResource,
        parameters: ProcessSoundParameters,
        effects_volume: u32,
        looping: bool,
        rng: &'a mut u32,
    },
    Start {
        voice: Option<ProcessSoundVoiceRef>,
    },
    Update {
        voice: ProcessSoundVoiceRef,
        parameters: ProcessSoundParameters,
        effects_volume: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessSoundVoiceReply {
    OneShot(bool),
    Created(VoicePublication),
    Completed,
}
#[derive(Debug, PartialEq, Eq)]
pub enum ProcessSoundVoiceServiceFault<E> {
    Required(ProcessSoundVoiceOperation),
    Backend(E),
}
pub trait ProcessSoundAudio: ProcessSoundVoiceTeardown {
    type Error;
    fn operation(
        &mut self,
        request: ProcessSoundVoiceRequest<'_>,
    ) -> Result<ProcessSoundVoiceReply, ProcessSoundVoiceServiceFault<Self::Error>>;
}

#[derive(Clone, Copy, Debug)]
struct SoundPolicy {
    range: i32,
    pan_distance: i32,
}

pub mod device;
pub mod lifecycle;
mod records;
mod voices;
pub use records::{
    ProcessSoundConstructionError, ProcessSoundRecordConstructionRequest, ProcessSoundRecordError,
    ProcessSoundRecordKind, ProcessSoundRecordSnapshot, ProcessSoundRecordState,
    ProcessSoundRecords, ProcessSoundRef, ProcessSoundTeardownLoan, ProcessSoundVoiceOperation,
    ProcessSoundVoiceRef, ProcessSoundVoiceTeardown, RequiredProcessSoundVoiceTeardown,
};
pub use voices::{
    ProcessSoundVoices, VoiceAllocationState, VoiceCreateRequest, VoiceExecutionError,
    VoicePlaybackBackend, VoicePlaybackFault, VoicePlaybackReply, VoicePlaybackRequest,
    VoicePoolFault, VoicePublication, VoiceSnapshot, RETAIL_VOICE_LIMIT, SPLASH_SOUNDS,
};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProcessSoundServiceError<E> {
    #[error(transparent)]
    Record(#[from] ProcessSoundRecordError),
    #[error("required audio service failed: {0}")]
    Audio(E),
}

#[derive(Clone, Debug)]
pub struct ProcessSoundRuntime {
    policy: SoundPolicy,
    records: ProcessSoundRecords,
    started_in_last_service: usize,
}

impl ProcessSoundRuntime {
    pub fn new(executable: FrontendExecutable<'_>) -> Result<Self, FrontendExecutableError> {
        let bytes = executable.mapped(0x0053_bb80, 8)?.bytes;
        Ok(Self {
            policy: SoundPolicy {
                range: i32::from_le_bytes(bytes[0..4].try_into().expect("DWORD")),
                pan_distance: i32::from_le_bytes(bytes[4..8].try_into().expect("DWORD")),
            },
            records: ProcessSoundRecords::new(),
            started_in_last_service: 0,
        })
    }

    pub fn pending_len(&self) -> usize {
        self.records.linked_len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.linked_len() == 0
    }

    /// Actual successful starts in the last draw service, including a prefix
    /// retained when a later required voice callback returns an error.
    pub fn started_in_last_service(&self) -> usize {
        self.started_in_last_service
    }

    pub fn records(&self) -> &ProcessSoundRecords {
        &self.records
    }
    pub fn records_mut(&mut self) -> &mut ProcessSoundRecords {
        &mut self.records
    }
    #[cfg(test)]
    pub(crate) fn fail_next_record_allocation(&mut self, code: u32) {
        self.records.fail_next_allocation(code);
    }

    /// Both one-shot wrappers share47FFF0 and retain current H9 custody. The
    /// concrete callback already supplied XYZ; no alias/RNG/start runs here.
    pub fn enqueue(
        &mut self,
        resident: &ResidentRuntime,
        request: ParticleSoundRequest,
    ) -> Result<(), ProcessSoundRecordError> {
        let resource = ResidentSoundResource::capture(resident, request.sound as i16);
        match self
            .records
            .construct(ProcessSoundRecordConstructionRequest {
                resource,
                kind: ProcessSoundRecordKind::OneShot,
                volume: request.volume,
                pitch: request.pitch,
                control_flag: request.flags,
                position: || Ok::<_, std::convert::Infallible>(request.position),
            }) {
            Ok(_) => Ok(()),
            Err(ProcessSoundConstructionError::Record(error)) => Err(error),
            Err(ProcessSoundConstructionError::Source(never)) => match never {},
        }
    }
    ///487F60 performs nullable H9 resolution before47FFF0. Even a NULL H9
    ///entry constructs a record. Current sourceXYZ is read after list linkage.
    pub fn create_persistent<E>(
        &mut self,
        resident: &ResidentRuntime,
        slot: i16,
        volume: u32,
        pitch: u32,
        flag: u32,
        position: impl FnOnce() -> Result<[u16; 3], E>,
    ) -> Result<ProcessSoundRef, ProcessSoundConstructionError<E>> {
        self.records
            .construct(ProcessSoundRecordConstructionRequest {
                resource: ResidentSoundResource::capture(resident, slot),
                kind: ProcessSoundRecordKind::Persistent,
                volume,
                pitch,
                control_flag: flag,
                position,
            })
    }
    pub fn delete_record<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        reference: ProcessSoundRef,
        voices: &mut V,
    ) -> Result<(), ProcessSoundRecordError> {
        self.records.delete(reference, voices)
    }
    ///4800E0 only removes flags14 mask0x02, preserving persistent records.
    pub fn cleanup_one_shots(&mut self) -> Result<(), ProcessSoundRecordError> {
        self.records
            .cleanup_one_shots(&mut RequiredProcessSoundVoiceTeardown)
    }
    ///480110 plus488020 unconditional outer4800E0. Ordered records share
    ///listener/RNG custody. A later error never removes a persistent record.
    pub fn service_world_draw<A: ProcessSoundAudio + ?Sized>(
        &mut self,
        listener: ProcessSoundListener,
        audio: &mut A,
        effects_volume: u32,
        rng: &mut u32,
    ) -> Result<usize, ProcessSoundServiceError<A::Error>> {
        self.started_in_last_service = 0;
        let result = (|| {
            if !listener.suppressed {
                let mut current = self.records.head();
                while let Some(reference) = current {
                    let row = self.records.snapshot(reference)?;
                    current = row.next; //480110 capturesNEXT before callbacks.
                    let (resource, inputs) = self.records.service_row(reference)?;
                    let spatial = sound_parameters(inputs, listener, self.policy);
                    if spatial.is_some() {
                        self.records.require_null_modulation(reference)?;
                    }
                    //4802C7 also reads pitch before the zero-gain retirement arm.
                    let pitch = self.records.pitch(reference)?;
                    if let Some(spatial) = spatial {
                        //Native fresh+10 follows gain/modulation before voice dispatch.
                        let parameters = ProcessSoundParameters {
                            volume: spatial.volume,
                            pitch,
                            pan: spatial.pan,
                            control_flag: spatial.control_flag,
                            placement: Some(spatial.placement),
                        };
                        let flags = self
                            .records
                            .snapshot(reference)?
                            .flags
                            .ok_or(ProcessSoundRecordError::Pending(reference))?;
                        if flags & 2 != 0 {
                            let reply = audio
                                .operation(ProcessSoundVoiceRequest::OneShot {
                                    resource,
                                    parameters,
                                    effects_volume,
                                    rng,
                                })
                                .map_err(|e| match e {
                                    ProcessSoundVoiceServiceFault::Required(operation) => {
                                        ProcessSoundServiceError::Record(
                                            ProcessSoundRecordError::RequiredVoice {
                                                record: reference,
                                                operation,
                                            },
                                        )
                                    }
                                    ProcessSoundVoiceServiceFault::Backend(e) => {
                                        ProcessSoundServiceError::Audio(e)
                                    }
                                })?;
                            let ProcessSoundVoiceReply::OneShot(started) = reply else {
                                return Err(ProcessSoundRecordError::VoiceProtocol.into());
                            };
                            self.started_in_last_service += usize::from(started);
                        } else {
                            let voice = self
                                .records
                                .snapshot(reference)?
                                .voice
                                .ok_or(ProcessSoundRecordError::Pending(reference))?;
                            if let Some(voice) = voice {
                                let reply = audio
                                    .operation(ProcessSoundVoiceRequest::Update {
                                        voice,
                                        parameters,
                                        effects_volume,
                                    })
                                    .map_err(|e| match e {
                                        ProcessSoundVoiceServiceFault::Required(operation) => {
                                            ProcessSoundServiceError::Record(
                                                ProcessSoundRecordError::RequiredVoice {
                                                    record: reference,
                                                    operation,
                                                },
                                            )
                                        }
                                        ProcessSoundVoiceServiceFault::Backend(e) => {
                                            ProcessSoundServiceError::Audio(e)
                                        }
                                    })?;
                                if reply != ProcessSoundVoiceReply::Completed {
                                    return Err(ProcessSoundRecordError::VoiceProtocol.into());
                                }
                            } else {
                                let reply = audio
                                    .operation(ProcessSoundVoiceRequest::Create {
                                        resource,
                                        parameters,
                                        effects_volume,
                                        looping: flags & 1 != 0,
                                        rng,
                                    })
                                    .map_err(|e| match e {
                                        ProcessSoundVoiceServiceFault::Required(operation) => {
                                            ProcessSoundServiceError::Record(
                                                ProcessSoundRecordError::RequiredVoice {
                                                    record: reference,
                                                    operation,
                                                },
                                            )
                                        }
                                        ProcessSoundVoiceServiceFault::Backend(e) => {
                                            ProcessSoundServiceError::Audio(e)
                                        }
                                    })?;
                                let ProcessSoundVoiceReply::Created(publication) = reply else {
                                    return Err(ProcessSoundRecordError::VoiceProtocol.into());
                                };
                                if let VoicePublication::Written(voice) = publication {
                                    self.records.publish_voice(reference, voice)?;
                                }
                                let voice = self
                                    .records
                                    .snapshot(reference)?
                                    .voice
                                    .ok_or(ProcessSoundRecordError::Pending(reference))?;
                                let reply = audio
                                    .operation(ProcessSoundVoiceRequest::Start { voice })
                                    .map_err(|e| match e {
                                        ProcessSoundVoiceServiceFault::Required(operation) => {
                                            ProcessSoundServiceError::Record(
                                                ProcessSoundRecordError::RequiredVoice {
                                                    record: reference,
                                                    operation,
                                                },
                                            )
                                        }
                                        ProcessSoundVoiceServiceFault::Backend(e) => {
                                            ProcessSoundServiceError::Audio(e)
                                        }
                                    })?;
                                if reply != ProcessSoundVoiceReply::Completed {
                                    return Err(ProcessSoundRecordError::VoiceProtocol.into());
                                }
                                self.started_in_last_service += usize::from(voice.is_some());
                            }
                        }
                    } else {
                        self.records.destroy_voice(reference, audio)?;
                    }
                }
            }
            Ok(self.started_in_last_service)
        })();
        let cleanup = self.cleanup_one_shots();
        match result {
            Err(error) => Err(error),
            Ok(count) => {
                cleanup?;
                Ok(count)
            }
        }
    }
}

/// Real particle enqueue adapter: births and updates share the process list;
/// alias resolution, RNG and voice starts belong to the later draw service.
pub struct ProcessParticleSoundQueue<'a> {
    pub sounds: &'a mut ProcessSoundRuntime,
    pub residency: &'a ResidentRuntime,
}

impl crate::process_particles::births::ParticleBirthAudio for ProcessParticleSoundQueue<'_> {
    type Error = ProcessSoundRecordError;
    fn explosion_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut crate::process_particles::ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.sounds.enqueue(self.residency, request)
    }
}
impl crate::process_particles::update::ParticleUpdateHost for ProcessParticleSoundQueue<'_> {
    type Error = ProcessSoundRecordError;
    fn impact_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut crate::process_particles::ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.sounds.enqueue(self.residency, request)
    }
}

#[derive(Clone, Copy, Debug)]
struct SpatialSoundInputs {
    position: [u16; 3],
    volume: u32,
    control_flag: bool,
}
impl From<ParticleSoundRequest> for SpatialSoundInputs {
    fn from(request: ParticleSoundRequest) -> Self {
        Self {
            position: request.position,
            volume: request.volume,
            control_flag: request.flags as u8 != 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpatialSoundParameters {
    volume: u32,
    pan: i32,
    control_flag: bool,
    placement: SoundPlacement,
}
fn sound_parameters(
    request: SpatialSoundInputs,
    listener: ProcessSoundListener,
    policy: SoundPolicy,
) -> Option<SpatialSoundParameters> {
    let relative: [i16; 3] = std::array::from_fn(|axis| {
        request.position[axis].wrapping_sub(listener.view.eye[axis] as u16) as i16
    });
    if request.volume == 0
        || relative
            .iter()
            .any(|word| i32::from(*word).abs() >= policy.range)
    {
        return None;
    }
    let rotated = listener.view.basis.inverse_words(relative);
    let x = i32::from(rotated[0]);
    let pan = if x.abs() > policy.pan_distance {
        if x < 0 {
            -10_000
        } else {
            10_000
        }
    } else {
        let factor = q31_divide(
            x.wrapping_add(policy.pan_distance) as u32,
            policy.pan_distance.wrapping_mul(2) as u32,
        ) as i32;
        ((i64::from(factor) * 20_000) >> 31) as i32 - 10_000
    };
    let squared = rotated.into_iter().fold(0_i32, |sum, word| {
        sum.wrapping_add(i32::from(word).wrapping_mul(i32::from(word)))
    });
    let distance = isqrt_48efc0(squared) as i32;
    let volume = if listener.phase == 5 {
        0
    } else if distance <= 256 {
        request.volume
    } else if distance < policy.range {
        let difference = policy.range.wrapping_sub(distance);
        let factor = if difference == policy.range {
            i32::MAX
        } else {
            (difference.wrapping_shl(15) / policy.range).wrapping_shl(16)
        };
        ((i64::from(request.volume as i32) * i64::from(factor)) >> 31) as u32
    } else {
        0
    };
    let under_water = listener
        .water
        .is_some_and(|plane| i32::from(request.position[1] as i16) < plane);
    (volume != 0).then_some(SpatialSoundParameters {
        volume,
        pan,
        control_flag: request.control_flag,
        placement: SoundPlacement {
            rotated,
            under_water,
        },
    })
}

#[cfg(test)]
#[path = "process_sound_tests.rs"]
mod tests;
