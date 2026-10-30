//! Original model history through the actual software backend.
//! Original 686/687 is stopped by an explicit test-only host error AFTER
//! the genuine first far edge has returned from ModelCommandBackend. New
//! renderer fixtures share the SAME scratch owner; no queue recovery is inferred.

use std::{convert::Infallible, path::Path};

use inf_formats::{BriefingModel, BriefingPayload, FrontendExecutable, ModelEntry};

use super::*;
use crate::{
    briefing_scene::{draw_briefing_model_preview, BriefingSceneFrame},
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    frontend_scene::{FrontendSceneHost, SceneCommand},
    menu_basis::MenuBasisMath,
    model_context::{ModelDispatch, ModelTransform},
    model_execution::{ModelExecutionHost, ModelPrimitiveRequest, ModelRootMode},
    model_scratch::{
        ModelScratchDrawState, ProcessModelScratch, RegisteredMarkUnwindApproximation,
    },
    model_vertices::{ModelCustomVertexError, ModelCustomVertexRequest, ModelVertices},
    scene_resources::ResidentSceneHost,
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

#[derive(Default)]
struct PreviewCommands(Vec<SceneCommand>);

impl FrontendSceneHost for PreviewCommands {
    type Error = Infallible;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.0.push(command);
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        unreachable!("this genuine preview producer never requests the footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        unreachable!("the preview frame already carries current dimensions")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        unreachable!("the preview producer submits H8 models")
    }
}

#[derive(Debug)]
enum ObservationError<E> {
    RealBackend(E),
    StopAfterRealFirstEdge,
}

fn custom_error<E>(
    error: ModelCustomVertexError<E>,
) -> ModelCustomVertexError<ObservationError<E>> {
    match error {
        ModelCustomVertexError::Fault(fault) => ModelCustomVertexError::Fault(fault),
        ModelCustomVertexError::Host(error) => {
            ModelCustomVertexError::Host(ObservationError::RealBackend(error))
        }
    }
}

#[derive(Clone, Debug)]
struct CopyWitness {
    parent: [[Option<u8>; 24]; 2],
    child: [[Option<u8>; 24]; 2],
    dispatch: ModelDispatch,
    backend_returned_normally: bool,
}

/// Only observes. Every operation, including the inspected edge, reaches the
/// unchanged real backend; this wrapper never returns primitive success itself.
struct ObserveCopy<H> {
    inner: H,
    scratch: ProcessModelScratch,
    parent_bytes: usize,
    projections: Vec<ModelDispatch>,
    witnesses: Vec<CopyWitness>,
}

impl<H: ModelValueCallback> ModelValueCallback for ObserveCopy<H> {
    type Error = ObservationError<H::Error>;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.inner
            .packed_value(values, selector)
            .map_err(ObservationError::RealBackend)
    }
}

impl<H: ModelExecutionHost> ModelExecutionHost for ObserveCopy<H> {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.inner
            .publish_transform(context, transform)
            .map_err(ObservationError::RealBackend)
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.inner
            .select_projection(dispatch)
            .map_err(ObservationError::RealBackend)?;
        self.projections.push(dispatch);
        Ok(())
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        let witness = if request.model.identity.global_index == 687
            && request.instruction.span.start == 516128
        {
            let parent = [6, 7].map(|slot| self.scratch.read_pool::<24>(slot * 24).unwrap());
            let child = [18, 19].map(|slot| {
                self.scratch
                    .read_pool::<24>(self.parent_bytes + slot * 24)
                    .unwrap()
            });
            // These are the same actual rows presented to the primitive.
            for (slot, raw) in [18_i16, 19].into_iter().zip(&child) {
                assert_eq!(
                    request.vertices.slot(slot).unwrap().projection.bytes,
                    raw[12..24]
                );
            }
            Some(CopyWitness {
                parent,
                child,
                dispatch: request.dispatch,
                backend_returned_normally: false,
            })
        } else {
            None
        };
        let result = self.inner.primitive(request, values);
        if let Some(mut witness) = witness {
            witness.backend_returned_normally = result.is_ok();
            self.witnesses.push(witness);
            result.map_err(ObservationError::RealBackend)?;
            return Err(ObservationError::StopAfterRealFirstEdge);
        }
        result.map_err(ObservationError::RealBackend)
    }
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        self.inner
            .begin_depth_group(depth, sorting)
            .map_err(ObservationError::RealBackend)
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.inner
            .end_depth_group()
            .map_err(ObservationError::RealBackend)
    }
    fn sine_quarter_table(&self) -> Option<&[u16; 4096]> {
        self.inner.sine_quarter_table()
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        self.inner.process_rng()
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        self.inner.custom_vertex_policy()
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.inner
            .required_custom_vertex(request, vertices, output)
            .map_err(custom_error)
    }
    fn furniture_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.inner
            .furniture_custom_vertex(request, vertices, output)
            .map_err(custom_error)
    }
}

