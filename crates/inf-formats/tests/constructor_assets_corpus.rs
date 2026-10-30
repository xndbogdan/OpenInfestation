use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use inf_formats::{
    constructor_dispatch, parse_constructor_asset_bank, parse_overlay, ConstructorPayloadView,
    FrontendExecutable, Preload, CONSTRUCTOR_SELECTOR_COUNT,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[test]
fn retail_constructor_cases_and_vtable_slots_match_the_supported_executable() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    let mapped = FrontendExecutable::parse(&exe).unwrap();
    let jump_table = text_range(
        &exe,
        0x0046_1a04,
        usize::from(CONSTRUCTOR_SELECTOR_COUNT) * 4,
    );
    let starts: Vec<_> = jump_table
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let mut vtables = BTreeSet::new();
    for selector in 1..=CONSTRUCTOR_SELECTOR_COUNT {
        let profile = constructor_dispatch(selector).unwrap();
        assert_eq!(
            mapped.word(profile.vtable + 8).unwrap(),
            profile.context_initializer,
            "selector {selector} context"
        );
        assert_eq!(
            mapped.word(profile.vtable + 12).unwrap(),
            profile.payload_initializer,
            "selector {selector} payload"
        );
        let start = starts[usize::from(selector - 1)];
        let end = starts
            .iter()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(0x0046_19f1);
        let case = text_range(&exe, start, (end - start) as usize);
        assert!(
            case.windows(5).enumerate().any(|(offset, b)| {
                b[0] == 0xe8
                    && (start + offset as u32 + 5)
                        .wrapping_add(u32::from_le_bytes(b[1..].try_into().unwrap()))
                        == profile.constructor
            }),
            "selector {selector} must call its recovered constructor"
        );
        vtables.insert(profile.vtable);
    }
    assert!(constructor_dispatch(0).is_none());
    assert!(constructor_dispatch(95).is_none());
    // Several selectors deliberately share one constructor/vtable.
    assert!(vtables.len() < usize::from(CONSTRUCTOR_SELECTOR_COUNT));
}

#[test]
fn retail_damage_response_consumer_matches_the_unsigned_stride_and_scale() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    let mapped = FrontendExecutable::parse(&exe).unwrap();
    // Actor construction installs this table before a class payload replaces it.
    assert_eq!(
        text_range(&exe, 0x0047_aa1e, 10),
        [0xc7, 0x86, 0xa4, 0, 0, 0, 0x60, 0x12, 0x53, 0]
    );
    for channel in 0..10 {
        assert_eq!(
            mapped.word(0x0053_1260 + channel * 4).unwrap(),
            if channel == 0 { 0 } else { 2000 | (256 << 16) }
        );
    }
    // The original radial descriptor exercises the tenth response (byte +2 = 9).
    assert_eq!(mapped.word(0x0053_1250).unwrap(), 1000 | (9 << 16));
    // zero SI; load threshold WORD at base + channel*4.
    assert_eq!(
        text_range(&exe, 0x0042_a90b, 6),
        [0x33, 0xf6, 0x66, 0x8b, 0x34, 0xba]
    );
    // zero EAX; subtract threshold; load multiplier WORD; multiply; SHR 8.
    assert_eq!(
        text_range(&exe, 0x0042_a918, 15),
        [0x33, 0xc0, 0x2b, 0xce, 0x66, 0x8b, 0x44, 0xba, 2, 0x0f, 0xaf, 0xc8, 0xc1, 0xe9, 8]
    );
}

/// Test-only access to original .text bytes; production asset parsing never executes or maps code.
fn text_range(bytes: &[u8], address: u32, length: usize) -> &[u8] {
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let half = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
    let pe = word(0x3c);
    let sections = pe + 24 + half(pe + 20);
    for index in 0..half(pe + 6) {
        let section = sections + index * 40;
        if &bytes[section..section + 8] == b".text\0\0\0" {
            let rva = address as usize - 0x0040_0000;
            let section_rva = word(section + 12);
            let size = word(section + 16);
            assert!(rva >= section_rva && rva + length <= section_rva + size);
            let offset = word(section + 20) + rva - section_rva;
            return &bytes[offset..offset + length];
        }
    }
    panic!("supported retail executable has .text");
}

