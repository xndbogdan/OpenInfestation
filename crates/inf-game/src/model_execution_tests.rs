use super::*;
use crate::{
    frontend_assets::FrontendAssets, menu::MenuBasis, Installation, ResidencyRequest,
    ResidentRuntime,
};
use inf_formats::{decode_model_program, ModelFlags};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Publish(ModelContext),
    Select(ModelDispatch),
    Primitive(u16, Option<u16>, Option<u16>),
    Callback(u32),
    Begin(i32, bool),
    End,
    RequiredCustom(i32, Vec<u16>),
}

#[derive(Default)]
struct Host {
    events: Vec<Event>,
    transforms: Vec<ModelTransform>,
    primitive_transforms: Vec<ModelTransform>,
    fail_primitive: bool,
    mutate_primitive: bool,
    fail_depth_group: bool,
    scratch_bases: Vec<(u16, usize, crate::model_lighting::ModelShade)>,
    custom_policy: Option<crate::model_vertices::ModelCustomVertexPolicy>,
    surface_worlds: Vec<[Option<[i32; 3]>; 2]>,
    /// Leave register 3 as the program left it instead of writing 7.
    keep_registers: bool,
    /// Getter answers by selector; others answer 0.
    getters: Vec<(u32, u16)>,
    /// Each primitive's raw command, in program order.
    commands: Vec<u16>,
    /// The root's 22 registers at the first packed-value callback.
    first_callback_registers: Option<Vec<Option<u16>>>,
}

impl ModelValueCallback for Host {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.events.push(Event::Callback(selector));
        if self.first_callback_registers.is_none() {
            self.first_callback_registers = Some(
                (0..22)
                    .map(|index| values.register(index).unwrap())
                    .collect(),
            );
        }
        if !self.keep_registers {
            values.write_register(3, 7).unwrap();
        }
        Ok(self
            .getters
            .iter()
            .find(|(getter, _)| *getter == selector)
            .map_or(0, |(_, value)| *value))
    }
}

impl ModelExecutionHost for Host {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::Publish(*context));
        self.transforms.push(transform);
        Ok(())
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.events.push(Event::Select(dispatch));
        Ok(())
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.primitive_transforms.push(request.transform);
        self.surface_worlds.push([
            request.vertices.world(4).ok(),
            request.vertices.world(5).ok(),
        ]);
        if let Some((_, base, expected_pool_start)) = self
            .scratch_bases
            .iter()
            .find(|(model, _, _)| *model == request.model.identity.global_index)
        {
            let first = -(*base as i32 / 4);
            assert_eq!(
                request.shades.scratch_shade(first).unwrap(),
                *expected_pool_start
            );
            // Below the pool, 0x148 bytes of the software rasterizer's
            // globals read zero; the next DWORD down is outside.
            assert_eq!(
                request.shades.scratch_shade(first - 82).unwrap(),
                crate::model_lighting::ModelShade::from_word(0)
            );
            assert!(matches!(
                request.shades.scratch_shade(first - 83),
                Err(ModelLightingError::Scratch(
                    crate::model_scratch::ModelScratchError::MappedShade { slot, offset }
                )) if slot == first - 83 && offset == -332
            ));
        }
        self.commands.push(request.instruction.raw_command);
        self.events.push(Event::Primitive(
            request.model.identity.global_index,
            values.register(1).unwrap(),
            values.register(4).unwrap(),
        ));
        if self.mutate_primitive {
            values.write_register(1, 99).unwrap();
        }
        if self.fail_primitive {
            Err("primitive backend failed")
        } else {
            Ok(())
        }
    }
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        self.events.push(Event::Begin(depth, sorting));
        if self.fail_depth_group {
            Err("depth group backend failed")
        } else {
            Ok(())
        }
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::End);
        Ok(())
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.events.push(Event::RequiredCustom(
            request.selector,
            request.extras.to_vec(),
        ));
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        self.custom_policy
            .unwrap_or(crate::model_vertices::ModelCustomVertexPolicy::Unowned)
    }
}

#[test]
fn sibling_model_allocations_reuse_the_restored_parent_scratch_cursor() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = executor(&runtime, 10000);
    let (model, source) = custom_model(
        &runtime,
        &[
            0x0e, 0, 751, 8, 0, 0x0e, 0, 751, 8, 0, 0x1b, 0, 1812, 8192, 0, 0,
        ],
    );
    let resources = RenderResources::new(&runtime);
    let child = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 751,
        })
        .unwrap();
    let RenderResourceData::Model(child) = child.data else {
        unreachable!()
    };
    let parent_base = usize::from(model.header.vertex_slot_count) * 24;
    let parent_end = parent_base + usize::from(model.header.shade_slot_count) * 4;
    let child_base = parent_end + usize::from(child.header.vertex_slot_count) * 24;
    // Both signed first references address actual pool byte zero: parent
    // world slot0's X DWORD. The immediately preceding reference is byte -4.
    let expected_pool_start = crate::model_lighting::ModelShade::from_word(
        i32::from(model.vertices[0].x).wrapping_add(request(757, &scratch).context.position[0])
            as u32,
    );
    let mut host = Host {
        scratch_bases: vec![
            (757, parent_base, expected_pool_start),
            (751, child_base, expected_pool_start),
        ],
        ..Host::default()
    };
    let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
    let mut run = Run {
        executor: &mut executor,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scratch.begin_draw().unwrap(),
        root_exit: None,
    };
    run.invoke(
        InvocationSeed {
            resource: ModelResource {
                model: &model,
                source,
            },
            context: request(757, &scratch).context,
            values,
            state_words: &[],
            parent_vertices: None,
            depth: 1,
            publications: 1,
            exact_origin: None,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(run.outcome.invocations, 3);
    assert_eq!(run.scratch.cursor().unwrap(), 0);
    assert_eq!(
        host.events
            .iter()
            .filter(|event| matches!(event, Event::Primitive(751, ..)))
            .count(),
        8
    );
}

fn resident() -> ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

fn executor(runtime: &ResidentRuntime, instructions: u64) -> ModelExecutor<'_> {
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    ModelExecutor::new(
        RenderResources::new(runtime),
        Arc::new(assets.sine_table().values),
        ModelExecutionLimits {
            instructions,
            invocation_depth: 8,
        },
    )
}

