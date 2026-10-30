use super::*;

fn command(
    target: InstanceCommandTarget,
    code: i32,
    payload: &[u8],
    scope: InstanceCommandReadScope,
) -> Result<InstanceCommandView, InstanceAssetError> {
    let mut source = vec![0xa5; 5];
    source.extend_from_slice(&code.to_le_bytes());
    source.extend_from_slice(payload);
    let main_payload = ByteSpan::new(5, source.len());
    source.extend_from_slice(&[0xee; 16]);
    decode_instance_command(InstanceCommandRequest {
        source: &source,
        main_payload,
        relocations: &[],
        command_offset: 0,
        target,
        read_scope: scope,
    })
}

#[test]
fn selector30_copies_exact_byte_word_and_six_byte_widths_without_runtime_coercion() {
    use InstanceCommandPayload::*;
    macro_rules! byte {
        ($variant:ident,$value:expr) => {
            $variant(InstanceField {
                span: ByteSpan::new(9, 10),
                value: $value,
            })
        };
    }
    macro_rules! word {
        ($variant:ident,$value:expr) => {
            $variant(InstanceField {
                span: ByteSpan::new(9, 11),
                value: $value,
            })
        };
    }
    let cases = vec![
        (16, vec![0x80], byte!(Actor40d800Maximum, -128)),
        (17, vec![0xff], byte!(Actor40d800Flag14e, 255)),
        (18, vec![0x80], byte!(Actor40d800Refill, 128)),
        (19, vec![1, 0x80], word!(Actor40d800Word144, 0x8001)),
        (
            20,
            vec![1, 0x80, 0xff, 0xff, 7, 0],
            Actor40d800Words146(InstanceField {
                span: ByteSpan::new(9, 15),
                value: [0x8001, 0xffff, 7],
            }),
        ),
        (21, vec![1, 0x80], word!(Selector30Placement180, 0x8001)),
        (22, vec![0xff], byte!(Selector30Byte15c, 255)),
        (23, vec![0xfe], byte!(Selector30Byte160, 254)),
        (24, vec![0xfd], byte!(Selector30Byte164, 253)),
        (25, vec![0xfc], byte!(Selector30Byte18f, 252)),
        (26, vec![0xfb], byte!(Selector30Byte167, 251)),
        (27, vec![0xfa], byte!(Selector30Flag18e, 250)),
        (28, vec![0xf9], byte!(Selector30Flag166, 249)),
        (29, vec![0xf8], byte!(Selector30Flag190, 248)),
        (30, vec![0xf7], byte!(Selector30Flag191, 247)),
        (31, vec![0xf6], byte!(Selector30Flag192, 246)),
        (32, vec![1, 0x80], word!(Selector30Word18c, 0x8001)),
    ];
    for (code, bytes, expected) in cases {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            let view =
                command(InstanceCommandTarget::Constructor(30), code, &bytes, scope).unwrap();
            assert_eq!(view.proven_method, Some(0x0048_bbf0));
            assert_eq!(
                view.payload_consumer,
                Some(if code <= 20 { 0x0040_d9c0 } else { 0x0048_bbf0 })
            );
            assert_eq!(view.payload, expected, "code {code}");
            for length in 0..bytes.len() {
                assert!(
                    matches!(
                        command(
                            InstanceCommandTarget::Constructor(30),
                            code,
                            &bytes[..length],
                            scope
                        ),
                        Err(InstanceAssetError::OutOfBounds { .. })
                    ),
                    "code {code} length {length}"
                );
            }
        }
    }
    for code in [i32::MIN, -1, 33, i32::MAX] {
        assert_eq!(
            command(
                InstanceCommandTarget::Constructor(30),
                code,
                &[],
                InstanceCommandReadScope::Inspection
            )
            .unwrap()
            .payload,
            Opaque
        );
    }
}

