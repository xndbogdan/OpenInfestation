use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectClassKind,
};
use std::collections::VecDeque;

const SINE: [u16; 4096] = [0; 4096];

pub(super) fn tables() -> ProcessActorBehaviorTables {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    ProcessActorBehaviorTables::from_executable(
        inf_formats::FrontendExecutable::parse(&raw).unwrap(),
    )
    .unwrap()
}

pub(super) fn process() -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x1234)),
    }
}

pub(super) fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let owner = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 20, 30]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut 0,
        )
        .unwrap();
    arena.derived_actor_mut(owner).unwrap().behavior_140 =
        super::super::build_actor_behavior(1, 0, 3);
    (arena, owner)
}

pub(super) fn vehicle_fixture() -> (ObjectArena, ObjectId, [ObjectId; 2]) {
    use crate::{object_runtime::ActorTerrainSurface, terrain_runtime::*};
    use inf_formats::{
        parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
        TERRAIN_ENTRY_FIXED_BYTES,
    };
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let source = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let bytes = source.mapped(0x530158, 704).unwrap().bytes;
    let table =
        std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    let (mut arena, candidate) = fixture();
    let vehicles = std::array::from_fn(|_| {
        let object = arena
            .allocate_selector2_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([0, 0, 0]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: Some(ActorTerrainSurface {
                                terrain: &terrain,
                                plane_height_fixed: 0,
                            }),
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: Some(0),
                            sine_quarter: &SINE,
                        },
                    },
                    world_mask_limits: [0, 0],
                },
                &mut 0,
            )
            .unwrap();
        assert!(arena.selector2_actor(object).unwrap().table_26c.is_none());
        assert!(arena
            .selector2_actor(object)
            .unwrap()
            .table_26c_known
            .is_none());
        arena.actor_mut(object).unwrap().maximum_health = Some(100);
        arena.admit_selector2_actor(object, &table, None).unwrap();
        assert_eq!(
            arena
                .selector2_actor(object)
                .unwrap()
                .table_26c_known
                .as_deref(),
            Some(&[true; 704])
        );
        object
    });
    (arena, candidate, vehicles)
}

fn packet(strength: i16, channel: u8, byte_03: Option<u8>) -> BehaviorPacketCopy {
    ActorDamagePacket {
        strength,
        channel,
        byte_03,
    }
    .into()
}

#[derive(Default)]
struct Host {
    created: Option<BehaviorTemporary<u32>>,
    queries: VecDeque<Option<u32>>,
    sources: VecDeque<Result<Option<BehaviorPacketCopy>, &'static str>>,
    events: Vec<&'static str>,
    fail_create: bool,
    fail_delete: bool,
    selection_mutation: Option<ObjectId>,
    readiness_on_delete: Option<u8>,
    row: Option<u32>,
    first_visible_on_second: Option<[Option<u8>; 4]>,
}

impl BehaviorPredicateHost for Host {
    type Error = &'static str;
    type Particle = u32;
    type Component = u32;
    fn create(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        request: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<u32>>, Self::Error> {
        self.events.push("create");
        match request {
            BehaviorCreationRequest::ParticleFactory { scratch_bytes, .. }
            | BehaviorCreationRequest::ActorResource { scratch_bytes, .. } => {
                assert_eq!(scratch_bytes, [None; 8])
            }
        }
        if let Some(owner) = self.selection_mutation {
            let behavior = arena
                .derived_actor_mut(owner)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap();
            behavior.table_14 = ActorBehaviorTable::Mode03;
            behavior.byte_10 = 0x7f;
        }
        if self.fail_create {
            Err("create unavailable")
        } else {
            Ok(self.created)
        }
    }
    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, Self::Error> {
        self.events.push(match request {
            BehaviorComponentQuery::Actor18(_) => "18",
            BehaviorComponentQuery::Component6c(_) => "6c",
            BehaviorComponentQuery::Component68(_) => "68",
        });
        Ok(self.queries.pop_front().expect("required query plan"))
    }
    fn packet_source(
        &mut self,
        _: &ObjectArena,
        context: &BehaviorPredicateContext<'_>,
        request: BehaviorPacketSourceRequest<u32, u32>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        let tables = &*context.tables;
        self.events.push(match request {
            BehaviorPacketSourceRequest::ParticlePointer {
                slot: BehaviorPacketSlot::First,
                ..
            } => "34",
            BehaviorPacketSourceRequest::ParticlePointer {
                slot: BehaviorPacketSlot::Second,
                ..
            } => {
                if let Some(expected) = self.first_visible_on_second {
                    assert_eq!(
                        tables
                            .read_process_bytes(self.row.unwrap() + 0x24, 4)
                            .unwrap(),
                        expected
                    );
                }
                "38"
            }
            BehaviorPacketSourceRequest::Channel148(_) => "148",
            BehaviorPacketSourceRequest::Strength146(_) => "146",
            BehaviorPacketSourceRequest::Complete13e(_) => "13e",
        });
        self.sources.pop_front().expect("required source plan")
    }
    fn delete_particle(
        &mut self,
        _: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        _: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        let tables = &mut *context.tables;
        assert_eq!(flags, 1);
        self.events.push("delete");
        if let Some(ready) = self.readiness_on_delete {
            tables
                .write_process_bytes(self.row.unwrap() + 0x22, &[Some(ready)])
                .unwrap();
        }
        if self.fail_delete {
            Err("delete unavailable")
        } else {
            Ok(())
        }
    }
}

#[test]
fn null_candidate_bypasses_unavailable_regions_and_failed_creation_does_not_admit_candidate() {
    let (mut arena, owner) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    tables.descriptor_bytes = None;
    let mut host = Host::default();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: u32::MAX },
                candidate: None
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(false)
    );
    tables = super::tests::tables();
    let candidate = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: candidate,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let selection = arena.actor_behavior_row_selection(owner).unwrap();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection,
                candidate: Some(candidate)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(true)
    );
    assert_eq!(host.events, ["create"]);
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), [Some(0)]);
}

