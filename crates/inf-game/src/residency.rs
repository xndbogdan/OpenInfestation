//! Native scene-tree resource lifetime (004EE1E0, 004EE6D0, 004EE160).
//!
//! Transactions stage immutable allocations and slot mutations before commit.
//! This safety boundary does not change the successful retail operation order.
//! Handler host resources and relocation callbacks are not executed here.

use std::collections::BTreeMap;
use std::sync::Arc;

use inf_formats::{OverlayKey, RequestKind, HANDLER_COUNT, RELOCATION_PHASE_ORDER};
use serde::Serialize;

use crate::installation::OverlayPhysicalReads;
use crate::loading_read::{ResourceReadHost, ResourceReadState};
use crate::residency_tree::{LeafOperation, SceneTree};
use crate::scene::phase_index;
use crate::{Installation, LoadedOverlay, OverlaySummary, ResourceScope, Result, SceneContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceSelection {
    pub difficulty: u8,
    pub language: u8,
}

impl ResourceSelection {
    fn context(self, level: u8) -> SceneContext {
        SceneContext::new(level, self.difficulty, self.language)
    }
}

/// Requests name resource ownership; none executes a session screen or actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResidencyRequest {
    Boot,
    Activate { level: u8 },
    Select { selection: ResourceSelection },
}

/// Real read callbacks receive this owner's committed table during staging.
/// This is TransactionalResidencyViewApproximation, not partial native pools.
pub trait ResidentResourceReadHost {
    fn sample_clock(&mut self) -> u64;
    fn invoke_loading_callback(
        &mut self,
        committed: &ResidentRuntime,
        state: &mut ResourceReadState,
        raw_progress: u32,
    ) -> u32;
}

pub enum ResidencyReads<'a> {
    NoCallbacks,
    Loading {
        state: &'a mut ResourceReadState,
        host: &'a mut dyn ResidentResourceReadHost,
    },
}

pub struct ResidencyApplyRequest<'a> {
    pub operation: ResidencyRequest,
    pub reads: ResidencyReads<'a>,
}

impl From<ResidencyRequest> for ResidencyApplyRequest<'_> {
    fn from(operation: ResidencyRequest) -> Self {
        Self {
            operation,
            reads: ResidencyReads::NoCallbacks,
        }
    }
}

struct CommittedResourceRead<'a> {
    committed: &'a ResidentRuntime,
    host: &'a mut dyn ResidentResourceReadHost,
}

