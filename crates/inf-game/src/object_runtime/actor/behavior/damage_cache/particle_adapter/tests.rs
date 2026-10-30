use super::super::tests::{fixture, process, tables};
use super::*;
use crate::object_runtime::{
    BehaviorPacketSlot, BehaviorPredicateError, BehaviorPredicateRequest, BehaviorRowSelection,
    ObjectClassKind,
};
use crate::process_particles::{ParticleError, PARTICLE_POOL_SIZE};
use std::collections::VecDeque;

// Supplied native fixture fields, not production mutation APIs. The composed
// comparer lives beneath the pool so it can inspect private list/free custody.
impl ObjectArena {
    pub(crate) fn set_behavior_particle_receipt_behavior(
        &mut self,
        actor: ObjectId,
        behavior: crate::object_runtime::ActorBehaviorState,
    ) {
        self.derived_actor_mut(actor).unwrap().behavior_140 = Some(behavior);
    }

    pub(crate) fn set_behavior_particle_receipt_responses(
        &mut self,
        actor: ObjectId,
        payload: std::sync::Arc<inf_formats::ActorConstructorPayload>,
    ) {
        self.actor_mut(actor).unwrap().damage_table =
            crate::object_runtime::ActorDamageTable::Payload(payload);
    }
}

#[derive(Default)]
struct Services {
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    notices: Vec<ParticleCreationNotice>,
    mutate_notice: bool,
    fail_notice: bool,
    effects: usize,
    impacts: usize,
    mutate_impact: bool,
    owner: Option<ObjectId>,
    observed_scratch: Option<[Option<u8>; 8]>,
    actor_result: Option<Result<Option<ObjectId>, &'static str>>,
    queries: VecDeque<Option<u32>>,
    packets: VecDeque<BehaviorPacketCopy>,
}

impl BehaviorParticleServices for Services {
    type Error = &'static str;
    type Component = u32;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }
    fn create_actor_resource(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        request: BehaviorActorResourceRequest,
    ) -> Result<Option<ObjectId>, Self::Error> {
        assert_eq!(request.scratch_bytes, [None; 8]);
        self.actor_result.take().unwrap_or(Err("unowned460310"))
    }
    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, Self::Error> {
        self.queries.pop_front().ok_or("unowned component query")
    }
    fn component_packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        _: BehaviorComponentPacketSourceRequest<u32>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        self.packets
            .pop_front()
            .map(Some)
            .ok_or("unowned component packet")
    }
    fn particle_effect(
        &mut self,
        context: BehaviorParticleServiceContext<'_, '_, '_>,
        request: BehaviorParticleEffectRequest<'_>,
    ) -> Result<(), Self::Error> {
        self.effects += 1;
        let BehaviorParticleEffectRequest::NetworkNotice { notice, sources } = request else {
            let BehaviorParticleEffectRequest::Impact489600(request) = request else {
                unreachable!()
            };
            self.impacts += 1;
            if !self.mutate_impact {
                return Err("unowned impact489600");
            }
            context.factory.process.process_null_handle = Some(ActorHandle::from_raw(42));
            *context.factory.rng = self.impacts as u32;
            context.tables.default_packet_bytes = Some([Some(0x55); 4]);
            if let Some(owner) = self.owner {
                context
                    .arena
                    .owned_mut(owner)
                    .unwrap()
                    .state
                    .base
                    .position_words = Some([self.impacts as u16, 8, 9]);
            }
            let position = context
                .particles
                .record(request.particle)
                .unwrap()
                .position()
                .unwrap();
            context
                .particles
                .record_mut(request.particle)
                .unwrap()
                .write_position([position[0].wrapping_add(1), position[1], position[2]])
                .unwrap();
            return if self.impacts == 3 {
                Err("impact third stopped")
            } else {
                Ok(())
            };
        };
        self.notices.push(notice);
        assert_eq!(notice.velocity_source, ParticleVelocitySource::SamePosition);
        assert_eq!(sources.vectors.position.0, [None; 6]);
        assert_eq!(*sources.scratch_tail, [None; 2]);
        if self.mutate_notice {
            sources.vectors.position = [10, 20, 30].into();
            *sources.scratch_tail = [Some(0xab), Some(0xcd)];
            context.factory.process.process_null_handle = Some(ActorHandle::from_raw(9));
            context.factory.process.network_flag = 0;
            self.phase.set(Some(4));
            *context.factory.rng = 0x7654_3210;
            context.tables.default_packet_bytes = Some([Some(0x77); 4]);
            if let Some(owner) = self.owner {
                context
                    .arena
                    .owned_mut(owner)
                    .unwrap()
                    .state
                    .base
                    .position_words = Some([7, 8, 9]);
                let behavior = context
                    .arena
                    .derived_actor_mut(owner)
                    .unwrap()
                    .behavior_140
                    .as_mut()
                    .unwrap();
                behavior.table_14 = crate::object_runtime::ActorBehaviorTable::Mode03;
                behavior.byte_10 = 0x7f;
            }
        }
        self.observed_scratch = Some(std::array::from_fn(|i| {
            if i < 6 {
                sources.vectors.position.0[i]
            } else {
                sources.scratch_tail[i - 6]
            }
        }));
        if self.fail_notice {
            Err("notice stopped")
        } else {
            Ok(())
        }
    }
}

