use super::*;

#[test]
fn map_annotation_commands_preserve_numeric_unsigned_widths_and_rejection_reads() {
    for code in [2, 6] {
        let view = decode(87, code, &[0xff, 0xff, 0xaa]).unwrap();
        assert_eq!(view.proven_method, Some(0x0048_c7a0));
        assert_eq!(view.payload_consumer, Some(0x0048_c7a0));
        let field = match view.payload {
            InstanceCommandPayload::MapAnnotationLabel(field)
            | InstanceCommandPayload::MapAnnotationRadius(field) => field,
            _ => panic!("wrong unsigned map WORD"),
        };
        assert_eq!(field.value, 65535);
        assert_eq!(field.span, ByteSpan::new(13, 15));
        for length in 0..2 {
            assert!(decode(87, code, &vec![0xff; length]).is_err());
        }
    }
    for code in 3..=5 {
        let view = decode(87, code, &[0xff, 0xaa]).unwrap();
        assert!(
            matches!(view.payload, InstanceCommandPayload::MapAnnotationColor { channel, value }
            if channel == (code-3) as u8 && value.value == 255 && value.span == ByteSpan::new(13,14))
        );
        assert!(decode(87, code, &[]).is_err());
    }
    for code in [i32::MIN, -1, 7, i32::MAX] {
        assert_eq!(
            decode(87, code, &[]).unwrap().payload,
            InstanceCommandPayload::Opaque
        );
    }
    for code in [0, 1] {
        assert_eq!(
            decode(87, code, &[]).unwrap().payload,
            InstanceCommandPayload::NoPayloadRead
        );
    }
}

