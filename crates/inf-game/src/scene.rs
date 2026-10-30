use std::path::{Path, PathBuf};

use inf_formats::{
    HandlerRegion, OverlayKey, OverlayLayout, RequestKind, RequestTable, HANDLER_COUNT,
};
use serde::Serialize;

use crate::{HandlerResources, SceneContext};

#[derive(Debug)]
pub struct LoadedOverlay {
    pub(crate) key: OverlayKey,
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) requests: RequestTable,
    pub(crate) layout: OverlayLayout,
    /// Indexed in the same order as `layout.regions`.
    pub(crate) handlers: Vec<HandlerResources>,
}

impl LoadedOverlay {
    pub fn key(&self) -> &OverlayKey {
        &self.key
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn requests(&self) -> &RequestTable {
        &self.requests
    }

    pub fn layout(&self) -> &OverlayLayout {
        &self.layout
    }

    pub fn handlers(&self) -> &[HandlerResources] {
        &self.handlers
    }

    pub fn handler(&self, handler: u8) -> Option<(&HandlerRegion, &HandlerResources)> {
        self.layout
            .regions
            .iter()
            .zip(&self.handlers)
            .find(|(region, _)| region.handler.get() == handler)
    }
}

/// One `FUN_004EFC00` registration, retained even after a later overwrite.
/// Indices address immutable storage in the owning scene, not retail memory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceRegistration {
    pub overlay_index: usize,
    pub region_index: usize,
    pub handler: u8,
    pub local_index: u16,
    pub global_index: u16,
    /// Index in `LoadedScene::registrations` replaced by this registration.
    pub replaces: Option<usize>,
}

/// Immutable owned resources for one independent scene, including registrations
/// that lost an alias or later-phase overwrite. Pool holes remain absent.
#[derive(Debug)]
pub struct LoadedScene {
    context: SceneContext,
    overlays: Vec<LoadedOverlay>,
    registrations: Vec<ResourceRegistration>,
    slots: [Vec<Option<usize>>; HANDLER_COUNT],
}

impl LoadedScene {
    pub(crate) fn new(context: SceneContext, capacities: [u16; HANDLER_COUNT]) -> Self {
        Self {
            context,
            overlays: Vec::new(),
            registrations: Vec::new(),
            slots: std::array::from_fn(|handler| vec![None; capacities[handler] as usize]),
        }
    }

    pub fn context(&self) -> SceneContext {
        self.context
    }

    pub fn overlays(&self) -> &[LoadedOverlay] {
        &self.overlays
    }

    pub fn registrations(&self) -> &[ResourceRegistration] {
        &self.registrations
    }

    pub fn resource(&self, handler: u8, global_index: u16) -> Option<&ResourceRegistration> {
        let index = self
            .slots
            .get(handler as usize)?
            .get(global_index as usize)?;
        index.map(|index| &self.registrations[index])
    }

    pub fn summary(&self) -> SceneSummary {
        let handlers = std::array::from_fn(|handler| HandlerSummary {
            handler: handler as u8,
            capacity: self.slots[handler].len(),
            registrations: self
                .registrations
                .iter()
                .filter(|entry| entry.handler as usize == handler)
                .count(),
            active_slots: self.slots[handler].iter().flatten().count(),
            opaque: matches!(handler, 13 | 14),
        });
        SceneSummary {
            context: self.context,
            scope: ResourceScope::IndependentScene,
            total_bytes: self
                .overlays
                .iter()
                .map(|overlay| overlay.bytes.len())
                .sum(),
            registrations: self.registrations.len(),
            active_slots: handlers.iter().map(|handler| handler.active_slots).sum(),
            overlays: self
                .overlays
                .iter()
                .map(|overlay| OverlaySummary {
                    key: overlay.key.clone(),
                    path: overlay.path.clone(),
                    bytes: overlay.bytes.len(),
                    handlers: overlay
                        .layout
                        .regions
                        .iter()
                        .map(|region| region.handler.get())
                        .collect(),
                })
                .collect(),
            handlers,
        }
    }

    pub(crate) fn register_overlay(&mut self, overlay: LoadedOverlay) {
        let overlay_index = self.overlays.len();
        for (region_index, region) in overlay.layout.regions.iter().enumerate() {
            let handler = region.handler.get();
            let request = &overlay.requests.0[handler as usize];
            for local_index in 0..request.count as u16 {
                // Installation validates every request before any load. These
                // slots cannot be mutated by consumers of the public API.
                let global_index = request
                    .global_index(local_index)
                    .expect("validated mapping");
                let slot = &mut self.slots[handler as usize][global_index as usize];
                let replaces = slot.replace(self.registrations.len());
                self.registrations.push(ResourceRegistration {
                    overlay_index,
                    region_index,
                    handler,
                    local_index,
                    global_index,
                    replaces,
                });
            }
        }
        self.overlays.push(overlay);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceScope {
    IndependentScene,
    ResidentResources,
}

pub(crate) const fn phase_index(phase: RequestKind) -> usize {
    match phase {
        RequestKind::Base => 0,
        RequestKind::Difficulty => 1,
        RequestKind::Language => 2,
        RequestKind::DifficultyLanguage => 3,
    }
}

/// Inventory of resource preparation, not evidence of executable gameplay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneSummary {
    pub context: SceneContext,
    pub scope: ResourceScope,
    pub total_bytes: usize,
    pub registrations: usize,
    pub active_slots: usize,
    pub overlays: Vec<OverlaySummary>,
    pub handlers: [HandlerSummary; HANDLER_COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OverlaySummary {
    pub key: OverlayKey,
    pub path: PathBuf,
    pub bytes: usize,
    pub handlers: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandlerSummary {
    pub handler: u8,
    pub capacity: usize,
    pub registrations: usize,
    pub active_slots: usize,
    pub opaque: bool,
}
