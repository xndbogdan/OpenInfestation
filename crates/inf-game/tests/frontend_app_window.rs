//! Explicit hardware smoke: native input/menu/window/audio together.

use inf_game::display_selection::{
    original_size, tier_for, DisplayMode, DisplaySelection, MonitorModes,
};
use inf_game::frontend_app::{
    run_frontend, FrontendAppClock, FrontendAppInput, FrontendAppRequest,
};
use inf_game::presentation::{ClassicScaling, NativeFrame, RendererChoice};
use inf_game::{
    frontend_assets::FrontendAssets, frontend_settings::FrontendSettings, menu::MenuCommand,
    name_entry::NameCompletion, session::SessionPhase,
};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn key(frame: u64, name: &str, pressed: bool) -> FrontendAppInput {
    FrontendAppInput {
        frame,
        key: name.into(),
        pressed,
    }
}

/// What SDL output 0, the fresh settings' device, offers the game: the same
/// physical-pixel list the Video menu reads.
fn monitor() -> MonitorModes {
    sdl2::hint::set("SDL_WINDOWS_DPI_AWARENESS", "permonitorv2");
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let size = |mode: sdl2::video::DisplayMode| [mode.w as u32, mode.h as u32];
    MonitorModes::new(
        video.desktop_display_mode(0).ok().map(size),
        (0..video.num_display_modes(0).unwrap())
            .map(|index| size(video.display_mode(0, index).unwrap())),
    )
}

/// An existing empty sidecar prevents importing the player's retail profiles.
fn empty_profile_session(tag: &str) -> FrontendAppRequest {
    session_with_display(
        tag,
        DisplaySelection {
            mode: DisplayMode::Window,
            size: [640, 480],
        },
    )
}

