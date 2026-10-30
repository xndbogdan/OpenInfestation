use std::path::Path;

use super::*;
use crate::frontend_assets::FrontendAssets;

#[derive(Debug)]
enum Event {
    Command(SceneCommand),
    Style,
    Dimensions(u16),
}

#[derive(Default)]
struct RecordingHost {
    events: Vec<Event>,
    fail_model: bool,
    viewport: Option<[i32; 2]>,
}

impl FrontendSceneHost for RecordingHost {
    type Error = &'static str;

    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.events.push(Event::Command(command));
        if matches!(command, SceneCommand::Star(star) if star.color_slot() < 0) {
            Err("negative color resource slot")
        } else if self.fail_model && matches!(command, SceneCommand::Model(_)) {
            Err("model backend unavailable")
        } else {
            Ok(())
        }
    }

    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        self.events.push(Event::Style);
        Ok(0x8765_4321)
    }

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.viewport.unwrap_or([640, 480]))
    }

    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error> {
        self.events.push(Event::Dimensions(slot));
        // Deliberately unequal fixture dimensions expose native placement
        // dependencies. These are recording-backend fixtures, not asset claims.
        Ok(match slot {
            1807 => [100, 20],
            1806 => [80, 10],
            1809 => [25, 10],
            1808 => [40, 20],
            1810 => [35, 15],
            1811 => [30, 25],
            _ => return Err("unexpected sprite"),
        })
    }
}

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn frame() -> SceneFrame {
    SceneFrame {
        projection: SceneProjection {
            focal: [160, 120],
            viewport: [640, 480],
            fade: [10000, 20000],
        },
        inherited_view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        camera_eye: [0; 3],
        camera_look_at: [0, 0, 1],
        animation: AnimationClock::default(),
        age_millis: 2000,
        exit_millis: 0,
    }
}

// This recording-only harness composes the two native phases on one fixture
// renderer. The production frame owns their distinct renderer lookups.
fn draw_background(
    scene: &FrontendScene,
    frame: SceneFrame,
    host: &mut RecordingHost,
) -> Result<SceneDrawOutcome, &'static str> {
    let result = scene.draw_scene(frame, host)?;
    host.viewport = Some(frame.projection.viewport);
    draw_footer(host)?;
    host.submit(SceneCommand::FlushBatch)?;
    Ok(result)
}

fn commands(host: &RecordingHost) -> Vec<SceneCommand> {
    host.events
        .iter()
        .filter_map(|event| match event {
            Event::Command(command) => Some(*command),
            _ => None,
        })
        .collect()
}

fn stars(host: &RecordingHost) -> Vec<StarQuad> {
    commands(host)
        .into_iter()
        .filter_map(|command| match command {
            SceneCommand::Star(star) => Some(star),
            _ => None,
        })
        .collect()
}

fn footer(host: &RecordingHost) -> Vec<FooterQuad> {
    commands(host)
        .into_iter()
        .filter_map(|command| match command {
            SceneCommand::Footer(quad) => Some(quad),
            _ => None,
        })
        .collect()
}

#[test]
fn retail_scene_submits_all_models_in_authored_order_between_stars_and_footer() {
    let assets = assets();
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let mut host = RecordingHost::default();
    let mut input = frame();
    input.camera_eye = [-12000, 0, 70000];
    input.camera_look_at = [20000, 0, 50000];
    let outcome = draw_background(&scene, input, &mut host).unwrap();
    assert_eq!(outcome.generated_stars, 10000);
    assert!(outcome.submitted_stars > 0);
    assert_eq!(outcome.submitted_models, 9);
    let commands = commands(&host);
    assert!(matches!(commands[0], SceneCommand::CoordinateShift(0)));
    assert!(matches!(
        commands[1],
        SceneCommand::DirectionalLight {
            rgb: [255, 210, 170],
            ..
        }
    ));
    assert!(matches!(
        commands[2],
        SceneCommand::AmbientLight([10, 6, 5])
    ));
    let models: Vec<_> = commands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            SceneCommand::Model(model) => Some((i, *model)),
            _ => None,
        })
        .collect();
    assert_eq!(
        models.iter().map(|(_, m)| m.model_slot).collect::<Vec<_>>(),
        [744, 749, 754, 756, 745, 748, 747, 755, 757]
    );
    for (model_index, (command_index, model)) in models.iter().enumerate() {
        assert_eq!(
            model.position,
            assets.scene_models()[model_index]
                .position
                .map(|v| i32::from(v) * 10)
        );
        let SceneCommand::PublishView(view) = commands[command_index - 2] else {
            panic!("missing view publication")
        };
        assert_eq!(
            view.eye,
            if model_index == 0 {
                [0, 0, -70000]
            } else {
                input.camera_eye
            }
        );
        assert!(matches!(
            commands[command_index - 1],
            SceneCommand::BeginBatch
        ));
        assert!(matches!(
            commands[command_index + 1],
            SceneCommand::FlushBatch
        ));
    }
    let first = models[0].0;
    assert!(commands[..first]
        .iter()
        .any(|c| matches!(c, SceneCommand::Star(_))));
    assert!(!commands[first..]
        .iter()
        .any(|c| matches!(c, SceneCommand::Star(_))));
    assert!(matches!(
        commands[first + 2],
        SceneCommand::ResetAfterFirstModel
    ));
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, SceneCommand::ResetAfterFirstModel))
            .count(),
        1
    );
    assert!(matches!(commands.last(), Some(SceneCommand::FlushBatch)));
    assert_eq!(
        footer(&host)
            .iter()
            .map(|q| q.sprite_slot)
            .collect::<Vec<_>>(),
        [1807, 1806, 1809, 1808, 1810, 1811]
    );
    assert_eq!(outcome.last_view.unwrap().eye, input.camera_eye);
}

