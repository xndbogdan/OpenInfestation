//! Process particle manager 00464A60, six pending/active lists and 400 cells.
//! Pool admission and construction are separate native operations. Unwritten
//! heap bytes survive reuse; exhausted/gated allocation returns native scratch.

use std::collections::VecDeque;

use thiserror::Error;

pub mod audio;
pub mod births;
pub mod draw;
pub mod factories;
pub mod impact;
pub mod oriented;
pub mod splashes;
pub mod timing;
pub mod update;

pub const PARTICLE_POOL_SIZE: usize = 400;
pub const PARTICLE_LIST_COUNT: usize = 6;
const PARTICLE_BYTES: usize = 64;

/// Portable containment after a reached feature failure. This has no native
/// payload byte: it prevents replaying partial effects or temporary draw XYZ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleStopPhase {
    Update,
    Collision,
    Retirement,
    Draw,
}

/// Native particle payload, after its eight-byte allocation header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleId {
    Pool(u16),
    /// Nonnull process record 0054EC60, also mutated by real constructors.
    Scratch,
}

/// Six available source bytes copied by 00464FB0 as a DWORD, then a WORD.
/// Unknown data is still copied; numeric consumers require their used spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleVectorBytes(pub [Option<u8>; 6]);

impl ParticleVectorBytes {
    pub fn from_words(words: [u16; 3]) -> Self {
        let mut bytes = [None; 6];
        for (axis, word) in words.into_iter().enumerate() {
            let [low, high] = word.to_le_bytes();
            bytes[axis * 2] = Some(low);
            bytes[axis * 2 + 1] = Some(high);
        }
        Self(bytes)
    }
}

impl From<[u16; 3]> for ParticleVectorBytes {
    fn from(words: [u16; 3]) -> Self {
        Self::from_words(words)
    }
}

#[derive(Clone, Debug)]
pub struct ParticleRecord {
    bytes: [u8; PARTICLE_BYTES],
    initialized: [bool; PARTICLE_BYTES],
}

impl ParticleRecord {
    fn heap() -> Self {
        Self {
            bytes: [0; PARTICLE_BYTES],
            initialized: [false; PARTICLE_BYTES],
        }
    }

    fn bss() -> Self {
        Self {
            bytes: [0; PARTICLE_BYTES],
            initialized: [true; PARTICLE_BYTES],
        }
    }

    pub fn read_byte(&self, offset: usize) -> Result<u8, ParticleError> {
        Ok(self.read::<1>(offset)?[0])
    }

    pub fn read_word(&self, offset: usize) -> Result<u16, ParticleError> {
        Ok(u16::from_le_bytes(self.read::<2>(offset)?))
    }

    pub fn read_dword(&self, offset: usize) -> Result<u32, ParticleError> {
        Ok(u32::from_le_bytes(self.read::<4>(offset)?))
    }

    pub fn write_word(&mut self, offset: usize, value: u16) -> Result<(), ParticleError> {
        self.write(offset, &value.to_le_bytes())
    }

    pub fn write_dword(&mut self, offset: usize, value: u32) -> Result<(), ParticleError> {
        self.write(offset, &value.to_le_bytes())
    }

    pub fn position(&self) -> Result<[u16; 3], ParticleError> {
        Ok([self.read_word(4)?, self.read_word(6)?, self.read_word(8)?])
    }

    pub fn write_position(&mut self, position: [u16; 3]) -> Result<(), ParticleError> {
        for (axis, word) in position.into_iter().enumerate() {
            self.write_word(4 + axis * 2, word)?;
        }
        Ok(())
    }

