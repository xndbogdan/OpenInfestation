use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextPairClassification},
    input::InputRuntime,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, DerivedActorConstructionRequest, ObjectClassKind,
        PlayerActorLookupError, PlayerInputDescriptor, PlayerInputDescriptorKind,
        PlayerInputDescriptorRequest, PlayerInputDescriptorSource, Selector35State,
    },
    player_input_source::{PlayerAnalogDescriptors, PlayerInputSource},
    process_particles::factories::ParticleFactoryProcessState,
    profile_name_format::ProfileNameClassification,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_mission::WorldMissionInputs,
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    grant_closure, parse_grant_table, parse_grant_texts, parse_terrain_bank, ByteSpan,
    FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion, MissionSaveState,
    PLAYER_INITIAL_GRANTS, TERRAIN_ENTRY_FIXED_BYTES,
};
use std::path::Path;

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 32,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> ActorConstructionRequest<'static> {
    // This genuine shared-constructor arm never consumes XYZ/terrain/sine.
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn family_allocation() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: allocation(),
        world_mask_limits: [0, 16],
    }
}

fn selector35(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_selector35_actor(family_allocation(), &mut 9)
        .unwrap()
}

fn selector35_mut(arena: &mut ObjectArena, actor: ObjectId) -> &mut Selector35State {
    let ActorExtension::Derived(derived) = &mut arena.actor_mut(actor).unwrap().extension else {
        panic!("actual derived constructor");
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual family constructor");
    };
    let Family428650Class::Selector35(state) = &mut family.class else {
        panic!("actual selector35 constructor");
    };
    state
}

fn mission() -> WorldMission {
    let mut save = MissionSaveState::new_game();
    save.owner_240 = std::array::from_fn(|i| 0x8100_0000 | i as u32);
    WorldMission::new(
        save,
        WorldMissionInputs {
            current_world_a0: u32::MAX,
            dword_1c: 0x1234_5678,
            byte_13: None,
            record_0c: None,
        },
    )
}

fn context(process: Option<&ParticleFactoryProcessState>) -> LiveGrantContext<'_> {
    LiveGrantContext {
        process,
        executable: None,
        resident: None,
        text_data: None,
    }
}

fn run(
    arena: &mut ObjectArena,
    mission: &mut WorldMission,
    grants: &LiveGrantContext<'_>,
) -> Result<(), WorldCleanupError> {
    arena.cleanup_world(WorldCleanupRequest {
        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        mission,
        grants,
        listeners: &mut crate::object_runtime::CreationListeners::new(),
        furniture: None,
    })
}

#[test]
fn empty_world_clears_inhibit_without_process_and_retains_published_players() {
    let mut arena = arena();
    let first = arena.add_world_player([0x8000_0000; 6]);
    let second = arena.add_world_player([0x1234_5678; 6]);
    let before = format!("{arena:?}");
    let mut mission = mission();
    let expected = mission.clone();
    mission.inhibit_byte_08 = 7;
    run(&mut arena, &mut mission, &context(None)).unwrap();
    assert_eq!(mission, expected);
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.world_players()[1].id, second);
}

#[test]
fn genuine_nonactor_callbacks_need_no_process_or_initialized_draw_data() {
    let mut arena = arena();
    let ids = [
        ObjectClassKind::Base,
        ObjectClassKind::VisualBase,
        ObjectClassKind::RotatingVisualBase,
        ObjectClassKind::TerrainLight,
        ObjectClassKind::Selector86,
        ObjectClassKind::MapAnnotation,
        ObjectClassKind::RouteNode,
    ]
    .map(|class| arena.allocate(class));
    let registry = format!("{:?}", arena.actor_handles());
    let mut mission = mission();
    let expected = mission.clone();
    run(&mut arena, &mut mission, &context(None)).unwrap();
    assert!(arena.is_empty());
    for id in ids {
        assert!(arena.object(id).is_err());
    }
    assert_eq!(mission, expected);
    assert_eq!(format!("{:?}", arena.actor_handles()), registry);
}

