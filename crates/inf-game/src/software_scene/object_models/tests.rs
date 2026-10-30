//! Captured-object binding on original H8 programs and the real command queue.

mod capture30;
mod firing;
mod projection_custody;

use super::*;
use crate::menu::MenuBasis;
use crate::object_runtime::ObjectId;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    frontend_scene::{SceneView, StarQuad},
    model_commands::ModelCommandServices,
    model_context::{ModelDispatch, ModelTransform},
    model_execution::{ModelExecutionHost, ModelPrimitiveRequest},
    model_values::{ModelValueCallback, ModelValueError, ModelValues},
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, DerivedActorConstructionRequest, ObjectModelCallbackError,
    },
    scene_resources::{ResolvedSceneCommand, SceneResourceBackend},
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    ModelInstructionKind, TERRAIN_ENTRY_FIXED_BYTES,
};
use std::{cell::Cell, path::Path, sync::Arc};

const TEST_CONTEXT: crate::object_runtime::ObjectDrawContext =
    crate::object_runtime::ObjectDrawContext {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        near: 1_000_000,
        far: 2_000_000,
        far_color: 0,
    };

fn fixture() -> (FrontendAssets, PrimitiveTables, ResidentRuntime) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    // Leaf48 loads the real parent1 H8 programs used below.
    resident
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    (assets, tables, resident)
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> ActorConstructionRequest<'static> {
    static UNUSED_SINE: [u16; 4096] = [0; 4096];
    // The native shared constructor's actual H12 NULL arm reads neither
    // uninitialized XYZ nor the sine backing. This is a declared input domain.
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn vehicle(arena: &mut ObjectArena) -> ObjectId {
    // Supplied parsed flat H12 supports the genuine vehicle constructor;
    // neither original OVL bytes nor the final actor class are modified.
    let raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, raw.len());
    let bank = parse_terrain_bank(
        &raw,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut request = allocation();
    request.allocation_position = Some([768, 600, 768]);
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap()
}

fn config(resident: &ResidentRuntime) -> SoftwareSceneConfig {
    let mut config = frontend_startup(
        resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap()
    .scene;
    // Explicit executor view, independent of actor XYZ and the root's
    // placement/common-root approximation. All model bytes stay original.
    config.environment.view = SceneView {
        eye: [0; 3],
        basis: MenuBasis::INITIAL,
    };
    config.environment.coordinate_shift = 0;
    config.environment.projection.fade = [1_000_000, 2_000_000];
    config.execution_limits.instructions = 100_000;
    config
}

struct RecordingServices<'s, 'r> {
    actual: ModelServices<'s, 'r>,
    values: Vec<(u32, u16)>,
    customs: Vec<RecordedCustom>,
}

#[derive(Debug)]
struct RecordedCustom {
    source: ObjectId,
    selector: i32,
    extras: Vec<u16>,
    words: [u16; 3],
    output: [i32; 3],
    actor_word_74: u16,
}

struct ObservedClock<'a> {
    clock: &'a AnimationClock,
    reads: Cell<usize>,
}

impl AnimationClockSource for ObservedClock<'_> {
    fn animation_value(&self) -> u32 {
        self.reads.set(self.reads.get() + 1);
        self.clock.value()
    }
}

impl ModelValueCallback for RecordingServices<'_, '_> {
    type Error = SoftwareSceneServiceError;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        let value = self.actual.packed_value(values, selector)?;
        self.values.push((selector, value));
        Ok(value)
    }
}

impl<'r> ModelCommandServices<'r> for RecordingServices<'_, 'r> {
    fn prepare_draw(
        &mut self,
        queue: &mut DrawQueue<PrimitiveCommand<'r>>,
    ) -> Result<bool, Self::Error> {
        self.actual.prepare_draw(queue)
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.actual.texture_sort_identity(texture)
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        self.actual.process_rng()
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        self.actual.custom_vertex_policy()
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        let ModelObjectCallbacks::Object(loan) = &self.actual.callbacks else {
            panic!("recording actual captured-source callback");
        };
        let source = loan.source;
        self.actual
            .required_custom_vertex(request, vertices, output)?;
        let ModelObjectCallbacks::Object(loan) = &self.actual.callbacks else {
            unreachable!("same genuine captured-source callback loan");
        };
        let actor_word_74 = loan.arena.actor(source).unwrap().word_74;
        self.customs.push(RecordedCustom {
            source,
            selector: request.selector,
            extras: request.extras.to_vec(),
            words: vertices.row_words_439e10(request.extras[0]).unwrap(),
            output: output.expect("successful source callback publishes output"),
            actor_word_74,
        });
        Ok(())
    }
}

struct RecordingCommands<'s, 'q, 'r> {
    actual: ModelCommandBackend<'r, 'q, RecordingServices<'s, 'r>>,
    models: Vec<u16>,
}

impl ModelValueCallback for RecordingCommands<'_, '_, '_> {
    type Error = ModelCommandError<SoftwareSceneServiceError>;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.actual.packed_value(values, selector)
    }
}

impl ModelExecutionHost for RecordingCommands<'_, '_, '_> {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.actual.publish_transform(context, transform)
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.actual.select_projection(dispatch)
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.models.push(request.model.identity.global_index);
        self.actual.primitive(request, values)
    }
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        self.actual.begin_depth_group(depth, sorting)
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.actual.end_depth_group()
    }
    fn sine_quarter_table(&self) -> Option<&[u16; 4096]> {
        self.actual.sine_quarter_table()
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        self.actual.process_rng()
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        self.actual.custom_vertex_policy()
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.actual
            .required_custom_vertex(request, vertices, output)
    }
}

