//! Selector 59, the Boulders: 00405780 over the settling base 00450400 (the
//! shared 0047A7A0, then +140 = 0), final table 0051F900, case 0x3B, 0x148
//! bytes. The table is the shared actor's 00527C00 but for +00 (004057B0),
//! +04 (the commands 00405D10), +0C (00405DE0), +1C (00405DC0), +24 (the
//! update 004057D0), +40 (00413120), +94 (the terrain contact 00405A00) and
//! +A4 (the pair reaction 00405E30).
//!
//! A boulder starts dormant and asleep (flags 0x04200000) with its rest
//! timer past the half second that puts it to sleep; a wake trigger
//! (selector 42) or command 16 wakes it. Awake, it falls and rolls under the
//! shared physics, and while it is drawn near its basis turns by the arc it
//! moved over its radius. Half a second still, it falls asleep. Its terrain
//! contact throws up dust, or a splash where water stands above the contact
//! point, when it moves fast; an actor it touches takes damage by the
//! boulder's own change of speed.
//!
//! Selector 82 (case 0x52, 0x14C bytes, table 0051FE88) is a boulder that
//! an eye leaves when it dies (00408560): 00409890 adds a 5 s timer +148,
//! and its update 004098C0 is its only other method. Once the timer runs
//! out it bursts (0048A8B0), sounds (9) and is disposed; either way the
//! boulder's update runs.

use inf_formats::{
    native_basis::{isqrt_48efc0, NativeBasisMath},
    InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
};
use thiserror::Error;

use super::{
    native_trunc_low32, ActorCommandContext, ActorCommandError, ActorConstructionError,
    ActorConstructionRequest, ActorDamagePacket, ActorExtension, ActorInitializationRequest,
    ActorMotionError, ActorPhysicsRequest, CollisionContact, FamilyUpdateError, FamilyUpdateHost,
};
use crate::{
    object_runtime::{mark_flag_change, ObjectArena, ObjectError, ObjectId},
    terrain_runtime::{TerrainRuntime, TerrainWaterSurfaceRequest},
    ResidentRuntime,
};

/// 00405DFE: the rest timer a new boulder starts with, past the sleep.
const INITIAL_REST: u32 = 0x0007_a121;
/// 004059C2: microseconds still before a boulder falls asleep.
const SLEEP_AFTER: u32 = 0x0007_a120;
/// Flag 0x200000 (dormant, for the wake trigger) and 0x4000000 (asleep,
/// which also keeps it out of the terrain and furniture contacts).
const DORMANT_ASLEEP: u32 = 0x0420_0000;
const ASLEEP: u32 = 0x0400_0000;
/// Flag 0x400000, which the wake trigger sets on a dormant actor.
const WOKEN: u32 = 0x0040_0000;
/// 0040596C: an awake boulder clears all three.
const AWAKE_CLEAR: u32 = 0x0460_0000;
/// The draw pass's near flag: a boulder rolls and throws dust only while
/// it is set.
const DRAWN_NEAR: u32 = 0x0200_0000;
/// 0040597F: gravity, linear drag and position integration.
const PHYSICS_MASK: u8 = 0xd;
/// 00405844..00405873: 10430 (65536 / 2pi), the WORD angle per unit of arc.
const ARC_TO_ANGLE: i32 = 10430;
/// 0055F280: the CRT-initialized world up vector.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];
/// 00405A5F: the speed a contact must pass to throw up dust.
const DUST_SPEED: i32 = 100;
/// 00405E53: the change of speed past which an ordinary boulder crushes.
const CRUSH_SPEED: i32 = 0x140;
/// 00405EFB: the crushing packet's damage channel.
const CRUSH_CHANNEL: u8 = 3;