fn native_scratch(assets: &FrontendAssets) -> ProcessModelScratch {
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let proof = executable
        .loader_zero_data(0x0056_4728, 0x0065_e72c - 0x0056_4728)
        .unwrap();
    ProcessModelScratch::native_loader_zero(&proof).unwrap()
}

fn through_original_first_edge(
    scene: &mut SoftwareScene<'_, '_>,
    authored: BriefingModel,
    model_age: i32,
    frame: BriefingSceneFrame,
    math: &MenuBasisMath,
    root: &ModelEntry,
    child: &ModelEntry,
) -> CopyWitness {
    assert_eq!(scene.scratch.cursor().unwrap(), 0);
    let mut produced = PreviewCommands::default();
    draw_briefing_model_preview(
        authored,
        model_age,
        root.header.visibility_extent,
        false,
        frame,
        math,
        &mut produced,
    )
    .unwrap();
    let mut model = None;
    for command in produced.0 {
        if let SceneCommand::Model(submission) = command {
            assert!(model.replace(submission).is_none());
        } else {
            ResidentSceneHost {
                resources: scene.resources,
                backend: &mut *scene,
            }
            .submit(command)
            .unwrap();
        }
    }
    let submission = model.unwrap();
    assert_eq!(submission.model_slot, 686);
    let child_call = root
        .program
        .instructions
        .iter()
        .position(|instruction| instruction.span.start == 515824)
        .unwrap();
    assert_eq!(
        root.program.instructions[child_call].raw_command as u8,
        0x0e
    );
    let _edge = child
        .program
        .instructions
        .iter()
        .position(|instruction| instruction.span.start == 516128)
        .unwrap();
    assert_eq!(child.program.instructions[0].raw_command, 0x0a2a);
    assert!(matches!(&child.program.instructions[0].kind,
        inf_formats::ModelInstructionKind::UnresolvedDynamic(command)
            if command.arguments == [18, 1]));
    let scratch = scene.scratch.clone();
    let parent_bytes = usize::from(root.header.vertex_slot_count) * 24
        + usize::from(root.header.shade_slot_count) * 4;
    let environment = scene.environment;
    let surface = scene.model_surface_binding(submission.surface_table, scene.terrain.as_ref());
    let sorting = scene.material_sorting();
    let mut rng = scene.process_rng;
    let services = ModelServices {
        callbacks: ModelObjectCallbacks::Ordinary(submission),
        sorting,
        textures: &mut scene.texture_identities,
        process_rng: rng.as_mut(),
        feed: None,
        motion: None,
        tint: None,
        sky: None,
        metal: None,
    };
    let backend = ModelCommandBackend::new(
        scene.resources,
        scene.tables,
        scene.primitive_state,
        &mut scene.queue,
        services,
    );
    let mut observed = ObserveCopy {
        inner: backend,
        scratch: scratch.clone(),
        parent_bytes,
        projections: Vec::new(),
        witnesses: Vec::new(),
    };
    // The full original program runs through its actual branches/groups.
    // Only the observing host injects a stop, after the real edge returns.
    let mut executor = ModelExecutor::new(
        scene.resources,
        Arc::clone(&scene.tables.sine_table),
        scene.execution_limits,
    );
    let result = executor.draw(
        ModelDrawRequest {
            scratch: &scratch,
            root_mode: ModelRootMode::CommonVisual,
            model_argument: 686,
            context: ModelContext::new(submission.position, submission.basis),
            environment,
            surface,
        },
        &mut observed,
    );
    assert!(
        matches!(
            result,
            Err(ModelExecutionError::Host(
                ObservationError::StopAfterRealFirstEdge
            ))
        ),
        "{result:?}"
    );
    assert_eq!(
        observed.projections.first(),
        Some(&if model_age == 0 {
            ModelDispatch::Far
        } else {
            ModelDispatch::Near
        })
    );
    assert_eq!(observed.projections.last(), Some(&ModelDispatch::Far));
    assert_eq!(observed.witnesses.len(), 1);
    let witness = observed.witnesses.remove(0);
    assert!(
        witness.backend_returned_normally,
        "the real first edge must return before the test-only stop"
    );
    assert_eq!(witness.dispatch, ModelDispatch::Far);
    assert_eq!(
        witness.child, witness.parent,
        "2A copies all 24 actual bytes"
    );
    drop(observed);
    scene.process_rng = rng;
    // Native queue/depth-group state is retained independently. Do not flush,
    // close an open group, reset, or claim native queue recovery here.
    let queue_prefix = scene.queue.state();
    let ModelScratchDrawState::Stopped(scope) = scratch.state().unwrap() else {
        panic!("the explicit observing-host stop must leave actual registered marks")
    };
    assert_eq!(
        scratch.cursor().unwrap(),
        parent_bytes
            + usize::from(child.header.vertex_slot_count) * 24
            + usize::from(child.header.shade_slot_count) * 4
    );
    scratch
        .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)
        .unwrap();
    assert_eq!(scratch.state().unwrap(), ModelScratchDrawState::Ready);
    assert_eq!(scratch.cursor().unwrap(), 0);
    assert_eq!(scene.queue.state(), queue_prefix);
    witness
}