/// `empty_profile_session` with a saved display; a window also requests its
/// size explicitly.
fn session_with_display(tag: &str, display: DisplaySelection) -> FrontendAppRequest {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join(format!("../.tmp/inf-session-{tag}-{nonce}"));
    let settings = directory.join("settings.json");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.set_display_selection(display);
    saved.persist().unwrap();
    let profiles = directory.join("profiles.sav");
    std::fs::write(&profiles, []).unwrap();
    let mut input = Vec::new();
    for (frame, name) in [
        (5, "Return"), // Title skip (when transcoded video/ is present; no-op on the failed-open path).
        (100, "Return"), // Root Start Game, then the deferred profile menu.
        (200, "Return"), // First empty profile, then the 1000 ms operation.
        (230, "Backspace"),
        (240, "A"), // Jameson -> JamesoA through the physical key translator.
    ] {
        input.extend([key(frame, name, true), key(frame + 1, name, false)]);
    }
    FrontendAppRequest {
        data_root: root,
        settings_path: settings,
        profiles_path: profiles,
        input_replay: input,
        dimensions: (display.mode == DisplayMode::Window).then_some(display.size),
        scaling: None,
        vehicle_position_carry: None,
        vehicle_gun_mounts: None,
        hovercraft_fans: None,
        exact_world_ticks: None,
        ultra_ground: None,
        ambience: None,
        headphones: None,
        database_models: None,
        exact_frontend_ticks: None,
        guided_mounts: None,
        completion_bonus: None,
        paced_actors: None,
        mission_completion: None,
        splash_voice_cap: None,
        all_cheats: None,
        prototype_world: None,
        pelagia_rework: None,
        renderer: Some(RendererChoice::Software),
        controller_layout: None,
        variant: None,
        language: None,
        hidden: false,
        max_frames: Some(260),
        snapshot: None,
        snapshots_at: Vec::new(),
        record_session: None,
        audio_out: None,
        mission: None,
        start: None,
        clock: FrontendAppClock::FixedStep { micros: 125_000 },
        session_record: None,
        session_replay: None,
    }
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn start_game_empty_profile_types_name_and_back_returns_to_frontend() {
    // Capture the name-entry surface before Back, then independently replay
    // through Back so both actual screen owners have a presented artifact.
    for back in [false, true] {
        let mut request = empty_profile_session(if back { "back" } else { "name" });
        let profiles = request.profiles_path.clone();
        let snapshot = profiles.with_file_name(if back { "frontend.png" } else { "name.png" });
        request.snapshot = Some(snapshot.clone());
        if back {
            request
                .input_replay
                .extend([key(280, "Escape", true), key(281, "Escape", false)]);
            request.max_frames = Some(320);
        }
        let report = run_frontend(request).unwrap();
        assert_eq!(report.frames, if back { 320 } else { 260 });
        assert_eq!(
            report.phase,
            if back {
                SessionPhase::Frontend
            } else {
                SessionPhase::Phase9
            }
        );
        let phases: Vec<_> = report.transitions.iter().map(|t| t.effective).collect();
        let mut expected = vec![SessionPhase::Frontend, SessionPhase::Phase9];
        if back {
            expected.push(SessionPhase::Frontend);
        }
        assert_eq!(phases, expected);
        assert!(report.name_entry_draws > 40);
        assert_eq!(report.scene.frame_begins, report.frames as usize);
        assert_eq!(report.scene.presentation_clears, report.frames as usize);
        assert_eq!(report.pixels.unsupported, 0);
        assert_eq!(report.display_rebuilds, 0);
        assert!(report.name_completions.is_empty());
        assert!(!report.exit_requested);
        assert!(report.music.active_selection && !report.music.stopped);
        assert_eq!(report.snapshot.as_ref(), Some(&snapshot));
        assert!(std::fs::metadata(snapshot).unwrap().len() > 1000);
        assert!(
            std::fs::read(profiles).unwrap().is_empty(),
            "Back does not save a profile"
        );
    }
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn name_completion_loads_world_briefing_then_drivable_gameplay() {
    let mut request = empty_profile_session("complete");
    let profiles = request.profiles_path.clone();
    let settings = request.settings_path.clone();
    let data_root = request.data_root.clone();
    // Tab calls 0045AEB0 directly, independently of the highlighted ring cell.
    request
        .input_replay
        .extend([key(260, "Tab", true), key(261, "Tab", false)]);
    // After the 9â†’3 world load (fresh-heap) and the single-tick briefing to
    // phase 4, drive with arrows/WASD so the vehicle moves under the chase
    // camera. Frames use the fixed 125 ms step, matching the scheduler tick.
    for (frame, name) in [
        (300, "Up"),
        (310, "Up"),
        (320, "Right"),
        (330, "Up"),
        (340, "Left"),
    ] {
        request
            .input_replay
            .extend([key(frame, name, true), key(frame + 2, name, false)]);
    }
    request.max_frames = Some(360);
    let snapshot = profiles.with_file_name("gameplay.png");
    request.snapshot = Some(snapshot.clone());
    request.hidden = true;
    let report = run_frontend(request).unwrap();
    assert_eq!(report.phase, SessionPhase::Gameplay);
    // Tab arms 1000 ms in the iteration that presents frame 260. At 125 ms
    // per iteration the delay reaches the delta in frame 267's tick, which
    // requests phase 3; the next first pass runs `LoadWorld` with fresh-heap
    // WORDs, briefing ticks once to phase 4, and gameplay ticks drive.
    assert!(matches!(
        report.name_completions[..],
        [NameCompletion::Phase3]
    ));
    assert_eq!(
        report
            .transitions
            .iter()
            .map(|t| t.effective)
            .collect::<Vec<_>>(),
        [
            SessionPhase::Frontend,
            SessionPhase::Phase9,
            SessionPhase::Phase3,
            SessionPhase::Gameplay
        ]
    );
    if report.frames != 360 {
        eprintln!(
            "gameplay smoke failed: phase={:?} frames={} transitions={:?} completions={:?}",
            report.phase,
            report.frames,
            report
                .transitions
                .iter()
                .map(|t| (t.previous, t.effective))
                .collect::<Vec<_>>(),
            report.name_completions,
        );
    }
    assert_eq!(report.frames, 360);
    // Briefing/gameplay draws are skeletons without `frame_begin` (no model
    // submission yet), so `frame_begins` counts only frontend/name-entry frames.
    // Presentation still clears every presented frame, including gameplay.
    assert!(report.scene.frame_begins >= 200);
    // Loading presentations clear too; a failed-open title (no title video)
    // presents and clears nothing on the first frame.
    assert!(matches!(
        report.frames as usize + report.loading.presentations - report.scene.presentation_clears,
        0 | 1
    ));
    assert_eq!(report.pixels.unsupported, 0);
    assert!(report.name_entry_draws > 40);
    let assets = FrontendAssets::open(&data_root).unwrap();
    let saved = FrontendSettings::load_or_defaults(settings, &assets).unwrap();
    assert_eq!(saved.stored_player_name(), Some(&b"JamesoA"[..]));
    assert_eq!(saved.stored_player_initials(), Some(&b"0up"[..]));
    assert!(
        std::fs::read(&profiles).unwrap().is_empty(),
        "completion serializes in memory; the profile file is not written"
    );
    // The snapshot proves the level reached the window through the native
    // world draw (`world_scene::draw_world_frame`: view setup, depth
    // state, entity models; sky/terrain skip per feature). Driving still
    // uses the arcade `tick_drive` approximation.
    assert_eq!(report.snapshot.as_ref(), Some(&snapshot));
    assert!(std::fs::metadata(&snapshot).unwrap().len() > 1000);
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn repeated_idle_demo_deadlines_keep_quit_and_new_game_usable() {
    // Use the real scheduled SDL session and physical key translation. Large
    // raw steps cross several native 60-second idle deadlines while camera
    // integration still receives the ordinary capped 125,000-us delta.
    for new_game in [false, true] {
        let mut request = empty_profile_session(if new_game {
            "idle-new-game"
        } else {
            "idle-quit"
        });
        request.hidden = true;
        request.clock = FrontendAppClock::FixedStep { micros: 1_000_000 };
        request.max_frames = Some(300);
        request.input_replay = vec![key(5, "Return", true), key(6, "Return", false)];
        for (frame, name) in if new_game {
            [(200, "Return"), (230, "Return"), (260, "Backspace")]
        } else {
            [(200, "Escape"), (230, "Down"), (260, "Return")]
        } {
            request
                .input_replay
                .extend([key(frame, name, true), key(frame + 1, name, false)]);
        }
        let profiles = request.profiles_path.clone();
        let snapshot = profiles.with_file_name("output.png");
        request.snapshot = Some(snapshot.clone());
        let report = run_frontend(request).unwrap();
        assert_eq!(report.frames, if new_game { 300 } else { 260 });
        assert_eq!(
            report.phase,
            if new_game {
                SessionPhase::Phase9
            } else {
                SessionPhase::Frontend
            }
        );
        let phases: Vec<_> = report.transitions.iter().map(|t| t.effective).collect();
        assert_eq!(
            phases,
            if new_game {
                vec![SessionPhase::Frontend, SessionPhase::Phase9]
            } else {
                vec![SessionPhase::Frontend]
            },
            "deferred demos must not complete a phase transition"
        );
        assert_eq!(report.exit_requested, !new_game);
        assert_eq!(report.scene.frame_begins, report.frames as usize);
        // Native 00404840 presents/clears nothing on the initial failed-open
        // title frame when optional transcoded video assets are absent.
        assert!(matches!(
            report.frames as usize - report.scene.presentation_clears,
            0 | 1
        ));
        assert_eq!(report.pixels.unsupported, 0);
        assert_eq!(report.display_rebuilds, 0);
        assert_eq!(report.dimensions, [640, 480]);
        assert!(report.menu_rows > 100 && report.glyph_passes > 1000);
        assert!(report.music.active_selection && !report.music.stopped);
        assert_eq!(report.audio.music_starts, 1);
        assert!(report.name_completions.is_empty());
        if new_game {
            assert!(report.name_entry_draws > 40);
        } else {
            assert_eq!(report.name_entry_draws, 0);
        }
        assert!(std::fs::read(profiles).unwrap().is_empty());
        assert_eq!(report.snapshot.as_ref(), Some(&snapshot));
        assert!(std::fs::metadata(&snapshot).unwrap().len() > 1000);
        eprintln!("idle deadline SDL smoke: {snapshot:?}");
    }
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn authored_level1_start_reaches_shared_gameplay_renderer() {
    let mut request = empty_profile_session("level1-shared-renderer");
    // Explicit investigation override exercises ordinary Level 1; normal New
    // Game retains mission22 and the intro, covered by the classic-output smoke.
    request.mission = Some(18);
    request.hidden = true;
    request.max_frames = Some(360);
    request.input_replay.extend([
        key(260, "Tab", true),
        key(261, "Tab", false),
        // Authored Escape invokes Start (0040D660). Its 2000 ms countdown
        // uses the native briefing 7/8 scale before the phase4 handoff.
        key(300, "Escape", true),
        key(301, "Escape", false),
    ]);
    let snapshot = request.settings_path.with_file_name("gameplay.png");
    request.snapshot = Some(snapshot.clone());
    let profiles = request.profiles_path.clone();
    let report = run_frontend(request).unwrap();
    assert_eq!(report.phase, SessionPhase::Gameplay);
    assert_eq!(report.frames, 360);
    // Level 1's first tutorial hint trigger pauses the game (phase 6 reuses
    // the gameplay screen) and its expiry returns to phase 4; see
    // PLAYER_COMMAND_RUNTIME.md#selector-50-hint-triggers.
    assert_eq!(
        report
            .transitions
            .iter()
            .map(|t| t.effective)
            .collect::<Vec<_>>(),
        [
            SessionPhase::Frontend,
            SessionPhase::Phase9,
            SessionPhase::Phase3,
            SessionPhase::Gameplay,
            SessionPhase::Phase6,
            SessionPhase::Gameplay,
        ]
    );
    assert!(matches!(
        report.name_completions[..],
        [NameCompletion::Phase3]
    ));
    assert!(report.loading.callbacks > 0);
    assert_eq!(report.loading.presentations, report.loading.callbacks);
    assert_eq!(report.loading.contained_errors, 0);
    assert_eq!(report.loading.model_background_draws, 0);
    assert!(matches!(
        360 + report.loading.presentations - report.scene.presentation_clears,
        0 | 1
    ));
    eprintln!("Level 1 consumed loading: {:?}", report.loading);
    let world = report
        .world_draw
        .expect("Level 1 uses the shared world renderer");
    // Shared selector16/vehicle camera executes independently of intro.
    // Native vehicle physics and the labelled arcade adapter remain separate.
    assert!(world.native_camera);
    assert_eq!(world.coordinate_shift, 16);
    assert!(world.outcome.terrain_commands > 0);
    assert!(world.outcome.submitted_models > 0);
    // Commands and the viewed PNG establish this port-side world regression;
    // they do not establish complete simulation or matched retail pixels.
    assert_eq!(report.pixels.unsupported, 0);
    assert_eq!(report.display_rebuilds, 0);
    assert_eq!(report.dimensions, [640, 480]);
    assert_eq!(report.output_dimensions, report.dimensions);
    assert!(std::fs::read(profiles).unwrap().is_empty());
    let (size, pixels) = read_rgba_snapshot(&snapshot);
    assert_eq!(size, report.dimensions);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[..3] != [0, 0, 0]));
    eprintln!("Level 1 SDL smoke: {snapshot:?}");
}

/// NetworkMenuRefusalApproximation: the root Network row refuses like a
/// completed profile slot (sound 4, then the confirmation's 5) and the
/// frontend keeps running on the root menu.
#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn root_network_row_refuses_and_keeps_the_frontend_running() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let settings = root.join(format!("../.tmp/inf-app-network-{nonce}/settings.json"));
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.words_mut()[0x40 / 4] = 0;
    saved.persist().unwrap();
    let mut input = Vec::new();
    for (frame, name) in [(5, "Return"), (500, "Down"), (540, "Down"), (580, "Return")] {
        input.extend([key(frame, name, true), key(frame + 1, name, false)]);
    }
    let report = run_frontend(FrontendAppRequest {
        data_root: root.clone(),
        settings_path: settings.clone(),
        profiles_path: settings.with_file_name("profiles.sav"),
        input_replay: input,
        dimensions: Some([640, 480]),
        scaling: None,
        vehicle_position_carry: None,
        vehicle_gun_mounts: None,
        hovercraft_fans: None,
        exact_world_ticks: None,
        ultra_ground: None,
        ambience: None,
        headphones: None,
        database_models: None,
        exact_frontend_ticks: None,
        guided_mounts: None,
        completion_bonus: None,
        paced_actors: None,
        mission_completion: None,
        splash_voice_cap: None,
        all_cheats: None,
        prototype_world: None,
        pelagia_rework: None,
        renderer: Some(RendererChoice::Software),
        controller_layout: None,
        variant: None,
        language: None,
        hidden: true,
        max_frames: Some(680),
        snapshot: None,
        snapshots_at: Vec::new(),
        record_session: None,
        audio_out: None,
        mission: None,
        start: None,
        clock: FrontendAppClock::FixedStep { micros: 16_000 },
        session_record: None,
        session_replay: None,
    })
    .unwrap();
    assert!(!report.exit_requested);
    assert_eq!(report.frames, 680);
    assert_eq!(report.pixels.unsupported, 0);
    // Two navigation cues, then refusal 4 and the confirmation 5.
    assert_eq!(report.audio.cue_requests, 4);
    assert_eq!(report.audio.cues_started, report.audio.cue_requests);
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn original_keyboard_quit_no_and_yes_drive_complete_frontend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let settings = root.join(format!("../.tmp/inf-app-window-{nonce}/settings.json"));
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.words_mut()[0x40 / 4] = 0;
    saved.persist().unwrap();
    for yes in [false, true] {
        let mut input = vec![
            key(5, "Return", true),
            key(6, "Return", false),
            key(500, "Escape", true),
            key(501, "Escape", false),
        ];
        if yes {
            input.extend([key(550, "Down", true), key(551, "Down", false)]);
        }
        input.extend([key(580, "Return", true), key(581, "Return", false)]);
        let report = run_frontend(FrontendAppRequest {
            data_root: root.clone(),
            settings_path: settings.clone(),
            profiles_path: settings.with_file_name("profiles.sav"),
            input_replay: input,
            dimensions: Some([640, 480]),
            scaling: None,
            vehicle_position_carry: None,
            vehicle_gun_mounts: None,
            hovercraft_fans: None,
            exact_world_ticks: None,
            ultra_ground: None,
            ambience: None,
            headphones: None,
            database_models: None,
            exact_frontend_ticks: None,
            guided_mounts: None,
            completion_bonus: None,
            paced_actors: None,
            mission_completion: None,
            splash_voice_cap: None,
            all_cheats: None,
            prototype_world: None,
            pelagia_rework: None,
            renderer: Some(RendererChoice::Software),
            controller_layout: None,
            variant: None,
            language: None,
            hidden: true,
            max_frames: Some(680),
            snapshot: None,
            snapshots_at: Vec::new(),
            record_session: None,
            audio_out: None,
            mission: None,
            start: None,
            clock: FrontendAppClock::FixedStep { micros: 16_000 },
            session_record: None,
            session_replay: None,
        })
        .unwrap();
        assert_eq!(report.exit_requested, yes);
        assert_eq!(report.frames, if yes { 580 } else { 680 });
        assert!(report.menu_rows > 100 && report.glyph_passes > 1000);
        assert_eq!(report.scene.frame_begins, report.frames as usize);
        assert_eq!(report.scene.presentation_clears, report.frames as usize);
        assert_eq!(report.pixels.unsupported, 0);
        assert_eq!(report.music.selection.unwrap().track_word, 8);
        assert!(report.music.active_selection && !report.music.stopped);
        assert_eq!(report.audio.music_starts, 1);
        assert_eq!(report.audio.cue_requests, if yes { 2 } else { 1 });
        assert_eq!(report.audio.cues_started, report.audio.cue_requests);
        assert!(report.audio.music_frames > 0 && report.audio.effect_frames > 0);
        assert!(settings.is_file());
    }
}

#[test]
#[ignore = "opens a real SDL window/audio device; run explicitly in release mode"]
fn video_device_resolution_and_filter_changes_keep_the_live_frontend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join(format!("../.tmp/inf-video-window-{nonce}"));
    let settings = directory.join("settings.json");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.words_mut()[0x40 / 4] = 0;
    saved.persist().unwrap();
    let mut input = Vec::new();
    let mut press = |frame, name| {
        input.extend([key(frame, name, true), key(frame + 1, name, false)]);
    };
    press(5, "Return"); // Title skip (when video/ present; no-op on failed-open).
    press(100, "Down");
    press(105, "Return"); // Options
    press(200, "Return"); // Video; native header selects camera 6
    press(300, "Down"); // Device
    press(310, "Return"); // Even a single unchanged device rebuilds.
    press(320, "Down"); // Resolution
    press(330, "Right"); // 640x480 -> the next listed window size
    press(340, "Left"); // and back
    press(350, "Right"); // and forward again
    press(360, "Down"); // Colour depth
    press(370, "Right"); // The native reset path still uses 16-bit descriptors.
    press(380, "Down"); // Bilinear filtering
    press(390, "Left"); // Linear -> point, rebuilding at the same resolution.
    press(410, "Escape"); // The native Back handler owns persistence.
    let report = run_frontend(FrontendAppRequest {
        data_root: root.clone(),
        settings_path: settings.clone(),
        profiles_path: directory.join("profiles.sav"),
        input_replay: input,
        dimensions: Some([640, 480]),
        scaling: None,
        vehicle_position_carry: None,
        vehicle_gun_mounts: None,
        hovercraft_fans: None,
        exact_world_ticks: None,
        ultra_ground: None,
        ambience: None,
        headphones: None,
        database_models: None,
        exact_frontend_ticks: None,
        guided_mounts: None,
        completion_bonus: None,
        paced_actors: None,
        mission_completion: None,
        splash_voice_cap: None,
        all_cheats: None,
        prototype_world: None,
        pelagia_rework: None,
        renderer: Some(RendererChoice::Software),
        controller_layout: None,
        variant: None,
        language: None,
        hidden: true,
        max_frames: Some(460),
        snapshot: Some(directory.join("after-video.png")),
        snapshots_at: Vec::new(),
        record_session: None,
        audio_out: None,
        mission: None,
        start: None,
        clock: FrontendAppClock::FixedStep { micros: 125_000 },
        session_record: None,
        session_replay: None,
    })
    .unwrap();
    let next = monitor()
        .step(DisplayMode::Window, [640, 480], MenuCommand::AdjustNext)
        .expect("a window list above 640x480");
    let tier = tier_for(next);
    // The default widescreen frame keeps the tier's height at the window's
    // aspect.
    let frame = NativeFrame::for_output(original_size(tier), next, ClassicScaling::Widescreen);
    assert_eq!(report.scaling, ClassicScaling::Widescreen);
    assert_eq!(report.dimensions, frame.size);
    assert_eq!(report.output_dimensions, next);
    assert_eq!(report.display_rebuilds, 6);
    assert_eq!(report.frames, 460);
    // A present title video holds frames 1..4 until Return at frame 5 skips
    // it; without one the title finishes on its first tick.
    let title_frames = if root.join("video").is_dir() { 4 } else { 0 };
    assert_eq!(
        report.age_millis,
        Some((460 - title_frames) * 125),
        "reset must not reconstruct or double-tick the screen"
    );
    assert_eq!(report.scene.frame_begins, 460);
    assert_eq!(report.scene.presentation_clears, 460);
    assert_eq!(report.pixels.unsupported, 0);
    assert_eq!(
        report.audio.music_starts, 1,
        "display changes retain the audio owner"
    );
    assert!(report.music.active_selection && !report.music.stopped);
    assert!(report.audio.cues_started > 10);
    let saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    assert_eq!(saved.words()[0x0c / 4], tier);
    assert_eq!(saved.display_selection().size, next);
    assert_eq!(saved.words()[0x44 / 4], 1);
    assert_eq!(saved.words()[0x3c / 4], 0);
    assert!(saved.current_display().is_some());
    let decoder = png::Decoder::new(std::fs::File::open(report.snapshot.unwrap()).unwrap());
    let reader = decoder.read_info().unwrap();
    assert_eq!(
        [reader.info().width, reader.info().height],
        report.output_dimensions
    );

    // Default relaunch uses the saved Resolution. An explicit pixel request
    // still overrides it, and the normal startup persistence records the
    // actual selected mode in both cases.
    for (dimensions, expected, variant) in
        [(None, frame.size, tier), (Some([640, 480]), [640, 480], 2)]
    {
        let restarted = run_frontend(FrontendAppRequest {
            data_root: root.clone(),
            settings_path: settings.clone(),
            profiles_path: directory.join("profiles.sav"),
            input_replay: Vec::new(),
            dimensions,
            scaling: None,
            vehicle_position_carry: None,
            vehicle_gun_mounts: None,
            hovercraft_fans: None,
            exact_world_ticks: None,
            ultra_ground: None,
            ambience: None,
            headphones: None,
            database_models: None,
            exact_frontend_ticks: None,
            guided_mounts: None,
            completion_bonus: None,
            paced_actors: None,
            mission_completion: None,
            splash_voice_cap: None,
            all_cheats: None,
            prototype_world: None,
            pelagia_rework: None,
            renderer: Some(RendererChoice::Software),
            controller_layout: None,
            variant: None,
            language: None,
            hidden: true,
            max_frames: Some(1),
            snapshot: None,
            snapshots_at: Vec::new(),
            record_session: None,
            audio_out: None,
            mission: None,
            start: None,
            clock: FrontendAppClock::FixedStep { micros: 125_000 },
            session_record: None,
            session_replay: None,
        })
        .unwrap();
        assert_eq!(restarted.dimensions, expected);
        assert_eq!(restarted.display_rebuilds, 0);
        assert_eq!(
            FrontendSettings::load_or_defaults(&settings, &assets)
                .unwrap()
                .words()[3],
            variant
        );
    }
}

