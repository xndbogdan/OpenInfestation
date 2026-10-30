use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::input::{ConsumerId, EventPacket, InputCallback, InputHost, InputStateError};
use crate::object_runtime::{
    ObjectClassKind, PlayerDestructionError, PlayerDestructionPhase, PlayerInputDescriptor,
    PlayerInputDescriptorKind, PlayerInputDescriptorRequest, PlayerInputDescriptorSource,
};
use crate::player_input_source::PlayerAnalogDescriptors;
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::{Installation, ResidencyRequest, ResidentRuntime};
use std::{convert::Infallible, path::Path};

struct Sources {
    assets: FrontendAssets,
    analog: PlayerAnalogDescriptors,
    classification: ProfileNameClassification,
    residency: ResidentRuntime,
    process: ParticleFactoryProcessState,
}

impl Sources {
    fn boot() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let analog = PlayerAnalogDescriptors::native_startup(assets.executable_bytes()).unwrap();
        let classification = ProfileNameClassification::native_startup(executable).unwrap();
        let process = ParticleFactoryProcessState::native_startup();
        let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        residency.apply(ResidencyRequest::Boot).unwrap();
        Self {
            assets,
            analog,
            classification,
            residency,
            process,
        }
    }

    fn descriptors(&self) -> PlayerInputSource<'_, '_> {
        PlayerInputSource {
            analog: &self.analog,
            executable: FrontendExecutable::parse(self.assets.executable_bytes()).unwrap(),
        }
    }

    fn construct(&self, owner: &mut WorldLoadOwner, input: &mut InputRuntime, context: u32) {
        let mut construction = owner.arena.reserve_world_player_construction(context);
        let text = current_grant_text(&self.classification);
        let grants = crate::object_runtime::LiveGrantContext {
            process: Some(&self.process),
            executable: Some(FrontendExecutable::parse(self.assets.executable_bytes()).unwrap()),
            resident: Some(&self.residency),
            text_data: Some(&text),
        };
        owner
            .arena
            .construct_world_player(&mut construction, input, &mut self.descriptors(), &grants)
            .unwrap();
        owner.constructions.push(construction);
        owner.player = owner.arena.first_player();
    }

    fn observe(&self, input: &mut InputRuntime, context: u32, identity: u32) -> ConsumerId {
        let PlayerInputDescriptor::Digital(descriptor) = self
            .descriptors()
            .descriptor(PlayerInputDescriptorRequest {
                kind: PlayerInputDescriptorKind::Digital,
                identity,
                player_offset: 0,
            })
            .unwrap()
        else {
            unreachable!()
        };
        let id = input.acquire_consumer(descriptor, context).unwrap();
        assert_eq!(input.consumer_state(id).unwrap().references, 2);
        input.release_consumer(id).unwrap();
        id
    }
}

fn owner() -> WorldLoadOwner {
    WorldLoadOwner::unprepared().unwrap()
}

#[derive(Default)]
struct Host(Vec<InputCallback>);

impl InputHost for Host {
    type Error = Infallible;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(7001)
    }
    fn callback(
        &mut self,
        callback: InputCallback,
        _: &mut InputRuntime,
    ) -> Result<(), Self::Error> {
        self.0.push(callback);
        Ok(())
    }
}

#[test]
fn current_grant_projection_preserves_missing_binding_and_each_raw_byte() {
    let unavailable = ProfileNameClassification { high_bytes: None };
    let text = current_grant_text(&unavailable);
    assert!(text.null_substitute.is_none());
    assert!(text.pair_classification.is_none());
    let mut high = [None; 256];
    high[b':' as usize] = Some(0x80);
    let current = ProfileNameClassification {
        high_bytes: Some(high),
    };
    let text = current_grant_text(&current);
    assert!(text.null_substitute.is_none());
    assert_eq!(
        text.pair_classification.unwrap(),
        GrantTextPairClassification {
            colon_high: Some(0x80),
            space_high: None,
        }
    );
    assert_eq!(current.high_bytes.unwrap(), high);
}

#[test]
fn callback_allocation_is_monotonic_port_identity_and_stops_before_wrap() {
    let mut next = PLAYER_CALLBACK_CONTEXT_START;
    assert_eq!(
        allocate_player_callback_context(&mut next),
        Ok(PLAYER_CALLBACK_CONTEXT_START)
    );
    assert_eq!(
        allocate_player_callback_context(&mut next),
        Ok(PLAYER_CALLBACK_CONTEXT_START + 1)
    );
    next = u32::MAX;
    assert!(allocate_player_callback_context(&mut next).is_err());
    assert_eq!(next, u32::MAX);
}