#[test]
fn raw_readiness_skips_all_initialization_and_packet_reads_are_lazy() {
    let (mut arena, candidate) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    tables.write_process_bytes(0x530e82, &[Some(0xfe)]).unwrap();
    tables
        .write_process_bytes(0x530e84, &[None, Some(0), None, None])
        .unwrap();
    tables.default_packet_bytes = None;
    tables.registry_bytes = None;
    process.process_null_handle = None;
    let request = BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
        candidate: Some(candidate),
    };
    let mut host = Host::default();
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Storage(
            BehaviorCacheStorageError::Unknown { address: 0x530e86 }
        ))
    ));
    tables.write_process_bytes(0x530e86, &[Some(1)]).unwrap();
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Storage(
            BehaviorCacheStorageError::Unknown { address: 0x530e84 }
        ))
    ));
    tables
        .write_process_bytes(0x530e84, &[Some(0xff), Some(0xff)])
        .unwrap();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(true)
    );
    tables.write_process_bytes(0x530e86, &[Some(10)]).unwrap();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::UnsupportedChannel {
            actor: candidate,
            channel: 10
        })
    );
    assert!(host.events.is_empty());
    assert_eq!(
        tables.read_process_bytes(0x530e82, 1).unwrap(),
        [Some(0xfe)]
    );
}

#[test]
fn contiguous_alias_and_captured_selection_survive_callbacks_and_second_source_is_fresh() {
    let (mut arena, owner) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let selection = BehaviorRowSelection::from_table(ActorBehaviorTable::Mode02, 0xff, owner);
    assert_eq!(
        selection,
        BehaviorRowSelection::from_table(ActorBehaviorTable::Mode13, 1, owner)
    );
    let packet0 = packet(-7, 1, None);
    let BehaviorPacketCopy::Full(expected) = packet0 else {
        unreachable!()
    };
    let mut host = Host {
        created: Some(BehaviorTemporary::Particle(17)),
        sources: VecDeque::from([Ok(Some(packet0)), Ok(None)]),
        selection_mutation: Some(owner),
        row: Some(0x530ee4),
        first_visible_on_second: Some(expected),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection,
                candidate: Some(owner)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(true)
    );
    assert_eq!(host.events, ["create", "34", "38", "delete"]);
    assert_eq!(tables.read_process_bytes(0x530f06, 1).unwrap(), [Some(1)]);
    assert_eq!(tables.read_process_bytes(0x530f08, 4).unwrap(), expected);
    let mut next = Host::default();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::from_table(ActorBehaviorTable::Mode13, 1, owner),
                candidate: Some(owner)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut next
        ),
        Ok(true)
    );
    assert!(next.events.is_empty());
}

