//! Selector 61, the Pursuit Ship: case 0x3D of 004603B0 allocates 0x150
//! bytes for the constructor 004128C0 (the shared 0047A7A0, then its own
//! fields and the final table 00520700). The table is the shared actor's
//! 00527C00 but for +00 (00412970 over the destructor 00412990), +10 (the
//! custom vertex 00413060), +24 (the update 00412A10), +40 (00413120, which
//! the Boulders share), +7C (00413130, its weapon behavior +138) and +AC
//! (the death 00412FA0).
//!
//! Selector-60 spawners make it in Artoria (world 11) and the Defence
//! Satellite (16). Without a target it does nothing but roll, one update in
//! four (0048F060 < 0x4000), for the first player's unit (0046B030): within
//! 2000 of it and ahead, inside the cone |x|, |y| < z of its own frame, the
//! unit becomes its follow target (004825D0). From then on it flies at that
//! target: within 2000 it matches the target's forward speed and closes on
//! it by twice the gap past its keep distance +144, between 50 and 2600;
//! farther off it holds +14C. Once the target is dead it climbs at 2600. It
//! turns at most delta >> 4 in yaw and pitch, halving the larger, banks to
//! and fro by 500 and rolls to that bank, and dies (0047CE90) where the
//! ground rises above it. Within 750 and ahead, its weapon ready, it fires
//! (0048D340).

use inf_formats::native_basis::{
    inverse_sine_q15, inverse_sine_q31, isqrt_48efc0, q31_divide, NativeBasisMath, RotationPlane,
};
use thiserror::Error;

use super::{
    derived::kinematics::ftol, ActorBehaviorGroup, ActorBehaviorState, ActorBehaviorTable,
    ActorConstructionError, ActorConstructionRequest, ActorExtension, ActorMotionError,
    ActorPhysicsRequest, ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode,
    ActorWeaponFireRequest,
};
use crate::object_runtime::{
    FollowTargetRequest, NativeHeapRef, ObjectArena, ObjectError, ObjectId,
};
use crate::process_particles::births::ParticleBurst;

/// 00412A4D: an update without a target takes one when the draw's low WORD
/// is below this, one in four.
const ACQUIRE_BELOW: u16 = 0x4000;
/// 00412AB1: the squared reach within which the unit becomes the target.
const ACQUIRE_SQUARED: u32 = 4_000_000;
/// 00412BAB: nearer than this the ship matches its target's speed.
const NEAR: i32 = 2000;
/// 00412C1F: the target speed past which the keep distance grows.
const FAST: i32 = 1500;
/// 00412C39 and 00412C60: the top speed, and the climb's.
const TOP_SPEED: i32 = 2600;
/// 00412C69: the lowest speed near a target.
const LOW_SPEED: i32 = 50;
/// 00412D05 and 00412CC0: the turn toward a target behind.
const BEHIND_TURN: i32 = 0x2000;
/// 00412DFC: the bank swings between plus and minus this.
const BANK_LIMIT: i32 = 500;
/// 00412F1B: nearer than this it may fire.
const FIRE_RANGE: i32 = 750;
/// 00412F41: an entry at this pressure (+1C) or more cannot fire.
const FIRE_PRESSURE: u32 = 6_000_000;
/// 0047B1E0's mask 8 (00412EB2): position integration alone.
const PHYSICS_MASK: u8 = 8;
/// 00412939: the weapon behavior's table.
const BEHAVIOR_TABLE: ActorBehaviorTable = ActorBehaviorTable::Selector61;
/// 0041294B and 00412952: the behavior's +04 and +08.
const BEHAVIOR_WORDS: (u32, u32) = (10_000, 150_000);
/// 00412FAB: the death's visual, through +4C (0047AAF0).
pub const PURSUIT_SHIP_VISUAL: u32 = 0x37;
/// 00413028: the death's radius block.
pub const PURSUIT_SHIP_BLAST: u32 = 0x0053_1250;
/// 00413038: the death's explosion type (0048A0F0).
pub const PURSUIT_SHIP_EXPLOSION: u32 = 1;

