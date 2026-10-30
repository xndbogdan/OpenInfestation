use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorConstructionRequest, ActorExtension, ActorHandle, ActorHandles, ActorRegistryConfig,
        ActorServiceResourceKind, ActorTerrainInputs, CreationListener, CreationListenerEffects,
        CreationListenerError, CreationListeners, NativeTargetClass, Selector93FactoryRequest,
    },
    process_particles::{audio::ParticleSoundRequest, factories::ParticleFactoryProcessState},
    process_sound::ProcessSoundRuntime,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    word_resources::H1WordError,
    world_actor_terrain::{WorldActorTerrainHost, WorldTerrainSource},
    world_mission::{WorldMissionError, WorldMissionInputs},
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{native_basis::NativeBasisMath, FrontendExecutable, MissionSaveState};
use std::path::Path;

const SINE: [u16; 4096] = [0; 4096];
const SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};

struct Sources {
    assets: FrontendAssets,
    runtime: ResidentRuntime,
    math: NativeBasisMath,
}

impl Sources {
    fn retail(with_world: bool) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let math = NativeBasisMath::new(assets.sine_table().values);
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        runtime
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        if with_world {
            // Sequential canonical activation: leaf1/48 alone owns no H1/H12.
            runtime
                .apply(ResidencyRequest::Activate { level: 28 })
                .unwrap();
        }
        Self {
            assets,
            runtime,
            math,
        }
    }

    fn sound_queue(&self) -> ProcessSoundRuntime {
        let mut sound = ProcessSoundRuntime::new(
            FrontendExecutable::parse(self.assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        for sound_id in [0, 0x8000] {
            sound
                .enqueue(
                    &self.runtime,
                    ParticleSoundRequest {
                        sound: sound_id,
                        position: [0x8001, 0xfffe, 3],
                        volume: 256,
                        pitch: 257,
                        flags: 2,
                    },
                )
                .unwrap();
        }
        sound
    }
}

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0x8001, 0xfffe, 0x7fff]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0x8765_4321,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

/// No successful substitute for a reached listener interface.
struct RequiredEffects;
impl CreationListenerEffects for RequiredEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

struct RequiredTerrain {
    calls: usize,
}
impl ActorTerrainHost for RequiredTerrain {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        Err("terrain")
    }
}

struct CleanupFailure {
    calls: usize,
}
impl WorldCompletionHost for CleanupFailure {
    type Error = &'static str;
    fn flush_link_records(&mut self) -> Result<(), Self::Error> {
        self.calls += 1;
        Err("cleanup")
    }
}

#[derive(Clone, Copy)]
struct Call {
    attract: bool,
    camera: Option<ObjectId>,
    tick: SchedulerTick,
}
impl Call {
    fn active(camera: ObjectId, delta: i32, elapsed: i32) -> Self {
        Self {
            attract: true,
            camera: Some(camera),
            tick: SchedulerTick {
                delta_micros: delta,
                elapsed_micros: elapsed,
            },
        }
    }
}

struct Owners {
    arena: ObjectArena,
    keeper: ObjectId,
    camera: ObjectId,
    state: AttractCycleState,
    process: ParticleFactoryProcessState,
    mission: WorldMission,
    session: SessionState,
    listeners: CreationListeners,
    effects: RequiredEffects,
}
impl Owners {
    fn new() -> Self {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 17,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let keeper = arena.allocate_actor(allocation()).unwrap();
        let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
        let mut state = AttractCycleState::from_native_constructor_fields();
        state.publication.handle_5dc = Some(arena.actor(keeper).unwrap().handle);
        Self {
            arena,
            keeper,
            camera,
            state,
            process: ParticleFactoryProcessState::native_startup(),
            mission: WorldMission::new(
                MissionSaveState::new_game(),
                WorldMissionInputs {
                    current_world_a0: 22,
                    dword_1c: 0x1234_5678,
                    byte_13: None,
                    record_0c: None,
                },
            ),
            session: SessionState::default(),
            listeners: CreationListeners::new(),
            effects: RequiredEffects,
        }
    }