/// 004407E0 code 3 retains its operand's address without reading it.
#[test]
fn selector64_gate_link_retains_its_operand_address() {
    let view = decode(64, 3, &[]).unwrap();
    assert_eq!(view.proven_method, Some(0x0044_07e0));
    assert_eq!(view.payload_consumer, Some(0x0044_07e0));
    assert_eq!(
        view.payload,
        InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: 4
        }
    );
    assert!(matches!(
        decode(64, 2, &[0x10, 0]).unwrap().payload,
        InstanceCommandPayload::ModelSlot(_)
    ));
    assert_eq!(
        decode(64, 4, &[]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
}

/// 00401080: codes 3/4 read a WORD, 5/6 one BYTE.
#[test]
fn selector42_trigger_commands_read_their_native_widths() {
    for code in [3, 4] {
        let view = decode(42, code, &[0x2c, 0x01, 0xaa]).unwrap();
        assert_eq!(view.proven_method, Some(0x0040_1080));
        assert_eq!(view.payload_consumer, Some(0x0040_1080));
        let field = match view.payload {
            InstanceCommandPayload::Selector42Radius(field) if code == 3 => field,
            InstanceCommandPayload::Selector42WakeRadius(field) if code == 4 => field,
            _ => panic!("wrong selector 42 WORD"),
        };
        assert_eq!(field.value, 300);
        assert_eq!(field.span, ByteSpan::new(13, 15));
        assert!(decode(42, code, &[0x2c]).is_err());
    }
    for code in [5, 6] {
        let view = decode(42, code, &[0x02, 0xaa]).unwrap();
        let field = match view.payload {
            InstanceCommandPayload::Selector42OnDraw(field) if code == 5 => field,
            InstanceCommandPayload::Selector42Disabled(field) if code == 6 => field,
            _ => panic!("wrong selector 42 BYTE"),
        };
        assert_eq!(field.value, 2);
        assert_eq!(field.span, ByteSpan::new(13, 14));
        assert!(decode(42, code, &[]).is_err());
    }
    assert_eq!(
        decode(42, 7, &[]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
}

/// 0043FF40: codes 3/4 read a WORD, 5/6 a DWORD; codes up to 2 keep the
/// visual base's reads and later codes stay opaque.
#[test]
fn selector50_trigger_commands_read_their_native_widths() {
    for code in [3, 4] {
        let view = decode(50, code, &[0x2c, 0x01, 0xaa]).unwrap();
        assert_eq!(view.proven_method, Some(0x0043_ff40));
        assert_eq!(view.payload_consumer, Some(0x0043_ff40));
        let field = match view.payload {
            InstanceCommandPayload::Selector50Radius(field) if code == 3 => field,
            InstanceCommandPayload::Selector50Seconds(field) if code == 4 => field,
            _ => panic!("wrong selector 50 WORD"),
        };
        assert_eq!(field.value, 300);
        assert_eq!(field.span, ByteSpan::new(13, 15));
        assert!(decode(50, code, &[0x2c]).is_err());
    }
    for code in [5, 6] {
        let view = decode(50, code, &[0x25, 0, 0, 0x01]).unwrap();
        let field = match view.payload {
            InstanceCommandPayload::Selector50Command(field) if code == 5 => field,
            InstanceCommandPayload::Selector50Silent(field) if code == 6 => field,
            _ => panic!("wrong selector 50 DWORD"),
        };
        assert_eq!(field.value, 0x0100_0025);
        assert_eq!(field.span, ByteSpan::new(13, 17));
        assert!(decode(50, code, &[0x25, 0, 0]).is_err());
    }
    assert!(matches!(
        decode(50, 2, &[0xff, 0xff]).unwrap().payload,
        InstanceCommandPayload::ModelSlot(_)
    ));
    assert_eq!(
        decode(50, 7, &[]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
}

fn decode(
    selector: u16,
    code: i32,
    payload: &[u8],
) -> Result<InstanceCommandView, InstanceAssetError> {
    decode_target(InstanceCommandTarget::Constructor(selector), code, payload)
}

fn decode_target(
    target: InstanceCommandTarget,
    code: i32,
    payload: &[u8],
) -> Result<InstanceCommandView, InstanceAssetError> {
    let mut source = vec![0xaa; 9];
    source.extend_from_slice(&code.to_le_bytes());
    source.extend_from_slice(payload);
    let main_payload = ByteSpan::new(9, source.len());
    source.extend_from_slice(&[0xee; 16]); // Must not satisfy an incomplete main allocation.
    decode_instance_command(InstanceCommandRequest {
        read_scope: crate::InstanceCommandReadScope::Inspection,
        source: &source,
        main_payload,
        relocations: &[],
        command_offset: 0,
        target,
    })
}

#[test]
fn shared_actor_target_preserves_its_exact_command_boundary() {
    let target = InstanceCommandTarget::SharedActor;
    for code in [i32::MIN, -1, 16, 17, 18, 19, i32::MAX] {
        let view = decode_target(target, code, &[]).unwrap();
        assert_eq!(view.target, target);
        assert_eq!(view.code.value, code);
        assert_eq!(view.proven_method, Some(0x0047_ada0));
        assert_eq!(view.payload_consumer, None);
        assert_eq!(view.payload, InstanceCommandPayload::Opaque);
    }
    for (code, consumer) in [(0, 0x0047_9e70), (1, 0x0047_9e70), (3, 0x0047_ada0)] {
        let view = decode_target(target, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, Some(consumer));
        assert_eq!(view.payload, InstanceCommandPayload::NoPayloadRead);
    }
    assert!(matches!(
        decode_target(target, 2, &0x8001_u16.to_le_bytes())
            .unwrap()
            .payload,
        InstanceCommandPayload::ModelSlot(InstanceModelSlot {
            signed_slot: -32767,
            ..
        })
    ));
    let conditional = decode_target(target, 13, &[]).unwrap();
    assert_eq!(conditional.target, target);
    assert_eq!(conditional.payload_consumer, Some(0x0047_ada0));
    assert_eq!(
        conditional.payload,
        InstanceCommandPayload::ActorDisableUnlessBit(ConditionalInstanceWord::OutsideAllocation {
            allocation_offset: 4,
        })
    );
}

#[test]
fn model_override_requires_an_audited_command_path_not_a_visual_context_initializer() {
    let view = decode(22, 2, &0x8001u16.to_le_bytes()).unwrap();
    assert_eq!(view.proven_method, Some(0x0047_a1a0));
    assert_eq!(view.payload_consumer, Some(0x0047_a1a0));
    assert_eq!(
        view.payload,
        InstanceCommandPayload::ModelSlot(InstanceModelSlot {
            raw: InstanceField {
                span: ByteSpan::new(13, 15),
                value: 0x8001
            },
            signed_slot: -32767,
        })
    );
    for selector in [0, 67, 95] {
        assert_eq!(
            decode(selector, 2, &[]).unwrap().payload,
            InstanceCommandPayload::Opaque
        );
    }
}

/// 00409320, the spawner's +04: codes 2 and 3 read a DWORD each, for +44
/// and +48; code 4 reads nothing.
#[test]
fn selector81_reads_its_count_and_distance_dwords() {
    let field = |value| InstanceField {
        span: ByteSpan::new(13, 17),
        value,
    };
    let view = decode(81, 2, &5u32.to_le_bytes()).unwrap();
    assert_eq!(view.payload_consumer, Some(0x0040_9320));
    assert_eq!(
        view.payload,
        InstanceCommandPayload::Selector81Count44(field(5))
    );
    assert_eq!(
        decode(81, 3, &700u32.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::Selector81Distance48(field(700))
    );
    assert!(matches!(
        decode(81, 2, &[5, 0, 0]),
        Err(InstanceAssetError::OutOfBounds { .. })
    ));
    assert_eq!(
        decode(81, 4, &[]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
}

#[test]
fn selector86_code2_keeps_signed_rate_and_bounds_the_runtime_write() {
    for rate in [i16::MIN, -1, 0, 1, i16::MAX] {
        let mut payload = [0xab; 8];
        payload[2..4].copy_from_slice(&rate.to_le_bytes());
        let view = decode(86, 2, &payload).unwrap();
        assert_eq!(view.proven_method, Some(0x0042_def0));
        assert_eq!(view.payload_consumer, Some(0x0042_dec0));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::Selector86Data {
                allocation_offset: 4,
                rate: InstanceField {
                    span: ByteSpan::new(15, 17),
                    value: rate
                },
            }
        );
        for length in 0..8 {
            assert!(decode(86, 2, &payload[..length]).is_err());
        }
    }
    for code in [0, 1] {
        assert_eq!(
            decode(86, code, &[]).unwrap().payload,
            InstanceCommandPayload::NoPayloadRead
        );
    }
    for code in [-1, 3, i32::MAX] {
        assert_eq!(
            decode(86, code, &[]).unwrap().payload,
            InstanceCommandPayload::Opaque
        );
    }
}

#[test]
fn route_node_preserves_word_reference_and_dword_mode_without_applying_state_policy() {
    for raw in [0_u16, 1, 0x8000, 0xffff] {
        let view = decode(51, 3, &raw.to_le_bytes()).unwrap();
        assert_eq!(view.proven_method, Some(0x0040_4cc0));
        assert_eq!(view.payload_consumer, Some(0x0040_4cc0));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::RouteNodePlacementReference(InstanceField {
                span: ByteSpan::new(13, 15),
                value: raw,
            })
        );
    }
    // Even rejected runtime mode inputs retain their full authored DWORD.
    for raw in [0_u32, 1, 2, 0x8000_0000, 0xffff_ffff] {
        let view = decode(51, 4, &raw.to_le_bytes()).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0040_4cc0));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::RouteNodeMode(InstanceField {
                span: ByteSpan::new(13, 17),
                value: raw,
            })
        );
    }
    for (code, width) in [(3, 2), (4, 4)] {
        for available in 0..width {
            assert!(
                matches!(decode(51, code, &vec![0; available]), Err(InstanceAssetError::OutOfBounds {
                offset: 4,
                length,
                ..
            }) if length == width)
            );
        }
    }
}

#[test]
fn route_node_no_operand_effects_and_visual_forwarding_do_not_read_unowned_bytes() {
    for (code, payload) in [
        (5, InstanceCommandPayload::RouteNodeFlagOr { mask: 2 }),
        (6, InstanceCommandPayload::NoPayloadRead),
        (7, InstanceCommandPayload::NoPayloadRead),
        (8, InstanceCommandPayload::RouteNodeFlagOr { mask: 0x10 }),
    ] {
        let view = decode(51, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0040_4cc0));
        assert_eq!(view.payload, payload);
    }
    for code in [0, 1] {
        let view = decode(51, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0047_9e70));
        assert_eq!(view.payload, InstanceCommandPayload::NoPayloadRead);
    }
    let view = decode(51, 2, &[0xff, 0xff]).unwrap();
    assert_eq!(view.payload_consumer, Some(0x0047_a1a0));
    assert!(
        matches!(view.payload, InstanceCommandPayload::ModelSlot(model) if model.signed_slot == -1)
    );
    for code in [i32::MIN, -1, 9, i32::MAX] {
        let view = decode(51, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, None);
        assert_eq!(view.payload, InstanceCommandPayload::Opaque);
    }
    // Identical code numbers in the neighboring terrain-light class still
    // require their actual byte/WORD operands.
    for code in [5, 6, 7] {
        assert!(matches!(
            decode(19, code, &[]),
            Err(InstanceAssetError::OutOfBounds { .. })
        ));
    }
}

#[test]
fn terrain_light_uses_signed_single_bytes_and_unsigned_word_without_actor_command_aliases() {
    for code in 3..=5 {
        for (raw, signed) in [(0x00, 0), (0x7f, 127), (0x80, -128), (0xff, -1)] {
            let view = decode(19, code, &[raw]).unwrap();
            assert_eq!(view.proven_method, Some(0x0044_8120));
            assert_eq!(view.payload_consumer, Some(0x0044_8120));
            assert_eq!(
                view.payload,
                InstanceCommandPayload::TerrainLightChannel {
                    channel: (code - 3) as u8,
                    value: InstanceField {
                        span: ByteSpan::new(13, 14),
                        value: signed,
                    },
                }
            );
        }
    }
    assert_eq!(
        decode(19, 6, &[0xff]).unwrap().payload,
        InstanceCommandPayload::TerrainLightDrawGate(InstanceField {
            span: ByteSpan::new(13, 14),
            value: 255,
        })
    );
    assert_eq!(
        decode(19, 7, &[0xff, 0xff]).unwrap().payload,
        InstanceCommandPayload::TerrainLightRadius(InstanceField {
            span: ByteSpan::new(13, 15),
            value: 65535,
        })
    );
    for code in [0, 1] {
        let view = decode(19, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0047_9e70));
        assert_eq!(view.payload, InstanceCommandPayload::NoPayloadRead);
    }
    let model = decode(19, 2, &[0xfe, 0xff]).unwrap();
    assert_eq!(model.payload_consumer, Some(0x0047_a1a0));
    assert!(
        matches!(model.payload, InstanceCommandPayload::ModelSlot(field) if field.signed_slot == -2)
    );
    for code in [i32::MIN, -1, 8, 15, i32::MAX] {
        let view = decode(19, code, &[]).unwrap();
        assert_eq!(view.payload_consumer, None);
        assert_eq!(view.payload, InstanceCommandPayload::Opaque);
    }
    for (code, length) in [(2, 2), (3, 1), (4, 1), (5, 1), (6, 1), (7, 2)] {
        for available in 0..length {
            assert!(matches!(
                decode(19, code, &vec![0; available]),
                Err(InstanceAssetError::OutOfBounds {
                    offset: 4,
                    length: required,
                    ..
                }) if required == length
            ));
        }
    }
}

