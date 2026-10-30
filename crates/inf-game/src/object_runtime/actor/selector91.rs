//! Selector 91, Powerball's ball: constructor 004027E0 over the Boulders'
//! 00405780 (case 0x5B, 0x184 bytes), final table 0051F1E0. The table is
//! the boulder's 0051F900 but for its destructor (00402880), +0C (004038E0),
//! +14 (004045C0), its update +24 (00403300), its light +28 (00404450), its
//! draw +2C (00403D30), its remote update +3C (00402DD0), +40 (00403CF0),
//! +A0 (00403630), its pair reaction +A4 (00403660), its sync +C0/+C4
//! (00403AE0, 00403BC0) and +CC (00404610). Its commands, +1C and terrain
//! contact are the boulder's.
//!
//! The ball keeps a state +148: free (0) or loose (1), when it rolls as a
//! boulder and a vehicle just ahead of it takes it; held (2, turned by the
//! angles it was taken at, or 3, ahead along its holder's basis), when it
//! rides its holder +14C and drags it; scored (4), waiting at home 500
//! above the ground until play resumes; 5 is only ever received. A vehicle
//! it touches takes a free ball, and drops a held one. Off on high ground
//! (at or above the strip's highest +170) it resets at home. The goals
//! (selector 92) score it on the host, whose sync carries the state, the
//! sequence +174 and the two sides' goal counts 0052E528/0052E529 (bit 7 a
//! won side) to the other machines; they say the goals.

use thiserror::Error;

use super::{native_trunc_low32, ActorExtension, FamilyUpdateHost};
use crate::network::protocol::GameSend;
use crate::object_runtime::{
    mark_flag_change, ActorHandle, ObjectArena, ObjectError, ObjectId, Selector59Error,
};
use crate::terrain_runtime::TerrainRuntime;
use inf_formats::native_basis::{angle_48f540, isqrt_48efc0, InverseSineQ31Error};

/// `00402C6B`: the time (ms) a dropped or reset ball cannot be taken.
const LOCK: i16 = 0x15e;
/// `004034E2`: a loose ball resets after this many ms.
const LOOSE_LIMIT: i32 = 30_000;
/// `00402D08`: a held ball's carry time, renewed every update.
const CARRY: i32 = 1_000_000;
/// `00402A9E`: a scored ball waits this far above home.
const SCORED_HEIGHT: u16 = 500;
/// `00402B20`: a reset ball's upward speed.
const RESET_RISE: u16 = 0x5dc;
/// `004035E0`, `004035F7`: the ball's model by the host's team flag.
const MODELS: [u16; 2] = [0x271, 0x2be];
/// `004029A1`: a vehicle within 500 can take the ball.
const TAKE_REACH_SQUARED: i32 = 250_000;
/// The ball's sounds (00487F10 with both values 0x10000): play resuming at
/// home, a side ahead or behind, and the goals.
pub const SOUND_KICK_OFF: u32 = 0x33;
pub const SOUND_GOAL: u32 = 0x34;
pub const SOUND_DOUBLE_GOAL: u32 = 0x35;
/// The goal bytes' bit for a side that has won, and their idle value.
pub const GOAL_WON: u8 = 0x80;

/// The ball's fields past the boulder's (+148..+182).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BallState {
    /// +148: 0 free, 1 loose, 2 and 3 held, 4 scored, 5 only received.
    pub state_148: u32,
    /// +14C, its holder; +150, the last holder; +154, the scorer.
    pub holder_14c: ActorHandle,
    pub last_150: ActorHandle,
    pub scorer_154: ActorHandle,
    /// +158: milliseconds loose.
    pub loose_158: i32,
    /// +15C, +15E: the angles it was taken at. The constructor leaves them
    /// unwritten, and a received state other than 2 fills them from stack
    /// residue (the sync's pointers), so they are known only while held.
    pub angles_15c: [Option<i16>; 2],
    /// +160..+164: home, where the payload put it.
    pub home_160: Option<[u16; 3]>,
    /// +168: the light's phase.
    pub light_168: u32,
    /// +16C: microseconds it still follows its holder.
    pub carry_16c: i32,
    /// +170: the highest ground along its strip; 0x10000 until found.
    pub threshold_170: i32,
    /// +174, the state's sequence; +17C, the sequence a throw or drop was
    /// made in, which another machine's ball repeats until the host's sync
    /// moves on.
    pub sequence_174: u16,
    pub throw_176: [i16; 3],
    pub throw_17c: u16,
    /// +17E: milliseconds it cannot be taken.
    pub lock_17e: i16,
    /// +180: milliseconds a scored ball waits.
    pub wait_180: i16,
}

impl BallState {
    /// `004027E0`'s writes.
    pub(in crate::object_runtime) fn new(null: ActorHandle) -> Self {
        Self {
            state_148: 0,
            holder_14c: null,
            last_150: null,
            scorer_154: null,
            loose_158: 0,
            angles_15c: [None; 2],
            home_160: None,
            light_168: 0,
            carry_16c: 0,
            threshold_170: 0x10000,
            sequence_174: 0,
            throw_176: [0; 3],
            throw_17c: 0,
            lock_17e: 0,
            wait_180: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum BallError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("ball {0:?} reads its unwritten {1}")]
    Unwritten(ObjectId, &'static str),
    #[error(transparent)]
    Angle(#[from] InverseSineQ31Error),
    #[error(transparent)]
    Boulder(Selector59Error<E>),
    #[error("ball {ball:?} service at {address:08X}: {error}")]
    Host {
        ball: ObjectId,
        address: u32,
        error: E,
    },
    #[error(
        "a short ball sync (no bit 7) reads stack residue (addresses) into +14C, +15C and +174"
    )]
    ShortSync,
    #[error("ball sync ends before its offset {0}")]
    SyncShort(usize),
    #[error("goal {0:?} has no side (+6C is {1}): 00403F40 adds to the bytes before 0052E528")]
    GoalSide(ObjectId, i32),
    #[error("goal {goal:?}'s scorer is in slot {slot}, past 0054E798's table")]
    ScorerSlot { goal: ObjectId, slot: usize },
    #[error("a Powerball line: {0}")]
    Format(String),
    #[error("Powerball's restart (0046A644) clears the network game's scores 0054E798, which no game holds")]
    NoScores,
}

/// The H0 pool's handler `0055EE50`, whose loader answers each line's text.
const H0_POOL: u32 = 0x0055_ee50;

/// What the ball reaches beyond the boulder's update.
pub trait BallHost: FamilyUpdateHost {
    /// `00487F10(sound, position, 0x10000, 0x10000)`, then 004923C0.
    fn ball_sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), Self::Error>;
    /// The H12 terrain the ball's checks read (0055EE3C).
    fn ball_terrain(&self) -> &TerrainRuntime;
    /// An H0 string to its NUL, None past the pool.
    fn ball_text(&self, index: u16) -> Result<Option<Vec<u8>>, Self::Error>;
    /// The resident models the ball's model refresh (0047A110) reads.
    fn ball_runtime(&self) -> &crate::ResidentRuntime;
    /// `004135D0(text, 0)` on the first player's message (00488AA0, +88).
    fn ball_line(&mut self, arena: &mut ObjectArena, text: Vec<u8>) -> Result<(), Self::Error>;
}