    fn run<T: ActorTerrainHost, C: WorldCompletionHost>(
        &mut self,
        source: &Sources,
        call: Call,
        terrain: &mut T,
        completion: &mut C,
    ) -> Result<AttractCycleOutcome, AttractCycleError<T::Error, C::Error>> {
        tick_attract_cycle(
            &mut self.arena,
            AttractCycleRequest {
                state: &mut self.state,
                attract: call.attract,
                captured_camera: call.camera,
                tick: call.tick,
                selections: source.assets.attract_selections(),
                factory: Selector93FactoryRequest {
                    allocation: allocation(),
                    process: &self.process,
                    local_owner: 1,
                    runtime: &source.runtime,
                    math: &source.math,
                    listeners: &mut self.listeners,
                    listener_effects: &mut self.effects,
                    network_world_active: false,
                },
                settings: SETTINGS,
                mission: &mut self.mission,
                session: &mut self.session,
            },
            AttractCycleServices {
                terrain,
                completion,
            },
        )
    }

    fn surrounding(&self) -> String {
        format!(
            "{:?}",
            (&self.mission, &self.session, &self.process, &self.listeners)
        )
    }

    fn expected_resolution(&self) -> ActorHandles {
        let mut expected = self.arena.actor_handles().unwrap().clone();
        let _ = expected.resolve(self.state.publication.handle_5dc.unwrap());
        expected
    }
}

/// Compare the entire registry separately before masking it from the full
/// arena Debug snapshot. No actual owner is changed or prewarmed by this helper.
fn arena_except_registry(arena: &ObjectArena) -> String {
    format!("{arena:?}").replacen(
        &format!("actor_handles: {:?}", arena.actor_handles()),
        "actor_handles: <independently compared>",
        1,
    )
}

fn assert_registry(arena: &ObjectArena, expected: &ActorHandles) {
    assert_eq!(
        format!("{:?}", arena.actor_handles().unwrap()),
        format!("{expected:?}")
    );
}

fn unused() -> RequiredTerrain {
    RequiredTerrain { calls: 0 }
}

fn allocation_classes(arena: &ObjectArena) -> Vec<NativeTargetClass> {
    arena
        .actor_service_heap()
        .attempts()
        .iter()
        .map(|kind| match kind {
            ActorServiceResourceKind::WeaponFireNode => {
                panic!("unexpected firing allocation in target fixture")
            }
            ActorServiceResourceKind::Target(class) => *class,
            ActorServiceResourceKind::FamilyRadiusScoreInput => {
                panic!("unexpected radius input allocation")
            }
        })
        .collect()
}

#[test]
fn constructor_fields_and_both_outer_bypasses_do_not_invent_unknown_backing() {
    let source = Sources::retail(false);
    for attract in [false, true] {
        let mut owner = Owners::new();
        owner.state = AttractCycleState::from_native_constructor_fields();
        assert_eq!(owner.state.selection_index_04, 0);
        assert_eq!(owner.state.armed_5d8, 0);
        assert_eq!(owner.state.timer_micros_5d4, None);
        assert_eq!(owner.state.publication.handle_5dc, None);
        assert_eq!(owner.state.publication.object_5e0, None);
        // A dead captured camera is harmless on attract=false; NULL is the
        // second guard. Neither reads process, publication, timer or sources.
        owner
            .arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: owner.camera,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        owner.process.process_null_handle = None;
        let before = (
            owner.state.clone(),
            owner.surrounding(),
            format!("{:?}", owner.arena),
        );
        let mut terrain = unused();
        let mut sound = source.sound_queue();
        let sound_before = format!("{sound:?}");
        assert_eq!(
            owner
                .run(
                    &source,
                    Call {
                        attract,
                        camera: if attract { None } else { Some(owner.camera) },
                        tick: SchedulerTick {
                            delta_micros: -1,
                            elapsed_micros: 999_999
                        },
                    },
                    &mut terrain,
                    &mut sound
                )
                .unwrap(),
            AttractCycleOutcome::Skipped
        );
        assert_eq!(
            (
                owner.state.clone(),
                owner.surrounding(),
                format!("{:?}", owner.arena)
            ),
            before
        );
        assert_eq!(format!("{sound:?}"), sound_before);
        assert_eq!(terrain.calls, 0);
        assert!(allocation_classes(&owner.arena).is_empty());
    }
}