#[test]
fn original_h8_children_keep_captured_source_distinct_from_component_and_resource() {
    let (assets, tables, resident) = fixture();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let original = arena.allocate_actor(allocation()).unwrap();
    let component = arena.allocate_actor(allocation()).unwrap();
    let captured = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            &mut 7,
        )
        .unwrap();
    assert_ne!(original, component);
    assert_ne!(component, captured);
    assert_ne!(original, captured);
    let arena_before = format!("{arena:?}");
    let mut clock = AnimationClock::default();
    clock.advance(740_000);
    for source in [original, component] {
        assert_eq!(
            ObjectModelCallbacks {
                arena: &arena,
                source,
                animation: &clock,
                executable,
            }
            .packed_value(1)
            .unwrap(),
            0
        );
    }
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
    {
        let RenderResourceData::Model(child) = scene
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 6,
            })
            .unwrap()
            .data
        else {
            panic!("authored child6")
        };
        // Original 1.ovl: 0016 jumps from its first argument at3740 by21
        // WORDs to3782 when captured selector1 != FFFF. The following0116
        // then falls through to child7; the clock row and child8 are skipped.
        for (index, start, opcode, arguments) in [
            (1, 3738, 0x0016, [21, 0x0081, 0x0040]),
            (6, 3782, 0x0116, [8, 0x0081, 0x0040]),
        ] {
            let instruction = &child.program.instructions[index];
            assert_eq!(instruction.span, ByteSpan::new(start, start + 8));
            assert_eq!(instruction.raw_command, opcode);
            let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
                panic!("authored branch")
            };
            assert_eq!(command.arguments, arguments);
        }
    }
    let packet = ObjectModelPacket {
        source: captured,
        model_slot: 5,
        position: [0, 0, 100_000],
        basis: MenuBasis::INITIAL,
    };
    let mut rng = 0x1020_3040;
    {
        let services = RecordingServices {
            actual: ModelServices {
                callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                    arena: &mut arena,
                    context: &TEST_CONTEXT,
                    source: packet.source,
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
        let outcome = executor
            .draw(
                ModelDrawRequest {
                    scratch: &scene.scratch,
                    root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                    model_argument: i32::from(packet.model_slot),
                    context: ModelContext::new(packet.position, packet.basis),
                    environment: scene.environment,
                    surface: crate::model_vertices::ModelSurfaceBinding::Default,
                },
                &mut commands,
            )
            .unwrap();
        assert_eq!(outcome.invocations, 3, "real 0E path5 ->6 ->7");
        assert_eq!(
            commands
                .models
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([5, 6, 7])
        );
        let values = &commands.actual.services.values;
        assert_eq!(
            values.iter().filter(|(selector, _)| *selector == 1).count(),
            4,
            "parent5 and child6 both read captured source +14"
        );
        assert!(values
            .iter()
            .filter(|(selector, _)| *selector == 1)
            .all(|(_, value)| *value == 1));
        assert!(
            values.iter().all(|(selector, _)| *selector != 0),
            "authored selector1=1 path skips the child6/8 clock rows"
        );
    }
    assert!(scene.queue.state().command_count > 0);
    assert_eq!(rng, 0x1020_3040, "these authored models consume no RNG");
    assert_eq!(format!("{arena:?}"), arena_before);
}

#[test]
fn reached_vehicle_field_error_preserves_queue_rng_and_allows_independent_feature() {
    let (assets, tables, resident) = fixture();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let source = vehicle(&mut arena);
    arena.forget_vehicle_track_phase(source).unwrap();
    let arena_before = format!("{arena:?}");
    let mut clock = AnimationClock::default();
    clock.advance(820_000);
    let observed_clock = ObservedClock {
        clock: &clock,
        reads: Cell::new(0),
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
    scene
        .star(
            StarQuad {
                corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                color_argument: 0,
            },
            0x00ff_0000,
        )
        .unwrap();
    // A retained caller RNG prefix, not an asserted draw-time native cadence.
    let mut rng = 0x1020_3040;
    crate::name_entry::name_entry_rng_next(&mut rng);
    scene.set_process_rng(Some(rng));
    let queue_before = format!("{:?}", scene.queue);
    let RenderResourceData::Model(model) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 162,
        })
        .unwrap()
        .data
    else {
        unreachable!("original H8 model")
    };
    let expected_prefix = [
        (0xf5, [1, 0x80, 0x800a]),
        (0xfc, [2, 0xc1, 0x8000]),
        (0xf0, [5, 0x84, 1]),
        (0xf0, [6, 0x83, 1]),
    ];
    for (instruction, (opcode, arguments)) in model.program.instructions.iter().zip(expected_prefix)
    {
        assert_eq!(instruction.raw_command, opcode);
        let ModelInstructionKind::RegisterOperation(command) = &instruction.kind else {
            panic!("original162 prefix must use packed register operations")
        };
        assert_eq!(command.arguments.as_slice(), &arguments);
    }
    assert!(
        ObjectModelCallbacks {
            arena: &arena,
            source,
            animation: &clock,
            executable,
        }
        .packed_value(4)
        .is_ok(),
        "the reached prior vehicle getter has actual backing"
    );
    let result = scene.draw_object_model(SoftwareObjectModelDrawRequest {
        terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
        root_mode: crate::model_execution::ModelRootMode::CommonVisual,
        context: &crate::object_runtime::ObjectDrawContext {
            view: scene.environment.view,
            near: scene.environment.projection.fade[0],
            far: scene.environment.projection.fade[1],
            far_color: scene.primitive_state.far_color,
        },
        packet: ObjectModelPacket {
            source,
            model_slot: 162,
            position: [0, 0, 100_000],
            basis: MenuBasis::INITIAL,
        },
        arena: &mut arena,
        animation: &observed_clock,
        executable,
    });
    // Original162 reads clock0 and vehicle4, then its F0 at +18 reaches
    // selector3/+1FC before any primitive. Earlier getter success is real.
    assert!(matches!(result, Err(SoftwareSceneError::Model(
        ModelExecutionError::Value(ModelValueError::Callback(
            ModelCommandError::Service(SoftwareSceneServiceError::ObjectModelCallback(
                ObjectModelCallbackError::RequiredField { object, offset: 0x1fc }
            ))
        ))
    )) if object == source));
    assert_eq!(
        observed_clock.reads.get(),
        1,
        "clock is read before the field stop"
    );
    assert_eq!(format!("{:?}", scene.queue), queue_before);
    assert_eq!(scene.process_rng(), Some(rng));
    assert_eq!(scene.stats.models, 0);
    assert!(!scene.queue.state().stopped);
    assert!(matches!(
        scene.submit(ResolvedSceneCommand::FlushBatch),
        Err(SoftwareSceneError::Stopped)
    ));
    scene.continue_after_feature_error().unwrap();
    scene
        .star(
            StarQuad {
                corners: [[30, 10], [30, 20], [40, 20], [40, 10]],
                color_argument: 0,
            },
            0x0000_ff00,
        )
        .unwrap();
    assert_eq!(scene.queue.state().command_count, 2);
    scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
    assert_eq!(scene.queue.state().last_flush_completed, 2);
    assert!(scene.queue.is_fully_replayed());
    assert!(scene.pixels.stats.fragments > 0);
    assert_eq!(scene.process_rng(), Some(rng));
    assert_eq!(format!("{arena:?}"), arena_before);
}

