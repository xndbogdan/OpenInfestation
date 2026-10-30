//! Selector 38, the mine: 00455560's class with final table 00525914, built
//! from resources 117 and 118 (the ACAM's weapon rows 11 and 13). Its
//! update (+24, 00456860) arms it once its fuse first falls to a second,
//! then keeps the fuse from running out. It picks the nearest enemy inside
//! its trigger radius (+158) through the actor scan 00475EC0 and tracks it
//! with a type-2 target at +15C; when that enemy moves away again, the mine
//! marks itself to detonate. The grenade's 00455E10 then runs. Its
//! detonation (+B0, 004566A0) tells its thrower (event 0x11, or 0x13 after a
//! burst 1500 above it when +154 is set) before the grenade's 004564B0.

use super::grenade26::{Grenade26Error, Grenade26Host};
use super::Family455560Tail;
use crate::object_runtime::{
    FollowTargetPoint, FollowTargetRequest, NativeHeapRef, ObjectArena, ObjectError, ObjectId,
};

/// 004568E0..004568F0: the scan's flags and its XZ reach, 2000 squared.
const SCAN_FLAGS: u32 = 0x24;
const SCAN_REACH_SQUARED: u32 = 0x3d_0900;
/// 0045687C..0045689F: a fuse at or below 1000 rearms it at 2000.
const REARM_BELOW: u16 = 1000;
const REARM_FUSE: u16 = 2000;
/// 004566B3/004566ED: the raised burst's visual, then the explosion's.
const RAISED_VISUAL: u32 = 0x3a;
const RAISED_EXPLOSION: u32 = 0x38;
/// 004566C3: the raised burst's height above the mine.
const RAISED_HEIGHT: u16 = 1500;
/// 0045670C/0045671B: the thrower's message, raised or not.
const RAISED_MESSAGE: u32 = 0x13;
const PLAIN_MESSAGE: u32 = 0x11;

type MineResult<T, E> = Result<T, Grenade26Error<E>>;

/// The mine's own tail fields, +154..+163.
struct MineTail<'a> {
    flat: &'a mut Option<u8>,
    trigger: &'a mut Option<u32>,
    target: &'a mut Option<NativeHeapRef>,
    tracked: &'a mut u32,
}

impl ObjectArena {
    pub fn is_mine38(&self, object: ObjectId) -> bool {
        self.behavior_resource(object)
            .is_ok_and(|state| state.selector == 38)
            && self.thrown(object).is_ok()
    }

