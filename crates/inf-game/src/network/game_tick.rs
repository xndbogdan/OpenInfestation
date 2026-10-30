//! The game protocol's work each frame, which the game's update
//! (`004875A0`) and the join wait (`0045FF80`) run: the header out
//! (`0045C320`), the player table's changes (`0045C940`) and the sends
//! (`0045C5A0`); and the protocol's reset when a game starts (`0045CB70`).

use super::game::{GameNetState, SlotRecord, TeamScore, SLOTS, TEAMS};
use super::lines::{format_line, MessageLines, LINE_TEXT};
use super::protocol::{GameProtocol, GameProtocolError, GameSend, GameWorldHost, GAME_CHANNEL};
use super::receivers::{GameCall, GameMessageHost};
use super::transport::{NetTransport, TransportState, PLAYER_TABLE_ENTRIES};

/// `0053804C`: the sends wait until more than this much time
/// (microseconds) has gathered.
const SEND_PERIOD: i32 = 100_000;
/// `0045C66C`: the host's game-over line changes every 150 clock ticks.
const LINE_PERIOD: u32 = 150;
/// `0045CCE0`'s files by kind (`0053838C`, `00538384`, `00538374`,
/// `00538368`, `0053835C`), in the game's folder.
pub const LINE_FILES: [&str; 5] = [
    "kill.txt",
    "Die.txt",
    "selfkill.txt",
    "Names.txt",
    "Host.txt",
];
/// `0045CB70` loads up to ten lines a group.
pub const LINE_GROUP_LIMIT: usize = 10;
/// `00538354`: the default for a missing H0 string.
const NULL_TEXT: &[u8] = b"null";

/// What `0045CCE0` reads a group from: the game's folder (`00463800`) and
/// the defaults H0 `0x5B`, `0x5A` and `0x5C` for kinds 0 to 2, each "null"
/// when H0 lacks it. Kinds 3 and 4 default to `005468D4`, a static empty
/// string nothing writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineGroupSource {
    pub folder: std::path::PathBuf,
    pub defaults: [Option<Vec<u8>>; 3],
}

impl LineGroupSource {
    /// `0045CCE0(kind)`: the file's lines (`0045CE60`), else one default.
    pub fn load(&self, kind: usize) -> Vec<Vec<u8>> {
        let lines = std::fs::read(crate::retail_path(&self.folder, LINE_FILES[kind]))
            .map(|bytes| parse_line_file(&bytes, LINE_GROUP_LIMIT))
            .unwrap_or_default();
        if !lines.is_empty() {
            return lines;
        }
        vec![match self.defaults.get(kind) {
            Some(text) => text.clone().unwrap_or_else(|| NULL_TEXT.to_vec()),
            None => Vec::new(),
        }]
    }
}

/// `0045CE60` over a file read in the CRT's text mode (`fopen` with "r":
/// `0x1A` ends it and CR LF reads as LF): `fgets` chunks of up to 255
/// bytes, each past its leading white space (`isspace`; bytes from 0x80 on,
/// which it would read as negative, count as none) and cut at its first CR
/// or LF; empty chunks and `#` comments are skipped, and up to `limit` lines
/// kept.
pub fn parse_line_file(bytes: &[u8], limit: usize) -> Vec<Vec<u8>> {
    let bytes = &bytes[..bytes.iter().position(|&b| b == 0x1a).unwrap_or(bytes.len())];
    let mut text = Vec::with_capacity(bytes.len());
    for (index, &byte) in bytes.iter().enumerate() {
        if !(byte == b'\r' && bytes.get(index + 1) == Some(&b'\n')) {
            text.push(byte);
        }
    }
    let mut lines = Vec::new();
    let mut rest = &text[..];
    while !rest.is_empty() && lines.len() < limit {
        let window = &rest[..rest.len().min(255)];
        let take = window
            .iter()
            .position(|&b| b == b'\n')
            .map_or(window.len(), |at| at + 1);
        let (chunk, tail) = rest.split_at(take);
        rest = tail;
        let start = chunk
            .iter()
            .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
            .unwrap_or(chunk.len());
        let line = &chunk[start..];
        let cut = line
            .iter()
            .position(|&b| b == b'\r' || b == b'\n')
            .unwrap_or(line.len());
        let line = &line[..cut];
        if !line.is_empty() && line[0] != b'#' {
            lines.push(line.to_vec());
        }
    }
    lines
}

/// An actor's send period, countdown and count (`+D8`, `+DC`, `+E0`), and
/// whether its `+38` bit 15 holds it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorSendTimer {
    pub period_d8: u32,
    pub countdown_dc: u32,
    pub count_e0: u32,
    pub held: bool,
}

