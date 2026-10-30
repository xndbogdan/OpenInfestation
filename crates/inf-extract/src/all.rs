//! Canonical full-installation export orchestration.
//!
//! Each family publishes atomically into its own managed directory. Keeping
//! the orchestration here prevents the CLI from developing a second set of
//! extraction rules that drifts from the individual commands.

use std::path::Path;

use crate::{
    fonts, installation, materials, missions, models, opaque, sounds, sprites, strings, tables,
    terrain, words, worlds,
};

pub const EXPORTED_FAMILY_NAMES: [&str; 13] = [
    "installation",
    "sounds",
    "sprites",
    "strings",
    "fonts",
    "world_assets",
    "missions",
    "word_resources",
    "materials",
    "record_tables",
    "opaque_resources",
    "models",
    "terrain",
];
pub const EXPORTED_FAMILIES: usize = EXPORTED_FAMILY_NAMES.len();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllExportReport {
    pub families: usize,
    pub installation: installation::InstallationExportReport,
    pub sounds: sounds::SoundExportReport,
    pub sprites: sprites::SpriteExportReport,
    pub strings: strings::StringExportReport,
    pub fonts: fonts::FontExportReport,
    pub worlds: worlds::WorldExportReport,
    pub missions: missions::MissionExportReport,
    pub words: words::WordExportReport,
    pub materials: materials::MaterialExportReport,
    pub tables: tables::TableExportReport,
    pub opaque: opaque::OpaqueExportReport,
    pub models: models::ModelExportReport,
    pub terrain: terrain::TerrainExportReport,
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
) -> Result<AllExportReport, Box<dyn std::error::Error>> {
    let installation = installation::export(data_root, output_root)?;
    let sounds = sounds::export(data_root, output_root)?;
    let sprites = sprites::export(
        data_root,
        output_root,
        &sprites::SpriteExportOptions::default(),
    )?;
    let strings = strings::export(data_root, output_root)?;
    let fonts = fonts::export(data_root, output_root)?;
    let worlds = worlds::export(data_root, output_root)?;
    let missions = missions::export(data_root, output_root)?;
    let words = words::export(data_root, output_root)?;
    let materials = materials::export(data_root, output_root)?;
    let tables = tables::export(data_root, output_root)?;
    let opaque = opaque::export(data_root, output_root)?;
    let models = models::export(
        data_root,
        output_root,
        &models::ModelExportOptions::default(),
    )?;
    let terrain = terrain::export(
        data_root,
        output_root,
        &terrain::TerrainExportOptions::default(),
    )?;

    Ok(AllExportReport {
        families: EXPORTED_FAMILIES,
        installation,
        sounds,
        sprites,
        strings,
        fonts,
        worlds,
        missions,
        words,
        materials,
        tables,
        opaque,
        models,
        terrain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_export_plan_includes_preload_and_every_handler_family_once() {
        assert_eq!(EXPORTED_FAMILIES, 13);
        assert_eq!(EXPORTED_FAMILY_NAMES[0], "installation");
        let unique = EXPORTED_FAMILY_NAMES
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), EXPORTED_FAMILIES);
        assert!(unique.contains("opaque_resources"));
        assert!(unique.contains("terrain"));
        assert!(unique.contains("models"));
        assert!(unique.contains("sprites"));
        assert!(unique.contains("fonts"));
        assert!(unique.contains("world_assets"));
    }
}
