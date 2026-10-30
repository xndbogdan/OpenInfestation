mod all;
mod coverage;
mod fonts;
mod installation;
mod inventory;
mod managed_output;
mod materials;
mod missions;
mod models;
mod opaque;
mod saves;
mod sounds;
mod sprites;
mod strings;
mod tables;
mod terrain;
mod words;
mod worlds;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(about = "Canonical Infestation data inspector")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export proven constructor fields, initial model bindings, and world placements.
    Worlds {
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },
    /// Export every supported family, preserving undecoded regions as opaque bytes.
    All {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory for every asset family.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Parse PreLoad.dat and frame every OVL in handler dispatch order.
    Inventory {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Emit aggregate counters without the per-file region list.
        #[arg(long)]
        summary_only: bool,
    },

    /// Audit typed asset coverage and remaining opaque or runtime-dependent data.
    Coverage {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Emit aggregate coverage without the per-model variant list.
        #[arg(long)]
        summary_only: bool,
    },

    /// Export exact PreLoad.dat, its parsed tree, and the complete OVL inventory.
    Installation {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath installation/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export Handler-9 PCM sounds and alias metadata using the runtime mapping.
    Sounds {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; sounds are written beneath sounds/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export Handler-3 sprite variants, raw payloads, and palette metadata.
    Sprites {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; sprites are written beneath sprites/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,

        /// Indexed-palette projection used for PNGs: `base` or a shade from 0 through 31.
        #[arg(long, default_value = "base")]
        shade: sprites::SpriteExportShade,

        /// Export only this exact installed OVL filename; repeat for multiple OVLs.
        #[arg(long = "ovl")]
        ovl_filters: Vec<String>,
    },

    /// Export Handler-12 terrain channels, raw payloads, and recovered Handler-7 previews.
    Terrain {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; terrain is written beneath terrain/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,

        /// Export only this exact installed OVL filename; repeat for multiple OVLs.
        #[arg(long = "ovl")]
        ovl_filters: Vec<String>,
    },

    /// Export Handler-0 strings, raw payloads, and global-slot metadata.
    Strings {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; strings are written beneath strings/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export Handler-4 font metrics, sprite arguments, and exact source bytes.
    Fonts {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath fonts/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export raw Handler-11 terrain lookup and Handler-15 selection tables.
    Missions {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; missions are written beneath missions/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export raw fixed-width resources from Handlers 1, 2, and 6.
    Words {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath word_resources/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export Handler-7 material values without guessing unresolved roles.
    Materials {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath materials/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export the proven fixed record layouts used by Handlers 5 and 10.
    Tables {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath record_tables/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Archive exact Handler-4/13/14 regions alongside their separate typed exports.
    Opaque {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; output is written beneath opaque_resources/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,
    },

    /// Export lossless Handler-8 models and conservative static-command diagnostic OBJs.
    Models {
        /// Directory containing PreLoad.dat and Overlay/.
        #[arg(long, default_value = "retail")]
        data_root: PathBuf,

        /// Root generated-data directory; models are written beneath models/.
        #[arg(long, default_value = "extracted")]
        output: PathBuf,

        /// Export only this exact installed OVL filename; repeat for multiple OVLs.
        #[arg(long = "ovl")]
        ovl_filters: Vec<String>,
    },

    /// Print every profile of a save (Infest.sav, retail's and the port's)
    /// with each field by its meaning.
    Save {
        /// The save files to read; none are written.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// An installation (Infest.exe, PreLoad.dat, Overlay/) to name grants
        /// and hints from.
        #[arg(long)]
        data_root: Option<PathBuf>,

        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Worlds { data_root, output } => {
            let report = worlds::export(&data_root, &output)?;
            println!(
                "Exported {} constructor resources ({} initial model bindings) and {} placements with {} instance commands across {} worlds into {}",
                report.constructor_resources,
                report.initial_model_bindings,
                report.placements,
                report.instance_commands,
                report.world_resources,
                output.join("world_assets").display()
            );
        }
        Command::All { data_root, output } => {
            let report = all::export(&data_root, &output)?;
            println!(
                "Exported all {} canonical Infestation data families into {}",
                report.families,
                output.display()
            );
        }
        Command::Inventory {
            data_root,
            summary_only,
        } => {
            let report = inventory::build(&data_root)?;
            if summary_only {
                serde_json::to_writer_pretty(std::io::stdout().lock(), &report.summary)?;
            } else {
                serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
            }
            println!();
        }
        Command::Coverage {
            data_root,
            summary_only,
        } => {
            let report = coverage::build(&data_root)?;
            if summary_only {
                serde_json::to_writer_pretty(std::io::stdout().lock(), &report.summary)?;
            } else {
                serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
            }
            println!();
        }
        Command::Installation { data_root, output } => {
            let report = installation::export(&data_root, &output)?;
            println!(
                "Exported exact PreLoad.dat and {}-OVL inventory ({} handler regions, {} covered bytes) into {}",
                report.overlay_files,
                report.handler_regions,
                report.covered_overlay_bytes,
                output.join("installation").display()
            );
        }
        Command::Sounds { data_root, output } => {
            let report = sounds::export(&data_root, &output)?;
            println!(
                "Exported {} Handler-9 slots ({} PCM WAVs, {} aliases, {} unknown, {} PCM bytes, {} exact raw regions) from {} OVLs into {}",
                report.slots,
                report.pcm_wavs,
                report.aliases,
                report.unknown,
                report.pcm_bytes,
                report.raw_region_files,
                report.sources,
                output.join("sounds").display()
            );
        }
        Command::Sprites {
            data_root,
            output,
            shade,
            ovl_filters,
        } => {
            let report = sprites::export(
                &data_root,
                &output,
                &sprites::SpriteExportOptions { shade, ovl_filters },
            )?;
            println!(
                "Exported {} Handler-3 variants ({} registered entries, {} ignored entries, {} unique palettes) from {} OVLs into {}",
                report.physical_variants,
                report.registered_entries,
                report.orphan_entries,
                report.unique_palette_files,
                report.sources,
                output.join("sprites").display()
            );
        }
        Command::Terrain {
            data_root,
            output,
            ovl_filters,
        } => {
            let report = terrain::export(
                &data_root,
                &output,
                &terrain::TerrainExportOptions { ovl_filters },
            )?;
            println!(
                "Exported {} Handler-12 terrain entries ({} cells, {} sparse records, {} PNGs) from {} OVLs into {}",
                report.entries,
                report.cells,
                report.sparse_records,
                report.pngs,
                report.sources,
                output.join("terrain").display()
            );
        }
        Command::Save {
            files,
            data_root,
            json,
        } => {
            println!("{}", saves::run(&files, data_root.as_deref(), json)?);
        }
        Command::Strings { data_root, output } => {
            let report = strings::export(&data_root, &output)?;
            println!(
                "Exported {} Handler-0 strings ({} raw payload bytes) from {} OVLs into {}",
                report.strings,
                report.payload_bytes,
                report.sources,
                output.join("strings").display()
            );
        }
        Command::Fonts { data_root, output } => {
            let report = fonts::export(&data_root, &output)?;
            println!(
                "Exported {} Handler-4 fonts ({} glyph metrics, {} sprite arguments, {} exact framed bytes) from {} OVLs into {}",
                report.fonts,
                report.glyph_metrics,
                report.sprite_arguments,
                report.framed_bytes,
                report.sources,
                output.join("fonts").display()
            );
        }
        Command::Missions { data_root, output } => {
            let report = missions::export(&data_root, &output)?;
            println!(
                "Exported {} placement tables ({} nonzero slots) and {} parameter records from {} OVLs into {}",
                report.placement_tables,
                report.nonzero_placement_slots,
                report.parameter_records,
                report.sources,
                output.join("missions").display()
            );
        }
        Command::Words { data_root, output } => {
            let report = words::export(&data_root, &output)?;
            println!(
                "Exported {} fixed u32 resources from {} Handler-1/2/6 regions into {}",
                report.entries,
                report.sources,
                output.join("word_resources").display()
            );
        }
        Command::Materials { data_root, output } => {
            let report = materials::export(&data_root, &output)?;
            println!(
                "Exported {} Handler-7 values ({} exact terrain palettes, {} payload bytes) from {} OVLs into {}",
                report.entries,
                report.terrain_palettes,
                report.payload_bytes,
                report.sources,
                output.join("materials").display()
            );
        }
        Command::Tables { data_root, output } => {
            let report = tables::export(&data_root, &output)?;
            println!(
                "Exported {} Handler-5 and {} Handler-10 records ({} typed sample grids, {} retained trailing bytes) from {} regions into {}",
                report.entries_by_handler[0],
                report.entries_by_handler[1],
                report.sample_grids,
                report.handler10_trailing_bytes,
                report.sources,
                output.join("record_tables").display()
            );
        }
        Command::Opaque { data_root, output } => {
            let report = opaque::export(&data_root, &output)?;
            println!(
                "Archived {} raw Handler-4/13/14 regions ({} requested resources, {} exact framed bytes) from {} OVLs into {}",
                report.regions,
                report.requested_resources,
                report.framed_bytes,
                report.source_ovls,
                output.join("opaque_resources").display()
            );
        }
        Command::Models {
            data_root,
            output,
            ovl_filters,
        } => {
            let report = models::export(
                &data_root,
                &output,
                &models::ModelExportOptions { ovl_filters },
            )?;
            println!(
                "Exported {} Handler-8 entries from {} OVLs ({} static-command diagnostic OBJs, {} explicitly omitted) into {}",
                report.entries,
                report.sources,
                report.diagnostic_objs,
                report.omitted_diagnostic_objs,
                output.join("models").display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprites_cli_accepts_repeatable_filters_and_a_shade_level() {
        let cli = Cli::try_parse_from([
            "inf-extract",
            "sprites",
            "--shade",
            "17",
            "--ovl",
            "0.ovl",
            "--ovl",
            "9d2.ovl",
        ])
        .unwrap();
        let Command::Sprites {
            shade, ovl_filters, ..
        } = cli.command
        else {
            panic!("expected sprites command");
        };
        assert_eq!(shade, sprites::SpriteExportShade::Level(17));
        assert_eq!(ovl_filters, ["0.ovl", "9d2.ovl"]);
    }

    #[test]
    fn models_cli_accepts_repeatable_filters() {
        let cli = Cli::try_parse_from([
            "inf-extract",
            "models",
            "--ovl",
            "1.ovl",
            "--ovl",
            "3d2.ovl",
        ])
        .unwrap();
        let Command::Models { ovl_filters, .. } = cli.command else {
            panic!("expected models command");
        };
        assert_eq!(ovl_filters, ["1.ovl", "3d2.ovl"]);
    }

    #[test]
    fn terrain_cli_accepts_repeatable_filters() {
        let cli = Cli::try_parse_from([
            "inf-extract",
            "terrain",
            "--ovl",
            "11.ovl",
            "--ovl",
            "29.ovl",
        ])
        .unwrap();
        let Command::Terrain { ovl_filters, .. } = cli.command else {
            panic!("expected terrain command");
        };
        assert_eq!(ovl_filters, ["11.ovl", "29.ovl"]);
    }
}
