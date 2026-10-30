//! Selector 26, the grenade: 00455560's class with final table 005268AC,
//! built from resources 119, 120, 123, 124 and 126 (the ACAM's weapon rows
//! 8, 9, 10, 14 and 15). Its update (+24, 00455E10) flies it (004559E0):
//! gravity, a bounce off the ground and a settle, a spin in the air, drag
//! and lift in water. Payload flag 1 trails smoke (004558B0), flag 4 burns
//! (004887D0 every 0.2 s after 5 s), and the fuse (004555B0) detonates it
//! through the state change 0047D670: state 1's +AC (00458F80), then state
//! 0's +B0 (004564B0). A detonated grenade with charges left (+14D) stays
//! spent, throwing a spark every 100 units of its timer until they run out
//! (00456350). Its light (+28, 00456560) brightens the ground beneath it.
//! Its draw is the common 0047D910, whose model word 0xFFFF hides it.

use std::convert::Infallible;

use thiserror::Error;

use super::{BehaviorResourceClass, Family455560State, Family455560Tail};
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::actor::derived::kinematics::ftol;
use crate::object_runtime::{
    mark_flag_change, ActorAngularResponseRequest, ActorContactError, ActorHandle,
    ActorMotionError, ActorPhysicsRequest, ActorTerrainHost, ObjectArena, ObjectError, ObjectId,
};
use crate::process_particles::births::ParticleBurst;
use crate::terrain_runtime::TerrainRuntime;
use inf_formats::native_basis::{NativeBasisMath, RotationPlane};

/// 00455E40: the spent timer's step and each spark's share of it.
const SPARK_STEP: u16 = 100;
/// 00455ED2: the burn's period after its first 5 s.
const BURN_PERIOD_MICROS: u32 = 200_000;
/// 00455EE4..00455EFE: the burn's radius block.
pub const BURN_BLOCK: [u8; 10] = [0xf4, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00];
/// 0045595B/00455994: the alert 004558B0 sends aliens near a fizzing
/// grenade (event 0xD to team 0x12), first within 800, then within 1200.
pub const ALERT_EVENT: u32 = 0xd;
pub const ALERT_TEAM: u32 = 0x12;
/// 004564D2: flag 2's chain broadcast, event 0xB within 2000.
pub const CHAIN_EVENT: u32 = 0xb;
const CHAIN_RADIUS: u32 = 2000;
/// 00458F87: the fuse state 1 sets.
const STATE_ONE_FUSE: u16 = 5000;
/// 00458D5D..00458D83: the default burst's debris.
const DEFAULT_BURST: ParticleBurst = ParticleBurst {
    debris_count: 0x10,
    position: [0; 3],
    placement_radius: 0x14,
    central_type: 1,
    debris_types: [1, 1],
    speed: 8,
    flags: 0,
};
/// 00455C5A: the scale (00520CC0, 256.0) of the up row's Y that decides
/// a settle.
const SETTLE_SCALE: f32 = 256.0;