#[test]
fn original_686_687_reuses_far_fog_in_a_later_near_parent_full_copy() {
    let root_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root_path).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let mut resident = ResidentRuntime::new(Installation::open(&root_path).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
    let program = inf_formats::parse_briefing_program(
        placements.world_source().overlay.bytes(),
        &placements.assets().worlds,
        placements.world(),
    )
    .unwrap();
    let authored = program
        .records
        .iter()
        .find_map(|record| match record.payload {
            BriefingPayload::Model(model) if model.model_slot == 686 => Some(model),
            _ => None,
        })
        .unwrap();
    let resolve = |argument| {
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument,
            })
            .unwrap();
        let RenderResourceData::Model(model) = resource.data else {
            unreachable!()
        };
        model
    };
    let (root, child) = (resolve(686), resolve(687));
    assert_eq!(
        root.span,
        inf_formats::ByteSpan {
            start: 515728,
            end: 515848
        }
    );
    assert_eq!(
        child.span,
        inf_formats::ByteSpan {
            start: 515848,
            end: 516164
        }
    );
    assert_eq!(
        (root.header.vertex_slot_count, root.header.shade_slot_count),
        (8, 2)
    );
    let mut animation = AnimationClock::default();
    animation.advance(40_000);
    let frame = BriefingSceneFrame {
        projection: startup.projection,
        age_millis: 2000,
        start_remaining_millis: 0,
        animation,
        selector_one: 1,
    };
    let mut observations = Vec::new();
    for ages in [vec![2000], vec![0, 2000]] {
        let scratch = native_scratch(&assets); // once per distinct process fixture
        let mut history = Vec::new();
        for age in ages {
            // SuppliedRendererRecreationFixture: distinct renderer/queue per
            // pose, SAME process scratch. This isolates byte history without
            // assuming completion/recovery of the previous queue/group prefix.
            let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    scratch.clone(),
                    (startup.scene).environment,
                ),
                resources,
                tables: &tables,
                frame: &mut pixels,
                config: startup.scene,
            })
            .unwrap();
            scene.set_terrain(Some(placements.terrain().clone()));
            scene.environment.coordinate_shift = 8;
            scene.graphics.publish_shift(8).unwrap(); // explicit supplied fixture publication
            scene.begin_frame().unwrap();
            history.push(through_original_first_edge(
                &mut scene, authored, age, frame, &math, root, child,
            ));
        }
        observations.push(history);
    }
    let fresh_near = &observations[0][0];
    for slot in &fresh_near.parent {
        assert_eq!(
            &slot[21..24],
            &[Some(0); 3],
            "native Near retains once-proven loader-zero fog and padding"
        );
    }
    let warmed_far = &observations[1][0];
    let reused_near = &observations[1][1];
    assert!(
        warmed_far.parent.iter().any(|slot| slot[21] != Some(0)),
        "the real Far writer must distinguish retained history from fresh zero"
    );
    for (far, near) in warmed_far.parent.iter().zip(&reused_near.parent) {
        assert_eq!(
            &near[21..24],
            &far[21..24],
            "later Near leaves original fog/padding bytes intact across retirement"
        );
    }
    assert_ne!(
        reused_near.child, fresh_near.child,
        "full raw2A must preserve process history rather than per-root zeroing"
    );
}
