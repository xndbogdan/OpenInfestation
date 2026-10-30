//! Selector30 virtual +24, 0048B3F0: the gate. It looks for the nearest
//! eligible actor within 1200 units of a point half its size along its own Y
//! axis, then drives its light (+174..+178) and its persistent sound +154.
//!
//! An actor inside the ring and in front of it (local Z between 0 and the
//! nearest so far, from 800) is tracked at +158. The next update, the tracked
//! actor passes (0048BE70, the [`pass`](super::pass)) once it is no longer in
//! front, unless the world still has aliens (+190) or the player lacks the
//! gate's grant (+18C with +191); either sends its own message. The first
//! find after two seconds of mission time names the destination. The +166
//! world publication stays a typed boundary.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{Selector30Error, Selector30State};
use crate::object_runtime::{
    ActorConstructionError, ActorExtension, ActorHandle, ObjectArena, ObjectError, ObjectId,
    PlayerAttachError, PlayerId,
};
use crate::process_sound::ProcessSoundRef;

/// 0048B120 stores these two colors once at module start.
const GREEN: [i8; 3] = [0x20, 0x50, 0x20];
const BLUE: [i8; 3] = [0x20, 0x20, 0x60];
/// DWORD 0053D980: the gate sound's full-volume range.
const SOUND_RANGE: i32 = 1500;
/// 0048B7ED: squared scan radius, 1200 units.
const SCAN_SQUARED: u32 = 0x15_f900;
/// 0048B71A: the tracking scan's starting local-Z bound.
const TRACK_DEPTH: i16 = 0x320;
/// 0048B5D4: the team the alien check counts, and 0048B5E6's excluded
/// resource (a gate's own).
const ALIEN_TEAM: u32 = 0x12;
const GATE_RESOURCE: u16 = 0xf;

#[derive(Clone, Copy, Debug)]
pub struct Selector30TickRequest {
    /// Shared animation clock 005471C0.
    pub animation_clock: u32,
    /// Process 0054E894, stored into +158 by a positive +164.
    pub process_null_handle: Option<ActorHandle>,
    /// Process 0055F448.
    pub network_active: bool,
    /// World W+38 (G+1E8), the mission's elapsed milliseconds.
    pub mission_elapsed_ms: Option<u32>,
}