fn environment() -> ModelEnvironment {
    ModelEnvironment {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [100000, 200000],
        },
        coordinate_shift: 0,
        fog_factor: 167,
        lighting: ModelLighting {
            direction: [0.0, 0.0, 1.0],
            directional_word: 0x00aa_d2ff,
            ambient: [10, 6, 5],
            ambient_word: 0x0005_060a,
        },
    }
}

fn request<'a>(argument: i32, scratch: &'a ProcessModelScratch) -> ModelDrawRequest<'a> {
    ModelDrawRequest {
        scratch,
        root_mode: ModelRootMode::CommonVisual,
        model_argument: argument,
        context: ModelContext::new([0, 0, 10000], MenuBasis::INITIAL),
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
    }
}

fn custom_model<'a>(runtime: &'a ResidentRuntime, words: &[u16]) -> (ModelEntry, RenderSource<'a>) {
    let resource = RenderResources::new(runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 757,
        })
        .unwrap();
    let RenderResourceData::Model(model) = resource.data else {
        unreachable!()
    };
    let mut model = model.clone();
    model.program = decode_model_program(words).unwrap();
    // This synthetic program needs world slot0; native757 only projects it.
    model.header.vertex_ranges[0] = 0;
    (model, resource.source)
}

#[test]
fn shared_surface_binding_selects_both_real_callbacks_independently_of_residency() {
    let runtime = resident();
    let terrain = crate::terrain_runtime::native_probe_fixture_terrain(&serde_json::json!({
        "spacing":256, "height_shift":5, "plane_shift":8, "plane_base":0x10000,
        "xz":[0,0], "heights":[0,0,0,0], "alternate":false
    }));
    for terrain_mode in [false, true] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (mut model, source) = custom_model(
            &runtime,
            &[0x012b, 4, 0, 0x012c, 5, 0, 0x1b, 0, 1812, 8192, 0, 0],
        );
        model.header.vertex_slot_count = model.header.vertex_slot_count.max(6);
        let surface = if terrain_mode {
            ModelSurfaceBinding::Terrain {
                terrain: Some(&terrain),
                world_flags: 0,
                wave_clock: 0x1234_5678,
                renderer_xyz: [11, 22, 33],
            }
        } else {
            ModelSurfaceBinding::Default
        };
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let context = request(757, &scratch).context;
        let vertex = model.vertices[0];
        let source_world = [vertex.x, vertex.y, vertex.z].map(i32::from);
        let source_world =
            std::array::from_fn(|index| source_world[index].wrapping_add(context.position[index]));
        let mut host = Host::default();
        let values = ModelValues::new_invocation(Arc::clone(&run.executor.sine_table));
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let expected_ground = if terrain_mode {
            [source_world[0], 0, source_world[2]]
        } else {
            [source_world[0], -300, source_world[2]]
        };
        let expected_project = if terrain_mode {
            terrain
                .project_44ffa0(
                    source_world,
                    &crate::terrain_runtime::Project44FFA0Inputs {
                        world_flags: 0,
                        wave_clock: 0x1234_5678,
                        renderer_xyz: [11, 22, 33],
                        table: None,
                    },
                )
                .unwrap()
        } else {
            let adjustment = source_world[1].wrapping_add(300) / 2;
            [
                source_world[0].wrapping_add(adjustment),
                -300,
                source_world[2].wrapping_add(adjustment),
            ]
        };
        assert!(!host.surface_worlds.is_empty());
        for worlds in host.surface_worlds {
            assert_eq!(worlds, [Some(expected_ground), Some(expected_project)]);
        }
    }
}

#[test]
fn root_negative_slot_skips_publication_and_errors_stop_reentry() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = executor(&runtime, 10000);
    let mut host = Host::default();
    assert_eq!(
        executor
            .draw(request(0x1234_ffff, &scratch), &mut host)
            .unwrap(),
        ModelExecutionOutcome::default()
    );
    assert!(host.events.is_empty());
    host.fail_primitive = true;
    assert!(matches!(
        executor.draw(request(757, &scratch), &mut host),
        Err(ModelExecutionError::Host("primitive backend failed"))
    ));
    assert!(executor.stopped());
    let length = host.events.len();
    assert!(matches!(
        executor.draw(request(757, &scratch), &mut host),
        Err(ModelExecutionError::Stopped)
    ));
    assert_eq!(host.events.len(), length);
}

#[test]
fn saved_child_basis_world_origin_and_four_word_inheritance_compose() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = executor(&runtime, 10000);
    let (model, source) = custom_model(
        &runtime,
        &[
            0xf0, 1, 1, 2, 0xf0, 4, 1, 2, 0x03, 0x20, 0, 0, 0x0e, 6, 751, 8, 0, 0x1b, 0, 1812,
            8192, 0, 0,
        ],
    );
    let context = request(757, &scratch).context;
    let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
    let mut run = Run {
        executor: &mut executor,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scratch.begin_draw().unwrap(),
        root_exit: None,
    };
    let mut host = Host {
        mutate_primitive: true,
        ..Host::default()
    };
    run.invoke(
        InvocationSeed {
            resource: ModelResource {
                model: &model,
                source,
            },
            context,
            values,
            state_words: &[],
            parent_vertices: None,
            depth: 1,
            publications: 1,
            exact_origin: None,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(run.outcome.invocations, 2);
    let primitives: Vec<_> = host
        .events
        .iter()
        .filter(|event| matches!(event, Event::Primitive(..)))
        .cloned()
        .collect();
    assert_eq!(primitives.len(), 5);
    assert_eq!(primitives[0], Event::Primitive(751, Some(3), None));
    assert_eq!(primitives[1], Event::Primitive(751, Some(99), None));
    assert_eq!(primitives[4], Event::Primitive(757, Some(3), Some(3)));
    let publications: Vec<_> = host
        .events
        .iter()
        .filter_map(|event| {
            if let Event::Publish(context) = event {
                Some(*context)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(publications.len(), 4);
    assert_eq!(publications[1], publications[2]);
    assert_eq!(publications[1].position, context.position); // raw origin0 already became world10000
    assert_eq!(publications[1].basis.words[1], (-1.0f32).to_bits());
    assert_eq!(*publications[1].saved_basis(), MenuBasis::INITIAL);
    assert_eq!(publications[3].basis, context.basis);
    assert_eq!(
        host.events[host.events.len() - 3],
        Event::Select(ModelDispatch::Near)
    );
    assert!(matches!(
        host.events[host.events.len() - 2],
        Event::Publish(_)
    ));
}

#[test]
fn invalid_child_fails_before_origin_or_transform_and_failed_child_does_not_restore() {
    let runtime = resident();
    for (child, origin, fail) in [(65535, 65535, false), (751, 0, true)] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(
            &runtime,
            &[0xee, 0, 63136, 65535, 0x0e, 0, child, 8, origin, 0xef, 0],
        );
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host {
            fail_primitive: fail,
            ..Host::default()
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        );
        if fail {
            assert!(matches!(result, Err(ModelExecutionError::Host(_))));
        } else {
            assert!(matches!(result, Err(ModelExecutionError::InvalidChild(-1))));
        }
        assert!(host.events.contains(&Event::Begin(7600, false)));
        assert!(!host.events.contains(&Event::End));
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Publish(_)))
                .count(),
            if fail { 3 } else { 1 }
        );
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Select(_)))
                .count(),
            if fail { 2 } else { 1 }
        );
    }
}

