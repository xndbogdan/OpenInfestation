use super::*;
use std::collections::HashMap;
pub(super) fn raw() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
fn pool(enabled: bool) -> ProcessSoundVoices {
    let raw = raw();
    let mut p =
        ProcessSoundVoices::native_startup(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    if enabled {
        *p.device_state_mut() = super::super::device::SoundDeviceRuntime {
            enabled: 1,
            device: Some(super::super::device::SoundDeviceIdentity(1)),
            configuration: 0,
        };
        p.initialize_list().unwrap();
    }
    p
}
#[derive(Default)]
struct Backend {
    events: Vec<(&'static str, ProcessSoundVoiceRef)>,
    playing: HashMap<ProcessSoundVoiceRef, bool>,
    fail: Option<(&'static str, bool)>,
    release_required: bool,
    configurations: usize,
    configuration_failure: bool,
}
impl VoicePlaybackBackend for Backend {
    type Error = u32;
    fn configure_device(&mut self) -> Result<(), VoicePlaybackFault<u32>> {
        self.configurations += 1;
        if self.configuration_failure {
            Err(VoicePlaybackFault::Required(73))
        } else {
            Ok(())
        }
    }
    fn operation(
        &mut self,
        r: VoicePlaybackRequest<'_>,
    ) -> Result<VoicePlaybackReply, VoicePlaybackFault<u32>> {
        use VoicePlaybackRequest as R;
        let (name, v) = match &r {
            R::Create { voice, .. } => ("create", *voice),
            R::Status { voice } => ("status", *voice),
            R::Restore { voice } => ("restore", *voice),
            R::Play { voice, .. } => ("play", *voice),
            R::Stop { voice } => ("stop", *voice),
            R::Release { voice } => ("release", *voice),
            R::Frequency { voice, .. } => ("frequency", *voice),
            R::Pan { voice, .. } => ("pan", *voice),
            R::Gain { voice, .. } => ("gain", *voice),
        };
        self.events.push((name, v));
        if name == "release" && self.release_required {
            return Err(VoicePlaybackFault::Required(72));
        }
        if self.fail == Some((name, false)) {
            self.fail = None;
            return Err(VoicePlaybackFault::Required(71));
        }
        if self.fail == Some((name, true)) {
            self.fail = None;
            return Err(VoicePlaybackFault::NativeResult(71));
        }
        match r {
            R::Create { .. } => {
                self.playing.insert(v, false);
            }
            R::Status { .. } => {
                return Ok(VoicePlaybackReply::Playing(*self.playing.get(&v).unwrap()))
            }
            R::Play { .. } => {
                self.playing.insert(v, true);
            }
            R::Stop { .. } => {
                self.playing.insert(v, false);
            }
            R::Release { .. } => {
                self.playing.remove(&v);
            }
            _ => {}
        }
        Ok(VoicePlaybackReply::Done)
    }
}
fn request() -> VoiceCreateRequest {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut r = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    r.apply(crate::ResidencyRequest::Boot).unwrap();
    VoiceCreateRequest {
        resource: ResidentSoundResource::capture(&r, 2),
        samples: vec![1000, -1000].into(),
        parameters: ProcessSoundParameters {
            volume: 65536,
            pitch: 65536,
            pan: 0,
            control_flag: false,
            placement: None,
        },
        master_volume: 15,
        looping: true,
    }
}
fn birth(p: &mut ProcessSoundVoices, b: &mut Backend, automatic: bool) -> ProcessSoundVoiceRef {
    assert_eq!(p.prepare_create(b), Ok(true));
    let r = p.allocate(request(), b).unwrap();
    if automatic {
        p.mark_one_shot(r).unwrap();
    }
    r
}
#[test]
fn independent_enabled_pointer_and_configuration_preserve_native_device_history() {
    let mut p = pool(false);
    let mut b = Backend::default();
    assert_eq!(p.prepare_create(&mut b), Ok(false));
    assert_eq!(p.start(None, &mut b), Ok(()));
    assert!(b.events.is_empty());
    // Explicit supplied post-initialization state. Full initialization/release
    // and their failures are authenticated by device.rs native receipts.
    *p.device_state_mut() = super::super::device::SoundDeviceRuntime {
        enabled: 7,
        device: Some(super::super::device::SoundDeviceIdentity(9)),
        configuration: 23,
    };
    assert_eq!(
        p.start(None, &mut b),
        Err(VoiceExecutionError::Pool(
            VoicePoolFault::RequiredVoicePointer
        ))
    );
    p.device_state_mut().device = None;
    assert!(p.enabled());
    assert_eq!(p.device_state().configuration, 23);
    assert_eq!(
        p.prepare_create(&mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::RequiredDevice))
    );
    // A pointer alone does not change the native enabled-byte gate.
    p.device_state_mut().enabled = 0;
    p.device_state_mut().device = Some(super::super::device::SoundDeviceIdentity(10));
    assert_eq!(p.prepare_create(&mut b), Ok(false));
}
#[test]
fn raw_buffer_configuration_publishes_only_after_normal_service_return() {
    let mut p = pool(true);
    let mut b = Backend {
        configuration_failure: true,
        ..Backend::default()
    };
    assert_eq!(
        p.configure_raw_source(&mut b),
        Err(VoiceExecutionError::Required(73))
    );
    assert_eq!(p.device_state().configuration, 0);
    assert_eq!(b.configurations, 1);
    b.configuration_failure = false;
    p.configure_raw_source(&mut b).unwrap();
    assert_eq!(p.device_state().configuration, 1);
    assert_eq!(b.configurations, 2);
    p.configure_raw_source(&mut b).unwrap();
    p.device_state_mut().configuration = 255;
    p.configure_raw_source(&mut b).unwrap();
    assert_eq!(p.device_state().configuration, 255);
    assert_eq!(b.configurations, 2);
    p.device_state_mut().device = None;
    assert_eq!(
        p.configure_raw_source(&mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::RequiredDevice))
    );
    assert_eq!(b.configurations, 2);
}
#[test]
fn create_native_control_order_and_retained60_allocation_fail_before_capacity() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let v = birth(&mut p, &mut b, false);
    assert_eq!(
        b.events.iter().map(|e| e.0).collect::<Vec<_>>(),
        ["create", "frequency", "pan", "gain"]
    );
    assert_eq!(p.snapshot(v).unwrap().retained_bytes, 60);
    assert_eq!(p.snapshot(v).unwrap().counter, 0);
    for _ in 1..24 {
        birth(&mut p, &mut b, false);
    }
    let count = b.events.len();
    p.fail_next_allocation();
    assert_eq!(
        p.allocate(request(), &mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::Allocation))
    );
    assert_eq!(
        p.allocate(request(), &mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::Capacity2202))
    );
    assert_eq!(b.events.len(), count);
    assert_eq!(p.live_len(), 24);
}
#[test]
fn sweep_evicts_oldest_auto_release_voice_and_never_persistent_capacity() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let first = birth(&mut p, &mut b, true);
    p.start(Some(first), &mut b).unwrap();
    for _ in 1..24 {
        birth(&mut p, &mut b, false);
    }
    assert_eq!(p.live_len(), 24);
    assert_eq!(p.prepare_create(&mut b), Ok(true));
    assert_eq!(p.live_len(), 23);
    assert_eq!(p.snapshot(first), Err(VoicePoolFault::Released(first)));
    let replacement = p.allocate(request(), &mut b).unwrap();
    assert_ne!(replacement, first);
    assert_eq!(p.prepare_create(&mut b), Ok(true));
    assert_eq!(p.live_len(), 24);
    assert_eq!(
        p.allocate(request(), &mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::Capacity2202))
    );
}
/// The splash cap: 12 playing splashes refuse a 13th, never another sound,
/// until one finishes; without the cap nothing is refused.
#[test]
fn twelve_playing_splashes_refuse_a_thirteenth() {
    for capped in [true, false] {
        let mut p = pool(true);
        p.set_limit(RETAIL_VOICE_LIMIT);
        if capped {
            p.set_splash_limit(Some(crate::frontend_settings::SPLASH_VOICES));
        }
        let mut b = Backend::default();
        // Started, so no sweep takes them as finished.
        let other = birth(&mut p, &mut b, true);
        p.start(Some(other), &mut b).unwrap();
        let splashes: Vec<_> = (0..crate::frontend_settings::SPLASH_VOICES)
            .map(|_| splash_birth(&mut p, &mut b))
            .collect();
        assert_eq!(p.splash_refused(SPLASH_SOUNDS[0]), Ok(capped));
        assert_eq!(p.splash_refused(6), Ok(false));
        // The create sweep frees a finished splash, and the cap with it.
        b.playing.insert(splashes[0], false);
        assert_eq!(p.prepare_create(&mut b), Ok(true));
        assert!(p.snapshot(splashes[1]).is_ok());
        assert_eq!(p.splash_refused(SPLASH_SOUNDS[1]), Ok(false));
    }
}

