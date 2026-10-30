use super::*;
use crate::{
    menu::MenuBasis,
    model_context::{ModelDispatch, ModelTransform},
    model_execution::ModelPrimitiveRequest,
};
use inf_formats::{
    decode_model_program, ByteSpan, ModelFlags, ModelHeader, ModelIdentity, ModelVectorI16,
};
use std::sync::Arc;

fn model() -> ModelEntry {
    let span = ByteSpan { start: 0, end: 0 };
    // Explicit decoded fixture storage, not a modified original OVL.
    ModelEntry {
        identity: ModelIdentity {
            local_index: 0,
            global_index: 0,
        },
        span,
        content_span: span,
        header_span: span,
        header: ModelHeader {
            visibility_extent: 0,
            command_word_count: 1,
            auxiliary_count: 0,
            flags: ModelFlags::from_raw(0),
            vertex_slot_count: 12,
            raw_vertex_count: 2,
            shade_slot_count: 0,
            raw_0e: 0,
            reserved_10: 0xffff,
            vertex_ranges: [0, 2, 4, 0xffff],
            reserved_1a: 0,
            shade_ranges: [0; 2],
        },
        auxiliary_span: span,
        auxiliary: vec![],
        auxiliary_program: None,
        vertices_span: span,
        vertices: vec![
            ModelVectorI16 { x: 2, y: 3, z: 4 },
            ModelVectorI16 { x: 5, y: 6, z: 7 },
        ],
        normals_span: span,
        normals: vec![],
        command_span: span,
        program: decode_model_program(&[0]).unwrap(),
        name: None,
        padding: span,
    }
}

fn instruction(command: u16, arguments: &[u16]) -> ModelInstruction {
    let words: Vec<_> = std::iter::once(command)
        .chain(arguments.iter().copied())
        .chain([0])
        .collect();
    decode_model_program(&words).unwrap().instructions.remove(0)
}

#[derive(Default)]
struct Host {
    values: Vec<u32>,
    custom: Vec<(i32, Vec<u16>)>,
    rng: u32,
    custom_fail: bool,
}
impl ModelValueCallback for Host {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.values.push(selector);
        values.write_register(1, 0x8000).unwrap();
        Ok(0x4000)
    }
}
impl ModelExecutionHost for Host {
    fn publish_transform(
        &mut self,
        _: &ModelContext,
        _: ModelTransform,
    ) -> Result<(), Self::Error> {
        panic!("vertex phase does not publish")
    }
    fn select_projection(&mut self, _: ModelDispatch) -> Result<(), Self::Error> {
        panic!("world-only phase does not select projection")
    }
    fn primitive(
        &mut self,
        _: ModelPrimitiveRequest<'_>,
        _: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        panic!("world-only phase does not draw")
    }
    fn begin_depth_group(&mut self, _: i32, _: bool) -> Result<(), Self::Error> {
        panic!("world-only phase does not queue")
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        panic!("world-only phase does not queue")
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        Some(&mut self.rng)
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.custom
            .push((request.selector, request.extras.to_vec()));
        vertices.slots[0].world = Some([77, 88, 99]);
        *output = Some([9, 8, 7]);
        if self.custom_fail {
            Err(ModelCustomVertexError::Host("actual callback stop"))
        } else {
            Ok(())
        }
    }
}

fn values() -> ModelValues {
    ModelValues::new_invocation(Arc::new([0; 4096]))
}
fn context() -> ModelContext {
    ModelContext::new([10, 20, 30], MenuBasis::INITIAL)
}
fn execute(
    vertices: &mut ModelVertices,
    instruction: &ModelInstruction,
    values: &mut ModelValues,
    host: &mut Host,
) -> Result<(), ModelVertexError<&'static str>> {
    vertices.execute_world_stream(
        ModelWorldStreamRequest {
            instruction,
            model: &model(),
            context: &context(),
            state_words: &[],
            parent: None,
            surface: ModelSurfaceBinding::Default,
        },
        values,
        host,
    )
}

#[test]
fn world_initialization_reads_only_three_ranges_and_leaves_projection_unknown() {
    let mut vertices = ModelVertices::new_invocation(12);
    vertices
        .initialize_world_ranges(&model(), &context(), [0, 2, 4])
        .unwrap();
    assert_eq!(vertices.world(0).unwrap(), [12, 23, 34]);
    assert!(vertices.world(1).is_err());
    assert_eq!(vertices.world(2).unwrap(), [15, 26, 37]);
    assert_eq!(vertices.world(3).unwrap(), [5, 26, 37]);
    assert!(vertices
        .slots()
        .iter()
        .all(|slot| slot.projection == ModelVertexProjection::default()));
    assert!(vertices.world(4).is_err());
}

