//! Selector 68, the Cranial Implanter (final table 00526398), and the
//! missiles' player-held slots, which every missile shares but only the
//! implant reaches: the steered flight (+44, 00457800), the view mode (+80,
//! 004575E0) and the hold (+88, 00457750). Selector 68's own update (+24,
//! 00458C30) retires an implant no player holds; its pair acceptance (+A0,
//! 00458DC0) gives the player the actor it touches, and its state 1 (+AC,
//! 00458E40) offers the player every actor near it before it goes.

use inf_formats::native_basis::isqrt_48efc0;

use super::missile::MissileResult;
use super::{MissileError, MissileHost};
use crate::object_runtime::actor::derived::kinematics::{
    ftol, transform_local_dwords, transform_world_dwords,
};
use crate::object_runtime::{
    mark_flag_change, ActorAngularResponseRequest, ActorHandle, ActorPhysicsRequest,
    CameraSettings, FollowTargetRequest, ObjectArena, ObjectError, ObjectId, PlayerControlBlock,
    PlayerId,
};
use crate::process_sound::ProcessSoundRef;
use crate::terrain_runtime::TerrainWaterSurfaceRequest;

/// 0045786B..0045788A: the engine sound and the gain and pitch it starts at.
const ENGINE_SOUND: u32 = 0x15;
const ENGINE_GAIN: u32 = 0x1000;
const ENGINE_PITCH: u32 = 0x800;
/// 005205B0: the nose's height in 1/4096 units.
const NOSE_SCALE: f64 = 4096.0;
/// 00457956/00457A0C: past this nose height a pitch toward it shrinks.
const NOSE_LIMIT: i32 = 3000;
/// 00457A8B: higher than this above the ground cell, the pitch turns down.
const CEILING: i32 = 4000;
/// 00457982/004579C4: the analog axes' dead zone.
const DEAD_ZONE: i32 = 0x3000;
/// 00457AF9/00457B15: the speed it holds, and the speed while thrusting.
const CRUISE: i32 = 1000;
const THRUST: i32 = 3000;
/// 00457B21: Fire spends the fuse once it is below this, 2 s flown.
const FIRE_FUSE: u16 = 28_000;
/// 00457AE3: the world flash of a lost launcher.
const LOST_FLASH: u32 = 0xf00;
/// 00457C62: below this squared rate the turning stops.
const STILL: u32 = 0xf424;
/// 00457D0E: 0047B1E0's position and world angular integration.
const FLIGHT_MASK: u8 = 0x18;
/// 0055F280: the CRT-initialized world up vector.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];
/// 00458E51: state 1's reach, 350 squared.
const STATE_ONE_REACH: u32 = 0x1_de84;
/// 00458E96: a candidate's flags under 0xB4 are 0x24.
const CANDIDATE_MASK: u32 = 0xb4;
const CANDIDATE_FLAGS: u32 = 0x24;
/// 00458DE2: the flags a taking implant keeps.
const TAKEN_KEEP: u32 = 0xffff_f68b;
/// 00457766: the fuse a player's hold sets, 30 s.
const HELD_FUSE: u16 = 30_000;
/// 00457796: the camera's view mode on a held missile.
const HELD_VIEW: u8 = 2;

/// The steered flight's services beyond the missiles' own.
pub trait MissileSteerHost: MissileHost {
    /// 00487F60 at the missile's +32 with `flag`: a new persistent sound.
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        missile: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
        flag: u32,
    ) -> Result<ProcessSoundRef, Self::Error>;
    /// 00487FC0 at the missile's +32.
    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        missile: ObjectId,
        reference: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// 0047CE90: the missile retires to state 1 with the process null.
    fn retire(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// The world's H14 flags (+4), whose 0x40 waves the water.
    fn world_flags(&self) -> u32;
}

