use super::*;
use crate::{frontend_audio::FrontendAudio, Installation, ResidencyRequest};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn resident() -> ResidentRuntime {
    let mut resident = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    resident
}

#[derive(Default)]
struct AdmittedSoundSourceFixture(PreparedPcmSources);
impl SoundSourcePreparation for AdmittedSoundSourceFixture {
    fn enter(&mut self) -> Result<bool, PlatformAudioError> {
        Ok(true)
    }
    fn pcm(
        &mut self,
        a: &Arc<LoadedOverlay>,
        r: usize,
        l: u16,
        b: &[u8],
    ) -> Result<Arc<[i16]>, PlatformAudioError> {
        self.0.prepare(a, r, l, b, || Ok(()))
    }
}
struct FixturePlayback<'a>(&'a mut Mixer);
impl crate::process_sound::VoicePlaybackBackend for FixturePlayback<'_> {
    type Error = crate::process_sound::VoicePoolFault;
    fn configure_device(
        &mut self,
    ) -> Result<(), crate::process_sound::VoicePlaybackFault<Self::Error>> {
        Ok(()) // supplied successful configuration; production uses actual SDL
    }
    fn operation(
        &mut self,
        request: crate::process_sound::VoicePlaybackRequest<'_>,
    ) -> Result<
        crate::process_sound::VoicePlaybackReply,
        crate::process_sound::VoicePlaybackFault<Self::Error>,
    > {
        mixer_voice_operation(self.0, request)
    }
}
struct Audio {
    mixer: Mixer,
    voices: crate::process_sound::ProcessSoundVoices,
    pcm: PreparedPcmSources,
}
impl Audio {
    fn new() -> Self {
        let raw = std::fs::read(data_root().join("Infest.exe")).unwrap();
        let mut voices = crate::process_sound::ProcessSoundVoices::native_startup(
            FrontendExecutable::parse(&raw).unwrap(),
        )
        .unwrap();
        // Supplied device fixture, independent from production native loans.
        *voices.device_state_mut() = crate::process_sound::device::SoundDeviceRuntime {
            enabled: 1,
            device: Some(crate::process_sound::device::SoundDeviceIdentity(1)),
            configuration: 0,
        };
        voices.initialize_list().unwrap();
        Self {
            mixer: Mixer::default(),
            voices,
            pcm: PreparedPcmSources::default(),
        }
    }
    fn cue(
        &mut self,
        resource: &ResidentSoundResource,
        p: [u32; 4],
        master: u32,
        rng: &mut u32,
    ) -> Result<CueReceipt, PlatformAudioError> {
        self.mixer.stats.cue_requests += 1;
        let receipt = play_one_shot_with(
            VoiceSourceLoans {
                voices: &mut self.voices,
                pcm: &mut self.pcm,
                backend: &mut FixturePlayback(&mut self.mixer),
            },
            OneShotSourceRequest {
                resource,
                parameters: p,
                master,
                rng,
                placement: None,
            },
        )?;
        self.mixer.stats.cues_started += u64::from(matches!(receipt, CueReceipt::Started { .. }));
        Ok(receipt)
    }
}
impl crate::process_sound::ProcessSoundVoiceTeardown for Audio {
    fn delete_voice(
        &mut self,
        _: crate::process_sound::ProcessSoundRef,
        voice: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        self.voices
            .delete(voice, &mut FixturePlayback(&mut self.mixer))
            .map_err(crate::process_sound::ProcessSoundRecordError::VoiceExecution)
    }
}
impl crate::process_sound::ProcessSoundAudio for Audio {
    type Error = PlatformAudioError;
    fn operation(
        &mut self,
        request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
    ) -> Result<
        crate::process_sound::ProcessSoundVoiceReply,
        crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
    > {
        use crate::process_sound::{
            ProcessSoundVoiceReply as Reply, ProcessSoundVoiceRequest as R,
        };
        let result: Result<Reply, PlatformAudioError> = (|| match request {
            R::OneShot {
                resource,
                parameters,
                effects_volume,
                rng,
            } => Ok(Reply::OneShot(matches!(
                self.cue(resource, parameters.words(), effects_volume, rng)?,
                CueReceipt::Started { .. }
            ))),
            R::Create {
                resource,
                parameters,
                effects_volume,
                looping,
                rng,
            } => {
                let (output, _) = create_voice_with(
                    VoiceSourceLoans {
                        voices: &mut self.voices,
                        pcm: &mut self.pcm,
                        backend: &mut FixturePlayback(&mut self.mixer),
                    },
                    VoiceSourceRequest {
                        resource,
                        parameters: parameters.words(),
                        master: effects_volume,
                        looping,
                        rng,
                        placement: None,
                    },
                )?;
                Ok(Reply::Created(output))
            }
            R::Start { voice } => {
                self.voices
                    .start(voice, &mut FixturePlayback(&mut self.mixer))?;
                Ok(Reply::Completed)
            }
            R::Update {
                voice,
                parameters,
                effects_volume,
            } => {
                self.voices.update(
                    voice,
                    parameters,
                    effects_volume,
                    &mut FixturePlayback(&mut self.mixer),
                )?;
                Ok(Reply::Completed)
            }
        })();
        result.map_err(crate::process_sound::ProcessSoundVoiceServiceFault::Backend)
    }
}

