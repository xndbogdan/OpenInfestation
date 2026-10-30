use std::{path::Path, sync::Arc};

use inf_formats::{
    decode_model_program, ByteSpan, ModelFlags, ModelHeader, ModelIdentity, ModelVectorI16,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{ModelCallbackPolicy, SceneModelSubmission},
    menu::MenuBasis,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    parse_terrain_bank, HandlerFraming, HandlerId, HandlerRegion, TerrainCell, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

fn source_model(slots: u16, ranges: [u16; 4], vertices: &[[i16; 3]]) -> ModelEntry {
    let span = ByteSpan { start: 0, end: 0 };
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
            vertex_slot_count: slots,
            raw_vertex_count: vertices.len() as u16,
            shade_slot_count: 2,
            raw_0e: 0,
            reserved_10: 0,
            vertex_ranges: ranges,
            reserved_1a: 0,
            shade_ranges: [0, 0],
        },
        auxiliary_span: span,
        auxiliary: vec![],
        auxiliary_program: None,
        vertices_span: span,
        vertices: vertices
            .iter()
            .map(|&[x, y, z]| ModelVectorI16 { x, y, z })
            .collect(),
        normals_span: span,
        normals: vec![],
        command_span: span,
        program: decode_model_program(&[0]).unwrap(),
        name: None,
        padding: span,
    }
}

pub(super) fn instruction(command: u16, arguments: &[u16]) -> ModelInstruction {
    let mut words = vec![command];
    words.extend_from_slice(arguments);
    words.push(0);
    decode_model_program(&words).unwrap().instructions.remove(0)
}

#[derive(Default)]
pub(super) struct RecordingProjection {
    pub(super) calls: Vec<ModelVertexInput>,
    fail_at: Option<usize>,
}

impl ModelVertexProjector for RecordingProjection {
    type Error = &'static str;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        self.calls.push(input);
        output.bytes[8] = Some(0x31);
        if self.fail_at == Some(self.calls.len()) {
            return Err("projection unavailable");
        }
        // Explicit recording fixture tokens, not projected pixels/depth.
        output.write_u32(0, 0x1234_5678);
        Ok(self.calls.len() as u32)
    }
}

#[derive(Default)]
struct Callback {
    selectors: Vec<u32>,
    result: u16,
    fail: bool,
}

impl ModelValueCallback for Callback {
    type Error = &'static str;

    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.selectors.push(selector);
        values.write_register(3, 0x4321).unwrap();
        if self.fail {
            Err("value unavailable")
        } else {
            Ok(self.result)
        }
    }
}

fn values() -> ModelValues {
    ModelValues::new_invocation(Arc::new([0; 4096]))
}

fn no_custom_callback(
    _: ModelCustomVertexRequest<'_>,
    _: &mut ModelVertices,
    _: &mut Option<[i32; 3]>,
) -> Result<(), ModelCustomVertexError<Infallible>> {
    panic!("ordinary row-source policy must not reach furniture services")
}

#[test]
fn custom_2d_unsigned_row_selectors_keep_native_low_word_and_mode_policies() {
    for (selector, extras) in [
        (0, [0x0400, 0]),
        (1, [0, 0x0400]),
        (2, [4, 0]),
        (3, [4, 0]),
        (5, [0xab04, 0]),
    ] {
        for mode in [1, 2, 3, 4, 8, 12, 15] {
            let mut slots = ModelVertices::new_invocation(10);
            slots.slots[4].world = Some([0x1234_8001, 0xabcd_7fff_u32 as i32, -2]);
            slots.slots[5].world = Some([10, 20, 30]);
            let mut projection = RecordingProjection::default();
            slots
                .execute_custom_2d(
                    &instruction(0x202d | (mode << 8), &[6, selector, extras[0], extras[1]]),
                    ModelCustomVertexPolicy::UnsignedRowSource,
                    &mut projection,
                    &mut no_custom_callback,
                )
                .unwrap();
            let expected = [-32767, 32767, -2];
            for parity in 0..2 {
                let pair = (mode >> (parity * 2)) & 3;
                assert_eq!(
                    slots.slots[6 + parity].world,
                    (pair & 1 != 0).then_some(expected)
                );
                assert_eq!(
                    slots.slots[6 + parity].projection.read_u32(4).is_some(),
                    pair & 2 != 0
                );
            }
            assert_eq!(
                projection.calls,
                vec![
                    ModelVertexInput::World(expected);
                    usize::from(mode & 2 != 0) + usize::from(mode & 8 != 0)
                ]
            );
        }
    }
    // Selector2 uses all16 index bits; interpreting this as signed would
    // incorrectly read a preceding allocation instead of the native row32768.
    let mut slots = ModelVertices::new_invocation(32770);
    slots.slots[32768].world = Some([1, 2, 3]);
    slots
        .execute_custom_2d(
            &instruction(0x212d, &[0, 2, 32768, 0]),
            ModelCustomVertexPolicy::UnsignedRowSource,
            &mut RecordingProjection::default(),
            &mut no_custom_callback,
        )
        .unwrap();
    assert_eq!(slots.world(0).unwrap(), [1, 2, 3]);
}

#[test]
fn custom_2d_preserves_reached_world_writes_when_projection_fails() {
    let mut slots = ModelVertices::new_invocation(10);
    slots.slots[4].world = Some([100, 200, 300]);
    let mut projection = RecordingProjection {
        fail_at: Some(1),
        ..Default::default()
    };
    assert_eq!(
        slots.execute_custom_2d(
            &instruction(0x2f2d, &[6, 2, 4, 0]),
            ModelCustomVertexPolicy::UnsignedRowSource,
            &mut projection,
            &mut no_custom_callback,
        ),
        Err(ModelVertexError::Projection("projection unavailable"))
    );
    assert_eq!(slots.slots[6].world, Some([100, 200, 300]));
    assert_eq!(slots.slots[6].projection.bytes[8], Some(0x31));
    assert_eq!(slots.slots[7], ModelVertexSlot::default());
    assert!(slots.stopped());
}