/// Options, Video, then Resolution Right from the starting size; Escape
/// saves through the native Back handler.
fn resolution_right(request: &mut FrontendAppRequest) {
    request.input_replay.clear();
    for (frame, name) in [
        (5, "Return"),
        (100, "Down"),
        (105, "Return"), // Options
        (200, "Return"), // Video
        (300, "Down"),   // Device
        (320, "Down"),   // Resolution
        (330, "Right"),  // The next listed size
        (410, "Escape"), // Save through native Back
    ] {
        request
            .input_replay
            .extend([key(frame, name, true), key(frame + 1, name, false)]);
    }
    request.max_frames = Some(430);
}

/// The window sizes above 1024x768 the monitor lists, with the one after.
fn widescreen_window_step() -> Option<([u32; 2], [u32; 2])> {
    let modes = monitor();
    let start = [1280, 720];
    if !modes.menu_sizes(DisplayMode::Window).contains(&start) {
        eprintln!("the desktop lists no 1280x720 window; case skipped");
        return None;
    }
    let next = modes.step(DisplayMode::Window, start, MenuCommand::AdjustNext)?;
    Some((start, next))
}

#[test]
#[ignore = "opens an SDL window/audio device; run explicitly in release mode"]
fn exact_resolution_survives_a_video_reset_and_saved_restart() {
    let Some((start, next)) = widescreen_window_step() else {
        return;
    };
    let mut request = empty_profile_session("exact-video");
    request.hidden = true;
    request.dimensions = Some(start);
    request.scaling = Some(ClassicScaling::FourThree);
    resolution_right(&mut request);
    request.snapshot = Some(request.settings_path.with_file_name("exact-output.png"));
    let report = run_frontend(request.clone()).unwrap();
    // A size above the originals draws the largest original inside it.
    assert_eq!(report.dimensions, original_size(tier_for(next)));
    assert_eq!(report.output_dimensions, next);
    assert_eq!(
        report.display,
        DisplaySelection {
            mode: DisplayMode::Window,
            size: next
        }
    );
    assert_eq!(report.display_rebuilds, 1);
    assert_eq!(report.pixels.unsupported, 0);
    let decoder = png::Decoder::new(std::fs::File::open(report.snapshot.unwrap()).unwrap());
    let reader = decoder.read_info().unwrap();
    assert_eq!([reader.info().width, reader.info().height], next);

    request.dimensions = None;
    request.scaling = None;
    request.input_replay.clear();
    request.max_frames = Some(1);
    request.snapshot = None;
    let restarted = run_frontend(request.clone()).unwrap();
    assert_eq!(restarted.dimensions, report.dimensions);
    assert_eq!(restarted.output_dimensions, next);
    assert_eq!(restarted.display, report.display);
    assert_eq!(restarted.scaling, ClassicScaling::FourThree);

    // An original size draws natively again.
    request.variant = Some(3);
    let original = run_frontend(request).unwrap();
    assert_eq!(original.dimensions, [800, 600]);
    assert_eq!(original.output_dimensions, [800, 600]);

    let full_hd = [1920, 1080];
    if !monitor().menu_sizes(DisplayMode::Window).contains(&full_hd) {
        eprintln!("the desktop lists no 1920x1080 window; name-entry case skipped");
        return;
    }
    let mut name_entry = empty_profile_session("exact-name");
    name_entry.dimensions = Some(full_hd);
    name_entry.scaling = Some(ClassicScaling::Integer);
    name_entry.hidden = true;
    name_entry.snapshot = Some(name_entry.settings_path.with_file_name("exact-name.png"));
    let entered = run_frontend(name_entry).unwrap();
    assert_eq!(entered.phase, SessionPhase::Phase9);
    assert!(entered.name_entry_draws > 40);
    assert_eq!(entered.dimensions, [1024, 768]);
    assert_eq!(entered.output_dimensions, full_hd);
}