#[test]
fn physical_header_order_and_multi_membership_use_fresh_heads() {
    let mut arena = arena();
    let a = arena.allocate(ObjectClassKind::Base);
    let b = arena.allocate(ObjectClassKind::Base);
    let c = arena.allocate(ObjectClassKind::Base);
    let d = arena.allocate(ObjectClassKind::Base);
    let e = arena.allocate(ObjectClassKind::Base);
    for id in [b, c, d, e] {
        arena.set_flag1(id, false).unwrap();
    }
    for id in [d, e] {
        arena.set_flag0(id, false).unwrap();
    }
    arena.dispose(e).unwrap();
    arena.swap_flag0_lists();
    arena.swap_flag1_lists();
    let f = arena.allocate(ObjectClassKind::Base);
    let header_custody = arena.list_owners;
    let objects = |role| {
        arena
            .list(role)
            .iter()
            .map(|node| node.object)
            .collect::<Vec<_>>()
    };
    assert_eq!(objects(ObjectList::Flag1Primary), [f]);
    assert_eq!(objects(ObjectList::Flag1Secondary), [a]);
    assert_eq!(objects(ObjectList::Flag0Primary), [f]);
    assert_eq!(objects(ObjectList::Flag0Secondary), [a, b, c]);
    assert_eq!(objects(ObjectList::Disposal), [e]);
    assert!(arena.list(ObjectList::Parking).contains(&ObjectNode {
        object: d,
        kind: ObjectNodeKind::Flag1,
    }));
    let mut mission = mission();
    run(&mut arena, &mut mission, &context(None)).unwrap();
    // Disabled flag nodes move to Parking, not the alternate heap header.
    // Actual swapped flag1 headers retire f/a; flag0 then reaches b/c,
    // disposal reaches e, and parked d remains last. Each destruction removes
    // all other memberships before those phases select a fresh current head.
    assert_eq!(arena.free, [f.slot, a.slot, b.slot, c.slot, e.slot, d.slot]);
    assert_eq!(arena.list_owners, header_custody);
    assert_eq!(arena.len(), 0);
    assert!(arena.lists.iter().all(Vec::is_empty));
}

#[test]
fn actor_fifo_precedes_earlier_common_nodes_and_missing_process_retains_all() {
    let mut arena = arena();
    let base = arena.allocate(ObjectClassKind::Base);
    let actor = arena.allocate_actor(allocation()).unwrap();
    let before = format!("{arena:?}");
    let mut mission = mission();
    let mut expected = mission.clone();
    expected.inhibit_byte_08 = 1;
    assert!(matches!(run(&mut arena, &mut mission, &context(None)),
        Err(WorldCleanupError::MissingProcess(id)) if id == actor));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(mission, expected);
    assert!(arena.object(base).is_ok());
}

#[test]
fn common_signed_byte_and_inhibit_bypass_out_of_range_world_index() {
    let mut arena = arena();
    let mut handles = Vec::new();
    for byte in [0xff, 0x80, 0, 0x7f] {
        let actor = arena.allocate_actor(allocation()).unwrap();
        arena.actor_mut(actor).unwrap().byte_78 = byte;
        handles.push(arena.actor(actor).unwrap().handle);
    }
    let mut registry = arena.actor_handles().unwrap().clone();
    for &handle in &handles {
        registry.release(handle);
    }
    let process = ParticleFactoryProcessState::native_startup();
    let mut mission = mission();
    let expected = mission.clone();
    run(&mut arena, &mut mission, &context(Some(&process))).unwrap();
    assert_eq!(mission, expected);
    assert!(arena.actor_list().is_empty());
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{registry:?}")
    );
    assert_eq!(arena.len(), 0);
}

/// 0047EC50 tells the others of a local actor's end: message 6, 5, 4 or 3
/// by its state, with its handle, queued for the next network tick.
#[test]
fn full_network_report_queues_the_event_and_handle() {
    for (state, event) in [(0, 6), (1, 5), (3, 4), (4, 3)] {
        let mut arena = arena();
        let actor = arena.allocate_actor(allocation()).unwrap();
        arena.actor_mut(actor).unwrap().word_c4 = state;
        let handle = arena.actor(actor).unwrap().handle;
        let mut process = ParticleFactoryProcessState::native_startup();
        process.network_flag = 0x100; // LOWBYTE0 does not make this offline.
        let mut mission = mission();
        run(&mut arena, &mut mission, &context(Some(&process))).unwrap();
        assert!(arena.actor_list().is_empty());
        assert_eq!(
            arena.take_network_sends(),
            [(
                event,
                crate::network::protocol::GameSend::Dword(handle.raw())
            )]
        );
    }
}