#[test]
fn actual_object_group_containment_preserves_payloads_rng_and_caller_custody() {
    use crate::object_runtime::Flag0ObjectDrawFailurePolicy::{
        Stop, UnownedObjectDrawCallbackApproximation,
    };
    for (policy, caller_group) in [
        (Stop, false),
        (UnownedObjectDrawCallbackApproximation, false),
        (UnownedObjectDrawCallbackApproximation, true),
    ] {
        let (assets, tables, resident) = fixture();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let mut arena = arena();
        let source = vehicle(&mut arena);
        arena.forget_vehicle_track_phase(source).unwrap();
        let clock = AnimationClock::default();
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
        // Actual earlier independent payload and optional caller-owned group.
        scene
            .star(
                StarQuad {
                    corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                    color_argument: 0,
                },
                0x0000_00ff,
            )
            .unwrap();
        if caller_group {
            scene
                .queue
                .begin_group(7, true, |_| Ok::<_, std::convert::Infallible>(()))
                .unwrap();
        }
        let root_was_open = scene.queue.has_only_implicit_root();
        assert_eq!(root_was_open, !caller_group);
        let mut rng = 0x1020_3040;
        let callback_error = {
            let services = ModelServices {
                callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                    arena: &mut arena,
                    context: &TEST_CONTEXT,
                    source,
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
            };
            let mut commands = ModelCommandBackend::new(
                scene.resources,
                &tables,
                scene.primitive_state,
                &mut scene.queue,
                services,
            );
            // Real queue protocol and captured getter, explicitly supplied command
            // sequence. This does not pretend an original model reaches this pair.
            commands.begin_depth_group(12, false).unwrap();
            // Actual queued payload inside the current object's new group, before
            // its reached packed getter failure. Closing retains this exact node.
            commands
                .queue
                .submit(crate::draw_queue::DrawSubmission {
                    depth: 0,
                    payload_blocks: 3,
                    receiver_offset: 0,
                    command: star_command(
                        StarQuad {
                            corners: [[30, 10], [30, 20], [40, 20], [40, 10]],
                            color_argument: 0,
                        },
                        0x00ff_0000,
                        0,
                    ),
                })
                .unwrap();
            crate::name_entry::name_entry_rng_next(commands.process_rng().unwrap());
            let mut values = ModelValues::new_invocation(Arc::clone(&tables.sine_table));
            commands.packed_value(&mut values, 3).unwrap_err()
        };
        assert_eq!(
            scene.queue.state().open_groups,
            if caller_group { 3 } else { 2 }
        );
        assert!(
            !scene.queue.state().stopped,
            "the actual missing getter does not corrupt queue"
        );
        let queue_prefix = format!("{:?}", scene.queue);
        scene.set_process_rng(Some(rng));
        // Same scene stop that draw_object_model sets on a returned model error.
        scene.stopped = true;
        let result = SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            effects: None,
            scene: &mut scene,
            animation: &clock,
            executable,
            attract_demo: &false,
            failure_policy: policy,
        }
        .model_error(
            162,
            root_was_open,
            SoftwareSceneError::Model(ModelExecutionError::Value(ModelValueError::Callback(
                callback_error,
            ))),
        );
        if policy == UnownedObjectDrawCallbackApproximation && !caller_group {
            assert!(matches!(
                result,
                Err(ObjectDrawServiceError::UnownedCallback(
                    ObjectDrawCallbackBoundary::ModelValue(_)
                ))
            ));
            assert!(!scene.stopped);
            assert!(scene.queue.has_only_implicit_root());
            assert_eq!(scene.stats.object_group_prefix_closures, 1);
            assert_eq!(
                scene.queue.state().command_count,
                3,
                "two payloads and their real group node survive"
            );
            scene
                .star(
                    StarQuad {
                        corners: [[50, 10], [50, 20], [60, 20], [60, 10]],
                        color_argument: 0,
                    },
                    0x0000_ff00,
                )
                .unwrap();
            scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
            assert_eq!(scene.queue.state().last_flush_completed, 3);
            assert!(scene.queue.is_fully_replayed());
            assert!(scene.pixels.stats.fragments > 0);
        } else {
            if caller_group {
                assert!(matches!(
                    result,
                    Err(ObjectDrawServiceError::Backend(
                        SoftwareSceneError::ObjectModelGroupPrefix {
                            model_slot: 162,
                            root_was_open: false,
                            open_groups: 3,
                            ..
                        }
                    ))
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(ObjectDrawServiceError::UnownedCallback(
                        ObjectDrawCallbackBoundary::ModelValue(_)
                    ))
                ));
            }
            assert!(scene.stopped);
            assert_eq!(format!("{:?}", scene.queue), queue_prefix);
            assert_eq!(scene.stats.object_group_prefix_closures, 0);
        }
        assert_eq!(scene.process_rng(), Some(rng));
        assert_ne!(rng, 0x1020_3040);
    }
}

// All original H8/H13/OVL bytes remain unchanged. The selected caller pose is a
// supplied test domain; this is not evidence of an original rendered frame.

use crate::model_vertices::{
    ModelCustomVertexError, ModelCustomVertexPolicy, ModelCustomVertexRequest, ModelVertexError,
    ModelVertexInput, ModelVertexProjection, ModelVertexProjector, ModelVertices,
};
use crate::object_runtime::{
    construct_world_object, ActorFactoryInputs, BaseContext, CreationListeners,
    DerivedActorFactoryInputs, ObjectConstructionOutcome, ObjectConstructionRequest,
};

