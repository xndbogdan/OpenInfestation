//! Direct `PreLoad.dat`/OVL catalog construction through `inf-formats`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use inf_formats::native_basis::NativeBasisMath;
use inf_formats::{
    analyze_model_diagnostic, parse_material_bank, parse_model_bank, parse_overlay,
    parse_sprite_bank, parse_terrain_bank, HandlerRequest, MaterialBank, MaterialPalette8,
    MaterialValue, ModelDiagnosticProjection, ModelEntry, ModelInstructionKind, OverlayKey,
    OverlayVariant, Preload, RequestKind, RequestTable, SceneContext, SceneTree, SpriteEntry,
    TerrainEntry, MATERIAL_HANDLER, MODEL_HANDLER, SPRITE_HANDLER, TERRAIN_HANDLER,
};

#[derive(Debug)]
pub struct CatalogError(String);

impl CatalogError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelLocator {
    pub name: String,
    pub global_id: u16,
}

#[derive(Clone, Debug)]
pub struct ModelAsset {
    /// One logical Handler-8 global slot. Local shells and recursively linked
    /// composites are presentation modes of this same identity, not separate
    /// catalog families.
    pub locator: ModelLocator,
    pub variants: Vec<ModelVariant>,
    /// Deterministic presentation default. In a single-OVL catalog this is
    /// the variant left in the global slot by the final local registration.
    pub default_variant: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOccurrence {
    pub source: String,
    pub local_id: u16,
}

#[derive(Clone, Debug)]
pub struct ModelVariant {
    pub source_bytes: Arc<[u8]>,
    pub entry: ModelEntry,
    pub diagnostic: ModelDiagnosticProjection,
    /// Every physical registration whose exact content payload matches this
    /// variant, in deterministic numeric-overlay/local-slot order.
    pub occurrences: Vec<ModelOccurrence>,
}

impl ModelVariant {
    pub fn child_model_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.entry
            .program
            .instructions
            .iter()
            .filter_map(|instruction| {
                let ModelInstructionKind::ModelInstance(instance) = &instruction.kind else {
                    return None;
                };
                u16::try_from(instance.model.raw()).ok()
            })
    }
}

impl ModelAsset {
    pub fn variant(&self, index: usize) -> Option<&ModelVariant> {
        self.variants.get(index)
    }

    pub fn default_variant(&self) -> Option<&ModelVariant> {
        self.variant(self.default_variant)
    }
}

#[derive(Clone, Debug)]
pub struct SpriteAsset {
    pub source: String,
    pub global_id: Option<u16>,
    pub source_bytes: Arc<[u8]>,
    pub entry: SpriteEntry,
}

#[derive(Clone, Debug)]
pub struct MaterialAsset {
    pub source: String,
    pub global_id: Option<u16>,
    pub local_id: u16,
    pub value: MaterialValue,
}

#[derive(Clone, Debug)]
pub struct TerrainAsset {
    pub level: u8,
    pub source: String,
    pub entry_index: usize,
    pub source_bytes: Arc<[u8]>,
    pub entry: TerrainEntry,
    pub palette: Option<MaterialPalette8>,
}

#[derive(Clone, Debug, Default)]
pub struct AssetCatalog {
    pub models: Vec<ModelAsset>,
    pub sprites: Vec<SpriteAsset>,
    pub materials: Vec<MaterialAsset>,
    pub terrain: Vec<TerrainAsset>,
    native_basis_math: Option<NativeBasisMath>,
    native_basis_math_error: Option<String>,
    pending_models: Vec<PendingModel>,
    model_parents: Vec<Vec<u16>>,
    /// Complete H3/H7 registration pool retained when `--ovl` limits the
    /// assets shown by the workbench. Model commands address global resource
    /// tables, so a model in one OVL can legitimately use a texture or
    /// material registered by another OVL.
    model_resources: Option<ModelResourceCatalog>,
    /// For each scene level, the leaves the retail tree holds after boot and
    /// that level's request, in load order (`004EE1E0` over `004EEEB0` and
    /// `004EED80`). The intro (32) also holds its group's bank 5.
    resident_orders: BTreeMap<u8, Vec<u8>>,
}

/// The leaves resident after booting `preload`'s tree and requesting
/// `level`, in load order; a later leaf's registrations replace an
/// earlier one's.
fn resident_order(preload: &Preload, level: u8) -> Option<Vec<u8>> {
    let mut tree = SceneTree::new(preload).ok()?;
    let mut operations = tree.boot();
    operations.extend(tree.request(level).ok()?);
    let mut resident: Vec<u8> = Vec::new();
    for operation in operations {
        match operation {
            inf_formats::LeafOperation::Load { level } => resident.push(level),
            inf_formats::LeafOperation::Unload { level } => resident.retain(|&held| held != level),
        }
    }
    Some(resident)
}

