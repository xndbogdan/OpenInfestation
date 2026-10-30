use std::collections::VecDeque;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Open,
    Format,
    Close,
    Present,
    TrackTwo,
    Mode,
    Position,
    Length(u32),
    Play(CdPlayRange),
    Stop,
    Volume(u32),
    Clock,
}

struct Backend {
    calls: Vec<Call>,
    modes: VecDeque<CdPlaybackMode>,
    present: bool,
    track_two_audio: bool,
    position: u32,
    length: Result<u32, &'static str>,
    now: i32,
    fail: Option<AudioOperation>,
}

impl Default for Backend {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            modes: VecDeque::new(),
            present: true,
            track_two_audio: true,
            position: 0x0302_0108,
            length: Ok(0x0000_0201), // one minute and two seconds, plus native margin
            now: 100,
            fail: None,
        }
    }
}

impl Backend {
    fn record(&mut self, call: Call, operation: AudioOperation) -> Result<(), &'static str> {
        self.calls.push(call);
        if self.fail == Some(operation) {
            Err("transport failure")
        } else {
            Ok(())
        }
    }
}

impl FrontendAudioBackend for Backend {
    type Error = &'static str;

    fn open_cd(&mut self) -> Result<(), Self::Error> {
        self.record(Call::Open, AudioOperation::Open)
    }
    fn set_tmsf_format(&mut self) -> Result<(), Self::Error> {
        self.record(Call::Format, AudioOperation::SetTimeFormat)
    }
    fn close_cd(&mut self) -> Result<(), Self::Error> {
        self.record(Call::Close, AudioOperation::Close)
    }
    fn media_present(&mut self) -> Result<bool, Self::Error> {
        self.record(Call::Present, AudioOperation::MediaPresent)?;
        Ok(self.present)
    }
    fn track_two_is_audio(&mut self) -> Result<bool, Self::Error> {
        self.record(Call::TrackTwo, AudioOperation::TrackTwoType)?;
        Ok(self.track_two_audio)
    }
    fn playback_mode(&mut self) -> Result<CdPlaybackMode, Self::Error> {
        self.record(Call::Mode, AudioOperation::Mode)?;
        Ok(self.modes.pop_front().unwrap_or(CdPlaybackMode::Playing))
    }
    fn position_tmsf(&mut self) -> Result<u32, Self::Error> {
        self.record(Call::Position, AudioOperation::Position)?;
        Ok(self.position)
    }
    fn track_length_msf(&mut self, track: u32) -> Result<u32, Self::Error> {
        self.calls.push(Call::Length(track));
        self.length
    }
    fn play_range(&mut self, range: CdPlayRange) -> Result<(), Self::Error> {
        self.record(Call::Play(range), AudioOperation::Play)
    }
    fn stop_cd(&mut self) -> Result<(), Self::Error> {
        self.record(Call::Stop, AudioOperation::Stop)
    }
    fn set_cd_volume(&mut self, volume: u32) -> Result<(), Self::Error> {
        self.record(Call::Volume(volume), AudioOperation::Volume)
    }
    fn clock_seconds(&mut self) -> Result<i32, Self::Error> {
        self.record(Call::Clock, AudioOperation::Clock)?;
        Ok(self.now)
    }
}

fn selected() -> (FrontendAudio, Backend) {
    let mut audio = FrontendAudio::new();
    let mut backend = Backend::default();
    audio.select_track(7, &mut backend).unwrap();
    (audio, backend)
}

fn playing() -> (FrontendAudio, Backend) {
    let (mut audio, mut backend) = selected();
    audio.resume(&mut 15, &mut backend).unwrap();
    (audio, backend)
}

#[test]
fn selection_is_retained_data_until_resume_runs_native_transport_order() {
    let (mut audio, mut backend) = selected();
    assert!(backend.calls.is_empty());
    assert_eq!(
        audio.state().selection,
        Some(MusicSelection {
            track_word: 8,
            saved_tmsf: 8
        })
    );
    audio.resume(&mut 15, &mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [
            Call::Volume(u32::MAX),
            Call::Open,
            Call::Format,
            Call::Present,
            Call::TrackTwo,
            Call::Length(8),
            Call::Play(CdPlayRange {
                from_tmsf: 8,
                to_tmsf: 9
            }),
            Call::Mode,
            Call::Clock
        ]
    );
    assert!(audio.state().device_open);
    assert!(audio.state().active_selection);
    assert_eq!(audio.state().deadline_seconds, 164);
}