#[test]
fn remote_or_unsigned_lifecycle_gate_skips_unowned_network_report() {
    for (remote, state) in [(false, 2), (false, 5), (false, u32::MAX), (true, 3)] {
        let mut arena = arena();
        let actor = arena.allocate_actor(allocation()).unwrap();
        arena.actor_mut(actor).unwrap().word_c4 = state;
        if remote {
            arena.owned_mut(actor).unwrap().state.base.flags |= 0x8000_0000;
        }
        let mut process = ParticleFactoryProcessState::native_startup();
        process.network_flag = 0x100;
        let mut mission = mission();
        let expected = mission.clone();
        run(&mut arena, &mut mission, &context(Some(&process))).unwrap();
        assert!(arena.object(actor).is_err());
        assert_eq!(mission, expected);
    }
}

#[test]
fn selector35_source_bytes_are_lazy_before_common_process_read() {
    for (first, second, offset) in [
        (Some(0), None, None),
        (None, None, Some(0x242)),
        (Some(1), None, Some(0x243)),
        (Some(2), Some(1), None),
        (Some(1), Some(1), None), // Constructor WORD7A0 skips player/grant data.
    ] {
        let mut arena = arena();
        let actor = selector35(&mut arena);
        let state = selector35_mut(&mut arena, actor);
        // Declared partial current scalar backing, not a native writer claim.
        state.bytes_240[2] = first;
        state.bytes_240[3] = second;
        let before = format!("{arena:?}");
        let mut mission = mission();
        let error = run(&mut arena, &mut mission, &context(None)).unwrap_err();
        match offset {
            Some(offset) => assert!(matches!(error,
                WorldCleanupError::UnknownSelector35Byte { actor: id, offset: actual }
                    if id == actor && actual == offset)),
            None => assert!(matches!(error, WorldCleanupError::MissingProcess(id) if id == actor)),
        }
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(mission.inhibit_byte_08, 1);
    }
}

#[test]
fn selector35_actual_player_setter_prefix_precedes_common_and_destructor() {
    for network in [None, Some(0)] {
        let mut arena = arena();
        let first = arena.add_world_player([2, 0, 0, 0, 0, 0]);
        let second = arena.add_world_player([0; 6]);
        let mut expected_players = arena.world_players().to_vec();
        expected_players[1].grants_9c[0] = 2;
        let actor = selector35(&mut arena);
        selector35_mut(&mut arena, actor).bytes_240[2..4].fill(Some(1));
        arena.actor_mut(actor).unwrap().word_7a = 1;
        let object = arena.object(actor).unwrap().clone();
        let registry = format!("{:?}", arena.actor_handles());
        let process = network.map(|network_flag| ParticleFactoryProcessState {
            network_flag,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: None,
        });
        let mut mission = mission();
        let error = run(&mut arena, &mut mission, &context(process.as_ref())).unwrap_err();
        match network {
            None => assert!(matches!(error,
                WorldCleanupError::Grant(LiveGrantError::MissingProcess { owner: GrantOwner::Player9c(id) })
                    if id == second)),
            Some(_) => assert!(matches!(
                error,
                WorldCleanupError::Grant(LiveGrantError::MissingExecutable { id: 1, .. })
            )),
        }
        assert_eq!(arena.first_player(), Some(first));
        assert_eq!(arena.world_players(), expected_players);
        assert_eq!(arena.object(actor).unwrap(), &object);
        assert_eq!(format!("{:?}", arena.actor_handles()), registry);
        assert_eq!(mission.inhibit_byte_08, 1);
    }
}

/// The burst and radius a Dormant ship's exit reaches, in order.
#[derive(Default)]
struct ExitEffects(Vec<crate::object_runtime::Selector35LifecycleEffect>);

impl crate::object_runtime::TargetNullHandleSource for ExitEffects {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        None
    }
}

impl crate::object_runtime::Selector35LifecycleHost for ExitEffects {
    type Error = std::convert::Infallible;
    fn particle_load_factor(&mut self) -> Result<u32, Self::Error> {
        unreachable!("0043E120 reads no load factor")
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: crate::object_runtime::Selector35LifecycleEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.0.push(effect);
        Ok(())
    }
}

