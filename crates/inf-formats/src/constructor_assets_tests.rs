use super::*;
use crate::{HandlerFraming, HandlerId, RelocationTail};

fn fixture(selector: u16, model_word: u16, payload: &[u8]) -> (Vec<u8>, HandlerRegion) {
    let mut bytes = vec![0xa5; 9];
    bytes.extend_from_slice(&selector.wrapping_sub(1).to_le_bytes());
    bytes.extend_from_slice(&model_word.to_le_bytes());
    bytes.extend_from_slice(payload);
    let main = ByteSpan::new(9, bytes.len());
    let empty = ByteSpan::new(bytes.len(), bytes.len());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    let region = HandlerRegion {
        handler: HandlerId::new(13).unwrap(),
        requested_count: 1,
        span: ByteSpan::new(9, bytes.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload: main,
            relocation_count: 0,
            relocation_offsets: empty,
            entry_offsets: ByteSpan::new(main.end, bytes.len()),
        }),
    };
    (bytes, region)
}

#[test]
fn model_binding_uses_header_argument_not_payload_flags_and_preserves_signedness() {
    for (raw_word, signed_slot) in [(0xffff, -1), (0x8001, -32767), (759, 759)] {
        let (source, region) = fixture(14, raw_word, &0x1234_5678u32.to_le_bytes());
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let asset = &bank.assets[0];
        let binding = asset.model_binding.unwrap();
        assert_eq!(binding.raw_word, raw_word);
        assert_eq!(binding.signed_slot, signed_slot);
        assert_eq!(binding.source_span, ByteSpan::new(11, 13));
        assert_eq!(binding.consumer, 0x0047_a050);
        assert_eq!(
            asset.payload,
            ConstructorPayloadView::Flags(ConstructorField {
                span: ByteSpan::new(13, 17),
                value: 0x1234_5678,
            })
        );
        assert_eq!(asset.dispatch.unwrap().context_initializer, 0x0047_a050);
    }
}

#[test]
fn route_node_keeps_flags_payload_separate_from_visual_and_placement_context() {
    let (source, region) = fixture(51, 0xfffe, &0x8000_000bu32.to_le_bytes());
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let asset = &bank.assets[0];
    let profile = asset.dispatch.unwrap();
    assert_eq!(profile.context_initializer, 0x0040_4c30);
    assert_eq!(profile.payload_initializer, 0x0047_9db0);
    assert_eq!(asset.model_binding.unwrap().signed_slot, -2);
    assert_eq!(
        asset.payload,
        ConstructorPayloadView::Flags(ConstructorField {
            span: ByteSpan::new(13, 17),
            value: 0x8000_000b,
        })
    );
}

#[test]
fn nonvisual_contexts_do_not_invent_a_header_model_reference() {
    for selector in [1, 67, 75, 81, 87, 94] {
        let (source, region) = fixture(selector, 42, &[0; 4]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        assert!(
            bank.assets[0].model_binding.is_none(),
            "selector {selector}"
        );
        assert!(matches!(
            bank.assets[0].payload,
            ConstructorPayloadView::Flags(_)
        ));
    }
}

#[test]
fn terrain_light_preserves_signed_channels_gate_and_unsigned_radius_with_exact_spans() {
    let payload = [0x03, 0x01, 0x80, 0xa0, 0x80, 0xff, 0x7f, 0xfe, 0xff, 0xff];
    let (source, region) = fixture(19, 759, &payload);
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    assert_eq!(
        bank.assets[0].payload,
        ConstructorPayloadView::TerrainLight(TerrainLightConstructorPayload {
            prefix_span: ByteSpan::new(13, 23),
            flags: ConstructorField {
                span: ByteSpan::new(13, 17),
                value: 0xa080_0103,
            },
            channels: ConstructorField {
                span: ByteSpan::new(17, 20),
                value: [-128, -1, 127],
            },
            draw_gate: ConstructorField {
                span: ByteSpan::new(20, 21),
                value: 254,
            },
            radius: ConstructorField {
                span: ByteSpan::new(21, 23),
                value: 65535,
            },
        })
    );
    assert_eq!(bank.assets[0].model_binding.unwrap().raw_word, 759);
    for length in 0..payload.len() {
        let (source, region) = fixture(19, 759, &payload[..length]);
        assert!(matches!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated {
                selector: 19,
                offset: 4,
                length: 10,
                allocation_bytes,
                ..
            }) if allocation_bytes == 4 + length
        ));
    }
}

