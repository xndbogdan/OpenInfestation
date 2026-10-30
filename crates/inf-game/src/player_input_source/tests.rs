use super::*;
use crate::input::{InputRuntime, InputStateError};
use crate::object_runtime::{
    LiveGrantContext, ObjectArena, PlayerConstructionError, PlayerConstructionPhase,
    PlayerInputError,
};
use crate::process_particles::factories::ParticleFactoryProcessState;

fn executable() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    std::fs::read(path).expect("supported retail Infest.exe is required")
}

fn request(index: usize) -> PlayerInputDescriptorRequest {
    let identities = [
        0x0054_ecf8,
        0x0054_ece8,
        0x0054_ed08,
        0x0053_a368,
        0x0053_a370,
        0x0053_a358,
        0x0053_a360,
        0x0053_a378,
        0x0053_a380,
        0x0053_a388,
        0x0053_a390,
    ];
    PlayerInputDescriptorRequest {
        kind: if index < 3 {
            PlayerInputDescriptorKind::Analog
        } else {
            PlayerInputDescriptorKind::Digital
        },
        identity: identities[index],
        player_offset: 0x5c + index as u32 * 4,
    }
}

struct Recording<'state, 'exe> {
    source: PlayerInputSource<'state, 'exe>,
    requests: Vec<PlayerInputDescriptorRequest>,
}

impl PlayerInputDescriptorSource for Recording<'_, '_> {
    type Error = PlayerInputSourceError;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error> {
        self.requests.push(request);
        self.source.descriptor(request)
    }
}

fn no_grant_sources() -> LiveGrantContext<'static> {
    LiveGrantContext {
        process: None,
        executable: None,
        resident: None,
        text_data: None,
    }
}

fn data_header(bytes: &[u8]) -> usize {
    let pe = read_u32(bytes, 0x3c).unwrap() as usize;
    let count = read_u16(bytes, pe + 6).unwrap() as usize;
    let start = pe + 24 + read_u16(bytes, pe + 20).unwrap() as usize;
    (0..count)
        .map(|index| start + index * 40)
        .find(|&offset| &bytes[offset..offset + 8] == b".data\0\0\0")
        .unwrap()
}

#[test]
fn verified_loader_seed_is_distinct_from_file_only_descriptor_parsing() {
    let bytes = executable();
    let original = bytes.clone();
    let owner = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let mapped = FrontendExecutable::parse(&bytes).unwrap();
    let mut source = PlayerInputSource {
        analog: &owner,
        executable: mapped,
    };
    for index in 0..3 {
        let requested = request(index);
        assert!(inf_formats::parse_analog_action_descriptor(mapped, requested.identity).is_err());
        assert_eq!(
            source.descriptor(requested).unwrap(),
            PlayerInputDescriptor::Analog(AnalogActionDescriptor {
                action: ActionDescriptor {
                    identity: requested.identity,
                    callback_address: None,
                    flags: 0,
                },
                period_micros: 0,
            })
        );
    }
    for index in 3..11 {
        let requested = request(index);
        assert_eq!(
            source.descriptor(requested).unwrap(),
            PlayerInputDescriptor::Digital(ActionDescriptor {
                identity: requested.identity,
                callback_address: (index == 10).then_some(0x0046_94d0),
                flags: 6,
            })
        );
    }
    assert_eq!(bytes, original);
}

#[test]
fn startup_rejects_changed_virtual_raw_and_supported_executable_profiles() {
    let bytes = executable();
    let data = data_header(&bytes);
    for offset in [data + 8, data + 12, data + 16, data + 20, data + 36] {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        assert!(PlayerAnalogDescriptors::native_startup(&changed).is_err());
    }
    let mut wrong_build = bytes.clone();
    let pe = read_u32(&wrong_build, 0x3c).unwrap() as usize;
    wrong_build[pe + 8] ^= 1;
    assert!(matches!(
        PlayerAnalogDescriptors::native_startup(&wrong_build),
        Err(PlayerInputSourceError::Executable(
            FrontendExecutableError::UnsupportedBuild
        ))
    ));
    assert!(matches!(
        PlayerAnalogDescriptors::native_startup(&bytes[..0x40]),
        Err(PlayerInputSourceError::Executable(_))
    ));
}

#[test]
fn supplied_current_records_are_returned_unchanged_and_unavailable_never_reseeds() {
    let bytes = executable();
    let startup = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let mut current = startup.current;
    let value = current[0].as_mut().unwrap();
    value.action.callback_address = Some(0x1234_5678);
    value.action.flags = 0x8000_0106;
    current[1] = None;
    current[2].as_mut().unwrap().period_micros = i32::MIN;
    let owner = PlayerAnalogDescriptors::from_current(current);
    let mut source = PlayerInputSource {
        analog: &owner,
        executable: FrontendExecutable::parse(&bytes).unwrap(),
    };
    for _ in 0..2 {
        assert_eq!(
            source.descriptor(request(0)).unwrap(),
            PlayerInputDescriptor::Analog(current[0].unwrap())
        );
        assert_eq!(
            source.descriptor(request(1)),
            Err(PlayerInputSourceError::UnavailableAnalog {
                identity: 0x0054_ece8,
            })
        );
        assert_eq!(
            source.descriptor(request(2)).unwrap(),
            PlayerInputDescriptor::Analog(current[2].unwrap())
        );
    }
    assert_eq!(owner.current, current);
    let unknown = PlayerInputDescriptorRequest {
        identity: 0x0054_ecf9,
        ..request(0)
    };
    assert_eq!(
        source.descriptor(unknown),
        Err(PlayerInputSourceError::UnknownAnalog {
            identity: unknown.identity,
        })
    );
}

