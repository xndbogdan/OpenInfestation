//! Direction producers 0048D340/0048D5A0 and retained queue 0048D0D0.
//! This owns queued intrinsic records, not later48CDB0/48C890 projectile effects.
//! Reached backing/service errors preserve actor, table, heap and RNG prefixes.
use super::super::derived::native_trunc_low32 as ftol;
use super::damage_cache::storage::{read_row_dword, read_row_word};
use super::{
    ActorBehaviorState, BehaviorCacheStorageError, BehaviorRowSelection, ProcessActorBehaviorTables,
};
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    mark_flag_change, ActorServiceAllocationRequest, ActorServiceHeapError,
    ActorServiceResourceKind, NativeHeapRef, NativeTargetQueryError, NativeWeaponFireNode,
    ObjectArena, ObjectError, ObjectId,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorWeaponFireMode {
    Predictive { publish_lead: bool },
    Scattered,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorWeaponFireRequest {
    pub target: Option<NativeHeapRef>,
    /// Native signed-distance arithmetic consumes these raw DWORD bits.
    pub distance: u32,
    pub world: [i16; 3],
    pub microseconds: u32,
    pub mode: ActorWeaponFireMode,
}
impl ActorWeaponFireRequest {
    pub fn native_address(self) -> u32 {
        match self.mode {
            ActorWeaponFireMode::Predictive { .. } => 0x0048_d340,
            ActorWeaponFireMode::Scattered => 0x0048_d5a0,
        }
    }
}
pub struct ActorWeaponFireContext<'a> {
    pub tables: &'a mut ProcessActorBehaviorTables,
    pub rng: &'a mut u32,
    /// Actual0055F0F8 loan. None is unowned, not a zero velocity. Follow
    /// virtual08 uses its own retained1A and never demands this operand.
    pub process_velocity: Option<[u16; 3]>,
}
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorWeaponFireError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error(transparent)]
    TargetQuery(#[from] NativeTargetQueryError),
    #[error(transparent)]
    Heap(#[from] ActorServiceHeapError),
    #[error("actor {0:?} requires its native140 behavior")]
    RequiredBehavior(ObjectId),
    #[error("actor {0:?} requires a target at native48D387 virtual08")]
    RequiredTarget(ObjectId),
    #[error("target {0:?} has unwritten virtual08 velocity")]
    RequiredTargetVelocity(NativeHeapRef),
    #[error("actor {actor:?} reaches18 DWORD offset{offset}, outside{entries} backed entries")]
    EntryBacking {
        actor: ObjectId,
        offset: i32,
        entries: usize,
    },
    #[error("actor {actor:?} reaches1C group{index}, outside{groups} backed groups")]
    GroupBacking {
        actor: ObjectId,
        index: i32,
        groups: usize,
    },
    #[error("portable nonprogressing weapon queue guard for actor {actor:?}: infinite count, timer {timer}, cursor {cursor}, no cycle jitter")]
    PortableNonProgressingWeaponQueueGuard {
        actor: ObjectId,
        timer: u32,
        cursor: u32,
    },
}

impl ObjectArena {
    pub fn fire_actor_weapon(
        &mut self,
        actor: ObjectId,
        request: ActorWeaponFireRequest,
        context: &mut ActorWeaponFireContext<'_>,
    ) -> Result<(), ActorWeaponFireError> {
        let mut world = request.world;
        match request.mode {
            ActorWeaponFireMode::Predictive { publish_lead } => {
                if self.weapon_behavior(actor)?.byte_0c != 0 {
                    let normal = normalize(world, LengthSource::Unspilled);
                    let reference = request
                        .target
                        .ok_or(ActorWeaponFireError::RequiredTarget(actor))?;
                    let target_velocity = self
                        .native_target(reference)?
                        .velocity_words(context.process_velocity)?
                        .ok_or(ActorWeaponFireError::RequiredTargetVelocity(reference))?;
                    let actor_velocity = self.actor(actor)?.words_8e_92;
                    let relative: [i32; 3] = std::array::from_fn(|i| {
                        i32::from(target_velocity[i].wrapping_sub(actor_velocity[i]) as i16)
                    });
                    let dot = (f64::from(relative[2]) * f64::from(normal[2])
                        + f64::from(relative[1]) * f64::from(normal[1]))
                        + f64::from(relative[0]) * f64::from(normal[0]);
                    let row = self.weapon_row(actor)?;
                    let speed = i32::from(read_row_word(self, context.tables, row, 2)? as i16);
                    let denominator = speed.wrapping_sub(ftol(dot));
                    let quotient = (denominator > 0)
                        .then(|| (request.distance.wrapping_shl(10) as i32) / denominator);
                    let (factor, multiplier) = if quotient.is_none_or(|v| v >= 4096) {
                        if publish_lead {
                            self.weapon_behavior_mut(actor)?.word_20 = 0x1400;
                        }
                        (4096_i32, i32::MAX)
                    } else {
                        let factor = quotient.expect("positive denominator");
                        if publish_lead {
                            self.weapon_behavior_mut(actor)?.word_20 =
                                factor.wrapping_mul(5).wrapping_shr(2) as u32;
                        }
                        (factor, factor.wrapping_mul(0x24400))
                    };
                    for axis in 0..3 {
                        let high = doubled_product_high(relative[axis], multiplier).wrapping_shl(4);
                        world[axis] = world[axis].wrapping_add(high as i16);
                    }
                    //48D485 captures fresh signed10 before48D6C0's profile kind
                    //and registry flags read. Lead20 writes already survived.
                    let row = self.weapon_row(actor)?;
                    let kind = i32::from(read_row_word(self, context.tables, row, 0)? as i16);
                    let address = 0x0053_dbb4_u32.wrapping_add(kind.wrapping_mul(16) as u32);
                    if context.tables.read_dword(address)? & 1 != 0 {
                        let square = factor.wrapping_mul(factor);
                        world[1] =
                            world[1].wrapping_add(doubled_product_high(square, 0x180000) as i16);
                    }
                }
                let spread = self.weapon_behavior(actor)?.word_04 as i32;
                if spread != 0 {
                    let amplitude = spread
                        .wrapping_mul(request.distance as i32)
                        .wrapping_shr(16);
                    add_spread(&mut world, amplitude, context.rng);
                }
            }
            ActorWeaponFireMode::Scattered => {
                let spread = self.weapon_behavior(actor)?.word_04 as i32;
                let amplitude = spread
                    .wrapping_mul(request.distance as i32)
                    .wrapping_shr(16)
                    .wrapping_add((request.distance as i32).wrapping_shr(2));
                add_spread(&mut world, amplitude, context.rng);
            }
        }
        let direction_bits = normalize(world, LengthSource::Float32Spill).map(f32::to_bits);
        self.queue_actor_weapon_direction(actor, request.microseconds, direction_bits, 0, context)
    }

    pub fn queue_actor_weapon_direction(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        direction: [u32; 3],
        fraction: u32,
        context: &mut ActorWeaponFireContext<'_>,
    ) -> Result<(), ActorWeaponFireError> {
        let behavior = self.weapon_behavior(actor)?;
        let index = i32::from(behavior.byte_10 as i8);
        let entry = index.wrapping_mul(32);
        let row = BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, actor);
        let mode = (index as u32)
            | if behavior.byte_10 == behavior.byte_11 {
                0x10000
            } else {
                0
            };
        let mut loop_timer = self.weapon_entry(actor, entry.wrapping_add(0x14))?;
        if loop_timer >= microseconds {
            return Ok(());
        }
        let mut backdate = 0_u32;
        loop {
            let count = self.weapon_entry(actor, entry)?;
            if count == 0 {
                return Ok(());
            }
            let old_timer = loop_timer;
            let mut initial_cursor = None;
            if count != u32::MAX {
                self.set_weapon_entry(actor, entry, count.wrapping_sub(1))?;
            }
            let shots = read_row_word(self, context.tables, row, 6)? as i16;
            if shots > 0 {
                let mut shot = 0_i32;
                loop {
                    let group_count = i32::from(self.weapon_behavior(actor)?.byte_0d as i8);
                    let cursor = self.weapon_entry(actor, entry.wrapping_add(0x18))? as i32;
                    initial_cursor.get_or_insert(cursor as u32);
                    if cursor >= group_count {
                        break;
                    }
                    let behavior = self.weapon_behavior_mut(actor)?;
                    let groups = behavior.groups_1c.len();
                    let group = behavior.groups_1c.get_mut(cursor as usize).ok_or(
                        ActorWeaponFireError::GroupBacking {
                            actor,
                            index: cursor,
                            groups,
                        },
                    )?;
                    group.word_0c |= 0x04;
                    let reference =
                        self.actor_services
                            .allocate(ActorServiceAllocationRequest {
                                kind: ActorServiceResourceKind::WeaponFireNode,
                            })?;
                    if let Some(reference) = reference {
                        //Native reads the installed head and fresh group only
                        //after allocation, never frees an older lost chain.
                        let next = self.weapon_behavior(actor)?.fire_head_00;
                        let group = self.weapon_entry(actor, entry.wrapping_add(0x18))?;
                        self.actor_services.construct_weapon_fire_node(
                            reference,
                            NativeWeaponFireNode {
                                direction_bits: direction,
                                compensation: backdate as i32,
                                group,
                                selection: mode,
                                speed_scale: fraction,
                                next,
                            },
                        )?;
                    }
                    self.weapon_behavior_mut(actor)?.fire_head_00 = reference;
                    let base = &mut self.owned_mut(actor)?.state.base;
                    let flags = base.flags | 0x0100_0000;
                    mark_flag_change(base, flags);
                    let cursor = self.weapon_entry(actor, entry.wrapping_add(0x18))?;
                    let cooldown = read_row_dword(self, context.tables, row, 0x0c)?;
                    self.set_weapon_entry(
                        actor,
                        entry
                            .wrapping_add(4)
                            .wrapping_add((cursor as i32).wrapping_mul(4)),
                        cooldown,
                    )?;
                    let cursor = self.weapon_entry(actor, entry.wrapping_add(0x18))?;
                    self.set_weapon_entry(actor, entry.wrapping_add(0x18), cursor.wrapping_add(1))?;
                    //Fresh selected entry/profile at1E6..237; captured outer
                    //entry and row remain separate native pointers.
                    let fresh = self.weapon_behavior(actor)?;
                    let fresh_entry = i32::from(fresh.byte_10 as i8)
                        .wrapping_mul(32)
                        .wrapping_add(0x1c);
                    let fresh_row = self.weapon_row(actor)?;
                    let cost = read_row_dword(self, context.tables, fresh_row, 0x1c)?;
                    let pressure = self.weapon_entry(actor, fresh_entry)?.wrapping_add(cost);
                    self.set_weapon_entry(actor, fresh_entry, pressure)?;
                    let fresh = self.weapon_behavior(actor)?;
                    let fresh_entry = i32::from(fresh.byte_10 as i8)
                        .wrapping_mul(32)
                        .wrapping_add(0x1c);
                    let pressure = self.weapon_entry(actor, fresh_entry)?;
                    if pressure > 6_000_000 {
                        let cost =
                            read_row_dword(self, context.tables, self.weapon_row(actor)?, 0x1c)?;
                        self.set_weapon_entry(
                            actor,
                            fresh_entry,
                            pressure.wrapping_add(cost.wrapping_mul(2)),
                        )?;
                    }
                    shot = shot.wrapping_add(1);
                    let shots = i32::from(read_row_word(self, context.tables, row, 6)? as i16);
                    if shot >= shots {
                        break;
                    }
                }
            }
            let spacing = read_row_dword(self, context.tables, row, 8)?;
            let timer = self
                .weapon_entry(actor, entry.wrapping_add(0x14))?
                .wrapping_add(spacing);
            self.set_weapon_entry(actor, entry.wrapping_add(0x14), timer)?;
            let spacing = read_row_dword(self, context.tables, row, 8)?;
            backdate = backdate.wrapping_sub(spacing);
            let group_count = i32::from(self.weapon_behavior(actor)?.byte_0d as i8);
            let cursor = self.weapon_entry(actor, entry.wrapping_add(0x18))?;
            let old_cursor = *initial_cursor.get_or_insert(cursor);
            let mut jitter_reached = false;
            if (cursor as i32) >= group_count {
                let cooldown = read_row_dword(self, context.tables, row, 0x10)?;
                let timer = self
                    .weapon_entry(actor, entry.wrapping_add(0x14))?
                    .wrapping_add(cooldown);
                self.set_weapon_entry(actor, entry.wrapping_add(0x14), timer)?;
                let cooldown = read_row_dword(self, context.tables, row, 0x10)?;
                self.set_weapon_entry(actor, entry.wrapping_add(0x18), 0)?;
                let jitter = self.weapon_behavior(actor)?.word_08 as i32;
                backdate = backdate.wrapping_sub(cooldown);
                if jitter != 0 {
                    jitter_reached = true;
                    let random = (name_entry_rng_next(context.rng) & 0x7f) as i32;
                    let amount = random
                        .wrapping_mul(jitter)
                        .wrapping_shr(8)
                        .wrapping_add(jitter.wrapping_sub(jitter >> 31).wrapping_shr(1));
                    let timer = self
                        .weapon_entry(actor, entry.wrapping_add(0x14))?
                        .wrapping_add(amount as u32);
                    self.set_weapon_entry(actor, entry.wrapping_add(0x14), timer)?;
                }
            }
            let timer = self.weapon_entry(actor, entry.wrapping_add(0x14))?;
            loop_timer = timer;
            if timer >= microseconds {
                return Ok(());
            }
            //PortableNonProgressingWeaponQueueGuard: after the full normal
            //writer suffix only. Finite counts and any reached cycle RNG stay
            //admitted. Pressure/head/effect stores are not native loop controls.
            let cursor = self.weapon_entry(actor, entry.wrapping_add(0x18))?;
            if count == u32::MAX && timer == old_timer && cursor == old_cursor && !jitter_reached {
                return Err(
                    ActorWeaponFireError::PortableNonProgressingWeaponQueueGuard {
                        actor,
                        timer,
                        cursor,
                    },
                );
            }
        }
    }

    pub(in crate::object_runtime) fn weapon_behavior(
        &self,
        actor: ObjectId,
    ) -> Result<&ActorBehaviorState, ActorWeaponFireError> {
        self.actor_behavior_storage(actor)?
            .as_ref()
            .ok_or(ActorWeaponFireError::RequiredBehavior(actor))
    }
    fn weapon_behavior_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut ActorBehaviorState, ActorWeaponFireError> {
        self.actor_behavior_storage_mut(actor)?
            .as_mut()
            .ok_or(ActorWeaponFireError::RequiredBehavior(actor))
    }
    fn weapon_row(&self, actor: ObjectId) -> Result<BehaviorRowSelection, ActorWeaponFireError> {
        let b = self.weapon_behavior(actor)?;
        Ok(BehaviorRowSelection::from_table(
            b.table_14, b.byte_10, actor,
        ))
    }
    pub(in crate::object_runtime) fn weapon_entry(
        &self,
        actor: ObjectId,
        offset: i32,
    ) -> Result<u32, ActorWeaponFireError> {
        let b = self.weapon_behavior(actor)?;
        let entries = b.entries_18.len();
        if offset < 0 || offset % 4 != 0 {
            return Err(ActorWeaponFireError::EntryBacking {
                actor,
                offset,
                entries,
            });
        }
        b.entries_18
            .get(offset as usize / 32)
            .map(|row| row[(offset as usize % 32) / 4])
            .ok_or(ActorWeaponFireError::EntryBacking {
                actor,
                offset,
                entries,
            })
    }
    fn set_weapon_entry(
        &mut self,
        actor: ObjectId,
        offset: i32,
        value: u32,
    ) -> Result<(), ActorWeaponFireError> {
        let b = self.weapon_behavior_mut(actor)?;
        let entries = b.entries_18.len();
        if offset < 0 || offset % 4 != 0 {
            return Err(ActorWeaponFireError::EntryBacking {
                actor,
                offset,
                entries,
            });
        }
        let row = b.entries_18.get_mut(offset as usize / 32).ok_or(
            ActorWeaponFireError::EntryBacking {
                actor,
                offset,
                entries,
            },
        )?;
        row[(offset as usize % 32) / 4] = value;
        Ok(())
    }
}