#[test]
fn retail_point_consumer_uses_relocated_word_pairs_and_a_fixed_sixteen_pair_loop() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    // Payload +0C pointer, first raw WORD, low-WORD multiplication by 220.
    assert_eq!(
        text_range(&exe, 0x0046_f334, 12),
        [0x8b, 0x48, 0x0c, 0x66, 0x8b, 0x14, 0x29, 0x66, 0x69, 0xd2, 0xdc, 0,]
    );
    // The second WORD is +2; source advances four bytes and stops at 64.
    assert_eq!(
        text_range(&exe, 0x0046_f34f, 8),
        [0x8b, 0x48, 0x0c, 0x66, 0x8b, 0x54, 0x29, 2]
    );
    assert_eq!(text_range(&exe, 0x0046_f36a, 3), [0x83, 0xc5, 4]);
    assert_eq!(
        text_range(&exe, 0x0046_f382, 5),
        [0x83, 0xfd, 0x40, 0x72, 0xa2]
    );
}

#[test]
fn retail_constructor_assets_expose_initial_model_bindings_and_shared_actor_fields() {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut total = 0;
    let mut models = 0;
    let mut actor = 0;
    let mut flags = 0;
    let mut terrain_lights = 0;
    let mut selector86 = 0;
    let mut selector83 = 0;
    let mut point_tables = 0;
    let mut opaque = 0;
    let mut arena = 0;
    let mut model_selectors = BTreeSet::new();
    let mut response_profiles = BTreeSet::new();
    let mut response_count = 0;
    let mut derived_selectors = BTreeMap::new();
    let mut derived_banks = BTreeMap::new();
    let mut sound_bindings = 0;
    let mut positive_sounds = 0;
    let mut sound_slots = BTreeSet::new();
    for level in [1, 6, 29, 30, 32] {
        let bytes = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 13)
            .unwrap();
        let bank = parse_constructor_asset_bank(&bytes, region).unwrap();
        assert_eq!(bank.assets.len(), bank.prefixes.entries.len());
        for (asset, prefix) in bank.assets.iter().zip(&bank.prefixes.entries) {
            let profile = asset.dispatch.unwrap();
            assert_eq!(asset.local_index, prefix.local_index);
            assert_eq!(profile.selector, prefix.constructor_selector);
            if let Some(binding) = asset.model_binding {
                assert_eq!(binding.source_span.start, prefix.prefix_span.start + 2);
                assert_eq!(binding.raw_word, prefix.argument_word);
                assert!((0..760).contains(&binding.signed_slot));
                model_selectors.insert(profile.selector);
                models += 1;
            }
            match &asset.payload {
                ConstructorPayloadView::SharedActor(view) => {
                    let raw = view.prefix_span.get(&bytes).unwrap();
                    assert_eq!(raw.len(), 0x42);
                    assert_eq!(
                        view.initial_health.value,
                        u16::from_le_bytes(raw[4..6].try_into().unwrap())
                    );
                    assert_eq!(view.embedded_10_offset, prefix.payload_offset + 0x10);
                    for (channel, response) in view.damage_responses.iter().enumerate() {
                        let at = 0x10 + channel * 4;
                        assert_eq!(response.channel, channel as u8);
                        assert_eq!(response.threshold.span.start, view.prefix_span.start + at);
                        assert_eq!(
                            response.multiplier_q8.span.end,
                            view.prefix_span.start + at + 4
                        );
                        assert_eq!(
                            response.threshold.value,
                            u16::from_le_bytes(raw[at..at + 2].try_into().unwrap())
                        );
                        assert_eq!(
                            response.multiplier_q8.value,
                            u16::from_le_bytes(raw[at + 2..at + 4].try_into().unwrap())
                        );
                        response_count += 1;
                    }
                    response_profiles.insert(
                        view.damage_responses
                            .map(|r| (r.threshold.value, r.multiplier_q8.value)),
                    );
                    if let Some(derived) = &view.derived {
                        *derived_selectors.entry(profile.selector).or_insert(0) += 1;
                        *derived_banks.entry(level).or_insert(0) += 1;
                        let start = prefix.prefix_span.end;
                        assert_eq!(
                            derived.span,
                            inf_formats::ByteSpan::new(start + 0x42, start + 0x58)
                        );
                        assert_eq!(derived.byte_44.value, bytes[start + 0x44]);
                        assert_eq!(
                            derived.word_48.value,
                            u16::from_le_bytes(
                                bytes[start + 0x48..start + 0x4a].try_into().unwrap()
                            )
                        );
                        assert_eq!(
                            derived.signed_word_4a.value,
                            i16::from_le_bytes(
                                bytes[start + 0x4a..start + 0x4c].try_into().unwrap()
                            )
                        );
                        assert_eq!(
                            derived.word_4c.value,
                            u32::from_le_bytes(
                                bytes[start + 0x4c..start + 0x50].try_into().unwrap()
                            )
                        );
                        for (index, binding) in derived.sound_bindings.iter().enumerate() {
                            assert_eq!(
                                binding.raw.span,
                                inf_formats::ByteSpan::new(
                                    start + 0x50 + index * 2,
                                    start + 0x52 + index * 2
                                )
                            );
                            let raw = u16::from_le_bytes(
                                binding.raw.span.get(&bytes).unwrap().try_into().unwrap(),
                            );
                            assert_eq!(binding.raw.value, raw);
                            assert_eq!(binding.signed_slot, raw as i16);
                            assert_eq!(binding.lookup_slot, if raw > 0 { Some(raw) } else { None });
                            if let Some(slot) = binding.lookup_slot {
                                positive_sounds += 1;
                                sound_slots.insert(slot);
                            }
                            sound_bindings += 1;
                        }
                    }
                    actor += 1;
                }
                ConstructorPayloadView::Flags(field) => {
                    assert_eq!(field.span.start, prefix.prefix_span.end);
                    flags += 1;
                }
                ConstructorPayloadView::Selector86(view) => {
                    assert_eq!(profile.selector, 86);
                    assert_eq!(view.prefix_span.start, prefix.prefix_span.end);
                    assert_eq!(view.prefix_span.len(), 12);
                    assert_eq!(view.retained_offset, prefix.payload_offset + 4);
                    let raw = view.rate.span.get(&bytes).unwrap();
                    assert_eq!(view.rate.value, i16::from_le_bytes(raw.try_into().unwrap()));
                    selector86 += 1;
                }
                ConstructorPayloadView::Selector83(view) => {
                    assert_eq!(profile.selector, 83);
                    assert_eq!(view.prefix_span.start, prefix.prefix_span.end);
                    assert_eq!(view.prefix_span.len(), 16);
                    let raw = view.prefix_span.get(&bytes).unwrap();
                    let word = |at: usize| i16::from_le_bytes(raw[at..at + 2].try_into().unwrap());
                    assert_eq!(view.extent.map(|field| field.value), [word(4), word(6)]);
                    assert_eq!(
                        [view.depth, view.travel, view.risen, view.sunk].map(|field| field.value),
                        [word(8), word(10), word(12), word(14)]
                    );
                    selector83 += 1;
                }
                ConstructorPayloadView::TerrainLight(view) => {
                    assert!(matches!(profile.selector, 19 | 54));
                    assert_eq!(view.prefix_span.start, prefix.prefix_span.end);
                    let raw = view.prefix_span.get(&bytes).unwrap();
                    assert_eq!(raw.len(), 10);
                    assert_eq!(
                        view.flags.value,
                        u32::from_le_bytes(raw[..4].try_into().unwrap())
                    );
                    assert_eq!(
                        view.channels.value,
                        [raw[4] as i8, raw[5] as i8, raw[6] as i8]
                    );
                    assert_eq!(view.draw_gate.value, raw[7]);
                    assert_eq!(view.radius.value, u16::from_le_bytes([raw[8], raw[9]]));
                    terrain_lights += 1;
                }
                ConstructorPayloadView::PointTable16(view) => {
                    assert_eq!(profile.selector, 70);
                    assert_eq!((level, asset.local_index), (1, 77));
                    assert_eq!(view.word_04.value, 3);
                    assert_eq!(view.points_pointer.value, 9652);
                    assert_eq!(view.points_span.len(), 64);
                    for (index, point) in view.points.iter().enumerate() {
                        assert_eq!(point.span.start, view.points_span.start + index * 4);
                        let raw = point.span.get(&bytes).unwrap();
                        assert_eq!(
                            point.value,
                            [
                                u16::from_le_bytes(raw[..2].try_into().unwrap()),
                                u16::from_le_bytes(raw[2..].try_into().unwrap()),
                            ]
                        );
                    }
                    point_tables += 1;
                }
                ConstructorPayloadView::Selector45(_) | ConstructorPayloadView::Selector46(_) => {
                    arena += 1
                }
                ConstructorPayloadView::Goal(view) => {
                    assert_eq!(profile.selector, 92);
                    assert!(matches!(view.side.value, 1 | 2));
                    arena += 1
                }
                ConstructorPayloadView::FlagsWords(view) => {
                    assert!(matches!(profile.selector, 71 | 72));
                    assert_eq!(view.prefix_span.start, prefix.prefix_span.end);
                    let raw = view.prefix_span.get(&bytes).unwrap();
                    assert_eq!(
                        view.words.map(|field| field.value),
                        [4, 6].map(|at| i16::from_le_bytes([raw[at], raw[at + 1]]))
                    );
                    arena += 1
                }
                ConstructorPayloadView::Opaque => opaque += 1,
            }
            total += 1;
        }
    }
    assert_eq!(
        (
            total,
            models,
            actor,
            flags,
            terrain_lights,
            selector86,
            selector83,
            point_tables,
            arena,
            opaque
        ),
        (162, 155, 123, 23, 2, 1, 3, 1, 9, 0)
    );
    assert_eq!(model_selectors.len(), 79);
    assert_eq!(response_count, 1230);
    assert_eq!(response_profiles.len(), 54);
    assert_eq!(
        derived_selectors,
        BTreeMap::from([
            (2, 1),
            (9, 4),
            (10, 8),
            (11, 8),
            (12, 3),
            (13, 1),
            (21, 3),
            (34, 1),
            (35, 11),
            (36, 4),
            (60, 1),
            (77, 3),
            (79, 2),
            (80, 2),
        ])
    );
    assert_eq!(
        derived_banks,
        BTreeMap::from([(1, 43), (29, 3), (30, 3), (32, 3)])
    );
    assert_eq!((sound_bindings, positive_sounds), (208, 141));
    assert_eq!(
        sound_slots,
        BTreeSet::from([12, 14, 31, 33, 35, 37, 39, 40, 41, 43, 44, 45, 73, 74])
    );
}