#[test]
fn missing_second_record_retains_real_first_reference_and_precedes_later_timer_and_grants() {
    let bytes = executable();
    let startup = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let mut current = startup.current;
    current[1] = None;
    current[2].as_mut().unwrap().period_micros = 1;
    let owner = PlayerAnalogDescriptors::from_current(current);
    let mut descriptors = Recording {
        source: PlayerInputSource {
            analog: &owner,
            executable: FrontendExecutable::parse(&bytes).unwrap(),
        },
        requests: Vec::new(),
    };
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x7100);
    let mut input = InputRuntime::new(987);
    let selected = input.create_context().unwrap();
    input.select_context(Some(selected)).unwrap();
    assert!(matches!(
        arena.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &no_grant_sources()
        ),
        Err(PlayerConstructionError::Input(PlayerInputError::Source { request: failed, error }))
            if failed == request(1)
                && error == (PlayerInputSourceError::UnavailableAnalog { identity: failed.identity })
    ));
    assert_eq!(descriptors.requests, [request(0), request(1)]);
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Stopped);
    assert!(carrier.unpublished_message().is_none());
    assert!(carrier.unpublished_grants().is_none());
    assert!(arena.world_players().is_empty());
    let id = input
        .acquire_analog_consumer(current[0].unwrap(), 0x7100)
        .unwrap();
    assert_eq!(input.analog_consumer_state(id).unwrap().references, 2);
    input.release_analog_consumer(id).unwrap();
    carrier.release_input_references(&mut input).unwrap();
    assert_eq!(
        input.analog_consumer_state(id),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(input.selected_context(), Some(selected));
    assert_eq!(input.clock_micros(), 987);
    assert_eq!(input.queued_len(), 0);
    assert_eq!(owner.current, current);
}

#[test]
fn wrong_identity_and_nonzero_period_keep_existing_reached_input_error_priority() {
    let bytes = executable();
    let startup = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    for identity_error in [false, true] {
        let mut current = startup.current;
        if identity_error {
            current[1].as_mut().unwrap().action.identity ^= 4;
        } else {
            current[1].as_mut().unwrap().period_micros = i32::MIN;
        }
        current[2] = None;
        let owner = PlayerAnalogDescriptors::from_current(current);
        let mut descriptors = Recording {
            source: PlayerInputSource {
                analog: &owner,
                executable: FrontendExecutable::parse(&bytes).unwrap(),
            },
            requests: Vec::new(),
        };
        let mut arena = ObjectArena::new();
        let mut carrier = arena.reserve_world_player_construction(0x7200);
        let mut input = InputRuntime::new(123);
        let error = arena
            .construct_world_player(
                &mut carrier,
                &mut input,
                &mut descriptors,
                &no_grant_sources(),
            )
            .unwrap_err();
        if identity_error {
            assert!(matches!(error,
                PlayerConstructionError::Input(PlayerInputError::Identity { request: failed, actual })
                    if failed == request(1) && actual == (failed.identity ^ 4)
            ));
        } else {
            assert!(matches!(error,
                PlayerConstructionError::Input(PlayerInputError::Input { request: failed, error: InputStateError::UnsupportedAnalogTimer(i32::MIN) })
                    if failed == request(1)
            ));
        }
        assert_eq!(descriptors.requests, [request(0), request(1)]);
        assert!(carrier.unpublished_message().is_none());
        let id = input
            .acquire_analog_consumer(current[0].unwrap(), 0x7200)
            .unwrap();
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 2);
        input.release_analog_consumer(id).unwrap();
        carrier.release_input_references(&mut input).unwrap();
        assert!(input.analog_consumer_state(id).is_err());
    }
}

