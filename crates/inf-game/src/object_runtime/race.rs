//! The races (Jet Race 1 and 2 and Buggy Race, worlds 28 and 29, mode 3),
//! three rotating visuals (`0047A490`):
//!
//! - Selector 70, the race (`0046E770`, table `00527390`, kept in
//!   `0054ED70` by its payload `0046E830` while none is): seventeen racer
//!   records of `0x24` bytes from `+70`, one a slot, the start `+2D4`, the
//!   sixteen grid places `+2D6` (`0046F300`), the laps to run `+338`.
//!   Its `+24` (`0046E880`) times the racers, passes them through the gates
//!   and ranks them on the host, holds this machine's vehicle on its grid
//!   place until it races, and puts it back on the course when it misses a
//!   gate or asks to be (`+34C`).
//! - Selector 71 (`0046F7F0`, `005273DC`): the first player's vehicle
//!   crossing it against its facing is told it goes the wrong way (`+19C`
//!   on its player, `0046D200`).
//! - Selector 72, a gate (`0046F9E0`, `00527428`), in the gate list
//!   `0054ED58` (newest first): its number `+6C`, the next gate's `+70`,
//!   its half width `+74` and its depth `+78`. Its draw is `0046FB50`.
//! - Selector 92, Powerball's goal (`00403EC0`, `0051F2B0`: selector 71's
//!   table but for its commands `00404420`, payload `00403EF0` and update
//!   `00403F40`): its side `+6C`, the points a goal gives `+70` and the
//!   ball's marks `+74`/`+75`. It scores the ball
//!   ([selector 91](actor/selector91.rs)).
//!
//! Message 42 (`0046F410`, `0046F5E0`) carries the host's records.

use inf_formats::native_basis::{isqrt_48efc0, sine_q15, NativeBasis};
use inf_formats::{
    FlagsWordsConstructorPayload, GoalConstructorPayload, InstanceCommandPayload,
    PointTableConstructorPayload,
};
use thiserror::Error;

use super::{ActorHandle, ObjectArena, ObjectClass, ObjectError, ObjectId, RotatingVisual};
use crate::network::game::GameNetState;

/// The racer records, one a slot (0 is never a player's).
pub const RACERS: usize = 17;
/// `0046E8A8`: the times count 50 ms steps.
const STEP_MICROS: i32 = 50_000;
/// `0046E9D8`: a racer's distance to its next gate before any is seen.
const UNSEEN: i32 = 0x7fff;
/// `0046F3B4`: a start's countdown, about three seconds of 256 µs.
pub const COUNTDOWN: u16 = 0x2dc6;
/// `+2D4` while no race runs or counts down.
pub const IDLE: u16 = 0xffff;
/// `0046E95F`: under this the other racers are let go.
const LET_GO: u16 = 0x186;
/// `0046F3A4`: the host's restart key waits a second.
const RESTART_WAIT: i32 = 1_000_000;
/// `0046F07C`: a missed gate puts the vehicle back after 1.75 s.
const MISSED_LIMIT: i32 = 0x1a_b3f0;
/// `0046D200`: the wrong way shows for a second.
const WRONG_WAY_MICROS: i32 = 1_000_000;
/// `00520A40`: the speed along its heading a vehicle is put back with.
const RESPAWN_SPEED: f64 = 128.0;
/// `0046F300`: the grid table's units.
const GRID_SCALE: u16 = 220;

/// A racer's record, `+70 + 0x24 * slot`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RacerRecord {
    /// +00: the grid place it starts from.
    pub grid_00: u8,
    /// +01: its place, from 0 (signed byte).
    pub rank_01: u8,
    /// +02, +04 and +06: its best lap, this lap and its race, in steps.
    pub best_02: u16,
    pub lap_04: u16,
    pub total_06: u16,
    /// +08: its vehicle.
    pub handle_08: u32,
    /// +0C: the last gate it passed; +10 the one it must pass next.
    pub gate_0c: u32,
    pub next_10: u32,
    /// +14: its distance to the next gate.
    pub distance_14: i32,
    /// +18: its laps; finishing adds 17 less the finishers before it.
    pub laps_18: u32,
    /// +1C: what ranks it.
    pub key_1c: i32,
    /// +20: bit 0 racing, bit 1 through the wrong gate.
    pub flags_20: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceState {
    /// +6C: the time short of the next 50 ms step.
    pub steps_6c: i32,
    pub records: [RacerRecord; RACERS],
    /// +2D4: [`IDLE`], the countdown, or 0 while the race runs.
    pub countdown_2d4: u16,
    /// +2D6: the grid places, turned and placed (`0046F300`).
    pub grid_2d6: [[u16; 3]; 16],
    /// +338: the laps to run.
    pub laps_338: u32,
    /// +33C: the racers that finished.
    pub finished_33c: u32,
    /// +340..+348: the grid's sway angles, which `0046E770` leaves to the
    /// heap.
    pub sway_340: Option<[u32; 3]>,
    /// +34C: this machine's player asked to be put back (`0046A54F`).
    pub restart_34c: u8,
    /// +354: the host's restart key's wait.
    pub wait_354: i32,
}

impl RaceState {
    /// `0046E770`: `+6C` 0, idle, two laps, the records cleared.
    fn new() -> Self {
        Self {
            steps_6c: 0,
            records: [RacerRecord::default(); RACERS],
            countdown_2d4: IDLE,
            grid_2d6: [[0; 3]; 16],
            laps_338: 2,
            finished_33c: 0,
            sway_340: None,
            restart_34c: 0,
            wait_354: 0,
        }
    }

