//! The game protocol's own state (`00538320`): the frame header both ways
//! (`0045C3A0`, `0045C3D0`), the packed vectors of its messages (`0045DC10`,
//! `0045DD00`) and the scores (`0045D350`, `0045D4C0`, `0045C150`,
//! `0045C190`).

use thiserror::Error;

/// Sixteen players and the session, by slot.
pub const SLOTS: usize = 17;
/// `0055F100`'s team objects, indexed by a slot's state below `0x13`.
pub const TEAMS: usize = 0x13;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameNetError {
    #[error("sender slot {0} is past the {SLOTS} session words")]
    Slot(usize),
    #[error("slot {slot}'s state {state} names no team object, which retail dereferences")]
    MissingTeam { slot: usize, state: u32 },
    #[error("a received frame header needs 16 bytes, not {0}")]
    ShortHeader(usize),
}

/// A team object's scores: `+4` its index, two words `+0C` and `+0E` the
/// game state carries, `+10` its score, `+12` its kills, `+14` its deaths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamScore {
    pub index: u32,
    pub words_0c: [u16; 2],
    pub score: u16,
    pub kills: u16,
    pub deaths: u16,
}

/// The slot record of `00462460` (`0054E888`, `0x18` bytes each); `+4` is the
/// last sequence received from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotRecord {
    pub sequence: u32,
}

