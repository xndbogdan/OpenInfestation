//! Selector 85, the Alien Brain: case 0x55 of 004603B0, 0x19C bytes,
//! through 00405F10 over selector 62's 00463F60 (the attack, which the brain
//! runs in its state 2). Its table 0051F9CC is the attack's 00526DC8 but for
//! +00 (00405F90 over the attack's destructor 00463FF0), +14 (the model
//! registers 00406630) and +24 (the update 00405FC0); the light +28
//! (004647E0) and +90 (00464930, which runs +24) are the attack's.
//!
//! Two process words tie it to the ending: 00546910, the phase the brain and
//! the director (selector 78) step each other through, which the constructor
//! zeroes, and 00546914, the restart that state 8 sets and the next update
//! takes. The update steps +184 through nine states, each entered with its
//! clock +17C at 0:
//!
//! - 0: every update saves the liquid plane (`[0055EE3C]->[0] >> [005483BC]`)
//!   at +19A; once the aliens hold no capture node (00547620), +160 clears
//!   and state 1 follows.
//! - 1: once the first player's unit holds the brain as its target, the player
//!   is given Invulnerability (0046B2B0(0x29, 1), its message held quiet),
//!   their +58 clears, the phase becomes 1 and state 2 follows.
//! - 2: in phase 2 the attack runs; once its +138 is 3 the brain drops to
//!   lifecycle 2 (0047D670(0, +84, 2)), the camera shakes (005483B0) and
//!   state 3 follows.
//! - 3: the phase becomes 3 until the director makes it 4; then state 4.
//! - 4: register 2 closes over 0x100000 us; 5: registers 3 and 4 fall to
//!   0xD000, register 1 following their mean; 6: register 5 sinks to 0xD000,
//!   then the phase becomes 5.
//! - 7: from 5 s the liquid rises from the saved level by a unit every 45 ms;
//!   after 50 s state 8, which puts it back and sets the restart.
//!
//! Below state 4 registers 3 and 4 swing on a sine of +178; from state 4 each
//! update whose first draw is above 0x6000 sets off a debris spark round it.

use thiserror::Error;

use super::{ActorConstructionError, ActorConstructionRequest, DerivedActorClass};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorHandle, ActorStateChangeRequest, ObjectArena, ObjectError, ObjectId,
        PlayerAttachError, PlayerId, PlayerMessageError, PlayerMessageOperation,
        PlayerMessageRequest, Selector62Error, Selector62Host,
    },
};

/// 0052ECA8, the swing's amplitude, which nothing writes.
const SWING: i32 = 0x444;
/// 00406316: the grant the lock gives, Invulnerability.
pub const BRAIN_INVULNERABILITY: u16 = 0x29;
/// 004060BA: the first draw above which a debris spark goes off.
const DEBRIS_DRAW: u32 = 0x6000;
/// 004063BA: register 2's closing time.
const CLOSING: u32 = 0x10_0000;
/// 00406414/00406454: the floor registers 3 to 5 fall to.
const FLOOR: i32 = 0xd000;
/// 00406545/00406551: the rise starts after 5 s and ends after 50 s.
const RISE_START: u32 = 5_000_000;
const RISE_END: u32 = 50_000_000;
/// 00406567: the rise's unit time, 45 ms.
const RISE_UNIT: u32 = 45_000;

/// 00405F10's fields after 00463F60.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector85State {
    /// +178, the swing's phase: the delta accumulates while +184 is below 4.
    pub swing_178: u32,
    /// +17C, the state's clock: every update adds the delta, a new state
    /// starts it at 0.
    pub clock_17c: u32,
    /// +180, zeroed by the constructor and read by nothing.
    pub dword_180: u32,
    /// +184, the state, 0 to 8.
    pub state_184: u32,
    /// +188/+18C, the speeds of registers 3 and 4 in state 5.
    pub speeds_188: [u32; 2],
    /// WORDs +190..+198, the model registers 1 to 5.
    pub registers_190: [u16; 5],
    /// WORD +19A, register 6: the liquid level state 0 saves.
    pub plane_19a: Option<u16>,
}

impl Selector85State {
    /// 00406630, the WORD at +18E plus twice the register: 0 is +18C's high
    /// WORD and 6 the saved level; past +19A the object ends.
    pub fn model_value(&self, register: u32) -> Option<u16> {
        match register {
            0 => Some((self.speeds_188[1] >> 16) as u16),
            1..=5 => Some(self.registers_190[register as usize - 1]),
            6 => self.plane_19a,
            _ => None,
        }
    }
}

