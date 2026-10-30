//! Selectors 28, 25 and 68, the missiles: 00455560's class with final
//! tables 005259E8 (28, resources 112 to 114: the ACAM's weapon row 4),
//! 00525B88 (25, resources 110, 127 and 128: row 5 and selector 11's
//! launchers) and 00526398 (68, resource 129: row 7). They share the light
//! (+28, 00457EE0), the draw (+2C, 00457620), the messages (+98, 00456AD0),
//! the contacts (+A4 00455640, +A8 00455660) and the lifecycle (+AC
//! 00456B10, +B0 00455680). Selector 28's update (+24, 00457370) holds the
//! missile about 50 above the ground beneath and ahead of it, flies it, runs
//! the fuse (004555B0) and lays its exhaust ribbon (selector 67). Selector
//! 25's (00458510) first homes on a type-2 target, its launcher's or one a
//! scan finds, then runs 28's.

use std::convert::Infallible;

use thiserror::Error;

use super::{BehaviorResourceClass, Family455560State, Family455560Tail};
use crate::object_runtime::actor::derived::kinematics::ftol;
use crate::object_runtime::{
    ActorHandle, ActorMotionError, ActorPhysicsRequest, FollowTargetPoint, FollowTargetRequest,
    NativeHeapRef, ObjectArena, ObjectError, ObjectId,
};
use crate::terrain_runtime::TerrainRuntime;
use inf_formats::native_basis::{
    inverse_sine_q31, isqrt_48efc0, normalize_words, q31_divide, NativeBasisMath,
    NativeLengthPrecision, RotationPlane,
};

/// 00457449/00457450: the height the missile holds above the ground, and
/// the gap below which it steers toward it.
const HOLD_HEIGHT: i32 = 50;
const STEER_BELOW: i32 = 200;
/// 0045851F/00458525: selector 25's speed limits without a slow payload.
const TOP_SPEED: i32 = 3500;
const LOW_SPEED: i32 = 1500;
/// 004585EC: closer than this, a target falling behind detonates it.
const CLOSE: i32 = 150;
/// 00458741/00458861: the prediction's horizon in milliseconds.
const HORIZON: i32 = 2000;
/// 00475EC0's flags and its XZ reach for the target scan, 2000 squared.
const SCAN_FLAGS: u32 = 0x24;
const SCAN_REACH_SQUARED: u32 = 0x3d_0900;
/// 00456B28: the world flash of a viewed missile's state 1.
const FLASH_RADIUS: u32 = 0xf00;

/// The missiles' lifecycle services: the state change 0047D670 and the
/// detonation's effects.
pub trait MissileLifecycleHost {
    type Error;
    /// 0047D670(0, source, 0) on the missile, whose states 1 and 0 come back
    /// to [`ObjectArena::missile_state_one`] and
    /// [`ObjectArena::detonate_missile`].
    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 004887D0(block, &position, owner, 1), the detonation's radius effect.
    fn burst(
        &mut self,
        arena: &mut ObjectArena,
        missile: ObjectId,
        block: [u8; 10],
        owner: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0048A0F0(2, position, 0, 0), the explosion.
    fn explosion(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 00487D70(position, 0xF00, 0), the flash of a viewed missile.
    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

/// The missiles' update services.
pub trait MissileHost: MissileLifecycleHost {
    /// H12/0 through 0055EE38, for the heights (00450A30).
    fn terrain(&mut self, arena: &mut ObjectArena) -> Result<&TerrainRuntime, Self::Error>;
    /// 00486600 with the missile's handle: a new ribbon, or none.
    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// 004869B0 on the ribbon with a point.
    fn extend_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// Settings +7C (through 00463AB0), which the target refresh 00482690
    /// reads.
    fn settings_7c(&self) -> u32;
    /// The inverse sine table 0053FF00 that 0048F520 reads.
    fn inverse_sine(&self) -> &[i16; 1024];
    /// The process null 0054E894, which the scan's evaluator keeps at +8.
    fn process_null(&self) -> Option<ActorHandle>;
    /// 00487FE0: a live persistent sound stops.
    fn release_sound(
        &mut self,
        reference: crate::process_sound::ProcessSoundRef,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum MissileError<E = Infallible> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error("missile {missile:?} is not a selector-25, 28 or 68 missile")]
    NotMissile { missile: ObjectId },
    #[error("missile {missile:?} reads its unwritten {field}")]
    Unwritten {
        missile: ObjectId,
        field: &'static str,
    },
    /// 004583ED: a launcher whose +58 is not 00404610 (itself).
    #[error("missile {missile:?}'s launcher {owner:?} has no ported +58")]
    Launcher { missile: ObjectId, owner: ObjectId },
    /// 00458571: the ribbon +14C no longer exists.
    #[error("missile {missile:?}'s ribbon {ribbon:?} is gone")]
    Ribbon { missile: ObjectId, ribbon: ObjectId },
    #[error("missile {missile:?} inverse sine at {address:08X} failed")]
    InverseSine { missile: ObjectId, address: u32 },
    #[error("missile target clone (004828D0): {0}")]
    Target(String),
    /// 0047DFD0, the status visual of a missile another machine fired.
    #[error("remote missile status visual: {0}")]
    Status(String),
    #[error("missile service failed: {0}")]
    Host(E),
}

pub(super) type MissileResult<T, E> = Result<T, MissileError<E>>;

impl MissileError {
    /// The service-free errors in any host's error type.
    fn widen<E>(self) -> MissileError<E> {
        match self {
            MissileError::Object(error) => MissileError::Object(error),
            MissileError::Motion(error) => MissileError::Motion(error),
            MissileError::NotMissile { missile } => MissileError::NotMissile { missile },
            MissileError::Unwritten { missile, field } => {
                MissileError::Unwritten { missile, field }
            }
            MissileError::Launcher { missile, owner } => MissileError::Launcher { missile, owner },
            MissileError::Ribbon { missile, ribbon } => MissileError::Ribbon { missile, ribbon },
            MissileError::InverseSine { missile, address } => {
                MissileError::InverseSine { missile, address }
            }
            MissileError::Target(reason) => MissileError::Target(reason),
            MissileError::Status(reason) => MissileError::Status(reason),
            MissileError::Host(never) => match never {},
        }
    }
}

/// One homing step's interval and the sine table its turns read.
struct HomingStep<'a> {
    delta: u32,
    sine_quarter: &'a [u16; 4096],
}

/// A light 0045B1A0 splats into the grid at the missile's X/Z; its fourth
/// colour byte is the stack's residue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissileLight {
    pub position_xz: [i32; 2],
    pub radius: i32,
    pub rgb: [u8; 3],
}

/// The tail's ribbon, sound and selector 25's homing fields.
pub(super) struct MissileTail<'a> {
    pub(super) ribbon: &'a mut Option<ObjectId>,
    pub(super) sound: &'a mut Option<crate::process_sound::ProcessSoundRef>,
    homing: Option<Homing<'a>>,
}

