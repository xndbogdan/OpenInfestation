//! One supported-process arena for 004EBE60/004EBEE0.
//! Vertex24 and shade4 allocations share [00564728,0065E728). Normal native
//! retirement restores the registered old cursor without clearing bytes.
//! Typed feature stops differ from native managed nonlocal errors; recovery is an
//! explicitly named portable approximation, never implicit Drop/reset recovery.

use std::{cell::RefCell, fmt, rc::Rc};

use inf_formats::FrontendLoaderZeroRange;
use thiserror::Error;

pub const MODEL_SCRATCH_BYTES: usize = 0xfa000;
const NATIVE_POOL: u32 = 0x0056_4728;
const NATIVE_CURSOR_END: u32 = 0x0065_e72c;

/// A signed shade below the pool reads native `[005645E0, 00564728)`: the
/// software rasterizer's span state, in the zero-filled tail of `.data`.
/// Every writer (`004B0730`, `004B0AE0`, `004B1110`, `004B1660`, `004B1D20`,
/// `004B2760`, `004B6B10`, `004B6F20`, `004B75F0`, `004B7BB0`, `004B8330`,
/// `004B8FA0`) is a method of the software renderer's vtable `00528FF0` with
/// no direct caller, and retail always builds the Direct3D renderer
/// (`005290E8`, which shares none of them), so these bytes read zero there.
/// Lower addresses keep the out-of-arena error.
const PRE_POOL_ZERO_BYTES: i64 = (NATIVE_POOL - 0x0056_45e0) as i64;

/// Cloning shares the same bytes, cursor, registration stack and draw state.
#[derive(Clone)]
pub struct ProcessModelScratch(Rc<RefCell<Arena>>);

#[derive(Clone, Debug)]
pub struct ModelScratchDrawScope {
    owner: Rc<()>,
    serial: u64,
    entry_cursor: usize,
}

impl PartialEq for ModelScratchDrawScope {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner) && self.serial == other.serial
    }
}
impl Eq for ModelScratchDrawScope {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelScratchDrawState {
    Ready,
    Running(ModelScratchDrawScope),
    Stopped(ModelScratchDrawScope),
}

#[derive(Clone, Copy, Debug)]
pub enum ModelScratchDrawExit {
    NormalReturn,
    StoppedOnError,
}

/// Actual typed containment choice. It is not a claim of native unwind parity.
#[derive(Clone, Copy, Debug)]
pub struct RegisteredMarkUnwindApproximation;

/// Counts stay unsigned WORDs; these are byte offsets, not native pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelScratchLayout {
    pub start: usize,
    pub vertex_count: u16,
    pub shade_base: usize,
    pub shade_count: u16,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct RegisteredAllocationMark {
    owner: Rc<()>,
    scope: u64,
    serial: u64,
}

impl PartialEq for RegisteredAllocationMark {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner)
            && self.scope == other.scope
            && self.serial == other.serial
    }
}
impl Eq for RegisteredAllocationMark {}

/// Layout is inspectable; the opaque registration proves its actual custody.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelScratchAllocation {
    pub layout: ModelScratchLayout,
    mark: RegisteredAllocationMark,
}

impl ModelScratchAllocation {
    pub fn mark(&self) -> &RegisteredAllocationMark {
        &self.mark
    }
}

/// What [`ProcessModelScratch::save`] keeps to put the arena back: its
/// cursor, registrations and draw state. The bytes the draws after it
/// allocate over wait in the arena's journal.
pub struct ModelScratchSave {
    owner: Rc<()>,
    cursor: usize,
    scope_serial: u64,
    allocation_serial: u64,
    state: ModelScratchDrawState,
    marks: Vec<ModelScratchAllocation>,
}