/// A ship left Dormant (descriptor 0, whose exit 0043E120 bursts and
/// radiates even on false) pauses the walk after its +30, with the actors
/// before it retired. Its exit runs with the caller's services; the walk then
/// resumes at its destructor, without a second +30, and finishes.
#[test]
fn a_dormant_ship_pauses_the_walk_for_its_exit_and_the_walk_resumes() {
    let mut arena = arena();
    let first = arena.allocate_actor(allocation()).unwrap();
    let first_handle = arena.actor(first).unwrap().handle;
    let blocked = selector35(&mut arena);
    arena.place_for_tests(blocked, [100, 200, 300]);
    let ActorExtension::Derived(derived) = &mut arena.actor_mut(blocked).unwrap().extension else {
        panic!("actual derived");
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual family");
    };
    family.word_184 = 0;
    let expected_actor = arena.object(blocked).unwrap().clone();
    let mut expected_registry = arena.actor_handles().unwrap().clone();
    expected_registry.release(first_handle);
    let process = ParticleFactoryProcessState::native_startup();
    let mut mission = mission();
    assert!(
        matches!(run(&mut arena, &mut mission, &context(Some(&process))),
        Err(WorldCleanupError::DestructionExitPending(id)) if id == blocked)
    );
    assert!(arena.object(first).is_err());
    assert_eq!(arena.object(blocked).unwrap(), &expected_actor);
    assert_eq!(arena.actor_list(), [blocked]);
    assert_eq!(arena.free, [first.slot]);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{expected_registry:?}")
    );
    assert_eq!(mission.inhibit_byte_08, 1);

    let mut effects = ExitEffects::default();
    let handle = arena.actor(blocked).unwrap().handle;
    arena
        .exit_selector35_dormant_for_destruction(blocked, &mut effects, &mut 0)
        .unwrap();
    let [crate::object_runtime::Selector35LifecycleEffect::Burst(burst), crate::object_runtime::Selector35LifecycleEffect::Radius {
        block_address: 0x0053_1250,
        source,
    }] = effects.0[..]
    else {
        panic!("0043E120's burst then radius: {:?}", effects.0);
    };
    assert_eq!(source, handle);
    assert_eq!(
        (burst.position, burst.central_type, burst.debris_count),
        ([100, 200, 300], 2, 16)
    );
    // A false exit leaves the status; the destructor stores -1 after it.
    assert_eq!(
        arena.family428650_actor(blocked).unwrap().word_228,
        expected_actor_word_228(&expected_actor)
    );
    assert_eq!(arena.family428650_actor(blocked).unwrap().word_184, -1);

    run(&mut arena, &mut mission, &context(Some(&process))).unwrap();
    assert!(arena.object(blocked).is_err());
    assert!(arena.actor_list().is_empty());
    assert_eq!(mission.inhibit_byte_08, 0);
}

fn expected_actor_word_228(object: &crate::object_runtime::ObjectState) -> u32 {
    let ObjectClass::Actor(actor) = &object.class else {
        panic!("actual actor");
    };
    let ActorExtension::Derived(derived) = &actor.extension else {
        panic!("actual derived");
    };
    let DerivedActorClass::Family428650(family) = &derived.class else {
        panic!("actual family");
    };
    family.word_228
}

/// Without the world's services an exit stays a typed boundary, and the
/// walk refuses to resume at another head than the one it paused at.
#[test]
fn a_dormant_exit_needs_its_ship_paused_at_the_head() {
    let mut arena = arena();
    let ship = selector35(&mut arena);
    let other = arena.allocate_actor(allocation()).unwrap();
    assert!(matches!(
        arena.exit_selector35_dormant_for_destruction(ship, &mut ExitEffects::default(), &mut 0),
        Err(crate::object_runtime::Selector35LifecycleError::Object(
            ObjectError::FamilyBehaviorDestructionBoundary(id)
        )) if id == ship
    ));
    arena.cleanup_exit_pending = Some(other);
    let process = ParticleFactoryProcessState::native_startup();
    assert!(matches!(
        run(&mut arena, &mut mission(), &context(Some(&process))),
        Err(WorldCleanupError::ResumedElsewhere { paused, head })
            if paused == other && head == ship
    ));
}

