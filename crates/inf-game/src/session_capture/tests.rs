use super::*;

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "inf-capture-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn capture_preserves_observed_pixels_pcm_and_monotonic_timestamps() {
    let directory = directory("bytes");
    let owner = SessionCapture::start(&directory, [1024, 768]).unwrap();
    let handle = owner.handle();
    let pixels = [20, 40, 60, 255].repeat(1024 * 768);
    let samples = [0.25_f32, -0.75, 0.0, 1.0];
    let audio_time = handle.elapsed_ns();
    handle.audio(audio_time, &samples);
    handle.frame([1024, 768], pixels.clone(), "loading", None, 12);
    let world = WorldPresentationState {
        session_phase: 4,
        current_world: 22,
        forced_black: false,
        animation_clock: 123,
        timing_average_microseconds: 80_000,
        particle_load_factor_16_16: 20_000,
        manager_far_1c: 6_144,
        manager_header_20: Some(2_560),
        retained_predraw_5c8_5cc: [6_208, 2_496],
        terrain_spacing_units: 128,
        renderer_view_eye: [32_768, -400, 65_535],
        renderer_projection_fade: [928, 6_208],
    };
    handle.frame([1024, 768], pixels.clone(), "linked", Some(world), 34);
    owner.finish().unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("capture.json")).unwrap()).unwrap();
    assert_eq!(manifest["complete"], true);
    assert_eq!(manifest["frames"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["frames"][0]["presentation"], "loading");
    assert!(manifest["frames"][0]["world"].is_null());
    assert_eq!(manifest["frames"][0]["readback_ns"], 12);
    assert_eq!(manifest["frames"][1]["readback_ns"], 34);
    assert_eq!(
        manifest["frames"][1]["world"],
        serde_json::to_value(world).unwrap()
    );
    assert_eq!(
        manifest["audio"]["chunks"][0]["callback_elapsed_ns"],
        audio_time
    );
    assert_eq!(manifest["audio"]["sample_frames"], 2);
    let frame_time = manifest["frames"][0]["elapsed_ns"].as_u64().unwrap();
    assert!(frame_time >= audio_time);
    assert!(manifest["finished_elapsed_ns"].as_u64().unwrap() >= frame_time);
    assert_eq!(
        fs::read(directory.join("audio.f32")).unwrap(),
        samples
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    );
    let decoder = png::Decoder::new(File::open(directory.join("frames/00000000.png")).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut decoded).unwrap();
    assert_eq!([info.width, info.height], [1024, 768]);
    assert_eq!(&decoded[..info.buffer_size()], &pixels);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn capture_overflow_is_nonblocking_and_incomplete_inputs_are_rejected() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let handle = CaptureHandle {
        sender,
        state: Arc::new(State {
            epoch: Instant::now(),
            stop: AtomicBool::new(false),
            complete: AtomicBool::new(false),
            finished_ns: AtomicU64::new(0),
            failure: AtomicU8::new(0),
        }),
    };
    handle.audio(0, &[0.0, 0.0]);
    handle.audio(1, &[0.0, 0.0]);
    assert!(handle.check().unwrap_err().to_string().contains("overflow"));
    let directory = directory("missing");
    assert!(SessionCapture::start(&directory, [640, 480]).is_err());
    let owner = SessionCapture::start(&directory, [1024, 768]).unwrap();
    assert!(SessionCapture::start(&directory, [1024, 768]).is_err());
    assert!(owner.finish().is_err());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("capture.json")).unwrap()).unwrap();
    assert_eq!(manifest["complete"], false);
    fs::remove_dir_all(directory).unwrap();
}