#[test]
fn outer_and_inner_child_culls_have_distinct_parent_callback_restoration() {
    let runtime = resident();
    for (position, fade, expected_selects) in [
        ([0, 0, -5000], [100000, 200000], 1),
        ([0, 0, 10000], [100, 200], 2),
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (mut model, source) = custom_model(&runtime, &[0x0e, 0, 751, 8, 0, 0]);
        // Parent admitted and force-near; child keeps its actual4000 extent/flags.
        model.header.visibility_extent = 20000;
        model.header.flags = ModelFlags::from_raw(0x20);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut environment = environment();
        environment.projection.fade = fade;
        let mut run = Run {
            executor: &mut executor,
            environment,
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: ModelContext::new(position, MenuBasis::INITIAL),
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(run.outcome.culled, 1);
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Publish(_)))
                .count(),
            4
        );
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Select(_)))
                .count(),
            expected_selects
        );
        assert!(matches!(host.events.last(), Some(Event::Publish(_))));
    }
}

/// D0, D4, D8 and E0 take the primitive arm with the other faces; the
/// arm's catch-all would return `Unsupported` for a face it lacks.
#[test]
fn every_retail_face_opcode_reaches_the_primitive_host() {
    let runtime = resident();
    for program in [
        &[0xd0, 0, 0, 1, 2, 0, 1, 2, 0][..],
        &[0xd4, 0, 0, 1, 2, 3, 0, 1, 2, 3, 0],
        &[0xd8, 0, 0, 0, 1, 2, 0],
        &[0xe0, 0, 0, 0, 1, 2, 3, 0],
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, program);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(
            host.events
                .iter()
                .filter(|event| matches!(event, Event::Primitive(..)))
                .count(),
            1,
            "{:#04x}",
            program[0]
        );
        assert_eq!(run.outcome.primitive_calls, 1);
    }
}

#[test]
fn branch_targets_require_decoded_boundaries_and_loops_obey_explicit_budget() {
    let program = decode_model_program(&[0x16, 3, 0, 1, 0xf0, 0, 0, 0, 0]).unwrap();
    let branch = &program.instructions[0];
    assert_eq!(branch_target::<()>(&program, branch, 3).unwrap(), 1);
    assert_eq!(branch_target::<()>(&program, branch, 7).unwrap(), 2);
    assert_eq!(branch_target::<()>(&program, branch, -1).unwrap(), 0);
    for offset in [0, 1, 2, 4, 8, -2] {
        assert!(matches!(
            branch_target::<()>(&program, branch, offset),
            Err(ModelExecutionError::Branch { .. })
        ));
    }
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut executor = executor(&runtime, 5);
    let (model, source) = custom_model(&runtime, &[0x16, 65535, 0, 1, 0]);
    let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
    let mut run = Run {
        executor: &mut executor,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scratch.begin_draw().unwrap(),
        root_exit: None,
    };
    assert!(matches!(
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut Host::default()
        ),
        Err(ModelExecutionError::Limit)
    ));
    assert_eq!(run.outcome.instructions, 5);
}

#[test]
fn identity_save_and_restore_republish_without_reinitializing_or_reselecting() {
    let runtime = resident();
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, &[0x0f, 0x10, 0x1b, 0, 1812, 8192, 0, 0]);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut environment = environment();
        if dispatch == ModelDispatch::Far {
            environment.projection.fade = [0, 200000];
        }
        let mut context = request(757, &scratch).context;
        context.rotate_saved_basis(1, 16384, &executor.basis_math);
        // Counter1 must survive0F and then be copied by10. The initial
        // current counter0 and saved quarter-turn are independently owned.
        assert_eq!(context.saved_basis().words[0], 1);
        let mut run = Run {
            executor: &mut executor,
            environment,
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let publications: Vec<_> = host
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Publish(context) = event {
                    Some(context)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(publications.len(), 2);
        assert_eq!(publications[0].basis.words[0], 0);
        assert_eq!(publications[1].basis.words[0], 1);
        assert_eq!(
            &publications[1].basis.words[1..],
            &MenuBasis::INITIAL.words[1..]
        );
        assert_eq!(publications[1].position, context.position);
        assert_eq!(host.transforms[1].view_basis.words[0], 2);
        assert_eq!(host.primitive_transforms, [host.transforms[1]]);
        assert_eq!(
            host.events
                .iter()
                .filter(|event| matches!(event, Event::Select(_)))
                .count(),
            1
        );
        assert!(host.events.contains(&Event::Select(dispatch)));
        assert_eq!(run.outcome.primitive_calls, 1);
    }
}

