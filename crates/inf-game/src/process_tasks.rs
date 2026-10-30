//! Process task storage, 0055F100[19], owned by module 0053BDC0.
//!
//! 00482C40/50 retain/release that module through 0048EE20/0048EEC0.
//! Its 004829A0 constructor creates the tasks; 00482C00 destroys registered
//! owners in slot order. Storage owns allocation/lifetime; relation executes
//! the recovered virtual +00 query bodies.

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::object_runtime::ActorHandle;

pub const TASK_CLASS_COUNT: usize = 19;

static NEXT_TASK_RUNTIME: AtomicU64 = AtomicU64::new(1);

pub mod damage;
pub mod relation;

/// Actual allocation order in 004829A0. Classes 0/1/18 have specialized
/// relation-query virtual methods; all other classes use 00482CE0.
pub const TASK_CONSTRUCTION_ORDER: [u32; TASK_CLASS_COUNT] = [
    0, 1, 18, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17,
];

/// A stable portable allocation identity, separate from its registered slot.
/// A duplicate constructor can allocate an object without replacing that slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskOwner {
    runtime: u64,
    allocation: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskCallback {
    /// 005280A8 → 004027B0; constant zero.
    Zero4027b0,
    /// 005280A4 → 004EFBF0; constant zero.
    Zero4efbf0,
    /// 005280A0 → 00482C90; required actor +5C/+7C query on class one.
    ComponentClassification,
    /// 0052809C → 00482CE0; actor +B4 / process network relation query.
    TeamClassification,
}

impl TaskCallback {
    fn for_classification(classification: u32) -> Self {
        match classification {
            0 => Self::Zero4027b0,
            1 => Self::Zero4efbf0,
            18 => Self::ComponentClassification,
            _ => Self::TeamClassification,
        }
    }
}

/// Reached state of a native 0x18-byte task. The constructor writes only the
/// vtable, DWORD +4 and DWORD +8. WORDs +0C..+14 remain unowned until reset;
/// padding +16 is neither initialized nor interpreted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub classification: u32,
    pub source: ActorHandle,
    pub callback: TaskCallback,
    pub counters: [Option<u16>; 5],
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum TaskStorageError {
    #[error("task classification {0} is outside process 0055F100[19]")]
    Classification(u32),
    #[error("task allocation {0:?} is no longer live")]
    MissingOwner(TaskOwner),
    #[error("native task slot {0} is null")]
    NullOwner(u32),
    #[error("process task allocation failed for classification {0}")]
    Allocation(u32),
}

/// Storage shared by frontend/world dependencies of actor module 00478A70.
/// It survives world replacement. Reference count zero is the native unloaded
/// state; retain/release execute constructor/destructor only on the edge.
#[derive(Debug)]
pub struct ProcessTaskRuntime {
    identity: u64,
    references: u32,
    slots: [Option<TaskOwner>; TASK_CLASS_COUNT],
    allocations: Vec<Option<TaskRecord>>,
}

impl Default for ProcessTaskRuntime {
    fn default() -> Self {
        let identity = NEXT_TASK_RUNTIME
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("process task runtime identity exhausted");
        Self {
            identity,
            references: 0,
            slots: [None; TASK_CLASS_COUNT],
            allocations: Vec::new(),
        }
    }
}

impl ProcessTaskRuntime {
    pub fn references(&self) -> u32 {
        self.references
    }

    pub fn owner(&self, classification: u32) -> Result<Option<TaskOwner>, TaskStorageError> {
        let index = task_index(classification)?;
        Ok(self.slots[index])
    }

    pub fn record(&self, owner: TaskOwner) -> Result<&TaskRecord, TaskStorageError> {
        if owner.runtime != self.identity {
            return Err(TaskStorageError::MissingOwner(owner));
        }
        self.allocations
            .get(owner.allocation)
            .and_then(Option::as_ref)
            .ok_or(TaskStorageError::MissingOwner(owner))
    }

    fn record_mut(&mut self, owner: TaskOwner) -> Result<&mut TaskRecord, TaskStorageError> {
        if owner.runtime != self.identity {
            return Err(TaskStorageError::MissingOwner(owner));
        }
        self.allocations
            .get_mut(owner.allocation)
            .and_then(Option::as_mut)
            .ok_or(TaskStorageError::MissingOwner(owner))
    }