#[test]
fn creation_source_and_delete_failures_preserve_exact_prior_cache_stores() {
    let (mut arena, candidate) = fixture();
    let request = BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
        candidate: Some(candidate),
    };
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    tables.default_packet_bytes = Some([Some(1), Some(2), Some(3), Some(4)]);
    let mut host = Host {
        fail_create: true,
        ..Default::default()
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Host {
            address: 0x48d891,
            ..
        })
    ));
    assert_eq!(
        tables.read_process_bytes(0x530e84, 8).unwrap(),
        [
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(1),
            Some(2),
            Some(3),
            Some(4)
        ]
    );
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), [Some(0)]);
    host = Host {
        created: Some(BehaviorTemporary::Particle(17)),
        sources: VecDeque::from([Ok(Some(packet(2300, 1, Some(0x99)))), Err("38 unavailable")]),
        ..Default::default()
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Host {
            address: 0x466120,
            ..
        })
    ));
    assert_eq!(host.events, ["create", "34", "38"]);
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), [Some(1)]);
    assert_eq!(
        tables.read_process_bytes(0x530e84, 4).unwrap(),
        [Some(0xfc), Some(8), Some(1), Some(0x99)]
    );
    tables.write_process_bytes(0x530e82, &[Some(0)]).unwrap();
    host = Host {
        created: Some(BehaviorTemporary::Particle(17)),
        sources: VecDeque::from([Ok(None), Ok(None)]),
        fail_delete: true,
        ..Default::default()
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Host {
            address: 0x48d8b0,
            ..
        })
    ));
    assert_eq!(host.events, ["create", "34", "38", "delete"]);
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), [Some(1)]);
}

#[test]
fn lazy_process_inputs_follow_defaults_and_null_precedes_factory_context() {
    let (mut arena, candidate) = fixture();
    let request = BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
        candidate: Some(candidate),
    };
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    process.process_null_handle = None;
    tables.write_process_bytes(0x53dbcc, &[None; 4]).unwrap();
    let mut host = Host::default();
    assert_eq!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::MissingProcessNull { address: 0x48d87c })
    );
    assert_eq!(
        tables.read_process_bytes(0x530e84, 8).unwrap(),
        [Some(0); 8]
    );
    process.process_null_handle = Some(ActorHandle::from_raw(0x1234));
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Storage(
            BehaviorCacheStorageError::Unknown { address: 0x53dbcc }
        ))
    ));
    assert!(host.events.is_empty());
}

#[test]
fn channel_store_precedes_unavailable_strength_and_unknown_copies_wait_for_consumption() {
    let (mut arena, candidate) = fixture();
    let temporary = arena.allocate(ObjectClassKind::Base);
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    // Mode07 kind15 uses the registry's actor resource branch.
    let row = 0x5310a0;
    let request = BehaviorPredicateRequest {
        selection: BehaviorRowSelection::SharedAddress { address: row },
        candidate: Some(candidate),
    };
    tables.default_packet_bytes = Some([Some(0), Some(0), Some(0), Some(0x77)]);
    let mut host = Host {
        created: Some(BehaviorTemporary::Actor(temporary)),
        queries: VecDeque::from([Some(31), Some(41)]),
        sources: VecDeque::from([
            Ok(Some(BehaviorPacketCopy::Byte(Some(1)))),
            Err("146 unavailable"),
        ]),
        ..Default::default()
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Host {
            address: 0x4863dd,
            ..
        })
    ));
    assert_eq!(
        tables.read_process_bytes(row + 0x24, 4).unwrap(),
        [Some(0), Some(0), Some(1), Some(0x77)]
    );
    assert_eq!(host.events, ["create", "18", "6c", "148", "146"]);
    assert!(!arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|n| n.object == temporary));
    tables.write_process_bytes(row + 0x22, &[Some(0)]).unwrap();
    host = Host {
        created: Some(BehaviorTemporary::Actor(temporary)),
        queries: VecDeque::from([Some(31), Some(41)]),
        sources: VecDeque::from([
            Ok(Some(BehaviorPacketCopy::Byte(Some(1)))),
            Ok(Some(BehaviorPacketCopy::Word([None, None]))),
        ]),
        ..Default::default()
    };
    assert!(matches!(
        arena.test_behavior_damage_candidate(
            request,
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Storage(
            BehaviorCacheStorageError::Unknown { address: 0x5310c4 }
        ))
    ));
    assert_eq!(
        tables.read_process_bytes(row + 0x24, 4).unwrap(),
        [None, None, Some(1), Some(0x77)]
    );
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|n| n.object == temporary));
}