#[test]
fn shared_fields_follow_actual_word_reads_without_ghidras_wider_dereferences() {
    let field = InstanceField {
        span: ByteSpan::new(13, 15),
        value: 60000,
    };
    let word = 60000u16.to_le_bytes();
    assert_eq!(
        decode(3, 5, &word).unwrap().payload,
        InstanceCommandPayload::ActorWord8c(field)
    );
    assert_eq!(
        decode(3, 7, &word).unwrap().payload,
        InstanceCommandPayload::ActorMaximumHealth(field)
    );
    assert_eq!(
        decode(3, 8, &word).unwrap().payload,
        InstanceCommandPayload::ActorCurrentHealth(field)
    );
    assert_eq!(
        decode(3, 9, &word).unwrap().payload,
        InstanceCommandPayload::ActorWord7a(field)
    );
    assert_eq!(
        decode(3, 15, &word).unwrap().payload,
        InstanceCommandPayload::ActorWord76(field)
    );
    assert_eq!(
        decode(3, 4, &[1, 0, 2, 0, 0, 0x80]).unwrap().payload,
        InstanceCommandPayload::ActorWords8e(InstanceField {
            span: ByteSpan::new(13, 19),
            value: [1, 2, 0x8000]
        })
    );
}

#[test]
fn signed_switch_and_shift_inputs_remain_raw_without_running_state_policy() {
    let value = -65535i32;
    let field = InstanceField {
        span: ByteSpan::new(13, 17),
        value,
    };
    assert_eq!(
        decode(3, 6, &value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::ActorSelectorB4(field)
    );
    assert_eq!(
        decode(3, 10, &value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::ActorShiftedByte7c(field)
    );
    assert_eq!(
        decode(3, 11, &value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::ActorShiftedByte7d(field)
    );
    assert_eq!(
        decode(3, 12, &value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::ActorStateC4(field)
    );
}

#[test]
fn no_read_and_unclassified_paths_never_guess_payload_length() {
    for selector in [1, 3, 8, 12, 22, 23, 67] {
        for code in [0, 1] {
            assert_eq!(
                decode(selector, code, &[]).unwrap().payload,
                InstanceCommandPayload::NoPayloadRead
            );
        }
    }
    for selector in [3, 8, 12, 23] {
        assert_eq!(
            decode(selector, 3, &[]).unwrap().payload,
            InstanceCommandPayload::NoPayloadRead
        );
        for code in [-1, i32::MAX] {
            assert_eq!(
                decode(selector, code, &[]).unwrap().payload,
                InstanceCommandPayload::Opaque
            );
        }
    }
}

#[test]
fn conditional_bit_commands_preserve_unsigned_indices_and_explicit_missing_operands() {
    for selector in [3, 8, 12, 23, 60, 69] {
        for value in [0u16, 31, 32, 0x8000, 0xffff] {
            let field = ConditionalInstanceWord::Available(InstanceField {
                span: ByteSpan::new(13, 15),
                value,
            });
            let unless = decode(selector, 13, &value.to_le_bytes()).unwrap();
            assert_eq!(unless.payload_consumer, Some(0x0047_ada0));
            assert_eq!(
                unless.payload,
                InstanceCommandPayload::ActorDisableUnlessBit(field)
            );
            assert_eq!(
                decode(selector, 14, &value.to_le_bytes()).unwrap().payload,
                InstanceCommandPayload::ActorDisableIfBit(field)
            );
        }
        for bytes in [vec![], vec![0x80]] {
            let missing = ConditionalInstanceWord::OutsideAllocation {
                allocation_offset: 4,
            };
            assert_eq!(
                decode(selector, 13, &bytes).unwrap().payload,
                InstanceCommandPayload::ActorDisableUnlessBit(missing)
            );
            assert_eq!(
                decode(selector, 14, &bytes).unwrap().payload,
                InstanceCommandPayload::ActorDisableIfBit(missing)
            );
        }
    }
    for selector in [0, 1, 22, 67, 95] {
        for code in [13, 14] {
            assert_eq!(
                decode(selector, code, &[0xff; 2]).unwrap().payload,
                InstanceCommandPayload::Opaque
            );
        }
    }
}

#[test]
fn shared_reads_are_bounded_against_main_allocation_and_preserve_independent_context() {
    for (code, length) in [
        (2, 1),
        (4, 5),
        (5, 1),
        (6, 3),
        (7, 1),
        (8, 1),
        (9, 1),
        (10, 3),
        (11, 3),
        (12, 3),
        (15, 1),
    ] {
        assert!(matches!(
            decode(3, code, &vec![0; length]),
            Err(InstanceAssetError::OutOfBounds { .. })
        ));
    }
    let source = [0; 12];
    for span in [ByteSpan::new(8, 4), ByteSpan::new(0, 13)] {
        assert!(matches!(
            decode_instance_command(InstanceCommandRequest {
                read_scope: crate::InstanceCommandReadScope::Inspection,
                source: &source,
                main_payload: span,
                relocations: &[],
                command_offset: 0,
                target: InstanceCommandTarget::Constructor(3)
            }),
            Err(InstanceAssetError::InvalidAllocation { .. })
        ));
    }
    assert!(matches!(
        decode_instance_command(InstanceCommandRequest {
            read_scope: crate::InstanceCommandReadScope::Inspection,
            source: &source,
            main_payload: ByteSpan::new(4, 12),
            relocations: &[],
            command_offset: 5,
            target: InstanceCommandTarget::Constructor(3)
        }),
        Err(InstanceAssetError::OutOfBounds {
            context: "instance command code",
            ..
        })
    ));
}

#[test]
fn specialized_fields_follow_the_actual_selector_and_exact_read_width() {
    let signed = InstanceField {
        span: ByteSpan::new(13, 17),
        value: i32::MIN + 1,
    };
    assert_eq!(
        decode(69, 16, &signed.value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::Selector69DoubledDword14c(signed)
    );
    assert_eq!(
        decode(69, 17, &signed.value.to_le_bytes()).unwrap().payload,
        InstanceCommandPayload::Selector69Dword144(signed)
    );
    for selector in [
        2, 7, 8, 9, 10, 11, 12, 13, 21, 33, 34, 35, 36, 60, 77, 79, 80,
    ] {
        assert_eq!(
            decode(selector, 16, &[]).unwrap().payload,
            InstanceCommandPayload::NoPayloadRead
        );
        let view = decode(selector, 17, &0x8001u16.to_le_bytes()).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0042_6030));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::ActorWord164(InstanceField {
                span: ByteSpan::new(13, 15),
                value: 0x8001,
            })
        );
        assert_eq!(
            decode(selector, 18, &u32::MAX.to_le_bytes())
                .unwrap()
                .payload,
            InstanceCommandPayload::ActorNegatedDword144(InstanceField {
                span: ByteSpan::new(13, 17),
                value: u32::MAX,
            })
        );
    }
    for selector in [3, 69] {
        assert_eq!(
            decode(selector, 18, &[]).unwrap().payload,
            InstanceCommandPayload::Opaque
        );
    }
    // 0040D9C0 reads the refill BYTE for its own selector 23 and through
    // 0042F100/0040A1D0/0043B7D0 for selectors 15, 37 and 39.
    for selector in [15, 23, 37, 39] {
        let view = decode(selector, 18, &[1]).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0040_d9c0));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::Actor40d800Refill(InstanceField {
                span: ByteSpan::new(13, 14),
                value: 1,
            })
        );
        assert!(matches!(
            decode(selector, 18, &[]),
            Err(InstanceAssetError::OutOfBounds { .. })
        ));
    }
    // 0042F100: code 21 reads nothing, 22 a DWORD, 23..25 a grant WORD; 26
    // is rejected without a read.
    let view = decode(15, 21, &[]).unwrap();
    assert_eq!(view.payload_consumer, Some(0x0042_f100));
    assert_eq!(view.payload, InstanceCommandPayload::NoPayloadRead);
    assert_eq!(
        decode(15, 22, &0x0102_0304u32.to_le_bytes())
            .unwrap()
            .payload,
        InstanceCommandPayload::Selector15Dword15c(InstanceField {
            span: ByteSpan::new(13, 17),
            value: 0x0102_0304,
        })
    );
    for code in 23..=25 {
        assert_eq!(
            decode(15, code, &0x25u16.to_le_bytes()).unwrap().payload,
            InstanceCommandPayload::Selector15GrantWord(InstanceField {
                span: ByteSpan::new(13, 15),
                value: 0x25,
            })
        );
    }
    assert_eq!(
        decode(15, 26, &[]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
    // 0040A1D0 code 23 sign-extends a WORD; code 24 reads one BYTE.
    let view = decode(37, 23, &0x8001u16.to_le_bytes()).unwrap();
    assert_eq!(view.payload_consumer, Some(0x0040_a1d0));
    assert_eq!(
        view.payload,
        InstanceCommandPayload::Selector37SignedWord160(InstanceField {
            span: ByteSpan::new(13, 15),
            value: 0x8001u16 as i16,
        })
    );
    assert_eq!(
        decode(37, 24, &[7]).unwrap().payload,
        InstanceCommandPayload::Selector37Seconds164(InstanceField {
            span: ByteSpan::new(13, 14),
            value: 7,
        })
    );
    // 0043B7D0 code 21 retains the operand address without a read; it has
    // no case 26.
    let view = decode(39, 21, &[]).unwrap();
    assert_eq!(view.payload_consumer, Some(0x0043_b7d0));
    assert_eq!(
        view.payload,
        InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: 4
        }
    );
    assert_eq!(
        decode(39, 26, &[1]).unwrap().payload,
        InstanceCommandPayload::Opaque
    );
    for (selector, code, length) in [
        (69, 16, 3),
        (69, 17, 3),
        (60, 17, 1),
        (60, 18, 3),
        (14, 2, 1),
    ] {
        assert!(matches!(
            decode(selector, code, &vec![0; length]),
            Err(InstanceAssetError::OutOfBounds { .. })
        ));
    }
}

