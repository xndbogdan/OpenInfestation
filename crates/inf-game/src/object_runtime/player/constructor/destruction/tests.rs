use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextPairClassification},
    input::{
        ActionDescriptor, AnalogConsumerId, ConsumerId, EventPacket, InputCallback, InputHost,
        InputStateError,
    },
    object_runtime::{
        ActorHandle, ActorRegistryConfig, ObjectClassKind, PlayerInputDescriptor,
        PlayerInputDescriptorKind, PlayerInputDescriptorRequest, PlayerInputDescriptorSource,
        PlayerMessageOperation, PlayerMessageRequest,
    },
    player_input_source::{PlayerAnalogDescriptors, PlayerInputSource},
    process_particles::factories::ParticleFactoryProcessState,
    profile_name_format::ProfileNameClassification,
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::FrontendExecutable;
use std::path::Path;

struct Sources {
    executable: Vec<u8>,
    analog: PlayerAnalogDescriptors,
    classification: ProfileNameClassification,
    resident: ResidentRuntime,
    process: ParticleFactoryProcessState,
}

impl Sources {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let executable = FrontendAssets::open(&root)
            .unwrap()
            .executable_bytes()
            .to_vec();
        let analog = PlayerAnalogDescriptors::native_startup(&executable).unwrap();
        let classification = ProfileNameClassification::native_startup(
            FrontendExecutable::parse(&executable).unwrap(),
        )
        .unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        Self {
            executable,
            analog,
            classification,
            resident,
            process: ParticleFactoryProcessState {
                network_flag: 0,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: Some(ActorHandle::from_raw(0x8123)),
            },
        }
    }

    fn source(&self) -> PlayerInputSource<'_, '_> {
        PlayerInputSource {
            analog: &self.analog,
            executable: FrontendExecutable::parse(&self.executable).unwrap(),
        }
    }

    fn construct(
        &self,
        arena: &mut ObjectArena,
        input: &mut InputRuntime,
        callback_context: u32,
    ) -> WorldPlayerConstruction {
        let mut carrier = arena.reserve_world_player_construction(callback_context);
        let text = GrantTextData {
            null_substitute: None,
            pair_classification: self.classification.high_bytes.as_ref().map(|bytes| {
                GrantTextPairClassification {
                    colon_high: bytes[usize::from(b':')],
                    space_high: bytes[usize::from(b' ')],
                }
            }),
        };
        let context = crate::object_runtime::grants::LiveGrantContext {
            process: Some(&self.process),
            executable: Some(FrontendExecutable::parse(&self.executable).unwrap()),
            resident: Some(&self.resident),
            text_data: Some(&text),
        };
        let expected = carrier.player();
        assert_eq!(
            arena
                .construct_world_player(&mut carrier, input, &mut self.source(), &context)
                .unwrap(),
            expected
        );
        carrier
    }
}

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x4567),
    })
    .unwrap();
    arena.allocate(ObjectClassKind::Base);
    arena.allocate(ObjectClassKind::VisualBase);
    arena
}

fn request(kind: PlayerInputDescriptorKind, identity: u32) -> PlayerInputDescriptorRequest {
    // The source consumes actual current descriptor identity/family. This
    // observer shares references explicitly; it is not another player ctor.
    PlayerInputDescriptorRequest {
        kind,
        identity,
        player_offset: 0,
    }
}

struct References {
    analog: [AnalogConsumerId; 3],
    digital: [ConsumerId; 8],
}

