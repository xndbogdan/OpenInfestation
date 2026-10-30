use super::*;
use crate::{Installation, ResidencyRequest};
use std::cell::Cell;
fn resident() -> crate::ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = crate::ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
}
fn birth(
    records: &mut ProcessSoundRecords,
    resident: &crate::ResidentRuntime,
    kind: ProcessSoundRecordKind,
    slot: i16,
) -> ProcessSoundRef {
    records
        .construct(ProcessSoundRecordConstructionRequest {
            resource: ResidentSoundResource::capture(resident, slot),
            kind,
            volume: 65536,
            pitch: 32768,
            control_flag: 0,
            position: || Ok::<_, &'static str>([1, 2, 3]),
        })
        .unwrap()
}
#[derive(Default)]
struct Voices {
    calls: Vec<(ProcessSoundRef, ProcessSoundVoiceRef)>,
    fail: bool,
}
impl ProcessSoundVoiceTeardown for Voices {
    fn delete_voice(
        &mut self,
        record: ProcessSoundRef,
        voice: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        self.calls.push((record, voice));
        if self.fail {
            Err(ProcessSoundRecordError::VoiceTeardown { voice, code: 71 })
        } else {
            Ok(())
        }
    }
}
#[test]
fn required_allocation_fails_before_position_or_list_publication() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let first = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let before = records.snapshot(first).unwrap();
    let read = Cell::new(false);
    records.fail_next_allocation(17);
    let result = records.construct(ProcessSoundRecordConstructionRequest {
        resource: ResidentSoundResource::capture(&resident, 6),
        kind: ProcessSoundRecordKind::OneShot,
        volume: 1,
        pitch: 2,
        control_flag: 0,
        position: || {
            read.set(true);
            Ok::<_, &'static str>([9; 3])
        },
    });
    assert!(matches!(
        result,
        Err(ProcessSoundConstructionError::Record(
            ProcessSoundRecordError::FixtureAllocation(17)
        ))
    ));
    assert!(!read.get());
    assert_eq!(records.linked_len(), 1);
    assert_eq!(records.snapshot(first).unwrap(), before);
}
#[test]
fn source_failure_retains_linked_resource_and40_bytes_with_unknown_flags() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let first = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let result = records.construct(ProcessSoundRecordConstructionRequest {
        resource: ResidentSoundResource::capture(&resident, 6),
        kind: ProcessSoundRecordKind::OneShot,
        volume: 1,
        pitch: 2,
        control_flag: 0,
        position: || Err::<[u16; 3], _>("sourceXYZ"),
    });
    assert!(matches!(
        result,
        Err(ProcessSoundConstructionError::Source("sourceXYZ"))
    ));
    let pending = records.snapshot(first).unwrap().next.unwrap();
    let row = records.snapshot(pending).unwrap();
    assert_eq!(row.previous, Some(first));
    assert_eq!(row.next, None);
    assert_eq!(row.retained_bytes, 40);
    assert_eq!(row.state, ProcessSoundRecordState::Linked);
    assert_eq!(
        (row.position, row.volume, row.pitch, row.flags),
        (None, None, None, None)
    );
    assert_eq!(
        records.entries[records.index(pending).unwrap()]
            .resource
            .as_ref()
            .unwrap()
            .requested_slot(),
        6
    );
    //Actual480110 and4800E0 demand flags first. No constructor-failure omission.
    assert!(
        matches!(records.service_row(pending), Err(ProcessSoundRecordError::Pending(r)) if r == pending)
    );
    assert_eq!(
        records.cleanup_one_shots(&mut Voices::default()),
        Err(ProcessSoundRecordError::Pending(pending))
    );
    assert_eq!(records.linked_len(), 2);
}
#[test]
fn fresh_pitch_demand_follows_required_modulation_not_spatial_packet_capture() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let reference = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let index = records.index(reference).unwrap();
    records.entries[index].snapshot.pitch = None;
    records.entries[index].snapshot.modulation_is_null = Some(false);
    assert!(records.service_row(reference).is_ok());
    assert_eq!(
        records.require_null_modulation(reference),
        Err(ProcessSoundRecordError::RequiredModulation(reference))
    );
    records.entries[index].snapshot.modulation_is_null = Some(true);
    assert!(records.require_null_modulation(reference).is_ok());
    assert_eq!(
        records.pitch(reference),
        Err(ProcessSoundRecordError::Pending(reference))
    );
}
#[test]
fn list_head_middle_tail_delete_and_next_capture_preserve_persistent_order() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let head = birth(&mut records, &resident, ProcessSoundRecordKind::OneShot, 6);
    let middle = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let tail = birth(&mut records, &resident, ProcessSoundRecordKind::OneShot, 9);
    assert_eq!(records.snapshot(head).unwrap().next, Some(middle));
    assert_eq!(records.snapshot(middle).unwrap().next, Some(tail));
    let mut voices = Voices::default();
    records.cleanup_one_shots(&mut voices).unwrap();
    assert_eq!(records.head, Some(middle));
    assert_eq!(records.tail, Some(middle));
    assert_eq!(records.snapshot(middle).unwrap().previous, None);
    assert_eq!(records.snapshot(middle).unwrap().next, None);
    for reference in [head, tail] {
        let row = records.snapshot(reference).unwrap();
        assert_eq!(row.state, ProcessSoundRecordState::Released);
        assert_eq!(row.retained_bytes, 0);
    }
    assert!(voices.calls.is_empty()); //Native constructor-NULL18 does not invoke backend.
    records.delete(middle, &mut voices).unwrap();
    assert!(records.head.is_none());
    assert!(records.tail.is_none());
    assert_eq!(records.linked_len(), 0);
}
#[test]
fn detached_voice_failure_retains_allocation_and_never_relinks_or_retries() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let a = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let b = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        6,
    );
    let c = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        9,
    );
    let voice = ProcessSoundVoiceRef {
        owner: 27,
        serial: 1,
    };
    let index = records.index(b).unwrap();
    records.entries[index].snapshot.voice = Some(Some(voice));
    let mut voices = Voices {
        fail: true,
        ..Voices::default()
    };
    assert_eq!(
        records.delete(b, &mut voices),
        Err(ProcessSoundRecordError::VoiceTeardown { voice, code: 71 })
    );
    assert_eq!(records.snapshot(a).unwrap().next, Some(c));
    assert_eq!(records.snapshot(c).unwrap().previous, Some(a));
    let row = records.snapshot(b).unwrap();
    assert_eq!(row.state, ProcessSoundRecordState::Detached);
    assert_eq!(row.voice, Some(Some(voice)));
    assert_eq!(row.retained_bytes, 40);
    assert_eq!(
        records.delete(b, &mut voices),
        Err(ProcessSoundRecordError::Detached(b))
    );
    assert_eq!(voices.calls, [(b, voice)]);
    //This error is a supplied portable service failure, not a DSStop HRESULT.
}
#[test]
fn normal_voice_return_clears18_before_release_and_foreign_identity_never_matches() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let reference = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let voice = ProcessSoundVoiceRef {
        owner: 2,
        serial: 1,
    };
    let index = records.index(reference).unwrap();
    records.entries[index].snapshot.voice = Some(Some(voice));
    let mut voices = Voices::default();
    records.delete(reference, &mut voices).unwrap();
    let row = records.snapshot(reference).unwrap();
    assert_eq!(row.voice, Some(None));
    assert_eq!(row.retained_bytes, 0);
    assert_eq!(row.state, ProcessSoundRecordState::Released);
    assert_eq!(voices.calls, [(reference, voice)]);
    assert_eq!(
        records.delete(reference, &mut voices),
        Err(ProcessSoundRecordError::Released(reference))
    );
    let mut other = ProcessSoundRecords::new();
    assert_eq!(
        other.delete(reference, &mut voices),
        Err(ProcessSoundRecordError::Foreign(reference))
    );
}
#[test]
fn process_clear_is_distinct_from_one_shot_cleanup_and_stops_at_real_prefix() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let a = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let b = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        6,
    );
    let c = birth(&mut records, &resident, ProcessSoundRecordKind::OneShot, 9);
    let index = records.index(b).unwrap();
    records.entries[index].snapshot.modulation_is_null = Some(false);
    let mut voices = Voices::default();
    assert_eq!(
        records.clear(&mut voices),
        Err(ProcessSoundRecordError::RequiredModulation(b))
    );
    assert_eq!(
        records.snapshot(a).unwrap().state,
        ProcessSoundRecordState::Released
    );
    assert_eq!(
        records.snapshot(b).unwrap().state,
        ProcessSoundRecordState::Detached
    );
    assert_eq!(records.head, Some(c));
    assert_eq!(
        records.snapshot(c).unwrap().state,
        ProcessSoundRecordState::Linked
    );
    assert!(voices.calls.is_empty());
}
#[test]
fn optional_position_update_skips_source_but_failed_source_preserves_gain_and_pitch() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    let reference = birth(
        &mut records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let before = records.snapshot(reference).unwrap();
    let result = records.update(reference, 19, 23, Some(|| Err::<[u16; 3], _>("freshXYZ")));
    assert!(matches!(
        result,
        Err(ProcessSoundConstructionError::Source("freshXYZ"))
    ));
    assert_eq!(records.snapshot(reference).unwrap(), before);
    records
        .update::<&'static str>(
            reference,
            19,
            23,
            None::<fn() -> Result<[u16; 3], &'static str>>,
        )
        .unwrap();
    let after = records.snapshot(reference).unwrap();
    assert_eq!(after.position, before.position);
    assert_eq!((after.volume, after.pitch), (Some(19), Some(23)));
}
#[test]
fn named_exhaustion_is_not_a_numeric_process_mode() {
    let resident = resident();
    let mut records = ProcessSoundRecords::new();
    records.process = ProcessSoundIdentity::Exhausted;
    let result = records.construct(ProcessSoundRecordConstructionRequest {
        resource: ResidentSoundResource::capture(&resident, 64),
        kind: ProcessSoundRecordKind::Persistent,
        volume: 1,
        pitch: 2,
        control_flag: 0,
        position: || -> Result<[u16; 3], &'static str> {
            panic!("exhausted identity precedes source")
        },
    });
    assert!(matches!(
        result,
        Err(ProcessSoundConstructionError::Record(
            ProcessSoundRecordError::IdentityExhausted
        ))
    ));
    assert_eq!(records.linked_len(), 0);
}