fn spawn_source(count: i32, rows: usize) -> (Vec<u8>, ByteSpan, Vec<AssetRelocation>) {
    let mut source = vec![0xcc; 9 + 32 + rows * 24];
    source[9..13].copy_from_slice(&19i32.to_le_bytes());
    source[13..15].copy_from_slice(&(-2i16).to_le_bytes());
    source[17..21].copy_from_slice(&count.to_le_bytes());
    source[21..25].copy_from_slice(&32u32.to_le_bytes());
    let main_payload = ByteSpan::new(9, source.len());
    source.extend_from_slice(&[0xee; 64]);
    let relocations = vec![AssetRelocation {
        table_span: ByteSpan::new(main_payload.end, main_payload.end + 4),
        field_offset: 12,
        field_span: ByteSpan::new(21, 25),
        target_offset: 32,
    }];
    (source, main_payload, relocations)
}

fn spawn_decode(
    source: &[u8],
    main_payload: ByteSpan,
    relocations: &[AssetRelocation],
) -> Result<InstanceCommandView, InstanceAssetError> {
    decode_instance_command(InstanceCommandRequest {
        read_scope: crate::InstanceCommandReadScope::Inspection,
        source,
        main_payload,
        relocations,
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(60),
    })
}

#[test]
fn selector60_dispatch_retains_before_any_header_or_row_read() {
    use crate::InstanceCommandReadScope;
    let source = 19_i32.to_le_bytes();
    let request = InstanceCommandRequest {
        source: &source,
        main_payload: ByteSpan::new(0, 4),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(60),
        read_scope: InstanceCommandReadScope::Dispatch,
    };
    assert_eq!(
        decode_instance_command(InstanceCommandRequest { ..request })
            .unwrap()
            .payload,
        InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: 4
        }
    );
    assert!(matches!(
        decode_instance_command(InstanceCommandRequest {
            read_scope: InstanceCommandReadScope::Inspection,
            ..request
        }),
        Err(InstanceAssetError::OutOfBounds { .. })
    ));
    let (source, main_payload, _) = spawn_source(1, 0);
    assert!(decode_instance_command(InstanceCommandRequest {
        source: &source,
        main_payload,
        ..request
    })
    .is_ok());
    assert!(spawn_decode(&source, main_payload, &[]).is_err());
}