impl ResourceReadHost for CommittedResourceRead<'_> {
    fn sample_clock(&mut self) -> u64 {
        self.host.sample_clock()
    }
    fn invoke_loading_callback(&mut self, state: &mut ResourceReadState, raw_progress: u32) -> u32 {
        self.host
            .invoke_loading_callback(self.committed, state, raw_progress)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResidencyEvent {
    OverlayLoaded { key: OverlayKey },
    OverlayReleased { key: OverlayKey },
    LeafActivated { level: u8 },
    LeafReleased { level: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidencyChange {
    pub request: ResidencyRequest,
    pub events: Vec<ResidencyEvent>,
}

/// Current slot provenance, not a persistent handle across mutations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidentRegistration {
    pub level: u8,
    pub phase: RequestKind,
    pub handler: u8,
    pub region_index: usize,
    pub local_index: u16,
    pub global_index: u16,
}

/// Current registration borrowed together with its immutable allocation. An
/// explicit Arc clone may retain source custody across a later slot replacement;
/// the registration itself still describes only this residency snapshot.
#[derive(Debug)]
pub struct ResidentResource<'a> {
    pub registration: &'a ResidentRegistration,
    pub overlay: &'a Arc<LoadedOverlay>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidentSummary {
    pub scope: ResourceScope,
    pub selection: ResourceSelection,
    /// Authored traversal order, not a claimed load sequence (use events).
    pub active_leaves: Vec<u8>,
    pub overlays: Vec<OverlaySummary>,
    pub total_bytes: usize,
    pub active_slots: [usize; HANDLER_COUNT],
}

type LeafAllocations = [Option<Arc<LoadedOverlay>>; 4];

#[derive(Clone, Debug)]
struct ResidentState {
    tree: SceneTree,
    selection: ResourceSelection,
    leaves: BTreeMap<u8, LeafAllocations>,
    slots: [Vec<Option<ResidentRegistration>>; HANDLER_COUNT],
}

// A display replacement stages a complete selector change while the old
// renderer still borrows its registrations. Clones share immutable allocations;
// committing the replacement happens only after the old renderer is retired.
#[derive(Clone, Debug)]
pub struct ResidentRuntime {
    installation: Installation,
    state: ResidentState,
    /// Successful applies that loaded or released anything. Registration
    /// values change only through those events, so an unchanged revision means
    /// lookups still resolve the same overlay allocations. Slot records may
    /// move when state is cloned on apply. Clones copy the counter.
    revision: u64,
}

impl ResidentRuntime {
    /// Starts inactive, as before PreLoad's root activation. Explicit `Boot`
    /// reproduces that activation; selector updates while inactive do no I/O.
    pub fn new(installation: Installation) -> Result<Self> {
        let preload = installation.preload();
        let state = ResidentState {
            tree: SceneTree::new(preload)?,
            selection: ResourceSelection {
                difficulty: preload.header.difficulty_flag,
                language: preload.header.language_flag,
            },
            leaves: BTreeMap::new(),
            slots: std::array::from_fn(|handler| {
                vec![None; preload.header.handler_capacities[handler] as usize]
            }),
        };
        Ok(Self {
            installation,
            state,
            revision: 0,
        })
    }

    /// See the `revision` field. Portable renderer custody uses this to decide
    /// when a residency snapshot must be retired; it has no native counterpart.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn resource(&self, handler: u8, global_index: u16) -> Option<ResidentResource<'_>> {
        let registration = self
            .state
            .slots
            .get(handler as usize)?
            .get(global_index as usize)?
            .as_ref()?;
        let overlay = self.state.leaves.get(&registration.level)?[phase_index(registration.phase)]
            .as_ref()?;
        Some(ResidentResource {
            registration,
            overlay,
        })
    }

    pub fn installation(&self) -> &Installation {
        &self.installation
    }

    /// The overlay leaves whose request tables (any of the four phases)
    /// register `global_index` in `handler`, in PreLoad order: the leaves an
    /// activation could load that resource from.
    pub fn leaves_registering(&self, handler: u8, global_index: u16) -> Vec<u8> {
        use inf_formats::RequestKind;
        self.installation
            .preload()
            .leaves()
            .filter(|leaf| {
                [
                    RequestKind::Base,
                    RequestKind::Difficulty,
                    RequestKind::Language,
                    RequestKind::DifficultyLanguage,
                ]
                .into_iter()
                .filter_map(|kind| leaf.requests(kind).get(usize::from(handler)))
                .any(|request| {
                    (0..request.count.max(0) as u16)
                        .any(|local| request.global_index(local) == Some(global_index))
                })
            })
            .map(|leaf| leaf.overlay_id)
            .collect()
    }

    pub(crate) fn resource_capacity(&self, handler: u8) -> Option<usize> {
        self.state.slots.get(usize::from(handler)).map(Vec::len)
    }

    pub fn summary(&self) -> ResidentSummary {
        let active_leaves = self.state.tree.active_leaves();
        let overlays: Vec<_> = active_leaves
            .iter()
            .flat_map(|level| self.state.leaves[level].iter().flatten())
            .map(|overlay| OverlaySummary {
                key: overlay.key().clone(),
                path: overlay.path().to_owned(),
                bytes: overlay.bytes().len(),
                handlers: overlay
                    .layout()
                    .regions
                    .iter()
                    .map(|region| region.handler.get())
                    .collect(),
            })
            .collect();
        ResidentSummary {
            scope: ResourceScope::ResidentResources,
            selection: self.state.selection,
            active_leaves,
            total_bytes: overlays.iter().map(|overlay| overlay.bytes).sum(),
            overlays,
            active_slots: std::array::from_fn(|handler| {
                self.state.slots[handler].iter().flatten().count()
            }),
        }
    }

    pub fn apply<'a>(
        &mut self,
        request: impl Into<ResidencyApplyRequest<'a>>,
    ) -> Result<ResidencyChange> {
        // Arc ownership keeps retained banks at the same allocation while a
        // failed file/typed parse leaves the entire old tree/table untouched.
        let mut next = self.state.clone();
        let mut events = Vec::new();
        let mut request = request.into();
        match &mut request.reads {
            ResidencyReads::NoCallbacks => next.apply_request(
                &self.installation,
                request.operation,
                &mut events,
                &mut OverlayPhysicalReads::NoCallbacks,
            )?,
            ResidencyReads::Loading { state, host } => {
                let mut callback = CommittedResourceRead {
                    committed: self,
                    host: &mut **host,
                };
                let mut reads = OverlayPhysicalReads::Loading {
                    state: &mut **state,
                    host: &mut callback,
                };
                next.apply_request(
                    &self.installation,
                    request.operation,
                    &mut events,
                    &mut reads,
                )?;
            }
        }
        self.state = next;
        if !events.is_empty() {
            self.revision += 1;
        }
        Ok(ResidencyChange {
            request: request.operation,
            events,
        })
    }
}

