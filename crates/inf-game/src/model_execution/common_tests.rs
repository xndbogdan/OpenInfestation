//! Supplied decoded programs exercise the generic Common05 owner.
//! Original EXE/OVL bytes and resource registrations remain unchanged.
use super::*;
use crate::model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector};
use inf_formats::{ModelSlotRef, ModelVectorI16};
use sha2::{Digest, Sha256};

#[derive(Default)]
struct CommonHost {
    callbacks: Vec<u32>,
    publications: Vec<ModelContext>,
    selections: usize,
    primitives: usize,
    fail_callback: Option<u32>,
    fail_primitive: bool,
    worlds: Vec<Option<[i32; 3]>>,
    shade0: Option<crate::model_lighting::ModelShade>,
    model: Option<u16>,
    source_ptr: Option<usize>,
    state_words: Vec<u16>,
}
impl ModelValueCallback for CommonHost {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.callbacks.push(selector);
        values.write_register(3, selector as u16).unwrap();
        if self.fail_callback == Some(selector) {
            Err("angle callback stop")
        } else {
            Ok(0)
        }
    }
}
impl ModelExecutionHost for CommonHost {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        _: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.publications.push(*context);
        Ok(())
    }
    fn select_projection(&mut self, _: ModelDispatch) -> Result<(), Self::Error> {
        self.selections += 1;
        Ok(())
    }
    fn primitive(
        &mut self,
        r: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.primitives += 1;
        self.worlds = [2, 4, 6, 7].map(|i| r.vertices.world(i).ok()).to_vec();
        self.shade0 = Some(r.shades.shade(ModelSlotRef::from_raw(0)).unwrap());
        self.model = Some(r.model.identity.global_index);
        self.source_ptr = Some(r.source.bytes.as_ptr() as usize);
        self.state_words = r.state_words.to_vec();
        assert_eq!(values.register(1).unwrap(), Some(17));
        if self.fail_primitive {
            Err("inner primitive stop")
        } else {
            Ok(())
        }
    }
    fn begin_depth_group(&mut self, _: i32, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
fn supplied_model<'a>(
    runtime: &'a ResidentRuntime,
    words: &[u16],
) -> (ModelEntry, RenderSource<'a>) {
    let (mut model, source) = custom_model(runtime, words);
    model.vertices = vec![
        ModelVectorI16 { x: 2, y: 3, z: 4 },
        ModelVectorI16 { x: 5, y: 6, z: 7 },
        ModelVectorI16 { x: 8, y: 9, z: 10 },
        ModelVectorI16 {
            x: 11,
            y: 12,
            z: 13,
        },
    ];
    model.header.vertex_slot_count = 12;
    model.header.raw_vertex_count = 4;
    model.header.shade_slot_count = 8;
    // Inline phase arguments, not the header, own these three ranges.
    model.header.reserved_10 = u16::MAX;
    model.header.vertex_ranges = [u16::MAX; 4];
    model.header.reserved_1a = u16::MAX;
    model.header.shade_ranges = [u16::MAX; 2];
    (model, source)
}
fn make_frame<'a>(
    model: &'a ModelEntry,
    source: RenderSource<'a>,
    scratch: &ProcessModelScratch,
    scope: &ModelScratchDrawScope,
    sine: Arc<[u16; 4096]>,
    dispatch: ModelDispatch,
) -> Invocation<'a, 'static, 'static> {
    let context = request(757, scratch).context;
    let env = environment();
    let transform = context.publish(env.view, env.coordinate_shift);
    let allocation = scratch.allocate(scope, 12, 8).unwrap();
    let mut vertices = ModelVertices::in_scratch(scratch, allocation.clone()).unwrap();
    vertices
        .initialize_world_ranges(model, &context, [0, 2, 4])
        .unwrap();
    Invocation {
        resource: ModelResource { model, source },
        context,
        values: ModelValues::new_invocation(sine),
        state_words: &[123, 456],
        parent_vertices: None,
        surface: ModelSurfaceBinding::Default,
        vertices,
        shades: ModelShades::in_process_scratch(scratch, allocation, env.lighting.ambient_word)
            .unwrap(),
        dispatch,
        transform,
        projector: ModelProjectionBackend {
            transform,
            view: env.view,
            projection: env.projection,
            dispatch,
            coordinate_shift: env.coordinate_shift,
            fog_factor: env.fog_factor,
        },
    }
}
fn inline(args: [u16; 12], body: &[u16]) -> Vec<u16> {
    let mut words = vec![0x05];
    words.extend(args);
    words.extend_from_slice(body);
    words.push(0); // inline terminator
    words.push(0); // containing program terminator
    words
}
fn common_args() -> [u16; 12] {
    // Independent raw4..8, world2..8, empty normals. Paired entries start6.
    [0, 0x81, 0x82, 0x83, 4, 2, 6, 8, 8, 0, 0, 0]
}
fn nested_body() -> Vec<u16> {
    // Packed literal 0x1120 decodes to 17. The r1 and saved-basis+A0 writes survive inline return.
    vec![0xf0, 1, 0x1120, 0, 0x03, 6, 1, 0, 0x1b, 0, 1812, 8192, 0]
}

