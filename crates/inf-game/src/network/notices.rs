//! The notices world code gives the game protocol when a player's vehicle is
//! hit (`0045DA50`) or dies (`0045D8F0`): the last attacker, the kill word,
//! the scores on the host and the death's line.

use thiserror::Error;

use super::game::{GameNetError, GameNetState};
use super::game_tick::GameFrameState;
use super::lines::format_line;
use super::protocol::GameSend;

/// `0045DA50`'s countdown: the attacker is remembered for 0.75 s.
const ATTACKER_MICROS: u32 = 750_000;

/// The protocol's state, moved from the link to the world while it updates.
#[derive(Clone, Debug, Default)]
pub struct NetworkGameHold {
    pub game: GameNetState,
    pub frame: GameFrameState,
}

impl NetworkGameHold {
    pub fn loan(&mut self) -> NetworkGameLoan<'_> {
        NetworkGameLoan {
            game: &mut self.game,
            frame: &mut self.frame,
        }
    }
}

/// The protocol's state world code reaches.
pub struct NetworkGameLoan<'a> {
    pub game: &'a mut GameNetState,
    pub frame: &'a mut GameFrameState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NoticeError {
    /// `0045D8F0` divides its draw by the group's count.
    #[error("the death's line group {0} is empty: retail divides by zero here")]
    EmptyLineGroup(&'static str),
    #[error(transparent)]
    Game(#[from] GameNetError),
}

/// What a death notice leaves to its caller: the sends, then the line for
/// the first player's message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathNotice {
    pub sends: Vec<(u32, GameSend)>,
    pub line: Option<Vec<u8>>,
}

impl NetworkGameLoan<'_> {
    /// `0045DA50` with the packet `[kind, kind, victim, attacker]`: the
    /// attacker (`0054E830`) is remembered for 0.75 s (`0054E82C`); the host
    /// moves the score (`0045D4C0`), a guest tells it (message 27).
    pub fn damage_notice(&mut self, packet: [u8; 4]) -> Option<(u32, GameSend)> {
        self.frame.attacker_830 = packet[3];
        self.frame.countdown_82c = ATTACKER_MICROS;
        if self.game.host {
            self.game.transfer_score(packet);
            None
        } else {
            Some((27, GameSend::Dword(u32::from_le_bytes(packet))))
        }
    }

    /// `0045D8F0` with the packet `[damage, damage, victim, killer]`. Unless
    /// the game is over, with both slots under 16 and a victim: a draw
    /// (`0048F060`) picks the line, an unknown killer is the attacker still
    /// remembered, the kill word counts the death and names the killer, the
    /// line is formatted (`0045D7C0`) from the self-kill, kill or death group;
    /// the host records the kill (`0045D350`), a guest sends its kill word
    /// (26); outside Powerball the line goes to every machine (31) and to
    /// the first player.
    pub fn death_notice(
        &mut self,
        mut packet: [u8; 4],
        rng: &mut u32,
    ) -> Result<DeathNotice, NoticeError> {
        let mut notice = DeathNotice {
            sends: Vec::new(),
            line: None,
        };
        if self.game.ended != 0 || packet[3] >= 0x10 || packet[2] >= 0x10 || packet[2] == 0 {
            return Ok(notice);
        }
        let draw = crate::name_entry::name_entry_rng_next(rng) & 0xffff;
        if packet[3] == 0 && self.frame.countdown_82c != 0 {
            packet[3] = self.frame.attacker_830;
        }
        let [_, _, victim, killer] = packet;
        let word = self.game.kill_word;
        self.game.kill_word = u32::from(killer) << 16 | (word as u16).wrapping_add(1) as u32;
        let (group, name, b) = if victim == killer {
            (
                &self.frame.selfkill_lines,
                "selfkill.txt",
                i32::from(killer),
            )
        } else if killer != 0 {
            (&self.frame.kill_lines, "kill.txt", i32::from(killer))
        } else {
            (&self.frame.die_lines, "Die.txt", 0)
        };
        if group.is_empty() {
            return Err(NoticeError::EmptyLineGroup(name));
        }
        let format = &group[draw as usize % group.len()];
        let line = format_line(
            format,
            0x100,
            [i32::from(victim), b],
            false,
            self.game,
            |_| None,
        );
        if self.game.host {
            let mode = self.game.game_mode;
            self.game
                .record_kill(i32::from(victim), i32::from(killer), mode)?;
        } else {
            notice.sends.push((26, GameSend::State));
        }
        if self.game.game_mode != 4 {
            notice.sends.push((31, GameSend::Text(line.clone())));
            notice.line = Some(line);
        }
        Ok(notice)
    }
}

#[cfg(test)]
#[path = "notices_tests.rs"]
mod tests;