/// 00546910 and 00546914, the ending's process words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlienBrainWords {
    /// 00546910, the ending's phase, 0 to 5.
    pub phase_546910: u32,
    /// 00546914, the brain's restart.
    pub restart_546914: u32,
}

/// What the brain's update reaches beyond the attack's.
pub trait Selector85Host: Selector62Host {
    /// 0048F060's seed, 0055F18C.
    fn rng(&mut self) -> &mut u32;
    /// 004665D0(1, point, velocity, owner, 0), a debris spark.
    fn spark(
        &mut self,
        arena: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// The H12 plane DWORD (`[0055EE3C]->[0]`) and the shift 005483BC.
    fn plane(&mut self) -> (i32, u32);
    /// Writes the H12 plane DWORD.
    fn set_plane(&mut self, plane: i32);
    /// 0046B2B0(id, amount) on the player, with its notices and messages.
    fn pickup(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), Self::Error>;
    /// 0047D670 on the brain.
    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
    ) -> Result<(), Self::Error>;
    /// 005483B0 = 1, the camera shake.
    fn shake(&mut self);
}

#[derive(Debug, Error)]
pub enum Selector85Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Attack(#[from] Selector62Error<E>),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
    #[error(
        "brain {actor:?} reads the liquid level +19A, unwritten before state 0, at {address:08X}"
    )]
    UnwrittenLevel { actor: ObjectId, address: u32 },
    /// 004062F6: the lock test calls the unit's +D0(0), which the port
    /// answers for the vehicle (00402790, its +148) alone.
    #[error("brain {actor:?} reads the target of {unit:?}, whose +D0 is not the vehicle's")]
    UnitTarget { actor: ObjectId, unit: ObjectId },
    #[error("brain {actor:?} service at {address:08X}: {error}")]
    Host {
        actor: ObjectId,
        address: u32,
        error: E,
    },
}

type Result85<T, E> = Result<T, Selector85Error<E>>;

impl ObjectArena {
    /// Case 0x55: 0x19C bytes, selector 62's 00463F60, then 00405F10: the
    /// clocks and the state 0, registers 1, 2 and 5 at 0xFFFF and 3 and 4 at
    /// 0xFFFF less the swing; the phase 00546910 returns to 0.
    pub fn allocate_selector85(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_selector62(request)?;
        let rest = (0xffff - SWING) as u16;
        self.selector62_mut(object)?.brain = Some(Box::new(Selector85State {
            swing_178: 0,
            clock_17c: 0,
            dword_180: 0,
            state_184: 0,
            speeds_188: [0; 2],
            registers_190: [0xffff, 0xffff, rest, rest, 0xffff],
            plane_19a: None,
        }));
        self.alien_brain.phase_546910 = 0;
        Ok(object)
    }

    pub fn selector85(&self, actor: ObjectId) -> Result<&Selector85State, ObjectError> {
        self.selector62(actor)?
            .brain
            .as_deref()
            .ok_or(ObjectError::NotSelector85(actor))
    }

    pub(crate) fn selector85_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector85State, ObjectError> {
        self.selector62_mut(actor)?
            .brain
            .as_deref_mut()
            .ok_or(ObjectError::NotSelector85(actor))
    }

    /// 00546910 and 00546914.
    pub fn alien_brain_words(&self) -> AlienBrainWords {
        self.alien_brain
    }

    pub fn alien_brain_words_mut(&mut self) -> &mut AlienBrainWords {
        &mut self.alien_brain
    }

    fn selector85_level<E>(&self, actor: ObjectId, address: u32) -> Result85<i32, E> {
        self.selector85(actor)?
            .plane_19a
            .map(|level| i32::from(level as i16))
            .ok_or(Selector85Error::UnwrittenLevel { actor, address })
    }