#[test]
fn selector86_retained_data_requires_writable_interval_and_preserves_signed_rate() {
    for rate in [i16::MIN, -1, 0, 1, i16::MAX] {
        let mut payload = vec![0; 12];
        payload[..4].copy_from_slice(&0x8000_0127_u32.to_le_bytes());
        payload[6..8].copy_from_slice(&rate.to_le_bytes());
        payload[10..12].copy_from_slice(&0xabcd_u16.to_le_bytes());
        let (source, region) = fixture(86, 42, &payload);
        let original = source.clone();
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::Selector86(view) = &bank.assets[0].payload else {
            panic!("selector86 view")
        };
        assert_eq!(view.flags.value, 0x8000_0127);
        assert_eq!(view.rate.value, rate);
        assert_eq!(view.retained_offset, 8);
        assert_eq!(view.prefix_span, ByteSpan::new(13, 25));
        assert!(bank.assets[0].model_binding.is_none());
        assert_eq!(source, original);
        for length in [4, 8, 10, 11] {
            let (source, region) = fixture(86, 42, &payload[..length]);
            assert!(matches!(
                parse_constructor_asset_bank(&source, &region),
                Err(ConstructorAssetError::Truncated { length: 12, .. })
            ));
        }
    }
}

#[test]
fn shared_actor_preserves_raw_fields_without_applying_runtime_clamps_or_overrides() {
    let mut payload = vec![0xee; 0x50];
    payload[0..4].copy_from_slice(&0x8012_3456u32.to_le_bytes());
    payload[4..6].copy_from_slice(&60000u16.to_le_bytes());
    payload[8..10].copy_from_slice(&0x8001u16.to_le_bytes());
    payload[12..16].copy_from_slice(&(-3i32).to_le_bytes());
    payload[0x38..0x3a].copy_from_slice(&175u16.to_le_bytes());
    payload[0x3c..0x40].copy_from_slice(&0xffff_0001u32.to_le_bytes());
    payload[0x40..0x42].copy_from_slice(&0xffffu16.to_le_bytes());
    let (source, region) = fixture(3, 700, &payload);
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("shared actor")
    };
    assert_eq!(actor.prefix_span, ByteSpan::new(13, 13 + 0x42));
    assert_eq!(
        actor.initial_health,
        ConstructorField {
            span: ByteSpan::new(17, 19),
            value: 60000
        }
    );
    assert_eq!(actor.flags.value, 0x8012_3456);
    assert_eq!(actor.word_08.value, 0x8001);
    assert_eq!(actor.selector_0c.value, -3);
    assert_eq!(actor.embedded_10_offset, 0x14);
    assert_eq!(actor.word_38.value, 175);
    assert_eq!(actor.word_3c.value, 0xffff_0001);
    assert_eq!(actor.word_40.value, 0xffff);
    assert!(actor.derived.is_none());
    assert_eq!(bank.prefixes.main_payload.len(), 4 + 0x50);
    assert_eq!(
        source, before,
        "unknown bytes and asset inputs remain untouched"
    );
}