#[test]
fn spawn_rows_preserve_raw_signed_inputs_placeholders_and_absolute_source_spans() {
    let (mut source, span, relocations) = spawn_source(2, 2);
    for index in 0..2 {
        let row = 41 + index * 24;
        source[row..row + 4].copy_from_slice(&(-2i32).to_le_bytes());
        for (word, value) in [-1i16, 0, i16::MIN, 3, -4, i16::MAX]
            .into_iter()
            .enumerate()
        {
            source[row + 4 + word * 2..row + 6 + word * 2].copy_from_slice(&value.to_le_bytes());
        }
        source[row + 16] = 0xff;
        source[row + 20..row + 24].copy_from_slice(&0xdeadbeefu32.to_le_bytes());
    }
    let original = source.clone();
    let view = spawn_decode(&source, span, &relocations).unwrap();
    assert_eq!(view.proven_method, Some(0x0048_0b30));
    assert_eq!(view.payload_consumer, Some(0x0048_0b30));
    let InstanceCommandPayload::PlacementSpawnList(list) = view.payload else {
        panic!("spawn list")
    };
    assert_eq!(list.header_span, ByteSpan::new(13, 25));
    assert_eq!(
        list.mode_word,
        InstanceField {
            span: ByteSpan::new(13, 15),
            value: -2
        }
    );
    assert_eq!(
        list.count,
        InstanceField {
            span: ByteSpan::new(17, 21),
            value: 2
        }
    );
    assert_eq!(
        list.rows_pointer,
        Some(InstanceField {
            span: ByteSpan::new(21, 25),
            value: 32
        })
    );
    assert_eq!(list.rows_span, Some(ByteSpan::new(41, 89)));
    assert_eq!(list.rows.len(), 2);
    for (index, row) in list.rows.iter().enumerate() {
        let start = 41 + index * 24;
        assert_eq!(row.index, index as u32);
        assert_eq!(row.span, ByteSpan::new(start, start + 24));
        assert_eq!(
            row.placement_reference,
            InstanceField {
                span: ByteSpan::new(start, start + 4),
                value: -2
            }
        );
        assert_eq!(
            row.signed_words_04_to_0e.map(|f| f.value),
            [-1, 0, i16::MIN, 3, -4, i16::MAX]
        );
        for (word, field) in row.signed_words_04_to_0e.iter().enumerate() {
            assert_eq!(
                field.span,
                ByteSpan::new(start + 4 + word * 2, start + 6 + word * 2)
            );
        }
        assert_eq!(
            row.flags,
            InstanceField {
                span: ByteSpan::new(start + 16, start + 17),
                value: 0xff
            }
        );
        assert_eq!(
            row.runtime_pointer_placeholder,
            InstanceField {
                span: ByteSpan::new(start + 20, start + 24),
                value: 0xdeadbeef
            }
        );
    }
    assert_eq!(
        source, original,
        "never apply the original initializer's writes"
    );
    assert_eq!(
        spawn_decode(&source, span, &relocations).unwrap().payload,
        InstanceCommandPayload::PlacementSpawnList(list)
    );
}