    /// 00405FC0, the brain's +24.
    pub fn tick_selector85<H: Selector85Host>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result85<(), H::Error> {
        let fail = |address| {
            move |error| Selector85Error::Host {
                actor,
                address,
                error,
            }
        };
        let brain = self.selector85_mut(actor)?;
        let mut next = brain.state_184;
        brain.clock_17c = brain.clock_17c.wrapping_add(delta);
        // 00405FF6: the restart puts the brain back in state 1 with its
        // registers open, and it no longer targets the player.
        if std::mem::take(&mut self.alien_brain.restart_546914) != 0 {
            self.selector62_mut(actor)?.may_target_160 = 0;
            let brain = self.selector85_mut(actor)?;
            brain.state_184 = 1;
            brain.registers_190 = [0xffff; 5];
            brain.speeds_188 = [0; 2];
            next = 1;
        }
        let state = self.selector85(actor)?.state_184;
        if (state as i32) < 4 {
            // 00406038: registers 3 and 4 swing on the sine of +178.
            let brain = self.selector85_mut(actor)?;
            brain.swing_178 = brain.swing_178.wrapping_add(delta);
            let turn = brain.swing_178 >> 5;
            let mut index = turn & 0x3ffc;
            if turn & 0x4000 != 0 {
                index = 0x3ffc - index;
            }
            let mut sine = u32::from(sine_quarter[(index >> 2) as usize]);
            if turn & 0x8000 != 0 {
                sine = sine.wrapping_neg();
            }
            let swing = i32::from(((sine as i32).wrapping_mul(SWING) >> 15) as i16);
            brain.registers_190[2] = swing.wrapping_sub(SWING).wrapping_sub(1) as u16;
            brain.registers_190[3] = 0xffff_i32.wrapping_sub(swing).wrapping_sub(SWING) as u16;
        } else {
            self.selector85_debris(actor, host)?;
        }
        match state {
            0 => {
                let (plane, shift) = host.plane();
                self.selector85_mut(actor)?.plane_19a = Some(plane.wrapping_shr(shift) as u16);
                if self.capture_nodes.aliens_547620 == 0 {
                    next = 1;
                    self.selector62_mut(actor)?.may_target_160 = 0;
                }
            }
            1 => {
                if let Some(player) = self.selector85_locker(actor)? {
                    // 0040631A..0040632A: the grant's message is held quiet
                    // (+89) and the player's +58 cleared after it.
                    let recipient = self.player_message(player)?.identity();
                    self.apply_player_message(PlayerMessageRequest {
                        recipient,
                        operation: PlayerMessageOperation::Suppress,
                    })?;
                    host.pickup(self, player, BRAIN_INVULNERABILITY, 1)
                        .map_err(fail(0x0040_6321))?;
                    self.world_player_mut(player)?.byte_58 = Some(0);
                    self.apply_player_message(PlayerMessageRequest {
                        recipient,
                        operation: PlayerMessageOperation::Enable,
                    })?;
                    next = 2;
                    self.alien_brain.phase_546910 = 1;
                }
            }
            2 => {
                if self.alien_brain.phase_546910 == 2 {
                    self.tick_selector62(actor, delta, host)?;
                    if self.selector62(actor)?.state_138 == 3 {
                        let source = self.actor(actor)?.handle;
                        let request = ActorStateChangeRequest {
                            target_state: 2,
                            argument: 0,
                            source,
                        };
                        host.change_state(self, actor, request)
                            .map_err(fail(0x0040_6379))?;
                        next = 3;
                        host.shake();
                    }
                }
            }
            3 => {
                if self.alien_brain.phase_546910 == 4 {
                    next = 4;
                } else {
                    self.alien_brain.phase_546910 = 3;
                }
            }
            4 => {
                let brain = self.selector85_mut(actor)?;
                if brain.clock_17c < CLOSING {
                    let elapsed = brain.clock_17c >> 10;
                    brain.registers_190[1] =
                        (0x03ff_fc00_u32.wrapping_sub(elapsed.wrapping_mul(0xffff)) >> 10) as u16;
                } else {
                    brain.registers_190[1] = 0;
                    next = 5;
                }
            }
            5 => {
                let step = (delta >> 5) as i32;
                let brain = self.selector85_mut(actor)?;
                for half in 0..2 {
                    let word = i32::from(brain.registers_190[2 + half]);
                    let pull = ((FLOOR - word) / 2 - 0x80).wrapping_mul(step) >> 15;
                    let speed =
                        (brain.speeds_188[half] as i32).wrapping_add(i32::from(pull as i16));
                    brain.speeds_188[half] = speed as u32;
                    let moved =
                        i32::from((speed.wrapping_mul(step) >> 15) as i16).wrapping_add(word);
                    brain.registers_190[2 + half] = moved.clamp(FLOOR, 0xffff) as u16;
                }
                let [lid, _, left, right, _] = brain.registers_190;
                if lid == 0 {
                    if i32::from(left) == FLOOR && i32::from(right) == FLOOR {
                        next = 6;
                    }
                } else {
                    let mean = (i32::from(right) + i32::from(left)) / 2 - FLOOR;
                    brain.registers_190[0] = if mean < 0x2000 {
                        (mean.wrapping_mul(0xffff) / 0x2000) as u16
                    } else {
                        0xffff
                    };
                }
            }
            6 => {
                let brain = self.selector85_mut(actor)?;
                if i32::from(brain.registers_190[4]) == FLOOR {
                    next = 7;
                    self.alien_brain.phase_546910 = 5;
                } else {
                    let sunk = u32::from(brain.registers_190[4]).wrapping_sub(delta >> 7) as u16;
                    brain.registers_190[4] = sunk.max(FLOOR as u16);
                }
            }
            7 => {
                let clock = self.selector85(actor)?.clock_17c;
                if clock > RISE_START {
                    if clock > RISE_END {
                        next = 8;
                    }
                    let level = self.selector85_level(actor, 0x0040_657b)?;
                    let risen = ((clock - RISE_START) / RISE_UNIT).wrapping_add(level as u32);
                    let (_, shift) = host.plane();
                    host.set_plane(risen.wrapping_shl(shift) as i32);
                }
            }
            8 => {
                self.alien_brain.restart_546914 = 1;
                let level = self.selector85_level(actor, 0x0040_65c6)?;
                let (_, shift) = host.plane();
                host.set_plane(level.wrapping_shl(shift));
            }
            _ => {}
        }
        let brain = self.selector85_mut(actor)?;
        if brain.state_184 != next {
            brain.state_184 = next;
            brain.clock_17c = 0;
        }
        Ok(())
    }