/// The protocol's state the frame's work keeps beside [`GameNetState`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameFrameState {
    /// `0054E810`, the time gathered towards the next send.
    pub gathered: u32,
    /// `0054E82C`, a countdown the delta wears down: how long the last
    /// attacker `0054E830` is remembered.
    pub countdown_82c: u32,
    pub attacker_830: u8,
    /// `0054E800`, the clock's 150-tick step a host line last went out on.
    pub line_step: u32,
    /// `00538344`, the slot whose objects the next send checks.
    pub check_slot: u32,
    /// The line groups `0045CB70` loads once (`0045CCE0`): `kill.txt`,
    /// `Die.txt`, `selfkill.txt` and `Host.txt` (counts `0054E6C0`,
    /// `0054E808`, `0054E458`, `0054E37C`). The module's release `0045C0B0`
    /// frees each group's lines (`0045C100`); here they drop with the
    /// state.
    pub kill_lines: Vec<Vec<u8>>,
    pub die_lines: Vec<Vec<u8>>,
    pub selfkill_lines: Vec<Vec<u8>>,
    pub host_lines: Vec<Vec<u8>>,
    /// `0054E460`, the four message lines.
    pub lines: MessageLines,
}

impl GameFrameState {
    /// `0045CB70`'s `0045CCE0` calls: each line group still empty loads
    /// (kinds 0, 1, 2 and 4: `kill.txt`, `Die.txt`, `selfkill.txt`,
    /// `Host.txt`), as a game starts ([`LineGroupSource::load`]).
    pub fn load_line_groups(&mut self, load: impl Fn(usize) -> Vec<Vec<u8>>) {
        for (group, kind) in [
            (&mut self.kill_lines, 0),
            (&mut self.die_lines, 1),
            (&mut self.selfkill_lines, 2),
            (&mut self.host_lines, 4),
        ] {
            if group.is_empty() {
                *group = load(kind);
            }
        }
    }
}

/// The world's side of the frame's work, beyond the protocol's host.
pub trait GameFrameHost: GameWorldHost {
    /// `00462460(slot)`'s entries from 1: the handles of the live objects
    /// the slot's table holds, in entry order; None past the table's owners.
    fn slot_handles(
        &mut self,
        slot: u8,
    ) -> Result<Option<Vec<u32>>, <Self as GameMessageHost>::Error>;
    fn send_timer(
        &mut self,
        handle: u32,
    ) -> Result<ActorSendTimer, <Self as GameMessageHost>::Error>;
    fn set_send_timer(
        &mut self,
        handle: u32,
        countdown_dc: u32,
        count_e0: u32,
    ) -> Result<(), <Self as GameMessageHost>::Error>;
    /// An object's `+3C`, the clock its next record is due by.
    fn deadline(&mut self, handle: u32) -> Result<i32, <Self as GameMessageHost>::Error>;
    /// Virtual `+C8` on an object whose records stopped: the shared actor's
    /// `0047ECF0` moves its deadline to the clock plus 200 and asks again
    /// with message 7, which the caller sends when this returns true.
    fn expire(&mut self, handle: u32) -> Result<bool, <Self as GameMessageHost>::Error>;
    /// `0054ED70`: whether the race's selector 70 actor exists (message 42).
    fn race_actor(&mut self) -> bool;
    /// `0048F060`, the process RNG's next value.
    fn random(&mut self) -> Result<u32, <Self as GameMessageHost>::Error>;
    /// `005471C0`, the shared animation clock.
    fn clock(&self) -> u32;
    /// `00463D10`: the network name (16 bytes) and initials (6).
    fn identity(&mut self) -> Result<([u8; 16], [u8; 6]), <Self as GameMessageHost>::Error>;
}

/// `strncpy` into `N` bytes, then the last byte cleared.
fn copy_text<const N: usize>(text: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    let text = text.split(|&c| c == 0).next().unwrap_or(&[]);
    let length = text.len().min(N);
    out[..length].copy_from_slice(&text[..length]);
    out[N - 1] = 0;
    out
}

impl GameNetState {
    /// The registry's owner records (`0054E888`): owner 0 reserved, the
    /// others' `+4` at 1, as `00461DE0` builds them.
    pub fn with_slot_records() -> Self {
        Self {
            records: (0..SLOTS)
                .map(|slot| SlotRecord {
                    sequence: u32::from(slot != 0),
                })
                .collect(),
            ..Self::default()
        }
    }