#[test]
fn nonpositive_spawn_count_does_not_read_a_pointer_or_rows() {
    for count in [i32::MIN, -1, 0] {
        let mut payload = vec![0; 8];
        payload[4..8].copy_from_slice(&count.to_le_bytes());
        let InstanceCommandPayload::PlacementSpawnList(list) =
            decode(60, 19, &payload).unwrap().payload
        else {
            panic!("spawn list")
        };
        assert_eq!(list.count.value, count);
        assert_eq!(list.header_span, ByteSpan::new(13, 21));
        assert_eq!(list.rows_pointer, None);
        assert_eq!(list.rows_span, None);
        assert!(list.rows.is_empty());
    }
    assert!(matches!(
        decode(60, 19, &[0; 7]),
        Err(InstanceAssetError::OutOfBounds { .. })
    ));
}

#[test]
fn spawn_pointer_requires_an_exact_relocation_and_rows_stay_inside_main_allocation() {
    let (mut source, span, relocations) = spawn_source(1, 1);
    assert!(matches!(
        spawn_decode(&source, span, &[]),
        Err(InstanceAssetError::MissingCommandRelocation(12))
    ));
    for mutation in 0..3 {
        let mut forged = relocations.clone();
        match mutation {
            0 => forged[0].field_offset += 4,
            1 => forged[0].field_span.start += 1,
            _ => forged[0].target_offset += 4,
        }
        assert!(matches!(
            spawn_decode(&source, span, &forged),
            Err(InstanceAssetError::MissingCommandRelocation(12))
        ));
    }
    assert!(matches!(
        spawn_decode(
            &source,
            ByteSpan::new(span.start, span.end - 1),
            &relocations
        ),
        Err(InstanceAssetError::OutOfBounds {
            context: "spawn list rows",
            ..
        })
    ));
    source[17..21].copy_from_slice(&i32::MAX.to_le_bytes());
    assert!(matches!(
        spawn_decode(&source, span, &relocations),
        Err(InstanceAssetError::OutOfBounds {
            context: "spawn list rows",
            ..
        }) | Err(InstanceAssetError::IntegerOverflow("spawn list rows"))
    ));
    source[17..21].copy_from_slice(&1i32.to_le_bytes());
    source[21..25].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut outside = relocations;
    outside[0].target_offset = u32::MAX;
    assert!(matches!(
        spawn_decode(&source, span, &outside),
        Err(InstanceAssetError::OutOfBounds {
            context: "spawn list rows",
            ..
        })
    ));
}