#[test]
fn opcode_1c_rotates_without_copy_or_zero_skip() {
    // 004A0DA0: no oriented copy and no raw-zero skip. The raw first
    // word selects the axis (signed 1/2, else fallback) and the
    // packed second word always resolves first — unlike 03, a raw
    // zero angle still rotates (resolving to immediate zero, so no
    // callback fires). The orientation-6 child publishes the saved
    // basis, whose counter proves each rotation ran.
    let runtime = resident();
    for axis in [1u16, 2, 3] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, &[0x1c, axis, 0, 0x0e, 6, 751, 8, 0, 0]);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let child = host
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Publish(context) = event {
                    Some(context)
                } else {
                    None
                }
            })
            .nth(1)
            .unwrap();
        assert_eq!(child.basis.words[0], 1);
        // Packed word 0 is an immediate, so no callback fires — but the
        // rotation above still ran, which is exactly what separates 1C
        // from 03's raw-zero skip.
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Callback(_)))
                .count(),
            0
        );
    }
}

#[test]
fn opcode_2a_advances_past_clear_modes_and_projects_the_root_origin() {
    // 004ED350: with no mode bits in 8..11 both branches skip every
    // effect, so a raw 0x002A record advances even top-level. A root
    // without a parent projects its own origin into the destination;
    // a destination past model 757's two slots stops the invocation.
    let runtime = resident();
    for (words, supported) in [
        (&[0x002au16, 54, 1, 0] as &[_], true),
        (&[0x022au16, 0, 1, 0] as &[_], true),
        (&[0x022au16, 54, 1, 0] as &[_], false),
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, words);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut Host::default(),
        );
        assert_eq!(result.is_ok(), supported, "words={words:04x?} {result:?}");
    }
}

/// Both griffin roots, the two-headed `demont3` 448 and `gryfin` 556, draw
/// wing 456 once per side; 456 reaches grandchild 457, whose F4 reads
/// register 9 before anything writes it. Unknown stops in the first wing,
/// before its tip 458 and the other wing. The labeled stack-history policy
/// seeds 457 in every world that loads it, not only the intro's.
#[test]
fn griffin_wing_model_457_register_9_needs_the_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let tips = |host: &Host| {
        host.events
            .iter()
            .filter(|event| matches!(event, Event::Primitive(458, ..)))
            .count()
    };
    // The intro world, Oolbryte, world 3 (448 alone) and world 25, which
    // loads both roots' copies of 449..458.
    for (levels, model) in [
        (&[47u8, 48, 32][..], 448),
        (&[48, 30][..], 556),
        (&[48, 13][..], 448),
        (&[48, 35][..], 448),
        (&[48, 35][..], 556),
    ] {
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        for &level in levels {
            runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        }
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut host = Host::default();
        let error = executor(&runtime, 100_000)
            .draw(request(model, &scratch), &mut host)
            .unwrap_err();
        assert!(
            matches!(
                error,
                ModelExecutionError::Value(ModelValueError::Fault(
                    ModelValueFault::UninitializedRegister { index: 9 }
                ))
            ),
            "{levels:?} {model}: {error:?}"
        );
        assert_eq!(tips(&host), 0, "{levels:?} {model}");
        for policy in [
            ModelStackRegisterPolicy::WingModel457StackHistoryApproximation,
            ModelStackRegisterPolicy::ObjectStackHistoryApproximations,
        ] {
            let scratch = ProcessModelScratch::isolated_unknown_fixture();
            let mut host = Host::default();
            let mut seeded = executor(&runtime, 100_000);
            seeded.set_stack_register_policy(policy);
            let outcome = seeded.draw(request(model, &scratch), &mut host).unwrap();
            assert_eq!(outcome.stack_register_seeds, 2, "{levels:?} {model}");
            // Each wing tip queues a fan and an edge.
            assert_eq!(tips(&host), 4, "{levels:?} {model}");
        }
    }
}

/// The vehicle draws the loading model's H8 117. As the truck (getter 1 is
/// the mode table's 2) with a transition in progress (getter 2 not FFFF),
/// child 131 reads the root's unwritten register 3. Unknown stops there;
/// the object draw's policy seeds it as the loading screen does.
#[test]
fn vehicle_model_117_register_3_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    // The getters as the loading screen answers them: selector 2, pulse 0
    // and getter 6 one; register 3 stays as the program leaves it.
    let truck = || Host {
        keep_registers: true,
        getters: vec![(1, 2), (2, 0), (6, 1)],
        custom_policy: Some(crate::model_vertices::ModelCustomVertexPolicy::UnsignedRowSource),
        ..Host::default()
    };
    let draw = |policy| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        executor.draw(request(117, &scratch), &mut truck())
    };
    for policy in [
        ModelStackRegisterPolicy::Unknown,
        ModelStackRegisterPolicy::WingModel457StackHistoryApproximation,
    ] {
        let error = draw(policy).unwrap_err();
        assert!(
            matches!(
                error,
                ModelExecutionError::Value(ModelValueError::Fault(
                    ModelValueFault::UninitializedRegister { index: 3 }
                ))
            ),
            "{policy:?}: {error:?}"
        );
    }
    for policy in [
        ModelStackRegisterPolicy::LoadingModel117StackHistoryApproximation,
        ModelStackRegisterPolicy::ObjectStackHistoryApproximations,
    ] {
        let outcome = draw(policy).unwrap();
        assert_eq!(outcome.stack_register_seeds, 1, "{policy:?}");
    }
}

/// As the settled skimmer (getter 1 is the mode table's 5, getter 2 is
/// FFFF), the vehicle's H8 117 enters child 127, which branches on its
/// register 15 before writing it. Only the object draw's policy seeds it,
/// beside the root's register 3, and the shop preview's, alone.
#[test]
fn skimmer_model_127_register_15_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let skimmer = || Host {
        keep_registers: true,
        getters: vec![(1, 5), (2, 0xffff)],
        custom_policy: Some(crate::model_vertices::ModelCustomVertexPolicy::UnsignedRowSource),
        ..Host::default()
    };
    let draw = |policy| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        executor.draw(request(117, &scratch), &mut skimmer())
    };
    for policy in [
        ModelStackRegisterPolicy::Unknown,
        ModelStackRegisterPolicy::WingModel457StackHistoryApproximation,
        ModelStackRegisterPolicy::LoadingModel117StackHistoryApproximation,
    ] {
        let error = draw(policy).unwrap_err();
        assert!(
            matches!(
                error,
                ModelExecutionError::Value(ModelValueError::Fault(
                    ModelValueFault::UninitializedRegister { index: 15 }
                ))
            ),
            "{policy:?}: {error:?}"
        );
    }
    let outcome = draw(ModelStackRegisterPolicy::ObjectStackHistoryApproximations).unwrap();
    assert_eq!(outcome.stack_register_seeds, 2);
    let outcome =
        draw(ModelStackRegisterPolicy::ShopPreviewModel127StackHistoryApproximation).unwrap();
    assert_eq!(outcome.stack_register_seeds, 1);
}

