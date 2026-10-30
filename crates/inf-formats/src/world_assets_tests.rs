use super::*;
use crate::{HandlerId, RelocationTail};

fn fixture(
    handler: u8,
    main: &[u8],
    relocations: &[u32],
    entries: &[u32],
) -> (Vec<u8>, HandlerRegion) {
    let mut source = vec![0xa5; 17];
    let main_payload = ByteSpan::new(source.len(), source.len() + main.len());
    source.extend_from_slice(main);
    let relocation_offsets = ByteSpan::new(source.len(), source.len() + relocations.len() * 4);
    for &offset in relocations {
        source.extend_from_slice(&offset.to_le_bytes());
    }
    let entry_offsets = ByteSpan::new(source.len(), source.len() + entries.len() * 4);
    for &offset in entries {
        source.extend_from_slice(&offset.to_le_bytes());
    }
    let region = HandlerRegion {
        handler: HandlerId::new(handler).unwrap(),
        requested_count: entries.len() as u32,
        span: ByteSpan::new(17, source.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload,
            relocation_count: relocations.len() as u32,
            relocation_offsets,
            entry_offsets,
        }),
    };
    (source, region)
}

fn put(main: &mut [u8], at: usize, value: u32) {
    main[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn world() -> Vec<u8> {
    let mut main = vec![0; 0x70];
    put(&mut main, 4, 0x1234_2080);
    put(&mut main, 0x38, 2);
    put(&mut main, 0x3c, 0x50);
    put(&mut main, 0x40, 0x50); // Deliberate pointer alias, not another record allocation.
    put(&mut main, 0x50, 0xffff_8001);
    for (index, word) in [0xffffu16, 2, 0x8000, 4, 5, 6].into_iter().enumerate() {
        main[0x54 + index * 2..0x56 + index * 2].copy_from_slice(&word.to_le_bytes());
    }
    main[0x60..].fill(0xcc);
    main
}

#[test]
fn constructor_prefix_preserves_selectors_argument_provenance_and_aliases() {
    let mut main = vec![0xcc; 24];
    put(&mut main, 4, 0x8000_ffff);
    put(&mut main, 12, 0xabcd_005d);
    put(&mut main, 20, 8);
    let (source, region) = fixture(13, &main, &[20], &[12, 4, 12]);
    let before = source.clone();
    let bank = parse_constructor_bank(&source, &region).unwrap();
    assert_eq!(bank.main_payload.get(&source).unwrap(), main);
    assert_eq!(
        bank.entries
            .iter()
            .map(|e| e.allocation_offset)
            .collect::<Vec<_>>(),
        [12, 4, 12]
    );
    assert_eq!(bank.entries[0].constructor_selector, 94);
    assert_eq!(bank.entries[0].argument_word, 0xabcd);
    assert_eq!(bank.entries[1].constructor_tag, u16::MAX);
    assert_eq!(bank.entries[1].constructor_selector, 0);
    assert_eq!(bank.entries[1].argument_word, 0x8000);
    assert_eq!(bank.entries[1].payload_offset, 8);
    assert_eq!(bank.entries[1].prefix_span, ByteSpan::new(21, 25));
    assert_eq!(bank.relocations[0].field_offset, 20);
    assert_eq!(bank.relocations[0].target_offset, 8);
    assert_eq!(
        source, before,
        "parsing never applies native relocation to the source"
    );
}

#[test]
fn placement_graph_preserves_order_aliases_full_argument_and_raw_units() {
    let (source, region) = fixture(14, &world(), &[0x40, 0x3c], &[0]);
    let bank = parse_world_bank(&source, &region).unwrap();
    let entry = &bank.entries[0];
    assert_eq!(entry.flags, 0x1234_2080);
    assert_eq!(entry.placements.len(), 2);
    let first = &entry.placements[0];
    assert_eq!(first.resource_argument, 0xffff_8001);
    assert_eq!(first.constructor_slot(), -32767);
    assert_eq!(first.position_words, [0xffff, 2, 0x8000]);
    assert_eq!(first.rotation_words, [4, 5, 6]);
    assert_eq!(first.constructor_rotation_words(), [6, 4, 5]);
    assert_eq!(first.payload_offset, 0x60);
    assert_eq!(first.prefix_span, ByteSpan::new(17 + 0x50, 17 + 0x60));
    assert_eq!(
        entry.placements[1].allocation_offset,
        first.allocation_offset
    );
    assert_eq!(entry.placements[1].index, 1);
    assert_ne!(entry.placements[1].pointer_span, first.pointer_span);
    assert_eq!(
        bank.relocations[0].field_offset, 0x40,
        "preserve native table order"
    );
}

#[test]
fn zero_placement_world_needs_no_relocation_or_invented_suffix() {
    let (source, region) = fixture(14, &[0; WORLD_HEADER_BYTES], &[], &[0]);
    let bank = parse_world_bank(&source, &region).unwrap();
    assert!(bank.entries[0].placements.is_empty());
    assert!(bank.entries[0].placement_pointer_span.is_empty());
}

#[test]
fn actor_mask_limits_preserve_signed_unsorted_header_bytes() {
    for limits in [[0x80, 0x7f], [0x7f, 0x80], [0xff, 0xff], [0, 16]] {
        let mut main = vec![0; WORLD_HEADER_BYTES];
        main[0x0e..0x10].copy_from_slice(&limits);
        let (source, region) = fixture(14, &main, &[], &[0]);
        let bank = parse_world_bank(&source, &region).unwrap();
        assert_eq!(
            bank.entries[0].actor_mask_limits(),
            limits.map(|value| value as i8)
        );
    }
}

#[test]
fn placement_pointer_must_be_in_native_relocation_table() {
    let (source, region) = fixture(14, &world(), &[0x3c], &[0]);
    assert_eq!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::MissingRelocation(0x40))
    );
}

