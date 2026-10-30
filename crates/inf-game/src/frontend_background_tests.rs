//! The first-entry camera exposes near-plane paths absent in fixed-pose tests.
use super::*;

#[test]
fn first_entry_camera_renders_six_hundred_consecutive_frames() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail frontend executable");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let resources = RenderResources::new(&runtime);
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let mut camera = FrontendCamera::new(
        &assets.camera_presets()[0],
        assets.position_scale(),
        FrontendEntry::First,
    );
    let mut animation = AnimationClock::default();
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut backend = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    for number in 1..=600 {
        animation.advance(16_000);
        camera.advance(16_000, CameraMotion::FirstEntry, assets.motion_scale());
        backend.begin_frame().unwrap();
        let input = SceneFrame {
            projection: startup.projection,
            inherited_view: backend.environment.view,
            camera_eye: camera.eye().position,
            camera_look_at: camera.look_at().position,
            animation,
            age_millis: number * 16,
            exit_millis: 0,
        };
        draw_background_frame(&scene, input, resources, &mut backend)
            .unwrap_or_else(|error| panic!("first-entry frame {number}: {error:?}"));
        if number == 600 {
            let changed = backend
                .pixels
                .frame
                .rgba()
                .chunks_exact(4)
                .filter(|pixel| **pixel != startup.clear_rgba)
                .count();
            assert!(
                changed > 10_000,
                "final frame changed only {changed} pixels"
            );
        }
        backend.after_present(startup.clear_rgba).unwrap();
    }
    assert_eq!(backend.stats.models, 5400);
    assert_eq!(backend.stats.footers, 3600);
    assert_eq!(backend.stats.frame_begins, 600);
    assert_eq!(backend.stats.presentation_clears, 600);
    assert_eq!(backend.pixels.stats.unsupported, 0);
    assert!(backend.stats.stars > 100_000);
}
