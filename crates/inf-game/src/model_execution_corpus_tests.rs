//! Corpus execution ending at an explicit recording primitive backend. These
//! checks establish prepared native inputs and call order, never visible pixels.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{ModelQuadLighting, ModelSlotRef};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::{AnimationClock, FrontendCamera, FrontendEntry},
    frontend_scene::{
        FrontendScene, FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneFrame,
        SceneModelSubmission, SceneProjection,
    },
    menu::MenuBasis,
    model_lighting::ModelShade,
    model_scratch::ProcessModelScratch,
    model_vertices::ModelVertexProjection,
    Installation, ResidencyRequest, ResidentRuntime,
};

const ROOTS: [u16; 9] = [744, 749, 754, 756, 745, 748, 747, 755, 757];
const CHILDREN: [u16; 4] = [746, 751, 752, 753];

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedVertex {
    slot: u16,
    world: Option<[i32; 3]>,
    projection: ModelVertexProjection,
}

#[derive(Debug)]
struct RecordedPrimitive {
    model: u16,
    opcode: u8,
    source_offset: usize,
    position: [i32; 3],
    state_words: Vec<u16>,
    vertices: Vec<PreparedVertex>,
    shades: Vec<ModelShade>,
    phase_register: Option<u16>,
    effect_registers: [Option<u16>; 2],
}

struct RecordingBackend {
    submission: SceneModelSubmission,
    expected_dispatch: Option<ModelDispatch>,
    primitives: Vec<RecordedPrimitive>,
    publications: Vec<ModelContext>,
    selections: Vec<ModelDispatch>,
    depth_events: Vec<Option<i32>>,
    callbacks: Vec<u32>,
}

impl RecordingBackend {
    fn new(model: u16, clock: u32, dispatch: ModelDispatch) -> Self {
        let mut animation = AnimationClock::default();
        animation.advance(clock * 20_000);
        Self {
            submission: SceneModelSubmission {
                model_slot: model as i16,
                position: [123, -456, 100_000],
                basis: MenuBasis::INITIAL,
                animation,
                primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
                failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
                surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
                policy: ModelCallbackPolicy::Frontend,
            },
            expected_dispatch: Some(dispatch),
            primitives: vec![],
            publications: vec![],
            selections: vec![],
            depth_events: vec![],
            callbacks: vec![],
        }
    }

    fn vertices(
        &self,
        request: &ModelPrimitiveRequest<'_>,
        references: impl IntoIterator<Item = ModelSlotRef>,
    ) -> Vec<PreparedVertex> {
        references
            .into_iter()
            .map(|reference| {
                let slot = request.vertices.slot(reference.raw() as i16).unwrap();
                assert!(slot.projection.read_u32(0).is_some(), "uninitialized XY");
                let depth = slot.projection.read_u32(4).expect("uninitialized depth") as i32;
                if let Some(dispatch) = self.expected_dispatch {
                    assert!(depth >= 64);
                    assert_eq!(slot.projection.bytes[8], Some(0x12));
                    if dispatch == ModelDispatch::Far {
                        assert!(slot.projection.bytes[9].is_some());
                    }
                    // Near leaves fog untouched in the retained process pool;
                    // exact prior-byte preservation is covered by scratch_tests.
                } else {
                    assert!(slot.projection.bytes[8].is_some());
                    if depth < 64 {
                        assert_eq!(slot.projection.bytes[8], Some(0xc0));
                    }
                    if request.dispatch == ModelDispatch::Far && depth >= 64 {
                        assert!(slot.projection.bytes[9].is_some());
                    }
                }
                // Padding retains earlier allocation bytes. The process-view
                // writer/copy tests pin the exact untouched-byte contract.
                PreparedVertex {
                    slot: reference.raw(),
                    world: slot.world,
                    projection: slot.projection,
                }
            })
            .collect()
    }

    fn shades(
        &self,
        request: &ModelPrimitiveRequest<'_>,
        references: impl IntoIterator<Item = ModelSlotRef>,
    ) -> Vec<ModelShade> {
        references
            .into_iter()
            .map(|reference| {
                let shade = request.shades.shade(reference).unwrap();
                assert!(shade.rgb.is_some(), "uninitialized RGB");
                if reference.raw() < 2 {
                    assert_eq!(shade.high_byte, Some(0xa7));
                }
                // Nonambient high bytes retain arena history; process lighting
                // tests verify full ambient stores and RGB-only preservation.
                shade
            })
            .collect()
    }
}