    /// `0045C320`: the header's sequence moves on, it takes this machine's
    /// player id, its slot and (as host) its slot again, and the status's
    /// state becomes this slot's.
    pub fn publish_header(&mut self, transport: &TransportState) {
        self.header_out[2] = self.header_out[2].wrapping_add(1);
        self.header_out[1] = transport.session_id;
        let slot = u32::from(transport.local_slot);
        let host = if transport.host { slot } else { 0 };
        self.header_out[3] = self.header_out[3] & 0xffff | slot << 16 | host << 24;
        let state = self.header_out[0] & 0x1f;
        self.states[usize::from(transport.local_slot)] = if state < 0x12 { state + 2 } else { 1 };
    }

    /// `0045CB70`: the session words, scores, kill words and game state
    /// cleared; every slot "Player N", absent and unheard; the message lines
    /// idle; the states and every task's team words cleared; team play
    /// (`0054E828`) for modes 0 and 4. `teams` holds the words of the
    /// nineteen tasks `0055F100` names, which retail writes through (a
    /// missing one faults). Its line groups load through
    /// [`GameFrameState::load_line_groups`].
    pub fn reset_game(&mut self, mode: u32, frame: &mut GameFrameState) {
        self.session_words = [0; SLOTS];
        self.scores = [[0; 3]; SLOTS];
        self.kill_words = [0; SLOTS];
        self.kill_word = 0;
        self.score_limit = 0;
        self.winner = 0;
        self.byte_823 = 0;
        self.winning_team = 0;
        self.team_game = 0;
        self.ended = 0;
        self.bytes_82a = [0; 2];
        for slot in 0..SLOTS {
            self.names[slot] = copy_text(format!("Player {slot}").as_bytes());
            self.present[slot] = 0;
            self.heard[slot] = 0;
        }
        for line in &mut frame.lines.lines {
            line.timer = 0;
        }
        self.states = [0; SLOTS];
        for team in self.teams.iter_mut().flatten() {
            *team = TeamScore {
                index: team.index,
                ..TeamScore::default()
            };
        }
        if mode == 0 || mode == 4 {
            self.team_game = 1;
        }
    }

    /// Whether all nineteen team tasks are present, as `0045CB70` needs.
    pub fn teams_present(&self) -> bool {
        self.teams.len() == TEAMS && self.teams.iter().all(Option::is_some)
    }
}