#[test]
fn native_alias_pitch_gain_and_random_consumption() {
    let resident = resident();
    let mut random = 0;
    let cue2 = resolve_sound(
        &ResidentSoundResource::capture(&resident, 2),
        [65536, 65536, 0, 1],
        &mut random,
        &mut AdmittedSoundSourceFixture::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!((cue2.slot, cue2.samples.len(), random), (2, 6228, 0));
    let cue3 = resolve_sound(
        &ResidentSoundResource::capture(&resident, 3),
        [65536, 65536, 0, 1],
        &mut random,
        &mut AdmittedSoundSourceFixture::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(cue3.slot, 2);
    assert_eq!(cue3.alias_count, 1);
    assert_eq!(cue3.parameters, [65536, 98304, 0, 1]);
    assert_eq!(frequency(cue3.parameters[1]), 33075);
    assert_eq!(random, 0x13d573);
    random = 0;
    let cue5 = resolve_sound(
        &ResidentSoundResource::capture(&resident, 5),
        [65536, 65536, 0, 1],
        &mut random,
        &mut AdmittedSoundSourceFixture::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(cue5.slot, 1);
    assert_eq!(cue5.parameters, [65536, 32768, 0, 1]);
    assert_eq!(frequency(cue5.parameters[1]), 11025);
    assert_eq!(random, 0x13d573);
    assert!(cue2.samples.iter().any(|sample| *sample != 0));
}

#[test]
fn retained_alias_uses_allocation_mapping_after_residency_owner_is_dropped() {
    let resident = resident();
    let retained = ResidentSoundResource::capture(&resident, 3);
    drop(resident);
    let mut random = 0;
    let resolved = resolve_sound(
        &retained,
        [65536, 65536, 0, 0],
        &mut random,
        &mut AdmittedSoundSourceFixture::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved.slot, 2);
    assert_eq!(resolved.samples.len(), 6228);
    assert_eq!(resolved.parameters, [65536, 98304, 0, 0]);
    assert_eq!(random, 0x13d573);
}

#[test]
fn process_one_shots_start_real_resident_pcm_and_outlive_queue_cleanup() {
    use crate::process_sound::{ProcessSoundListener, ProcessSoundRuntime};
    let resident = resident();
    let raw = std::fs::read(data_root().join("Infest.exe")).unwrap();
    let mut sounds = ProcessSoundRuntime::new(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    for sound in [6, 9, 10] {
        sounds
            .enqueue(
                &resident,
                crate::process_particles::audio::ParticleSoundRequest {
                    sound,
                    position: [128, 0, 0],
                    volume: 65536,
                    pitch: 65536,
                    flags: 0,
                },
            )
            .unwrap();
    }
    let mut audio = Audio::new();
    let count = sounds
        .service_world_draw(
            ProcessSoundListener {
                view: crate::frontend_scene::SceneView {
                    eye: [0; 3],
                    basis: crate::menu::MenuBasis::INITIAL,
                },
                phase: 4,
                suppressed: false,
                water: None,
            },
            &mut audio,
            15,
            &mut 0,
        )
        .unwrap();
    assert_eq!(count, 3);
    assert!(sounds.is_empty());
    drop(resident);
    assert_eq!(audio.mixer.effects.len(), 3);
    assert!(audio.mixer.effects.iter().all(|voice| voice.pan > 0));
    let mut output = [0.0; 4096];
    let mut nonzero = false;
    while audio.mixer.effects.iter().any(|v| v.playing) {
        audio.mixer.callback(&mut output);
        nonzero |= output.iter().any(|value| *value != 0.0);
    }
    assert!(nonzero);
    assert_eq!(audio.voices.live_len(), 3); //finished playback remains until the native sweep.
    audio
        .voices
        .sweep(false, &mut FixturePlayback(&mut audio.mixer))
        .unwrap();
    assert!(audio.mixer.effects.is_empty());
    assert!(audio.mixer.stats.effect_frames > 0);
    assert_eq!(audio.mixer.stats.cues_started, 3);
}

#[test]
fn shared_one_shot_admission_evicts_first_auto_release_before_voice_25() {
    let resident = resident();
    let resource = ResidentSoundResource::capture(&resident, 6);
    let mut audio = Audio::new();
    let mut first = None;
    for index in 0..25 {
        let receipt = audio
            .cue(&resource, [65536, 65536, 0, 0], 15, &mut 0)
            .unwrap();
        assert!(matches!(receipt, CueReceipt::Started { .. }));
        if index == 0 {
            first = audio.voices.head();
        }
    }
    assert_eq!(audio.voices.live_len(), 24);
    assert_eq!(audio.mixer.effects.len(), 24);
    let first = first.unwrap();
    assert_eq!(
        audio.voices.snapshot(first),
        Err(crate::process_sound::VoicePoolFault::Released(first))
    );
    assert_eq!(audio.mixer.stats.cue_requests, 25);
    assert_eq!(audio.mixer.stats.cues_started, 25);
}

/// The splash cap over resident H9 data: the 13th splash is refused while
/// the twelve play, and draws the RNG a played splash draws.
#[test]
fn a_thirteenth_splash_is_dropped_while_twelve_play() {
    let resident = resident();
    let [first_slot, second_slot] = crate::process_sound::SPLASH_SOUNDS;
    let mut audio = Audio::new();
    audio
        .voices
        .set_limit(crate::process_sound::RETAIL_VOICE_LIMIT);
    audio
        .voices
        .set_splash_limit(Some(crate::frontend_settings::SPLASH_VOICES));
    let other = ResidentSoundResource::capture(&resident, 6);
    audio.cue(&other, [65536, 65536, 0, 0], 15, &mut 0).unwrap();
    let other = audio.voices.head().unwrap();
    let slot = |index: usize| {
        if index % 2 == 0 {
            first_slot
        } else {
            second_slot
        }
    };
    for index in 0..crate::frontend_settings::SPLASH_VOICES {
        let splash = ResidentSoundResource::capture(&resident, slot(index));
        let receipt = audio
            .cue(&splash, [65536, 65536, 0, 0], 15, &mut 0)
            .unwrap();
        assert!(matches!(receipt, CueReceipt::Started { .. }));
    }
    let first_splash = audio.voices.snapshot(other).unwrap().next.unwrap();
    let splash = ResidentSoundResource::capture(&resident, slot(0));
    let mut refused_rng = 0x1234_5678;
    let receipt = audio
        .cue(&splash, [65536, 65536, 0, 0], 15, &mut refused_rng)
        .unwrap();
    assert_eq!(
        receipt,
        CueReceipt::Capacity2202 {
            requested_slot: first_slot
        }
    );
    assert_eq!(
        audio.voices.live_len(),
        1 + crate::frontend_settings::SPLASH_VOICES
    );
    assert!(audio.voices.snapshot(other).is_ok());
    assert!(audio.voices.snapshot(first_splash).is_ok());
    let mut played_rng = 0x1234_5678;
    let receipt = Audio::new()
        .cue(&splash, [65536, 65536, 0, 0], 15, &mut played_rng)
        .unwrap();
    assert!(matches!(receipt, CueReceipt::Started { .. }));
    assert_eq!(refused_rng, played_rng);
}

#[test]
fn alias_wrapping_and_native_low_byte_gain_branch() {
    let params = alias_parameters(
        [0x12345678, 0xf1234567, 0, 1],
        [0, 0x10203, 0xfedcba98, 0x32123],
        0xfedc,
    );
    // The native >=65536 gain branch really masks the low byte, not WORD.
    let expected_gain = (0x1234u32)
        .wrapping_mul(0x2123)
        .wrapping_add(3u32.wrapping_mul(0x12345678))
        .wrapping_add((0x78u32.wrapping_mul(0x2123)) >> 16);
    assert_eq!(params[0], expected_gain);
    assert_eq!(params[2..], [0, 1]);
    assert_eq!(attenuation(65536, 0), -10000);
    assert_eq!(attenuation(65536, 15), 0);
    assert!(attenuation(65536, 7) < 0);
}

#[test]
fn original_executable_alias_and_volume_fixtures() {
    #[derive(serde::Deserialize)]
    struct Alias {
        parameters: [u32; 4],
        fields: [u32; 4],
        seed: u32,
        seed_after: u32,
        output: [u32; 4],
    }
    #[derive(serde::Deserialize)]
    struct Attenuation {
        settings: u32,
        gain: u32,
        output: i32,
    }
    #[derive(serde::Deserialize)]
    struct Fixtures {
        aliases: Vec<Alias>,
        attenuation: Vec<Attenuation>,
    }
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("../tests/fixtures/platform_audio_native.json")).unwrap();
    for case in fixtures.aliases {
        let next = case.seed.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
        assert_eq!(next, case.seed_after);
        assert_eq!(
            alias_parameters(case.parameters, case.fields, next >> 16),
            case.output
        );
    }
    for case in fixtures.attenuation {
        assert_eq!(
            attenuation(case.gain, case.settings),
            case.output,
            "settings={}, gain={}",
            case.settings,
            case.gain
        );
    }
}

/// The mixer's output for `frames` frames, past the limiter's delay.
fn mix(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut output = vec![0.0; 2 * (frames + LIMITER_AHEAD - 1)];
    mixer.callback(&mut output);
    output.split_off(2 * (LIMITER_AHEAD - 1))
}

/// A sum past full scale is limited, not cut: its channels keep their
/// ratio, and the frames before it ease down to meet it.
#[test]
fn pcm_mixer_combines_channels_limits_and_finishes_without_looping() {
    let mut mixer = Mixer {
        cd_volume: [0.5, 1.0],
        effects_volume: 15,
        music: Some(MusicVoice {
            track: 8,
            samples: vec![16384, -16384, 32767, 32767].into(),
            frame: 0,
            playing: true,
        }),
        effects: vec![EffectVoice {
            voice: crate::process_sound::ProcessSoundVoiceRef {
                owner: 1,
                serial: 1,
            },
            playing: true,
            looping: false,
            samples: vec![16384, 16384].into(),
            position: 0.0,
            step: 1.0,
            attenuation_hundredth_db: attenuation(65536, 15),
            pan: 0,
            gains: None,
            placed: None,
            low: 0.0,
            ears: Ears::default(),
        }],
        ..Mixer::default()
    };
    let output = mix(&mut mixer, 4);
    // Frame 1 sums to (1.0, 1.5): scaled to full scale, not cut.
    assert!((output[3] - 1.0).abs() < 1.0e-5, "{}", output[3]);
    assert!((output[2] / output[3] - 1.0 / 1.5).abs() < 1.0e-5);
    // Frame 0, (0.75, 0.0), already on its way down.
    assert!(output[0] > 0.45 && output[0] < 0.75, "{}", output[0]);
    assert_eq!(output[1], 0.0);
    assert_eq!(output[4..], [0.0; 4]);
    assert_eq!(mixer.stats.music_frames, 2);
    assert_eq!(mixer.stats.effect_frames, 2);
    assert!(mixer.completion);
    assert!(!mixer.music.as_ref().unwrap().playing);
    assert_eq!(mixer.effects.len(), 1);
    assert!(!mixer.effects[0].playing); //physical finish does not unlink native voice history.
}

#[test]
fn absent_cdaudio_directory_is_silence_not_a_factory_failure() {
    // Retail with no CD in the drive still boots: the transport opens,
    // the probes report no media, and selection/resume/service stay Ok
    // without starting playback or clearing the volume word.
    let empty = std::env::temp_dir().join(format!(
        "inf-no-cdaudio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&empty).unwrap();
    assert!(!empty.join("cdaudio").exists());
    let sdl = sdl2::init().unwrap();
    let subsystem = sdl.audio().unwrap();
    let assets = FrontendAssets::open(&data_root()).unwrap();
    let mut backend = PlatformAudio::open(&subsystem, &empty, &assets).unwrap();
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(backend.voices.device_state().enabled, 1);
    assert!(backend.voices.device_state().device.is_some());
    assert!(matches!(
        backend
            .voices
            .prepare_create(&mut FixturePlayback(&mut Mixer::default())),
        Err(crate::process_sound::VoiceExecutionError::Pool(
            crate::process_sound::VoicePoolFault::RequiredListInitialization
        ))
    ));
    backend.acquire_session_sounds(&mut sounds).unwrap();
    assert!(backend.acquire_session_sounds(&mut sounds).is_err());
    backend.open_cd().unwrap();
    backend.set_tmsf_format().unwrap();
    assert_eq!(backend.media_present().unwrap(), false);
    assert_eq!(backend.track_two_is_audio().unwrap(), false);
    let mut music = FrontendAudio::new();
    let mut volume = 8;
    music.select_track(7, &mut backend).unwrap();
    music.resume(&mut volume, &mut backend).unwrap();
    assert_eq!(volume, 8);
    let state = music.state();
    assert_eq!(state.selection.unwrap().track_word, 8);
    assert!(!state.active_selection);
    assert!(!state.audio_media_present);
    assert!(!state.stopped);
    music.service(&mut volume, &mut backend).unwrap();
    assert_eq!(volume, 8);
    assert!(!music.state().stopped);
    let resources = resident();
    sounds
        .create_persistent(&resources, 5, 65536, 65536, 0, || {
            Ok::<_, std::convert::Infallible>([0; 3])
        })
        .unwrap();
    // Prepare a real, silent SDL voice buffer without issuing Play. Shutdown
    // must clear both process records and remaining automatic/native voices.
    backend
        .create_voice(
            &ResidentSoundResource::capture(&resources, 5),
            [65536, 65536, 0, 1],
            None,
            15,
            false,
            &mut 0,
        )
        .unwrap();
    assert_eq!(backend.voices.live_len(), 1);
    assert_eq!(backend.voices.device_state().configuration, 1);
    let mut foreign = crate::process_sound::ProcessSoundRuntime::new(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    assert!(backend.shutdown_sounds(&mut foreign).is_err());
    assert_eq!(sounds.pending_len(), 1);
    assert_eq!(backend.voices.live_len(), 1);
    music.release(&mut backend).unwrap();
    backend.shutdown_sounds(&mut sounds).unwrap();
    assert!(sounds.is_empty());
    assert_eq!(backend.voices.live_len(), 0);
    assert!(backend.device.lock().effects.is_empty());
    assert_eq!(backend.voices.device_state().enabled, 1);
    assert_eq!(backend.voices.device_state().device, None);
    backend.shutdown_sounds(&mut sounds).unwrap();
    std::fs::remove_dir(&empty).unwrap();
}

#[test]
fn resident_pcm_survives_owner_drop_and_reaches_real_mixer() {
    let cue = resolve_sound(
        &ResidentSoundResource::capture(&resident(), 5),
        [65536, 65536, 0, 1],
        &mut 0,
        &mut AdmittedSoundSourceFixture::default(),
    )
    .unwrap()
    .unwrap();
    let mut mixer = Mixer {
        effects_volume: 15,
        effects: vec![EffectVoice {
            voice: crate::process_sound::ProcessSoundVoiceRef {
                owner: 1,
                serial: 1,
            },
            playing: true,
            looping: false,
            samples: cue.samples,
            position: 0.0,
            step: f64::from(frequency(cue.parameters[1])) / 44100.0,
            attenuation_hundredth_db: attenuation(cue.parameters[0], 15),
            pan: 0,
            gains: None,
            placed: None,
            low: 0.0,
            ears: Ears::default(),
        }],
        ..Mixer::default()
    };
    let mut output = [0.0; 4096];
    mixer.callback(&mut output);
    assert!(output.iter().any(|sample| sample.abs() > 0.01));
    assert!(mixer.stats.peak > 0.01);
}

#[test]
fn installed_frontend_track_has_exact_disc_duration_and_audible_samples() {
    let pcm = decode_track(&data_root().join("cdaudio/track08.ogg"), 8).unwrap();
    assert_eq!(pcm.len(), 16706 * 588 * 2);
    assert!(pcm.iter().any(|sample| sample.unsigned_abs() > 1000));
    assert_eq!(frames_to_msf(16706 * 588), 3 | (42 << 8) | (56 << 16));
    assert!(matches!(
        track_sectors(0x108),
        Err(PlatformAudioError::Track(0x108))
    ));
}

/// Explicit opt-in hardware test: no dummy device or mute fallback. SDL's
/// callback and queried TMSF positions must actually advance. Run serially:
/// cargo test -p inf-game platform_audio_device -- --ignored --test-threads=1
#[test]
#[ignore = "opens the actual default SDL audio device and plays original frontend audio"]
fn platform_audio_device_plays_pauses_resumes_original_music_and_cues() {
    let sdl = sdl2::init().unwrap();
    let subsystem = sdl.audio().unwrap();
    let assets = FrontendAssets::open(&data_root()).unwrap();
    let mut backend = PlatformAudio::open(&subsystem, &data_root(), &assets).unwrap();
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    backend.acquire_session_sounds(&mut sounds).unwrap();
    let mut music = FrontendAudio::new();
    let mut volume = 8;
    music.select_track(7, &mut backend).unwrap();
    music.resume(&mut volume, &mut backend).unwrap();
    let resident = resident();
    let cue = backend.play_resident_cue(&resident, 5, 10, &mut 0).unwrap();
    assert!(matches!(cue, CueReceipt::Started { .. }));
    std::thread::sleep(std::time::Duration::from_millis(300));
    let position = backend.position_tmsf().unwrap();
    assert_eq!(position & 0xff, 8);
    assert!(position >> 8 > 0);
    music.pause(&mut backend).unwrap();
    let saved = music.state().selection.unwrap().saved_tmsf;
    assert_eq!(saved & 0xff, 8);
    music.resume(&mut volume, &mut backend).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(backend.position_tmsf().unwrap() > saved);
    assert!(backend.stats().music_frames > 0);
    assert!(backend.stats().effect_frames > 0);
    assert!(backend.stats().peak > 0.01);
    music.release(&mut backend).unwrap();
}

/// A track of `frames` at half scale.
fn music_voice(track: u8, frames: usize) -> MusicVoice {
    MusicVoice {
        track,
        samples: vec![16384; 2 * frames].into(),
        frame: 0,
        playing: true,
    }
}

/// The Alien Brain's hushed track runs its frames in silence to its end, so
/// the CD's position and end stay retail's; another track mixes exactly as
/// with no hush.
#[test]
fn a_hushed_track_runs_silent_and_others_mix_as_retail() {
    let mixer = |hushed_track| Mixer {
        cd_volume: [1.0; 2],
        hushed_track,
        ..Mixer::default()
    };
    // `play_range`'s start: a voice of the hushed track starts hushed.
    let mut hushed = mixer(Some(5));
    hushed.start_music(music_voice(5, 4));
    assert!(mix(&mut hushed, 4).iter().all(|&sample| sample == 0.0));
    assert_eq!(hushed.stats.music_starts, 1);
    assert_eq!(hushed.stats.music_frames, 4);
    assert!(hushed.completion);
    let mut retail = mixer(None);
    retail.start_music(music_voice(8, 4));
    let retail = mix(&mut retail, 4);
    assert!(retail.iter().any(|&sample| sample != 0.0));
    let mut other = mixer(Some(5));
    other.start_music(music_voice(8, 4));
    assert_eq!(mix(&mut other, 4), retail);
}

/// A hush that comes or goes while its track plays ramps 1/1024 a frame:
/// down to silence over 1,024 frames, and up again as it lifts.
#[test]
fn the_hush_ramps_over_1024_frames() {
    for (from, to) in [(None, Some(5)), (Some(5), None)] {
        let mut mixer = Mixer {
            cd_volume: [1.0; 2],
            hushed_track: from,
            ..Mixer::default()
        };
        mixer.start_music(music_voice(5, 2_000));
        mixer.hushed_track = to;
        let output = mix(&mut mixer, 1_100);
        for (frame, pair) in output.chunks_exact(2).enumerate() {
            let ramp = ((frame + 1) as f32 / 1024.0).min(1.0);
            let gain = if to.is_some() { 1.0 - ramp } else { ramp };
            assert!(
                pair.iter()
                    .all(|&sample| (sample - 0.5 * gain).abs() < 1e-6),
                "{from:?} to {to:?}, frame {frame}: {pair:?}"
            );
        }
    }
}

#[test]
fn title_voice_upsamples_stereo_frames_with_linear_interp() {
    let mut mixer = Mixer {
        title: Some(TitleVoice {
            samples: vec![0, 0, 1000, -1000].into(),
            position: 0.0,
        }),
        ..Mixer::default()
    };
    // Two 22050 Hz input frames render as four 44100 Hz output frames.
    let output = mix(&mut mixer, 4);
    let pcm = |frame: usize, channel: usize| (output[frame * 2 + channel] * 32768.0).round();
    assert_eq!((pcm(0, 0), pcm(0, 1)), (0.0, 0.0));
    assert_eq!((pcm(1, 0), pcm(1, 1)), (500.0, -500.0));
    assert_eq!((pcm(2, 0), pcm(2, 1)), (1000.0, -1000.0));
    // The final output frame holds the last input frame: the beyond-end
    // read falls back instead of inventing samples.
    assert_eq!((pcm(3, 0), pcm(3, 1)), (1000.0, -1000.0));
    assert_eq!(mixer.stats.title_frames, 4);
    assert!(mixer.title.is_none());
    assert!(mixer.stats.peak > 0.01);
    // A spent voice stays silent without consuming further frames.
    assert_eq!(mix(&mut mixer, 2), [0.0; 4]);
    assert_eq!(mixer.stats.title_frames, 4);
}

#[test]
fn title_voice_empty_completes_silently() {
    let mut mixer = Mixer {
        title: Some(TitleVoice {
            samples: Vec::new().into(),
            position: 0.0,
        }),
        ..Mixer::default()
    };
    let mut output = [1.0f32; 4];
    mixer.callback(&mut output);
    assert_eq!(output, [0.0; 4]);
    assert!(mixer.title.is_none());
    assert_eq!(mixer.stats.title_frames, 0);
}

#[test]
fn title_voice_replaces_and_clears() {
    let mut mixer = Mixer {
        title: Some(TitleVoice {
            samples: vec![1000, 1000].into(),
            position: 0.0,
        }),
        ..Mixer::default()
    };
    mixer.title = Some(TitleVoice {
        samples: vec![-1000, -1000].into(),
        position: 0.0,
    });
    let output = mix(&mut mixer, 1);
    assert!((output[0] * 32768.0).round() == -1000.0);
    mixer.title = None;
    assert_eq!(mix(&mut mixer, 1), [0.0; 2]);
}

#[test]
fn original_h9_slot64_is_direct_pcm_and_retained_allocations_survive_release_reload() {
    use sha2::{Digest, Sha256};
    let mut resident = resident();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let actual = resident.resource(9, 64).unwrap();
    assert_eq!(
        (
            actual.registration.level,
            actual.registration.local_index,
            actual.registration.region_index
        ),
        (1, 34, 2)
    );
    assert_eq!(
        actual
            .overlay
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_ascii_lowercase(),
        "1.ovl"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(actual.overlay.bytes())),
        "aeefe3f312118008f9a3379e41927b8dd71a749345706503b0abe17860cc7ce7"
    );
    let HandlerResources::Sounds(bank) = &actual.overlay.handlers()[2] else {
        panic!("actual H9")
    };
    let entry = &bank.entries[34];
    assert_eq!(entry.fields_04_to_10, [1226336, 2438, 0, 0]);
    let SoundEntryKind::Pcm { data, .. } = entry.kind else {
        panic!("slot64 is original direct PCM, not alias")
    };
    let bytes = data.get(actual.overlay.bytes()).unwrap();
    assert_eq!(bytes.len(), 2438);
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "0c790bfa77d0e0ce6ac784301ff20b92a0c78264eb35eead3a024413e2bcbf2b"
    );
    let old = ResidentSoundResource::capture(&resident, 64);
    let alias = ResidentSoundResource::capture(&resident, 31);
    let mut source = AdmittedSoundSourceFixture::default();
    let mut seed = 0;
    let first = resolve_sound(&old, [65536, 65536, 0, 0], &mut seed, &mut source)
        .unwrap()
        .unwrap();
    let repeated = resolve_sound(&old, [65536, 65536, 0, 0], &mut seed, &mut source)
        .unwrap()
        .unwrap();
    assert_eq!(
        (first.slot, first.samples.len(), first.alias_count, seed),
        (64, 1219, 0, 0)
    );
    assert!(Arc::ptr_eq(&first.samples, &repeated.samples));
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert!(resident.resource(9, 64).is_none());
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let new = ResidentSoundResource::capture(&resident, 64);
    assert!(!Arc::ptr_eq(
        &old.allocation.as_ref().unwrap().0,
        &new.allocation.as_ref().unwrap().0
    ));
    let replacement = resolve_sound(&new, [65536, 65536, 0, 0], &mut seed, &mut source)
        .unwrap()
        .unwrap();
    assert!(!Arc::ptr_eq(&first.samples, &replacement.samples));
    assert_eq!(first.samples.as_ref(), replacement.samples.as_ref());
    drop(resident);
    let retained = resolve_sound(&alias, [65536, 65536, 0, 0], &mut seed, &mut source)
        .unwrap()
        .unwrap();
    assert_eq!(
        (retained.slot, retained.alias_count, seed),
        (30, 1, 0x13d573)
    );
    //No authored alias crosses an allocation in this corpus. A future cross-
    //allocation pointer fixup remains required; current residency never retargets
    //either of these captured immutable allocations after release/reload.
}
#[test]
fn actual_slot64_persistent_voice_loops_updates_and_synchronous_delete_uses_same_owner() {
    use crate::process_sound::{
        ProcessSoundListener, ProcessSoundRecordState, ProcessSoundRuntime,
        ProcessSoundTeardownLoan,
    };
    let mut resident = resident();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let raw = std::fs::read(data_root().join("Infest.exe")).unwrap();
    let mut sounds = ProcessSoundRuntime::new(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    let record = sounds
        .create_persistent(&resident, 64, 65536, 65536, 1, || Ok::<_, &str>([1, 2, 3]))
        .unwrap();
    let listener = ProcessSoundListener {
        view: crate::frontend_scene::SceneView {
            eye: [0; 3],
            basis: crate::menu::MenuBasis::INITIAL,
        },
        phase: 4,
        suppressed: false,
        water: None,
    };
    let mut audio = Audio::new();
    let mut rng = 0;
    assert_eq!(
        sounds
            .service_world_draw(listener, &mut audio, 15, &mut rng)
            .unwrap(),
        1
    );
    assert_eq!(rng, 0);
    let voice = sounds
        .records()
        .snapshot(record)
        .unwrap()
        .voice
        .unwrap()
        .unwrap();
    assert_eq!(audio.voices.head(), Some(voice));
    assert!(audio.voices.snapshot(voice).unwrap().looping);
    let mut output = [0.0; 8192];
    audio.mixer.callback(&mut output);
    assert!(output.iter().any(|v| *v != 0.0));
    assert!(audio.mixer.effects[0].playing);
    let position = audio.mixer.effects[0].position;
    sounds
        .records_mut()
        .update(
            record,
            32768,
            32768,
            None::<fn() -> Result<[u16; 3], &'static str>>,
        )
        .unwrap();
    assert_eq!(
        sounds
            .service_world_draw(listener, &mut audio, 7, &mut rng)
            .unwrap(),
        0
    );
    assert_eq!(audio.mixer.effects[0].position, position);
    assert_eq!(
        audio.voices.snapshot(voice).unwrap().parameters.pitch,
        32768
    );
    assert_eq!(audio.voices.snapshot(voice).unwrap().counter, -1);
    sounds
        .delete_record(record, &mut ProcessSoundTeardownLoan::Available(&mut audio))
        .unwrap();
    assert!(audio.mixer.effects.is_empty());
    assert_eq!(audio.voices.live_len(), 0);
    assert_eq!(
        sounds.records().snapshot(record).unwrap().state,
        ProcessSoundRecordState::Released
    );
}
#[test]
fn missing_teardown_stops_at_nonnull_voice_after_real_record_unlink() {
    use crate::process_sound::{
        ProcessSoundListener, ProcessSoundRecordError, ProcessSoundRecordState,
        ProcessSoundRuntime, ProcessSoundTeardownLoan, ProcessSoundVoiceOperation,
    };
    let mut resident = resident();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let raw = std::fs::read(data_root().join("Infest.exe")).unwrap();
    let mut sounds = ProcessSoundRuntime::new(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    let unstarted = sounds
        .create_persistent(&resident, 64, 1, 1, 0, || Ok::<_, &str>([0; 3]))
        .unwrap();
    sounds
        .delete_record(unstarted, &mut ProcessSoundTeardownLoan::RequiredAtReach)
        .unwrap();
    let record = sounds
        .create_persistent(&resident, 64, 65536, 65536, 1, || Ok::<_, &str>([0; 3]))
        .unwrap();
    let listener = ProcessSoundListener {
        view: crate::frontend_scene::SceneView {
            eye: [0; 3],
            basis: crate::menu::MenuBasis::INITIAL,
        },
        phase: 4,
        suppressed: false,
        water: None,
    };
    let mut audio = Audio::new();
    sounds
        .service_world_draw(listener, &mut audio, 15, &mut 0)
        .unwrap();
    let voice = sounds.records().snapshot(record).unwrap().voice;
    assert_eq!(
        sounds.delete_record(record, &mut ProcessSoundTeardownLoan::RequiredAtReach),
        Err(ProcessSoundRecordError::RequiredVoice {
            record,
            operation: ProcessSoundVoiceOperation::Delete
        })
    );
    let row = sounds.records().snapshot(record).unwrap();
    assert_eq!(row.state, ProcessSoundRecordState::Detached);
    assert_eq!(row.voice, voice);
    assert_eq!(row.retained_bytes, 40);
    assert_eq!(audio.voices.live_len(), 1);
    assert_eq!(audio.mixer.effects.len(), 1);
}
#[test]
fn gain_publication_is_per_voice_and_menu_reattenuation_does_not_restart_or_touch_music() {
    let resident = resident();
    let resource = ResidentSoundResource::capture(&resident, 2);
    let mut audio = Audio::new();
    let (a, _) = create_voice_with(
        VoiceSourceLoans {
            voices: &mut audio.voices,
            pcm: &mut audio.pcm,
            backend: &mut FixturePlayback(&mut audio.mixer),
        },
        VoiceSourceRequest {
            resource: &resource,
            parameters: [65536, 65536, 0, 0],
            master: 15,
            looping: true,
            rng: &mut 0,
            placement: None,
        },
    )
    .unwrap();
    let crate::process_sound::VoicePublication::Written(a) = a else {
        panic!("real PCM")
    };
    audio
        .voices
        .start(Some(a), &mut FixturePlayback(&mut audio.mixer))
        .unwrap();
    let old_gain = audio.mixer.effects[0].attenuation_hundredth_db;
    let (c, _) = create_voice_with(
        VoiceSourceLoans {
            voices: &mut audio.voices,
            pcm: &mut audio.pcm,
            backend: &mut FixturePlayback(&mut audio.mixer),
        },
        VoiceSourceRequest {
            resource: &resource,
            parameters: [32768, 32768, 0, 0],
            master: 7,
            looping: false,
            rng: &mut 0,
            placement: None,
        },
    )
    .unwrap();
    let crate::process_sound::VoicePublication::Written(c) = c else {
        panic!("real PCM")
    };
    audio
        .voices
        .start_one_shot(c, &mut FixturePlayback(&mut audio.mixer))
        .unwrap();
    assert_eq!(audio.mixer.effects[0].attenuation_hundredth_db, old_gain);
    audio.mixer.callback(&mut [0.0; 64]);
    let positions = audio
        .mixer
        .effects
        .iter()
        .map(|v| v.position)
        .collect::<Vec<_>>();
    let states = [
        audio.voices.snapshot(a).unwrap(),
        audio.voices.snapshot(c).unwrap(),
    ];
    let music = audio.mixer.cd_volume;
    audio.mixer.effects_volume = 1;
    audio
        .voices
        .reattenuate(1, &mut FixturePlayback(&mut audio.mixer))
        .unwrap();
    assert_eq!(
        audio
            .mixer
            .effects
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>(),
        positions
    );
    assert_eq!(
        [
            audio.voices.snapshot(a).unwrap(),
            audio.voices.snapshot(c).unwrap()
        ],
        states
    );
    assert_eq!(audio.mixer.cd_volume, music);
    assert_eq!(
        audio.mixer.effects[0].attenuation_hundredth_db,
        attenuation(65536, 1)
    );
    assert_eq!(
        audio.mixer.effects[1].attenuation_hundredth_db,
        attenuation(32768, 1)
    );
}

#[test]
fn complete_original_h9_alias_corpus_stays_in_its_authentic_allocation() {
    use inf_formats::RequestKind;
    let installation = Installation::open(&data_root()).unwrap();
    let mut declarations = Vec::new();
    for leaf in installation.preload().leaves() {
        for phase in [
            RequestKind::Base,
            RequestKind::Difficulty,
            RequestKind::Language,
            RequestKind::DifficultyLanguage,
        ] {
            let request = leaf.requests(phase).get(9).unwrap();
            if request.count != 0 {
                declarations.push((leaf.overlay_id, phase, request.count));
            }
        }
    }
    assert_eq!(
        declarations,
        [(0, RequestKind::Base, 30), (1, RequestKind::Base, 47)]
    );
    let mut resident = ResidentRuntime::new(installation).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    for (level, source_slot, expected_count) in [(0, 2, 30), (1, 64, 47)] {
        let source = resident.resource(9, source_slot).unwrap();
        assert_eq!(source.registration.level, level);
        let HandlerResources::Sounds(bank) =
            &source.overlay.handlers()[source.registration.region_index]
        else {
            panic!("actual H9")
        };
        assert_eq!(bank.entries.len(), expected_count);
        let mapping = source.overlay.requests().get(9).unwrap();
        for entry in &bank.entries {
            if let SoundEntryKind::Alias {
                target_global_slot, ..
            } = entry.kind
            {
                let target = u16::try_from(target_global_slot).unwrap();
                let local = (0..bank.entries.len())
                    .rev()
                    .find(|i| mapping.global_index(*i as u16) == Some(target))
                    .expect("authored alias stays in this immutable allocation");
                let actual = resident.resource(9, target).unwrap();
                assert!(Arc::ptr_eq(source.overlay, actual.overlay));
                assert_eq!(actual.registration.local_index, local as u16);
            }
        }
    }
    //The native loader can fix aliases through a different global allocation.
    //This original corpus supplies no such alias; that service remains typed
    //AliasCustody rather than a current-table substitution or fabricated NULL.
}

#[test]
fn a_muted_mixer_mixes_but_sends_silence() {
    let mut mixer = Mixer {
        title: Some(TitleVoice {
            samples: Arc::from(vec![16384_i16; 512]),
            position: 0.0,
        }),
        muted: true,
        ..Mixer::default()
    };
    let mut output = [1.0_f32; 2 * (LIMITER_AHEAD + 8)];
    mixer.callback(&mut output);
    assert!(output.iter().all(|sample| *sample == 0.0));
    assert_eq!(mixer.stats.title_frames, (LIMITER_AHEAD + 8) as u64);
    assert!(mixer.stats.peak > 0.4);
}

fn ambience_voice(id: u32, samples: Vec<i16>, channels: u8, looping: bool) -> AmbienceVoice {
    AmbienceVoice {
        id,
        sound: Arc::new(remastered_pak::DecodedSound { channels, samples }),
        position: 0,
        looping,
        gain: 1.0,
        target: 1.0,
        pan: 0.0,
        target_pan: 0.0,
        stopping: false,
        step: 1.0,
        target_step: 1.0,
        fraction: 0.0,
        cut: 1.0,
        target_cut: 1.0,
        low: [0.0; 2],
        send: 0.0,
        aim: None,
        target_aim: None,
        ears: Ears::default(),
        effect: false,
    }
}

/// At Ambience 0 the bus is silent: the mix is retail's, sample for sample
/// (the limiter idle, under full scale).
#[test]
fn ambience_at_zero_leaves_retails_mix_alone() {
    let title = || {
        Some(TitleVoice {
            samples: Arc::from(
                (0..256)
                    .map(|i| (i * 251 % 65536) as i16)
                    .collect::<Vec<_>>(),
            ),
            position: 0.0,
        })
    };
    let mut plain = Mixer {
        title: title(),
        ..Mixer::default()
    };
    let mut with_bus = Mixer {
        title: title(),
        ambience: vec![ambience_voice(1, vec![30000; 512], 1, true)],
        ambience_volume: 0.0,
        ..Mixer::default()
    };
    let (mut a, mut b) = ([0.0_f32; 128], [0.0_f32; 128]);
    plain.callback(&mut a);
    with_bus.callback(&mut b);
    assert_eq!(a, b);
}

/// A looping voice crossfades its end into its start: a ramp that jumps
/// back at the end of the sound plays on with no jump at its seam.
#[test]
fn a_looping_ambience_voice_has_no_seam() {
    let frames = 4000_usize;
    let ramp: Vec<i16> = (0..frames).map(|i| (i * 8) as i16).collect();
    let mut voice = ambience_voice(1, ramp, 1, true);
    let mut last: Option<f32> = None;
    let mut largest = 0.0_f32;
    for _ in 0..frames * 3 {
        let [left, _] = voice.next(false).expect("a loop never ends");
        if let Some(previous) = last {
            largest = largest.max((left - previous).abs());
        }
        last = Some(left);
    }
    // The raw wrap would jump about 0.98; the seam stays within a few steps.
    assert!(largest < 0.01, "a jump of {largest}");
}

/// A stopped voice fades out within a buffer and leaves the bus.
#[test]
fn a_stopped_ambience_voice_fades_and_goes() {
    let mut mixer = Mixer {
        ambience: vec![ambience_voice(7, vec![16000; 20000], 1, true)],
        ambience_volume: 1.0,
        ..Mixer::default()
    };
    mixer.ambience[0].stopping = true;
    mixer.ambience[0].target = 0.0;
    let output = mix(&mut mixer, 1100);
    assert!(output[0].abs() > 0.1, "it fades rather than cuts");
    assert!(mixer.ambience.is_empty());
}

/// A voice an octave up reads two frames a mix frame; half that, it reads
/// between frames.
#[test]
fn a_pitched_ambience_voice_reads_faster_or_between_frames() {
    let ramp: Vec<i16> = (0..4000).map(|i| (i * 8) as i16).collect();
    let mut up = ambience_voice(1, ramp.clone(), 1, false);
    (up.step, up.target_step) = (2.0, 2.0);
    let mut down = ambience_voice(2, ramp, 1, false);
    (down.step, down.target_step) = (0.5, 0.5);
    for _ in 0..100 {
        up.next(false);
        down.next(false);
    }
    assert_eq!((up.position, down.position), (200, 50));
    // Down an octave, the next frame falls halfway between two.
    let [left, _] = down.next(false).unwrap();
    let pan = std::f32::consts::FRAC_PI_4.cos();
    assert!((left - (50.0 * 8.0) / 32768.0 * pan).abs() < 1.0e-6);
    let [left, _] = down.next(false).unwrap();
    assert!((left - (50.5 * 8.0) / 32768.0 * pan).abs() < 1.0e-6);
}

/// A dulled voice smooths a jump; an open one passes it as it is.
#[test]
fn a_dulled_ambience_voice_loses_its_highs() {
    let square: Vec<i16> = (0..4000)
        .map(|i| if i % 2 == 0 { 16000 } else { -16000 })
        .collect();
    let mut open = ambience_voice(1, square.clone(), 1, false);
    let mut dull = ambience_voice(2, square, 1, false);
    (dull.cut, dull.target_cut) = (0.1, 0.1);
    let swing = |voice: &mut AmbienceVoice| {
        let samples: Vec<f32> = (0..200).map(|_| voice.next(false).unwrap()[0]).collect();
        samples[100..]
            .iter()
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    let (open, dull) = (swing(&mut open), swing(&mut dull));
    assert!((open - 16000.0 / 32768.0 * std::f32::consts::FRAC_PI_4.cos()).abs() < 1.0e-6);
    assert!(dull < open * 0.1, "{dull} against {open}");
}

/// A voice sending to the reverb leaves a tail after it ends; one sending
/// nothing leaves silence.
#[test]
fn a_sent_voice_leaves_a_tail() {
    let tail = |send: f32| {
        let mut voice = ambience_voice(1, vec![16000; 2000], 1, false);
        voice.send = send;
        let mut mixer = Mixer {
            ambience: vec![voice],
            ambience_volume: 1.0,
            ..Mixer::default()
        };
        mixer.reverb.shape(reverb::ReverbShape {
            seconds: 1.0,
            damping: 0.3,
            wet: 1.0,
        });
        let mut output = vec![0.0_f32; 2 * 8000];
        mixer.callback(&mut output);
        // Long after the voice's 2,000 frames.
        output[2 * 6000..]
            .iter()
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    assert!(tail(1.0) > 1.0e-3);
    assert_eq!(tail(0.0), 0.0);
}

/// One of retail's positional sounds, `rotated` from the camera, played
/// through a mixer at Ambience `level`: its output.
fn placed_effect(rotated: [i16; 3], under_water: bool, level: f32) -> Vec<f32> {
    let placement = crate::process_sound::SoundPlacement {
        rotated,
        under_water,
    };
    let mut mixer = Mixer {
        effects: vec![EffectVoice {
            voice: crate::process_sound::ProcessSoundVoiceRef {
                owner: 1,
                serial: 1,
            },
            playing: true,
            looping: true,
            samples: (0..64)
                .map(|i| if i % 2 == 0 { 12_000 } else { -12_000 })
                .collect::<Vec<i16>>()
                .into(),
            position: 0.0,
            step: 1.0,
            attenuation_hundredth_db: 0,
            pan: if rotated[0] > 0 { 9999 } else { 0 },
            gains: None,
            placed: Some(PlacedEffect::new(placement)),
            low: 0.0,
            ears: Ears::default(),
        }],
        ambience_volume: level,
        ..Mixer::default()
    };
    let mut output = vec![0.0_f32; 2 * 512];
    mixer.callback(&mut output);
    output
}

/// At Ambience 0 a placed sound of retail's is retail's, sample for sample;
/// above it, one to the right pans by its direction at constant power, as
/// loud as retail's in the middle, and one under the water is muffled.
#[test]
fn retails_sounds_are_placed_only_under_remastered() {
    let peak = |output: &[f32], channel: usize| {
        output[2 * 256..]
            .iter()
            .skip(channel)
            .step_by(2)
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    let mut plain = placed_effect([4096, 0, 0], false, 0.0);
    let mut retail = Mixer {
        effects: vec![EffectVoice {
            voice: crate::process_sound::ProcessSoundVoiceRef {
                owner: 1,
                serial: 1,
            },
            playing: true,
            looping: true,
            samples: (0..64)
                .map(|i| if i % 2 == 0 { 12_000 } else { -12_000 })
                .collect::<Vec<i16>>()
                .into(),
            position: 0.0,
            step: 1.0,
            attenuation_hundredth_db: 0,
            pan: 9999,
            gains: None,
            placed: None,
            low: 0.0,
            ears: Ears::default(),
        }],
        ..Mixer::default()
    };
    let mut expected = vec![0.0_f32; 2 * 512];
    retail.callback(&mut expected);
    assert_eq!(plain, expected);
    plain = placed_effect([4096, 0, 400], false, 1.0);
    assert!(peak(&plain, 1) > 2.0 * peak(&plain, 0), "to the right");
    let middle = placed_effect([0, 0, 2000], false, 1.0);
    assert!((peak(&middle, 0) - peak(&middle, 1)).abs() < 1.0e-6);
    let under = placed_effect([0, 0, 2000], true, 1.0);
    assert!(peak(&under, 0) < 0.2 * peak(&middle, 0), "muffled");
}

/// Inside the vehicle's shell (first person) the ambience is quieter and
/// duller: a fast square wave comes through much weaker.
#[test]
fn the_shell_dulls_the_ambience() {
    let swing = |shell: bool| {
        let square: Vec<i16> = (0..4000)
            .map(|i| if i % 2 == 0 { 16000 } else { -16000 })
            .collect();
        let mut mixer = Mixer {
            ambience: vec![ambience_voice(1, square, 1, true)],
            ambience_volume: 1.0,
            shell,
            ..Mixer::default()
        };
        let mut output = vec![0.0_f32; 2 * 2000];
        mixer.callback(&mut output);
        output[2 * 1000..]
            .iter()
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    assert!(swing(true) < 0.5 * swing(false));
}

/// Headphones: a click to the right reaches the right ear at once and the
/// left 0.66 ms later, in the head's shadow; a tone from behind is duller
/// than from ahead; a hum turned to the side keeps its loudness.
#[test]
fn the_ears_hear_a_placed_sound_apart() {
    let mut ears = Ears::default();
    let click: Vec<[f32; 2]> = (0..64)
        .map(|i| ears.hear(if i == 0 { 1.0 } else { 0.0 }, [1.0, 1.0]))
        .collect();
    let first = |ear: usize| click.iter().position(|frame| frame[ear].abs() > 1.0e-6);
    assert_eq!((first(1), first(0)), (Some(0), Some(29)));
    let peak = |ear: usize| {
        click
            .iter()
            .fold(0.0_f32, |most, frame| most.max(frame[ear].abs()))
    };
    assert!(peak(0) < 0.5 * peak(1), "the left ear in the head's shadow");
    let tone = |ahead: f32| {
        let mut ears = Ears::default();
        (0..2000)
            .map(|i| ears.hear(if i % 2 == 0 { 1.0 } else { -1.0 }, [0.0, ahead])[0])
            .skip(1000)
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    assert!(tone(-1.0) < 0.6 * tone(1.0), "behind is duller");
    // A 200 Hz hum keeps its loudness, both ears together, as it goes from
    // ahead to the side: within 1 dB.
    let power = |side: f32| {
        let mut ears = Ears::default();
        (0..8820)
            .map(|i| {
                let hum = (std::f32::consts::TAU * 200.0 * i as f32 / 44_100.0).sin();
                ears.hear(hum, [side, 0.0])
            })
            .skip(4410)
            .map(|[l, r]| l * l + r * r)
            .sum::<f32>()
    };
    let turned = 10.0 * (power(1.0) / power(0.0)).log10();
    assert!(turned.abs() < 1.0, "{turned} dB at the side");
}

/// An aimed voice goes through the ears only with headphones; without,
/// its pan places it at constant power as before.
#[test]
fn an_aimed_voice_takes_the_ears_with_headphones() {
    let mix = |headphones: bool| {
        let mut voice = ambience_voice(1, vec![12000; 4000], 1, true);
        (voice.aim, voice.target_aim) = (Some([1.0, 1.0]), Some([1.0, 1.0]));
        let mut mixer = Mixer {
            ambience: vec![voice],
            ambience_volume: 1.0,
            headphones,
            ..Mixer::default()
        };
        let mut output = vec![0.0_f32; 2 * 512];
        mixer.callback(&mut output);
        [output[2 * 400], output[2 * 400 + 1]]
    };
    let [left, right] = mix(false);
    assert!((left - right).abs() < 1.0e-6, "the pan, 0: the middle");
    let [left, right] = mix(true);
    assert!(right > left * 1.2, "the right ear louder: {left} {right}");
}

/// The limiter keeps a loud bus over retail's mix under its ceiling.
#[test]
fn the_limiter_holds_the_ceiling() {
    let mut mixer = Mixer {
        title: Some(TitleVoice {
            samples: Arc::from(vec![30000_i16; 4096]),
            position: 0.0,
        }),
        ambience: vec![ambience_voice(1, vec![30000; 8192], 1, true)],
        ambience_volume: 1.0,
        ..Mixer::default()
    };
    let mut output = [0.0_f32; 2048];
    mixer.callback(&mut output);
    assert!(output
        .iter()
        .all(|sample| sample.abs() <= LIMITER_CEILING + 1.0e-6));
}

/// The limiter leaves a mix under full scale untouched, only delayed; it
/// holds a burst past full scale to it, easing in ahead of the burst rather
/// than cutting it; and it lets go once the burst has passed.
#[test]
fn the_limiter_only_touches_what_would_clip() {
    let mut limiter = Limiter::default();
    let quiet: Vec<[f32; 2]> = (0..300)
        .map(|i| [(i as f32 * 0.1).sin() * 0.9, (i as f32 * 0.07).cos() * 0.9])
        .collect();
    let out: Vec<[f32; 2]> = quiet.iter().map(|frame| limiter.next(*frame)).collect();
    let delay = LIMITER_AHEAD - 1;
    assert_eq!(out[delay..], quiet[..quiet.len() - delay]);
    // A burst at twice full scale, between quiet stretches.
    let mut limiter = Limiter::default();
    let input: Vec<[f32; 2]> = (0..3500)
        .map(|i| {
            let loud = if (500..700).contains(&i) { 2.0 } else { 0.5 };
            [
                loud * (i as f32 * 0.05).sin(),
                loud * (i as f32 * 0.05).cos(),
            ]
        })
        .collect();
    let out: Vec<[f32; 2]> = input.iter().map(|frame| limiter.next(*frame)).collect();
    assert!(out
        .iter()
        .flatten()
        .all(|sample| sample.abs() <= LIMITER_CEILING + 1.0e-5));
    // The burst comes through at full scale, its shape kept (scaled, not cut).
    let peak = out
        .iter()
        .flatten()
        .fold(0.0_f32, |most, sample| most.max(sample.abs()));
    assert!(peak > 0.95, "{peak}");
    // From half its gain it is back in 0.05 s: the quiet stretch at its own
    // level again.
    let last = out[3499];
    let source = input[3499 - delay];
    assert!((last[0] - source[0]).abs() < 1.0e-6 && (last[1] - source[1]).abs() < 1.0e-6);
}

/// The Ambience volume follows retail's Sound curve: full at 15, silent at
/// 0, well down at the default 4.
#[test]
fn the_ambience_volume_takes_retails_curve() {
    assert_eq!(ambience_gain(0), 0.0);
    assert!((ambience_gain(15) - 1.0).abs() < 1.0e-6);
    let default = ambience_gain(crate::modern_settings::DEFAULT_AMBIENCE);
    assert!(default > 0.0 && default < ambience_gain(10), "{default}");
    assert!(ambience_gain(1) < ambience_gain(2));
}

/// The levels the event log takes: a mono voice at half scale, panned to
/// the middle, is 9 dB down on each side; retail's buses are silent.
#[test]
fn the_buses_levels_add_up_each_bus_apart() {
    let mut mixer = Mixer {
        ambience: vec![ambience_voice(1, vec![16384; 4096], 1, true)],
        ambience_volume: 1.0,
        ..Mixer::default()
    };
    let mut output = [0.0_f32; 2 * 1024];
    mixer.callback(&mut output);
    let levels = mixer.levels.levels().unwrap();
    assert!((levels.ambience + 9.03).abs() < 0.05, "{levels:?}");
    assert_eq!(
        (levels.effects, levels.music, levels.dip),
        (-120.0, -120.0, 0.0)
    );
    assert_eq!(LevelSums::default().levels(), None);
}

/// The bed dips under a loud effect and comes back after it.
#[test]
fn the_ambience_ducks_under_loud_effects() {
    let bed = || ambience_voice(1, vec![8000; 200_000], 1, true);
    let effect = |samples: Vec<i16>| EffectVoice {
        voice: crate::process_sound::ProcessSoundVoiceRef {
            owner: 1,
            serial: 1,
        },
        playing: true,
        looping: false,
        samples: samples.into(),
        position: 0.0,
        step: 1.0,
        attenuation_hundredth_db: 0,
        pan: 0,
        gains: None,
        placed: None,
        low: 0.0,
        ears: Ears::default(),
    };
    let level = |with_blast: bool| {
        let mut mixer = Mixer {
            ambience: vec![bed()],
            ambience_volume: 1.0,
            effects: with_blast
                .then(|| effect(vec![30000; 512]))
                .into_iter()
                .collect(),
            ..Mixer::default()
        };
        let mut output = vec![0.0_f32; 2 * 4096];
        mixer.callback(&mut output);
        // Just after the blast ends: the bed alone, still dipped.
        output[2 * 600]
    };
    let (quiet, after_blast) = (level(false), level(true));
    assert!(after_blast < quiet * 0.7, "{after_blast} against {quiet}");
}

/// Port option: under the water the game's sound is muffled, ramping in,
/// and the music stays as it was.
#[test]
fn under_the_water_the_game_is_muffled_and_the_music_kept() {
    let rate = OUTPUT_RATE as f32;
    let cut = 1.0 - (-std::f32::consts::TAU * UNDER_WATER_HERTZ / rate).exp();
    let step = 1.0 - (-1.0 / (UNDER_WATER_SECONDS * rate)).exp();
    // A 5 kHz tone as the game, a steady level as the music, for a second.
    let run = |mixer: &mut Mixer| {
        let mut loudest = 0.0_f32;
        for index in 0..OUTPUT_RATE as usize {
            let tone = (std::f32::consts::TAU * 5000.0 * index as f32 / rate).sin() * 0.5;
            let mut frame = [tone + 0.25, tone + 0.25];
            mixer.muffle(&mut frame, [0.25; 2], cut, step);
            if index > OUTPUT_RATE as usize / 2 {
                loudest = loudest.max((frame[0] - 0.25).abs());
            }
        }
        loudest
    };
    let mut mixer = Mixer::default();
    // Above the water nothing changes.
    assert!((run(&mut mixer) - 0.5).abs() < 1.0e-3);
    mixer.under_water = true;
    let muffled = run(&mut mixer);
    assert!(muffled < 0.1, "{muffled}");
    // The music's steady level passes untouched: the tone's swing stays
    // centred on it.
    let mut frame = [0.25, 0.25];
    mixer.muffle(&mut frame, [0.25; 2], cut, step);
    assert!((frame[0] - 0.25).abs() < 0.1);
}