#[test]
fn derived_actor_fields_keep_exact_widths_raw_gaps_sound_policy_and_aliases() {
    let mut payload = vec![0xcc; 0x58];
    payload[0x44] = 0xff;
    payload[0x48..0x4a].copy_from_slice(&0x8000u16.to_le_bytes());
    payload[0x4a..0x4c].copy_from_slice(&i16::MIN.to_le_bytes());
    payload[0x4c..0x50].copy_from_slice(&0xfffffffdu32.to_le_bytes());
    for (index, value) in [0, 0x7fff, 0x8000, 0xffffu16].into_iter().enumerate() {
        payload[0x50 + index * 2..0x52 + index * 2].copy_from_slice(&value.to_le_bytes());
    }
    let (mut source, mut region) = fixture(12, 17, &payload);
    source.extend_from_slice(&0u32.to_le_bytes());
    region.requested_count = 2;
    region.span.end = source.len();
    let HandlerFraming::RelocationTail(tail) = &mut region.framing else {
        unreachable!()
    };
    tail.entry_offsets.end = source.len();
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    assert_eq!(bank.assets[0].payload, bank.assets[1].payload);
    for asset in &bank.assets {
        let ConstructorPayloadView::SharedActor(actor) = &asset.payload else {
            panic!("actor")
        };
        assert_eq!(actor.prefix_span, ByteSpan::new(13, 13 + 0x42));
        let derived = actor.derived.as_ref().unwrap();
        assert_eq!(derived.span, ByteSpan::new(13 + 0x42, 13 + 0x58));
        assert_eq!(
            derived.raw_gaps,
            [
                ByteSpan::new(13 + 0x42, 13 + 0x44),
                ByteSpan::new(13 + 0x45, 13 + 0x48)
            ]
        );
        assert_eq!(derived.consumer, 0x00426c10);
        assert_eq!(
            derived.byte_44,
            ConstructorField {
                span: ByteSpan::new(13 + 0x44, 13 + 0x45),
                value: 0xff
            }
        );
        assert_eq!(
            derived.word_48,
            ConstructorField {
                span: ByteSpan::new(13 + 0x48, 13 + 0x4a),
                value: 0x8000
            }
        );
        assert_eq!(
            derived.signed_word_4a,
            ConstructorField {
                span: ByteSpan::new(13 + 0x4a, 13 + 0x4c),
                value: i16::MIN
            }
        );
        assert_eq!(
            derived.word_4c,
            ConstructorField {
                span: ByteSpan::new(13 + 0x4c, 13 + 0x50),
                value: 0xfffffffd
            }
        );
        for (index, binding) in derived.sound_bindings.iter().enumerate() {
            assert_eq!(
                binding.raw.span,
                ByteSpan::new(13 + 0x50 + index * 2, 13 + 0x52 + index * 2)
            );
            assert_eq!(binding.raw.value, [0, 0x7fff, 0x8000, 0xffff][index]);
            assert_eq!(binding.signed_slot, [0, i16::MAX, i16::MIN, -1][index]);
            assert_eq!(binding.lookup_slot, [None, Some(0x7fff), None, None][index]);
            assert_eq!(binding.object_offset, 0x17c + index as u16 * 2);
            assert_eq!(
                binding.consumer,
                if index == 3 { 0x00424280 } else { 0x004294a0 }
            );
        }
    }
    assert_eq!(source, before);
}