    /// Transfer data custody, including overwriting prior known bytes with
    /// unknown source bytes. No numeric interpretation is performed here.
    pub fn copy_bytes(&mut self, offset: usize, bytes: &[Option<u8>]) -> Result<(), ParticleError> {
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= PARTICLE_BYTES)
            .ok_or(ParticleError::RecordRange {
                offset,
                bytes: bytes.len(),
            })?;
        for (index, value) in bytes.iter().copied().enumerate() {
            self.initialized[offset + index] = value.is_some();
            if let Some(value) = value {
                self.bytes[offset + index] = value;
            }
        }
        debug_assert_eq!(end, offset + bytes.len());
        Ok(())
    }

    pub fn source_bytes<const N: usize>(
        &self,
        offset: usize,
    ) -> Result<[Option<u8>; N], ParticleError> {
        let end = offset
            .checked_add(N)
            .filter(|end| *end <= PARTICLE_BYTES)
            .ok_or(ParticleError::RecordRange { offset, bytes: N })?;
        if offset < 0x24 && end > 0x18 {
            return Err(ParticleError::PortableListPointer { offset, bytes: N });
        }
        Ok(std::array::from_fn(|i| {
            self.initialized[offset + i].then_some(self.bytes[offset + i])
        }))
    }

    fn copy_vector(
        &mut self,
        offset: usize,
        vector: ParticleVectorBytes,
    ) -> Result<(), ParticleError> {
        self.copy_bytes(offset, &vector.0[..4])?;
        self.copy_bytes(offset + 4, &vector.0[4..])
    }

    fn read<const N: usize>(&self, offset: usize) -> Result<[u8; N], ParticleError> {
        let end = offset
            .checked_add(N)
            .filter(|end| *end <= PARTICLE_BYTES)
            .ok_or(ParticleError::RecordRange { offset, bytes: N })?;
        if offset < 0x24 && end > 0x18 {
            return Err(ParticleError::PortableListPointer { offset, bytes: N });
        }
        if !self.initialized[offset..end].iter().all(|byte| *byte) {
            return Err(ParticleError::Unwritten { offset, bytes: N });
        }
        Ok(self.bytes[offset..end].try_into().expect("checked width"))
    }

    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), ParticleError> {
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= PARTICLE_BYTES)
            .ok_or(ParticleError::RecordRange {
                offset,
                bytes: bytes.len(),
            })?;
        self.bytes[offset..end].copy_from_slice(bytes);
        self.initialized[offset..end].fill(true);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleAllocationRequest {
    /// 00466100 ECX; native allocator ignores size after receiving it.
    pub payload_bytes: u32,
    /// 00466100 EDX; signed inclusive category bound for reclamation.
    pub last_reclaim_category: i32,
    /// Actual process 0055F448. Nonzero bypasses the phase read.
    pub network_active: bool,
    /// Current manager phase +64C. Required only when offline.
    pub phase: Option<u32>,
}