pub trait Selector30UpdateHost {
    type Error;
    /// 00487F60 with sound 0x1A at the gate position.
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        volume: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, Self::Error>;
    /// 00487FC0 with the gate position.
    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// 00487FE0.
    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), Self::Error>;
    /// 0046B090 of the actor's handle: the player whose unit it is.
    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, Self::Error>;
    /// 0048B60A: 0047DD80(gate, actor, 0, 4, 0x161), the actor's +98 with
    /// code 4 and H0 string 0x161 ("You must not leave until you have
    /// cleared this world of aliens").
    fn aliens_remain(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// 0048B614: G+1C6 = 1, the world cleared of aliens.
    fn aliens_cleared(&mut self) -> Result<(), Self::Error>;
    /// 0048B411: G+1D8 (the mission's W+28) takes the gate's +164 and
    /// G+1DA (W+2A) 0x119.
    fn publish_gate_word(&mut self, word_164: u16) -> Result<(), Self::Error>;
    /// 0048B661..0048B6FB: on the player's message (004135D0), "%s: %s" of
    /// H0 string 0x164 and the name of the grant the gate wants.
    fn missing_grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<(), Self::Error>;
    /// 0048B937..0048BA17: on the player's message, H0 string 0x15F ("Warp
    /// to %s") with world `world`'s name (H0 0xE0 + world), or H0 0x160
    /// ("Surface-bound warpgate") for a gate to no world.
    fn destination(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        world: u32,
    ) -> Result<(), Self::Error>;
    /// 0048B705: 0048BE70, the actor's pass through the gate.
    fn pass(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector30UpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Selector30(#[from] Selector30Error),
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    #[error("selector30 {actor:?} requires {field}")]
    RequiredInput {
        actor: ObjectId,
        field: &'static str,
    },
    #[error("selector30 sound service failed: {0}")]
    Sound(E),
    #[error("selector30 {gate:?} service at {address:08X} failed: {error}")]
    Service {
        gate: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    pub fn tick_selector30<H: Selector30UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: Selector30TickRequest,
        host: &mut H,
    ) -> Result<(), Selector30UpdateError<H::Error>> {
        let state = self.selector30_actor(actor)?;
        // 0048B3F9..28: a set +166 with a nonnegative +164 publishes it at
        // G+1D8 with 0x119 at G+1DA; the update then goes on.
        if state.byte_166 != 0 && state.word_164 as i16 >= 0 {
            host.publish_gate_word(state.word_164).map_err(|error| {
                Selector30UpdateError::Service {
                    gate: actor,
                    address: 0x0048_b411,
                    error,
                }
            })?;
        }
        let positive_164 = state.word_164 as i16 > 0;
        let state = self.selector30_mut(actor)?;
        state.word_178 = 0;
        state.word_17c = 0x4b0;
        if positive_164 {
            let null = request
                .process_null_handle
                .ok_or(Selector30UpdateError::RequiredInput {
                    actor,
                    field: "process NULL 0054E894",
                })?;
            let state = self.selector30_mut(actor)?;
            state.word_168 = 0;
            state.intermediate.word_158 = Some(null.raw());
            return self.selector30_sound(actor, host);
        }
        let half = u32::from(self.object(actor)?.base.word_28) >> 1;
        let half_squared = half.wrapping_mul(half);
        let point = self.selector30_scan_point(actor, half)?;
        let held = self.selector30_actor(actor)?.intermediate.word_158.ok_or(
            Selector30UpdateError::RequiredInput {
                actor,
                field: "+158 handle",
            },
        )?;
        if let Some(passing) = self.resolve_actor_handle(ActorHandle::from_raw(held))? {
            self.selector30_crossing(actor, passing, point, half_squared, host)?;
            // 0048B70A: the process NULL, whether the actor passed or not.
            let null = request
                .process_null_handle
                .ok_or(Selector30UpdateError::RequiredInput {
                    actor,
                    field: "process NULL 0054E894",
                })?;
            self.selector30_mut(actor)?.intermediate.word_158 = Some(null.raw());
        }
        let (team, gate_18e) = (
            self.actor(actor)?.word_b4,
            self.selector30_actor(actor)?.byte_18e,
        );
        let basis = self.actor(actor)?.rotating.basis;
        let mut best: Option<(ObjectId, u32)> = None;
        let mut best_squared = SCAN_SQUARED + 1;
        let mut depth = TRACK_DEPTH;
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & 0x888 != 0x800
                || (self.actor(candidate)?.word_c4 as i32) < 3
            {
                continue;
            }
            if team == 0x12 && self.actor(candidate)?.word_b4 != 0x12 {
                continue;
            }
            // 0048B78D: virtual +5C, which only Selector12's table answers.
            if gate_18e != 0 && self.selector12_actor(candidate).is_err() {
                continue;
            }
            let other = self
                .object(candidate)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(candidate))?;
            let delta = word_delta(point, other);
            let squared = squared_words(delta);
            if squared >= SCAN_SQUARED {
                continue;
            }
            if squared < best_squared {
                best = Some((candidate, squared));
                best_squared = squared;
            }
            if squared < half_squared.wrapping_add(half_squared) {
                // 0048B810: 0047A660 into the gate's frame.
                let local = basis.inverse_words(delta);
                let radial = radial_squared(local);
                let nearer = if gate_18e != 0 && self.selector12_actor(candidate).is_ok() {
                    // 0048B84A: within 9/8 of the ring, either side.
                    let depth_here = i32::from(local[2]).abs();
                    (radial < half_squared.wrapping_mul(9) >> 3 && depth_here < i32::from(depth))
                        .then_some(depth_here as i16)
                } else {
                    // 0048B86B: within the ring and in front of it.
                    (radial < half_squared && local[2] > 0 && local[2] < depth).then_some(local[2])
                };
                if let Some(nearer) = nearer {
                    depth = nearer;
                    let handle = self.actor(candidate)?.handle.raw();
                    self.selector30_mut(actor)?.intermediate.word_158 = Some(handle);
                }
            }
        }
        let Some((found, best)) = best else {
            self.selector30_mut(actor)?.word_168 = 0;
            return self.selector30_sound(actor, host);
        };
        let distance = isqrt_48efc0(best as i32) & 0xffff;
        let distance_17c = if distance > half {
            distance.wrapping_sub(half) as u16
        } else {
            0
        };
        self.selector30_mut(actor)?.word_17c = distance_17c;
        if !request.network_active {
            let elapsed =
                request
                    .mission_elapsed_ms
                    .ok_or(Selector30UpdateError::RequiredInput {
                        actor,
                        field: "mission elapsed W+38",
                    })?;
            if elapsed as i32 > 2000 && self.selector30_actor(actor)?.word_168 == 0 {
                // 0048B91F: the first find names the destination.
                let player = host
                    .find_player(self, found)
                    .map_err(|error| service(actor, 0x0048_b92a, error))?;
                if let Some(player) = player {
                    let world = self.selector30_actor(actor)?.word_15c;
                    host.destination(self, player, world)
                        .map_err(|error| service(actor, 0x0048_ba17, error))?;
                }
            }
        }
        let state = self.selector30_mut(actor)?;
        state.word_168 = 1;
        let squared_17c = u32::from(distance_17c) * u32::from(distance_17c);
        state.word_178 = ((SCAN_SQUARED.wrapping_sub(squared_17c)) as i32 >> 13) as u32;
        state.word_174 = pulse_color(request.animation_clock, state.word_174);
        self.selector30_sound(actor, host)
    }

    /// 0048B4EE..0048B705: the actor tracked last update. Inside the ring
    /// and no longer in front of it (local Z at most 0) it passes, as does
    /// a selector-12 actor anywhere near a +18E gate. With +190 no alien may
    /// be left; with +18C and +191 the first player must own that grant.
    fn selector30_crossing<H: Selector30UpdateHost>(
        &mut self,
        gate: ObjectId,
        passing: ObjectId,
        point: [u16; 3],
        half_squared: u32,
        host: &mut H,
    ) -> Result<(), Selector30UpdateError<H::Error>> {
        let other = self
            .object(passing)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(passing))?;
        let delta = word_delta(point, other);
        if squared_words(delta) >= half_squared.wrapping_add(half_squared) {
            return Ok(());
        }
        let local = self.actor(gate)?.rotating.basis.inverse_words(delta);
        let state = self.selector30_actor(gate)?;
        let (gate_18e, byte_190, word_18c, byte_191) = (
            state.byte_18e,
            state.byte_190,
            state.word_18c,
            state.byte_191,
        );
        let selector12 = gate_18e != 0 && self.selector12_actor(passing).is_ok();
        if !selector12 && (radial_squared(local) >= half_squared || local[2] > 0) {
            return Ok(());
        }
        if byte_190 != 0 {
            if self.selector30_aliens_alive()? {
                return host
                    .aliens_remain(self, gate, passing)
                    .map_err(|error| service(gate, 0x0048_b60a, error));
            }
            host.aliens_cleared()
                .map_err(|error| service(gate, 0x0048_b614, error))?;
        }
        if word_18c != 0 && byte_191 != 0 {
            // 0048B637: 00488AA0's first player and 004752E0 on its +9C.
            if let Some(player) = self.first_player() {
                let grants = self.world_player(player)?.grants_9c;
                if !inf_formats::grant_is_owned(&grants, u32::from(word_18c))? {
                    return host
                        .missing_grant(self, player, word_18c)
                        .map_err(|error| service(gate, 0x0048_b6fb, error));
                }
            }
        }
        host.pass(self, gate, passing)
            .map_err(|error| service(gate, 0x0048_b705, error))
    }

    /// 0048B5B5..0048B612: a live alien (team 0x12, +C4 at least 3) that is
    /// neither built from a gate's resource nor flagged 0x8000.
    fn selector30_aliens_alive(&self) -> Result<bool, ObjectError> {
        for &candidate in self.actor_list() {
            let base = &self.object(candidate)?.base;
            let actor = self.actor(candidate)?;
            if actor.word_b4 == ALIEN_TEAM
                && actor.word_c4 as i32 >= 3
                && base.resource_word != GATE_RESOURCE
                && base.flags & 0x8000 == 0
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn selector30_mut(&mut self, actor: ObjectId) -> Result<&mut Selector30State, Selector30Error> {
        match &mut self
            .actor_mut(actor)
            .map_err(|_| Selector30Error::InvalidObject(actor))?
            .extension
        {
            ActorExtension::Selector30(state) => Ok(state),
            _ => Err(Selector30Error::NotSelector30(actor)),
        }
    }

    /// 0048B46B..DA: the actor position plus half its size along basis Y,
    /// each product truncated by __ftol.
    fn selector30_scan_point(&self, actor: ObjectId, half: u32) -> Result<[u16; 3], ObjectError> {
        let basis = self.actor(actor)?.rotating.basis;
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let half = f64::from(half as u16 as i16);
        Ok(std::array::from_fn(|axis| {
            let component = f64::from(f32::from_bits(basis.words[4 + axis]));
            (crate::object_runtime::native_trunc_low32(half * component) as u16)
                .wrapping_add(position[axis])
        }))
    }

    /// 0048BB51..E2: create or update the gate sound while +168 is set,
    /// otherwise release it.
    fn selector30_sound<H: Selector30UpdateHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector30UpdateError<H::Error>> {
        let state = self.selector30_actor(actor)?;
        if state.word_168 as i32 > 0 {
            let volume = (SOUND_RANGE - i32::from(state.word_17c)).wrapping_shl(16) / SOUND_RANGE;
            match state.intermediate.sound_154 {
                None => {
                    let reference = host
                        .create_sound(self, actor, volume as u32, 0x10000)
                        .map_err(Selector30UpdateError::Sound)?;
                    self.selector30_mut(actor)?.intermediate.sound_154 = Some(reference);
                }
                Some(reference) => host
                    .update_sound(self, actor, reference, volume as u32, 0x10000)
                    .map_err(Selector30UpdateError::Sound)?,
            }
        } else if let Some(reference) = state.intermediate.sound_154 {
            host.release_sound(reference)
                .map_err(Selector30UpdateError::Sound)?;
            self.selector30_mut(actor)?.intermediate.sound_154 = None;
        }
        Ok(())
    }
}

fn service<E>(gate: ObjectId, address: u32, error: E) -> Selector30UpdateError<E> {
    Selector30UpdateError::Service {
        gate,
        address,
        error,
    }
}

/// The gate's point less the actor's, axis by axis with WORD wrapping.
fn word_delta(point: [u16; 3], other: [u16; 3]) -> [i16; 3] {
    std::array::from_fn(|axis| point[axis].wrapping_sub(other[axis]) as i16)
}

/// The unsigned sum of the signed squares, as the scan compares it.
fn squared_words(delta: [i16; 3]) -> u32 {
    delta.into_iter().fold(0_u32, |sum, word| {
        let word = i32::from(word);
        sum.wrapping_add(word.wrapping_mul(word) as u32)
    })
}

/// Local X squared plus local Y squared: the distance from the ring's axis.
fn radial_squared(local: [i16; 3]) -> u32 {
    let [x, y] = [local[0], local[1]].map(i32::from);
    x.wrapping_mul(x).wrapping_add(y.wrapping_mul(y)) as u32
}

/// 0048BA44..4B: the clock times four, as a signed remainder of 256, blends
/// green and blue in two 128-step halves. Each channel is the signed sum
/// divided by 256 toward zero; byte +177 is left alone.
fn pulse_color(clock: u32, previous: u32) -> u32 {
    let mut phase = clock.wrapping_shl(2) as i32 % 256;
    let (first, second) = if phase > 0x7f {
        phase -= 0x80;
        (GREEN, BLUE)
    } else {
        (BLUE, GREEN)
    };
    let rest = 0x80 - phase;
    let mut bytes = previous.to_le_bytes();
    for channel in 0..3 {
        let sum = i32::from(first[channel]) * phase + i32::from(second[channel]) * rest;
        bytes[channel] = (sum / 256) as u8;
    }
    u32::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests;