#[test]
fn derived_actor_lookup_eligibility_is_explicit_for_each_sound_word() {
    for value in [0, 1, 0x7fff, 0x8000, 0xffffu16] {
        let mut payload = vec![0; 0x58];
        for at in [0x50, 0x52, 0x54, 0x56] {
            payload[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        let (source, region) = fixture(60, 0, &payload);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor")
        };
        for binding in &actor.derived.as_ref().unwrap().sound_bindings {
            assert_eq!(binding.raw.value, value);
            assert_eq!(binding.signed_slot, value as i16);
            assert_eq!(
                binding.lookup_slot,
                if (1..0x8000).contains(&value) {
                    Some(value)
                } else {
                    None
                }
            );
        }
    }
}

#[test]
fn derived_actor_extent_depends_on_the_proven_initializer_not_the_shared_base() {
    for selector in 1..=CONSTRUCTOR_SELECTOR_COUNT {
        let supported = matches!(
            selector,
            2 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 21 | 33 | 34 | 35 | 36 | 60 | 77 | 79 | 80
        );
        let (source, region) = fixture(selector, 0, &[0; 0x78]);
        if selector == 70 {
            continue;
        } // This unrelated class requires a relocated table.
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let has_view = matches!(&bank.assets[0].payload,ConstructorPayloadView::SharedActor(actor) if actor.derived.is_some());
        assert_eq!(has_view, supported, "selector {selector}");
        if supported {
            for length in [0x42, 0x44, 0x49, 0x4b, 0x4f, 0x51, 0x53, 0x55, 0x57] {
                let (source, region) = fixture(selector, 0, &vec![0; length]);
                assert_eq!(
                    parse_constructor_asset_bank(&source, &region),
                    Err(ConstructorAssetError::Truncated {
                        local_index: 0,
                        selector,
                        offset: 4,
                        length: if matches!(selector, 8 | 9 | 10 | 21 | 35 | 36) {
                            0x74
                        } else {
                            0x58
                        },
                        allocation_bytes: 4 + length,
                    })
                );
            }
        }
    }
}

#[test]
fn damage_responses_preserve_unsigned_words_and_exact_channel_spans() {
    let mut payload = vec![0xee; 0x50];
    let values = [
        (0, 0),
        (0, 256),
        (1, 128),
        (32767, 32768),
        (32768, 65535),
        (65535, 32768),
        (2, 512),
        (1000, 64),
        (3, 255),
        (60000, 65000),
    ];
    for (channel, &(threshold, multiplier)) in values.iter().enumerate() {
        let offset = 0x10 + channel * 4;
        payload[offset..offset + 2].copy_from_slice(&u16::to_le_bytes(threshold));
        payload[offset + 2..offset + 4].copy_from_slice(&u16::to_le_bytes(multiplier));
    }
    payload[0x38..0x3a].copy_from_slice(&175u16.to_le_bytes());
    let (mut source, mut region) = fixture(3, 700, &payload);
    // An alias is still a separate registration, with identical source custody.
    source.extend_from_slice(&0u32.to_le_bytes());
    region.requested_count = 2;
    region.span.end += 4;
    let HandlerFraming::RelocationTail(tail) = &mut region.framing else {
        unreachable!()
    };
    tail.entry_offsets.end += 4;
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    for (index, asset) in bank.assets.iter().enumerate() {
        assert_eq!(asset.local_index, index as u32);
        let ConstructorPayloadView::SharedActor(actor) = &asset.payload else {
            panic!("shared actor")
        };
        for (channel, (response, &(threshold, multiplier))) in
            actor.damage_responses.iter().zip(&values).enumerate()
        {
            let start = 13 + 0x10 + channel * 4;
            assert_eq!(response.channel, channel as u8);
            assert_eq!(
                response.threshold,
                ConstructorField {
                    span: ByteSpan::new(start, start + 2),
                    value: threshold,
                }
            );
            assert_eq!(
                response.multiplier_q8,
                ConstructorField {
                    span: ByteSpan::new(start + 2, start + 4),
                    value: multiplier,
                }
            );
        }
        assert_eq!(
            actor.damage_responses[9].multiplier_q8.span.end,
            actor.word_38.span.start
        );
        assert_eq!(actor.word_38.value, 175);
    }
    assert_eq!(source, before);
}

#[test]
fn unknown_selectors_keep_their_payload_opaque() {
    for selector in [0, 95, u16::MAX] {
        let (source, region) = fixture(selector, 18, &[]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        assert!(bank.assets[0].dispatch.is_none());
        assert!(bank.assets[0].model_binding.is_none());
        assert_eq!(bank.assets[0].payload, ConstructorPayloadView::Opaque);
    }
}

#[test]
fn required_payload_fields_are_bounded_and_do_not_read_relocation_tail_bytes() {
    for (selector, length, required) in [(14, 3, 4), (3, 0x41, 0x42), (70, 0x0f, 0x10)] {
        let (source, region) = fixture(selector, 12, &vec![0; length]);
        assert_eq!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated {
                local_index: 0,
                selector,
                offset: 4,
                length: required,
                allocation_bytes: 4 + length,
            })
        );
    }
}