impl<H: GameFrameHost> GameProtocol<'_, H>
where
    <H as GameMessageHost>::Error: std::fmt::Debug,
{
    /// `0045C940`: once, this slot's name and initials, and as host the
    /// winning score; then each other slot whose player changed in the
    /// table: its scores cleared and its kill word's low half, its record's
    /// sequence reset, absent and to be asked its name, and when a player
    /// had it, every object its table holds disposed. Whether anything was.
    /// (`Names.txt`, whose lines would come first, is never loaded.)
    pub fn receive_slots(
        &mut self,
        transport: &TransportState,
    ) -> Result<bool, GameProtocolError<<H as GameMessageHost>::Error>> {
        let local = usize::from(transport.local_slot);
        if local == 0 {
            return Ok(false);
        }
        if self.state.present[local] == 0 {
            let (name, initials) = self.host.identity().map_err(GameProtocolError::Host)?;
            self.state.names[local] = name;
            self.state.details[local] = initials;
            self.state.present[local] = 1;
            if transport.host {
                self.state.score_limit = self.host.settings_word(0x30) as u16;
            }
        }
        let table = transport.players;
        let mut disposed = false;
        for slot in 1..PLAYER_TABLE_ENTRIES.min(SLOTS) {
            if slot == local || table[slot] == self.state.session_words[slot] {
                continue;
            }
            self.state.scores[slot] = [0; 3];
            self.state.kill_words[slot] &= 0xffff_0000;
            if let Some(record) = self.state.records.get_mut(slot) {
                record.sequence = 0;
                self.state.present[slot] = 0;
                self.state.heard[slot] = 1;
                if self.state.session_words[slot] != 0 {
                    let handles = self
                        .host
                        .slot_handles(slot as u8)
                        .map_err(GameProtocolError::Host)?;
                    for handle in handles.unwrap_or_default() {
                        self.host
                            .call(GameCall::Dispose { object: handle }, self.state)
                            .map_err(GameProtocolError::Host)?;
                        disposed = true;
                    }
                }
            }
            self.state.session_words[slot] = table[slot];
        }
        Ok(disposed)
    }

    /// `0045C5A0`, with the frame's delta. Once more than a tenth of a
    /// second has gathered: the host sends its game state, the world's
    /// values and the territory list (after a game-over line every 150 clock
    /// ticks once the game has ended), a guest its kill word when its deaths
    /// moved; each slot heard without details is asked its name; each local
    /// object whose countdown ran out sends its record; and one other slot a
    /// time has its overdue objects expire. The frame goes out either way.
    /// The flow control (`0054E858`, `0054E85C`) never holds a send back:
    /// the port's sends leave at once, so `00493830` and `004937E0` read 0.
    pub fn send_frame(
        &mut self,
        transport: &mut NetTransport,
        frame: &mut GameFrameState,
        delta: u32,
    ) -> Result<(), GameProtocolError<<H as GameMessageHost>::Error>> {
        frame.gathered = frame.gathered.wrapping_add(delta);
        frame.countdown_82c = frame.countdown_82c.saturating_sub(delta);
        let gathered = frame.gathered;
        if gathered as i32 <= SEND_PERIOD {
            transport.finish_frame(GAME_CHANNEL, self);
            return Ok(());
        }
        frame.gathered = 0;
        let local = usize::from(transport.state.local_slot);
        if !transport.state.host {
            if self.state.scores[local][1] != self.state.kill_word as u16 {
                self.send(transport, 26, GameSend::State)?;
            }
        } else {
            let step = self.host.clock() / LINE_PERIOD;
            if self.state.ended != 0
                && (frame.host_lines.len() > 1 || self.state.team_game != 0)
                && frame.line_step != step
            {
                frame.line_step = step;
                let line = frame.lines.allocate();
                if self.state.team_game == 0 {
                    let random = self.host.random().map_err(GameProtocolError::Host)?;
                    let pick = i64::from(random & 0xffff) % (frame.host_lines.len() as i64 - 1);
                    let winner = i32::from(self.state.winner);
                    let text = format_line(
                        &frame.host_lines[pick as usize + 1],
                        LINE_TEXT,
                        [winner, winner],
                        false,
                        self.state,
                        |_| None,
                    );
                    let stored = &mut frame.lines.lines[line].text;
                    *stored = [0; LINE_TEXT];
                    stored[..text.len()].copy_from_slice(&text);
                    self.send(transport, 31, GameSend::Text(text))?;
                }
            }
            self.send(transport, 28, GameSend::State)?;
            self.send(transport, 36, GameSend::WorldValues)?;
            self.send(transport, 38, GameSend::Territory)?;
            if self.host.race_actor() {
                self.send(transport, 42, GameSend::Race)?;
            }
        }
        for slot in 1..SLOTS {
            if self.state.heard[slot] != 0 {
                if self.state.present[slot] == 0 {
                    self.send(transport, 29, GameSend::Dword(slot as u32))?;
                }
                self.state.heard[slot] = 0;
            }
        }
        let handles = self
            .host
            .slot_handles(local as u8)
            .map_err(GameProtocolError::Host)?;
        for handle in handles.unwrap_or_default() {
            let timer = self
                .host
                .send_timer(handle)
                .map_err(GameProtocolError::Host)?;
            if timer.countdown_dc >= gathered {
                self.host
                    .set_send_timer(handle, timer.countdown_dc - gathered, timer.count_e0)
                    .map_err(GameProtocolError::Host)?;
                continue;
            }
            if timer.held {
                continue;
            }
            let mut countdown = timer.period_d8;
            if countdown != 0 {
                let random = self.host.random().map_err(GameProtocolError::Host)?;
                let jitter = (random & 0xffff).wrapping_mul(delta >> 2) >> 13;
                countdown = countdown.wrapping_add(jitter);
            }
            self.host
                .set_send_timer(handle, countdown, timer.count_e0.wrapping_add(1))
                .map_err(GameProtocolError::Host)?;
            self.send(transport, 1, GameSend::Actor(handle))?;
        }
        frame.check_slot = frame.check_slot.wrapping_add(1);
        if frame.check_slot != local as u32 {
            let handles = self
                .host
                .slot_handles(frame.check_slot as u8)
                .map_err(GameProtocolError::Host)?;
            match handles {
                None => frame.check_slot = 0,
                Some(handles) => {
                    let clock = self.host.clock() as i32;
                    for handle in handles {
                        if self
                            .host
                            .deadline(handle)
                            .map_err(GameProtocolError::Host)?
                            < clock
                            && self.host.expire(handle).map_err(GameProtocolError::Host)?
                        {
                            self.send(transport, 7, GameSend::Dword(handle))?;
                        }
                    }
                }
            }
        }
        transport.finish_frame(GAME_CHANNEL, self);
        transport.finish_frame(GAME_CHANNEL, self);
        Ok(())
    }
}

#[cfg(test)]
#[path = "game_tick_tests.rs"]
mod tests;