#[test]
fn first_arm_commits_before_unknown_handle_and_unknown_timer_is_late() {
    let source = Sources::retail(false);
    let mut owner = Owners::new();
    owner.state = AttractCycleState::from_native_constructor_fields();
    let before = (owner.surrounding(), format!("{:?}", owner.arena));
    let mut terrain = unused();
    let mut sound = source.sound_queue();
    let call = Call::active(owner.camera, 125_000, 250_000);
    assert!(matches!(
        owner.run(&source, call, &mut terrain, &mut sound),
        Err(AttractCycleError::HandleUnavailable)
    ));
    assert_eq!(owner.state.armed_5d8, 1);
    assert_eq!(owner.state.timer_micros_5d4, Some(0));
    assert_eq!(owner.state.publication.handle_5dc, None);
    assert_eq!((owner.surrounding(), format!("{:?}", owner.arena)), before);
    assert_eq!(sound.pending_len(), 2);

    owner.state.publication.handle_5dc = Some(owner.arena.actor(owner.keeper).unwrap().handle);
    owner.state.timer_micros_5d4 = None;
    owner.state.armed_5d8 = 0xff;
    let expected = owner.expected_resolution();
    let before_arena = arena_except_registry(&owner.arena);
    let state = owner.state.clone();
    assert!(matches!(
        owner.run(&source, call, &mut terrain, &mut sound),
        Err(AttractCycleError::TimerUnavailable)
    ));
    assert_eq!(owner.state, state);
    assert_registry(&owner.arena, &expected);
    assert_eq!(arena_except_registry(&owner.arena), before_arena);
    assert_eq!(owner.surrounding(), before.0);
    assert_eq!(terrain.calls, 0);
    assert!(allocation_classes(&owner.arena).is_empty());
}

#[test]
fn live_handle_resolves_without_camera_or_setup_or_process_null_reads() {
    let source = Sources::retail(false);
    assert!(source.runtime.resource(1, 0).is_none());
    let mut owner = Owners::new();
    owner
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner.camera,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    owner.process.process_null_handle = None;
    let expected = owner.expected_resolution();
    let before = (owner.surrounding(), arena_except_registry(&owner.arena));
    let mut terrain = unused();
    let mut sound = source.sound_queue();
    let sound_before = format!("{sound:?}");
    assert_eq!(
        owner
            .run(
                &source,
                Call::active(owner.camera, 125_000, 250_000),
                &mut terrain,
                &mut sound
            )
            .unwrap(),
        AttractCycleOutcome::Waiting
    );
    assert_eq!(owner.state.timer_micros_5d4, Some(125_000));
    assert_eq!(owner.state.armed_5d8, 1);
    assert_eq!(owner.state.publication.object_5e0, None);
    assert_registry(&owner.arena, &expected);
    assert_eq!(
        (owner.surrounding(), arena_except_registry(&owner.arena)),
        before
    );
    assert_eq!(format!("{sound:?}"), sound_before);
    assert_eq!(terrain.calls, 0);
    assert!(allocation_classes(&owner.arena).is_empty());
}

#[test]
fn stale_resolver_and_required_h1_stop_before_construction_or_timer() {
    let source = Sources::retail(false);
    let mut owner = Owners::new();
    owner
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner.keeper,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let expected = owner.expected_resolution();
    let before = (owner.surrounding(), arena_except_registry(&owner.arena));
    let publication = owner.state.publication.clone();
    let mut terrain = unused();
    let mut sound = source.sound_queue();
    assert!(matches!(
        owner.run(
            &source,
            Call::active(owner.camera, 125_000, 250_000),
            &mut terrain,
            &mut sound
        ),
        Err(AttractCycleError::Setup(AttractActorSetupError::Position(
            H1WordError::NotResident { slot: 0 }
        )))
    ));
    assert_eq!(owner.state.publication, publication);
    assert_eq!(owner.state.timer_micros_5d4, Some(0));
    assert_eq!(owner.state.armed_5d8, 1);
    assert_registry(&owner.arena, &expected);
    assert_eq!(
        (owner.surrounding(), arena_except_registry(&owner.arena)),
        before
    );
    assert_eq!(sound.pending_len(), 2);
    assert_eq!(terrain.calls, 0);
    assert!(allocation_classes(&owner.arena).is_empty());
}