fn observe_references(sources: &Sources, input: &mut InputRuntime, context: u32) -> References {
    let mut source = sources.source();
    let analog = [0x0054_ecf8, 0x0054_ece8, 0x0054_ed08].map(|identity| {
        let PlayerInputDescriptor::Analog(descriptor) = source
            .descriptor(request(PlayerInputDescriptorKind::Analog, identity))
            .unwrap()
        else {
            unreachable!()
        };
        let id = input.acquire_analog_consumer(descriptor, context).unwrap();
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 2);
        input.release_analog_consumer(id).unwrap();
        id
    });
    let digital = [
        0x0053_a368,
        0x0053_a370,
        0x0053_a358,
        0x0053_a360,
        0x0053_a378,
        0x0053_a380,
        0x0053_a388,
        0x0053_a390,
    ]
    .map(|identity| {
        let PlayerInputDescriptor::Digital(descriptor) = source
            .descriptor(request(PlayerInputDescriptorKind::Digital, identity))
            .unwrap()
        else {
            unreachable!()
        };
        let id = input.acquire_consumer(descriptor, context).unwrap();
        assert_eq!(input.consumer_state(id).unwrap().references, 2);
        input.release_consumer(id).unwrap();
        id
    });
    References { analog, digital }
}

fn non_player_owners(arena: &ObjectArena) -> String {
    // Every arena owner other than the actual FIFO records compared separately.
    format!(
        "{:?}{:?}{:?}",
        (
            arena.identity,
            &arena.slots,
            &arena.free,
            arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations,
        ),
        (
            &arena.actor_handles,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            arena.world_load,
        ),
        (arena.players.next_id, arena.players.camera_74),
    )
}

#[derive(Default)]
struct Host {
    callbacks: Vec<InputCallback>,
}

impl InputHost for Host {
    type Error = std::convert::Infallible;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(9001)
    }

    fn callback(
        &mut self,
        event: InputCallback,
        _input: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        self.callbacks.push(event);
        Ok(())
    }
}

#[test]
fn complete_destructor_unlinks_same_fifo_message_then_releases_actual_inputs() {
    let sources = Sources::new();
    let source_before = format!(
        "{:?}{:?}{:?}",
        sources.analog, sources.classification, sources.process
    );
    let mut arena = arena();
    let mut input = InputRuntime::new(77);
    let mut first = sources.construct(&mut arena, &mut input, 0x9100);
    let mut second = sources.construct(&mut arena, &mut input, 0x9200);
    let first_refs = observe_references(&sources, &mut input, first.callback_context());
    let second_refs = observe_references(&sources, &mut input, second.callback_context());
    let token = arena.player_message(first.player()).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient: token,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    // Enabling the send gate does not populate any constructor-NULL storage.
    let first_before = arena.world_player(first.player()).unwrap().clone();
    let second_before = arena.world_player(second.player()).unwrap().clone();
    let others = non_player_owners(&arena);
    arena
        .destroy_constructed_world_player(&mut first, &mut input)
        .unwrap();
    assert_eq!(
        first.destruction_phase(),
        Some(PlayerDestructionPhase::Complete)
    );
    assert_eq!(first.retiring_player(), Some(&first_before));
    assert_eq!(arena.world_players(), [second_before.clone()]);
    assert_eq!(arena.first_player(), Some(second.player()));
    assert_eq!(non_player_owners(&arena), others);
    assert!(arena.world_player(first.player()).is_err());
    assert_eq!(
        arena.apply_player_message(PlayerMessageRequest {
            recipient: token,
            operation: PlayerMessageOperation::Suppress,
        }),
        Err(PlayerMessageError::UnknownMessage(token))
    );
    for id in first_refs.digital {
        assert_eq!(
            input.consumer_state(id),
            Err(InputStateError::InvalidConsumer)
        );
    }
    for id in first_refs.analog {
        assert_eq!(
            input.analog_consumer_state(id),
            Err(InputStateError::InvalidAnalogConsumer)
        );
    }
    for id in second_refs.digital {
        assert_eq!(input.consumer_state(id).unwrap().references, 1);
    }
    for id in second_refs.analog {
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 1);
    }
    let before_retry = format!("{arena:?}");
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut first, &mut input),
        Err(PlayerDestructionError::AlreadyAttempted(
            PlayerDestructionPhase::Complete
        ))
    ));
    assert_eq!(format!("{arena:?}"), before_retry);
    assert_eq!(input.clock_micros(), 77);
    assert_eq!(input.selected_context(), None);
    assert_eq!(input.queued_len(), 0);
    assert_eq!(
        format!(
            "{:?}{:?}{:?}",
            sources.analog, sources.classification, sources.process
        ),
        source_before
    );
    arena
        .destroy_constructed_world_player(&mut second, &mut input)
        .unwrap();
    assert!(arena.world_players().is_empty());
}