#[test]
fn rejects_truncated_prefixes_pointer_arrays_and_outside_targets() {
    let (source, region) = fixture(13, &[0; 3], &[], &[0]);
    assert!(matches!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "constructor prefix",
            ..
        })
    ));
    let (source, region) = fixture(14, &[0; 59], &[], &[0]);
    assert!(matches!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "world header",
            ..
        })
    ));
    let mut main = world();
    put(&mut main, 0x38, u32::MAX);
    let (source, region) = fixture(14, &main, &[0x3c, 0x40], &[0]);
    assert!(matches!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "placement pointer table",
            ..
        }) | Err(WorldAssetError::IntegerOverflow(_))
    ));
    let mut main = world();
    put(&mut main, 0x3c, 0x6f);
    let (source, region) = fixture(14, &main, &[0x3c, 0x40], &[0]);
    assert!(matches!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "placement prefix",
            ..
        })
    ));
    put(&mut main, 0x3c, 0x71);
    let (source, region) = fixture(14, &main, &[0x3c, 0x40], &[0]);
    assert!(matches!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "relocation target",
            ..
        })
    ));
}

#[test]
fn rejects_invalid_relocation_sites_duplicates_and_table_lengths() {
    let (source, region) = fixture(13, &[0; 4], &[1], &[0]);
    assert!(matches!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::OutOfBounds {
            context: "relocation field",
            ..
        })
    ));
    let (source, mut region) = fixture(13, &[0; 4], &[0, 0], &[0]);
    assert_eq!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::DuplicateRelocation(0))
    );
    let HandlerFraming::RelocationTail(tail) = &mut region.framing else {
        unreachable!()
    };
    tail.relocation_count = 3;
    assert!(matches!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::TableSize {
            context: "relocation table",
            ..
        })
    ));
    let (source, mut region) = fixture(13, &[0; 4], &[], &[0]);
    region.requested_count = 2;
    assert!(matches!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::TableSize {
            context: "entry table",
            ..
        })
    ));
}

#[test]
fn rejects_wrong_handler_framing_and_invalid_source_spans() {
    let (source, mut region) = fixture(13, &[0; 4], &[], &[0]);
    assert!(matches!(
        parse_world_bank(&source, &region),
        Err(WorldAssetError::WrongHandler {
            expected: 14,
            actual: 13
        })
    ));
    let HandlerFraming::RelocationTail(tail) = &mut region.framing else {
        unreachable!()
    };
    tail.main_payload = ByteSpan::new(17, usize::MAX);
    assert!(matches!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::InvalidSpan { .. })
    ));
    region.framing = HandlerFraming::Standard {
        payload: ByteSpan::new(17, 21),
    };
    assert_eq!(
        parse_constructor_bank(&source, &region),
        Err(WorldAssetError::WrongFraming(13))
    );
}