/// The grenade's services.
pub trait Grenade26Host {
    type Error;
    /// 0055F154, the particle load factor the smoke reads.
    fn load_factor(&self) -> u32;
    /// H12/0 through 0055EE38, for the height (00450A30) and the ground
    /// normal (0047A2F0).
    fn terrain(&mut self, arena: &mut ObjectArena) -> Result<&TerrainRuntime, Self::Error>;
    /// H12/0's water plane, `[0055EE3C]->[0] >> [005483BC]`.
    fn water_level(&mut self, arena: &mut ObjectArena) -> Result<i16, Self::Error>;
    /// 00489D90(0x0B, position, velocity, 0x800), a smoke puff.
    fn smoke(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0047DBF0(radius, event, 0, team) from the grenade.
    fn alert(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        radius: u32,
        event: u32,
        team: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 004887D0(block, &position, owner, 1): the burn's block, or the
    /// grenade's own +13E at detonation.
    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 004665D0(1, position, velocity, owner), a spent grenade's spark.
    fn spark(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// The grenade's +4C (0047AAF0) for `resource` with no position: the
    /// default burst's visual.
    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        resource: u16,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0048A8B0.
    fn debris(
        &mut self,
        arena: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0047DD20(radius, event, data) from the grenade.
    fn broadcast(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        radius: u32,
        event: u32,
        data: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0048A0F0(1, position, 0, 0), the explosion.
    fn explosion(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0047D670(0, source, 0) on the grenade, whose state 1 and 0 come back
    /// to [`ObjectArena::grenade26_state_one`] and
    /// [`ObjectArena::detonate_grenade26`].
    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// Settings +7C (through 00463AB0), which the mine's target refresh
    /// 00482690 reads.
    fn settings_7c(&self) -> u32;
    /// 004887D0(block, &point, owner, 1) from a point on the stack: the
    /// flat mine's burst above itself.
    fn burst_at(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        point: [u16; 3],
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0047DD40(owner, 0, event, 0) from `sender`: its owner's +98, when
    /// the owner's handle resolves.
    fn owner_message(
        &mut self,
        arena: &mut ObjectArena,
        sender: ObjectId,
        event: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0047DFD0, the burn's status visual.
    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        delta: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Grenade26Error<E = Infallible> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error("grenade {grenade:?} is not a selector-26 grenade")]
    NotGrenade { grenade: ObjectId },
    #[error("grenade {grenade:?} reads its unwritten {field}")]
    Unwritten {
        grenade: ObjectId,
        field: &'static str,
    },
    #[error("grenade ground normal: {0}")]
    Normal(String),
    /// 0047DFD0, the status visual of a grenade another machine threw.
    #[error("remote grenade status visual: {0}")]
    Status(String),
    #[error("grenade service failed: {0}")]
    Host(E),
}

impl<E> From<ActorContactError<E, Infallible>> for Grenade26Error<E> {
    fn from(error: ActorContactError<E, Infallible>) -> Self {
        match error {
            ActorContactError::Terrain(error) => Grenade26Error::Host(error),
            ActorContactError::Object(error) => Grenade26Error::Object(error),
            ActorContactError::MissingPosition(grenade) => Grenade26Error::Unwritten {
                grenade,
                field: "position",
            },
            ActorContactError::Motion(error) => Grenade26Error::Motion(error),
            ActorContactError::Table(error) => Grenade26Error::Normal(error.to_string()),
            ActorContactError::Header(never) => match never {},
            ActorContactError::RequiredHeaderByte4 => {
                Grenade26Error::Normal("the H14 header's byte 4".into())
            }
            ActorContactError::RequiredNativeStackComparison => {
                Grenade26Error::Normal("the native stack comparison".into())
            }
        }
    }
}

/// A light 0045B1A0 splats into the grid at the grenade's X/Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grenade26Light {
    pub position_xz: [i32; 2],
    pub radius: i32,
    pub rgb: [u8; 3],
}

type GrenadeResult<T, E> = Result<T, Grenade26Error<E>>;

/// 0047A2F0's terrain through the grenade's host.
struct HostTerrain<'h, H>(&'h mut H);

impl<H: Grenade26Host> ActorTerrainHost for HostTerrain<'_, H> {
    type Error = H::Error;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.0.terrain(arena)
    }
}

/// A basis row's three floats (+48, +54 or +60).
fn basis_row(basis: &inf_formats::native_basis::NativeBasis, row: usize) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from_bits(basis.words[1 + row * 3 + axis]))
}

fn abs16(value: u16) -> i32 {
    (value as i16 as i32).abs()
}

/// `x * 7 / 8`, truncating toward zero.
fn seven_eighths(value: u16) -> u16 {
    (i32::from(value as i16).wrapping_mul(7) / 8) as u16
}

/// `x * 15 / 16`, truncating toward zero.
fn fifteen_sixteenths(value: u16) -> u16 {
    (i32::from(value as i16).wrapping_mul(15) / 16) as u16
}

impl ObjectArena {
    pub(crate) fn grenade26(&self, grenade: ObjectId) -> Result<&Family455560State, ObjectError> {
        match self.behavior_resource(grenade)?.selector {
            26 => self.thrown(grenade),
            _ => Err(ObjectError::NotBehaviorResource(grenade)),
        }
    }