#[test]
fn late_setup_terrain_error_retains_published_actor_and_unticked_arm() {
    let source = Sources::retail(true);
    let mut owner = Owners::new();
    owner
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner.keeper,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let camera = owner.arena.object(owner.camera).unwrap().clone();
    let before_mission = owner.mission.clone();
    let before_session = owner.session.clone();
    let process = owner.process.clone();
    let mut terrain = unused();
    let mut sound = source.sound_queue();
    assert!(matches!(
        owner.run(
            &source,
            Call::active(owner.camera, 125_000, 250_000),
            &mut terrain,
            &mut sound
        ),
        Err(AttractCycleError::Setup(AttractActorSetupError::Terrain(
            "terrain"
        )))
    ));
    let published = owner.state.publication.object_5e0.unwrap();
    assert!(matches!(
        owner.arena.actor(published).unwrap().extension,
        ActorExtension::Selector93(_)
    ));
    assert_eq!(
        owner.state.publication.handle_5dc,
        Some(owner.arena.actor(published).unwrap().handle)
    );
    assert_eq!(
        owner.arena.actor_handles().unwrap().cache().object,
        Some(published)
    );
    assert_eq!(
        owner.arena.object(published).unwrap().base.position_words,
        Some([0x8001, 0xfffe, 0x7fff])
    );
    assert_eq!(owner.arena.object(owner.camera).unwrap(), &camera);
    assert_eq!(owner.state.timer_micros_5d4, Some(0));
    assert_eq!(owner.state.armed_5d8, 1);
    assert_eq!(owner.mission, before_mission);
    assert_eq!(owner.session, before_session);
    assert_eq!(owner.process, process);
    assert_eq!(sound.pending_len(), 2);
    assert_eq!(terrain.calls, 1);
    assert!(allocation_classes(&owner.arena).is_empty());
}