/// 0046AE90 for the implant: the player takes the actor whose handle it is
/// given, and the camera follows it.
pub trait ImplantTakeHost {
    type Error;
    fn take(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<bool, Self::Error>;
}

/// A compiler reciprocal: the high DWORD of `value * magic`, shifted, plus
/// its sign bit, which truncates the quotient toward zero.
fn magic_quotient(value: i32, magic: i32, shift: u32) -> i32 {
    let high = (((i64::from(value) * i64::from(magic)) >> 32) as i32) >> shift;
    high.wrapping_add(((high as u32) >> 31) as i32)
}

/// 004578F1..004579F5: the block's turn (+10) and pitch (+14) give
/// `-step` while positive and `step` while negative; with neither, an
/// analog block (+0) gives each axis (+4, +8) beyond 0x3000 as
/// `(|axis| - 0x3000) * step / 0xD000` with its sign.
fn steering_rates(block: &PlayerControlBlock, step: i32) -> (i32, i32) {
    let (turn, pitch) = (block.turn_14, block.pitch_18);
    if turn | pitch != 0 {
        let rate = |value: i32| match value {
            0 => 0,
            value if value < 0 => step,
            _ => step.wrapping_neg(),
        };
        return (rate(turn), rate(pitch));
    }
    if block.analog_04 == 0 {
        return (0, 0);
    }
    let axis = |value: i32| {
        let magnitude = value.wrapping_abs();
        if magnitude <= DEAD_ZONE {
            return 0;
        }
        let beyond = if value < 0 {
            DEAD_ZONE.wrapping_sub(magnitude)
        } else {
            magnitude.wrapping_sub(DEAD_ZONE)
        };
        magic_quotient(beyond.wrapping_mul(step), 0x4ec4_ec4f, 14)
    };
    (axis(block.axes_08[0]), axis(block.axes_08[1]))
}

/// 00457939..00457A1E: a pitch toward the nose's limit shrinks by the
/// nose's height past it, in 1/1024ths of the pitch.
fn nose_limited_pitch(pitch: i32, nose: i32) -> i32 {
    if pitch > 0 {
        if nose < -NOSE_LIMIT {
            return pitch.wrapping_add(nose.wrapping_add(NOSE_LIMIT).wrapping_mul(pitch) >> 10);
        }
    } else if nose > NOSE_LIMIT {
        return pitch.wrapping_sub(nose.wrapping_sub(NOSE_LIMIT).wrapping_mul(pitch) >> 10);
    }
    pitch
}

/// 00457A20..00457ABA: more than 4000 above the ground cell beneath
/// (`gap`), the pitch turns down by `(gap - 4000) * step / 4000`, to at
/// most a step.
fn ceiling_pitch(pitch: i32, gap: i32, step: i32) -> i32 {
    if gap <= CEILING {
        return pitch;
    }
    pitch
        .wrapping_add(magic_quotient(
            gap.wrapping_sub(CEILING).wrapping_mul(step),
            0x1062_4dd3,
            8,
        ))
        .min(step)
}

/// 00457AF5..00457B5C: the speed approaches 1000, or itself above that, or
/// 3000 while thrusting, by `((delta >> 5) * 1000) >> 15` a flight, and
/// never passes it.
fn approached_speed(speed: i32, delta: u32, thrusting: bool) -> i32 {
    let target = if thrusting { THRUST } else { speed.max(CRUISE) };
    let speed = if speed < target {
        speed.wrapping_add((((delta >> 5).wrapping_mul(1000) as i32) >> 15) as i16 as i32)
    } else {
        speed
    };
    speed.min(target)
}

/// 00457BE1..00457CDB: the rates are held to ±0x7FFF, then lose
/// `((step * |w|) >> 15) + 32`, and stop below 250.
fn damped_rates(rates: [u32; 3], step: i32) -> [u32; 3] {
    let [a, b, c] = rates.map(|word| (word as i32).clamp(-0x7fff, 0x7fff));
    let squared = c
        .wrapping_mul(c)
        .wrapping_add(b.wrapping_mul(b))
        .wrapping_add(a.wrapping_mul(a));
    if (squared as u32) < STILL {
        return [0; 3];
    }
    let length = isqrt_48efc0(squared) as i32;
    let loss = ((step.wrapping_mul(length) >> 15) as i16 as i32).wrapping_add(0x20);
    if length <= loss {
        return [0; 3];
    }
    let keep = length - loss;
    [a, b, c].map(|value| (keep.wrapping_mul(value) / length) as u32)
}

impl ObjectArena {
    /// 0046B090 over the registry's null (the value of 0054E894): the
    /// player whose +D8, or +D4 while +D8 is that null, is `handle`.
    pub(crate) fn player_holding(
        &self,
        handle: ActorHandle,
    ) -> Result<Option<PlayerId>, ObjectError> {
        let null = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        if handle == null {
            return Ok(None);
        }
        Ok(self
            .players
            .list
            .iter()
            .find(|player| {
                if player.handle_d8 != null {
                    player.handle_d8 == handle
                } else {
                    player.handle_d4 == handle
                }
            })
            .map(|player| player.id))
    }

