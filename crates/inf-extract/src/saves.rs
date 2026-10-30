//! `inf-extract save`: every profile of an `Infest.sav` (retail's and the
//! port's are one file) with each field by its
//! meaning. With an installation, grant IDs and hints read as their names.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use inf_formats::{
    parse_grant_texts, parse_overlay, parse_string_bank, FrontendExecutable, OverlayKey, Preload,
    SaveArchive, SaveBlockSource, SaveDirectoryEnd, SaveProfileReport, ENERGY_ROWS,
};
use serde::Serialize;

/// The hint texts' H0 strings, `0053A27C`, IDs 0..72.
const HINT_TEXT_ADDRESS: u32 = 0x0053_a27c;
const HINT_COUNT: u32 = 0x49;
/// The weapons' names (`0041A740`'s H0 strings).
const WEAPON_NAMES: [u16; 16] = [
    0x14c, 0x14d, 0x14e, 0x14f, 0x150, 0x151, 0x152, 0x153, 0x144, 0x145, 0x146, 0x147, 0x148,
    0x149, 0x14a, 0x14b,
];
/// The English common overlays holding these strings.
const NAME_OVERLAYS: [&str; 2] = ["0l0.ovl", "1l0.ovl"];

/// Names from an installation: H0 strings, each grant's text and each
/// hint's text.
pub struct SaveNames {
    strings: BTreeMap<u16, Vec<u8>>,
    grant_texts: Vec<u16>,
    hint_texts: Vec<u16>,
}

#[derive(Serialize)]
struct SaveFileReport {
    path: String,
    bytes: usize,
    /// Profiles read before the directory ended.
    profiles: Vec<ProfileEntry>,
    /// Why reading stopped, as `0044A170` stops.
    end: String,
}

#[derive(Serialize)]
struct ProfileEntry {
    index: usize,
    /// Blocks read from their backup copy (`00449F50` falls back on a bad CRC).
    backup_blocks: Vec<usize>,
    report: SaveProfileReport,
}

impl SaveNames {
    pub fn load(data_root: &Path) -> Result<Self, String> {
        let executable_bytes = std::fs::read(data_root.join("Infest.exe"))
            .map_err(|error| format!("Infest.exe: {error}"))?;
        let executable = FrontendExecutable::parse(&executable_bytes).map_err(|e| e.to_string())?;
        let grant_texts = parse_grant_texts(executable)
            .map_err(|e| e.to_string())?
            .to_vec();
        let table = executable
            .mapped(HINT_TEXT_ADDRESS, HINT_COUNT as usize * 2)
            .map_err(|e| e.to_string())?;
        let hint_texts = table
            .bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let preload_bytes = std::fs::read(data_root.join("PreLoad.dat"))
            .map_err(|error| format!("PreLoad.dat: {error}"))?;
        let preload = Preload::parse(&preload_bytes).map_err(|e| e.to_string())?;
        let mut strings = BTreeMap::new();
        for name in NAME_OVERLAYS {
            let key = OverlayKey::parse_filename(name).map_err(|e| e.to_string())?;
            let leaf = preload
                .leaf(key.level)
                .ok_or(format!("{name} has no PreLoad.dat leaf"))?;
            let requests = leaf.requests(key.request_kind());
            let path = data_root.join("Overlay").join(name);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let layout = parse_overlay(&bytes, requests).map_err(|e| e.to_string())?;
            let region = layout
                .regions
                .iter()
                .find(|region| region.handler.get() == 0)
                .ok_or(format!("{name} has no strings"))?;
            let bank = parse_string_bank(&bytes, region).map_err(|e| e.to_string())?;
            for entry in &bank.entries {
                let Some(global) = requests
                    .get(0)
                    .and_then(|request| request.global_index(entry.local_index as u16))
                else {
                    continue;
                };
                strings
                    .entry(global)
                    .or_insert_with(|| bytes[entry.bytes.start..entry.bytes.end].to_vec());
            }
        }
        Ok(Self {
            strings,
            grant_texts,
            hint_texts,
        })
    }

    fn string(&self, id: u16) -> Option<String> {
        self.strings
            .get(&id)
            .map(|bytes| bytes.iter().map(|&byte| char::from(byte)).collect())
    }

    fn grant(&self, id: u32) -> Option<String> {
        let text = *self.grant_texts.get(id as usize)?;
        (text != 0).then(|| self.string(text)).flatten()
    }

    fn hint(&self, id: u32) -> Option<String> {
        self.string(*self.hint_texts.get(id as usize)?)
    }
}

fn read_report(path: &Path) -> Result<SaveFileReport, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let scan = SaveArchive::scan_directory(&bytes);
    let profiles = scan
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| ProfileEntry {
            index,
            backup_blocks: scan
                .block_sources
                .get(index)
                .map(|sources| {
                    sources
                        .iter()
                        .enumerate()
                        .filter(|(_, source)| matches!(source, SaveBlockSource::Backup { .. }))
                        .map(|(block, _)| block)
                        .collect()
                })
                .unwrap_or_default(),
            report: SaveProfileReport::new(record),
        })
        .collect();
    let end = match &scan.end {
        SaveDirectoryEnd::EndOfFile => "end of file".to_string(),
        other => format!("{other:?}"),
    };
    Ok(SaveFileReport {
        path: path.display().to_string(),
        bytes: bytes.len(),
        profiles,
        end,
    })
}