#[test]
fn actual_current_banks_setup_sound_cleanup_and_real_player_complete_the_cycle() {
    let source = Sources::retail(true);
    let h1 = source.runtime.resource(1, 0).unwrap();
    let HandlerResources::Words(words) = &h1.overlay.handlers()[h1.registration.region_index]
    else {
        panic!("H1")
    };
    let packed = words.entries[usize::from(h1.registration.local_index)].raw;
    let xz = [packed as u16, (packed >> 16) as u16];
    let h12 = source.runtime.resource(12, 0).unwrap();
    let HandlerResources::Terrain(bank) = &h12.overlay.handlers()[h12.registration.region_index]
    else {
        panic!("H12")
    };
    let data = &bank.entries[usize::from(h12.registration.local_index)];
    let prepared = TerrainRuntime::new(
        data,
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let retained = WorldTerrainSource::retain(&h12);
    let mut terrain = WorldActorTerrainHost {
        residency: &source.runtime,
        source: &retained,
        prepared: &prepared,
    };
    let before_sources = (
        h1.overlay.bytes().to_vec(),
        h12.overlay.bytes().to_vec(),
        source.runtime.revision(),
        format!("{prepared:?}"),
    );
    let mut owner = Owners::new();
    owner
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: owner.keeper,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    owner.state.armed_5d8 = 1;
    owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
    let first = owner.arena.add_world_player([0x1111_2222; 6]);
    let second = owner.arena.add_world_player([0x3333_4444; 6]);
    let first_before = owner.arena.world_player(first).unwrap().clone();
    let second_before = owner.arena.world_player(second).unwrap().clone();
    let process = owner.process.clone();
    let mut expected_mission = owner.mission.clone();
    let next = source.assets.attract_selections().values[1];
    expected_mission.save.owner_3c[22] |= 1;
    expected_mission.save.mission_selection = next;
    expected_mission.save.owner_a4 = 0;
    let mut sound = source.sound_queue();
    assert_eq!(
        owner
            .run(
                &source,
                Call::active(owner.camera, 125_000, 250_000),
                &mut terrain,
                &mut sound
            )
            .unwrap(),
        AttractCycleOutcome::Advanced {
            next_selection: next
        }
    );
    let published = owner.state.publication.object_5e0.unwrap();
    let y = (prepared
        .raw_height_at(xz)
        .max((prepared.plane_base() >> (prepared.plane_shift() & 31)) as i16) as u16)
        .wrapping_add(512);
    assert_eq!(
        owner.arena.object(published).unwrap().base.position_words,
        Some([xz[0], y, xz[1]])
    );
    assert_eq!(
        owner.state.publication.handle_5dc,
        Some(owner.arena.actor(published).unwrap().handle)
    );
    assert_eq!(
        allocation_classes(&owner.arena),
        [NativeTargetClass::ActorFollow]
    );
    assert!(owner
        .arena
        .selector16(owner.camera)
        .unwrap()
        .target_18c
        .is_some());
    assert_eq!(owner.arena.world_player(first).unwrap(), &first_before);
    assert_eq!(owner.arena.world_player(second).unwrap(), &second_before);
    // Constructor-zero checkpoints are the public fixture domain here; the
    // lower owner's separate tests cover nonzero ED/F0-only reset custody.
    assert_eq!(owner.arena.world_player(first).unwrap().byte_ed, 0);
    assert_eq!(owner.arena.world_player(first).unwrap().dword_f0, 0);
    assert_eq!(owner.mission, expected_mission);
    assert_eq!(owner.process, process);
    assert_eq!(owner.state.selection_index_04, 1);
    assert_eq!(owner.state.timer_micros_5d4, Some(0));
    assert_eq!(owner.state.armed_5d8, 0);
    assert!(owner.session.transition_pending());
    assert_eq!(owner.session.requested_phase(), SessionPhase::Phase8);
    assert_eq!(owner.session.last_delta_micros(), 0);
    assert_eq!(sound.pending_len(), 0);
    assert_eq!(sound.started_in_last_service(), 0);
    assert_eq!(owner.listeners.len(), 0);
    assert_eq!(
        (
            h1.overlay.bytes().to_vec(),
            h12.overlay.bytes().to_vec(),
            source.runtime.revision(),
            format!("{prepared:?}")
        ),
        before_sources
    );
}

#[test]
fn timer_uses_capped_delta_and_unsigned_strict_threshold_and_dword_wrap() {
    let source = Sources::retail(false);
    for (initial, delta, elapsed, sum, advances) in [
        (0, 125_000, 250_000, 125_000, false),
        (
            ADVANCE_AFTER_MICROS - 125_000,
            125_000,
            250_000,
            ADVANCE_AFTER_MICROS,
            false,
        ),
        (ADVANCE_AFTER_MICROS, 1, 0, ADVANCE_AFTER_MICROS + 1, true),
        (0xffff_0000, 125_000, 1, 59_464, false),
        (0, -1, 0, u32::MAX, true),
    ] {
        let mut owner = Owners::new();
        owner.state.armed_5d8 = 0x80;
        owner.state.timer_micros_5d4 = Some(initial);
        owner.process.network_flag = 0x100;
        let before = owner.mission.clone();
        let mut terrain = unused();
        let mut sound = source.sound_queue();
        let outcome = owner
            .run(
                &source,
                Call::active(owner.camera, delta, elapsed),
                &mut terrain,
                &mut sound,
            )
            .unwrap();
        if advances {
            assert_eq!(
                outcome,
                AttractCycleOutcome::Advanced {
                    next_selection: source.assets.attract_selections().values[1]
                }
            );
            assert_eq!(owner.state.timer_micros_5d4, Some(0));
            assert_eq!(owner.state.armed_5d8, 0);
            assert_eq!(owner.session.requested_phase(), SessionPhase::Phase8);
        } else {
            assert_eq!(outcome, AttractCycleOutcome::Waiting);
            assert_eq!(owner.state.timer_micros_5d4, Some(sum));
            assert_eq!(owner.state.armed_5d8, 0x80);
            assert!(!owner.session.transition_pending());
        }
        assert_eq!(owner.mission, before);
        assert_eq!(sound.pending_len(), 2);
        assert_eq!(terrain.calls, 0);
        assert!(allocation_classes(&owner.arena).is_empty());
        assert_eq!(owner.session.last_delta_micros(), 0);
    }
}

#[test]
fn every_authored_index_and_invalid_wrapping_indices_use_the_real_cache() {
    let source = Sources::retail(false);
    for index in (0..11).chain([11, 0x8000_0000, u32::MAX]) {
        let mut owner = Owners::new();
        owner.process.network_flag = 1;
        owner.state.armed_5d8 = 1;
        owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
        owner.state.selection_index_04 = index;
        let next_index = match index {
            0..=9 => index + 1,
            _ => 0,
        };
        let expected = source.assets.attract_selections().values[next_index as usize];
        let mut terrain = unused();
        let mut sound = source.sound_queue();
        assert_eq!(
            owner
                .run(
                    &source,
                    Call::active(owner.camera, 1, 0),
                    &mut terrain,
                    &mut sound
                )
                .unwrap(),
            AttractCycleOutcome::Advanced {
                next_selection: expected
            }
        );
        assert_eq!(owner.state.selection_index_04, next_index);
        assert_eq!(sound.pending_len(), 2);
    }
    // Same structurally supported EXE, with one declared current authored word
    // changed in memory. No original file or captured behavior is replaced.
    let mut source = source;
    let mut bytes = source.assets.executable_bytes().to_vec();
    let span = FrontendExecutable::parse(&bytes)
        .unwrap()
        .mapped(inf_formats::FRONTEND_ATTRACT_SELECTION_ADDRESS + 4, 4)
        .unwrap()
        .file_span;
    bytes[span.start..span.end].copy_from_slice(&0x8000_0001_u32.to_le_bytes());
    source.assets = FrontendAssets::from_executable(bytes).unwrap();
    let mut owner = Owners::new();
    owner.process.network_flag = 1;
    owner.state.armed_5d8 = 1;
    owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
    let mut terrain = unused();
    let mut sound = source.sound_queue();
    assert_eq!(
        owner
            .run(
                &source,
                Call::active(owner.camera, 1, 0),
                &mut terrain,
                &mut sound
            )
            .unwrap(),
        AttractCycleOutcome::Advanced {
            next_selection: 0x8000_0001
        }
    );
}

#[test]
fn required_cleanup_error_retains_timer_selector_prefix_and_stops_before_mission() {
    let source = Sources::retail(false);
    let mut owner = Owners::new();
    owner.state.armed_5d8 = 1;
    owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
    let before = owner.surrounding();
    let expected = owner.expected_resolution();
    let before_arena = arena_except_registry(&owner.arena);
    let mut terrain = unused();
    let mut cleanup = CleanupFailure { calls: 0 };
    assert!(matches!(
        owner.run(
            &source,
            Call::active(owner.camera, 1, 0),
            &mut terrain,
            &mut cleanup
        ),
        Err(AttractCycleError::Completion(
            WorldCompletionError::Cleanup("cleanup")
        ))
    ));
    assert_eq!(cleanup.calls, 1);
    assert_eq!(owner.state.selection_index_04, 1);
    assert_eq!(owner.state.timer_micros_5d4, Some(0));
    assert_eq!(owner.state.armed_5d8, 1);
    assert_eq!(owner.surrounding(), before);
    assert_registry(&owner.arena, &expected);
    assert_eq!(arena_except_registry(&owner.arena), before_arena);
}

#[test]
fn real_cleanup_then_missing_player_or_mission_retains_each_reached_prefix() {
    let source = Sources::retail(false);
    for invalid_world in [false, true] {
        let mut owner = Owners::new();
        if invalid_world {
            owner.mission.current_world_a0 = u32::MAX;
        }
        owner.state.armed_5d8 = 1;
        owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
        let mut expected_mission = owner.mission.clone();
        if !invalid_world {
            expected_mission.save.owner_3c[22] |= 1;
            expected_mission.save.mission_selection = source.assets.attract_selections().values[1];
            expected_mission.save.owner_a4 = 0;
        }
        let session = owner.session.clone();
        let expected_registry = owner.expected_resolution();
        let before_arena = arena_except_registry(&owner.arena);
        let mut terrain = unused();
        let mut sound = source.sound_queue();
        let error = owner
            .run(
                &source,
                Call::active(owner.camera, 1, 0),
                &mut terrain,
                &mut sound,
            )
            .unwrap_err();
        if invalid_world {
            assert!(matches!(
                error,
                AttractCycleError::Completion(WorldCompletionError::Mission(
                    WorldMissionError::WorldOutOfRange(u32::MAX)
                ))
            ));
        } else {
            assert!(matches!(
                error,
                AttractCycleError::Completion(WorldCompletionError::MissingPlayer)
            ));
        }
        assert_eq!(owner.mission, expected_mission);
        assert_eq!(owner.session, session);
        assert_eq!(owner.state.timer_micros_5d4, Some(0));
        assert_eq!(owner.state.selection_index_04, 1);
        assert_eq!(owner.state.armed_5d8, 1);
        assert_eq!(sound.pending_len(), 0);
        assert_registry(&owner.arena, &expected_registry);
        assert_eq!(arena_except_registry(&owner.arena), before_arena);
    }
}

#[test]
fn full_network_bits_and_both_phase_requests_preserve_first_pending_policy() {
    let source = Sources::retail(false);
    for network in [0, 0x100, 0x8000_0000, u32::MAX] {
        for pending in [false, true] {
            let mut owner = Owners::new();
            let first = owner.arena.add_world_player([0x1111_2222; 6]);
            let second = owner.arena.add_world_player([0x3333_4444; 6]);
            owner.process.network_flag = network;
            // The live branch and null-actor completion do not read NULL.
            owner.process.process_null_handle = None;
            owner.state.armed_5d8 = 1;
            owner.state.timer_micros_5d4 = Some(ADVANCE_AFTER_MICROS);
            if pending {
                assert!(owner.session.request_phase(SessionPhase::Frontend));
            }
            let first_before = owner.arena.world_player(first).unwrap().clone();
            let second_before = owner.arena.world_player(second).unwrap().clone();
            let mut expected_mission = owner.mission.clone();
            if network == 0 {
                expected_mission.save.owner_3c[22] |= 1;
                expected_mission.save.mission_selection =
                    source.assets.attract_selections().values[1];
                expected_mission.save.owner_a4 = 0;
            }
            let expected_registry = owner.expected_resolution();
            let before_arena = arena_except_registry(&owner.arena);
            let process = owner.process.clone();
            let mut terrain = unused();
            let mut sound = source.sound_queue();
            assert_eq!(
                owner
                    .run(
                        &source,
                        Call::active(owner.camera, 1, 250_000),
                        &mut terrain,
                        &mut sound
                    )
                    .unwrap(),
                AttractCycleOutcome::Advanced {
                    next_selection: source.assets.attract_selections().values[1],
                }
            );
            assert_eq!(owner.mission, expected_mission);
            assert_eq!(sound.pending_len(), if network == 0 { 0 } else { 2 });
            assert_eq!(owner.process, process);
            assert_eq!(owner.arena.world_player(first).unwrap(), &first_before);
            assert_eq!(owner.arena.world_player(second).unwrap(), &second_before);
            assert_registry(&owner.arena, &expected_registry);
            assert_eq!(arena_except_registry(&owner.arena), before_arena);
            assert_eq!(owner.state.armed_5d8, 0);
            assert_eq!(
                owner.session.requested_phase(),
                if pending {
                    SessionPhase::Frontend
                } else {
                    SessionPhase::Phase8
                }
            );
            assert!(owner.session.transition_pending());
            assert!(!owner.session.request_phase(SessionPhase::Gameplay));
            assert_eq!(owner.session.current_phase(), SessionPhase::Title);
            assert_eq!(owner.session.last_delta_micros(), 0);
            assert_eq!(terrain.calls, 0);
            assert!(allocation_classes(&owner.arena).is_empty());
        }
    }
}