/// While the vehicle turns into the skimmer (getter 1 is the mode table's
/// 5, getter 2 the progress, not FFFF), its H8 117 enters child 126 instead,
/// which branches twice on its register 15 before writing it, and 126's
/// child 141 once on its own, which it never writes. Only the object draw's
/// policy seeds them, beside the root's register 3, and the morph then
/// draws to the end.
#[test]
fn skimmer_morph_model_126_register_15_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let draw = |policy| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        let mut host = Host {
            keep_registers: true,
            getters: vec![(1, 5), (2, 0x4000)],
            custom_policy: Some(crate::model_vertices::ModelCustomVertexPolicy::UnsignedRowSource),
            ..Host::default()
        };
        let result = executor.draw(request(117, &scratch), &mut host);
        (result, host.commands.len())
    };
    let mut prefix = 0;
    for policy in [
        ModelStackRegisterPolicy::Unknown,
        ModelStackRegisterPolicy::WingModel457StackHistoryApproximation,
        ModelStackRegisterPolicy::LoadingModel117StackHistoryApproximation,
    ] {
        let (result, primitives) = draw(policy);
        assert!(
            matches!(
                result,
                Err(ModelExecutionError::Value(ModelValueError::Fault(
                    ModelValueFault::UninitializedRegister { index: 15 }
                )))
            ),
            "{policy:?}: {result:?}"
        );
        prefix = primitives;
    }
    assert_eq!(prefix, 39);
    let (result, primitives) = draw(ModelStackRegisterPolicy::ObjectStackHistoryApproximations);
    assert_eq!(result.unwrap().stack_register_seeds, 3);
    assert_eq!(primitives, 115);
}

/// An unarmed laser mine (getter 1, the mine's +138, is 0) jumps over the
/// pair that writes its H8 85's register 3, then branches on it before its
/// third fin and its base. Unknown stops there and loses both; the object
/// draw's policy starts it at 0xCF2, which skips the fin's three lights
/// (01E2/03E2) and draws the rest down to the base (05E6). An armed mine
/// writes the register itself first.
#[test]
fn laser_mine_model_85_register_3_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let draw = |policy, armed| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        let mut host = Host {
            keep_registers: true,
            getters: vec![(1, armed)],
            ..Host::default()
        };
        let result = executor.draw(request(85, &scratch), &mut host);
        (result, host.commands)
    };
    let lights = |commands: &[u16]| {
        commands
            .iter()
            .filter(|&&command| command as u8 == 0xe2 && command >> 8 != 0)
            .count()
    };
    let (result, commands) = draw(ModelStackRegisterPolicy::Unknown, 0);
    assert!(
        matches!(
            result,
            Err(ModelExecutionError::Value(ModelValueError::Fault(
                ModelValueFault::UninitializedRegister { index: 3 }
            )))
        ),
        "{result:?}"
    );
    assert_eq!(commands.len(), 23);
    assert!(!commands.contains(&0x5e6));
    let (result, commands) = draw(
        ModelStackRegisterPolicy::ObjectStackHistoryApproximations,
        0,
    );
    assert_eq!(result.unwrap().stack_register_seeds, 1);
    assert_eq!(commands.len(), 29);
    assert_eq!(commands.last(), Some(&0x5e6));
    assert_eq!(lights(&commands), 0);
    let (result, commands) = draw(ModelStackRegisterPolicy::Unknown, 1);
    assert_eq!(result.unwrap().stack_register_seeds, 0);
    assert_eq!(commands.len(), 27);
    assert_eq!(commands.last(), Some(&0x5e6));
}

/// The proximity mine (resource 117) draws H8 84, whose model value 1 is the
/// mine's armed byte +138. Unarmed, as it flies, its `0116 [40, 81, 0]`
/// jumps over the pair that writes register 3 and its `0016 [8, C3, 1]`
/// then reads it. Unknown stops there after 10 primitives, losing the
/// body; the object draw's policy starts it at retail's 0x2861 (the lab's
/// `proximity-mine-throw.json`), which jumps past the lights to the body.
/// Armed, it writes the register first and needs no seed.
#[test]
fn proximity_mine_model_84_register_3_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let draw = |policy, armed| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        let mut host = Host {
            keep_registers: true,
            getters: vec![(1, armed)],
            ..Host::default()
        };
        let result = executor.draw(request(84, &scratch), &mut host);
        (result, host.commands)
    };
    let (result, commands) = draw(ModelStackRegisterPolicy::Unknown, 0);
    assert!(
        matches!(
            result,
            Err(ModelExecutionError::Value(ModelValueError::Fault(
                ModelValueFault::UninitializedRegister { index: 3 }
            )))
        ),
        "{result:?}"
    );
    assert_eq!(commands.len(), 10);
    let (result, seeded) = draw(
        ModelStackRegisterPolicy::ObjectStackHistoryApproximations,
        0,
    );
    assert_eq!(result.unwrap().stack_register_seeds, 1);
    let (result, armed) = draw(ModelStackRegisterPolicy::Unknown, 1);
    assert_eq!(result.unwrap().stack_register_seeds, 0);
    // Seeded, the flying mine draws its first ten primitives, one face the
    // armed path jumps over, then the armed body's twelve quads; neither
    // lights the 01E2 sprites.
    assert_eq!((seeded.len(), armed.len()), (23, 22));
    assert_eq!(seeded[..10], armed[..10]);
    assert_eq!(seeded[11..], armed[10..]);
    assert!(!seeded.contains(&0x1e2) && !armed.contains(&0x1e2));
}

