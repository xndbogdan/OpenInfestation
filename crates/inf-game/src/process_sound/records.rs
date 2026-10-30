//! Shared47FFF0/4800B0/47FFA0 record storage. Portable references retain
//! process identity; native pointers and malloc address/reuse are not recreated.
//! Backing is genuinely fallible and retained, never read as initialized bytes.
use super::SpatialSoundInputs;
use crate::platform_audio::ResidentSoundResource;
use std::{
    mem::MaybeUninit,
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

static NEXT_PROCESS: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessSoundRef {
    sound_process: u64,
    serial: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessSoundVoiceRef {
    pub(crate) owner: u64,
    pub(crate) serial: u64,
}
impl std::fmt::Debug for ProcessSoundVoiceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSoundVoiceRef")
            .field("sound_voice_pool", &self.owner)
            .field("serial", &self.serial)
            .finish()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessSoundRecordKind {
    Persistent,
    OneShot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessSoundVoiceOperation {
    Create,
    Start,
    Update,
    Delete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessSoundRecordState {
    Linked,
    Detached,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ProcessSoundRecordError {
    #[error("sound record belongs to another process: {0:?}")]
    Foreign(ProcessSoundRef),
    #[error("sound record identity is absent: {0:?}")]
    Unknown(ProcessSoundRef),
    #[error("sound record was released: {0:?}")]
    Released(ProcessSoundRef),
    #[error("sound record has already been unlinked: {0:?}")]
    Detached(ProcessSoundRef),
    #[error("sound record constructor is incomplete: {0:?}")]
    Pending(ProcessSoundRef),
    #[error("required40-byte sound record allocation failed")]
    Allocation,
    #[error("process sound identity exhausted")]
    IdentityExhausted,
    #[error("sound record {record:?} requires actual persistent voice operation {operation:?}")]
    RequiredVoice {
        record: ProcessSoundRef,
        operation: ProcessSoundVoiceOperation,
    },
    #[error("sound record {0:?} requires actual modulation1C ownership")]
    RequiredModulation(ProcessSoundRef),
    #[error("actor sound record {0:?} requires the actual process teardown loan")]
    RequiredTeardown(ProcessSoundRef),
    //Portable identity/service failure only. Native DSStop HRESULT errors
    //are consumed by4923C0 before normal release and must not enter this arm.
    #[error("required portable voice teardown failed for {voice:?}: code {code}")]
    VoiceTeardown {
        voice: ProcessSoundVoiceRef,
        code: u32,
    },
    #[error("voice service returned a reply for a different operation")]
    VoiceProtocol,
    #[error(transparent)]
    VoiceExecution(super::VoiceExecutionError<super::VoicePoolFault>),
    #[cfg(test)]
    #[error("supplied sound allocation failure {0}")]
    FixtureAllocation(u32),
}
#[derive(Debug, Error)]
pub enum ProcessSoundConstructionError<E> {
    #[error(transparent)]
    Record(#[from] ProcessSoundRecordError),
    #[error("sound record source read failed")]
    Source(E),
}
/// Actual nonNULL18 teardown only. Native47FFD0 skips this service for NULL.
/// The production SDL persistent implementation is a separate owner; absence
/// is an error, not disabled510620 or successful NULL destruction.
pub trait ProcessSoundVoiceTeardown {
    fn delete_voice(
        &mut self,
        record: ProcessSoundRef,
        voice: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError>;
}
/// Explicit actual platform service availability. Required remains lazy until
/// native record18 is nonNULL; it never publishes disabled device history.
pub enum ProcessSoundTeardownLoan<'a> {
    Available(&'a mut dyn ProcessSoundVoiceTeardown),
    RequiredAtReach,
}
impl ProcessSoundVoiceTeardown for ProcessSoundTeardownLoan<'_> {
    fn delete_voice(
        &mut self,
        record: ProcessSoundRef,
        voice: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        match self {
            Self::Available(host) => host.delete_voice(record, voice),
            Self::RequiredAtReach => RequiredProcessSoundVoiceTeardown.delete_voice(record, voice),
        }
    }
}
pub struct RequiredProcessSoundVoiceTeardown;
impl ProcessSoundVoiceTeardown for RequiredProcessSoundVoiceTeardown {
    fn delete_voice(
        &mut self,
        record: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        Err(ProcessSoundRecordError::RequiredVoice {
            record,
            operation: ProcessSoundVoiceOperation::Delete,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessSoundRecordSnapshot {
    pub state: ProcessSoundRecordState,
    ///None is portable logical terminator55F0EC, not native NULL next.
    pub next: Option<ProcessSoundRef>,
    ///None is portable logical head55F0E8. Native55F0F0 is terminator.prev.
    pub previous: Option<ProcessSoundRef>,
    pub position: Option<[u16; 3]>,
    pub volume: Option<u32>,
    pub pitch: Option<u32>,
    pub flags: Option<u32>,
    pub voice: Option<Option<ProcessSoundVoiceRef>>,
    pub modulation_is_null: Option<bool>,
    pub retained_bytes: usize,
}
pub struct ProcessSoundRecordConstructionRequest<P> {
    pub resource: ResidentSoundResource,
    pub kind: ProcessSoundRecordKind,
    pub volume: u32,
    pub pitch: u32,
    pub control_flag: u32,
    ///Demanded after actual allocation, link publication and resource8.
    pub position: P,
}
#[derive(Clone)]
struct Record {
    snapshot: ProcessSoundRecordSnapshot,
    resource: Option<ResidentSoundResource>,
    backing: Option<Vec<MaybeUninit<u8>>>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Record")
            .field("snapshot", &self.snapshot)
            .field("resource", &self.resource)
            .finish()
    }
}
#[derive(Clone, Copy)]
enum ProcessSoundIdentity {
    Assigned(u64),
    Exhausted,
}
impl std::fmt::Debug for ProcessSoundIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assigned(identity) => f
                .debug_struct("Assigned")
                .field("sound_process", identity)
                .finish(),
            Self::Exhausted => f.write_str("Exhausted"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ProcessSoundRecords {
    process: ProcessSoundIdentity,
    entries: Vec<Record>,
    head: Option<ProcessSoundRef>,
    tail: Option<ProcessSoundRef>,
    #[cfg(test)]
    allocation_script: std::collections::VecDeque<u32>,
}
impl ProcessSoundRecords {
    pub(crate) fn pool_identity(&self) -> Result<u64, ProcessSoundRecordError> {
        match self.process {
            ProcessSoundIdentity::Assigned(identity) => Ok(identity),
            ProcessSoundIdentity::Exhausted => Err(ProcessSoundRecordError::IdentityExhausted),
        }
    }
    /// 0047FEF0 initialization at the first resource acquisition. A retained
    /// live list is not a new process and must not be silently discarded.
    pub(crate) fn initialize_list(&mut self) -> Result<(), ProcessSoundRecordError> {
        if let Some(record) = self.head {
            return Err(ProcessSoundRecordError::RequiredTeardown(record));
        }
        self.head = None;
        self.tail = None;
        Ok(())
    }
    pub(super) fn new() -> Self {
        // Exhaustion never emits a reused token. It is reported at allocation.
        let process =
            match NEXT_PROCESS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            }) {
                Ok(identity) => ProcessSoundIdentity::Assigned(identity),
                Err(_) => ProcessSoundIdentity::Exhausted,
            };
        Self {
            process,
            entries: Vec::new(),
            head: None,
            tail: None,
            #[cfg(test)]
            allocation_script: Default::default(),
        }
    }
    fn index(&self, reference: ProcessSoundRef) -> Result<usize, ProcessSoundRecordError> {
        let ProcessSoundIdentity::Assigned(process) = self.process else {
            return Err(ProcessSoundRecordError::IdentityExhausted);
        };
        if reference.sound_process != process {
            return Err(ProcessSoundRecordError::Foreign(reference));
        }
        let index = reference
            .serial
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n < self.entries.len())
            .ok_or(ProcessSoundRecordError::Unknown(reference))?;
        Ok(index)
    }
    fn live(&self, reference: ProcessSoundRef) -> Result<usize, ProcessSoundRecordError> {
        let index = self.index(reference)?;
        match self.entries[index].snapshot.state {
            ProcessSoundRecordState::Linked => Ok(index),
            ProcessSoundRecordState::Detached => Err(ProcessSoundRecordError::Detached(reference)),
            ProcessSoundRecordState::Released => Err(ProcessSoundRecordError::Released(reference)),
        }
    }
    pub fn snapshot(
        &self,
        reference: ProcessSoundRef,
    ) -> Result<ProcessSoundRecordSnapshot, ProcessSoundRecordError> {
        Ok(self.entries[self.index(reference)?].snapshot)
    }
    pub(super) fn head(&self) -> Option<ProcessSoundRef> {
        self.head
    }
    /// Every record in construction order with its requested resource slot.
    #[cfg(test)]
    pub(crate) fn constructed(&self) -> Vec<(Option<i16>, ProcessSoundRecordSnapshot)> {
        self.entries
            .iter()
            .map(|record| {
                (
                    record
                        .resource
                        .as_ref()
                        .map(ResidentSoundResource::requested_slot),
                    record.snapshot,
                )
            })
            .collect()
    }
    pub fn linked_len(&self) -> usize {
        self.entries
            .iter()
            .filter(|r| r.snapshot.state == ProcessSoundRecordState::Linked)
            .count()
    }
    ///47FFF0: allocation/list links and resource8 precede lazy XYZ. A source
    ///failure keeps the linked pending record; caller publication has not run.
    pub fn construct<E, P: FnOnce() -> Result<[u16; 3], E>>(
        &mut self,
        request: ProcessSoundRecordConstructionRequest<P>,
    ) -> Result<ProcessSoundRef, ProcessSoundConstructionError<E>> {
        let ProcessSoundRecordConstructionRequest {
            resource,
            kind,
            volume,
            pitch,
            control_flag: flag,
            position,
        } = request;
        let slot = resource.requested_slot();
        #[cfg(test)]
        if let Some(code) = self.allocation_script.pop_front() {
            return Err(ProcessSoundRecordError::FixtureAllocation(code).into());
        }
        let ProcessSoundIdentity::Assigned(process) = self.process else {
            return Err(ProcessSoundRecordError::IdentityExhausted.into());
        };
        let serial = u64::try_from(self.entries.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(ProcessSoundRecordError::IdentityExhausted)?;
        self.entries
            .try_reserve(1)
            .map_err(|_| ProcessSoundRecordError::Allocation)?;
        let mut backing = Vec::<MaybeUninit<u8>>::new();
        backing
            .try_reserve_exact(40)
            .map_err(|_| ProcessSoundRecordError::Allocation)?;
        backing.resize_with(40, MaybeUninit::uninit);
        let reference = ProcessSoundRef {
            sound_process: process,
            serial,
        };
        self.entries.push(Record {
            snapshot: ProcessSoundRecordSnapshot {
                state: ProcessSoundRecordState::Linked,
                next: None,
                previous: self.tail,
                position: None,
                volume: None,
                pitch: None,
                flags: None,
                voice: None,
                modulation_is_null: None,
                retained_bytes: 40,
            },
            resource: Some(resource),
            backing: Some(backing),
        });
        if let Some(tail) = self.tail {
            let index = self.live(tail)?;
            self.entries[index].snapshot.next = Some(reference);
        } else {
            self.head = Some(reference);
        }
        self.tail = Some(reference);
        let xyz = position().map_err(ProcessSoundConstructionError::Source)?;
        crate::event_log::sound(xyz, volume, pitch, flag, slot);
        let row = &mut self.entries[(serial - 1) as usize].snapshot;
        row.position = Some(xyz); //native DWORDXY then WORDZ, one whole source DTO.
        row.volume = Some(volume);
        row.pitch = Some(pitch);
        row.flags = Some(1 | if flag as u8 != 0 { 4 } else { 0 });
        row.voice = Some(None);
        row.modulation_is_null = Some(true);
        //47FF60 replaces the completed47FFF0 flags only after normal birth.
        if kind == ProcessSoundRecordKind::OneShot {
            row.flags = Some(2 | if flag as u8 != 0 { 4 } else { 0 });
        }
        Ok(reference)
    }
    ///4800B0: currentXYZ precedes gain/pitch. No H9 or alias/RNG reread.
    pub fn update<E>(
        &mut self,
        reference: ProcessSoundRef,
        volume: u32,
        pitch: u32,
        position: Option<impl FnOnce() -> Result<[u16; 3], E>>,
    ) -> Result<(), ProcessSoundConstructionError<E>> {
        let index = self.live(reference)?;
        if let Some(source) = position {
            let xyz = source().map_err(ProcessSoundConstructionError::Source)?;
            self.entries[index].snapshot.position = Some(xyz);
        }
        self.entries[index].snapshot.volume = Some(volume);
        self.entries[index].snapshot.pitch = Some(pitch);
        Ok(())
    }
    pub(super) fn service_row(
        &self,
        reference: ProcessSoundRef,
    ) -> Result<(&ResidentSoundResource, SpatialSoundInputs), ProcessSoundRecordError> {
        let index = self.live(reference)?;
        let record = &self.entries[index];
        let row = record.snapshot;
        let flags = row
            .flags
            .ok_or(ProcessSoundRecordError::Pending(reference))?; //first480110 demand.
        let position = row
            .position
            .ok_or(ProcessSoundRecordError::Pending(reference))?;
        let volume = row
            .volume
            .ok_or(ProcessSoundRecordError::Pending(reference))?;
        Ok((
            record.resource.as_ref().expect("live resource8"),
            SpatialSoundInputs {
                position,
                volume,
                control_flag: flags & 4 != 0,
            },
        ))
    }
    pub(super) fn require_null_modulation(
        &self,
        reference: ProcessSoundRef,
    ) -> Result<(), ProcessSoundRecordError> {
        match self.snapshot(reference)?.modulation_is_null {
            Some(true) => Ok(()),
            Some(false) => Err(ProcessSoundRecordError::RequiredModulation(reference)),
            None => Err(ProcessSoundRecordError::Pending(reference)),
        }
    }
    pub(super) fn pitch(&self, reference: ProcessSoundRef) -> Result<u32, ProcessSoundRecordError> {
        self.snapshot(reference)?
            .pitch
            .ok_or(ProcessSoundRecordError::Pending(reference))
    }
    pub(super) fn publish_voice(
        &mut self,
        reference: ProcessSoundRef,
        voice: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        let index = self.live(reference)?;
        self.entries[index].snapshot.voice = Some(Some(voice));
        Ok(())
    }
    pub(super) fn destroy_voice<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        reference: ProcessSoundRef,
        voices: &mut V,
    ) -> Result<(), ProcessSoundRecordError> {
        let index = self.index(reference)?;
        let voice = self.entries[index]
            .snapshot
            .voice
            .ok_or(ProcessSoundRecordError::Pending(reference))?;
        if let Some(voice) = voice {
            voices.delete_voice(reference, voice)?;
            self.entries[index].snapshot.voice = Some(None);
        }
        Ok(())
    }
    ///47FFA0: unlink first, then1C/free, then18 teardown, then40B release.
    ///A typed error retains detached state and fields. Drop never runs this.
    pub fn delete<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        reference: ProcessSoundRef,
        voices: &mut V,
    ) -> Result<(), ProcessSoundRecordError> {
        let index = self.live(reference)?;
        let row = self.entries[index].snapshot;
        if let Some(next) = row.next {
            let i = self.live(next)?;
            self.entries[i].snapshot.previous = row.previous;
        } else {
            self.tail = row.previous;
        }
        if let Some(previous) = row.previous {
            let i = self.live(previous)?;
            self.entries[i].snapshot.next = row.next;
        } else {
            self.head = row.next;
        }
        self.entries[index].snapshot.state = ProcessSoundRecordState::Detached;
        match self.entries[index].snapshot.modulation_is_null {
            Some(true) => {}
            Some(false) => return Err(ProcessSoundRecordError::RequiredModulation(reference)),
            None => return Err(ProcessSoundRecordError::Pending(reference)),
        }
        self.destroy_voice(reference, voices)?;
        self.entries[index].resource = None;
        self.entries[index].backing = None;
        self.entries[index].snapshot.retained_bytes = 0;
        self.entries[index].snapshot.state = ProcessSoundRecordState::Released;
        Ok(())
    }
    pub fn cleanup_one_shots<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        voices: &mut V,
    ) -> Result<(), ProcessSoundRecordError> {
        let mut current = self.head;
        while let Some(reference) = current {
            let row = self.snapshot(reference)?;
            current = row.next; //capture NEXT before delete.
            if row
                .flags
                .ok_or(ProcessSoundRecordError::Pending(reference))?
                & 2
                != 0
            {
                self.delete(reference, voices)?;
            }
        }
        Ok(())
    }
    ///47FF20 process clear, distinct from4800E0 world one-shot cleanup.
    ///CapturesNEXT and preserves each normal/error delete prefix. No Drop hook.
    pub fn clear<V: ProcessSoundVoiceTeardown + ?Sized>(
        &mut self,
        voices: &mut V,
    ) -> Result<(), ProcessSoundRecordError> {
        let mut current = self.head;
        while let Some(reference) = current {
            current = self.snapshot(reference)?.next;
            self.delete(reference, voices)?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn fail_next_allocation(&mut self, code: u32) {
        self.allocation_script.push_back(code);
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_guards;
