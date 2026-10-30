//! Common +100 selection 00428DB0, distinct selector12 selection 004686E0,
//! scanner 00428BB0 and shared inline-table services 00482F50/00482FD0/00482D50.
//! No actor update or scene attaches these kernels implicitly.

use crate::object_runtime::WeightedTargetConstructorNull;
use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{Family428650Class, Family429e20Class, FamilyTargetSlot};
use crate::frontend_camera::AnimationClockSource;
use crate::object_runtime::{
    ActorConstructionError, ActorHandle, BehaviorParticleAdapter, BehaviorParticleServices,
    BehaviorPredicateContext, BehaviorPredicateHost, FamilyCandidateGateError,
    FamilyCandidateGateRequest, NativeTargetError, ObjectArena, ObjectError, ObjectId,
    WeightedTargetReferent, WeightedTargetRequest,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_tasks::{ProcessTaskRuntime, TaskStorageError};

/// The captured inline +1A0 allocation, including selector12's identical
/// storage. Row borrows never survive a candidate callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTargetTable {
    pub owner: ObjectId,
}

/// Native score-zero locals have no pointer writer. Keep that custody
/// distinct from a supplied null pointer and a selected live actor identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilySelectionCandidate {
    Unwritten,
    Null,
    Actor(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTargetSelection {
    pub score: u32,
    pub candidate: FamilySelectionCandidate,
}

impl FamilyTargetSelection {
    pub const EMPTY: Self = Self {
        score: 0,
        candidate: FamilySelectionCandidate::Unwritten,
    };

    fn improve(&mut self, local: Self) {
        if (self.score as i32) < local.score as i32 {
            // 00428D69/85 and0046879F/A3 write score before candidate.
            self.score = local.score;
            self.candidate = local.candidate;
        }
    }

    fn weighted_actor_request<E>(
        self,
    ) -> Result<WeightedTargetRequest<'static>, FamilySelectionError<E>> {
        let referent = match self.candidate {
            FamilySelectionCandidate::Actor(actor) => Some(actor),
            FamilySelectionCandidate::Null => None,
            FamilySelectionCandidate::Unwritten if self.score == 0 => None,
            FamilySelectionCandidate::Unwritten => {
                return Err(FamilySelectionError::UnwrittenCandidate { score: self.score })
            }
        };
        // At score0 the native publisher never dereferences the raw candidate
        // bits. None is an unused semantic referent, not a native pointer write.
        Ok(WeightedTargetRequest {
            constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            referent: WeightedTargetReferent::Actor(referent),
            score: self.score,
        })
    }
}