/// Thumper H8 46's first three `0A26` lerps read register 1 before its
/// `F5 [1, 80, 400A]` writes it. Unknown stops at the first. The briefing
/// preview after the map frame starts registers 0..3 and 10..21 with that
/// frame's context bases, register 1 at 0, and the model runs to its end;
/// registers 4..9 stay unknown. An object draw (Terra Nova's generator)
/// starts register 1 at the gameplay residue 0x1A and also runs to its end.
#[test]
fn thumper_model_46_register_1_comes_from_the_map_frames_bases() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let draw = |policy| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        let mut host = Host {
            keep_registers: true,
            ..Host::default()
        };
        let result = executor.draw(request(46, &scratch), &mut host);
        (result, host)
    };
    let (result, _) = draw(ModelStackRegisterPolicy::Unknown);
    assert!(
        matches!(
            result,
            Err(ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Value(ModelValueError::Fault(
                    ModelValueFault::UninitializedRegister { index: 1 }
                ))
            ))
        ),
        "{result:?}"
    );
    let (result, host) = draw(ModelStackRegisterPolicy::BriefingPreviewMapFrameBases);
    assert_eq!(result.unwrap().stack_register_seeds, 1);
    assert!(!host.commands.is_empty());
    let registers = host.first_callback_registers.unwrap();
    assert_eq!(registers[4..10], [None; 6]);
    for (index, value) in BRIEFING_PREVIEW_MAP_FRAME_REGISTERS {
        if index != 1 {
            assert_eq!(registers[index as usize], Some(value), "register {index}");
        }
    }
    let (result, host) = draw(ModelStackRegisterPolicy::ObjectStackHistoryApproximations);
    assert_eq!(result.unwrap().stack_register_seeds, 1);
    assert!(!host.commands.is_empty());

    // Terrain furniture (Helniazine's thumper): the root starts over the block
    // the previous furniture root left, and leaves its own for the next.
    let carried = |block| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(ModelStackRegisterPolicy::FurnitureRootCarry);
        executor.set_root_registers(block);
        let mut host = Host {
            keep_registers: true,
            ..Host::default()
        };
        let result = executor.draw(request(46, &scratch), &mut host);
        (
            result.map(|outcome| outcome.stack_register_seeds),
            executor.root_registers(),
            host,
        )
    };
    let (result, block, _) = carried(ModelRegisterBlock::default());
    assert!(result.is_err());
    assert_eq!(block, ModelRegisterBlock::default());
    let mut previous = ModelValues::new_invocation(Arc::clone(&executor(&runtime, 1).sine_table));
    previous.write_register(1, 3).unwrap();
    let (result, block, host) = carried(previous.register_block());
    assert_eq!(result.unwrap(), 0);
    assert!(!host.commands.is_empty());
    assert_eq!(host.first_callback_registers.unwrap()[1], Some(3));
    assert_ne!(block, previous.register_block());
    let (result, _, _) = carried(block);
    assert_eq!(result.unwrap(), 0);
}

/// The blast wave H8 112 (`exwave4`) runs only its child 115 (`exwave7`),
/// eight of whose lerps read register 1, which neither writes; the child
/// starts with the root's registers 0..3. Unknown stops in the child, so the
/// wave never draws; the object draw's policy starts the root's register 1
/// at retail's 0x1A (the lab's `foldrak-blast-wave.json`) and both run to
/// their ends.
#[test]
fn explosion_wave_model_112_register_1_needs_the_object_stack_history_approximation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let draw = |policy| {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 100_000);
        executor.set_stack_register_policy(policy);
        let mut host = Host {
            keep_registers: true,
            getters: vec![(1, 0x8000)],
            ..Host::default()
        };
        let result = executor.draw(request(112, &scratch), &mut host);
        (result, host.commands)
    };
    let (result, commands) = draw(ModelStackRegisterPolicy::Unknown);
    assert!(
        format!("{result:?}").contains("UninitializedRegister { index: 1 }"),
        "{result:?}"
    );
    assert!(commands.is_empty());
    let (result, commands) = draw(ModelStackRegisterPolicy::ObjectStackHistoryApproximations);
    assert_eq!(result.unwrap().stack_register_seeds, 1);
    assert!(!commands.is_empty());
}

#[test]
fn opcode_2d_advances_nibble0_and_skips_nibble2() {
    // 004EC670: nibble 0 runs zero loop passes (exact advance);
    // nibble 2 is the labeled skip-advance-2d-nibble2 (advance,
    // copies skipped); any other nibble stays fail-closed.
    // Argument counts follow 2 + (raw >> 12).
    let runtime = resident();
    for (words, supported) in [
        (&[0x002du16, 54, 1, 0] as &[_], true),
        (&[0x222du16, 54, 1, 44, 0, 0] as &[_], true),
        (&[0x232du16, 54, 1, 44, 0, 0] as &[_], false),
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, words);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut Host::default(),
        );
        assert_eq!(result.is_ok(), supported, "words={words:04x?}");
    }
}

#[test]
fn furniture_2d_never_inherits_world_skip_or_an_inert_default_callback() {
    let runtime = resident();
    for mode in [0, 2] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let words = [0x202d | (mode << 8), 0, 7, 0, 1, 0];
        let (model, source) = custom_model(&runtime, &words);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut Host {
                custom_policy: Some(crate::model_vertices::ModelCustomVertexPolicy::Furniture),
                ..Host::default()
            },
        );
        if mode == 0 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(ModelExecutionError::Vertex(ModelVertexError::Custom(
                    ModelCustomVertexError::Fault(ModelVertexFault::UnownedCustomVertex)
                )))
            ));
        }
    }
}

#[test]
fn raw_zero_angle_skips_callback_but_encoded_zero_rotates_saved_basis() {
    let runtime = resident();
    for (angle, counter, callbacks) in [(0, 0, 0), (0x80, 1, 1)] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, &[0x03, 0, 0, angle, 0x0e, 6, 751, 8, 0, 0]);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let child = host
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Publish(context) = event {
                    Some(context)
                } else {
                    None
                }
            })
            .nth(1)
            .unwrap();
        assert_eq!(child.basis.words[0], counter);
        assert_eq!(
            host.events
                .iter()
                .filter(|e| matches!(e, Event::Callback(_)))
                .count(),
            callbacks
        );
    }
}

