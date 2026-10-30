use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextError, GrantTextLiteral, GrantTextPairClassification},
    input::InputStateError,
    object_runtime::{
        ActorHandle, ActorRegistryConfig, GrantOwner, LiveGrantError, ObjectClassKind, ObjectId,
        PlayerConstructionPhase, PlayerInputDescriptor, PlayerInputDescriptorRequest,
        PlayerInputError,
    },
    player_input_source::{PlayerAnalogDescriptors, PlayerInputSource, PlayerInputSourceError},
    process_particles::factories::ParticleFactoryProcessState,
    profile_name_format::ProfileNameClassification,
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{grant_closure, parse_grant_table, FrontendExecutable, PlayerSaveState};
use std::path::Path;

struct Sources {
    assets: FrontendAssets,
    resident: ResidentRuntime,
    analog: PlayerAnalogDescriptors,
    classification: ProfileNameClassification,
}

impl Sources {
    fn boot() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let analog = PlayerAnalogDescriptors::native_startup(assets.executable_bytes()).unwrap();
        let classification = ProfileNameClassification::native_startup(executable).unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        Self {
            assets,
            resident,
            analog,
            classification,
        }
    }

    fn executable(&self) -> FrontendExecutable<'_> {
        FrontendExecutable::parse(self.assets.executable_bytes()).unwrap()
    }

    fn descriptors(&self) -> Descriptors<'_> {
        Descriptors {
            source: PlayerInputSource {
                analog: &self.analog,
                executable: self.executable(),
            },
            requests: Vec::new(),
            fail_offset: None,
        }
    }
}

#[derive(Debug, Error)]
enum DescriptorError {
    #[error("declared current descriptor unavailable")]
    Unavailable,
    #[error(transparent)]
    Current(#[from] PlayerInputSourceError),
}

struct Descriptors<'a> {
    source: PlayerInputSource<'a, 'a>,
    requests: Vec<PlayerInputDescriptorRequest>,
    fail_offset: Option<u32>,
}

impl PlayerInputDescriptorSource for Descriptors<'_> {
    type Error = DescriptorError;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error> {
        self.requests.push(request);
        if self.fail_offset == Some(request.player_offset) {
            return Err(DescriptorError::Unavailable);
        }
        Ok(self.source.descriptor(request)?)
    }
}

/// Project this test's actual retained CRT owner for the explicit operation.
/// No live PortSession attachment, missing-byte defaults or545900 are implied.
fn current_text(classification: &ProfileNameClassification) -> GrantTextData<'_> {
    GrantTextData {
        null_substitute: None,
        pair_classification: classification.high_bytes.as_ref().map(|high| {
            GrantTextPairClassification {
                colon_high: high[b':' as usize],
                space_high: high[b' ' as usize],
            }
        }),
    }
}

fn grants<'a>(
    sources: &'a Sources,
    process: Option<&'a ParticleFactoryProcessState>,
    text: Option<&'a GrantTextData<'a>>,
) -> LiveGrantContext<'a> {
    LiveGrantContext {
        process,
        executable: Some(sources.executable()),
        resident: Some(&sources.resident),
        text_data: text,
    }
}

fn dirty_mission() -> MissionSaveState {
    MissionSaveState {
        mission_selection: 0xfedcba98,
        elapsed_seconds: 0x88776655,
        owner_b0: [0x12345678; 100],
        owner_240: [0xfedcba98; 100],
        owner_3c: [0x80000003; 25],
        owner_a4: 0xffff,
    }
}

struct Owners {
    arena: ObjectArena,
    constructions: Vec<WorldPlayerConstruction>,
    mission: MissionSaveState,
    owner: bool,
    cycle: AttractCycleState,
    phase: PlayerPreparePhase,
    input: InputRuntime,
    sentinel: ObjectId,
}

struct Attempt<'a, 'g, S> {
    callback_context: u32,
    attract: bool,
    descriptors: &'a mut S,
    grants: &'a LiveGrantContext<'g>,
    selections: &'a FrontendAttractSelectionTable,
}