fn point_table_fixture() -> (Vec<u8>, HandlerRegion) {
    let mut payload = vec![0xee; 16 + 64];
    payload[4..8].copy_from_slice(&0x8000_0003u32.to_le_bytes());
    // +8 is deliberately not 16: the native consumer uses a fixed loop bound.
    payload[12..16].copy_from_slice(&20u32.to_le_bytes());
    for index in 0..16 {
        let at = 16 + index * 4;
        payload[at..at + 2].copy_from_slice(&(0xffffu16 - index as u16).to_le_bytes());
        payload[at + 2..at + 4].copy_from_slice(&(0x8000u16 + index as u16).to_le_bytes());
    }
    let (mut source, mut region) = fixture(70, 18, &payload);
    let HandlerFraming::RelocationTail(tail) = &mut region.framing else {
        unreachable!()
    };
    // Replace the old entry-offset word with the relocation-field offset;
    // the entry-offset table follows it, still referring to allocation +0.
    source[tail.main_payload.end..].copy_from_slice(&16u32.to_le_bytes());
    source.extend_from_slice(&0u32.to_le_bytes());
    tail.relocation_count = 1;
    tail.relocation_offsets = ByteSpan::new(tail.main_payload.end, tail.main_payload.end + 4);
    tail.entry_offsets = ByteSpan::new(tail.main_payload.end + 4, source.len());
    region.span.end = source.len();
    (source, region)
}

#[test]
fn specialized_point_table_preserves_raw_words_and_fixed_consumer_count() {
    let (source, region) = point_table_fixture();
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let asset = &bank.assets[0];
    assert_eq!(asset.dispatch.unwrap().payload_initializer, 0x0046_e860);
    assert_eq!(asset.model_binding.unwrap().signed_slot, 18);
    let ConstructorPayloadView::PointTable16(view) = &asset.payload else {
        panic!("point table")
    };
    assert_eq!(view.prefix_span, ByteSpan::new(13, 29));
    assert_eq!(view.word_04.value, 0x8000_0003);
    assert_eq!(
        view.points_pointer,
        ConstructorField {
            span: ByteSpan::new(25, 29),
            value: 20
        }
    );
    assert_eq!(view.points_span, ByteSpan::new(29, 93));
    for (index, point) in view.points.iter().enumerate() {
        assert_eq!(point.span, ByteSpan::new(29 + index * 4, 33 + index * 4));
        assert_eq!(point.value, [0xffff - index as u16, 0x8000 + index as u16]);
    }
    assert_eq!(source, before);
}

#[test]
fn specialized_point_table_requires_relocation_and_bounds_the_full_target() {
    let (mut source, region) = point_table_fixture();
    // An in-range target with only 63 bytes left must not consume the tail.
    source[25..29].copy_from_slice(&21u32.to_le_bytes());
    assert_eq!(
        parse_constructor_asset_bank(&source, &region),
        Err(ConstructorAssetError::Truncated {
            local_index: 0,
            selector: 70,
            offset: 21,
            length: 64,
            allocation_bytes: 84,
        })
    );
    // An in-range raw pointer is insufficient without native relocation custody.
    let mut payload = vec![0; 80];
    payload[12..16].copy_from_slice(&20u32.to_le_bytes());
    let (source, region) = fixture(70, 18, &payload);
    assert_eq!(
        parse_constructor_asset_bank(&source, &region),
        Err(ConstructorAssetError::MissingPointTableRelocation {
            local_index: 0,
            field_offset: 16,
        })
    );
}

#[test]
fn direct_derived_vtable_overrides_are_not_replaced_by_the_base_constructor_profile() {
    let base = constructor_dispatch(12).unwrap();
    let derived = constructor_dispatch(13).unwrap();
    assert_eq!(base.constructor, derived.constructor);
    assert_eq!(base.payload_initializer, derived.payload_initializer);
    assert_eq!(base.vtable, 0x0052_7058);
    assert_eq!(derived.vtable, 0x0052_69c4);
    assert_eq!(
        constructor_dispatch(51).unwrap().context_initializer,
        0x0040_4c30
    );
    assert_eq!(constructor_dispatch(27).unwrap().vtable, 0x0052_5848);
}