struct Homing<'a> {
    distance: &'a mut Option<u16>,
    speed: &'a mut u16,
    slow: &'a mut u8,
    target: &'a mut Option<NativeHeapRef>,
}

/// The squared length of signed WORDs with wrapping sums, then 0048EFC0:
/// a sum with bit 31 set is quartered before the root and doubled after.
fn length(words: [u16; 3]) -> i32 {
    let squared = words.iter().fold(0_i32, |sum, &word| {
        let word = i32::from(word as i16);
        sum.wrapping_add(word.wrapping_mul(word))
    });
    if squared < 0 {
        (isqrt_48efc0(((squared as u32) >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(squared) as i32
    }
}

/// `x / 1000` by the reciprocal 0x10624DD3, truncating toward zero.
fn per_mille(value: i32) -> i32 {
    let high = ((i64::from(value) * 0x1062_4dd3) >> 32) as i32;
    let quotient = high >> 6;
    quotient.wrapping_add((quotient as u32 >> 31) as i32)
}

impl ObjectArena {
    /// A missile: selectors 25, 28 and 68 on 00455560's family.
    pub(crate) fn missile(&self, object: ObjectId) -> Result<&Family455560State, ObjectError> {
        let state = self.behavior_resource(object)?;
        match &state.class {
            BehaviorResourceClass::Family455560(family)
                if matches!(state.selector, 25 | 28 | 68) =>
            {
                Ok(family)
            }
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    pub(super) fn missile_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Family455560State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            crate::object_runtime::ActorExtension::BehaviorResource(state)
                if matches!(state.selector, 25 | 28 | 68) =>
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

    /// What the guided view `00456CF0` reads of a missile: whether BYTE
    /// +150 is set, and the fuse +13A.
    pub fn missile_view_state(&self, missile: ObjectId) -> Result<(bool, u16), ObjectError> {
        let state = self.missile(missile)?;
        let disrupted = match &state.tail {
            Family455560Tail::Zeroed14c { byte_150, .. }
            | Family455560Tail::Selector25 { byte_150, .. } => *byte_150 != 0,
            _ => return Err(ObjectError::NotBehaviorResource(missile)),
        };
        Ok((disrupted, state.word_13a))
    }

    pub fn is_missile(&self, object: ObjectId) -> bool {
        self.missile(object).is_ok()
    }

    /// Selector 25, the homing missile.
    pub fn is_missile25(&self, object: ObjectId) -> bool {
        self.behavior_resource(object)
            .is_ok_and(|state| state.selector == 25)
            && self.is_missile(object)
    }

    pub(super) fn missile_tail<E>(
        &mut self,
        missile: ObjectId,
    ) -> MissileResult<MissileTail<'_>, E> {
        match &mut self.missile_mut(missile)?.tail {
            Family455560Tail::Zeroed14c {
                ribbon_14c,
                sound_154,
                ..
            } => Ok(MissileTail {
                ribbon: ribbon_14c,
                sound: sound_154,
                homing: None,
            }),
            Family455560Tail::Selector25 {
                ribbon_14c,
                sound_154,
                word_158,
                word_15a,
                byte_15e,
                target_160,
                ..
            } => Ok(MissileTail {
                ribbon: ribbon_14c,
                sound: sound_154,
                homing: Some(Homing {
                    distance: word_158,
                    speed: word_15a,
                    slow: byte_15e,
                    target: target_160,
                }),
            }),
            _ => Err(MissileError::NotMissile { missile }),
        }
    }

    fn missile_homing<E>(&mut self, missile: ObjectId) -> MissileResult<Homing<'_>, E> {
        self.missile_tail::<E>(missile)?
            .homing
            .ok_or(MissileError::NotMissile { missile })
    }

    /// `00458ADD..00458C04`, once `00458A50` has built a missile (H13 0x6E):
    /// it stands at `position`, and with no target yet (+160) it follows
    /// `target` (004825D0), refreshed (+18), facing that target's point
    /// (00490370 with the world's up 0055F280): the offset over its spilled
    /// length, +X when zero. Its first update (+24 with 0) is the caller's.
    pub fn place_missile25(
        &mut self,
        missile: ObjectId,
        position: [u16; 3],
        target: ActorHandle,
        settings_7c: u32,
    ) -> MissileResult<(), Infallible> {
        self.owned_mut(missile)?.state.base.position_words = Some(position);
        if self.missile_homing::<Infallible>(missile)?.target.is_some() {
            return Ok(());
        }
        let follow = self.follow_target_resource(FollowTargetRequest {
            handle: target,
            point: FollowTargetPoint::ResolveActor,
        })?;
        *self.missile_homing::<Infallible>(missile)?.target = follow;
        let Some(follow) = follow else {
            return Ok(());
        };
        let mut native = self.native_target(follow)?;
        let refreshed = self.refresh_native_target(&mut native, settings_7c);
        self.write_native_target(follow, native)?;
        refreshed?;
        let crate::object_runtime::NativeTarget::ActorFollow(data) = self.native_target(follow)?
        else {
            return Err(MissileError::Target(
                "004825D0 built no follow target".into(),
            ));
        };
        let point = data.focus_08.ok_or(MissileError::Unwritten {
            missile,
            field: "target point +08",
        })?;
        let offset = std::array::from_fn(|axis| point[axis].wrapping_sub(position[axis]) as i16);
        let forward = normalize_words(offset, NativeLengthPrecision::Spilled);
        let basis = &mut self.actor_mut(missile)?.rotating.basis;
        inf_formats::native_basis::orient_from_forward(basis, forward, [0.0, 1.0, 0.0], false);
        Ok(())
    }

    pub(super) fn missile_position<E>(&self, missile: ObjectId) -> MissileResult<[u16; 3], E> {
        self.object(missile)?
            .base
            .position_words
            .ok_or(MissileError::Unwritten {
                missile,
                field: "position",
            })
    }

    /// 00457370, selector 28's +24 and the tail of 25's and 68's.
    pub fn tick_missile_base<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        // 00457379..0045738B: a live +154, which the steered flight started
        // for a missile its player let go, stops (00487FE0).
        if let Some(reference) = self.missile_tail::<H::Error>(missile)?.sound.take() {
            host.release_sound(reference).map_err(MissileError::Host)?;
        }
        // 00457396..004574A3: the gap to the ground beneath and a quarter of
        // the velocity ahead, the smaller, steers the vertical speed.
        let [x, y, z] = self.missile_position::<H::Error>(missile)?;
        let velocity = self.actor(missile)?.words_8e_92;
        let (here, ahead) = {
            let terrain = host.terrain(self).map_err(MissileError::Host)?;
            let quarter = |word: u16| ((word as i16) >> 2) as u16;
            (
                i32::from(terrain.height_at([x, z])),
                i32::from(terrain.height_at([
                    x.wrapping_add(quarter(velocity[0])),
                    z.wrapping_add(quarter(velocity[2])),
                ])),
            )
        };
        let height = i32::from(y as i16);
        let clearance = height.wrapping_sub(here);
        let lookahead = (i32::from(velocity[1] as i16) >> 2)
            .wrapping_add(height)
            .wrapping_sub(ahead);
        let gap = clearance.min(lookahead);
        if gap < STEER_BELOW {
            let gap = gap.wrapping_sub(HOLD_HEIGHT);
            let step = (delta >> 5) as i32;
            let adjust = if gap > 0 {
                gap.min((step.wrapping_mul(400) >> 15) as i16 as i32)
            } else {
                gap.max((step.wrapping_mul(-1000) >> 15) as i16 as i32)
            };
            let speed = &mut self.actor_mut(missile)?.words_8e_92[1];
            *speed = speed.wrapping_sub(adjust as u16);
        }
        // 004574A3: 0047B1E0 with the position alone.
        self.tick_actor_physics(
            missile,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 8,
                sine_quarter,
            },
        )?;
        // 004574AD..004574F3: below the ground, the fuse is spent.
        let [x, y, z] = self.missile_position::<H::Error>(missile)?;
        let ground = host
            .terrain(self)
            .map_err(MissileError::Host)?
            .height_at([x, z]);
        if ground > y as i16 {
            self.missile_mut(missile)?.word_13a = 0;
        }
        self.missile_fuse(missile, delta, rng, host)?;
        self.missile_ribbon(missile, delta, host)
    }

    /// `00457504..004575CE`, and the remote `00457660`'s after `0047EBF0`:
    /// the exhaust ribbon `+14C` reaches the missile, or a missile without
    /// one starts it (`00486600` with its handle).
    pub(super) fn missile_ribbon<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        let ribbon = *self.missile_tail::<H::Error>(missile)?.ribbon;
        if let Some(ribbon) = ribbon {
            let position = self.missile_position::<H::Error>(missile)?;
            return host
                .extend_ribbon(self, ribbon, delta, position)
                .map_err(MissileError::Host);
        }
        let handle = self.actor(missile)?.handle;
        let Some(ribbon) = host
            .create_ribbon(self, handle)
            .map_err(MissileError::Host)?
        else {
            return Ok(());
        };
        *self.missile_tail::<H::Error>(missile)?.ribbon = Some(ribbon);
        // The first three points, from a quarter of the velocity behind to
        // the missile in eighths.
        let position = self.missile_position::<H::Error>(missile)?;
        let velocity = self.actor(missile)?.words_8e_92;
        let mut point: [u16; 3] = std::array::from_fn(|axis| {
            position[axis].wrapping_sub(((i32::from(velocity[axis] as i16) * 2) >> 3) as u16)
        });
        host.extend_ribbon(self, ribbon, delta, point)
            .map_err(MissileError::Host)?;
        for _ in 0..2 {
            let velocity = self.actor(missile)?.words_8e_92;
            for axis in 0..3 {
                point[axis] = point[axis].wrapping_add(((velocity[axis] as i16) >> 3) as u16);
            }
            let ribbon =
                self.missile_tail::<H::Error>(missile)?
                    .ribbon
                    .ok_or(MissileError::Unwritten {
                        missile,
                        field: "ribbon +14C",
                    })?;
            host.extend_ribbon(self, ribbon, delta, point)
                .map_err(MissileError::Host)?;
        }
        Ok(())
    }

    /// 004555B0: the fuse runs down by the delta's milliseconds and the age
    /// rises; at zero the missile goes to state 0 (0047D670).
    pub(super) fn missile_fuse<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        let step = delta >> 10;
        let state = self.missile_mut(missile)?;
        state.word_13a = if u32::from(state.word_13a) < step {
            0
        } else {
            state.word_13a.wrapping_sub(step as u16)
        };
        state.word_13c = state.word_13c.wrapping_add(step as u16);
        if state.word_13a == 0 {
            let source = self.actor(missile)?.handle;
            host.detonate(self, missile, source, rng)
                .map_err(MissileError::Host)?;
        }
        Ok(())
    }

    /// 00458510, selector 25's +24.
    pub fn tick_missile25<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        let mut top = TOP_SPEED;
        let mut low = LOW_SPEED;
        let speed = i32::from(*self.missile_homing::<H::Error>(missile)?.speed as i16);
        if self.missile_homing::<H::Error>(missile)?.target.is_none() {
            *self.missile_homing::<H::Error>(missile)?.distance = Some(0x7fff);
            self.acquire_missile25_target(missile, host)?;
        }
        let mut desired = speed;
        if let Some(target) = *self.missile_homing::<H::Error>(missile)?.target {
            desired = self.home_missile25(
                missile,
                target,
                HomingStep {
                    delta,
                    sine_quarter,
                },
                &mut low,
                &mut top,
                host,
            )?;
        }
        // 004589AF..004589EB: the desired speed within its limits, then the
        // speed eases toward it.
        let desired = if desired < low {
            low
        } else if desired > top {
            top
        } else {
            desired
        };
        let homing = self.missile_homing::<H::Error>(missile)?;
        let difference = desired.wrapping_sub(i32::from(*homing.speed as i16));
        if difference != 0 {
            let eased = (((delta >> 4) as i32).wrapping_mul(difference) >> 15)
                .wrapping_add(i32::from(*homing.speed as i16));
            *homing.speed = eased as u16;
        }
        // 004589EB..00458A31: the velocity along the forward row.
        let speed = f64::from(i32::from(
            *self.missile_homing::<H::Error>(missile)?.speed as i16,
        ));
        let basis = self.actor(missile)?.rotating.basis;
        let forward = |axis: usize| f64::from(f32::from_bits(basis.words[7 + axis]));
        let velocity: [u16; 3] = std::array::from_fn(|axis| ftol(speed * forward(axis)) as u16);
        self.actor_mut(missile)?.words_8e_92 = velocity;
        self.tick_missile_base(missile, delta, sine_quarter, rng, host)
    }

    /// 0045855B..004589AB with a target: the refresh, the distance and
    /// closing speed, the predicted point, the lead height and the turn.
    /// Returns the desired speed and may narrow the limits.
    fn home_missile25<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        target: NativeHeapRef,
        step: HomingStep<'_>,
        low: &mut i32,
        top: &mut i32,
        host: &mut H,
    ) -> MissileResult<i32, H::Error> {
        let HomingStep {
            delta,
            sine_quarter,
        } = step;
        self.refresh_follow_target_resource(target, host.settings_7c())?;
        let data = self.actor_follow_target(target)?;
        let unwritten = |field| MissileError::Unwritten { missile, field };
        let focus = data.focus_08.ok_or(unwritten("target focus +08"))?;
        let target_velocity = data.velocity_1a.ok_or(unwritten("target velocity +1A"))?;
        let position = self.missile_position::<H::Error>(missile)?;
        let mut offset: [u16; 3] =
            std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]));
        let distance = length(offset);
        // 004585EC..00458613: close and falling behind, the fuse is spent.
        // The last distance decides only a close missile: one 00458A50 built
        // (which never writes +158) starts 1500 out.
        let homing = self.missile_homing::<H::Error>(missile)?;
        let close = distance < CLOSE
            && distance
                >= i32::from(homing.distance.ok_or(unwritten("last distance +158"))? as i16);
        *homing.distance = Some(distance as u16);
        if close {
            self.missile_mut(missile)?.word_13a = 0;
        }
        // 0045861A..00458684: the closing speed, the relative velocity along
        // the normalized line of sight.
        let velocity = self.actor(missile)?.words_8e_92;
        let relative: [i16; 3] =
            std::array::from_fn(|axis| velocity[axis].wrapping_sub(target_velocity[axis]) as i16);
        let sight = normalize_words(
            offset.map(|word| word as i16),
            NativeLengthPrecision::Retained,
        );
        let closing = ftol(
            (f64::from(relative[2]) * f64::from(sight[2])
                + f64::from(relative[1]) * f64::from(sight[1]))
                + f64::from(relative[0]) * f64::from(sight[0]),
        );
        // 00458686..0045870B: a slow missile (+15E) narrows its limits.
        if *self.missile_homing::<H::Error>(missile)?.slow != 0 {
            let speed = i32::from(*self.missile_homing::<H::Error>(missile)?.speed as i16);
            *low = speed.wrapping_sub(closing).wrapping_add(200).max(200);
            let age = self.missile(missile)?.word_13c;
            let young = i32::from(age < 2000);
            *top = if young.wrapping_sub(closing).wrapping_add(speed) == 0 {
                500
            } else {
                700 - i32::from(age) / 10
            };
            if low.wrapping_add(200) > *top {
                *top = low.wrapping_add(200);
            }
        }
        // 0045870D..0045873B: the time to the target in milliseconds.
        let time = if distance < closing.wrapping_mul(2) {
            let time = distance.wrapping_mul(1000) / closing;
            if time > HORIZON {
                Some(HORIZON)
            } else if time <= 50 {
                None
            } else {
                Some(time)
            }
        } else {
            Some(HORIZON)
        };
        let mut reach = distance;
        if let Some(time) = time {
            // 00458741..00458829: the target where its velocity takes it.
            let predicted: [u16; 3] = std::array::from_fn(|axis| {
                let lead = per_mille(i32::from(target_velocity[axis] as i16).wrapping_mul(time));
                (lead as u16).wrapping_add(focus[axis])
            });
            offset = std::array::from_fn(|axis| predicted[axis].wrapping_sub(position[axis]));
            reach = length(offset);
        }
        let time = time.unwrap_or(0);
        // 0045882F..0045885C: past half a second, aim above it.
        if time > 500 {
            let lift = if time < 2500 { (time - 500) >> 3 } else { 250 };
            let lift = lift.min(reach / 2);
            offset[1] = offset[1].wrapping_add(lift as u16);
        }
        // 00458861..004588BC: the desired speed.
        let desired = if time < HORIZON {
            let basis = self.actor(missile)?.rotating.basis;
            let forward = |axis: usize| f64::from(f32::from_bits(basis.words[7 + axis]));
            let along = ftol(
                (f64::from(target_velocity[1] as i16) * forward(1)
                    + f64::from(target_velocity[2] as i16) * forward(2))
                    + f64::from(target_velocity[0] as i16) * forward(0),
            );
            along.wrapping_add(reach).wrapping_add(400)
        } else {
            *top
        };
        // 004588C0..004589AB: the turn toward the offset in the missile's
        // frame (0047A660), yaw and pitch each at most a sixteenth of the
        // delta, the smaller halved.
        let local = self
            .actor(missile)?
            .rotating
            .basis
            .inverse_words(offset.map(|word| word as i16));
        let limit = (delta >> 4) as i32;
        let table = host.inverse_sine();
        let angle = |component: i16, behind: bool, address: u32| -> MissileResult<i32, H::Error> {
            let ratio = q31_divide(i32::from(component) as u32, reach as u32) as i32;
            let mut angle = inverse_sine_q31(ratio, table)
                .map_err(|_| MissileError::InverseSine { missile, address })?;
            if behind {
                angle = if angle > 0 { angle + 0x4000 } else { -0x4000 };
            }
            Ok(angle.clamp(-limit, limit))
        };
        let yaw = angle(local[0], local[2] < 0, 0x0045_88fa)?;
        let pitch = angle(local[1], local[2] < 0, 0x0045_8937)?;
        let math = NativeBasisMath::new(*sine_quarter);
        if pitch.abs() < yaw.abs() {
            self.rotate_visual(missile, RotationPlane::Xz, yaw, &math)?;
            self.rotate_visual(missile, RotationPlane::Yz, -(pitch / 2), &math)?;
        } else {
            self.rotate_visual(missile, RotationPlane::Yz, -pitch, &math)?;
            self.rotate_visual(missile, RotationPlane::Xz, yaw / 2, &math)?;
        }
        Ok(desired)
    }

    /// 004583A0: the old target goes. The launcher's +58 holder's target
    /// (+D0: +148) is cloned (004828D0) when it is a type-2 target on
    /// neither the missile nor an actor of the missile's owner; otherwise a
    /// scan (00475EC0 with 00458070's evaluator) builds a type-2 target
    /// (004825D0) on the best candidate.
    fn acquire_missile25_target<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        if let Some(old) = self.missile_homing::<H::Error>(missile)?.target.take() {
            self.release_actor_service(old)?;
        }
        let (handle, owner_handle) = {
            let actor = self.actor(missile)?;
            (actor.handle, actor.owner_handle)
        };
        let owner = self
            .resolve_actor_handle(owner_handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        if let Some(owner) = owner {
            let holder = self.missile_holder::<H::Error>(missile, owner)?;
            if let Some(held) = self.derived_actor(holder)?.target_148 {
                if let crate::object_runtime::NativeTarget::ActorFollow(data) =
                    self.native_target(held)?
                {
                    let target_actor = self
                        .resolve_actor_handle(data.handle_04)
                        .map_err(|_| ObjectError::MissingActorRegistry)?;
                    let inherits = data.word_10 == 2
                        && data.handle_04 != handle
                        && match target_actor {
                            Some(actor) => self.actor(actor)?.owner_handle != owner_handle,
                            None => false,
                        };
                    if inherits {
                        let clone = self
                            .clone_native_target(
                                held,
                                crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
                            )
                            .map_err(|error| MissileError::Target(error.to_string()))?;
                        *self.missile_homing::<H::Error>(missile)?.target = clone;
                    }
                }
            }
        }
        if self.missile_homing::<H::Error>(missile)?.target.is_some() {
            return Ok(());
        }
        if let Some(found) = self.missile25_scan(missile, host)? {
            let handle = self.actor(found)?.handle;
            let target = self.follow_target_resource(FollowTargetRequest {
                handle,
                point: FollowTargetPoint::ResolveActor,
            })?;
            *self.missile_homing::<H::Error>(missile)?.target = target;
        }
        Ok(())
    }

    /// The launcher's +58: the vehicle's and selector 11's 00404610 answer
    /// themselves.
    fn missile_holder<E>(&self, missile: ObjectId, owner: ObjectId) -> MissileResult<ObjectId, E> {
        let answers_itself = self.selector2_actor(owner).is_ok()
            || self.derived_actor(owner).is_ok_and(|derived| {
                matches!(
                    derived.class,
                    crate::object_runtime::DerivedActorClass::Selector11(_)
                )
            });
        if answers_itself {
            Ok(owner)
        } else {
            Err(MissileError::Launcher { missile, owner })
        }
    }

    /// 004580B0 for `candidate`, over its ground cell's height.
    fn missile25_score<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        candidate: ObjectId,
        host: &mut H,
    ) -> MissileResult<u32, H::Error> {
        let [cx, _, cz] = self.missile_position::<H::Error>(candidate)?;
        let ground = host
            .terrain(self)
            .map_err(MissileError::Host)?
            .raw_height_at([cx, cz]);
        let process_null = host.process_null().ok_or(MissileError::Unwritten {
            missile,
            field: "the process null 0054E894",
        })?;
        self.missile25_candidate_score(missile, candidate, ground, process_null)
            .map_err(MissileError::widen)
    }

    /// 004580B0, the scan's score for `candidate` whose ground cell stands
    /// at `ground`: nothing below lifecycle 2, for the launcher or on the
    /// missile's side; nothing over 128 above or below its ground without
    /// +15C, nor under 128 without +15D unless it is a vehicle in mode 4 or
    /// 5; nothing behind the missile or outside 60 degrees of its forward
    /// row. Otherwise 0x7FFFFFFF less the lateral square, a quarter of it
    /// off for the evaluator's +8 (the process null).
    pub(crate) fn missile25_candidate_score(
        &self,
        missile: ObjectId,
        candidate: ObjectId,
        ground: i16,
        process_null: ActorHandle,
    ) -> MissileResult<u32, Infallible> {
        let (lifecycle, candidate_handle, side) = {
            let actor = self.actor(candidate)?;
            (actor.word_c4 as i32, actor.handle, actor.word_b4)
        };
        let (owner, own_side) = {
            let actor = self.actor(missile)?;
            (actor.owner_handle, actor.word_b4)
        };
        if lifecycle <= 1 || candidate_handle == owner || side == own_side {
            return Ok(0);
        }
        let [cx, cy, cz] = self.missile_position(candidate)?;
        let altitude = i32::from(ground)
            .wrapping_sub(i32::from(cy as i16))
            .wrapping_abs();
        let (air, low) = match &self.missile(missile)?.tail {
            Family455560Tail::Selector25 {
                byte_15c, byte_15d, ..
            } => (*byte_15c, *byte_15d),
            _ => return Err(MissileError::NotMissile { missile }),
        };
        if air == 0 && altitude > 0x80 {
            return Ok(0);
        }
        if low == 0 && altitude < 0x80 {
            // 00458177: the candidate's +64 vehicle view in mode 4 or 5. Only
            // the vehicle's table answers (00404610); every other actor
            // table holds the null 00401300.
            let mode = self
                .selector2_actor(candidate)
                .ok()
                .map(|vehicle| vehicle.dwords_1e8_1f4[0]);
            if !matches!(mode, Some(4 | 5)) {
                return Ok(0);
            }
        }
        let position = self.missile_position(missile)?;
        let offset = [
            i32::from(cx.wrapping_sub(position[0]) as i16),
            i32::from(cy.wrapping_sub(position[1]) as i16),
            i32::from(cz.wrapping_sub(position[2]) as i16),
        ];
        let basis = self.actor(missile)?.rotating.basis;
        let forward = |axis: usize| f64::from(f32::from_bits(basis.words[7 + axis]));
        let ahead = ftol(
            (f64::from(offset[1]) * forward(1) + f64::from(offset[2]) * forward(2))
                + f64::from(offset[0]) * forward(0),
        );
        if ahead <= 0 {
            return Ok(0);
        }
        let squared = offset[0]
            .wrapping_mul(offset[0])
            .wrapping_add(offset[1].wrapping_mul(offset[1]))
            .wrapping_add(offset[2].wrapping_mul(offset[2]));
        // 004581FD..00458216: the high DWORD of squared * 2^30.
        let quarter = ((i64::from(squared) << 30) >> 32) as i32;
        if (ahead.wrapping_mul(ahead) as u32) <= quarter as u32 {
            return Ok(0);
        }
        let mut lateral = squared.wrapping_sub(ahead.wrapping_mul(ahead));
        if candidate_handle == process_null {
            lateral = lateral.wrapping_sub(((lateral as u32) >> 2) as i32);
        }
        Ok(0x7fff_ffff_u32.wrapping_sub(lateral as u32))
    }

    /// 00475EC0 with the missile's evaluator: the actor list from G+48, each
    /// actor with both scan flags nearer than 2000 in XZ (unsigned squares)
    /// and not the missile, keeping the strictly best score.
    fn missile25_scan<H: MissileHost>(
        &mut self,
        missile: ObjectId,
        host: &mut H,
    ) -> MissileResult<Option<ObjectId>, H::Error> {
        let [x, _, z] = self.missile_position::<H::Error>(missile)?;
        let mut best = None;
        let mut best_score = 0_u32;
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & SCAN_FLAGS != SCAN_FLAGS {
                continue;
            }
            let [cx, _, cz] = self.missile_position::<H::Error>(candidate)?;
            let dx = i32::from(x.wrapping_sub(cx) as i16);
            let dz = i32::from(z.wrapping_sub(cz) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= SCAN_REACH_SQUARED || candidate == missile {
                continue;
            }
            let score = self.missile25_score(missile, candidate, host)?;
            if score > best_score {
                best_score = score;
                best = Some(candidate);
            }
        }
        Ok(best)
    }

    /// 00457EE0, the missiles' +28: within 1500 of the ground cell beneath,
    /// red up to 20, green and blue `(5h - 7500) / 1500` (below zero, so
    /// their bytes wrap), as wide as two thirds of the height and at least
    /// 256.
    pub fn missile_light(
        &self,
        missile: ObjectId,
        terrain: &TerrainRuntime,
    ) -> MissileResult<Option<MissileLight>, Infallible> {
        self.missile(missile)?;
        let [x, y, z] = self.missile_position(missile)?;
        let above = i32::from(y as i16).wrapping_sub(i32::from(terrain.raw_height_at([x, z])));
        if above >= 1500 {
            return Ok(None);
        }
        let red = (1500_i32.wrapping_sub(above).wrapping_mul(20) / 1500) as u8;
        let blue = (above.wrapping_mul(5).wrapping_sub(7500) / 1500) as u8;
        let radius = (above.wrapping_mul(500) / 750).max(0x100);
        Ok(Some(MissileLight {
            position_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius,
            rgb: [red, blue, blue],
        }))
    }

    /// 00456AD0, the missiles' +98: events 0xB and 0xC set +150 and answer
    /// 1; otherwise 00455740: events 5 and 9 spend the fuse and answer 1.
    pub fn missile_message(&mut self, missile: ObjectId, event: u32) -> Result<u32, ObjectError> {
        if matches!(event, 0xb | 0xc) {
            match &mut self.missile_mut(missile)?.tail {
                Family455560Tail::Zeroed14c { byte_150, .. }
                | Family455560Tail::Selector25 { byte_150, .. } => *byte_150 = 1,
                _ => return Err(ObjectError::NotBehaviorResource(missile)),
            }
            return Ok(1);
        }
        if matches!(event, 5 | 9) {
            self.missile_mut(missile)?.word_13a = 0;
            return Ok(1);
        }
        self.missile(missile)?;
        Ok(0)
    }

    /// 00455610, the missiles' +A0 as the grenade's: the launcher is no
    /// contact for the first 500 ms.
    pub fn missile_ignores(&self, missile: ObjectId, other: ObjectId) -> Result<bool, ObjectError> {
        let age = self.missile(missile)?.word_13c;
        Ok(age < 500 && self.actor(missile)?.owner_handle == self.actor(other)?.handle)
    }

    /// 00455640 (+A4) and 00455660 (+A8, which answers 0 and stops there):
    /// an armed missile that touches anything detonates (0047D670(0, +84,
    /// 0)).
    pub fn missile_touch<H: MissileLifecycleHost>(
        &mut self,
        missile: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        if self.missile(missile)?.byte_138 == 0 {
            return Ok(());
        }
        let source = self.actor(missile)?.handle;
        host.detonate(self, missile, source, rng)
            .map_err(MissileError::Host)
    }

    /// 00456B10, the missiles' +AC: a viewed missile (+D0) flashes the
    /// world, then it goes to state 0 (0047D670(0, +84, 0)).
    pub fn missile_state_one<H: MissileLifecycleHost>(
        &mut self,
        missile: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.missile(missile)?;
        if self.actor(missile)?.byte_d0 != 0 {
            let position = self.missile_position::<H::Error>(missile)?;
            host.flash(self, position, FLASH_RADIUS, rng)
                .map_err(MissileError::Host)?;
        }
        let source = self.actor(missile)?.handle;
        host.detonate(self, missile, source, rng)
            .map_err(MissileError::Host)
    }

    /// 00455680, the missiles' +B0: unless retired (flag 0x80000000), the
    /// radius effect of its own block from its owner, then the explosion
    /// (0048A0F0(2, position, 0, 0)) and the disposal.
    pub fn detonate_missile<H: MissileLifecycleHost>(
        &mut self,
        missile: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.missile(missile)?;
        self.detonate_family455560(missile, rng, host)
    }

    /// 00455680 on any class of the family whose table binds it: the
    /// missiles' +B0, and the tail of the Bomb's.
    pub(super) fn detonate_family455560<H: MissileLifecycleHost>(
        &mut self,
        missile: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        let family = |arena: &ObjectArena| match &arena.behavior_resource(missile)?.class {
            BehaviorResourceClass::Family455560(family) => Ok(family.bytes_13e),
            BehaviorResourceClass::Family485f50(_) => {
                Err(ObjectError::NotBehaviorResource(missile))
            }
        };
        family(self)?;
        if self.object(missile)?.base.flags & 0x8000_0000 == 0 {
            let block = family(self)?.ok_or(MissileError::Unwritten {
                missile,
                field: "radius block +13E",
            })?;
            let owner = self.actor(missile)?.owner_handle;
            host.burst(self, missile, block, owner, rng)
                .map_err(MissileError::Host)?;
        }
        let position = self.missile_position::<H::Error>(missile)?;
        host.explosion(self, position, rng)
            .map_err(MissileError::Host)?;
        self.dispose(missile)?;
        Ok(())
    }

    /// 004582D0's deletion of a live +160 target, before 00456A70.
    pub(in crate::object_runtime) fn release_missile25_target(
        &mut self,
        missile: ObjectId,
    ) -> Result<(), ObjectError> {
        let target = match &mut self.missile_mut(missile)?.tail {
            Family455560Tail::Selector25 { target_160, .. } => target_160.take(),
            _ => return Err(ObjectError::NotBehaviorResource(missile)),
        };
        if let Some(target) = target {
            self.release_actor_service(target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "missile_tests.rs"]
mod tests;