#[test]
fn custom_2d_clear_modes_do_not_read_unknown_inputs() {
    let mut slots = ModelVertices::new_invocation(0);
    slots
        .execute_custom_2d(
            &instruction(0x002d, &[65535, 4]),
            ModelCustomVertexPolicy::UnsignedRowSource,
            &mut RecordingProjection::default(),
            &mut no_custom_callback,
        )
        .unwrap();
}

/// 0043A2D0's default arm (selectors 4 and 6..255 by low byte) returns its
/// uninitialized stack locals. CustomVertexStackResidueApproximation leaves
/// exactly the fields native writes from them unknown, calls no projector,
/// and lets the walk continue.
#[test]
fn custom_2d_stack_residue_selectors_forget_only_the_written_fields() {
    let known = ModelVertexSlot {
        world: Some([1, 2, 3]),
        projection: ModelVertexProjection {
            bytes: [Some(7); 12],
        },
    };
    for (command, selector) in [(0x012d, 6), (0x022d, 4), (0x0b2d, 0x0107), (0x0e2d, 10)] {
        let mut slots = ModelVertices::new_invocation(4);
        slots.slots = vec![known; 4];
        let mut projection = RecordingProjection::default();
        slots
            .execute_custom_2d(
                &instruction(command, &[2, selector]),
                ModelCustomVertexPolicy::UnsignedRowSource,
                &mut projection,
                &mut no_custom_callback,
            )
            .unwrap();
        assert!(projection.calls.is_empty());
        assert!(!slots.stopped());
        let modes = (command >> 8) & 15;
        for parity in 0..2 {
            let pair = (modes >> (parity * 2)) & 3;
            let slot = slots.slots[2 + parity];
            assert_eq!(slot.world, (pair & 1 == 0).then_some([1, 2, 3]));
            let tail = if pair & 2 == 0 { Some(7) } else { None };
            assert_eq!(slot.projection.bytes, [tail; 12]);
        }
        assert_eq!(slots.slots[..2], [known; 2]);
    }
}

#[test]
fn custom_2d_stack_residue_clears_the_shared_scratch_bytes() {
    let scratch = ProcessModelScratch::isolated_unknown_fixture();
    let scope = scratch.begin_draw().unwrap();
    let allocation = scratch.allocate(&scope, 4, 0).unwrap();
    for index in 0..4 {
        scratch
            .write_vertex::<24>(&allocation, index, 0, [Some(9); 24])
            .unwrap();
    }
    let mut slots = ModelVertices::in_scratch(&scratch, allocation.clone()).unwrap();
    slots
        .execute_custom_2d(
            &instruction(0x092d, &[1, 6]),
            ModelCustomVertexPolicy::UnsignedRowSource,
            &mut RecordingProjection::default(),
            &mut no_custom_callback,
        )
        .unwrap();
    // Mode pairs 1 then 2: slot 1 loses its world, slot 2 its projection.
    let raw = |index| scratch.read_vertex::<24>(&allocation, index, 0).unwrap();
    let mut forgot_world = [Some(9); 24];
    forgot_world[..12].fill(None);
    let mut forgot_tail = [Some(9); 24];
    forgot_tail[12..].fill(None);
    assert_eq!(raw(1), forgot_world);
    assert_eq!(raw(2), forgot_tail);
    assert_eq!(raw(0), [Some(9); 24]);
    assert_eq!(raw(3), [Some(9); 24]);
    assert_eq!(slots.slots()[1].world, None);
    assert_eq!(slots.slots()[2].projection.bytes, [None; 12]);
}

#[test]
fn native_allocation_has_no_initialized_coordinates_or_projection_bytes() {
    let slots = ModelVertices::new_invocation(6);
    assert_eq!(slots.slots(), &[ModelVertexSlot::default(); 6]);
    assert_eq!(
        slots.world(0),
        Err(ModelVertexFault::UnknownWorld { index: 0 })
    );
    assert_eq!(
        slots.slot(-1),
        Err(ModelVertexFault::InvalidSlot {
            index: -1,
            count: 6
        })
    );
}

#[test]
fn root_preprocess_uses_distinct_ranges_and_reflects_source_x_before_world_basis() {
    let model = source_model(
        8,
        [2, 4, 6, 8],
        &[[10, 11, 12], [20, 21, 22], [-32768, 31, 32], [40, 41, 42]],
    );
    let mut context = ModelContext::new([100, 200, 300], MenuBasis::INITIAL);
    // World X receives local Y, world Y receives local X.
    context.basis.words[1] = 0;
    context.basis.words[2] = 1.0_f32.to_bits();
    context.basis.words[4] = 1.0_f32.to_bits();
    context.basis.words[5] = 0;
    let mut slots = ModelVertices::new_invocation(8);
    let mut projection = RecordingProjection::default();
    slots.initialize(&model, &context, &mut projection).unwrap();
    assert_eq!(
        projection.calls,
        [
            ModelVertexInput::Local([10, 11, 12]),
            ModelVertexInput::Local([20, 21, 22]),
            ModelVertexInput::Local([-32768, 31, 32]),
            ModelVertexInput::Local([32768, 31, 32]),
            ModelVertexInput::Local([40, 41, 42]),
            ModelVertexInput::Local([-40, 41, 42]),
        ]
    );
    assert_eq!(slots.world(2), Ok([121, 220, 322]));
    assert_eq!(slots.world(4), Ok([131, -32568, 332]));
    assert_eq!(slots.world(5), Ok([131, 32968, 332]));
    for index in [0, 1, 3, 6, 7] {
        assert_eq!(slots.slot(index).unwrap().world, None);
    }
    assert_eq!(slots.slot(1).unwrap(), &ModelVertexSlot::default());
    let output = &slots.slot(4).unwrap().projection;
    assert_eq!(output.read_u32(0), Some(0x1234_5678));
    assert_eq!(output.read_u32(4), Some(3));
    assert_eq!(output.bytes[8..], [Some(0x31), None, None, None]);
}

