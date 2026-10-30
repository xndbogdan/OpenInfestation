//! Actual selector30 capture through original H8 common/late service paths.
//!
//! Constructor30 with H12 NULL declares this callback domain. Prefix budgets
//! and the injected later projector fault are portable test boundaries; these
//! cases do not establish H13 loading, a complete manager/frame, or retail pixels.

use super::*;

fn capture30_service_custody(arena: &ObjectArena, source: ObjectId) -> String {
    let current = arena.object(source).unwrap();
    let exact = format!("{current:?}");
    let mut normalized = current.clone();
    let crate::object_runtime::ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("actual actor");
    };
    let crate::object_runtime::ActorExtension::Selector30(state) = &mut actor.extension else {
        panic!("actual constructor30");
    };
    state.position_16c = None;
    let whole = format!("{arena:?}");
    assert_eq!(whole.matches(&exact).count(), 1);
    whole.replacen(&exact, &format!("{normalized:?}"), 1)
}

#[test]
fn original62_child63_first_full8_capture_uses_same_source_in_both_roots() {
    use crate::model_execution::{ModelDrawRequest, ModelExecutionError, ModelRootMode};
    for root_mode in [ModelRootMode::CommonVisual, ModelRootMode::ManagerLate] {
        for budget in [4, 5] {
            let (assets, tables, resident) = fixture(); //real Boot->48 parent1
            let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
            let mut arena = arena();
            let source = arena.allocate_selector30_actor(allocation()).unwrap();
            assert_eq!(arena.selector30_actor(source).unwrap().position_16c, None);
            assert_eq!(arena.object(source).unwrap().base.position_words, None);
            let arena_before = format!("{arena:?}");
            let clock = AnimationClock::default();
            let context = ObjectDrawContext {
                view: SceneView {
                    eye: [65536, -65536, 65536],
                    basis: MenuBasis::INITIAL,
                },
                ..TEST_CONTEXT
            };
            let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
            let mut config = config(&resident);
            config.environment.view = context.view;
            // Two parent records, two child no-op0022 records, then222D.
            // The declared budget stops before/after that real custom record;
            // no claim that native execution faults at the same point.
            config.execution_limits.instructions = budget;
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                    (config).environment,
                ),
                resources: RenderResources::new(&resident),
                tables: &tables,
                frame: &mut pixels,
                config,
            })
            .unwrap();
            for (slot, expected_records) in [(62, 2usize), (63, 3usize)] {
                let RenderResourceData::Model(model) = scene
                    .resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Model,
                        argument: slot,
                    })
                    .unwrap()
                    .data
                else {
                    panic!("original parent1 H8/{slot}");
                };
                assert!(model.program.instructions.len() >= expected_records);
                if slot == 62 {
                    assert_eq!(model.program.instructions[0].raw_command, 0x0003);
                    assert_eq!(
                        model.program.instructions[1].span,
                        ByteSpan::new(26058, 26068)
                    );
                    let ModelInstructionKind::ModelInstance(child) =
                        &model.program.instructions[1].kind
                    else {
                        panic!("original62 child packet");
                    };
                    assert_eq!(child.model.raw(), 63);
                } else {
                    assert_eq!(model.program.instructions[0].raw_command, 0x0022);
                    assert_eq!(model.program.instructions[1].raw_command, 0x0022);
                    assert_eq!(model.program.instructions[2].raw_command, 0x222d);
                    assert_eq!(
                        model.program.instructions[2].span,
                        ByteSpan::new(26670, 26680)
                    );
                    let ModelInstructionKind::UnresolvedDynamic(raw) =
                        &model.program.instructions[2].kind
                    else {
                        panic!("original63 custom packet");
                    };
                    assert_eq!(raw.arguments, [118, 8, 44, 0]);
                }
            }
            let queue_before = format!("{:?}", scene.queue);
            let mut rng = 0x1020_3040;
            let (values, observed) = {
                let services = RecordingServices {
                    actual: ModelServices {
                        callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                            arena: &mut arena,
                            source,
                            context: &context,
                            animation: &clock,
                            executable,
                            effects: None,
                            guided_mounts: false,
                        }),
                        sorting: false,
                        textures: &mut scene.texture_identities,
                        process_rng: Some(&mut rng),
                        feed: None,
                        motion: None,
                        tint: None,
                        sky: None,
                        metal: None,
                    },
                    values: Vec::new(),
                    customs: Vec::new(),
                };
                let mut commands = RecordingCommands {
                    actual: ModelCommandBackend::new(
                        scene.resources,
                        &tables,
                        scene.primitive_state,
                        &mut scene.queue,
                        services,
                    ),
                    models: Vec::new(),
                };
                let mut executor = ModelExecutor::new(
                    scene.resources,
                    Arc::clone(&tables.sine_table),
                    scene.execution_limits,
                );
                let result = executor.draw(
                    ModelDrawRequest {
                        scratch: &scene.scratch,
                        root_mode,
                        model_argument: 62,
                        context: ModelContext::new([65536, -65536, 75536], MenuBasis::INITIAL),
                        environment: scene.environment,
                        surface: crate::model_vertices::ModelSurfaceBinding::Default,
                    },
                    &mut commands,
                );
                assert!(
                    matches!(result, Err(ModelExecutionError::Limit)),
                    "root={root_mode:?}, budget={budget}: {result:?}"
                );
                assert!(
                    commands.models.is_empty(),
                    "prefix stops before first DA primitive"
                );
                (
                    std::mem::take(&mut commands.actual.services.values),
                    std::mem::take(&mut commands.actual.services.customs),
                )
            };
            assert!(
                values.is_empty(),
                "original4000 basis angle is a literal, not a callback/register"
            );
            if budget == 4 {
                assert!(observed.is_empty());
                assert_eq!(format!("{arena:?}"), arena_before);
            } else {
                assert_eq!(observed.len(), 1);
                let call = &observed[0];
                assert_eq!(call.source, source);
                assert_eq!(call.selector, 8);
                assert_eq!(call.extras, [44, 0]);
                assert_eq!(call.output, caller_rebase(call.words, context.view.eye));
                assert_eq!(
                    arena.selector30_actor(source).unwrap().position_16c,
                    Some(call.words)
                );
                assert_eq!(capture30_service_custody(&arena, source), arena_before);
            }
            assert_eq!(format!("{:?}", scene.queue), queue_before);
            assert_eq!(
                rng, 0x1020_3040,
                "original0022 has mode0 and consumes no RNG"
            );
        }
    }
}

