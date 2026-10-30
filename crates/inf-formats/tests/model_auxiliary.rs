use inf_formats::{
    decode_model_auxiliary, ByteSpan, ModelAuxiliaryError, ModelAuxiliaryInstructionKind as Kind,
    ModelAuxiliaryRotation,
};

// Auxiliary opcode, shared renderer opcode, and native returned argument size.
// 97 deliberately has nine trailing WORDs because its command is 9700, not the
// skipped byte: 004EC700 returns args + (command >> 12) * 2 + 4.
const SHARED_HANDLERS: [(u8, u8, usize); 13] = [
    (0x07, 0x20, 2),
    (0x18, 0x23, 6),
    (0x2c, 0x25, 6),
    (0x2d, 0x24, 8),
    (0x2e, 0x21, 4),
    (0x2f, 0x26, 8),
    (0x3d, 0x22, 6),
    (0x49, 0x2b, 4),
    (0x61, 0x27, 12),
    (0x7e, 0x28, 10),
    (0x85, 0x2a, 4),
    (0x96, 0x2c, 4),
    (0x97, 0x2d, 22),
];

fn word_bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn framed(opcode: u8, prefix: usize, arguments_start: usize, args: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x87; prefix];
    bytes.push(opcode);
    // Nonzero and otherwise-invalid opcodes make accidental padding execution
    // visible. These bytes belong to this instruction's source span.
    bytes.resize(arguments_start, 0xf9);
    bytes.extend_from_slice(args);
    bytes.push(0x88);
    bytes
}

#[test]
fn every_shared_handler_uses_its_native_extent_and_opcode_command() {
    for (opcode, renderer_opcode, size) in SHARED_HANDLERS {
        for (prefix, arguments_start) in [(0, 2), (1, 4), (2, 4), (3, 6)] {
            let words: Vec<_> = (0..size / 2).map(|i| 0x8000 | i as u16).collect();
            let args = word_bytes(&words);
            let bytes = framed(opcode, prefix, arguments_start, &args);
            let program = decode_model_auxiliary(&bytes).unwrap();
            let instruction = &program.instructions[prefix];
            assert_eq!(
                instruction.kind,
                Kind::SharedVertexHandler {
                    renderer_opcode,
                    handler_command: u16::from(opcode) << 8,
                    arguments: words,
                },
                "opcode {opcode:02x}, prefix {prefix}"
            );
            assert_eq!(
                instruction.span,
                ByteSpan::new(prefix, arguments_start + size)
            );
            assert_eq!(
                instruction.arguments_span,
                ByteSpan::new(arguments_start, arguments_start + size)
            );
            assert_eq!(instruction.raw_arguments, args);
            assert_eq!(
                program.terminator,
                ByteSpan::new(arguments_start + size, arguments_start + size + 1)
            );
        }
    }
}

#[test]
fn skipped_mode_like_bytes_are_opaque_even_when_the_opcode_is_odd_aligned() {
    for skipped in [0, 1, 4, 0x88, 0xff] {
        // Opcode at 1: p+1 and the extra alignment byte at 3 are skipped.
        let bytes = [0x87, 0x07, skipped, 0x88, 0xfe, 0xff, 0x88];
        let program = decode_model_auxiliary(&bytes).unwrap();
        assert_eq!(program.instructions.len(), 2);
        assert_eq!(program.instructions[1].span, ByteSpan::new(1, 6));
        assert_eq!(program.instructions[1].arguments_span, ByteSpan::new(4, 6));
        assert_eq!(
            program.instructions[1].kind,
            Kind::SharedVertexHandler {
                renderer_opcode: 0x20,
                handler_command: 0x0700,
                arguments: vec![0xfffe],
            }
        );
    }
}