#[test]
fn world_transform_retains_x87_order_rounding_low_dword_and_wrapping_translation() {
    let mut context = ModelContext::new([0; 3], MenuBasis::INITIAL);
    context.basis.words[1] = 0.5_f32.to_bits();
    assert_eq!(transform_world(&context, [1, 0, 0]), [0, 0, 0]);
    assert_eq!(transform_world(&context, [3, 0, 0]), [2, 0, 0]);
    assert_eq!(transform_world(&context, [-3, 0, 0]), [-2, 0, 0]);

    // (z*2^54 + y*-2^54) + x*1 retains the final1. Reordered addition loses it.
    context.basis.words[1] = 1.0_f32.to_bits();
    context.basis.words[4] = (-18_014_398_509_481_984_f32).to_bits();
    context.basis.words[7] = 18_014_398_509_481_984_f32.to_bits();
    assert_eq!(transform_world(&context, [1, 1, 1])[0], 1);

    context = ModelContext::new([i32::MAX, 0, 0], MenuBasis::INITIAL);
    assert_eq!(transform_world(&context, [1, 0, 0])[0], i32::MIN);
    context.position[0] = 7;
    context.basis.words[1] = 4_294_967_296_f32.to_bits();
    assert_eq!(transform_world(&context, [1, 0, 0])[0], 7);
    context.basis.words[1] = f32::NAN.to_bits();
    assert_eq!(transform_world(&context, [1, 0, 0])[0], 7);
}

#[test]
fn midpoint_wraps_signed_sum_truncates_toward_zero_and_observes_alias_order() {
    let mut slots = ModelVertices::new_invocation(4);
    for (slot, world) in
        slots
            .slots
            .iter_mut()
            .zip([[i32::MAX, -3, 1], [100, 100, 100], [3, 0, 4], [20, 20, 20]])
    {
        slot.world = Some(world);
    }
    let mut projection = RecordingProjection::default();
    slots
        .execute(
            &instruction(0x0f23, &[1, 0, 2]),
            &mut values(),
            &mut Callback::default(),
            &mut projection,
        )
        .unwrap();
    let first = [-1_073_741_823, -1, 2];
    let second = [-536_870_901, 9, 11];
    assert_eq!(slots.world(1), Ok(first));
    assert_eq!(slots.world(2), Ok(second));
    assert_eq!(
        projection.calls,
        [
            ModelVertexInput::World(first),
            ModelVertexInput::World(second)
        ]
    );
}

#[test]
fn projection_only_midpoint_leaves_destination_world_and_unknown_tail_untouched() {
    let mut slots = ModelVertices::new_invocation(6);
    slots.slots[0].world = Some([-3, 4, 9]);
    slots.slots[2].world = Some([0, 9, 14]);
    let mut projection = RecordingProjection::default();
    slots
        .execute(
            &instruction(0x0223, &[4, 0, 2]),
            &mut values(),
            &mut Callback::default(),
            &mut projection,
        )
        .unwrap();
    assert_eq!(projection.calls, [ModelVertexInput::World([-1, 6, 11])]);
    assert_eq!(slots.slots[4].world, None);
    assert_eq!(slots.slots[4].projection.bytes[9..], [None; 3]);
    assert_eq!(slots.slots[5], ModelVertexSlot::default());
}

#[test]
fn interpolation_uses_native_even_step_signed_product_not_float_lerp() {
    assert_eq!(interpolate(100, 103, 32768), 100);
    assert_eq!(interpolate(100, 97, 32768), 98);
    assert_eq!(interpolate(100, 104, 32768), 102);
    assert_eq!(interpolate(i32::MAX, i32::MIN, 65535), i32::MAX);
    for left in [i32::MIN, -100, 0, 99, i32::MAX] {
        for right in [i32::MIN, -17, 0, 102, i32::MAX] {
            for weight in [0, 1, 16384, 32768, 49152, 65535] {
                let product =
                    i64::from(right.wrapping_sub(left)) * i64::from(i32::from(weight) << 14);
                let low = product as u32;
                let high = (product >> 32) as u32;
                let rcl = high.wrapping_shl(1) | (low >> 31);
                let expected = left.wrapping_add((rcl as i32).wrapping_mul(2));
                assert_eq!(interpolate(left, right, weight), expected);
            }
        }
    }
}

#[test]
fn zero_output_interpolation_still_calls_live_packed_callback() {
    let mut slots = ModelVertices::new_invocation(0);
    let mut values = values();
    let mut callback = Callback::default();
    let mut projection = RecordingProjection::default();
    slots
        .execute(
            &instruction(0x0026, &[65535, 65535, 65535, 0x80]),
            &mut values,
            &mut callback,
            &mut projection,
        )
        .unwrap();
    assert_eq!(callback.selectors, [0]);
    assert_eq!(values.register(3), Ok(Some(0x4321)));
    assert!(projection.calls.is_empty());
}