struct Arena {
    owner: Rc<()>,
    bytes: Vec<Option<u8>>,
    cursor: usize,
    scope_serial: u64,
    allocation_serial: u64,
    state: ModelScratchDrawState,
    marks: Vec<ModelScratchAllocation>,
    /// While a save is open: from where, and the bytes there before the
    /// draws since allocated over them. Every write lands in an allocation,
    /// so these are all the bytes they can change.
    journal: Option<(usize, Vec<Option<u8>>)>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModelScratchError {
    #[error("loader-zero proof {start:#x}..{end:#x} does not cover scratch and cursor")]
    LoaderZeroProof { start: u32, end: u32 },
    #[error("model scratch already keeps a save")]
    SaveOpen,
    #[error("model scratch keeps no save to put back")]
    NoSave,
    #[error("model scratch owner is already borrowed")]
    BorrowConflict,
    #[error("model scratch needs {expected}; actual state is {actual:?}")]
    DrawState {
        expected: &'static str,
        actual: ModelScratchDrawState,
    },
    #[error("model scratch scope belongs to another owner")]
    ForeignScope,
    #[error("model scratch scope {0} is not current")]
    StaleScope(u64),
    #[error("model scratch registration identity exhausted")]
    IdentityExhausted,
    #[error("model scratch byte arithmetic overflowed")]
    AllocationOverflow,
    #[error("model scratch allocation ends at {end:#x}, beyond 0xFA000 bytes")]
    Capacity { end: usize },
    #[error("model scratch allocation belongs to another owner")]
    ForeignAllocation,
    #[error("model scratch allocation mark {0} is no longer registered")]
    RetiredAllocation(u64),
    #[error("model scratch allocation layout differs from its registration")]
    LayoutMismatch,
    #[error("model scratch retirement requires mark {expected}, received {actual}")]
    AllocationOrder { expected: u64, actual: u64 },
    #[error("model scratch normal return still has {0} registered allocations")]
    UnretiredAllocations(usize),
    #[error("model scratch cursor {actual:#x} differs from registered {expected:#x}")]
    Cursor { expected: usize, actual: usize },
    #[error("model scratch {kind} row {index}/{count} field {offset}+{width} is invalid")]
    SlotRange {
        kind: &'static str,
        index: u16,
        count: u16,
        offset: usize,
        width: usize,
    },
    #[error("model scratch read {offset:#x}+{width} is outside the arena")]
    PoolRange { offset: usize, width: usize },
    #[error("signed shade {slot} reaches unmapped model scratch offset {offset}")]
    MappedShade { slot: i32, offset: i64 },
}

impl fmt::Debug for ProcessModelScratch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(arena) => f
                .debug_struct("ProcessModelScratch")
                .field("state", &arena.state)
                .field("cursor", &arena.cursor)
                .field("registered_allocations", &arena.marks.len())
                .finish(),
            Err(_) => f.write_str("ProcessModelScratch(<borrowed>)"),
        }
    }
}

impl ProcessModelScratch {
    /// Root's intrinsic PE API produces this nonforgeable generic proof. The
    /// entire pool AND cursor must be in the supported loader-zero virtual tail.
    /// Calling this creates a distinct process owner; never use it per draw.
    pub fn native_loader_zero(proof: &FrontendLoaderZeroRange) -> Result<Self, ModelScratchError> {
        if proof.start() > NATIVE_POOL || proof.end() < NATIVE_CURSOR_END {
            return Err(ModelScratchError::LoaderZeroProof {
                start: proof.start(),
                end: proof.end(),
            });
        }
        Ok(Self::with_initial_byte(Some(0)))
    }

    /// Explicit supplied fixture domain. Unknown bytes never become zero.
    pub fn isolated_unknown_fixture() -> Self {
        Self::with_initial_byte(None)
    }

    fn with_initial_byte(byte: Option<u8>) -> Self {
        Self(Rc::new(RefCell::new(Arena {
            owner: Rc::new(()),
            bytes: vec![byte; MODEL_SCRATCH_BYTES],
            cursor: 0,
            scope_serial: 0,
            allocation_serial: 0,
            state: ModelScratchDrawState::Ready,
            marks: Vec::new(),
            journal: None,
        })))
    }