#[test]
fn selector30_payload_preserves_signed_maximum_and_distinct_copy_widths() {
    let mut raw = [0xa5; 0x58];
    raw[0x44..0x48].copy_from_slice(&i32::MIN.to_le_bytes());
    raw[0x48..0x4e].copy_from_slice(&[0xff, 0xff, 0, 0x80, 1, 0]);
    raw[0x50..0x54].copy_from_slice(&0x89ab_cdef_u32.to_le_bytes());
    raw[0x54..0x58].copy_from_slice(&0xfedc_ba98_u32.to_le_bytes());
    let (source, region) = fixture(30, 42, &raw);
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("actor payload")
    };
    assert!(actor.derived.is_none());
    assert!(actor.selector34.is_none());
    assert!(actor.selector35.is_none());
    let payload = actor.selector30.as_ref().unwrap();
    assert_eq!(payload.signed_dword_44.value, i32::MIN);
    assert_eq!(
        payload.signed_dword_44.span,
        ByteSpan::new(13 + 0x44, 13 + 0x48)
    );
    assert_eq!(payload.words_48_4c.value, [65535, 32768, 1]);
    assert_eq!(
        payload.words_48_4c.span,
        ByteSpan::new(13 + 0x48, 13 + 0x4e)
    );
    assert_eq!(
        (payload.dword_50.value, payload.dword_54.value),
        (0x89ab_cdef, 0xfedc_ba98)
    );
    for length in 0x42..raw.len() {
        let (source, region) = fixture(30, 42, &raw[..length]);
        assert!(matches!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated {
                selector: 30,
                length: 0x58,
                ..
            })
        ));
    }
}

#[test]
fn selector34_retains_tail_address_without_reading_postinit_operands() {
    let raw = [0xa5; 0x58];
    let (source, region) = fixture(34, 42, &raw);
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("actor payload")
    };
    assert!(actor.derived.is_some());
    assert!(actor.selector30.is_none());
    assert!(actor.selector35.is_none());
    let payload = actor.selector34.as_ref().unwrap();
    assert_eq!(payload.retained_58_offset, 4 + 0x58);
    assert_eq!(
        payload.retained_58_offset as usize,
        bank.prefixes.main_payload.len()
    );
    assert_eq!(source, before);
    for length in 0x42..raw.len() {
        let (source, region) = fixture(34, 42, &raw[..length]);
        assert!(matches!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated {
                selector: 34,
                length: 0x58,
                ..
            })
        ));
    }
}

#[test]
fn family4236d0_payload_preserves_table_selectors_words_and_retained_address() {
    let mut raw = [0xa5; 0x78];
    raw[0x68..0x6c].copy_from_slice(&0x8000_0001_u32.to_le_bytes());
    raw[0x6c..0x6e].copy_from_slice(&0xfffe_u16.to_le_bytes());
    raw[0x6e..0x70].copy_from_slice(&0x8001_u16.to_le_bytes());
    raw[0x70..0x74].copy_from_slice(&u32::MAX.to_le_bytes());
    for selector in [8, 9, 10, 21, 36] {
        let (source, region) = fixture(selector, 42, &raw);
        let before = source.clone();
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        let payload = actor.family4236d0.as_ref().unwrap();
        assert!(actor.derived.is_some());
        assert_eq!(payload.retained_58_offset, 4 + 0x58);
        assert_eq!(payload.dword_68.value, 0x8000_0001);
        assert_eq!(payload.dword_68.span, ByteSpan::new(13 + 0x68, 13 + 0x6c));
        assert_eq!(payload.word_6c.value, 0xfffe);
        assert_eq!(payload.word_6c.span, ByteSpan::new(13 + 0x6c, 13 + 0x6e));
        assert_eq!(payload.word_6e.value, 0x8001);
        assert_eq!(payload.word_6e.span, ByteSpan::new(13 + 0x6e, 13 + 0x70));
        assert_eq!(payload.dword_70.value, u32::MAX);
        assert_eq!(payload.dword_70.span, ByteSpan::new(13 + 0x70, 13 + 0x74));
        assert_eq!(source, before);
        for length in 0x58..0x74 {
            let (source, region) = fixture(selector, 42, &raw[..length]);
            assert!(matches!(
                parse_constructor_asset_bank(&source, &region),
                Err(ConstructorAssetError::Truncated { length: 0x74, .. })
            ));
        }
    }
    // These adjacent chains bypass 00423850, despite sharing 00426C10.
    for selector in [7, 11, 12, 33, 34, 35, 60, 77] {
        let (source, region) = fixture(selector, 42, &raw);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        assert!(actor.family4236d0.is_none());
    }
}