    /// `0046F3F0`, the host's restart while the race runs: no one races.
    pub fn stop(&mut self) {
        if self.countdown_2d4 == 0 {
            for record in &mut self.records[1..] {
                record.flags_20 = 0;
            }
        }
    }

    /// `0046F390`, then the host's restart key, at most once a second: an
    /// idle race counts down, a race no one runs any more goes idle.
    pub fn restart(&mut self) {
        if self.wait_354 != 0 {
            return;
        }
        self.wait_354 = RESTART_WAIT;
        if self.countdown_2d4 == IDLE {
            self.countdown_2d4 = COUNTDOWN;
        } else if self.countdown_2d4 == 0
            && self.records[1..]
                .iter()
                .all(|record| record.flags_20 & 1 == 0)
        {
            self.countdown_2d4 = IDLE;
        }
    }

    /// `0046F760`: a vehicle at lifecycle `lifecycle` claims its slot's
    /// record, unless the race runs and another vehicle holds it.
    pub fn claim(&mut self, handle: u32, lifecycle: u32, null: u32) -> Result<(), RaceError> {
        let record = self
            .records
            .get_mut((handle >> 27) as usize)
            .ok_or(RaceError::Slot(handle))?;
        if (lifecycle as i32) < 3 {
            record.flags_20 &= !1;
        } else if record.handle_08 != handle {
            if self.countdown_2d4 != IDLE && record.handle_08 != null {
                record.flags_20 &= !1;
            } else {
                record.handle_08 = handle;
            }
        }
        Ok(())
    }
}

/// Selector 71: `+6C` and `+70`, the half extents along its Z and X.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrongWayState {
    pub half_z_6c: i32,
    pub half_x_70: i32,
}

/// Selector 72, a gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateState {
    /// +6C: its number (1 is the line).
    pub number_6c: u32,
    /// +70: the gate after it.
    pub next_70: u32,
    /// +74: its half width (and its radius `+28`).
    pub half_width_74: i32,
    /// +78: its depth along its facing.
    pub depth_78: i32,
}

/// Selector 92, Powerball's goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalState {
    /// +6C: the side whose goal count it raises (1 or 2; 0 unset).
    pub side_6c: i32,
    /// +70: the points a goal gives (1 unless a command 3 says).
    pub points_70: u32,
    /// +74, +75: the ball seen in front of its plane, and behind it.
    pub crossed_74: u8,
    pub crossed_75: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RacePart {
    Race(Box<RaceState>),
    WrongWay(WrongWayState),
    Gate(GateState),
    Goal(GoalState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceObject {
    pub rotating: RotatingVisual,
    pub part: RacePart,
}

impl RaceObject {
    pub(super) fn new(selector: u32) -> Self {
        Self {
            rotating: RotatingVisual::new(),
            part: match selector {
                70 => RacePart::Race(Box::new(RaceState::new())),
                // 0046F7F0.
                71 => RacePart::WrongWay(WrongWayState {
                    half_z_6c: 0x200,
                    half_x_70: 0x80,
                }),
                // 00403EC0.
                92 => RacePart::Goal(GoalState {
                    side_6c: 0,
                    points_70: 1,
                    crossed_74: 0,
                    crossed_75: 0,
                }),
                // 0046F9E0.
                _ => RacePart::Gate(GateState::default()),
            },
        }
    }
}

/// What the race's update reads besides the arena.
pub struct RaceRequest<'a> {
    /// The update's microseconds.
    pub delta: u32,
    /// `0055F2F4` and `0055F3A0`.
    pub local_slot: u8,
    pub host: bool,
    /// `0053DF00`, for the grid's sway.
    pub sine: &'a [u16; 4096],
    /// `0054E894`.
    pub null: u32,
}

#[derive(Debug, Error)]
pub enum RaceError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("race {0:?} runs outside a network game")]
    NoNetworkGame(ObjectId),
    #[error("race {0:?}: the actor registry: {1}")]
    Registry(ObjectId, String),
    #[error("race {0:?}'s vehicle {1:?} has no maximum health +A2")]
    MaximumHealth(ObjectId, ObjectId),
    #[error("gate {0:?}'s half width +74 is 0, which 0046FC82 divides by")]
    ZeroWidth(ObjectId),
    #[error("message 42 reached a world without a race (0046F5E0 writes through 0054ED70)")]
    NoRace,
    #[error("message 42's payload ends before its records (0046F5E0 reads on)")]
    ShortPayload,
    #[error("vehicle handle {0:#010x} names a slot past the records (0046F760 writes on)")]
    Slot(u32),
}