#[test]
fn original63_full8_capture_and_eye_rebase_survive_later_projector_error() {
    let (assets, tables, resident) = fixture();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let source = arena.allocate_selector30_actor(allocation()).unwrap();
    let arena_before = format!("{arena:?}");
    let clock = AnimationClock::default();
    let context = ObjectDrawContext {
        view: SceneView {
            eye: [0x1_8001, -0x1_8000, 0x2_ffff],
            basis: MenuBasis::INITIAL,
        },
        ..TEST_CONTEXT
    };
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config(&resident)).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &tables,
        frame: &mut pixels,
        config: config(&resident),
    })
    .unwrap();
    let RenderResourceData::Model(model) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 63,
        })
        .unwrap()
        .data
    else {
        panic!("original parent1 H8/63");
    };
    let instruction = &model.program.instructions[2];
    assert_eq!(instruction.raw_command, 0x222d);
    assert_eq!(instruction.span, ByteSpan::new(26670, 26680));
    let ModelInstructionKind::UnresolvedDynamic(raw) = &instruction.kind else {
        panic!("original63 custom");
    };
    assert_eq!(raw.arguments, [118, 8, 44, 0]);
    let model_context = ModelContext::new([0x3_fff0, -0x2_0000, 0x5_8001], MenuBasis::INITIAL);
    let mut vertices = ModelVertices::new_invocation(model.header.vertex_slot_count);
    let [a, b, c, _] = model.header.vertex_ranges;
    // A bounded direct original packet case starts at the genuine world-row
    // initializer. Whole62 prefix reach is owned by the separate budget test.
    vertices
        .initialize_world_ranges(
            model,
            &model_context,
            [i32::from(a), i32::from(b), i32::from(c)],
        )
        .unwrap();
    let captured = vertices.row_words_439e10(44).unwrap();
    let row_before = *vertices.slot(44).unwrap();
    let destination_before = *vertices.slot(118).unwrap();
    let expected = caller_rebase(captured, context.view.eye);
    assert_ne!(expected, caller_rebase(captured, model_context.position));
    let queue_before = format!("{:?}", scene.queue);
    let mut observed_output = None;
    let mut projector = FailAfterRealCallback { actual_input: None };
    {
        let mut services = ModelServices {
            callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                arena: &mut arena,
                source,
                context: &context,
                animation: &clock,
                executable,
                effects: None,
                guided_mounts: false,
            }),
            sorting: false,
            textures: &mut scene.texture_identities,
            process_rng: None,
            feed: None,
            motion: None,
            tint: None,
            sky: None,
            metal: None,
        };
        let result = vertices.execute_custom_2d(
            instruction,
            ModelCustomVertexPolicy::RequiredObject,
            &mut projector,
            &mut |request, vertices, output| {
                services.required_custom_vertex(request, vertices, output)?;
                observed_output = *output;
                Ok::<_, ModelCustomVertexError<SoftwareSceneServiceError>>(())
            },
        );
        assert!(matches!(
            result,
            Err(ModelVertexError::Projection(
                "declared later projector boundary"
            ))
        ));
    }
    assert_eq!(
        arena.selector30_actor(source).unwrap().position_16c,
        Some(captured)
    );
    assert_eq!(capture30_service_custody(&arena, source), arena_before);
    assert_eq!(observed_output, Some(expected));
    assert_eq!(
        projector.actual_input,
        Some(ModelVertexInput::World(expected))
    );
    assert!(vertices.stopped());
    assert_eq!(vertices.slot(44).unwrap(), &row_before);
    assert_eq!(
        vertices.slot(118).unwrap(),
        &destination_before,
        "common222D mode2 projects without a preceding WORLD store"
    );
    assert_eq!(format!("{:?}", scene.queue), queue_before);
}
