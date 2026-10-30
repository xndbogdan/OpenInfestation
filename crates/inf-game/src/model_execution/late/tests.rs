use super::*;
use crate::{
    frontend_assets::FrontendAssets, menu::MenuBasis, Installation, ResidencyRequest,
    ResidentRuntime,
};
use inf_formats::{decode_model_program, ModelVectorI16};

#[derive(Default)]
struct Host {
    published: Vec<ModelContext>,
    selected: usize,
    primitives: usize,
    groups: usize,
    callbacks: Vec<u32>,
    custom: usize,
    fail_callback: bool,
}
impl ModelValueCallback for Host {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.callbacks.push(selector);
        values.write_register(1, 17).unwrap();
        if self.fail_callback {
            Err("packed callback stop")
        } else {
            Ok(0)
        }
    }
}
impl ModelExecutionHost for Host {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        _: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.published.push(*context);
        Ok(())
    }
    fn select_projection(&mut self, _: ModelDispatch) -> Result<(), Self::Error> {
        self.selected += 1;
        Ok(())
    }
    fn primitive(
        &mut self,
        _: ModelPrimitiveRequest<'_>,
        _: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.primitives += 1;
        Ok(())
    }
    fn begin_depth_group(&mut self, _: i32, _: bool) -> Result<(), Self::Error> {
        self.groups += 1;
        Ok(())
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.groups += 1;
        Ok(())
    }
    fn required_custom_vertex(
        &mut self,
        _: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.custom += 1;
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }
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
fn executor(runtime: &ResidentRuntime) -> ModelExecutor<'_> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    ModelExecutor::new(
        RenderResources::new(runtime),
        Arc::new(assets.sine_table().values),
        ModelExecutionLimits {
            instructions: 1000,
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
            fade: [1, 2],
        },
        coordinate_shift: 0,
        fog_factor: 0,
        lighting: ModelLighting {
            direction: [0.0; 3],
            directional_word: 0,
            ambient: [0; 3],
            ambient_word: 0,
        },
    }
}
fn run<'a, 'b, 't>(
    executor: &'b mut ModelExecutor<'a>,
    scratch: &'t ProcessModelScratch,
    scratch_scope: &ModelScratchDrawScope,
) -> Run<'a, 'b, 't> {
    Run {
        executor,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch,
        scratch_scope: scratch_scope.clone(),
        root_exit: None,
    }
}
fn source(runtime: &ResidentRuntime) -> RenderSource<'_> {
    RenderResources::new(runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 757,
        })
        .unwrap()
        .source
}
fn model(runtime: &ResidentRuntime, words: &[u16]) -> ModelEntry {
    let resource = RenderResources::new(runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 757,
        })
        .unwrap();
    let RenderResourceData::Model(model) = resource.data else {
        unreachable!()
    };
    // Canonical parsed entry, with an explicitly authored decoded instruction
    // fixture and geometry. Original resident bytes/registrations are unchanged.
    let mut model = model.clone();
    model.program = decode_model_program(words).unwrap();
    model.vertices = vec![
        ModelVectorI16 { x: 2, y: 3, z: 4 },
        ModelVectorI16 { x: 5, y: 6, z: 7 },
    ];
    model.header.vertex_slot_count = 12;
    model.header.raw_vertex_count = 2;
    model.header.vertex_ranges = [0, 2, 4, 0xffff];
    model.header.reserved_10 = 0xffff;
    model.header.shade_slot_count = 0;
    model
}
fn context() -> ModelContext {
    ModelContext::new([10, 20, -100000], MenuBasis::INITIAL)
}
fn frame<'a>(
    model: &'a ModelEntry,
    source: RenderSource<'a>,
    executor: &ModelExecutor<'_>,
) -> LateInvocation<'a, 'static> {
    let context = context();
    let mut vertices = ModelVertices::new_invocation(model.header.vertex_slot_count);
    vertices
        .initialize_world_ranges(model, &context, [0, 2, 4])
        .unwrap();
    LateInvocation {
        resource: ModelResource { model, source },
        transform: context.publish(environment().view, 0),
        context,
        values: ModelValues::new_invocation(Arc::clone(&executor.sine_table)),
        vertices,
        state_words: &[],
        parent_vertices: None,
    }
}