#[test]
fn family_home_fields_and_placement_word_preserve_raw_bits_and_exact_read_frontiers() {
    use InstanceCommandPayload::*;
    for target in [
        InstanceCommandTarget::Family428650,
        InstanceCommandTarget::Family4236d0,
        InstanceCommandTarget::Constructor(8),
        InstanceCommandTarget::Constructor(9),
        InstanceCommandTarget::Constructor(10),
        InstanceCommandTarget::Constructor(12),
        InstanceCommandTarget::Constructor(13),
        InstanceCommandTarget::Constructor(21),
        InstanceCommandTarget::Constructor(34),
        InstanceCommandTarget::Constructor(35),
        InstanceCommandTarget::Constructor(36),
    ] {
        let cases = vec![
            (
                20,
                vec![1, 0x80, 0xff, 0xff],
                FamilyHomeTerrainXZ(InstanceField {
                    span: ByteSpan::new(9, 13),
                    value: [0x8001, 0xffff],
                }),
            ),
            (
                21,
                vec![1, 0x80, 0xff, 0xff, 7, 0],
                FamilyHomeTerrainXYZ(InstanceField {
                    span: ByteSpan::new(9, 15),
                    value: [0x8001, 0xffff, 7],
                }),
            ),
            (
                22,
                0x80010000_u32.to_le_bytes().to_vec(),
                FamilyPlacementReference(InstanceField {
                    span: ByteSpan::new(9, 13),
                    value: 0x80010000,
                }),
            ),
        ];
        for (code, bytes, expected) in cases {
            let view = command(target, code, &bytes, InstanceCommandReadScope::Dispatch).unwrap();
            assert_eq!(view.payload_consumer, Some(0x0042_76b0));
            assert_eq!(view.payload, expected);
            for length in 0..bytes.len() {
                assert!(matches!(
                    command(
                        target,
                        code,
                        &bytes[..length],
                        InstanceCommandReadScope::Dispatch
                    ),
                    Err(InstanceAssetError::OutOfBounds { .. })
                ));
            }
        }
        assert_eq!(
            command(target, 19, &[], InstanceCommandReadScope::Dispatch)
                .unwrap()
                .payload,
            NoPayloadRead
        );
        assert_eq!(
            command(target, 16, &[], InstanceCommandReadScope::Dispatch)
                .unwrap()
                .payload,
            NoPayloadRead
        );
        assert!(matches!(
            command(
                target,
                17,
                &[0x80, 0xff],
                InstanceCommandReadScope::Dispatch
            )
            .unwrap()
            .payload,
            ActorWord164(InstanceField { value: 0xff80, .. })
        ));
    }
}

#[test]
fn family4236d0_accepts_no_read_code23_and_retains_code24_without_tail_reads() {
    use InstanceCommandPayload::*;
    for target in [
        InstanceCommandTarget::Family4236d0,
        InstanceCommandTarget::Constructor(8),
        InstanceCommandTarget::Constructor(9),
        InstanceCommandTarget::Constructor(10),
        InstanceCommandTarget::Constructor(21),
        InstanceCommandTarget::Constructor(36),
    ] {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            for (code, expected) in [
                (23, NoPayloadRead),
                (
                    24,
                    RetainedPayloadPointer {
                        allocation_offset: 4,
                    },
                ),
            ] {
                let view = command(target, code, &[], scope).unwrap();
                assert_eq!(view.payload_consumer, Some(0x0042_39e0));
                assert_eq!(view.payload, expected);
                assert_eq!(
                    view.proven_method,
                    Some(
                        if matches!(target, InstanceCommandTarget::Constructor(10 | 21)) {
                            0x0042_5240
                        } else {
                            0x0042_39e0
                        }
                    )
                );
            }
            for code in [i32::MIN, -1, 25, i32::MAX] {
                if code == 25 && matches!(target, InstanceCommandTarget::Constructor(10 | 21)) {
                    continue;
                }
                let view = command(target, code, &[], scope).unwrap();
                assert_eq!(view.payload_consumer, None);
                assert_eq!(view.payload, Opaque);
            }
        }
    }
}

#[test]
fn selector12_and13_retain_command23_through_the_exact_shared_consumer() {
    use InstanceCommandPayload::*;
    for selector in [12, 13] {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            let view =
                command(InstanceCommandTarget::Constructor(selector), 23, &[], scope).unwrap();
            assert_eq!(
                view.proven_method,
                Some(if selector == 12 {
                    0x0046_72c0
                } else {
                    0x0046_89d0
                })
            );
            assert_eq!(view.payload_consumer, Some(0x0046_72c0));
            assert_eq!(
                view.payload,
                RetainedPayloadPointer {
                    allocation_offset: 4
                }
            );
            // Selector13's higher overrides remain opaque; no extra reads are inferred.
            for code in [i32::MIN, -1, 24, i32::MAX] {
                let view = command(
                    InstanceCommandTarget::Constructor(selector),
                    code,
                    &[],
                    scope,
                )
                .unwrap();
                assert_eq!(view.payload_consumer, None);
                assert_eq!(view.payload, Opaque);
            }
        }
    }
}

