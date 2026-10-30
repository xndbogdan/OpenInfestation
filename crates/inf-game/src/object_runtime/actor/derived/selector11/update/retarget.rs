//! 004015F0: release the current target, then scan for a new one. Flag
//! +1AE & 4 selects scanner 0051F194 (00475F50) with filter 0051F18C
//! (004014C0); otherwise scanner 0051F184 (00476180) with filter 0051F17C
//! (004013B0) and occlusion test 00475FF0. Without a find, or while +1B0 is
//! running, a kind-8 point near the actor becomes the target. The scanners
//! and filters are short-lived heap objects with no other observable state;
//! scanner A starts its two candidate lists empty (00402650).

use inf_formats::native_basis::{isqrt_48efc0, normalize_words, NativeLengthPrecision};

use super::{random, Selector11UpdateError, Selector11UpdateHost};
use crate::object_runtime::actor::derived::family_actor_list_selection::query_canonical_task_relation;
use crate::object_runtime::{
    ActorExtension, ActorServiceAllocationRequest, ActorServiceResourceKind, DerivedActorClass,
    FollowTargetRequest, NativeTarget, NativeTargetClass, ObjectArena, ObjectError, ObjectId,
};
use crate::process_tasks::{relation::TaskRelationRequest, ProcessTaskRuntime};
use crate::terrain_runtime::TerrainRuntime;

/// Process services read by filter A.
pub trait Selector11ScanHost {
    /// Process task storage 0055F100, mutable for the relation query.
    fn scan_tasks(&mut self) -> Option<&mut ProcessTaskRuntime>;
    /// Process DWORD 0055F448, read by the team relation body.
    fn scan_network_flag(&self) -> Option<u32>;
    /// The resident H12 read by a model-0x41 source's height gate.
    fn scan_terrain(&self) -> Option<&TerrainRuntime>;
}

/// One scanned actor: the scanners keep the candidate, its score and the
/// squared distance; scanner A also keeps the relative position.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    actor: ObjectId,
    delta: [i16; 3],
    distance_squared: u32,
    score: u32,
}