fn splash_birth(p: &mut ProcessSoundVoices, b: &mut Backend) -> ProcessSoundVoiceRef {
    assert_eq!(p.prepare_create(b), Ok(true));
    let voice = p.allocate(request(), b).unwrap();
    p.mark_one_shot(voice).unwrap();
    p.note_requested_slot(voice, SPLASH_SOUNDS[1]).unwrap();
    p.start(Some(voice), b).unwrap();
    voice
}

#[test]
fn signed_start_crossings_and_errors_retain_native_counter_prefix() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let v = birth(&mut p, &mut b, false);
    b.events.clear();
    b.fail = Some(("restore", true));
    assert_eq!(
        p.start(Some(v), &mut b),
        Err(VoiceExecutionError::NativeResult(71))
    );
    assert_eq!(p.snapshot(v).unwrap().counter, -1);
    assert_eq!(b.events, [("restore", v)]);
    b.events.clear();
    p.start(Some(v), &mut b).unwrap();
    assert_eq!(p.snapshot(v).unwrap().counter, -2);
    assert!(b.events.is_empty());
    p.set_counter(v, i32::MIN);
    p.start(Some(v), &mut b).unwrap();
    assert_eq!(p.snapshot(v).unwrap().counter, i32::MAX);
    assert_eq!(b.events, [("stop", v)]);
}
#[test]
fn one_shot_consumes_native_restore_and_play_but_not_missing_service() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let a = birth(&mut p, &mut b, false);
    b.events.clear();
    b.fail = Some(("restore", true));
    assert_eq!(p.start_one_shot(a, &mut b), Ok(()));
    assert_eq!(b.events, [("restore", a)]);
    assert!(p.snapshot(a).unwrap().automatic);
    let c = birth(&mut p, &mut b, false);
    b.events.clear();
    b.fail = Some(("play", true));
    assert_eq!(p.start_one_shot(c, &mut b), Ok(()));
    assert_eq!(b.events, [("restore", c), ("play", c)]);
    let d = birth(&mut p, &mut b, false);
    b.events.clear();
    b.fail = Some(("restore", false));
    assert_eq!(
        p.start_one_shot(d, &mut b),
        Err(VoiceExecutionError::Required(71))
    );
    assert_eq!(p.snapshot(d).unwrap().counter, -1);
    p.set_counter(d, i32::MIN);
    b.events.clear();
    b.fail = Some(("stop", true));
    assert_eq!(p.start_one_shot(d, &mut b), Ok(()));
    assert_eq!(p.snapshot(d).unwrap().counter, i32::MAX);
    assert_eq!(b.events, [("stop", d)]);
    p.set_counter(d, i32::MIN);
    b.events.clear();
    b.fail = Some(("stop", false));
    assert_eq!(
        p.start_one_shot(d, &mut b),
        Err(VoiceExecutionError::Required(71))
    );
    assert_eq!(p.snapshot(d).unwrap().counter, i32::MAX);
    assert_eq!(b.events, [("stop", d)]);
    p.set_counter(d, i32::MIN);
    b.events.clear();
    b.fail = Some(("stop", true));
    assert_eq!(
        p.start(Some(d), &mut b),
        Err(VoiceExecutionError::NativeResult(71))
    );
    assert_eq!(p.snapshot(d).unwrap().counter, i32::MAX);
    assert_eq!(b.events, [("stop", d)]);
}
#[test]
fn update_parameters_precede_ordered_failure_and_do_not_restart() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let v = birth(&mut p, &mut b, false);
    p.start(Some(v), &mut b).unwrap();
    b.events.clear();
    b.fail = Some(("pan", true));
    let params = ProcessSoundParameters {
        volume: 17,
        pitch: 23,
        pan: -91,
        control_flag: true,
        placement: None,
    };
    assert_eq!(
        p.update(v, params, 7, &mut b),
        Err(VoiceExecutionError::NativeResult(71))
    );
    assert_eq!(p.snapshot(v).unwrap().parameters, params);
    assert_eq!(p.snapshot(v).unwrap().counter, -1);
    assert_eq!(b.events, [("frequency", v), ("pan", v)]);
}
#[test]
fn delete_consumes_native_stop_and_required_fault_stops_before_unlink() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let a = birth(&mut p, &mut b, false);
    let c = birth(&mut p, &mut b, false);
    p.start(Some(a), &mut b).unwrap();
    b.fail = Some(("stop", false));
    b.events.clear();
    assert_eq!(p.delete(a, &mut b), Err(VoiceExecutionError::Required(71)));
    assert_eq!(p.head(), Some(a));
    let row = p.snapshot(a).unwrap();
    assert_eq!(
        (row.counter, row.retained_bytes, row.next),
        (0, 60, Some(c))
    );
    p.set_counter(a, -1);
    b.fail = Some(("stop", true));
    b.events.clear();
    assert_eq!(p.delete(a, &mut b), Ok(()));
    assert_eq!(b.events, [("stop", a), ("release", a)]);
    assert_eq!(p.head(), Some(c));
    assert_eq!(p.snapshot(c).unwrap().previous, None);
    assert_eq!(p.snapshot(a), Err(VoicePoolFault::Released(a)));
}
#[test]
fn next_capture_and_foreign_pool_identity_are_independent_of_decoded_data() {
    let mut p = pool(true);
    let mut q = pool(true);
    let mut b = Backend::default();
    let a = birth(&mut p, &mut b, true);
    p.start(Some(a), &mut b).unwrap();
    let c = birth(&mut p, &mut b, true);
    p.start(Some(c), &mut b).unwrap();
    let other = birth(&mut q, &mut b, false);
    assert_eq!(p.snapshot(other), Err(VoicePoolFault::Foreign(other)));
    b.playing.insert(a, false);
    b.playing.insert(c, false);
    b.events.clear();
    p.sweep(false, &mut b).unwrap();
    assert_eq!(p.live_len(), 0);
    assert_eq!(
        b.events,
        [
            ("status", a),
            ("stop", a),
            ("release", a),
            ("status", c),
            ("stop", c),
            ("release", c)
        ]
    );
    assert_ne!(a, other);
    assert!(format!("{a:?}").contains("sound_voice_pool"));
}