impl Owners {
    fn new() -> Self {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let sentinel = arena.allocate(ObjectClassKind::Base);
        arena.set_flag0(sentinel, true).unwrap();
        let mut cycle = AttractCycleState::from_native_constructor_fields();
        // Explicit supplied current backing, not additional constructor stores.
        cycle.armed_5d8 = 0x80;
        cycle.selection_index_04 = 7;
        cycle.timer_micros_5d4 = Some(0x89abcdef);
        cycle.publication.handle_5dc = Some(ActorHandle::from_raw(0x8001ffff));
        let mut input = InputRuntime::new(0x123456789);
        let selected = input.create_context().unwrap();
        input.select_context(Some(selected)).unwrap();
        Self {
            arena,
            constructions: Vec::new(),
            mission: dirty_mission(),
            owner: false,
            cycle,
            phase: PlayerPreparePhase::NotStarted,
            input,
            sentinel,
        }
    }

    fn run<S: PlayerInputDescriptorSource>(
        &mut self,
        attempt: Attempt<'_, '_, S>,
    ) -> Result<PlayerPrepareOutcome, PlayerPrepareError<S::Error>> {
        prepare_player(
            PlayerPrepareRequest {
                arena: &mut self.arena,
                constructions: &mut self.constructions,
                mission: &mut self.mission,
                owner: &mut self.owner,
                cycle: &mut self.cycle,
                phase: &mut self.phase,
                callback_context: attempt.callback_context,
                attract: attempt.attract,
                source: crate::player_prepare::PlayerPrepareSource::NewGame,
                network_world: None,
            },
            PlayerPrepareContext {
                input: &mut self.input,
                descriptors: attempt.descriptors,
                grants: attempt.grants,
                selections: attempt.selections,
            },
        )
    }

    fn release(&mut self) {
        for construction in &mut self.constructions {
            construction
                .release_input_references(&mut self.input)
                .unwrap();
        }
    }
}

#[test]
fn input_error_keeps_retained_carrier_and_real_prefix_before_armed_mission_or_grants() {
    let sources = Sources::boot();
    let mut descriptors = sources.descriptors();
    descriptors.fail_offset = Some(0x80);
    let context = grants(&sources, None, None);
    let mut owners = Owners::new();
    let mission = owners.mission.clone();
    let cycle = owners.cycle.clone();
    let sentinel = owners.arena.object(owners.sentinel).unwrap().clone();
    let selected = owners.input.selected_context();
    assert!(matches!(
        owners.run(Attempt {
            callback_context: 0x8100,
            attract: true,
            descriptors: &mut descriptors,
            grants: &context,
            selections: sources.assets.attract_selections(),
        }),
        Err(PlayerPrepareError::Construction(PlayerConstructionError::Input(
            PlayerInputError::Source { request, error: DescriptorError::Unavailable }
        ))) if request.player_offset == 0x80
    ));
    assert!(owners.owner);
    assert_eq!(owners.phase, PlayerPreparePhase::Constructing);
    assert_eq!(owners.constructions.len(), 1);
    let carrier = &owners.constructions[0];
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Stopped);
    assert_eq!(carrier.callback_context(), 0x8100);
    assert!(carrier.unpublished_message().is_none());
    assert!(carrier.unpublished_grants().is_none());
    assert!(owners.arena.first_player().is_none());
    assert_eq!(owners.mission, mission);
    assert_eq!(owners.cycle, cycle);
    assert_eq!(owners.arena.object(owners.sentinel).unwrap(), &sentinel);
    assert_eq!(descriptors.requests.len(), 10);
    let requested = descriptors.requests[0];
    let PlayerInputDescriptor::Analog(value) = descriptors.source.descriptor(requested).unwrap()
    else {
        panic!("actual analog source");
    };
    let retained = owners.input.acquire_analog_consumer(value, 0x8100).unwrap();
    assert_eq!(
        owners
            .input
            .analog_consumer_state(retained)
            .unwrap()
            .references,
        2
    );
    owners.input.release_analog_consumer(retained).unwrap();
    assert_eq!(owners.input.selected_context(), selected);
    assert_eq!(owners.input.clock_micros(), 0x123456789);
    assert_eq!(owners.input.queued_len(), 0);
    owners.release();
    assert_eq!(
        owners.input.analog_consumer_state(retained),
        Err(InputStateError::InvalidAnalogConsumer)
    );
}