#[test]
fn common05_full_native_body_and_both_table_entries_are_guarded() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&exe[0xa1150..0xa12d7])),
        "73c97843cd340224a92c59d9c0ba8336de133629df5f9c8a1aeb4b05521ea5d7"
    );
    let executable = inf_formats::FrontendExecutable::parse(&exe).unwrap();
    for table in [0x00540dc0, 0x005411c0] {
        assert_eq!(
            u32::from_le_bytes(
                executable
                    .mapped(table + 5 * 4, 4)
                    .unwrap()
                    .bytes
                    .try_into()
                    .unwrap()
            ),
            0x004a1150
        );
    }
}

#[test]
fn inline05_keeps_actual_common_frame_values_views_and_table_in_both_modes() {
    let runtime = resident();
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let scratch = ProcessModelScratch::isolated_unknown_fixture();
        let mut ex = executor(&runtime, 100);
        let words = inline(common_args(), &nested_body());
        let (model, source) = supplied_model(&runtime, &words);
        let scope = scratch.begin_draw().unwrap();
        let mut f = make_frame(
            &model,
            source,
            &scratch,
            &scope,
            Arc::clone(&ex.sine_table),
            dispatch,
        );
        f.context.basis.words[1] = 2.0f32.to_bits();
        let original = f.context;
        let origin = f.vertices.world(0).unwrap();
        let cursor = scratch.cursor().unwrap();
        let mut host = CommonHost::default();
        let mut run = Run {
            executor: &mut ex,
            environment: environment(),
            surface: ModelSurfaceBinding::Default,
            outcome: ModelExecutionOutcome::default(),
            scratch: &scratch,
            scratch_scope: scope,
            root_exit: None,
        };
        run.program(&mut f, &model.program, 1, &mut host).unwrap();
        assert_eq!(host.callbacks, [1, 2, 3]);
        assert_eq!(host.publications.len(), 2);
        assert_eq!(host.publications[0].position, origin);
        assert_eq!(
            host.publications[0].basis.words[0],
            original.basis.words[0].wrapping_add(3)
        );
        assert_eq!(host.publications[1].position, original.position);
        assert_eq!(host.primitives, 1);
        assert_eq!(host.selections, 0);
        assert_eq!(host.model, Some(model.identity.global_index));
        assert_eq!(host.source_ptr, Some(source.bytes.as_ptr() as usize));
        assert_eq!(host.state_words, [123, 456]);
        assert_eq!(f.context.position, original.position);
        assert_eq!(f.context.basis, original.basis);
        assert_eq!(
            f.context.saved_basis().words[1..],
            host.publications[0].basis.words[1..]
        );
        assert_ne!(
            f.context.saved_basis().words[1..],
            original.saved_basis().words[1..]
        );
        assert_eq!(f.values.register(1).unwrap(), Some(17));
        assert_eq!(f.values.register(3).unwrap(), Some(3));
        assert_eq!(f.dispatch, dispatch);
        assert_eq!(f.projector.dispatch, dispatch);
        assert_eq!(f.projector.transform, f.transform);
        assert!(f.vertices.slot(4).unwrap().projection.read_u32(4).is_some());
        assert!(f.vertices.slot(6).unwrap().projection.read_u32(4).is_some());
        assert!(f.vertices.slot(7).unwrap().projection.read_u32(4).is_some());
        assert_eq!(
            host.shade0,
            Some(crate::model_lighting::ModelShade::from_word(
                environment().lighting.ambient_word
            ))
        );
        assert_eq!(scratch.cursor().unwrap(), cursor);
        assert_eq!(run.outcome.invocations, 0);
        assert_eq!(run.outcome.instructions, 4); // containing05 + three inner records
        assert_eq!(host.worlds[0], f.vertices.world(2).ok());
        assert_eq!(host.worlds[3], f.vertices.world(7).ok());
    }
}