pub struct ParticleConstruction {
    pub category: usize,
    pub position: ParticleVectorBytes,
    /// A null velocity pointer leaves the reached heap bytes unchanged.
    pub velocity: Option<ParticleVectorBytes>,
    /// Native source ActorHandle +14, supplied by the actual caller.
    pub source_handle: u32,
    pub flags: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParticleError {
    #[error("particle pool index {0} is outside 400 cells")]
    PoolIndex(u16),
    #[error("particle category {0} is outside six native lists")]
    Category(usize),
    #[error("offline particle allocation requires the current phase")]
    MissingPhase,
    #[error("particle producer {address:08X} requires a nonnull velocity pointer")]
    RequiredBirthVelocity { address: u32 },
    #[error("particle bytes +{offset:02X} ({bytes}) are constructor-unwritten")]
    Unwritten { offset: usize, bytes: usize },
    #[error("particle range +{offset:02X} ({bytes}) exceeds the native payload")]
    RecordRange { offset: usize, bytes: usize },
    #[error("particle list/self pointer +{offset:02X} ({bytes}) requires portable list custody")]
    PortableListPointer { offset: usize, bytes: usize },
    #[error("particle {0:?} has no owned destructor")]
    Destructor(ParticleId),
    #[error("particle {0:?} has no live list node")]
    MissingNode(ParticleId),
    #[error("scratch release requires its unowned native allocation header")]
    ScratchRelease,
    #[error("particle {0:?} requires 00466280's reached impact service")]
    ImpactDestructorRequired(ParticleId),
}

/// Process pool survives world replacement. Only its 00464F60 clear owner
/// releases admitted cells; new world construction must not reset it.
#[derive(Clone, Debug)]
pub struct ProcessParticleRuntime {
    records: Vec<ParticleRecord>,
    scratch: ParticleRecord,
    occupants: [Option<ParticleId>; PARTICLE_POOL_SIZE],
    free: Vec<u16>,
    allocation_marker: Option<u16>,
    active: [VecDeque<ParticleId>; PARTICLE_LIST_COUNT],
    pending: [VecDeque<ParticleId>; PARTICLE_LIST_COUNT],
    stopped: [Option<ParticleStopPhase>; PARTICLE_POOL_SIZE],
    scratch_stopped: Option<ParticleStopPhase>,
    /// 0054EC5C increments as a DWORD; lookup alone uses modulo 100.
    pub vector_cursor: u32,
    /// Six bytes at0054ECB8. Native PE loader zero-fill initializes this once;
    /// 00466A50 takes its address and rereads it after synchronous services.
    /// Kept mutable and process-owned: zero startup does not prove immutability.
    pub emitter_velocity_54ecb8: ParticleVectorBytes,
    /// Published by 00464B30 before it walks occupied physical cells.
    pub update_globals: update::ParticleUpdateGlobals,
    /// The factories' network notices (`00492500` with 0x17 or 0x18), for
    /// the next network pass.
    network_sends: Vec<crate::network::OrderedSend>,
    /// Port option: the view shown's draw, apart from the game's, which
    /// already made this frame's births, makes none: every birth takes the
    /// scratch record, as a full or gated pool's does.
    pub(crate) refusing_births: bool,
    /// A receipt that stopped native code at `00492500` stops here too.
    #[cfg(test)]
    pub(crate) refuse_network_sends: bool,
}

impl ProcessParticleRuntime {
    pub fn queue_network_send(&mut self, id: u32, send: crate::network::protocol::GameSend) {
        self.network_sends
            .push((crate::network::next_send_order(), id, send));
    }

    pub fn take_ordered_network_sends(&mut self) -> Vec<crate::network::OrderedSend> {
        std::mem::take(&mut self.network_sends)
    }
}

impl Default for ProcessParticleRuntime {
    fn default() -> Self {
        Self {
            records: (0..PARTICLE_POOL_SIZE)
                .map(|_| ParticleRecord::heap())
                .collect(),
            // 0054EC60 is virtual zero-fill in the original .data section.
            scratch: ParticleRecord::bss(),
            occupants: [None; PARTICLE_POOL_SIZE],
            // 00464A60 pushes cells 0..399 through 00464E30, hence 399 first,
            // while it is the current manager 0054ECB4 (set by 00464B10,
            // cleared by 00464B20, later held from 00487180); the port passes
            // the pool to its users instead.
            free: (0..PARTICLE_POOL_SIZE as u16).collect(),
            allocation_marker: None,
            active: std::array::from_fn(|_| VecDeque::new()),
            pending: std::array::from_fn(|_| VecDeque::new()),
            stopped: [None; PARTICLE_POOL_SIZE],
            scratch_stopped: None,
            vector_cursor: 0,
            emitter_velocity_54ecb8: ParticleVectorBytes::from_words([0; 3]),
            update_globals: update::ParticleUpdateGlobals::default(),
            network_sends: Vec::new(),
            refusing_births: false,
            #[cfg(test)]
            refuse_network_sends: false,
        }
    }
}

/// The cell whose payload start holds leftover heap words in Infest01's pool.
const FRESH_POOL_HEAP_WORDS_CELL: usize = 32;
const FRESH_POOL_HEAP_WORDS_BYTES: usize = 0x18;

impl ProcessParticleRuntime {
    /// FreshParticlePoolZeroApproximation: production startup with known
    /// zero payload bytes. Infest01's 0x7080-byte block (0051246A from
    /// 00495940, returned at 2FD49:4CC) is zero apart from leftover heap
    /// words in cell 0's header and cell 32's payload +00..+17, which stay
    /// unknown here. Constructors still leave their bytes unwritten in the
    /// native sense; only a cell's first occupant sees these zeros. The
    /// native heap history before that allocation is not replayed.
    pub fn fresh_pool_zero_approximation() -> Self {
        let mut runtime = Self::default();
        for (cell, record) in runtime.records.iter_mut().enumerate() {
            *record = ParticleRecord::bss();
            if cell == FRESH_POOL_HEAP_WORDS_CELL {
                record.initialized[..FRESH_POOL_HEAP_WORDS_BYTES].fill(false);
            }
        }
        runtime
    }

