use std::{path::Path, sync::Arc};

use inf_formats::FrontendExecutable;

use super::*;
use crate::{
    draw_queue::DrawInvocation,
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{
        FrontendScene, FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneFrame,
        SceneModelSubmission, SceneProjection, SceneView,
    },
    menu::MenuBasis,
    model_context::ModelProjection,
    model_execution::{ModelDrawRequest, ModelEnvironment, ModelExecutionLimits, ModelExecutor},
    model_lighting::ModelLighting,
    model_primitives::PrimitiveCallback,
    Installation, ResidencyRequest, ResidentRuntime,
};

const MODELS: [i16; 13] = [
    744, 745, 746, 747, 748, 749, 751, 752, 753, 754, 755, 756, 757,
];
const CAPACITY: u32 = 8 * 1024 * 1024; // explicit synthetic arena capacity

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn resident() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    resident
}

fn tables(assets: &FrontendAssets) -> PrimitiveTables {
    PrimitiveTables::from_executable(FrontendExecutable::parse(assets.executable_bytes()).unwrap())
        .unwrap()
}

fn submission(model_slot: i16, clock: u32) -> SceneModelSubmission {
    let mut animation = AnimationClock::default();
    animation.advance(clock * 20_000);
    assert_eq!(animation.value(), clock);
    SceneModelSubmission {
        model_slot,
        position: [0, 0, 100_000],
        basis: MenuBasis::INITIAL,
        animation,
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
        policy: ModelCallbackPolicy::Frontend,
    }
}

struct Services {
    submission: SceneModelSubmission,
    prepares: usize,
    selectors: Vec<u32>,
}

impl ModelValueCallback for Services {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        self.selectors.push(selector);
        self.submission
            .callback_value(selector)
            .ok_or("pure virtual callback")
    }
}

impl<'a> ModelCommandServices<'a> for Services {
    fn prepare_draw(
        &mut self,
        _: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error> {
        self.prepares += 1;
        // Explicit fixture renderer policy: no material batching. This is not
        // a production graphics backend or an assertion about selected GPU mode.
        Ok(false)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        Err("unbatched fixture must not ask for a fabricated texture address")
    }
}

fn environment(far: bool) -> ModelEnvironment {
    ModelEnvironment {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [4096; 2],
            viewport: [16384; 2],
            center: [8192; 2],
            fade: if far {
                [0, 1_000_000]
            } else {
                [1_000_000, 2_000_000]
            },
        },
        coordinate_shift: 0,
        fog_factor: 16,
        lighting: ModelLighting {
            direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            directional_word: 0x00aa_d2ff,
            ambient: [10, 6, 5],
            ambient_word: 0xab05_060a,
        },
    }
}