fn named(id: u32, name: Option<String>) -> String {
    match name {
        Some(name) => format!("{id:#x} {name}"),
        None => format!("{id:#x}"),
    }
}

fn text_report(file: &SaveFileReport, names: Option<&SaveNames>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({} bytes, {} profiles, ended at {})",
        file.path,
        file.bytes,
        file.profiles.len(),
        file.end
    );
    for entry in &file.profiles {
        let report = &entry.report;
        let mission = &report.mission;
        let player = &report.player;
        let seconds = mission.elapsed_seconds;
        let _ = writeln!(out, "\nProfile {}: {:?}", entry.index, report.name);
        if !entry.backup_blocks.is_empty() {
            let _ = writeln!(
                out,
                "  read from backup copies: blocks {:?}",
                entry.backup_blocks
            );
        }
        let _ = writeln!(
            out,
            "  next world: selection {} (overlay {}), start point {}",
            mission.next_selection,
            mission.next_selection.wrapping_add(10),
            mission.start_point
        );
        let _ = writeln!(
            out,
            "  play time: {}:{:02}:{:02} ({} s)",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60,
            seconds
        );
        for world in &mission.worlds {
            let mut flags = Vec::new();
            if world.left_by_gate {
                flags.push("left by its gate");
            }
            if world.completed {
                flags.push("completed");
            }
            if world.already_completed {
                flags.push("already completed");
            }
            if world.stage_one_complete {
                flags.push("stage 1 complete");
            }
            let _ = writeln!(
                out,
                "  world {}: status {:#x} [{}], {} pickups collected {:?}, {} placements finished {:?}",
                world.world,
                world.status,
                flags.join(", "),
                world.collected_pickups.len(),
                world.collected_pickups,
                world.finished_placements.len(),
                world.finished_placements
            );
        }
        let _ = writeln!(
            out,
            "  tokens {}, crystals blue {} green {} red {} (of 30), turbo charges {}",
            player.tokens,
            player.crystals[0],
            player.crystals[1],
            player.crystals[2],
            player.turbo_charges
        );
        let _ = writeln!(
            out,
            "  ACAMs lost {}, rank {} (medals), form {}",
            player.acams_lost, player.rank, player.vehicle_mode
        );
        let weapon = |index: i32| -> String {
            let name = usize::try_from(index)
                .ok()
                .and_then(|slot| WEAPON_NAMES.get(slot))
                .and_then(|&id| names.and_then(|names| names.string(id)));
            match name {
                Some(name) => format!("{index} {name}"),
                None => index.to_string(),
            }
        };
        let _ = writeln!(
            out,
            "  weapons: primary {}, secondary {}",
            weapon(player.primary_weapon),
            weapon(player.secondary_weapon)
        );
        let counts: Vec<String> = player
            .weapon_counts
            .iter()
            .enumerate()
            .filter(|(_, &count)| count != 0)
            .map(|(index, count)| format!("{}: {count}", weapon(index as i32)))
            .collect();
        let _ = writeln!(out, "  weapon counts: {}", counts.join(", "));
        let levels: Vec<String> = ENERGY_ROWS
            .iter()
            .zip(player.energy_levels)
            .map(|(row, level)| format!("{row} {level}%"))
            .collect();
        let _ = writeln!(out, "  energy distribution: {}", levels.join(", "));
        if player.byte_143 != 0 {
            let _ = writeln!(out, "  byte +143: {:#x}", player.byte_143);
        }
        let _ = writeln!(
            out,
            "  hold items (resource WORDs): {:?}",
            player.hold_items
        );
        let grants = |ids: &[u32]| -> Vec<String> {
            ids.iter()
                .map(|&id| named(id, names.and_then(|names| names.grant(id))))
                .collect()
        };
        let _ = writeln!(out, "  upgrades owned ({}):", player.upgrades.len());
        for line in grants(&player.upgrades) {
            let _ = writeln!(out, "    {line}");
        }
        let _ = writeln!(
            out,
            "  known (blueprints and database, {}):",
            player.known.len()
        );
        for line in grants(&player.known) {
            let _ = writeln!(out, "    {line}");
        }
        let _ = writeln!(out, "  hints shown ({}):", player.hints_shown.len());
        for &hint in &player.hints_shown {
            let _ = writeln!(
                out,
                "    {}",
                named(hint, names.and_then(|names| names.hint(hint)))
            );
        }
        let distinct: std::collections::BTreeSet<u8> = report.tail.iter().copied().collect();
        let _ = writeln!(
            out,
            "  unused tail: {} bytes{}",
            report.tail.len(),
            if distinct.len() == 1 {
                format!(" of {:#04x}", report.tail[0])
            } else {
                String::new()
            }
        );
    }
    out
}

pub fn run(
    paths: &[std::path::PathBuf],
    data_root: Option<&Path>,
    json: bool,
) -> Result<String, String> {
    let names = data_root.map(SaveNames::load).transpose()?;
    let reports = paths
        .iter()
        .map(|path| read_report(path))
        .collect::<Result<Vec<_>, _>>()?;
    if json {
        return serde_json::to_string_pretty(&reports).map_err(|e| e.to_string());
    }
    Ok(reports
        .iter()
        .map(|report| text_report(report, names.as_ref()))
        .collect::<Vec<_>>()
        .join("\n"))
}
