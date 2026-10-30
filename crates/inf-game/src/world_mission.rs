//! Load-time fields of the mission owner W = G+1B0 around 0048DC30.
//!
//! The serialized half is [`MissionSaveState`]: +30 selection, +240 four
//! completion DWORDs per world and +3C per-world status. The runtime current
//! world +A0 is separate from the +30 selection and is not serialized. The
//! genuine 0048D9F0 constructor stores1 at0048DA88 independently of its later
//! reset of serialized selection to22; explicit supplied-owner fixtures may
//! instead use `WorldMissionInputs` without claiming this constructor.
//!
//! 0048DC30 first clears byte +A6, marks the previous world's +3C status
//! (bit 2 when bit 1 is set, testing only the low byte), clears +38 and then
//! copies +30 into +A0. 0048EA80 reads the current world's four completion
//! DWORDs at +240 + A0*16. After resource activation the prelude also derives
//! +13..+2C from the H14 world header, and the loader tail writes +10, clears
//! the 0x158-byte record named by +0C and sets +12. Terrain globals chosen by
//! header flag 0x80 belong to the terrain owner.

use inf_formats::{MissionSaveState, WorldResource};
use thiserror::Error;

/// +3C and +240 hold one entry per world; 0048EBA0 clears 25 of them.
pub const MISSION_WORLDS: u32 = 25;
/// Worlds 25..33 are the network arenas (`0053C570`), which retail indexes
/// past the per-world arrays.
pub const ARENA_WORLDS: std::ops::Range<u32> = MISSION_WORLDS..34;

/// Inputs consumed by 0048EAD0 independently of mission selection/current world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementDeathRequest {
    pub index: i32,
    /// Live W+08; the loader temporarily inhibits bookkeeping with this byte.
    pub inhibit_byte_08: u8,
    /// Native final byte argument. A nonzero value bypasses W+08 inhibition.
    pub force: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldMission {
    pub save: MissionSaveState,
    /// W+08 / G+1B8, initialized0 by 0048D9F0 at48DA1A. Manager cleanup
    /// 00478CD0 writes1 before its callback walk and0 on normal return;
    /// placement bookkeeping must read this retained byte, including during
    /// callbacks. The retained-manager cleanup walk is a separate owner.
    pub inhibit_byte_08: u8,
    /// +A0, the world whose completion words 0048EA80 reads.
    pub current_world_a0: u32,
    /// Constructor stores +A6=0 but leaves DWORD+38 unwritten. Explicit input
    /// construction leaves both unknown; the load prelude writes both fields.
    pub byte_a6: Option<u8>,
    pub dword_38: Option<u32>,
    /// +0C, aliased by G+1BC: constructor 0048D9F0 allocates the0x158-byte
    /// statistics record,00480FC0 clears it, and the loader's 00480FD0 clears
    /// it again. Recovered retirement fields belong to `world_statistics`.
    pub record_0c: Option<Box<[u8; RECORD_0C_BYTES]>>,
    /// +1C visibility distance: 0048D9F0 initializes 0x2800 and 0048E2E0
    /// updates it from frame timing (`world_fade`). Retained across reload.
    pub dword_1c: u32,
    /// Byte +13 is cleared only when header flag 0x1000 is set.
    pub byte_13: Option<u8>,
    pub word_10: Option<u16>,
    pub byte_12: Option<u8>,
    pub bytes_14_16: Option<[u8; 3]>,
    /// +17 (G+1C7), the doom flag: the constructor sets it, a course
    /// target's countdown clears it, and `start_doom_countdown` sets it.
    pub byte_17: Option<u8>,
    pub byte_18: Option<u8>,
    pub dword_20: Option<u32>,
    pub dword_24: Option<u32>,
    pub word_28: Option<u16>,
    /// W+2A (G+1DA): a gate's 0048B411 writes 0x119 beside its +164 in
    /// W+28. The constructor and the loader write only W+28's WORD.
    pub word_2a: Option<u16>,
    pub dword_2c: Option<u32>,
    /// Port carrier, not a W field: a stabiliser's death (`0047653F`) wrote
    /// process DWORD 005483B0 = 1 during this world tick. The lifecycle host
    /// owns only the mission, so the session applies it before the camera
    /// and the terrain draw read the word.
    pub terrain_event_5483b0: bool,
}