fn vehicle(arena: &mut ObjectArena) -> ObjectId {
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
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            terrain: ActorTerrainInputs {
                terrain: Some(ActorTerrainSurface {
                    terrain: &terrain,
                    plane_height_fixed: 0,
                }),
                world_flags: Some(0),
                ..allocation().terrain
            },
            ..allocation()
        },
        ..family_allocation()
    };
    arena.allocate_selector2_actor(request, &mut 0).unwrap()
}

#[test]
fn genuine_vehicle_event10_lookup_error_precedes_common_and_destruction() {
    let mut arena = arena();
    let vehicle = vehicle(&mut arena);
    let before = format!("{arena:?}");
    let mut mission = mission();
    // No process would also fail common47EC50, but actual event10 reaches its
    // selector2 receiver's player lookup first, with the original full handle.
    assert!(matches!(
        run(&mut arena, &mut mission, &context(None)),
        Err(WorldCleanupError::Vehicle(
            ActorVehicleCleanupNotificationError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            )
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(arena.actor_list(), [vehicle]);
    assert_eq!(mission.inhibit_byte_08, 1);
}

#[test]
fn actual_prepared_player_input_message_and_registry_survive_successful_drain() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let classification = ProfileNameClassification::native_startup(executable).unwrap();
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: classification.high_bytes.as_ref().map(|high| {
            GrantTextPairClassification {
                colon_high: high[b':' as usize],
                space_high: high[b' ' as usize],
            }
        }),
    };
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let analog = PlayerAnalogDescriptors::native_startup(assets.executable_bytes()).unwrap();
    let mut descriptors = PlayerInputSource {
        analog: &analog,
        executable,
    };
    let process = ParticleFactoryProcessState::native_startup();
    let grants = LiveGrantContext {
        process: Some(&process),
        executable: Some(executable),
        resident: Some(&resident),
        text_data: Some(&text),
    };
    let mut arena = arena();
    let mut input = InputRuntime::new(0x1234_5678);
    let mut carrier = arena.reserve_world_player_construction(0x7700);
    let player = arena
        .construct_world_player(&mut carrier, &mut input, &mut descriptors, &grants)
        .unwrap();
    let players = arena.world_players().to_vec();
    let message = arena.player_message(player).unwrap().identity();
    let PlayerInputDescriptor::Analog(value) = descriptors
        .descriptor(PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Analog,
            identity: 0x0054_ecf8,
            player_offset: 0x5c,
        })
        .unwrap()
    else {
        panic!("actual retained analog source");
    };
    let reference = input
        .acquire_analog_consumer(value, carrier.callback_context())
        .unwrap();
    let reference_before = input.analog_consumer_state(reference).unwrap();
    assert_eq!(reference_before.references, 2);
    let input_before = (input.clock_micros(), input.queued_len(), input.stopped());
    let carrier_before = (
        carrier.player(),
        carrier.phase(),
        carrier.callback_context(),
        carrier.destruction_phase(),
    );
    let resident_before = format!("{resident:?}");
    let actor = arena.allocate_actor(allocation()).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let mut registry = arena.actor_handles().unwrap().clone();
    registry.release(handle);
    let mut mission = mission();
    let expected_mission = mission.clone();
    run(&mut arena, &mut mission, &grants).unwrap();
    assert_eq!(arena.world_players(), players);
    assert_eq!(arena.first_player(), Some(player));
    assert_eq!(arena.player_message(player).unwrap().identity(), message);
    assert_eq!(
        input.analog_consumer_state(reference).unwrap(),
        reference_before
    );
    assert_eq!(
        (input.clock_micros(), input.queued_len(), input.stopped()),
        input_before
    );
    assert_eq!(
        (
            carrier.player(),
            carrier.phase(),
            carrier.callback_context(),
            carrier.destruction_phase()
        ),
        carrier_before
    );
    assert_eq!(format!("{resident:?}"), resident_before);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{registry:?}")
    );
    assert_eq!(mission, expected_mission);
    assert!(arena.is_empty());
    // Explicit component release uses the same acquiring runtime; it is not
    // performed by manager cleanup and does not claim full player destruction.
    carrier.release_input_references(&mut input).unwrap();
    assert_eq!(
        input.analog_consumer_state(reference).unwrap().references,
        1
    );
    input.release_analog_consumer(reference).unwrap();
    assert!(input.analog_consumer_state(reference).is_err());
}