#[derive(Clone, Debug, Default)]
struct ModelResourceCatalog {
    textures: Vec<SpriteAsset>,
    materials: Vec<MaterialAsset>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSelection {
    pub model_index: usize,
    pub variant_index: usize,
    /// Physical registration whose source should provide the active level
    /// context for global Handler-3/Handler-7 resolution. Name/global-ID
    /// selections leave this unset and use the variant's first occurrence.
    pub occurrence_index: Option<usize>,
}

#[derive(Clone, Debug)]
struct PendingModel {
    grouping: ModelGroupingRecord,
    source_bytes: Arc<[u8]>,
    entry: ModelEntry,
    diagnostic: ModelDiagnosticProjection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelGroupingRecord {
    name: String,
    global_id: u16,
    source: String,
    local_id: u16,
    source_order: (u8, u8, u8, u8),
    content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelGroupPlan {
    name: String,
    global_id: u16,
    variants: Vec<ModelVariantPlan>,
    default_variant: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelVariantPlan {
    representative: usize,
    occurrences: Vec<usize>,
}

fn load_native_basis_math(data_root: &Path) -> Result<NativeBasisMath, String> {
    let path = data_root.join("Infest.exe");
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let executable =
        inf_formats::FrontendExecutable::parse(&bytes).map_err(|error| error.to_string())?;
    let table = inf_formats::parse_frontend_sine_table(
        executable.data(),
        inf_formats::FRONTEND_SINE_TABLE_ADDRESS,
    )
    .map_err(|error| error.to_string())?;
    Ok(NativeBasisMath::new(table.values))
}

impl AssetCatalog {
    /// Exact supported executable table; absent inputs never use generated trig.
    pub fn native_basis_math(&self) -> Option<&NativeBasisMath> {
        self.native_basis_math.as_ref()
    }

    pub fn native_basis_math_error(&self) -> Option<&str> {
        self.native_basis_math_error.as_deref()
    }

    pub fn load(data_root: &Path, ovl_filter: Option<&Path>) -> Result<Self, CatalogError> {
        let preload_path = data_root.join("PreLoad.dat");
        let preload_bytes = fs::read(&preload_path).map_err(|error| {
            CatalogError::new(format!("cannot read {}: {error}", preload_path.display()))
        })?;
        let preload = Preload::parse(&preload_bytes).map_err(|error| {
            CatalogError::new(format!("cannot parse {}: {error}", preload_path.display()))
        })?;
        let selected_overlay = ovl_filter
            .map(|filter| {
                select_overlay_paths(data_root, Some(filter)).and_then(|mut paths| {
                    paths.pop().ok_or_else(|| {
                        CatalogError::new(format!("no OVL matches {}", filter.display()))
                    })
                })
            })
            .transpose()?;
        let selected_source = selected_overlay
            .as_deref()
            .map(overlay_source_name)
            .transpose()?;
        let mut overlay_paths = select_overlay_paths(data_root, None)?;
        if let Some(path) = selected_overlay {
            let selected_name = overlay_source_name(&path)?;
            if !overlay_paths.iter().any(|candidate| {
                overlay_source_name(candidate)
                    .is_ok_and(|name| name.eq_ignore_ascii_case(&selected_name))
            }) {
                overlay_paths.push(path);
                overlay_paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
            }
        }
        let native_math = load_native_basis_math(data_root);
        let resident_orders = preload
            .leaves()
            .filter_map(|leaf| {
                resident_order(&preload, leaf.overlay_id).map(|order| (leaf.overlay_id, order))
            })
            .collect();
        let mut catalog = Self {
            native_basis_math_error: native_math.as_ref().err().cloned(),
            native_basis_math: native_math.ok(),
            resident_orders,
            ..Self::default()
        };

        for path in overlay_paths {
            catalog.load_overlay(&preload, &path)?;
        }

        if let Some(selected_source) = selected_source.as_deref() {
            catalog.model_resources = Some(ModelResourceCatalog {
                textures: catalog.sprites.clone(),
                materials: catalog.materials.clone(),
            });
            catalog
                .pending_models
                .retain(|model| model.grouping.source.eq_ignore_ascii_case(selected_source));
            catalog
                .sprites
                .retain(|asset| asset.source.eq_ignore_ascii_case(selected_source));
            catalog
                .materials
                .retain(|asset| asset.source.eq_ignore_ascii_case(selected_source));
            catalog
                .terrain
                .retain(|asset| asset.source.eq_ignore_ascii_case(selected_source));
        }

        catalog.finalize_models(selected_source.is_some())?;
        catalog.sprites.sort_by_key(|asset| {
            (
                asset.global_id.unwrap_or(u16::MAX),
                asset.source.to_ascii_lowercase(),
                asset.entry.local_index,
            )
        });
        catalog.materials.sort_by_key(|asset| {
            (
                asset.global_id.unwrap_or(u16::MAX),
                asset.source.to_ascii_lowercase(),
            )
        });
        catalog
            .terrain
            .sort_by_key(|asset| (asset.level, asset.source.clone(), asset.entry_index));
        Ok(catalog)
    }

    pub fn model_variant_count(&self) -> usize {
        self.models.iter().map(|asset| asset.variants.len()).sum()
    }

    pub fn model_occurrence_count(&self) -> usize {
        self.models
            .iter()
            .flat_map(|asset| &asset.variants)
            .map(|variant| variant.occurrences.len())
            .sum()
    }

    /// Resolve the model payload left in a global slot after one OVL's local
    /// Handler-8 registrations run in increasing local-index order.
    ///
    /// This is a conservative single-source diagnostic context, not the
    /// scene-active table produced by loading base/difficulty/language OVLs.
    pub fn active_model_variant_in_source(
        &self,
        global_id: u16,
        source: &str,
    ) -> Option<&ModelVariant> {
        self.models
            .iter()
            .find(|asset| asset.locator.global_id == global_id)?
            .variants
            .iter()
            .filter_map(|variant| {
                variant
                    .occurrences
                    .iter()
                    .filter(|occurrence| occurrence.source.eq_ignore_ascii_case(source))
                    .max_by_key(|occurrence| occurrence.local_id)
                    .map(|occurrence| (occurrence.local_id, variant))
            })
            .max_by_key(|(local_id, _)| *local_id)
            .map(|(_, variant)| variant)
    }

    pub fn model_parents(&self, child_id: u16) -> &[u16] {
        self.model_parents
            .get(usize::from(child_id))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Resolve the global Handler-3 table entry selected by the scene's
    /// difficulty/language context. Model commands already store the
    /// global ID; Handler-8 never rewrites it to a same-OVL local slot.
    pub fn model_texture(
        &self,
        global_id: u16,
        scene: &SceneContext,
        model_source: &str,
    ) -> Option<&SpriteAsset> {
        let model_level = OverlayKey::parse_filename(model_source)
            .ok()
            .map(|key| key.level);
        self.model_texture_resources()
            .iter()
            .filter(|asset| asset.global_id == Some(global_id))
            .filter_map(|asset| {
                self.model_resource_load_order(&asset.source, scene, model_level)
                    .map(|order| (order, asset.entry.local_index, asset))
            })
            .max_by_key(|(order, local_id, _)| (order.clone(), *local_id))
            .map(|(_, _, asset)| asset)
    }

    /// Resolve the active global Handler-7 material using the same
    /// base/difficulty/language/combined replacement order as Handler-3.
    /// Exact-level registrations replace shared startup resources.
    pub fn model_material(
        &self,
        global_id: u16,
        scene: &SceneContext,
        model_source: &str,
    ) -> Option<&MaterialAsset> {
        let model_level = OverlayKey::parse_filename(model_source)
            .ok()
            .map(|key| key.level);
        self.model_material_resources()
            .iter()
            .filter(|asset| asset.global_id == Some(global_id))
            .filter_map(|asset| {
                self.model_resource_load_order(&asset.source, scene, model_level)
                    .map(|order| (order, asset.local_id, asset))
            })
            .max_by_key(|(order, local_id, _)| (order.clone(), *local_id))
            .map(|(_, _, asset)| asset)
    }

    fn model_texture_resources(&self) -> &[SpriteAsset] {
        self.model_resources
            .as_ref()
            .map_or(&self.sprites, |resources| &resources.textures)
    }

    fn model_material_resources(&self) -> &[MaterialAsset] {
        self.model_resources
            .as_ref()
            .map_or(&self.materials, |resources| &resources.materials)
    }

    /// Search the logical global-slot catalog. Source queries select the
    /// matching content variant instead of silently returning another OVL's
    /// representative occurrence.
    pub fn find_model(&self, query: &str, after: Option<usize>) -> Option<ModelSelection> {
        let query = query.trim();
        if query.is_empty() || self.models.is_empty() {
            return None;
        }
        let numeric = parse_model_id(query);
        let needle = query.to_ascii_lowercase();
        let first = after.map_or(0, |index| step_index(index, self.models.len(), 1));
        (0..self.models.len()).find_map(|offset| {
            let model_index = (first + offset) % self.models.len();
            let asset = &self.models[model_index];
            if numeric.is_some_and(|id| asset.locator.global_id == id)
                || asset.locator.name.to_ascii_lowercase().contains(&needle)
            {
                return Some(ModelSelection {
                    model_index,
                    variant_index: asset.default_variant,
                    occurrence_index: None,
                });
            }
            asset
                .variants
                .iter()
                .enumerate()
                .find_map(|(variant_index, variant)| {
                    variant
                        .occurrences
                        .iter()
                        .position(|occurrence| {
                            occurrence.source.to_ascii_lowercase().contains(&needle)
                        })
                        .map(|occurrence_index| ModelSelection {
                            model_index,
                            variant_index,
                            occurrence_index: Some(occurrence_index),
                        })
                })
        })
    }

    fn load_overlay(&mut self, preload: &Preload, path: &Path) -> Result<(), CatalogError> {
        let source_name = overlay_source_name(path)?;
        let key = OverlayKey::parse_filename(&source_name).map_err(|error| {
            CatalogError::new(format!("invalid overlay {}: {error}", path.display()))
        })?;
        let leaf = preload.leaf(key.level).ok_or_else(|| {
            CatalogError::new(format!(
                "{} has level {}, absent from PreLoad.dat",
                path.display(),
                key.level
            ))
        })?;
        let requests = leaf.requests(key.request_kind());
        let source: Arc<[u8]> = fs::read(path)
            .map_err(|error| CatalogError::new(format!("cannot read {}: {error}", path.display())))?
            .into();
        let layout = parse_overlay(&source, requests).map_err(|error| {
            CatalogError::new(format!("cannot parse {}: {error}", path.display()))
        })?;

        let material_bank = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == MATERIAL_HANDLER)
            .map(|region| parse_material_bank(&source, region))
            .transpose()
            .map_err(|error| {
                CatalogError::new(format!(
                    "cannot parse materials in {}: {error}",
                    path.display()
                ))
            })?;

        if let Some(bank) = material_bank.as_ref() {
            self.add_materials(
                &source_name,
                requests
                    .get(MATERIAL_HANDLER as usize)
                    .expect("PreLoad request table has sixteen handlers"),
                bank,
            );
        }

        for region in &layout.regions {
            match region.handler.get() {
                MODEL_HANDLER => self.add_models(
                    &source_name,
                    Arc::clone(&source),
                    &key,
                    requests
                        .get(MODEL_HANDLER as usize)
                        .expect("PreLoad request table has sixteen handlers"),
                    region,
                )?,
                SPRITE_HANDLER => self.add_sprites(
                    &source_name,
                    Arc::clone(&source),
                    requests
                        .get(SPRITE_HANDLER as usize)
                        .expect("PreLoad request table has sixteen handlers"),
                    region,
                )?,
                TERRAIN_HANDLER => self.add_terrain(
                    &source_name,
                    key.level,
                    Arc::clone(&source),
                    material_bank.as_ref(),
                    region,
                )?,
                _ => {}
            }
        }
        Ok(())
    }

    fn add_models(
        &mut self,
        source_name: &str,
        source: Arc<[u8]>,
        key: &OverlayKey,
        request: &HandlerRequest,
        region: &inf_formats::HandlerRegion,
    ) -> Result<(), CatalogError> {
        let bank = parse_model_bank(&source, region, request, key).map_err(|error| {
            CatalogError::new(format!("cannot parse models in {source_name}: {error}"))
        })?;
        for entry in bank.entries {
            let name = entry
                .name
                .as_ref()
                .and_then(|name| name.ascii(&source))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("UNNAMED_{:04}", entry.identity.global_index));
            let diagnostic = analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest {
                entry: &entry,
                view: inf_formats::ModelDiagnosticView::Unconditioned,
            });
            let content = entry.content_span.get(&source).ok_or_else(|| {
                CatalogError::new(format!(
                    "model content span {:?} is outside {source_name}",
                    entry.content_span
                ))
            })?;
            self.pending_models.push(PendingModel {
                grouping: ModelGroupingRecord {
                    name,
                    global_id: entry.identity.global_index,
                    source: source_name.to_owned(),
                    local_id: entry.identity.local_index,
                    source_order: overlay_order(key),
                    content: content.to_vec(),
                },
                diagnostic,
                source_bytes: Arc::clone(&source),
                entry,
            });
        }
        Ok(())
    }

    fn finalize_models(&mut self, single_overlay: bool) -> Result<(), CatalogError> {
        let plans = plan_model_groups(
            &self
                .pending_models
                .iter()
                .map(|pending| pending.grouping.clone())
                .collect::<Vec<_>>(),
            single_overlay,
        )?;
        let pending = std::mem::take(&mut self.pending_models);
        self.models = plans
            .into_iter()
            .map(|plan| {
                let variants = plan
                    .variants
                    .into_iter()
                    .map(|variant| {
                        let representative = &pending[variant.representative];
                        ModelVariant {
                            source_bytes: Arc::clone(&representative.source_bytes),
                            entry: representative.entry.clone(),
                            diagnostic: representative.diagnostic.clone(),
                            occurrences: variant
                                .occurrences
                                .into_iter()
                                .map(|index| ModelOccurrence {
                                    source: pending[index].grouping.source.clone(),
                                    local_id: pending[index].grouping.local_id,
                                })
                                .collect(),
                        }
                    })
                    .collect::<Vec<_>>();
                ModelAsset {
                    locator: ModelLocator {
                        name: plan.name,
                        global_id: plan.global_id,
                    },
                    variants,
                    default_variant: plan.default_variant,
                }
            })
            .collect();
        self.model_parents = build_model_parent_index(&self.models);
        Ok(())
    }

    fn add_sprites(
        &mut self,
        source_name: &str,
        source: Arc<[u8]>,
        request: &HandlerRequest,
        region: &inf_formats::HandlerRegion,
    ) -> Result<(), CatalogError> {
        let bank = parse_sprite_bank(&source, region).map_err(|error| {
            CatalogError::new(format!("cannot parse sprites in {source_name}: {error}"))
        })?;
        for entry in bank.entries {
            let global_id = u16::try_from(entry.local_index)
                .ok()
                .and_then(|local| request.global_index(local));
            self.sprites.push(SpriteAsset {
                source: source_name.to_owned(),
                global_id,
                source_bytes: Arc::clone(&source),
                entry,
            });
        }
        Ok(())
    }

    fn add_materials(&mut self, source_name: &str, request: &HandlerRequest, bank: &MaterialBank) {
        self.materials
            .extend(
                bank.entries
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(local_index, value)| MaterialAsset {
                        source: source_name.to_owned(),
                        local_id: u16::try_from(local_index)
                            .expect("Handler-7 local index fits the u16 request mapping"),
                        global_id: u16::try_from(local_index)
                            .ok()
                            .and_then(|local| request.global_index(local)),
                        value,
                    }),
            );
    }

