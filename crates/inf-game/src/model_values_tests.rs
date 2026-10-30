use std::{collections::BTreeMap, path::Path};

use inf_formats::{decode_model_program, ModelEntry};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{ModelCallbackPolicy, SceneModelSubmission},
    menu::MenuBasis,
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime,
};

fn table() -> Arc<[u16; 4096]> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    Arc::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

fn instruction(opcode: u16, arguments: [u16; 3]) -> ModelInstruction {
    decode_model_program(&[opcode, arguments[0], arguments[1], arguments[2], 0])
        .unwrap()
        .instructions
        .remove(0)
}

struct NoCallback;

impl ModelValueCallback for NoCallback {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        Err("unexpected callback")
    }
}

#[test]
fn immediate_decoder_matches_native_word_rotation_and_inversion() {
    let mut values = ModelValues::new_invocation(table());
    for raw in 0..=u16::MAX {
        if raw & 0x80 != 0 {
            continue;
        }
        let shift = u32::from((raw >> 2) & 15);
        let value = u32::from(raw & 0xff03);
        let rotated = if shift == 0 {
            value
        } else {
            (value << shift) | (value >> (16 - shift))
        };
        let expected = if raw & 0x40 != 0 { !rotated } else { rotated } as u16;
        assert_eq!(values.evaluate_packed(raw, &mut NoCallback), Ok(expected));
    }
}

#[test]
fn unknown_and_context_aliases_never_become_neutral_registers() {
    for index in 0..22 {
        let mut values = ModelValues::new_invocation(table());
        assert_eq!(values.register(index), Ok(None));
        assert_eq!(
            values.evaluate_packed(0xc0 | index as u16, &mut NoCallback),
            Err(ModelValueError::Fault(
                ModelValueFault::UninitializedRegister { index: index as u8 }
            ))
        );
        assert!(values.stopped());
    }
    for index in 22..64 {
        let mut values = ModelValues::new_invocation(table());
        assert_eq!(
            values.evaluate_packed(0xc0 | index, &mut NoCallback),
            Err(ModelValueError::Fault(
                ModelValueFault::ContextAliasedRegister { index: index as u8 }
            ))
        );
    }
    for index in [-1, 64, i16::MAX] {
        let mut values = ModelValues::new_invocation(table());
        assert_eq!(
            values.write_register(index, 1),
            Err(ModelValueFault::InvalidRegister { index })
        );
    }
}

#[test]
fn child_copies_only_first_four_and_has_no_copy_back_or_zero_fill() {
    let mut parent = ModelValues::new_invocation(table());
    parent.write_register(0, 100).unwrap();
    parent.write_register(3, 300).unwrap();
    parent.write_register(4, 400).unwrap();
    let mut child = parent.child_invocation().unwrap();
    assert_eq!(child.register(0), Ok(Some(100)));
    assert_eq!(child.register(1), Ok(None));
    assert_eq!(child.register(3), Ok(Some(300)));
    assert_eq!(child.register(4), Ok(None));
    child.write_register(0, 101).unwrap();
    assert_eq!(parent.register(0), Ok(Some(100)));
    // An inline scope shares the same owner, so its writes remain visible.
    let nested = &mut parent;
    nested
        .execute_register(&instruction(0xf0, [0, 0xc0, 1]), &mut NoCallback)
        .unwrap();
    assert_eq!(parent.register(0), Ok(Some(101)));
}

#[test]
fn arithmetic_preserves_word_wrap_unsigned_bounds_and_32_bit_shift_counts() {
    let samples = [
        (0xf0, 65535, 2, 1),
        (0xf1, 0, 1, 65535),
        (0xf2, 65535, 2, 65534),
        (0xf3, 65535, 2, 32767),
        (0xf3, 17, 0, 17),
        (0xf4, 0x8001, 16, 0),
        (0xf4, 0x8001, 32, 0x8001),
        (0xf5, 1, 16, 0),
        (0xf5, 1, 31, 0),
        (0xf5, 1, 32, 1),
        (0xf6, 65535, 2, 65535),
        (0xf7, 17, 18, 17),
        (0xf7, 18, 18, 17),
        (0xf7, 0, 0, 65535),
        (0xf8, 1, 3, 21845),
        (0xf8, 65535, 1, 0),
        (0xf8, 17, 0, 17),
        (0xf9, 0x8001, 1, 0xc000),
        (0xf9, 0x8001, 16, 65535),
        (0xf9, 0x8001, 32, 0x8001),
        (0xfb, 3, 4, 0),
        (0xfb, 4, 4, 4),
        (0xfc, 4, 3, 0),
        (0xfc, 4, 4, 4),
        (0xff, 0xa55a, 0x0ff0, 0x0550),
    ];
    let mut values = ModelValues::new_invocation(table());
    for (opcode, left, right, expected) in samples {
        values.write_register(0, left).unwrap();
        values.write_register(1, right).unwrap();
        assert_eq!(
            values.execute_register(&instruction(opcode, [2, 0xc0, 0xc1]), &mut NoCallback),
            Ok(expected),
            "opcode {opcode:x}"
        );
    }
}