#[test]
#[ignore = "opens an OpenGL context and audio device; run explicitly in release mode"]
fn opengl_renderer_follows_a_video_reset_like_the_cpu_raster() {
    // The Video reset rebuilds the renderer and its frame mid-session; the
    // OpenGL device must follow it, then export the drawn output.
    let Some((start, next)) = widescreen_window_step() else {
        return;
    };
    let mut snapshots = Vec::new();
    for renderer in [RendererChoice::Software, RendererChoice::OpenGl] {
        let mut request = empty_profile_session("opengl-video");
        request.hidden = true;
        request.renderer = Some(renderer);
        request.dimensions = Some(start);
        request.scaling = Some(ClassicScaling::FourThree);
        resolution_right(&mut request);
        request.snapshot = Some(request.settings_path.with_file_name("opengl-video.png"));
        let report = run_frontend(request).unwrap();
        assert_eq!(
            report.renderer.name(),
            format!("{renderer:?}").to_lowercase()
        );
        assert_eq!(report.dimensions, original_size(tier_for(next)));
        assert_eq!(report.output_dimensions, next);
        assert_eq!(report.display_rebuilds, 1);
        assert_eq!(report.pixels.unsupported, 0);
        snapshots.push(read_rgba_snapshot(&report.snapshot.unwrap()));
    }
    let [(cpu_size, cpu), (gpu_size, gpu)] = <[_; 2]>::try_from(snapshots).unwrap();
    assert_eq!(cpu_size, next);
    assert_eq!(gpu_size, cpu_size);
    // The CPU output enlarges native pixels; OpenGL drew the same
    // submissions at the presented size. Under each native pixel's centre
    // they agree except along edges.
    let native = original_size(tier_for(next));
    let rect = ClassicScaling::FourThree.destination(native, next);
    let mut differing = 0;
    for y in 0..native[1] {
        let ty = rect.y as u32
            + ((f64::from(y) + 0.5) * f64::from(rect.height) / f64::from(native[1])) as u32;
        for x in 0..native[0] {
            let tx = rect.x as u32
                + ((f64::from(x) + 0.5) * f64::from(rect.width) / f64::from(native[0])) as u32;
            let offset = ((ty * next[0] + tx) * 4) as usize;
            if cpu[offset..offset + 3]
                .iter()
                .zip(&gpu[offset..offset + 3])
                .any(|(a, b)| a.abs_diff(*b) > 24)
            {
                differing += 1;
            }
        }
    }
    let share = f64::from(differing) / f64::from(native[0] * native[1]);
    assert!(share <= 0.02, "{differing} native pixels differ");
}