/// 00480FD0's fixed memset length.
pub const RECORD_0C_BYTES: usize = 0x158;

/// Load-time inputs of W, including retained visibility from `world_fade`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldMissionInputs {
    pub current_world_a0: u32,
    pub dword_1c: u32,
    pub byte_13: Option<u8>,
    pub record_0c: Option<Box<[u8; RECORD_0C_BYTES]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum WorldMissionError {
    /// Native indexing is unbounded and would read neighboring owner fields.
    #[error("mission world index {0} is outside the 25 per-world entries")]
    WorldOutOfRange(u32),
    /// W+240 + 16 * world for an arena world reaches W+3D0's counter and the
    /// game object's fields past the mission (G+584...).
    #[error("arena world {0}'s completion words are the game object's fields past the mission")]
    ArenaCompletion(u32),
    /// W+3C + 4 * world for 27 and 28 is the start override's X or Z WORD,
    /// which the port drops with +A6.
    #[error("arena world {0}'s status is a start override word the port does not keep")]
    ArenaStatus(u32),
    /// The tail's IDIV by header WORD +16 raises #DE for zero.
    #[error("world header WORD +16 is zero, the loader tail's divisor")]
    ZeroTailDivisor,
    #[error("the mission's countdown reads its unwritten {0}")]
    UnwrittenCountdown(&'static str),
}

/// `0048E51A`: the doom's last stage runs 3 s.
const DOOM_LAST_MICROS: u32 = 3_000_000;

/// What the mission tick's countdown (`0048E33E..0048E51A`) did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionCountdownTick {
    /// No countdown runs (+24 is 0).
    Idle,
    /// +24 counted down by the delta, or reached 0 without +17.
    Counted,
    /// +17 is set, under 5 s are left and the first player has a unit:
    /// the caller runs `0048E380..0048E4C5`, the chance of an explosion
    /// near it. The countdown has counted down.
    Explosions,
    /// The end's first stage with a player unit: +24 is 0 and +18 set. The
    /// caller retires the unit if it has a vehicle view (`0048E505..
    /// 0048E515`), then arms the last seconds
    /// ([`WorldMission::arm_doom_last_seconds`]).
    UnitEnd,
    /// The end's first stage without a unit: +18 set, 3 s more.
    LastSeconds,
    /// The end's second stage: +18 clear and +15 set; the caller ends the
    /// world with `00487560(world, 1, 0)`, the current world again.
    MissionFailed { world: u32 },
}

impl WorldMission {
    /// All represented 0048D9F0 constructor fields in the successful statistics
    /// allocation domain. Native mallocNULL, raw backing and the later process
    /// tint initialization43A370 are separate owners. WORD+10/DWORD+38 remain
    /// unknown; zeros in adjacent fields do not establish them.
    pub fn native_constructor() -> Self {
        Self {
            // PUSH1/PUSH22 at48DA44/45 ->48EBA0 at48DAA7.
            save: MissionSaveState::new_game(),
            inhibit_byte_08: 0,  //48DA1A.
            current_world_a0: 1, //48DA88, EAX retained from48DA3F.
            byte_a6: Some(0),    //48DA95.
            dword_38: None,
            record_0c: Some(Box::new([0; RECORD_0C_BYTES])), //480FC0 ->48DA3C.
            dword_1c: 0x2800,                                //48DA66.
            byte_13: Some(1),                                //48DA54.
            word_10: None,
            byte_12: Some(0),          //48DA51.
            bytes_14_16: Some([0; 3]), //48DA57/5A/5D.
            byte_17: Some(1),          //48DA60, AL of 48DA54.
            byte_18: Some(0),          //48DA63.
            dword_20: Some(0x400),     //48DA6D; hexadecimal.
            dword_24: Some(0),         //48DA74.
            word_28: Some(0),          //48DA77.
            word_2a: None,
            dword_2c: Some(0), //48DA7F.
            terrain_event_5483b0: false,
        }
    }