/// 00412FC1..00413002: sixteen type-1 debris round a type-1 centre within
/// 20, speed 8, no flags.
fn death_burst(position: [u16; 3]) -> ParticleBurst {
    ParticleBurst {
        debris_count: 0x10,
        position,
        placement_radius: 0x14,
        central_type: 1,
        debris_types: [1, 1],
        speed: 8,
        flags: 0,
    }
}

/// 004128C0's fields after 0047A7A0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector61State {
    /// +138, 0048CC70(2, 1, 0, 0052F748) with +04 = 10000, +08 = 150000 and
    /// +0C = 1.
    pub behavior_138: Option<ActorBehaviorState>,
    /// BYTE +13C: the bank swings up while it is set. The constructor leaves
    /// it to allocator residue (a fresh-heap carry); the update first reads it.
    pub bank_up_13c: Option<u8>,
    /// +140, the bank, between -500 and 500 or a step past either.
    pub bank_140: i32,
    /// +144, (0048F060 & 0x7F) + 0x50: the gap it keeps to its target.
    pub keep_144: u32,
    /// +148, the follow target on the first player's unit (0x20 bytes).
    pub target_148: Option<NativeHeapRef>,
    /// WORD +14C, the speed it holds far from its target: 0.
    pub speed_14c: u16,
}

/// What the update reaches beyond the arena.
pub trait Selector61Host {
    type Error: std::fmt::Display;
    /// 00450A30 on H12/0, acquired through 0055EE38 if need be.
    fn ground_height(&mut self, arena: &mut ObjectArena, xz: [u16; 2]) -> Result<i16, Self::Error>;
    /// 0047CE90 on the ship: the ground rose above it.
    fn retire(&mut self, arena: &mut ObjectArena, ship: ObjectId) -> Result<(), Self::Error>;
    /// The process RNG word, its tables and velocity, which 0048F060 and a
    /// shot (0048D340) read.
    fn firing(&mut self) -> ActorWeaponFireContext<'_>;
}

/// The update's world inputs.
#[derive(Clone, Copy)]
pub struct Selector61UpdateRequest<'a> {
    pub microseconds: u32,
    /// Settings +7C, which the target refresh 00482690 reads.
    pub settings_7c: u32,
    /// 0053FF00, which 0048F520 and 0048F4D0 read.
    pub inverse_sine: &'a [i16; 1024],
    pub sine_quarter: &'a [u16; 4096],
}