    fn add_terrain(
        &mut self,
        source_name: &str,
        level: u8,
        source: Arc<[u8]>,
        material_bank: Option<&MaterialBank>,
        region: &inf_formats::HandlerRegion,
    ) -> Result<(), CatalogError> {
        let bank = parse_terrain_bank(&source, region).map_err(|error| {
            CatalogError::new(format!("cannot parse terrain in {source_name}: {error}"))
        })?;
        // This association is made only while loading H12 from the same OVL.
        let palette = material_bank.and_then(MaterialBank::palette8);
        for (entry_index, entry) in bank.entries.into_iter().enumerate() {
            self.terrain.push(TerrainAsset {
                level,
                source: source_name.to_owned(),
                entry_index,
                source_bytes: Arc::clone(&source),
                entry,
                palette,
            });
        }
        Ok(())
    }
}

/// One Handler-8 global slot as registered by a single scene load.
///
/// A later load phase overwrites an earlier one slot by slot, exactly as
/// `FUN_004EFC00` replaces the mapped global pointer, so each global ID
/// appears at most once.
#[derive(Clone, Debug)]
pub struct SceneModel {
    pub global_id: u16,
    pub name: String,
    pub source: String,
    pub local_id: u16,
    pub source_bytes: Arc<[u8]>,
    pub entry: ModelEntry,
    pub diagnostic: ModelDiagnosticProjection,
}

/// The Handler-8/3/7 registrations of one explicit scene load.
///
/// Unlike [`AssetCatalog`], which unions every installed OVL into a diagnostic
/// namespace, this loads only the scene's own four OVL files in `FUN_004EE680`
/// dispatch order and keeps the winning registration per global slot. Files
/// absent from disk are recorded in `missing_files` and skipped; a present
/// file must still parse exactly against its scene request table.
/// Authored Group/Select residency is documented in RUNTIME_FOUNDATION.md;
/// this diagnostic single-leaf catalog does not execute that lifetime policy.
#[derive(Clone, Debug)]
pub struct SceneCatalog {
    pub context: SceneContext,
    pub models: Vec<SceneModel>,
    pub textures: Vec<SpriteAsset>,
    pub materials: Vec<MaterialAsset>,
    pub loaded_phases: Vec<RequestKind>,
    pub missing_files: Vec<String>,
}

impl SceneCatalog {
    pub fn load(data_root: &Path, context: SceneContext) -> Result<Self, CatalogError> {
        const KINDS: [RequestKind; 4] = [
            RequestKind::Base,
            RequestKind::Difficulty,
            RequestKind::Language,
            RequestKind::DifficultyLanguage,
        ];
        let preload_path = data_root.join("PreLoad.dat");
        let preload_bytes = fs::read(&preload_path).map_err(|error| {
            CatalogError::new(format!("cannot read {}: {error}", preload_path.display()))
        })?;
        let preload = Preload::parse(&preload_bytes).map_err(|error| {
            CatalogError::new(format!("cannot parse {}: {error}", preload_path.display()))
        })?;
        let tables = context.request_tables(&preload).ok_or_else(|| {
            CatalogError::new(format!(
                "level {} is absent from PreLoad.dat",
                context.level
            ))
        })?;
        let filenames = context.overlay_filenames();
        let mut catalog = Self {
            context,
            models: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            loaded_phases: Vec::new(),
            missing_files: Vec::new(),
        };
        let mut model_winners: BTreeMap<u16, usize> = BTreeMap::new();
        let mut texture_winners: BTreeMap<u16, usize> = BTreeMap::new();
        let mut material_winners: BTreeMap<u16, usize> = BTreeMap::new();
        for index in 0..4 {
            let kind = KINDS[index];
            let filename = &filenames[index];
            let table = tables[index];
            let path = data_root.join("Overlay").join(filename);
            let Ok(bytes) = fs::read(&path) else {
                catalog.missing_files.push(filename.clone());
                continue;
            };
            let source: Arc<[u8]> = bytes.into();
            let layout = parse_overlay(&source, table).map_err(|error| {
                CatalogError::new(format!("cannot parse {}: {error}", path.display()))
            })?;
            let key = OverlayKey::parse_filename(filename).map_err(|error| {
                CatalogError::new(format!("invalid overlay {}: {error}", path.display()))
            })?;
            catalog.add_scene_models(
                filename,
                Arc::clone(&source),
                &key,
                table,
                &layout,
                &mut model_winners,
            )?;
            catalog.add_scene_materials(filename, &source, table, &layout, &mut material_winners);
            catalog.add_scene_textures(
                filename,
                Arc::clone(&source),
                table,
                &layout,
                &mut texture_winners,
            )?;
            catalog.loaded_phases.push(kind);
        }
        catalog.models.sort_by_key(|model| model.global_id);
        catalog.textures.sort_by_key(|asset| asset.global_id);
        catalog.materials.sort_by_key(|asset| asset.global_id);
        Ok(catalog)
    }