impl ObjectArena {
    pub(super) fn selector11_retarget<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        self.selector11_mut(actor)?.word_1ae &= !8;
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            self.release_actor_service(reference)?;
            self.selector11_mut(actor)?.word_1ae |= 1;
            self.derived_actor_mut(actor)?.target_148 = None;
        }
        if self.selector11_actor(actor)?.word_1b0 == 0 {
            let found = if self.selector11_actor(actor)?.word_1ae & 4 != 0 {
                self.selector11_scan_b(actor)?
            } else {
                self.selector11_scan_a(actor, host)?
            };
            if let Some(found) = found {
                // 0040173B..76: a 0x20 type-2 target through 004825D0(handle, 0).
                let handle = self.actor(found)?.handle;
                let allocation = self
                    .allocate_actor_service(ActorServiceAllocationRequest {
                        kind: ActorServiceResourceKind::Target(NativeTargetClass::ActorFollow),
                    })
                    .map_err(ObjectError::from)?;
                if let Some(reference) = allocation {
                    let data = self.follow_target(FollowTargetRequest::resolve_actor(handle))?;
                    self.construct_native_target(reference, NativeTarget::ActorFollow(data))?;
                }
                self.derived_actor_mut(actor)?.target_148 = allocation;
                if allocation.is_some() {
                    let state = self.selector11_mut(actor)?;
                    state.word_1ae &= !1;
                    state.word_184 = Some(handle.raw());
                    state.word_190 = 1_000_000;
                    if state.word_1ae & 2 != 0 {
                        // 004017CE: 0047C840 on this actor with the found handle.
                        host.retaliation(self, actor, handle).map_err(|error| {
                            Selector11UpdateError::Host {
                                address: 0x0040_17d1,
                                error,
                            }
                        })?;
                    }
                    return Ok(());
                }
            }
        }
        self.selector11_random_point(actor, host)
    }

    /// 004017E8..00401906: a kind-8 point target near the actor.
    fn selector11_random_point<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        let mut point = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let state = self.selector11_actor(actor)?;
        if state.word_1ae & 0x10 == 0 || state.word_1b0 != 0 {
            let x = random(host, 0x0040_180a)? & 0x7ff;
            point[0] = point[0].wrapping_add(x as u16).wrapping_sub(0x400);
            let y = random(host, 0x0040_182c)? & 0x3ff;
            point[1] = point[1].wrapping_add(y as u16);
            let z = random(host, 0x0040_1849)? & 0x7ff;
            point[2] = point[2].wrapping_add(z as u16).wrapping_sub(0x400);
        }
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            self.release_actor_service(reference)?;
        }
        let allocation = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(ObjectError::from)?;
        if let Some(reference) = allocation {
            // 00401887: +04 is the current process NULL 0054E894.
            let null = host
                .process_null_handle()
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0x148,
                })?;
            let target = crate::object_runtime::FixedPositionTarget::from_point(null, Some(point));
            self.construct_native_target(reference, NativeTarget::FixedPosition(target))?;
        }
        self.derived_actor_mut(actor)?.target_148 = allocation;
        if self.selector11_actor(actor)?.word_1b0 == 0 {
            let random = random(host, 0x0040_18e7)? & 0xffff;
            let state = self.selector11_mut(actor)?;
            state.word_1ae |= 8;
            state.word_190 = random.wrapping_add(0x16e36) << 4;
        } else {
            let random = random(host, 0x0040_18be)? & 0xffff;
            self.selector11_mut(actor)?.word_190 = random.wrapping_mul(8).wrapping_add(250_000);
        }
        Ok(())
    }

    /// Scanner B 00475F50 with filter B 004014C0: X/Z range (the signed
    /// WORD +164 squared), flag 0x20 set and 0x80 clear, not itself. Filter
    /// B scores only a live candidate (+C4 3) whose virtual +74 answers,
    /// which of the represented classes is the falling rock alone
    /// (00404610): 00427620 gives the DWORD +164 less the root of the
    /// squared distance (0048EFC0). The strictly best score (unsigned)
    /// wins; zero finds none.
    fn selector11_scan_b<E>(
        &mut self,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, Selector11UpdateError<E>> {
        let range = self.derived_actor(actor)?.word_164 as i16 as i32;
        let range_squared = range.wrapping_mul(range) as u32;
        let source = self.selector11_position(actor)?;
        let (mut best, mut best_score) = (None, 0_u32);
        for candidate in self.actor_list().to_vec() {
            let flags = self.object(candidate)?.base.flags;
            if flags & 0x80 != 0 || flags & 0x20 != 0x20 {
                continue;
            }
            let other = self.selector11_position(candidate)?;
            let dx = i32::from(source[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(source[2].wrapping_sub(other[2]) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= range_squared || candidate == actor {
                continue;
            }
            // 004014C7: +C4 3, then the candidate's virtual +74.
            if self.actor(candidate)?.word_c4 != 3 || !actor_virtual_74(self, candidate)? {
                continue;
            }
            let root = isqrt_48efc0(squared as i32);
            let score = (self.derived_actor(actor)?.word_164 as u32).wrapping_sub(root);
            if score > best_score {
                (best, best_score) = (Some(candidate), score);
            }
        }
        Ok(best)
    }

    /// Scanner A 00476180 with filter A 004013B0. Candidates with a nonzero
    /// score are tried in list order; a higher score wins only when no
    /// nearer candidate occludes it (00475FF0).
    fn selector11_scan_a<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<Option<ObjectId>, Selector11UpdateError<H::Error>> {
        let range = self.derived_actor(actor)?.word_164 as i16 as i32;
        let range_squared = range.wrapping_mul(range) as u32;
        // 0047618F..C1: virtual +7C is 004027A0 (+140) for derived actors.
        let origin = if self.derived_actor(actor)?.behavior_140.is_some() {
            self.actor_behavior_position(
                actor,
                crate::object_runtime::ActorBehaviorPositionRequest::SelectedWeaponGroup,
            )?
        } else {
            self.selector11_position(actor)?
        };
        let mut zero = Vec::new();
        let mut scored = Vec::new();
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & 0xa4 != 0x24 {
                continue;
            }
            let other = self.selector11_position(candidate)?;
            let delta: [i16; 3] =
                std::array::from_fn(|axis| other[axis].wrapping_sub(origin[axis]) as i16);
            let [x, y, z] = delta.map(i32::from);
            let squared = (x.wrapping_mul(x) as u32)
                .wrapping_add(y.wrapping_mul(y) as u32)
                .wrapping_add(z.wrapping_mul(z) as u32);
            if squared >= range_squared || candidate == actor {
                continue;
            }
            let score = self.selector11_filter_a(actor, candidate, squared, host)?;
            let entry = Candidate {
                actor: candidate,
                delta,
                distance_squared: squared,
                score,
            };
            if score != 0 {
                scored.push(entry);
            } else {
                zero.push(entry);
            }
        }
        let mut best: Option<Candidate> = None;
        // 004762C9..33: each tried candidate moves to the end of the first list.
        let mut tried = zero;
        for (index, candidate) in scored.iter().enumerate() {
            if candidate.score > best.map_or(0, |best| best.score) {
                let others = tried.iter().chain(&scored[index + 1..]);
                if !occluded(self, candidate, others)? {
                    best = Some(*candidate);
                }
            }
            tried.push(*candidate);
        }
        Ok(best.map(|best| best.actor))
    }

    /// 004013B0 for one candidate. Zero rejects.
    fn selector11_filter_a<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        candidate: ObjectId,
        distance_squared: u32,
        host: &mut H,
    ) -> Result<u32, Selector11UpdateError<H::Error>> {
        if self.actor(candidate)?.word_c4 != 3 {
            return Ok(0);
        }
        if self.actor(actor)?.rotating.visual.model_word == 0x41 {
            let position = self.selector11_position(candidate)?;
            let terrain = host
                .scan_terrain()
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0x42,
                })?;
            let ground = terrain.raw_height_at([position[0], position[2]]);
            let clearance = i32::from(position[1] as i16) - i32::from(ground);
            let vehicle = actor_virtual_64(self, candidate)?;
            if clearance < 0x80 {
                return Ok(0);
            }
            if let Some(vehicle) = vehicle {
                let state = self.selector2_actor(vehicle)?.dwords_1e8_1f4[0];
                if state != 4 && state != 5 {
                    return Ok(0);
                }
            }
        }
        if actor_virtual_54(self, candidate)? {
            return Ok(0);
        }
        let team = self.actor(actor)?.word_b4;
        let network_flag =
            host.scan_network_flag()
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0xb4,
                })?;
        let tasks = host
            .scan_tasks()
            .ok_or(Selector11UpdateError::RequiredTasks)?;
        let owner = if team < 0x13 {
            tasks.owner(team)?
        } else {
            None
        };
        let owner = owner.ok_or(Selector11UpdateError::MissingTask { actor, team })?;
        let relation = query_canonical_task_relation(
            self,
            tasks,
            &network_flag,
            TaskRelationRequest {
                owner,
                actor: candidate,
            },
        )?;
        if relation == 0 {
            return Ok(0);
        }
        // 00427620: +164 minus the root of the squared distance.
        let range = self.derived_actor(actor)?.word_164;
        let base = range.wrapping_sub(isqrt_48efc0(distance_squared as i32) as i32);
        Ok((base as u32).wrapping_mul(relation) >> 2)
    }

    fn selector11_position(&self, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }
}