#[test]
fn sine_cosine_use_replicated_original_words_and_signed_product_floor() {
    let mut values = ModelValues::new_invocation(table());
    for (angle, expected) in [
        (0, 0),
        (4, 23),
        (0x1000, 25077),
        (0x4000, 65533),
        (0x8000, 0),
        (0x8004, 65512),
        (0xc000, 2),
    ] {
        values.write_register(0, 65535).unwrap();
        values.write_register(1, angle).unwrap();
        assert_eq!(
            values.execute_register(&instruction(0xfd, [2, 0xc0, 0xc1]), &mut NoCallback),
            Ok(expected)
        );
        values
            .write_register(1, angle.wrapping_sub(0x4000))
            .unwrap();
        assert_eq!(
            values.execute_register(&instruction(0xfe, [2, 0xc0, 0xc1]), &mut NoCallback),
            Ok(expected)
        );
    }
}

#[test]
fn callbacks_mutate_live_words_in_operand_order_and_fa_keeps_full_selector_sum() {
    struct Callback {
        selectors: Vec<u32>,
    }
    impl ModelValueCallback for Callback {
        type Error = &'static str;
        fn packed_value(
            &mut self,
            values: &mut ModelValues,
            selector: u32,
        ) -> Result<u16, Self::Error> {
            self.selectors.push(selector);
            match selector {
                0 => {
                    values.write_register(1, 9).unwrap();
                    Ok(3)
                }
                0x1234 => Ok(55),
                131070 => Ok(77),
                _ => Err("unexpected selector"),
            }
        }
    }
    let mut callback = Callback {
        selectors: Vec::new(),
    };
    let mut values = ModelValues::new_invocation(table());
    values.write_register(1, 2).unwrap();
    assert_eq!(
        values.execute_register(&instruction(0xf0, [2, 0x80, 0xc1]), &mut callback),
        Ok(12)
    );
    assert_eq!(values.evaluate_packed(0x12b4, &mut callback), Ok(55));
    values.write_register(0, 65535).unwrap();
    values.write_register(1, 65535).unwrap();
    assert_eq!(
        values.execute_register(&instruction(0xfa, [2, 0xc0, 0xc1]), &mut callback),
        Ok(77)
    );
    assert_eq!(callback.selectors, [0, 0x1234, 131070]);
}

#[test]
fn callback_error_preserves_prior_writes_and_stops_destination_publication() {
    struct Failing;
    impl ModelValueCallback for Failing {
        type Error = &'static str;
        fn packed_value(&mut self, values: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
            values.write_register(1, 9).unwrap();
            Err("failed")
        }
    }
    let mut values = ModelValues::new_invocation(table());
    values.write_register(2, 44).unwrap();
    assert_eq!(
        values.execute_register(&instruction(0xf0, [2, 0x80, 0xc1]), &mut Failing),
        Err(ModelValueError::Callback("failed"))
    );
    assert_eq!(values.register(1), Ok(Some(9)));
    assert_eq!(values.register(2), Ok(Some(44)));
    assert_eq!(
        values.evaluate_packed(0, &mut NoCallback),
        Err(ModelValueError::Fault(ModelValueFault::Stopped))
    );
}

#[test]
fn swallowed_nested_error_stops_outer_write_and_preserves_immediate_error_precedence() {
    struct Nested {
        outer_error: bool,
    }
    impl ModelValueCallback for Nested {
        type Error = &'static str;
        fn packed_value(&mut self, values: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
            assert!(values.evaluate_packed(0x80, &mut NoCallback).is_err());
            if self.outer_error {
                Err("outer")
            } else {
                Ok(2)
            }
        }
    }
    for outer_error in [false, true] {
        let mut values = ModelValues::new_invocation(table());
        values.write_register(2, 44).unwrap();
        let result = values.execute_register(
            &instruction(0xf0, [2, 0x80, 1]),
            &mut Nested { outer_error },
        );
        assert_eq!(
            result,
            if outer_error {
                Err(ModelValueError::Callback("outer"))
            } else {
                Err(ModelValueError::Fault(ModelValueFault::Stopped))
            }
        );
        assert_eq!(values.register(2), Ok(Some(44)));
    }
}