    pub fn scene_model(&self, global_id: u16) -> Option<&SceneModel> {
        self.models
            .iter()
            .find(|model| model.global_id == global_id)
    }

    pub fn scene_texture(&self, global_id: u16) -> Option<&SpriteAsset> {
        self.textures
            .iter()
            .find(|asset| asset.global_id == Some(global_id))
    }

    pub fn scene_material(&self, global_id: u16) -> Option<&MaterialAsset> {
        self.materials
            .iter()
            .find(|asset| asset.global_id == Some(global_id))
    }

    fn add_scene_models(
        &mut self,
        source_name: &str,
        source: Arc<[u8]>,
        key: &OverlayKey,
        table: &RequestTable,
        layout: &inf_formats::OverlayLayout,
        winners: &mut BTreeMap<u16, usize>,
    ) -> Result<(), CatalogError> {
        let Some(region) = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == MODEL_HANDLER)
        else {
            return Ok(());
        };
        let request = table
            .get(MODEL_HANDLER as usize)
            .expect("PreLoad request table has sixteen handlers");
        let bank = parse_model_bank(&source, region, request, key).map_err(|error| {
            CatalogError::new(format!("cannot parse models in {source_name}: {error}"))
        })?;
        for entry in bank.entries {
            let name = entry
                .name
                .as_ref()
                .and_then(|name| name.ascii(&source))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("UNNAMED_{:04}", entry.identity.global_index));
            let diagnostic = analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest {
                entry: &entry,
                view: inf_formats::ModelDiagnosticView::Unconditioned,
            });
            let model = SceneModel {
                global_id: entry.identity.global_index,
                name,
                source: source_name.to_owned(),
                local_id: entry.identity.local_index,
                source_bytes: Arc::clone(&source),
                entry,
                diagnostic,
            };
            // Registration order is the retail order: increasing local index
            // within a phase, base/difficulty/language/combined across phases.
            // A repeated global slot replaces the earlier registration.
            match winners.get(&model.global_id) {
                Some(&index) => self.models[index] = model,
                None => {
                    winners.insert(model.global_id, self.models.len());
                    self.models.push(model);
                }
            }
        }
        Ok(())
    }

    fn add_scene_materials(
        &mut self,
        source_name: &str,
        source: &Arc<[u8]>,
        table: &RequestTable,
        layout: &inf_formats::OverlayLayout,
        winners: &mut BTreeMap<u16, usize>,
    ) {
        let Some(region) = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == MATERIAL_HANDLER)
        else {
            return;
        };
        let Ok(bank) = parse_material_bank(source, region) else {
            return;
        };
        let request = table
            .get(MATERIAL_HANDLER as usize)
            .expect("PreLoad request table has sixteen handlers");
        for (local_index, value) in bank.entries.iter().copied().enumerate() {
            let Ok(local) = u16::try_from(local_index) else {
                continue;
            };
            let Some(global_id) = request.global_index(local) else {
                continue;
            };
            let asset = MaterialAsset {
                source: source_name.to_owned(),
                global_id: Some(global_id),
                local_id: local,
                value,
            };
            match winners.get(&global_id) {
                Some(&index) => self.materials[index] = asset,
                None => {
                    winners.insert(global_id, self.materials.len());
                    self.materials.push(asset);
                }
            }
        }
    }

    fn add_scene_textures(
        &mut self,
        source_name: &str,
        source: Arc<[u8]>,
        table: &RequestTable,
        layout: &inf_formats::OverlayLayout,
        winners: &mut BTreeMap<u16, usize>,
    ) -> Result<(), CatalogError> {
        let Some(region) = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == SPRITE_HANDLER)
        else {
            return Ok(());
        };
        let bank = parse_sprite_bank(&source, region).map_err(|error| {
            CatalogError::new(format!("cannot parse sprites in {source_name}: {error}"))
        })?;
        let request = table
            .get(SPRITE_HANDLER as usize)
            .expect("PreLoad request table has sixteen handlers");
        for entry in bank.entries {
            let Some(global_id) = u16::try_from(entry.local_index)
                .ok()
                .and_then(|local| request.global_index(local))
            else {
                continue;
            };
            let asset = SpriteAsset {
                source: source_name.to_owned(),
                global_id: Some(global_id),
                source_bytes: Arc::clone(&source),
                entry,
            };
            match winners.get(&global_id) {
                Some(&index) => self.textures[index] = asset,
                None => {
                    winners.insert(global_id, self.textures.len());
                    self.textures.push(asset);
                }
            }
        }
        Ok(())
    }
}