fn original_class11_source(
    arena: &mut ObjectArena,
    assets: &FrontendAssets,
    resident: &ResidentRuntime,
) -> ObjectId {
    let placements =
        crate::world_placement::WorldPlacements::new(resident, TerrainSpacing::Units256).unwrap();
    let limits = placements.world().actor_mask_limits();
    let mut listeners = CreationListeners::new();
    let mut effects = crate::world_backend::EmptyListenerEffects;
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let mut rng = 17;
    // Original H13 global11 is the parent1 tag10/class11 record; its real
    // BYTE44=6 creates Mode06 through the existing shared admission owner.
    // No suffix is used here: model552 is an explicit original-H8 fixture.
    let result = construct_world_object(
        arena,
        resident,
        ObjectConstructionRequest {
            resource_argument: 11,
            context: BaseContext {
                position_words: Some([0xfff0, 0x0040, 0x8001]),
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(ActorFactoryInputs {
                allocation: allocation(),
                rotation_words: Some([0; 3]),
                local_owner: 1,
                first_player_grants: None,
                grant_table: None,
                math: &math,
                derived: Some(DerivedActorFactoryInputs {
                    world_mask_limits: limits,
                    rng: &mut rng,
                }),
                listeners: &mut listeners,
                listener_effects: &mut effects,
                network_world_active: false,
                terrain: None,
                world_start: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
            }),
        },
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(source) = result else {
        panic!("canonical original class11 resource accepted");
    };
    assert!(arena.selector11_actor(source).is_ok());
    let behavior = arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(behavior.byte_0d, 1);
    assert_eq!(behavior.groups_1c.len(), 1);
    assert_ne!(behavior.groups_1c[0].word_0c & 4, 0);
    source
}

fn caller_rebase(words: [u16; 3], eye: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| {
        i32::from(words[i].wrapping_sub(eye[i] as u16) as i16).wrapping_add(eye[i])
    })
}

struct FailAfterRealCallback {
    actual_input: Option<ModelVertexInput>,
}
impl ModelVertexProjector for FailAfterRealCallback {
    type Error = &'static str;
    fn project(
        &mut self,
        input: ModelVertexInput,
        _: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        self.actual_input = Some(input);
        Err("declared later projector boundary")
    }
}

#[test]
fn actual_service_rebases_caller_eye_and_retains_capture_on_later_projection_error() {
    let (assets, tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let source = original_class11_source(&mut arena, &assets, &resident);
    let actor_position = arena.object(source).unwrap().base.position_words.unwrap();
    let actor_basis = arena.actor(source).unwrap().rotating.basis;
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
    let RenderResourceData::Model(child) = scene
        .resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 554,
        })
        .unwrap()
        .data
    else {
        panic!("original H8 child554");
    };
    let instruction = child
        .program
        .instructions
        .iter()
        .find(|instruction| instruction.span == ByteSpan::new(374104, 374114))
        .expect("original first child554 custom instruction");
    assert_eq!(instruction.raw_command, 0x222d);
    let ModelInstructionKind::UnresolvedDynamic(raw) = &instruction.kind else {
        panic!("original custom packet");
    };
    assert_eq!(raw.arguments, [28, 2, 10, 0]);
    let model_context = ModelContext::new([0x3_fff0, -0x2_0000, 0x5_8001], MenuBasis::INITIAL);
    assert_ne!(model_context.position, context.view.eye);
    let mut vertices = ModelVertices::new_invocation(child.header.vertex_slot_count);
    let [a, b, c, _] = child.header.vertex_ranges;
    // The genuine world-range phase is shared by both roots. We intentionally
    // start at that phase instead of asserting the full child chain rendered.
    vertices
        .initialize_world_ranges(
            child,
            &model_context,
            [i32::from(a), i32::from(b), i32::from(c)],
        )
        .unwrap();
    let captured = vertices.row_words_439e10(10).unwrap();
    let source_slot_before = *vertices.slot(10).unwrap();
    let destination_before = *vertices.slot(28).unwrap();
    let expected = caller_rebase(captured, context.view.eye);
    assert_ne!(expected, caller_rebase(captured, model_context.position));
    let local = actor_basis.inverse_words(std::array::from_fn(|i| {
        captured[i].wrapping_sub(actor_position[i]) as i16
    }));
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
        assert_eq!(
            services.custom_vertex_policy(),
            ModelCustomVertexPolicy::RequiredObject
        );
        let result = vertices.execute_custom_2d(
            instruction,
            ModelCustomVertexPolicy::RequiredObject,
            &mut projector,
            &mut |request, vertices, output| {
                // Call the actual service. Recording happens only after real
                // success; no callback output or group mutation is simulated.
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
    assert!(vertices.stopped());
    assert_eq!(observed_output, Some(expected));
    assert_eq!(
        projector.actual_input,
        Some(ModelVertexInput::World(expected))
    );
    assert_eq!(vertices.slot(10).unwrap(), &source_slot_before);
    assert_eq!(
        vertices.slot(28).unwrap(),
        &destination_before,
        "mode2 projects without world-store"
    );
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [
            Some(local[0] as u16),
            Some(local[1] as u16),
            Some(local[2] as u16),
            Some(captured[0]),
            Some(captured[1]),
            Some(captured[2])
        ]
    );
    assert_eq!(group.word_0c, 3);
    assert_eq!(format!("{:?}", scene.queue), queue_before);
}

#[test]
fn original_gun_children_use_the_same_actual_source_and_caller_loan_in_both_roots() {
    use crate::model_execution::{ModelDrawRequest, ModelRootMode};
    for root_mode in [ModelRootMode::CommonVisual, ModelRootMode::ManagerLate] {
        let (assets, tables, mut resident) = fixture();
        resident
            .apply(ResidencyRequest::Activate { level: 32 })
            .unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let mut arena = arena();
        let source = original_class11_source(&mut arena, &assets, &resident);
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
        let mut rng = 0x1020_3040;
        let observed = {
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
                    model_argument: 552,
                    context: ModelContext::new([65536, -65536, 75536], MenuBasis::INITIAL),
                    environment: scene.environment,
                    surface: crate::model_vertices::ModelSurfaceBinding::Default,
                },
                &mut commands,
            );
            assert!(
                commands.actual.services.customs.len() >= 2,
                "root={root_mode:?}, actual callbacks={:?}, reached error={result:?}",
                commands.actual.services.customs
            );
            std::mem::take(&mut commands.actual.services.customs)
        };
        // Original552→553→554 runs both genuine2D records. These observations
        // establish callback reach, not whole-root/pixel or retail acceptance.
        for (call, extras) in observed.iter().take(2).zip([[10, 0], [12, 1]]) {
            assert_eq!(call.source, source);
            assert_eq!(call.selector, 2);
            assert_eq!(call.extras, extras);
            assert_eq!(call.output, caller_rebase(call.words, context.view.eye));
        }
        let group = &arena
            .derived_actor(source)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .groups_1c[0];
        assert_eq!(group.word_0c, 3);
        assert_eq!(group.coordinates_00_0a[3..], observed[0].words.map(Some));
        assert!(!scene.queue.state().stopped);
    }
}

#[derive(Clone, Copy, Debug)]
enum PrimitiveRecoveryCase {
    Strict,
    Approximate,
    CallerGroup,
    AbortedPresentation,
    QueueStopped,
}

