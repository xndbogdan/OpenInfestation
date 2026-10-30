//! Selector12 virtual +104, 00468350. The cached radius square, carried EBX,
//! grant and interaction branches are distinct from the common +104 policy.

use super::{
    actor_selection_request, query_canonical_task_relation, FamilyActorListSelectionError,
};
use crate::object_runtime::actor::derived::Family429e20Class;
use crate::object_runtime::WeightedTargetConstructorNull;
use crate::object_runtime::{
    ActorComponentQueryMode, ActorComponentQueryRequest, ActorInteractionNotificationContext,
    ActorInteractionNotificationRequest, ActorInteractionRelationRequest, BehaviorPredicateContext,
    FamilyCandidateGateRequest, FamilySelectionCandidate, FamilySelectionHost,
    FamilySelectionRequest, FamilyTargetSelection, FamilyTargetSlot, ObjectArena, ObjectError,
    ObjectId,
};
use crate::process_tasks::{relation::TaskRelationRequest, TaskStorageError};

struct Selector12Selection {
    classification: u32,
    radius_squared: u32,
    quarter_radius_squared: u32,
    /// 004683DA seeds EBX once. The far, relation-zero arm writes nothing;
    /// later rows can consume a value left by an enemy, grant or interaction.
    carried_score: u32,
    primary: FamilyTargetSelection,
    secondary: FamilyTargetSelection,
    interaction: FamilyTargetSelection,
    grant: FamilyTargetSelection,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketCopy,
        BehaviorPacketSourceRequest, BehaviorPredicateHost, BehaviorTemporary,
        DerivedActorConstructionRequest, ProcessActorBehaviorTables, Selector12GrantContext,
    };
    use crate::process_particles::factories::ParticleFactoryProcessState;
    use crate::process_tasks::ProcessTaskRuntime;

    struct PureHost(ProcessTaskRuntime);

    impl FamilySelectionHost for PureHost {
        fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
            &mut self.0
        }

        fn animation_clock(&self) -> Option<&dyn crate::frontend_camera::AnimationClockSource> {
            None
        }
    }

    impl BehaviorPredicateHost for PureHost {
        type Error = ();
        type Particle = ();
        type Component = ();

        fn create(
            &mut self,
            _: &mut ObjectArena,
            _: &mut BehaviorPredicateContext<'_>,
            _: BehaviorCreationRequest,
        ) -> Result<Option<BehaviorTemporary<()>>, ()> {
            panic!("relation-zero phase must not create")
        }

        fn query_component(
            &mut self,
            _: &mut ObjectArena,
            _: &mut BehaviorPredicateContext<'_>,
            _: BehaviorComponentQuery<()>,
        ) -> Result<Option<()>, ()> {
            panic!("relation-zero phase must not query a gate component")
        }

        fn packet_source(
            &mut self,
            _: &ObjectArena,
            _: &BehaviorPredicateContext<'_>,
            _: BehaviorPacketSourceRequest<(), ()>,
        ) -> Result<Option<BehaviorPacketCopy>, ()> {
            panic!("relation-zero phase must not copy a gate packet")
        }

        fn delete_particle(
            &mut self,
            _: &mut ObjectArena,
            _: &mut BehaviorPredicateContext<'_>,
            _: (),
            _: u32,
        ) -> Result<(), ()> {
            panic!("relation-zero phase must not delete a temporary")
        }
    }

    #[test]
    fn near_zero_relation_executes_negative_carried_register_arithmetic() {
        let null = ActorHandle::from_raw(0x1234);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: null,
        })
        .unwrap();
        let request = |position| DerivedActorConstructionRequest {
            actor: ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some(position),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &[0; 4096],
                },
            },
            world_mask_limits: [3, 3],
        };
        let grants = inf_formats::GrantTable {
            virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
            records: Vec::new(),
        };
        let actor = arena
            .allocate_selector12_actor(
                request([0; 3]),
                &mut 0,
                Selector12GrantContext {
                    table: &grants,
                    first_player_grants: Some(&[0; 6]),
                },
            )
            .unwrap();
        let candidate = arena
            .allocate_derived_actor(request([8, 0, 0]), &mut 0)
            .unwrap();
        arena.actor_mut(actor).unwrap().word_b4 = 18;
        arena.actor_mut(candidate).unwrap().word_b4 = 1;
        arena.actor_mut(candidate).unwrap().word_c4 = 3;
        arena.owned_mut(candidate).unwrap().state.base.flags = 0x20;
        arena.derived_actor_mut(candidate).unwrap().behavior_140 =
            crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
        // Earlier captured radius1000 remains the range/quarter input. Fresh
        // radius4 makes the reached base -4. This is a private phase receipt;
        // public best-zero outcomes cannot distinguish carried -1 from zero.
        arena.derived_actor_mut(actor).unwrap().word_164 = 4;
        let mut selection = Selector12Selection {
            classification: 18,
            radius_squared: 1_000_000,
            quarter_radius_squared: 250_000,
            carried_score: 250_000,
            primary: null_selection(0),
            secondary: null_selection(0),
            interaction: null_selection(0),
            grant: null_selection(0),
        };
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .unwrap();
        let mut tables = ProcessActorBehaviorTables::from_executable(
            inf_formats::FrontendExecutable::parse(&raw).unwrap(),
        )
        .unwrap();
        let mut process = ParticleFactoryProcessState {
            process_null_handle: None,
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
        };
        let mut host = PureHost(ProcessTaskRuntime::default());
        host.0.retain(null).unwrap();
        arena
            .visit_selector12_actor_list_candidate::<PureHost>(
                actor,
                candidate,
                &mut selection,
                &mut BehaviorPredicateContext {
                    tables: &mut tables,
                    process: &mut process,
                    rng: &mut 0,
                },
                &mut host,
            )
            .unwrap();
        assert_eq!(selection.carried_score as i32, -1);
        assert_eq!(selection.secondary.score, 0);
        assert_eq!(
            selection.secondary.candidate,
            FamilySelectionCandidate::Null
        );
    }
}

