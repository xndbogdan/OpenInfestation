//! Native audio-specific resource loans over the SDL transport.
//!
//! H9's CRT constructor acquires first. Application table 5220F0 reaches
//! 47FF40 through 488000/528720 before its direct 4F4210 voice loan.
//! Release reverses these loans: application voices, records, then H9.
//! The FX device is an actual lease on the SDL mixer. CD and title share
//! that physical transport, so native FX release does not close their device.
//! Unrelated application dependencies retain their separate subsystem owners.

use super::*;
use crate::process_sound::{
    device::{SoundDeviceHost, SoundDeviceIdentity},
    lifecycle::{
        SoundLifecycleHost, SoundResourceLifecycle, SOUND_HANDLER_RESOURCE, SOUND_RECORD_RESOURCE,
        SOUND_VOICE_RESOURCE,
    },
    ProcessSoundRecords, ProcessSoundVoiceTeardown, ProcessSoundVoices,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DEVICE: AtomicU64 = AtomicU64::new(1);

pub(super) struct PlatformSoundLifecycle {
    graph: SoundResourceLifecycle,
    executable: Arc<[u8]>,
    cleanup_initialized: bool,
    fx_device: Option<SoundDeviceIdentity>,
    handler_held: bool,
    records_held: Option<u64>,
    application_voice_held: bool,
    stopped: bool,
}

impl PlatformSoundLifecycle {
    pub(super) fn open_handler(
        audio: &sdl2::AudioSubsystem,
        executable: &[u8],
        voices: &mut ProcessSoundVoices,
    ) -> Result<(Self, AudioDevice<Mixer>), PlatformAudioError> {
        let mapped = FrontendExecutable::parse(executable).map_err(parameters)?;
        let mut owner = Self {
            graph: SoundResourceLifecycle::native_startup(mapped).map_err(parameters)?,
            executable: Arc::from(executable),
            cleanup_initialized: false,
            fx_device: None,
            handler_held: false,
            records_held: None,
            application_voice_held: false,
            stopped: false,
        };
        let mut device = None;
        owner.acquire(
            SOUND_HANDLER_RESOURCE,
            voices,
            Transport::Opening {
                audio,
                output: &mut device,
            },
            None,
        )?;
        owner.handler_held = true;
        let device = device.ok_or_else(|| required("H9 acquired without its SDL device"))?;
        Ok((owner, device))
    }

    pub(super) fn acquire_session(
        &mut self,
        voices: &mut ProcessSoundVoices,
        device: &mut AudioDevice<Mixer>,
        records: &mut ProcessSoundRecords,
    ) -> Result<(), PlatformAudioError> {
        if !self.handler_held || self.records_held.is_some() || self.application_voice_held {
            return Err(required(
                "session sound loans already acquired or H9 released",
            ));
        }
        let identity = records.pool_identity()?;
        self.acquire(
            SOUND_RECORD_RESOURCE,
            voices,
            Transport::Existing(device),
            Some(records),
        )?;
        self.records_held = Some(identity);
        self.acquire(
            SOUND_VOICE_RESOURCE,
            voices,
            Transport::Existing(device),
            Some(records),
        )?;
        self.application_voice_held = true;
        Ok(())
    }

    pub(super) fn shutdown(
        &mut self,
        voices: &mut ProcessSoundVoices,
        device: &mut AudioDevice<Mixer>,
        records: &mut ProcessSoundRecords,
    ) -> Result<(), PlatformAudioError> {
        if self
            .records_held
            .is_some_and(|id| records.pool_identity() != Ok(id))
        {
            return Err(required(
                "shutdown requires the acquired process record pool",
            ));
        }
        if self.application_voice_held {
            self.release(SOUND_VOICE_RESOURCE, voices, device, records)?;
            self.application_voice_held = false;
        }
        if self.records_held.is_some() {
            self.release(SOUND_RECORD_RESOURCE, voices, device, records)?;
            self.records_held = None;
        }
        if self.handler_held {
            self.release(SOUND_HANDLER_RESOURCE, voices, device, records)?;
            self.handler_held = false;
        }
        Ok(())
    }

    fn acquire(
        &mut self,
        resource: u32,
        voices: &mut ProcessSoundVoices,
        transport: Transport<'_>,
        records: Option<&mut ProcessSoundRecords>,
    ) -> Result<(), PlatformAudioError> {
        if self.stopped {
            return Err(required("previous lifecycle failure retained its prefix"));
        }
        let executable = FrontendExecutable::parse(&self.executable).map_err(parameters)?;
        let result = self.graph.acquire(
            resource,
            executable,
            &mut Host {
                voices,
                transport,
                records,
                fx_device: &mut self.fx_device,
                cleanup_initialized: &mut self.cleanup_initialized,
            },
        );
        self.stopped = result.is_err();
        let error = result.map_err(|e| required(&e.to_string()))?;
        if error != 0 {
            return Err(required(&format!("native acquisition error {error:08X}")));
        }
        Ok(())
    }

    fn release(
        &mut self,
        resource: u32,
        voices: &mut ProcessSoundVoices,
        device: &mut AudioDevice<Mixer>,
        records: &mut ProcessSoundRecords,
    ) -> Result<(), PlatformAudioError> {
        if self.stopped {
            return Err(required("previous lifecycle failure retained its prefix"));
        }
        let executable = FrontendExecutable::parse(&self.executable).map_err(parameters)?;
        let result = self.graph.release(
            resource,
            executable,
            &mut Host {
                voices,
                transport: Transport::Existing(device),
                records: Some(records),
                fx_device: &mut self.fx_device,
                cleanup_initialized: &mut self.cleanup_initialized,
            },
        );
        self.stopped = result.is_err();
        result.map_err(|e| required(&e.to_string()))
    }
}

fn parameters(error: inf_formats::FrontendExecutableError) -> PlatformAudioError {
    PlatformAudioError::Parameters(error.to_string())
}
fn required(message: &str) -> PlatformAudioError {
    PlatformAudioError::Lifecycle(message.to_owned())
}

enum Transport<'a> {
    Opening {
        audio: &'a sdl2::AudioSubsystem,
        output: &'a mut Option<AudioDevice<Mixer>>,
    },
    Existing(&'a mut AudioDevice<Mixer>),
}
impl Transport<'_> {
    fn device(&mut self) -> Result<&mut AudioDevice<Mixer>, PlatformAudioError> {
        match self {
            Self::Opening { output, .. } => output
                .as_mut()
                .ok_or_else(|| required("SDL transport missing")),
            Self::Existing(device) => Ok(device),
        }
    }
}

