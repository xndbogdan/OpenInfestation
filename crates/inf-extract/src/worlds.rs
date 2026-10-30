//! Bounded constructor/world asset export. Unproven class extensions stay raw;
//! a placement reference is not instantiated or resolved across unrelated OVLs.

use std::{error::Error, fs, path::Path};

use inf_formats::{
    parse_constructor_asset_bank, parse_world_instance_bank, ByteSpan, ConstructorAssetBank,
    ConstructorPayloadView, HandlerRequest, InstanceAssetBank, OverlayKey, Preload, RequestKind,
};
use serde::Serialize;

use crate::{inventory, managed_output::StagedOutput};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WorldExportReport {
    pub constructor_banks: usize,
    pub constructor_resources: usize,
    pub initial_model_bindings: usize,
    pub shared_actor_prefixes: usize,
    pub damage_responses: usize,
    pub derived_actor_prefixes: usize,
    pub constructor_sound_bindings: usize,
    pub flags_only_prefixes: usize,
    pub terrain_light_prefixes: usize,
    pub selector86_prefixes: usize,
    pub selector83_prefixes: usize,
    pub selector45_prefixes: usize,
    pub selector46_prefixes: usize,
    pub flags_words_prefixes: usize,
    pub goal_prefixes: usize,
    pub point_table_prefixes: usize,
    pub opaque_class_payloads: usize,
    pub world_banks: usize,
    pub world_resources: usize,
    pub placements: usize,
    pub instance_commands: usize,
}

#[derive(Serialize)]
struct WorldManifest {
    schema_version: u32,
    kind: &'static str,
    policy: &'static str,
    totals: WorldExportReport,
    sources: Vec<WorldSource>,
}