pub enum FamilyTargetScanOutputs<'a> {
    PrimaryOnly(&'a mut FamilyTargetSelection),
    Both {
        primary: &'a mut FamilyTargetSelection,
        secondary: &'a mut FamilyTargetSelection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTargetScanRequest {
    pub actor: ObjectId,
    pub table: FamilyTargetTable,
    /// Actual caller-owned XYZ WORDs; the scanner reads only X and Z.
    pub center_words: [u16; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilySelectionRequest {
    pub actor: ObjectId,
}

/// The live task owner belongs with the host's mutable services. Each getter
/// borrow ends before the candidate gate, allowing a later scanner to observe
/// task-source changes made by that same host. No task snapshot is installed.
/// The getter exposes the existing owner and has no synchronous side effects.
pub trait FamilySelectionHost: BehaviorPredicateHost {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime;

    /// Pure borrow of the actual process clock, acquired only at a reached
    /// notification. No clock value is copied or held across mutable gates.
    fn animation_clock(&self) -> Option<&dyn AnimationClockSource>;
}

/// Narrow storage custody for the concrete particle adapter's services.
/// Mutable notice/impact methods on these same services can change the actual
/// task and clock owners. Both getters are pure and replace neither owner.
pub trait FamilySelectionServices {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime;
    fn animation_clock(&self) -> Option<&dyn AnimationClockSource>;
}

impl<S: BehaviorParticleServices + FamilySelectionServices> FamilySelectionHost
    for BehaviorParticleAdapter<'_, '_, S>
{
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
        FamilySelectionServices::process_tasks(self.services)
    }

    fn animation_clock(&self) -> Option<&dyn AnimationClockSource> {
        FamilySelectionServices::animation_clock(self.services)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyTargetTableError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
    #[error("family table {owner:?} reached unrepresented entry {index}")]
    UnrepresentedEntry { owner: ObjectId, index: u32 },
    #[error("family table requires current process-null at {address:08X}")]
    MissingProcessNull { address: u32 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyTargetScanError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Table(#[from] FamilyTargetTableError),
    #[error(transparent)]
    Task(#[from] TaskStorageError),
    #[error(transparent)]
    Gate(#[from] FamilyCandidateGateError<E>),
    #[error("target scanner requires actor {0:?}'s position WORDs")]
    MissingPosition(ObjectId),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilySelectionError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Table(#[from] FamilyTargetTableError),
    #[error(transparent)]
    Scan(#[from] FamilyTargetScanError<E>),
    #[error(transparent)]
    Gate(#[from] FamilyCandidateGateError<E>),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("family +100 requires actor {0:?}'s position WORDs")]
    MissingPosition(ObjectId),
    #[error("common +100 requires current process-null at {address:08X}")]
    MissingProcessNull { address: u32 },
    #[error("nonzero selection score {score:08X} has an unwritten candidate")]
    UnwrittenCandidate { score: u32 },
}

fn null_at(
    process: &ParticleFactoryProcessState,
    address: u32,
) -> Result<ActorHandle, FamilyTargetTableError> {
    process
        .process_null_handle
        .ok_or(FamilyTargetTableError::MissingProcessNull { address })
}

fn weighted_score(weight: u32, base: u32) -> u32 {
    if weight < 0x02d0_0000 {
        ((base as i32) >> 2) as u32
    } else {
        // The native unsigned reciprocal sequence divides the wrapped DWORD
        // product by 0x5A00. It is not a signed division or wider product.
        (weight >> 13).wrapping_mul(base) / 0x5a00
    }
}

impl ObjectArena {
    fn family_entry(
        &self,
        table: FamilyTargetTable,
        index: u32,
    ) -> Result<super::FamilyHandleEntry, FamilyTargetTableError> {
        self.family428650_actor(table.owner)?
            .entries_1a4
            .get(index as usize)
            .copied()
            .ok_or(FamilyTargetTableError::UnrepresentedEntry {
                owner: table.owner,
                index,
            })
    }

    /// 00482F50 does not inspect count or guard the index. Positive storage
    /// beyond the four represented rows is an admission boundary, reached
    /// lazily after earlier writes, rather than an invented native check.
    pub fn resolve_family_table_entry(
        &mut self,
        table: FamilyTargetTable,
        index: u32,
        process: &ParticleFactoryProcessState,
    ) -> Result<Option<ObjectId>, FamilyTargetTableError> {
        let handle = self.family_entry(table, index)?.handle;
        let resolved = self.resolve_actor_handle(handle)?;
        if let Some(actor) = resolved {
            if self.actor(actor)?.word_c4 as i32 >= 1 {
                return Ok(Some(actor));
            }
        }
        let null = null_at(process, 0x0048_2f73)?;
        self.family428650_actor_mut(table.owner)?.entries_1a4[index as usize].handle = null;
        Ok(None)
    }

    /// Actual 00482FD0 unsigned-index guard precedes every table/null read.
    pub fn clear_family_table_entry(
        &mut self,
        table: FamilyTargetTable,
        index: u32,
        process: &ParticleFactoryProcessState,
    ) -> Result<(), FamilyTargetTableError> {
        if index < 4 {
            let null = null_at(process, 0x0048_2fd9)?;
            self.family428650_actor_mut(table.owner)?.entries_1a4[index as usize].handle = null;
        }
        Ok(())
    }

    /// 00482F90: the first row holding `handle` takes the process NULL and
    /// the table is pruned (00482D50). A table without it is left alone.
    pub fn forget_family_table_handle(
        &mut self,
        table: FamilyTargetTable,
        handle: ActorHandle,
        process: &ParticleFactoryProcessState,
    ) -> Result<(), FamilyTargetTableError> {
        let count = self.family428650_actor(table.owner)?.count_1a0 as i32;
        let mut index = 0;
        while index < count {
            if self.family_entry(table, index as u32)?.handle == handle {
                let null = null_at(process, 0x0048_2fb2)?;
                self.family428650_actor_mut(table.owner)?.entries_1a4[index as usize].handle = null;
                return self.prune_family_target_table(table, process);
            }
            index += 1;
        }
        Ok(())
    }

    /// 00482D50's intentional instruction asymmetry: when indices differ,
    /// copy the earlier survivor row INTO the current row, not the converse.
    /// Tail handle clearing retains both weight DWORDs and counts the nulls
    /// observed before each reverse copy.
    pub fn prune_family_target_table(
        &mut self,
        table: FamilyTargetTable,
        process: &ParticleFactoryProcessState,
    ) -> Result<(), FamilyTargetTableError> {
        let (mut input, mut survivor, mut dead) = (0u32, 0u32, 0u32);
        while (input as i32) < self.family428650_actor(table.owner)?.count_1a0 as i32 {
            let null = null_at(process, 0x0048_2d6e)?;
            if self.family_entry(table, input)?.handle == null {
                dead = dead.wrapping_add(1);
            } else {
                if input != survivor {
                    let handle = self.family_entry(table, survivor)?.handle;
                    self.family428650_actor_mut(table.owner)?.entries_1a4[input as usize].handle =
                        handle;
                    let word = self.family_entry(table, survivor)?.word_04;
                    self.family428650_actor_mut(table.owner)?.entries_1a4[input as usize].word_04 =
                        word;
                    let word = self.family_entry(table, survivor)?.word_08;
                    self.family428650_actor_mut(table.owner)?.entries_1a4[input as usize].word_08 =
                        word;
                }
                survivor = survivor.wrapping_add(1);
            }
            input = input.wrapping_add(1);
        }
        while (survivor as i32) < self.family428650_actor(table.owner)?.count_1a0 as i32 {
            let null = null_at(process, 0x0048_2dc2)?;
            self.family_entry(table, survivor)?;
            self.family428650_actor_mut(table.owner)?.entries_1a4[survivor as usize].handle = null;
            survivor = survivor.wrapping_add(1);
        }
        let family = self.family428650_actor_mut(table.owner)?;
        family.count_1a0 = family.count_1a0.wrapping_sub(dead);
        Ok(())
    }

    /// Shared 00427620 owner. Scanner and periodic producers deliberately
    /// retain their distinct distance calculations before this scalar call.
    pub(in crate::object_runtime::actor) fn family_radius_score(
        &self,
        actor: ObjectId,
        distance_squared: u32,
    ) -> Result<u32, ObjectError> {
        let radius = self.derived_actor(actor)?.word_164 as u32;
        Ok(radius.wrapping_sub(isqrt_48efc0(distance_squared as i32)))
    }

    pub fn scan_family_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilyTargetScanRequest,
        outputs: FamilyTargetScanOutputs<'_>,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<(), FamilyTargetScanError<H::Error>> {
        let actor = request.actor;
        let radius = self.derived_actor(actor)?.word_164 as u32;
        let radius_squared = radius.wrapping_mul(radius);
        // Admission of the exact family query tables, including selector12's
        // identical +7C. Other derived-class virtuals are not inferred here.
        self.family428650_actor(actor)?;
        // Canonical +7C = 004027A0, after the radius capture. Its raw result
        // enables primary scoring even if a later gate replaces behavior.
        let primary_enabled = self.actor_behavior_presence_7c(actor)?;
        let class = self.actor(actor)?.word_b4;
        let task_source = {
            let tasks = host.process_tasks();
            let owner = tasks
                .owner(class)?
                .ok_or(TaskStorageError::NullOwner(class))?;
            tasks.record(owner)?.source
        };
        let secondary_enabled = matches!(&outputs, FamilyTargetScanOutputs::Both { .. });
        let (mut primary, mut secondary) =
            (FamilyTargetSelection::EMPTY, FamilyTargetSelection::EMPTY);
        let (mut index, mut dirty) = (0u32, false);
        while (index as i32) < self.family428650_actor(request.table.owner)?.count_1a0 as i32 {
            let candidate =
                self.resolve_family_table_entry(request.table, index, context.process)?;
            let invalid = match candidate {
                Some(candidate) => self.actor(candidate)?.word_c4 as i32 <= 1,
                None => true,
            };
            if invalid {
                self.clear_family_table_entry(request.table, index, context.process)?;
                dirty = true;
                index = index.wrapping_add(1);
                continue;
            }
            let candidate = candidate.expect("nonnull resolved lifecycle > 1");
            let position = self
                .object(candidate)?
                .base
                .position_words
                .ok_or(FamilyTargetScanError::MissingPosition(candidate))?;
            let x = i32::from(request.center_words[0] as i16) - i32::from(position[0] as i16);
            let z = i32::from(request.center_words[2] as i16) - i32::from(position[2] as i16);
            let distance_squared =
                (x.wrapping_mul(x) as u32).wrapping_add(z.wrapping_mul(z) as u32);
            if distance_squared < radius_squared && self.actor(candidate)?.handle != task_source {
                let base = self.family_radius_score(actor, distance_squared)?;
                if primary_enabled {
                    let weight = self.family_entry(request.table, index)?.word_08;
                    if weight != 0 {
                        let score = if weight >= 0x0b40_0000 {
                            base
                        } else {
                            weighted_score(weight, base)
                        };
                        if (score as i32) > primary.score as i32
                            && self.test_family_candidate_gate(
                                FamilyCandidateGateRequest {
                                    actor,
                                    candidate: Some(candidate),
                                },
                                context,
                                host,
                            )?
                        {
                            primary = FamilyTargetSelection {
                                score,
                                candidate: FamilySelectionCandidate::Actor(candidate),
                            };
                        }
                    }
                }
                if secondary_enabled {
                    // Reached AFTER the mutable gate; retain base but reread
                    // the captured row's current secondary weight.
                    let weight = self.family_entry(request.table, index)?.word_04;
                    if weight < 0x0b40_0000 {
                        let score = weighted_score(weight, base);
                        if (score as i32) > secondary.score as i32 {
                            secondary = FamilyTargetSelection {
                                score,
                                candidate: FamilySelectionCandidate::Actor(candidate),
                            };
                        }
                    }
                }
            }
            index = index.wrapping_add(1);
        }
        if dirty {
            self.prune_family_target_table(request.table, context.process)?;
        }
        match outputs {
            FamilyTargetScanOutputs::PrimaryOnly(output) => output.improve(primary),
            FamilyTargetScanOutputs::Both {
                primary: first,
                secondary: second,
            } => {
                first.improve(primary);
                second.improve(secondary);
            }
        }
        Ok(())
    }

    /// Actual represented family virtual +100; its two native stack arguments
    /// are unused. Class custody selects distinct common/selector12 policies;
    /// mutable actor+B4 and scene identities do not select the callback.
    pub fn select_family_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilySelectionError<H::Error>> {
        let actor = request.actor;
        if matches!(&self.family428650_actor(actor)?.class, Family428650Class::Family429e20(state) if matches!(state.class, Family429e20Class::Selector12(_)))
        {
            return self.select_selector12_targets(request, context, host);
        }
        self.select_common_family_targets(request, context, host)
    }

    fn select_common_family_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilySelectionError<H::Error>> {
        let actor = request.actor;
        let mut center = self
            .object(actor)?
            .base
            .position_words
            .ok_or(FamilySelectionError::MissingPosition(actor))?;
        let null = context.process.process_null_handle.ok_or(
            FamilySelectionError::MissingProcessNull {
                address: 0x0042_8dd1,
            },
        )?;
        let source = ActorHandle::from_raw(self.family428650_actor(actor)?.word_20c);
        let (mut primary, mut secondary) =
            (FamilyTargetSelection::EMPTY, FamilyTargetSelection::EMPTY);
        if source != null {
            if let Some(reference) = self
                .resolve_actor_handle(source)
                .map_err(FamilyTargetTableError::Registry)?
            {
                let reference_position = self
                    .object(reference)?
                    .base
                    .position_words
                    .ok_or(FamilySelectionError::MissingPosition(reference))?;
                center = std::array::from_fn(|axis| {
                    let delta =
                        i32::from(center[axis] as i16) - i32::from(reference_position[axis] as i16);
                    reference_position[axis].wrapping_add((delta >> 2) as u16)
                });
                if let Some(component) = self.actor_damage_source_component(reference)? {
                    let table = FamilyTargetTable { owner: component };
                    if self.family428650_actor(component)?.count_1a0 != 0 {
                        self.scan_family_targets(
                            FamilyTargetScanRequest {
                                actor,
                                table,
                                center_words: center,
                            },
                            FamilyTargetScanOutputs::PrimaryOnly(&mut primary),
                            context,
                            host,
                        )?;
                        let boost = self.family428650_actor(actor)?.word_214;
                        primary.score = primary.score.wrapping_add(
                            ((primary.score.wrapping_mul(boost) as i32) >> 10) as u32,
                        );
                    }
                }
            } else {
                let null = context.process.process_null_handle.ok_or(
                    FamilySelectionError::MissingProcessNull {
                        address: 0x0042_8eab,
                    },
                )?;
                let family = self.family428650_actor_mut(actor)?;
                family.word_210 = 0;
                family.word_20c = null.raw();
            }
        }
        if self.family428650_actor(actor)?.count_1a0 != 0 {
            self.scan_family_targets(
                FamilyTargetScanRequest {
                    actor,
                    table: FamilyTargetTable { owner: actor },
                    center_words: center,
                },
                FamilyTargetScanOutputs::Both {
                    primary: &mut primary,
                    secondary: &mut secondary,
                },
                context,
                host,
            )?;
        }
        let first = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1d4,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..primary.weighted_actor_request()?
            },
        )?;
        let second = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1dc,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..secondary.weighted_actor_request()?
            },
        )?;
        Ok(first || second)
    }
}

mod selector12;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod selector12_native_receipts;

#[cfg(test)]
pub(super) mod native_support;