    pub fn stopped_phase(&self, id: ParticleId) -> Option<ParticleStopPhase> {
        match id {
            ParticleId::Pool(slot) => self.stopped.get(usize::from(slot)).copied().flatten(),
            ParticleId::Scratch => self.scratch_stopped,
        }
    }

    pub(crate) fn stop_particle(&mut self, id: ParticleId, phase: ParticleStopPhase) {
        match id {
            ParticleId::Pool(slot) => {
                if let Some(value) = self.stopped.get_mut(usize::from(slot)) {
                    *value = Some(phase);
                }
            }
            ParticleId::Scratch => self.scratch_stopped = Some(phase),
        }
    }

    fn clear_particle_stop(&mut self, id: ParticleId) {
        match id {
            ParticleId::Pool(slot) => {
                if let Some(value) = self.stopped.get_mut(usize::from(slot)) {
                    *value = None;
                }
            }
            ParticleId::Scratch => self.scratch_stopped = None,
        }
    }
    /// 004656A0's shared draw/update floor cache. The cache bit precedes
    /// the query, and flag10 selects interpolation rather than raw cells.
    pub(crate) fn cached_terrain_floor(
        &mut self,
        id: ParticleId,
        terrain: &crate::terrain_runtime::TerrainRuntime,
    ) -> Result<i16, ParticleError> {
        let flags = self.record(id)?.read_word(0x12)?;
        if flags & 0x20 != 0 {
            return Ok(self.record(id)?.read_word(0x24)? as i16);
        }
        self.record_mut(id)?.write_word(0x12, flags | 0x20)?;
        let [x, _, z] = self.record(id)?.position()?;
        let height = if flags & 0x10 != 0 {
            terrain.height_at([x, z])
        } else {
            terrain.raw_height_at([x, z])
        };
        self.record_mut(id)?.write_word(0x24, height as u16)?;
        Ok(height)
    }

    pub fn record(&self, id: ParticleId) -> Result<&ParticleRecord, ParticleError> {
        match id {
            ParticleId::Pool(index) => self
                .records
                .get(usize::from(index))
                .ok_or(ParticleError::PoolIndex(index)),
            ParticleId::Scratch => Ok(&self.scratch),
        }
    }

    pub fn record_mut(&mut self, id: ParticleId) -> Result<&mut ParticleRecord, ParticleError> {
        match id {
            ParticleId::Pool(index) => self
                .records
                .get_mut(usize::from(index))
                .ok_or(ParticleError::PoolIndex(index)),
            ParticleId::Scratch => Ok(&mut self.scratch),
        }
    }

    /// `00464DF0`: physical record n (`00464E10`, 400 of `0x48` bytes) while
    /// it holds a class, nothing past the pool.
    pub fn occupant(&self, index: usize) -> Option<ParticleId> {
        self.occupants.get(index).copied().flatten()
    }

