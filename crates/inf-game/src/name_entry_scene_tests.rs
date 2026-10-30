use std::path::Path;

use inf_formats::ModelInstructionKind;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};

fn math() -> MenuBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    MenuBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

#[derive(Default)]
struct Recording {
    commands: Vec<SceneCommand>,
}

impl FrontendSceneHost for Recording {
    type Error = std::convert::Infallible;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.commands.push(command);
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("0045A560 draws no footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("0045A560 receives the surface through its projection")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("0045A560 draws no sprite")
    }
}

fn frame(age_millis: i32, surface: [i32; 2], focal: [i32; 2]) -> NameEntrySceneFrame {
    NameEntrySceneFrame {
        projection: SceneProjection {
            focal,
            viewport: surface,
            fade: [0x10_0000; 2],
        },
        age_millis,
        animation: AnimationClock::default(),
        selector_one: 1,
    }
}

fn draw(frame: NameEntrySceneFrame, math: &MenuBasisMath) -> Vec<SceneCommand> {
    let mut host = Recording::default();
    draw_name_entry_scene(frame, math, &mut host).unwrap();
    host.commands
}

#[test]
fn backdrop_lights_publishes_and_queues_three_models_without_batches() {
    let commands = draw(frame(2000, [640, 480], [320, 320]), &math());
    let kinds: Vec<&str> = commands
        .iter()
        .map(|command| match command {
            SceneCommand::DirectionalLight { .. } => "directional",
            SceneCommand::AmbientLight(_) => "ambient",
            SceneCommand::Setup(_) => "setup",
            SceneCommand::PublishView(_) => "view",
            SceneCommand::Model(_) => "model",
            other => panic!("0045A560 does not emit {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "directional",
            "ambient",
            "setup",
            "view",
            "model",
            "model",
            "model"
        ]
    );
    let slots: Vec<i16> = commands
        .iter()
        .filter_map(|command| match command {
            SceneCommand::Model(model) => Some(model.model_slot),
            _ => None,
        })
        .collect();
    assert_eq!(slots, NAME_ENTRY_BACKDROP_MODELS);
    // Unlike the frontend (00483D41), the focal pair is not doubled.
    let SceneCommand::Setup(setup) = commands[2] else {
        unreachable!()
    };
    assert_eq!(setup.focal, [320, 320]);
    assert_eq!(setup.center, [320, 240]);
    assert_eq!(setup.fade_factor, 0x100_0000);
}

#[test]
fn name_entry_callback_policy_differs_from_the_frontend_only_for_1_2_and_6() {
    let mut animation = AnimationClock::default();
    animation.advance(0x1_2345 * 20_000);
    let submission = |policy| SceneModelSubmission {
        model_slot: 748,
        position: [0; 3],
        basis: MenuBasis::INITIAL,
        animation,
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
        policy,
    };
    let name_entry = submission(ModelCallbackPolicy::NameEntry { selector_one: 2 });
    let frontend = submission(ModelCallbackPolicy::Frontend);
    for (selector, expected) in [
        (0, 0x2345),
        (1, 2),
        (2, 0xffff),
        (3, 0),
        (4, 0),
        (5, 0),
        (6, 1),
        (7, 0),
        (0x100, 0),
    ] {
        assert_eq!(
            name_entry.callback_value(selector),
            Some(expected),
            "{selector}"
        );
        let frontend_expected = if selector == 0 { 0x2345 } else { 0 };
        assert_eq!(frontend.callback_value(selector), Some(frontend_expected));
    }
}

#[derive(serde::Deserialize)]
struct NativeModel {
    slot: i16,
    position: [i32; 3],
    basis: [u32; 10],
}

#[derive(serde::Deserialize)]
struct NativeCase {
    age: i64,
    surface: [i32; 2],
    focal: [i32; 2],
    direction: [u32; 3],
    directional_rgb: [u8; 3],
    ambient: [i32; 3],
    ambient_bytes: [u8; 3],
    projection: [i32; 6],
    fade: [i32; 3],
    view: [u32; 13],
    models: Vec<NativeModel>,
}

/// 0045A560 executed unmodified with 0049FEA0 recorded
/// (.tmp/inf-name-entry-scene-oracle.py), statics seeded from the phase-9
/// values in the retail recording.
#[test]
fn backdrop_matches_native_0045a560_receipts() {
    let cases: Vec<NativeCase> = serde_json::from_str(include_str!(
        "../tests/fixtures/name_entry_scene_native.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 26);
    let math = math();
    for case in cases {
        let age = case.age as i32;
        let commands = draw(frame(age, case.surface, case.focal), &math);
        let SceneCommand::DirectionalLight { direction, rgb } = commands[0] else {
            unreachable!()
        };
        assert_eq!(direction.map(f32::to_bits), case.direction, "age {age}");
        assert_eq!(rgb, case.directional_rgb, "age {age}");
        let SceneCommand::AmbientLight(ambient) = commands[1] else {
            unreachable!()
        };
        assert_eq!(ambient, case.ambient, "age {age}");
        // 00490130's separately clamped bytes.
        assert_eq!(
            ambient.map(|value| value.clamp(0, 255) as u8),
            case.ambient_bytes
        );
        let SceneCommand::Setup(setup) = commands[2] else {
            unreachable!()
        };
        let published = [
            setup.focal[0],
            setup.focal[1],
            setup.viewport[0],
            setup.viewport[1],
            setup.center[0],
            setup.center[1],
        ];
        assert_eq!(published, case.projection, "age {age}");
        assert_eq!([setup.fade_factor, setup.fade[0], setup.fade[1]], case.fade);
        let SceneCommand::PublishView(view) = commands[3] else {
            unreachable!()
        };
        let mut words = view.eye.map(|v| v as u32).to_vec();
        words.extend(view.basis.words);
        assert_eq!(words, case.view, "age {age}");
        for (command, model) in commands[4..].iter().zip(&case.models) {
            let SceneCommand::Model(submission) = command else {
                unreachable!()
            };
            assert_eq!(submission.model_slot, model.slot);
            assert_eq!(
                submission.position, model.position,
                "age {age} {}",
                model.slot
            );
            assert_eq!(
                submission.basis.words, model.basis,
                "age {age} {}",
                model.slot
            );
        }
    }
}

/// Every packed operand the program walker evaluates: register sources,
/// branch operands, rotation angles, weighted-vertex weights and effect
/// size/angle. Returns callback selectors (0x80 set, 0x40 clear).
fn packed_selectors(entry: &inf_formats::ModelEntry) -> (Vec<u16>, Vec<i16>, usize) {
    let mut selectors = Vec::new();
    let mut children = Vec::new();
    let mut computed = 0;
    for instruction in &entry.program.instructions {
        let opcode = instruction.raw_command as u8;
        let arguments: &[u16] = match &instruction.kind {
            ModelInstructionKind::RegisterOperation(command)
            | ModelInstructionKind::UnresolvedDynamic(command) => &command.arguments,
            ModelInstructionKind::Opaque(command) => &command.arguments,
            ModelInstructionKind::ModelInstance(instance) => {
                children.push(instance.model.raw());
                &[]
            }
            _ => &[],
        };
        let operands = match opcode {
            0xfa => {
                computed += 1;
                arguments.get(1..3)
            }
            0xf0..=0xff | 0x16 => arguments.get(1..3),
            0x03 => arguments.get(2..3),
            0x26 => arguments.get(3..4),
            0x1b => arguments.get(2..4),
            _ => None,
        };
        selectors.extend(
            operands
                .unwrap_or_default()
                .iter()
                .filter(|raw| *raw & 0xc0 == 0x80)
                .map(|raw| raw & 0xff3f),
        );
    }
    (selectors, children, computed)
}

#[test]
fn retail_backdrop_models_consume_only_the_shared_clock_selector() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resources = RenderResources::new(&runtime);
    for slot in NAME_ENTRY_BACKDROP_MODELS {
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: i32::from(slot),
            })
            .unwrap();
        let RenderResourceData::Model(entry) = resource.data else {
            panic!("model resource")
        };
        let (selectors, children, computed) = packed_selectors(entry);
        // No child invocations and no computed (0xFA) selectors, so the
        // 0043A270/00483C40 difference at selectors 1, 2 and 6 is unused.
        assert!(children.is_empty(), "model {slot} children {children:?}");
        assert_eq!(computed, 0, "model {slot}");
        assert!(
            selectors.iter().all(|selector| *selector == 0),
            "model {slot} selectors {selectors:?}"
        );
        assert_eq!(selectors.is_empty(), slot != 757, "model {slot}");
    }
}