/// The settings words a host's header carries (settings `+10`, `+1C`, `+20`,
/// `+24`, `+28`, `+2C`, `+34`, `+4C` and `+70`), and the terrain rebuild a
/// changed detail asks for.
pub trait GameHeaderHost {
    type Error;
    fn settings_word(&mut self, offset: usize) -> u32;
    fn set_settings_word(&mut self, offset: usize, value: u32);
    /// The mission's current world (`+A0`) and its byte `+12`.
    fn mission(&self) -> (i32, u8);
    /// `0045C506`: H12's first slot resident or loaded, then `004502C0`.
    /// False leaves the detail unchanged.
    fn rebuild_terrain(&mut self, detail: u32) -> Result<bool, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameNetState {
    /// `0054E838..0054E844`, the header this machine sends.
    pub header_out: [u32; 4],
    /// `0054E788..0054E794`, the last header received.
    pub header_in: [u32; 4],
    /// `0054E338`, each slot's session word.
    pub session_words: [u32; SLOTS],
    /// `0054E380`, each slot's state: 1, or its status plus 2.
    pub states: [u32; SLOTS],
    /// `0054E400`, slots heard in the current world.
    pub heard: [u8; SLOTS],
    /// `0054E3EC`, slots whose details arrived (message 30).
    pub present: [u8; SLOTS],
    /// `0054E5B0` (16 bytes) and `0054E71C` (6 bytes): each slot's name and
    /// details.
    pub names: [[u8; 16]; SLOTS],
    pub details: [[u8; 6]; SLOTS],
    /// `0054E414`, the last kill word from each slot; `0054E804`, this
    /// machine's.
    pub kill_words: [u32; SLOTS],
    pub kill_word: u32,
    pub records: Vec<SlotRecord>,
    /// `0054E798`: kills, deaths and score by slot.
    pub scores: [[u16; 3]; SLOTS],
    pub teams: [Option<TeamScore>; TEAMS],
    /// `0054E820..0054E82B`, which message 28 carries as three DWORDs: the
    /// score limit (0: none), the winner `0054E822` and a byte, the winning
    /// team `0054E824`, team play `0054E828`, the game's end `0054E829` and
    /// two bytes.
    pub score_limit: u16,
    pub winner: u8,
    pub byte_823: u8,
    pub winning_team: u32,
    pub team_game: u8,
    pub ended: u8,
    pub bytes_82a: [u8; 2],
    /// `0055F3A0`, set on the host.
    pub host: bool,
    /// Settings `+10`, the game mode the scores read.
    pub game_mode: u32,
    /// `0054E864` and `0054E860`: the bucket and the value message 22's next
    /// frame starts from.
    pub furniture_cursor: [u32; 2],
    /// `0054E814`, the host's world word; `0054E81C` its settings bit 23;
    /// `0054E84C`, detail changes refused for want of H12.
    pub host_world: i32,
    pub host_flag_23: u8,
    pub detail_refusals: u32,
}

impl Default for GameNetState {
    fn default() -> Self {
        Self {
            header_out: [0; 4],
            header_in: [0; 4],
            session_words: [0; SLOTS],
            states: [0; SLOTS],
            heard: [0; SLOTS],
            present: [0; SLOTS],
            names: [[0; 16]; SLOTS],
            details: [[0; 6]; SLOTS],
            kill_words: [0; SLOTS],
            kill_word: 0,
            records: Vec::new(),
            scores: [[0; 3]; SLOTS],
            teams: [None; TEAMS],
            score_limit: 0,
            winner: 0,
            byte_823: 0,
            winning_team: 0,
            team_game: 0,
            ended: 0,
            bytes_82a: [0; 2],
            host: false,
            game_mode: 0,
            furniture_cursor: [0; 2],
            host_world: 0,
            host_flag_23: 0,
            detail_refusals: 0,
        }
    }
}

/// `0045DC10`: a WORD vector in three 10-bit fields (X low) and a 2-bit
/// exponent. Each component is shifted right once, and once more for each
/// halving that brings the largest magnitude to `0x3FE`; the exponent keeps
/// that shift plus 3, modulo 4.
pub fn pack_word_vector(vector: [i16; 3]) -> u32 {
    let [x, y, z] = vector.map(i32::from);
    let magnitude = x.unsigned_abs().max(y.unsigned_abs()).max(z.unsigned_abs());
    let mut shift = 1;
    let mut largest = magnitude;
    while largest > 0x3fe {
        largest >>= 1;
        shift += 1;
    }
    let field = |value: i32| (value >> shift) as u32 & 0x3ff;
    ((shift + 3) as u32) << 30 | field(z) << 20 | field(y) << 10 | field(x)
}

/// `0045DD00`: a DWORD vector in three 9-bit fields (X low) and the 5-bit
/// shift that brings the largest magnitude to `0xFF`.
///
/// The largest is chosen by signed compares, so a magnitude that wraps
/// (`i32::MIN`) is passed over unless all three wrap.
pub fn pack_dword_vector(vector: [i32; 3]) -> u32 {
    let [x, y, z] = vector;
    let [ax, ay, az] = [x, y, z].map(i32::wrapping_abs);
    let larger_yz = if ay > az { ay } else { az };
    let mut largest = if ax > larger_yz { ax } else { larger_yz } as u32;
    let mut shift = 0u32;
    while largest > 0xff {
        largest >>= 1;
        shift += 1;
    }
    let field = |value: i32| (value >> shift) as u32 & 0x1ff;
    ((field(z) | shift << 9) << 9 | field(y)) << 9 | field(x)
}

/// `0045DCC0`: [`pack_word_vector`]'s fields back, each sign-extended from
/// its WORD and shifted down by 5 less the exponent.
pub fn unpack_word_vector(packed: u32) -> [i16; 3] {
    let shift = 5u32.wrapping_sub(packed >> 30) & 0x1f;
    std::array::from_fn(|index| {
        let field = (packed >> (10 * index)) << 6;
        (i32::from(field as u16 as i16) >> shift) as i16
    })
}

/// `0045DD90`: [`pack_dword_vector`]'s fields back, each sign-extended from
/// the DWORD's top and shifted down by 23 less the shift.
pub fn unpack_dword_vector(packed: u32) -> [i32; 3] {
    let shift = 23u32.wrapping_sub(packed >> 27) & 0x1f;
    std::array::from_fn(|index| (((packed >> (9 * index)) << 23) as i32) >> shift)
}

impl GameNetState {
    /// `0045C3A0`: the header out, 16 bytes.
    pub fn write_header(&self, out: &mut [u8]) -> usize {
        for (index, word) in self.header_out.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        16
    }