#[test]
fn deleting_fifo_retains_actual_records_context_and_shared_queued_callback() {
    let sources = Sources::boot();
    let mut owner = owner();
    let sentinel = owner.arena.allocate(ObjectClassKind::Base);
    let sentinel_before = owner.arena.object(sentinel).unwrap().clone();
    let source_before = format!(
        "{:?}{:?}{:?}",
        sources.analog, sources.classification, sources.process
    );
    let mission_before = owner.mission.clone();
    let mut input = InputRuntime::new(81);
    let context = input.create_context().unwrap();
    input.select_context(Some(context)).unwrap();
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START);
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START + 1);
    let first = owner.constructions[0].player();
    let second = owner.constructions[1].player();
    let records = owner.arena.world_players().to_vec();
    assert_eq!(
        records.iter().map(|player| player.id).collect::<Vec<_>>(),
        [first, second]
    );
    let first_input = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a390);
    let second_input = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START + 1, 0x0053_a390);
    assert_ne!(first_input, second_input);
    let PlayerInputDescriptor::Digital(descriptor) = sources
        .descriptors()
        .descriptor(PlayerInputDescriptorRequest {
            kind: PlayerInputDescriptorKind::Digital,
            identity: 0x0053_a390,
            player_offset: 0,
        })
        .unwrap()
    else {
        unreachable!()
    };
    let shared = input
        .acquire_consumer(descriptor, PLAYER_CALLBACK_CONTEXT_START)
        .unwrap();
    let mut host = Host::default();
    input
        .emit(shared, 1, &mut EventPacket::default(), &mut host)
        .unwrap();
    assert_eq!(input.queued_len(), 1);

    retire_constructed_players(&mut owner, &mut input).unwrap();
    assert!(owner.arena.world_players().is_empty());
    assert_eq!(owner.player, None);
    for (construction, record) in owner.constructions.iter().zip(&records) {
        assert_eq!(
            construction.destruction_phase(),
            Some(PlayerDestructionPhase::Complete)
        );
        assert_eq!(construction.retiring_player(), Some(record));
    }
    assert_eq!(owner.arena.object(sentinel).unwrap(), &sentinel_before);
    assert_eq!(owner.mission, mission_before);
    assert_eq!(input.selected_context(), Some(context));
    assert_eq!(input.consumer_state(shared).unwrap().references, 1);
    assert_eq!(
        input.consumer_state(second_input),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(input.queued_len(), 1);
    input.poll(&mut host).unwrap();
    assert_eq!(
        host.0
            .iter()
            .map(|callback| (callback.address, callback.context, callback.value))
            .collect::<Vec<_>>(),
        [(0x0046_94d0, PLAYER_CALLBACK_CONTEXT_START, 1)]
    );
    assert!(!input.held(shared).unwrap());
    assert_eq!(
        format!(
            "{:?}{:?}{:?}",
            sources.analog, sources.classification, sources.process
        ),
        source_before
    );
    input.release_consumer(shared).unwrap();
}

#[test]
fn imported_second_player_stops_after_first_real_destructor_without_fabricated_carrier() {
    let sources = Sources::boot();
    let mut owner = owner();
    let mut input = InputRuntime::new(82);
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START);
    let constructed = owner.constructions[0].player();
    let imported = owner.arena.add_world_player([0x12345678; 6]);
    let imported_before = owner.arena.world_player(imported).unwrap().clone();
    let first_input = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a390);
    assert!(matches!(
        retire_constructed_players(&mut owner, &mut input),
        Err(PlayerLoopError::Boundary(
            "current player has no retained constructor carrier"
        ))
    ));
    assert!(owner.arena.world_player(constructed).is_err());
    assert_eq!(owner.arena.world_players(), [imported_before]);
    assert_eq!(owner.arena.first_player(), Some(imported));
    assert_eq!(owner.constructions.len(), 1);
    assert_eq!(
        owner.constructions[0].destruction_phase(),
        Some(PlayerDestructionPhase::Complete)
    );
    assert_eq!(
        input.consumer_state(first_input),
        Err(InputStateError::InvalidConsumer)
    );
}