impl ModelValueCallback for RecordingBackend {
    type Error = String;

    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        self.callbacks.push(selector);
        self.submission
            .callback_value(selector)
            .ok_or_else(|| "pure virtual callback".to_owned())
    }
}

impl ModelExecutionHost for RecordingBackend {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        if self.expected_dispatch.is_some() {
            assert_eq!(transform.view_position, context.position);
        }
        self.publications.push(*context);
        Ok(())
    }

    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        if let Some(expected) = self.expected_dispatch {
            assert_eq!(dispatch, expected);
        }
        self.selections.push(dispatch);
        Ok(())
    }

    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        assert_eq!(Some(&request.dispatch), self.selections.last());
        if self.expected_dispatch.is_some() {
            assert_eq!(request.context.position, request.transform.view_position);
        }
        let offset = request.instruction.span.start;
        assert_eq!(
            u16::from_le_bytes(request.source.bytes[offset..offset + 2].try_into().unwrap()),
            request.instruction.raw_command
        );
        assert_eq!(
            request.model.identity.global_index,
            request.source.registration.global_index
        );
        let (vertices, shades) = match &request.instruction.kind {
            ModelInstructionKind::TriangleFan(fan) => {
                assert_eq!(fan.flags.raw(), 10);
                assert_eq!(fan.vertices.len(), 3);
                (
                    self.vertices(&request, fan.vertices.iter().map(|vertex| vertex.vertex)),
                    self.shades(
                        &request,
                        fan.vertices.iter().map(|vertex| vertex.lighting_ref),
                    ),
                )
            }
            ModelInstructionKind::Quad(quad) => {
                let ModelQuadLighting::Shared(shade) = quad.lighting else {
                    return Err("unexpected per-vertex quad shading".into());
                };
                assert_eq!(quad.flags.raw(), 0);
                (
                    self.vertices(&request, quad.vertices),
                    self.shades(&request, [shade]),
                )
            }
            ModelInstructionKind::Opaque(command)
                if request.instruction.raw_command as u8 == 0x1b =>
            {
                // Record the native effect command and prepared source vertex.
                // This fixture does not manufacture effect geometry or execute
                // material lookup, clipping, queue submission or rasterization.
                assert_eq!(command.arguments.len(), 4);
                (
                    self.vertices(&request, [ModelSlotRef::from_raw(command.arguments[0])]),
                    vec![],
                )
            }
            _ => return Err("unsupported recording primitive".into()),
        };
        self.primitives.push(RecordedPrimitive {
            model: request.model.identity.global_index,
            opcode: request.instruction.raw_command as u8,
            source_offset: offset,
            position: request.context.position,
            state_words: request.state_words.to_vec(),
            vertices,
            shades,
            phase_register: values.register(5).unwrap(),
            effect_registers: [values.register(6).unwrap(), values.register(7).unwrap()],
        });
        Ok(())
    }

    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        assert!(!sorting);
        assert!(self.depth_events.is_empty());
        self.depth_events.push(Some(depth));
        Ok(())
    }

    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        assert_eq!(self.depth_events.len(), 1);
        self.depth_events.push(None);
        Ok(())
    }
}

fn environment(dispatch: ModelDispatch) -> ModelEnvironment {
    let fade = match dispatch {
        ModelDispatch::Near => [1_000_000, 2_000_000],
        ModelDispatch::Far => [0, 2_000_000],
    };
    ModelEnvironment {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [2560, 2560],
            viewport: [16_384, 16_384],
            center: [8192, 8192],
            fade,
        },
        coordinate_shift: 0,
        fog_factor: (0x100_0000_i64 / i64::from(fade[1] - fade[0])) as i32,
        lighting: ModelLighting {
            direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            directional_word: 0x00aa_d2ff,
            ambient: [10, 6, 5],
            ambient_word: 0xa705_060a,
        },
    }
}

fn setup() -> (ResidentRuntime, Arc<[u16; 4096]>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    assert_eq!(
        assets
            .scene_models()
            .iter()
            .map(|model| model.model_slot as u16)
            .collect::<Vec<_>>(),
        ROOTS
    );
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    (runtime, Arc::new(assets.sine_table().values))
}