#[test]
fn selector35_offline_fanout_preserves_both_constructed_players_and_input_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let classification = ProfileNameClassification::native_startup(executable).unwrap();
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: classification.high_bytes.as_ref().map(|high| {
            GrantTextPairClassification {
                colon_high: high[b':' as usize],
                space_high: high[b' ' as usize],
            }
        }),
    };
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let analog = PlayerAnalogDescriptors::native_startup(assets.executable_bytes()).unwrap();
    let mut descriptors = PlayerInputSource {
        analog: &analog,
        executable,
    };
    let process = ParticleFactoryProcessState::native_startup();
    let grants = LiveGrantContext {
        process: Some(&process),
        executable: Some(executable),
        resident: Some(&resident),
        text_data: Some(&text),
    };
    let mut arena = arena();
    let mut input = InputRuntime::new(0x7654_3210);
    let mut carriers = [
        arena.reserve_world_player_construction(0x7700),
        arena.reserve_world_player_construction(0x7800),
    ];
    for carrier in &mut carriers {
        arena
            .construct_world_player(carrier, &mut input, &mut descriptors, &grants)
            .unwrap();
    }
    let before_players = arena.world_players().to_vec();
    assert_eq!(before_players.len(), 2);
    let messages = before_players
        .iter()
        .map(|player| arena.player_message(player.id).unwrap().clone())
        .collect::<Vec<_>>();
    let PlayerInputDescriptor::Analog(value) = descriptors
        .descriptor(PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Analog,
            identity: 0x0054_ecf8,
            player_offset: 0x5c,
        })
        .unwrap()
    else {
        panic!("actual retained analog source");
    };
    let references = carriers
        .iter()
        .map(|carrier| {
            input
                .acquire_analog_consumer(value, carrier.callback_context())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_ne!(references[0], references[1]);
    let before_references = references
        .iter()
        .map(|&id| input.analog_consumer_state(id).unwrap())
        .collect::<Vec<_>>();
    assert!(before_references.iter().all(|state| state.references == 2));
    let input_before = (input.clock_micros(), input.queued_len(), input.stopped());
    let resident_before = format!("{resident:?}");
    // Original low record2 names H0/017C, with pair formatting flags0C. Both
    // real constructor masks lack bit2; no synthetic formatter or record patch.
    assert_eq!(parse_grant_texts(executable).unwrap()[2], 0x17c);
    assert!(before_players
        .iter()
        .all(|player| player.grants_9c[0] & 4 == 0));
    let mut seeds = PLAYER_INITIAL_GRANTS.to_vec();
    seeds.push(2);
    let expected_mask = grant_closure(&parse_grant_table(executable).unwrap(), &seeds).unwrap();
    let mut expected_players = before_players.clone();
    for player in &mut expected_players {
        player.grants_9c = expected_mask;
    }
    let actor = selector35(&mut arena);
    selector35_mut(&mut arena, actor).bytes_240[2..4].fill(Some(1));
    arena.actor_mut(actor).unwrap().word_7a = 2;
    let mut registry = arena.actor_handles().unwrap().clone();
    registry.release(arena.actor(actor).unwrap().handle);
    let mut mission = mission();
    let expected_mission = mission.clone();
    run(&mut arena, &mut mission, &grants).unwrap();
    assert_eq!(arena.world_players(), expected_players);
    assert_eq!(arena.first_player(), Some(before_players[0].id));
    for (player, message) in before_players.iter().zip(messages) {
        assert_eq!(arena.player_message(player.id).unwrap(), &message);
    }
    for (&reference, &state) in references.iter().zip(&before_references) {
        assert_eq!(input.analog_consumer_state(reference).unwrap(), state);
    }
    assert_eq!(
        (input.clock_micros(), input.queued_len(), input.stopped()),
        input_before
    );
    assert_eq!(format!("{resident:?}"), resident_before);
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{registry:?}")
    );
    assert_eq!(mission, expected_mission);
    assert!(arena.actor_list().is_empty());
    assert_eq!(arena.free, [actor.slot]);
    assert!(arena.lists.iter().all(Vec::is_empty));
    for (carrier, reference) in carriers.iter_mut().zip(references) {
        carrier.release_input_references(&mut input).unwrap();
        assert_eq!(
            input.analog_consumer_state(reference).unwrap().references,
            1
        );
        input.release_analog_consumer(reference).unwrap();
        assert!(input.analog_consumer_state(reference).is_err());
    }
}