#[test]
fn required_root_publishes_before_negative_outside_and_in_range_missing_resources() {
    let runtime = resident();
    let resources = RenderResources::new(&runtime);
    let capacity = resources.pool_capacity(RenderResourceKind::Model);
    let missing = (0..capacity)
        .find(|&i| {
            matches!(
                resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i as i32
                }),
                Err(RenderResourceError::NotResident { .. })
            )
        })
        .unwrap();
    for argument in [-1, capacity as i32 + 1, missing as i32] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = executor(&runtime);
        let mut host = Host::default();
        let result = executor.draw(
            ModelDrawRequest {
                scratch: &scratch,
                root_mode: ModelRootMode::ManagerLate,
                model_argument: argument,
                context: context(),
                environment: environment(),
                surface: ModelSurfaceBinding::Default,
            },
            &mut host,
        );
        assert!(result.is_err());
        assert!(executor.stopped());
        assert_eq!(host.published, [context()]);
        assert_eq!((host.selected, host.primitives, host.groups), (0, 0, 0));
    }
}

#[test]
fn late_invocation_has_no_common_culling_projection_or_ambient_shade_requirement() {
    let runtime = resident();
    let model = model(&runtime, &[0]);
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut host = Host::default();
    let mut run = run(&mut executor, &scratch, &scratch_scope);
    run.late_invoke(
        InvocationSeed {
            resource: ModelResource {
                model: &model,
                source: source(&runtime),
            },
            context: context(),
            values: ModelValues::new_invocation(Arc::clone(&run.executor.sine_table)),
            state_words: &[],
            parent_vertices: None,
            depth: 1,
            publications: 1,
            exact_origin: None,
        },
        None,
        &mut host,
    )
    .unwrap();
    assert_eq!(run.outcome.invocations, 1);
    assert_eq!(run.outcome.culled, 0);
    assert_eq!(run.scratch.cursor().unwrap(), 0);
    assert_eq!(host.published, [context()]);
    assert_eq!((host.selected, host.primitives, host.groups), (0, 0, 0));
    // Same published position fails the common frustum. The late path does not
    // change it or manufacture a Near/Far selection to gain admission.
    assert!(!context()
        .publish(environment().view, 0)
        .in_frustum(model.header.visibility_extent, &environment().projection));
}

#[test]
fn direct_record_skips_do_not_call_values_primitives_groups_or_rng() {
    let runtime = resident();
    let model = model(
        &runtime,
        &[
            0xeb, 0xec, 0xef, 0xea, 0xffff, 0xee, 0xffff, 0x80, 0x80, 0x15, 0xffff, 0x07, 0xffff,
            0xffff, 0xffff, 0xffff, 0x02, 0x80, 0x80, 0x80, 0xe8, 0x80, 0xffff, 0x06, 0x80, 0xffff,
            0,
        ],
    );
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let mut host = Host::default();
    run(&mut executor, &scratch, &scratch_scope)
        .late_program(&mut frame, &model.program, 1, &mut host)
        .unwrap();
    assert!(host.callbacks.is_empty());
    assert!(host.published.is_empty());
    assert_eq!(
        (host.selected, host.primitives, host.groups, host.custom),
        (0, 0, 0, 0)
    );
    assert_eq!(frame.values.register(0).unwrap(), None);
}

#[test]
fn late_child_negative_bypasses_missing_state_and_does_not_republish_parent() {
    let runtime = resident();
    let model = model(&runtime, &[0x0e, 6, 0xffff, 6, 0]);
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let mut host = Host::default();
    run(&mut executor, &scratch, &scratch_scope)
        .late_program(&mut frame, &model.program, 1, &mut host)
        .unwrap();
    assert!(host.published.is_empty());
    assert_eq!(frame.context, context());
    assert_eq!(frame.values.register(0).unwrap(), None);
}