#[test]
fn completed_original_custom_draw_survives_supplied_primitive_failure_policy() {
    use crate::object_runtime::Flag0ObjectDrawFailurePolicy::{
        Stop, UnownedObjectDrawCallbackApproximation,
    };
    for case in [
        PrimitiveRecoveryCase::Strict,
        PrimitiveRecoveryCase::Approximate,
        PrimitiveRecoveryCase::CallerGroup,
        PrimitiveRecoveryCase::AbortedPresentation,
        PrimitiveRecoveryCase::QueueStopped,
    ] {
        let (assets, tables, mut resident) = fixture();
        resident
            .apply(ResidencyRequest::Activate { level: 32 })
            .unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let mut arena = arena();
        let source = original_class11_source(&mut arena, &assets, &resident);
        let source_before = arena.object(source).unwrap().clone();
        let clock = AnimationClock::default();
        let context = ObjectDrawContext {
            view: SceneView {
                eye: [65536, -65536, 65536],
                basis: MenuBasis::INITIAL,
            },
            ..TEST_CONTEXT
        };
        let mut pixels = SoftwareFrame::new(64, 48, [17, 23, 31, 255]);
        let mut supplied_config = config(&resident);
        supplied_config.environment.view = context.view;
        supplied_config.pipeline.viewport = [0, 0, 63, 47];
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (supplied_config).environment,
            ),
            resources: RenderResources::new(&resident),
            tables: &tables,
            frame: &mut pixels,
            config: supplied_config,
        })
        .unwrap();
        scene.set_process_rng(Some(0x1020_3040));
        // Earlier independent actual payload: recovery must retain it.
        scene
            .star(
                StarQuad {
                    corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                    color_argument: 0,
                },
                0x0000_00ff,
            )
            .unwrap();
        // Run the actual common root, children and mutable source service.
        // Its former DA clipped boundary is implemented and must now complete.
        scene
            .draw_object_model(SoftwareObjectModelDrawRequest {
                terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
                packet: ObjectModelPacket {
                    source,
                    model_slot: 552,
                    position: [65536, -65536, 165536],
                    basis: MenuBasis::INITIAL,
                },
                root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                context: &context,
                arena: &mut arena,
                animation: &clock,
                executable,
            })
            .unwrap();
        assert!(!scene.stopped);
        assert!(!scene.queue.state().stopped);
        let group = &arena
            .derived_actor(source)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .groups_1c[0];
        assert_eq!(group.word_0c, 3, "the preceding real selector2 completed");
        assert!(group.coordinates_00_0a.iter().all(Option::is_some));
        assert_ne!(arena.object(source).unwrap(), &source_before);
        scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
        assert!(scene.queue.is_fully_replayed());
        assert!(scene.pixels.stats.fragments > 0);

        // Begin a separate compact recovery-policy fixture. Debug snapshots of
        // the actual textured queue would expand each packet's entire OVL slice.
        // The completed original draw and its pixels remain established above.
        scene.queue.reset();
        scene
            .star(
                StarQuad {
                    corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                    color_argument: 0,
                },
                0x0000_00ff,
            )
            .unwrap();
        if matches!(case, PrimitiveRecoveryCase::CallerGroup) {
            scene
                .queue
                .begin_group(7, true, |_| Ok::<_, std::convert::Infallible>(()))
                .unwrap();
        }
        let root_was_open = scene.queue.has_only_implicit_root();
        assert_eq!(
            root_was_open,
            !matches!(case, PrimitiveRecoveryCase::CallerGroup)
        );
        let arena_prefix = format!("{arena:?}");
        let rng_prefix = scene.process_rng();
        // Caller-origin output is independently pinned by the actual service
        // and recorded child tests. Here group backing is its persistent prefix.

        // Deliberately supplied returned-error policy domain after the completed
        // draw, not a claim that original552 still reaches an unsupported DA.
        // A real later group/payload pins closure without dropping prior output.
        scene
            .queue
            .begin_group(12, false, |_| Ok::<_, std::convert::Infallible>(()))
            .unwrap();
        scene
            .star(
                StarQuad {
                    corners: [[30, 10], [30, 20], [40, 20], [40, 10]],
                    color_argument: 0,
                },
                0x00ff_0000,
            )
            .unwrap();
        let error = SoftwareSceneError::Model(ModelExecutionError::Host(
            ModelCommandError::Primitive(crate::model_primitives::PrimitiveError::Unsupported {
                opcode: 0xda,
                reason: "supplied unsupported primitive policy fixture",
            }),
        ));
        scene.stopped = true;

        if matches!(case, PrimitiveRecoveryCase::AbortedPresentation) {
            // Explicit supplied port guard state; no native writer claim.
            scene.aborted_presentation = true;
        }
        if matches!(case, PrimitiveRecoveryCase::QueueStopped) {
            // Genuine queue failure after the retained model prefix. This
            // tests recovery's current guard, not a claim DA caused this fault.
            let fault = scene
                .queue
                .submit(crate::draw_queue::DrawSubmission {
                    depth: 0,
                    payload_blocks: usize::MAX,
                    receiver_offset: 0,
                    command: star_command(
                        StarQuad {
                            corners: [[30, 10], [30, 20], [40, 20], [40, 10]],
                            color_argument: 0,
                        },
                        0x00ff_0000,
                        0,
                    ),
                })
                .unwrap_err();
            assert_eq!(fault, crate::draw_queue::DrawQueueFault::AllocationSize);
            assert!(scene.queue.state().stopped);
        }
        let queue_prefix = format!("{:?}", scene.queue);
        let queue_state_prefix = scene.queue.state();
        let policy = if matches!(case, PrimitiveRecoveryCase::Strict) {
            Stop
        } else {
            UnownedObjectDrawCallbackApproximation
        };
        let result = SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            effects: None,
            scene: &mut scene,
            animation: &clock,
            executable,
            attract_demo: &false,
            failure_policy: policy,
        }
        .model_error(552, root_was_open, error);
        assert_eq!(format!("{arena:?}"), arena_prefix);
        assert_eq!(scene.process_rng(), rng_prefix);
        match case {
            PrimitiveRecoveryCase::Strict | PrimitiveRecoveryCase::Approximate => {
                assert!(
                    matches!(result, Err(ObjectDrawServiceError::UnownedCallback(
                    ObjectDrawCallbackBoundary::ModelFeature { model_slot: 552, ref reason }
                )) if reason.contains("0xda") && reason.contains("supplied unsupported primitive policy fixture"))
                );
                if matches!(case, PrimitiveRecoveryCase::Strict) {
                    assert!(scene.stopped);
                    assert_eq!(format!("{:?}", scene.queue), queue_prefix);
                    assert_eq!(scene.stats.object_group_prefix_closures, 0);
                } else {
                    assert!(!scene.stopped);
                    assert!(scene.queue.has_only_implicit_root());
                    assert_eq!(
                        scene.queue.state().command_count,
                        queue_state_prefix.command_count
                    );
                    assert_eq!(
                        scene.stats.object_group_prefix_closures,
                        queue_state_prefix.open_groups - 1
                    );
                    // Recovery admits an independent next payload after the
                    // supplied failure without dropping the completed draw.
                    scene
                        .star(
                            StarQuad {
                                corners: [[50, 10], [50, 20], [60, 20], [60, 10]],
                                color_argument: 0,
                            },
                            0x0000_ff00,
                        )
                        .unwrap();
                    assert_eq!(
                        scene.queue.state().command_count,
                        queue_state_prefix.command_count + 1
                    );
                    scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
                    assert!(scene.queue.is_fully_replayed());
                    assert!(scene.pixels.stats.fragments > 0);
                }
            }
            PrimitiveRecoveryCase::CallerGroup => {
                assert!(matches!(
                    result,
                    Err(ObjectDrawServiceError::Backend(
                        SoftwareSceneError::ObjectModelGroupPrefix {
                            model_slot: 552,
                            root_was_open: false,
                            ..
                        }
                    ))
                ));
                assert!(scene.stopped);
                assert_eq!(format!("{:?}", scene.queue), queue_prefix);
                assert_eq!(scene.stats.object_group_prefix_closures, 0);
            }
            PrimitiveRecoveryCase::AbortedPresentation | PrimitiveRecoveryCase::QueueStopped => {
                assert!(matches!(
                    result,
                    Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Stopped))
                ));
                assert!(scene.stopped);
                assert_eq!(format!("{:?}", scene.queue), queue_prefix);
                assert_eq!(scene.stats.object_group_prefix_closures, 0);
            }
        }
    }
}