#[test]
fn family467070_retains_end_address_without_reading_later_clamp_inputs() {
    for selector in [12, 13] {
        let (source, region) = fixture(selector, 42, &[0xa5; 0x58]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        assert!(actor.derived.is_some());
        let retained = actor.family467070.as_ref().unwrap().retained_58_offset;
        assert_eq!(retained, 4 + 0x58);
        assert_eq!(retained as usize, bank.prefixes.main_payload.len());
        let (source, region) = fixture(selector, 42, &[0xa5; 0x57]);
        assert!(matches!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated { length: 0x58, .. })
        ));
    }
    for selector in [7, 11, 34, 35, 36] {
        let (source, region) = fixture(selector, 42, &[0; 0x74]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        assert!(actor.family467070.is_none());
    }
}

#[test]
fn family425190_payload_reads_full_dword_only_for_the_proven_initializer_chain() {
    for selector in [10, 21] {
        for value in [0, 1, 2, 3, 4, 5, 0x0001_0004, 0x8000_0000, u32::MAX] {
            let mut raw = [0xa5; 0x78];
            raw[0x74..0x78].copy_from_slice(&value.to_le_bytes());
            let (source, region) = fixture(selector, 42, &raw);
            let before = source.clone();
            let bank = parse_constructor_asset_bank(&source, &region).unwrap();
            let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
                panic!("actor payload")
            };
            assert!(actor.family4236d0.is_some());
            assert_eq!(
                actor.family425190.as_ref().unwrap().dword_74,
                ConstructorField {
                    span: ByteSpan::new(13 + 0x74, 13 + 0x78),
                    value,
                }
            );
            assert_eq!(source, before);
        }
        for length in 0x74..0x78 {
            let (source, region) = fixture(selector, 42, &vec![0; length]);
            assert_eq!(
                parse_constructor_asset_bank(&source, &region),
                Err(ConstructorAssetError::Truncated {
                    local_index: 0,
                    selector,
                    offset: 4,
                    length: 0x78,
                    allocation_bytes: 4 + length,
                })
            );
        }
    }
    for selector in [8, 9, 34, 35, 36] {
        let (source, region) = fixture(selector, 42, &[0; 0x74]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        assert!(actor.family425190.is_none());
    }
}

#[test]
fn selector35_payload_retains_address_and_bounds_full_dword_reads() {
    let mut raw = [0xa5; 0x74];
    raw[0x6c..0x70].copy_from_slice(&u32::MAX.to_le_bytes());
    raw[0x70..0x74].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    let (source, region) = fixture(35, 42, &raw);
    let before = source.clone();
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("actor payload")
    };
    assert!(actor.derived.is_some());
    assert!(actor.selector30.is_none());
    assert!(actor.selector34.is_none());
    let payload = actor.selector35.as_ref().unwrap();
    assert_eq!(payload.retained_58_offset, 4 + 0x58);
    assert_eq!(payload.dword_6c.value, u32::MAX);
    assert_eq!(payload.dword_70.value, 0x8000_0000);
    assert_eq!(payload.dword_70.span, ByteSpan::new(13 + 0x70, 13 + 0x74));
    assert_eq!(source, before);
    for length in 0x58..raw.len() {
        let (source, region) = fixture(35, 42, &raw[..length]);
        assert!(matches!(
            parse_constructor_asset_bank(&source, &region),
            Err(ConstructorAssetError::Truncated {
                selector: 35,
                length: 0x74,
                ..
            })
        ));
    }
    // The adjacent class shares the central payload, not selector 35's tail.
    let (source, region) = fixture(34, 42, &raw[..0x58]);
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("actor payload")
    };
    assert!(actor.selector35.is_none());
}