#[test]
fn pause_preserves_position_and_resume_uses_full_track_duration_again() {
    let (mut audio, mut backend) = playing();
    audio.notify(4);
    backend.calls.clear();
    audio.pause(&mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [Call::Mode, Call::Position, Call::Stop, Call::Close]
    );
    assert_eq!(audio.state().selection.unwrap().saved_tmsf, 0x0302_0108);
    assert_eq!(audio.state().deadline_seconds, 164);
    assert!(audio.state().retry_pending);
    assert!(!audio.state().device_open);
    assert!(!audio.state().active_selection);

    backend.calls.clear();
    backend.now = 110;
    audio.resume(&mut 12, &mut backend).unwrap();
    assert_eq!(backend.calls[0], Call::Volume(0xcccc_cccc));
    assert!(backend.calls.contains(&Call::Play(CdPlayRange {
        from_tmsf: 0x0302_0108,
        to_tmsf: 9
    })));
    assert_eq!(audio.state().deadline_seconds, 174);
    assert!(audio.state().retry_pending); // resume does not clear the retry global
}

#[test]
fn service_uses_strict_deadline_and_restarts_completed_track_without_volume_write() {
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    backend.now = 164;
    audio.service(&mut 15, &mut backend).unwrap();
    assert_eq!(backend.calls, [Call::Clock]);
    assert_eq!(audio.state().deadline_seconds, 164);

    backend.calls.clear();
    backend.now = 165;
    backend.modes.push_back(CdPlaybackMode::Other);
    audio.service(&mut 15, &mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [
            Call::Clock,
            Call::Present,
            Call::TrackTwo,
            Call::Mode,
            Call::Length(8),
            Call::Play(CdPlayRange {
                from_tmsf: 8,
                to_tmsf: 9
            }),
            Call::Mode,
            Call::Clock
        ]
    );
    assert_eq!(audio.state().deadline_seconds, 229);
    assert!(!audio.state().retry_pending);
}

#[test]
fn retry_while_still_playing_does_not_restart_or_extend_deadline() {
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    assert!(audio.notify(8));
    audio.service(&mut 15, &mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [Call::Clock, Call::Present, Call::TrackTwo, Call::Mode]
    );
    assert_eq!(audio.state().deadline_seconds, 164);
    assert!(!audio.state().retry_pending);
}

#[test]
fn failed_to_start_sets_retry_and_does_not_manufacture_a_deadline() {
    let (mut audio, mut backend) = selected();
    backend.modes.push_back(CdPlaybackMode::Other);
    audio.resume(&mut 15, &mut backend).unwrap();
    assert!(!audio.state().active_selection);
    assert!(audio.state().retry_pending);
    assert_eq!(audio.state().deadline_seconds, 0);
    assert_eq!(audio.state().selection.unwrap().saved_tmsf, 8);
    assert!(!backend.calls.contains(&Call::Clock));
    backend.calls.clear();
    audio.service(&mut 15, &mut backend).unwrap();
    assert_eq!(backend.calls[0], Call::Present);
    assert!(audio.state().active_selection);
    assert!(!audio.state().retry_pending);
}

#[test]
fn media_absence_checks_track_two_but_skips_play_and_clock() {
    for (present, track_two_audio) in [(false, true), (true, false), (false, false)] {
        let (mut audio, mut backend) = selected();
        backend.present = present;
        backend.track_two_audio = track_two_audio;
        audio.resume(&mut 15, &mut backend).unwrap();
        assert_eq!(
            backend.calls,
            [
                Call::Volume(u32::MAX),
                Call::Open,
                Call::Format,
                Call::Present,
                Call::TrackTwo
            ]
        );
        assert!(!audio.state().audio_media_present);
        assert!(!audio.state().active_selection);
        assert!(audio.state().device_open);
    }
}

#[test]
fn no_selection_and_muted_service_skip_backend_while_muted_resume_pauses() {
    let mut audio = FrontendAudio::new();
    let mut backend = Backend::default();
    audio.notify(1);
    audio.pause(&mut backend).unwrap();
    audio.resume(&mut 15, &mut backend).unwrap();
    audio.service(&mut 15, &mut backend).unwrap();
    assert!(backend.calls.is_empty());
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    backend.now = 1000;
    audio.service(&mut 0, &mut backend).unwrap();
    assert!(backend.calls.is_empty());
    assert_eq!(audio.state().deadline_seconds, 164);
    audio.resume(&mut 0, &mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [Call::Mode, Call::Position, Call::Stop, Call::Close]
    );
}