#[test]
fn selector10_and21_command25_accept_without_any_operand_read() {
    use InstanceCommandPayload::*;
    for target in [
        InstanceCommandTarget::Constructor(10),
        InstanceCommandTarget::Constructor(21),
    ] {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            let view = command(target, 25, &[], scope).unwrap();
            assert_eq!(view.proven_method, Some(0x0042_5240));
            assert_eq!(view.payload_consumer, Some(0x0042_5240));
            assert_eq!(view.payload, NoPayloadRead);
            for code in [26, i32::MAX] {
                let view = command(target, code, &[], scope).unwrap();
                assert_eq!(view.payload_consumer, None);
                assert_eq!(view.payload, Opaque);
            }
        }
    }
}

#[test]
fn selector34_retains_command23_and_rejects_higher_signed_codes_without_reading() {
    use InstanceCommandPayload::*;
    for scope in [
        InstanceCommandReadScope::Inspection,
        InstanceCommandReadScope::Dispatch,
    ] {
        let view = command(InstanceCommandTarget::Constructor(34), 23, &[], scope).unwrap();
        assert_eq!(view.proven_method, Some(0x0043_3c90));
        assert_eq!(view.payload_consumer, Some(0x0043_3c90));
        assert_eq!(
            view.payload,
            RetainedPayloadPointer {
                allocation_offset: 4
            }
        );
        for code in [i32::MIN, -1, 24, 25, i32::MAX] {
            let view = command(InstanceCommandTarget::Constructor(34), code, &[], scope).unwrap();
            assert_eq!(view.proven_method, Some(0x0043_3c90));
            assert_eq!(view.payload_consumer, None);
            assert_eq!(view.payload, Opaque, "code {code}");
        }
    }
}

#[test]
fn selector35_retained_operand_reads_nothing_and_packed_word_remains_unmodified() {
    use InstanceCommandPayload::*;
    for scope in [
        InstanceCommandReadScope::Inspection,
        InstanceCommandReadScope::Dispatch,
    ] {
        let view = command(InstanceCommandTarget::Constructor(35), 23, &[], scope).unwrap();
        assert_eq!(view.payload_consumer, Some(0x0043_d270));
        assert_eq!(
            view.payload,
            RetainedPayloadPointer {
                allocation_offset: 4
            }
        );
        let bytes = 0xabcd1234_u32.to_le_bytes();
        assert_eq!(
            command(InstanceCommandTarget::Constructor(35), 24, &bytes, scope)
                .unwrap()
                .payload,
            Selector35Packed240(InstanceField {
                span: ByteSpan::new(9, 13),
                value: 0xabcd1234
            })
        );
        for length in 0..4 {
            assert!(command(
                InstanceCommandTarget::Constructor(35),
                24,
                &bytes[..length],
                scope
            )
            .is_err());
        }
    }
    assert_eq!(
        command(
            InstanceCommandTarget::Family428650,
            23,
            &[],
            InstanceCommandReadScope::Dispatch
        )
        .unwrap()
        .payload,
        Opaque
    );
    assert_eq!(
        command(
            InstanceCommandTarget::Constructor(35),
            25,
            &[],
            InstanceCommandReadScope::Dispatch
        )
        .unwrap()
        .payload,
        Opaque
    );
}

#[test]
fn selector74_reads_one_word_for_every_code_above_15() {
    use InstanceCommandPayload::*;
    let field = |value| InstanceField {
        span: ByteSpan::new(9, 11),
        value,
    };
    for (code, expected) in [
        (16, Selector74Width(field(0x8001))),
        (17, Selector74Depth(field(0x8001))),
        (18, Selector74Height(field(0x8001))),
        (19, Selector74Refused(field(0x8001))),
        (i32::MAX, Selector74Refused(field(0x8001))),
    ] {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            let view = command(
                InstanceCommandTarget::Constructor(74),
                code,
                &[1, 0x80],
                scope,
            )
            .unwrap();
            assert_eq!(view.proven_method, Some(0x0042_d6f0));
            assert_eq!(view.payload_consumer, Some(0x0042_d6f0));
            assert_eq!(view.payload, expected, "code {code}");
            assert!(matches!(
                command(InstanceCommandTarget::Constructor(74), code, &[1], scope),
                Err(InstanceAssetError::OutOfBounds { .. })
            ));
        }
    }
    // Codes through 15, negative ones too, are the shared actor's 0047ADA0.
    for code in [15, -1] {
        let view = command(
            InstanceCommandTarget::Constructor(74),
            code,
            &[1, 0x80],
            InstanceCommandReadScope::Dispatch,
        )
        .unwrap();
        assert_ne!(view.payload_consumer, Some(0x0042_d6f0), "code {code}");
    }
}