#[test]
fn authored_positive_pass_precedes_clock_shift_pass_and_callback_is_native() {
    let assets = assets();
    let mut model = assets.scene_models()[0];
    model.rotation_controls = [-4, 0x3000, 0x1000];
    model.position = [-32768, 32767, -1];
    let scene = FrontendScene::new(vec![model], i32::MAX, assets.sine_table().values);
    let mut clock = AnimationClock::default();
    clock.advance(20000 * 12345);
    let submission = scene.model_submission(&model, clock);
    let mut expected = MenuBasis::INITIAL;
    scene
        .math
        .rotate_in_plane(&mut expected, 0x1000, RotationPlane::Xy);
    scene
        .math
        .rotate_in_plane(&mut expected, 0x3000, RotationPlane::Xz);
    scene
        .math
        .rotate_in_plane(&mut expected, 12345 << 4, RotationPlane::Yz);
    assert_eq!(submission.basis, expected);
    assert_eq!(
        submission.position,
        [-32768i32, 32767, -1].map(|v| v.wrapping_mul(i32::MAX))
    );
    assert_eq!(submission.callback_value(0), Some(12345));
    assert_eq!(submission.callback_value(0x40), Some(0)); // consumer, not callback, masks
    assert_eq!(submission.callback_value(1), Some(0));
    model.rotation_controls = [i32::MIN, 0, 0];
    let mut expected = MenuBasis::INITIAL;
    scene
        .math
        .rotate_in_plane(&mut expected, 12345, RotationPlane::Yz);
    assert_eq!(scene.model_submission(&model, clock).basis, expected);
}

#[test]
fn sky_static_values_approximation_remains_independent_of_actor_callbacks() {
    let submission = SceneModelSubmission {
        model_slot: 0x228,
        position: [0; 3],
        basis: MenuBasis::INITIAL,
        animation: AnimationClock::default(),
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
        policy: ModelCallbackPolicy::WorldSkyStaticValuesApproximation,
    };
    // Sky-only static approximation: no authenticated object packet. It does
    // not substitute for the separate actual-object callback/clock owner.
    for selector in [0, 1, 2, 6, 0x40] {
        assert_eq!(submission.callback_value(selector), Some(0));
    }
}

#[test]
fn starfield_uses_inherited_view_reseeds_each_frame_and_obeys_fades() {
    let scene = FrontendScene::new(Vec::new(), 1, assets().sine_table().values);
    let mut a = RecordingHost::default();
    let mut b = RecordingHost::default();
    let input = frame();
    draw_background(&scene, input, &mut a).unwrap();
    let mut other = input;
    other.camera_eye = [-999, 4242, 777];
    draw_background(&scene, other, &mut b).unwrap();
    assert_eq!(stars(&a), stars(&b));
    assert!(stars(&a).iter().any(|star| star.color_slot() > 37));
    let mut c = RecordingHost::default();
    other.age_millis = 0;
    other.exit_millis = 1000;
    draw_background(&scene, other, &mut c).unwrap();
    assert_eq!(stars(&a).len(), stars(&c).len());
    assert!(stars(&c).iter().all(|star| star.color_slot() == 37));
    let mut d = RecordingHost::default();
    other.age_millis = 2000;
    draw_background(&scene, other, &mut d).unwrap();
    assert!(stars(&d)
        .iter()
        .all(|star| (37..=38).contains(&star.color_slot())));
    other.inherited_view.eye = [70000, 0, 0];
    let mut e = RecordingHost::default();
    draw_background(&scene, other, &mut e).unwrap();
    assert_ne!(stars(&a), stars(&e));
}

#[test]
fn wrapped_fade_color_keeps_full_loader_argument_and_signed_word_resource_index() {
    let scene = FrontendScene::new(Vec::new(), 1, assets().sine_table().values);
    let mut input = frame();
    input.exit_millis = -3_300_000;
    let mut host = RecordingHost::default();
    assert_eq!(
        draw_background(&scene, input, &mut host),
        Err("negative color resource slot")
    );
    let Some(Event::Command(SceneCommand::Star(star))) = host.events.last() else {
        panic!("missing failed star submission")
    };
    assert!((33037..=33048).contains(&star.color_argument));
    assert!((-32499..=-32488).contains(&star.color_slot()));
    assert!(!host
        .events
        .iter()
        .any(|event| matches!(event, Event::Style)));
}