#[test]
fn missing_process_is_late_first_grant_error_with_all_eleven_references_retained() {
    let sources = Sources::boot();
    let mut descriptors = sources.descriptors();
    let context = grants(&sources, None, None);
    let mut owners = Owners::new();
    let mission = owners.mission.clone();
    let cycle = owners.cycle.clone();
    assert!(matches!(
        owners.run(Attempt {
            callback_context: 0x8200,
            attract: false,
            descriptors: &mut descriptors,
            grants: &context,
            selections: sources.assets.attract_selections(),
        }),
        Err(PlayerPrepareError::Construction(
            PlayerConstructionError::Grant(LiveGrantError::MissingProcess {
                owner: GrantOwner::Player9c(_)
            })
        ))
    ));
    assert_eq!(owners.constructions.len(), 1);
    let carrier = &owners.constructions[0];
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Stopped);
    assert_eq!(carrier.unpublished_grants(), Some(&[2, 0, 0, 0, 0, 0]));
    assert_eq!(carrier.unpublished_message().unwrap().suppression_byte(), 1);
    assert!(owners.arena.world_player(carrier.player()).is_err());
    assert_eq!(descriptors.requests.len(), 11);
    assert_eq!(owners.phase, PlayerPreparePhase::Constructing);
    assert!(owners.owner);
    assert_eq!(owners.cycle, cycle);
    assert_eq!(owners.mission, mission);
    owners.release();
}

#[test]
fn late_missing_null_keeps_complete_grants_without_clearing_armed_or_selecting() {
    let sources = Sources::boot();
    let mut descriptors = sources.descriptors();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.network_flag = 0x100;
    process.process_null_handle = None;
    let before_process = process;
    let context = grants(&sources, Some(&process), None);
    let mut owners = Owners::new();
    let mission = owners.mission.clone();
    let cycle = owners.cycle.clone();
    assert!(matches!(
        owners.run(Attempt {
            callback_context: 0x8300,
            attract: true,
            descriptors: &mut descriptors,
            grants: &context,
            selections: sources.assets.attract_selections(),
        }),
        Err(PlayerPrepareError::Construction(
            PlayerConstructionError::MissingProcessNull
        ))
    ));
    assert_eq!(
        owners.constructions[0].unpublished_grants(),
        Some(
            &grant_closure(
                &parse_grant_table(sources.executable()).unwrap(),
                &inf_formats::PLAYER_INITIAL_GRANTS,
            )
            .unwrap()
        )
    );
    assert!(owners.arena.first_player().is_none());
    assert_eq!(owners.cycle, cycle);
    assert_eq!(owners.mission, mission);
    assert_eq!(process, before_process);
    assert!(owners.owner);
    owners.release();
}

