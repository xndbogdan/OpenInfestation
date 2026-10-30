use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectDestructionRequest, SoundTeardownService,
};
use crate::process_sound::{
    ProcessSoundAudio, ProcessSoundListener, ProcessSoundRecordState, ProcessSoundServiceError,
    RequiredProcessSoundVoiceTeardown,
};
use crate::{Installation, ResidencyRequest};
fn assets() -> &'static crate::frontend_assets::FrontendAssets {
    static ASSETS: std::sync::OnceLock<crate::frontend_assets::FrontendAssets> =
        std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        crate::frontend_assets::FrontendAssets::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap()
    })
}
fn sounds() -> ProcessSoundRuntime {
    ProcessSoundRuntime::new(
        inf_formats::FrontendExecutable::parse(assets().executable_bytes()).unwrap(),
    )
    .unwrap()
}
fn resident() -> ResidentRuntime {
    let mut resident = ResidentRuntime::new(
        Installation::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
            .unwrap(),
    )
    .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
}
fn add_actor(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let mut constructor_rng = 0;
    arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
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
                        sine_quarter: &assets().sine_table().values,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut constructor_rng,
        )
        .unwrap()
}
fn fixture() -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = add_actor(&mut arena, [384, 64, 640]);
    let listener = add_actor(&mut arena, [384, 64, 640]);
    (arena, actor, listener)
}
//Supplied callback results exercise the intrinsic481B10 phase. Both IDs are
//genuine native constructor allocations; this is not a production factory acceptance test.
struct Camera {
    result: Result<Option<ObjectId>, &'static str>,
    calls: usize,
    install: Option<(ObjectId, [u16; 3])>,
}
impl ActorAudioCameraHost for Camera {
    type Error = &'static str;
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        self.calls += 1;
        if let Some((actor, xyz)) = self.install {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(xyz);
        }
        self.result
    }
}
fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    camera: &mut Camera,
    resident: &ResidentRuntime,
    sounds: &mut ProcessSoundRuntime,
    controls: [i32; 2],
) -> Result<(), Selector36PersistentAudioError<&'static str>> {
    arena.tick_selector36_persistent_audio(
        actor,
        Selector36PersistentAudioRequest { controls },
        camera,
        resident,
        sounds,
        &mut RequiredProcessSoundVoiceTeardown,
    )
}
fn camera(listener: ObjectId) -> Camera {
    Camera {
        result: Ok(Some(listener)),
        calls: 0,
        install: None,
    }
}
#[test]
fn genuine_null_camera_and_camera_error_precede_unknown_xyz_and_sound_loan() {
    let resident = resident();
    let (mut arena, actor, _) = fixture();
    let mut sounds = sounds();
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut host = Camera {
        result: Ok(None),
        calls: 0,
        install: None,
    };
    visit(
        &mut arena,
        actor,
        &mut host,
        &resident,
        &mut sounds,
        [0, i32::MAX],
    )
    .unwrap();
    assert_eq!(host.calls, 1);
    assert!(sounds.is_empty());
    assert_eq!(arena.selector36_actor(actor).unwrap().word_280, 0);
    host.result = Err("478E00");
    assert!(matches!(
        visit(&mut arena, actor, &mut host, &resident, &mut sounds, [0, 0]),
        Err(Selector36PersistentAudioError::Camera("478E00"))
    ));
    assert_eq!(arena.selector36_actor(actor).unwrap().word_284, 0);
}
#[test]
fn camera_mutation_is_retained_and_fresh_pose_used_for_birth() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    let mut host = camera(listener);
    host.install = Some((actor, [390, 66, 645]));
    visit(&mut arena, actor, &mut host, &resident, &mut sounds, [0, 0]).unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    let row = sounds.records().snapshot(reference).unwrap();
    assert_eq!(row.position, Some([390, 66, 645]));
    assert_eq!(
        (row.volume, row.pitch, row.flags, row.voice),
        (Some(8192), Some(8192), Some(1), Some(None))
    );
    assert_eq!(row.retained_bytes, 40);
    assert_eq!(sounds.pending_len(), 1);
    assert_eq!(host.calls, 1);
}
#[test]
fn unsigned_squared_range_includes_equality_and_far_null_bypasses_smoothing() {
    let resident = resident();
    for (distance, born) in [(7680u16, true), (7681, false), (32768, false)] {
        let (mut arena, actor, listener) = fixture();
        let mut sounds = sounds();
        arena.owned_mut(listener).unwrap().state.base.position_words =
            Some([384u16.wrapping_add(distance), 64, 640]);
        visit(
            &mut arena,
            actor,
            &mut camera(listener),
            &resident,
            &mut sounds,
            [0, 0],
        )
        .unwrap();
        assert_eq!(
            arena.derived_actor(actor).unwrap().sound_168.is_some(),
            born
        );
        assert_eq!(
            arena.selector36_actor(actor).unwrap().word_280,
            if born { 8192 } else { 0 }
        );
    }
}
#[test]
fn signed_control_abs_clamp_and_arithmetic_smoothing_preserve_native_wrap() {
    let resident = resident();
    for (control, expected) in [(0, 8192), (-1024, 10240), (2048, 10240), (i32::MIN, 8192)] {
        let (mut arena, actor, listener) = fixture();
        let mut sounds = sounds();
        visit(
            &mut arena,
            actor,
            &mut camera(listener),
            &resident,
            &mut sounds,
            [i32::MAX, control],
        )
        .unwrap();
        let state = arena.selector36_actor(actor).unwrap();
        assert_eq!((state.word_280, state.word_284), (expected, expected));
    }
    assert_eq!(smooth(u32::MAX, 32768), 8191); //Native SAR2 with old signed -1.
    assert_eq!(smooth(32772, 32768), 32771);
}
#[test]
fn allocation_failure_preserves_smoothing_prefix_and_unpublished168() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    sounds.fail_next_record_allocation(63);
    assert!(matches!(
        visit(
            &mut arena,
            actor,
            &mut camera(listener),
            &resident,
            &mut sounds,
            [0, 1024]
        ),
        Err(Selector36PersistentAudioError::Sound(
            ProcessSoundRecordError::FixtureAllocation(63)
        ))
    ));
    let state = arena.selector36_actor(actor).unwrap();
    assert_eq!((state.word_280, state.word_284), (10240, 10240));
    assert_eq!(arena.derived_actor(actor).unwrap().sound_168, None);
    assert!(sounds.is_empty());
}
#[test]
fn existing_record_updates_same_identity_xyz_and_scalars_without_new_allocation() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    //An actual following birth would consume this script. Existing487FC0 must not allocate.
    sounds.fail_next_record_allocation(99);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([390, 65, 642]);
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    assert_eq!(
        arena.derived_actor(actor).unwrap().sound_168,
        Some(reference)
    );
    let row = sounds.records().snapshot(reference).unwrap();
    assert_eq!(row.position, Some([390, 65, 642]));
    assert_eq!((row.volume, row.pitch), (Some(14336), Some(14336)));
    assert_eq!(sounds.pending_len(), 1);
}
#[test]
fn far_deletion_releases_same_process_record_before_normal168_null_store() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    arena.owned_mut(listener).unwrap().state.base.position_words = Some([8065, 64, 640]);
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().sound_168, None);
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().state,
        ProcessSoundRecordState::Released
    );
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().retained_bytes,
        0
    );
    assert_eq!(arena.selector36_actor(actor).unwrap().word_280, 8192);
}
#[test]
fn foreign_record_error_retains_smoothed_prefix_and_actor_reference() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut original = sounds();
    let mut other = sounds();
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut original,
        [0, 0],
    )
    .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    assert!(
        matches!(visit(&mut arena, actor, &mut camera(listener), &resident, &mut other, [0,0]),
        Err(Selector36PersistentAudioError::Sound(ProcessSoundRecordError::Foreign(r))) if r==reference)
    );
    assert_eq!(
        arena.derived_actor(actor).unwrap().sound_168,
        Some(reference)
    );
    assert_eq!(arena.selector36_actor(actor).unwrap().word_280, 14336);
    assert_eq!(
        original.records().snapshot(reference).unwrap().volume,
        Some(8192)
    );
}
#[test]
fn destructor_missing_loan_is_lazy_after_real140_prefix_and_before_handle_release() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    //Supplied already-exited descriptor domain permits the native425F70 suffix.
    arena.family428650_actor_mut(actor).unwrap().word_184 = -1;
    arena.derived_actor_mut(actor).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(6, 0, 0);
    let handle = arena.actor(actor).unwrap().handle;
    assert_eq!(
        arena.destroy(ObjectDestructionRequest {
            object: actor,
            sound: SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::Sound(
            ProcessSoundRecordError::RequiredTeardown(reference)
        ))
    );
    assert!(arena.derived_actor(actor).unwrap().behavior_140.is_none());
    assert_eq!(
        arena.derived_actor(actor).unwrap().sound_168,
        Some(reference)
    );
    assert_eq!(arena.actor(actor).unwrap().handle, handle);
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().state,
        ProcessSoundRecordState::Linked
    );
    let retired = arena
        .destroy(ObjectDestructionRequest {
            object: actor,
            sound: SoundTeardownService::Available {
                sounds: &mut sounds,
                voices: &mut RequiredProcessSoundVoiceTeardown,
            },
        })
        .unwrap();
    assert!(matches!(
        retired.class,
        crate::object_runtime::ObjectClass::Actor(_)
    ));
    assert!(arena.object(actor).is_err());
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().state,
        ProcessSoundRecordState::Released
    );
}
#[test]
fn constructor_null168_requires_no_teardown_loan() {
    let (mut arena, actor, _) = fixture();
    arena.family428650_actor_mut(actor).unwrap().word_184 = -1;
    assert_eq!(arena.derived_actor(actor).unwrap().sound_168, None);
    arena
        .destroy(ObjectDestructionRequest {
            object: actor,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.object(actor).is_err());
}
struct Audio {
    calls: Vec<i16>,
}
impl crate::process_sound::ProcessSoundVoiceTeardown for Audio {
    fn delete_voice(
        &mut self,
        record: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        crate::process_sound::ProcessSoundVoiceTeardown::delete_voice(
            &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            record,
            voice,
        )
    }
}
impl ProcessSoundAudio for Audio {
    type Error = &'static str;
    fn operation(
        &mut self,
        request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
    ) -> Result<
        crate::process_sound::ProcessSoundVoiceReply,
        crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
    > {
        use crate::process_sound::{
            ProcessSoundVoiceOperation as Operation, ProcessSoundVoiceReply as Reply,
            ProcessSoundVoiceRequest as Request, ProcessSoundVoiceServiceFault as Fault,
        };
        let Request::OneShot {
            resource,
            parameters: _,
            effects_volume: _,
            rng: _,
        } = request
        else {
            return Err(Fault::Required(match request {
                Request::Create { .. } => Operation::Create,
                Request::Start { .. } => Operation::Start,
                Request::Update { .. } => Operation::Update,
                Request::OneShot { .. } => unreachable!(),
            }));
        };
        (|| -> Result<bool, Self::Error> {
            self.calls.push(resource.requested_slot());
            Ok(true)
        })()
        .map(Reply::OneShot)
        .map_err(Fault::Backend)
    }
}
#[test]
fn persistent_service_is_reached_required_after_earlier_one_shot_and_preserves_record() {
    let resident = resident();
    let (mut arena, actor, listener) = fixture();
    let mut sounds = sounds();
    let shot = crate::process_particles::audio::ParticleSoundRequest {
        sound: 6,
        position: [384, 64, 640],
        volume: 65536,
        pitch: 65536,
        flags: 0,
    };
    sounds.enqueue(&resident, shot).unwrap();
    visit(
        &mut arena,
        actor,
        &mut camera(listener),
        &resident,
        &mut sounds,
        [0, 0],
    )
    .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    sounds
        .enqueue(
            &resident,
            crate::process_particles::audio::ParticleSoundRequest { sound: 9, ..shot },
        )
        .unwrap();
    let mut audio = Audio { calls: Vec::new() };
    let mut rng = 0x1234abcd;
    let result = sounds.service_world_draw(
        ProcessSoundListener {
            view: crate::frontend_scene::SceneView {
                eye: [384, 64, 640],
                basis: crate::menu::MenuBasis::INITIAL,
            },
            phase: 4,
            suppressed: false,
            water: None,
        },
        &mut audio,
        15,
        &mut rng,
    );
    assert_eq!(
        result,
        Err(ProcessSoundServiceError::Record(
            ProcessSoundRecordError::RequiredVoice {
                record: reference,
                operation: crate::process_sound::ProcessSoundVoiceOperation::Create
            }
        ))
    );
    assert_eq!(audio.calls, [6]);
    assert_eq!(sounds.started_in_last_service(), 1);
    assert_eq!(rng, 0x1234abcd);
    assert_eq!(sounds.pending_len(), 1);
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().state,
        ProcessSoundRecordState::Linked
    );
}