#[test]
fn first_digital_mapping_error_is_lazy_after_three_real_analog_acquisitions() {
    let bytes = executable();
    let owner = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let mut missing_digital = bytes.clone();
    let data = data_header(&missing_digital);
    let raw_bytes = request(3).identity - DATA_ADDRESS;
    missing_digital[data + 16..data + 20].copy_from_slice(&raw_bytes.to_le_bytes());
    // An explicitly truncated raw view, not a different loader seed or fallback.
    let mut descriptors = Recording {
        source: PlayerInputSource {
            analog: &owner,
            executable: FrontendExecutable::parse(&missing_digital).unwrap(),
        },
        requests: Vec::new(),
    };
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x7300);
    let mut input = InputRuntime::new(456);
    assert!(matches!(
        arena.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &no_grant_sources()
        ),
        Err(PlayerConstructionError::Input(PlayerInputError::Source {
            request: failed,
            error: PlayerInputSourceError::Digital(InputTableError::Executable(
                FrontendExecutableError::DataAddress(address)
            )),
        })) if failed == request(3) && address == failed.identity
    ));
    assert_eq!(
        descriptors.requests,
        (0..4).map(request).collect::<Vec<_>>()
    );
    let ids = owner.current.map(|record| {
        let id = input
            .acquire_analog_consumer(record.unwrap(), 0x7300)
            .unwrap();
        assert_eq!(input.analog_consumer_state(id).unwrap().references, 2);
        input.release_analog_consumer(id).unwrap();
        id
    });
    carrier.release_input_references(&mut input).unwrap();
    for id in ids {
        assert!(input.analog_consumer_state(id).is_err());
    }
    assert!(arena.world_players().is_empty());
    assert!(carrier.unpublished_message().is_none());
    assert_eq!(input.clock_micros(), 456);
}

#[test]
fn repeated_source_loans_preserve_actual_shared_references_and_conflicts() {
    let bytes = executable();
    let owner = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let original = owner.current;
    let mapped = FrontendExecutable::parse(&bytes).unwrap();
    let mut input = InputRuntime::new(888);
    let mut ids = Vec::new();
    for _ in 0..2 {
        let mut source = PlayerInputSource {
            analog: &owner,
            executable: mapped,
        };
        let PlayerInputDescriptor::Analog(value) = source.descriptor(request(0)).unwrap() else {
            panic!("analog record expected");
        };
        ids.push(input.acquire_analog_consumer(value, 0x7400).unwrap());
    }
    assert_eq!(ids[0], ids[1]);
    assert_eq!(input.analog_consumer_state(ids[0]).unwrap().references, 2);
    let mut changed = original;
    changed[0].as_mut().unwrap().action.flags = 6;
    let conflicting = PlayerAnalogDescriptors::from_current(changed);
    let mut source = PlayerInputSource {
        analog: &conflicting,
        executable: mapped,
    };
    let PlayerInputDescriptor::Analog(value) = source.descriptor(request(0)).unwrap() else {
        panic!("analog record expected");
    };
    assert_eq!(
        input.acquire_analog_consumer(value, 0x7500),
        Err(InputStateError::DescriptorConflict)
    );
    assert_eq!(input.analog_consumer_state(ids[0]).unwrap().references, 2);
    assert_eq!(owner.current, original);
    input.release_analog_consumer(ids[0]).unwrap();
    assert_eq!(input.analog_consumer_state(ids[1]).unwrap().references, 1);
    input.release_analog_consumer(ids[1]).unwrap();
    assert_eq!(
        input.analog_consumer_state(ids[0]),
        Err(InputStateError::InvalidAnalogConsumer)
    );
    assert_eq!(input.clock_micros(), 888);
}

#[test]
fn real_constructor_consumes_retained_source_in_all_eleven_slots_and_releases_actual_refs() {
    let bytes = executable();
    let owner = PlayerAnalogDescriptors::native_startup(&bytes).unwrap();
    let original = owner.current;
    let mapped = FrontendExecutable::parse(&bytes).unwrap();
    let mut descriptors = Recording {
        source: PlayerInputSource {
            analog: &owner,
            executable: mapped,
        },
        requests: Vec::new(),
    };
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x7600);
    let mut input = InputRuntime::new(999);
    let mut process = ParticleFactoryProcessState::native_startup();
    process.network_flag = 0x100;
    let context = LiveGrantContext {
        process: Some(&process),
        executable: Some(mapped),
        resident: None,
        text_data: None,
    };
    let player = arena
        .construct_world_player(&mut carrier, &mut input, &mut descriptors, &context)
        .unwrap();
    assert_eq!(
        descriptors.requests,
        (0..11).map(request).collect::<Vec<_>>()
    );
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Published);
    assert_eq!(arena.first_player(), Some(player));
    let PlayerInputDescriptor::Digital(value) = descriptors.source.descriptor(request(10)).unwrap()
    else {
        panic!("digital record expected");
    };
    let digital = input.acquire_consumer(value, 0x7600).unwrap();
    assert_eq!(input.consumer_state(digital).unwrap().references, 2);
    input.release_consumer(digital).unwrap();
    let analog = input
        .acquire_analog_consumer(original[0].unwrap(), 0x7600)
        .unwrap();
    assert_eq!(input.analog_consumer_state(analog).unwrap().references, 2);
    input.release_analog_consumer(analog).unwrap();
    carrier.release_input_references(&mut input).unwrap();
    assert!(input.consumer_state(digital).is_err());
    assert!(input.analog_consumer_state(analog).is_err());
    assert_eq!(arena.first_player(), Some(player));
    assert_eq!(owner.current, original);
    assert_eq!(input.clock_micros(), 999);
    assert_eq!(input.queued_len(), 0);
}