#[test]
fn current_crt_binding_and_each_literal_error_precede_null_and_reset() {
    let sources = Sources::boot();
    for (colon, high) in [
        (true, None),
        (false, None),
        (true, Some(0x80)),
        (false, Some(0xff)),
    ] {
        let mut classification = sources.classification.clone();
        let table = classification.high_bytes.as_mut().unwrap();
        table[if colon { b':' } else { b' ' } as usize] = high;
        let original_classification = classification.clone();
        let text = current_text(&classification);
        let mut process = ParticleFactoryProcessState::native_startup();
        process.process_null_handle = None;
        let context = grants(&sources, Some(&process), Some(&text));
        let mut descriptors = sources.descriptors();
        let mut owners = Owners::new();
        let mission = owners.mission.clone();
        let cycle = owners.cycle.clone();
        let error = owners
            .run(Attempt {
                callback_context: 0x8400,
                attract: true,
                descriptors: &mut descriptors,
                grants: &context,
                selections: sources.assets.attract_selections(),
            })
            .unwrap_err();
        let literal = if colon {
            GrantTextLiteral::Colon
        } else {
            GrantTextLiteral::Space
        };
        if let Some(raw) = high {
            assert!(matches!(error,
                PlayerPrepareError::Construction(PlayerConstructionError::Grant(
                    LiveGrantError::Text(GrantTextError::ClassificationBoundary { literal: actual, high_byte })
                )) if actual == literal && high_byte == raw
            ));
        } else {
            assert!(matches!(error,
                PlayerPrepareError::Construction(PlayerConstructionError::Grant(
                    LiveGrantError::Text(GrantTextError::MissingClassificationByte(actual))
                )) if actual == literal
            ));
        }
        assert_eq!(
            owners.constructions[0].unpublished_grants(),
            Some(&[2, 0, 0, 0, 0, 0])
        );
        assert_eq!(owners.mission, mission);
        assert_eq!(owners.cycle, cycle);
        assert_eq!(classification, original_classification);
        assert!(owners.arena.first_player().is_none());
        assert!(owners.owner);
        owners.release();
    }
}

#[test]
fn absent_current_classification_binding_is_not_replaced_by_startup_or_literal_defaults() {
    let sources = Sources::boot();
    let classification = ProfileNameClassification { high_bytes: None };
    let text = current_text(&classification);
    assert!(text.pair_classification.is_none());
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = None;
    let context = grants(&sources, Some(&process), Some(&text));
    let mut descriptors = sources.descriptors();
    let mut owners = Owners::new();
    let mission = owners.mission.clone();
    let cycle = owners.cycle.clone();
    assert!(matches!(
        owners.run(Attempt {
            callback_context: 0x8480,
            attract: false,
            descriptors: &mut descriptors,
            grants: &context,
            selections: sources.assets.attract_selections(),
        }),
        Err(PlayerPrepareError::Construction(
            PlayerConstructionError::Grant(LiveGrantError::Text(
                GrantTextError::MissingClassificationBinding(GrantTextLiteral::Colon)
            ))
        ))
    ));
    assert_eq!(
        owners.constructions[0].unpublished_grants(),
        Some(&[2, 0, 0, 0, 0, 0])
    );
    assert_eq!(owners.cycle, cycle);
    assert_eq!(owners.mission, mission);
    assert_eq!(owners.phase, PlayerPreparePhase::Constructing);
    assert!(owners.owner);
    assert!(classification.high_bytes.is_none());
    owners.release();
}