#[test]
fn second_pair_only_xors_sources_without_reading_the_skipped_first_pair() {
    let mut slots = ModelVertices::new_invocation(6);
    slots.slots[1].world = Some([4, 8, 12]);
    slots.slots[3].world = Some([8, 12, 16]);
    let mut projection = RecordingProjection::default();
    slots
        .execute(
            &instruction(0x0823, &[4, 0, 2]),
            &mut values(),
            &mut Callback::default(),
            &mut projection,
        )
        .unwrap();
    assert_eq!(projection.calls, [ModelVertexInput::World([6, 10, 14])]);
    assert_eq!(slots.slots[4], ModelVertexSlot::default());
    assert_eq!(slots.slots[5].world, None);
    assert_eq!(slots.slots[5].projection.read_u32(4), Some(1));
}

#[test]
fn failed_value_callback_retains_its_register_write_without_touching_vertices() {
    let mut slots = ModelVertices::new_invocation(6);
    let mut values = values();
    let mut callback = Callback {
        fail: true,
        ..Callback::default()
    };
    let mut projection = RecordingProjection::default();
    assert_eq!(
        slots.execute(
            &instruction(0x0326, &[4, 0, 2, 0x80]),
            &mut values,
            &mut callback,
            &mut projection
        ),
        Err(ModelVertexError::Value(ModelValueError::Callback(
            "value unavailable"
        )))
    );
    assert_eq!(values.register(3), Ok(Some(0x4321)));
    assert!(values.stopped());
    assert!(slots.stopped());
    assert_eq!(slots.slots(), &[ModelVertexSlot::default(); 6]);
    assert!(projection.calls.is_empty());
}

#[test]
fn interpolation_callback_then_pair_projection_preserves_prior_writes_on_failure() {
    let mut slots = ModelVertices::new_invocation(6);
    for index in 0..4 {
        slots.slots[index].world = Some([index as i32 * 10; 3]);
    }
    let mut values = values();
    let mut callback = Callback {
        result: 32768,
        ..Callback::default()
    };
    let mut projection = RecordingProjection {
        fail_at: Some(2),
        ..RecordingProjection::default()
    };
    let command = instruction(0x0f26, &[4, 0, 2, 0x80]);
    assert_eq!(
        slots.execute(&command, &mut values, &mut callback, &mut projection),
        Err(ModelVertexError::Projection("projection unavailable"))
    );
    assert_eq!(slots.world(4), Ok([10; 3]));
    assert_eq!(slots.world(5), Ok([20; 3]));
    assert_eq!(slots.slots[4].projection.read_u32(4), Some(1));
    assert_eq!(slots.slots[5].projection.read_u32(4), None);
    assert_eq!(slots.slots[5].projection.bytes[8], Some(0x31));
    assert!(slots.stopped());
    assert_eq!(
        slots.execute(&command, &mut values, &mut callback, &mut projection),
        Err(ModelVertexError::Fault(ModelVertexFault::Stopped))
    );
    assert_eq!(callback.selectors, [0]);
    assert_eq!(projection.calls.len(), 2);
}

#[test]
fn failed_preprojection_never_reaches_world_population() {
    let model = source_model(4, [0, 0, 4, 4], &[[1, 2, 3], [4, 5, 6]]);
    let mut slots = ModelVertices::new_invocation(4);
    let mut projection = RecordingProjection {
        fail_at: Some(2),
        ..RecordingProjection::default()
    };
    assert_eq!(
        slots.initialize(
            &model,
            &ModelContext::new([0; 3], MenuBasis::INITIAL),
            &mut projection
        ),
        Err(ModelVertexError::Projection("projection unavailable"))
    );
    assert!(slots.slots.iter().all(|slot| slot.world.is_none()));
    assert_eq!(slots.slots[0].projection.read_u32(4), Some(1));
    assert_eq!(slots.slots[1].projection.bytes[8], Some(0x31));
    assert!(slots.stopped());
}

#[test]
fn invalid_raw_ranges_and_unknown_sources_fail_without_fabricating_coordinates() {
    let model = source_model(4, [0, 0, 4, 4], &[[1, 2, 3]]);
    let mut slots = ModelVertices::new_invocation(4);
    let mut projection = RecordingProjection::default();
    assert_eq!(
        slots.initialize(
            &model,
            &ModelContext::new([0; 3], MenuBasis::INITIAL),
            &mut projection
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInitialization
        ))
    );
    assert!(projection.calls.is_empty());
    let mut slots = ModelVertices::new_invocation(4);
    assert_eq!(
        slots.execute(
            &instruction(0x0123, &[2, 0, 1]),
            &mut values(),
            &mut Callback::default(),
            &mut projection
        ),
        Err(ModelVertexError::Fault(ModelVertexFault::UnknownWorld {
            index: 0
        }))
    );
    assert!(slots.slots.iter().all(|slot| slot.world.is_none()));
}

struct SceneCallback(SceneModelSubmission);
impl ModelValueCallback for SceneCallback {
    type Error = &'static str;

    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        self.0
            .callback_value(selector)
            .ok_or("pure virtual callback")
    }
}

fn resident_model(runtime: &ResidentRuntime, id: u16) -> &ModelEntry {
    let resource = runtime.resource(8, id).unwrap();
    let HandlerResources::Models(bank) =
        &resource.overlay.handlers()[resource.registration.region_index]
    else {
        panic!("wrong model registration");
    };
    &bank.entries[usize::from(resource.registration.local_index)]
}