fn doubled_product_high(left: i32, right: i32) -> i32 {
    ((i64::from(left)
        .wrapping_mul(i64::from(right))
        .wrapping_shl(1))
        >> 32) as i32
}
fn add_spread(world: &mut [i16; 3], amplitude: i32, rng: &mut u32) {
    for value in world {
        let random = (name_entry_rng_next(rng) & 0xffff) as i32;
        let amount = amplitude.wrapping_sub(random.wrapping_mul(amplitude).wrapping_shr(15));
        *value = value.wrapping_add(amount as i16);
    }
}
#[derive(Clone, Copy)]
enum LengthSource {
    Unspilled,
    Float32Spill,
}
/// Supported retail FPCW=027F uses 53-bit arithmetic. The initial 00417DA0
/// normalize keeps its length in a register; the final producer stores length
/// to float before division. Do not merge these two rounding boundaries.
/// `00417DA0`: a WORD vector over its length, in floats; (1, 0, 0) for the
/// zero vector.
pub(in crate::object_runtime::actor) fn normalize_417da0(world: [i16; 3]) -> [f32; 3] {
    normalize(world, LengthSource::Unspilled)
}

fn normalize(world: [i16; 3], source: LengthSource) -> [f32; 3] {
    let [x, y, z] = world.map(f64::from);
    let length = ((z * z + y * y) + x * x).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let divisor = match source {
        LengthSource::Unspilled => length,
        LengthSource::Float32Spill => f64::from(length as f32),
    };
    [x, y, z].map(|value| (value / divisor) as f32)
}
#[cfg(test)]
mod tests;
