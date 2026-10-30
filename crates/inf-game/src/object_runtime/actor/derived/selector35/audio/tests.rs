use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, DerivedActorConstructionRequest,
    },
    process_sound::{ProcessSoundRecordState, RequiredProcessSoundVoiceTeardown},
    Installation, ResidencyRequest,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
fn resident() -> ResidentRuntime {
    let mut residency = ResidentRuntime::new(
        Installation::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
            .unwrap(),
    )
    .unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    residency
}
fn sound_runtime() -> ProcessSoundRuntime {
    ProcessSoundRuntime::new(
        inf_formats::FrontendExecutable::parse(assets().executable_bytes()).unwrap(),
    )
    .unwrap()
}
fn add_actor(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
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
            &mut 0,
        )
        .unwrap()
}
fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}
struct Camera {
    actor: ObjectId,
    result: Result<Option<ObjectId>, &'static str>,
    mutation: Option<[u16; 3]>,
    calls: usize,
}
impl ActorAudioCameraHost for Camera {
    type Error = &'static str;
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        self.calls += 1;
        if let Some(position) = self.mutation {
            arena
                .owned_mut(self.actor)
                .unwrap()
                .state
                .base
                .position_words = Some(position);
        }
        self.result
    }
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_u64().unwrap() as u16)
}

#[test]
fn selector35_audio_native_range_threshold_and_camera_prefix_match() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_audio_native.json"
    )))
    .unwrap();
    let bytes = assets().executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["provenance"]["exe_sha256"].as_str().unwrap()
    );
    let guard = &fixture["guard"];
    let at = guard["address"].as_u64().unwrap() as usize - 0x400000;
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&bytes[at..at + guard["size"].as_u64().unwrap() as usize])
        ),
        guard["sha256"].as_str().unwrap()
    );
    let residency = resident();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut arena = arena();
        let actor = add_actor(&mut arena, words(&input["position"]));
        let listener = add_actor(&mut arena, words(&input["camera"]));
        let mut sounds = sound_runtime();
        arena.actor_mut(actor).unwrap().word_8c = input["word8c"].as_u64().unwrap() as u16;
        // Deliberately contradict +8C: native audio must not select by size +28.
        arena.owned_mut(actor).unwrap().state.base.word_28 =
            if input["word8c"].as_u64().unwrap() > 400 {
                1
            } else {
                65535
            };
        let initial = input["existing"].as_bool().unwrap().then(|| {
            sounds
                .create_persistent(&residency, 16, 123, 456, 0, || {
                    Ok::<_, ObjectError>([7, 8, 9])
                })
                .unwrap()
        });
        arena.derived_actor_mut(actor).unwrap().sound_168 = initial;
        let mut camera = Camera {
            actor,
            result: Ok(input["camera_present"]
                .as_bool()
                .unwrap()
                .then_some(listener)),
            mutation: (!input["camera_mutation"].is_null())
                .then(|| words(&input["camera_mutation"])),
            calls: 0,
        };
        arena
            .tick_selector35_persistent_audio(
                actor,
                &mut camera,
                &residency,
                &mut sounds,
                &mut RequiredProcessSoundVoiceTeardown,
            )
            .unwrap();
        assert_eq!(camera.calls, 1, "case {index}");
        assert_eq!(
            arena.object(actor).unwrap().base.position_words,
            Some(words(&case["output"]["position"]))
        );
        let reference = arena.derived_actor(actor).unwrap().sound_168;
        assert_eq!(
            reference.is_some(),
            case["output"]["sound"].as_bool().unwrap(),
            "case {index}"
        );
        if let Some(event) = case["events"].as_array().unwrap().get(1) {
            if event["kind"] == "delete" {
                assert_eq!(
                    sounds.records().snapshot(initial.unwrap()).unwrap().state,
                    ProcessSoundRecordState::Released
                );
            } else {
                let record = sounds.records().snapshot(reference.unwrap()).unwrap();
                assert_eq!(
                    record.position,
                    Some(words(&event["position"])),
                    "case {index}"
                );
                assert_eq!(
                    record.volume,
                    Some(event["volume"].as_u64().unwrap() as u32),
                    "case {index}"
                );
                assert_eq!(
                    record.pitch,
                    Some(event["pitch"].as_u64().unwrap() as u32),
                    "case {index}"
                );
                if initial.is_some() {
                    assert_eq!(reference, initial);
                }
            }
        } else {
            assert_eq!(reference, initial);
            if let Some(reference) = reference {
                assert_eq!(
                    sounds.records().snapshot(reference).unwrap().volume,
                    Some(123)
                );
            }
        }
    }
}

#[test]
fn selector35_audio_null_camera_precedes_unknown_position_and_allocation() {
    let mut arena = arena();
    let actor = add_actor(&mut arena, [0; 3]);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut camera = Camera {
        actor,
        result: Ok(None),
        mutation: None,
        calls: 0,
    };
    let residency = resident();
    let mut sounds = sound_runtime();
    sounds.fail_next_record_allocation(17);
    arena
        .tick_selector35_persistent_audio(
            actor,
            &mut camera,
            &residency,
            &mut sounds,
            &mut RequiredProcessSoundVoiceTeardown,
        )
        .unwrap();
    assert!(sounds.is_empty());
    camera.result = Err("camera failure");
    assert!(matches!(
        arena.tick_selector35_persistent_audio(
            actor,
            &mut camera,
            &residency,
            &mut sounds,
            &mut RequiredProcessSoundVoiceTeardown
        ),
        Err(Selector35PersistentAudioError::Camera("camera failure"))
    ));
    let listener = add_actor(&mut arena, [0; 3]);
    camera.result = Ok(Some(listener));
    camera.mutation = Some([0; 3]);
    assert!(matches!(
        arena.tick_selector35_persistent_audio(
            actor,
            &mut camera,
            &residency,
            &mut sounds,
            &mut RequiredProcessSoundVoiceTeardown
        ),
        Err(Selector35PersistentAudioError::Sound(
            ProcessSoundRecordError::FixtureAllocation(17)
        ))
    ));
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([0; 3])
    );
    assert!(arena.derived_actor(actor).unwrap().sound_168.is_none());
}