#[test]
fn callback_dword_alignment_is_relative_to_the_standalone_model_span() {
    for (opcode, callback_offset, parameters) in [
        (0x8e, 0, vec![i32::MIN]),
        (0x8f, 8, vec![i32::MIN, -1, i32::MAX]),
        (0x90, 4, vec![-1, i32::MAX]),
    ] {
        let mut args: Vec<_> = parameters.iter().flat_map(|v| v.to_le_bytes()).collect();
        args.extend_from_slice(&i16::MIN.to_le_bytes());
        for (prefix, arguments_start) in [(0, 4), (1, 4), (2, 4), (3, 4), (4, 8)] {
            let bytes = framed(opcode, prefix, arguments_start, &args);
            // The slice itself has an unrelated odd address in its owner. It
            // must not affect the relocated-model alignment calculation.
            let mut enclosing = vec![0xad];
            enclosing.extend_from_slice(&bytes);
            let program = decode_model_auxiliary(&enclosing[1..]).unwrap();
            let instruction = &program.instructions[prefix];
            assert_eq!(
                instruction.kind,
                Kind::Callback {
                    callback_offset,
                    parameters: parameters.clone(),
                    vertex_slot: i16::MIN,
                }
            );
            assert_eq!(
                instruction.arguments_span,
                ByteSpan::new(arguments_start, arguments_start + args.len())
            );
            assert_eq!(instruction.raw_arguments, args);
        }
    }
}

#[test]
fn word_alignment_at_odd_opcode_preserves_signed_transform_slots() {
    let bytes = framed(0x31, 1, 2, &word_bytes(&[0x8000, 0xffff, 0x7fff]));
    let program = decode_model_auxiliary(&bytes).unwrap();
    assert_eq!(program.instructions[1].span, ByteSpan::new(1, 8));
    assert_eq!(
        program.instructions[1].kind,
        Kind::Transform {
            vertex_slots: [i16::MIN, -1, i16::MAX]
        }
    );
}

#[test]
fn register_fields_are_signed_but_packed_operands_retain_all_bits() {
    let bytes = framed(0x36, 0, 2, &word_bytes(&[0xfffd, 0x8000, 0x8000, 0xffff]));
    let program = decode_model_auxiliary(&bytes).unwrap();
    assert_eq!(
        program.instructions[0].kind,
        Kind::RegisterOperation {
            selector: -3,
            destination: i16::MIN,
            packed_operands: [0x8000, 0xffff],
        }
    );
}

#[test]
fn rotation_records_preserve_first_and_second_reset_asymmetry() {
    // Both modes are zero: native first rotates the existing saved basis;
    // native second restores an oriented current basis before rotating.
    let bytes = framed(0x30, 0, 2, &[2, 0, 0, 0, 0xff, 0, 0, 0x80, 0x80]);
    let program = decode_model_auxiliary(&bytes).unwrap();
    assert_eq!(
        program.instructions[0].kind,
        Kind::Rotate {
            second_present_raw: 2,
            first: ModelAuxiliaryRotation {
                mode_raw: 0,
                packed_angle: 0,
                orientation_raw: -1,
            },
            second: Some(ModelAuxiliaryRotation {
                mode_raw: 0,
                packed_angle: 0x8000,
                orientation_raw: i8::MIN,
            }),
        }
    );
    assert_eq!(program.terminator, ByteSpan::new(11, 12));
}

#[test]
fn single_rotation_ends_after_five_argument_bytes() {
    let bytes = framed(0x30, 1, 2, &[0, 0xf7, 0xff, 0x80, 0xfe]);
    let program = decode_model_auxiliary(&bytes).unwrap();
    assert_eq!(
        program.instructions[1].kind,
        Kind::Rotate {
            second_present_raw: 0,
            first: ModelAuxiliaryRotation {
                mode_raw: 0xf7,
                packed_angle: 0x80ff,
                orientation_raw: -2,
            },
            second: None,
        }
    );
    assert_eq!(program.terminator, ByteSpan::new(7, 8));
}

#[test]
fn child_record_retains_variable_state_and_signed_resource_identity() {
    let args = word_bytes(&[0xffff, 0xffe6, 12, 0xfffe, 0x8000, 0x1234]);
    let bytes = framed(0x05, 0, 2, &args);
    let program = decode_model_auxiliary(&bytes).unwrap();
    let Kind::ChildModel {
        model,
        orientation,
        argument_bytes,
        state_words,
    } = &program.instructions[0].kind
    else {
        panic!("expected auxiliary child");
    };
    assert_eq!(model.raw(), -1);
    assert_eq!(orientation.raw(), 0xffe6);
    assert!(orientation.uses_saved_basis());
    assert_eq!(*argument_bytes, 12);
    assert_eq!(state_words, &[0xfffe, 0x8000, 0x1234]);
    assert_eq!(program.instructions[0].raw_arguments, args);
    assert_eq!(program.terminator, ByteSpan::new(14, 15));
}

