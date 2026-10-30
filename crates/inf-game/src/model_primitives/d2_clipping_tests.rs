use super::*;
use crate::model_primitives::d2::prepare_d2;

#[test]
fn actual_d2_callers_match_original_three_source_geometry_packets_and_prefixes() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../tests/fixtures/d2_clipping_native.json")).unwrap();
    assert_eq!(
        receipts.exe_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(receipts.cases.len(), 98);
    let tables = tables();
    for receipt in &receipts.cases {
        let input = &receipt.input;
        let points: Vec<_> = input.vertices.iter().copied().map(point).collect();
        let scratch = clip_polygon::<&'static str>(
            ClipSource::Triangle(std::array::from_fn(|i| {
                (
                    &points[i],
                    crate::model_lighting::ModelShade::from_word(input.scratch_colors[i]),
                )
            })),
            state(input).projection,
            &tables,
            0xd2,
        );
        if receipt.error.as_deref() == Some("division") {
            assert!(
                matches!(
                    scratch,
                    Err(PrimitiveError::Unsupported { opcode: 0xd2, .. })
                ),
                "{}",
                input.name
            );
        } else {
            let actual = scratch.unwrap();
            let expected = receipt.scratch.as_ref().unwrap();
            assert_eq!(actual.len(), expected.len(), "{} scratch count", input.name);
            for (actual, expected) in actual.iter().zip(expected) {
                assert_eq!(actual.projected.xy, expected.xy, "{} XY", input.name);
                assert_eq!(
                    actual.projected.depth, expected.depth,
                    "{} depth",
                    input.name
                );
                assert_eq!(actual.projected.clip, expected.clip, "{} clip", input.name);
                assert_eq!(
                    actual.projected.fog,
                    Some(expected.fog),
                    "{} fog",
                    input.name
                );
                assert_eq!(actual.uv, expected.uv, "{} UV", input.name);
                assert_eq!(actual.high_byte, expected.high, "{} high", input.name);
                assert_eq!(actual.rgb, Some(expected.rgb), "{} RGB", input.name);
            }
        }
        let fixture = Fixture::new(input);
        let sprite = sprite(input);
        let mut host = host(input, &sprite, receipts.texture_address);
        let result = prepare_d2(
            &fixture.request(input.far),
            state(input),
            &tables,
            &mut host,
        );
        assert_eq!(
            result.is_err(),
            receipt.error.is_some(),
            "{}: {result:?}",
            input.name
        );
        match receipt.error.as_deref() {
            Some("division") => assert!(matches!(
                result,
                Err(PrimitiveError::Unsupported { opcode: 0xd2, .. })
            )),
            Some(_) => assert!(matches!(result, Err(PrimitiveError::Host(_)))),
            None => assert_eq!(result.unwrap().commands, host.completed, "{}", input.name),
        }
        assert_eq!(host.events, receipt.events, "{} host order", input.name);
        assert_eq!(
            host.attempted.len(),
            receipt.commands.len(),
            "{} allocations",
            input.name
        );
        assert_eq!(
            host.completed,
            receipt
                .commands
                .iter()
                .filter(|command| command.payload.is_some())
                .count(),
            "{} completed prefix",
            input.name
        );
        for (actual, expected) in host.attempted.iter().zip(&receipt.commands) {
            assert_eq!(
                actual.callback.native_address(),
                expected.header[0],
                "{} callback",
                input.name
            );
            assert_eq!(
                actual.header_words,
                expected.header[1..],
                "{} header",
                input.name
            );
            assert_eq!(actual.depth, expected.key, "{} sort", input.name);
            assert_eq!(
                actual.payload_blocks, expected.blocks,
                "{} reservation",
                input.name
            );
            if let Some(payload) = &expected.payload {
                assert_eq!(actual.payload, *payload, "{} packet", input.name);
                assert_eq!(
                    actual.texture.map(|(offset, _)| offset),
                    expected.texture_offset,
                    "{} relocation",
                    input.name
                );
            }
        }
    }
}

#[test]
fn triangle_copied_shades_are_read_before_native_early_abort() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../tests/fixtures/d2_clipping_native.json")).unwrap();
    let input = &receipts
        .cases
        .iter()
        .find(|r| r.input.name == "0-clip40-abort")
        .unwrap()
        .input;
    let sprite = sprite(input);
    let tables = tables();
    for shade_word in 5..8 {
        let mut fixture = Fixture::new(input);
        // -32768 resolves below the arena start; positive 32767 still names
        // mapped unknown bytes in the 0xfa000-byte arena. Both original D2
        // callers copy all three DWORDs before 497250's clip40 abort.
        fixture.bytes[shade_word * 2..shade_word * 2 + 2]
            .copy_from_slice(&0x8000_u16.to_le_bytes());
        let words: Vec<_> = fixture.bytes[..18]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        fixture.model.program = decode_model_program(&words).unwrap();
        for far in [false, true] {
            let mut host = host(input, &sprite, receipts.texture_address);
            let result = prepare_d2(&fixture.request(far), state(input), &tables, &mut host);
            assert!(
                matches!(
                    result,
                    Err(PrimitiveError::Lighting(
                        crate::model_lighting::ModelLightingError::ScratchRead { slot: -32768 }
                    ))
                ),
                "shade word {shade_word}, far {far}: {result:?}"
            );
            assert!(host.events.is_empty());
        }
    }
}

#[test]
fn triangle_mapped_unknown_shades_preserve_packet_occupancy() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../tests/fixtures/d2_clipping_native.json")).unwrap();
    let input = &receipts
        .cases
        .iter()
        .find(|r| r.input.name == "0-mask1-cull0")
        .unwrap()
        .input;
    let mut fixture = Fixture::new(input);
    for word in 5..8 {
        fixture.bytes[word * 2..word * 2 + 2].copy_from_slice(&2_u16.to_le_bytes());
    }
    let words: Vec<_> = fixture.bytes[..18]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    fixture.model.program = decode_model_program(&words).unwrap();
    let sprite = sprite(input);
    for far in [false, true] {
        let mut host = host(input, &sprite, receipts.texture_address);
        let outcome =
            prepare_d2(&fixture.request(far), state(input), &tables(), &mut host).unwrap();
        assert_eq!(outcome.commands, 1);
        assert_eq!(host.attempted[0].payload[0x20..0x2c], [None; 12]);
    }
}
