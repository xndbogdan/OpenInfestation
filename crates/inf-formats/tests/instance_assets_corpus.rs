use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use inf_formats::{
    constructor_dispatch, decode_instance_command, parse_constructor_bank, parse_overlay,
    parse_world_instance_bank, ByteSpan, FrontendExecutable, InstanceCommandPayload,
    InstanceCommandRequest, InstanceCommandTarget, Preload, CONSTRUCTOR_SELECTOR_COUNT,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[test]
fn retail_bit_gates_read_unsigned_words_only_after_the_world_object_guard() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    // Both cases guard the first-world-object return before reading one WORD,
    // and pass the object's +9C bitset separately from the actor being disabled.
    for (address, displacement) in [(0x0047_af0a, 0x67), (0x0047_af38, 0x39)] {
        assert_eq!(
            text_range(&exe, address, 18),
            [
                0x85,
                0xc0,
                0x74,
                displacement,
                0x8b,
                0x4c,
                0x24,
                0x10,
                0x66,
                0x8b,
                0x11,
                0x8d,
                0x88,
                0x9c,
                0,
                0,
                0,
                0x52,
            ]
        );
    }
    // Opposite skip conditions after the same predicate:13 disables on false,
    // 14 on true. 4752E0 masks the input to an unsigned WORD, not a signed index.
    assert_eq!(text_range(&exe, 0x0047_af21, 4), [0x84, 0xc0, 0x75, 0x50]);
    assert_eq!(text_range(&exe, 0x0047_af4f, 4), [0x84, 0xc0, 0x74, 0x22]);
    assert_eq!(
        text_range(&exe, 0x0047_52e6, 15),
        [0x66, 0x85, 0xc0, 0x75, 5, 0xb0, 1, 0xc2, 4, 0, 0x25, 0xff, 0xff, 0, 0,]
    );
}

/// Original .text access only for native-contract regression checks.
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
fn route_node_native_dispatch_pins_exact_reads_wrapping_and_operand_free_cases() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    assert_eq!(
        text_range(&exe, 0x0040_4cd7, 10),
        [
            0x83, 0xc2, 0xfd, // code - 3
            0xb0, 1, // success result and mode0 flag mask
            0x83, 0xfa, 5, 0x77, 0x25, // unsigned table bound -> rejection
        ]
    );
    let destinations: Vec<_> = text_range(&exe, 0x0040_4d20, 24)
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert_eq!(
        destinations,
        [
            0x0040_4ce8,
            0x0040_4cf8,
            0x0040_4d11,
            0x0040_4d08,
            0x0040_4d08,
            0x0040_4d18
        ]
    );
    assert_eq!(
        text_range(&exe, 0x0040_4cec, 12),
        [
            0x66, 0x8b, 0x12, // WORD read
            0x66, 0x4a, // WORD decrement
            0x66, 0x89, 0x51, 0x5e, // WORD store +5E
            0xc2, 8, 0,
        ]
    );
    assert_eq!(
        text_range(&exe, 0x0040_4cfc, 15),
        [
            0x8b, 0x12, // full DWORD read
            0x83, 0xea, 0, 0x74, 8, // zero -> flag1 path
            0x4a, 0x74, 2, // one -> unchanged success
            0x32, 0xc0, // other -> false
            0xc2, 8, 0,
        ]
    );
    assert_eq!(
        text_range(&exe, 0x0040_4d0b, 20),
        [
            0x08, 0x41, 0x48, 0xc2, 8, 0, // mode0 OR1
            0x80, 0x49, 0x48, 2, 0xc2, 8, 0, // code5 OR2
            0x80, 0x49, 0x48, 0x10, 0xc2, 8, 0, // code8 OR0x10
        ]
    );
    // The placement identity at +5C comes from context, not H13 payload.
    assert_eq!(
        text_range(&exe, 0x0040_4c3e, 10),
        [0x66, 0x8b, 0x47, 0x0c, 0x5f, 0x66, 0x89, 0x46, 0x5c, 0x5e,]
    );
}

