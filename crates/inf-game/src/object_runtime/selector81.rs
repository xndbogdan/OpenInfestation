//! Selector 81, world 19's spawner: constructor 00409250 over the base
//! object 00479C60, final table 0051FE44, 0x5C bytes. The table is the base
//! object's 00527B24 but for its destructor (00409290, which frees +58), its
//! commands +04 (00409320: 2 the member count +44, 3 the distance +48), its
//! +1C (00409360, the member list) and its update +24 (00409390).
//!
//! It keeps the first player's unit in view: each time the unit gets +48
//! away from the point it last passed, the spawner moves there and +4C
//! becomes the speed the unit made (+50 the time it took); five seconds
//! without such a move set +4C to zero. On its timer +40, while a member is
//! missing and the unit is slow (+4C under +48), it raises a member
//! (resource 0x86, selector 80) round the unit: a bearing from the buckets
//! at 0052EDC0 and a range of 500 to 1011 in the unit's frame (0047A6C0),
//! where the ground is more than 32 below the liquid, 256 clear of the
//! members and 1300 of every objective. With the aliens' nodes all taken
//! (00547620) the spawner disposes itself.

use thiserror::Error;

use super::{ActorHandle, ObjectArena, ObjectClass, ObjectError, ObjectId};
use crate::{
    name_entry::name_entry_rng_next, terrain_runtime::TerrainRuntime,
    world_radius_effect::radius_word_distance,
};

/// `004097E5`: the members' resource.
pub const MEMBER_RESOURCE: u32 = 0x86;
/// SpawnerFirstHeightApproximation: an update's first attempt turns its
/// offset through the unit's basis with a Y (`[ESP+2E]`, read at
/// `004095B7`) that nothing in the update has written; later attempts take
/// the previous attempt's ground. The word is whatever the object updated
/// just before the spawner left there: in the lab (`spawner-y-world19.json`,
/// retail world 19 at 125 ms) 0 and 1 on alternate ticks, which move the
/// turned point by at most a unit, and once eyes updated just before it
/// 0x2801, which throws the attempts thousands of units across a tilted
/// unit. The port starts from 0, retail's while the word is 0 or 1.
pub const SPAWNER_FIRST_HEIGHT_APPROXIMATION: u16 = 0;
/// `004093A7`: five seconds without a move make the unit slow.
const STILL_LIMIT: u32 = 5_000_000;
/// `0052EDC0`: the bearings' cumulative percentages, an eighth of a turn
/// each (the last one is never passed).
const BEARINGS: [i32; 8] = [15, 35, 45, 50, 55, 65, 85, 101];
/// `00409740`: a member within 256 of the point is too close.
const MEMBER_CLEARANCE: u32 = 0x10000;
/// `004097BE`: an objective within 1300 blocks the spawn.
const OBJECTIVE_CLEARANCE: u32 = 0x19_c990;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector81State {
    /// +40, the spawn timer.
    pub timer_40: u32,
    /// +44, the members kept (command 2; 5).
    pub count_44: u32,
    /// +48, the distance a moving unit covers (command 3; 1000).
    pub distance_48: u32,
    /// +4C, the speed the unit made over its last such distance (20000).
    pub speed_4c: u32,
    /// +50, the time since that move.
    pub still_50: u32,
    pub dword_54: u32,
    /// +58, the members' handles: 00409360 allocates +44 DWORDs it never
    /// writes; a member's spawn writes its own.
    pub members_58: Vec<Option<u32>>,
}

impl Selector81State {
    pub(super) fn new() -> Self {
        Self {
            timer_40: 0,
            count_44: 5,
            distance_48: 1000,
            speed_4c: 20_000,
            still_50: 0,
            dword_54: 0,
            members_58: Vec::new(),
        }
    }
}