/// 00475FF0: whether a nearer listed candidate lies across this one's ray.
fn occluded<'a>(
    arena: &ObjectArena,
    candidate: &Candidate,
    others: impl Iterator<Item = &'a Candidate>,
) -> Result<bool, ObjectError> {
    // 00417DA0 keeps FSQRT in a register for all three divisions.
    let direction =
        normalize_words(candidate.delta, NativeLengthPrecision::Retained).map(f64::from);
    for other in others {
        if other.distance_squared >= candidate.distance_squared {
            continue;
        }
        let [x, y, z] = other.delta.map(f64::from);
        let along = ((x * direction[0] + y * direction[1]) + z * direction[2]) as i32;
        if along <= 0 {
            continue;
        }
        let cross = [
            ((z * direction[1] - y * direction[2]) as i32) as i16,
            ((x * direction[2] - z * direction[0]) as i32) as i16,
            ((y * direction[0] - x * direction[1]) as i32) as i16,
        ];
        let [cx, cy, cz] = cross.map(i32::from);
        let perpendicular = (cx.wrapping_mul(cx) as u32)
            .wrapping_add(cz.wrapping_mul(cz) as u32)
            .wrapping_add(cy.wrapping_mul(cy) as u32);
        let size = u32::from(arena.object(other.actor)?.base.word_28);
        if perpendicular < (size >> 1).wrapping_mul(size) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Virtual +74: only the falling rock's table (00404610) returns itself.
fn actor_virtual_74(arena: &ObjectArena, actor: ObjectId) -> Result<bool, ObjectError> {
    Ok(matches!(
        represented_actor(arena, actor)?,
        ActorExtension::Selector44(_)
    ))
}

/// Virtual +54: only Selector30 (0048B170's table) returns itself.
fn actor_virtual_54(arena: &ObjectArena, actor: ObjectId) -> Result<bool, ObjectError> {
    Ok(matches!(
        represented_actor(arena, actor)?,
        ActorExtension::Selector30(_)
    ))
}

/// Virtual +64: only Selector2's table (00404610) returns itself.
fn actor_virtual_64(arena: &ObjectArena, actor: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
    Ok(match represented_actor(arena, actor)? {
        ActorExtension::Derived(derived) => {
            matches!(derived.class, DerivedActorClass::Selector2(_)).then_some(actor)
        }
        _ => None,
    })
}

/// Every actor final table the port constructs; the slots above were read
/// from each table (selectors 2, 6, 7, 10, 11, 12, 16, 30, 33, 34, 35, 36,
/// 60, 66, 77, 88, 90 and 93).
fn represented_actor(arena: &ObjectArena, actor: ObjectId) -> Result<&ActorExtension, ObjectError> {
    Ok(&arena.actor(actor)?.extension)
}