/// The world's sine quarter, by the turn's top 14 bits (0053DF00).
fn sine(angle: i32, table: &[u16; 4096]) -> i32 {
    let low = angle & 0x3ffc;
    let index = if angle & 0x4000 != 0 {
        0x3ffc - low
    } else {
        low
    } >> 2;
    let value = i32::from(table[index as usize]);
    if angle & 0x8000 != 0 {
        -value
    } else {
        value
    }
}

/// The H12 cell under a point, in world units, as a WORD.
pub(crate) fn cell_height(terrain: &TerrainRuntime, [x, z]: [u16; 2]) -> i32 {
    let sampling = terrain.sampling();
    let mask = !((sampling.spacing.units() - 1) >> 7);
    let cell = terrain.cell_at_grid([(x >> 7) & mask, (z >> 7) & mask]);
    i32::from(i16::from(cell.height_i8()) << sampling.height_scale.shift())
}

/// `0048EFC0` over a squared length, halving through a set sign bit first.
fn length(square: i32) -> i32 {
    if square < 0 {
        (isqrt_48efc0(((square as u32) >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(square) as i32
    }
}

impl ObjectArena {
    pub fn ball(&self, object: ObjectId) -> Result<&BallState, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector59(state) => state
                .ball_148
                .as_deref()
                .ok_or(ObjectError::NotBall(object)),
            _ => Err(ObjectError::NotBall(object)),
        }
    }

    pub(in crate::object_runtime) fn ball_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut BallState, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector59(state) => state
                .ball_148
                .as_deref_mut()
                .ok_or(ObjectError::NotBall(object)),
            _ => Err(ObjectError::NotBall(object)),
        }
    }

    pub fn is_ball(&self, object: ObjectId) -> bool {
        self.ball(object).is_ok()
    }

    /// `005468D0`, the ball the goals and the HUD read.
    pub fn powerball(&self) -> Option<ObjectId> {
        self.powerball
    }

    /// `0052E528`/`0052E529`, each side's goals (bit 7 a won side), and
    /// `0052E52C`/`0052E52D`, the counts another machine last said.
    pub fn powerball_goals(&self) -> [u8; 2] {
        self.powerball_goals
    }

    pub fn set_powerball_goals(&mut self, goals: [u8; 2]) {
        self.powerball_goals = goals;
    }

    fn handle_actor(&mut self, handle: ActorHandle) -> Result<Option<ObjectId>, ObjectError> {
        self.resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)
    }

    fn ball_position<E>(&self, ball: ObjectId) -> Result<[u16; 3], BallError<E>> {
        self.object(ball)?
            .base
            .position_words
            .ok_or(BallError::Unwritten(ball, "position +32"))
    }

    /// Case 0x5B (selector 91): the boulder's 00405780, then 004027E0,
    /// whose handles read the process NULL as 0047A7A0 did for +88.
    pub fn allocate_selector91(
        &mut self,
        request: crate::object_runtime::ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, crate::object_runtime::ActorConstructionError> {
        let object = self.allocate_selector59(request)?;
        let null = self.actor(object)?.owner_handle;
        self.start_ball(object, null)?;
        Ok(object)
    }

    /// `004027E0` after the boulder's construction: the ball's fields, the
    /// ball pointer 005468D0 when none is set, and every goal byte idle
    /// (0x80).
    pub(in crate::object_runtime) fn start_ball(
        &mut self,
        ball: ObjectId,
        null: ActorHandle,
    ) -> Result<(), ObjectError> {
        match &mut self.actor_mut(ball)?.extension {
            ActorExtension::Selector59(state) => {
                state.ball_148 = Some(Box::new(BallState::new(null)));
            }
            _ => return Err(ObjectError::NotBall(ball)),
        }
        if self.powerball.is_none() {
            self.powerball = Some(ball);
        }
        self.powerball_goals = [GOAL_WON; 2];
        self.powerball_seen = [GOAL_WON; 2];
        Ok(())
    }

    /// `004028A0`: the ball pointer clears when it names this ball's handle.
    pub(in crate::object_runtime) fn end_ball(
        &mut self,
        ball: ObjectId,
    ) -> Result<(), ObjectError> {
        if let Some(current) = self.powerball {
            if current == ball || self.actor(current)?.handle == self.actor(ball)?.handle {
                self.powerball = None;
            }
        }
        Ok(())
    }

    /// `004038E0`, after the boulder's `00405DE0`: awake (+140 = 0, flag
    /// 0x4000000 clear) and home where it stands.
    pub(in crate::object_runtime) fn finish_ball_payload(
        &mut self,
        ball: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector59_rest_clear(ball)?;
        let base = &mut self.owned_mut(ball)?.state.base;
        let flags = base.flags & !0x0400_0000;
        mark_flag_change(base, flags);
        let home = base.position_words;
        self.ball_mut(ball)?.home_160 = home;
        Ok(())
    }

    /// `00402A40(state, handle)`, a local ball's change of state: a ball
    /// another machine owns, or one already in the state, keeps it.
    pub fn set_ball_state<H: BallHost>(
        &mut self,
        ball: ObjectId,
        state: u32,
        handle: ActorHandle,
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        let null = host
            .process_null_handle()
            .ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
        if self.object(ball)?.base.flags & 0x8000_0000 != 0 || self.ball(ball)?.state_148 == state {
            return Ok(());
        }
        let goals = self.powerball_goals;
        let mut stored = state;
        let mut start = |arena: &mut Self, ball| -> Result<(), BallError<H::Error>> {
            // Cases 0 and 5: the kick-off's sound, and the strip's highest
            // ground once; then case 1's lock.
            let sound = match goals {
                [0, 0] => Some(SOUND_KICK_OFF),
                [a, b] if a != b => Some(if b < a { 0x36 } else { 0x37 }),
                _ => None,
            };
            if let Some(sound) = sound {
                let position = arena.ball_position(ball)?;
                host.ball_sound(sound, position)
                    .map_err(|error| BallError::Host {
                        ball,
                        address: 0x0040_2b6b,
                        error,
                    })?;
            }
            if arena.ball(ball)?.threshold_170 == 0x10000 {
                let [x, _, z] = arena.ball_position(ball)?;
                let terrain = host.ball_terrain();
                let mut highest = cell_height(terrain, [x, z]);
                let mut column = x;
                for _ in 0..0x40 {
                    column = column.wrapping_add(0x400);
                    highest = highest.max(cell_height(terrain, [column, z]));
                }
                arena.ball_mut(ball)?.threshold_170 = highest;
            }
            Ok(())
        };
        match state {
            4 => {
                let random = if (goals[0] | goals[1]) & GOAL_WON == 0 {
                    Some(host.random().map_err(|error| BallError::Host {
                        ball,
                        address: 0x0040_2aab,
                        error,
                    })?)
                } else {
                    None
                };
                self.score_ball_fields(ball, random)?
            }
            5 | 0 => {
                if state == 5 {
                    let home = self
                        .ball(ball)?
                        .home_160
                        .ok_or(BallError::Unwritten(ball, "home +160"))?;
                    self.actor_mut(ball)?.words_8e_92 = [0, RESET_RISE, 0];
                    self.owned_mut(ball)?.state.base.position_words = Some(home);
                    stored = 0;
                }
                start(self, ball)?;
                self.lock_ball(ball, null)?;
            }
            1 => self.lock_ball(ball, null)?,
            2 | 3 => {
                let state = self.ball_mut(ball)?;
                if stored == 2 {
                    state.holder_14c = handle;
                }
                state.last_150 = state.holder_14c;
            }
            _ => {}
        }
        self.advance_ball_state(ball, stored)?;
        Ok(())
    }

    /// `00402A40`'s case 4: still, 500 above home, waiting 500 ms while a
    /// side has won (or the goals are idle) and otherwise 5000 ms and up
    /// to 2047 more by `random`, a 0048F060 draw; the last holder is the
    /// scorer.
    fn score_ball_fields<E>(
        &mut self,
        ball: ObjectId,
        random: Option<u32>,
    ) -> Result<(), BallError<E>> {
        let home = self
            .ball(ball)?
            .home_160
            .ok_or(BallError::Unwritten(ball, "home +160"))?;
        self.actor_mut(ball)?.words_8e_92 = [0; 3];
        let position = [home[0], home[1].wrapping_add(SCORED_HEIGHT), home[2]];
        self.owned_mut(ball)?.state.base.position_words = Some(position);
        let goals = self.powerball_goals;
        let wait = if (goals[0] | goals[1]) & GOAL_WON != 0 {
            500
        } else {
            let random = random.ok_or(BallError::Unwritten(ball, "a draw of 0048F060"))?;
            ((random as u16) >> 5).wrapping_add(5000) as i16
        };
        let state = self.ball_mut(ball)?;
        state.wait_180 = wait;
        state.scorer_154 = state.last_150;
        Ok(())
    }

    /// `00402A40`'s cases 0, 1 and 5: let go, and not to be taken for 350 ms.
    fn lock_ball(&mut self, ball: ObjectId, null: ActorHandle) -> Result<(), ObjectError> {
        let state = self.ball_mut(ball)?;
        state.lock_17e = LOCK;
        state.holder_14c = null;
        Ok(())
    }

    /// `00402A40(1)`, which reaches nothing beyond the ball: loose, unless
    /// another machine owns it or it is loose already.
    fn loose_ball(&mut self, ball: ObjectId, null: ActorHandle) -> Result<(), ObjectError> {
        if self.object(ball)?.base.flags & 0x8000_0000 != 0 || self.ball(ball)?.state_148 == 1 {
            return Ok(());
        }
        self.lock_ball(ball, null)?;
        self.advance_ball_state(ball, 1)
    }

    /// `00402A40`'s tail: the state stored and its sequence moved on (never 0).
    fn advance_ball_state(&mut self, ball: ObjectId, stored: u32) -> Result<(), ObjectError> {
        let state = self.ball_mut(ball)?;
        state.sequence_174 = state.sequence_174.wrapping_add(1);
        if state.sequence_174 == 0 {
            state.sequence_174 = 1;
        }
        state.state_148 = stored;
        Ok(())
    }

    /// `004028D0`: unless locked, each vehicle on the actor list (flags
    /// 0x924 under 0x19FEC, with a +64 view) within 500 whose frame puts
    /// the ball just ahead (between its size and that plus 4/3 of the
    /// ball's ahead, under half the ball's across) records the ball's
    /// bearing in its frame (`0048F540`) and takes it. The walk goes on.
    pub fn ball_take_check<H: BallHost>(
        &mut self,
        ball: ObjectId,
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        if self.ball(ball)?.lock_17e != 0 {
            return Ok(());
        }
        let size = u32::from(self.object(ball)?.base.word_28);
        let (half, ahead) = ((size >> 1) as i32, ((size * 4) as i32) / 3);
        for actor in self.actor_list().to_vec() {
            if self.object(actor)?.base.flags & 0x19fec != 0x924 {
                continue;
            }
            if self.actor_view_64(actor)?.is_none() {
                continue;
            }
            let here = self.ball_position(ball)?;
            let there = self
                .object(actor)?
                .base
                .position_words
                .ok_or(BallError::Unwritten(actor, "taker position"))?;
            let offset: [i16; 3] =
                std::array::from_fn(|axis| here[axis].wrapping_sub(there[axis]) as i16);
            let square = offset
                .iter()
                .map(|&value| i32::from(value) * i32::from(value))
                .fold(0i32, i32::wrapping_add);
            if square >= TAKE_REACH_SQUARED {
                continue;
            }
            let local = self.actor(actor)?.rotating.basis.inverse_words(offset);
            let reach = i32::from(self.object(actor)?.base.word_28);
            let front = i32::from(local[2]);
            if !(front > reach && front < reach + ahead && i32::from(local[1]).abs() < half) {
                continue;
            }
            let angles = [
                angle_48f540(front, i32::from(local[0]), inverse_sine)? as i16,
                angle_48f540(front, i32::from(local[1]), inverse_sine)? as i16,
            ];
            self.ball_mut(ball)?.angles_15c = angles.map(Some);
            let handle = self.actor(actor)?.handle;
            self.set_ball_state(ball, 2, handle, host)?;
        }
        Ok(())
    }

    /// Virtual `+64` of an actor: a vehicle is its own view; the camera
    /// and every other class have none.
    fn actor_view_64(&self, actor: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        Ok(self.selector2_actor(actor).is_ok().then_some(actor))
    }

    /// `00403660`, the ball's +A4 with `other`: a vehicle (its +64 view)
    /// takes a free, unlocked ball at its bearing; a held ball drops
    /// (`00403940` with no velocity).
    pub fn ball_contact<H: BallHost>(
        &mut self,
        ball: ObjectId,
        other: ObjectId,
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        let Some(view) = self.actor_view_64(other)? else {
            return Ok(());
        };
        let state = self.ball(ball)?;
        if state.lock_17e == 0 && state.state_148 <= 1 {
            let here = self.ball_position(ball)?;
            let there = self
                .object(view)?
                .base
                .position_words
                .ok_or(BallError::Unwritten(view, "taker position"))?;
            let offset: [i16; 3] =
                std::array::from_fn(|axis| here[axis].wrapping_sub(there[axis]) as i16);
            let local = self.actor(view)?.rotating.basis.inverse_words(offset);
            let front = i32::from(local[2]);
            let angles = [
                angle_48f540(front, i32::from(local[0]), inverse_sine)? as i16,
                angle_48f540(front, i32::from(local[1]), inverse_sine)? as i16,
            ];
            self.ball_mut(ball)?.angles_15c = angles.map(Some);
            let handle = self.actor(view)?.handle;
            return self.set_ball_state(ball, 2, handle, host);
        }
        if matches!(state.state_148, 2 | 3) {
            let holder = state.holder_14c;
            self.throw_ball(ball, holder, None, 0, host)?;
        }
        Ok(())
    }

    /// `00403940(holder, velocity, mode)`: a ball its `holder` holds is let
    /// go, with `velocity` (none, a drop: no throw) added to its own; modes
    /// 0 and 1 jitter X and Z by up to the throw's ground speed over 8 (or
    /// 64 ahead in mode 0's state 3, and mode 0 halves a turned throw
    /// first). Then it is loose, the throw remembers its sequence, and a
    /// ball another machine owns tells it (message 44).
    pub fn throw_ball<H: BallHost>(
        &mut self,
        ball: ObjectId,
        holder: ActorHandle,
        velocity: Option<[i16; 3]>,
        mode: u32,
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        let null = host.process_null_handle();
        self.throw_ball_with(ball, holder, velocity, mode, null, || host.random())
    }

    /// [`Self::throw_ball`] with its draws of 0048F060 from `random`.
    fn throw_ball_with<E>(
        &mut self,
        ball: ObjectId,
        holder: ActorHandle,
        velocity: Option<[i16; 3]>,
        mode: u32,
        null: Option<ActorHandle>,
        mut random: impl FnMut() -> Result<u32, E>,
    ) -> Result<(), BallError<E>> {
        let state = self.ball(ball)?;
        if !matches!(state.state_148, 2 | 3) || state.holder_14c != holder {
            return Ok(());
        }
        let held = state.state_148;
        match velocity {
            None => self.ball_mut(ball)?.throw_176 = [0; 3],
            Some(velocity) => {
                let mut throw = velocity;
                if mode <= 1 {
                    let [x, _, z] = velocity.map(i32::from);
                    let speed = isqrt_48efc0(x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)))
                        as i16 as i32;
                    let spread = if mode == 0 && held == 3 {
                        speed / 64
                    } else {
                        if mode == 0 {
                            throw = throw.map(|value| value / 2);
                        }
                        speed / 8
                    };
                    for axis in [0, 2] {
                        let draw = (random().map_err(|error| BallError::Host {
                            ball,
                            address: 0x0048_f060,
                            error,
                        })? & 0xffff) as i32;
                        let jitter = spread - ((draw * spread) >> 15);
                        throw[axis] = throw[axis].wrapping_add(jitter as i16);
                    }
                }
                self.ball_mut(ball)?.throw_176 = throw;
                let speed = &mut self.actor_mut(ball)?.words_8e_92;
                for axis in 0..3 {
                    speed[axis] = speed[axis].wrapping_add(throw[axis] as u16);
                }
            }
        }
        let null = null.ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
        self.loose_ball(ball, null)?;
        let state = self.ball_mut(ball)?;
        state.throw_17c = state.sequence_174;
        if self.object(ball)?.base.flags & 0x8000_0000 != 0 {
            let send = self.ball_send(ball, true)?;
            self.queue_network_send(44, send);
        }
        Ok(())
    }

    /// Message 43 or 44 about `ball` (00403D60, 00403DF0): its holder, the
    /// throw's sequence and, for 44, the throw.
    fn ball_send(&self, ball: ObjectId, throw: bool) -> Result<GameSend, ObjectError> {
        let state = self.ball(ball)?;
        Ok(GameSend::Ball {
            holder: state.holder_14c.raw(),
            sequence: state.throw_17c,
            throw: throw.then_some(state.throw_176),
        })
    }

    /// `00402CE0`: a held ball follows its holder for a second after; a
    /// scored one stops. Following, it drags its holder's X and Z speed by
    /// the delta's sixteenths, takes its holder's speed and sits at reach.
    fn carry_ball<E>(
        &mut self,
        ball: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<(), BallError<E>> {
        let state = self.ball_mut(ball)?;
        match state.state_148 {
            2 | 3 => state.carry_16c = CARRY,
            4 => state.carry_16c = 0,
            _ => {}
        }
        if state.carry_16c == 0 {
            return Ok(());
        }
        let holder = state.holder_14c;
        if let Some(holder) = self.handle_actor(holder)? {
            self.drag_holder(ball, holder, delta)?;
            self.attach_ball(ball, sine_quarter)?;
        }
        let state = self.ball_mut(ball)?;
        state.carry_16c = if state.carry_16c > delta as i32 {
            state.carry_16c - delta as i32
        } else {
            0
        };
        Ok(())
    }

    /// `00403464..004034BB`: the holder's X and Z speed lose `speed *
    /// (delta >> 4) >> 16` (0047B510 on the holder), and the ball takes
    /// the holder's speed.
    fn drag_holder(
        &mut self,
        ball: ObjectId,
        holder: ObjectId,
        delta: u32,
    ) -> Result<(), ObjectError> {
        let speed = self.actor(holder)?.words_8e_92;
        let factor = (delta >> 4).wrapping_neg() as i32;
        let drag = |word: u16| ((i32::from(word as i16).wrapping_mul(factor)) >> 16) as u16;
        self.add_actor_linear_velocity(holder, [drag(speed[0]), 0, drag(speed[2])])?;
        let speed = self.actor(holder)?.words_8e_92;
        self.actor_mut(ball)?.words_8e_92 = speed;
        Ok(())
    }

    /// `00403750`: a held ball sits its and its holder's sizes from the
    /// holder: turned by its angles in the holder's frame (state 2) or
    /// straight ahead along the holder's basis (3), each axis truncated.
    pub fn attach_ball<E>(
        &mut self,
        ball: ObjectId,
        sine_quarter: &[u16; 4096],
    ) -> Result<(), BallError<E>> {
        let state = self.ball(ball)?;
        let held = state.state_148;
        if !matches!(held, 2 | 3) {
            return Ok(());
        }
        let angles = state.angles_15c;
        let Some(holder) = self.handle_actor(state.holder_14c)? else {
            return Ok(());
        };
        let reach = i32::from(self.object(holder)?.base.word_28)
            + i32::from(self.object(ball)?.base.word_28);
        let basis = self.actor(holder)?.rotating.basis;
        let offset: [u16; 3] = if held == 2 {
            let [Some(a), Some(b)] = angles else {
                return Err(BallError::Unwritten(ball, "angles +15C"));
            };
            let part = |value: i32| (value.wrapping_mul(reach) >> 15) as i16;
            let local = [
                part(sine(i32::from(a), sine_quarter)),
                part(sine(i32::from(b), sine_quarter)),
                part(sine(i32::from(a).wrapping_add(0x4000), sine_quarter)),
            ];
            basis.transform_words(local)
        } else {
            let reach = f64::from(reach as i16);
            std::array::from_fn(|axis| {
                let row = f64::from(f32::from_bits(basis.words[7 + axis]));
                native_trunc_low32(reach * row) as u16
            })
        };
        let at = self
            .object(holder)?
            .base
            .position_words
            .ok_or(BallError::Unwritten(holder, "holder position"))?;
        let position = std::array::from_fn(|axis| at[axis].wrapping_add(offset[axis]));
        self.owned_mut(ball)?.state.base.position_words = Some(position);
        Ok(())
    }

    /// Each held angle a step `delta >> 6` nearer zero.
    fn settle_angles<E>(&mut self, ball: ObjectId, delta: u32) -> Result<(), BallError<E>> {
        let step = (delta >> 6) as i32;
        let state = self.ball_mut(ball)?;
        for angle in &mut state.angles_15c {
            let Some(value) = *angle else {
                return Err(BallError::Unwritten(ball, "angles +15C"));
            };
            if value == 0 {
                continue;
            }
            let value = i32::from(value);
            *angle = Some(if value > step {
                (value - step) as i16
            } else if value < -step {
                (value + step) as i16
            } else {
                0
            });
        }
        Ok(())
    }

    /// `+42` by the host's team flag 0054E81C, then 0047A110.
    fn ball_model(
        &mut self,
        ball: ObjectId,
        team_flag: u8,
        runtime: &crate::ResidentRuntime,
    ) -> Result<(), ObjectError> {
        let model = MODELS[usize::from(team_flag != 0)];
        self.store_visual_model_word(ball, model, runtime)
    }

    /// `00403300`, the ball's +24 on the machine that owns it.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_ball<H: BallHost>(
        &mut self,
        ball: ObjectId,
        delta: u32,
        team_flag: u8,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        let null = host
            .process_null_handle()
            .ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
        self.selector59_rest_clear(ball)?;
        if self.ball(ball)?.state_148 <= 1 {
            self.ball_take_check(ball, inverse_sine, host)?;
        }
        let [x, _, z] = self.ball_position(ball)?;
        if cell_height(host.ball_terrain(), [x, z]) >= self.ball(ball)?.threshold_170 {
            self.set_ball_state(ball, 5, null, host)?;
        }
        let step = (delta >> 10) as i32;
        let state = self.ball(ball)?.state_148;
        match state {
            0 | 1 => {
                if state == 1 {
                    if self.ball(ball)?.loose_158 > LOOSE_LIMIT {
                        self.set_ball_state(ball, 5, null, host)?;
                        return self.finish_ball_tick(ball, delta, team_flag, sine_quarter, host);
                    }
                    self.ball_mut(ball)?.loose_158 += step;
                }
                let lock = &mut self.ball_mut(ball)?.lock_17e;
                *lock = if i32::from(*lock) > step {
                    (i32::from(*lock) - step) as i16
                } else {
                    0
                };
                self.tick_selector59(ball, delta, status_load_factor, sine_quarter, host)
                    .map_err(BallError::Boulder)?;
            }
            2 | 3 => {
                if state == 2 {
                    let angles = self.ball(ball)?.angles_15c;
                    if angles == [Some(0), Some(0)] {
                        let holder = self.ball(ball)?.holder_14c;
                        self.set_ball_state(ball, 3, holder, host)?;
                    } else {
                        self.settle_angles(ball, delta)?;
                    }
                }
                self.ball_mut(ball)?.loose_158 = 0;
                let holder = self.ball(ball)?.holder_14c;
                match self.handle_actor(holder)? {
                    Some(holder) if self.actor(holder)?.word_c4 as i32 >= 3 => {
                        self.drag_holder(ball, holder, delta)?;
                        self.attach_ball(ball, sine_quarter)?;
                    }
                    _ => self.set_ball_state(ball, 1, null, host)?,
                }
            }
            4 => {
                let goals = self.powerball_goals;
                if (goals[0] | goals[1]) & GOAL_WON == 0 {
                    let wait = &mut self.ball_mut(ball)?.wait_180;
                    if i32::from(*wait) > step {
                        *wait = (i32::from(*wait) - step) as i16;
                    } else {
                        self.set_ball_state(ball, 0, null, host)?;
                    }
                }
            }
            5 => {
                self.set_flag1(ball, false)?;
                self.set_flag0(ball, false)?;
                let base = &mut self.owned_mut(ball)?.state.base;
                let flags = base.flags & 0xffff_fefb;
                mark_flag_change(base, flags);
            }
            _ => {}
        }
        self.finish_ball_tick(ball, delta, team_flag, sine_quarter, host)
    }

    fn finish_ball_tick<H: BallHost>(
        &mut self,
        ball: ObjectId,
        delta: u32,
        team_flag: u8,
        sine_quarter: &[u16; 4096],
        host: &H,
    ) -> Result<(), BallError<H::Error>> {
        self.carry_ball(ball, delta, sine_quarter)?;
        self.ball_model(ball, team_flag, host.ball_runtime())?;
        Ok(())
    }

    /// `00402DD0`, the ball's +3C on another machine. First the goals the
    /// host's sync brought: for each side whose count rose by one or two
    /// while neither has won, the first player hears the scorer (+156's
    /// owner): "%s" for the scorer's own side (H0 0x80, 0x81 for two, with
    /// sound 0x34 or 0x35 at the ball) or an own goal (0x7F), and the
    /// side's win (0x82 with H0 0x3A on). Then, as the host's, a free ball
    /// can be taken (which only its angles feel) and rolls, slowed near the
    /// ground; a held one settles its angles; a throw or drop is told again
    /// until the host's sync moves on; and it is carried and modelled.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_remote_ball<H: BallHost>(
        &mut self,
        ball: ObjectId,
        delta: u32,
        team_flag: u8,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        self.selector59_rest_clear(ball)?;
        for side in 0..2 {
            let (goals, seen) = (self.powerball_goals[side], self.powerball_seen[side]);
            if goals == seen {
                continue;
            }
            self.powerball_seen[side] = goals;
            let rise = i32::from(goals & 0x7f) - i32::from(seen & 0x7f);
            let both = self.powerball_goals[0] & self.powerball_goals[1];
            if self.first_player().is_none() || both & GOAL_WON != 0 || !(1..3).contains(&rise) {
                continue;
            }
            let scorer = usize::from(self.ball(ball)?.scorer_154.owner());
            let text = self.goal_line(ball, scorer, side as u32 + 2, rise, host)?;
            if !text.is_empty() {
                host.ball_line(self, text)
                    .map_err(|error| BallError::Host {
                        ball,
                        address: 0x0040_2fda,
                        error,
                    })?;
            }
            if goals & GOAL_WON != 0 {
                let text = self.win_line(ball, side as u16 + 0x3a, host)?;
                host.ball_line(self, text)
                    .map_err(|error| BallError::Host {
                        ball,
                        address: 0x0040_306d,
                        error,
                    })?;
            }
        }
        let state = self.ball(ball)?.state_148;
        if state <= 1 {
            self.ball_take_check(ball, inverse_sine, host)?;
        }
        match self.ball(ball)?.state_148 {
            0 | 1 => {
                self.tick_selector59(ball, delta, status_load_factor, sine_quarter, host)
                    .map_err(BallError::Boulder)?;
                let [x, y, z] = self.ball_position(ball)?;
                let ground = i32::from(host.ball_terrain().height_at([x, z]));
                let size = i32::from(self.object(ball)?.base.word_28);
                if i32::from(y as i16) < ground + size + 0x1e {
                    let speed = self
                        .actor(ball)?
                        .words_8e_92
                        .map(|word| i32::from(word as i16));
                    let square = speed
                        .iter()
                        .fold(0i32, |sum, v| sum.wrapping_add(v.wrapping_mul(*v)));
                    let length = isqrt_48efc0(square) as i32;
                    let slowed = length
                        - i32::from(
                            (((square as u32 >> 10).wrapping_mul(delta >> 5)) as i32 >> 15) as i16,
                        )
                        - 1;
                    let speed_words = &mut self.actor_mut(ball)?.words_8e_92;
                    *speed_words = if length > 0 {
                        speed.map(|v| (v.wrapping_mul(slowed) / length) as u16)
                    } else {
                        [0; 3]
                    };
                }
            }
            2 => {
                if self.ball(ball)?.angles_15c != [Some(0), Some(0)] {
                    self.settle_angles(ball, delta)?;
                }
            }
            _ => {}
        }
        let state = self.ball(ball)?;
        if state.throw_17c != 0 {
            if state.throw_17c == state.sequence_174 {
                let thrown = state.throw_176 != [0; 3];
                let send = self.ball_send(ball, thrown)?;
                self.queue_network_send(if thrown { 44 } else { 43 }, send);
            } else {
                self.ball_mut(ball)?.throw_17c = 0;
            }
        }
        self.finish_ball_tick(ball, delta, team_flag, sine_quarter, host)
    }

    /// A goal's line for `scorer` (+156's owner) at a side whose team is
    /// `team`: "%s" with the scorer's name (H0 0x80 for one, 0x81 for
    /// two, each with its sound at the ball or goal), or an own goal's
    /// (0x7F), when the scorer plays for another team.
    fn goal_line<H: BallHost>(
        &mut self,
        at: ObjectId,
        scorer: usize,
        team: u32,
        points: i32,
        host: &mut H,
    ) -> Result<Vec<u8>, BallError<H::Error>> {
        let game = &self
            .network_game_mut()
            .ok_or(BallError::Unwritten(at, "the network game's names"))?
            .game;
        let name = game
            .names
            .get(scorer)
            .map(|name| name.split(|&b| b == 0).next().unwrap_or_default().to_vec())
            .unwrap_or_default();
        let own = game.states.get(scorer).copied() == Some(team);
        let (format, sound) = match (own, points) {
            (false, _) => (0x7f, None),
            (true, 1) => (0x80, Some(SOUND_GOAL)),
            _ => (0x81, Some(SOUND_DOUBLE_GOAL)),
        };
        let format = host
            .ball_text(format)
            .map_err(|error| BallError::Host {
                ball: at,
                address: H0_POOL,
                error,
            })?
            .ok_or_else(|| BallError::Format(crate::free_look_view::null_format(format.into())))?;
        let text =
            crate::free_look_view::sprintf(&format, &[Some(&name)]).map_err(BallError::Format)?;
        if let Some(sound) = sound {
            let position = self.ball_position(at)?;
            host.ball_sound(sound, position)
                .map_err(|error| BallError::Host {
                    ball: at,
                    address: 0x0040_2fbb,
                    error,
                })?;
        }
        Ok(text)
    }

    /// "%s team wins" (H0 0x82) with the side's team name.
    fn win_line<H: BallHost>(
        &self,
        at: ObjectId,
        team_name: u16,
        host: &H,
    ) -> Result<Vec<u8>, BallError<H::Error>> {
        let text = |index: u16| {
            host.ball_text(index).map_err(|error| BallError::Host {
                ball: at,
                address: H0_POOL,
                error,
            })
        };
        let name = text(team_name)?;
        let format = text(0x82)?
            .ok_or_else(|| BallError::Format(crate::free_look_view::null_format(0x82)))?;
        crate::free_look_view::sprintf(&format, &[name.as_deref()]).map_err(BallError::Format)
    }

    /// `00403630`, +A0: whether `actor` holds the ball.
    pub fn ball_held_by(&self, ball: ObjectId, actor: ObjectId) -> Result<bool, ObjectError> {
        let state = self.ball(ball)?;
        Ok(matches!(state.state_148, 2 | 3) && self.actor(actor)?.handle == state.holder_14c)
    }

    /// `00403D30`, +2C's prefix: a held ball sits at reach before the base
    /// draw 0047AFC0.
    pub fn place_drawn_ball<E>(
        &mut self,
        ball: ObjectId,
        sine_quarter: &[u16; 4096],
    ) -> Result<(), BallError<E>> {
        self.attach_ball(ball, sine_quarter)
    }

    /// `00404450`, +28: the light's phase advances by the world's light
    /// delta (0055F0E0); a held ball glows grey (the phase over 16) and,
    /// past 0x3F, red or for team 2 blue, a free one cycles three phases
    /// of the phase over 8 a third of a turn apart. A light of 1000 at its
    /// X and Z (0045B1A0).
    pub fn ball_light(
        &mut self,
        ball: ObjectId,
        light_delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<[u8; 4], ObjectError> {
        let holder = self.ball(ball)?.holder_14c;
        let holder = self.handle_actor(holder)?;
        let state = self.ball_mut(ball)?;
        state.light_168 = state.light_168.wrapping_add(light_delta);
        let phase = state.light_168;
        let channel =
            |angle: u32| ((sine(i32::from(angle as u16 as i16), sine_quarter) as i16) >> 12) as u8;
        Ok(match holder {
            Some(holder) => {
                let grey = channel(phase >> 4);
                let mut color = [grey, grey, grey, 0];
                if self.actor(holder)?.word_b4 == 2 {
                    color[2] = grey.wrapping_add(0x3f);
                } else {
                    color[0] = grey.wrapping_add(0x3f);
                }
                color
            }
            None => {
                let base = phase >> 3;
                [
                    channel(base),
                    channel(base.wrapping_add(0x5555)),
                    channel(base.wrapping_add(0xaaaa)),
                    0,
                ]
            }
        })
    }

    /// `00403CF0`, +40 in a network game, after the boulder's 00413120
    /// (0047E2E0, then dirty bit 0x10): the bit cleared and 0x280 set, +D8
    /// cleared, and scored (00402A40(4)). Its constructor has just made
    /// every goal byte idle, so the wait is 500 ms and draws no number.
    pub(in crate::object_runtime) fn start_network_ball(
        &mut self,
        ball: ObjectId,
    ) -> Result<(), BallError<std::convert::Infallible>> {
        self.actor_mut(ball)?.word_d8 = 0;
        let base = &mut self.owned_mut(ball)?.state.base;
        base.dirty_word = (base.dirty_word & !0x10) | 0x280;
        let foreign = base.flags & 0x8000_0000 != 0;
        if foreign || self.ball(ball)?.state_148 == 4 {
            return Ok(());
        }
        self.score_ball_fields(ball, None)?;
        self.advance_ball_state(ball, 4)?;
        Ok(())
    }

    /// `004213C8..00421538`, a vehicle's damage in Powerball (mode 4),
    /// which it never takes. A blow of over 500 to the ball's holder
    /// (unless the ball dealt it) or from the holder knocks the ball loose
    /// (00403940 in mode 1): upward 300 or, for a collision with a live
    /// actor, half the holder's velocity less the other's and, upward, as
    /// much again as their facings (+48's third rows) agree, times 750.
    pub fn powerball_hit(
        &mut self,
        vehicle: ObjectId,
        packet: super::ActorDamagePacket,
        source: ActorHandle,
        null: Option<ActorHandle>,
        rng: &mut u32,
    ) -> Result<(), BallError<std::convert::Infallible>> {
        let Some(ball) = self.powerball else {
            return Ok(());
        };
        let holder = self.ball(ball)?.holder_14c;
        let ours = holder == self.actor(vehicle)?.handle && self.actor(ball)?.handle != source;
        if !ours && holder != source || packet.strength <= 500 {
            return Ok(());
        }
        let mut throw = [0, 300, 0];
        if packet.channel == 1 {
            if let Some(other) = self.handle_actor(source)? {
                let rows = |arena: &Self, actor| -> Result<[f64; 3], ObjectError> {
                    let words = arena.actor(actor)?.rotating.basis.words;
                    Ok([7, 8, 9].map(|index| f64::from(f32::from_bits(words[index]))))
                };
                let (own, theirs) = (rows(self, vehicle)?, rows(self, other)?);
                let agree = super::derived::kinematics::ftol(
                    ((theirs[2] * own[2] + theirs[1] * own[1]) + theirs[0] * own[0]) * 750.0,
                ) as i16;
                let (from, to) = if ours {
                    (vehicle, other)
                } else {
                    (other, vehicle)
                };
                let (a, b) = (self.actor(from)?.words_8e_92, self.actor(to)?.words_8e_92);
                let half = |axis: usize| (a[axis].wrapping_sub(b[axis]) as i16) / 2;
                throw = [
                    half(0),
                    half(1).wrapping_add(i32::from(agree).abs() as i16),
                    half(2),
                ];
            }
        }
        self.throw_ball_with(ball, holder, Some(throw), 1, null, || {
            Ok(crate::name_entry::name_entry_rng_next(rng))
        })
    }

    /// `0046A644`, the host's Powerball restart (its key +42 in mode 4).
    /// With both goal bytes idle, play starts (both 0); otherwise the ball
    /// is let go (00402A40(1)) and scored (00402A40(4)), while the goal
    /// bytes still say how long it waits, every player's score clears
    /// (0054E798) and both goal bytes go idle. Either way the key then
    /// waits half a second (0054ED18).
    pub fn restart_powerball<H: BallHost>(
        &mut self,
        countdown_ed18: &mut u32,
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        if self.powerball_goals == [GOAL_WON; 2] {
            self.powerball_goals = [0; 2];
        } else {
            if let Some(ball) = self.powerball {
                let null = host
                    .process_null_handle()
                    .ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
                self.set_ball_state(ball, 1, null, host)?;
                self.set_ball_state(ball, 4, null, host)?;
            }
            self.network_game_mut()
                .ok_or(BallError::NoScores)?
                .game
                .scores = Default::default();
            self.powerball_goals = [GOAL_WON; 2];
        }
        *countdown_ed18 = 500_000;
        Ok(())
    }

    /// `00403F40`, the goal's +24. Off the host it is 00479F20(0): the
    /// goal leaves the update list. On the host, within 4000 of the ball,
    /// the ball's offset in the goal's frame (0047A660) inside its radius
    /// +28 (across X and Y) marks the side of its plane it lies on: in
    /// front (Z above 0, +74) or behind (+75). Outside, the offset carried
    /// along the ball's velocity to the plane does the same, or clears
    /// both. Once both are marked while neither side has won, the goal
    /// scores: its side's goal count (and the count last said) rises by
    /// its points +70 and wins at the host settings' +30 over 1000; the
    /// marks clear; the ball is scored (00402A40(4)); its scorer, a live
    /// actor, gains the points as kills (0054E798) for a goal of its own
    /// side's or, for an own goal, has its deaths set to them; and the
    /// first player hears it as another machine would (00402DD0), the
    /// sounds at the goal.
    pub fn tick_goal<H: BallHost>(
        &mut self,
        goal: ObjectId,
        on_host: bool,
        limit_30: u32,
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        if !on_host {
            self.set_flag1(goal, false)?;
            return Ok(());
        }
        let marks = |arena: &mut Self, mark: Option<bool>| -> Result<(), ObjectError> {
            let state = arena.goal_state_mut(goal)?;
            match mark {
                Some(true) => state.crossed_74 = 1,
                Some(false) => state.crossed_75 = 1,
                None => (state.crossed_74, state.crossed_75) = (0, 0),
            }
            Ok(())
        };
        let Some(ball) = self.powerball else {
            marks(self, None)?;
            return Ok(());
        };
        let size = i32::from(self.object(goal)?.base.word_28);
        let reach = size.wrapping_mul(size);
        let here = self.ball_position(ball)?;
        let there = self
            .object(goal)?
            .base
            .position_words
            .ok_or(BallError::Unwritten(goal, "position +32"))?;
        let offset: [i16; 3] =
            std::array::from_fn(|axis| here[axis].wrapping_sub(there[axis]) as i16);
        let square = offset
            .iter()
            .map(|&value| i32::from(value) * i32::from(value))
            .fold(0i32, i32::wrapping_add);
        if length(square) >= 4000 {
            marks(self, None)?;
            return Ok(());
        }
        let basis = self.race_object(goal)?.rotating.basis;
        let mut local = basis.inverse_words(offset);
        let across = |local: [i16; 3]| {
            (i32::from(local[0]) * i32::from(local[0]))
                .wrapping_add(i32::from(local[1]) * i32::from(local[1]))
        };
        if across(local) < reach {
            marks(self, Some(local[2] > 0))?;
        } else {
            let velocity = self.actor(ball)?.words_8e_92.map(|word| word as i16);
            let along = basis.inverse_words(velocity);
            if along[2] != 0 {
                let depth = i32::from(local[2]);
                let carry = |value: i16| (i32::from(value) * depth / i32::from(along[2])) as i16;
                local[0] = local[0].wrapping_add(carry(along[0]));
                local[1] = local[1].wrapping_add(carry(along[1]));
                marks(self, (across(local) < reach).then_some(local[2] > 0))?;
            }
        }
        let state = *self.goal_state(goal)?;
        let goals = self.powerball_goals;
        if state.crossed_74 == 0 || state.crossed_75 == 0 || (goals[0] | goals[1]) & GOAL_WON != 0 {
            return Ok(());
        }
        if !(1..=2).contains(&state.side_6c) {
            return Err(BallError::GoalSide(goal, state.side_6c));
        }
        let side = state.side_6c as usize - 1;
        let points = state.points_70;
        let count = self.powerball_goals[side].wrapping_add(points as u8);
        self.powerball_goals[side] = count;
        self.powerball_seen[side] = self.powerball_seen[side].wrapping_add(points as u8);
        let won = i32::from(count) >= limit_30 as i32 / 1000;
        if won {
            self.powerball_goals[side] = count | GOAL_WON;
        }
        marks(self, None)?;
        let null = host
            .process_null_handle()
            .ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
        self.set_ball_state(ball, 4, null, host)?;
        let scorer = self.ball(ball)?.scorer_154;
        let slot = (scorer.raw() >> 27) as usize;
        if let Some(actor) = self.handle_actor(scorer)? {
            let team = self.actor(actor)?.word_b4;
            let own_goal = team == if side == 0 { 3 } else { 2 };
            let game = &mut self
                .network_game_mut()
                .ok_or(BallError::Unwritten(
                    goal,
                    "the network game's scores 0054E798",
                ))?
                .game;
            let record = game
                .scores
                .get_mut(slot)
                .ok_or(BallError::ScorerSlot { goal, slot })?;
            if own_goal {
                record[1] = points as u16;
            } else {
                record[0] = record[0].wrapping_add(points as u16);
            }
        }
        if self.first_player().is_some() {
            let text = self.goal_line(goal, slot, side as u32 + 2, points as i32, host)?;
            host.ball_line(self, text)
                .map_err(|error| BallError::Host {
                    ball: goal,
                    address: 0x0040_4348,
                    error,
                })?;
            if won {
                let text = self.win_line(goal, side as u16 + 0x3a, host)?;
                host.ball_line(self, text)
                    .map_err(|error| BallError::Host {
                        ball: goal,
                        address: 0x0040_43d9,
                        error,
                    })?;
            }
        }
        Ok(())
    }
}