#[test]
fn retail_nine_roots_and_thirteen_model_closure_initialize_and_run_vertex_operations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let table = Arc::new(assets.sine_table().values);
    let roots = [744, 749, 754, 756, 745, 748, 747, 755, 757];
    let children = [746, 751, 752, 753];
    for clock_value in [0, 1, 31, 63, 64, 65535] {
        let mut clock = AnimationClock::default();
        clock.advance(clock_value * 20000);
        let mut midpoint_count = 0;
        let mut interpolation_count = 0;
        for id in roots.into_iter().chain(children) {
            let model = resident_model(&runtime, id);
            let context = ModelContext::new([900, -800, 700], MenuBasis::INITIAL);
            let mut slots = ModelVertices::new_invocation(model.header.vertex_slot_count);
            let mut projection = RecordingProjection::default();
            slots.initialize(model, &context, &mut projection).unwrap();
            let [a, b, c, d] = model.header.vertex_ranges;
            assert_eq!(projection.calls.len(), usize::from(b / 2 + d - b));
            for index in 0..model.header.vertex_slot_count {
                let expected = index >= a && index < c && (index % 2 == 0 || index >= b);
                assert_eq!(
                    slots.slots[usize::from(index)].world.is_some(),
                    expected,
                    "model{id} slot{index}"
                );
            }
            let mut values = ModelValues::new_invocation(Arc::clone(&table));
            let mut callback = SceneCallback(SceneModelSubmission {
                primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
                failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,

                surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
                model_slot: id as i16,
                position: context.position,
                basis: context.basis,
                animation: clock,
                policy: ModelCallbackPolicy::Frontend,
            });
            // This focused test executes register/vertex records only. The
            // program walker owns branches, child contexts and primitive order.
            for record in &model.program.instructions {
                match record.raw_command as u8 {
                    0xf0..=0xff => {
                        values.execute_register(record, &mut callback).unwrap();
                    }
                    0x23 | 0x26 => {
                        slots
                            .execute(record, &mut values, &mut callback, &mut projection)
                            .unwrap();
                        if record.raw_command as u8 == 0x23 {
                            midpoint_count += 1;
                        } else {
                            interpolation_count += 1;
                        }
                    }
                    _ => {}
                }
            }
            assert!(!slots.stopped());
            if id == 751 {
                for index in [10, 12, 14, 16] {
                    assert_eq!(slots.slot(index).unwrap().world, None);
                    assert!(slots.slot(index).unwrap().projection.read_u32(4).is_some());
                }
            }
            if id == 746 {
                // Authored0026 at destination46 evaluates its operand but has
                // no write bits. Eight0226 records project their outputs only.
                assert_eq!(slots.slot(46).unwrap(), &ModelVertexSlot::default());
                for index in [38, 40, 42, 44, 48, 50, 52, 54] {
                    assert_eq!(slots.slot(index).unwrap().world, None);
                    assert!(slots.slot(index).unwrap().projection.read_u32(4).is_some());
                }
            }
        }
        assert_eq!((midpoint_count, interpolation_count), (4, 9));
    }
}