#[test]
fn failed_cleanup_retains_unlinked_real_allocation_and_never_reuses_identity() {
    let mut p = pool(true);
    let mut b = Backend::default();
    b.fail = Some(("pan", true));
    b.release_required = true;
    assert_eq!(
        p.allocate(request(), &mut b),
        Err(VoiceExecutionError::Required(72))
    );
    let pending = b.events[0].1;
    assert_eq!(p.snapshot(pending), Err(VoicePoolFault::Pending(pending)));
    assert_eq!((p.head(), p.live_len()), (None, 0));
    let row = &p.voices[(pending.serial - 1) as usize];
    assert_eq!(row.state, VoiceAllocationState::AllocatedUnlinked);
    assert!(row.snapshot.is_none());
    assert_eq!(row.backing.as_ref().unwrap().len(), 60);
    assert!(row.resource.is_some());
    b.release_required = false;
    b.events.clear();
    let next = birth(&mut p, &mut b, false);
    assert_ne!(next, pending);
    assert!(next.serial > pending.serial);
    assert_eq!(p.snapshot(pending), Err(VoicePoolFault::Pending(pending)));
    assert_eq!(
        p.start(Some(pending), &mut b),
        Err(VoiceExecutionError::Pool(VoicePoolFault::Pending(pending)))
    );
}
#[test]
fn global_reattenuation_visits_automatic_and_persistent_in_native_order() {
    let mut p = pool(true);
    let mut b = Backend::default();
    let a = birth(&mut p, &mut b, false);
    let c = birth(&mut p, &mut b, true);
    p.start(Some(a), &mut b).unwrap();
    p.start(Some(c), &mut b).unwrap();
    let before = [p.snapshot(a).unwrap(), p.snapshot(c).unwrap()];
    b.events.clear();
    p.reattenuate(7, &mut b).unwrap();
    assert_eq!(b.events, [("gain", a), ("gain", c)]);
    assert_eq!([p.snapshot(a).unwrap(), p.snapshot(c).unwrap()], before);
    b.events.clear();
    b.fail = Some(("gain", true));
    assert_eq!(
        p.reattenuate(1, &mut b),
        Err(VoiceExecutionError::NativeResult(71))
    );
    assert_eq!(b.events, [("gain", a)]);
    assert_eq!([p.snapshot(a).unwrap(), p.snapshot(c).unwrap()], before);
}