#[test]
fn child_byte_displacement_can_be_odd_and_retains_the_opaque_tail() {
    let args = [0xff, 0xff, 6, 0, 9, 0, 0xfe, 0xff, 0xa7];
    let bytes = framed(0x05, 0, 2, &args);
    let program = decode_model_auxiliary(&bytes).unwrap();
    let Kind::ChildModel {
        argument_bytes,
        state_words,
        ..
    } = &program.instructions[0].kind
    else {
        panic!("expected auxiliary child");
    };
    assert_eq!(*argument_bytes, 9);
    assert_eq!(state_words, &[0xfffe]);
    assert_eq!(program.instructions[0].raw_arguments, args);
    assert_eq!(program.terminator, ByteSpan::new(11, 12));
}

#[test]
fn group_callback_fields_are_signed_and_fallback_can_target_odd_terminator() {
    let mut bytes = vec![0x8c, 0xfa, 25, 0, 0x89, 0xfb];
    bytes.extend(word_bytes(&[0xffff, 0x8000, 0x7fff, 0]));
    bytes.extend([0x8a, 0xfc]);
    bytes.extend(word_bytes(&[0x8000, 0xffff, 0x7fff]));
    bytes.extend([0x8b, 0xfd, 0x0d, 0xf0, 0x8d, 0x88]);
    let program = decode_model_auxiliary(&bytes).unwrap();
    let kinds: Vec<_> = program
        .instructions
        .iter()
        .map(|i| i.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::GroupBegin {
                displacement_bytes: 25,
                target: 27,
            },
            Kind::GroupCallback {
                callback_offset: 12,
                vertex_slots: vec![-1, i16::MIN, i16::MAX, 0],
            },
            Kind::GroupCallback {
                callback_offset: 16,
                vertex_slots: vec![i16::MIN, -1, i16::MAX],
            },
            Kind::GroupRequireHit {
                retained_word: 0xf00d,
            },
            Kind::GroupEnd,
        ]
    );
    assert_eq!(program.terminator, ByteSpan::new(27, 28));
}

#[test]
fn all_one_byte_commands_and_post_terminator_bytes_remain_distinct() {
    let bytes = [0x87, 0x93, 0x94, 0x8d, 0x88, 0x91, 0xff, 0x88];
    let program = decode_model_auxiliary(&bytes).unwrap();
    assert_eq!(program.span, ByteSpan::new(0, 5));
    assert_eq!(program.terminator, ByteSpan::new(4, 5));
    assert_eq!(program.padding, ByteSpan::new(5, 8));
    assert_eq!(program.instructions.len(), 4);
    for (i, kind) in [Kind::NoOp, Kind::NoOp, Kind::NoOp, Kind::GroupEnd]
        .into_iter()
        .enumerate()
    {
        assert_eq!(program.instructions[i].kind, kind);
        assert_eq!(program.instructions[i].span, ByteSpan::new(i, i + 1));
        assert_eq!(
            program.instructions[i].arguments_span,
            ByteSpan::new(i + 1, i + 1)
        );
        assert!(program.instructions[i].raw_arguments.is_empty());
    }
    let empty = decode_model_auxiliary(&[0x88]).unwrap();
    assert!(empty.instructions.is_empty());
    assert_eq!(empty.padding, ByteSpan::new(1, 1));
}

#[test]
fn relative_branches_preserve_forward_backward_and_self_edges_without_execution() {
    for opcode in [0x8c, 0x95] {
        for (prefix, displacement, target, suffix) in [
            (0, 2_i16, 4, vec![0x88]),
            (0, 2, 4, vec![0x87, 0x88]),
            (0, -2, 0, vec![0x88]),
            (1, -2, 0, vec![0x88]),
            (1, -1, 1, vec![0x88]),
        ] {
            let mut bytes = vec![0x87; prefix];
            bytes.push(opcode);
            bytes.resize(2, 0xfa);
            bytes.extend_from_slice(&displacement.to_le_bytes());
            bytes.extend(suffix);
            let program = decode_model_auxiliary(&bytes).unwrap();
            let expected = if opcode == 0x8c {
                Kind::GroupBegin {
                    displacement_bytes: displacement,
                    target,
                }
            } else {
                Kind::BranchUnlessHit {
                    displacement_bytes: displacement,
                    target,
                }
            };
            assert_eq!(program.instructions[prefix].kind, expected);
        }
    }
}