#[test]
fn behavior_resource_initializers_read_their_exact_tails() {
    let raw: Vec<u8> = (0..0x60_u8).map(|byte| byte.wrapping_mul(7)).collect();
    let word = |at: usize| u16::from_le_bytes([raw[at], raw[at + 1]]);
    let dword = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    // (selector, initializer's last byte read)
    for (selector, length) in [
        (32_u16, 0x46_usize),
        (31, 0x46),
        (28, 0x4e),
        (68, 0x4e),
        (26, 0x55),
        (27, 0x55),
        (38, 0x60),
        (25, 0x54),
    ] {
        let (source, region) = fixture(selector, 3, &raw[..length]);
        let bank = parse_constructor_asset_bank(&source, &region).unwrap();
        let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
            panic!("actor payload")
        };
        let payload = actor.behavior_resource.as_deref().unwrap();
        match (selector, payload) {
            (31 | 32, BehaviorResourcePayload::Strength44 { word_44 }) => {
                assert_eq!(word_44.value, word(0x44));
                assert_eq!(word_44.span, ByteSpan::new(13 + 0x44, 13 + 0x46));
            }
            (_, BehaviorResourcePayload::Packet44 { bytes_44, tail }) => {
                assert_eq!(&bytes_44.value[..], &raw[0x44..0x4e]);
                assert_eq!(bytes_44.span, ByteSpan::new(13 + 0x44, 13 + 0x4e));
                match (selector, tail) {
                    (28 | 68, BehaviorResourcePayloadTail::None) => {}
                    (26 | 27, BehaviorResourcePayloadTail::Bytes50And54 { byte_50, byte_54 }) => {
                        assert_eq!((byte_50.value, byte_54.value), (raw[0x50], raw[0x54]));
                    }
                    (
                        38,
                        BehaviorResourcePayloadTail::Bytes50And54Dwords58And5c {
                            byte_50,
                            byte_54,
                            dword_58,
                            dword_5c,
                        },
                    ) => {
                        assert_eq!((byte_50.value, byte_54.value), (raw[0x50], raw[0x54]));
                        assert_eq!((dword_58.value, dword_5c.value), (dword(0x58), dword(0x5c)));
                    }
                    (
                        25,
                        BehaviorResourcePayloadTail::Word50Bytes52And53 {
                            word_50,
                            byte_52,
                            byte_53,
                        },
                    ) => {
                        assert_eq!(
                            (word_50.value, byte_52.value, byte_53.value),
                            (word(0x50), raw[0x52], raw[0x53])
                        );
                    }
                    other => panic!("selector {selector}: {other:?}"),
                }
            }
            other => panic!("selector {selector}: {other:?}"),
        }
        // One byte short of the initializer's reads is a truncated bank.
        let (source, region) = fixture(selector, 3, &raw[..length - 1]);
        assert!(
            matches!(
                parse_constructor_asset_bank(&source, &region),
                Err(ConstructorAssetError::Truncated { length: l, .. }) if l == length
            ),
            "{selector}"
        );
    }
    // The shared wrapper's class reads nothing past +42.
    let (source, region) = fixture(6, 3, &raw[..0x42]);
    let bank = parse_constructor_asset_bank(&source, &region).unwrap();
    let ConstructorPayloadView::SharedActor(actor) = &bank.assets[0].payload else {
        panic!("actor payload")
    };
    assert!(actor.behavior_resource.is_none());
}