#[test]
fn comparison_right_callback_initializes_the_subsequent_left_register_read() {
    struct RightWriter {
        selectors: Vec<u32>,
    }
    impl ModelValueCallback for RightWriter {
        type Error = &'static str;
        fn packed_value(
            &mut self,
            values: &mut ModelValues,
            selector: u32,
        ) -> Result<u16, Self::Error> {
            self.selectors.push(selector);
            assert_eq!(selector, 0);
            assert_eq!(values.register(1), Ok(None));
            values.write_register(1, 9).unwrap();
            Ok(9)
        }
    }

    // Pin the complete original opcode16 body, including its right-first
    // packed read at004A0CA0 and left read at004A0CAC, not just an operand.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&assets.executable_bytes()[0x0a0c90..0x0a0cdb])
        ),
        "ce9dba1695669346dd3704cced1da7f8de6486da277abccfc922bdaa21ab602c"
    );
    let mut values = ModelValues::new_invocation(table());
    let mut callback = RightWriter {
        selectors: Vec::new(),
    };
    assert_eq!(
        values.branch_decision(&instruction(0x16, [7, 0xc1, 0x80]), &mut callback),
        Ok(ModelValueBranch::FallThrough)
    );
    assert_eq!(callback.selectors, [0]);
    assert_eq!(values.register(1), Ok(Some(9)));
    assert!(!values.stopped());
}

#[test]
fn comparison_unknown_right_stops_before_left_callback_and_its_writes() {
    struct LeftWriter {
        selectors: Vec<u32>,
    }
    impl ModelValueCallback for LeftWriter {
        type Error = &'static str;
        fn packed_value(
            &mut self,
            values: &mut ModelValues,
            selector: u32,
        ) -> Result<u16, Self::Error> {
            self.selectors.push(selector);
            values.write_register(0, 55).unwrap();
            values.write_register(1, 9).unwrap();
            Ok(9)
        }
    }

    let mut values = ModelValues::new_invocation(table());
    let mut callback = LeftWriter {
        selectors: Vec::new(),
    };
    assert_eq!(
        values.branch_decision(&instruction(0x16, [7, 0x80, 0xc1]), &mut callback),
        Err(ModelValueError::Fault(
            ModelValueFault::UninitializedRegister { index: 1 }
        ))
    );
    assert!(callback.selectors.is_empty());
    assert_eq!(values.register(0), Ok(None));
    assert_eq!(values.register(1), Ok(None));
    assert!(values.stopped());
}

/// A jump of three lands where falling through does (004A0CC4/004A0CD1), so
/// the factory model's read of an unwritten register 17 decides nothing and
/// must not stop the model; a callback operand still runs first.
#[test]
fn a_comparison_that_decides_nothing_reads_an_unwritten_register_without_stopping() {
    struct RightWriter {
        selectors: Vec<u32>,
    }
    impl ModelValueCallback for RightWriter {
        type Error = &'static str;
        fn packed_value(
            &mut self,
            values: &mut ModelValues,
            selector: u32,
        ) -> Result<u16, Self::Error> {
            self.selectors.push(selector);
            values.write_register(1, 9).unwrap();
            Ok(9)
        }
    }
    // 1.ovl+1C19E: 0016 0003 00D1 0001.
    let mut values = ModelValues::new_invocation(table());
    assert_eq!(
        values.branch_decision(&instruction(0x16, [3, 0xd1, 1]), &mut NoCallback),
        Ok(ModelValueBranch::FallThrough)
    );
    assert!(!values.stopped());
    assert_eq!(values.register(17), Ok(None));
    let mut callback = RightWriter {
        selectors: Vec::new(),
    };
    assert_eq!(
        values.branch_decision(&instruction(0x116, [3, 0xd1, 0x80]), &mut callback),
        Ok(ModelValueBranch::FallThrough)
    );
    assert_eq!(callback.selectors, [0]);
    assert_eq!(values.register(1), Ok(Some(9)));
    assert!(!values.stopped());
    // Any other jump decides, and keeps the strict unwritten-register stop.
    assert_eq!(
        values.branch_decision(&instruction(0x16, [4, 0xd1, 1]), &mut NoCallback),
        Err(ModelValueError::Fault(
            ModelValueFault::UninitializedRegister { index: 17 }
        ))
    );
    assert!(values.stopped());
}

#[test]
fn comparison_uses_signed_argument_relative_jump_and_full_command_mode() {
    let mut values = ModelValues::new_invocation(table());
    for (command, left, right, jump) in [
        (0x16, 1, 1, false),
        (0x16, 1, 2, true),
        (0x116, 1, 2, false),
        (0x116, 1, 1, true),
        (0x216, 1, 1, true),
        (0x8016, 1, 2, true),
    ] {
        assert_eq!(
            values.branch_decision(
                &instruction(command, [0xfffe, left, right]),
                &mut NoCallback
            ),
            Ok(if jump {
                ModelValueBranch::Jump {
                    words_from_arguments: -2,
                }
            } else {
                ModelValueBranch::FallThrough
            })
        );
    }
}