    /// 0048EE20 + 004829A0. Non-null Rust allocations are admitted as actual
    /// owners; an allocation failure is an explicit portable error retaining
    /// the reached prefix, never an invented null task or successful no-op.
    pub fn retain(&mut self, process_null: ActorHandle) -> Result<(), TaskStorageError> {
        let previous = self.references;
        self.references = previous.wrapping_add(1);
        if previous == 0 {
            for classification in TASK_CONSTRUCTION_ORDER {
                self.construct(classification, process_null)?;
            }
        }
        Ok(())
    }

    /// 004829A0's per-object admission: allocate, execute 00482950, install
    /// the final specialized vtable. Publication occurs only when the class
    /// slot is null; an existing registered object's source is retained.
    pub fn construct(
        &mut self,
        classification: u32,
        process_null: ActorHandle,
    ) -> Result<TaskOwner, TaskStorageError> {
        let index = task_index(classification)?;
        self.allocations
            .try_reserve(1)
            .map_err(|_| TaskStorageError::Allocation(classification))?;
        let owner = TaskOwner {
            runtime: self.identity,
            allocation: self.allocations.len(),
        };
        self.allocations.push(Some(TaskRecord {
            classification,
            source: process_null,
            callback: TaskCallback::for_classification(classification),
            counters: [None; 5],
        }));
        if self.slots[index].is_none() {
            self.slots[index] = Some(owner);
        }
        Ok(owner)
    }

    /// 00482980 clears the classification slot unconditionally, even if this
    /// object is not its current registered owner. 00482C00 then frees it.
    pub fn destroy(&mut self, owner: TaskOwner) -> Result<(), TaskStorageError> {
        let classification = self.record(owner)?.classification;
        self.slots[task_index(classification)?] = None;
        self.allocations[owner.allocation] = None;
        Ok(())
    }

    /// 0048EEC0 + 00482C00. Underflow wraps like the native DWORD; callers
    /// must balance acquisitions. Unregistered duplicate allocations are not
    /// part of the native slot walk and are therefore not destroyed here.
    pub fn release(&mut self) -> Result<(), TaskStorageError> {
        self.references = self.references.wrapping_sub(1);
        if self.references == 0 {
            for classification in 0..TASK_CLASS_COUNT {
                if let Some(owner) = self.slots[classification] {
                    self.destroy(owner)?;
                    self.slots[classification] = None;
                }
            }
        }
        Ok(())
    }

    /// Direct DWORD +8 publication, used after 0047C6E0 has already executed
    /// 00482C60's guarded reads. The guard belongs to that caller, not storage.
    pub fn write_source(
        &mut self,
        owner: TaskOwner,
        source: ActorHandle,
    ) -> Result<(), TaskStorageError> {
        self.record_mut(owner)?.source = source;
        Ok(())
    }

    /// Exact 00482C60(source, enable). A process-null argument returns before
    /// dereferencing the owner; clear requires equality with the live +8.
    pub fn set_source(
        &mut self,
        owner: TaskOwner,
        source: ActorHandle,
        enable: bool,
        process_null: ActorHandle,
    ) -> Result<(), TaskStorageError> {
        if source == process_null {
            return Ok(());
        }
        let task = self.record_mut(owner)?;
        if enable {
            task.source = source;
        } else if task.source == source {
            task.source = process_null;
        }
        Ok(())
    }

    /// 0045CB70's reset writes each registered owner's WORDs +0C/+0E/+10/
    /// +12/+14 in slot order. A missing owner is a required error, retaining
    /// the already reset prefix; DWORD +8 is not reset.
    pub fn reset_counters(&mut self) -> Result<(), TaskStorageError> {
        for classification in 0..TASK_CLASS_COUNT {
            let owner = self.slots[classification]
                .ok_or(TaskStorageError::NullOwner(classification as u32))?;
            self.record_mut(owner)?.counters = [Some(0); 5];
        }
        Ok(())
    }
}

fn task_index(classification: u32) -> Result<usize, TaskStorageError> {
    if classification < TASK_CLASS_COUNT as u32 {
        Ok(classification as usize)
    } else {
        Err(TaskStorageError::Classification(classification))
    }
}

#[cfg(test)]
#[path = "process_tasks_tests.rs"]
mod tests;