    /// 0045780E..00457849: the velocity along the nose row (+60..+68),
    /// `(Y * Y' + Z * Z') + X * X'` in x87, truncated (00511BA8).
    fn missile_forward_speed(&self, missile: ObjectId) -> Result<i32, ObjectError> {
        let actor = self.actor(missile)?;
        let row = |index: usize| f64::from(f32::from_bits(actor.rotating.basis.words[index]));
        let [x, y, z] = actor.words_8e_92.map(|word| f64::from(word as i16));
        Ok(ftol((y * row(8) + z * row(9)) + x * row(7)))
    }

    /// 00457800, the missiles' +44, while a player holds one. `controls` is
    /// the player's block (player +4), which the missile reads 4 bytes
    /// lower.
    pub fn tick_missile_steered<H: MissileSteerHost>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        controls: &PlayerControlBlock,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.missile(missile)?;
        let forward = self.missile_forward_speed(missile)?;
        let step = (delta >> 4) as i32;
        // 00457863..004578DD: the engine sound starts at the missile, or
        // follows the speed (two more conversions of the same sum).
        match *self.missile_tail::<H::Error>(missile)?.sound {
            None => {
                let reference = host
                    .create_sound(self, missile, ENGINE_SOUND, ENGINE_GAIN, ENGINE_PITCH, 1)
                    .map_err(MissileError::Host)?;
                *self.missile_tail::<H::Error>(missile)?.sound = Some(reference);
            }
            Some(reference) => {
                let gain = (forward.wrapping_add(0x400) << 4) as u32;
                let pitch = forward.wrapping_mul(8).wrapping_add(0x800) as u32;
                host.update_sound(self, missile, reference, gain, pitch)
                    .map_err(MissileError::Host)?;
            }
        }
        // 004578E2..00457905: the drag, (delta >> 10) * speed / 2000.
        let drag = magic_quotient(
            ((delta as i32) >> 10).wrapping_mul(forward),
            -0x1062_4dd3,
            7,
        );
        let speed = forward.wrapping_add(drag);
        let (yaw, pitch) = steering_rates(controls, step);
        // 00457939..00457ABA: the pitch near the vertical, and over the
        // ground cell beneath.
        let nose = {
            let actor = self.actor(missile)?;
            ftol(f64::from(f32::from_bits(actor.rotating.basis.words[8])) * NOSE_SCALE)
        };
        let pitch = nose_limited_pitch(pitch, nose);
        let [x, y, z] = self.missile_position::<H::Error>(missile)?;
        let cell = host
            .terrain(self)
            .map_err(MissileError::Host)?
            .raw_height_at([x, z]);
        let gap = i32::from(y as i16).wrapping_sub(i32::from(cell));
        let pitch = ceiling_pitch(pitch, gap, step);
        // 00457ABE..00457AF0: a launcher (+88) gone or below lifecycle 3
        // flashes the world and retires the missile, which flies on.
        let owner = self.actor(missile)?.owner_handle;
        let launcher = self
            .resolve_actor_handle(owner)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let live = match launcher {
            Some(launcher) => (self.actor(launcher)?.word_c4 as i32) >= 3,
            None => false,
        };
        if !live {
            let position = self.missile_position::<H::Error>(missile)?;
            host.flash(self, position, LOST_FLASH, rng)
                .map_err(MissileError::Host)?;
            host.retire(self, missile, rng)
                .map_err(MissileError::Host)?;
        }
        // 00457AF5..00457B5C: Fire spends the fuse once 2 s are flown, and
        // the speed approaches its target.
        if controls.fire_20 != 0 {
            let state = self.missile_mut(missile)?;
            if state.word_13a < FIRE_FUSE {
                state.word_13a = 0;
            }
        }
        let speed = approached_speed(speed, delta, controls.thrust_1c > 0);
        // 00457B60..00457BCB: with a rate, the angular velocity in the
        // missile's frame (0047A680) doubles a rate against it, the roll is
        // minus half the turn, and 0047B4B0 adds them in the world's frame.
        if pitch | yaw != 0 {
            let actor = self.actor(missile)?;
            let basis = actor.rotating.basis;
            let local = transform_world_dwords(basis, actor.words_94_9c.map(|word| word as i32));
            let (yaw, roll) = if yaw == 0 {
                (0, 0)
            } else {
                let yaw = if (local[1] ^ yaw) < 0 {
                    yaw.wrapping_shl(1)
                } else {
                    yaw
                };
                (yaw, (yaw / 2).wrapping_neg())
            };
            let pitch = if pitch == 0 {
                0
            } else if (local[0] ^ pitch) < 0 {
                pitch.wrapping_shl(1)
            } else {
                pitch
            };
            let world = transform_local_dwords(basis, [pitch, yaw, roll]);
            self.add_actor_angular_velocity(missile, world.map(|word| word as u32))?;
        }
        // 00457BD0..00457BDC: 0047D440 turns the up row (+54) toward the
        // world's up by the step.
        let up = {
            let words = self.actor(missile)?.rotating.basis.words;
            [4, 5, 6].map(|index| f32::from_bits(words[index]))
        };
        self.apply_actor_angular_response(
            missile,
            ActorAngularResponseRequest {
                current_axis: up,
                desired_axis: WORLD_UP,
                gain: step,
            },
        )?;
        // 00457BE1..00457CDB: the damping.
        let angular = &mut self.actor_mut(missile)?.words_94_9c;
        *angular = damped_rates(*angular, step);
        // 00457CE1..00457D13: the velocity is the speed along the nose
        // (0047A6C0 in place), and 0047B1E0 moves and turns the missile.
        let velocity = self.transform_visual_vector_words(missile, [0, 0, speed as u16])?;
        self.actor_mut(missile)?.words_8e_92 = velocity;
        self.tick_actor_physics(
            missile,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: FLIGHT_MASK,
                sine_quarter,
            },
        )?;
        // 00457D18..00457E8E: below the floor, the higher of the ground
        // (00450A30) and the water (00410F60's at wave clock 0), it rises
        // onto it and the fuse is spent.
        let [x, y, z] = self.missile_position::<H::Error>(missile)?;
        let world_flags = host.world_flags();
        let floor = {
            let terrain = host.terrain(self).map_err(MissileError::Host)?;
            let ground = terrain.height_at([x, z]);
            let water = terrain.water_surface_height(
                TerrainWaterSurfaceRequest {
                    xz: [x, z],
                    wave_clock: 0,
                    world_flags,
                    terrain_height: ground,
                },
                sine_quarter,
            );
            ground.max(water)
        };
        if floor > y as i16 {
            if let Some(position) = self.owned_mut(missile)?.state.base.position_words.as_mut() {
                position[1] = floor as u16;
            }
            self.missile_mut(missile)?.word_13a = 0;
        }
        // 00457E8E..00457EC8: the fuse (004555B0), then the exhaust ribbon,
        // made on the first flight, one point a flight.
        self.missile_fuse(missile, delta, rng, host)?;
        if self.missile_tail::<H::Error>(missile)?.ribbon.is_none() {
            let handle = self.actor(missile)?.handle;
            let ribbon = host
                .create_ribbon(self, handle)
                .map_err(MissileError::Host)?;
            *self.missile_tail::<H::Error>(missile)?.ribbon = ribbon;
        }
        if let Some(ribbon) = *self.missile_tail::<H::Error>(missile)?.ribbon {
            let position = self.missile_position::<H::Error>(missile)?;
            host.extend_ribbon(self, ribbon, delta, position)
                .map_err(MissileError::Host)?;
        }
        Ok(())
    }

    /// 004575E0, the missiles' +80: held by a player (0046B090 of +84), a
    /// missile takes any view mode but 0; otherwise only 0. A taken mode is
    /// kept at +D0.
    pub(crate) fn missile_view_mode(
        &mut self,
        missile: ObjectId,
        mode: u8,
    ) -> Result<bool, ObjectError> {
        let handle = self.actor(missile)?.handle;
        if self.player_holding(handle)?.is_some() != (mode != 0) {
            return Ok(false);
        }
        self.actor_mut(missile)?.byte_d0 = mode;
        Ok(true)
    }

    /// 00457750, the missiles' +88, after its 00478E00 (`camera`): the fuse
    /// becomes 30 s, and with a camera the camera follows the missile
    /// (004825D0, 00410E70) in view mode 2 (0040F800), with +187 set. It
    /// always accepts.
    pub(crate) fn hold_missile(
        &mut self,
        missile: ObjectId,
        camera: Option<ObjectId>,
        settings: CameraSettings,
    ) -> Result<(), ObjectError> {
        self.missile_mut(missile)?.word_13a = HELD_FUSE;
        if let Some(camera) = camera {
            let handle = self.actor(missile)?.handle;
            let target = self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
            self.replace_camera_target(camera, target, settings)?;
            self.set_camera_view_mode(camera, HELD_VIEW)?;
            self.set_camera_byte_187(camera)?;
        }
        Ok(())
    }

    /// Selector 68, the implant.
    pub fn is_implant(&self, object: ObjectId) -> bool {
        self.behavior_resource(object)
            .is_ok_and(|state| state.selector == 68)
            && self.is_missile(object)
    }

    /// 00458C30, selector 68's +24, which the world reaches only for an
    /// implant no player holds: it retires (0047CE90).
    pub fn tick_implant<H: MissileSteerHost>(
        &mut self,
        implant: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.missile(implant)?;
        host.retire(self, implant, rng).map_err(MissileError::Host)
    }

    /// 00458DC0, selector 68's +A0; true is no contact. Its launcher (+88)
    /// is none. Another actor not retired (flag 0x80000000), which the
    /// player holding the implant (0046B090 of +84) takes (0046AE90), is
    /// none either: the implant keeps only flags `0xFFFFF68B` and is
    /// disposed.
    pub fn implant_accepts<H: ImplantTakeHost>(
        &mut self,
        implant: ObjectId,
        other: ObjectId,
        host: &mut H,
    ) -> MissileResult<bool, H::Error> {
        self.missile(implant)?;
        let (own, launcher) = {
            let actor = self.actor(implant)?;
            (actor.handle, actor.owner_handle)
        };
        let handle = self.actor(other)?.handle;
        if handle == launcher {
            return Ok(true);
        }
        if self.object(other)?.base.flags & 0x8000_0000 != 0 {
            return Ok(false);
        }
        let Some(player) = self.player_holding(own)? else {
            return Ok(false);
        };
        if !host
            .take(self, player, handle)
            .map_err(MissileError::Host)?
        {
            return Ok(false);
        }
        let base = &mut self.owned_mut(implant)?.state.base;
        mark_flag_change(base, base.flags & TAKEN_KEEP);
        self.dispose(implant)?;
        Ok(true)
    }

    /// 00458E40, selector 68's +AC. Held by a player (flag 0x10 and
    /// 0046B090 of +84), it offers that player each actor of the actor list
    /// (G+48), in order, whose flags under 0xB4 are 0x24, within 350 (the
    /// squared distance, unsigned, below the reach), not itself and not its
    /// launcher: each one the player takes (0046AE90) narrows the reach to
    /// its distance. Then it is disposed.
    pub fn implant_state_one<H: ImplantTakeHost>(
        &mut self,
        implant: ObjectId,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.missile(implant)?;
        let (own, launcher) = {
            let actor = self.actor(implant)?;
            (actor.handle, actor.owner_handle)
        };
        let held = if self.object(implant)?.base.flags & 0x10 != 0 {
            self.player_holding(own)?
        } else {
            None
        };
        if let Some(player) = held {
            let [x, y, z] = self.missile_position::<H::Error>(implant)?;
            let mut reach = STATE_ONE_REACH;
            for candidate in self.actor_list().to_vec() {
                if self.object(candidate)?.base.flags & CANDIDATE_MASK != CANDIDATE_FLAGS {
                    continue;
                }
                let [cx, cy, cz] = self.missile_position::<H::Error>(candidate)?;
                let offset = |a: u16, b: u16| i32::from(a.wrapping_sub(b) as i16);
                let (dx, dy, dz) = (offset(cx, x), offset(cy, y), offset(cz, z));
                let squared = dy
                    .wrapping_mul(dy)
                    .wrapping_add(dz.wrapping_mul(dz))
                    .wrapping_add(dx.wrapping_mul(dx)) as u32;
                if squared >= reach {
                    continue;
                }
                let handle = self.actor(candidate)?.handle;
                if handle == own || handle == launcher {
                    continue;
                }
                if host
                    .take(self, player, handle)
                    .map_err(MissileError::Host)?
                {
                    reach = squared;
                }
            }
        }
        self.dispose(implant)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "implant_tests.rs"]
mod tests;