/// `00403AE0`: the ball's sync record into `out`, its length, or -1 when
/// `room` is short. With the ball pointer naming it, bit 7 marks the long
/// form: the two goal bytes, the sequence and the state's holder (with the
/// angles' high bytes held turned) or scorer.
pub fn encode_ball(
    state: &BallState,
    goals: [u8; 2],
    is_powerball: bool,
    pointer_set: bool,
    out: &mut [u8],
    room: usize,
) -> i32 {
    let mut size = 1;
    let mut flag = state.state_148 as u8;
    if pointer_set {
        if is_powerball {
            flag |= 0x80;
            size = 5;
        }
        match state.state_148 {
            2 => size += 6,
            3 | 4 => size += 4,
            _ => {}
        }
    }
    if room < size {
        return -1;
    }
    out[0] = flag;
    if flag & 0x80 == 0 {
        return 1;
    }
    out[1] = goals[0];
    out[2] = goals[1];
    out[3..5].copy_from_slice(&state.sequence_174.to_le_bytes());
    match state.state_148 {
        2 => {
            let [a, b] = state
                .angles_15c
                .map(|angle| (angle.unwrap_or(0) as u16 >> 8) as u8);
            out[5] = a;
            out[6] = b;
            out[7..11].copy_from_slice(&state.holder_14c.raw().to_le_bytes());
            11
        }
        3 => {
            out[5..9].copy_from_slice(&state.holder_14c.raw().to_le_bytes());
            9
        }
        4 => {
            out[5..9].copy_from_slice(&state.scorer_154.raw().to_le_bytes());
            9
        }
        _ => 5,
    }
}