#[test]
fn complete_offline_prepare_resets_actual_mission_and_keeps_first_player_authoritative() {
    let sources = Sources::boot();
    let process = ParticleFactoryProcessState::native_startup();
    let before_process = process;
    let text = current_text(&sources.classification);
    let before_classification = sources.classification.clone();
    let context = grants(&sources, Some(&process), Some(&text));
    let mut descriptors = sources.descriptors();
    let mut owners = Owners::new();
    let first = owners.arena.add_world_player([0x12345678; 6]);
    let mut saved = PlayerSaveState::new_player([0x12345678; 6]);
    saved.owner_118 = 0x89abcdef;
    saved.owner_13c = [0xff008041, 0x0100fe00];
    saved.owner_130 = [0x80000000, 0x13579bdf, 0xffffffff];
    owners
        .arena
        .restore_player_save_prefix(first, &saved)
        .unwrap();
    let first_before = owners.arena.world_player(first).unwrap().clone();
    let message = owners.arena.player_message(first).unwrap().identity();
    let sentinel = owners.arena.object(owners.sentinel).unwrap().clone();
    let cycle = owners.cycle.clone();
    let selected = owners.input.selected_context();
    let outcome = owners
        .run(Attempt {
            callback_context: 0x8500,
            attract: false,
            descriptors: &mut descriptors,
            grants: &context,
            selections: sources.assets.attract_selections(),
        })
        .unwrap();
    assert_eq!(outcome.selection, 22);
    assert_ne!(outcome.constructed, first);
    assert_eq!(owners.constructions.len(), 1);
    assert_eq!(owners.constructions[0].player(), outcome.constructed);
    assert_eq!(
        owners.constructions[0].phase(),
        PlayerConstructionPhase::Published
    );
    assert_eq!(owners.phase, PlayerPreparePhase::Complete);
    assert!(owners.owner);
    assert_eq!(owners.mission, MissionSaveState::new_game());
    assert_eq!(
        owners.cycle,
        AttractCycleState {
            armed_5d8: 0,
            ..cycle
        }
    );
    assert_eq!(owners.arena.first_player(), Some(first));
    assert_eq!(owners.arena.world_player(first).unwrap(), &first_before);
    assert_eq!(
        owners.arena.player_message(first).unwrap().identity(),
        message
    );
    // Export is explicitly a caller phase and selects the real first player,
    // rather than the newly constructed one or a prepared closure/default.
    assert_eq!(
        owners
            .arena
            .export_player_save_state(owners.arena.first_player().unwrap())
            .unwrap(),
        saved
    );
    let actual = owners.arena.world_player(outcome.constructed).unwrap();
    assert_eq!(
        actual.grants_9c,
        grant_closure(
            &parse_grant_table(sources.executable()).unwrap(),
            &inf_formats::PLAYER_INITIAL_GRANTS,
        )
        .unwrap()
    );
    assert_eq!(actual.handle_d4, ActorHandle::from_raw(0));
    assert_eq!(actual.dword_50, Some(0));
    assert_eq!(
        owners
            .arena
            .player_message(outcome.constructed)
            .unwrap()
            .suppression_byte(),
        1
    );
    assert_eq!(owners.arena.object(owners.sentinel).unwrap(), &sentinel);
    assert_eq!(descriptors.requests.len(), 11);
    assert_eq!(process, before_process);
    assert_eq!(sources.classification, before_classification);
    assert!(text.null_substitute.is_none()); // Genuine Boot sources need no545900.
    assert_eq!(owners.input.selected_context(), selected);
    assert_eq!(owners.input.clock_micros(), 0x123456789);
    assert_eq!(owners.input.queued_len(), 0);
    owners.release();
    assert_eq!(owners.arena.first_player(), Some(first));
}

#[test]
fn full_nonzero_network_is_reached_after_publication_and_armed_clear() {
    let sources = Sources::boot();
    for network in [1, 0x100, 0x80000000] {
        let mut process = ParticleFactoryProcessState::native_startup();
        process.network_flag = network;
        let before_process = process;
        // No residency/CRT is consumed by the constructor's network arm.
        let context = LiveGrantContext {
            process: Some(&process),
            executable: Some(sources.executable()),
            resident: None,
            text_data: None,
        };
        let mut descriptors = sources.descriptors();
        let mut owners = Owners::new();
        let mission = owners.mission.clone();
        let cycle = owners.cycle.clone();
        assert!(matches!(
            owners.run(Attempt {
                callback_context: 0x8600,
                attract: false,
                descriptors: &mut descriptors,
                grants: &context,
                selections: sources.assets.attract_selections(),
            }),
            Err(PlayerPrepareError::NetworkMissionSelection { network: actual }) if actual == network
        ));
        assert_eq!(owners.phase, PlayerPreparePhase::SelectingMission);
        assert_eq!(
            owners.constructions[0].phase(),
            PlayerConstructionPhase::Published
        );
        assert_eq!(
            owners.arena.first_player(),
            Some(owners.constructions[0].player())
        );
        assert_eq!(
            owners.cycle,
            AttractCycleState {
                armed_5d8: 0,
                ..cycle
            }
        );
        assert_eq!(owners.mission, mission);
        assert_eq!(descriptors.requests.len(), 11);
        assert_eq!(process, before_process);
        assert!(owners.owner);
        owners.release();
    }
}