    pub fn new(save: MissionSaveState, inputs: WorldMissionInputs) -> Self {
        Self {
            save,
            inhibit_byte_08: 0,
            current_world_a0: inputs.current_world_a0,
            byte_a6: None,
            dword_38: None,
            record_0c: inputs.record_0c,
            dword_1c: inputs.dword_1c,
            byte_13: inputs.byte_13,
            word_10: None,
            byte_12: None,
            bytes_14_16: None,
            byte_17: None,
            byte_18: None,
            dword_20: None,
            dword_24: None,
            word_28: None,
            word_2a: None,
            dword_2c: None,
            terrain_event_5483b0: false,
        }
    }

    /// 0048DC30's mission prelude, before resources and placements. Nothing
    /// is written when the previous world is out of range.
    pub fn begin_world_load(&mut self) -> Result<(), WorldMissionError> {
        let previous = self.current_world_a0;
        let marked = match self.status_byte(previous) {
            Ok(status) => status & 2 == 2,
            // 0048C5FF sets +A6 only with fresh override words, and only +A6
            // lets 0048DAF0 read them, so the bit this would set is never read.
            Err(WorldMissionError::ArenaStatus(_)) => false,
            Err(error) => return Err(error),
        };
        if marked {
            match previous {
                world if world < MISSION_WORLDS => self.save.owner_3c[world as usize] |= 4,
                26 => self.save.owner_a4 |= 4,
                29..=33 => self.save.owner_b0[previous as usize - 29] |= 4,
                // 25, W+A0 itself, never has bit 1.
                _ => unreachable!("world {previous} has no status bit 1"),
            }
        }
        self.byte_a6 = Some(0);
        self.dword_38 = Some(0);
        self.current_world_a0 = self.save.mission_selection;
        Ok(())
    }

    /// The prelude's header writes after resource activation: +14..+16 = 0,
    /// +24 = sign-extended WORD +0C, +13 = 0 for flag 0x1000, +28/+2C = 0,
    /// then +20 = min(header +8, trunc(+1C * 3 / 4)) with signed DWORDs.
    pub fn apply_world_header(&mut self, header: &WorldResource) {
        let words = &header.header_words_00_to_34;
        self.bytes_14_16 = Some([0; 3]);
        self.dword_24 = Some(i32::from(words[3] as u16 as i16) as u32);
        if header.flags & 0x1000 != 0 {
            self.byte_13 = Some(0);
        }
        self.word_28 = Some(0);
        self.dword_2c = Some(0);
        let clamp = (self.dword_1c as i32).wrapping_mul(3) / 4;
        self.dword_20 = Some((words[2] as i32).min(clamp) as u32);
    }

    /// Loader tail after player setup: +10 = 0x10000 / signed WORD +16
    /// (quotient AX), 00480FD0 clears a present +0C record, then +12 = 1.
    pub fn finish_world_load(&mut self, header: &WorldResource) -> Result<(), WorldMissionError> {
        let divisor = i32::from((header.header_words_00_to_34[5] >> 16) as u16 as i16);
        if divisor == 0 {
            return Err(WorldMissionError::ZeroTailDivisor);
        }
        self.word_10 = Some((0x1_0000 / divisor) as u16);
        if let Some(record) = self.record_0c.as_mut() {
            record.fill(0);
        }
        self.byte_12 = Some(1);
        Ok(())
    }

    /// The low byte of W+3C + 4 * world, which 0048DC30 and the briefing
    /// (0040C6A0) test. Past the 25 statuses an arena world reads W+A0 (25),
    /// the start ordinal at W+A4 (26; +A6 is zero then), the start override's
    /// words (27, 28) and the first world's pickup words at W+B0 (29..33).
    pub fn status_byte(&self, world: u32) -> Result<u8, WorldMissionError> {
        match world {
            world if world < MISSION_WORLDS => Ok(self.save.owner_3c[world as usize] as u8),
            25 => Ok(self.current_world_a0 as u8),
            26 => Ok(self.save.owner_a4 as u8),
            27 | 28 => Err(WorldMissionError::ArenaStatus(world)),
            29..=33 => Ok(self.save.owner_b0[world as usize - 29] as u8),
            _ => Err(WorldMissionError::WorldOutOfRange(world)),
        }
    }