#[test]
fn expanded_profiles_preserve_the_original_signed_forwarding_guard_and_arguments() {
    let exe = fs::read(root().join("Infest.exe")).unwrap();
    // Virtual +4 wrappers: original signed upper bound and exact shared callee.
    // The verified head also proves no later specialized code runs after forwarding.
    for (method, limit, parent) in [
        (0x00401080, 2, 0x0047a1a0),
        (0x00402330, 18, 0x00426030),
        (0x00404420, 2, 0x0047a1a0),
        (0x00404cc0, 2, 0x0047a1a0),
        (0x00405d10, 15, 0x0047ada0),
        (0x00406800, 15, 0x0047ada0),
        (0x00408880, 18, 0x00426030),
        (0x00409320, 1, 0x00479e70),
        (0x0040a1d0, 20, 0x0040d9c0),
        (0x0040f7d0, 2, 0x0047a1a0),
        (0x00411100, 15, 0x0047ada0),
        (0x00425240, 24, 0x004239e0),
        (0x0042d6f0, 15, 0x0047ada0),
        (0x0042def0, 1, 0x00479e70),
        (0x0042f100, 20, 0x0040d9c0),
        (0x00433c90, 22, 0x00429440),
        (0x0043b7d0, 20, 0x0040d9c0),
        (0x0043c2a0, 15, 0x0047ada0),
        (0x0043c840, 2, 0x0047a1a0),
        (0x0043d270, 22, 0x00429440),
        (0x0043ff40, 2, 0x0047a1a0),
        (0x004407e0, 2, 0x0047a1a0),
        (0x00443900, 15, 0x0047ada0),
        (0x00445900, 1, 0x00479e70),
        (0x00448120, 2, 0x0047a1a0),
        (0x00454220, 2, 0x0047a1a0),
        (0x00454880, 15, 0x0047ada0),
        (0x00454a30, 15, 0x0047ada0),
        (0x00458c40, 15, 0x0047ada0),
        (0x0045c010, 2, 0x0047a1a0),
        (0x00464940, 2, 0x0047a1a0),
        (0x004689d0, 23, 0x004672c0),
        (0x0046d780, 2, 0x0047a1a0),
        (0x0046e4f0, 15, 0x0047ada0),
        (0x0046f810, 2, 0x0047a1a0),
        (0x0046fa70, 2, 0x0047a1a0),
        (0x00473f70, 15, 0x0047ada0),
        (0x00475cb0, 2, 0x0047a1a0),
        (0x004763f0, 20, 0x0040d9c0),
        (0x00478860, 15, 0x0047ada0),
        (0x0047fdc0, 20, 0x0040d9c0),
        (0x00480b30, 18, 0x00426030),
        (0x0048bbf0, 20, 0x0040d9c0),
        (0x0048c7a0, 1, 0x00479e70),
        (0x00429440, 22, 0x004276b0),
    ] {
        let bytes = text_range(&exe, method, 31);
        let (call, expected_head, expected_tail): (usize, Vec<u8>, &[u8]) =
            if matches!(method, 0x0043c840 | 0x0043ff40 | 0x004689d0) {
                (
                    21,
                    vec![
                        0x8b, 0x44, 0x24, 4, 0x56, 0x83, 0xf8, limit, 0x8b, 0xf1, 0x7f, 0x11, 0x8b,
                        0x4c, 0x24, 12, 0x51, 0x50, 0x8b, 0xce, 0xe8,
                    ],
                    &[0x5e, 0xc2, 8, 0],
                )
            } else if bytes[1] == 0x54 {
                (
                    16,
                    vec![
                        0x8b, 0x54, 0x24, 4, 0x83, 0xfa, limit, 0x7f, 14, 0x8b, 0x44, 0x24, 8,
                        0x50, 0x52, 0xe8,
                    ],
                    &[0xc2, 8, 0],
                )
            } else {
                (
                    16,
                    vec![
                        0x8b, 0x44, 0x24, 4, 0x83, 0xf8, limit, 0x7f, 14, 0x8b, 0x54, 0x24, 8,
                        0x52, 0x50, 0xe8,
                    ],
                    &[0xc2, 8, 0],
                )
            };
        assert_eq!(&bytes[..call], expected_head, "method {method:08x}");
        let relative = u32::from_le_bytes(bytes[call..call + 4].try_into().unwrap());
        assert_eq!(
            (method + call as u32 + 4).wrapping_add(relative),
            parent,
            "method {method:08x}"
        );
        assert_eq!(
            &bytes[call + 4..call + 4 + expected_tail.len()],
            expected_tail,
            "method {method:08x}"
        );
    }
    // Selector10 and selector21 retain 00425240 in virtual +4. Above the
    // signed shared limit, only exact code25 accepts, without any operand read.
    assert_eq!(
        text_range(&exe, 0x0042_5257, 13),
        [0x33, 0xc9, 0x83, 0xf8, 0x19, 0x0f, 0x94, 0xc1, 0x8a, 0xc1, 0xc2, 8, 0]
    );
    // The intermediate +18 path uses a larger stack frame but preserves both args.
    assert_eq!(
        text_range(&exe, 0x004276b0, 35),
        [
            0x8b, 0x44, 0x24, 4, 0x83, 0xec, 8, 0x83, 0xf8, 18, 0x55, 0x8b, 0xe9, 0x7f, 20, 0x8b,
            0x4c, 0x24, 20, 0x51, 0x50, 0x8b, 0xcd, 0xe8, 0x64, 0xe9, 0xff, 0xff, 0x5d, 0x83, 0xc4,
            8, 0xc2, 8, 0,
        ]
    );
}