#[test]
fn angle_callback_stop_keeps_new_origin_and_value_prefix_without_publication_or_restore() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut ex = executor(&runtime, 100);
    let (model, source) = supplied_model(&runtime, &inline(common_args(), &nested_body()));
    let scope = scratch.begin_draw().unwrap();
    let mut f = make_frame(
        &model,
        source,
        &scratch,
        &scope,
        Arc::clone(&ex.sine_table),
        ModelDispatch::Near,
    );
    let origin = f.vertices.world(0).unwrap();
    let bytes = scratch.read_pool::<320>(0).unwrap();
    let mut host = CommonHost {
        fail_callback: Some(2),
        ..Default::default()
    };
    let mut run = Run {
        executor: &mut ex,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scope,
        root_exit: None,
    };
    assert!(matches!(
        run.program(&mut f, &model.program, 1, &mut host),
        Err(ModelExecutionError::Value(ModelValueError::Callback(
            "angle callback stop"
        )))
    ));
    assert_eq!(host.callbacks, [1, 2]);
    assert!(host.publications.is_empty());
    assert_eq!(f.context.position, origin);
    assert_eq!(f.values.register(3).unwrap(), Some(2));
    assert_eq!(scratch.read_pool::<320>(0).unwrap(), bytes);
}

#[test]
fn inner_primitive_stop_retains_nested_transform_and_completed_phase_prefix() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut ex = executor(&runtime, 100);
    let (model, source) = supplied_model(&runtime, &inline(common_args(), &nested_body()));
    let scope = scratch.begin_draw().unwrap();
    let mut f = make_frame(
        &model,
        source,
        &scratch,
        &scope,
        Arc::clone(&ex.sine_table),
        ModelDispatch::Near,
    );
    let original = f.context;
    let origin = f.vertices.world(0).unwrap();
    let cursor = scratch.cursor().unwrap();
    let mut host = CommonHost {
        fail_primitive: true,
        ..Default::default()
    };
    let mut run = Run {
        executor: &mut ex,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scope,
        root_exit: None,
    };
    assert!(matches!(
        run.program(&mut f, &model.program, 1, &mut host),
        Err(ModelExecutionError::Host("inner primitive stop"))
    ));
    assert_eq!(host.publications.len(), 1);
    assert_eq!(host.primitives, 1);
    assert_eq!(f.context.position, origin);
    assert_ne!(f.context.position, original.position);
    assert_eq!(f.projector.transform, f.transform);
    assert!(f.vertices.slot(4).unwrap().projection.read_u32(4).is_some());
    assert!(f.vertices.world(6).is_ok());
    assert_eq!(f.values.register(1).unwrap(), Some(17));
    assert_eq!(scratch.cursor().unwrap(), cursor);
}

#[test]
fn later_invalid_world_range_cannot_erase_completed_raw_projection_prefix() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut ex = executor(&runtime, 100);
    let mut args = common_args();
    args[5] = 0xffff;
    let (model, source) = supplied_model(&runtime, &inline(args, &[]));
    let scope = scratch.begin_draw().unwrap();
    let mut f = make_frame(
        &model,
        source,
        &scratch,
        &scope,
        Arc::clone(&ex.sine_table),
        ModelDispatch::Near,
    );
    let mut host = CommonHost::default();
    let mut run = Run {
        executor: &mut ex,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scope,
        root_exit: None,
    };
    assert!(matches!(
        run.program(&mut f, &model.program, 1, &mut host),
        Err(ModelExecutionError::Vertex(ModelVertexError::Fault(
            ModelVertexFault::InvalidInitialization
        )))
    ));
    assert_eq!(host.publications.len(), 1);
    assert!(f.vertices.slot(4).unwrap().projection.read_u32(4).is_some());
    assert!(f.vertices.slot(7).unwrap().projection.read_u32(4).is_some());
    assert_eq!(host.primitives, 0);
}

#[test]
fn inline_recursion_uses_portable_depth_budget_without_a_new_allocation() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut ex = executor(&runtime, 100);
    ex.limits.invocation_depth = 1;
    let (model, source) = supplied_model(&runtime, &inline(common_args(), &nested_body()));
    let scope = scratch.begin_draw().unwrap();
    let mut f = make_frame(
        &model,
        source,
        &scratch,
        &scope,
        Arc::clone(&ex.sine_table),
        ModelDispatch::Near,
    );
    let cursor = scratch.cursor().unwrap();
    let mut host = CommonHost::default();
    let mut run = Run {
        executor: &mut ex,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scope,
        root_exit: None,
    };
    assert!(matches!(
        run.program(&mut f, &model.program, 1, &mut host),
        Err(ModelExecutionError::Limit)
    ));
    assert_eq!(host.publications.len(), 1);
    assert_eq!(host.primitives, 0);
    assert_eq!(scratch.cursor().unwrap(), cursor);
}