#[test]
fn late_actual_input_failure_retains_unlinked_record_and_stops_next_fifo_player() {
    let sources = Sources::boot();
    let mut owner = owner();
    let mut input = InputRuntime::new(83);
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START);
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START + 1);
    let first = owner.constructions[0].player();
    let second = owner.constructions[1].player();
    let first_before = owner.arena.world_player(first).unwrap().clone();
    let second_before = owner.arena.world_player(second).unwrap().clone();
    let stale = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a380);
    let last = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a390);
    let earlier = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a378);
    // Explicit violated lifetime fixture: another owner releases +7C's actual
    // acquired reference. The constructor carrier retains its stale wrapper.
    input.release_consumer(stale).unwrap();
    assert!(matches!(
        retire_constructed_players(&mut owner, &mut input),
        Err(PlayerLoopError::Destruction(PlayerDestructionError::Input(
            _
        )))
    ));
    assert!(owner.arena.world_player(first).is_err());
    assert_eq!(owner.arena.world_players(), [second_before]);
    assert_eq!(
        owner.constructions[0].destruction_phase(),
        Some(PlayerDestructionPhase::MessageDestroyed)
    );
    assert_eq!(
        owner.constructions[0].retiring_player(),
        Some(&first_before)
    );
    assert_eq!(owner.constructions[1].destruction_phase(), None);
    assert_eq!(
        input.consumer_state(last),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(input.consumer_state(earlier).unwrap().references, 1);
    // The outer stopped pass must prevent replay. The lower destructor also
    // independently rejects the same unfinished carrier before more effects.
    assert!(matches!(
        owner
            .arena
            .destroy_constructed_world_player(&mut owner.constructions[0], &mut input,),
        Err(PlayerDestructionError::AlreadyAttempted(
            PlayerDestructionPhase::MessageDestroyed
        ))
    ));
}

#[test]
fn unpublished_carrier_is_retained_without_a_full_deleting_destructor() {
    let mut owner = owner();
    let mut input = InputRuntime::new(84);
    owner.constructions.push(
        owner
            .arena
            .reserve_world_player_construction(PLAYER_CALLBACK_CONTEXT_START),
    );
    let before = format!("{:?}", owner.arena);
    assert!(matches!(
        retire_constructed_players(&mut owner, &mut input),
        Err(PlayerLoopError::Boundary(_))
    ));
    assert_eq!(format!("{:?}", owner.arena), before);
    assert_eq!(
        owner.constructions[0].phase(),
        PlayerConstructionPhase::Allocated
    );
    assert_eq!(owner.constructions[0].destruction_phase(), None);
}

#[test]
fn actual_published_head_retires_before_later_stopped_constructor_boundary() {
    let sources = Sources::boot();
    let mut owner = owner();
    let mut input = InputRuntime::new(85);
    sources.construct(&mut owner, &mut input, PLAYER_CALLBACK_CONTEXT_START);
    let first = owner.constructions[0].player();
    let first_input = sources.observe(&mut input, PLAYER_CALLBACK_CONTEXT_START, 0x0053_a390);
    let mut partial = owner
        .arena
        .reserve_world_player_construction(PLAYER_CALLBACK_CONTEXT_START + 1);
    let context = crate::object_runtime::LiveGrantContext {
        process: Some(&sources.process),
        executable: Some(FrontendExecutable::parse(sources.assets.executable_bytes()).unwrap()),
        resident: None,
        text_data: None,
    };
    // Actual missing resident source stops grants only after all eleven real
    // acquisitions. No player FIFO entry or full destructor is fabricated.
    assert!(owner
        .arena
        .construct_world_player(
            &mut partial,
            &mut input,
            &mut sources.descriptors(),
            &context,
        )
        .is_err());
    assert_eq!(partial.phase(), PlayerConstructionPhase::Stopped);
    let partial_input = sources.observe(&mut input, partial.callback_context(), 0x0053_a390);
    let partial_mask = partial.unpublished_grants().copied();
    let partial_message = partial.unpublished_message().unwrap().clone();
    owner.constructions.push(partial);
    assert!(matches!(
        retire_constructed_players(&mut owner, &mut input),
        Err(PlayerLoopError::Boundary(
            "unpublished player preparation requires explicit component/unwind custody"
        ))
    ));
    assert!(owner.arena.world_player(first).is_err());
    assert!(owner.arena.world_players().is_empty());
    assert_eq!(
        owner.constructions[0].destruction_phase(),
        Some(PlayerDestructionPhase::Complete)
    );
    assert_eq!(
        input.consumer_state(first_input),
        Err(InputStateError::InvalidConsumer)
    );
    assert_eq!(input.consumer_state(partial_input).unwrap().references, 1);
    assert_eq!(
        owner.constructions[1].phase(),
        PlayerConstructionPhase::Stopped
    );
    assert_eq!(
        owner.constructions[1].unpublished_grants().copied(),
        partial_mask
    );
    assert_eq!(
        owner.constructions[1].unpublished_message(),
        Some(&partial_message)
    );
    assert_eq!(owner.constructions[1].destruction_phase(), None);
}