#[test]
fn unsupported_instructions_and_destinations_fail_before_callbacks() {
    for (record, fault) in [
        (
            instruction(0x16, [1, 0, 0]),
            ModelValueFault::UnsupportedOpcode { opcode: 0x16 },
        ),
        (
            instruction(0xf0, [22, 0x80, 0x80]),
            ModelValueFault::ContextAliasedRegister { index: 22 },
        ),
        (
            instruction(0xf0, [65535, 0x80, 0x80]),
            ModelValueFault::InvalidRegister { index: -1 },
        ),
    ] {
        let mut values = ModelValues::new_invocation(table());
        assert_eq!(
            values.execute_register(&record, &mut NoCallback),
            Err(ModelValueError::Fault(fault))
        );
        assert!(values.stopped());
    }
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
        panic!("wrong model registration")
    };
    &bank.entries[usize::from(resource.registration.local_index)]
}

#[test]
fn retail_frontend_values_execute_at_multiple_clock_values_without_rendering() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let table = Arc::new(assets.sine_table().values);
    for clock_value in [0u32, 1, 3, 4, 60, 63, 64, 65535, 65536] {
        let mut clock = AnimationClock::default();
        clock.advance(clock_value * 20000);
        let mut callback = SceneCallback(SceneModelSubmission {
            model_slot: 0,
            position: [0; 3],
            basis: MenuBasis::INITIAL,
            animation: clock,
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::Frontend,
        });
        for id in [747, 749, 755, 756, 757] {
            let model = resident_model(&runtime, id);
            let mut values = ModelValues::new_invocation(Arc::clone(&table));
            let mut count = 0;
            for record in &model.program.instructions {
                if matches!(record.kind, ModelInstructionKind::RegisterOperation(_)) {
                    values.execute_register(record, &mut callback).unwrap();
                    count += 1;
                }
            }
            if id == 757 {
                assert_eq!(count, 2);
                assert_eq!(
                    values.register(6),
                    Ok(Some((clock_value as u16).wrapping_shl(5)))
                );
                assert_eq!(
                    values.register(7),
                    Ok(Some((clock_value as u16).wrapping_shl(6)))
                );
            } else {
                assert_eq!(count, 1);
                assert_eq!(
                    values.register(1),
                    Ok(Some((clock_value as u16).wrapping_shl(4)))
                );
            }
        }
        // Child746 has16 guarded face groups. Follow only its proven value
        // branch addresses; count selected records without evaluating vertices,
        // clipping, lighting, child bases or submitting any primitive.
        let model = resident_model(&runtime, 746);
        let parent = ModelValues::new_invocation(Arc::clone(&table));
        let mut values = parent.child_invocation().unwrap();
        let program = &model.program;
        let offsets: BTreeMap<_, _> = program
            .instructions
            .iter()
            .enumerate()
            .map(|(i, v)| (v.span.start, i))
            .chain(std::iter::once((
                program.terminator.start,
                program.instructions.len(),
            )))
            .collect();
        let mut index = 0;
        let mut selected_groups = Vec::new();
        let mut quads = 0;
        while index < program.instructions.len() {
            let record = &program.instructions[index];
            match &record.kind {
                ModelInstructionKind::RegisterOperation(_) => {
                    values.execute_register(record, &mut callback).unwrap();
                }
                ModelInstructionKind::UnresolvedDynamic(command)
                    if record.raw_command as u8 == 0x16 =>
                {
                    match values.branch_decision(record, &mut callback).unwrap() {
                        ModelValueBranch::Jump {
                            words_from_arguments,
                        } => {
                            let target = (record.span.start as i64
                                + 2
                                + i64::from(words_from_arguments) * 2)
                                as usize;
                            index = offsets[&target];
                            continue;
                        }
                        ModelValueBranch::FallThrough => selected_groups.push(
                            values
                                .evaluate_packed(command.arguments[2], &mut callback)
                                .unwrap(),
                        ),
                    }
                }
                ModelInstructionKind::Quad(_) => quads += 1,
                _ => {}
            }
            index += 1;
        }
        let phase = (clock_value as u16).wrapping_shl(10);
        assert_eq!(values.register(1), Ok(Some(phase)));
        assert_eq!(values.register(12), Ok(Some(phase)));
        assert_eq!(values.register(5), Ok(Some(phase / 4096)));
        assert_eq!(selected_groups, [phase / 4096]);
        assert_eq!(quads, 4);
        assert_eq!(parent.register(1), Ok(None));
    }
}
