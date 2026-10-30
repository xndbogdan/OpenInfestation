//! Actor-list virtual +104: common 00428F40 and selector12's 00468350.
//! Their three-axis WORD metrics retain distinct selection/publication policies.
//! Canonical query borrows end before the actual mutable candidate gate.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::{Family428650Class, Family429e20Class, FamilyTargetSlot};
use crate::object_runtime::{
    ActorConstructionError, ActorExtension, ActorHandle, ActorInteractionError,
    BehaviorPredicateContext, DerivedActorClass, FamilyCandidateGateError,
    FamilyCandidateGateRequest, FamilySelectionCandidate, FamilySelectionHost,
    FamilySelectionRequest, FamilyTargetSelection, NativeTargetError, ObjectArena, ObjectError,
    ObjectId, WeightedTargetConstructorNull, WeightedTargetReferent, WeightedTargetRequest,
};
use crate::process_tasks::{
    relation::{TaskRelationContext, TaskRelationError, TaskRelationHost, TaskRelationRequest},
    ProcessTaskRuntime, TaskOwner, TaskStorageError,
};

mod selector12;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyActorListSelectionError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
    #[error(transparent)]
    Task(#[from] TaskStorageError),
    #[error(transparent)]
    Relation(#[from] TaskRelationError<ObjectError>),
    #[error(transparent)]
    Gate(#[from] FamilyCandidateGateError<E>),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error(transparent)]
    Interaction(#[from] ActorInteractionError),
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    #[error("actor-list +104 requires actor {0:?}'s position WORDs")]
    MissingPosition(ObjectId),
    #[error("common +104 requires current process-null at {address:08X}")]
    MissingProcessNull { address: u32 },
    #[error("actor-list +104's captured actor-list node {0:?} was removed")]
    RemovedCurrentNode(ObjectId),
    #[error("nonzero selection score {score:08X} has an unwritten candidate")]
    UnwrittenCandidate { score: u32 },
}

/// Only the proven pure canonical +5C/+7C bodies are composed here. The
/// standalone task-relation owner retains its broader mutable host boundary.
struct CanonicalTaskQueries<'a> {
    network_flag: &'a u32,
}

impl TaskRelationHost for CanonicalTaskQueries<'_> {
    type Error = ObjectError;
    type Component = ObjectId;

    fn network_flag(&self) -> Option<u32> {
        Some(*self.network_flag)
    }

    fn actor_component_5c(
        &mut self,
        context: TaskRelationContext<'_>,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, ObjectError> {
        // Only selector12's represented final table has +5C = 00404610.
        // All other represented final tables have the pure null 00401300.
        Ok(match &context.arena.actor(actor)?.extension {
            ActorExtension::Derived(state) => match &state.class {
                DerivedActorClass::Family428650(state) => match &state.class {
                    Family428650Class::Family429e20(state) => match &state.class {
                        Family429e20Class::Selector12(_) => Some(actor),
                        Family429e20Class::Base | Family429e20Class::Family4236d0(_) => None,
                    },
                    Family428650Class::Base
                    | Family428650Class::Selector34(_)
                    | Family428650Class::Selector35(_) => None,
                },
                DerivedActorClass::Base
                | DerivedActorClass::Selector2(_)
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => None,
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => None,
        })
    }

    fn component_presence_7c(
        &mut self,
        context: TaskRelationContext<'_>,
        component: ObjectId,
    ) -> Result<u32, ObjectError> {
        // 004027A0 returns the actual +140 pointer. The relation consumes
        // only full-DWORD presence; no fabricated allocation address is used.
        Ok(u32::from(
            context.arena.actor_behavior_presence_7c(component)?,
        ))
    }
}

/// Complete canonical task-query phase. Its live network/arena/task borrows
/// end before any candidate gate. Owner selection remains caller-specific.
pub(in crate::object_runtime::actor::derived) fn query_canonical_task_relation(
    arena: &mut ObjectArena,
    tasks: &mut ProcessTaskRuntime,
    network_flag: &u32,
    request: TaskRelationRequest,
) -> Result<u32, TaskRelationError<ObjectError>> {
    tasks.query_relation(request, arena, &mut CanonicalTaskQueries { network_flag })
}

struct ActorListSelection {
    classification: u32,
    primary_enabled: bool,
    task: TaskOwner,
    task_source: ActorHandle,
    radius_squared: u32,
    center: [u16; 3],
    primary: FamilyTargetSelection,
    reverse: FamilyTargetSelection,
    group_sum: [u32; 3],
    group_count: u32,
}

impl ObjectArena {
    /// Dispatch the actual represented +104 class: common 00428F40 or
    /// selector12 00468350. Both native stack arguments are unused. Static
    /// selector13 has no represented constructor; no update dispatch attaches.
    ///
    /// This same typed arena represents the captured native game/list owner.
    /// Its live actor list admits canonical appends/removal of other nodes.
    /// Removing the captured current node is a reached custody error; arena
    /// replacement, ObjectId ABA, raw global-owner/list aliases, dangling reads
    /// and allocator rebinding are outside this admission.
    pub fn select_family_actor_list_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilyActorListSelectionError<H::Error>> {
        let actor = request.actor;
        // Actual class custody selects the body independently of mutable B4.
        if matches!(&self.family428650_actor(actor)?.class, Family428650Class::Family429e20(state) if matches!(state.class, Family429e20Class::Selector12(_)))
        {
            return self.select_selector12_actor_list_targets(request, context, host);
        }
        self.select_common_actor_list_targets(request, context, host)
    }

    fn select_common_actor_list_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilyActorListSelectionError<H::Error>> {
        let actor = request.actor;
        // 00428F54: capture pure self +7C before radius/class, then the class
        // task's +08 and separately the actual task allocation before center.
        let primary_enabled = self.actor_behavior_presence_7c(actor)?;
        let radius = self.derived_actor(actor)?.word_164 as u32;
        let radius_squared = radius.wrapping_mul(radius);
        let classification = self.actor(actor)?.word_b4;
        let (task_source, task) = {
            let tasks = host.process_tasks();
            let owner = tasks
                .owner(classification)?
                .ok_or(TaskStorageError::NullOwner(classification))?;
            let source = tasks.record(owner)?.source;
            let task = tasks
                .owner(classification)?
                .ok_or(TaskStorageError::NullOwner(classification))?;
            (source, task)
        };
        let center = self.family_actor_list_center(actor, context)?;
        let mut selection = ActorListSelection {
            classification,
            primary_enabled,
            task,
            task_source,
            radius_squared,
            center,
            primary: FamilyTargetSelection::EMPTY,
            reverse: FamilyTargetSelection::EMPTY,
            group_sum: [0; 3],
            group_count: 0,
        };
        let mut index = 0;
        while let Some(&candidate) = self.actor_list().get(index) {
            self.visit_family_actor_list_candidate(
                actor,
                candidate,
                &mut selection,
                context,
                host,
            )?;
            // 00429231: reload this captured node's current successor after
            // all callbacks. A snapshot of the whole pass would lose appends.
            index = self
                .actor_list()
                .iter()
                .position(|&value| value == candidate)
                .ok_or(FamilyActorListSelectionError::RemovedCurrentNode(candidate))?
                + 1;
        }

        let (mut group_point, mut group_score) = (None, 0);
        if (selection.group_count as i32) > 1 {
            let means = selection
                .group_sum
                .map(|sum| (sum as i32) / selection.group_count as i32);
            let squared = means.into_iter().fold(0u32, |sum, mean| {
                sum.wrapping_add(mean.wrapping_mul(mean) as u32)
            });
            if squared > 0x20000 {
                // 00429285..29D writes the point before signed-input isqrt.
                // A high-bit squared value still consumes fresh self XYZ,
                // even though 0048EFC0 then returns zero.
                let position = self.family_actor_list_position(actor)?;
                group_point = Some(std::array::from_fn(|axis| {
                    position[axis].wrapping_add(means[axis] as u16)
                }));
                group_score = isqrt_48efc0(squared as i32);
            }
        }
        // 004292AB reads fresh null before +20C; +214 is consumed only when
        // unequal. Retain the captured primary candidate while boosting score.
        let null = context.process.process_null_handle.ok_or(
            FamilyActorListSelectionError::MissingProcessNull {
                address: 0x0042_92ab,
            },
        )?;
        if self.family428650_actor(actor)?.word_20c != null.raw() {
            let boost = self.family428650_actor(actor)?.word_214;
            selection.primary.score = selection
                .primary
                .score
                .wrapping_add(((boost.wrapping_mul(selection.primary.score) as i32) >> 10) as u32);
        }

        let first = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1f4,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(selection.primary)?
            },
        )?;
        let second = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1ec,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(selection.reverse)?
            },
        )?;
        // The native pointer is always nonnull local storage. At score zero
        // its possibly unwritten bytes are unused; Point(None) represents only
        // that semantic bypass, not null pointer/local-byte parity.
        let third = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1e4,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Point(group_point),
                score: group_score,
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
            },
        )?;
        Ok(first || second || third)
    }

    fn family_actor_list_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], FamilyActorListSelectionError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(FamilyActorListSelectionError::MissingPosition(actor))
    }

    fn family_actor_list_center<E>(
        &mut self,
        actor: ObjectId,
        context: &BehaviorPredicateContext<'_>,
    ) -> Result<[u16; 3], FamilyActorListSelectionError<E>> {
        let source = ActorHandle::from_raw(self.family428650_actor(actor)?.word_20c);
        let null = context.process.process_null_handle.ok_or(
            FamilyActorListSelectionError::MissingProcessNull {
                address: 0x0042_8fc6,
            },
        )?;
        if source == null {
            return self.family_actor_list_position(actor);
        }
        if let Some(reference) = self.resolve_actor_handle(source)? {
            let reference_position = self.family_actor_list_position(reference)?;
            let position = self.family_actor_list_position(actor)?;
            return Ok(std::array::from_fn(|axis| {
                let delta =
                    i32::from(position[axis] as i16) - i32::from(reference_position[axis] as i16);
                reference_position[axis].wrapping_add((delta >> 2) as u16)
            }));
        }
        let null = context.process.process_null_handle.ok_or(
            FamilyActorListSelectionError::MissingProcessNull {
                address: 0x0042_9024,
            },
        )?;
        // Preserve the distinct +104 order: +20C before +210, then XYZ.
        self.family428650_actor_mut(actor)?.word_20c = null.raw();
        self.family428650_actor_mut(actor)?.word_210 = 0;
        self.family_actor_list_position(actor)
    }

    fn visit_family_actor_list_candidate<H: FamilySelectionHost>(
        &mut self,
        actor: ObjectId,
        candidate: ObjectId,
        selection: &mut ActorListSelection,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<(), FamilyActorListSelectionError<H::Error>> {
        // A retained constructor prefix keeps its native node, but must not
        // become a target or contribute to the group until construction owns it.
        if self.construction_is_stopped(candidate)? {
            return Ok(());
        }
        if self.object(candidate)?.base.flags & 0xa0 != 0x20 {
            return Ok(());
        }
        let position = self.family_actor_list_position(candidate)?;
        let delta: [i32; 3] = std::array::from_fn(|axis| {
            i32::from(position[axis].wrapping_sub(selection.center[axis]) as i16)
        });
        let distance_squared = delta.into_iter().fold(0u32, |sum, value| {
            sum.wrapping_add(value.wrapping_mul(value) as u32)
        });
        if distance_squared >= selection.radius_squared {
            return Ok(());
        }
        let classification = self.actor(candidate)?.word_b4;
        if classification == selection.classification {
            if let Some(component) = self.actor_derived_component(candidate)? {
                let velocity = self.actor(component)?.words_8e_92;
                for axis in 0..3 {
                    selection.group_sum[axis] = selection.group_sum[axis]
                        .wrapping_add(delta[axis] as u32)
                        .wrapping_add(i32::from(velocity[axis] as i16) as u32);
                }
                selection.group_count = selection.group_count.wrapping_add(1);
            }
            return Ok(());
        }
        if classification == 0 {
            return Ok(());
        }
        let Some(component) = self.actor_derived_component(candidate)? else {
            return Ok(());
        };
        // Direct +140 after +58, not candidate virtual +7C. Preserve this
        // captured presence and fresh-radius base across the mutable gate.
        let candidate_present = self.derived_actor(component)?.behavior_140.is_some();
        let base = self.family_radius_score(actor, distance_squared)?;
        let relation = {
            query_canonical_task_relation(
                self,
                host.process_tasks(),
                &context.process.network_flag,
                TaskRelationRequest {
                    owner: selection.task,
                    actor: candidate,
                },
            )?
        };
        if relation != 0 && selection.primary_enabled {
            let mut score = (relation.wrapping_mul(base) as i32) / 4;
            if !candidate_present {
                score >>= 1;
            }
            if self.actor(candidate)?.word_c4 as i32 > 1 {
                score |= 0xc000;
                if score > selection.primary.score as i32
                    && self.actor(candidate)?.handle != selection.task_source
                    && self.test_family_candidate_gate(
                        FamilyCandidateGateRequest {
                            actor,
                            candidate: Some(candidate),
                        },
                        context,
                        host,
                    )?
                {
                    selection.primary.score = score as u32;
                    selection.primary.candidate = FamilySelectionCandidate::Actor(candidate);
                }
            }
        }
        // 004291E7: this class/task is fresh after the gate, while group
        // comparisons retain the caller's original classification.
        let classification = self.actor(candidate)?.word_b4;
        let relation = {
            let tasks = host.process_tasks();
            let owner = tasks
                .owner(classification)?
                .ok_or(TaskStorageError::NullOwner(classification))?;
            query_canonical_task_relation(
                self,
                tasks,
                &context.process.network_flag,
                TaskRelationRequest { owner, actor },
            )?
        };
        let mut score = (relation.wrapping_mul(base) as i32) / 4;
        if !candidate_present {
            score >>= 2;
        }
        if score > selection.reverse.score as i32 {
            selection.reverse.score = score as u32;
            selection.reverse.candidate = FamilySelectionCandidate::Actor(candidate);
        }
        Ok(())
    }
}

fn actor_selection_request<E>(
    selection: FamilyTargetSelection,
) -> Result<WeightedTargetRequest<'static>, FamilyActorListSelectionError<E>> {
    let actor = match selection.candidate {
        FamilySelectionCandidate::Actor(actor) => Some(actor),
        FamilySelectionCandidate::Null => None,
        FamilySelectionCandidate::Unwritten if selection.score == 0 => None,
        FamilySelectionCandidate::Unwritten => {
            return Err(FamilyActorListSelectionError::UnwrittenCandidate {
                score: selection.score,
            });
        }
    };
    Ok(WeightedTargetRequest {
        referent: WeightedTargetReferent::Actor(actor),
        score: selection.score,
        constructor_null: WeightedTargetConstructorNull::ActorRegistry,
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod selector12_native_receipts;