#[test]
fn returned_nonunsupported_host_errors_never_acquire_feature_recovery() {
    // Deliberately supplied returned-error policy domains, not a claim these
    // errors occur in original552. Real source/queue suites own actual reaches.
    use crate::model_primitives::PrimitiveError;
    use crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation;
    let errors: [ModelCommandError<SoftwareSceneServiceError>; 7] = [
        ModelCommandError::Queue(crate::draw_queue::DrawQueueFault::ArenaFull {
            requested: 64,
            available: 0,
        }),
        ModelCommandError::Publication,
        ModelCommandError::Stopped,
        ModelCommandError::Service(SoftwareSceneServiceError::TextureResource),
        ModelCommandError::Primitive(PrimitiveError::Unknown {
            field: "fog byte",
            index: 0,
        }),
        ModelCommandError::Primitive(PrimitiveError::Record(0)),
        ModelCommandError::Primitive(PrimitiveError::Vertex(
            crate::model_vertices::ModelVertexFault::UnknownWorld { index: 0 },
        )),
    ];
    for (index, error) in errors.into_iter().enumerate() {
        let (assets, tables, resident) = fixture();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let clock = AnimationClock::default();
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
        scene.set_process_rng(Some(0x1020_3040));
        scene
            .star(
                StarQuad {
                    corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                    color_argument: 0,
                },
                0x0000_00ff,
            )
            .unwrap();
        scene
            .queue
            .begin_group(7, false, |_| Ok::<_, std::convert::Infallible>(()))
            .unwrap();
        let queue_prefix = format!("{:?}", scene.queue);
        let rng_prefix = scene.process_rng();
        let error = SoftwareSceneError::Model(ModelExecutionError::Host(error));
        scene.stopped = true; //explicit returned execution-failure state
        let result = SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            effects: None,
            scene: &mut scene,
            animation: &clock,
            executable,
            attract_demo: &false,
            failure_policy: UnownedObjectDrawCallbackApproximation,
        }
        .model_error(552, true, error);
        let Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Model(
            ModelExecutionError::Host(actual),
        ))) = result
        else {
            panic!("only Unsupported can enter feature recovery");
        };
        match (index, &actual) {
            (
                0,
                ModelCommandError::Queue(crate::draw_queue::DrawQueueFault::ArenaFull {
                    requested: 64,
                    available: 0,
                }),
            )
            | (1, ModelCommandError::Publication)
            | (2, ModelCommandError::Stopped)
            | (3, ModelCommandError::Service(SoftwareSceneServiceError::TextureResource))
            | (
                4,
                ModelCommandError::Primitive(PrimitiveError::Unknown {
                    field: "fog byte",
                    index: 0,
                }),
            )
            | (5, ModelCommandError::Primitive(PrimitiveError::Record(0)))
            | (
                6,
                ModelCommandError::Primitive(PrimitiveError::Vertex(
                    crate::model_vertices::ModelVertexFault::UnknownWorld { index: 0 },
                )),
            ) => {}
            _ => panic!("hard error identity changed: index={index}, actual={actual:?}"),
        }
        assert!(scene.stopped);
        assert_eq!(format!("{:?}", scene.queue), queue_prefix);
        assert_eq!(scene.process_rng(), rng_prefix);
        assert_eq!(scene.stats.object_group_prefix_closures, 0);
    }
}