/// What the death reaches, in order.
pub trait Selector61DeathHost {
    type Error;
    /// `+4C(0, resource, 0)`, the shared child factory `0047AAF0`.
    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        ship: ObjectId,
        resource: u32,
    ) -> Result<(), Self::Error>;
    /// `0048A8B0`, a particle burst.
    fn burst(&mut self, arena: &mut ObjectArena, burst: ParticleBurst) -> Result<(), Self::Error>;
    /// World `004887D0(block, &+32, +84, 1)` from the ship's live position.
    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        ship: ObjectId,
        block_address: u32,
        source: crate::object_runtime::ActorHandle,
    ) -> Result<(), Self::Error>;
    /// `0048A0F0(type, &+32, 0, 0)`, the explosion.
    fn explosion(
        &mut self,
        arena: &mut ObjectArena,
        particle_type: u32,
        position: [u16; 3],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector61Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Physics(#[from] ActorMotionError),
    #[error(transparent)]
    Fire(#[from] ActorWeaponFireError),
    #[error(transparent)]
    Tick(#[from] crate::object_runtime::ActorBehaviorTickError),
    #[error("Pursuit Ship {ship:?} reads its unwritten +{offset:03X} at {address:08X}")]
    Unwritten {
        ship: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("Pursuit Ship {ship:?} reaches 0048F520 outside its table at {address:08X}")]
    InverseSine { ship: ObjectId, address: u32 },
    #[error("Pursuit Ship {ship:?} service at {address:08X}: {error}")]
    Host {
        ship: ObjectId,
        address: u32,
        error: E,
    },
}

type Result61<T, E> = Result<T, Selector61Error<E>>;

impl ObjectArena {
    /// Case 0x3D: 0x150 bytes, 0047A7A0, then +140 = 0, +144 from one draw,
    /// +148 = 0, WORD +14C = 0 and the weapon behavior +138.
    pub fn allocate_selector61(
        &mut self,
        request: ActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let keep_144 = (crate::name_entry::name_entry_rng_next(rng) & 0x7f) + 0x50;
        let bank_up_13c = self
            .actor_allocation
            .initial_dword(super::allocation::ActorAllocationDwordCarry::Selector61Bank13c)
            .map(|carry| carry as u8);
        let (groups, entries, extra) = (2_u8, 1_u8, 0_u8);
        let behavior = ActorBehaviorState {
            fire_head_00: None,
            firing_stop_00: None,
            sound_28: None,
            word_04: BEHAVIOR_WORDS.0,
            word_08: BEHAVIOR_WORDS.1,
            byte_0c: 1,
            byte_0d: groups,
            byte_0e: entries,
            byte_0f: extra,
            byte_10: 0,
            byte_11: entries,
            table_14: BEHAVIOR_TABLE,
            entries_18: vec![[u32::MAX, 0, 0, 0, 0, 0, 0, 0]; usize::from(entries + extra)],
            groups_1c: vec![
                ActorBehaviorGroup {
                    coordinates_00_0a: [None; 6],
                    word_0c: 4,
                };
                usize::from(groups)
            ],
            word_20: 0,
            words_24: vec![None; usize::from(groups)],
            word_2c: 0,
            word_30: None,
            word_34: None,
            word_38: 0,
        };
        self.actor_mut(object)?.extension = ActorExtension::Selector61(Box::new(Selector61State {
            behavior_138: Some(behavior),
            bank_up_13c,
            bank_140: 0,
            keep_144,
            target_148: None,
            speed_14c: 0,
        }));
        Ok(object)
    }

    pub fn selector61(&self, ship: ObjectId) -> Result<&Selector61State, ObjectError> {
        match &self.actor(ship)?.extension {
            ActorExtension::Selector61(state) => Ok(state),
            _ => Err(ObjectError::NotSelector61(ship)),
        }
    }

    pub(in crate::object_runtime) fn selector61_mut(
        &mut self,
        ship: ObjectId,
    ) -> Result<&mut Selector61State, ObjectError> {
        match &mut self.actor_mut(ship)?.extension {
            ActorExtension::Selector61(state) => Ok(state),
            _ => Err(ObjectError::NotSelector61(ship)),
        }
    }

    fn selector61_position<E>(&self, ship: ObjectId) -> Result61<[u16; 3], E> {
        Ok(self
            .object(ship)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(ship))?)
    }

    /// 00412A10, the ship's +24.
    pub fn tick_selector61<H: Selector61Host>(
        &mut self,
        ship: ObjectId,
        request: Selector61UpdateRequest<'_>,
        host: &mut H,
    ) -> Result61<(), H::Error> {
        let delta = request.microseconds;
        let mut speed = i32::from(self.selector61(ship)?.speed_14c as i16);
        if self.selector61(ship)?.target_148.is_none() {
            self.acquire_selector61_target(ship, host)?;
        }
        let Some(target) = self.selector61(ship)?.target_148 else {
            return Ok(());
        };
        let position = self.selector61_position(ship)?;
        // 00412B30: the target's +18 refreshes it and answers its +C4.
        let (offset, distance) =
            if (self.refresh_follow_target_resource(target, request.settings_7c)? as i32) <= 0 {
                speed = TOP_SPEED;
                ([0, NEAR as u16, 0], NEAR)
            } else {
                let followed = self.actor_follow_target(target)?;
                let focus = followed.focus_08.ok_or(Selector61Error::Unwritten {
                    ship,
                    offset: 0x148,
                    address: 0x0041_2b51,
                })?;
                let offset: [u16; 3] =
                    std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]));
                let [x, y, z] = offset.map(|word| i32::from(word as i16));
                let squared = x
                    .wrapping_mul(x)
                    .wrapping_add(y.wrapping_mul(y))
                    .wrapping_add(z.wrapping_mul(z)) as u32;
                let distance = if squared & 0x8000_0000 != 0 {
                    (isqrt_48efc0((squared >> 2) as i32) as i32).wrapping_shl(1)
                } else {
                    isqrt_48efc0(squared as i32) as i32
                };
                if distance < NEAR {
                    speed = self.selector61_chase_speed(ship, &followed, distance)?;
                }
                (offset, distance)
            };
        // 00412C7F: the offset in the ship's frame.
        let local = self
            .actor(ship)?
            .rotating
            .basis
            .inverse_words(offset.map(|word| word as i16));
        let limit = (delta >> 4) as i32;
        let turn = |component: i16, address: u32| -> Result61<i32, H::Error> {
            let ratio = q31_divide(i32::from(component) as u32, distance as u32) as i32;
            let mut angle = inverse_sine_q31(ratio, request.inverse_sine)
                .map_err(|_| Selector61Error::InverseSine { ship, address })?;
            if local[2] < 0 {
                angle = if angle > 0 {
                    angle.wrapping_add(BEHIND_TURN)
                } else {
                    -BEHIND_TURN
                };
            }
            Ok(if angle > limit {
                limit
            } else if angle < limit.wrapping_neg() {
                limit.wrapping_neg()
            } else {
                angle
            })
        };
        let yaw = turn(local[0], 0x0041_2ca6)?;
        let pitch = turn(local[1], 0x0041_2ceb)?;
        let math = NativeBasisMath::new(*request.sine_quarter);
        // 00412D2D: the larger of the two turns goes in at half.
        if pitch.wrapping_abs() > yaw.wrapping_abs() {
            self.rotate_visual(ship, RotationPlane::Xz, yaw, &math)?;
            self.rotate_visual(ship, RotationPlane::Yz, (pitch / 2).wrapping_neg(), &math)?;
        } else {
            self.rotate_visual(ship, RotationPlane::Xz, yaw / 2, &math)?;
            self.rotate_visual(ship, RotationPlane::Yz, pitch.wrapping_neg(), &math)?;
        }
        // 00412D62..00412DCD: the forward speed rises toward the wanted one,
        // or drops to it at once.
        let forward = {
            let actor = self.actor(ship)?;
            let f = |word: usize| f64::from(f32::from_bits(actor.rotating.basis.words[word]));
            let [vx, vy, vz] = actor.words_8e_92.map(|word| f64::from(word as i16));
            ftol((vz * f(9) + vy * f(8)) + vx * f(7))
        };
        let mut current = forward;
        if current != speed {
            let gap = speed.wrapping_sub(current).wrapping_shl(12);
            let doubled = i64::from(delta as i32)
                .wrapping_mul(i64::from(gap))
                .wrapping_shl(1);
            let mut step = (doubled >> 32) as i32;
            if step < 0 {
                step >>= 10;
            }
            current = current.wrapping_add(step);
            if current > speed {
                current = speed;
            }
        }
        // 00412DD0..00412E2F: the bank swings by delta >> 4 times 500 / 32768.
        let swing = i32::from(((limit.wrapping_mul(500)) >> 15) as i16);
        let up = self
            .selector61(ship)?
            .bank_up_13c
            .ok_or(Selector61Error::Unwritten {
                ship,
                offset: 0x13c,
                address: 0x0041_2dd0,
            })?;
        let state = self.selector61_mut(ship)?;
        if up != 0 {
            state.bank_140 = state.bank_140.wrapping_add(swing);
            if state.bank_140 > BANK_LIMIT {
                state.bank_up_13c = Some(0);
            }
        } else {
            state.bank_140 = state.bank_140.wrapping_sub(swing);
            if state.bank_140 < -BANK_LIMIT {
                state.bank_up_13c = Some(1);
            }
        }
        let bank = state.bank_140;
        // 00412E36..00412E73: the roll to the bank, less the roll it has.
        let wanted = bank.wrapping_shl(12) / 500;
        let rolled = {
            let word = self.actor(ship)?.rotating.basis.words[2];
            inverse_sine_q15(
                ftol(f64::from(f32::from_bits(word)) * 32768.0),
                request.inverse_sine,
            )
        };
        self.rotate_visual(ship, RotationPlane::Xy, wanted.wrapping_sub(rolled), &math)?;
        // 00412E78..00412EB7: the bank, its sixteenth and the speed, in the
        // ship's frame, are the velocity; then the position moves.
        let local_velocity = [
            bank as u16,
            (bank >> 4).wrapping_abs() as u16,
            current as u16,
        ];
        let velocity = self.transform_visual_vector_words(ship, local_velocity)?;
        self.actor_mut(ship)?.words_8e_92 = velocity;
        self.tick_actor_physics(
            ship,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: PHYSICS_MASK,
                sine_quarter: request.sine_quarter,
            },
        )?;
        // 00412EBC..00412EF9: X and Z before the terrain, Y after it.
        let [x, _, z] = self.selector61_position(ship)?;
        let ground = host
            .ground_height(self, [x, z])
            .map_err(|error| Selector61Error::Host {
                ship,
                address: 0x0041_2eec,
                error,
            })?;
        if ground > self.selector61_position::<H::Error>(ship)?[1] as i16 {
            host.retire(self, ship)
                .map_err(|error| Selector61Error::Host {
                    ship,
                    address: 0x0041_2ef9,
                    error,
                })?;
        }
        // 00412EFE..00412F7D: the weapon.
        if self.selector61(ship)?.behavior_138.is_none() {
            return Ok(());
        }
        self.tick_actor_behavior(ship, delta)?;
        if local[2] <= 0 || distance >= FIRE_RANGE {
            return Ok(());
        }
        let ready = {
            let behavior = self
                .selector61(ship)?
                .behavior_138
                .as_ref()
                .expect("checked above");
            let entry = behavior
                .entries_18
                .get(behavior.byte_10 as i8 as usize)
                .ok_or(Selector61Error::Unwritten {
                    ship,
                    offset: 0x138,
                    address: 0x0041_2f33,
                })?;
            entry[5] == 0 && entry[0] != 0 && entry[7] < FIRE_PRESSURE
        };
        let [lx, ly, _] = local.map(i32::from);
        let across = (lx.wrapping_mul(lx).wrapping_add(ly.wrapping_mul(ly)) as u32) >> 2;
        if ready && across < distance.wrapping_mul(distance) as u32 {
            let target = self.selector61(ship)?.target_148;
            self.fire_actor_weapon(
                ship,
                ActorWeaponFireRequest {
                    target,
                    distance: distance as u32,
                    world: offset.map(|word| word as i16),
                    microseconds: delta,
                    mode: ActorWeaponFireMode::Predictive { publish_lead: true },
                },
                &mut host.firing(),
            )?;
        }
        Ok(())
    }

    /// 00412A48..00412B18: one draw in four takes the first player's unit
    /// within 2000 and ahead as the target.
    fn acquire_selector61_target<H: Selector61Host>(
        &mut self,
        ship: ObjectId,
        host: &mut H,
    ) -> Result61<(), H::Error> {
        let draw = crate::name_entry::name_entry_rng_next(host.firing().rng);
        if draw as u16 >= ACQUIRE_BELOW {
            return Ok(());
        }
        let Some(unit) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let position = self.selector61_position(ship)?;
        let theirs = self
            .object(unit)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(unit))?;
        let offset: [u16; 3] =
            std::array::from_fn(|axis| theirs[axis].wrapping_sub(position[axis]));
        let [x, y, z] = offset.map(|word| i32::from(word as i16));
        let squared = x
            .wrapping_mul(x)
            .wrapping_add(z.wrapping_mul(z))
            .wrapping_add(y.wrapping_mul(y)) as u32;
        if squared >= ACQUIRE_SQUARED {
            return Ok(());
        }
        let local = self
            .actor(ship)?
            .rotating
            .basis
            .inverse_words(offset.map(|word| word as i16));
        if local[2] <= 0 {
            return Ok(());
        }
        let [lx, ly, _] = local.map(i32::from);
        if lx.wrapping_mul(lx).wrapping_add(ly.wrapping_mul(ly)) as u32 >= squared {
            return Ok(());
        }
        let handle = self.actor(unit)?.handle;
        let target = self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
        self.selector61_mut(ship)?.target_148 = target;
        Ok(())
    }

    /// 00412BBA..00412C67: near its target the ship wants the target's
    /// forward speed (its velocity along its own basis row 2), plus twice
    /// the gap past +144, which a fast target widens by half its excess.
    fn selector61_chase_speed<E>(
        &mut self,
        ship: ObjectId,
        followed: &crate::object_runtime::ActorFollowTarget,
        distance: i32,
    ) -> Result61<i32, E> {
        let basis = self.follow_target_basis(followed)?;
        let velocity = followed.velocity_1a.ok_or(Selector61Error::Unwritten {
            ship,
            offset: 0x148,
            address: 0x0041_2bcf,
        })?;
        let f = |word: usize| f64::from(f32::from_bits(basis.words[word]));
        let [vx, vy, vz] = velocity.map(|word| f64::from(word as i16));
        let along = ftol((vy * f(8) + vz * f(9)) + vx * f(7));
        let mut keep = self.selector61(ship)?.keep_144 as i32;
        if along > FAST {
            keep = keep.wrapping_add(along.wrapping_sub(FAST) / 2);
        }
        let speed = along.wrapping_add(distance.wrapping_sub(keep).wrapping_mul(2));
        Ok(if speed > TOP_SPEED {
            TOP_SPEED
        } else if speed < LOW_SPEED {
            LOW_SPEED
        } else {
            speed
        })
    }

    /// 00412FA0, the ship's +AC: the visual 0x37, the burst at its position,
    /// unless flag bit 31 the world radius effect with its own handle, the
    /// type-1 explosion, then its disposal (00479FA0).
    pub fn retire_selector61<H: Selector61DeathHost>(
        &mut self,
        ship: ObjectId,
        host: &mut H,
    ) -> Result61<(), H::Error> {
        self.selector61(ship)?;
        let fail = |address| {
            move |error| Selector61Error::Host {
                ship,
                address,
                error,
            }
        };
        host.visual(self, ship, PURSUIT_SHIP_VISUAL)
            .map_err(fail(0x0041_2faf))?;
        let position = self.selector61_position(ship)?;
        host.burst(self, death_burst(position))
            .map_err(fail(0x0041_300a))?;
        if self.object(ship)?.base.flags & 0x8000_0000 == 0 {
            let source = self.actor(ship)?.handle;
            host.blast(self, ship, PURSUIT_SHIP_BLAST, source)
                .map_err(fail(0x0041_302d))?;
        }
        let position = self.selector61_position(ship)?;
        host.explosion(self, PURSUIT_SHIP_EXPLOSION, position)
            .map_err(fail(0x0041_303d))?;
        self.dispose(ship)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector61_tests.rs"]
mod tests;