    pub fn active(&self, category: usize) -> Result<&VecDeque<ParticleId>, ParticleError> {
        self.active
            .get(category)
            .ok_or(ParticleError::Category(category))
    }

    pub fn pending(&self, category: usize) -> Result<&VecDeque<ParticleId>, ParticleError> {
        self.pending
            .get(category)
            .ok_or(ParticleError::Category(category))
    }

    pub fn allocation_marker(&self) -> Option<u16> {
        self.allocation_marker
    }
    pub fn free_count(&self) -> usize {
        self.free.len()
    }

    /// 00464ED0. Exhaustion tests active tails only; pending births do not
    /// become reclamation candidates until 00464DA0 transfers them.
    pub fn allocate(
        &mut self,
        request: ParticleAllocationRequest,
    ) -> Result<ParticleId, ParticleError> {
        self.allocate_reclaiming(request, Self::destroy)
    }

    /// One allocator policy; callers supply the reached deleting callback.
    /// A failed destructor retains prior body writes and prevents the following
    /// free-cell pop.
    pub(super) fn allocate_reclaiming<E: From<ParticleError>>(
        &mut self,
        request: ParticleAllocationRequest,
        mut destroy: impl FnMut(&mut Self, ParticleId) -> Result<(), E>,
    ) -> Result<ParticleId, E> {
        if self.refusing_births
            || !request.network_active && request.phase.ok_or(ParticleError::MissingPhase)? != 4
        {
            return Ok(ParticleId::Scratch);
        }
        let mut slot = self.free.pop();
        if slot.is_none() && request.last_reclaim_category >= 0 {
            for category in 0..=request.last_reclaim_category as usize {
                let active = self
                    .active
                    .get(category)
                    .ok_or(ParticleError::Category(category))?;
                if let Some(id) = active.back().copied() {
                    if category < request.last_reclaim_category as usize
                        || self.record(id)?.read_byte(0x12)? & 0x80 == 0
                    {
                        destroy(self, id)?;
                        slot = self.free.pop();
                        break;
                    }
                }
            }
        }
        let Some(slot) = slot else {
            return Ok(ParticleId::Scratch);
        };
        self.allocation_marker = Some(slot);
        self.clear_particle_stop(ParticleId::Pool(slot));
        Ok(ParticleId::Pool(slot))
    }

    /// 00464FB0, including 00464E50's marker-guarded pending publication.
    /// +20 is represented by list custody; no fake native self pointer is
    /// stored in portable bytes. Construction without a marker still runs.
    pub fn construct(
        &mut self,
        id: ParticleId,
        request: ParticleConstruction,
    ) -> Result<(), ParticleError> {
        let velocity = request.velocity;
        self.construct_resolving_velocity(id, request, || Ok(velocity))
    }

    /// The velocity source read occurs after publication, never as an eager
    /// preflight before the position and birth-field copies.
    pub(super) fn construct_resolving_velocity<E: From<ParticleError>>(
        &mut self,
        id: ParticleId,
        request: ParticleConstruction,
        velocity: impl FnOnce() -> Result<Option<ParticleVectorBytes>, E>,
    ) -> Result<(), E> {
        crate::event_log::particle(
            request.category,
            request.position.0,
            request.source_handle,
            request.flags,
        );
        self.clear_particle_stop(id);
        let record = self.record_mut(id)?;
        record.copy_vector(4, request.position)?;
        record.write_word(0x10, 0x2000)?;
        let mut flags = (request.flags as u16) | 0x9c;
        record.write_word(0x12, flags)?;
        record.write_dword(0x14, request.source_handle)?;
        record.write_dword(0, 0x0052_6ed8)?;
        if let Some(slot) = self.allocation_marker {
            self.pending
                .get_mut(request.category)
                .ok_or(ParticleError::Category(request.category))?
                .push_front(id);
            self.occupants[usize::from(slot)] = Some(id);
            self.allocation_marker = None;
        }
        if flags & 0x100 != 0 {
            flags |= 1;
            self.record_mut(id)?.write_word(0x12, flags)?;
        }
        let record = self.record_mut(id)?;
        if let Some(velocity) = velocity()? {
            record.copy_vector(0x0a, velocity)?;
        }
        Ok(())
    }