fn original_class12_source(
    arena: &mut ObjectArena,
    assets: &FrontendAssets,
    resident: &ResidentRuntime,
) -> ObjectId {
    let placements =
        crate::world_placement::WorldPlacements::new(resident, TerrainSpacing::Units256).unwrap();
    let terrain = placements.terrain();
    let limits = placements.world().actor_mask_limits();
    let grant_table = inf_formats::parse_grant_table(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let mut listeners = CreationListeners::new();
    let mut effects = crate::world_backend::EmptyListenerEffects;
    let mut rng = 17;
    // Parent1's original H13 global3 tag11 selects constructor12/model223.
    // A real H12 and EXE GrantTable are required by this complete constructor;
    // neither is a replacement resource, and the suffix remains genuinely absent.
    let result = construct_world_object(
        arena,
        resident,
        ObjectConstructionRequest {
            resource_argument: 3,
            context: BaseContext {
                position_words: Some([0xfff0, 0x0040, 0x8001]),
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(ActorFactoryInputs {
                allocation: allocation(),
                rotation_words: Some([0; 3]),
                local_owner: 1,
                first_player_grants: None,
                grant_table: Some(&grant_table),
                math: &math,
                derived: Some(DerivedActorFactoryInputs {
                    world_mask_limits: limits,
                    rng: &mut rng,
                }),
                listeners: &mut listeners,
                listener_effects: &mut effects,
                network_world_active: false,
                terrain: Some(terrain),
                world_start: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
            }),
        },
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(source) = result else {
        panic!("original H13 global3 constructor12 admitted");
    };
    assert!(arena.selector12_actor(source).is_ok());
    source
}

#[test]
fn original_scientist_prefix_reaches_real_height_callback_in_both_roots() {
    use crate::model_execution::{ModelDrawRequest, ModelRootMode};
    for root_mode in [ModelRootMode::CommonVisual, ModelRootMode::ManagerLate] {
        for budget in [13, 14] {
            let (assets, tables, mut resident) = fixture();
            resident
                .apply(ResidencyRequest::Activate { level: 32 })
                .unwrap();
            let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
            let mut arena = arena();
            let source = original_class12_source(&mut arena, &assets, &resident);
            let actor_y = arena.object(source).unwrap().base.position_words.unwrap()[1];
            let height_before = arena.actor(source).unwrap().word_74;
            let clock = AnimationClock::default();
            let observed_clock = ObservedClock {
                clock: &clock,
                reads: Cell::new(0),
            };
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
            // A declared executor budget isolates the original prefix. Limit
            // 13 stops before the custom; 14 stops at the next real DA record.
            // It is not an assertion that the original game faults there.
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
            let queue_before = format!("{:?}", scene.queue);
            let mut rng = 0x1020_3040;
            let (values, observed) = {
                let services = RecordingServices {
                    actual: ModelServices {
                        callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                            arena: &mut arena,
                            source,
                            context: &context,
                            animation: &observed_clock,
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
                        model_argument: 223,
                        context: ModelContext::new([65536, -65536, 75536], MenuBasis::INITIAL),
                        environment: scene.environment,
                        surface: crate::model_vertices::ModelSurfaceBinding::Default,
                    },
                    &mut commands,
                );
                assert!(
                    matches!(result, Err(ModelExecutionError::Limit)),
                    "root={root_mode:?}, budget={budget}, result={result:?}"
                );
                assert!(commands.models.is_empty(), "budget stops before first DA");
                (
                    std::mem::take(&mut commands.actual.services.values),
                    std::mem::take(&mut commands.actual.services.customs),
                )
            };
            assert_eq!(observed_clock.reads.get(), 4);
            assert_eq!(values, vec![(0, 0); 4]);
            if budget == 13 {
                assert!(observed.is_empty());
                assert_eq!(arena.actor(source).unwrap().word_74, height_before);
            } else {
                assert_eq!(observed.len(), 1);
                let call = &observed[0];
                assert_eq!(call.source, source);
                assert_eq!(call.selector, 10);
                assert_eq!(call.extras, [44, 0]);
                let expected_height = call.words[1].wrapping_sub(actor_y);
                assert_eq!(call.actor_word_74, expected_height);
                assert_eq!(arena.actor(source).unwrap().word_74, expected_height);
                assert_eq!(call.output, caller_rebase(call.words, context.view.eye));
            }
            assert_eq!(format!("{:?}", scene.queue), queue_before);
            assert_eq!(rng, 0x1020_3040, "original prefix consumes no RNG");
        }
    }
}

#[test]
fn original_scientist_custom_retains_height_and_full_eye_output_before_projection_error() {
    let (assets, tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let source = original_class12_source(&mut arena, &assets, &resident);
    let actor_y = arena.object(source).unwrap().base.position_words.unwrap()[1];
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
            argument: 223,
        })
        .unwrap()
        .data
    else {
        panic!("original model223");
    };
    let instruction = &model.program.instructions[13];
    assert_eq!(instruction.span, ByteSpan::new(190148, 190158));
    assert_eq!(instruction.raw_command, 0x222d);
    let ModelInstructionKind::UnresolvedDynamic(raw) = &instruction.kind else {
        panic!("original custom packet");
    };
    assert_eq!(raw.arguments, [54, 10, 44, 0]);
    let model_context = ModelContext::new([0x3_fff0, -0x2_0000, 0x5_8001], MenuBasis::INITIAL);
    let mut vertices = ModelVertices::new_invocation(model.header.vertex_slot_count);
    let [a, b, c, _] = model.header.vertex_ranges;
    vertices
        .initialize_world_ranges(
            model,
            &model_context,
            [i32::from(a), i32::from(b), i32::from(c)],
        )
        .unwrap();
    let row = vertices.row_words_439e10(44).unwrap();
    let row_before = *vertices.slot(44).unwrap();
    let destination_before = *vertices.slot(54).unwrap();
    let expected = caller_rebase(row, context.view.eye);
    assert_ne!(expected, caller_rebase(row, model_context.position));
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
        arena.actor(source).unwrap().word_74,
        row[1].wrapping_sub(actor_y)
    );
    assert_eq!(observed_output, Some(expected));
    assert_eq!(
        projector.actual_input,
        Some(ModelVertexInput::World(expected))
    );
    assert!(vertices.stopped());
    assert_eq!(vertices.slot(44).unwrap(), &row_before);
    assert_eq!(
        vertices.slot(54).unwrap(),
        &destination_before,
        "original common mode2 projects without world-store"
    );
    assert_eq!(format!("{:?}", scene.queue), queue_before);
    // The fixture initializes actual row44 but deliberately starts at this
    // packet. This projector-negative case does not claim whole-prefix reach,
    // and cannot be described as ManagerLate (which forces world-only mode1).
}

// Genuine selector36 allocation domain; these tests do not claim full H13
// postinitialization or parent492 traversal. Original497 bytes stay unchanged.
fn special11_service_source(arena: &mut ObjectArena) -> ObjectId {
    let source = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap();
    assert_eq!(
        arena.selector36_actor(source).unwrap().words_278_27c,
        [None; 3]
    );
    assert_eq!(arena.family4236d0_actor(source).unwrap().word_264, 0);
    source
}

fn special11_service_custody_except_capture(arena: &ObjectArena, source: ObjectId) -> String {
    use crate::object_runtime::{
        ActorExtension, DerivedActorClass, Family4236d0Class, Family428650Class, Family429e20Class,
        ObjectClass,
    };
    let current = arena.object(source).unwrap();
    let exact = format!("{current:?}");
    let mut normalized = current.clone();
    let ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("actual actor");
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived");
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual family");
    };
    let Family428650Class::Family429e20(family) = &mut family.class else {
        panic!("actual family");
    };
    let Family429e20Class::Family4236d0(family) = &mut family.class else {
        panic!("actual family");
    };
    family.word_264 = 0;
    let Family4236d0Class::Selector36(state) = &mut family.class else {
        panic!("actual selector36");
    };
    state.words_278_27c = [None; 3];
    let whole = format!("{arena:?}");
    assert_eq!(whole.matches(&exact).count(), 1);
    whole.replacen(&exact, &format!("{normalized:?}"), 1)
}

#[test]
fn original497_common_and_late_first_record_capture_same_source_and_caller_origin() {
    use crate::model_execution::{ModelDrawRequest, ModelExecutionError, ModelRootMode};
    for root_mode in [ModelRootMode::CommonVisual, ModelRootMode::ManagerLate] {
        let (assets, tables, mut resident) = fixture();
        resident
            .apply(ResidencyRequest::Activate { level: 32 })
            .unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let mut arena = arena();
        let source = special11_service_source(&mut arena);
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
        let mut supplied_config = config(&resident);
        supplied_config.environment.view = context.view;
        // Actual original497's first record is the callback; the next record
        // is stopped by a declared executor budget, not a native fault.
        supplied_config.execution_limits.instructions = 1;
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (supplied_config).environment,
            ),
            resources: RenderResources::new(&resident),
            tables: &tables,
            frame: &mut pixels,
            config: supplied_config,
        })
        .unwrap();
        let RenderResourceData::Model(model) = scene
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 497,
            })
            .unwrap()
            .data
        else {
            panic!("original H8/497");
        };
        assert_eq!(model.program.instructions[0].raw_command, 0x2f2d);
        assert_eq!(
            model.program.instructions[0].span,
            ByteSpan::new(445084, 445094)
        );
        assert_eq!(model.vertices[6].x, 0);
        assert_eq!(model.vertices[6].y, 141);
        assert_eq!(model.vertices[6].z, -15);
        let queue_before = format!("{:?}", scene.queue);
        let mut rng = 0x1020_3040;
        let (result, observed) = {
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
                    model_argument: 497,
                    context: ModelContext::new([65536, -65536, 75536], MenuBasis::INITIAL),
                    environment: scene.environment,
                    surface: crate::model_vertices::ModelSurfaceBinding::Default,
                },
                &mut commands,
            );
            (
                result,
                std::mem::take(&mut commands.actual.services.customs),
            )
        };
        assert!(
            matches!(result, Err(ModelExecutionError::Limit)),
            "root={root_mode:?}: {result:?}"
        );
        assert_eq!(
            observed.len(),
            if root_mode == ModelRootMode::CommonVisual {
                2
            } else {
                1
            }
        );
        for call in &observed {
            assert_eq!(call.source, source);
            assert_eq!(call.selector, 11);
            assert_eq!(call.extras, [12, 0]);
            assert_eq!(call.output, caller_rebase(call.words, context.view.eye));
        }
        assert_eq!(
            arena.selector36_actor(source).unwrap().words_278_27c,
            observed.last().unwrap().words.map(Some)
        );
        assert_eq!(arena.family4236d0_actor(source).unwrap().word_264, 0x0800);
        assert_eq!(
            special11_service_custody_except_capture(&arena, source),
            arena_before
        );
        assert_eq!(format!("{:?}", scene.queue), queue_before);
        assert_eq!(rng, 0x1020_3040);
    }
}

