use super::*;
use crate::{HandlerFraming, HandlerId, RelocationTail};

fn put(main: &mut [u8], offset: usize, value: u32) {
    main[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(main: &[u8], relocations: &[u32], entries: &[u32]) -> (Vec<u8>, HandlerRegion) {
    let mut source = vec![0xa5; 13];
    let main_payload = ByteSpan::new(source.len(), source.len() + main.len());
    source.extend_from_slice(main);
    let relocation_offsets = ByteSpan::new(source.len(), source.len() + relocations.len() * 4);
    for offset in relocations {
        source.extend_from_slice(&offset.to_le_bytes());
    }
    let entry_offsets = ByteSpan::new(source.len(), source.len() + entries.len() * 4);
    for offset in entries {
        source.extend_from_slice(&offset.to_le_bytes());
    }
    let region = HandlerRegion {
        handler: HandlerId::new(14).unwrap(),
        requested_count: entries.len() as u32,
        span: ByteSpan::new(13, source.len()),
        framing: HandlerFraming::RelocationTail(RelocationTail {
            main_payload,
            relocation_count: relocations.len() as u32,
            relocation_offsets,
            entry_offsets,
        }),
    };
    (source, region)
}

fn main_payload() -> Vec<u8> {
    let mut main = vec![0xcc; 0x90];
    put(&mut main, 0x38, 2);
    put(&mut main, 0x3c, 0x50);
    put(&mut main, 0x40, 0x50);
    put(&mut main, 0x60, 2);
    put(&mut main, 0x64, 0x78);
    put(&mut main, 0x68, 0x78);
    put(&mut main, 0x78, 4);
    main
}

#[test]
fn graph_retains_world_placement_and_command_aliases_with_exact_pointer_custody() {
    let (source, region) = fixture(&main_payload(), &[0x3c, 0x40, 0x64, 0x68], &[0, 0]);
    let before = source.clone();
    let bank = parse_world_instance_bank(&source, &region).unwrap();
    assert_eq!(bank.instances.len(), 2);
    for (local, world) in bank.instances.iter().enumerate() {
        assert_eq!(world.world_local_index as usize, local);
        assert_eq!(world.placements.len(), 2);
        for (index, placement) in world.placements.iter().enumerate() {
            assert_eq!(placement.placement_index as usize, index);
            assert_eq!(placement.suffix_offset, 0x60);
            assert_eq!(
                placement.count,
                InstanceField {
                    span: ByteSpan::new(13 + 0x60, 13 + 0x64),
                    value: 2
                }
            );
            assert_eq!(
                placement.command_pointer_span,
                ByteSpan::new(13 + 0x64, 13 + 0x6c)
            );
            assert_eq!(placement.commands.len(), 2);
            for (i, command) in placement.commands.iter().enumerate() {
                assert_eq!(command.index as usize, i);
                assert_eq!(
                    command.pointer_span,
                    ByteSpan::new(13 + 0x64 + i * 4, 13 + 0x68 + i * 4)
                );
                assert_eq!(command.allocation_offset, 0x78);
                assert_eq!(
                    command.code,
                    InstanceField {
                        span: ByteSpan::new(13 + 0x78, 13 + 0x7c),
                        value: 4
                    }
                );
                assert_eq!(command.payload_offset, 0x7c);
            }
        }
    }
    assert_eq!(source, before);
    assert_eq!(bank.worlds.main_payload.len(), 0x90);
}

#[test]
fn nonpositive_counts_keep_native_signed_skip_without_reading_a_pointer() {
    for count in [0, -1, i32::MIN] {
        let mut main = main_payload();
        main.truncate(0x64);
        put(&mut main, 0x60, count as u32);
        let (source, region) = fixture(&main, &[0x3c, 0x40], &[0]);
        let bank = parse_world_instance_bank(&source, &region).unwrap();
        for placement in &bank.instances[0].placements {
            assert_eq!(placement.count.value, count);
            assert!(placement.commands.is_empty());
            assert_eq!(
                placement.command_pointer_span,
                ByteSpan::new(13 + 0x64, 13 + 0x64)
            );
        }
    }
}

#[test]
fn count_and_table_bounds_fail_before_reading_tail_or_allocating_commands() {
    let mut main = main_payload();
    main.truncate(0x63);
    let (source, region) = fixture(&main, &[0x3c, 0x40], &[0]);
    assert!(matches!(
        parse_world_instance_bank(&source, &region),
        Err(InstanceAssetError::OutOfBounds {
            context: "instance command count",
            offset: 0x60,
            length: 4,
            allocation_bytes: 0x63,
        })
    ));
    let mut main = main_payload();
    put(&mut main, 0x60, i32::MAX as u32);
    let (source, region) = fixture(&main, &[0x3c, 0x40], &[0]);
    assert!(matches!(
        parse_world_instance_bank(&source, &region),
        Err(InstanceAssetError::OutOfBounds {
            context: "instance command pointer table",
            ..
        }) | Err(InstanceAssetError::IntegerOverflow(_))
    ));
}

#[test]
fn every_walked_command_pointer_requires_an_actual_relocation_field() {
    let (source, region) = fixture(&main_payload(), &[0x3c, 0x40, 0x64], &[0]);
    assert_eq!(
        parse_world_instance_bank(&source, &region),
        Err(InstanceAssetError::MissingCommandRelocation(0x68))
    );
}

#[test]
fn relocated_targets_must_have_a_complete_command_code_inside_main_allocation() {
    for target in [0x8d, 0x90] {
        let mut main = main_payload();
        put(&mut main, 0x64, target);
        let (source, region) = fixture(&main, &[0x3c, 0x40, 0x64, 0x68], &[0]);
        assert!(
            matches!(parse_world_instance_bank(&source, &region), Err(InstanceAssetError::OutOfBounds {
            context: "instance command code", offset, length: 4, allocation_bytes: 0x90,
        }) if offset == target)
        );
    }
}

#[test]
fn zero_target_is_allocation_base_and_unknown_signed_codes_are_not_rewritten() {
    let mut main = main_payload();
    put(&mut main, 0, i32::MIN as u32);
    put(&mut main, 0x64, 0);
    let (source, region) = fixture(&main, &[0x3c, 0x40, 0x64, 0x68], &[0]);
    let bank = parse_world_instance_bank(&source, &region).unwrap();
    let command = &bank.instances[0].placements[0].commands[0];
    assert_eq!(command.allocation_offset, 0);
    assert_eq!(command.code.value, i32::MIN);
    assert_eq!(command.payload_offset, 4);
}