    /// 00464DA0 moves pending heads to active tails in category order. The
    /// +14 active anchor is tail.prev, so older active particles stay first.
    pub fn transfer_pending(&mut self) {
        for category in 0..PARTICLE_LIST_COUNT {
            while let Some(id) = self.pending[category].pop_front() {
                self.active[category].push_back(id);
            }
        }
    }

    /// Owned scalar deletion shares 00465050's base unlink/release. The EFC
    /// impact branch and unknown class destructors remain required services.
    pub fn destroy(&mut self, id: ParticleId) -> Result<(), ParticleError> {
        if self.prepare_destroy(id)? {
            return Err(ParticleError::ImpactDestructorRequired(id));
        }
        self.finish_destroy(id, 1)
    }

    /// Select the actual body and preserve its entry writes before a service.
    /// True denotes the reached EFC impact branch, not a successful callback.
    pub(super) fn prepare_destroy(&mut self, id: ParticleId) -> Result<bool, ParticleError> {
        let vtable = self.record(id)?.read_dword(0)?;
        if vtable == 0x0052_6efc {
            self.record_mut(id)?.write_dword(0, vtable)?;
            return Ok(self.record(id)?.read_byte(0x12)? & 0x0c == 0);
        }
        if !matches!(
            vtable,
            0x0052_6ed8
                | 0x0052_87b4
                | 0x0052_87f4
                | 0x0052_8814
                | 0x0052_8834
                | 0x0052_8854
                | 0x0052_8874
                | 0x0052_8894
                | 0x0052_88b4
                | 0x0052_88d4
                | 0x0052_88f4
                | 0x0052_8914
                | 0x0052_8934
                | 0x0052_8954
                | 0x0052_8974
                | 0x0052_6f20
                | 0x0052_6f40
                | 0x0052_6f60
                | 0x0052_6f80
        ) {
            return Err(ParticleError::Destructor(id));
        }
        Ok(false)
    }

    /// 00465050 and the deleting wrapper's low BYTE bit0 release policy.
    pub(super) fn finish_destroy(
        &mut self,
        id: ParticleId,
        flags: u32,
    ) -> Result<(), ParticleError> {
        self.record_mut(id)?.write_dword(0, 0x0052_6ed8)?;
        self.unlink(id)?;
        self.clear_particle_stop(id);
        if flags & 1 == 0 {
            return Ok(());
        }
        let ParticleId::Pool(slot) = id else {
            return Err(ParticleError::ScratchRelease);
        };
        self.occupants[usize::from(slot)] = None;
        self.free.push(slot);
        Ok(())
    }

    fn unlink(&mut self, id: ParticleId) -> Result<(), ParticleError> {
        let mut removed = false;
        for list in self.active.iter_mut().chain(self.pending.iter_mut()) {
            if let Some(index) = list.iter().position(|value| *value == id) {
                list.remove(index);
                removed = true;
                break;
            }
        }
        if !removed {
            return Err(ParticleError::MissingNode(id));
        }
        Ok(())
    }

    /// 00464F60 directly unlinks occupied cells in physical ascending order.
    /// It does not call class destructors or overwrite the payload vtable.
    /// The physical header is returned even if its occupant is scratch.
    pub fn clear(&mut self) -> Result<(), ParticleError> {
        for slot in 0..PARTICLE_POOL_SIZE {
            if let Some(id) = self.occupants[slot] {
                self.unlink(id)?;
                self.clear_particle_stop(id);
                self.occupants[slot] = None;
                self.free.push(slot as u16);
            }
        }
        self.scratch_stopped = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