#[test]
fn original497_actual_capture_rebase_and_world_store_survive_later_projector_failure() {
    let (assets, tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let source = special11_service_source(&mut arena);
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
            argument: 497,
        })
        .unwrap()
        .data
    else {
        panic!("original H8/497");
    };
    let instruction = &model.program.instructions[0];
    assert_eq!(instruction.span, ByteSpan::new(445084, 445094));
    assert_eq!(instruction.raw_command, 0x2f2d);
    let ModelInstructionKind::UnresolvedDynamic(raw) = &instruction.kind else {
        panic!("original2D");
    };
    assert_eq!(raw.arguments, [46, 11, 12, 0]);
    let model_context = ModelContext::new([0x3_fff0, -0x2_0000, 0x5_8001], MenuBasis::INITIAL);
    let mut vertices = ModelVertices::new_invocation(model.header.vertex_slot_count);
    let [a, b, c, _] = model.header.vertex_ranges;
    vertices
        .initialize_world_ranges(
            model,
            &model_context,
            [i32::from(a), i32::from(b), i32::from(c)],
        )
        .unwrap();
    let captured = vertices.row_words_439e10(12).unwrap();
    let source_before = *vertices.slot(12).unwrap();
    let destination46 = *vertices.slot(46).unwrap();
    let destination47 = *vertices.slot(47).unwrap();
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
    assert!(vertices.stopped());
    assert_eq!(observed_output, Some(expected));
    assert_eq!(
        projector.actual_input,
        Some(ModelVertexInput::World(expected))
    );
    assert_eq!(
        arena.selector36_actor(source).unwrap().words_278_27c,
        captured.map(Some)
    );
    assert_eq!(arena.family4236d0_actor(source).unwrap().word_264, 0x0800);
    assert_eq!(
        special11_service_custody_except_capture(&arena, source),
        arena_before
    );
    assert_eq!(vertices.slot(12).unwrap(), &source_before);
    let mut expected46 = destination46;
    expected46.world = Some(expected); //mode3 world store precedes actual projection
    assert_eq!(vertices.slot(46).unwrap(), &expected46);
    assert_eq!(vertices.slot(47).unwrap(), &destination47); //second pass unreached
    assert_eq!(format!("{:?}", scene.queue), queue_before);
}

// Append in software_scene/object_models/tests.rs. Uses the existing actual
// constructor/resource fixtures; it does not inject a returned classifier error.
#[test]
fn stopped_process_scratch_is_hard_through_real_owned_object_model_bridge() {
    use crate::model_scratch::{
        ModelScratchDrawExit, ModelScratchDrawState, ModelScratchError, ProcessModelScratch,
    };
    use crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation;

    let (assets, tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let resources = RenderResources::new(&resident);
    assert!(matches!(
        resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 497,
            })
            .unwrap()
            .data,
        RenderResourceData::Model(_)
    ));
    let mut arena = arena();
    let source = special11_service_source(&mut arena);
    let arena_before = format!("{arena:?}");
    let clock = AnimationClock::default();
    let observed = ObservedClock {
        clock: &clock,
        reads: Cell::new(0),
    };
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            ProcessModelScratch::isolated_unknown_fixture(),
            (config(&resident)).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: config(&resident),
    })
    .unwrap();
    scene.set_process_rng(Some(0x1020_3040));
    scene
        .star(
            StarQuad {
                corners: [[10, 10], [10, 20], [20, 20], [20, 10]],
                color_argument: 0,
            },
            0x0000_00ff,
        )
        .unwrap();

    // Explicit supplied stopped invocation. One real registered allocation and
    // actual byte writes establish the prefix before the next model request.
    let scope = scene.scratch.begin_draw().unwrap();
    let allocation = scene.scratch.allocate(&scope, 2, 2).unwrap();
    scene
        .scratch
        .write_vertex(&allocation, 0, 9, [Some(0xd5)])
        .unwrap();
    scene
        .scratch
        .write_shade(
            &allocation,
            1,
            0,
            [Some(0x17), Some(0x23), Some(0x31), Some(0xa5)],
        )
        .unwrap();
    scene
        .scratch
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    let pool_before = scene.scratch.read_pool::<56>(0).unwrap();
    let cursor_before = scene.scratch.cursor().unwrap();
    assert_eq!(cursor_before, allocation.layout.end);
    let queue_before = format!("{:?}", scene.queue);
    let rng_before = scene.process_rng();
    let stats_before = format!("{:?}", scene.stats);

    let result = Flag0ObjectDrawHost::model(
        &mut SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            effects: None,
            scene: &mut scene,
            animation: &observed,
            executable,
            attract_demo: &false,
            failure_policy: UnownedObjectDrawCallbackApproximation,
        },
        ObjectModelDrawSubmission {
            arena: &mut arena,
            context: &TEST_CONTEXT,
            packet: ObjectModelPacket {
                source,
                model_slot: 497,
                position: [65536, -65536, 75536],
                basis: MenuBasis::INITIAL,
            },
            root: crate::model_execution::ModelRootMode::CommonVisual,
        },
    );
    let Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Model(
        ModelExecutionError::Scratch(ModelScratchError::DrawState { expected, actual }),
    ))) = result
    else {
        panic!("stopped byte-pool custody must stay hard, got {result:?}");
    };
    assert_eq!(expected, "Ready");
    assert_eq!(actual, ModelScratchDrawState::Stopped(scope.clone()));
    assert!(scene.stopped());
    assert_eq!(
        scene.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(scope)
    );
    assert_eq!(scene.scratch.cursor().unwrap(), cursor_before);
    assert_eq!(scene.scratch.read_pool::<56>(0).unwrap(), pool_before);
    assert_eq!(
        scene.scratch.read_vertex::<1>(&allocation, 0, 9).unwrap(),
        [Some(0xd5)],
        "the actual allocation mark remains registered"
    );
    assert_eq!(format!("{:?}", scene.queue), queue_before);
    assert_eq!(scene.process_rng(), rng_before);
    assert_eq!(format!("{:?}", scene.stats), stats_before);
    assert_eq!(
        observed.reads.get(),
        0,
        "begin_draw fails before actor callbacks"
    );
    assert_eq!(format!("{arena:?}"), arena_before);
}