#[test]
fn installed_specialized_commands_keep_relocated_rows_and_authored_model_inputs() {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    // Pin the concrete original bank rather than assuming a slot is a selector.
    let shared = fs::read(root.join("Overlay/1.ovl")).unwrap();
    let shared_leaf = preload.leaf(1).unwrap();
    let layout = parse_overlay(&shared, &shared_leaf.base).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|r| r.handler.get() == 13)
        .unwrap();
    let constructors = parse_constructor_bank(&shared, region).unwrap();
    let specialized: BTreeMap<_, _> = constructors
        .entries
        .iter()
        .filter(|entry| matches!(entry.constructor_selector, 60 | 69))
        .map(|entry| {
            (
                shared_leaf
                    .base
                    .get(13)
                    .unwrap()
                    .global_index(entry.local_index as u16)
                    .unwrap() as i16,
                entry.constructor_selector,
            )
        })
        .collect();
    assert_eq!(specialized, BTreeMap::from([(5, 69), (45, 60)]));
    // No other original H13 bank registers either slot: scene residency cannot
    // silently replace the concrete selectors under test.
    for level in [6, 29, 30, 32] {
        let request = preload.leaf(level).unwrap().base.get(13).unwrap();
        for local in 0..request.count as u16 {
            assert!(!specialized.contains_key(&(request.global_index(local).unwrap() as i16)));
        }
    }
    let mut placements = BTreeMap::new();
    let mut codes = BTreeMap::new();
    let mut lists = 0;
    let mut rows = 0;
    for level in 11..=43 {
        let source = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&source, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_instance_bank(&source, region).unwrap();
        for (world, instances) in bank.worlds.entries.iter().zip(&bank.instances) {
            for (placement, suffix) in world.placements.iter().zip(&instances.placements) {
                let Some(&selector) = specialized.get(&placement.constructor_slot()) else {
                    continue;
                };
                *placements.entry(selector).or_insert(0) += 1;
                for command in &suffix.commands {
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &source,
                        main_payload: bank.worlds.main_payload,
                        relocations: &bank.worlds.relocations,
                        command_offset: command.allocation_offset,
                        target: InstanceCommandTarget::Constructor(selector),
                    })
                    .unwrap();
                    *codes.entry((selector, view.code.value)).or_insert(0) += 1;
                    if let InstanceCommandPayload::PlacementSpawnList(list) = &view.payload {
                        lists += 1;
                        assert!(list.count.value > 0);
                        assert_eq!(list.count.value as usize, list.rows.len());
                        assert_eq!(list.rows_span.unwrap().len(), list.rows.len() * 24);
                        let pointer = list.rows_pointer.unwrap();
                        assert!(bank
                            .worlds
                            .relocations
                            .iter()
                            .any(|r| r.field_span == pointer.span
                                && r.target_offset == pointer.value));
                        for row in &list.rows {
                            assert_eq!(row.span.len(), 24);
                            assert_eq!(
                                row.placement_reference.value,
                                i32::from_le_bytes(
                                    source[row.span.start..row.span.start + 4]
                                        .try_into()
                                        .unwrap()
                                )
                            );
                            for field in &row.signed_words_04_to_0e {
                                assert_eq!(
                                    field.value,
                                    i16::from_le_bytes(
                                        field.span.get(&source).unwrap().try_into().unwrap()
                                    )
                                );
                            }
                            assert_eq!(
                                row.runtime_pointer_placeholder.value,
                                u32::from_le_bytes(
                                    row.runtime_pointer_placeholder
                                        .span
                                        .get(&source)
                                        .unwrap()
                                        .try_into()
                                        .unwrap()
                                )
                            );
                            rows += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(placements, BTreeMap::from([(60, 55), (69, 21)]));
    assert_eq!(
        codes,
        BTreeMap::from([
            ((60, 1), 3),
            ((60, 2), 15),
            ((60, 9), 1),
            ((60, 13), 2),
            ((60, 14), 10),
            ((60, 15), 1),
            ((60, 16), 1),
            ((60, 18), 31),
            ((60, 19), 55),
            ((69, 9), 8),
            ((69, 16), 21),
            ((69, 17), 7),
        ])
    );
    assert_eq!((lists, rows), (55, 56));
}

/// Every installed placement of the shared terrain-light class `selector`,
/// its commands decoded for that class, whose consumer is `method`.
struct TerrainLightPlacements {
    slot: i16,
    placements: usize,
    first_world_placements: usize,
    codes: BTreeMap<i32, usize>,
    preload: Preload,
}

fn terrain_light_placements(selector: u16, method: u32) -> TerrainLightPlacements {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let shared = fs::read(root.join("Overlay/1.ovl")).unwrap();
    let shared_leaf = preload.leaf(1).unwrap();
    let layout = parse_overlay(&shared, &shared_leaf.base).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|r| r.handler.get() == 13)
        .unwrap();
    let constructors = parse_constructor_bank(&shared, region).unwrap();
    let light = constructors
        .entries
        .iter()
        .find(|entry| entry.constructor_selector == selector)
        .unwrap();
    let slot = shared_leaf
        .base
        .get(13)
        .unwrap()
        .global_index(light.local_index as u16)
        .unwrap() as i16;
    let mut placements = 0;
    let mut first_world_placements = 0;
    let mut codes = BTreeMap::new();
    for level in 11..=43 {
        let source = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&source, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_instance_bank(&source, region).unwrap();
        for (world, instances) in bank.worlds.entries.iter().zip(&bank.instances) {
            for (placement, suffix) in world.placements.iter().zip(&instances.placements) {
                if placement.constructor_slot() != slot {
                    continue;
                }
                placements += 1;
                first_world_placements += usize::from(level == 32);
                for command in &suffix.commands {
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &source,
                        main_payload: bank.worlds.main_payload,
                        relocations: &bank.worlds.relocations,
                        command_offset: command.allocation_offset,
                        target: InstanceCommandTarget::Constructor(selector),
                    })
                    .unwrap();
                    assert_eq!(view.proven_method, Some(method));
                    *codes.entry(view.code.value).or_insert(0) += 1;
                    match view.payload {
                        InstanceCommandPayload::NoPayloadRead => {
                            assert!(matches!(view.code.value, 0 | 1))
                        }
                        InstanceCommandPayload::ModelSlot(model) => {
                            assert_eq!(view.code.value, 2);
                            let bytes = model.raw.span.get(&source).unwrap();
                            assert_eq!(
                                model.raw.value,
                                u16::from_le_bytes(bytes.try_into().unwrap())
                            );
                        }
                        InstanceCommandPayload::TerrainLightChannel { channel, value } => {
                            assert_eq!(view.code.value, i32::from(channel) + 3);
                            assert_eq!(value.span.len(), 1);
                            assert_eq!(value.value, source[value.span.start] as i8);
                            assert_eq!(view.payload_consumer, Some(method));
                        }
                        InstanceCommandPayload::TerrainLightDrawGate(value) => {
                            assert_eq!(view.code.value, 6);
                            assert_eq!(value.span.len(), 1);
                            assert_eq!(value.value, source[value.span.start]);
                            assert_eq!(view.payload_consumer, Some(method));
                        }
                        InstanceCommandPayload::TerrainLightRadius(value) => {
                            assert_eq!(view.code.value, 7);
                            let bytes = value.span.get(&source).unwrap();
                            assert_eq!(value.value, u16::from_le_bytes(bytes.try_into().unwrap()));
                            assert_eq!(view.payload_consumer, Some(method));
                        }
                        other => panic!("unexpected light command {other:?}"),
                    }
                }
            }
        }
    }
    TerrainLightPlacements {
        slot,
        placements,
        first_world_placements,
        codes,
        preload,
    }
}