#[test]
fn attract_selection_bypasses_network_branch_after_construction_and_wraps_real_index() {
    let sources = Sources::boot();
    let table = sources.assets.attract_selections();
    for (index, next) in [(0, 1), (10, 0), (u32::MAX, 0), (11, 0)] {
        let mut process = ParticleFactoryProcessState::native_startup();
        process.network_flag = 0x100;
        let context = LiveGrantContext {
            process: Some(&process),
            executable: Some(sources.executable()),
            resident: None,
            text_data: None,
        };
        let mut descriptors = sources.descriptors();
        let mut owners = Owners::new();
        owners.cycle.selection_index_04 = index;
        let cycle = owners.cycle.clone();
        let outcome = owners
            .run(Attempt {
                callback_context: 0x8700,
                attract: true,
                descriptors: &mut descriptors,
                grants: &context,
                selections: table,
            })
            .unwrap();
        assert_eq!(outcome.selection, table.values[next]);
        assert_eq!(
            owners.cycle,
            AttractCycleState {
                armed_5d8: 0,
                selection_index_04: next as u32,
                ..cycle
            }
        );
        assert_eq!(
            owners.mission,
            MissionSaveState {
                mission_selection: table.values[next],
                ..MissionSaveState::new_game()
            }
        );
        assert_eq!(owners.arena.first_player(), Some(outcome.constructed));
        assert_eq!(owners.phase, PlayerPreparePhase::Complete);
        assert_eq!(
            owners.constructions[0].phase(),
            PlayerConstructionPhase::Published
        );
        assert_eq!(process.network_flag, 0x100);
        owners.release();
    }
}

#[test]
fn repeated_prepares_keep_fifo_carriers_and_distinct_actual_input_lifetimes() {
    let sources = Sources::boot();
    let process = ParticleFactoryProcessState::native_startup();
    let text = current_text(&sources.classification);
    let context = grants(&sources, Some(&process), Some(&text));
    let mut descriptors = sources.descriptors();
    let mut owners = Owners::new();
    let mut players = Vec::new();
    for callback_context in [0x8800, 0x8900] {
        players.push(
            owners
                .run(Attempt {
                    callback_context,
                    attract: false,
                    descriptors: &mut descriptors,
                    grants: &context,
                    selections: sources.assets.attract_selections(),
                })
                .unwrap()
                .constructed,
        );
    }
    assert_eq!(owners.constructions.len(), 2);
    assert_eq!(owners.constructions[0].callback_context(), 0x8800);
    assert_eq!(owners.constructions[1].callback_context(), 0x8900);
    assert_eq!(
        owners
            .arena
            .world_players()
            .iter()
            .map(|player| player.id)
            .collect::<Vec<_>>(),
        players
    );
    let requested = descriptors.requests[0];
    let PlayerInputDescriptor::Analog(value) = descriptors.source.descriptor(requested).unwrap()
    else {
        panic!("actual analog source");
    };
    let first = owners.input.acquire_analog_consumer(value, 0x8800).unwrap();
    let second = owners.input.acquire_analog_consumer(value, 0x8900).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        owners
            .input
            .analog_consumer_state(first)
            .unwrap()
            .references,
        2
    );
    assert_eq!(
        owners
            .input
            .analog_consumer_state(second)
            .unwrap()
            .references,
        2
    );
    owners.input.release_analog_consumer(first).unwrap();
    owners.input.release_analog_consumer(second).unwrap();
    owners.constructions[0]
        .release_input_references(&mut owners.input)
        .unwrap();
    assert!(owners.input.analog_consumer_state(first).is_err());
    assert_eq!(
        owners
            .input
            .analog_consumer_state(second)
            .unwrap()
            .references,
        1
    );
    owners.constructions[1]
        .release_input_references(&mut owners.input)
        .unwrap();
    assert!(owners.input.analog_consumer_state(second).is_err());
    assert_eq!(owners.arena.first_player(), Some(players[0]));
    assert_eq!(owners.input.clock_micros(), 0x123456789);
    assert_eq!(owners.input.queued_len(), 0);
}