fn null_selection(score: u32) -> FamilyTargetSelection {
    FamilyTargetSelection {
        score,
        candidate: FamilySelectionCandidate::Null,
    }
}

fn improve(selection: &mut FamilyTargetSelection, candidate: ObjectId, score: u32) {
    if (score as i32) > selection.score as i32 {
        selection.score = score;
        selection.candidate = FamilySelectionCandidate::Actor(candidate);
    }
}

impl ObjectArena {
    pub(super) fn select_selector12_actor_list_targets<H: FamilySelectionHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilyActorListSelectionError<H::Error>> {
        let actor = request.actor;
        // 00468363..394 captures B4/radius before actual self +7C. No self
        // position, task, source or common +20C/+210/+214 fields are read here.
        let classification = self.actor(actor)?.word_b4;
        let radius = self.derived_actor(actor)?.word_164 as u32;
        let radius_squared = radius.wrapping_mul(radius);
        let quarter_radius_squared = radius_squared >> 2;
        let primary = if self.actor_behavior_presence_7c(actor)? {
            0
        } else {
            i32::MAX as u32
        };
        let mut selection = Selector12Selection {
            classification,
            radius_squared,
            quarter_radius_squared,
            carried_score: quarter_radius_squared,
            primary: null_selection(primary),
            secondary: null_selection(0),
            interaction: null_selection(0),
            grant: null_selection(0),
        };
        let mut index = 0;
        while let Some(&candidate) = self.actor_list().get(index) {
            self.visit_selector12_actor_list_candidate::<H>(
                actor,
                candidate,
                &mut selection,
                context,
                host,
            )?;
            // 00468622 reloads the captured current node's live successor.
            // Same-arena append/removal is supported; raw owner/link aliases,
            // arena replacement and ObjectId ABA remain outside admission.
            index = self
                .actor_list()
                .iter()
                .position(|&value| value == candidate)
                .ok_or(FamilyActorListSelectionError::RemovedCurrentNode(candidate))?
                + 1;
        }

        let first = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1ec,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(selection.secondary)?
            },
        )?;
        // 00468657/81/A6 reread radius after each preceding publisher. EDI's
        // restored suffix value is the primary threshold, including NULL7C's
        // INT_MAX seed. All four NULL candidate locals were initialized.
        let interaction = self.adjust_selector12_actor_list_score(actor, selection.interaction)?;
        let second = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target274,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(interaction)?
            },
        )?;
        let grant = self.adjust_selector12_actor_list_score(actor, selection.grant)?;
        let third = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target27c,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(grant)?
            },
        )?;
        let primary = self.adjust_selector12_actor_list_score(actor, selection.primary)?;
        let fourth = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1f4,
            crate::object_runtime::WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..actor_selection_request(primary)?
            },
        )?;
        Ok(first || second || third || fourth)
    }

    fn adjust_selector12_actor_list_score<E>(
        &self,
        actor: ObjectId,
        selection: FamilyTargetSelection,
    ) -> Result<FamilyTargetSelection, FamilyActorListSelectionError<E>> {
        let radius = self.derived_actor(actor)?.word_164 as u32;
        Ok(FamilyTargetSelection {
            score: radius
                .wrapping_mul(2)
                .wrapping_add(((selection.score as i32) >> 1) as u32),
            candidate: selection.candidate,
        })
    }

    fn visit_selector12_actor_list_candidate<H: FamilySelectionHost>(
        &mut self,
        actor: ObjectId,
        candidate: ObjectId,
        selection: &mut Selector12Selection,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<(), FamilyActorListSelectionError<H::Error>> {
        // Portable containment preserves the native node and carried score;
        // incomplete constructor fields must not enter any selection policy.
        if self.construction_is_stopped(candidate)? {
            return Ok(());
        }
        if self.object(candidate)?.base.flags & 0xa0 != 0x20 {
            return Ok(());
        }
        let position = self.family_actor_list_position(candidate)?;
        let self_position = self.family_actor_list_position(actor)?;
        let delta: [i32; 3] = std::array::from_fn(|axis| {
            i32::from(position[axis].wrapping_sub(self_position[axis]) as i16)
        });
        let distance_squared = delta.into_iter().fold(0u32, |sum, value| {
            sum.wrapping_add(value.wrapping_mul(value) as u32)
        });
        if distance_squared >= selection.radius_squared {
            return Ok(());
        }

        let classification = self.actor(candidate)?.word_b4;
        if classification != selection.classification {
            if classification == 0 || (self.actor(candidate)?.word_c4 as i32) < 3 {
                if distance_squared < selection.quarter_radius_squared {
                    let id = u32::from(self.actor(candidate)?.word_7a);
                    // Native signed wrapped Y <512; negative Y has no abs gate.
                    if id != 0 && delta[1] < 512 {
                        let Family429e20Class::Selector12(state) =
                            &self.family429e20_actor(actor)?.class
                        else {
                            return Err(ObjectError::NotSelector12(actor).into());
                        };
                        if !inf_formats::grant_is_owned(&state.grants_24c.mask_04, id)? {
                            selection.carried_score =
                                self.family_radius_score(actor, distance_squared)?;
                            improve(&mut selection.grant, candidate, selection.carried_score);
                        }
                    }
                }
            } else if let Some(component) = self.actor_derived_component(candidate)? {
                // 00468487 directly captures +140, retaining presence/base
                // through the gate rather than calling candidate virtual7C.
                let present = self.derived_actor(component)?.behavior_140.is_some();
                let base = self.family_radius_score(actor, distance_squared)?;
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
                if relation != 0 {
                    selection.carried_score = base;
                    if (base as i32) > selection.primary.score as i32
                        && self.test_family_candidate_gate(
                            FamilyCandidateGateRequest {
                                actor,
                                candidate: Some(candidate),
                            },
                            context,
                            host,
                        )?
                    {
                        selection.primary.score = base;
                        selection.primary.candidate = FamilySelectionCandidate::Actor(candidate);
                    }
                    if !present {
                        selection.carried_score = ((selection.carried_score as i32) >> 2) as u32;
                    }
                } else if !present {
                    selection.carried_score = 0;
                } else if distance_squared < 0x90000 {
                    // CDQ/AND3/ADD/SAR2 then SAR2. Negative odd values make
                    // trunc(base/4)>>2 distinct from trunc(base/16).
                    selection.carried_score = (((base as i32) / 4) >> 2) as u32;
                }
                // The far relation-zero/present arm at00468502 writes no EBX.
                improve(&mut selection.secondary, candidate, selection.carried_score);
            }
        }

        if selection.classification == 1 {
            return Ok(());
        }
        let component = self.query_actor_component(ActorComponentQueryRequest {
            actor: candidate,
            mode: ActorComponentQueryMode::Interaction54,
        })?;
        if component.is_none() && self.object(candidate)?.base.flags & 0x1000_0080 != 0x1000_0000 {
            return Ok(());
        }
        {
            // Acquire the actual clock after any preceding mutable gate and
            // release this pure borrow before further task/service access.
            self.notify_actor_interaction(
                ActorInteractionNotificationRequest {
                    candidate,
                    source: Some(actor),
                    distance_squared,
                },
                ActorInteractionNotificationContext {
                    animation_clock: host.animation_clock(),
                },
            )?;
        }
        if let Some(component) = component {
            let relation =
                self.query_actor_interaction_relation(ActorInteractionRelationRequest {
                    component,
                    source: Some(actor),
                })?;
            selection.carried_score = if relation == 0 {
                0
            } else if self
                .query_actor_component(ActorComponentQueryRequest {
                    actor,
                    mode: ActorComponentQueryMode::Self50,
                })?
                .is_none()
                && distance_squared < 0x10000
            {
                // Native passes the actual NULL returned by +50 as distance0.
                self.family_radius_score(actor, 0)?.wrapping_shl(2)
            } else {
                ((self.family_radius_score(actor, distance_squared)? as i32) / relation as i32)
                    as u32
            };
        } else {
            selection.carried_score = if self.actor(actor)?.word_b4 == 18 {
                0
            } else {
                self.family_radius_score(actor, distance_squared)?
            };
        }
        improve(
            &mut selection.interaction,
            candidate,
            selection.carried_score,
        );
        Ok(())
    }
}