fn overlay_order(key: &OverlayKey) -> (u8, u8, u8, u8) {
    match key.variant {
        OverlayVariant::Base => (key.level, 0, 0, 0),
        OverlayVariant::Difficulty { value } => (key.level, 1, value, 0),
        OverlayVariant::Language { value } => (key.level, 2, value, 0),
        OverlayVariant::DifficultyLanguage {
            difficulty,
            language,
        } => (key.level, 3, difficulty, language),
    }
}

/// Language used by the workbench until it gains a language control. This keeps
/// the currently fixed choice visible at every call site instead of hiding a
/// zero inside the load-order helper.
pub const VIEWER_DEFAULT_LANGUAGE: u8 = 0;

impl AssetCatalog {
    /// Order of the active OVL registrations that can supply a model's
    /// global H3/H7 resource. The leaves resident with the model's level
    /// load in the retail tree's order, each leaf's base, difficulty,
    /// language and combined overlays in that order (`004EE680`); a later
    /// registration replaces an earlier one. Difficulty and language come
    /// from the explicit scene context; the workbench builds that context
    /// from its difficulty control and [`VIEWER_DEFAULT_LANGUAGE`]. A source
    /// without a level keeps the startup leaves 0 and 1.
    fn model_resource_load_order(
        &self,
        source: &str,
        scene: &SceneContext,
        model_level: Option<u8>,
    ) -> Option<(u8, u8, u8, String)> {
        let key = OverlayKey::parse_filename(source).ok()?;
        let level_scope = match model_level.and_then(|level| self.resident_orders.get(&level)) {
            Some(order) => order.iter().position(|&level| level == key.level)? as u8,
            None if key.level <= 1 => key.level,
            None => return None,
        };
        model_resource_phase(key, scene, level_scope, source)
    }
}