fn read_rgba_snapshot(path: &Path) -> ([u32; 2], Vec<u8>) {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    pixels.truncate(info.buffer_size());
    ([info.width, info.height], pixels)
}

#[test]
#[ignore = "uses a hidden SDL window/audio device; run explicitly in release mode"]
fn borderless_output_preserves_intro_and_authored_briefing_pixels() {
    // Reach both screens through normal New Game, including the empty intro
    // briefing, then the native Escape -> results -> Level 1 briefing handoff.
    // Borderless draws the largest original inside the desktop over it; a
    // window of that original's size is the baseline. Integer output makes
    // every native RGB pixel directly comparable. The separate SDL quadrant
    // test covers fractional fit and stretched output.
    let desktop = monitor().desktop.expect("SDL reports the desktop");
    let layout = original_size(tier_for(desktop));
    for (tag, frames, skip_intro, expected_phase) in [
        ("intro", 300, false, SessionPhase::Gameplay),
        ("briefing", 380, true, SessionPhase::Phase3),
    ] {
        let mut baseline = None;
        for mode in [DisplayMode::Window, DisplayMode::Borderless] {
            let mut request = session_with_display(
                &format!("display-{tag}-{mode:?}"),
                DisplaySelection { mode, size: layout },
            );
            request.hidden = true;
            request.max_frames = Some(frames);
            // Leave the window's scaling unoverridden to cover the default.
            if mode == DisplayMode::Borderless {
                request.scaling = Some(ClassicScaling::Integer);
            }
            let resolution = format!("{mode:?}");
            request
                .input_replay
                .extend([key(260, "Tab", true), key(261, "Tab", false)]);
            if skip_intro {
                request
                    .input_replay
                    .extend([key(320, "Escape", true), key(321, "Escape", false)]);
            }
            let snapshot = request.settings_path.with_file_name("output.png");
            request.snapshot = Some(snapshot.clone());
            let report = run_frontend(request.clone()).unwrap();
            assert_eq!(report.phase, expected_phase, "{tag}/{resolution}");
            assert_eq!(report.frames, frames);
            assert_eq!(report.dimensions, layout);
            assert_eq!(
                report.output_dimensions,
                match mode {
                    DisplayMode::Borderless => desktop,
                    _ => layout,
                }
            );
            // Without optional transcoded title assets, native 00404840's
            // failed-open path presents/clears nothing on the first frame.
            assert!(matches!(
                frames as usize + report.loading.presentations - report.scene.presentation_clears,
                0 | 1
            ));
            assert!(report.loading.callbacks > 0);
            assert_eq!(report.loading.presentations, report.loading.callbacks);
            assert_eq!(report.loading.contained_errors, 0);
            // The post-intro loading model's register 3 starts from
            // Infest01's stack, so it draws whole (MODEL_RUNTIME.md,
            // LoadingModel117StackHistoryApproximation).
            assert_eq!(report.scene.model_value_prefix_skips, 0);
            if skip_intro {
                assert!(report.loading.model_background_draws > 0);
                assert!(report.scene.retired_resource_batches > 0);
            } else {
                assert_eq!(report.loading.model_background_draws, 0);
            }
            eprintln!("{tag}/{resolution} consumed loading: {:?}", report.loading);
            assert!(report.scene.terrain_commands > 0);
            assert_eq!(report.pixels.unsupported, 0);
            assert_eq!(report.display_rebuilds, 0);
            assert_eq!(report.snapshot.as_ref(), Some(&snapshot));
            let mut phases = vec![
                SessionPhase::Frontend,
                SessionPhase::Phase9,
                SessionPhase::Phase3,
                SessionPhase::Gameplay,
            ];
            if skip_intro {
                phases.extend([SessionPhase::Phase8, SessionPhase::Phase3]);
            } else {
                let world = report.world_draw.expect("intro reached the world renderer");
                assert!(world.native_camera);
                assert!(world.outcome.terrain_commands > 0);
            }
            assert_eq!(
                report
                    .transitions
                    .iter()
                    .map(|t| t.effective)
                    .collect::<Vec<_>>(),
                phases,
                "{tag}/{resolution}"
            );
            let assets = FrontendAssets::open(&request.data_root).unwrap();
            let saved =
                FrontendSettings::load_or_defaults(&request.settings_path, &assets).unwrap();
            assert_eq!(
                saved.words()[0x0c / 4],
                tier_for(layout),
                "the desktop's tier"
            );
            assert_eq!(saved.display_selection(), report.display);
            assert_eq!(saved.scaling(), report.scaling);
            let (size, pixels) = read_rgba_snapshot(&snapshot);
            assert_eq!(size, report.output_dimensions);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[..3] != [0, 0, 0]));

            if mode == DisplayMode::Window {
                // The default widescreen draws a 4:3 window's own tier frame.
                assert_eq!(report.scaling, ClassicScaling::Widescreen);
                baseline = Some((report, pixels));
                continue;
            }
            let (native, native_pixels) = baseline.as_ref().unwrap();
            assert_eq!(
                report.scene, native.scene,
                "{tag}/{resolution} native draws"
            );
            assert_eq!(report.pixels.fragments, native.pixels.fragments);
            assert_eq!(
                report.world_draw, native.world_draw,
                "native camera/projection"
            );
            let rect = ClassicScaling::Integer.destination(native.dimensions, size);
            let scale = rect.width / native.dimensions[0];
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let local = [x as i32 - rect.x, y as i32 - rect.y];
                    let offset = ((y * size[0] + x) * 4) as usize;
                    let expected = if local[0] >= 0
                        && local[1] >= 0
                        && local[0] < rect.width as i32
                        && local[1] < rect.height as i32
                    {
                        let source_x = local[0] as u32 / scale;
                        let source_y = local[1] as u32 / scale;
                        let source = ((source_y * native.dimensions[0] + source_x) * 4) as usize;
                        &native_pixels[source..source + 3]
                    } else {
                        &[0, 0, 0]
                    };
                    assert_eq!(
                        &pixels[offset..offset + 3],
                        expected,
                        "{tag}/{resolution} RGB pixel ({x},{y})"
                    );
                    assert_eq!(pixels[offset + 3], 255, "SDL output must remain opaque");
                }
            }
        }
    }
}