fn raw() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}

fn creation(callback: u32, context_word: u32, null_handle: ActorHandle) -> BehaviorCreationRequest {
    BehaviorCreationRequest::ParticleFactory {
        callback,
        context_word,
        null_handle,
        scratch_bytes: [None; 8],
    }
}

fn predicate(candidate: Option<ObjectId>) -> BehaviorPredicateRequest {
    BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
        candidate,
    }
}

#[test]
fn every_registry_particle_factory_composes_unknown_scratch_packets_and_normal_deletion() {
    let raw = raw();
    let exe = FrontendExecutable::parse(&raw).unwrap();
    for kind in [2u16, 3, 4, 5, 6, 7, 9, 30, 31] {
        let (mut arena, candidate) = fixture();
        let mut tables = tables();
        let bytes = kind.to_le_bytes().map(Some);
        tables.write_process_bytes(0x530e60, &bytes).unwrap();
        let mut process = process();
        let mut rng = 0x1234_5678;
        let mut pool = ProcessParticleRuntime::default();
        let mut services = Services::default();
        let mut context = BehaviorPredicateContext {
            tables: &mut tables,
            process: &mut process,
            rng: &mut rng,
        };
        let mut adapter = BehaviorParticleAdapter {
            particles: &mut pool,
            executable: exe,
            services: &mut services,
        };
        assert!(
            arena
                .test_behavior_damage_candidate(
                    predicate(Some(candidate)),
                    &mut context,
                    &mut adapter
                )
                .is_ok(),
            "kind{kind}"
        );
        assert_eq!(
            context.tables.read_process_bytes(0x530e82, 1).unwrap(),
            [Some(1)]
        );
        assert_eq!(adapter.particles.free_count(), PARTICLE_POOL_SIZE);
        assert_eq!(adapter.particles.occupant(399), None);
        let record = adapter.particles.record(ParticleId::Pool(399)).unwrap();
        assert_eq!(record.source_bytes::<6>(4).unwrap(), [None; 6]);
        assert_eq!(record.source_bytes::<6>(0x0a).unwrap(), [None; 6]);
        assert_eq!(record.read_dword(0).unwrap(), 0x526ed8);
        assert_eq!(adapter.services.effects, 0);
        let after = *context.rng;
        assert_eq!(after != 0x1234_5678, kind >= 30);
        // A cache hit shares this row and draws no second RNG value.
        assert!(arena
            .test_behavior_damage_candidate(predicate(Some(candidate)), &mut context, &mut adapter)
            .is_ok());
        assert_eq!(*context.rng, after);
    }
}

#[test]
fn notification_mutations_share_process_scratch_arena_and_tables_without_reselection() {
    let raw = raw();
    let (mut arena, candidate) = fixture();
    let mut tables = tables();
    let mut process = process();
    process.network_flag = 1;
    let mut rng = 123;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services {
        mutate_notice: true,
        owner: Some(candidate),
        ..Default::default()
    };
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    assert!(arena
        .test_behavior_damage_candidate(predicate(Some(candidate)), &mut context, &mut adapter)
        .is_ok());
    assert_eq!(adapter.services.notices[0].event, 0x17);
    assert_eq!(
        context.process.process_null_handle,
        Some(ActorHandle::from_raw(9))
    );
    assert_eq!(*context.rng, 0x7654_3210);
    assert_eq!(
        context.tables.read_process_bytes(0x530e82, 1).unwrap(),
        [Some(1)]
    );
    assert_eq!(
        arena.object(candidate).unwrap().base.position_words,
        Some([7, 8, 9])
    );
    let record = adapter.particles.record(ParticleId::Pool(399)).unwrap();
    assert_eq!(record.position().unwrap(), [10, 20, 30]);
    assert_eq!(
        record.source_bytes::<6>(4).unwrap(),
        record.source_bytes::<6>(0x0a).unwrap()
    );
    assert_eq!(record.read_dword(0x14).unwrap(), 0x1234);
    assert_eq!(
        adapter.services.observed_scratch.unwrap()[6..],
        [Some(0xab), Some(0xcd)]
    );
}