#[test]
fn root_copy_high_pair_still_writes_first_xyz_and_copies_only_xyz_to_second() {
    let mut vertices = ModelVertices::new_invocation(12);
    vertices.slots[4].projection.write_u32(4, 0x1111);
    vertices.slots[5].projection.write_u32(4, 0x2222);
    execute(
        &mut vertices,
        &instruction(0x042a, &[4, 0xffff]),
        &mut values(),
        &mut Host::default(),
    )
    .unwrap();
    assert_eq!(vertices.world(4).unwrap(), [10, 20, 30]);
    assert_eq!(vertices.world(5).unwrap(), [10, 20, 30]);
    assert_eq!(vertices.slots[4].projection.read_u32(4), Some(0x1111));
    assert_eq!(vertices.slots[5].projection.read_u32(4), Some(0x2222));
}

#[test]
fn parent_copy_reads_state_in_clear_mode_and_copies_full_slot_only_when_reached() {
    let model = model();
    let context = context();
    let mut parent = ModelVertices::new_invocation(2);
    parent.slots[0].world = Some([1, 2, 3]);
    parent.slots[0].projection.write_u32(4, 123);
    parent.slots[1].world = Some([4, 5, 6]);
    parent.slots[1].projection.write_u32(4, 456);
    let mut vertices = ModelVertices::new_invocation(12);
    let clear = instruction(0x002a, &[0xffff, 0]);
    let request = ModelWorldStreamRequest {
        instruction: &clear,
        model: &model,
        context: &context,
        state_words: &[],
        parent: Some(&parent),
        surface: ModelSurfaceBinding::Default,
    };
    assert!(matches!(
        vertices.execute_world_stream(request, &mut values(), &mut Host::default()),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x2a }
        ))
    ));
    assert!(vertices.stopped());
    let mut vertices = ModelVertices::new_invocation(12);
    let copy = instruction(0x052a, &[4, 0]);
    vertices
        .execute_world_stream(
            ModelWorldStreamRequest {
                instruction: &copy,
                model: &model,
                context: &context,
                state_words: &[0],
                parent: Some(&parent),
                surface: ModelSurfaceBinding::Default,
            },
            &mut values(),
            &mut Host::default(),
        )
        .unwrap();
    assert_eq!(vertices.slots[4], parent.slots[0]);
    assert_eq!(vertices.slots[5], parent.slots[1]);
}

#[test]
fn world_math_controls_have_signed_wrapping_outputs_and_pair_sources() {
    for (command, args, expected) in [
        (0x0521, vec![8, 0], [10, 20, 30]),
        (0x0523, vec![8, 0, 2], [20, 30, 40]),
        (0x0524, vec![8, 0, 2, 4], [35, 45, 55]),
        (0x0525, vec![8, 0, 2], [30, 40, 50]),
        (0x0526, vec![8, 0, 2, 0x8000], [20, 30, 40]),
        (0x0527, vec![8, 0, 2, 4, 6, 0], [10, 20, 30]),
        (0x0528, vec![8, 0, 2, 0x4000, 0x4000], [20, 30, 40]),
        (0x0529, vec![8, 0, 2, 4, 6, 0, 0], [10, 20, 30]),
    ] {
        let mut vertices = ModelVertices::new_invocation(12);
        for (index, world) in [
            [10, 20, 30],
            [-10, -20, -30],
            [30, 40, 50],
            [-30, -40, -50],
            [5, 15, 25],
            [-5, -15, -25],
            [7, 17, 27],
            [-7, -17, -27],
        ]
        .into_iter()
        .enumerate()
        {
            vertices.slots[index].world = Some(world);
        }
        execute(
            &mut vertices,
            &instruction(command, &args),
            &mut values(),
            &mut Host::default(),
        )
        .unwrap();
        assert_eq!(vertices.world(8).unwrap(), expected, "opcode {command:x}");
        assert!(vertices.world(9).is_ok());
        assert!(vertices.slots[8].projection == ModelVertexProjection::default());
    }
    assert_eq!(cubic([0, 100, 200, 300], 0x2000_0000), 150);
    assert_eq!(cubic([i32::MAX, i32::MIN, 123, -456], 0), i32::MAX);
}

/// The late stream masks projection bits off before 004EC810, so every
/// receipt case without them must leave the same world writes, subtracting
/// the context position the root published.
#[test]
fn late_origin_offset_matches_original_004ec810_world_writes() {
    let receipt = crate::model_vertices::offset_25_tests::receipt();
    let mut compared = 0;
    for case in &receipt.cases {
        if case.command & 0x0a00 != 0 {
            continue;
        }
        let mut vertices = crate::model_vertices::offset_25_tests::seeded(case);
        let words = case.arguments.map(|word| word as u16);
        vertices
            .execute_world_stream(
                ModelWorldStreamRequest {
                    instruction: &instruction(case.command, &words),
                    model: &model(),
                    context: &ModelContext::new(case.origin, MenuBasis::INITIAL),
                    state_words: &[],
                    parent: None,
                    surface: ModelSurfaceBinding::Default,
                },
                &mut values(),
                &mut Host::default(),
            )
            .unwrap();
        let worlds: Vec<_> = vertices
            .slots
            .iter()
            .map(|slot| slot.world.unwrap())
            .collect();
        assert_eq!(
            worlds,
            crate::model_vertices::offset_25_tests::expected_worlds(case),
            "{}",
            case.name
        );
        compared += 1;
    }
    assert_eq!(compared, 15);
}