#[test]
fn derived_actor_native_chains_preserve_the_payload_base_and_sound_read_policy() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    let mapped = FrontendExecutable::parse(&exe).unwrap();
    // Selector10/21's extra operand remains a complete raw DWORD. Its later
    // helper switch is unsigned; high bits never alias one of choices 0..4.
    assert_eq!(text_range(&exe, 0x0042_529e, 3), [0x8b, 0x47, 0x74]);
    assert_eq!(
        text_range(&exe, 0x0042_52a2, 6),
        [0x89, 0x86, 0x68, 2, 0, 0]
    );
    assert_eq!(
        text_range(&exe, 0x0042_52c9, 11),
        [0x8b, 0x86, 0x68, 2, 0, 0, 0x83, 0xf8, 4, 0x0f, 0x87]
    );
    assert_eq!(mapped.word(0x0053aa14 + 9 * 4).unwrap(), 0x0055ee98);
    // Each virtual +0C forwards context to +08, then its distinct second
    // argument unchanged to the class-data helper.
    for (method, target) in [
        (0x004239c0, 0x00423850),
        (0x00425270, 0x00425290),
        (0x00426bf0, 0x00426c10),
        (0x00433c70, 0x00433c00),
        (0x0043d250, 0x0043d1c0),
        (0x00468310, 0x00468330),
        (0x00482480, 0x00482460),
    ] {
        let code = text_range(&exe, method, 31);
        assert_eq!(
            &code[..23],
            &[
                0x56, 0x8b, 0xf1, 0x8b, 0x4c, 0x24, 8, 0x8b, 6, 0x51, 0x8b, 0xce, 0xff, 0x50, 8,
                0x8b, 0x54, 0x24, 12, 0x8b, 0xce, 0x52, 0xe8
            ],
            "method {method:08x}"
        );
        let relative = u32::from_le_bytes(code[23..27].try_into().unwrap());
        assert_eq!((method + 27).wrapping_add(relative), target);
        assert_eq!(&code[27..], &[0x5e, 0xc2, 8, 0]);
    }
    // Helpers push the same source pointer before invoking their shared parent.
    for (method, parent) in [
        (0x00423850, 0x00426c10),
        (0x00425290, 0x00423850),
        (0x00426c10, 0x0047ac10),
        (0x00433c00, 0x00426c10),
        (0x0043d1c0, 0x00426c10),
        (0x00468330, 0x00426c10),
        (0x00482460, 0x00423850),
    ] {
        let code = text_range(&exe, method, 14);
        assert_eq!(
            &code[..10],
            &[0x56, 0x57, 0x8b, 0x7c, 0x24, 12, 0x8b, 0xf1, 0x57, 0xe8]
        );
        let relative = u32::from_le_bytes(code[10..14].try_into().unwrap());
        assert_eq!((method + 14).wrapping_add(relative), parent);
    }
    assert_eq!(text_range(&exe, 0x00426c1e, 3), [0x8a, 0x47, 0x44]); // BYTE +44
    assert_eq!(text_range(&exe, 0x00426c27, 5), [0x66, 0x83, 0x7f, 0x48, 0]); // WORD zero-test
    assert_eq!(text_range(&exe, 0x00426c36, 4), [0x0f, 0xbf, 0x47, 0x4a]); // signed WORD
    assert_eq!(text_range(&exe, 0x00426c40, 3), [0x8b, 0x4f, 0x4c]); // DWORD
    for (address, register, offset) in [
        (0x00426c49, 0x57, 0x50),
        (0x00426c54, 0x47, 0x52),
        (0x00426c5f, 0x4f, 0x54),
        (0x00426c6a, 0x57, 0x56),
    ] {
        assert_eq!(text_range(&exe, address, 4), [0x66, 0x8b, register, offset]);
    }
    // All four later sound paths use signed object WORDs; +50/+52/+54 have
    // inline zero/negative gates, +56 has zero gate then generic signed lookup.
    for (address, offset) in [
        (0x00429698, 0x7c),
        (0x004296e5, 0x7e),
        (0x00429c41, 0x80),
        (0x00424320, 0x82),
    ] {
        assert_eq!(
            text_range(&exe, address, 7),
            [0x0f, 0xbf, 0x8e, offset, 1, 0, 0]
        );
    }
    assert_eq!(
        text_range(&exe, 0x0042965c, 15),
        [0x66, 0x3b, 0xc5, 0x0f, 0x84, 0xa0, 0, 0, 0, 0x0f, 0x8c, 0x9a, 0, 0, 0]
    );
    assert_eq!(
        text_range(&exe, 0x004296b5, 7),
        [0x66, 0x3b, 0xc5, 0x74, 0x4b, 0x7c, 0x49]
    );
    assert_eq!(
        text_range(&exe, 0x00429c10, 7),
        [0x66, 0x85, 0xc0, 0x74, 0x4c, 0x7c, 0x4a]
    );
    assert_eq!(
        text_range(&exe, 0x0042430c, 21),
        [
            0x66, 0x3b, 0xc5, 0x74, 0x2f, 0x50, 0xb9, 0x98, 0xee, 0x55, 0, 0xe8, 0x24, 0x12, 0xfe,
            0xff, 0x85, 0xc0, 0x74, 0x20, 0x0f
        ]
    );
    assert_eq!(
        text_range(&exe, 0x00405545, 5),
        [0x66, 0x85, 0xd2, 0x7c, 0x1d]
    );
}