    /// `0045C3D0`: keeps a received header. The sender's session word must
    /// match and its slot have a record; the frame goes on only in the
    /// current world (header WORD 6 against the mission's `+A0`, with its
    /// `+12` set). A sequence older than the record's ends there; otherwise
    /// the slot's state follows the status, and a host's header carries its
    /// settings. Returns whether the frame goes on.
    pub fn read_header<H: GameHeaderHost>(
        &mut self,
        header: &[u8],
        host: &mut H,
    ) -> Result<bool, GameNetErrorOr<H::Error>> {
        let words: [u32; 4] = std::array::from_fn(|index| {
            header
                .get(index * 4..index * 4 + 4)
                .map_or(0, |bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        });
        if header.len() < 16 {
            return Err(GameNetError::ShortHeader(header.len()).into());
        }
        self.header_in = words;
        let slot = ((words[3] >> 16) & 0xff) as usize;
        let session = *self
            .session_words
            .get(slot)
            .ok_or(GameNetError::Slot(slot))?;
        if words[1] != session {
            return Ok(false);
        }
        let Some(record) = self.records.get(slot).copied() else {
            return Ok(false);
        };
        let mut proceed = false;
        let (world, flag_12) = host.mission();
        if i32::from(words[3] as u16 as i16) == world && flag_12 != 0 {
            proceed = true;
            self.heard[slot] = 1;
        }
        if words[2] < record.sequence {
            return Ok(proceed);
        }
        self.records[slot].sequence = words[2];
        let status = words[0];
        self.states[slot] = match status & 0x1f {
            0x1f => 1,
            state => state + 2,
        };
        if (words[3] >> 24) & 0xff == 0 {
            return Ok(proceed);
        }
        host.set_settings_word(0x4c, (status >> 5) & 0x1f);
        host.set_settings_word(0x1c, (status >> 10) & 0x1f);
        host.set_settings_word(0x10, (status >> 16) & 0x7f);
        self.host_flag_23 = ((status >> 23) & 1) as u8;
        let detail = (status >> 24) & 3;
        if host.settings_word(0x70) != detail {
            if host.rebuild_terrain(detail).map_err(GameNetErrorOr::Host)? {
                host.set_settings_word(0x70, detail);
            } else {
                self.detail_refusals = self.detail_refusals.wrapping_add(1);
            }
        }
        for (bit, offset) in [(26, 0x20), (27, 0x24), (28, 0x28), (29, 0x2c), (30, 0x34)] {
            host.set_settings_word(offset, (status >> bit) & 1);
        }
        self.host_world = i32::from(words[3] as u16 as i16);
        Ok(proceed)
    }

    fn team(&mut self, slot: usize) -> Result<&mut TeamScore, GameNetError> {
        let state = self.states[slot];
        self.teams
            .get_mut(state as usize)
            .and_then(Option::as_mut)
            .ok_or(GameNetError::MissingTeam { slot, state })
    }

    /// `0045D350` on the host: the victim dies and, unless it killed itself,
    /// the killer scores 200, or 300 when the victim led (none in modes 2
    /// to 4); teams count too. A self-kill costs 200 outside modes 2 and 3.
    pub fn record_kill(&mut self, victim: i32, killer: i32, mode: u32) -> Result<(), GameNetError> {
        if !(0..=16).contains(&victim)
            || !(0..=16).contains(&killer)
            || !self.host
            || self.ended != 0
        {
            return Ok(());
        }
        let (victim, killer) = (victim as usize, killer as usize);
        let modes_2_3 = mode == 2 || mode == 3;
        let points = if modes_2_3 || mode == 4 {
            0
        } else if self.scores[killer][2] < self.scores[victim][2] {
            300
        } else {
            200
        };
        self.scores[victim][1] = self.scores[victim][1].wrapping_add(1);
        let victim_team = self.team(victim)?;
        victim_team.deaths = victim_team.deaths.wrapping_add(1);
        if victim != killer {
            let killer_team = self.team(killer)?;
            killer_team.kills = killer_team.kills.wrapping_add(1);
            self.scores[killer][0] = self.scores[killer][0].wrapping_add(1);
            self.scores[killer][2] = self.scores[killer][2].wrapping_add(points);
        } else if !modes_2_3 {
            let score = &mut self.scores[killer][2];
            *score = score.saturating_sub(200);
        }
        self.check_player_limit(killer as u8);
        Ok(())
    }

    /// `0045D4C0`, a score transfer: WORD amount, then the loser's and the
    /// winner's slots. The winner gains it (doubled when behind the loser)
    /// and the loser loses it, down to 0; a transfer to oneself only costs.
    pub fn transfer_score(&mut self, transfer: [u8; 4]) {
        let amount = u16::from_le_bytes([transfer[0], transfer[1]]);
        let (loser, winner) = (usize::from(transfer[2]), usize::from(transfer[3]));
        if loser > 16 || winner > 16 || self.ended != 0 {
            return;
        }
        let mut amount = i32::from(amount);
        if loser != winner {
            if self.scores[winner][2] < self.scores[loser][2] {
                amount *= 2;
            }
            self.scores[winner][2] = self.scores[winner][2].wrapping_add(amount as u16);
        }
        let score = &mut self.scores[loser][2];
        *score = if i32::from(*score) > amount {
            score.wrapping_sub(amount as u16)
        } else {
            0
        };
        self.check_player_limit(transfer[3]);
    }

    /// `0045D1E0`, the host's game state (message 28): `0054E820..0054E82B`,
    /// then the scores of the slots its first mask names (the others
    /// cleared) and the five words from `+0C` of the teams its second names.
    /// Returns the bytes used.
    pub fn receive_game_state(&mut self, payload: &[u8]) -> Result<i32, GameNetError> {
        let word = |at: usize| -> Result<u16, GameNetError> {
            payload
                .get(at..at + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .ok_or(GameNetError::ShortHeader(payload.len()))
        };
        let dword = |at: usize| -> Result<u32, GameNetError> {
            Ok(u32::from(word(at)?) | u32::from(word(at + 2)?) << 16)
        };
        let [limit, winning_team, flags] = [dword(0)?, dword(4)?, dword(8)?];
        self.score_limit = limit as u16;
        self.winner = (limit >> 16) as u8;
        self.byte_823 = (limit >> 24) as u8;
        self.winning_team = winning_team;
        [
            self.team_game,
            self.ended,
            self.bytes_82a[0],
            self.bytes_82a[1],
        ] = flags.to_le_bytes();
        let (present, teams) = (word(12)?, word(14)?);
        let mut at = 16;
        for bit in 0..16 {
            let slot = bit + 1;
            if present & 1 << bit == 0 {
                self.scores[slot] = [0; 3];
                continue;
            }
            self.scores[slot] = [word(at)?, word(at + 2)?, word(at + 4)?];
            at += 6;
        }
        for bit in 0..16 {
            if teams & 1 << bit == 0 {
                continue;
            }
            let index = bit + 2;
            let words = [
                word(at)?,
                word(at + 2)?,
                word(at + 4)?,
                word(at + 6)?,
                word(at + 8)?,
            ];
            let team = self.teams[index]
                .as_mut()
                .ok_or(GameNetError::MissingTeam {
                    slot: 0,
                    state: index as u32,
                })?;
            team.words_0c = [words[0], words[1]];
            team.score = words[2];
            team.kills = words[3];
            team.deaths = words[4];
            at += 10;
        }
        Ok(at as i32)
    }

    /// `0045C150`: outside team play, a slot reaching the score limit wins.
    pub fn check_player_limit(&mut self, slot: u8) -> bool {
        if slot > 16 || self.score_limit == 0 || self.team_game != 0 {
            return false;
        }
        if self.scores[usize::from(slot)][2] < self.score_limit {
            return false;
        }
        self.winner = slot;
        self.ended = 1;
        true
    }

    /// `0045C190`: in team play, a team reaching the score limit wins.
    pub fn check_team_limit(&mut self, team: TeamScore) -> bool {
        if team.index >= TEAMS as u32 || self.score_limit == 0 || self.team_game == 0 {
            return false;
        }
        if team.score < self.score_limit {
            return false;
        }
        self.winning_team = team.index;
        self.ended = 1;
        true
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameNetErrorOr<H> {
    #[error(transparent)]
    Game(#[from] GameNetError),
    #[error("game header host: {0:?}")]
    Host(H),
}

#[cfg(test)]
#[path = "game_tests.rs"]
mod tests;