#[test]
fn footer_preserves_unequal_width_dependencies_and_lookup_order() {
    let mut host = RecordingHost::default();
    draw_footer(&mut host).unwrap();
    assert!(matches!(host.events[0], Event::Style));
    let expected = [
        (1807, [[16, 448], [116, 448], [116, 468], [16, 468]]),
        (1806, [[544, 458], [624, 458], [624, 468], [544, 468]]),
        (1809, [[300, 460], [320, 460], [320, 468], [300, 468]]),
        (1808, [[280, 460], [312, 460], [312, 476], [280, 476]]),
        (1810, [[320, 460], [348, 460], [348, 472], [320, 472]]),
        (1811, [[348, 460], [372, 460], [372, 480], [348, 480]]),
    ];
    for ((slot, corners), quad) in expected.into_iter().zip(footer(&host)) {
        assert_eq!(
            quad,
            FooterQuad {
                sprite_slot: slot,
                corners,
                style_word: 0x8765_4321,
                vertex_attribute: 0x40
            }
        );
    }
    for pair in host.events[1..].chunks_exact(2) {
        let [Event::Dimensions(slot), Event::Command(SceneCommand::Footer(quad))] = pair else {
            panic!("lookup/submission order changed")
        };
        assert_eq!(*slot, quad.sprite_slot);
    }
}

#[test]
fn footer_strip_downscale_depends_on_width_only() {
    let mut host = RecordingHost {
        viewport: Some([800, 900]),
        ..Default::default()
    };
    draw_footer(&mut host).unwrap();
    let quads = footer(&host);
    assert_eq!(
        quads[0].corners,
        [[20, 841], [145, 841], [145, 878], [20, 878]]
    );
    assert_eq!(
        quads[2].corners,
        [[375, 868], [400, 868], [400, 878], [375, 878]]
    );
    assert_eq!(
        quads[3].corners,
        [[350, 868], [390, 868], [390, 888], [350, 888]]
    );
    assert_eq!(footer_strip_size([25, 10], [799, 900]), [24, 15]);
}

#[test]
fn camera_basis_zero_parallel_and_axis_directions_match_native_fallbacks() {
    assert_eq!(camera_basis([0; 3], [0, 0, 1]), MenuBasis::INITIAL);
    let vector = |basis: MenuBasis, first: usize| {
        std::array::from_fn::<_, 3, _>(|i| f32::from_bits(basis.words[first + i]))
    };
    let zero = camera_basis([42; 3], [42; 3]);
    assert_eq!(vector(zero, 7), [1.0, 0.0, 0.0]);
    assert_eq!(vector(zero, 1), [0.0, 0.0, -1.0]);
    assert_eq!(vector(zero, 4), [0.0, 1.0, 0.0]);
    let vertical = camera_basis([0; 3], [0, 10, 0]);
    assert_eq!(vector(vertical, 1), [1.0, 0.0, 0.0]);
    assert_eq!(vector(vertical, 4), [0.0, 0.0, -1.0]);
    assert_eq!(vector(vertical, 7), [0.0, 1.0, 0.0]);
}

#[test]
fn projection_preserves_near_gate_truncation_and_signed_coordinate_reduction() {
    let setup = SceneSetup::new(frame().projection);
    assert_eq!(setup.focal, [320, 240]);
    assert_eq!(setup.center, [320, 240]);
    assert_eq!(setup.fade_factor, 1677);
    assert_eq!(project_star([0, 0, 63], setup), None);
    assert_eq!(project_star([-1, 1, 1000], setup), Some([320, 240]));
    assert_eq!(project_star([0, 0, 64], setup), Some([320, 240]));
    assert_eq!(project_star([100000, 0, 64], setup), None);
    let large = SceneSetup {
        focal: [1, 1],
        viewport: [65536, 65536],
        center: [32767, 0],
        ..setup
    };
    assert_eq!(project_star([0, 0, 64], large), Some([16383, 0]));
    assert_eq!(integer_low32(4294967295.0), -1);
    assert_eq!(integer_low32(f64::INFINITY), 0);
    assert_eq!(integer_low32(f64::NAN), 0);
}

#[test]
fn required_backend_failure_keeps_prior_view_publication_and_stops_submission() {
    let assets = assets();
    let scene = FrontendScene::new(
        vec![assets.scene_models()[0]],
        0,
        assets.sine_table().values,
    );
    let mut host = RecordingHost {
        fail_model: true,
        ..Default::default()
    };
    assert_eq!(
        draw_background(&scene, frame(), &mut host),
        Err("model backend unavailable")
    );
    let commands = commands(&host);
    assert!(matches!(commands.last(), Some(SceneCommand::Model(_))));
    assert!(commands.iter().any(|c| matches!(
        c,
        SceneCommand::PublishView(SceneView {
            eye: [0, 0, -70000],
            ..
        })
    )));
    assert!(!host
        .events
        .iter()
        .any(|e| matches!(e, Event::Style | Event::Dimensions(_))));
}