    /// Port option: keep the arena as it is, its cursor, registrations and
    /// draw state, and from now each byte the draws allocate over, which
    /// [`Self::restore`] puts back, so the draws between leave no residue
    /// for later draws to read (the world draw's view shown, drawn apart
    /// from the game's). The one process owner goes on.
    pub fn save(&self) -> Result<ModelScratchSave, ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        if arena.journal.is_some() {
            return Err(ModelScratchError::SaveOpen);
        }
        arena.journal = Some((arena.cursor, Vec::new()));
        Ok(ModelScratchSave {
            owner: Rc::clone(&arena.owner),
            cursor: arena.cursor,
            scope_serial: arena.scope_serial,
            allocation_serial: arena.allocation_serial,
            state: arena.state.clone(),
            marks: arena.marks.clone(),
        })
    }

    /// Put the arena back as [`Self::save`] kept it. A failed restore
    /// closes the save too, which leaves the bytes as the draws left them.
    pub fn restore(&self, save: ModelScratchSave) -> Result<(), ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        let journal = arena.journal.take();
        if !Rc::ptr_eq(&arena.owner, &save.owner) {
            return Err(ModelScratchError::ForeignScope);
        }
        let (from, bytes) = journal.ok_or(ModelScratchError::NoSave)?;
        arena.bytes[from..from + bytes.len()].copy_from_slice(&bytes);
        arena.cursor = save.cursor;
        arena.scope_serial = save.scope_serial;
        arena.allocation_serial = save.allocation_serial;
        arena.state = save.state;
        arena.marks = save.marks;
        Ok(())
    }

    /// Current byte cursor; reading it does not allocate, retire or reset storage.
    pub fn cursor(&self) -> Result<usize, ModelScratchError> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?
            .cursor)
    }

    pub fn state(&self) -> Result<ModelScratchDrawState, ModelScratchError> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?
            .state
            .clone())
    }

    pub fn begin_draw(&self) -> Result<ModelScratchDrawScope, ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        if !matches!(arena.state, ModelScratchDrawState::Ready) {
            return Err(ModelScratchError::DrawState {
                expected: "Ready",
                actual: arena.state.clone(),
            });
        }
        let serial = arena
            .scope_serial
            .checked_add(1)
            .ok_or(ModelScratchError::IdentityExhausted)?;
        let scope = ModelScratchDrawScope {
            owner: Rc::clone(&arena.owner),
            serial,
            entry_cursor: arena.cursor,
        };
        arena.scope_serial = serial;
        arena.state = ModelScratchDrawState::Running(scope.clone());
        Ok(scope)
    }

    pub fn allocate(
        &self,
        scope: &ModelScratchDrawScope,
        vertices: u16,
        shades: u16,
    ) -> Result<ModelScratchAllocation, ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_scope(scope, false)?;
        let shade_base = arena
            .cursor
            .checked_add(usize::from(vertices) * 24)
            .ok_or(ModelScratchError::AllocationOverflow)?;
        let end = shade_base
            .checked_add(usize::from(shades) * 4)
            .ok_or(ModelScratchError::AllocationOverflow)?;
        if end > MODEL_SCRATCH_BYTES {
            return Err(ModelScratchError::Capacity { end });
        }
        let serial = arena
            .allocation_serial
            .checked_add(1)
            .ok_or(ModelScratchError::IdentityExhausted)?;
        let allocation = ModelScratchAllocation {
            layout: ModelScratchLayout {
                start: arena.cursor,
                vertex_count: vertices,
                shade_base,
                shade_count: shades,
                end,
            },
            mark: RegisteredAllocationMark {
                owner: Rc::clone(&arena.owner),
                scope: scope.serial,
                serial,
            },
        };
        let start = arena.cursor;
        let Arena { bytes, journal, .. } = &mut *arena;
        if let Some((from, kept)) = journal {
            // Allocations stack up from the save's cursor; keep each byte
            // before its first allocation over it.
            if start < *from {
                kept.splice(0..0, bytes[start..*from].iter().copied());
                *from = start;
            }
            let covered = *from + kept.len();
            if end > covered {
                kept.extend_from_slice(&bytes[covered..end]);
            }
        }
        arena.allocation_serial = serial;
        arena.cursor = end;
        arena.marks.push(allocation.clone());
        Ok(allocation)
    }

    /// Only explicit normal retirement unregisters this mark. Bytes survive.
    pub fn retire(
        &self,
        scope: &ModelScratchDrawScope,
        mark: &RegisteredAllocationMark,
    ) -> Result<(), ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_scope(scope, false)?;
        arena.require_mark(mark)?;
        let top = arena
            .marks
            .last()
            .ok_or(ModelScratchError::RetiredAllocation(mark.serial))?;
        if top.mark.serial != mark.serial {
            return Err(ModelScratchError::AllocationOrder {
                expected: top.mark.serial,
                actual: mark.serial,
            });
        }
        if arena.cursor != top.layout.end {
            return Err(ModelScratchError::Cursor {
                expected: top.layout.end,
                actual: arena.cursor,
            });
        }
        let old_cursor = top.layout.start;
        arena.marks.pop();
        arena.cursor = old_cursor;
        Ok(())
    }

    pub fn finish_draw(
        &self,
        scope: &ModelScratchDrawScope,
        exit: ModelScratchDrawExit,
    ) -> Result<(), ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_scope(scope, matches!(exit, ModelScratchDrawExit::StoppedOnError))?;
        if matches!(exit, ModelScratchDrawExit::StoppedOnError) {
            arena.state = ModelScratchDrawState::Stopped(scope.clone());
            return Ok(());
        }
        if !arena.marks.is_empty() {
            let count = arena.marks.len();
            arena.state = ModelScratchDrawState::Stopped(scope.clone());
            return Err(ModelScratchError::UnretiredAllocations(count));
        }
        if arena.cursor != scope.entry_cursor {
            arena.state = ModelScratchDrawState::Stopped(scope.clone());
            return Err(ModelScratchError::Cursor {
                expected: scope.entry_cursor,
                actual: arena.cursor,
            });
        }
        arena.state = ModelScratchDrawState::Ready;
        Ok(())
    }

    /// Explicit feature recovery uses actual registered old marks, LIFO. It
    /// preserves all bytes and is neither native normal return nor fake success.
    pub fn recover_stopped_draw(
        &self,
        scope: &ModelScratchDrawScope,
        _: RegisteredMarkUnwindApproximation,
    ) -> Result<(), ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_scope(scope, true)?;
        if !matches!(arena.state, ModelScratchDrawState::Stopped(_)) {
            return Err(ModelScratchError::DrawState {
                expected: "Stopped",
                actual: arena.state.clone(),
            });
        }
        // Validate the complete internally registered chain before changing it.
        let mut cursor = arena.cursor;
        for allocation in arena.marks.iter().rev() {
            arena.require_mark(&allocation.mark)?;
            if allocation.layout.end != cursor {
                return Err(ModelScratchError::Cursor {
                    expected: allocation.layout.end,
                    actual: cursor,
                });
            }
            cursor = allocation.layout.start;
        }
        if cursor != scope.entry_cursor {
            return Err(ModelScratchError::Cursor {
                expected: scope.entry_cursor,
                actual: cursor,
            });
        }
        while let Some(allocation) = arena.marks.pop() {
            arena.cursor = allocation.layout.start;
        }
        arena.state = ModelScratchDrawState::Ready;
        Ok(())
    }

    /// Actual whole-pool snapshot, also usable to inspect a stopped/retired
    /// prefix. No field, knownness or allocation defaults are synthesized.
    pub fn read_pool<const N: usize>(
        &self,
        offset: usize,
    ) -> Result<[Option<u8>; N], ModelScratchError> {
        let arena = self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        let end = offset
            .checked_add(N)
            .filter(|end| *end <= MODEL_SCRATCH_BYTES)
            .ok_or(ModelScratchError::PoolRange { offset, width: N })?;
        Ok(std::array::from_fn(|i| arena.bytes[offset..end][i]))
    }

    /// Validate read custody even when a view has no slots to read.
    pub fn validate_allocation(
        &self,
        allocation: &ModelScratchAllocation,
    ) -> Result<(), ModelScratchError> {
        let arena = self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_allocation(allocation, false)
    }

    pub fn read_vertex<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
    ) -> Result<[Option<u8>; N], ModelScratchError> {
        self.read_slot(allocation, index, offset, SlotKind::Vertex)
    }

    /// Every vertex slot of an allocation in order, custody checked once.
    pub fn read_vertices(
        &self,
        allocation: &ModelScratchAllocation,
        mut each: impl FnMut([Option<u8>; 24]),
    ) -> Result<(), ModelScratchError> {
        let arena = self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_allocation(allocation, false)?;
        let start = allocation.layout.start;
        for index in 0..usize::from(allocation.layout.vertex_count) {
            let at = start + index * 24;
            each(std::array::from_fn(|i| arena.bytes[at + i]));
        }
        Ok(())
    }

    pub fn write_vertex<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
        bytes: [Option<u8>; N],
    ) -> Result<(), ModelScratchError> {
        self.write_slot(allocation, index, offset, bytes, SlotKind::Vertex)
    }

    pub fn read_shade<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
    ) -> Result<[Option<u8>; N], ModelScratchError> {
        self.read_slot(allocation, index, offset, SlotKind::Shade)
    }

    pub fn write_shade<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
        bytes: [Option<u8>; N],
    ) -> Result<(), ModelScratchError> {
        self.write_slot(allocation, index, offset, bytes, SlotKind::Shade)
    }

    /// Native signed shade references may reach parent vertices/shades or old
    /// allocation bytes outside the declared local shade count, but never OOB.
    pub fn read_mapped_shade(
        &self,
        allocation: &ModelScratchAllocation,
        slot: i32,
    ) -> Result<[Option<u8>; 4], ModelScratchError> {
        let arena = self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        arena.require_allocation(allocation, false)?;
        let offset = allocation.layout.shade_base as i64 + i64::from(slot) * 4;
        if !(-PRE_POOL_ZERO_BYTES..=(MODEL_SCRATCH_BYTES - 4) as i64).contains(&offset) {
            return Err(ModelScratchError::MappedShade { slot, offset });
        }
        Ok(std::array::from_fn(|i| {
            match usize::try_from(offset + i as i64) {
                Ok(at) => arena.bytes[at],
                // Below the pool: the software rasterizer's globals, zero in a
                // Direct3D run (see PRE_POOL_ZERO_BYTES).
                Err(_) => Some(0),
            }
        }))
    }

    fn read_slot<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
        kind: SlotKind,
    ) -> Result<[Option<u8>; N], ModelScratchError> {
        let arena = self
            .0
            .try_borrow()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        let at = arena.slot_offset(allocation, index, offset, N, kind, false)?;
        Ok(std::array::from_fn(|i| arena.bytes[at + i]))
    }

    fn write_slot<const N: usize>(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
        bytes: [Option<u8>; N],
        kind: SlotKind,
    ) -> Result<(), ModelScratchError> {
        let mut arena = self
            .0
            .try_borrow_mut()
            .map_err(|_| ModelScratchError::BorrowConflict)?;
        let at = arena.slot_offset(allocation, index, offset, N, kind, true)?;
        arena.bytes[at..at + N].copy_from_slice(&bytes);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SlotKind {
    Vertex,
    Shade,
}

impl Arena {
    fn require_scope(
        &self,
        scope: &ModelScratchDrawScope,
        allow_stopped: bool,
    ) -> Result<(), ModelScratchError> {
        if !Rc::ptr_eq(&self.owner, &scope.owner) {
            return Err(ModelScratchError::ForeignScope);
        }
        let current = match &self.state {
            ModelScratchDrawState::Running(current) => current,
            ModelScratchDrawState::Stopped(current) if allow_stopped => current,
            _ => {
                return Err(ModelScratchError::DrawState {
                    expected: "Running (or explicitly stopped)",
                    actual: self.state.clone(),
                })
            }
        };
        if current != scope {
            return Err(ModelScratchError::StaleScope(scope.serial));
        }
        Ok(())
    }

    fn require_mark(
        &self,
        mark: &RegisteredAllocationMark,
    ) -> Result<&ModelScratchAllocation, ModelScratchError> {
        if !Rc::ptr_eq(&self.owner, &mark.owner) {
            return Err(ModelScratchError::ForeignAllocation);
        }
        let scope = match &self.state {
            ModelScratchDrawState::Running(scope) | ModelScratchDrawState::Stopped(scope) => scope,
            ModelScratchDrawState::Ready => {
                return Err(ModelScratchError::RetiredAllocation(mark.serial))
            }
        };
        if mark.scope != scope.serial {
            return Err(ModelScratchError::RetiredAllocation(mark.serial));
        }
        // From the top: reads nearly always reach the newest allocation.
        self.marks
            .iter()
            .rev()
            .find(|entry| entry.mark.serial == mark.serial)
            .ok_or(ModelScratchError::RetiredAllocation(mark.serial))
    }

    fn require_allocation(
        &self,
        allocation: &ModelScratchAllocation,
        write: bool,
    ) -> Result<(), ModelScratchError> {
        let registered = self.require_mark(&allocation.mark)?;
        if registered.layout != allocation.layout {
            return Err(ModelScratchError::LayoutMismatch);
        }
        if write && !matches!(self.state, ModelScratchDrawState::Running(_)) {
            return Err(ModelScratchError::DrawState {
                expected: "Running",
                actual: self.state.clone(),
            });
        }
        Ok(())
    }

    fn slot_offset(
        &self,
        allocation: &ModelScratchAllocation,
        index: u16,
        offset: usize,
        width: usize,
        kind: SlotKind,
        write: bool,
    ) -> Result<usize, ModelScratchError> {
        self.require_allocation(allocation, write)?;
        let (name, base, count, stride) = match kind {
            SlotKind::Vertex => (
                "vertex",
                allocation.layout.start,
                allocation.layout.vertex_count,
                24,
            ),
            SlotKind::Shade => (
                "shade",
                allocation.layout.shade_base,
                allocation.layout.shade_count,
                4,
            ),
        };
        if index >= count || offset.checked_add(width).is_none_or(|end| end > stride) {
            return Err(ModelScratchError::SlotRange {
                kind: name,
                index,
                count,
                offset,
                width,
            });
        }
        Ok(base + usize::from(index) * stride + offset)
    }
}

#[cfg(test)]
#[path = "model_scratch_tests.rs"]
mod tests;