    /// `W+B0 + 4 * index` over `W+B0..W+3CF` (the pickup words, then the
    /// completion words), which `0048E930`, `0048E980`, `0048E9C0`,
    /// `0048EA10` and `0048EA30` index by world unchecked: an arena world's
    /// words are the first worlds' completion words.
    pub fn b0_span_word_mut(&mut self, index: usize) -> Result<&mut u32, WorldMissionError> {
        let save = &mut self.save;
        match index {
            0..100 => Ok(&mut save.owner_b0[index]),
            100..200 => Ok(&mut save.owner_240[index - 100]),
            _ => Err(WorldMissionError::WorldOutOfRange(index as u32 / 4)),
        }
    }

    /// The four DWORDs 0048EA80 indexes for the current world.
    pub fn current_world_completion(&self) -> Result<[u32; 4], WorldMissionError> {
        let world = self.current_world_a0;
        if ARENA_WORLDS.contains(&world) {
            return Err(WorldMissionError::ArenaCompletion(world));
        }
        if world >= MISSION_WORLDS {
            return Err(WorldMissionError::WorldOutOfRange(world));
        }
        let start = world as usize * 4;
        Ok(self.save.owner_240[start..start + 4]
            .try_into()
            .expect("four completion words"))
    }

    /// 0048EAD0 sets the current world's placement-completion bit. Signed
    /// division truncates toward zero, so -31..-2 deliberately reach word 0;
    /// -1 alone is the null index, and lower negative indices fail the native
    /// unsigned quotient gate. The low five index bits select the DWORD bit.
    pub fn record_placement_death(
        &mut self,
        request: PlacementDeathRequest,
    ) -> Result<bool, WorldMissionError> {
        let Some((word, bit)) = self.placement_bit(request)? else {
            return Ok(false);
        };
        *word |= bit;
        Ok(true)
    }

    /// 0048EB30, 0048EAD0's twin: the same gates, then it clears the bit.
    pub fn clear_placement_record(
        &mut self,
        request: PlacementDeathRequest,
    ) -> Result<bool, WorldMissionError> {
        let Some((word, bit)) = self.placement_bit(request)? else {
            return Ok(false);
        };
        *word &= !bit;
        Ok(true)
    }

    /// `0043B987..0043BA01`: a timed course target knocked out adds to the
    /// countdown +24 its seconds in microseconds, four times that when the
    /// longer pulse (message 0xC, its +164) hit it. Unless +18 is set, a
    /// running countdown no longer than the target's own seconds keeps
    /// going and gains half the new time (signed halving, wrapping sums).
    /// Then +17 clears.
    pub fn add_target_countdown(
        &mut self,
        seconds: u8,
        quadruple: bool,
    ) -> Result<(), WorldMissionError> {
        let unit = u32::from(seconds).wrapping_mul(1_000_000) as i32;
        let mut amount = if quadruple {
            unit.wrapping_mul(4)
        } else {
            unit
        };
        let held = self
            .byte_18
            .ok_or(WorldMissionError::UnwrittenCountdown("+18"))?;
        if held == 0 {
            let current =
                self.dword_24
                    .ok_or(WorldMissionError::UnwrittenCountdown("+24"))? as i32;
            if current != 0 && current <= unit {
                amount = (amount / 2).wrapping_add(current);
            }
        }
        self.dword_24 = Some(amount as u32);
        self.byte_17 = Some(0);
        Ok(())
    }

    /// The doom countdown's writers outside the course targets: the respawn
    /// at the loss limit (`0046A8C6..0046A8D7`, 1 µs), the water bridge
    /// (`00409DFD..00409E1F`) and the Seismic Stabiliser (`00476520`) store
    /// G+1D4 (+24) and set G+1C7 (+17).
    pub fn start_doom_countdown(&mut self, micros: u32) {
        self.dword_24 = Some(micros);
        self.byte_17 = Some(1);
    }