#[test]
fn installed_terrain_light_commands_keep_exact_authored_bytes_and_class_meanings() {
    let lights = terrain_light_placements(19, 0x0044_8120);
    for level in [6, 29, 30, 32] {
        let request = lights.preload.leaf(level).unwrap().base.get(13).unwrap();
        for local in 0..request.count as u16 {
            assert_ne!(request.global_index(local).unwrap() as i16, lights.slot);
        }
    }
    assert_eq!(lights.first_world_placements, 42);
    assert_eq!((lights.slot, lights.placements), (53, 1125));
    assert_eq!(
        lights.codes,
        BTreeMap::from([
            (1, 67),
            (2, 374),
            (3, 1124),
            (4, 1122),
            (5, 1091),
            (6, 267),
            (7, 1125)
        ])
    );
}

/// Selector 54's 00464940 decodes the same codes as selector 19's 00448120.
#[test]
fn installed_rotating_terrain_light_commands_decode_as_selector_19s() {
    let lights = terrain_light_placements(54, 0x0046_4940);
    assert_eq!(lights.first_world_placements, 0);
    assert_eq!((lights.slot, lights.placements), (35, 133));
    // Every placement sets its model and draw gate.
    assert_eq!(
        lights.codes,
        BTreeMap::from([
            (1, 2),
            (2, 133),
            (3, 126),
            (4, 126),
            (5, 126),
            (6, 133),
            (7, 126)
        ])
    );
}