#[test]
fn full_selector_null_and_unsupported_callback_preserve_distinct_ready_prefixes() {
    let raw = raw();
    let (mut arena, _) = fixture();
    let stale = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: stale,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut tables = tables();
    tables
        .write_process_bytes(0x53dbcc, &0x10002u32.to_le_bytes().map(Some))
        .unwrap();
    let mut process = process();
    process.network_flag = 1;
    let mut rng = 123;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services::default();
    services.phase.set(None);
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    assert!(arena
        .test_behavior_damage_candidate(predicate(Some(stale)), &mut context, &mut adapter)
        .unwrap());
    assert_eq!(
        context.tables.read_process_bytes(0x530e82, 1).unwrap(),
        [Some(0)]
    );
    assert_eq!(*context.rng, 123);
    assert_eq!(adapter.services.effects, 0);
    context
        .tables
        .write_process_bytes(0x53dbc8, &0x12345678u32.to_le_bytes().map(Some))
        .unwrap();
    assert!(matches!(
        arena.test_behavior_damage_candidate(predicate(Some(stale)), &mut context, &mut adapter),
        Err(BehaviorPredicateError::Host {
            address: 0x48d891,
            error: BehaviorParticleAdapterError::UnsupportedFactory(0x12345678)
        })
    ));
    assert_eq!(
        context.tables.read_process_bytes(0x530e84, 8).unwrap(),
        [Some(0); 8]
    );
    context.process.process_null_handle = None;
    context.tables.descriptor_bytes = None;
    assert!(!arena
        .test_behavior_damage_candidate(predicate(None), &mut context, &mut adapter)
        .unwrap());
}

#[test]
fn packet_sources_are_fresh_and_captured_source_null_differs_from_notice_global() {
    let raw = raw();
    let (mut arena, _) = fixture();
    let mut tables = tables();
    let mut process = process();
    process.process_null_handle = Some(ActorHandle::from_raw(9));
    process.network_flag = 1;
    let mut rng = 0;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services::default();
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    let Some(BehaviorTemporary::Particle(particle)) = adapter
        .create(
            &mut arena,
            &mut context,
            creation(0x466850, 8, ActorHandle::from_raw(7)),
        )
        .unwrap()
    else {
        panic!("real allocation")
    };
    assert_eq!(adapter.services.notices[0].event, 0x18);
    assert_eq!(
        adapter.services.notices[0].source_handle,
        Some(ActorHandle::from_raw(7))
    );
    let first = adapter
        .packet_source(
            &arena,
            &context,
            BehaviorPacketSourceRequest::ParticlePointer {
                particle,
                slot: BehaviorPacketSlot::First,
            },
        )
        .unwrap()
        .unwrap();
    adapter
        .particles
        .record_mut(particle)
        .unwrap()
        .write_dword(0x38, 0x539ea0)
        .unwrap();
    assert_eq!(
        adapter
            .packet_source(
                &arena,
                &context,
                BehaviorPacketSourceRequest::ParticlePointer {
                    particle,
                    slot: BehaviorPacketSlot::Second
                }
            )
            .unwrap(),
        Some(first)
    );
    adapter
        .delete_particle(&mut arena, &mut context, particle, 1)
        .unwrap();
    assert_eq!(adapter.services.effects, 1); // Fresh flags9C bypass impact.
}

#[test]
fn notice_errors_and_scratch_deletion_keep_reached_cache_and_factory_custody() {
    let raw = raw();
    let (mut arena, candidate) = fixture();
    let mut tables = tables();
    let mut process = process();
    process.network_flag = 1;
    let mut rng = 0;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services {
        fail_notice: true,
        mutate_notice: true,
        ..Default::default()
    };
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    let error = arena
        .test_behavior_damage_candidate(predicate(Some(candidate)), &mut context, &mut adapter)
        .unwrap_err();
    assert!(error.to_string().contains("notice stopped"));
    assert!(matches!(
        error,
        BehaviorPredicateError::Host {
            address: 0x48d891,
            error: BehaviorParticleAdapterError::Factory(ParticleFactoryError::Network(
                "notice stopped"
            ))
        }
    ));
    assert_eq!(
        context.tables.read_process_bytes(0x530e82, 1).unwrap(),
        [Some(0)]
    );
    assert_eq!(adapter.particles.free_count(), PARTICLE_POOL_SIZE);
    assert_eq!(
        context.process.process_null_handle,
        Some(ActorHandle::from_raw(9))
    );
    adapter.services.fail_notice = false;
    context.process.network_flag = 0;
    adapter.services.phase.set(Some(2));
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            predicate(Some(candidate)),
            &mut context,
            &mut adapter
        ),
        Err(BehaviorPredicateError::Host {
            address: 0x48d8b0,
            error: BehaviorParticleAdapterError::Factory(ParticleFactoryError::Particle(
                ParticleError::MissingNode(ParticleId::Scratch)
            ))
        })
    ));
    assert_eq!(
        context.tables.read_process_bytes(0x530e82, 1).unwrap(),
        [Some(1)]
    );
    assert_eq!(
        adapter
            .particles
            .record(ParticleId::Scratch)
            .unwrap()
            .source_bytes::<6>(4)
            .unwrap(),
        [None; 6]
    );
    assert_eq!(
        adapter
            .particles
            .record(ParticleId::Scratch)
            .unwrap()
            .read_dword(0)
            .unwrap(),
        0x526ed8
    );
}