    /// A grenade (26), a charge (27) or a mine (38): the classes whose
    /// update is 00455E10 (which the mine runs after its own prefix) or
    /// 00455F30 over the same flight and fuse. They share the light, the
    /// messages, the contacts but the charge's +A0, the lifecycle's states
    /// and the grenade's draw but the charge's.
    pub(crate) fn thrown(&self, object: ObjectId) -> Result<&Family455560State, ObjectError> {
        let state = self.behavior_resource(object)?;
        match &state.class {
            BehaviorResourceClass::Family455560(family)
                if matches!(state.selector, 26 | 27 | 38) =>
            {
                Ok(family)
            }
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    pub(in crate::object_runtime) fn thrown_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Family455560State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            crate::object_runtime::ActorExtension::BehaviorResource(state)
                if matches!(state.selector, 26 | 27 | 38) =>
            {
                match &mut state.class {
                    BehaviorResourceClass::Family455560(family) => Ok(family),
                    BehaviorResourceClass::Family485f50(_) => {
                        Err(ObjectError::NotBehaviorResource(object))
                    }
                }
            }
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    pub fn is_grenade26(&self, object: ObjectId) -> bool {
        self.grenade26(object).is_ok()
    }

    pub fn is_thrown(&self, object: ObjectId) -> bool {
        self.thrown(object).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn grenade26_set_fuse_for_tests(&mut self, grenade: ObjectId, fuse: u16) {
        self.thrown_mut(grenade).unwrap().word_13a = fuse;
    }

    #[cfg(test)]
    pub(crate) fn grenade26_set_owner_for_tests(&mut self, grenade: ObjectId, owner: ActorHandle) {
        self.actor_mut(grenade).unwrap().owner_handle = owner;
    }

    #[cfg(test)]
    pub(crate) fn set_position_for_tests(&mut self, object: ObjectId, position: [u16; 3]) {
        self.owned_mut(object).unwrap().state.base.position_words = Some(position);
    }

    /// The tail's bytes +14C..+14F and DWORD +150.
    pub(super) fn grenade26_tail<E>(
        &mut self,
        grenade: ObjectId,
    ) -> GrenadeResult<GrenadeTail<'_>, E> {
        match &mut self.thrown_mut(grenade)?.tail {
            Family455560Tail::Flags14c {
                byte_14c,
                byte_14d,
                byte_14e,
                byte_14f,
                dword_150,
                ..
            }
            | Family455560Tail::Selector38 {
                byte_14c,
                byte_14d,
                byte_14e,
                byte_14f,
                dword_150,
                ..
            } => Ok(GrenadeTail {
                flags: byte_14c.as_mut().ok_or(Grenade26Error::Unwritten {
                    grenade,
                    field: "payload flags +14C",
                })?,
                charges: byte_14d.as_mut().ok_or(Grenade26Error::Unwritten {
                    grenade,
                    field: "charges +14D",
                })?,
                detonate: byte_14e,
                resting: byte_14f,
                burn: dword_150,
            }),
            _ => Err(Grenade26Error::NotGrenade { grenade }),
        }
    }

    fn grenade26_flags<E>(&mut self, grenade: ObjectId) -> GrenadeResult<u8, E> {
        Ok(*self.grenade26_tail(grenade)?.flags)
    }

    pub(super) fn grenade26_position<E>(&self, grenade: ObjectId) -> GrenadeResult<[u16; 3], E> {
        self.object(grenade)?
            .base
            .position_words
            .ok_or(Grenade26Error::Unwritten {
                grenade,
                field: "position",
            })
    }

    /// 00455E10, the grenade's +24.
    pub fn tick_grenade26<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        self.grenade26(grenade)?;
        self.update_thrown(grenade, delta, sine_quarter, rng, host)
    }

    /// 00455E10 on a grenade or, after its prefix, a mine.
    pub(super) fn update_thrown<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        self.thrown(grenade)?;
        if self.actor(grenade)?.word_c4 == 0 {
            // 00455E21..00455E74: spent, a spark per 100 units of the timer.
            let state = self.thrown_mut(grenade)?;
            state.word_13a = state.word_13a.wrapping_add((delta >> 10) as u16);
            while self.thrown(grenade)?.word_13a > SPARK_STEP {
                let state = self.thrown_mut(grenade)?;
                state.word_13a = state.word_13a.wrapping_sub(SPARK_STEP);
                self.grenade26_spark(grenade, false, rng, host)?;
                let tail = self.grenade26_tail::<H::Error>(grenade)?;
                *tail.charges = tail.charges.wrapping_sub(1);
                if *tail.charges == 0 {
                    self.dispose(grenade)?;
                }
            }
            return Ok(());
        }
        self.grenade26_flight(grenade, delta, sine_quarter, host)?;
        if *self.grenade26_tail::<H::Error>(grenade)?.detonate != 0 {
            self.thrown_mut(grenade)?.word_13a = 0;
        }
        self.grenade26_smoke(grenade, rng, host)?;
        if self.grenade26_flags::<H::Error>(grenade)? & 4 != 0 {
            // 00455EAE..00455F0D: after 5 s, a burst every 0.2 s.
            let tail = self.grenade26_tail::<H::Error>(grenade)?;
            if (*tail.burn as i32) > delta as i32 {
                *tail.burn = tail.burn.wrapping_sub(delta);
            } else {
                *tail.burn = BURN_PERIOD_MICROS;
                let owner = self.actor(grenade)?.owner_handle;
                host.burst(self, grenade, BURN_BLOCK, owner, rng)
                    .map_err(Grenade26Error::Host)?;
            }
            host.status_visual(self, grenade, delta, rng)
                .map_err(Grenade26Error::Host)?;
        }
        self.grenade26_fuse(grenade, delta, rng, host)
    }