impl ResidentState {
    /// Stage the complete authored tree/select operation while read callbacks
    /// borrow the enclosing owner's committed registration table.
    fn apply_request(
        &mut self,
        installation: &Installation,
        request: ResidencyRequest,
        events: &mut Vec<ResidencyEvent>,
        reads: &mut OverlayPhysicalReads<'_>,
    ) -> Result<()> {
        match request {
            ResidencyRequest::Boot => {
                let operations = self.tree.boot();
                self.apply_leaves(installation, operations, events, reads)?;
            }
            ResidencyRequest::Activate { level } => {
                let operations = self.tree.request(level)?;
                self.apply_leaves(installation, operations, events, reads)?;
            }
            ResidencyRequest::Select { selection } => {
                let previous = self.selection;
                self.selection = selection;
                // 004EE160 traverses ALL active leaves for one phase before
                // starting the next phase. This is not a per-leaf full reload:
                // the root's +0C (difficulty), +10 (language) and +14 (both)
                // reach every active child through the branches (004EEC30,
                // 004EEC60, 004EEC90), and each leaf releases and reloads that
                // phase (004EE920, 004EE9E0, 004EEAA0, through 004EE4E0).
                for (changed, phase) in [
                    (
                        previous.difficulty != selection.difficulty,
                        RequestKind::Difficulty,
                    ),
                    (
                        previous.language != selection.language,
                        RequestKind::Language,
                    ),
                    (previous != selection, RequestKind::DifficultyLanguage),
                ] {
                    if changed {
                        for level in self.tree.active_leaves() {
                            self.unload_overlay(level, phase, events);
                            self.load_overlay(installation, level, phase, events, reads)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl ResidentState {
    fn apply_leaves(
        &mut self,
        installation: &Installation,
        operations: Vec<LeafOperation>,
        events: &mut Vec<ResidencyEvent>,
        reads: &mut OverlayPhysicalReads<'_>,
    ) -> Result<()> {
        for operation in operations {
            match operation {
                LeafOperation::Load { level } => {
                    self.leaves.insert(level, std::array::from_fn(|_| None));
                    for phase in RELOCATION_PHASE_ORDER {
                        self.load_overlay(installation, level, phase, events, reads)?;
                    }
                    events.push(ResidencyEvent::LeafActivated { level });
                }
                LeafOperation::Unload { level } => {
                    for phase in RELOCATION_PHASE_ORDER {
                        self.unload_overlay(level, phase, events);
                    }
                    self.leaves.remove(&level);
                    events.push(ResidencyEvent::LeafReleased { level });
                }
            }
        }
        Ok(())
    }

    fn load_overlay(
        &mut self,
        installation: &Installation,
        level: u8,
        phase: RequestKind,
        events: &mut Vec<ResidencyEvent>,
        reads: &mut OverlayPhysicalReads<'_>,
    ) -> Result<()> {
        let Some(overlay) =
            installation.prepare_overlay(self.selection.context(level), phase, reads)?
        else {
            return Ok(());
        };
        for (region_index, region) in overlay.layout().regions.iter().enumerate() {
            let handler = region.handler.get();
            let request = &overlay.requests().0[handler as usize];
            for local_index in 0..request.count as u16 {
                let global_index = request
                    .global_index(local_index)
                    .expect("validated mapping");
                self.slots[handler as usize][global_index as usize] = Some(ResidentRegistration {
                    level,
                    phase,
                    handler,
                    region_index,
                    local_index,
                    global_index,
                });
            }
        }
        events.push(ResidencyEvent::OverlayLoaded {
            key: overlay.key().clone(),
        });
        self.leaves
            .get_mut(&level)
            .expect("active leaf allocation owner")[phase_index(phase)] = Some(Arc::new(overlay));
        Ok(())
    }

    fn unload_overlay(&mut self, level: u8, phase: RequestKind, events: &mut Vec<ResidencyEvent>) {
        let Some(overlay) = self
            .leaves
            .get_mut(&level)
            .expect("active leaf allocation owner")[phase_index(phase)]
        .take() else {
            return;
        };
        // 004EE6D0 invokes each active handler's cleanup (its final table's
        // +0C, crate::native_pools), then 004EFC00 with
        // null for EVERY local mapping. It never checks the current owner or
        // restores an earlier registration. Keep those clears even if another
        // retained leaf or later phase last wrote this slot.
        for region in &overlay.layout().regions {
            let handler = region.handler.get() as usize;
            let request = &overlay.requests().0[handler];
            for local in 0..request.count as u16 {
                let global = request.global_index(local).expect("validated mapping");
                self.slots[handler][global as usize] = None;
            }
        }
        events.push(ResidencyEvent::OverlayReleased {
            key: overlay.key().clone(),
        });
    }
}