#[test]
fn deletion_preserves_shared_reference_and_queued_callback_snapshots() {
    let sources = Sources::new();
    let mut arena = arena();
    let mut input = InputRuntime::new(78);
    let mut carrier = sources.construct(&mut arena, &mut input, 0x9300);
    let refs = observe_references(&sources, &mut input, carrier.callback_context());
    let PlayerInputDescriptor::Digital(descriptor) = sources
        .source()
        .descriptor(request(PlayerInputDescriptorKind::Digital, 0x0053_a390))
        .unwrap()
    else {
        unreachable!()
    };
    let shared = input
        .acquire_consumer(descriptor, carrier.callback_context())
        .unwrap();
    assert_eq!(shared, refs.digital[7]);
    let unrelated = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 0x1111,
                callback_address: Some(0x2222),
                flags: 6,
            },
            0x9400,
        )
        .unwrap();
    let mut host = Host::default();
    input
        .emit(shared, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    input
        .emit(unrelated, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    assert_eq!(input.queued_len(), 2);
    arena
        .destroy_constructed_world_player(&mut carrier, &mut input)
        .unwrap();
    assert_eq!(input.consumer_state(shared).unwrap().references, 1);
    assert_eq!(input.consumer_state(unrelated).unwrap().references, 1);
    assert_eq!(input.queued_len(), 2);
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.callbacks
            .iter()
            .map(|event| (event.address, event.context, event.value))
            .collect::<Vec<_>>(),
        [(0x0046_94d0, 0x9300, 1), (0x2222, 0x9400, 1)]
    );
    // Other player inputs reached final release: their bulk queue invalidation
    // clears both live state targets, while the two callback snapshots survive.
    assert!(!input.held(shared).unwrap());
    assert!(!input.held(unrelated).unwrap());
    input.release_consumer(shared).unwrap();
    input.release_consumer(unrelated).unwrap();
}

#[test]
fn foreign_and_absent_fifo_fail_before_message_or_reference_effects() {
    let sources = Sources::new();
    let mut owner = arena();
    let mut input = InputRuntime::new(79);
    let mut carrier = sources.construct(&mut owner, &mut input, 0x9500);
    let refs = observe_references(&sources, &mut input, carrier.callback_context());
    let mut foreign = arena();
    foreign.add_world_player([0xdeadbeef; 6]);
    let owner_before = format!("{owner:?}");
    let foreign_before = format!("{foreign:?}");
    assert!(matches!(
        foreign.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::ForeignArena)
    ));
    assert_eq!(format!("{owner:?}"), owner_before);
    assert_eq!(format!("{foreign:?}"), foreign_before);
    assert_eq!(carrier.destruction_phase(), None);
    assert!(carrier.retiring_player().is_none());

    // Declared malformed lifetime fixture: the actual published record is
    // absent. No successful destructor or native pointer-fault claim follows.
    let record = owner.players.list.pop().unwrap();
    let missing_before = format!("{owner:?}");
    assert!(matches!(
        owner.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::NotListed(player)) if player == carrier.player()
    ));
    assert_eq!(format!("{owner:?}"), missing_before);
    for id in refs.digital {
        assert_eq!(input.consumer_state(id).unwrap().references, 1);
    }
    for id in refs.analog {
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 1);
    }
    owner.players.list.push(record);
    owner
        .destroy_constructed_world_player(&mut carrier, &mut input)
        .unwrap();
}