#[test]
fn supplied_child_invocation_inherits_only_four_values_and_publishes_twice() {
    let runtime = resident();
    let model = model(&runtime, &[0x0016, 3, 0xc0, 0, 0]);
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut values = ModelValues::new_invocation(Arc::clone(&executor.sine_table));
    values.write_register(0, 0).unwrap();
    values.write_register(4, 99).unwrap();
    let child = values.child_invocation().unwrap();
    assert_eq!(child.register(0).unwrap(), Some(0));
    assert_eq!(child.register(4).unwrap(), None);
    let mut host = Host::default();
    let mut run = run(&mut executor, &scratch, &scratch_scope);
    run.late_invoke(
        InvocationSeed {
            resource: ModelResource {
                model: &model,
                source: source(&runtime),
            },
            context: context().child([1, 2, 3], 6),
            values: child,
            state_words: &[0],
            parent_vertices: Some(ModelVertices::new_invocation(12)),
            depth: 2,
            publications: 2,
            exact_origin: None,
        },
        None,
        &mut host,
    )
    .unwrap();
    assert_eq!(host.published.len(), 2);
    assert_eq!(host.published[0], host.published[1]);
    assert_eq!((host.selected, host.primitives, host.groups), (0, 0, 0));
    assert_eq!(values.register(4).unwrap(), Some(99));
}

#[test]
fn nested_rotation_retains_early_origin_and_packed_write_on_error_without_parent_restore() {
    let runtime = resident();
    let model = model(
        &runtime,
        &[0x05, 0, 0x80, 0, 0, 0, 0, 2, 4, 0, 0, 0, 0, 0, 0],
    );
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let expected_origin = frame.vertices.world(0).unwrap();
    let mut host = Host {
        fail_callback: true,
        ..Host::default()
    };
    assert!(matches!(
        run(&mut executor, &scratch, &scratch_scope).late_program(
            &mut frame,
            &model.program,
            1,
            &mut host
        ),
        Err(ModelExecutionError::Value(ModelValueError::Callback(
            "packed callback stop"
        )))
    ));
    assert_eq!(frame.context.position, expected_origin);
    assert_eq!(frame.values.register(1).unwrap(), Some(17));
    assert_eq!(host.callbacks, [0]);
    assert!(host.published.is_empty());
}

#[test]
fn nested_normal_return_restores_only_thirteen_words_and_keeps_inner_saved_basis() {
    let runtime = resident();
    let model = model(
        &runtime,
        &[
            0x05, 0, 0, 0, 0, 0, 0, 2, 4, 0, 0, 0, 0, 0x03, 0x21, 0, 0, 0, 0,
        ],
    );
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let mut host = Host::default();
    let parent = frame.context;
    run(&mut executor, &scratch, &scratch_scope)
        .late_program(&mut frame, &model.program, 1, &mut host)
        .unwrap();
    assert_eq!(frame.context.position, parent.position);
    assert_eq!(frame.context.basis, parent.basis);
    assert_ne!(frame.context.saved_basis(), parent.saved_basis());
    assert_eq!(host.published.len(), 2);
    assert_eq!(host.published.last().unwrap().position, parent.position);
    assert_eq!((host.selected, host.primitives, host.groups), (0, 0, 0));
}

#[test]
fn forced_custom_stop_leaves_nested_publication_and_prior_world_prefix() {
    let runtime = resident();
    let model = model(
        &runtime,
        &[
            0x05, 0, 0, 0, 0, 0, 0, 2, 4, 0, 0, 0, 0, 0x052a, 4, 0xffff, 0x022d, 6, 3, 0, 0,
        ],
    );
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let mut host = Host::default();
    let origin = frame.vertices.world(0).unwrap();
    assert!(matches!(
        run(&mut executor, &scratch, &scratch_scope).late_program(
            &mut frame,
            &model.program,
            1,
            &mut host
        ),
        Err(ModelExecutionError::Vertex(ModelVertexError::Custom(
            ModelCustomVertexError::Fault(ModelVertexFault::UnownedCustomVertex)
        )))
    ));
    assert_eq!(host.custom, 1);
    assert_eq!(host.published.len(), 1);
    assert_eq!(frame.context.position, origin);
    assert_eq!(frame.vertices.world(4).unwrap(), origin);
    assert_eq!(frame.vertices.world(5).unwrap(), origin);
    assert!(frame.vertices.world(6).is_err());
    assert!(frame.vertices.stopped());
}