#[test]
#[ignore = "changes a real SDL display mode; run explicitly in release mode"]
fn video_fullscreen_startup_can_return_to_windowed_through_the_authored_row() {
    // Exclusive legacy modes depend on the actual monitor/driver. A missing
    // mode is not evidence that the portable reset succeeded.
    let supports_mode = {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        (0..video.num_display_modes(0).unwrap()).any(|index| {
            let mode = video.display_mode(0, index).unwrap();
            mode.w == 800 && mode.h == 600
        })
    };
    if !supports_mode {
        eprintln!("800x600 exclusive display mode unavailable; fullscreen hardware case skipped");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join(format!("../.tmp/inf-fullscreen-window-{nonce}"));
    let settings = directory.join("settings.json");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut saved = FrontendSettings::load_or_defaults(&settings, &assets).unwrap();
    saved.words_mut()[0x40 / 4] = 1;
    saved.persist().unwrap();
    // A present title video holds frames 1..4 until Return at frame 5.
    let title_frames = if root.join("video").is_dir() { 4 } else { 0 };
    // A hidden SDL window only records fullscreen flags. Present through a
    // shown exclusive surface before testing the authored return action.
    let entered = run_frontend(FrontendAppRequest {
        data_root: root.clone(),
        settings_path: settings.clone(),
        profiles_path: directory.join("profiles.sav"),
        input_replay: Vec::new(),
        dimensions: Some([800, 600]),
        scaling: None,
        vehicle_position_carry: None,
        vehicle_gun_mounts: None,
        hovercraft_fans: None,
        exact_world_ticks: None,
        ultra_ground: None,
        ambience: None,
        headphones: None,
        database_models: None,
        exact_frontend_ticks: None,
        guided_mounts: None,
        completion_bonus: None,
        paced_actors: None,
        mission_completion: None,
        splash_voice_cap: None,
        all_cheats: None,
        prototype_world: None,
        pelagia_rework: None,
        renderer: Some(RendererChoice::Software),
        controller_layout: None,
        variant: None,
        language: None,
        hidden: false,
        max_frames: Some(1),
        snapshot: None,
        snapshots_at: Vec::new(),
        record_session: None,
        audio_out: None,
        mission: None,
        start: None,
        clock: FrontendAppClock::FixedStep { micros: 125_000 },
        session_record: None,
        session_replay: None,
    })
    .unwrap();
    assert!(entered.fullscreen);
    assert_eq!(entered.dimensions, [800, 600]);
    assert_eq!(entered.frames, 1);
    assert_eq!(entered.scene.presentation_clears, 1);
    // The first frame is the frontend's unless the title video plays first.
    if title_frames == 0 {
        assert!(entered.pixels.fragments > 0);
    }
    let mut input = Vec::new();
    let mut press = |frame, name| {
        input.extend([key(frame, name, true), key(frame + 1, name, false)]);
    };
    press(5, "Return"); // Title skip (when video/ present; no-op on failed-open).
    press(100, "Down");
    press(105, "Return");
    press(200, "Return");
    for index in 0..6 {
        press(300 + index * 3, "Down");
    }
    press(330, "Left"); // Full screen -> Off
    press(340, "Escape");
    let report = run_frontend(FrontendAppRequest {
        data_root: root,
        settings_path: settings.clone(),
        profiles_path: directory.join("profiles.sav"),
        input_replay: input,
        dimensions: Some([800, 600]),
        scaling: None,
        vehicle_position_carry: None,
        vehicle_gun_mounts: None,
        hovercraft_fans: None,
        exact_world_ticks: None,
        ultra_ground: None,
        ambience: None,
        headphones: None,
        database_models: None,
        exact_frontend_ticks: None,
        guided_mounts: None,
        completion_bonus: None,
        paced_actors: None,
        mission_completion: None,
        splash_voice_cap: None,
        all_cheats: None,
        prototype_world: None,
        pelagia_rework: None,
        renderer: Some(RendererChoice::Software),
        controller_layout: None,
        variant: None,
        language: None,
        hidden: false,
        max_frames: Some(360),
        snapshot: None,
        snapshots_at: Vec::new(),
        record_session: None,
        audio_out: None,
        mission: None,
        start: None,
        clock: FrontendAppClock::FixedStep { micros: 125_000 },
        session_record: None,
        session_replay: None,
    })
    .unwrap();
    assert_eq!(report.dimensions, [800, 600]);
    assert!(!report.fullscreen);
    assert_eq!(report.display_rebuilds, 1);
    assert_eq!(report.age_millis, Some((360 - title_frames) * 125));
    assert_eq!(
        FrontendSettings::load_or_defaults(settings, &assets)
            .unwrap()
            .words()[0x40 / 4],
        0
    );
}
