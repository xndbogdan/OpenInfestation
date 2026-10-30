//! Ordered native4F4230/4F4100 voices, separate from40-byte actor records.
//! SDL playback is a short explicit service. No callback or Drop rewinds a list.
use super::{ProcessSoundParameters, ProcessSoundVoiceRef};
use crate::platform_audio::ResidentSoundResource;
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use std::{
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_POOL: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePoolIdentity {
    Assigned { sound_voice_pool: u64 },
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePoolFault {
    Foreign(ProcessSoundVoiceRef),
    Unknown(ProcessSoundVoiceRef),
    Released(ProcessSoundVoiceRef),
    Pending(ProcessSoundVoiceRef),
    RequiredDevice,
    RequiredVoicePointer,
    Allocation,
    IdentityExhausted,
    Capacity2202,
    LiveListInitialization,
    RequiredListInitialization,
}
impl std::fmt::Display for VoicePoolFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VoicePoolFault {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePlaybackFault<E> {
    Required(E),
    NativeResult(E),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VoiceExecutionError<E> {
    #[error(transparent)]
    Pool(#[from] VoicePoolFault),
    #[error("required voice playback service: {0}")]
    Required(E),
    #[error("native voice operation result: {0}")]
    NativeResult(E),
}
impl<E> From<VoicePlaybackFault<E>> for VoiceExecutionError<E> {
    fn from(e: VoicePlaybackFault<E>) -> Self {
        match e {
            VoicePlaybackFault::Required(e) => Self::Required(e),
            VoicePlaybackFault::NativeResult(e) => Self::NativeResult(e),
        }
    }
}
/// These are distinct native service phases, not an atomic control shortcut.
pub enum VoicePlaybackRequest<'a> {
    Create {
        voice: ProcessSoundVoiceRef,
        samples: &'a Arc<[i16]>,
    },
    Status {
        voice: ProcessSoundVoiceRef,
    },
    Restore {
        voice: ProcessSoundVoiceRef,
    },
    Play {
        voice: ProcessSoundVoiceRef,
        looping: bool,
    },
    Stop {
        voice: ProcessSoundVoiceRef,
    },
    Release {
        voice: ProcessSoundVoiceRef,
    },
    Frequency {
        voice: ProcessSoundVoiceRef,
        pitch: u32,
    },
    Pan {
        voice: ProcessSoundVoiceRef,
        pan: i32,
        /// Remastered's placement of the sound (the port's own).
        placement: Option<super::SoundPlacement>,
    },
    Gain {
        voice: ProcessSoundVoiceRef,
        volume: u32,
        master: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePlaybackReply {
    Done,
    Playing(bool),
}
pub trait VoicePlaybackBackend {
    type Error;
    /// 00510450's first raw-buffer setup, before configuration publication.
    /// The SDL backend validates its normal shared playback device here.
    fn configure_device(&mut self) -> Result<(), VoicePlaybackFault<Self::Error>>;
    fn operation(
        &mut self,
        request: VoicePlaybackRequest<'_>,
    ) -> Result<VoicePlaybackReply, VoicePlaybackFault<Self::Error>>;
}
pub struct VoiceCreateRequest {
    pub resource: ResidentSoundResource,
    pub samples: Arc<[i16]>,
    pub parameters: ProcessSoundParameters,
    pub master_volume: u32,
    pub looping: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePublication {
    Unwritten,
    Written(ProcessSoundVoiceRef),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceAllocationState {
    AllocatedUnlinked,
    Linked,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSnapshot {
    pub state: VoiceAllocationState,
    pub previous: Option<ProcessSoundVoiceRef>,
    pub next: Option<ProcessSoundVoiceRef>,
    pub parameters: ProcessSoundParameters,
    pub counter: i32,
    pub looping: bool,
    pub automatic: bool,
    pub retained_bytes: usize,
}
struct Voice {
    state: VoiceAllocationState,
    snapshot: Option<VoiceSnapshot>,
    resource: Option<ResidentSoundResource>,
    backing: Option<Vec<MaybeUninit<u8>>>,
    /// Its cue requested one of [`SPLASH_SOUNDS`].
    splash: bool,
}
/// The voices retail's pool admits (`0x2202` past them).
pub const RETAIL_VOICE_LIMIT: usize = 24;
/// The H9 sounds a falling drop's splash plays (`0048A8F0`, one landing in
/// 32): the hovercraft's spray keeps about 20 of them sounding over water.
pub const SPLASH_SOUNDS: [i16; 2] = [9, 10];

/// Non-Clone process pool; Vec entries retain tombstones and never reuse serials.
pub struct ProcessSoundVoices {
    identity: VoicePoolIdentity,
    /// The live voices a creation's sweep and admission allow: retail's 24.
    limit: usize,
    /// That fix's cap on [`SPLASH_SOUNDS`] voices; none in retail.
    splash_limit: Option<usize>,
    device: super::device::SoundDeviceRuntime,
    voices: Vec<Voice>,
    head: Option<ProcessSoundVoiceRef>,
    tail: Option<ProcessSoundVoiceRef>,
    list_initialized: bool,
    #[cfg(test)]
    allocation_failures: std::collections::VecDeque<()>,
}
impl std::fmt::Debug for ProcessSoundVoices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSoundVoices")
            .field("identity", &self.identity)
            .field("device", &self.device)
            .field("head", &self.head)
            .field("tail", &self.tail)
            .field(
                "rows",
                &self
                    .voices
                    .iter()
                    .map(|v| (v.state, v.snapshot, v.backing.as_ref().map(Vec::len)))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}
impl ProcessSoundVoices {
    /// Original enabled/device/config are loader-zero, once per process. A
    /// missing mixer is not a call to this constructor and is never disabled.
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        executable.loader_zero_data(0x0065_f248, 12)?;
        let identity = NEXT_POOL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()
            .map(|sound_voice_pool| VoicePoolIdentity::Assigned { sound_voice_pool })
            .unwrap_or(VoicePoolIdentity::Exhausted);
        Ok(Self {
            identity,
            limit: RETAIL_VOICE_LIMIT,
            splash_limit: None,
            device: super::device::SoundDeviceRuntime::native_startup(executable)?,
            voices: Vec::new(),
            head: None,
            tail: None,
            list_initialized: false,
            #[cfg(test)]
            allocation_failures: std::collections::VecDeque::new(),
        })
    }
    /// The pool's limit on live voices (at least one).
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }
    /// The cap on live splash voices (at least one), or none.
    pub fn set_splash_limit(&mut self, limit: Option<usize>) {
        self.splash_limit = limit.map(|limit| limit.max(1));
    }
    /// Port fix: a one-shot of `slot` is a splash past the cap. After the
    /// create sweep every live splash is still playing, and those keep
    /// playing: the new one is refused instead.
    pub fn splash_refused(&self, slot: i16) -> Result<bool, VoicePoolFault> {
        let Some(limit) = self.splash_limit else {
            return Ok(false);
        };
        if !SPLASH_SOUNDS.contains(&slot) {
            return Ok(false);
        }
        let mut count = 0;
        let mut current = self.head;
        while let Some(r) = current {
            let row = self.snapshot(r)?;
            current = row.next;
            count += usize::from(row.automatic && self.voices[self.index(r)?].splash);
        }
        Ok(count >= limit)
    }
    /// Tags a created voice by the slot its cue requested: an H9 alias
    /// leaves the voice holding its target's slot.
    pub fn note_requested_slot(
        &mut self,
        r: ProcessSoundVoiceRef,
        slot: i16,
    ) -> Result<(), VoicePoolFault> {
        let i = self.index(r)?;
        self.voices[i].splash = SPLASH_SOUNDS.contains(&slot);
        Ok(())
    }
    pub fn device_state(&self) -> &super::device::SoundDeviceRuntime {
        &self.device
    }
    pub(crate) fn device_state_mut(&mut self) -> &mut super::device::SoundDeviceRuntime {
        &mut self.device
    }
    /// 004F40B0 publishes the empty sentinel chain. Its volume reference is
    /// the same retained f32 ln(65536) used by the playback gain conversion.
    pub(crate) fn initialize_list(&mut self) -> Result<(), VoicePoolFault> {
        if self.head.is_some() {
            return Err(VoicePoolFault::LiveListInitialization);
        }
        self.head = None;
        self.tail = None;
        self.list_initialized = true;
        Ok(())
    }
    pub(crate) fn configure_raw_source<B: VoicePlaybackBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        self.require_device()?;
        if self.device.configuration == 0 {
            backend.configure_device()?;
            self.device.configuration = 1;
        }
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.device.enabled != 0
    }
    fn require_device(&self) -> Result<(), VoicePoolFault> {
        if self.device.device.is_some() {
            Ok(())
        } else {
            Err(VoicePoolFault::RequiredDevice)
        }
    }
    pub fn head(&self) -> Option<ProcessSoundVoiceRef> {
        self.head
    }
    pub fn live_len(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state == VoiceAllocationState::Linked)
            .count()
    }
    fn index(&self, r: ProcessSoundVoiceRef) -> Result<usize, VoicePoolFault> {
        if self.identity
            != (VoicePoolIdentity::Assigned {
                sound_voice_pool: r.owner,
            })
        {
            return Err(VoicePoolFault::Foreign(r));
        }
        let i = r
            .serial
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(VoicePoolFault::Unknown(r))?;
        let v = self.voices.get(i).ok_or(VoicePoolFault::Unknown(r))?;
        match v.state {
            VoiceAllocationState::Released => return Err(VoicePoolFault::Released(r)),
            VoiceAllocationState::AllocatedUnlinked => return Err(VoicePoolFault::Pending(r)),
            VoiceAllocationState::Linked => {}
        }
        Ok(i)
    }
    pub fn snapshot(&self, r: ProcessSoundVoiceRef) -> Result<VoiceSnapshot, VoicePoolFault> {
        Ok(self.voices[self.index(r)?]
            .snapshot
            .expect("linked voice has published fields"))
    }
    /// Every recursive alias Create must enter this phase before reading its
    /// source. False means native disabled/unwritten, not missing backend.
    pub fn prepare_create<B: VoicePlaybackBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<bool, VoiceExecutionError<B::Error>> {
        if !self.enabled() {
            return Ok(false);
        }
        self.require_device()?;
        self.sweep(false, b)?;
        Ok(true)
    }
    /// Raw shared-buffer preparation has already succeeded. This method owns
    /// fallible60-byte custody BEFORE count>=limit, buffer duplicate and controls.
    pub fn allocate<B: VoicePlaybackBackend>(
        &mut self,
        r: VoiceCreateRequest,
        b: &mut B,
    ) -> Result<ProcessSoundVoiceRef, VoiceExecutionError<B::Error>> {
        self.require_device()?;
        let VoicePoolIdentity::Assigned { sound_voice_pool } = self.identity else {
            return Err(VoicePoolFault::IdentityExhausted.into());
        };
        #[cfg(test)]
        if self.allocation_failures.pop_front().is_some() {
            return Err(VoicePoolFault::Allocation.into());
        }
        self.voices
            .try_reserve(1)
            .map_err(|_| VoicePoolFault::Allocation)?;
        let mut backing = Vec::<MaybeUninit<u8>>::new();
        backing
            .try_reserve_exact(60)
            .map_err(|_| VoicePoolFault::Allocation)?;
        backing.resize_with(60, MaybeUninit::uninit);
        if self.live_len() >= self.limit {
            return Err(VoicePoolFault::Capacity2202.into());
        }
        let serial = u64::try_from(self.voices.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(VoicePoolFault::IdentityExhausted)?;
        let voice = ProcessSoundVoiceRef {
            owner: sound_voice_pool,
            serial,
        };
        // Reserve identity and real60-byte backing before duplicate/controls.
        // Portable service failures retain this unlinked prefix; they must not
        // manufacture published native fields or recycle a still-live identity.
        let index = self.voices.len();
        self.voices.push(Voice {
            state: VoiceAllocationState::AllocatedUnlinked,
            snapshot: None,
            resource: Some(r.resource),
            backing: Some(backing),
            splash: false,
        });
        if let Err(error) = b.operation(VoicePlaybackRequest::Create {
            voice,
            samples: &r.samples,
        }) {
            if matches!(&error, VoicePlaybackFault::NativeResult(_)) {
                let v = &mut self.voices[index];
                v.state = VoiceAllocationState::Released;
                v.resource = None;
                v.backing = None;
            }
            return Err(error.into());
        }
        let controls = (|| {
            b.operation(VoicePlaybackRequest::Frequency {
                voice,
                pitch: r.parameters.pitch,
            })?;
            b.operation(VoicePlaybackRequest::Pan {
                voice,
                pan: r.parameters.pan,
                placement: r.parameters.placement,
            })?;
            b.operation(VoicePlaybackRequest::Gain {
                voice,
                volume: r.parameters.volume,
                master: r.master_volume,
            })?;
            Ok::<(), VoicePlaybackFault<B::Error>>(())
        })();
        if let Err(error) = controls {
            // Native Release result is disposed; portable missing/corrupt
            // custody stops with allocation and identity retained at this phase.
            if let Err(VoicePlaybackFault::Required(e)) =
                b.operation(VoicePlaybackRequest::Release { voice })
            {
                return Err(VoiceExecutionError::Required(e));
            }
            let v = &mut self.voices[index];
            v.state = VoiceAllocationState::Released;
            v.resource = None;
            v.backing = None;
            return Err(error.into());
        }
        self.voices[index].snapshot = Some(VoiceSnapshot {
            state: VoiceAllocationState::Linked,
            previous: self.tail,
            next: None,
            parameters: r.parameters,
            counter: 0,
            looping: r.looping,
            automatic: false,
            retained_bytes: 60,
        });
        self.voices[index].state = VoiceAllocationState::Linked;
        if let Some(tail) = self.tail {
            let i = self.index(tail)?;
            self.voices[i].snapshot.as_mut().expect("linked tail").next = Some(voice);
        } else {
            self.head = Some(voice);
        }
        self.tail = Some(voice);
        Ok(voice)
    }
    pub fn mark_one_shot(&mut self, r: ProcessSoundVoiceRef) -> Result<(), VoicePoolFault> {
        let i = self.index(r)?;
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .automatic = true;
        Ok(())
    }
    pub fn start<B: VoicePlaybackBackend>(
        &mut self,
        r: Option<ProcessSoundVoiceRef>,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        if !self.enabled() {
            return Ok(());
        }
        self.require_device()?;
        let r = r.ok_or(VoicePoolFault::RequiredVoicePointer)?;
        let i = self.index(r)?;
        let old = self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter;
        let new = old.wrapping_sub(1);
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter = new;
        if old >= 0 && new < 0 {
            b.operation(VoicePlaybackRequest::Restore { voice: r })?;
            b.operation(VoicePlaybackRequest::Play {
                voice: r,
                looping: self.voices[i]
                    .snapshot
                    .as_ref()
                    .expect("linked voice")
                    .looping,
            })?;
        } else if old < 0 && new >= 0 {
            b.operation(VoicePlaybackRequest::Stop { voice: r })?;
        }
        Ok(())
    }
    /// One-shot wrapper's own inline Start has different Play error transport.
    pub fn start_one_shot<B: VoicePlaybackBackend>(
        &mut self,
        r: ProcessSoundVoiceRef,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        self.mark_one_shot(r)?;
        if !self.enabled() {
            return Ok(());
        }
        self.require_device()?;
        let i = self.index(r)?;
        let old = self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter;
        let new = old.wrapping_sub(1);
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter = new;
        if old >= 0 && new < 0 {
            match b.operation(VoicePlaybackRequest::Restore { voice: r }) {
                Err(VoicePlaybackFault::Required(e)) => {
                    return Err(VoiceExecutionError::Required(e))
                }
                Err(VoicePlaybackFault::NativeResult(_)) => return Ok(()), //4F4756..4775 consumes Restore result without Play.
                Ok(_) => {}
            }
            match b.operation(VoicePlaybackRequest::Play {
                voice: r,
                looping: self.voices[i]
                    .snapshot
                    .as_ref()
                    .expect("linked voice")
                    .looping,
            }) {
                Err(VoicePlaybackFault::Required(e)) => {
                    return Err(VoiceExecutionError::Required(e))
                }
                _ => {} //native4F4771 discards the Play result.
            }
        } else if old < 0 && new >= 0 {
            //4F4770..4775 consumes this inline Stop result; persistent Start
            //tail-jumps5105B0 and propagates it instead.
            if let Err(VoicePlaybackFault::Required(e)) =
                b.operation(VoicePlaybackRequest::Stop { voice: r })
            {
                return Err(VoiceExecutionError::Required(e));
            }
        }
        Ok(())
    }
    /// Native Update has no enabled gate. Copies all parameters before calls.
    pub fn update<B: VoicePlaybackBackend>(
        &mut self,
        r: ProcessSoundVoiceRef,
        p: ProcessSoundParameters,
        master: u32,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        let i = self.index(r)?;
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .parameters = p;
        b.operation(VoicePlaybackRequest::Frequency {
            voice: r,
            pitch: p.pitch,
        })?;
        b.operation(VoicePlaybackRequest::Pan {
            voice: r,
            pan: p.pan,
            placement: p.placement,
        })?;
        b.operation(VoicePlaybackRequest::Gain {
            voice: r,
            volume: p.volume,
            master,
        })?;
        Ok(())
    }
    /// Complete4F4780 walk: caller publishes current global volume first.
    /// No enabled/status/automatic gate, no frequency/pan/counter mutation.
    /// A backend failure retains earlier gains and stops before the next row.
    pub fn reattenuate<B: VoicePlaybackBackend>(
        &mut self,
        master: u32,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        let mut current = self.head;
        while let Some(voice) = current {
            let row = self.snapshot(voice)?;
            b.operation(VoicePlaybackRequest::Gain {
                voice,
                volume: row.parameters.volume,
                master,
            })?;
            current = self.snapshot(voice)?.next;
        }
        Ok(())
    }
    pub fn delete<B: VoicePlaybackBackend>(
        &mut self,
        r: ProcessSoundVoiceRef,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        if !self.enabled() {
            return Ok(());
        }
        self.require_device()?;
        let i = self.index(r)?;
        let old = self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter;
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter = 0;
        if old < 0 {
            if let Err(VoicePlaybackFault::Required(e)) =
                b.operation(VoicePlaybackRequest::Stop { voice: r })
            {
                return Err(VoiceExecutionError::Required(e));
            }
        }
        b.operation(VoicePlaybackRequest::Release { voice: r })?;
        let row = self.voices[i].snapshot.expect("linked voice");
        if let Some(next) = row.next {
            let n = self.index(next)?;
            self.voices[n]
                .snapshot
                .as_mut()
                .expect("linked next")
                .previous = row.previous;
        } else {
            self.tail = row.previous;
        }
        if let Some(previous) = row.previous {
            let p = self.index(previous)?;
            self.voices[p]
                .snapshot
                .as_mut()
                .expect("linked previous")
                .next = row.next;
        } else {
            self.head = row.next;
        }
        let v = &mut self.voices[i];
        v.resource = None;
        v.backing = None;
        v.state = VoiceAllocationState::Released;
        v.snapshot = None;
        Ok(())
    }
    pub fn sweep<B: VoicePlaybackBackend>(
        &mut self,
        force: bool,
        b: &mut B,
    ) -> Result<(), VoiceExecutionError<B::Error>> {
        if !self.list_initialized {
            return Err(VoicePoolFault::RequiredListInitialization.into());
        }
        let mut next = self.head;
        while let Some(r) = next {
            let row = self.snapshot(r)?;
            next = row.next;
            if force {
                self.delete(r, b)?;
            } else if row.automatic {
                match b.operation(VoicePlaybackRequest::Status { voice: r }) {
                    Ok(VoicePlaybackReply::Playing(true)) => {}
                    Ok(VoicePlaybackReply::Playing(false))
                    | Err(VoicePlaybackFault::NativeResult(_)) => self.delete(r, b)?,
                    Err(VoicePlaybackFault::Required(e)) => {
                        return Err(VoiceExecutionError::Required(e))
                    }
                    Ok(VoicePlaybackReply::Done) => {
                        return Err(VoicePoolFault::RequiredVoicePointer.into())
                    }
                }
            }
        }
        if self.live_len() >= self.limit {
            let mut current = self.head;
            while let Some(r) = current {
                let row = self.snapshot(r)?;
                current = row.next;
                if row.automatic {
                    self.delete(r, b)?;
                    break;
                }
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn fail_next_allocation(&mut self) {
        self.allocation_failures.push_back(());
    }
    #[cfg(test)]
    pub(super) fn set_counter(&mut self, r: ProcessSoundVoiceRef, c: i32) {
        let i = self.index(r).unwrap();
        self.voices[i]
            .snapshot
            .as_mut()
            .expect("linked voice")
            .counter = c;
    }
}
#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