fn primitive_state(environment: ModelEnvironment) -> PrimitiveState {
    PrimitiveState {
        mode: 0,
        far_color: 0x1234_5678,
        effect_color: 0x8765_4321,
        projection: environment.projection,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Packet {
    invocation: DrawInvocation,
    callback: PrimitiveCallback,
    depth: u32,
    headers: [u32; 3],
    payload: Vec<Option<u8>>,
    texture: Option<(usize, u16, u16, u8)>,
}

fn snapshot(
    invocation: DrawInvocation,
    command: &PrimitiveCommand<'_>,
    resources: RenderResources<'_>,
) -> Packet {
    assert_eq!(command.payload.len(), command.payload_blocks * 16);
    assert_eq!(command.header_words, [0; 3]);
    assert_eq!(invocation.receiver_offset, command.header_words[0]);
    let texture = command.texture.map(|(offset, texture)| {
        assert_eq!(texture.request.kind, RenderResourceKind::Sprite);
        assert!(matches!(texture.data, RenderResourceData::Sprite(_)));
        let resident = resources.resolve(texture.request).unwrap();
        assert!(std::ptr::eq(
            texture.source.registration,
            resident.source.registration
        ));
        assert!(std::ptr::eq(texture.source.bytes, resident.source.bytes));
        assert!(command.payload[offset..offset + 4]
            .iter()
            .all(Option::is_none));
        (
            offset,
            texture.source.registration.global_index,
            texture.source.registration.local_index,
            texture.source.registration.level,
        )
    });
    Packet {
        invocation,
        callback: command.callback,
        depth: command.depth,
        headers: command.header_words,
        payload: command.payload.clone(),
        texture,
    }
}

fn flush(
    queue: &mut DrawQueue<PrimitiveCommand<'_>>,
    resources: RenderResources<'_>,
) -> Vec<Packet> {
    let mut packets = Vec::new();
    let count = queue
        .flush(|invocation, command| {
            packets.push(snapshot(invocation, command, resources));
            Ok::<_, Infallible>(())
        })
        .unwrap();
    assert_eq!(count, packets.len());
    packets
}

fn is_far(callback: PrimitiveCallback) -> bool {
    matches!(
        callback,
        PrimitiveCallback::TexturedTriangleFar
            | PrimitiveCallback::FlatTriangleFar
            | PrimitiveCallback::TexturedQuadFar
            | PrimitiveCallback::TexturedUvTriangleFar
            | PrimitiveCallback::TexturedUvQuadFar
            | PrimitiveCallback::EffectQuadFar
    )
}

#[test]
fn original_thirteen_model_closure_executes_actual_primitives_and_queue_at_four_clocks() {
    let resident = resident();
    let assets = assets();
    let tables = tables(&assets);
    let resources = RenderResources::new(&resident);
    let mut seen_callbacks = std::collections::BTreeSet::new();
    let mut animated_757 = Vec::new();
    for far in [false, true] {
        for clock in [0, 3, 1024, 65535] {
            for model in MODELS {
                let submission = submission(model, clock);
                let environment = environment(far);
                let context = ModelContext::new(submission.position, submission.basis);
                let mut queue = DrawQueue::new(CAPACITY).unwrap();
                let mut backend = ModelCommandBackend::new(
                    resources,
                    &tables,
                    primitive_state(environment),
                    &mut queue,
                    Services {
                        submission,
                        prepares: 0,
                        selectors: Vec::new(),
                    },
                );
                let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
                let mut executor = ModelExecutor::new(
                    resources,
                    Arc::clone(&tables.sine_table),
                    ModelExecutionLimits {
                        instructions: 100_000,
                        invocation_depth: 16,
                    },
                );
                let outcome = executor
                    .draw(
                        ModelDrawRequest {
                            scratch: &scratch,
                            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                            model_argument: i32::from(model),
                            context,
                            environment,
                            surface: crate::model_vertices::ModelSurfaceBinding::Default,
                        },
                        &mut backend,
                    )
                    .unwrap_or_else(|error| {
                        panic!("model {model}, far={far}, clock={clock}: {error:?}")
                    });
                assert!(outcome.instructions > 0);
                assert_eq!(
                    backend.dispatch(),
                    Some(if far {
                        ModelDispatch::Far
                    } else {
                        ModelDispatch::Near
                    })
                );
                assert_eq!(
                    backend.publication().unwrap().0.position,
                    submission.position
                );
                assert!(!backend.stopped());
                let prepared = backend.outcome().commands;
                assert!(prepared > 0, "model {model}, far={far}, clock={clock}");
                assert_eq!(backend.services.prepares, prepared + outcome.depth_groups);
                assert!(backend
                    .services
                    .selectors
                    .iter()
                    .all(|selector| *selector == 0));
                drop(backend);
                assert_eq!(queue.state().command_count, prepared + outcome.depth_groups);
                assert_eq!(queue.state().open_groups, 1);
                let packets = flush(&mut queue, resources);
                assert_eq!(packets.len(), prepared);
                assert!(packets.iter().all(|packet| is_far(packet.callback) == far));
                if model != 755 {
                    for pair in packets.windows(2) {
                        assert!((pair[0].depth as i32) >= (pair[1].depth as i32));
                        if pair[0].depth == pair[1].depth {
                            assert!(
                                pair[0].invocation.command_offset
                                    < pair[1].invocation.command_offset
                            );
                        }
                    }
                } else {
                    assert_eq!(outcome.depth_groups, 1);
                    let quad_indices: Vec<_> = packets
                        .iter()
                        .enumerate()
                        .filter_map(|(i, packet)| {
                            matches!(
                                packet.callback,
                                PrimitiveCallback::TexturedQuadNear
                                    | PrimitiveCallback::TexturedQuadFar
                            )
                            .then_some(i)
                        })
                        .collect();
                    assert_eq!(quad_indices.len(), 4);
                    assert!(quad_indices.windows(2).all(|pair| pair[1] == pair[0] + 1));
                    assert!(quad_indices
                        .windows(2)
                        .all(|pair| packets[pair[0]].invocation.command_offset
                            < packets[pair[1]].invocation.command_offset));
                }
                for packet in &packets {
                    seen_callbacks.insert(packet.callback.native_address());
                    match packet.callback {
                        PrimitiveCallback::TexturedTriangleNear
                        | PrimitiveCallback::TexturedTriangleFar => {
                            for index in [0x23, 0x27, 0x2b] {
                                assert_eq!(packet.payload[index], None);
                            }
                        }
                        PrimitiveCallback::TexturedQuadNear => {
                            assert!(packet.payload[0x28..].iter().all(Option::is_none))
                        }
                        PrimitiveCallback::EffectQuadNear => {
                            assert!(packet.payload[0x2c..].iter().all(Option::is_none))
                        }
                        PrimitiveCallback::EffectQuadFar => {
                            assert!(packet.payload[0x34..].iter().all(Option::is_none))
                        }
                        _ => {}
                    }
                }
                if model == 757 && !far {
                    animated_757.push(packets[0].payload.clone());
                }
                let closed_state = queue.state();
                assert_eq!(flush(&mut queue, resources), packets);
                assert_eq!(queue.state(), closed_state);
            }
        }
    }
    for callback in [
        PrimitiveCallback::TexturedTriangleNear,
        PrimitiveCallback::TexturedTriangleFar,
        PrimitiveCallback::TexturedQuadNear,
        PrimitiveCallback::TexturedQuadFar,
        PrimitiveCallback::EffectQuadNear,
        PrimitiveCallback::EffectQuadFar,
    ] {
        assert!(
            seen_callbacks.contains(&callback.native_address()),
            "{callback:?}"
        );
    }
    assert!(animated_757.windows(2).any(|pair| pair[0] != pair[1]));
}

/// Test composition only: model commands execute concretely; non-model scene
/// operations are recorded or applied to the fixture's explicit render state.
/// There is no pixel backend or successful production graphics stub here.
struct SceneFixture<'a> {
    scratch: crate::model_scratch::ProcessModelScratch,
    resources: RenderResources<'a>,
    tables: &'a PrimitiveTables,
    queue: DrawQueue<PrimitiveCommand<'a>>,
    environment: ModelEnvironment,
    model_slots: Vec<i16>,
    batches: Vec<Vec<Packet>>,
    star_materials: Vec<u16>,
    views: Vec<SceneView>,
    begins: usize,
    resets: usize,
}

impl FrontendSceneHost for SceneFixture<'_> {
    type Error = String;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        match command {
            SceneCommand::CoordinateShift(shift) => self.environment.coordinate_shift = shift,
            SceneCommand::FarColor(_) => {
                panic!("frontend fixture does not submit context far color")
            }
            SceneCommand::DirectionalLight { direction, rgb } => {
                self.environment.lighting.direction = direction;
                self.environment.lighting.directional_word =
                    u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 0]);
            }
            SceneCommand::AmbientLight(rgb) => {
                self.environment.lighting.ambient = rgb;
                let bytes = rgb.map(|value| value.clamp(0, 255) as u8);
                self.environment.lighting.ambient_word =
                    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
            }
            SceneCommand::Setup(setup) => {
                self.environment.projection = ModelProjection {
                    focal: setup.focal,
                    viewport: setup.viewport,
                    center: setup.center,
                    fade: setup.fade,
                };
                self.environment.fog_factor = setup.fade_factor;
            }
            SceneCommand::PublishFade(_) => {
                panic!("frontend fixture does not replace fade independently")
            }
            SceneCommand::BeginBatch => {
                self.queue.reset();
                self.begins += 1;
            }
            SceneCommand::FlushBatch => {
                let packets = flush(&mut self.queue, self.resources);
                assert_eq!(flush(&mut self.queue, self.resources), packets);
                self.batches.push(packets);
            }
            SceneCommand::StarView(view) => self.environment.view = view,
            SceneCommand::Star(quad) => {
                let resource = self
                    .resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Material,
                        argument: quad.color_argument,
                    })
                    .map_err(|error| error.to_string())?;
                assert!(matches!(resource.data, RenderResourceData::Material(_)));
                self.star_materials
                    .push(resource.source.registration.global_index);
            }
            SceneCommand::PublishView(view) => {
                self.environment.view = view;
                self.views.push(view);
            }
            SceneCommand::Model(submission) => {
                self.model_slots.push(submission.model_slot);
                let mut backend = ModelCommandBackend::new(
                    self.resources,
                    self.tables,
                    primitive_state(self.environment),
                    &mut self.queue,
                    Services {
                        submission,
                        prepares: 0,
                        selectors: Vec::new(),
                    },
                );
                let mut executor = ModelExecutor::new(
                    self.resources,
                    Arc::clone(&self.tables.sine_table),
                    ModelExecutionLimits {
                        instructions: 100_000,
                        invocation_depth: 16,
                    },
                );
                executor
                    .draw(
                        ModelDrawRequest {
                            scratch: &self.scratch,
                            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                            model_argument: i32::from(submission.model_slot),
                            context: ModelContext::new(submission.position, submission.basis),
                            environment: self.environment,
                            surface: crate::model_vertices::ModelSurfaceBinding::Default,
                        },
                        &mut backend,
                    )
                    .map_err(|error| format!("model {}: {error:?}", submission.model_slot))?;
            }
            SceneCommand::ResetAfterFirstModel => {
                assert_eq!(self.batches.len(), 2); // stars, then first model
                self.resets += 1;
            }
            SceneCommand::Footer(_) => {
                return Err("footer raster submission outside model fixture".into())
            }
        }
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        Err("footer outside fixture".into())
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Err("footer outside fixture".into())
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        Err("footer outside fixture".into())
    }
}