#[test]
fn branches_reject_skipped_bytes_argument_interiors_padding_and_outside() {
    for opcode in [0x8c, 0x95] {
        for displacement in [-3_i16, -1, 0, 1, 3, 4, i16::MIN, i16::MAX] {
            let mut bytes = vec![opcode, 0xfa];
            bytes.extend_from_slice(&displacement.to_le_bytes());
            bytes.extend([0x88, 0x87]); // offset 5 is retained padding, not a target.
            assert_eq!(
                decode_model_auxiliary(&bytes),
                Err(ModelAuxiliaryError::InvalidBranchTarget {
                    offset: 0,
                    displacement_bytes: displacement,
                }),
                "opcode {opcode:02x}, displacement {displacement}"
            );
        }
    }
}

#[test]
fn truncated_operands_fail_before_reading_any_missing_byte() {
    let mut fixtures: Vec<_> = SHARED_HANDLERS
        .iter()
        .map(|&(op, _, size)| framed(op, 0, 2, &vec![0; size]))
        .collect();
    fixtures.extend([
        framed(0x05, 0, 2, &word_bytes(&[1, 6, 12, 2, 4, 6])),
        framed(0x30, 0, 2, &[0, 1, 0, 0, 0]),
        framed(0x30, 0, 2, &[1, 1, 0, 0, 0, 0, 0, 0, 0]),
        framed(0x31, 0, 2, &[0; 6]),
        framed(0x36, 0, 2, &[0; 8]),
        framed(0x89, 0, 2, &[0; 8]),
        framed(0x8a, 0, 2, &[0; 6]),
        framed(0x8b, 0, 2, &[0; 2]),
        framed(0x8c, 0, 2, &[2, 0]),
        framed(0x8e, 0, 4, &[0; 6]),
        framed(0x8f, 0, 4, &[0; 14]),
        framed(0x90, 0, 4, &[0; 10]),
        framed(0x95, 0, 2, &[2, 0]),
    ]);
    for bytes in fixtures {
        decode_model_auxiliary(&bytes).unwrap();
        for cut in 1..bytes.len() - 1 {
            assert!(
                matches!(
                    decode_model_auxiliary(&bytes[..cut]),
                    Err(ModelAuxiliaryError::Truncated { offset: 0, opcode, .. })
                        if opcode == bytes[0]
                ),
                "opcode {:02x}, cut {cut}",
                bytes[0]
            );
        }
    }
}

#[test]
fn bounded_child_decoder_rejects_overlapping_or_backward_record_extents() {
    // The native code does not check these. This decoder deliberately supports
    // nonoverlapping child records whose attachment WORD is within the record.
    for extent in [i16::MIN, -2, 0, 1, 6, 7] {
        let bytes = framed(0x05, 0, 2, &word_bytes(&[1, 6, extent as u16, 2]));
        assert_eq!(
            decode_model_auxiliary(&bytes),
            Err(ModelAuxiliaryError::UnsupportedChildExtent {
                offset: 0,
                argument_bytes: extent,
            })
        );
    }
}

#[test]
fn unknown_bytes_are_not_padding_before_the_terminator() {
    for opcode in [0x00, 0x06, 0x86, 0x91, 0x92, 0x98, 0xff] {
        assert_eq!(
            decode_model_auxiliary(&[0x87, opcode, 0x88]),
            Err(ModelAuxiliaryError::UnknownOpcode { offset: 1, opcode })
        );
    }
}

#[test]
fn missing_terminator_is_distinct_from_truncated_operands() {
    for (bytes, offset) in [
        (vec![], 0),
        (vec![0x87, 0x93], 2),
        (vec![0x07, 0xf9, 0, 0], 4),
        (vec![0x30, 0xf9, 0, 1, 0, 0, 0], 7),
    ] {
        assert_eq!(
            decode_model_auxiliary(&bytes),
            Err(ModelAuxiliaryError::MissingTerminator { offset })
        );
    }
}