#[derive(Default)]
struct RangeProjector {
    calls: Vec<ModelVertexInput>,
    fail_second: bool,
}
impl ModelVertexProjector for RangeProjector {
    type Error = &'static str;
    fn project(
        &mut self,
        input: ModelVertexInput,
        out: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        self.calls.push(input);
        out.bytes[8] = Some(0x31);
        if self.fail_second && self.calls.len() == 2 {
            Err("projection prefix stop")
        } else {
            out.write_u32(0, 0x12345678);
            Ok(self.calls.len() as u32)
        }
    }
}
#[test]
fn independent_raw_phase_preserves_second_projector_error_bytes_before_world_phase() {
    let runtime = resident();
    let (model, _) = supplied_model(&runtime, &[0]);
    let mut vertices = ModelVertices::new_invocation(12);
    let mut p = RangeProjector {
        fail_second: true,
        ..Default::default()
    };
    assert!(matches!(
        vertices.initialize_projection_ranges(&model, [4, 6, 8], &mut p),
        Err(ModelVertexError::Projection("projection prefix stop"))
    ));
    assert_eq!(
        p.calls,
        [
            ModelVertexInput::Local([8, 9, 10]),
            ModelVertexInput::Local([11, 12, 13])
        ]
    );
    assert_eq!(vertices.slot(4).unwrap().projection.read_u32(4), Some(1));
    assert_eq!(vertices.slot(6).unwrap().projection.bytes[8], Some(0x31));
    assert_eq!(vertices.slot(6).unwrap().projection.read_u32(4), None);
    assert_eq!(vertices.slot(7).unwrap().projection.bytes, [None; 12]);
    assert_eq!(vertices.slot(4).unwrap().world, None);
    assert!(vertices.stopped());
}
#[test]
fn independent_normal_range_retains_ambient_and_high_byte_without_header_reseed() {
    let runtime = resident();
    let (mut model, _) = supplied_model(&runtime, &[0]);
    // Explicit intrinsic normal-byte fixture, not an edited original resource.
    let bytes = [1u8, 0, 2, 0, 3, 0];
    model.span = inf_formats::ByteSpan { start: 0, end: 6 };
    model.content_span = model.span;
    model.normals_span = model.span;
    let mut shades = ModelShades::new_invocation(8, 0x7f05060a).unwrap();
    let ambient = shades.shade(ModelSlotRef::from_raw(0)).unwrap();
    shades
        .initialize_normal_ranges(
            &model,
            &bytes,
            &MenuBasis::INITIAL,
            environment().lighting,
            [2, 2, 4],
        )
        .unwrap();
    assert_eq!(shades.shade(ModelSlotRef::from_raw(0)).unwrap(), ambient);
    assert_eq!(shades.shade(ModelSlotRef::from_raw(1)).unwrap(), ambient);
    for (slot, normal) in [(2, [1, 2, 3]), (3, [-1, 2, 3])] {
        let shade = shades.shade(ModelSlotRef::from_raw(slot)).unwrap();
        assert_eq!(
            shade.rgb,
            Some(crate::model_lighting::shade_normal(
                normal,
                &MenuBasis::INITIAL,
                environment().lighting
            ))
        );
        assert_eq!(shade.high_byte, None);
    }
    assert_eq!(
        shades.shade(ModelSlotRef::from_raw(4)).unwrap(),
        crate::model_lighting::ModelShade::default()
    );
}

#[test]
fn literal_zero_angles_still_run_three_native_rotations_and_normal_restoration() {
    let runtime = resident();
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let mut ex = executor(&runtime, 100);
    let mut args = common_args();
    args[1..4].fill(0);
    let (model, source) = supplied_model(&runtime, &inline(args, &[]));
    let scope = scratch.begin_draw().unwrap();
    let mut f = make_frame(
        &model,
        source,
        &scratch,
        &scope,
        Arc::clone(&ex.sine_table),
        ModelDispatch::Near,
    );
    let original = f.context;
    let mut host = CommonHost::default();
    let mut run = Run {
        executor: &mut ex,
        environment: environment(),
        surface: ModelSurfaceBinding::Default,
        outcome: ModelExecutionOutcome::default(),
        scratch: &scratch,
        scratch_scope: scope,
        root_exit: None,
    };
    run.program(&mut f, &model.program, 1, &mut host).unwrap();
    assert!(host.callbacks.is_empty());
    assert_eq!(host.publications.len(), 2);
    assert_eq!(
        host.publications[0].basis.words[0],
        original.basis.words[0].wrapping_add(3)
    );
    assert_eq!(f.context, original);
}