/// What the update reaches beyond the arena.
pub trait Selector81Host {
    type Error: std::fmt::Display;
    /// The H12 terrain under the spawn (`0055EE3C`).
    fn terrain(&self) -> &TerrainRuntime;
    /// `00460310(0x86, point, NULL, -1, NULL, 0054E894)`, then the new
    /// object's +18: its actor, or none.
    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector81Error<E: std::fmt::Display> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Handles(#[from] super::ActorConstructionError),
    #[error("spawner {object:?} reads member {index}'s unwritten handle at 0040949D")]
    UnwrittenMember { object: ObjectId, index: usize },
    /// `004097EF` uses the factory's answer without a NULL test.
    #[error("spawner {0:?} raised no member (00460310 answered NULL)")]
    MissingMember(ObjectId),
    #[error("spawner {object:?} service at {address:08X}: {error}")]
    Host {
        object: ObjectId,
        address: u32,
        error: E,
    },
}

/// The sine table at 0053DF00, by the turn's top 14 bits.
fn sine(angle: u32, table: &[u16; 4096]) -> i32 {
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

/// `004095F5..004096C0`: the highest of the four corners of the H12 cell
/// under the point, in world units.
fn corner_height(terrain: &TerrainRuntime, [x, z]: [u16; 2]) -> i32 {
    let sampling = terrain.sampling();
    let units = sampling.spacing.units();
    let mask = !((units - 1) >> 7);
    let step = units >> 7;
    let (x0, z0) = ((x >> 7) & mask, (z >> 7) & mask);
    let (x1, z1) = ((x0 + step) & 0x1ff, (z0 + step) & 0x1ff);
    let shift = sampling.height_scale.shift();
    [(x0, z0), (x0, z1), (x1, z0), (x1, z1)]
        .into_iter()
        .map(|(x, z)| i32::from(terrain.cell_at_grid([x, z]).height_i8()) << shift)
        .max()
        .expect("four corners")
}

impl ObjectArena {
    pub fn selector81(&self, object: ObjectId) -> Result<&Selector81State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector81(state) => Ok(state),
            _ => Err(ObjectError::NotSelector81(object)),
        }
    }