    /// 004062C5..0040630F: the first player (00488AA0), when their unit
    /// (0046B030) answers +58 and that answer's +D0(0) targets the brain.
    fn selector85_locker<E>(&mut self, actor: ObjectId) -> Result85<Option<PlayerId>, E> {
        let player = self.first_player();
        let unit = self.first_player_primary_actor()?;
        let (Some(player), Some(unit)) = (player, unit) else {
            return Ok(None);
        };
        let Some(unit) = self.actor_derived_component(unit)? else {
            return Ok(None);
        };
        let derived = self.derived_actor(unit)?;
        if !matches!(derived.class, DerivedActorClass::Selector2(_)) {
            return Err(Selector85Error::UnitTarget { actor, unit });
        }
        let Some(target) = derived.target_148 else {
            return Ok(None);
        };
        let held = self.native_target(target)?.handle_04();
        Ok((held == self.actor(actor)?.handle).then_some(player))
    }

    /// 004060B0..00406263: from state 4 on, a first draw above 0x6000 sets
    /// off a debris spark within the brain's radius (WORD +28), rising
    /// toward its middle; an odd draw throws it any way, an even one up.
    fn selector85_debris<H: Selector85Host>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result85<(), H::Error> {
        let first = name_entry_rng_next(host.rng()) & 0xffff;
        if first <= DEBRIS_DRAW {
            return Ok(());
        }
        let object = self.object(actor)?;
        let radius = u32::from(object.base.word_28);
        let [x, y, z] = object
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let rng = host.rng();
        let mut offset = || {
            ((name_entry_rng_next(rng) & 0xffff).wrapping_mul(radius) as i32 >> 15)
                .wrapping_sub(radius as i32) as i16
        };
        let (dx, dz) = (offset(), offset());
        let squared = i32::from(dx)
            .wrapping_mul(i32::from(dx))
            .wrapping_add(i32::from(dz).wrapping_mul(i32::from(dz)));
        let rise = ((radius >> 1) as i32)
            .wrapping_sub(inf_formats::native_basis::isqrt_48efc0(squared) as i32)
            as i16;
        let mut point = [
            x.wrapping_add(dx as u16),
            y.wrapping_add(rise.max(0) as u16),
            z.wrapping_add(dz as u16),
        ];
        let mut draw = |modulus: u32| ((name_entry_rng_next(rng) & 0xffff) % modulus) as i16;
        let velocity = if first & 1 != 0 {
            [draw(1200) - 600, draw(1200) - 600, draw(1200) - 600]
        } else {
            let velocity = [draw(1000) - 500, draw(200) + 600, draw(1000) - 500];
            point[0] = point[0].wrapping_add((velocity[0] / 16) as u16);
            point[2] = point[2].wrapping_add((velocity[2] / 16) as u16);
            velocity
        };
        point[1] = point[1].wrapping_add((velocity[1] / 16) as u16);
        let owner = self.actor(actor)?.handle;
        host.spark(self, point, velocity.map(|v| v as u16), owner)
            .map_err(|error| Selector85Error::Host {
                actor,
                address: 0x0040_625e,
                error,
            })
    }
}

#[cfg(test)]
#[path = "selector85_tests.rs"]
mod tests;