#[test]
fn release_and_replacement_reset_position_without_clearing_process_retry_state() {
    let (mut audio, mut backend) = playing();
    audio.notify(1);
    backend.calls.clear();
    audio.select_track(1, &mut backend).unwrap();
    assert_eq!(backend.calls, [Call::Mode, Call::Stop, Call::Close]);
    assert_eq!(
        audio.state().selection,
        Some(MusicSelection {
            track_word: 2,
            saved_tmsf: 2
        })
    );
    assert_eq!(audio.state().deadline_seconds, 164);
    assert!(audio.state().retry_pending);
    backend.calls.clear();
    audio.release(&mut backend).unwrap();
    assert!(backend.calls.is_empty());
    assert_eq!(audio.state().selection, None);
    assert_eq!(audio.state().deadline_seconds, 164);
    assert!(audio.state().retry_pending);
}

#[test]
fn inactive_mode_on_pause_resets_position_without_stop_or_close() {
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    backend.modes.push_back(CdPlaybackMode::Other);
    audio.pause(&mut backend).unwrap();
    assert_eq!(backend.calls, [Call::Mode]);
    assert!(!audio.state().active_selection);
    assert!(audio.state().device_open); // native leaves its opened device alone
    assert_eq!(audio.state().selection.unwrap().saved_tmsf, 8);
}

#[test]
fn length_query_failure_uses_native_one_hour_fallback() {
    let (mut audio, mut backend) = selected();
    backend.length = Err("length unavailable");
    audio.resume(&mut 15, &mut backend).unwrap();
    assert_eq!(audio.state().deadline_seconds, 3700);
    assert!(!audio.state().stopped);
}

#[test]
fn deadline_is_signed_wrapping_wall_clock_with_zero_sentinel() {
    let (mut audio, mut backend) = selected();
    backend.now = i32::MAX;
    audio.resume(&mut 15, &mut backend).unwrap();
    assert_eq!(audio.state().deadline_seconds, i32::MIN + 63);
    backend.calls.clear();
    // Signed comparison expires this wrapped deadline immediately; it is not
    // an unsigned elapsed-time comparison or a saturating Duration.
    audio.service(&mut 15, &mut backend).unwrap();
    assert_eq!(audio.state().deadline_seconds, 0);
    assert_eq!(
        backend.calls,
        [Call::Clock, Call::Present, Call::TrackTwo, Call::Mode]
    );

    let (mut audio, mut backend) = selected();
    backend.now = -64;
    audio.resume(&mut 15, &mut backend).unwrap();
    assert_eq!(audio.state().deadline_seconds, 0);
    backend.calls.clear();
    audio.service(&mut 15, &mut backend).unwrap();
    assert!(backend.calls.is_empty());
}

#[test]
fn full_track_and_volume_words_preserve_native_shift_or_and_low_byte_rules() {
    let mut audio = FrontendAudio::new();
    let mut backend = Backend::default();
    audio.select_track(0x1fe, &mut backend).unwrap();
    assert_eq!(audio.state().selection.unwrap().track_word, 0x1ff);
    audio.resume(&mut 0x10, &mut backend).unwrap();
    assert_eq!(backend.calls[0], Call::Volume(0x1111_1110));
    assert!(backend.calls.contains(&Call::Length(0x1ff)));
    assert!(backend.calls.contains(&Call::Play(CdPlayRange {
        from_tmsf: 0xff,
        to_tmsf: 0
    })));

    let mut audio = FrontendAudio::new();
    audio.select_track(u32::MAX, &mut backend).unwrap();
    assert_eq!(audio.state().selection.unwrap().track_word, 0);
}

#[test]
fn notification_two_is_handled_without_changing_retry_and_unknown_codes_are_ignored() {
    let mut audio = FrontendAudio::new();
    assert!(audio.notify(2));
    assert!(!audio.state().retry_pending);
    for code in [0, 3, 5, 6, 7, 9, u32::MAX] {
        assert!(!audio.notify(code));
        assert!(!audio.state().retry_pending);
    }
    for code in [1, 4, 8] {
        let mut audio = FrontendAudio::new();
        assert!(audio.notify(code));
        assert!(audio.notify(2));
        assert!(audio.state().retry_pending);
    }
}

#[test]
fn play_failure_disables_shared_volume_and_stops_after_prior_transport_effects() {
    let (mut audio, mut backend) = selected();
    backend.fail = Some(AudioOperation::Play);
    let mut volume = 15;
    assert_eq!(
        audio.resume(&mut volume, &mut backend),
        Err(FrontendAudioError::Backend {
            operation: AudioOperation::Play,
            error: "transport failure"
        })
    );
    assert_eq!(volume, 0);
    assert!(audio.state().stopped);
    assert!(audio.state().device_open);
    assert!(!audio.state().active_selection);
    let calls = backend.calls.clone();
    assert_eq!(
        audio.resume(&mut volume, &mut backend),
        Err(FrontendAudioError::Stopped)
    );
    assert_eq!(backend.calls, calls);
}