fn model_resource_phase(
    key: OverlayKey,
    scene: &SceneContext,
    level_scope: u8,
    source: &str,
) -> Option<(u8, u8, u8, String)> {
    let load_phase = match key.variant {
        OverlayVariant::Base => 0,
        OverlayVariant::Difficulty { value } if value == scene.difficulty => 1,
        OverlayVariant::Language { value } if value == scene.language => 2,
        OverlayVariant::DifficultyLanguage {
            difficulty: overlay_difficulty,
            language,
        } if overlay_difficulty == scene.difficulty && language == scene.language => 3,
        _ => return None,
    };

    Some((
        level_scope,
        key.level,
        load_phase,
        source.to_ascii_lowercase(),
    ))
}

fn overlay_source_name(path: &Path) -> Result<String, CatalogError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| CatalogError::new(format!("non-UTF-8 overlay path {}", path.display())))
}

fn build_model_parent_index(models: &[ModelAsset]) -> Vec<Vec<u16>> {
    let slot_count = models
        .iter()
        .map(|asset| usize::from(asset.locator.global_id) + 1)
        .max()
        .unwrap_or(0);
    let mut parents = vec![Vec::new(); slot_count];
    for asset in models {
        for child_id in asset
            .variants
            .iter()
            .flat_map(ModelVariant::child_model_ids)
        {
            let Some(child_parents) = parents.get_mut(usize::from(child_id)) else {
                continue;
            };
            if !child_parents.contains(&asset.locator.global_id) {
                child_parents.push(asset.locator.global_id);
            }
        }
    }
    parents
}

fn plan_model_groups(
    records: &[ModelGroupingRecord],
    single_overlay: bool,
) -> Result<Vec<ModelGroupPlan>, CatalogError> {
    let mut order = (0..records.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        let record = &records[index];
        (
            record.global_id,
            record.source_order,
            record.source.to_ascii_lowercase(),
            record.local_id,
        )
    });

    let mut groups = Vec::<ModelGroupPlan>::new();
    for index in order {
        let record = &records[index];
        if groups
            .last()
            .is_none_or(|group| group.global_id != record.global_id)
        {
            groups.push(ModelGroupPlan {
                name: record.name.clone(),
                global_id: record.global_id,
                variants: Vec::new(),
                default_variant: 0,
            });
        }
        let group = groups.last_mut().expect("group was just inserted");
        if group.name != record.name {
            return Err(CatalogError::new(format!(
                "global model #{} has conflicting names {:?} and {:?}",
                record.global_id, group.name, record.name
            )));
        }
        let variant_index = group
            .variants
            .iter()
            .position(|variant| records[variant.representative].content == record.content)
            .unwrap_or_else(|| {
                group.variants.push(ModelVariantPlan {
                    representative: index,
                    occurrences: Vec::new(),
                });
                group.variants.len() - 1
            });
        group.variants[variant_index].occurrences.push(index);
        if single_overlay {
            // Handler8_Load registers local entries in increasing order, and
            // ResourceReader overwrites the mapped global pointer slot.
            group.default_variant = variant_index;
        }
    }
    Ok(groups)
}

