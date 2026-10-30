//! Every field of a saved profile by its meaning, as the original reads it.
//! [`SaveRecord`] keeps offset names and exact bytes; this report names
//! each field after its consumers in the executable (SAVE_FORMAT.md lists
//! them). Bit sets are expanded into the indices they hold.

use serde::Serialize;

use crate::{MissionSaveState, PlayerSaveState, SaveRecord};

/// World status bits in mission `+3C`, one DWORD per world.
pub mod world_status {
    /// `0048E820`: the world ended through one of its warpgates.
    pub const LEFT_BY_GATE: u32 = 1;
    /// Completed: the gate's pass `0048BE70` with `+167 = 1` (and its
    /// `+18C` blueprint known). The results page awards its medal.
    pub const COMPLETED: u32 = 2;
    /// The next load (`0048DC30`) marks a completed world it leaves; a
    /// later completion there reads "World already completed" and scores
    /// nothing (`0042B6D0`).
    pub const ALREADY_COMPLETED: u32 = 4;
    /// "Mission stage 1 complete": a gate with any other nonzero `+167`.
    pub const STAGE_ONE: u32 = 8;
}

/// The Energy Distribution rows, in the bytes `+13C..+142`.
pub const ENERGY_ROWS: [&str; 7] = [
    "Cannons",
    "Plasmas",
    "Lasers",
    "Flamethrower",
    "Rockets",
    "Grenade",
    "Shielding",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SaveProfileReport {
    /// The name's bytes, shown as Latin-1.
    pub name: String,
    pub mission: MissionReport,
    pub player: PlayerReport,
    /// Bytes after the serialized state, which saving leaves as they were.
    pub tail: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissionReport {
    /// `+30`: the world the next load starts (`0048DC30` activates overlay
    /// selection + 10). A finished world stores its gate's destination.
    pub next_selection: u32,
    /// `+34`: play time in whole seconds; the profile menu shows hours and
    /// minutes.
    pub elapsed_seconds: u32,
    /// `+A4`: the start in the next world's H1 table (`0048DAF0` reads its
    /// slot `2 * A4` for X/Z, `0048DBC0` slot `2 * A4 + 1` for the heading),
    /// which the gate stores as its ordinal less one.
    pub start_point: u16,
    /// Worlds 0..24 holding any status or bit.
    pub worlds: Vec<WorldReport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldReport {
    pub world: usize,
    /// `+3C[world]`, raw; [`world_status`] names its bits.
    pub status: u32,
    pub left_by_gate: bool,
    pub completed: bool,
    pub already_completed: bool,
    pub stage_one_complete: bool,
    /// `+B0[world * 4..]`: the 128 pickup serials collected
    /// (`0048E930/80/C0`), which the loader disposes.
    pub collected_pickups: Vec<u32>,
    /// `+240[world * 4..]`: the 128 placement indices finished
    /// (`0048EAD0`): destroyed or, for a factory, staffed. The loader skips
    /// them (`0048EA80`).
    pub finished_placements: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerReport {
    /// `+100`: the upgrades the ACAM owns (`0046BDC0`), grant IDs.
    pub upgrades: Vec<u32>,
    /// `+118`: tokens (grant 7); Energy Distribution spends them
    /// (`0046C6D0`) and the results page shows them.
    pub tokens: u32,
    /// `+11C`: ACAMs lost (`0046A860`), against the world's loss limit.
    pub acams_lost: u16,
    /// `+11E`: overall rank, the sum of the medals awarded (`0042BBF0`).
    pub rank: u16,
    /// `+120/+124/+128`: the blue, green and red resource crystal tanks
    /// (grants 0xE..0x10, two per crystal, up to 30).
    pub crystals: [u32; 3],
    /// `+12C`: turbo charges (grant 3).
    pub turbo_charges: u32,
    /// `+130`: the hints already shown (`0046C760`/`0046C820`), IDs 0..72.
    pub hints_shown: Vec<u32>,
    /// `+13C..+142`: each Energy Distribution row's level, a signed
    /// percentage ([`ENERGY_ROWS`]).
    pub energy_levels: [i8; 7],
    /// `+143`, copied with the levels and read by nothing.
    pub byte_143: u8,
    /// `+144`: the ACAM's form, vehicle `+1E8` (1 buggy, 2 truck,
    /// 3 hovercraft, 4 VTOL, 5 skimmer).
    pub vehicle_mode: u32,
    /// `+148`/`+14C`: the selected primary (0..7) and secondary (8..15)
    /// weapons, behavior `+10`/`+11`.
    pub primary_weapon: i32,
    pub secondary_weapon: i32,
    /// `+150..+18C`: each weapon's count, behavior entry `+0`; -1 is
    /// unlimited.
    pub weapon_counts: [i32; 16],
    /// `+190..+194`: the resource WORD (`+30`) of each actor in the hold,
    /// rebuilt in the hold at a respawn; 0 is empty.
    pub hold_items: [u16; 3],
    /// `+A0`, the grant owner `+9C`'s mask: the IDs known (blueprints and
    /// database entries), which factories list and the database shows.
    pub known: Vec<u32>,
}

fn bits(words: &[u32]) -> Vec<u32> {
    let mut set = Vec::new();
    for (word, &value) in words.iter().enumerate() {
        for bit in 0..32 {
            if value & 1 << bit != 0 {
                set.push(word as u32 * 32 + bit);
            }
        }
    }
    set
}

impl MissionReport {
    pub fn new(mission: &MissionSaveState) -> Self {
        let worlds = (0..mission.owner_3c.len())
            .filter_map(|world| {
                let status = mission.owner_3c[world];
                let range = world * 4..world * 4 + 4;
                let collected_pickups = bits(&mission.owner_b0[range.clone()]);
                let finished_placements = bits(&mission.owner_240[range]);
                (status != 0 || !collected_pickups.is_empty() || !finished_placements.is_empty())
                    .then(|| WorldReport {
                        world,
                        status,
                        left_by_gate: status & world_status::LEFT_BY_GATE != 0,
                        completed: status & world_status::COMPLETED != 0,
                        already_completed: status & world_status::ALREADY_COMPLETED != 0,
                        stage_one_complete: status & world_status::STAGE_ONE != 0,
                        collected_pickups,
                        finished_placements,
                    })
            })
            .collect();
        Self {
            next_selection: mission.mission_selection,
            elapsed_seconds: mission.elapsed_seconds,
            start_point: mission.owner_a4,
            worlds,
        }
    }
}

impl PlayerReport {
    pub fn new(player: &PlayerSaveState) -> Self {
        let levels: Vec<u8> = player
            .owner_13c
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        Self {
            upgrades: bits(&player.owner_100),
            tokens: player.owner_118,
            acams_lost: player.owner_11c,
            rank: player.owner_11e,
            crystals: [
                player.owner_120[0],
                player.owner_120[1],
                player.owner_120[2],
            ],
            turbo_charges: player.owner_120[3],
            hints_shown: bits(&player.owner_130),
            energy_levels: std::array::from_fn(|row| levels[row] as i8),
            byte_143: levels[7],
            vehicle_mode: player.owner_144,
            primary_weapon: player.owner_148 as i32,
            secondary_weapon: player.owner_14c as i32,
            weapon_counts: player.owner_150.map(|count| count as i32),
            hold_items: player.owner_190,
            known: bits(&player.owner_a0),
        }
    }
}

impl SaveProfileReport {
    pub fn new(record: &SaveRecord) -> Self {
        Self {
            name: record.name().iter().map(|&byte| char::from(byte)).collect(),
            mission: MissionReport::new(&record.mission()),
            player: PlayerReport::new(&record.player()),
            tail: record.tail().to_vec(),
        }
    }
}

#[cfg(test)]
#[path = "save_report_tests.rs"]
mod tests;