/// What a received sync record tells (`00403BC0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BallSync {
    pub state: u32,
    pub goals: [u8; 2],
    pub sequence: u16,
    pub holder: ActorHandle,
    /// The angles' high bytes held turned; the other states fill them from
    /// stack residue, and a scored record's first from the scorer's low byte.
    pub angles: [Option<i16>; 2],
    pub scorer: Option<ActorHandle>,
}

/// `00403BC0`'s read of a long record (a short one reads stack residue).
pub fn read_ball_sync(
    record: &[u8],
    null: ActorHandle,
) -> Result<BallSync, BallError<std::convert::Infallible>> {
    let byte = |at: usize| record.get(at).copied().ok_or(BallError::SyncShort(at));
    let dword = |at: usize| -> Result<u32, BallError<std::convert::Infallible>> {
        record
            .get(at..at + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
            .ok_or(BallError::SyncShort(at))
    };
    let flag = byte(0)?;
    if flag & 0x80 == 0 {
        return Err(BallError::ShortSync);
    }
    let state = u32::from(flag & 0x7f);
    let sequence = u16::from_le_bytes([byte(3)?, byte(4)?]);
    let (holder, angles, scorer) = match state {
        2 => (
            ActorHandle::from_raw(dword(7)?),
            [
                Some((u16::from(byte(5)?) << 8) as i16),
                Some((u16::from(byte(6)?) << 8) as i16),
            ],
            None,
        ),
        3 => (ActorHandle::from_raw(dword(5)?), [None; 2], None),
        4 => {
            let scorer = dword(5)?;
            (
                ActorHandle::from_raw(scorer),
                [Some(((scorer & 0xff) << 8) as u16 as i16), None],
                Some(ActorHandle::from_raw(scorer)),
            )
        }
        _ => (null, [None; 2], None),
    };
    Ok(BallSync {
        state,
        goals: [byte(1)?, byte(2)?],
        sequence,
        holder,
        angles,
        scorer,
    })
}

impl ObjectArena {
    /// `00403BC0`'s writes: the goal bytes, then the ball's fields; the
    /// throw's X and Y take its Z (+17A), the throw's sequence clears; held,
    /// the last holder is the holder; scored, the scorer is set. Back to
    /// free from another state, it answers the ball's position for the
    /// kick-off's sound (0x33).
    pub fn apply_ball_sync(
        &mut self,
        ball: ObjectId,
        sync: BallSync,
    ) -> Result<Option<[u16; 3]>, BallError<std::convert::Infallible>> {
        self.powerball_goals = sync.goals;
        let state = self.ball_mut(ball)?;
        let previous = state.state_148;
        state.angles_15c = sync.angles;
        state.sequence_174 = sync.sequence;
        state.state_148 = sync.state;
        state.throw_17c = 0;
        state.throw_176[0] = state.throw_176[2];
        state.throw_176[1] = state.throw_176[2];
        state.holder_14c = sync.holder;
        match sync.state {
            2 | 3 => state.last_150 = sync.holder,
            4 => {
                if let Some(scorer) = sync.scorer {
                    state.scorer_154 = scorer;
                }
            }
            0 if previous != 0 => return Ok(Some(self.ball_position(ball)?)),
            _ => {}
        }
        Ok(None)
    }

    /// Messages 43 and 44 received (`00403DA0`, `00403E50`): the ball held
    /// by `holder` in the sequence `sequence` drops, or is thrown with
    /// `throw` (mode 2).
    pub fn receive_ball_message<H: BallHost>(
        &mut self,
        holder: ActorHandle,
        sequence: u16,
        throw: Option<[i16; 3]>,
        host: &mut H,
    ) -> Result<(), BallError<H::Error>> {
        let Some(ball) = self.powerball else {
            return Ok(());
        };
        let state = self.ball(ball)?;
        if state.sequence_174 != sequence
            || state.holder_14c != holder
            || !matches!(state.state_148, 2 | 3)
        {
            return Ok(());
        }
        match throw {
            None => {
                let null = host
                    .process_null_handle()
                    .ok_or(BallError::Unwritten(ball, "process null 0054E894"))?;
                self.set_ball_state(ball, 1, null, host)
            }
            Some(throw) => self.throw_ball(ball, holder, Some(throw), 2, host),
        }
    }
}

#[cfg(test)]
#[path = "selector91_tests.rs"]
mod tests;