fn model(resources: RenderResources<'_>, id: u16) -> &ModelEntry {
    let resource = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(id),
        })
        .unwrap();
    let RenderResourceData::Model(model) = resource.data else {
        panic!("model resource")
    };
    model
}

#[test]
fn all_thirteen_frontend_models_execute_actual_geometry_and_shades_in_both_tables() {
    let (runtime, table) = setup();
    let resources = RenderResources::new(&runtime);
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        for clock in [0, 1, 4, 31, 63, 64, 65535] {
            let scratch = ProcessModelScratch::isolated_unknown_fixture();
            let mut executor = ModelExecutor::new(
                resources,
                Arc::clone(&table),
                ModelExecutionLimits {
                    instructions: 100_000,
                    invocation_depth: 16,
                },
            );
            let mut total = ModelExecutionOutcome::default();
            let mut counts = BTreeMap::<u8, usize>::new();
            for id in ROOTS.into_iter().chain(CHILDREN) {
                let mut host = RecordingBackend::new(id, clock, dispatch);
                let outcome = executor
                    .draw(
                        ModelDrawRequest {
                            scratch: &scratch,
                            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                            model_argument: i32::from(id),
                            context: ModelContext::new(
                                host.submission.position,
                                host.submission.basis,
                            ),
                            environment: environment(dispatch),
                            surface: crate::model_vertices::ModelSurfaceBinding::Default,
                        },
                        &mut host,
                    )
                    .unwrap_or_else(|error| panic!("model{id} {dispatch:?} clock{clock}: {error}"));
                assert_eq!(outcome.culled, 0);
                assert_eq!(outcome.primitive_calls as usize, host.primitives.len());
                let child = match id {
                    745 => Some((746, 20)),
                    747 => Some((753, 532)),
                    749 | 755 => Some((751, 532)),
                    756 => Some((752, 532)),
                    _ => None,
                };
                assert_eq!(outcome.invocations, if child.is_some() { 2 } else { 1 });
                assert_eq!(host.publications.len(), if child.is_some() { 4 } else { 1 });
                assert_eq!(host.selections.len(), if child.is_some() { 3 } else { 1 });
                let first = host.publications.first().unwrap();
                let last = host.publications.last().unwrap();
                assert_eq!((first.position, first.basis), (last.position, last.basis));
                if let Some((child, slot)) = child {
                    let raw = model(resources, id).vertices[slot as usize / 2];
                    let origin = [
                        123 + i32::from(raw.x),
                        -456 + i32::from(raw.y),
                        100_000 + i32::from(raw.z),
                    ];
                    let child_records: Vec<_> = host
                        .primitives
                        .iter()
                        .filter(|record| record.model == child)
                        .collect();
                    assert_eq!(child_records.len(), 4);
                    for record in child_records {
                        assert_eq!(record.position, origin);
                        assert_eq!(record.state_words, [slot]);
                    }
                }
                if id == 755 {
                    let raw = model(resources, id).vertices[266];
                    assert_eq!(
                        host.depth_events,
                        [Some(100_000 + i32::from(raw.z) - 2400), None]
                    );
                    assert_eq!(outcome.depth_groups, 1);
                } else {
                    assert!(host.depth_events.is_empty());
                }
                for record in &host.primitives {
                    *counts.entry(record.opcode).or_default() += 1;
                    assert!(!record.vertices.is_empty());
                    if record.opcode != 0x1b {
                        assert!(!record.shades.is_empty());
                    }
                    if record.model == 746 {
                        assert_eq!(
                            record.phase_register,
                            Some((clock as u16).wrapping_shl(10) / 4096)
                        );
                    }
                    if record.model == 757 {
                        assert_eq!(
                            record.effect_registers,
                            [
                                Some((clock as u16).wrapping_shl(5)),
                                Some((clock as u16).wrapping_shl(6))
                            ]
                        );
                    }
                }
                total.invocations += outcome.invocations;
                total.primitive_calls += outcome.primitive_calls;
                total.depth_groups += outcome.depth_groups;
            }
            assert_eq!(
                counts,
                BTreeMap::from([(0x01, 3168), (0xe2, 40), (0x1b, 18)])
            );
            assert_eq!(
                (total.invocations, total.primitive_calls, total.depth_groups),
                (18, 3226, 1)
            );
            assert!(!executor.stopped());
        }
    }
}