/// Flat synthetic terrain: every cell carries the same height with no
/// sparse records, so sampling is exact everywhere. Mirrors the
/// terrain-runtime fixture without importing it. `plane` sets the H12
/// datum independently of heights.
pub(super) fn flat_terrain(height: i8, plane: i32) -> TerrainRuntime {
    let cells = vec![TerrainCell::from_raw(height as u8 as u16); TERRAIN_CELL_COUNT];
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[0..4].copy_from_slice(&plane.to_le_bytes());
    for (index, cell) in cells.iter().enumerate() {
        let offset = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&cell.raw().to_le_bytes());
    }
    for (index, cell) in cells.iter().enumerate() {
        let offset = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&cell.raw().to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let mut bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let source = bank.entries.remove(0);
    TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

#[test]
fn opcode_2b_grounds_vertices_to_sampled_terrain() {
    // Flat height 10 at Units32 shift is exactly 320 everywhere.
    let terrain = flat_terrain(10, 0);
    assert_eq!(terrain.height_at([1000, 2000]), 320);
    let mut slots = ModelVertices::new_invocation(6);
    slots.slots[0].world = Some([1000, 9999, 2000]);
    slots.slots[1].world = Some([1100, 9999, 2100]);
    let mut projection = RecordingProjection::default();
    // Mode 1 writes grounded XYZ without projecting.
    slots
        .execute_ground_2b(
            surface_request(0x012b, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[4].world, Some([1000, 320, 2000]));
    assert!(projection.calls.is_empty());
    // Mode 2 refreshes only the projection DWORD from grounded XYZ.
    slots
        .execute_ground_2b(
            surface_request(0x022b, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[4].world, Some([1000, 320, 2000]));
    assert_eq!(
        projection.calls,
        [ModelVertexInput::World([1000, 320, 2000])]
    );
    assert_eq!(slots.slots[4].projection.read_u32(4), Some(1));
    // Mode 4 reaches only the second pair with xored sources.
    slots
        .execute_ground_2b(
            surface_request(0x042b, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[5].world, Some([1100, 320, 2100]));
    assert_eq!(slots.slots[4].world, Some([1000, 320, 2000]));
    // Mode 0 consumes the record without effect.
    slots
        .execute_ground_2b(
            surface_request(0x002b, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    // Unset sources fault instead of grounding garbage. Each fault trips
    // the stopped containment, so fault cases use fresh invocations.
    let mut fault_slots = ModelVertices::new_invocation(6);
    fault_slots.slots[0].world = Some([1000, 9999, 2000]);
    assert_eq!(
        fault_slots.execute_ground_2b(
            surface_request(0x012b, 4, 2),
            &mut project_inputs(Some(&terrain)),
            &mut projection
        ),
        Err(ModelVertexError::Fault(ModelVertexFault::UnknownWorld {
            index: 2
        }))
    );
    // Selected terrain callbacks require backing before reading the source.
    let mut no_terrain_slots = ModelVertices::new_invocation(6);
    no_terrain_slots.slots[0].world = Some([1000, 9999, 2000]);
    assert_eq!(
        no_terrain_slots.execute_ground_2b(
            surface_request(0x012b, 4, 0),
            &mut project_inputs(None),
            &mut projection
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::MissingSurfaceTerrain {
                callback: ModelSurfaceCallback::Ground,
            }
        ))
    );
}

pub(super) fn project_inputs<'a>(
    terrain: Option<&'a TerrainRuntime>,
) -> ModelSurfaceCallbackInputs<'a> {
    ModelSurfaceCallbackInputs {
        binding: ModelSurfaceBinding::Terrain {
            terrain,
            world_flags: 0,
            wave_clock: 0,
            renderer_xyz: [1, 2, 3],
        },
        table: None,
    }
}

pub(super) fn surface_request(
    command: u16,
    destination: i16,
    source: i16,
) -> ModelSurfaceVertexRequest {
    ModelSurfaceVertexRequest {
        command: ModelVertexCommand::RendererWord(command),
        destination,
        source,
    }
}

#[test]
fn opcode_2c_projects_vertices_through_terrain_without_a_mode_mask() {
    // Flat height 7 is 224; plane 0x10000 gives datum 256.
    let terrain = flat_terrain(7, 0x10000);
    let mut slots = ModelVertices::new_invocation(6);
    slots.slots[0].world = Some([1000, 260, 2000]);
    slots.slots[1].world = Some([1100, 300, 2100]);
    let mut projection = RecordingProjection::default();
    // Copy path (near datum) writes the context without factors.
    slots
        .execute_project_2c(
            surface_request(0x012c, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[4].world, Some([1, 2, 3]));
    assert!(projection.calls.is_empty());
    // Bit1-only refreshes the projection DWORD from the copied value.
    slots
        .execute_project_2c(
            surface_request(0x022c, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[4].world, Some([1, 2, 3]));
    assert_eq!(projection.calls, [ModelVertexInput::World([1, 2, 3])]);
    assert_eq!(
        slots.slots[4].projection.read_u32(4),
        Some(1_u32.wrapping_sub(20))
    );
    // Unmasked mode reaches later passes with sticky parity.
    let mut wide_slots = ModelVertices::new_invocation(7);
    wide_slots.slots[0].world = Some([1000, 260, 2000]);
    wide_slots.slots[1].world = Some([1100, 262, 2100]);
    wide_slots
        .execute_project_2c(
            surface_request(0x102c, 4, 0),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(wide_slots.slots[4].world, None);
    assert_eq!(wide_slots.slots[5].world, None);
    assert_eq!(wide_slots.slots[6].world, Some([1, 2, 3]));
    // Mode 0 consumes the record without effect.
    slots
        .execute_project_2c(
            surface_request(0x002c, -32768, 32767),
            &mut project_inputs(None),
            &mut projection,
        )
        .unwrap();
    // Tail path runs end to end on terrain-carried factors: the
    // 0x2C stop for slot 1DA-class vertices is gone.
    // (This would trip stopped containment after a fault, so it runs
    // before the fault cases below.)
    slots
        .execute_project_2c(
            surface_request(0x012c, 4, 1),
            &mut project_inputs(Some(&terrain)),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[4].world, Some([1056, 256, 2056]));
    // Other opcodes and missing terrain keep prior boundaries, each on
    // fresh slots since faults trip the stopped containment.
    for (arguments, terrain, expected) in [
        (
            surface_request(0x012b, 4, 0),
            Some(&terrain),
            Err(ModelVertexError::Fault(
                ModelVertexFault::UnsupportedOpcode { opcode: 0x2b },
            )),
        ),
        (
            surface_request(0x012c, 4, 0),
            None,
            Err(ModelVertexError::Fault(
                ModelVertexFault::MissingSurfaceTerrain {
                    callback: ModelSurfaceCallback::Project,
                },
            )),
        ),
        (
            surface_request(0x012c, 4, 2),
            Some(&terrain),
            Err(ModelVertexError::Fault(ModelVertexFault::UnknownWorld {
                index: 2,
            })),
        ),
    ] {
        let mut fault_slots = ModelVertices::new_invocation(6);
        fault_slots.slots[0].world = Some([1000, 260, 2000]);
        assert_eq!(
            fault_slots.execute_project_2c(
                arguments,
                &mut project_inputs(terrain),
                &mut projection
            ),
            expected
        );
    }
}

fn copy_parent() -> ModelVertices {
    let mut parent = ModelVertices::new_invocation(16);
    for (index, slot) in parent.slots.iter_mut().enumerate() {
        let base = index as i32 * 100;
        slot.world = Some([base + 1, base + 2, base + 3]);
        slot.projection.write_u32(0, (base + 4) as u32);
        slot.projection.write_u32(4, (base + 5) as u32);
        slot.projection.bytes[8] = Some((base + 6) as u8);
        slot.projection.bytes[9] = Some((base + 7) as u8);
    }
    parent
}

#[test]
fn copy_2a_plain_advance_writes_nothing() {
    let mut slots = ModelVertices::new_invocation(32);
    let parent = copy_parent();
    slots
        .execute_copy_2a(
            &instruction(0x002a, &[24, 1]),
            &[14, 12, 10],
            Some(&parent),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default(),
        )
        .unwrap();
    assert_eq!(slots.slots[24], ModelVertexSlot::default());
}

#[test]
fn copy_2a_first_group_copies_tabled_parent_slot() {
    // 554's first op: 0x0A2A [24,1] also sets bit 11, so both copies
    // land: slot 24 from parent 12, slot 25 from parent 13.
    let mut slots = ModelVertices::new_invocation(32);
    let parent = copy_parent();
    slots
        .execute_copy_2a(
            &instruction(0x0a2a, &[24, 1]),
            &[14, 12, 10],
            Some(&parent),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default(),
        )
        .unwrap();
    assert_eq!(slots.slots[24], parent.slots[12]);
    assert_eq!(slots.slots[24].world, Some([1201, 1202, 1203]));
    assert_eq!(slots.slots[25], parent.slots[13]);
    assert_eq!(slots.slots[26], ModelVertexSlot::default());
}

#[test]
fn copy_2a_groups_are_independent() {
    // Bits 8-9 alone copy one slot; bits 10-11 alone copy the
    // mirror-indexed slot into destination + 1.
    let mut slots = ModelVertices::new_invocation(32);
    let parent = copy_parent();
    slots
        .execute_copy_2a(
            &instruction(0x022a, &[24, 1]),
            &[14, 12, 10],
            Some(&parent),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default(),
        )
        .unwrap();
    assert_eq!(slots.slots[24], parent.slots[12]);
    assert_eq!(slots.slots[25], ModelVertexSlot::default());
    let mut slots = ModelVertices::new_invocation(32);
    slots
        .execute_copy_2a(
            &instruction(0x082a, &[26, 2]),
            &[14, 12, 10],
            Some(&parent),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default(),
        )
        .unwrap();
    assert_eq!(slots.slots[26], ModelVertexSlot::default());
    // Table 2 -> parent 10, mirror ^1 -> 11, into destination + 1.
    assert_eq!(slots.slots[27], parent.slots[11]);
}

#[test]
fn copy_2a_mirror_side_reads_the_odd_slot_of_a_mirror_only_copy() {
    let parent = copy_parent();
    let copy = |command: u16, slot: u16, mirror| {
        let mut slots = ModelVertices::new_invocation(32);
        slots
            .execute_copy_2a(
                &instruction(command, &[26, 1]),
                &[0, slot],
                Some(&parent),
                mirror,
                [0; 3],
                &mut RecordingProjection::default(),
            )
            .unwrap();
        slots
    };
    // An even slot's mirror copy is its partner either way.
    for mirror in [MirrorCopySource::Partner, MirrorCopySource::MirrorSide] {
        assert_eq!(copy(0x082a, 12, mirror).slots[27], parent.slots[13]);
    }
    // An odd one: retail reads its even partner, the fix the slot itself.
    let retail = copy(0x082a, 13, MirrorCopySource::Partner);
    assert_eq!(retail.slots[27], parent.slots[12]);
    let fixed = copy(0x082a, 13, MirrorCopySource::MirrorSide);
    assert_eq!(fixed.slots[27], parent.slots[13]);
    assert_eq!(fixed.slots[26], ModelVertexSlot::default());
    // A copy of both slots keeps retail's pair, swapped or not.
    let both = copy(0x0a2a, 13, MirrorCopySource::MirrorSide);
    assert_eq!(both.slots[26], parent.slots[13]);
    assert_eq!(both.slots[27], parent.slots[12]);
}

#[test]
fn copy_2a_rejects_bad_shapes_and_bounds() {
    // Each case runs on a fresh invocation: the first failure stops
    // its vertices, like a retail invocation.
    let fresh = ModelVertices::new_invocation(32);
    // Wrong opcode never reaches the engine.
    assert!(matches!(
        fresh.clone().execute_copy_2a(
            &instruction(0x002b, &[24, 1]),
            &[14, 12, 10],
            Some(&copy_parent()),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnsupportedOpcode { opcode: 0x2b }
        ))
    ));
    // Table index past the state words fails closed (retail would
    // read out of bounds).
    assert!(matches!(
        fresh.clone().execute_copy_2a(
            &instruction(0x0a2a, &[24, 9]),
            &[14, 12, 10],
            Some(&copy_parent()),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x2a }
        ))
    ));
    // Parent slot past the parent allocation fails closed.
    assert!(matches!(
        fresh.clone().execute_copy_2a(
            &instruction(0x0a2a, &[24, 1]),
            &[0, 99],
            Some(&copy_parent()),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidSlot { .. }
        ))
    ));
    // Destination past the current allocation fails closed.
    assert!(matches!(
        fresh.clone().execute_copy_2a(
            &instruction(0x0a2a, &[40, 1]),
            &[14, 12, 10],
            Some(&copy_parent()),
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidSlot { .. }
        ))
    ));
    // A root destination past the allocation also fails closed.
    assert!(matches!(
        fresh.clone().execute_copy_2a(
            &instruction(0x0a2a, &[40, 1]),
            &[14, 12, 10],
            None,
            MirrorCopySource::Partner,
            [0; 3],
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidSlot { .. }
        ))
    ));
}