    /// 004559E0.
    pub(super) fn grenade26_flight<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        if *self.grenade26_tail::<H::Error>(grenade)?.resting != 0 {
            // 00455DCC: a resting grenade wakes at more than 8 on any axis.
            let velocity = self.actor(grenade)?.words_8e_92;
            if abs16(velocity[0]) > 8 || abs16(velocity[2]) > 8 || abs16(velocity[1]) > 8 {
                *self.grenade26_tail::<H::Error>(grenade)?.resting = 0;
            }
            return Ok(());
        }
        let mask = if self.grenade26_flags::<H::Error>(grenade)? & 0x20 != 0 {
            0x18
        } else {
            0x1f
        };
        self.tick_actor_physics(
            grenade,
            ActorPhysicsRequest {
                microseconds: delta,
                mask,
                sine_quarter,
            },
        )?;
        let [x, y, z] = self.grenade26_position(grenade)?;
        let height = host
            .terrain(self)
            .map_err(Grenade26Error::Host)?
            .height_at([x, z]);
        let size = self.object(grenade)?.base.word_28;
        let ground = i32::from(height) + i32::from(size >> 1);
        if ground > i32::from(y as i16) {
            return self.grenade26_bounce(grenade, ground, delta, host);
        }
        // 00455C95..00455CC9: the spin, about the basis's Y-Z plane.
        let velocity = self.actor(grenade)?.words_8e_92;
        let spin = abs16(velocity[2])
            .wrapping_add(abs16(velocity[1]))
            .wrapping_add(abs16(velocity[0]));
        let math = NativeBasisMath::new(*sine_quarter);
        math.rotate_in_plane(
            &mut self.actor_mut(grenade)?.rotating.basis,
            spin,
            RotationPlane::Yz,
        );
        // 00455CCE..00455DC9: below the water, drag and lift toward a depth
        // between 100 and 1000 that its own handle picks.
        let water = host.water_level(self).map_err(Grenade26Error::Host)?;
        let y = self.grenade26_position(grenade)?[1] as i16;
        if water <= y {
            return Ok(());
        }
        let handle_high = (self.actor(grenade)?.handle.raw() >> 16) as i32;
        let depth = ((handle_high & 0x3ff) % 10 + 1) * 100;
        let target =
            i32::from(host.water_level(self).map_err(Grenade26Error::Host)?).wrapping_sub(depth);
        let actor = self.actor_mut(grenade)?;
        let [vx, vy, vz] = actor.words_8e_92;
        actor.words_8e_92[0] = fifteen_sixteenths(vx);
        let lifted = seven_eighths(vy);
        actor.words_8e_92[1] = lifted;
        actor.words_8e_92[2] = fifteen_sixteenths(vz);
        let y = i32::from(y);
        if y < target {
            let lift = target.wrapping_sub(y).wrapping_mul((delta >> 3) as i32) >> 15;
            self.actor_mut(grenade)?.words_8e_92[1] = (lift as u16).wrapping_add(lifted);
        }
        Ok(())
    }