/// Case 0x3B's writes: 00450400's +140, then 00405780's three BYTEs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector59State {
    /// +138: the position at the last update, from the postinitialization
    /// on. The constructor leaves it unwritten.
    pub home_138: Option<[u16; 3]>,
    /// DWORD +140: microseconds still, unsigned. The payload initialization
    /// stores 500001, so a new boulder is asleep.
    pub rest_140: u32,
    /// BYTE +144: it crushes what it touches; 1 at construction, and
    /// command 17 stores its BYTE == 0.
    pub crushes_144: u8,
    /// BYTE +145: it crushes at any change of speed, not only above 320;
    /// command 18 stores its BYTE != 0.
    pub crushes_any_145: u8,
    /// BYTE +146: it smokes, running the status visual 0047DFD0 on every
    /// awake update; command 19 stores its BYTE != 0.
    pub smokes_146: u8,
    /// Selector 82's DWORD +148, the microseconds until it bursts; None for
    /// a boulder's 0x148 bytes.
    pub timer_148: Option<u32>,
    /// Selector 91's fields from +148, Powerball's ball.
    pub ball_148: Option<Box<super::selector91::BallState>>,
}

impl Selector59State {
    fn new() -> Self {
        Self {
            home_138: None,
            rest_140: 0,
            crushes_144: 1,
            crushes_any_145: 0,
            smokes_146: 0,
            timer_148: None,
            ball_148: None,
        }
    }
}

/// 00409898: selector 82's timer.
const REMAINS_TIMER: u32 = 5_000_000;
/// 00409940: the remains' sound.
const REMAINS_SOUND: u32 = 9;

/// 004098E9..0040992B: the remains' burst, the Powerball's record with
/// debris types 5.
fn remains_burst(position: [u16; 3]) -> crate::process_particles::births::ParticleBurst {
    crate::process_particles::births::ParticleBurst {
        debris_count: 0x10,
        position,
        placement_radius: 0x14,
        central_type: 1,
        debris_types: [5, 5],
        speed: 8,
        flags: 0,
    }
}

/// What selector 82's update reaches beyond the boulder's.
pub trait Selector82Host: FamilyUpdateHost {
    /// 0048A8B0, a burst.
    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        burst: crate::process_particles::births::ParticleBurst,
    ) -> Result<(), Self::Error>;
    /// 00487F10(sound, position, 0x10000, 0x10000), then 004923C0.
    fn sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// The particle a contact throws up, type 0 and flags 0, with the boulder's
/// halved velocity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoulderParticle {
    /// 00489600, where the water stands above the contact point.
    Splash,
    /// 004894A0 without its shadow, elsewhere.
    Dust,
}

/// The services the contact's dust reaches after the shared 0047D2F0.
pub trait Selector59ContactHost {
    type Error;
    /// One 0048F060 draw.
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// H12 entry zero and the animation clock 005471C0.
    fn surface(&self) -> Option<(&TerrainRuntime, u32, &[u16; 4096])>;
    /// One particle at `position`.
    fn particle(
        &mut self,
        kind: BoulderParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector59Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("boulder {0:?} reads its unwritten +138 before its postinitialization")]
    UnwrittenHome(ObjectId),
    #[error("boulder {0:?} divides its arc by a zero radius at 00405876")]
    ZeroRadius(ObjectId),
    #[error("boulder {0:?} physics 0047B1E0")]
    Physics(ObjectId, #[source] ActorMotionError),
    #[error(transparent)]
    Status(FamilyUpdateError<E>),
    #[error("boulder {0:?} is not selector 82, whose +148 times its burst")]
    NotSelector82(ObjectId),
    #[error("selector 82 {actor:?} service at {address:08X}: {error}")]
    Host {
        actor: ObjectId,
        address: u32,
        error: E,
    },
}

impl<E> From<FamilyUpdateError<E>> for Selector59Error<E> {
    fn from(error: FamilyUpdateError<E>) -> Self {
        Self::Status(error)
    }
}

#[derive(Debug, Error)]
pub enum Selector59ContactError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("boulder {actor:?} contact left DWORD +{offset:02X} unwritten at 00405A96")]
    RequiredContact { actor: ObjectId, offset: u8 },
    #[error("boulder {0:?} contact needs H12 entry zero at 00405B15")]
    MissingTerrain(ObjectId),
    #[error("boulder contact service failed: {0}")]
    Host(E),
}