#[test]
fn selector11_commands_keep_the_native_operand_widths_and_all_ten_copy_bytes() {
    for (code, operand, expected) in [
        (
            19,
            0x8000_0000_u32.to_le_bytes().to_vec(),
            InstanceCommandPayload::Selector11Flag02(InstanceField {
                span: ByteSpan::new(13, 17),
                value: 0x8000_0000,
            }),
        ),
        (
            20,
            vec![0, 255, 128, 129, 1, 2, 3, 4, 5, 6],
            InstanceCommandPayload::Selector11Block1a0(InstanceField {
                span: ByteSpan::new(13, 23),
                value: [0, 255, 128, 129, 1, 2, 3, 4, 5, 6],
            }),
        ),
        (
            21,
            0xffff_0123_u32.to_le_bytes().to_vec(),
            InstanceCommandPayload::Selector11BehaviorWord04(ConditionalInstanceDword::Available(
                InstanceField {
                    span: ByteSpan::new(13, 17),
                    value: 0xffff_0123,
                },
            )),
        ),
        (
            22,
            0x0100_u16.to_le_bytes().to_vec(),
            InstanceCommandPayload::Selector11Flag04(InstanceField {
                span: ByteSpan::new(13, 15),
                value: 0x0100,
            }),
        ),
        (
            23,
            0x8000_u16.to_le_bytes().to_vec(),
            InstanceCommandPayload::Selector11Flag10(InstanceField {
                span: ByteSpan::new(13, 15),
                value: 0x8000,
            }),
        ),
    ] {
        let view = decode(11, code, &operand).unwrap();
        assert_eq!(view.proven_method, Some(0x0040_2330));
        assert_eq!(view.payload_consumer, Some(0x0040_2330));
        assert_eq!(view.payload, expected);
        // Bytes immediately beyond each operand are irrelevant to this consumer.
        let mut extended = operand;
        extended.extend_from_slice(&[0xaa; 4]);
        assert_eq!(decode(11, code, &extended).unwrap().payload, expected);
    }
}