#[test]
fn authored_nine_model_scene_drives_the_concrete_backend_in_native_batch_order() {
    let resident = resident();
    let assets = assets();
    let tables = tables(&assets);
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let camera = &assets.camera_presets()[0];
    let camera_eye = camera.eye.map(|v| v * assets.position_scale());
    let camera_look_at = camera.target.map(|v| v * assets.position_scale());
    let mut frame = SceneFrame {
        projection: SceneProjection {
            focal: [160, 120],
            viewport: [640, 480],
            fade: [10_000, 200_000],
        },
        inherited_view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        camera_eye,
        camera_look_at,
        animation: AnimationClock::default(),
        age_millis: 2000,
        exit_millis: 0,
    };
    for clock in [0, 1024, 65535] {
        frame.animation = submission(0, clock).animation;
        let mut host = SceneFixture {
            scratch: crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            resources: RenderResources::new(&resident),
            tables: &tables,
            queue: DrawQueue::new(CAPACITY).unwrap(),
            environment: environment(false),
            model_slots: Vec::new(),
            batches: Vec::new(),
            star_materials: Vec::new(),
            views: Vec::new(),
            begins: 0,
            resets: 0,
        };
        let outcome = scene.draw_scene(frame, &mut host).unwrap();
        assert_eq!(outcome.submitted_models, 9);
        assert_eq!(
            host.model_slots,
            [744, 749, 754, 756, 745, 748, 747, 755, 757]
        );
        assert_eq!(host.batches.len(), 10);
        assert!(host.batches[0].is_empty()); // stars have their separate recorded boundary
        assert_eq!(host.begins, 11); // star batch, nine models, final footer batch
        assert_eq!(host.resets, 1);
        assert_eq!(host.views[0].eye, [0, 0, -70000]);
        assert!(host.views[1..].iter().all(|view| view.eye == camera_eye));
        assert_eq!(host.star_materials.len(), outcome.submitted_stars as usize);
        assert!(host
            .star_materials
            .iter()
            .all(|slot| (37..=48).contains(slot)));
        assert!(host.batches[1..].iter().any(|batch| !batch.is_empty()));
        assert_eq!(host.queue.state().command_count, 0); // final BeginBatch executes reset
        assert_eq!(host.queue.state().open_groups, 1);
    }
}