#[test]
fn selector79_reads_code_19s_partner_and_leaves_lower_codes_to_selector_77() {
    use InstanceCommandPayload::*;
    let view = command(
        InstanceCommandTarget::Constructor(79),
        19,
        &[0x5b, 0, 0, 0],
        InstanceCommandReadScope::Dispatch,
    )
    .unwrap();
    assert_eq!(view.payload_consumer, Some(0x0040_8880));
    assert_eq!(
        view.payload,
        Selector79Link(InstanceField {
            span: ByteSpan::new(9, 13),
            value: 0x5b,
        })
    );
    assert!(matches!(
        command(
            InstanceCommandTarget::Constructor(79),
            19,
            &[0x5b, 0, 0],
            InstanceCommandReadScope::Dispatch
        ),
        Err(InstanceAssetError::OutOfBounds { .. })
    ));
    // 00408880 passes codes through 18 to selector 77's 00426030.
    let view = command(
        InstanceCommandTarget::Constructor(79),
        17,
        &[0x84, 0x03],
        InstanceCommandReadScope::Dispatch,
    )
    .unwrap();
    assert_eq!(view.payload_consumer, Some(0x0042_6030));
}

#[test]
fn selector84_reads_code_16s_link_and_refuses_later_codes_unread() {
    use InstanceCommandPayload::*;
    for scope in [
        InstanceCommandReadScope::Inspection,
        InstanceCommandReadScope::Dispatch,
    ] {
        let view = command(
            InstanceCommandTarget::Constructor(84),
            16,
            &[5, 0, 0, 0x80],
            scope,
        )
        .unwrap();
        assert_eq!(view.proven_method, Some(0x0043_c2a0));
        assert_eq!(view.payload_consumer, Some(0x0043_c2a0));
        assert_eq!(
            view.payload,
            Selector84Link(InstanceField {
                span: ByteSpan::new(9, 13),
                value: 0x8000_0005,
            })
        );
        assert!(matches!(
            command(
                InstanceCommandTarget::Constructor(84),
                16,
                &[5, 0, 0],
                scope
            ),
            Err(InstanceAssetError::OutOfBounds { .. })
        ));
    }
    // 0043C2A0 refuses codes above 16 without a read, and codes through 15,
    // negative ones too, are the shared actor's 0047ADA0.
    for code in [17, i32::MAX, 15, -1] {
        let view = command(
            InstanceCommandTarget::Constructor(84),
            code,
            &[],
            InstanceCommandReadScope::Dispatch,
        );
        if let Ok(view) = view {
            assert_ne!(view.payload_consumer, Some(0x0043_c2a0), "code {code}");
        }
    }
}

#[test]
fn selector53_copies_one_byte_per_spawner_field_and_refuses_later_codes_unread() {
    use InstanceCommandPayload::*;
    let field = |value| InstanceField {
        span: ByteSpan::new(9, 10),
        value,
    };
    for (code, expected) in [
        (16, Selector53Kind138(field(0x80))),
        (17, Selector53Count139(field(0x80))),
        (18, Selector53Seconds13a(field(0x80))),
        (19, Selector53Repeat13b(field(0x80))),
    ] {
        for scope in [
            InstanceCommandReadScope::Inspection,
            InstanceCommandReadScope::Dispatch,
        ] {
            let view =
                command(InstanceCommandTarget::Constructor(53), code, &[0x80], scope).unwrap();
            assert_eq!(view.proven_method, Some(0x0045_4a30));
            assert_eq!(view.payload_consumer, Some(0x0045_4a30));
            assert_eq!(view.payload, expected, "code {code}");
            assert!(matches!(
                command(InstanceCommandTarget::Constructor(53), code, &[], scope),
                Err(InstanceAssetError::OutOfBounds { .. })
            ));
        }
    }
    // Codes through 15 are the shared actor's 0047ADA0.
    let view = command(
        InstanceCommandTarget::Constructor(53),
        15,
        &[1, 0x80],
        InstanceCommandReadScope::Dispatch,
    )
    .unwrap();
    assert_eq!(view.payload_consumer, Some(0x0047_ada0));
    assert!(matches!(
        view.payload,
        ActorWord76(InstanceField { value: 0x8001, .. })
    ));
    for code in [20, i32::MAX] {
        assert_eq!(
            command(
                InstanceCommandTarget::Constructor(53),
                code,
                &[],
                InstanceCommandReadScope::Dispatch
            )
            .unwrap()
            .payload,
            Opaque
        );
    }
}