#[test]
fn animated_child_branch_selects_different_face_groups_with_concrete_interpolated_xy() {
    let (runtime, table) = setup();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = ModelExecutor::new(
        RenderResources::new(&runtime),
        table,
        ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    );
    let mut snapshots = vec![];
    for clock in [0, 1, 4, 31, 63, 64] {
        let mut host = RecordingBackend::new(746, clock, ModelDispatch::Near);
        executor
            .draw(
                ModelDrawRequest {
                    scratch: &scratch,
                    root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                    model_argument: 746,
                    context: ModelContext::new(host.submission.position, host.submission.basis),
                    environment: environment(ModelDispatch::Near),
                    surface: crate::model_vertices::ModelSurfaceBinding::Default,
                },
                &mut host,
            )
            .unwrap();
        assert_eq!(host.primitives.len(), 4);
        snapshots.push((
            host.primitives
                .iter()
                .map(|record| record.source_offset)
                .collect::<Vec<_>>(),
            host.primitives
                .iter()
                .flat_map(|record| record.vertices.iter().cloned())
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(snapshots[0].0, snapshots[1].0); // Same phase, changing lerp input.
    assert_ne!(snapshots[0].1, snapshots[1].1);
    assert_ne!(snapshots[0].0, snapshots[2].0);
    assert_ne!(snapshots[2].0, snapshots[3].0);
    assert_ne!(snapshots[3].0, snapshots[4].0);
    assert_eq!(snapshots[0], snapshots[5]); // Native16-bit phase wraps at64 ticks.
    assert!(snapshots[0]
        .1
        .iter()
        .any(|vertex| vertex.slot >= 38 && vertex.world.is_none()));
}

#[derive(Default)]
struct SceneRecording {
    commands: Vec<SceneCommand>,
}

impl FrontendSceneHost for SceneRecording {
    type Error = &'static str;

    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        if matches!(command, SceneCommand::Footer(_)) {
            return Err("footer is outside this model composition fixture");
        }
        self.commands.push(command);
        Ok(())
    }

    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        Err("unexpected footer style lookup")
    }

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Err("unexpected footer dimensions")
    }

    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        Err("unexpected footer sprite lookup")
    }
}