#[test]
fn unpublished_stop_uses_only_explicit_component_cleanup() {
    let sources = Sources::new();
    let mut arena = arena();
    let mut input = InputRuntime::new(80);
    let mut carrier = arena.reserve_world_player_construction(0x9600);
    let before = format!("{arena:?}");
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::NotPublished(
            PlayerConstructionPhase::Allocated
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    let context = crate::object_runtime::grants::LiveGrantContext {
        process: Some(&sources.process),
        executable: Some(FrontendExecutable::parse(&sources.executable).unwrap()),
        resident: None,
        text_data: None,
    };
    assert!(arena
        .construct_world_player(&mut carrier, &mut input, &mut sources.source(), &context)
        .is_err());
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Stopped);
    let refs = observe_references(&sources, &mut input, carrier.callback_context());
    let mask = carrier.unpublished_grants().copied();
    let message = carrier.unpublished_message().unwrap().clone();
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::NotPublished(
            PlayerConstructionPhase::Stopped
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(carrier.unpublished_grants().copied(), mask);
    assert_eq!(carrier.unpublished_message(), Some(&message));
    assert_eq!(carrier.destruction_phase(), None);
    assert!(carrier.retiring_player().is_none());
    carrier.release_input_references(&mut input).unwrap();
    for id in refs.digital {
        assert_eq!(
            input.consumer_state(id),
            Err(InputStateError::InvalidConsumer)
        );
    }
    for id in refs.analog {
        assert_eq!(
            input.analog_consumer_state(id),
            Err(InputStateError::InvalidAnalogConsumer)
        );
    }
}

#[test]
fn manually_released_published_components_are_not_skipped_by_full_destructor() {
    let sources = Sources::new();
    let mut arena = arena();
    let mut input = InputRuntime::new(81);
    let mut carrier = sources.construct(&mut arena, &mut input, 0x9700);
    carrier.release_input_references(&mut input).unwrap();
    let before = format!("{arena:?}");
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::InputComponentReleaseAttempted(player)) if player == carrier.player()
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(carrier.destruction_phase(), None);
    assert!(carrier.retiring_player().is_none());
}

#[test]
fn late_input_failure_retains_unlinked_message_and_reverse_release_prefix() {
    let sources = Sources::new();
    let mut arena = arena();
    let mut input = InputRuntime::new(82);
    let mut carrier = sources.construct(&mut arena, &mut input, 0x9800);
    let refs = observe_references(&sources, &mut input, carrier.callback_context());
    let record = arena.world_player(carrier.player()).unwrap().clone();
    let others = non_player_owners(&arena);
    // Genuine source-domain violation: another owner final-releases +7C's
    // actual consumer. The carrier's stale wrapper is retained, not repaired.
    input.release_consumer(refs.digital[5]).unwrap();
    let failed = PlayerInputReleaseError {
        request: PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Digital,
            identity: 0x0053_a380,
            player_offset: 0x7c,
        },
        error: InputStateError::InvalidConsumer,
    };
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::Input(error)) if error == failed
    ));
    assert!(arena.world_players().is_empty());
    assert_eq!(carrier.retiring_player(), Some(&record));
    assert_eq!(
        carrier.destruction_phase(),
        Some(PlayerDestructionPhase::MessageDestroyed)
    );
    assert_eq!(non_player_owners(&arena), others);
    for index in 6..8 {
        assert_eq!(
            input.consumer_state(refs.digital[index]),
            Err(InputStateError::InvalidConsumer)
        );
    }
    for index in 0..5 {
        assert_eq!(
            input
                .consumer_state(refs.digital[index])
                .unwrap()
                .references,
            1
        );
    }
    for id in refs.analog {
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 1);
    }
    assert!(matches!(
        arena.destroy_constructed_world_player(&mut carrier, &mut input),
        Err(PlayerDestructionError::AlreadyAttempted(
            PlayerDestructionPhase::MessageDestroyed
        ))
    ));
    assert_eq!(carrier.release_input_references(&mut input), Err(failed));
    assert_eq!(carrier.retiring_player(), Some(&record));
    assert_eq!(non_player_owners(&arena), others);
}