#[test]
fn lower_actor_resource_and_components_remain_required_and_disposal_is_owned() {
    let raw = raw();
    let (mut arena, candidate) = fixture();
    let temporary = arena.allocate(ObjectClassKind::Base);
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services::default();
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    let request = BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: 0x5310a0 },
        candidate: Some(candidate),
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(request, &mut context, &mut adapter),
        Err(BehaviorPredicateError::Host {
            address: 0x48d8d3,
            error: BehaviorParticleAdapterError::Lower("unowned460310")
        })
    ));
    assert_eq!(
        context.tables.read_process_bytes(0x5310c2, 1).unwrap(),
        [Some(0)]
    );
    adapter.services.actor_result = Some(Ok(Some(temporary)));
    adapter.services.queries = VecDeque::from([Some(1), Some(2)]);
    adapter.services.packets = VecDeque::from([
        BehaviorPacketCopy::Byte(Some(1)),
        BehaviorPacketCopy::Word([Some(0xff), Some(0xff)]),
    ]);
    assert!(arena
        .test_behavior_damage_candidate(request, &mut context, &mut adapter)
        .unwrap());
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == temporary));
    assert!(arena.object(temporary).is_ok());
}

#[test]
fn changed_efc_flags_demand_real_velocity_and_impact_mutations_keep_partial_prefix() {
    let raw = raw();
    let (mut arena, owner) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut pool = ProcessParticleRuntime::default();
    let mut services = Services {
        owner: Some(owner),
        ..Default::default()
    };
    let mut context = BehaviorPredicateContext {
        tables: &mut tables,
        process: &mut process,
        rng: &mut rng,
    };
    let mut adapter = BehaviorParticleAdapter {
        particles: &mut pool,
        executable: FrontendExecutable::parse(&raw).unwrap(),
        services: &mut services,
    };
    let Some(BehaviorTemporary::Particle(particle)) = adapter
        .create(
            &mut arena,
            &mut context,
            creation(0x466710, 2, ActorHandle::from_raw(0x1234)),
        )
        .unwrap()
    else {
        panic!("real allocation")
    };
    adapter
        .particles
        .record_mut(particle)
        .unwrap()
        .write_word(0x12, 0)
        .unwrap();
    assert!(matches!(
        adapter.delete_particle(&mut arena, &mut context, particle, 1),
        Err(BehaviorParticleAdapterError::Factory(
            ParticleFactoryError::Particle(ParticleError::Unwritten {
                offset: 0x0c,
                bytes: 2
            })
        ))
    ));
    assert_eq!(adapter.services.impacts, 0);
    assert!(adapter.particles.pending(3).unwrap().contains(&particle));
    let record = adapter.particles.record_mut(particle).unwrap();
    record.write_position([10, 20, 30]).unwrap();
    record.write_word(0x0a, 100).unwrap();
    record.write_word(0x0c, 0).unwrap();
    record.write_word(0x0e, 0).unwrap();
    adapter.services.mutate_impact = true;
    let error = adapter
        .delete_particle(&mut arena, &mut context, particle, 1)
        .unwrap_err();
    assert!(error.to_string().contains("impact third stopped"));
    assert!(matches!(
        error,
        BehaviorParticleAdapterError::Factory(ParticleFactoryError::Impact("impact third stopped"))
    ));
    assert_eq!(adapter.services.impacts, 3);
    let scale = f32::from_bits(u32::from_le_bytes(
        adapter
            .executable
            .mapped(0x526ef8, 4)
            .unwrap()
            .bytes
            .try_into()
            .unwrap(),
    )) as u16;
    assert_eq!(
        adapter
            .particles
            .record(particle)
            .unwrap()
            .position()
            .unwrap(),
        [13 + 2 * scale, 20, 30]
    );
    assert_eq!(adapter.particles.free_count(), PARTICLE_POOL_SIZE - 1);
    assert!(adapter.particles.pending(3).unwrap().contains(&particle));
    assert_eq!(
        context.process.process_null_handle,
        Some(ActorHandle::from_raw(42))
    );
    assert_eq!(*context.rng, 3);
    assert_eq!(context.tables.default_packet_bytes, Some([Some(0x55); 4]));
    assert_eq!(
        arena.object(owner).unwrap().base.position_words,
        Some([3, 8, 9])
    );
}