/// Close-range retail E2 records read mapped stale arena words outside their
/// declared shade slots. The UV-only callbacks omit that unknown RGB; actual
/// native clipping and queued packets must still execute, with no skipped face.
#[test]
fn close_range_menu_quads_preserve_mapped_unknown_scratch_and_emit_uv_commands() {
    let resident = resident();
    let assets = assets();
    let tables = tables(&assets);
    let resources = RenderResources::new(&resident);
    for (model, ordinary_prefix) in [(751_i16, 1), (755, 23)] {
        let submission = SceneModelSubmission {
            model_slot: model,
            position: [0, 0, 2000],
            basis: MenuBasis::INITIAL,
            animation: AnimationClock::default(),
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::Frontend,
        };
        let environment = environment(false);
        let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
        let request = ModelDrawRequest {
            scratch: &scratch,
            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
            model_argument: i32::from(model),
            context: ModelContext::new(submission.position, submission.basis),
            environment,
            surface: crate::model_vertices::ModelSurfaceBinding::Default,
        };
        let mut queue = DrawQueue::new(CAPACITY).unwrap();
        let mut backend = ModelCommandBackend::new(
            resources,
            &tables,
            primitive_state(environment),
            &mut queue,
            Services {
                submission,
                prepares: 0,
                selectors: Vec::new(),
            },
        );
        let mut executor = ModelExecutor::new(
            resources,
            Arc::clone(&tables.sine_table),
            ModelExecutionLimits {
                instructions: 100_000,
                invocation_depth: 16,
            },
        );
        executor.draw(request, &mut backend).unwrap();
        assert!(!backend.stopped(), "model {model}");
        assert!(!executor.stopped(), "model {model}");
        assert!(
            backend.outcome().commands > ordinary_prefix,
            "model {model}"
        );
        let packets = flush(&mut *backend.queue, resources);
        assert_eq!(packets.len(), backend.outcome().commands, "model {model}");
        assert!(
            packets.iter().any(|packet| matches!(
                packet.callback,
                PrimitiveCallback::TexturedUvTriangleNear | PrimitiveCallback::TexturedUvQuadNear
            )),
            "model {model}"
        );
        assert_eq!(
            flush(&mut *backend.queue, resources),
            packets,
            "model {model}"
        );
    }
}