#[test]
fn selector11_conditional_behavior_read_remains_distinct_from_required_operands() {
    for (code, width) in [(19, 4), (20, 10), (22, 2), (23, 2)] {
        for length in 0..width {
            assert!(
                matches!(
                    decode(11, code, &vec![0; length]),
                    Err(InstanceAssetError::OutOfBounds { .. })
                ),
                "selector11 code {code} missing byte {length} must not use relocation tail"
            );
        }
    }
    for length in 0..4 {
        let view = decode(11, 21, &vec![0; length]).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0040_2330));
        assert_eq!(
            view.payload,
            InstanceCommandPayload::Selector11BehaviorWord04(
                ConditionalInstanceDword::OutsideAllocation {
                    allocation_offset: 4
                }
            )
        );
    }
    for code in [i32::MIN, -1, 24, i32::MAX] {
        let view = decode(11, code, &[]).unwrap();
        assert_eq!(view.proven_method, Some(0x0040_2330));
        assert_eq!(view.payload_consumer, None);
        assert_eq!(view.payload, InstanceCommandPayload::Opaque);
    }
    for (code, operand, consumer) in [
        (3, vec![], 0x0047_ada0),
        (16, vec![], 0x0042_6030),
        (17, vec![0xff; 2], 0x0042_6030),
        (18, vec![0x80; 4], 0x0042_6030),
    ] {
        let view = decode(11, code, &operand).unwrap();
        assert_eq!(view.proven_method, Some(0x0040_2330));
        assert_eq!(view.payload_consumer, Some(consumer));
    }
}