#[test]
fn all_4486_retail_suffixes_expose_11476_ordered_relocated_commands() {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let expected_commands = [
        115, 257, 267, 576, 398, 676, 330, 497, 432, 412, 618, 381, 243, 304, 322, 159, 281, 520,
        115, 444, 229, 297, 198, 554, 206, 431, 240, 427, 788, 19, 159, 431, 150,
    ];
    let mut counts = BTreeMap::new();
    let mut codes = BTreeMap::new();
    let mut placements = 0;
    let mut commands = 0;
    for (level, expected) in (11..=43).zip(expected_commands) {
        let source = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&source, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_instance_bank(&source, region).unwrap();
        assert_eq!(bank.instances.len(), bank.worlds.entries.len());
        let relocations: BTreeMap<_, _> = bank
            .worlds
            .relocations
            .iter()
            .map(|r| (r.field_offset, r.target_offset))
            .collect();
        let mut this_world = 0;
        let mut unique_targets = BTreeSet::new();
        for (world, instance) in bank.worlds.entries.iter().zip(&bank.instances) {
            assert_eq!(world.local_index, instance.world_local_index);
            assert_eq!(world.placements.len(), instance.placements.len());
            for (placement, suffix) in world.placements.iter().zip(&instance.placements) {
                assert_eq!(placement.index, suffix.placement_index);
                assert_eq!(placement.payload_offset, suffix.suffix_offset);
                assert_eq!(suffix.count.span.start, placement.prefix_span.end);
                assert_eq!(suffix.count.value as usize, suffix.commands.len());
                assert_eq!(suffix.command_pointer_span.len(), suffix.commands.len() * 4);
                *counts.entry(suffix.count.value).or_insert(0) += 1;
                for (index, command) in suffix.commands.iter().enumerate() {
                    assert_eq!(command.index as usize, index);
                    let pointer_offset =
                        (command.pointer_span.start - bank.worlds.main_payload.start) as u32;
                    assert_eq!(relocations[&pointer_offset], command.allocation_offset);
                    assert_eq!(command.payload_offset, command.allocation_offset + 4);
                    assert_eq!(
                        command.code.span.start,
                        bank.worlds.main_payload.start + command.allocation_offset as usize
                    );
                    assert_eq!(
                        command.code.value,
                        i32::from_le_bytes(
                            command.code.span.get(&source).unwrap().try_into().unwrap()
                        )
                    );
                    *codes.entry(command.code.value).or_insert(0) += 1;
                    unique_targets.insert(command.allocation_offset);
                    this_world += 1;
                }
                placements += 1;
            }
        }
        assert_eq!(this_world, expected, "world leaf {level}");
        assert_eq!(
            unique_targets.len(),
            this_world,
            "installed graph has distinct command targets"
        );
        commands += this_world;
    }
    assert_eq!((placements, commands), (4486, 11476));
    assert_eq!(
        counts,
        BTreeMap::from([
            (0, 246),
            (1, 1073),
            (2, 1506),
            (3, 302),
            (4, 772),
            (5, 173),
            (6, 367),
            (7, 46),
            (8, 1)
        ])
    );
    assert_eq!(
        codes,
        BTreeMap::from([
            (1, 213),
            (2, 850),
            (3, 2964),
            (4, 2451),
            (5, 1371),
            (6, 659),
            (7, 1280),
            (8, 58),
            (9, 110),
            (10, 15),
            (11, 11),
            (12, 4),
            (13, 81),
            (14, 39),
            (15, 7),
            (16, 128),
            (17, 152),
            (18, 400),
            (19, 95),
            (20, 1),
            (21, 150),
            (22, 133),
            (23, 125),
            (24, 62),
            (25, 19),
            (26, 34),
            (27, 18),
            (28, 16),
            (29, 1),
            (30, 13),
            (31, 2),
            (32, 14)
        ])
    );
}