#[test]
fn nested_direction_block_uses_wrapped_word_deltas_and_restores_parent_normally() {
    let runtime = resident();
    let mut model = model(&runtime, &[0x09, 0, 2, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    model.vertices = vec![
        ModelVectorI16 { x: 0, y: 0, z: 0 },
        ModelVectorI16 { x: 0, y: 0, z: 10 },
        ModelVectorI16 { x: 0, y: 10, z: 0 },
    ];
    model.header.raw_vertex_count = 3;
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    frame
        .vertices
        .initialize_world_ranges(&model, &frame.context, [0, 0, 6])
        .unwrap();
    let parent = frame.context;
    let mut host = Host::default();
    run(&mut executor, &scratch, &scratch_scope)
        .late_program(&mut frame, &model.program, 1, &mut host)
        .unwrap();
    assert_eq!(host.published.len(), 2);
    assert_eq!(host.published[0].basis, MenuBasis::INITIAL);
    assert_eq!(host.published[0].position, parent.position);
    assert_eq!(frame.context, parent);
    assert!(host.callbacks.is_empty());
}

#[test]
fn reserved_direct_table_slot_stops_after_prior_value_write() {
    let runtime = resident();
    // Supplied decoded reserved slot: the canonical parser rejects11. Decode
    // the same one-WORD record shape at10, then replace this in-memory test
    // command only. No original OVL bytes or parser admission are changed.
    let mut model = model(&runtime, &[0xf0, 1, 0x80, 0, 0x10, 0]);
    model.program.instructions[1].raw_command = 0x11;
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    let mut host = Host::default();
    assert!(matches!(
        run(&mut executor, &scratch, &scratch_scope).late_program(
            &mut frame,
            &model.program,
            1,
            &mut host
        ),
        Err(ModelExecutionError::Unsupported { opcode: 0x11, .. })
    ));
    assert_eq!(frame.values.register(1).unwrap(), Some(0));
    assert_eq!(host.callbacks, [0]);
    assert_eq!((host.primitives, host.groups), (0, 0));
}

#[test]
fn depth_branch_uses_signed_full_command_xor_and_decoded_relative_boundary() {
    let runtime = resident();
    // Z=-100000 >= -200000 gives1. Full signed mode2 XOR1 remains nonzero,
    // so jump to terminator and never read uninitialized register0 below.
    let model = model(&runtime, &[0x0213, 7, 0xf2c0, 0xfffc, 0x03, 0, 0, 0xc0, 0]);
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scratch_scope = scratch.begin_draw().unwrap();
    let mut executor = executor(&runtime);
    let mut frame = frame(&model, source(&runtime), &executor);
    run(&mut executor, &scratch, &scratch_scope)
        .late_program(&mut frame, &model.program, 1, &mut Host::default())
        .unwrap();
    assert!(!frame.values.stopped());
    // A complete canonical variable-length08 record distinguishes signed
    // SAR8=-128 (target before program) from unsigned128 (terminator target).
    let mut words = [0u16; 130];
    words[0] = 0x8008;
    let invalid = decode_model_program(&words).unwrap();
    assert!(matches!(
        run(&mut executor, &scratch, &scratch_scope).late_program(
            &mut frame,
            &invalid,
            1,
            &mut Host::default()
        ),
        Err(ModelExecutionError::Branch { .. })
    ));
}

#[test]
fn complete_late_root_child_and_table_original_guards_remain_fixed() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    for (a, b, expected) in [
        (
            0xed610,
            0xed7d4,
            "6ca4a90f8e67638492f728ea4f370ee1a80f2dbbbe19f9c9b1cdf205d57af8c2",
        ),
        (
            0xed7e0,
            0xed9b5,
            "704f31a9747b071ce62f8e3a770d47219e4033c4d41295e56044bf1d8159fcef",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(&bytes[a..b])), expected);
    }
    // Map this file-backed .data span using the actual PE section's RVA/raw
    // offsets. The direct table is not either common projection table.
    let pe = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    let table = pe.mapped(0x544764, 1024).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(table.bytes)),
        "3b8a16e84d74dbf97acf5a13bd7c1ef95adafe449ad0bdf1839da91bf5ff5ef0"
    );
}
