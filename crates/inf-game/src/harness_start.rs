//! The start harness (`--start [STATE.json]`), a port debug tool: the
//! session skips the title, the menus and the briefing and loads one world
//! with an injected game state, the way loading a saved game brings it in.
//! `00487380` constructs a fresh player and `00449B80` restores the working
//! profile's mission and player prefix over it; the harness supplies that
//! mission and prefix instead of a profile slot, so the respawn builds the
//! vehicle from them as it does for any loaded game.
//!
//! Every field is optional. An empty state (or `--start` without a file) is
//! a new game's player arriving at Level 1 (world 18) at its first start.
//! The ids are the game's grant IDs (`inf-extract save` names them); lists
//! also take "all" or "none".
//!
//! ```json
//! {
//!   "world": 5,                     // mission +30, the world to load
//!   "start": 0,                     // mission +A4, the world's H1 start
//!   "position": [24000, 31000],     // X/Z instead of the start's; Y is the ground's
//!   "heading": 90,                  // degrees, as the H1 table stores them
//!   "elapsed_seconds": 600,         // mission +34
//!   "world_status": {"18": 6},      // mission +3C bits per world
//!   "upgrades": [50, 155],          // +100: grant IDs owned
//!   "known": "all",                 // nested +A0: grant IDs known (closure applied)
//!   "tokens": 12,                   // +118
//!   "crystals": [30, 30, 30],       // +120/+124/+128: blue, green, red
//!   "turbo": 3,                     // +12C
//!   "hints_seen": "none",           // +130: hint IDs 0..72
//!   "energy": [1, 0, 0, 0, 0, 0, 2],// +13C..+142: Energy Distribution levels
//!   "acams_lost": 0,                // +11C (the world load zeroes it)
//!   "rank": 0,                      // +11E
//!   "form": 1,                      // +144: buggy 1 .. skimmer 5
//!   "primary": 0, "secondary": 8,   // +148/+14C
//!   "ammo": {"8": 20, "9": 5},      // +150[weapon]: count, -1 unlimited
//!   "hold": [0, 0, 0],              // +190: resource WORD of each carried actor
//!   "rng": 4240278240,              // 0055F18C at the first world tick (see below)
//!   "clock_us": 16000,              // 00546F30 there, with that tick's delta
//!   "briefing_frames": 120,         // frames of the briefing before the mission
//!   "briefing_keys": true,          // or: the briefing ends by its own Start (the keys)
//!   "vehicle": "0000…",             // a recorded vehicle's +000..+5FF, 3072 hex digits
//!   "camera_cache": [14226, 1446, 52491], // 005469E8 at the first world tick
//!   "world_far": 5120,              // the world manager's far W+1C at the load
//!   "day_clock_ms": 780000,         // the tint schedule's timer W+2C after the load
//!   "tint_running": true,           // mission +13 after the load (Artoria's dark)
//!   "weather": true,                // a rain or snow world's fall at the load (port weather)
//!   "load_rng": 3854101754,         // 0055F18C at the world load's entry
//!   "captured_nodes": [0, 1, 2],    // capture nodes taken at the first world tick
//!   "profile": 0                    // the --profiles slot loaded first
//! }
//! ```
//!
//! `rng` and `clock_us` are for the retail scenario lab's comparisons
//! (`runtime_re/lab`): the RNG word and the animation
//! clock's microsecond accumulator the first world tick starts from,
//! retail's own at the same point, which a start without the intro and menus
//! cannot reach. A saved game holds neither. `briefing_frames`, for the same
//! comparisons, holds the briefing (phase 3) that many frames before the
//! harness starts the mission, as retail's briefing stays up while its menu
//! is worked: its view draws the world's objects, whose draw callbacks set
//! state the first world tick reads. `briefing_keys` instead leaves the
//! briefing to the `--input-replay` keys, as retail's player worked its menu:
//! its Start Mission starts the briefing's own start timer, and the pane stops
//! drawing then, which a held briefing does not. `vehicle`, a TTD recording's vehicle
//! bytes (`lab/snapshot.py`'s state), has the first world tick write that
//! vehicle's drive state into the spawned one
//! ([`ObjectArena::inject_vehicle_drive_state`]), so a window in the middle
//! of a world replays from where the recording was. `camera_cache` is the
//! process's cached camera position `005469E8`..`EC`, which survives a world
//! change: a new game's Level 1 starts with the intro's last camera there,
//! and what gates on the camera's distance in the first tick reads it.
//! `world_far` is the world manager's far distance `W+1C` (`G+1CC`), which
//! the timing feedback lowers to `0x1400` when frames run slow and a world
//! load keeps: a new game's Level 1 starts with the intro's, where a save's
//! first world starts at `0x2800`. The load clamps the world's header to
//! three quarters of it and starts the drawn far there (`00488380`), so a
//! new game draws fewer objects at first: the factory, among others.
//! `day_clock_ms` is the world's tint timer `W+2C` (`G+1DC`), which the
//! world tick adds its milliseconds to and the load's header zeroes: a
//! probe starts there to see a world's sunset or night without playing up
//! to it. `tint_running` sets mission `+13`, which starts a world's tint
//! schedule running as Artoria's dead generators do.
//! `load_rng` is the RNG word at the world load's entry (`0048DC30`): the
//! load's own draws (the terrain map, each constructor's, the camera's
//! search) then start from retail's word, as `rng` does at the first tick.
//!
//! `captured_nodes` lists capture nodes (selector 84, the n-th in placement
//! order: world 19's six beacons) that the first player's vehicle takes at
//! the first world tick, as a scientist's touch takes them (`0043C150`):
//! each, with its linked gate, joins the vehicle's team and leaves the
//! aliens' count, so a start at the Alien Brain can bring its beacons taken.
//!
//! The position replaces only the first spawn: a later respawn (after the
//! vehicle's death) uses the world's start, as in retail. The world load
//! zeroes `acams_lost` (`0046A880`), as every visit counts its own losses,
//! so a loss limit is only reached by dying in play.
//!
//! Without `profile` no profile slot is loaded, so a results screen's save
//! fails. `profile` loads that slot of the `--profiles` file first, as the
//! profile menu's load does (`0044A170`, then `00449B80` restores its name,
//! mission and player), and the state's fields then replace what it
//! restored; the results screen then saves into that slot. The lab gives the
//! port retail's own start save, its one profile, and loads slot 0, as
//! retail's run loads it. Use a scratch `--profiles` file with it.