    /// `0048E33E..0048E51A`, the mission tick's countdown +24. `unit` is
    /// whether the first player's unit (`0046AE60`) exists, which native
    /// looks up only while +24 runs. A countdown past the signed delta counts
    /// down by it; with +17 set, a player unit and under 5 s left, the
    /// explosions follow. Otherwise it ends at 0, and with +17 set the end
    /// takes two stages: +18 set and 3 s more (`0048E4FF..0048E51A`), then +18
    /// clear, +15 set and the world's end (`0048E4DC..0048E4F8`).
    pub fn tick_countdown(
        &mut self,
        delta: u32,
        unit: bool,
    ) -> Result<MissionCountdownTick, WorldMissionError> {
        let countdown = self
            .dword_24
            .ok_or(WorldMissionError::UnwrittenCountdown("+24"))?;
        if countdown == 0 {
            return Ok(MissionCountdownTick::Idle);
        }
        let doom = self
            .byte_17
            .ok_or(WorldMissionError::UnwrittenCountdown("+17"))?
            != 0;
        if countdown as i32 > delta as i32 {
            let left = countdown.wrapping_sub(delta);
            self.dword_24 = Some(left);
            return Ok(if doom && (left as i32) < 5_000_000 && unit {
                MissionCountdownTick::Explosions
            } else {
                MissionCountdownTick::Counted
            });
        }
        if !doom {
            self.dword_24 = Some(0);
            return Ok(MissionCountdownTick::Counted);
        }
        let second = self
            .byte_18
            .ok_or(WorldMissionError::UnwrittenCountdown("+18"))?
            != 0;
        if second {
            let mut bytes = self
                .bytes_14_16
                .ok_or(WorldMissionError::UnwrittenCountdown("+14..+16"))?;
            bytes[1] = 1;
            self.dword_24 = Some(0);
            self.byte_18 = Some(0);
            self.bytes_14_16 = Some(bytes);
            return Ok(MissionCountdownTick::MissionFailed {
                world: self.current_world_a0,
            });
        }
        self.dword_24 = Some(0);
        self.byte_18 = Some(1);
        if unit {
            // 0048E505..0048E515 retire the unit between the +18 and the +24
            // writes; the caller does, then arms the 3 s.
            return Ok(MissionCountdownTick::UnitEnd);
        }
        self.arm_doom_last_seconds();
        Ok(MissionCountdownTick::LastSeconds)
    }

    /// `0048E51A`: the end's first stage gives 3 s more.
    pub fn arm_doom_last_seconds(&mut self) {
        self.dword_24 = Some(DOOM_LAST_MICROS);
    }

    /// `0048BDE7..0048BE0D`: a gate that changed sides marks its placement
    /// (`0048EAD0(+78, 0)`) or clears it (`0048EB30(+78, 0)`).
    pub fn apply_gate_record(
        &mut self,
        record: crate::object_runtime::GateRecord,
    ) -> Result<bool, WorldMissionError> {
        use crate::object_runtime::GateRecord;
        let (index, mark) = match record {
            GateRecord::Mark(index) => (index, true),
            GateRecord::Clear(index) => (index, false),
        };
        let request = PlacementDeathRequest {
            index: i32::from(index),
            inhibit_byte_08: self.inhibit_byte_08,
            force: 0,
        };
        if mark {
            self.record_placement_death(request)
        } else {
            self.clear_placement_record(request)
        }
    }

    /// The completion DWORD and bit 0048EAD0 and 0048EB30 address, or none
    /// when their gates turn the request away.
    fn placement_bit(
        &mut self,
        request: PlacementDeathRequest,
    ) -> Result<Option<(&mut u32, u32)>, WorldMissionError> {
        if request.index == -1 || (request.inhibit_byte_08 != 0 && request.force == 0) {
            return Ok(None);
        }
        let word = (request.index / 32) as u32;
        if word >= 25 || word >= 4 {
            return Ok(None);
        }
        let world = self.current_world_a0;
        if ARENA_WORLDS.contains(&world) {
            return Err(WorldMissionError::ArenaCompletion(world));
        }
        if world >= MISSION_WORLDS {
            return Err(WorldMissionError::WorldOutOfRange(world));
        }
        Ok(Some((
            &mut self.save.owner_240[world as usize * 4 + word as usize],
            1_u32.wrapping_shl(request.index as u32 & 31),
        )))
    }
}

#[cfg(test)]
#[path = "world_mission_tests.rs"]
mod tests;