#[test]
fn authored_scene_submissions_keep_actual_positions_rotations_and_camera_admission() {
    let (runtime, table) = setup();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let camera = FrontendCamera::new(
        &assets.camera_presets()[0],
        assets.position_scale(),
        FrontendEntry::Returning,
    );
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let mut animation = AnimationClock::default();
    animation.advance(17 * 20_000);
    // Explicit representative renderer state. Camera pose, model positions,
    // rotation controls, callbacks and scale come from the actual executable.
    // No model or child receives the artificial forward offset of the broad
    // admission test above; culling is part of this composition's result.
    let frame = SceneFrame {
        projection: SceneProjection {
            focal: [160, 120],
            viewport: [640, 480],
            fade: [30_000, 120_000],
        },
        inherited_view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        camera_eye: camera.eye().position,
        camera_look_at: camera.look_at().position,
        animation,
        age_millis: 2000,
        exit_millis: 0,
    };
    let mut recording = SceneRecording::default();
    assert_eq!(
        scene
            .draw_scene(frame, &mut recording)
            .unwrap()
            .submitted_models,
        9
    );
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = ModelExecutor::new(
        RenderResources::new(&runtime),
        table,
        ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    );
    let (mut shift, mut setup, mut view, mut directional, mut ambient) =
        (None, None, None, None, None);
    let mut outcomes = vec![];
    let mut preparations = vec![];
    let mut nonidentity_bases = 0;
    for command in recording.commands {
        match command {
            SceneCommand::CoordinateShift(value) => shift = Some(value),
            SceneCommand::DirectionalLight { direction, rgb } => {
                directional = Some((direction, rgb))
            }
            SceneCommand::AmbientLight(rgb) => ambient = Some(rgb),
            SceneCommand::Setup(value) => setup = Some(value),
            SceneCommand::PublishView(value) => view = Some(value),
            SceneCommand::Model(submission) => {
                let model_index = outcomes.len();
                assert_eq!(submission.model_slot as u16, ROOTS[model_index]);
                assert_eq!(
                    submission.position,
                    assets.scene_models()[model_index]
                        .position
                        .map(|value| i32::from(value) * assets.position_scale())
                );
                nonidentity_bases += usize::from(submission.basis != MenuBasis::INITIAL);
                let setup = setup.expect("scene setup before model");
                let view = view.expect("camera publication before model");
                assert_eq!(
                    view.eye,
                    if model_index == 0 {
                        [0, 0, -70_000]
                    } else {
                        frame.camera_eye
                    }
                );
                let (direction, [r, g, b]) = directional.expect("directional light before model");
                let [ar, ag, ab] = ambient.expect("ambient light before model");
                let environment = ModelEnvironment {
                    view,
                    projection: ModelProjection {
                        focal: setup.focal,
                        viewport: setup.viewport,
                        center: setup.center,
                        fade: setup.fade,
                    },
                    coordinate_shift: shift.expect("coordinate shift before model"),
                    fog_factor: setup.fade_factor,
                    lighting: ModelLighting {
                        direction,
                        directional_word: u32::from_le_bytes([r, g, b, 0]),
                        ambient: [ar, ag, ab],
                        // Explicit inherited high byte; ambient RGB does not
                        // establish a new value for that native byte.
                        ambient_word: u32::from_le_bytes([ar as u8, ag as u8, ab as u8, 0xa7]),
                    },
                };
                let mut host =
                    RecordingBackend::new(submission.model_slot as u16, 17, ModelDispatch::Near);
                host.submission = submission;
                host.expected_dispatch = None;
                let outcome = executor
                    .draw(
                        ModelDrawRequest {
                            scratch: &scratch,
                            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                            model_argument: i32::from(submission.model_slot),
                            context: ModelContext::new(submission.position, submission.basis),
                            environment,
                            surface: crate::model_vertices::ModelSurfaceBinding::Default,
                        },
                        &mut host,
                    )
                    .unwrap_or_else(|error| {
                        panic!("scene model{}: {error}", submission.model_slot)
                    });
                assert_eq!(host.publications[0].position, submission.position);
                assert_eq!(host.publications[0].basis, submission.basis);
                outcomes.push((submission.model_slot, outcome));
                preparations.push(host);
            }
            // These are retained recording operations. They are deliberately
            // not represented as completed renderer work by this fixture.
            SceneCommand::BeginBatch
            | SceneCommand::FlushBatch
            | SceneCommand::StarView(_)
            | SceneCommand::Star(_)
            | SceneCommand::ResetAfterFirstModel => {}
            SceneCommand::Footer(_) => panic!("unexpected footer"),
            SceneCommand::FarColor(_) => panic!("frontend does not submit context far color"),
            SceneCommand::PublishFade(_) => panic!("frontend does not replace fade independently"),
        }
    }
    assert_eq!(outcomes.len(), 9);
    assert!(nonidentity_bases > 0);
    assert!(!executor.stopped());
    let invocations: usize = outcomes.iter().map(|(_, result)| result.invocations).sum();
    let culled: usize = outcomes.iter().map(|(_, result)| result.culled).sum();
    let primitives: u64 = outcomes
        .iter()
        .map(|(_, result)| result.primitive_calls)
        .sum();
    let near = preparations
        .iter()
        .flat_map(|host| &host.selections)
        .filter(|&&value| value == ModelDispatch::Near)
        .count();
    let far = preparations
        .iter()
        .flat_map(|host| &host.selections)
        .filter(|&&value| value == ModelDispatch::Far)
        .count();
    assert_eq!((invocations, culled, primitives), (14, 2, 2680));
    // Selections include restoring the parent after each admitted child.
    assert_eq!((near, far), (2, 15));
    assert_eq!(
        outcomes
            .iter()
            .map(|(id, result)| (*id, result.culled, result.primitive_calls))
            .collect::<Vec<_>>(),
        [
            (744, 0, 16),
            (749, 0, 532),
            (754, 1, 0),
            (756, 0, 532),
            (745, 0, 8),
            (748, 0, 528),
            (747, 0, 532),
            (755, 0, 532),
            (757, 1, 0)
        ]
    );
    assert_eq!(
        outcomes
            .iter()
            .map(|(_, result)| result.depth_groups)
            .sum::<usize>(),
        1
    );
}