/// `00512521`, MSVC's `qsort` (shortsort below nine), over `items`.
pub fn native_qsort<T: Copy>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> i32) {
    if items.len() < 2 {
        return;
    }
    let mut stack = Vec::new();
    let (mut lo, mut hi) = (0usize, items.len() - 1);
    loop {
        let size = hi - lo + 1;
        if size <= 8 {
            // 00512675: the largest to the end, repeatedly.
            let mut end = hi;
            while end > lo {
                let mut max = lo;
                for p in lo + 1..=end {
                    if compare(&items[p], &items[max]) > 0 {
                        max = p;
                    }
                }
                items.swap(max, end);
                end -= 1;
            }
        } else {
            items.swap(lo + size / 2, lo);
            let (mut loguy, mut higuy) = (lo, hi + 1);
            loop {
                loop {
                    loguy += 1;
                    if loguy > hi || compare(&items[loguy], &items[lo]) > 0 {
                        break;
                    }
                }
                loop {
                    higuy -= 1;
                    if higuy <= lo || compare(&items[higuy], &items[lo]) < 0 {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                items.swap(loguy, higuy);
            }
            items.swap(lo, higuy);
            if higuy as isize - 1 - lo as isize >= hi as isize - loguy as isize {
                if lo + 1 < higuy {
                    stack.push((lo, higuy - 1));
                }
                if loguy < hi {
                    lo = loguy;
                    continue;
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo + 1 < higuy {
                    hi = higuy - 1;
                    continue;
                }
            }
        }
        match stack.pop() {
            Some((next_lo, next_hi)) => (lo, hi) = (next_lo, next_hi),
            None => return,
        }
    }
}

/// `0048EFC0` of a squared length, a sign bit quartering it first.
fn length(delta: [i16; 3]) -> i32 {
    let squared = delta.iter().fold(0_i32, |sum, &word| {
        sum.wrapping_add(i32::from(word).wrapping_mul(i32::from(word)))
    });
    if squared as u32 & 0x8000_0000 != 0 {
        (isqrt_48efc0((squared as u32 >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(squared) as i32
    }
}

/// `__ftol` of a scaled basis component, the low WORD kept.
fn scaled(scale: f64, word: u32) -> u16 {
    let value = (scale * f64::from(f32::from_bits(word))).trunc();
    if value.is_finite() && (-9.2e18..9.2e18).contains(&value) {
        value as i64 as u16
    } else {
        0
    }
}

impl ObjectArena {
    pub fn race_object(&self, object: ObjectId) -> Result<&RaceObject, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Race(race) => Ok(race),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    pub(super) fn race_object_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut RaceObject, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Race(race) => Ok(race),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    pub fn race_state(&self, object: ObjectId) -> Result<&RaceState, ObjectError> {
        match &self.race_object(object)?.part {
            RacePart::Race(state) => Ok(state),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    pub fn race_state_mut(&mut self, object: ObjectId) -> Result<&mut RaceState, ObjectError> {
        match &mut self.race_object_mut(object)?.part {
            RacePart::Race(state) => Ok(state),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    pub fn gate_state(&self, object: ObjectId) -> Result<&GateState, ObjectError> {
        match &self.race_object(object)?.part {
            RacePart::Gate(state) => Ok(state),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    /// `0054ED70`: the race, while one is built.
    pub fn race(&self) -> Option<ObjectId> {
        self.race
    }

    /// `0054ED58`, the gates, newest first. Its static initializer
    /// `0046F9B0` sets the list up empty, as each world's starts here.
    pub fn race_gates(&self) -> &[ObjectId] {
        &self.race_gates
    }

    /// `0054ED74`, the microseconds the race has run (the gates' posts).
    pub fn race_clock(&self) -> u32 {
        self.race_clock
    }

    /// `0046E830`: the laps to run (+4) and the grid (`0046F300`): each
    /// record numbered its slot less one, each place 220 times its WORDs
    /// at `[first, 0, second]`, turned by the race (`0047A6C0`) and placed
    /// at it. The first race built is the race.
    pub(super) fn initialize_race_payload(
        &mut self,
        object: ObjectId,
        payload: &PointTableConstructorPayload,
    ) -> Result<(), ObjectError> {
        let position = self.position(object)?;
        let race = self.race_object_mut(object)?;
        let basis = race.rotating.basis;
        let RacePart::Race(state) = &mut race.part else {
            return Err(ObjectError::NotRaceObject(object));
        };
        state.laps_338 = payload.word_04.value;
        for (slot, record) in state.records[1..].iter_mut().enumerate() {
            record.grid_00 = slot as u8;
        }
        for (place, point) in state.grid_2d6.iter_mut().zip(&payload.points) {
            let [first, second] = point.value.map(|word| word.wrapping_mul(GRID_SCALE));
            let turned = basis.transform_words([first as i16, 0, second as i16]);
            *place = std::array::from_fn(|axis| turned[axis].wrapping_add(position[axis]));
        }
        if self.race.is_none() {
            self.race = Some(object);
        }
        Ok(())
    }

    /// `0046F890` and `0046FB20` after the flags: selector 71's half
    /// extents `+70` and `+6C` (halved), or the gate's half width `+74`
    /// (halved, also its radius `+28`) and depth `+78`.
    pub(super) fn initialize_race_marker_payload(
        &mut self,
        object: ObjectId,
        payload: &FlagsWordsConstructorPayload,
    ) -> Result<(), ObjectError> {
        let [first, second] = payload.words.map(|field| i32::from(field.value));
        match &mut self.race_object_mut(object)?.part {
            RacePart::WrongWay(state) => {
                state.half_x_70 = first / 2;
                state.half_z_6c = second / 2;
            }
            RacePart::Gate(state) => {
                state.half_width_74 = first / 2;
                state.depth_78 = second;
                self.owned_mut(object)?.state.base.word_28 = (first / 2) as u16;
            }
            RacePart::Race(_) | RacePart::Goal(_) => {
                return Err(ObjectError::NotRaceObject(object))
            }
        }
        Ok(())
    }

    /// `00403F10` after the flags: the side, when 1 or 2.
    pub(super) fn initialize_goal_payload(
        &mut self,
        object: ObjectId,
        payload: &GoalConstructorPayload,
    ) -> Result<(), ObjectError> {
        let side = payload.side.value;
        if (1..=2).contains(&side) {
            self.goal_state_mut(object)?.side_6c = side;
        }
        Ok(())
    }

    pub fn goal_state(&self, object: ObjectId) -> Result<&GoalState, ObjectError> {
        match &self.race_object(object)?.part {
            RacePart::Goal(state) => Ok(state),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    pub(super) fn goal_state_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut GoalState, ObjectError> {
        match &mut self.race_object_mut(object)?.part {
            RacePart::Goal(state) => Ok(state),
            _ => Err(ObjectError::NotRaceObject(object)),
        }
    }

    /// `0046F810` codes 3 and 4 (selector 71), `0046FA70` codes 3 to 6 (a
    /// gate), `00404420` code 3 (a goal's points).
    pub(super) fn apply_race_command(
        &mut self,
        object: ObjectId,
        code: i32,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let InstanceCommandPayload::RaceDword(value) = payload else {
            return Ok(false);
        };
        let value = value.value as i32;
        match (&mut self.race_object_mut(object)?.part, code) {
            (RacePart::WrongWay(state), 3) => state.half_x_70 = value / 2,
            (RacePart::WrongWay(state), 4) => state.half_z_6c = value / 2,
            (RacePart::Gate(state), 3) => state.number_6c = value as u32,
            (RacePart::Gate(state), 4) => state.next_70 = value as u32,
            (RacePart::Gate(state), 5) => {
                state.half_width_74 = value / 2;
                self.owned_mut(object)?.state.base.word_28 = (value / 2) as u16;
            }
            (RacePart::Gate(state), 6) => state.depth_78 = value,
            (RacePart::Goal(state), 3) => state.points_70 = value as u32,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The race's `+24`, `0046E880`.
    pub fn tick_race(
        &mut self,
        race: ObjectId,
        request: &RaceRequest<'_>,
        rng: &mut u32,
    ) -> Result<(), RaceError> {
        let mut hold = self
            .take_network_game()
            .ok_or(RaceError::NoNetworkGame(race))?;
        let result = self.tick_race_with(race, request, rng, &mut hold.game);
        self.lend_network_game(hold);
        result
    }

    fn tick_race_with(
        &mut self,
        race: ObjectId,
        request: &RaceRequest<'_>,
        rng: &mut u32,
        game: &mut GameNetState,
    ) -> Result<(), RaceError> {
        let delta = request.delta;
        self.race_clock = self.race_clock.wrapping_add(delta);
        let local = usize::from(request.local_slot) % RACERS;
        let state = self.race_state_mut(race)?;
        let elapsed = state.steps_6c.wrapping_add(delta as i32);
        let steps = elapsed / STEP_MICROS;
        state.steps_6c = elapsed % STEP_MICROS;
        let shift = delta >> 8;
        if !request.host {
            if state.countdown_2d4 != IDLE {
                state.countdown_2d4 = if u32::from(state.countdown_2d4) > shift {
                    state.countdown_2d4.wrapping_sub(shift as u16)
                } else {
                    0
                };
            }
        } else {
            state.wait_354 = if state.wait_354 > delta as i32 {
                state.wait_354 - delta as i32
            } else {
                0
            };
            if state.countdown_2d4 == IDLE {
                // 0046E90A: idle, the records start afresh.
                for record in &mut state.records {
                    *record = RacerRecord {
                        grid_00: record.grid_00,
                        handle_08: record.handle_08,
                        ..RacerRecord::default()
                    };
                }
                state.finished_33c = 0;
            } else {
                if u32::from(state.countdown_2d4) > shift {
                    state.countdown_2d4 = state.countdown_2d4.wrapping_sub(shift as u16);
                    if state.countdown_2d4 < LET_GO {
                        for (slot, record) in state.records.iter_mut().enumerate().skip(1) {
                            if slot != local {
                                record.flags_20 |= 1;
                            }
                            record.laps_18 = 0;
                            record.next_10 = 1;
                        }
                    }
                } else if state.countdown_2d4 != 0 {
                    state.countdown_2d4 = 0;
                    state.records[local].flags_20 |= 1;
                }
                self.time_racers(race, steps as u16)?;
                self.rank_racers(race, game)?;
            }
        }
        let result = if self.race_state(race)?.records[local].flags_20 & 1 == 0 {
            self.hold_on_grid(race, local, request, rng)
        } else {
            self.keep_on_course(race, local, request)
        };
        self.race_state_mut(race)?.restart_34c = 0;
        result
    }

    /// `0046E9C9..0046EC08`: each racing record's vehicle timed and passed
    /// through the gates, then every record keyed.
    fn time_racers(&mut self, race: ObjectId, steps: u16) -> Result<(), RaceError> {
        let gates = self.race_gates.clone();
        let mut records = self.race_state(race)?.records;
        let (laps_to_run, mut finished) = {
            let state = self.race_state(race)?;
            (state.laps_338, state.finished_33c)
        };
        for record in &mut records {
            record.distance_14 = UNSEEN;
            if record.flags_20 & 1 != 0 {
                let vehicle = self
                    .resolve_actor_handle(ActorHandle::from_raw(record.handle_08))
                    .map_err(|error| RaceError::Registry(race, error.to_string()))?;
                match vehicle {
                    None => record.flags_20 &= !1,
                    Some(vehicle) => {
                        record.lap_04 = record.lap_04.wrapping_add(steps);
                        record.total_06 = record.total_06.wrapping_add(steps);
                        let position = self.position(vehicle)?;
                        for &gate in &gates {
                            let state = *self.gate_state(gate)?;
                            let gate_position = self.position(gate)?;
                            let delta: [i16; 3] = std::array::from_fn(|axis| {
                                position[axis].wrapping_sub(gate_position[axis]) as i16
                            });
                            let distance = length(delta);
                            let local = self.race_object(gate)?.rotating.basis.inverse_words(delta);
                            let inside = local[2] > 0
                                && i32::from(local[2]) < state.depth_78
                                && i32::from(local[0]).abs() < state.half_width_74;
                            if state.number_6c == record.next_10 {
                                if !inside {
                                    record.distance_14 = record.distance_14.min(distance);
                                    continue;
                                }
                                record.gate_0c = state.number_6c;
                                if state.number_6c == 1
                                    && record.laps_18 as i32 >= laps_to_run as i32
                                {
                                    record.flags_20 &= !1;
                                    record.laps_18 =
                                        record.laps_18.wrapping_add(17u32.wrapping_sub(finished));
                                    finished = finished.wrapping_add(1);
                                }
                                record.next_10 = state.next_70;
                                if state.number_6c == 1 {
                                    if record.laps_18 != 0 {
                                        if record.best_02 == 0 || record.best_02 > record.lap_04 {
                                            record.best_02 = record.lap_04;
                                        }
                                        record.lap_04 = 0;
                                    }
                                    record.laps_18 = record.laps_18.wrapping_add(1);
                                }
                                record.distance_14 = UNSEEN;
                                record.flags_20 &= !2;
                            } else if inside {
                                if state.next_70 == record.next_10 {
                                    record.flags_20 &= !2;
                                } else {
                                    record.flags_20 |= 2;
                                }
                            }
                        }
                    }
                }
            }
            record.key_1c = (record.laps_18 << 24)
                .wrapping_add(record.gate_0c << 16)
                .wrapping_sub(record.distance_14 as u32)
                .wrapping_add(0xffff) as i32;
        }
        let state = self.race_state_mut(race)?;
        state.records = records;
        state.finished_33c = finished;
        Ok(())
    }

    /// `0046EC0E..0046ECAE`: the records sorted by key (`0046F2D0`), and
    /// while the race runs each place handed out, the slots' scores set
    /// to 17 less their places and the grid places from the places before.
    fn rank_racers(&mut self, race: ObjectId, game: &mut GameNetState) -> Result<(), RaceError> {
        let state = self.race_state_mut(race)?;
        state.records[0].key_1c = 0x8000_0001_u32 as i32;
        let mut order: [usize; RACERS] = std::array::from_fn(|index| index);
        let keys: [i32; RACERS] = std::array::from_fn(|index| state.records[index].key_1c);
        native_qsort(&mut order, |a, b| keys[*b].wrapping_sub(keys[*a]));
        if state.countdown_2d4 != 0 {
            return Ok(());
        }
        for (place, &slot) in order.iter().enumerate() {
            let before = i32::from(state.records[slot].rank_01 as i8);
            state.records[slot].rank_01 = place as u8;
            let rank = i32::from(state.records[place].rank_01 as i8);
            game.scores[place][2] = 17_i32.wrapping_sub(rank) as u16;
            state.records[place].grid_00 = (16 - before).clamp(0, 15) as u8;
        }
        Ok(())
    }

    /// `0046ECFB..0046F00F`: this machine's live vehicle, not yet racing,
    /// held on its grid place, swaying, turned as the race, still, and
    /// healed by 5000 (to its most).
    fn hold_on_grid(
        &mut self,
        race: ObjectId,
        local: usize,
        request: &RaceRequest<'_>,
        rng: &mut u32,
    ) -> Result<(), RaceError> {
        let Some(vehicle) = self
            .first_player_primary_actor()
            .map_err(|error| RaceError::Registry(race, error.to_string()))?
        else {
            return Ok(());
        };
        if self.actor(vehicle)?.word_c4 != 3 {
            return Ok(());
        }
        let delta = request.delta;
        let state = self.race_state_mut(race)?;
        // +00 only ever holds a nibble.
        let place = state.grid_2d6[usize::from(state.records[local].grid_00 & 0xf)];
        let mut sway = state.sway_340.unwrap_or_else(|| {
            log_sway_residue();
            [0; 3]
        });
        for (angle, scale) in sway.iter_mut().zip([5_u32, 7, 9]) {
            let random = crate::name_entry::name_entry_rng_next(rng) & 0xffff;
            *angle = angle.wrapping_add(random.wrapping_mul(scale).wrapping_mul(delta) >> 16);
        }
        state.sway_340 = Some(sway);
        let basis = self.race_object(race)?.rotating.basis;
        let mut point = place;
        for (&angle, (shift, row)) in sway.iter().zip([(11, 7), (12, 4), (13, 1)]) {
            let offset = (sine_q15((angle as i32) >> 6, request.sine) >> shift) as i16;
            for axis in 0..3 {
                point[axis] =
                    point[axis].wrapping_add(scaled(f64::from(offset), basis.words[row + axis]));
            }
        }
        self.owned_mut(vehicle)?.state.base.position_words = Some(point);
        let actor = self.actor_mut(vehicle)?;
        actor.rotating.basis = basis;
        let velocity = actor.words_8e_92.map(|word| word.wrapping_neg());
        self.add_actor_linear_velocity(vehicle, velocity)?;
        let actor = self.actor_mut(vehicle)?;
        let most = actor
            .maximum_health
            .ok_or(RaceError::MaximumHealth(race, vehicle))?;
        actor.health = actor.health.wrapping_add(5000).min(most);
        Ok(())
    }

    /// `0046F012..0046F2AC`: a racing vehicle through the wrong gate for
    /// 1.75 s is put back 500 above its next gate; one asking (`+34C`) 250
    /// above its last gate (or at the race); each turned as that gate and
    /// moving along its heading at 128 (`0047DDF0`).
    fn keep_on_course(
        &mut self,
        race: ObjectId,
        local: usize,
        request: &RaceRequest<'_>,
    ) -> Result<(), RaceError> {
        let record = self.race_state(race)?.records[local];
        let player = self.player_for_unit(record.handle_08, request.null);
        let vehicle = self
            .first_player_primary_actor()
            .map_err(|error| RaceError::Registry(race, error.to_string()))?;
        let (Some(player), Some(vehicle)) = (player, vehicle) else {
            return Ok(());
        };
        // 0046D210 with "not through the wrong gate".
        let missed = &mut self.players.list[player].dword_198;
        if record.flags_20 & 2 == 0 {
            *missed = 0;
        } else if *missed == 0 {
            *missed = 1;
        }
        let (position, basis) = if *missed != 0 {
            let time = (*missed as i32).wrapping_add(request.delta as i32);
            *missed = if time <= MISSED_LIMIT { time as u32 } else { 1 };
            if time <= MISSED_LIMIT {
                return Ok(());
            }
            let Some(gate) = self.gate_numbered(record.next_10)? else {
                return Ok(());
            };
            let mut position = self.position(gate)?;
            position[1] = position[1].wrapping_add(500);
            (position, Some(self.race_object(gate)?.rotating.basis))
        } else {
            if self.race_state(race)?.restart_34c == 0 {
                return Ok(());
            }
            match self.gate_numbered(record.gate_0c)? {
                Some(gate) => {
                    let mut position = self.position(gate)?;
                    position[1] = position[1].wrapping_add(250);
                    (position, Some(self.race_object(gate)?.rotating.basis))
                }
                None => (
                    self.position(race)?,
                    Some(self.race_object(race)?.rotating.basis),
                ),
            }
        };
        let actor = self.actor_mut(vehicle)?;
        if let Some(basis) = basis {
            actor.rotating.basis = basis;
        }
        let forward = actor.rotating.basis;
        actor.words_8e_92 = [7, 8, 9].map(|index| scaled(RESPAWN_SPEED, forward.words[index]));
        self.owned_mut(vehicle)?.state.base.position_words = Some(position);
        Ok(())
    }

    /// `0046F760` on the race for `vehicle`.
    pub(crate) fn claim_race_record(
        &mut self,
        vehicle: ObjectId,
        null: ActorHandle,
    ) -> Result<(), RaceError> {
        let Some(race) = self.race else {
            return Ok(());
        };
        let actor = self.actor(vehicle)?;
        let (handle, lifecycle) = (actor.handle.raw(), actor.word_c4);
        self.race_state_mut(race)?
            .claim(handle, lifecycle, null.raw())
    }

    /// The gate list's first gate numbered `number`.
    fn gate_numbered(&self, number: u32) -> Result<Option<ObjectId>, ObjectError> {
        for &gate in &self.race_gates {
            if self.gate_state(gate)?.number_6c == number {
                return Ok(Some(gate));
            }
        }
        Ok(None)
    }

    /// `0046B090`: the player whose `+D8` (or `+D4` while `+D8` is the
    /// null) is `handle`; none for the null.
    fn player_for_unit(&self, handle: u32, null: u32) -> Option<usize> {
        if handle == null {
            return None;
        }
        self.players.list.iter().position(|player| {
            let unit = if player.handle_d8.raw() != null {
                player.handle_d8
            } else {
                player.handle_d4
            };
            unit.raw() == handle
        })
    }

    /// Selector 71's `+24`, `0046F8C0`: the first player's live vehicle
    /// inside its half extents, facing against it, shows its player the
    /// wrong way for a second.
    pub fn tick_wrong_way(&mut self, object: ObjectId, null: u32) -> Result<(), RaceError> {
        let Some(vehicle) = self
            .first_player_primary_actor()
            .map_err(|error| RaceError::Registry(object, error.to_string()))?
        else {
            return Ok(());
        };
        if self.actor(vehicle)?.word_c4 != 3 {
            return Ok(());
        }
        let own = self.position(object)?;
        let theirs = self.position(vehicle)?;
        let delta: [i16; 3] =
            std::array::from_fn(|axis| theirs[axis].wrapping_sub(own[axis]) as i16);
        let marker = self.race_object(object)?;
        let RacePart::WrongWay(state) = marker.part else {
            return Err(ObjectError::NotRaceObject(object).into());
        };
        let local = marker.rotating.basis.inverse_words(delta);
        if i32::from(local[0]).abs() >= state.half_x_70
            || i32::from(local[2]).abs() >= state.half_z_6c
        {
            return Ok(());
        }
        let facing = forward_dot(marker.rotating.basis, self.actor(vehicle)?.rotating.basis);
        if facing.is_nan() || facing < 0.0 {
            let handle = self.actor(vehicle)?.handle.raw();
            if let Some(player) = self.player_for_unit(handle, null) {
                self.players.list[player].dword_19c = WRONG_WAY_MICROS;
            }
        }
        Ok(())
    }

    /// Message 42's encoder `0046F410`: records 1 to 16's grid places
    /// and places (a byte each), their flags' low two bits (a DWORD), the
    /// start, a mask of the records with a race time, and for each of them
    /// its times, its laps and, through the wrong gate, its next gate.
    /// -1 when the room runs out.
    pub fn encode_race(&self, out: &mut [u8]) -> Result<i32, ObjectError> {
        let Some(race) = self.race else {
            return Ok(0);
        };
        Ok(encode_race_records(self.race_state(race)?, out))
    }

    /// Message 42's receiver `0046F5E0`; the bytes it used.
    pub fn receive_race(&mut self, payload: &[u8]) -> Result<i32, RaceError> {
        let race = self.race.ok_or(RaceError::NoRace)?;
        receive_race_records(self.race_state_mut(race)?, payload).ok_or(RaceError::ShortPayload)
    }
}

/// `0046FB68`: a post's widest half width on screen.
const POST_WIDTH: i32 = 0x96;
/// `0046FCD3`: the posts' tops stand 100 above the gate.
const POST_RISE: u16 = 100;
/// `0046FD88`: their feet 50 under the ground.
pub const POST_SINK: i32 = 0x32;

impl ObjectArena {
    /// `0046FC59..0046FCE5`: a gate's eight posts' tops, in draw order:
    /// four distances `t = (+74 * ring / 4 + 0054ED74 / 2048) mod +74`
    /// along its X row, each followed by `-t`, 100 above the gate.
    pub fn gate_post_tops(&self, gate: ObjectId) -> Result<Vec<[u16; 3]>, RaceError> {
        let object = self.race_object(gate)?;
        let RacePart::Gate(state) = object.part else {
            return Err(ObjectError::NotRaceObject(gate).into());
        };
        let width = state.half_width_74 as u32;
        if width == 0 {
            return Err(RaceError::ZeroWidth(gate));
        }
        let position = self.position(gate)?;
        let basis = object.rotating.basis;
        let mut tops = Vec::with_capacity(8);
        for ring in 0..4 {
            let along = ((state.half_width_74.wrapping_mul(ring) >> 2) as u32)
                .wrapping_add(self.race_clock >> 11)
                % width;
            for t in [along, along.wrapping_neg()] {
                let scale = f64::from(t as u16 as i16);
                tops.push([
                    scaled(scale, basis.words[1]).wrapping_add(position[0]),
                    scaled(scale, basis.words[2])
                        .wrapping_add(POST_RISE)
                        .wrapping_add(position[1]),
                    scaled(scale, basis.words[3]).wrapping_add(position[2]),
                ]);
            }
        }
        Ok(tops)
    }
}

/// `0046FD29..0046FD42`: a post end's half width on screen.
fn post_width(focal_x: i32, depth: i32) -> i32 {
    if depth < 1 {
        return POST_WIDTH;
    }
    (focal_x.wrapping_mul(POST_WIDTH) / depth + 1).min(POST_WIDTH)
}

/// `0046FD45..00470113` for one post, its top and foot projected: drawn
/// when the foot is deeper than 0x40, either end is on screen and the
/// screen step is longer than one; each end widened across the step by its
/// own size; far when either end carries fog.
pub fn gate_beam(
    top: super::Selector69Projected,
    foot: super::Selector69Projected,
    focal_x: i32,
) -> Result<Option<super::Selector69Quad>, super::Selector69UnwrittenFog> {
    let top_size = post_width(focal_x, top.depth);
    if foot.depth <= 0x40 {
        return Ok(None);
    }
    let foot_size = post_width(focal_x, foot.depth);
    if (top.clip | foot.clip) == 0 {
        return Ok(None);
    }
    let run = i32::from(top.xy.wrapping_sub(foot.xy) as u16 as i16);
    let rise = i32::from((foot.xy >> 16).wrapping_sub(top.xy >> 16) as u16 as i16);
    let squared = rise.wrapping_mul(rise).wrapping_add(run.wrapping_mul(run));
    if squared as u32 <= 1 {
        return Ok(None);
    }
    let (Some(top_fog), Some(foot_fog)) = (top.fog, foot.fog) else {
        return Err(super::Selector69UnwrittenFog);
    };
    let length = isqrt_48efc0(squared) as i32;
    let part = |size: i32, value: i32| (size.wrapping_mul(value) / length) as u16;
    let corner = |end: super::Selector69Projected, size: i32, plus: bool| {
        let [x, y] = [part(size, rise), part(size, run)];
        let apply = |value: u16, offset: u16| {
            if plus {
                value.wrapping_add(offset)
            } else {
                value.wrapping_sub(offset)
            }
        };
        super::RibbonCorner {
            xy: u32::from(apply(end.xy as u16, x))
                | u32::from(apply((end.xy >> 16) as u16, y)) << 16,
            depth: end.depth,
        }
    };
    Ok(Some(super::Selector69Quad {
        far: (top_fog | foot_fog) != 0,
        depth: (top.depth.wrapping_add(foot.depth) / 2) as u32,
        corners: [
            corner(top, top_size, false),
            corner(foot, foot_size, false),
            corner(foot, foot_size, true),
            corner(top, top_size, true),
        ],
        fog: [top_fog, foot_fog, foot_fog, top_fog],
    }))
}

/// `0046FB7B..0046FBF7`: the gate's sprite, H3 slot 6 seen from in front
/// (its facing against the view's at or above 0), else slot 1.
pub fn gate_sprite_slot(view: NativeBasis, gate: NativeBasis) -> i32 {
    if forward_dot(gate, view) >= 0.0 {
        6
    } else {
        1
    }
}

/// The network words the vehicle reads, None offline (`0055F448` clear).
pub fn vehicle_network(
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
) -> Option<super::VehicleNetwork> {
    (process.network_flag != 0).then(|| super::VehicleNetwork {
        mode: process.settings[0x10 / 4],
        catchup: process.settings[0x4c / 4] as i32,
        local_slot: process.local_slot,
    })
}

/// `(Z product + Y product) + X product` of two bases' forward rows.
fn forward_dot(a: NativeBasis, b: NativeBasis) -> f64 {
    let row = |basis: NativeBasis, index: usize| f64::from(f32::from_bits(basis.words[index]));
    (row(b, 9) * row(a, 9) + row(b, 8) * row(a, 8)) + row(b, 7) * row(a, 7)
}

fn log_sway_residue() {
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        eprintln!(
            "RaceSwayResidueApproximation: 0046E770 leaves the grid's sway angles +340..+348 \
             to the heap; the port starts them at zero (NETWORK_RUNTIME.md)"
        )
    });
}

/// `0046F410` over the race's records, its room checks as they are: the
/// laps byte wants four bytes of room, as does the next gate's.
pub fn encode_race_records(state: &RaceState, out: &mut [u8]) -> i32 {
    let records = &state.records[1..];
    let mut at = 0;
    let room = out.len();
    let fits = |at: usize, wanted: usize| room - at >= wanted;
    for record in records {
        if !fits(at, 1) {
            return -1;
        }
        out[at] = record.grid_00 & 0xf | record.rank_01 << 4;
        at += 1;
    }
    let flags = records
        .iter()
        .enumerate()
        .fold(0_u32, |flags, (index, record)| {
            flags | u32::from(record.flags_20 & 3) << (2 * index)
        });
    if !fits(at, 4) {
        return -1;
    }
    out[at..at + 4].copy_from_slice(&flags.to_le_bytes());
    at += 4;
    if !fits(at, 2) {
        return -1;
    }
    out[at..at + 2].copy_from_slice(&state.countdown_2d4.to_le_bytes());
    at += 2;
    let mask = records
        .iter()
        .enumerate()
        .fold(0_u16, |mask, (index, record)| {
            mask | u16::from(record.total_06 != 0) << index
        });
    if !fits(at, 2) {
        return -1;
    }
    out[at..at + 2].copy_from_slice(&mask.to_le_bytes());
    at += 2;
    for record in records.iter().filter(|record| record.total_06 != 0) {
        if !fits(at, 6) {
            return -1;
        }
        out[at..at + 2].copy_from_slice(&record.best_02.to_le_bytes());
        out[at + 2..at + 4].copy_from_slice(&record.lap_04.to_le_bytes());
        out[at + 4..at + 6].copy_from_slice(&record.total_06.to_le_bytes());
        at += 6;
        if !fits(at, 4) {
            return -1;
        }
        out[at] = record.laps_18 as u8;
        at += 1;
        if record.flags_20 & 2 != 0 {
            if !fits(at, 4) {
                return -1;
            }
            out[at] = record.next_10 as u8;
            at += 1;
        }
    }
    at as i32
}

/// `0046F5E0` into the race's records; None when the payload is short.
pub fn receive_race_records(state: &mut RaceState, payload: &[u8]) -> Option<i32> {
    let head = payload.get(..24)?;
    let flags = u32::from_le_bytes(head[16..20].try_into().ok()?);
    let mask = u16::from_le_bytes(head[22..24].try_into().ok()?);
    for (index, record) in state.records[1..].iter_mut().enumerate() {
        record.grid_00 = head[index] & 0xf;
        record.rank_01 = head[index] >> 4;
        record.flags_20 = (flags >> (2 * index)) as u8 & 3;
    }
    state.countdown_2d4 = u16::from_le_bytes(head[20..22].try_into().ok()?);
    let mut at = 24;
    for (index, record) in state.records[1..].iter_mut().enumerate() {
        if mask & 1 << index == 0 {
            record.best_02 = 0;
            record.lap_04 = 0;
            record.total_06 = 0;
            record.laps_18 = 0;
            continue;
        }
        let bytes = payload.get(at..at + 7)?;
        record.best_02 = u16::from_le_bytes([bytes[0], bytes[1]]);
        record.lap_04 = u16::from_le_bytes([bytes[2], bytes[3]]);
        record.total_06 = u16::from_le_bytes([bytes[4], bytes[5]]);
        record.laps_18 = u32::from(bytes[6]);
        at += 7;
        if record.flags_20 & 2 != 0 {
            record.next_10 = u32::from(*payload.get(at)?);
            at += 1;
        }
    }
    Some(at as i32)
}

#[cfg(test)]
#[path = "race_tests.rs"]
mod tests;