fn jitter_slots() -> ModelVertices {
    // Source pair at 4/5 plus scratch destinations, mirroring the
    // copy-parent seeding above.
    let mut slots = ModelVertices::new_invocation(16);
    slots.slots[4].world = Some([1000, 2000, 3000]);
    slots.slots[5].world = Some([-1000, -2000, -3000]);
    slots
}

fn jittered_triple(base: [i32; 3], seed: u32) -> ([i32; 3], u32) {
    // The 0x22-specific composition over the shared LCG owner: three
    // draws in x/y/z order, each `(draw << 16) >> shift` added
    // wrapping. The LCG itself is owned in `name_entry_tests`.
    let mut rng = seed;
    let shift = 25;
    let triple = std::array::from_fn(|component| {
        let draw = crate::name_entry::name_entry_rng_next(&mut rng);
        let jitter = (draw as i32).wrapping_shl(16).wrapping_shr(shift);
        base[component].wrapping_add(jitter)
    });
    (triple, rng)
}

#[test]
fn jitter_22_plain_advance_writes_nothing_and_draws_no_rng() {
    // Gate child 63's six `0x0022` ops: exact advance, no RNG custody
    // needed, destinations stay unknown.
    let mut slots = jitter_slots();
    let mut rng = 0x1234_5678;
    let mut projection = RecordingProjection::default();
    slots
        .execute_jitter_22(
            &instruction(0x0022, &[10, 4, 27]),
            Some(&mut rng),
            &mut projection,
        )
        .unwrap();
    assert_eq!(slots.slots[10], ModelVertexSlot::default());
    assert_eq!(slots.slots[11], ModelVertexSlot::default());
    assert_eq!(rng, 0x1234_5678);
    assert!(projection.calls.is_empty());
}