#[test]
fn deletion_mutation_readiness_is_reread_before_candidate_and_dispose_is_owned() {
    let (mut arena, candidate) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let invalid = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: invalid,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut host = Host {
        created: Some(BehaviorTemporary::Particle(17)),
        sources: VecDeque::from([Ok(None), Ok(None)]),
        row: Some(0x530e60),
        readiness_on_delete: Some(0),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
                candidate: Some(invalid)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(true)
    );
    let temporary = arena.allocate(ObjectClassKind::Base);
    host = Host {
        created: Some(BehaviorTemporary::Actor(temporary)),
        queries: VecDeque::from([Some(31), None, Some(41)]),
        sources: VecDeque::from([Ok(Some(packet(-1, 1, None)))]),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: 0x5310a0 },
                candidate: Some(candidate)
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Ok(true)
    );
    assert_eq!(host.events, ["create", "18", "6c", "68", "13e"]);
    assert!(arena.object(temporary).is_ok());
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|n| n.object == temporary));
}

#[test]
fn empty_end_writes_and_nonnull_protocol_errors_preserve_reached_custody() {
    let (mut arena, candidate) = fixture();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let before = tables.clone();
    for address in [0x530e60 + 860, 0x53dba8 + 528, 0x546a94 + 4] {
        tables.write_process_bytes(address, &[]).unwrap();
    }
    assert_eq!(tables, before);
    let temporary = arena.allocate(ObjectClassKind::Base);
    let mut host = Host {
        created: Some(BehaviorTemporary::Actor(temporary)),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: 0x530e60 },
                candidate: Some(candidate),
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::TemporaryClass { address: 0x48d893 })
    );
    assert_eq!(tables.read_process_bytes(0x530e82, 1).unwrap(), [Some(1)]);
    host = Host {
        created: Some(BehaviorTemporary::Particle(7)),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: 0x5310a0 },
                candidate: Some(candidate),
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::TemporaryClass { address: 0x48d8d8 })
    );
    assert_eq!(tables.read_process_bytes(0x5310c2, 1).unwrap(), [Some(1)]);
}

#[test]
fn vehicle_known_mask_tracks_copies_and_upgrade_writes_without_duplicate_values() {
    let (mut arena, _, vehicles) = vehicle_fixture();
    let mut tables = tables();
    let row = BehaviorRowSelection::VehicleCopy {
        owner: vehicles[0],
        byte_offset: 0,
    };
    storage::write_row(
        &mut arena,
        &mut tables,
        row,
        0x24,
        &[None, Some(3), None, Some(4)],
    )
    .unwrap();
    assert_eq!(
        &arena
            .selector2_actor(vehicles[0])
            .unwrap()
            .table_26c_known
            .as_deref()
            .unwrap()[0x24..0x28],
        &[false, true, false, true]
    );
    assert!(arena
        .selector2_actor(vehicles[1])
        .unwrap()
        .table_26c_known
        .as_deref()
        .unwrap()
        .iter()
        .all(|v| *v));
    arena
        .selector2_actor_mut(vehicles[0])
        .unwrap()
        .table_26c_known
        .as_deref_mut()
        .unwrap()
        .fill(false);
    let mut bits = [0; 6];
    for id in [0x95, 0x96, 0xa0] {
        bits[id >> 5] |= 1 << (id & 31);
    }
    arena
        .apply_vehicle_upgrades(vehicles[0], None, &bits, &mut [0; 16], &[0; 16], false)
        .unwrap();
    let state = arena.selector2_actor(vehicles[0]).unwrap();
    let known = state.table_26c_known.as_deref().unwrap();
    for range in [0x06..0x08, 0x08..0x0c, 0xb0..0xb2, 0x134..0x136] {
        assert!(known[range].iter().all(|v| *v));
    }
    assert!(!known[0x0c]);
    assert_eq!(state.table_26c.as_deref().unwrap()[2], 120_000);
    assert_eq!((state.table_26c.as_deref().unwrap()[1] >> 16) as u16, 2);
}

#[test]
fn unavailable_temporary_identity_stops_owned_disposal_without_inventing_a_host_failure() {
    let (mut arena, candidate) = fixture();
    let stale = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: stale,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut tables = tables();
    let mut process = process();
    let mut rng = 0;
    let mut host = Host {
        created: Some(BehaviorTemporary::Actor(stale)),
        queries: VecDeque::from([None]),
        ..Default::default()
    };
    assert_eq!(
        arena.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection: BehaviorRowSelection::SharedAddress { address: 0x5310a0 },
                candidate: Some(candidate),
            },
            &mut BehaviorPredicateContext {
                tables: &mut tables,
                process: &mut process,
                rng: &mut rng
            },
            &mut host
        ),
        Err(BehaviorPredicateError::Object(ObjectError::UnknownObject(
            stale
        )))
    );
    assert_eq!(tables.read_process_bytes(0x5310c2, 1).unwrap(), [Some(1)]);
    assert_eq!(host.events, ["create", "18"]);
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .is_empty());
}