use std::collections::BTreeMap;
use std::path::Path;

use inf_formats::{
    grant_into, GrantTable, MissionSaveState, PlayerSaveState, SaveRecord, GRANT_MASK_WORDS,
    MISSION_SAVE_BYTES, PLAYER_SAVE_BYTES, SAVE_PAYLOAD_BYTES,
};
use serde::Deserialize;
use thiserror::Error;

/// The world a default start loads: Level 1.
pub const DEFAULT_WORLD: u32 = 18;
/// Grant IDs run 0..192 (six DWORDs).
const GRANT_IDS: u32 = (GRANT_MASK_WORDS * 32) as u32;
/// Hint IDs run 0..73 (`00414610` steps through 0..0x48).
const HINT_IDS: u32 = 0x49;
/// Mission arrays hold one entry per world.
const WORLDS: u32 = 25;

/// A list of IDs, or every or no ID.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum IdList {
    Ids(Vec<u32>),
    Word(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessStart {
    pub world: Option<u32>,
    pub start: Option<u16>,
    pub position: Option<[u16; 2]>,
    pub heading: Option<i16>,
    pub elapsed_seconds: Option<u32>,
    pub world_status: Option<BTreeMap<u32, u32>>,
    pub upgrades: Option<IdList>,
    pub known: Option<IdList>,
    pub tokens: Option<u32>,
    pub crystals: Option<[u32; 3]>,
    pub turbo: Option<u32>,
    pub hints_seen: Option<IdList>,
    pub energy: Option<[i8; 7]>,
    pub acams_lost: Option<u16>,
    pub rank: Option<u16>,
    pub form: Option<u32>,
    pub primary: Option<u32>,
    pub secondary: Option<u32>,
    pub ammo: Option<BTreeMap<usize, i32>>,
    pub hold: Option<[u16; 3]>,
    pub rng: Option<u32>,
    pub clock_us: Option<u32>,
    pub briefing_frames: Option<u32>,
    pub briefing_keys: Option<bool>,
    pub vehicle: Option<String>,
    pub camera_cache: Option<[u16; 3]>,
    pub world_far: Option<u32>,
    pub day_clock_ms: Option<u32>,
    pub tint_running: Option<bool>,
    pub weather: Option<bool>,
    pub load_rng: Option<u32>,
    pub profile: Option<usize>,
    pub captured_nodes: Option<Vec<usize>>,
}

/// Where the first spawn lands instead of the world's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessSpawn {
    /// X and Z WORDs; Y is the ground's, as for any start.
    pub position: Option<[u16; 2]>,
    /// Degrees, converted as the H1 table's are (`0048DBC0`).
    pub heading: Option<i16>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HarnessStartError {
    #[error("start state {path}: {reason}")]
    File { path: String, reason: String },
    #[error("{field}: \"{word}\" is neither \"all\" nor \"none\"")]
    Word { field: &'static str, word: String },
    #[error("{field}: ID {id} is past its {limit}")]
    Id {
        field: &'static str,
        id: u32,
        limit: u32,
    },
    #[error("world {0} is past the 25 mission worlds")]
    World(u32),
    #[error("ammo: weapon {0} is past the sixteen weapons")]
    Weapon(usize),
    #[error("known: {0}")]
    Grant(String),
    #[error("save: {0}")]
    Save(String),
}

impl HarnessStart {
    /// The state in `path`, or the default start without one.
    pub fn load(path: Option<&Path>) -> Result<Self, HarnessStartError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let error = |reason: String| HarnessStartError::File {
            path: path.display().to_string(),
            reason,
        };
        let text = std::fs::read_to_string(path).map_err(|e| error(e.to_string()))?;
        let state: Self = serde_json::from_str(&text).map_err(|e| error(e.to_string()))?;
        state.validate()?;
        Ok(state)
    }

    fn validate(&self) -> Result<(), HarnessStartError> {
        let world = self.world.unwrap_or(DEFAULT_WORLD);
        if world >= WORLDS {
            return Err(HarnessStartError::World(world));
        }
        if let Some(status) = &self.world_status {
            if let Some(&world) = status.keys().find(|&&world| world >= WORLDS) {
                return Err(HarnessStartError::World(world));
            }
        }
        for (field, list, limit) in [
            ("upgrades", &self.upgrades, GRANT_IDS),
            ("known", &self.known, GRANT_IDS),
            ("hints_seen", &self.hints_seen, HINT_IDS),
        ] {
            ids(field, list.as_ref(), limit)?;
        }
        if let Some(ammo) = &self.ammo {
            if let Some(&weapon) = ammo.keys().find(|&&weapon| weapon >= 16) {
                return Err(HarnessStartError::Weapon(weapon));
            }
        }
        self.vehicle_bytes()?;
        Ok(())
    }

    /// The recorded vehicle's 0x600 bytes, when the state names them.
    pub fn vehicle_bytes(&self) -> Result<Option<Vec<u8>>, HarnessStartError> {
        let Some(hex) = &self.vehicle else {
            return Ok(None);
        };
        let bytes = (hex.len() == 0xc00 && hex.is_ascii())
            .then(|| {
                (0..0x600)
                    .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
                    .collect::<Option<Vec<u8>>>()
            })
            .flatten();
        bytes.map(Some).ok_or_else(|| HarnessStartError::File {
            path: "vehicle".into(),
            reason: "not the 3072 hex digits of a vehicle's +000..+5FF".into(),
        })
    }

    /// The mission a new game resets (`0048EBA0`) with this state's world,
    /// start, play time and world bits.
    pub fn mission(&self) -> MissionSaveState {
        let mut mission = MissionSaveState::new_game();
        mission.mission_selection = self.world.unwrap_or(DEFAULT_WORLD);
        mission.owner_a4 = self.start.unwrap_or(0);
        if let Some(seconds) = self.elapsed_seconds {
            mission.elapsed_seconds = seconds;
        }
        if let Some(status) = &self.world_status {
            for (&world, &bits) in status {
                mission.owner_3c[world as usize] = bits;
            }
        }
        mission
    }

    /// The fresh player's prefix with this state's fields written over it.
    pub fn apply_player(
        &self,
        player: &mut PlayerSaveState,
        grants: &GrantTable,
    ) -> Result<(), HarnessStartError> {
        if let Some(list) = &self.upgrades {
            player.owner_100 = mask(ids("upgrades", Some(list), GRANT_IDS)?);
        }
        if let Some(list) = &self.known {
            let mut known = [0; GRANT_MASK_WORDS];
            for id in ids("known", Some(list), GRANT_IDS)?.unwrap_or_default() {
                grant_into(&mut known, grants, id)
                    .map_err(|error| HarnessStartError::Grant(error.to_string()))?;
            }
            player.owner_a0 = known;
        }
        if let Some(list) = &self.hints_seen {
            let words = mask(ids("hints_seen", Some(list), HINT_IDS)?);
            player.owner_130.copy_from_slice(&words[..3]);
        }
        if let Some(tokens) = self.tokens {
            player.owner_118 = tokens;
        }
        if let Some(crystals) = self.crystals {
            player.owner_120[..3].copy_from_slice(&crystals);
        }
        if let Some(turbo) = self.turbo {
            player.owner_120[3] = turbo;
        }
        if let Some(levels) = self.energy {
            let bytes: [u8; 8] = std::array::from_fn(|index| match index {
                0..=6 => levels[index] as u8,
                // +143, which nothing reads, keeps its value.
                _ => player.owner_13c[1].to_le_bytes()[3],
            });
            player.owner_13c = [
                u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")),
                u32::from_le_bytes(bytes[4..].try_into().expect("four bytes")),
            ];
        }
        if let Some(lost) = self.acams_lost {
            player.owner_11c = lost;
        }
        if let Some(rank) = self.rank {
            player.owner_11e = rank;
        }
        if let Some(form) = self.form {
            player.owner_144 = form;
        }
        if let Some(primary) = self.primary {
            player.owner_148 = primary;
        }
        if let Some(secondary) = self.secondary {
            player.owner_14c = secondary;
        }
        if let Some(ammo) = &self.ammo {
            for (&weapon, &count) in ammo {
                player.owner_150[weapon] = count as u32;
            }
        }
        if let Some(hold) = self.hold {
            player.owner_190 = hold;
        }
        Ok(())
    }

    /// This state as a saved profile named `name`, for the retail scenario
    /// lab (`--write-start-save`, runtime_re/lab): the mission and player
    /// prefix over the fresh player `00487380` constructs, with a zero tail
    /// (the bytes past the state are heap leftovers in retail). A save holds
    /// no free position, only the mission's `+A4` start, so `position` and
    /// `heading` are refused.
    pub fn save_record(
        &self,
        name: &[u8],
        grants: &GrantTable,
    ) -> Result<SaveRecord, HarnessStartError> {
        if self.spawn().is_some()
            || self.rng.is_some()
            || self.clock_us.is_some()
            || self.briefing_frames.is_some()
            || self.briefing_keys.is_some()
            || self.vehicle.is_some()
            || self.camera_cache.is_some()
            || self.world_far.is_some()
            || self.day_clock_ms.is_some()
            || self.tint_running.is_some()
            || self.weather.is_some()
            || self.load_rng.is_some()
            || self.profile.is_some()
            || self.captured_nodes.is_some()
        {
            return Err(HarnessStartError::Save(
                "position, heading, rng, clock_us, briefing_frames, briefing_keys, vehicle, camera_cache, world_far, day_clock_ms, tint_running, weather, load_rng, profile and captured_nodes are not part of a saved game"
                    .into(),
            ));
        }
        let mut game = crate::offline_prepare::offline_game_with_selection(grants, self.world)
            .map_err(|error| HarnessStartError::Grant(error.to_string()))?;
        game.mission = self.mission();
        self.apply_player(&mut game.player, grants)?;
        let tail =
            vec![0; SAVE_PAYLOAD_BYTES - name.len() - 1 - MISSION_SAVE_BYTES - PLAYER_SAVE_BYTES];
        SaveRecord::from_state(name, &game.mission, &game.player, &tail)
            .map_err(|error| HarnessStartError::Save(error.to_string()))
    }

    /// The first spawn's position and heading, if either replaces the start's.
    pub fn spawn(&self) -> Option<HarnessSpawn> {
        (self.position.is_some() || self.heading.is_some()).then_some(HarnessSpawn {
            position: self.position,
            heading: self.heading,
        })
    }
}

/// The IDs of `list`: None for "none", every ID below `limit` for "all".
fn ids(
    field: &'static str,
    list: Option<&IdList>,
    limit: u32,
) -> Result<Option<Vec<u32>>, HarnessStartError> {
    match list {
        None => Ok(None),
        Some(IdList::Word(word)) => match word.as_str() {
            "all" => Ok(Some((0..limit).collect())),
            "none" => Ok(Some(Vec::new())),
            _ => Err(HarnessStartError::Word {
                field,
                word: word.clone(),
            }),
        },
        Some(IdList::Ids(ids)) => {
            if let Some(&id) = ids.iter().find(|&&id| id >= limit) {
                return Err(HarnessStartError::Id { field, id, limit });
            }
            Ok(Some(ids.clone()))
        }
    }
}

fn mask(ids: Option<Vec<u32>>) -> [u32; GRANT_MASK_WORDS] {
    let mut words = [0; GRANT_MASK_WORDS];
    for id in ids.unwrap_or_default() {
        words[(id >> 5) as usize] |= 1 << (id & 31);
    }
    words
}

#[cfg(test)]
#[path = "harness_start_tests.rs"]
mod tests;