#[derive(Serialize)]
struct WorldSource {
    ovl: String,
    overlay: OverlayKey,
    handler: u8,
    request_kind: RequestKind,
    request: HandlerRequest,
    framed_region: ByteSpan,
    raw_framed_region: String,
    decoded: String,
    /// Ordered local registration, not a corpus-wide last-writer winner.
    global_slots_in_local_order: Vec<u16>,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum WorldAssetBank {
    ConstructorAssets(ConstructorAssetBank),
    WorldInstances(InstanceAssetBank),
}

pub fn export(data_root: &Path, output_root: &Path) -> Result<WorldExportReport, Box<dyn Error>> {
    let inventory = inventory::build(data_root)?;
    let preload = Preload::parse(&fs::read(data_root.join("PreLoad.dat"))?)?;
    let staged = StagedOutput::new(&output_root.join("world_assets"))?;
    let mut totals = WorldExportReport::default();
    let mut sources = Vec::new();
    for file in inventory.files {
        let regions: Vec<_> = file
            .regions
            .iter()
            .filter(|r| matches!(r.handler.get(), 13 | 14))
            .collect();
        if regions.is_empty() {
            continue;
        }
        let bytes = fs::read(data_root.join("Overlay").join(&file.name))?;
        let requests = preload
            .leaf(file.key.level)
            .expect("inventory validated leaf")
            .requests(file.key.request_kind());
        for region in regions {
            let handler = region.handler.get();
            let decoded = match handler {
                13 => {
                    let bank = parse_constructor_asset_bank(&bytes, region)
                        .map_err(|e| format!("{} handler 13: {e}", file.name))?;
                    totals.constructor_banks += 1;
                    totals.constructor_resources += bank.assets.len();
                    for asset in &bank.assets {
                        totals.initial_model_bindings += usize::from(asset.model_binding.is_some());
                        match asset.payload {
                            ConstructorPayloadView::SharedActor(ref actor) => {
                                totals.shared_actor_prefixes += 1;
                                totals.damage_responses += actor.damage_responses.len();
                                if let Some(derived) = &actor.derived {
                                    totals.derived_actor_prefixes += 1;
                                    totals.constructor_sound_bindings +=
                                        derived.sound_bindings.len();
                                }
                            }
                            ConstructorPayloadView::Flags(_) => totals.flags_only_prefixes += 1,
                            ConstructorPayloadView::TerrainLight(_) => {
                                totals.terrain_light_prefixes += 1
                            }
                            ConstructorPayloadView::Selector86(_) => {
                                totals.selector86_prefixes += 1
                            }
                            ConstructorPayloadView::Selector83(_) => {
                                totals.selector83_prefixes += 1
                            }
                            ConstructorPayloadView::Selector45(_) => {
                                totals.selector45_prefixes += 1
                            }
                            ConstructorPayloadView::Selector46(_) => {
                                totals.selector46_prefixes += 1
                            }
                            ConstructorPayloadView::FlagsWords(_) => {
                                totals.flags_words_prefixes += 1
                            }
                            ConstructorPayloadView::Goal(_) => totals.goal_prefixes += 1,
                            ConstructorPayloadView::PointTable16(_) => {
                                totals.point_table_prefixes += 1
                            }
                            ConstructorPayloadView::Opaque => totals.opaque_class_payloads += 1,
                        }
                    }
                    WorldAssetBank::ConstructorAssets(bank)
                }
                14 => {
                    let bank = parse_world_instance_bank(&bytes, region)
                        .map_err(|e| format!("{} handler 14: {e}", file.name))?;
                    totals.world_banks += 1;
                    totals.world_resources += bank.worlds.entries.len();
                    totals.placements += bank
                        .worlds
                        .entries
                        .iter()
                        .map(|w| w.placements.len())
                        .sum::<usize>();
                    totals.instance_commands += bank
                        .instances
                        .iter()
                        .flat_map(|world| &world.placements)
                        .map(|placement| placement.commands.len())
                        .sum::<usize>();
                    WorldAssetBank::WorldInstances(bank)
                }
                _ => unreachable!(),
            };
            let mut global_slots = Vec::with_capacity(region.requested_count as usize);
            for local in 0..region.requested_count {
                let local = u16::try_from(local)?;
                global_slots.push(
                    requests.0[usize::from(handler)]
                        .global_index(local)
                        .ok_or_else(|| {
                            format!(
                                "{} handler {handler} local slot {local} has no global mapping",
                                file.name
                            )
                        })?,
                );
            }
            let stem = Path::new(&file.name).file_stem().unwrap().to_str().unwrap();
            let raw_file = format!("{stem}/handler_{handler}_framed.bin");
            let decoded_file = format!("{stem}/handler_{handler}.json");
            let directory = staged.path().join(stem);
            fs::create_dir_all(&directory)?;
            write(
                &staged.path().join(&raw_file),
                region.span.get(&bytes).expect("bounded region"),
            )?;
            write(
                &staged.path().join(&decoded_file),
                &serde_json::to_vec_pretty(&decoded)?,
            )?;
            sources.push(WorldSource {
                ovl: file.name.clone(),
                overlay: file.key.clone(),
                handler,
                request_kind: file.key.request_kind(),
                request: requests.0[usize::from(handler)].clone(),
                framed_region: region.span,
                raw_framed_region: raw_file,
                decoded: decoded_file,
                global_slots_in_local_order: global_slots,
            });
        }
    }
    let manifest = WorldManifest {
        schema_version: 10, kind: "infestation_world_assets",
        policy: "Only consumer-proven constructor fields, damage-response inputs, placements and ordered instance command graphs are named. Model bindings are initial H8 arguments, not final rendered models. Shared actor prefixes are not complete class payloads. Command payload semantics require the actual constructor selector; raw per-bank exports do not guess it. All spans are absolute OVL offsets; subtract framed_region.start to address a raw export. Allocation-relative offsets never become host pointers. Full framed bytes retain unknown class extensions and header fields. Position/rotation words are authored inputs; terrain height and native coordinate conversions have not been applied. Resource references require the selected scene's residency, not a corpus-wide winner.",
        totals, sources,
    };
    write(
        &staged.path().join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    staged.commit()?;
    Ok(totals)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()).into())
}

#[cfg(test)]
#[path = "worlds_tests.rs"]
mod tests;