struct Host<'a> {
    voices: &'a mut ProcessSoundVoices,
    transport: Transport<'a>,
    records: Option<&'a mut ProcessSoundRecords>,
    fx_device: &'a mut Option<SoundDeviceIdentity>,
    cleanup_initialized: &'a mut bool,
}
impl SoundLifecycleHost for Host<'_> {
    type Error = PlatformAudioError;
    fn callback(&mut self, address: u32) -> Result<u32, Self::Error> {
        match address {
            0x0049_4840 => {
                // Native empty cleanup stack plus CRT setjmp registration.
                // Scoped Rust Result propagation supplies the unwind boundary;
                // no native x86 jump-buffer bytes enter portable state.
                *self.cleanup_initialized = true;
            }
            0x0051_0250 => self
                .voices
                .device_state_mut()
                .initialize(&mut DeviceHost {
                    transport: &mut self.transport,
                    identity: self.fx_device,
                })
                .map_err(|e| required(&e.to_string()))?,
            0x0051_0330 => self
                .voices
                .device_state_mut()
                .release(&mut DeviceHost {
                    transport: &mut self.transport,
                    identity: self.fx_device,
                })
                .map_err(|e| required(&e.to_string()))?,
            0x004f_40b0 => self.voices.initialize_list()?,
            0x004f_40f0 => self.voices.sweep(
                true,
                &mut SdlVoicePlayback {
                    device: self.transport.device()?,
                },
            )?,
            0x0047_fef0 => self
                .records
                .as_deref_mut()
                .ok_or_else(|| required("47FEF0 requires process records"))?
                .initialize_list()?,
            0x0047_ff20 => {
                let records = self
                    .records
                    .as_deref_mut()
                    .ok_or_else(|| required("47FF20 requires process records"))?;
                records.clear(&mut RecordTeardown {
                    voices: self.voices,
                    device: self.transport.device()?,
                })?;
            }
            _ => return Err(required(&format!("unowned native callback {address:08X}"))),
        }
        Ok(0)
    }
    fn report_error(&mut self, error: u32) -> Result<(), Self::Error> {
        if !*self.cleanup_initialized {
            return Err(required(
                "error reporting requires the initialized cleanup boundary",
            ));
        }
        eprintln!("Infestation: sound resource acquisition failed ({error:08X})");
        Ok(())
    }
}