    fn mine38_tail<E>(&mut self, mine: ObjectId) -> MineResult<MineTail<'_>, E> {
        match &mut self.thrown_mut(mine)?.tail {
            Family455560Tail::Selector38 {
                byte_154,
                dword_158,
                object_15c,
                dword_160,
                ..
            } => Ok(MineTail {
                flat: byte_154,
                trigger: dword_158,
                target: object_15c,
                tracked: dword_160,
            }),
            _ => Err(Grenade26Error::NotGrenade { grenade: mine }),
        }
    }

    /// Byte +154: a set one measures distances in X and Z only and bursts
    /// 1500 above the mine.
    fn mine38_flat<E>(&mut self, mine: ObjectId) -> MineResult<bool, E> {
        let flat = *self.mine38_tail::<E>(mine)?.flat;
        flat.map(|flat| flat != 0).ok_or(Grenade26Error::Unwritten {
            grenade: mine,
            field: "flat +154",
        })
    }

    /// DWORD +158, the trigger radius, squared in 32 bits as retail does.
    fn mine38_trigger_squared<E>(&mut self, mine: ObjectId) -> MineResult<u32, E> {
        let trigger = *self.mine38_tail::<E>(mine)?.trigger;
        let trigger = trigger.ok_or(Grenade26Error::Unwritten {
            grenade: mine,
            field: "trigger radius +158",
        })?;
        Ok(trigger.wrapping_mul(trigger))
    }

    /// The squared distance from the mine to `point`, with signed WORD
    /// deltas and wrapping sums; a flat mine takes its own height.
    fn mine38_distance<E>(&mut self, mine: ObjectId, point: [u16; 3]) -> MineResult<u32, E> {
        let [x, y, z] = self.grenade26_position::<E>(mine)?;
        let height = if self.mine38_flat::<E>(mine)? {
            y
        } else {
            point[1]
        };
        let deltas = [
            x.wrapping_sub(point[0]),
            y.wrapping_sub(height),
            z.wrapping_sub(point[2]),
        ];
        Ok(deltas.iter().fold(0_i32, |sum, &delta| {
            let delta = i32::from(delta as i16);
            sum.wrapping_add(delta.wrapping_mul(delta))
        }) as u32)
    }

    /// 004567A0, the scan's score for `candidate`: nothing below lifecycle
    /// 2, for the mine's thrower, or on the mine's side, side 1 or side 0.
    /// Inside the trigger radius (unsigned squares) it scores 0x7FFFFFFF less
    /// the squared distance, so the nearest wins.
    fn mine38_score<E>(&mut self, mine: ObjectId, candidate: ObjectId) -> MineResult<u32, E> {
        let (lifecycle, handle, side) = {
            let actor = self.actor(candidate)?;
            (actor.word_c4 as i32, actor.handle, actor.word_b4)
        };
        let (owner, own_side) = {
            let actor = self.actor(mine)?;
            (actor.owner_handle, actor.word_b4)
        };
        if lifecycle <= 1 || handle == owner || side == own_side || side == 1 || side == 0 {
            return Ok(0);
        }
        let point = self.grenade26_position::<E>(candidate)?;
        let squared = self.mine38_distance::<E>(mine, point)?;
        if squared < self.mine38_trigger_squared::<E>(mine)? {
            Ok(0x7fff_ffff_u32.wrapping_sub(squared))
        } else {
            Ok(0)
        }
    }

    /// 00475EC0 with the mine's evaluator (00456780, table 005259E0): the
    /// actor list from G+48, each actor with both scan flags nearer than
    /// 2000 in XZ (unsigned squares) and not the mine, keeping the strictly
    /// best score. A zero best finds nothing.
    fn mine38_scan<E>(&mut self, mine: ObjectId) -> MineResult<Option<ObjectId>, E> {
        let [x, _, z] = self.grenade26_position::<E>(mine)?;
        let mut best = None;
        let mut best_score = 0_u32;
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & SCAN_FLAGS != SCAN_FLAGS {
                continue;
            }
            let [cx, _, cz] = self.grenade26_position::<E>(candidate)?;
            let dx = i32::from(x.wrapping_sub(cx) as i16);
            let dz = i32::from(z.wrapping_sub(cz) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= SCAN_REACH_SQUARED || candidate == mine {
                continue;
            }
            let score = self.mine38_score::<E>(mine, candidate)?;
            if score > best_score {
                best_score = score;
                best = Some(candidate);
            }
        }
        Ok(best)
    }

    /// 00456860, the mine's +24.
    pub fn tick_mine38<H: Grenade26Host>(
        &mut self,
        mine: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> MineResult<(), H::Error> {
        if !self.is_mine38(mine) {
            return Err(Grenade26Error::NotGrenade { grenade: mine });
        }
        if self.thrown(mine)?.word_13a <= REARM_BELOW {
            // 0045688D..004568B1: armed, the fuse back at 2 s, the tracked
            // distance cleared and the target deleted.
            let state = self.thrown_mut(mine)?;
            state.word_13a = REARM_FUSE;
            state.byte_138 = 1;
            let tail = self.mine38_tail::<H::Error>(mine)?;
            *tail.tracked = 0;
            if let Some(target) = tail.target.take() {
                self.release_actor_service(target)?;
            }
        }
        match *self.mine38_tail::<H::Error>(mine)?.target {
            None => {
                if let Some(found) = self.mine38_scan::<H::Error>(mine)? {
                    // 00456909..0045693B: a 0x20 type-2 target on the found
                    // actor's handle (004825D0), or none if allocation fails.
                    let handle = self.actor(found)?.handle;
                    let target = self.follow_target_resource(FollowTargetRequest {
                        handle,
                        point: FollowTargetPoint::ResolveActor,
                    })?;
                    *self.mine38_tail::<H::Error>(mine)?.target = target;
                    // 00456941..004569A2: the distance to the actor itself.
                    let point = self.grenade26_position::<H::Error>(found)?;
                    let squared = self.mine38_distance::<H::Error>(mine, point)?;
                    *self.mine38_tail::<H::Error>(mine)?.tracked = squared;
                }
            }
            Some(target) => {
                let tracked = *self.mine38_tail::<H::Error>(mine)?.tracked;
                // 004569B8: a signed compare with the trigger radius squared.
                if (tracked as i32) < self.mine38_trigger_squared::<H::Error>(mine)? as i32 {
                    // 004569C6: the target's +18 (00482690) refreshes its
                    // focus; the distance is to that focus.
                    self.refresh_follow_target_resource(target, host.settings_7c())?;
                    let focus = self.actor_follow_target(target)?.focus_08.ok_or(
                        Grenade26Error::Unwritten {
                            grenade: mine,
                            field: "target focus +08",
                        },
                    )?;
                    let squared = self.mine38_distance::<H::Error>(mine, focus)?;
                    let armed = self.thrown(mine)?.byte_138 != 0;
                    // 00456A28: moving away, armed, it marks itself.
                    if (squared as i32) > (tracked as i32) && armed {
                        *self.grenade26_tail::<H::Error>(mine)?.detonate = 1;
                    } else {
                        *self.mine38_tail::<H::Error>(mine)?.tracked = squared;
                    }
                }
            }
        }
        self.update_thrown(mine, delta, sine_quarter, rng, host)
    }

    /// 004566A0, the mine's +B0. A flat mine builds the default burst
    /// (00458D20) with visual 0x3A, applies its block 1500 above itself with
    /// visual 0x38 left for the explosion, and tells its thrower 0x13; any
    /// other tells it 0x11. Then the grenade's 004564B0.
    pub fn detonate_mine38<H: Grenade26Host>(
        &mut self,
        mine: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MineResult<(), H::Error> {
        if !self.is_mine38(mine) {
            return Err(Grenade26Error::NotGrenade { grenade: mine });
        }
        let message = if self.mine38_flat::<H::Error>(mine)? {
            let [x, y, z] = self.grenade26_position::<H::Error>(mine)?;
            let point = [x, y.wrapping_add(RAISED_HEIGHT), z];
            self.thrown_mut(mine)?.dword_148 = RAISED_VISUAL;
            self.grenade26_default_burst(mine, rng, host)?;
            self.thrown_mut(mine)?.dword_148 = RAISED_EXPLOSION;
            let block = self
                .thrown(mine)?
                .bytes_13e
                .ok_or(Grenade26Error::Unwritten {
                    grenade: mine,
                    field: "radius block +13E",
                })?;
            let owner = self.actor(mine)?.owner_handle;
            host.burst_at(self, mine, point, block, owner, rng)
                .map_err(Grenade26Error::Host)?;
            RAISED_MESSAGE
        } else {
            PLAIN_MESSAGE
        };
        host.owner_message(self, mine, message, rng)
            .map_err(Grenade26Error::Host)?;
        self.detonate_grenade26(mine, rng, host)
    }

    /// 00456640's deletion of a live +15C target, before 0047AA60.
    pub(in crate::object_runtime) fn release_mine38_target(
        &mut self,
        mine: ObjectId,
    ) -> Result<(), ObjectError> {
        let target = match &mut self.thrown_mut(mine)?.tail {
            Family455560Tail::Selector38 { object_15c, .. } => object_15c.take(),
            _ => return Err(ObjectError::NotBehaviorResource(mine)),
        };
        if let Some(target) = target {
            self.release_actor_service(target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mine38_tests.rs"]
mod tests;