#[test]
fn pause_failure_retains_saved_position_and_does_not_claim_close() {
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    backend.fail = Some(AudioOperation::Stop);
    assert!(matches!(
        audio.pause(&mut backend),
        Err(FrontendAudioError::Backend {
            operation: AudioOperation::Stop,
            ..
        })
    ));
    assert_eq!(
        audio.state().selection.unwrap().saved_tmsf,
        backend.position
    );
    assert!(audio.state().active_selection);
    assert!(audio.state().device_open);
    assert_eq!(backend.calls, [Call::Mode, Call::Position, Call::Stop]);
}

#[test]
fn format_failure_closes_just_opened_backend_and_preserves_first_error() {
    let (mut audio, mut backend) = selected();
    backend.fail = Some(AudioOperation::SetTimeFormat);
    assert!(matches!(
        audio.resume(&mut 15, &mut backend),
        Err(FrontendAudioError::Backend {
            operation: AudioOperation::SetTimeFormat,
            ..
        })
    ));
    assert_eq!(
        backend.calls,
        [
            Call::Volume(u32::MAX),
            Call::Open,
            Call::Format,
            Call::Close
        ]
    );
    assert!(audio.state().stopped);
    assert!(!audio.state().device_open);
}

#[test]
fn service_clears_retry_before_a_failed_play_attempt() {
    let (mut audio, mut backend) = selected();
    audio.notify(8);
    backend.fail = Some(AudioOperation::Play);
    let mut volume = 15;
    assert!(matches!(
        audio.service(&mut volume, &mut backend),
        Err(FrontendAudioError::Backend {
            operation: AudioOperation::Play,
            ..
        })
    ));
    assert_eq!(volume, 0);
    assert!(!audio.state().retry_pending);
    assert!(!backend
        .calls
        .iter()
        .any(|call| matches!(call, Call::Volume(_))));
}

#[test]
fn volume_setting_resumes_then_writes_the_linear_aux_level() {
    let (mut audio, mut backend) = playing();
    backend.calls.clear();
    let mut word = 7;
    audio.apply_volume_setting(&mut word, &mut backend).unwrap();
    // Resume re-applies the nibble volume; the active track keeps playing,
    // then 004F7D80 writes the equal in-range linear level again.
    assert_eq!(
        backend.calls,
        [
            Call::Volume(0x7777_7777),
            Call::Present,
            Call::TrackTwo,
            Call::Mode,
            Call::Volume(0x7777_7777),
        ]
    );
    // Zero pauses the track, then still zeroes the AUX volume.
    backend.calls.clear();
    audio.apply_volume_setting(&mut 0, &mut backend).unwrap();
    assert_eq!(
        backend.calls,
        [
            Call::Mode,
            Call::Position,
            Call::Stop,
            Call::Close,
            Call::Volume(0)
        ]
    );
    // Without a selection the AUX write still runs.
    let mut audio = FrontendAudio::new();
    let mut backend = Backend::default();
    audio.apply_volume_setting(&mut 3, &mut backend).unwrap();
    assert_eq!(backend.calls, [Call::Volume(0x3333_3333)]);
}

#[test]
fn linear_aux_level_matches_nibbles_only_until_copies_carry_or_wrap() {
    for word in 0..=15 {
        assert_eq!(music_aux_level(word), expanded_volume(word));
    }
    // Below the multiply wrap the level is `word * 0x1111`; nibble copies
    // that share no set bits (16) still coincide, overlapping ones carry.
    assert_eq!(music_aux_level(16), 0x0001_1110);
    assert_eq!(expanded_volume(16), 0x0001_1110);
    assert_eq!(music_aux_level(17), 0x0001_2221);
    assert_eq!(expanded_volume(17), 0x0001_1111);
    assert_eq!(music_aux_level(0x1_0000), 0xFFFF_EEEF);
    assert_eq!(expanded_volume(0x1_0000), 0x1111_0000);
    // Wrapping multiply, then signed division toward zero.
    assert_eq!(music_aux_level(u32::MAX), (-4369i32) as u32);
    assert_eq!(music_aux_level(0x8000_0000), (i32::MIN / 15) as u32);
}

#[test]
fn failed_volume_resume_clears_the_word_and_stops_before_the_aux_write() {
    let (mut audio, mut backend) = selected();
    backend.fail = Some(AudioOperation::Play);
    let mut word = 9;
    assert!(audio.apply_volume_setting(&mut word, &mut backend).is_err());
    // Resume cleared the shared word; the owner stops before the AUX write.
    assert_eq!(word, 0);
    assert_eq!(
        backend
            .calls
            .iter()
            .filter(|c| matches!(c, Call::Volume(_)))
            .count(),
        1
    );
    assert!(audio.state().stopped);
}