struct RecordTeardown<'a> {
    voices: &'a mut ProcessSoundVoices,
    device: &'a mut AudioDevice<Mixer>,
}
impl ProcessSoundVoiceTeardown for RecordTeardown<'_> {
    fn delete_voice(
        &mut self,
        _: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        self.voices
            .delete(
                voice,
                &mut SdlVoicePlayback {
                    device: self.device,
                },
            )
            .map_err(crate::process_sound::ProcessSoundRecordError::VoiceExecution)
    }
}

struct DeviceHost<'a, 'b> {
    transport: &'a mut Transport<'b>,
    identity: &'a mut Option<SoundDeviceIdentity>,
}
impl SoundDeviceHost for DeviceHost<'_, '_> {
    type Error = PlatformAudioError;
    fn create(&mut self, output: &mut Option<SoundDeviceIdentity>) -> Result<i32, Self::Error> {
        let Transport::Opening {
            audio,
            output: physical,
        } = self.transport
        else {
            return Err(required(
                "device reacquisition requires a new SDL transport loan",
            ));
        };
        let desired = AudioSpecDesired {
            freq: Some(OUTPUT_RATE as i32),
            channels: Some(2),
            samples: Some(1024),
        };
        let device = audio
            .open_playback(None, &desired, |_| Mixer {
                cd_volume: [1.0; 2],
                effects_volume: 15,
                ..Mixer::default()
            })
            .map_err(PlatformAudioError::Device)?;
        if device.spec().freq != OUTPUT_RATE as i32 || device.spec().channels != 2 {
            return Err(PlatformAudioError::Device(
                "device did not accept 44,100 Hz stereo".into(),
            ));
        }
        let serial = NEXT_DEVICE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| required("sound device identity exhausted"))?;
        let identity = SoundDeviceIdentity(serial);
        **physical = Some(device);
        *self.identity = Some(identity);
        *output = Some(identity);
        Ok(0)
    }
    fn hresult_error(&mut self, result: i32) -> Result<Option<u32>, Self::Error> {
        if result == 0 {
            Ok(None)
        } else {
            Err(required("SDL transport has no native HRESULT error object"))
        }
    }
    fn report_error(&mut self, _: u32) -> Result<u32, Self::Error> {
        Err(required("SDL transport has no native device error object"))
    }
    fn dispose_error(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(required("SDL transport has no native device error object"))
    }
    fn release(&mut self, device: SoundDeviceIdentity) -> Result<(), Self::Error> {
        if *self.identity != Some(device) {
            return Err(required("FX device lease identity mismatch"));
        }
        if !self.transport.device()?.lock().effects.is_empty() {
            return Err(required(
                "FX device release reached before voice buffers were released",
            ));
        }
        *self.identity = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