    /// 00455A57..00455C92: the grenade is in the ground.
    fn grenade26_bounce<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        ground: i32,
        delta: u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        if self.grenade26_flags::<H::Error>(grenade)? & 0x40 != 0 {
            *self.grenade26_tail::<H::Error>(grenade)?.detonate = 1;
        }
        let mut normal = [None; 3];
        self.actor_terrain_normal::<HostTerrain<'_, H>, Infallible>(
            grenade,
            &mut HostTerrain(host),
            &mut normal,
        )?;
        let normal = normal.map(|axis| axis.expect("0047A2F0 stores all three"));
        let [x, y, z] = self.grenade26_position(grenade)?;
        let depth = f64::from((ground.wrapping_sub(i32::from(y as i16)) / 2) as i16);
        let push = normal.map(|axis| ftol(depth * f64::from(axis)) as u16);
        let mut position = [
            x.wrapping_add(push[0]),
            y.wrapping_add(push[1]),
            z.wrapping_add(push[2]),
        ];
        if i32::from(position[1] as i16) < ground {
            position[1] = ground as u16;
        }
        self.owned_mut(grenade)?.state.base.position_words = Some(position);
        // The velocity's speed into the ground, then a 7/8 damping.
        let velocity = self.actor(grenade)?.words_8e_92;
        let dot = ftol(
            (f64::from(velocity[0] as i16 as f32) * f64::from(normal[0])
                + f64::from(velocity[1] as i16 as f32) * f64::from(normal[1]))
                + f64::from(velocity[2] as i16 as f32) * f64::from(normal[2]),
        );
        self.actor_mut(grenade)?.words_8e_92 = velocity.map(seven_eighths);
        let into = dot.wrapping_neg();
        if into > 0 {
            // 00455B73..00455BBB: one and a half times back out, along the normal.
            let back = f64::from((into.wrapping_mul(3) / 2) as i16);
            let impulse = normal.map(|axis| ftol(back * f64::from(axis)) as u16);
            self.add_actor_linear_velocity(grenade, impulse)?;
        }
        let actor = self.actor_mut(grenade)?;
        actor.words_8e_92[0] = seven_eighths(actor.words_8e_92[0]);
        actor.words_8e_92[2] = seven_eighths(actor.words_8e_92[2]);
        let current_axis = basis_row(&self.actor(grenade)?.rotating.basis, 1);
        self.apply_actor_angular_response(
            grenade,
            ActorAngularResponseRequest {
                current_axis,
                desired_axis: normal,
                gain: (delta >> 3) as i32,
            },
        )?;
        // 00455C18..00455C85: slow and upright, it settles.
        let velocity = self.actor(grenade)?.words_8e_92;
        if abs16(velocity[0]) < 30 && abs16(velocity[2]) < 30 && abs16(velocity[1]) < 100 {
            let up_y = basis_row(&self.actor(grenade)?.rotating.basis, 1)[1];
            if ftol(f64::from(up_y) * f64::from(SETTLE_SCALE)) > 0xf0 {
                self.actor_mut(grenade)?.words_8e_92 = [0; 3];
                *self.grenade26_tail::<H::Error>(grenade)?.resting = 1;
            }
        }
        Ok(())
    }

    /// 004558B0: with flag 1, smoke once older than a second, as the load
    /// factor allows, and a fizz that alerts the aliens near it past 1.5 s.
    pub(super) fn grenade26_smoke<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        if self.grenade26_flags(grenade)? & 1 == 0 || self.thrown(grenade)?.word_13c <= 1000 {
            return Ok(());
        }
        let quota = 0xc000_i32.wrapping_sub(host.load_factor() as i32) / 0x2000;
        let puff = quota < 2 || (name_entry_rng_next(rng) & 0xffff) as i32 % quota == 0;
        if puff {
            let velocity = [
                (name_entry_rng_next(rng) as u16) >> 9,
                (name_entry_rng_next(rng) as u16) >> 11,
                (name_entry_rng_next(rng) as u16) >> 9,
            ];
            let position = self.grenade26_position(grenade)?;
            host.smoke(self, position, velocity, rng)
                .map_err(Grenade26Error::Host)?;
        }
        if self.thrown(grenade)?.word_13c <= 1500 {
            return Ok(());
        }
        let tail = self.grenade26_tail::<H::Error>(grenade)?;
        if *tail.flags & 0x80 == 0 {
            *tail.flags |= 0x80;
            host.alert(self, grenade, 800, ALERT_EVENT, ALERT_TEAM, rng)
                .map_err(Grenade26Error::Host)?;
        } else if (name_entry_rng_next(rng) & 0xffff) % 16 == 0 {
            host.alert(self, grenade, 1200, ALERT_EVENT, ALERT_TEAM, rng)
                .map_err(Grenade26Error::Host)?;
        }
        Ok(())
    }

    /// 004555B0: the fuse runs down by the delta's milliseconds and the age
    /// rises; at zero the grenade goes to state 0 (0047D670).
    pub(super) fn grenade26_fuse<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        let step = delta >> 10;
        let state = self.thrown_mut(grenade)?;
        state.word_13a = if u32::from(state.word_13a) < step {
            0
        } else {
            state.word_13a.wrapping_sub(step as u16)
        };
        state.word_13c = state.word_13c.wrapping_add(step as u16);
        if state.word_13a == 0 {
            let source = self.actor(grenade)?.handle;
            host.detonate(self, grenade, source, rng)
                .map_err(Grenade26Error::Host)?;
        }
        Ok(())
    }

    /// 00456350: a spark from the grenade, ±500 on each axis (`wide`
    /// false, the spent timer's) or ±200/±600/±200, its position moved by a
    /// sixteenth of that velocity (only upward for the narrow one).
    fn grenade26_spark<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        narrow: bool,
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        if self.object(grenade)?.base.flags & 0x8000_0000 != 0 {
            return Ok(());
        }
        let mut position = self.grenade26_position(grenade)?;
        let mut draw = |range: i32| ((name_entry_rng_next(rng) & 0xffff) as i32 % range) as u16;
        let velocity = if narrow {
            let velocity = [
                draw(400).wrapping_sub(200),
                draw(1200).wrapping_sub(600),
                draw(400).wrapping_sub(200),
            ];
            position[1] = position[1].wrapping_add(sixteenth(velocity[1]));
            velocity
        } else {
            let velocity = [
                draw(1000).wrapping_sub(500),
                draw(1000).wrapping_sub(500),
                draw(1000).wrapping_sub(500),
            ];
            position[0] = position[0].wrapping_add(sixteenth(velocity[0]));
            position[2] = position[2].wrapping_add(sixteenth(velocity[2]));
            position[1] = position[1].wrapping_add(sixteenth(velocity[1]));
            velocity
        };
        let owner = self.actor(grenade)?.owner_handle;
        host.spark(self, position, velocity, owner, rng)
            .map_err(Grenade26Error::Host)
    }

    /// 00458F80, the grenade's +AC: state 1 arms it to detonate with a
    /// 5 s timer.
    pub fn grenade26_state_one(&mut self, grenade: ObjectId) -> Result<(), ObjectError> {
        let state = self.thrown_mut(grenade)?;
        state.word_13a = STATE_ONE_FUSE;
        match &mut state.tail {
            Family455560Tail::Flags14c { byte_14e, .. }
            | Family455560Tail::Selector38 { byte_14e, .. } => *byte_14e = 1,
            _ => return Err(ObjectError::NotBehaviorResource(grenade)),
        }
        Ok(())
    }

    /// 004564B0, the grenade's +B0: the default burst without payload
    /// flags, flag 2's chain broadcast, the radius effect of its own block,
    /// the explosion, then spent with charges left or disposed.
    pub fn detonate_grenade26<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        let flags = self.grenade26_flags(grenade)?;
        if flags == 0 {
            self.grenade26_default_burst(grenade, rng, host)?;
        }
        if self.grenade26_flags(grenade)? & 2 != 0 {
            host.broadcast(self, grenade, CHAIN_RADIUS, CHAIN_EVENT, CHAIN_RADIUS, rng)
                .map_err(Grenade26Error::Host)?;
        }
        if self.object(grenade)?.base.flags & 0x8000_0000 == 0 {
            let block = self
                .thrown(grenade)?
                .bytes_13e
                .ok_or(Grenade26Error::Unwritten {
                    grenade,
                    field: "radius block +13E",
                })?;
            let owner = self.actor(grenade)?.owner_handle;
            host.burst(self, grenade, block, owner, rng)
                .map_err(Grenade26Error::Host)?;
        }
        let position = self.grenade26_position(grenade)?;
        host.explosion(self, position, rng)
            .map_err(Grenade26Error::Host)?;
        if *self.grenade26_tail::<H::Error>(grenade)?.charges != 0 {
            self.thrown_mut(grenade)?.word_13a = 0;
            let base = &mut self.owned_mut(grenade)?.state.base;
            let flags = base.flags & 0xffff_fefb;
            mark_flag_change(base, flags);
        } else {
            self.dispose(grenade)?;
        }
        Ok(())
    }

    /// 00458D20: the +148 visual through the grenade's +4C, then sixteen
    /// pieces of debris at its position.
    pub(super) fn grenade26_default_burst<H: Grenade26Host>(
        &mut self,
        grenade: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> GrenadeResult<(), H::Error> {
        let resource = self.thrown(grenade)?.dword_148 as u16;
        host.spawn(self, grenade, resource, rng)
            .map_err(Grenade26Error::Host)?;
        let position = self.grenade26_position(grenade)?;
        host.debris(
            self,
            ParticleBurst {
                position,
                ..DEFAULT_BURST
            },
            rng,
        )
        .map_err(Grenade26Error::Host)
    }

    /// 00456560, the grenade's +28: within 1500 of the ground beneath it, a
    /// grey light of up to 20, as wide as two thirds of the height and at
    /// least 256.
    pub fn grenade26_light(
        &self,
        grenade: ObjectId,
        terrain: &TerrainRuntime,
    ) -> GrenadeResult<Option<Grenade26Light>, Infallible> {
        self.thrown(grenade)?;
        let [x, y, z] = self.grenade26_position(grenade)?;
        let height = terrain.raw_height_at([x, z]);
        let above = i32::from(y as i16).wrapping_sub(i32::from(height));
        if above >= 1500 {
            return Ok(None);
        }
        let level = (1500_i32.wrapping_sub(above).wrapping_mul(20) / 1500) as u8;
        let radius = (above.wrapping_mul(500) / 750).max(0x100);
        Ok(Some(Grenade26Light {
            position_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius,
            rgb: [level; 3],
        }))
    }

    /// 004557F0, the grenade's +98(sender, distance², event, data): event 9
    /// marks it to detonate (+14E), and so does event 10 from a null sender
    /// or one whose handle (+84) is the grenade's owner (+88). It answers 1
    /// when it marked itself, 0 otherwise.
    pub fn grenade26_message(
        &mut self,
        grenade: ObjectId,
        sender: Option<ObjectId>,
        event: u32,
    ) -> Result<u32, ObjectError> {
        self.thrown(grenade)?;
        let marks = match (event, sender) {
            (9, _) | (10, None) => true,
            (10, Some(sender)) => self.actor(sender)?.handle == self.actor(grenade)?.owner_handle,
            _ => false,
        };
        if marks {
            match &mut self.thrown_mut(grenade)?.tail {
                Family455560Tail::Flags14c { byte_14e, .. }
                | Family455560Tail::Selector38 { byte_14e, .. } => *byte_14e = 1,
                _ => return Err(ObjectError::NotBehaviorResource(grenade)),
            }
        }
        Ok(u32::from(marks))
    }

    /// 00455610, the grenade's +A0: its owner is no contact for its first
    /// 500 ms.
    pub fn grenade26_ignores(
        &self,
        grenade: ObjectId,
        other: ObjectId,
    ) -> Result<bool, ObjectError> {
        let age = self.thrown(grenade)?.word_13c;
        Ok(age < 500 && self.actor(grenade)?.owner_handle == self.actor(other)?.handle)
    }

    /// 00455830, the grenade's +A4: without payload flag 1, touching an
    /// actor whose +68 is null detonates it; with flag 8 only one of
    /// another team (+B4). `other_answers_68` is the partner's +68 result.
    pub fn grenade26_touch(
        &mut self,
        grenade: ObjectId,
        other: ObjectId,
        other_answers_68: bool,
    ) -> Result<(), ObjectError> {
        let flags = self
            .grenade26_flags::<Infallible>(grenade)
            .map_err(|_| ObjectError::NotBehaviorResource(grenade))?;
        if flags & 1 != 0 || other_answers_68 {
            return Ok(());
        }
        if flags & 8 != 0 && self.actor(other)?.word_b4 == self.actor(grenade)?.word_b4 {
            return Ok(());
        }
        let tail = self
            .grenade26_tail::<Infallible>(grenade)
            .map_err(|_| ObjectError::NotBehaviorResource(grenade))?;
        *tail.detonate = 1;
        Ok(())
    }

    /// 00455880, the grenade's +A8 before the shared 0047BFD0: with payload
    /// flag 0x40, hitting furniture detonates it.
    pub fn grenade26_impact(&mut self, grenade: ObjectId) -> Result<(), ObjectError> {
        let tail = self
            .grenade26_tail::<Infallible>(grenade)
            .map_err(|_| ObjectError::NotBehaviorResource(grenade))?;
        if *tail.flags & 0x40 != 0 {
            *tail.detonate = 1;
        }
        Ok(())
    }
}

/// The tail's fields the grenade and the mine share.
pub(super) struct GrenadeTail<'a> {
    pub(super) flags: &'a mut u8,
    pub(super) charges: &'a mut u8,
    pub(super) detonate: &'a mut u8,
    pub(super) resting: &'a mut u8,
    pub(super) burn: &'a mut u32,
}

fn sixteenth(value: u16) -> u16 {
    (i32::from(value as i16) / 16) as u16
}

#[cfg(test)]
#[path = "grenade26_tests.rs"]
mod tests;