    fn selector81_mut(&mut self, object: ObjectId) -> Result<&mut Selector81State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector81(state) => Ok(state),
            _ => Err(ObjectError::NotSelector81(object)),
        }
    }

    /// 00409320 codes 2 and 3; codes through 1 are the base object's
    /// (00479E70) and higher ones refused.
    pub(super) fn apply_selector81_command(
        &mut self,
        object: ObjectId,
        payload: &inf_formats::InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let state = self.selector81_mut(object)?;
        match payload {
            inf_formats::InstanceCommandPayload::Selector81Count44(value) => {
                state.count_44 = value.value
            }
            inf_formats::InstanceCommandPayload::Selector81Distance48(value) => {
                state.distance_48 = value.value
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 00409360, the spawner's +1C: +58 gets +44 DWORDs, unwritten (the
    /// allocation never fails here).
    pub(super) fn finish_selector81(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let count = self.selector81(object)?.count_44 as usize;
        let member = self
            .actor_allocation
            .initial_dword(super::actor::ActorAllocationDwordCarry::Selector81Members58);
        self.selector81_mut(object)?.members_58 = vec![member; count];
        Ok(())
    }

    /// 00409390, the spawner's +24.
    pub fn tick_selector81<H: Selector81Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), Selector81Error<H::Error>> {
        let unit = self.first_player_primary_actor()?;
        let state = self.selector81_mut(object)?;
        if state.still_50 > STILL_LIMIT {
            state.speed_4c = 0;
        } else {
            state.still_50 = state.still_50.wrapping_add(delta);
        }
        let Some(unit) = unit else {
            return Ok(());
        };
        let position = |arena: &Self, object: ObjectId| {
            arena
                .object(object)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(object))
        };
        let own = position(self, object)?;
        let theirs = position(self, unit)?;
        let offset: [i16; 3] =
            std::array::from_fn(|axis| own[axis].wrapping_sub(theirs[axis]) as i16);
        let distance = radius_word_distance(offset);
        if distance as i32 > self.selector81(object)?.distance_48 as i32 {
            self.owned_mut(object)?.state.base.position_words = Some(theirs);
            let state = self.selector81_mut(object)?;
            state.speed_4c = if state.still_50 > 0x7ff {
                (distance << 10) / (state.still_50 >> 10)
            } else {
                distance.wrapping_mul(1000)
            };
            state.still_50 = 0;
        }
        let state = self.selector81_mut(object)?;
        if state.timer_40 > delta {
            state.timer_40 -= delta;
            return Ok(());
        }
        state.timer_40 = 0;
        if state.speed_4c as i32 >= state.distance_48 as i32 {
            return Ok(());
        }
        // 0040946D: the members' actors, and the last one missing.
        let mut members = Vec::new();
        let mut missing = None;
        for index in 0..(self.selector81(object)?.count_44 as i32).max(0) as usize {
            let handle = self
                .selector81(object)?
                .members_58
                .get(index)
                .copied()
                .flatten()
                .ok_or(Selector81Error::UnwrittenMember { object, index })?;
            let member = self.resolve_actor_handle(ActorHandle::from_raw(handle))?;
            if member.is_none() {
                missing = Some(index);
            }
            members.push(member);
        }
        let Some(missing) = missing else {
            return Ok(());
        };
        // The retry flag is the delta argument's slot: its low byte until
        // 00409767 writes 1.
        let mut done = delta as u8;
        let mut height: Option<u16> = None;
        for _ in 0..8 {
            let percent = ((name_entry_rng_next(rng) & 0xffff) * 100) as i32 >> 16;
            let bucket = BEARINGS
                .iter()
                .position(|&limit| percent <= limit)
                .unwrap_or(7);
            let angle = ((name_entry_rng_next(rng) & 0xffff) >> 3) + ((bucket as u32) << 13);
            let range = ((name_entry_rng_next(rng) & 0xffff) >> 7) as i32 + 500;
            let x = (sine(angle + 0x4000, sine_quarter).wrapping_mul(range) >> 15) as u16;
            let z = (sine(angle, sine_quarter).wrapping_mul(range) >> 15) as u16;
            let y = height.unwrap_or(SPAWNER_FIRST_HEIGHT_APPROXIMATION);
            let turned = self.transform_visual_vector_words(unit, [x, y, z])?;
            let velocity = self.actor(unit)?.words_8e_92;
            let theirs = position(self, unit)?;
            let point_x = turned[0].wrapping_add(velocity[0].wrapping_add(theirs[0]));
            let point_z = turned[2].wrapping_add(velocity[2].wrapping_add(theirs[2]));
            let terrain = host.terrain();
            let ground = corner_height(terrain, [point_x, point_z]) as u16;
            let level = (terrain.plane_base() >> terrain.plane_shift()) as i16;
            height = Some(ground);
            let point = [point_x, ground, point_z];
            let near = |arena: &Self, actor: ObjectId, limit: u32| -> Result<bool, ObjectError> {
                let [x, _, z] = position(arena, actor)?;
                let dx = i32::from(point_x.wrapping_sub(x) as i16);
                let dz = i32::from(point_z.wrapping_sub(z) as i16);
                Ok((dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) as u32) < limit)
            };
            // 004096F5: the ground must lie more than 32 below the liquid,
            // and 00409701 no member within 256.
            let mut clear = i32::from(ground as i16) + 0x20 < i32::from(level);
            if clear {
                for member in members.iter().flatten() {
                    if near(self, *member, MEMBER_CLEARANCE)? {
                        clear = false;
                        break;
                    }
                }
            }
            if clear {
                if self.capture_node_counts().aliens_547620 == 0 {
                    self.dispose(object)?;
                } else {
                    done = 1;
                    // 0040976D: an objective (flags 0x28 of 0xA8, a kept
                    // +54) within 1300 blocks it.
                    let mut blocked = false;
                    for actor in self.actor_list().to_vec() {
                        if self.object(actor)?.base.flags & 0xa8 == 0x28
                            && self.actor_completion_kept(actor)?
                            && near(self, actor, OBJECTIVE_CLEARANCE)?
                        {
                            blocked = true;
                            break;
                        }
                    }
                    if !blocked {
                        let member = host
                            .spawn(self, point, rng)
                            .map_err(|error| Selector81Error::Host {
                                object,
                                address: 0x0040_97ea,
                                error,
                            })?
                            .ok_or(Selector81Error::MissingMember(object))?;
                        let handle = self.actor(member).ok().map(|actor| actor.handle.raw());
                        members[missing] = handle.map(|_| member);
                        if let Some(handle) = handle {
                            self.selector81_mut(object)?.members_58[missing] = Some(handle);
                        }
                    }
                    let draw = name_entry_rng_next(rng) & 0xffff;
                    self.selector81_mut(object)?.timer_40 =
                        if self.capture_node_counts().aliens_547620 != 0 {
                            (draw + 31_250) << 5
                        } else {
                            (draw + 78_125) << 7
                        };
                }
            }
            if done != 0 {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector81_tests.rs"]
mod tests;