#[test]
fn two_weights_resolve_before_zero_mode_and_first_callback_mutation_is_visible() {
    let mut vertices = ModelVertices::new_invocation(12);
    let mut values = values();
    let mut host = Host::default();
    execute(
        &mut vertices,
        &instruction(0x0028, &[0xffff, 0xffff, 0xffff, 0x80, 0xc1]),
        &mut values,
        &mut host,
    )
    .unwrap();
    assert_eq!(host.values, [0]);
    assert_eq!(values.register(1).unwrap(), Some(0x8000));
    assert!(vertices.slots.iter().all(|slot| slot.world.is_none()));
}

#[test]
fn forced_custom_runs_under_original_clear_mode_and_failure_keeps_callback_prefix() {
    let mut vertices = ModelVertices::new_invocation(12);
    let mut host = Host {
        custom_fail: true,
        ..Host::default()
    };
    assert!(matches!(
        execute(
            &mut vertices,
            &instruction(0x022d, &[4, 7]),
            &mut values(),
            &mut host
        ),
        Err(ModelVertexError::Custom(ModelCustomVertexError::Host(
            "actual callback stop"
        )))
    ));
    assert_eq!(host.custom, [(7, vec![])]);
    assert_eq!(vertices.world(0).unwrap(), [77, 88, 99]);
    assert!(vertices.world(4).is_err());
    assert!(vertices.stopped());
    assert!(matches!(
        execute(
            &mut vertices,
            &instruction(0x0120, &[0]),
            &mut values(),
            &mut host
        ),
        Err(ModelVertexError::Fault(ModelVertexFault::Stopped))
    ));
}

#[test]
fn jitter_retains_rng_and_first_world_write_before_second_source_stop() {
    let mut vertices = ModelVertices::new_invocation(12);
    vertices.slots[0].world = Some([1, 2, 3]);
    let mut host = Host {
        rng: 123,
        ..Host::default()
    };
    assert!(matches!(
        execute(
            &mut vertices,
            &instruction(0x0522, &[8, 0, 17]),
            &mut values(),
            &mut host
        ),
        Err(ModelVertexError::Fault(ModelVertexFault::UnknownWorld {
            index: 1
        }))
    ));
    let mut seed = 123u32;
    let draws: [i32; 3] = std::array::from_fn(|_| {
        seed = seed.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
        (seed >> 16) as i32
    });
    assert_eq!(host.rng, seed);
    assert_eq!(
        vertices.world(8).unwrap(),
        std::array::from_fn(|i| [1i32, 2, 3][i].wrapping_add(draws[i].wrapping_shl(16) >> 17))
    );
    assert!(vertices.world(9).is_err());
}

#[test]
fn raw_twenty_retains_full_mode_and_world_prefix_before_unowned_high_projection() {
    let mut vertices = ModelVertices::new_invocation(12);
    // Bits12/13 survive nativeF5FF; pass2 writes world then reaches required
    // projection state outside this late typed owner. The low pair succeeds.
    assert!(matches!(
        execute(
            &mut vertices,
            &instruction(0x3120, &[0]),
            &mut values(),
            &mut Host::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnsupportedOpcode { opcode: 0x20 }
        ))
    ));
    assert_eq!(vertices.world(0).unwrap(), [12, 23, 34]);
    assert_eq!(vertices.world(2).unwrap(), [12, 23, 34]);
    assert!(vertices.world(1).is_err());
}

#[test]
fn world_surface_controls_use_actual_default_binding_without_projecting_output() {
    for (opcode, expected) in [(0x052b, [10, -300, 30]), (0x052c, [170, -300, 190])] {
        let mut vertices = ModelVertices::new_invocation(12);
        vertices.slots[0].world = Some([10, 20, 30]);
        vertices.slots[1].world = Some([40, 50, 60]);
        execute(
            &mut vertices,
            &instruction(opcode, &[8, 0]),
            &mut values(),
            &mut Host::default(),
        )
        .unwrap();
        assert_eq!(vertices.world(8).unwrap(), expected);
        assert!(vertices.slots[8].projection == ModelVertexProjection::default());
        assert!(vertices.world(9).is_ok());
    }
}

#[test]
fn project_surface_keeps_full_upper_mode_and_reuses_mirror_source_after_first_pass() {
    let mut vertices = ModelVertices::new_invocation(12);
    vertices.slots[0].world = Some([10, 20, 30]);
    vertices.slots[1].world = Some([40, 50, 60]);
    execute(
        &mut vertices,
        &instruction(0x152c, &[8, 0]),
        &mut values(),
        &mut Host::default(),
    )
    .unwrap();
    assert_eq!(vertices.world(8).unwrap(), [170, -300, 190]);
    assert_eq!(vertices.world(9).unwrap(), [215, -300, 235]);
    assert_eq!(vertices.world(10).unwrap(), [215, -300, 235]);
    assert!(vertices.slots[10].projection == ModelVertexProjection::default());
}