pub fn default_data_root() -> PathBuf {
    let current = PathBuf::from(".");
    if current.join("PreLoad.dat").is_file() {
        return current;
    }
    let parent = PathBuf::from("retail");
    if parent.join("PreLoad.dat").is_file() {
        return parent;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn select_overlay_paths(
    data_root: &Path,
    filter: Option<&Path>,
) -> Result<Vec<PathBuf>, CatalogError> {
    if let Some(filter) = filter {
        for candidate in [
            filter.to_path_buf(),
            data_root.join(filter),
            data_root.join("Overlay").join(filter),
        ] {
            if candidate.is_file() {
                return Ok(vec![candidate]);
            }
        }
    }

    let overlay_dir = data_root.join("Overlay");
    let mut paths = fs::read_dir(&overlay_dir)
        .map_err(|error| {
            CatalogError::new(format!("cannot read {}: {error}", overlay_dir.display()))
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ovl"))
        })
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());

    if let Some(filter) = filter {
        let needle = filter.to_string_lossy().to_ascii_lowercase();
        paths.retain(|path| {
            let file = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            file == needle || stem == needle
        });
        if paths.is_empty() {
            return Err(CatalogError::new(format!(
                "no OVL matches {} in {}",
                filter.display(),
                overlay_dir.display()
            )));
        }
    }
    Ok(paths)
}

pub fn step_index(current: usize, length: usize, delta: isize) -> usize {
    if length == 0 {
        return 0;
    }
    (current as isize + delta).rem_euclid(length as isize) as usize
}

fn parse_model_id(query: &str) -> Option<u16> {
    let query = query.strip_prefix('#').unwrap_or(query);
    if let Some(hex) = query
        .strip_prefix("0x")
        .or_else(|| query.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).ok()
    } else if query.bytes().all(|byte| byte.is_ascii_digit()) {
        query.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grouping_record(
        name: &str,
        global_id: u16,
        source: &str,
        local_id: u16,
        content: &[u8],
    ) -> ModelGroupingRecord {
        let key = OverlayKey::parse_filename(source).unwrap();
        ModelGroupingRecord {
            name: name.to_owned(),
            global_id,
            source: source.to_owned(),
            local_id,
            source_order: overlay_order(&key),
            content: content.to_vec(),
        }
    }

    #[test]
    fn stepping_wraps_in_both_directions() {
        assert_eq!(step_index(0, 3, -1), 2);
        assert_eq!(step_index(2, 3, 1), 0);
        assert_eq!(step_index(0, 0, 1), 0);
    }

    #[test]
    fn grouping_collapses_equal_content_but_preserves_real_variants() {
        let records = [
            grouping_record("ALPHA", 9, "12.ovl", 7, b"same"),
            grouping_record("BETA", 4, "20.ovl", 2, b"other"),
            grouping_record("ALPHA", 9, "11.ovl", 3, b"same"),
            grouping_record("ALPHA", 9, "12.ovl", 8, b"different"),
        ];
        let plans = plan_model_groups(&records, false).unwrap();

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].global_id, 4);
        assert_eq!(plans[1].global_id, 9);
        assert_eq!(plans[1].variants.len(), 2);
        assert_eq!(plans[1].variants[0].occurrences, vec![2, 0]);
        assert_eq!(plans[1].variants[1].occurrences, vec![3]);
        assert_eq!(plans[1].default_variant, 0);
    }

    #[test]
    fn single_overlay_default_is_the_final_local_registration() {
        let records = [
            grouping_record("ALPHA", 9, "35.ovl", 2, b"first"),
            grouping_record("ALPHA", 9, "35.ovl", 8, b"second"),
            grouping_record("ALPHA", 9, "35.ovl", 11, b"first"),
        ];
        let plans = plan_model_groups(&records, true).unwrap();

        assert_eq!(plans[0].variants.len(), 2);
        assert_eq!(plans[0].default_variant, 0);
        assert_eq!(plans[0].variants[0].occurrences, vec![0, 2]);
    }

    #[test]
    fn grouping_rejects_conflicting_names_for_one_global_slot() {
        let records = [
            grouping_record("ALPHA", 9, "11.ovl", 0, b"same"),
            grouping_record("BETA", 9, "12.ovl", 0, b"same"),
        ];
        let error = plan_model_groups(&records, false).unwrap_err();
        assert!(error.to_string().contains("conflicting names"));
    }

    #[test]
    fn retail_catalog_groups_physical_registrations_without_losing_variants() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, None).unwrap();

        assert_eq!(catalog.models.len(), 760);
        assert_eq!(catalog.model_variant_count(), 849);
        assert_eq!(catalog.model_occurrence_count(), 5_142);
        assert_eq!(catalog.materials.len(), 378);
        assert_eq!(
            catalog
                .models
                .iter()
                .filter(|asset| asset.variants.len() > 1)
                .count(),
            16
        );
        assert_eq!(
            catalog
                .models
                .iter()
                .filter(|asset| asset.variants.len() == 1)
                .count(),
            744
        );
        assert!(catalog
            .models
            .iter()
            .enumerate()
            .all(|(index, asset)| usize::from(asset.locator.global_id) == index));

        let repeated = &catalog.models[303];
        assert_eq!(repeated.locator.name, "cweapons2");
        assert_eq!(repeated.variants.len(), 1);
        assert_eq!(repeated.variants[0].occurrences.len(), 39);

        let backdrop = &catalog.models[278];
        assert_eq!(backdrop.locator.name, "backdrop");
        assert_eq!(backdrop.variants.len(), 26);

        let mark_material = catalog
            .model_material(10, &SceneContext::new(0, 0, 0), "0.ovl")
            .unwrap();
        assert_eq!(mark_material.source, "0.ovl");
        assert_eq!(mark_material.value.raw(), 0x0000_4200);

        let bigdish_texture = catalog
            .model_texture(221, &SceneContext::new(17, 4, 0), "17.ovl")
            .unwrap();
        assert_eq!(bigdish_texture.source, "1d4.ovl");
        let bigdish_variant = bigdish_texture
            .entry
            .variant(inf_formats::SpriteRenderMode::Mode0)
            .unwrap();
        assert_eq!((bigdish_variant.width, bigdish_variant.height), (64, 64));

        let active_fern = catalog
            .active_model_variant_in_source(343, "35.ovl")
            .expect("35.ovl registers fern");
        assert!(active_fern.occurrences.iter().any(|occurrence| occurrence
            .source
            .eq_ignore_ascii_case("35.ovl")
            && occurrence.local_id == 322));
        assert_eq!(catalog.model_parents(43), [42]);
        assert_eq!(catalog.model_parents(44), [43]);
        assert_eq!(catalog.model_parents(61), [60, 579]);

        let source_match = catalog.find_model("14.ovl", None).unwrap();
        let source_occurrence = &catalog.models[source_match.model_index].variants
            [source_match.variant_index]
            .occurrences[source_match.occurrence_index.unwrap()];
        assert!(source_occurrence.source.eq_ignore_ascii_case("14.ovl"));
        assert_eq!(
            catalog.find_model("#42", None).unwrap().occurrence_index,
            None
        );
        assert_eq!(catalog.find_model("#42", None).unwrap().model_index, 42);
        assert_eq!(catalog.find_model("0x2a", None).unwrap().model_index, 42);
        assert_eq!(
            catalog
                .find_model("cweapons2", None)
                .map(|selection| selection.model_index),
            Some(303)
        );
    }

    #[test]
    fn filtered_retail_catalog_defaults_to_the_runtime_final_variant() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, Some(Path::new("35.ovl"))).unwrap();
        let fern = catalog
            .models
            .iter()
            .find(|asset| asset.locator.global_id == 343)
            .unwrap();

        assert_eq!(fern.variants.len(), 2);
        assert_eq!(fern.default_variant, 1);
        assert_eq!(
            fern.default_variant()
                .unwrap()
                .occurrences
                .last()
                .unwrap()
                .local_id,
            322
        );
    }

    #[test]
    fn filtered_retail_catalog_retains_cross_overlay_model_resources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, Some(Path::new("17.ovl"))).unwrap();

        assert!(catalog
            .models
            .iter()
            .any(|asset| asset.locator.name.eq_ignore_ascii_case("BIGDISH")));
        assert!(catalog
            .models
            .iter()
            .flat_map(|asset| &asset.variants)
            .flat_map(|variant| &variant.occurrences)
            .all(|occurrence| occurrence.source.eq_ignore_ascii_case("17.ovl")));

        // H3 #221 is not registered by 17.ovl. The original global resource
        // table obtains its active difficulty-4 entry from 1d4.ovl.
        let texture = catalog
            .model_texture(221, &SceneContext::new(17, 4, 0), "17.ovl")
            .unwrap();
        assert_eq!(texture.source, "1d4.ovl");
        assert!(catalog
            .sprites
            .iter()
            .all(|asset| asset.source.eq_ignore_ascii_case("17.ovl")));
    }

    fn retail_preload() -> Preload {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap()
    }

    #[test]
    fn model_resource_order_uses_only_resident_overlays_and_later_load_phases() {
        let preload = retail_preload();
        let catalog = AssetCatalog {
            resident_orders: [17, 32]
                .into_iter()
                .map(|level| (level, resident_order(&preload, level).unwrap()))
                .collect(),
            ..AssetCatalog::default()
        };
        // Level 17 holds the startup leaves, its group's bank 7 and itself.
        assert_eq!(catalog.resident_orders[&17], [0, 1, 7, 17]);
        let order = |source: &str| {
            catalog.model_resource_load_order(source, &SceneContext::new(17, 4, 0), Some(17))
        };
        assert!(order("17d4l0.ovl") > order("17l0.ovl"));
        assert!(order("17l0.ovl") > order("17d4.ovl"));
        assert!(order("17d4.ovl") > order("17.ovl"));
        assert!(order("17.ovl") > order("7d4.ovl"));
        assert!(order("7d4.ovl") > order("1d4.ovl"));
        assert!(order("1d4.ovl") > order("0d4.ovl"));
        assert!(order("17d3.ovl").is_none());
        assert!(order("16d4.ovl").is_none());
        // The intro (32) holds bank 5 with it, which supplies its signs.
        assert_eq!(catalog.resident_orders[&32], [0, 1, 5, 32]);
        let intro = |source: &str| {
            catalog.model_resource_load_order(source, &SceneContext::new(32, 0, 0), Some(32))
        };
        assert!(intro("32.ovl") > intro("5d0.ovl"));
        assert!(intro("5d0.ovl").is_some());
        assert!(intro("5d2.ovl").is_none());
        assert!(intro("6d0.ovl").is_none());
        // A source without a level keeps the startup leaves.
        assert!(catalog
            .model_resource_load_order("1.ovl", &SceneContext::new(17, 4, 0), None)
            .is_some());
        assert!(catalog
            .model_resource_load_order("7.ovl", &SceneContext::new(17, 4, 0), None)
            .is_none());
    }

    /// The intro's three zone signs draw sprites 82..84 of leaf 5's tier
    /// file, global slots 1370..1372, which the tree holds with leaf 32.
    #[test]
    fn intro_sign_textures_come_from_the_resident_tier_bank() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, None).unwrap();
        for tier in 0..5 {
            let scene = SceneContext::new(32, tier, 0);
            for (global, local) in [(1370, 82), (1371, 83), (1372, 84)] {
                let texture = catalog.model_texture(global, &scene, "32.ovl").unwrap();
                assert_eq!(
                    texture.source,
                    format!("5d{tier}.ovl"),
                    "tier {tier} slot {global}"
                );
                assert_eq!(
                    texture.entry.local_index, local,
                    "tier {tier} slot {global}"
                );
            }
        }
    }

    #[test]
    fn scene_level_1_resolves_only_its_own_base_model_table() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let scene = SceneCatalog::load(&root, SceneContext::new(1, 0, 0)).unwrap();

        // Level 1 base frames 278 Handler-8 entries but its signed segments
        // map locals 254/255 backwards onto globals 195/196, which locals
        // 193/194 already claimed. The later registrations win exactly as
        // `FUN_004EFC00` overwrites the mapped slot, leaving 276 winners.
        // No later phase requests Handler 8, so every winner comes from 1.ovl.
        assert_eq!(scene.models.len(), 276);
        assert!(scene.models.iter().all(|model| model.source == "1.ovl"));
        assert!(scene
            .models
            .windows(2)
            .all(|pair| pair[0].global_id < pair[1].global_id));
        assert_eq!(
            scene.loaded_phases,
            [
                RequestKind::Base,
                RequestKind::Difficulty,
                RequestKind::Language,
                RequestKind::DifficultyLanguage,
            ]
        );
        assert!(scene.missing_files.is_empty());

        // The scene's Handler-3 pool arrives via the difficulty phase alone.
        assert!(!scene.textures.is_empty());
        assert!(scene.textures.iter().all(|asset| asset.source == "1d0.ovl"));
        assert!(!scene.materials.is_empty());
        assert!(scene.materials.iter().all(|asset| asset.source == "1.ovl"));

        let first = &scene.models[0];
        assert_eq!(
            scene
                .scene_model(first.global_id)
                .expect("scene-global lookup")
                .source,
            "1.ovl"
        );
        assert_eq!(
            scene.scene_model(195).expect("overwritten slot").local_id,
            254
        );
        assert_eq!(
            scene.scene_model(196).expect("overwritten slot").local_id,
            255
        );
    }

    #[test]
    fn scene_level_17_uses_its_explicit_difficulty_context() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let scene = SceneCatalog::load(&root, SceneContext::new(17, 4, 0)).unwrap();

        assert_eq!(scene.models.len(), 147);
        assert!(scene.models.iter().all(|model| model.source == "17.ovl"));
        // Level 17's difficulty and combined files are empty and request
        // nothing, so its small base Handler-3 pool stands unmodified.
        assert!(!scene.textures.is_empty());
        assert!(scene.textures.iter().all(|asset| asset.source == "17.ovl"));
        assert!(scene.missing_files.is_empty());
    }

    #[test]
    fn scene_unknown_level_is_an_explicit_error() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let error = SceneCatalog::load(&root, SceneContext::new(44, 2, 1)).unwrap_err();
        assert!(error.to_string().contains("absent from PreLoad.dat"));
    }
}