#[test]
fn ea_and_15_open_depth_groups_on_slot_depth_like_ee_without_bias() {
    // 0xEA/0x15 (004A0810/004A08F0) mirror the 0xEE shape with a single
    // slot word and no low/high bias: same depth as 0xEE with zero
    // bias, sorting false for EA and true for 0x15.
    // 0xE8/0xE9 (004A0770/004A07C0) over the one-slot list [0] key the same
    // unsorted depth; over an empty list, their start values.
    let mut reference = None;
    for (words, sorting, fixed) in [
        (&[0xea, 0, 0][..], false, None),
        (&[0x15, 0, 0][..], true, None),
        (&[0xee, 0, 0, 0, 0][..], false, None),
        (&[0xe8, 0, 0xffff, 0][..], false, None),
        (&[0xe9, 0, 0xffff, 0][..], false, None),
        (&[0xe8, 0xffff, 0][..], false, Some(-0x7fff_ffff)),
        (&[0xe9, 0xffff, 0][..], false, Some(0x7fff_ffff)),
        // 0xED (004A0880): the immediate signed high:low depth.
        (&[0xed, 0x1234, 0x0001, 0][..], false, Some(0x1_1234)),
        (&[0xed, 0x8000, 0xffff, 0][..], false, Some(-0x8000)),
    ] {
        let runtime = resident();
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) = custom_model(&runtime, words);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let begins: Vec<_> = host
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Begin(depth, flag) => Some((*depth, *flag)),
                _ => None,
            })
            .collect();
        assert_eq!(begins.len(), 1);
        assert_eq!(begins[0].1, sorting);
        if let Some(depth) = fixed {
            assert_eq!(begins[0].0, depth);
        } else if !sorting {
            match reference {
                None => reference = Some(begins[0].0),
                Some(expected) => assert_eq!(begins[0].0, expected),
            }
        }
    }
}

#[test]
fn eb_and_ec_use_origin_and_literal_depth_without_vertex_reads_in_both_tables() {
    let runtime = resident();
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (mut model, source) = custom_model(&runtime, &[0xeb, 0xef, 0xec, 0xef, 0]);
        // No projected vertex can accidentally provide either group depth.
        model.header.vertex_ranges = [0; 4];
        model.header.vertex_slot_count = 0;
        let mut env = environment();
        if dispatch == ModelDispatch::Far {
            env.projection.fade = [0, 200000];
        }
        env.view.eye[2] = 123;
        let mut run = Run {
            executor: &mut executor,
            environment: env,
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let values = ModelValues::new_invocation(Arc::clone(&run.executor.sine_table));
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(host.events[1], Event::Select(dispatch));
        assert_eq!(
            &host.events[2..],
            &[
                Event::Begin(9877, false),
                Event::End,
                Event::Begin(-1, false),
                Event::End,
            ]
        );
        assert_eq!(run.outcome.depth_groups, 2);
    }
}

/// 004A3880 (06) only steps past its 0xFFFF-ended list; 004A0250 (08) jumps
/// the command's SAR 8 in WORDs from its first argument.
#[test]
fn opcodes_06_and_08_step_past_their_words() {
    let runtime = resident();
    for (words, expected) in [
        (
            &[0x06, 1, 2, 0xffff, 0xec, 0xef, 0][..],
            &[Event::Begin(-1, false), Event::End][..],
        ),
        (
            &[0x0208, 0xec, 0xef, 0xeb, 0xef, 0][..],
            &[Event::Begin(9877, false), Event::End][..],
        ),
        (
            &[0x0008, 0xec, 0xef, 0][..],
            &[Event::Begin(-1, false), Event::End][..],
        ),
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (mut model, source) = custom_model(&runtime, words);
        model.header.vertex_ranges = [0; 4];
        model.header.vertex_slot_count = 0;
        let mut env = environment();
        env.view.eye[2] = 123;
        let mut run = Run {
            executor: &mut executor,
            environment: env,
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let values = ModelValues::new_invocation(Arc::clone(&run.executor.sine_table));
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(&host.events[2..], expected, "{words:x?}");
    }
}

/// 004A0C50: 13 jumps when "origin Z >= the signed high:low literal" differs
/// from the command's SAR 8; the jump counts from the first argument WORD.
#[test]
fn opcode_13_jumps_on_origin_depth_against_its_literal() {
    let runtime = resident();
    // Origin Z is 9877 with the eye at 123 (as EB's test).
    for (command, low, high, jumped) in [
        (0x13, 9877, 0, true),
        (0x13, 9878, 0, false),
        (0x113, 9877, 0, false),
        (0x113, 9878, 0, true),
        (0x13, 0, 0xffff, true),
    ] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (mut model, source) = custom_model(
            &runtime,
            &[command, 5, low, high, 0xec, 0xef, 0xeb, 0xef, 0],
        );
        model.header.vertex_ranges = [0; 4];
        model.header.vertex_slot_count = 0;
        let mut env = environment();
        env.view.eye[2] = 123;
        let mut run = Run {
            executor: &mut executor,
            environment: env,
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let values = ModelValues::new_invocation(Arc::clone(&run.executor.sine_table));
        let mut host = Host::default();
        run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        )
        .unwrap();
        let expected: &[Event] = if jumped {
            &[Event::Begin(9877, false), Event::End]
        } else {
            &[
                Event::Begin(-1, false),
                Event::End,
                Event::Begin(9877, false),
                Event::End,
            ]
        };
        assert_eq!(&host.events[2..], expected, "{command:#x} {low} {high}");
    }
}

#[test]
fn eb_after_a_child_uses_restored_parent_publication_and_failure_stops_the_group_tail() {
    let runtime = resident();
    for fail_depth_group in [false, true] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        let (model, source) =
            custom_model(&runtime, &[0x0e, 0, 751, 8, 0, 0xeb, 0xef, 0xec, 0xef, 0]);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host {
            fail_depth_group,
            ..Host::default()
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        );
        assert_eq!(host.transforms.last().unwrap().view_position[2], 10000);
        let tail = host
            .events
            .iter()
            .rposition(|event| matches!(event, Event::Publish(_)))
            .unwrap()
            + 1;
        if fail_depth_group {
            assert!(matches!(
                result,
                Err(ModelExecutionError::Host("depth group backend failed"))
            ));
            assert_eq!(&host.events[tail..], &[Event::Begin(10000, false)]);
            assert_eq!(run.outcome.depth_groups, 0);
        } else {
            result.unwrap();
            assert_eq!(
                &host.events[tail..],
                &[
                    Event::Begin(10000, false),
                    Event::End,
                    Event::Begin(-1, false),
                    Event::End,
                ]
            );
            assert_eq!(run.outcome.depth_groups, 2);
        }
    }
}