#[test]
fn jitter_22_set_bits_without_rng_stay_open() {
    // Frontend scenes leave the process word unset and keep the
    // previous unsupported boundary.
    let mut slots = jitter_slots();
    let mut projection = RecordingProjection::default();
    assert!(matches!(
        slots.execute_jitter_22(&instruction(0x0222, &[10, 4, 25]), None, &mut projection),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnsupportedOpcode { opcode: 0x22 }
        ))
    ));
    assert_eq!(slots.slots[10], ModelVertexSlot::default());
    assert!(projection.calls.is_empty());
    assert!(slots.stopped());
}

#[test]
fn jitter_22_project_only_pass_draws_three_and_stores_depth_only() {
    // Gate child 63's `0x0222 [120, 44, 25]`, downsized: bit 1
    // projects without storing world, but still consumes all three
    // draws (retail determinism for later ops).
    let mut slots = jitter_slots();
    let mut rng = 0x1234_5678;
    let mut projection = RecordingProjection::default();
    let (expected, advanced) = jittered_triple([1000, 2000, 3000], rng);
    slots
        .execute_jitter_22(
            &instruction(0x0222, &[10, 4, 25]),
            Some(&mut rng),
            &mut projection,
        )
        .unwrap();
    assert_eq!(rng, advanced);
    assert_eq!(projection.calls, [ModelVertexInput::World(expected)]);
    assert_eq!(slots.slots[10].world, None);
    assert_eq!(slots.slots[10].projection.read_u32(0), Some(0x1234_5678));
    assert_eq!(slots.slots[10].projection.read_u32(4), Some(1));
    assert_eq!(slots.slots[11], ModelVertexSlot::default());
}

#[test]
fn jitter_22_full_nibble_runs_both_passes_with_xor_sources() {
    // Nibble 0xF: pass 0 stores + projects dest from src, pass 1 the
    // same from src ^ 1 into dest + 1; six draws in order.
    let mut slots = jitter_slots();
    let mut rng = 0xabcd_ef01;
    let mut projection = RecordingProjection::default();
    let (first, mid) = jittered_triple([1000, 2000, 3000], rng);
    let (second, advanced) = jittered_triple([-1000, -2000, -3000], mid);
    slots
        .execute_jitter_22(
            &instruction(0x0f22, &[10, 4, 25]),
            Some(&mut rng),
            &mut projection,
        )
        .unwrap();
    assert_eq!(rng, advanced);
    assert_eq!(
        projection.calls,
        [
            ModelVertexInput::World(first),
            ModelVertexInput::World(second)
        ]
    );
    assert_eq!(slots.slots[10].world, Some(first));
    assert_eq!(slots.slots[11].world, Some(second));
    assert_eq!(slots.slots[12], ModelVertexSlot::default());
}

#[test]
fn jitter_22_store_only_pass_skips_projection_but_keeps_draws() {
    // Bit 0 alone stores the jittered world with no project call;
    // the three draws are still consumed.
    let mut slots = jitter_slots();
    let mut rng = 0x0bad_f00d;
    let mut projection = RecordingProjection::default();
    let (expected, advanced) = jittered_triple([1000, 2000, 3000], rng);
    slots
        .execute_jitter_22(
            &instruction(0x0122, &[10, 4, 25]),
            Some(&mut rng),
            &mut projection,
        )
        .unwrap();
    assert_eq!(rng, advanced);
    assert!(projection.calls.is_empty());
    assert_eq!(slots.slots[10].world, Some(expected));
}

#[test]
fn jitter_22_rejects_bad_shapes_and_bounds() {
    let fresh = ModelVertices::new_invocation(16);
    let mut projection = RecordingProjection::default();
    // Wrong opcode never reaches the engine.
    assert!(matches!(
        fresh.clone().execute_jitter_22(
            &instruction(0x0023, &[10, 4, 25]),
            Some(&mut 0x1234_5678),
            &mut projection
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnsupportedOpcode { opcode: 0x23 }
        ))
    ));
    // Two argument words fail closed (retail reads three). The program
    // decoder always consumes three words for 0x22, so the short shape
    // is built directly.
    let short = ModelInstruction {
        span: ByteSpan { start: 0, end: 0 },
        raw_command: 0x0222,
        kind: ModelInstructionKind::UnresolvedDynamic(inf_formats::ModelUnresolvedCommand {
            arguments: vec![10, 4],
            dependencies: vec![],
        }),
    };
    assert!(matches!(
        fresh.clone().execute_jitter_22(
            &short,
            Some(&mut 0x1234_5678),
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidInstruction { opcode: 0x22 }
        ))
    ));
    // Unknown source world fails closed (retail would read garbage).
    assert!(matches!(
        fresh.clone().execute_jitter_22(
            &instruction(0x0122, &[10, 7, 25]),
            Some(&mut 0x1234_5678),
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::UnknownWorld { .. }
        ))
    ));
    // Destination past the allocation fails closed.
    assert!(matches!(
        jitter_slots().execute_jitter_22(
            &instruction(0x0122, &[40, 4, 25]),
            Some(&mut 0x1234_5678),
            &mut RecordingProjection::default()
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::InvalidSlot { .. }
        ))
    ));
}
