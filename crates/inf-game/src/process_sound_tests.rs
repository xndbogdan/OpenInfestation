use super::*;
use crate::{Installation, ResidencyRequest};

fn data_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn runtime() -> ProcessSoundRuntime {
    let raw = std::fs::read(data_root().join("Infest.exe")).unwrap();
    ProcessSoundRuntime::new(FrontendExecutable::parse(&raw).unwrap()).unwrap()
}

fn resident() -> ResidentRuntime {
    let mut resident = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
}

#[derive(Default)]
struct Audio {
    calls: Vec<(i16, ProcessSoundParameters)>,
    fail_at: Option<usize>,
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
            parameters,
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
            self.calls.push((resource.requested_slot(), parameters));
            if self.fail_at == Some(self.calls.len()) {
                Err("voice")
            } else {
                Ok(true)
            }
        })()
        .map(Reply::OneShot)
        .map_err(Fault::Backend)
    }
}

fn listener() -> ProcessSoundListener {
    ProcessSoundListener {
        view: SceneView {
            eye: [0; 3],
            basis: crate::menu::MenuBasis::INITIAL,
        },
        phase: 4,
        suppressed: false,
        water: None,
    }
}

fn request(sound: u32) -> ParticleSoundRequest {
    ParticleSoundRequest {
        sound,
        position: [0; 3],
        volume: 65536,
        pitch: 65536,
        flags: 0,
    }
}

#[test]
fn one_shot_enqueue_preserves_xyz_order_and_discards_queue_after_partial_voice_failure() {
    let resident = resident();
    let mut sounds = runtime();
    let mut second = request(9);
    second.position = [65535, 0, 0];
    for request in [request(6), second, request(10)] {
        sounds.enqueue(&resident, request).unwrap();
    }
    assert_eq!(sounds.pending_len(), 3);
    let mut audio = Audio {
        fail_at: Some(2),
        ..Audio::default()
    };
    let mut rng = 0x12345678;
    assert_eq!(
        sounds.service_world_draw(listener(), &mut audio, 15, &mut rng),
        Err(ProcessSoundServiceError::Audio("voice"))
    );
    assert_eq!(
        audio.calls.iter().map(|call| call.0).collect::<Vec<_>>(),
        [6, 9]
    );
    assert!(audio.calls[1].1.pan < 0);
    assert_eq!(sounds.started_in_last_service(), 1);
    assert!(sounds.is_empty());
    assert_eq!(rng, 0x12345678); // Supplied voice service consumes no alias RNG.
    assert_eq!(
        sounds.service_world_draw(listener(), &mut audio, 15, &mut rng),
        Ok(0)
    );
    assert_eq!(sounds.started_in_last_service(), 0);
    assert_eq!(audio.calls.len(), 2); // Disposed requests are not retried.
}

#[test]
fn native_phase_and_process_gates_discard_one_shots_without_voice_or_rng() {
    let resident = resident();
    for listener in [
        ProcessSoundListener {
            phase: 5,
            ..listener()
        },
        ProcessSoundListener {
            suppressed: true,
            ..listener()
        },
    ] {
        let mut sounds = runtime();
        sounds.enqueue(&resident, request(6)).unwrap();
        let mut audio = Audio::default();
        let mut rng = 42;
        assert_eq!(
            sounds.service_world_draw(listener, &mut audio, 15, &mut rng),
            Ok(0)
        );
        assert!(audio.calls.is_empty());
        assert!(sounds.is_empty());
        assert_eq!(rng, 42);
    }
}

#[test]
fn native_pan_positive_endpoint_is_9999_and_gate_is_strict() {
    let sounds = runtime();
    let parameters = sound_parameters(
        ParticleSoundRequest {
            position: [4096, 0, 0],
            ..request(6)
        }
        .into(),
        listener(),
        sounds.policy,
    )
    .unwrap();
    assert_eq!(parameters.pan, 9999);
    assert_eq!(parameters.volume, 32768);
    assert_eq!(
        sound_parameters(
            ParticleSoundRequest {
                position: [8192, 0, 0],
                ..request(6)
            }
            .into(),
            listener(),
            sounds.policy
        ),
        None
    );
}

#[test]
fn original_bytecode_one_shot_listener_parameter_fixtures() {
    #[derive(serde::Deserialize)]
    struct Fixtures {
        cases: Vec<Case>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        sound: u32,
        position: [u16; 3],
        eye: [i32; 3],
        basis: [u32; 10],
        volume: u32,
        pitch: u32,
        flags: u32,
        phase: u32,
        suppressed: bool,
        parameters: Vec<[u32; 4]>,
    }
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("../tests/fixtures/process_sound_native.json")).unwrap();
    let resident = resident();
    let mut sounds = runtime();
    for (index, case) in fixtures.cases.into_iter().enumerate() {
        sounds
            .enqueue(
                &resident,
                ParticleSoundRequest {
                    sound: case.sound,
                    position: case.position,
                    volume: case.volume,
                    pitch: case.pitch,
                    flags: case.flags,
                },
            )
            .unwrap();
        let mut audio = Audio::default();
        let mut rng = 0;
        sounds
            .service_world_draw(
                ProcessSoundListener {
                    view: SceneView {
                        eye: case.eye,
                        basis: crate::menu::MenuBasis { words: case.basis },
                    },
                    phase: case.phase,
                    suppressed: case.suppressed,
                    water: None,
                },
                &mut audio,
                15,
                &mut rng,
            )
            .unwrap();
        assert_eq!(
            audio
                .calls
                .iter()
                .map(|call| call.1.words())
                .collect::<Vec<_>>(),
            case.parameters,
            "native listener fixture {index}"
        );
        assert!(sounds.is_empty());
        assert_eq!(rng, 0);
    }
}