#[test]
fn all_audited_command_profiles_match_the_original_vtable_plus_four() {
    let bytes = fs::read(root().join("Infest.exe")).unwrap();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let mut audited = 0;
    for selector in 1..=CONSTRUCTOR_SELECTOR_COUNT {
        let view = decode_instance_command(InstanceCommandRequest {
            read_scope: inf_formats::InstanceCommandReadScope::Inspection,
            source: &[0; 4],
            main_payload: ByteSpan::new(0, 4),
            relocations: &[],
            command_offset: 0,
            target: InstanceCommandTarget::Constructor(selector),
        })
        .unwrap();
        if let Some(method) = view.proven_method {
            let dispatch = constructor_dispatch(selector).unwrap();
            assert_eq!(
                exe.word(dispatch.vtable + 4).unwrap(),
                method,
                "selector {selector}"
            );
            assert_eq!(view.payload, InstanceCommandPayload::NoPayloadRead);
            audited += 1;
        } else {
            assert_eq!(view.payload, InstanceCommandPayload::Opaque);
        }
    }
    assert_eq!(audited, 94);
}

/// Every installed Siren (selector 47, shared H13 slot 26) sets its alarm
/// seconds (code 22), so its BYTE +16C is never left unwritten; nine set a
/// radius (code 23) over the constructor's 0xA00. Its own codes decode
/// through 0047FDC0.
#[test]
fn installed_sirens_set_their_seconds() {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut placements = 0;
    let mut leaves = BTreeSet::new();
    let mut codes = BTreeMap::new();
    for level in 11..=43 {
        let source = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&source, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_instance_bank(&source, region).unwrap();
        for (world, instances) in bank.worlds.entries.iter().zip(&bank.instances) {
            for (placement, suffix) in world.placements.iter().zip(&instances.placements) {
                if placement.constructor_slot() != 26 {
                    continue;
                }
                placements += 1;
                leaves.insert(level);
                let mut seconds = false;
                for command in &suffix.commands {
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &source,
                        main_payload: bank.worlds.main_payload,
                        relocations: &bank.worlds.relocations,
                        command_offset: command.allocation_offset,
                        target: InstanceCommandTarget::Constructor(47),
                    })
                    .unwrap();
                    *codes.entry(view.code.value).or_insert(0) += 1;
                    match (view.code.value, &view.payload) {
                        (22, InstanceCommandPayload::Selector47Seconds16c(value)) => {
                            assert_eq!(view.payload_consumer, Some(0x0047_fdc0));
                            assert_eq!(value.value, source[value.span.start]);
                            seconds = true;
                        }
                        (23, InstanceCommandPayload::Selector47Radius164(value)) => {
                            let bytes = value.span.get(&source).unwrap();
                            assert_eq!(value.value, i16::from_le_bytes(bytes.try_into().unwrap()));
                        }
                        (22 | 23, other) => panic!("unexpected Siren payload {other:?}"),
                        _ => {}
                    }
                }
                assert!(seconds, "level {level} placement {:?}", placement);
            }
        }
    }
    assert_eq!(placements, 12);
    assert_eq!(
        leaves.into_iter().collect::<Vec<_>>(),
        [14, 18, 19, 20, 21, 25, 27, 31]
    );
    assert_eq!(
        codes,
        BTreeMap::from([
            (6, 1),
            (10, 8),
            (11, 1),
            (13, 3),
            (14, 1),
            (22, 12),
            (23, 9)
        ])
    );
}
