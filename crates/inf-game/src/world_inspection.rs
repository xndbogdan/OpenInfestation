//! Read-only H14 -> H13 -> initial H8 and proven H9 inputs in one resident state.
//!
//! This uses the real registration winners, including shared/family banks and
//! prior unloads. It never activates a missing resource or executes a constructor,
//! placement command, terrain adjustment, or model program.

use std::{collections::HashMap, path::PathBuf};

use inf_formats::{
    decode_instance_command, parse_constructor_asset_bank, parse_world_instance_bank, ByteSpan,
    ConstructorAsset, ConstructorAssetBank, ConstructorPayloadView, ConstructorResource,
    ConstructorSoundBinding, InstanceCommandRequest, InstanceCommandTarget, InstanceCommandView,
    OverlayKey, SoundEntry, WorldInstanceAssets, WorldResource,
};
use serde::Serialize;
use thiserror::Error;

use crate::{
    HandlerResources, ResidentRegistration, ResidentResource, ResidentRuntime, ResourceSelection,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidentAssetSource {
    pub ovl: PathBuf,
    pub overlay: OverlayKey,
    pub registration: ResidentRegistration,
}

impl ResidentAssetSource {
    fn from_resource(resource: &ResidentResource<'_>) -> Self {
        Self {
            ovl: resource.overlay.path().to_owned(),
            overlay: resource.overlay.key().clone(),
            registration: resource.registration.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidentWorldInspection {
    pub schema_version: u32,
    pub policy: &'static str,
    pub selection: ResourceSelection,
    pub active_leaves: Vec<u8>,
    pub source: ResidentAssetSource,
    pub main_payload: ByteSpan,
    pub world: WorldResource,
    pub instance_assets: WorldInstanceAssets,
    /// Same order as world.placements; indices are never deduplicated.
    pub placement_links: Vec<PlacementAssetLinks>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlacementAssetLinks {
    pub placement_index: u32,
    pub constructor: ConstructorReference,
    /// None means H13 could not be resolved, so no class-specific view was attempted.
    /// An empty vector means the resolved placement has no commands.
    pub command_views: Option<Vec<InstanceCommandView>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ConstructorReference {
    NegativeArgument { signed_slot: i16 },
    NotResident { global_slot: u16 },
    Resolved(Box<InspectedConstructor>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InspectedConstructor {
    pub source: ResidentAssetSource,
    pub resource: ConstructorResource,
    pub asset: ConstructorAsset,
    pub initial_model: InitialModelReference,
    /// Proven H13 sound inputs only, in authored field order. An empty vector
    /// does not prove that the class never uses any other sound resources.
    pub sound_bindings: Vec<InspectedConstructorSound>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InspectedConstructorSound {
    pub binding: ConstructorSoundBinding,
    pub resource: ConstructorSoundReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ConstructorSoundReference {
    DisabledZero,
    NegativeArgument { signed_slot: i16 },
    NotResident { global_slot: u16 },
    Resolved(Box<InspectedSound>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InspectedSound {
    pub source: ResidentAssetSource,
    /// Exact registered H9 record. Alias targets are retained, not recursively
    /// resolved or executed as playback; PCM stays in its owning OVL span.
    pub entry: SoundEntry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum InitialModelReference {
    /// No supported constructor profile establishes this header word's use.
    BindingUnproven,
    /// This context initializer does not consume the H13 header model word.
    NotConsumed,
    /// The exact -1 argument takes 0047A050's disabled branch.
    Disabled,
    /// Other negative arguments skip model lookup, without becoming slot zero.
    NegativeArgument {
        signed_slot: i16,
    },
    NotResident {
        global_slot: u16,
    },
    Resolved(InspectedModel),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InspectedModel {
    pub source: ResidentAssetSource,
    pub content_span: ByteSpan,
    pub name_bytes: Option<Vec<u8>>,
    pub name_ascii: Option<String>,
}

#[derive(Debug, Error)]
pub enum WorldInspectionError {
    #[error("Handler-14 global slot {global_slot} is not resident")]
    WorldNotResident { global_slot: u16 },
    #[error("{source_info:?} does not resolve to its registered local record")]
    Registration { source_info: ResidentAssetSource },
    #[error("{source_info:?}: {error}")]
    Decode {
        source_info: ResidentAssetSource,
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Inspect the current resident world without changing resource ownership.
/// Missing references stay visible; only malformed source or inconsistent
/// registrations fail the snapshot. These are authored initial inputs, not
/// a report of executed object state or final visual assignments.
pub fn inspect_world_assets(
    runtime: &ResidentRuntime,
    global_slot: u16,
) -> Result<ResidentWorldInspection, WorldInspectionError> {
    let resource = runtime
        .resource(14, global_slot)
        .ok_or(WorldInspectionError::WorldNotResident { global_slot })?;
    let source = ResidentAssetSource::from_resource(&resource);
    let region = resource
        .overlay
        .layout()
        .regions
        .get(resource.registration.region_index)
        .ok_or_else(|| WorldInspectionError::Registration {
            source_info: source.clone(),
        })?;
    let bank = parse_world_instance_bank(resource.overlay.bytes(), region).map_err(|error| {
        WorldInspectionError::Decode {
            source_info: source.clone(),
            error: Box::new(error),
        }
    })?;
    let world = bank
        .worlds
        .entries
        .get(usize::from(resource.registration.local_index))
        .ok_or_else(|| WorldInspectionError::Registration {
            source_info: source.clone(),
        })?
        .clone();
    let instance_assets = bank.instances[usize::from(resource.registration.local_index)].clone();
    let mut inspection = Inspection {
        runtime,
        constructors: HashMap::new(),
    };
    let placement_links = world
        .placements
        .iter()
        .zip(&instance_assets.placements)
        .map(|(placement, instance)| {
            let constructor = inspection.constructor(placement.constructor_slot())?;
            let command_views = if let ConstructorReference::Resolved(class) = &constructor {
                Some(
                    instance
                        .commands
                        .iter()
                        .map(|command| {
                            decode_instance_command(InstanceCommandRequest {
                                read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                                source: resource.overlay.bytes(),
                                main_payload: bank.worlds.main_payload,
                                relocations: &bank.worlds.relocations,
                                command_offset: command.allocation_offset,
                                target: InstanceCommandTarget::Constructor(
                                    class.resource.constructor_selector,
                                ),
                            })
                            .map_err(|error| {
                                WorldInspectionError::Decode {
                                    source_info: source.clone(),
                                    error: Box::new(error),
                                }
                            })
                        })
                        .collect::<Result<_, _>>()?,
                )
            } else {
                None
            };
            Ok(PlacementAssetLinks {
                placement_index: placement.index,
                constructor,
                command_views,
            })
        })
        .collect::<Result<_, WorldInspectionError>>()?;
    let summary = runtime.summary();
    Ok(ResidentWorldInspection {
        schema_version: 5,
        policy: "Read-only resident registrations. H13 model and sound bindings are authored inputs; class initialization and ordered H14 commands may change live state. H9 aliases retain their registered record without following native pointer-fixup lifetime or playback. No constructors, lazy loads, terrain adjustment or model programs have executed. All spans address the source OVL, not relocated memory.",
        selection: summary.selection,
        active_leaves: summary.active_leaves,
        source,
        main_payload: bank.worlds.main_payload,
        world,
        instance_assets,
        placement_links,
    })
}

struct Inspection<'a> {
    runtime: &'a ResidentRuntime,
    constructors: HashMap<(OverlayKey, usize), ConstructorAssetBank>,
}

impl Inspection<'_> {
    fn constructor(&mut self, slot: i16) -> Result<ConstructorReference, WorldInspectionError> {
        let Ok(global_slot) = u16::try_from(slot) else {
            return Ok(ConstructorReference::NegativeArgument { signed_slot: slot });
        };
        let Some(resource) = self.runtime.resource(13, global_slot) else {
            return Ok(ConstructorReference::NotResident { global_slot });
        };
        let source = ResidentAssetSource::from_resource(&resource);
        let key = (source.overlay.clone(), resource.registration.region_index);
        // Cache only inside this immutable snapshot. A later resource reload
        // must perform fresh lookups and cannot reuse an earlier bank winner.
        if !self.constructors.contains_key(&key) {
            let region = resource
                .overlay
                .layout()
                .regions
                .get(key.1)
                .ok_or_else(|| WorldInspectionError::Registration {
                    source_info: source.clone(),
                })?;
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).map_err(
                |error| WorldInspectionError::Decode {
                    source_info: source.clone(),
                    error: Box::new(error),
                },
            )?;
            self.constructors.insert(key.clone(), bank);
        }
        let bank = &self.constructors[&key];
        let local = usize::from(resource.registration.local_index);
        let Some((prefix, asset)) = bank.prefixes.entries.get(local).zip(bank.assets.get(local))
        else {
            return Err(WorldInspectionError::Registration {
                source_info: source,
            });
        };
        let sound_bindings = match &asset.payload {
            ConstructorPayloadView::SharedActor(actor) => actor
                .derived
                .as_ref()
                .map(|derived| derived.sound_bindings.as_slice())
                .unwrap_or_default(),
            _ => &[],
        }
        .iter()
        .map(|&binding| {
            self.sound(binding)
                .map(|resource| InspectedConstructorSound { binding, resource })
        })
        .collect::<Result<_, _>>()?;
        Ok(ConstructorReference::Resolved(Box::new(
            InspectedConstructor {
                source,
                resource: prefix.clone(),
                asset: asset.clone(),
                initial_model: self.model(asset)?,
                sound_bindings,
            },
        )))
    }

    fn sound(
        &self,
        binding: ConstructorSoundBinding,
    ) -> Result<ConstructorSoundReference, WorldInspectionError> {
        if binding.signed_slot == 0 {
            return Ok(ConstructorSoundReference::DisabledZero);
        }
        let Ok(global_slot) = u16::try_from(binding.signed_slot) else {
            return Ok(ConstructorSoundReference::NegativeArgument {
                signed_slot: binding.signed_slot,
            });
        };
        let Some(resource) = self.runtime.resource(9, global_slot) else {
            return Ok(ConstructorSoundReference::NotResident { global_slot });
        };
        let source = ResidentAssetSource::from_resource(&resource);
        let entry = match resource
            .overlay
            .handlers()
            .get(resource.registration.region_index)
        {
            Some(HandlerResources::Sounds(bank)) => bank
                .entries
                .get(usize::from(resource.registration.local_index)),
            _ => None,
        }
        .ok_or_else(|| WorldInspectionError::Registration {
            source_info: source.clone(),
        })?;
        Ok(ConstructorSoundReference::Resolved(Box::new(
            InspectedSound {
                source,
                entry: entry.clone(),
            },
        )))
    }

    fn model(
        &self,
        asset: &ConstructorAsset,
    ) -> Result<InitialModelReference, WorldInspectionError> {
        let Some(binding) = asset.model_binding else {
            return Ok(if asset.dispatch.is_some() {
                InitialModelReference::NotConsumed
            } else {
                InitialModelReference::BindingUnproven
            });
        };
        if binding.signed_slot == -1 {
            return Ok(InitialModelReference::Disabled);
        }
        let Ok(global_slot) = u16::try_from(binding.signed_slot) else {
            return Ok(InitialModelReference::NegativeArgument {
                signed_slot: binding.signed_slot,
            });
        };
        let Some(resource) = self.runtime.resource(8, global_slot) else {
            return Ok(InitialModelReference::NotResident { global_slot });
        };
        let source = ResidentAssetSource::from_resource(&resource);
        let model = match resource
            .overlay
            .handlers()
            .get(resource.registration.region_index)
        {
            Some(HandlerResources::Models(bank)) => bank
                .entries
                .get(usize::from(resource.registration.local_index)),
            _ => None,
        }
        .ok_or_else(|| WorldInspectionError::Registration {
            source_info: source.clone(),
        })?;
        Ok(InitialModelReference::Resolved(InspectedModel {
            source,
            content_span: model.content_span,
            name_bytes: model
                .name
                .as_ref()
                .and_then(|name| name.raw(resource.overlay.bytes()))
                .map(<[u8]>::to_vec),
            name_ascii: model
                .name
                .as_ref()
                .and_then(|name| name.ascii(resource.overlay.bytes()))
                .map(str::to_owned),
        }))
    }
}

#[cfg(test)]
#[path = "world_inspection_tests.rs"]
mod tests;