/// 0048EFC0 over a squared length, halving through a set sign bit first
/// (00405A46, 00405E9C).
fn length(square: i32) -> i32 {
    if square < 0 {
        (isqrt_48efc0(((square as u32) >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(square) as i32
    }
}

/// 00405840..0040591B: each axis's move since +138 as a WORD angle (the
/// signed WORD difference times 10430, divided by the radius +28), the world
/// up vector crossed with it in the FPU's 53-bit precision (X and Z spilled
/// to f32 first) and truncated (00511BA8). A zero radius faults at the first
/// division (00405876).
fn roll_turn(position: [u16; 3], home: [u16; 3], radius: u16) -> Option<[i16; 3]> {
    let radius = i32::from(radius);
    if radius == 0 {
        return None;
    }
    let [x, y, z] = std::array::from_fn(|axis| {
        i32::from(position[axis].wrapping_sub(home[axis]) as i16).wrapping_mul(ARC_TO_ANGLE)
            / radius
    });
    let [up_x, up_y, up_z] = WORLD_UP.map(f64::from);
    let (x_spilled, z_spilled) = (f64::from(x as f32), f64::from(z as f32));
    let (y, z) = (f64::from(y), f64::from(z));
    Some(
        [
            native_trunc_low32(z * up_y - y * up_z),
            native_trunc_low32(x_spilled * up_z - z_spilled * up_x),
            native_trunc_low32(y * up_x - x_spilled * up_y),
        ]
        .map(|value| value as i16),
    )
}

impl ObjectArena {
    /// Case 0x3B: 0x148 bytes, 00450400 and 00405780.
    pub fn allocate_selector59(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector59(Box::new(Selector59State::new()));
        Ok(object)
    }

    /// Case 0x52: the boulder's 00405780, then 00409890's timer.
    pub fn allocate_selector82(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_selector59(request)?;
        self.selector59_mut(object)?.timer_148 = Some(REMAINS_TIMER);
        Ok(object)
    }

    /// 004098C0, selector 82's +24: once the timer +148 is below the delta
    /// it bursts, sounds and is disposed, else the timer falls; then the
    /// boulder's update 004057D0.
    pub fn tick_selector82<H: Selector82Host>(
        &mut self,
        remains: ObjectId,
        delta: u32,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector59Error<H::Error>> {
        let fail = |address| {
            move |error| Selector59Error::Host {
                actor: remains,
                address,
                error,
            }
        };
        let timer = self
            .selector59(remains)?
            .timer_148
            .ok_or(Selector59Error::NotSelector82(remains))?;
        if timer < delta {
            let position = self
                .object(remains)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(remains))?;
            host.burst(self, remains_burst(position))
                .map_err(fail(0x0040_992f))?;
            host.sound(REMAINS_SOUND, position)
                .map_err(fail(0x0040_9945))?;
            self.dispose(remains)?;
        } else {
            self.selector59_mut(remains)?.timer_148 = Some(timer - delta);
        }
        self.tick_selector59(remains, delta, status_load_factor, sine_quarter, host)
    }

    pub fn selector59(&self, object: ObjectId) -> Result<&Selector59State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector59(state) => Ok(state),
            _ => Err(ObjectError::NotSelector59(object)),
        }
    }

    fn selector59_mut(&mut self, object: ObjectId) -> Result<&mut Selector59State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector59(state) => Ok(state),
            _ => Err(ObjectError::NotSelector59(object)),
        }
    }

    /// 00405DE0, the boulder's +0C: the shared +08 and 0047AC10 as
    /// 0047AD70 runs them, then the rest timer 500001 and flags 0x04200000
    /// (asleep and dormant), marking the low nine bits' change.
    pub fn initialize_selector59_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.selector59(object)?;
        self.initialize_actor(object, request, runtime, math)?;
        self.selector59_mut(object)?.rest_140 = INITIAL_REST;
        let base = &mut self.owned_mut(object)?.state.base;
        let flags = base.flags | DORMANT_ASLEEP;
        mark_flag_change(base, flags);
        Ok(())
    }

    /// 00405D10: the shared 0047ADA0 through code 15 (signed); code 16 with
    /// a nonzero BYTE wakes it (+140 = 0, flags 0x04200000 cleared), 17
    /// stores BYTE == 0 at +144, 18 and 19 BYTE != 0 at +145 and +146; any
    /// higher code refuses without a read.
    pub fn apply_selector59_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(59) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector59(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let byte = match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector59Wake(value))
            | (17, InstanceCommandPayload::Selector59Crushes144(value))
            | (18, InstanceCommandPayload::Selector59CrushesAny145(value))
            | (19, InstanceCommandPayload::Selector59Smokes146(value)) => value.value,
            (16..=19, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        };
        let state = self.selector59_mut(object)?;
        match code {
            16 => {
                if byte != 0 {
                    state.rest_140 = 0;
                    let base = &mut self.owned_mut(object)?.state.base;
                    let flags = base.flags & !DORMANT_ASLEEP;
                    mark_flag_change(base, flags);
                }
            }
            17 => state.crushes_144 = u8::from(byte == 0),
            18 => state.crushes_any_145 = u8::from(byte != 0),
            _ => state.smokes_146 = u8::from(byte != 0),
        }
        Ok(true)
    }

    /// +140 = 0: awake, as the ball's 004038E0 and its updates keep it.
    pub(in crate::object_runtime) fn selector59_rest_clear(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector59_mut(object)?.rest_140 = 0;
        Ok(())
    }

    /// 00405DC0, the boulder's +1C in place of the shared 0047AAD0: it rises
    /// by its +28 (WORD, wrapping) and keeps that position as +138.
    pub fn finish_selector59_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector59(object)?;
        let base = &mut self.owned_mut(object)?.state.base;
        let mut position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))?;
        position[1] = position[1].wrapping_add(base.word_28);
        base.position_words = Some(position);
        self.selector59_mut(object)?.home_138 = Some(position);
        Ok(())
    }

    /// 004057D0, the boulder's +24. Where it was at the last update and
    /// still (no X or Z velocity, Y within 32), a woken boulder wakes fully
    /// (+140 = 0, flags 0x04600000 cleared) and moves; otherwise an asleep
    /// one returns, and a resting one adds the delta to +140 and moves, or
    /// falls asleep (flag 0x4000000) without moving once that reaches half a
    /// second. Having moved, it rolls while drawn near and keeps its new
    /// position. Moving clears a set +140 and the three flags, then runs the
    /// shared physics 0047B1E0(delta, 0xD) and, smoking, the status visual
    /// 0047DFD0.
    pub fn tick_selector59<H: FamilyUpdateHost>(
        &mut self,
        boulder: ObjectId,
        delta: u32,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector59Error<H::Error>> {
        let home = self
            .selector59(boulder)?
            .home_138
            .ok_or(Selector59Error::UnwrittenHome(boulder))?;
        let position = self
            .object(boulder)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(boulder))?;
        if position == home {
            let velocity = self.actor(boulder)?.words_8e_92;
            if (velocity[0] | velocity[2]) == 0 && i32::from(velocity[1] as i16).abs() <= 0x20 {
                let flags = self.object(boulder)?.base.flags;
                if flags & WOKEN != 0 {
                    self.selector59_mut(boulder)?.rest_140 = 0;
                    let base = &mut self.owned_mut(boulder)?.state.base;
                    mark_flag_change(base, flags & !AWAKE_CLEAR);
                    return self.move_selector59(
                        boulder,
                        delta,
                        status_load_factor,
                        sine_quarter,
                        host,
                    );
                }
                let rest = self.selector59(boulder)?.rest_140;
                if rest >= SLEEP_AFTER {
                    return Ok(());
                }
                let rest = rest.wrapping_add(delta);
                self.selector59_mut(boulder)?.rest_140 = rest;
                if rest < SLEEP_AFTER {
                    return self.move_selector59(
                        boulder,
                        delta,
                        status_load_factor,
                        sine_quarter,
                        host,
                    );
                }
                let base = &mut self.owned_mut(boulder)?.state.base;
                mark_flag_change(base, flags | ASLEEP);
                return Ok(());
            }
        } else {
            if self.object(boulder)?.base.flags & DRAWN_NEAR != 0 {
                self.roll_selector59(boulder, position, home, sine_quarter)?;
            }
            self.selector59_mut(boulder)?.home_138 = Some(position);
        }
        if self.selector59(boulder)?.rest_140 != 0 {
            self.selector59_mut(boulder)?.rest_140 = 0;
            let base = &mut self.owned_mut(boulder)?.state.base;
            let flags = base.flags & !AWAKE_CLEAR;
            mark_flag_change(base, flags);
        }
        self.move_selector59(boulder, delta, status_load_factor, sine_quarter, host)
    }

    /// 00405981..0040599B.
    fn move_selector59<H: FamilyUpdateHost>(
        &mut self,
        boulder: ObjectId,
        delta: u32,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector59Error<H::Error>> {
        self.tick_actor_physics(
            boulder,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: PHYSICS_MASK,
                sine_quarter,
            },
        )
        .map_err(|error| Selector59Error::Physics(boulder, error))?;
        if self.selector59(boulder)?.smokes_146 != 0 {
            self.tick_status_visual(boulder, delta, status_load_factor, host)?;
        }
        Ok(())
    }

    /// 00405840..0040593D: the world turn for the arc moved since +138
    /// ([`roll_turn`]), turned into the boulder's frame (0047A660) and back
    /// out to turn its basis (0047A760).
    fn roll_selector59<E>(
        &mut self,
        boulder: ObjectId,
        position: [u16; 3],
        home: [u16; 3],
        sine_quarter: &[u16; 4096],
    ) -> Result<(), Selector59Error<E>> {
        let radius = self.object(boulder)?.base.word_28;
        let turn = roll_turn(position, home, radius).ok_or(Selector59Error::ZeroRadius(boulder))?;
        let mut basis = self.actor(boulder)?.rotating.basis;
        let local = basis.inverse_words(turn);
        let world = basis.transform_words(local);
        super::selector16::rotate_world_vector(&mut basis, world, sine_quarter);
        self.actor_mut(boulder)?.rotating.basis = basis;
        Ok(())
    }

    /// 00405A13..00405CF8, the boulder's +94 after the shared 0047D2F0.
    /// Drawn near and faster than 100, a draw below `(speed + 20) * 64`
    /// throws up one particle at the contact point (the radius back along
    /// the normal) with half its velocity (Y's magnitude): a splash where the
    /// water (00410F60 with its waves, over 00450A30's ground) stands above
    /// the point, dust elsewhere. The same draw below half of that throws
    /// up a second, the point moved by up to half the radius either way on
    /// each axis.
    pub fn selector59_contact<H: Selector59ContactHost>(
        &mut self,
        boulder: ObjectId,
        contact: CollisionContact,
        host: &mut H,
    ) -> Result<(), Selector59ContactError<H::Error>> {
        self.selector59(boulder)?;
        if self.object(boulder)?.base.flags & DRAWN_NEAR == 0 {
            return Ok(());
        }
        let velocity = self
            .actor(boulder)?
            .words_8e_92
            .map(|word| i32::from(word as i16));
        let square = velocity[0]
            .wrapping_mul(velocity[0])
            .wrapping_add(velocity[1].wrapping_mul(velocity[1]))
            .wrapping_add(velocity[2].wrapping_mul(velocity[2]));
        let speed = length(square);
        if speed <= DUST_SPEED {
            return Ok(());
        }
        let chance = speed.wrapping_add(20).wrapping_shl(6);
        let roll = (host.random().map_err(Selector59ContactError::Host)? & 0xffff) as i32;
        if chance <= roll {
            return Ok(());
        }
        let normal = contact.normal_bits();
        let mut point = [0u16; 3];
        let reach = f64::from(self.object(boulder)?.base.word_28.wrapping_neg() as i16);
        let position = self
            .object(boulder)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(boulder))?;
        for axis in 0..3 {
            let bits = normal[axis].ok_or(Selector59ContactError::RequiredContact {
                actor: boulder,
                offset: 4 + 4 * axis as u8,
            })?;
            let offset = native_trunc_low32(reach * f64::from(f32::from_bits(bits))) as u16;
            point[axis] = offset.wrapping_add(position[axis]);
        }
        let half = |value: i32| ((value - (value >> 31)) >> 1) as u16;
        let thrown = [
            half(velocity[0]),
            half(velocity[1].abs()),
            half(velocity[2]),
        ];
        let kind = {
            let (terrain, clock, sine_quarter) = host
                .surface()
                .ok_or(Selector59ContactError::MissingTerrain(boulder))?;
            let ground = terrain.height_at([point[0], point[2]]);
            // 00405B5E..00405C2C inline 00410F60 with its waves always on.
            let water = terrain.water_surface_height(
                TerrainWaterSurfaceRequest {
                    xz: [point[0], point[2]],
                    wave_clock: clock,
                    world_flags: 0x40,
                    terrain_height: ground,
                },
                sine_quarter,
            );
            if water > point[1] as i16 {
                BoulderParticle::Splash
            } else {
                BoulderParticle::Dust
            }
        };
        host.particle(kind, point, thrown)
            .map_err(Selector59ContactError::Host)?;
        if (chance - (chance >> 31)) >> 1 <= roll {
            return Ok(());
        }
        let half_radius = i32::from(self.object(boulder)?.base.word_28 >> 1);
        let span = half_radius.wrapping_mul(2);
        for word in &mut point {
            let draw = (host.random().map_err(Selector59ContactError::Host)? & 0xffff) as i32;
            let jitter = half_radius.wrapping_sub(draw.wrapping_mul(span) >> 16);
            *word = word.wrapping_add(jitter as u16);
        }
        host.particle(kind, point, thrown)
            .map_err(Selector59ContactError::Host)
    }

    /// 00405E30, the boulder's +A4(other, &own_saved, &other_saved): with
    /// +144 or +145 set, its change of speed since `own_saved` past 320 (or
    /// any, with +145) damages the partner (0047C6E0) by
    /// `isqrt(excess) * excess >> 4`, times its mass +8C `>> 6`, at most
    /// 0xFFFF, on channel 3 from its handle. The packet's BYTE +3 is the
    /// high byte of the reused argument slot, which the port leaves
    /// unwritten.
    pub fn selector59_crush(
        &self,
        boulder: ObjectId,
        own_saved: [u16; 3],
    ) -> Result<Option<ActorDamagePacket>, ObjectError> {
        let state = self.selector59(boulder)?;
        if state.crushes_144 == 0 && state.crushes_any_145 == 0 {
            return Ok(None);
        }
        let threshold = if state.crushes_any_145 != 0 {
            0
        } else {
            CRUSH_SPEED
        };
        let actor = self.actor(boulder)?;
        let change: [i32; 3] = std::array::from_fn(|axis| {
            i32::from(own_saved[axis].wrapping_sub(actor.words_8e_92[axis]) as i16)
        });
        let square = change[0]
            .wrapping_mul(change[0])
            .wrapping_add(change[1].wrapping_mul(change[1]))
            .wrapping_add(change[2].wrapping_mul(change[2]));
        let speed = length(square);
        if speed <= threshold {
            return Ok(None);
        }
        let excess = speed.wrapping_sub(threshold);
        let damage = (isqrt_48efc0(excess) as i32).wrapping_mul(excess) >> 4;
        let damage = (i32::from(actor.word_8c).wrapping_mul(damage) >> 6).min(0xffff);
        Ok(Some(ActorDamagePacket {
            strength: damage as u16 as i16,
            channel: CRUSH_CHANNEL,
            byte_03: None,
        }))
    }
}

#[cfg(test)]
#[path = "selector59_tests.rs"]
mod tests;