#[test]
fn unknown_voice_is_demanded_only_by_reached_playback_or_retirement() {
    use crate::process_sound::{
        ProcessSoundAudio, ProcessSoundListener, ProcessSoundRuntime, ProcessSoundServiceError,
    };
    struct NoPlayback;
    impl crate::process_sound::ProcessSoundVoiceTeardown for NoPlayback {
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
    impl ProcessSoundAudio for NoPlayback {
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
                resource: _,
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
                panic!("unknown persistent voice cannot dispatch one-shot playback")
            })()
            .map(Reply::OneShot)
            .map_err(Fault::Backend)
        }
    }
    let resident = resident();
    let executable_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&executable_bytes).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let reference = birth(
        &mut sounds.records,
        &resident,
        ProcessSoundRecordKind::Persistent,
        64,
    );
    let index = sounds.records.index(reference).unwrap();
    sounds.records.entries[index].snapshot.voice = None;
    let listener = ProcessSoundListener {
        view: crate::frontend_scene::SceneView {
            eye: [1, 2, 3],
            basis: crate::menu::MenuBasis::INITIAL,
        },
        phase: 4,
        suppressed: true,
        water: None,
    };
    let mut rng = 0x1234abcd;
    assert_eq!(
        sounds.service_world_draw(listener, &mut NoPlayback, 15, &mut rng),
        Ok(0)
    );
    //Even rejected gain reads fresh pitch before the optional18 deletion.
    sounds.records.entries[index].snapshot.volume = Some(0);
    sounds.records.entries[index].snapshot.pitch = None;
    sounds.records.entries[index].snapshot.voice = Some(None);
    assert_eq!(
        sounds.service_world_draw(
            ProcessSoundListener {
                suppressed: false,
                ..listener
            },
            &mut NoPlayback,
            15,
            &mut rng,
        ),
        Err(ProcessSoundServiceError::Record(
            ProcessSoundRecordError::Pending(reference)
        ))
    );
    sounds.records.entries[index].snapshot.pitch = Some(32768);
    sounds.records.entries[index].snapshot.voice = None;
    for volume in [0, 65536] {
        sounds.records.entries[index].snapshot.volume = Some(volume);
        assert_eq!(
            sounds.service_world_draw(
                ProcessSoundListener {
                    suppressed: false,
                    ..listener
                },
                &mut NoPlayback,
                15,
                &mut rng,
            ),
            Err(ProcessSoundServiceError::Record(
                ProcessSoundRecordError::Pending(reference)
            ))
        );
    }
    assert_eq!(
        sounds.records.snapshot(reference).unwrap().state,
        ProcessSoundRecordState::Linked
    );
    assert_eq!(rng, 0x1234abcd);
}