#[test]
fn required_object_2d_reaches_callback_before_projection_and_keeps_earlier_primitive() {
    let runtime = resident();
    for mode in [0, 2] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime, 10000);
        // Test-only decoded instructions over unchanged real H8 backing. The
        // first primitive is a completed caller prefix before the required
        // source+10 boundary; the second must remain unreached on error.
        let words = [
            0x1b,
            0,
            1812,
            8192,
            0,
            0x102d | (mode << 8),
            0,
            0xfff9,
            23,
            0x1b,
            0,
            1812,
            8192,
            0,
            0,
        ];
        let (model, source) = custom_model(&runtime, &words);
        let values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
        let mut run = Run {
            executor: &mut executor,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scratch.begin_draw().unwrap(),
            root_exit: None,
        };
        let mut host = Host {
            custom_policy: Some(crate::model_vertices::ModelCustomVertexPolicy::RequiredObject),
            ..Host::default()
        };
        let result = run.invoke(
            InvocationSeed {
                resource: ModelResource {
                    model: &model,
                    source,
                },
                context: request(757, &scratch).context,
                values,
                state_words: &[],
                parent_vertices: None,
                depth: 1,
                publications: 1,
                exact_origin: None,
            },
            &mut host,
        );
        let calls: Vec<_> = host
            .events
            .iter()
            .filter_map(|event| match event {
                Event::RequiredCustom(selector, extras) => Some((*selector, extras.clone())),
                _ => None,
            })
            .collect();
        if mode == 0 {
            assert!(result.is_ok());
            assert!(calls.is_empty());
            assert_eq!(run.outcome.primitive_calls, 2);
            assert_eq!(run.scratch.cursor().unwrap(), 0);
        } else {
            assert!(matches!(
                result,
                Err(ModelExecutionError::Vertex(ModelVertexError::Custom(
                    ModelCustomVertexError::Fault(ModelVertexFault::UnownedCustomVertex)
                )))
            ));
            assert_eq!(calls, vec![(-7, vec![23])]);
            assert_eq!(run.outcome.primitive_calls, 1);
            assert!(matches!(
                host.events[host.events.len() - 2],
                Event::Primitive(757, ..)
            ));
            assert_eq!(
                host.events.last(),
                Some(&Event::RequiredCustom(-7, vec![23]))
            );
            assert!(run.scratch.cursor().unwrap() > 0);
        }
    }
}

#[path = "model_execution/common_tests.rs"]
mod common;

/// `hoverjet2`, the settled hovercraft, saves its two rear fans' basis
/// (opcode 03) mirrored, orientation 0x20, the only such words in either
/// model, and `hoverjet1`, transforming, with 0. The port fix takes only the
/// settled one's.
#[test]
fn the_settled_hovercrafts_fans_alone_take_the_fix() {
    let (resident, _assets, _world) = crate::test_worlds::offline_world_retained(
        9,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let orientations = |slot: u16| -> Vec<u16> {
        let binding = crate::object_runtime::resident_model_resource(&resident, slot)
            .unwrap()
            .unwrap();
        binding
            .model
            .program
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.kind {
                inf_formats::ModelInstructionKind::Opaque(command)
                    if instruction.raw_command as u8 == 0x03 =>
                {
                    command.arguments.first().copied()
                }
                _ => None,
            })
            .collect()
    };
    // The last two are the fans'; the transforming model's are all 0.
    assert_eq!(orientations(121), [0, 0, 0, 0, 0x20, 0x20]);
    assert!(orientations(120)
        .iter()
        .all(|&orientation| orientation == 0));
    assert!(hovercraft_fans_unmirrored(121, 0x20, true));
    assert!(!hovercraft_fans_unmirrored(121, 0x20, false));
    assert!(!hovercraft_fans_unmirrored(120, 0x20, true));
    assert!(!hovercraft_fans_unmirrored(121, 0, true));
}

/// The fix turns the settled hovercraft's fans as retail does (a left steer
/// swivels them left, 2026-10-24) and only puts them the right way
/// out: retail's saved and turned basis with its X axis negated, right-handed
/// again. Saved unmirrored, the same turn swivelled them the other way.
#[test]
fn the_fixed_fans_swivel_as_retails_do() {
    let math = MenuBasisMath::new(
        crate::frontend_assets::FrontendAssets::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap()
        .sine_table()
        .values,
    );
    let row = |basis: &MenuBasis, first: usize| -> [f32; 3] {
        std::array::from_fn(|k| f32::from_bits(basis.words[first + k]))
    };
    let turned = |unmirror: bool, orientation: u16| {
        let mut context = crate::model_context::ModelContext::new([0; 3], MenuBasis::INITIAL);
        context.save_oriented_basis(orientation);
        context.rotate_saved_basis(1, 0x0800, &math);
        if unmirror {
            context.unmirror_saved_basis();
        }
        *context.saved_basis()
    };
    let retail = turned(false, 0x20);
    let fixed = turned(true, 0x20);
    let unmirrored = turned(false, 0);
    // The fixed fans' Y and Z rows are retail's, and X is its negation: a
    // fan symmetric across X stands where retail's does.
    assert_eq!(row(&fixed, 4), row(&retail, 4));
    assert_eq!(row(&fixed, 7), row(&retail, 7));
    assert_eq!(row(&fixed, 1), row(&retail, 1).map(|c| -c));
    // Right-handed again: X cross Y is Z.
    let [x, y, z] = [row(&fixed, 1), row(&fixed, 4), row(&fixed, 7)];
    let cross = [
        x[1] * y[2] - x[2] * y[1],
        x[2] * y[0] - x[0] * y[2],
        x[0] * y[1] - x[1] * y[0],
    ];
    assert!((0..3).map(|k| cross[k] * z[k]).sum::<f32>() > 0.99);
    // Saved unmirrored, its nose (Z) leans the other way across X.
    assert!(row(&unmirrored, 7)[0] * row(&fixed, 7)[0] < 0.0);
}
