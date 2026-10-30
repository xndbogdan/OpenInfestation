//! Closure checks against synthetic tables; the installed executable's exact
//! result is pinned by the corpus test.

use super::*;

fn synthetic(dependencies: &[[u16; 3]]) -> GrantTable {
    let records = dependencies
        .iter()
        .enumerate()
        .map(|(index, &deps)| {
            let mut raw = [0u8; GRANT_RECORD_BYTES];
            for (slot, value) in deps.iter().enumerate() {
                raw[8 + slot * 2..10 + slot * 2].copy_from_slice(&value.to_le_bytes());
            }
            GrantRecord {
                id: GRANT_FIRST_ID + index as u32,
                file_span: ByteSpan::new(0, GRANT_RECORD_BYTES),
                raw,
                dependencies: deps,
            }
        })
        .collect();
    GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records,
    }
}

#[test]
fn borrowed_mask_query_keeps_zero_special_and_checks_the_last_word_boundary() {
    let mut mask = [0; GRANT_MASK_WORDS];
    assert_eq!(grant_is_owned(&mask, 0), Ok(true));
    assert_eq!(grant_is_owned(&mask, 191), Ok(false));
    mask[5] = 1 << 31;
    assert_eq!(grant_is_owned(&mask, 191), Ok(true));
    assert_eq!(grant_is_owned(&mask, 190), Ok(false));
    assert_eq!(grant_is_owned(&mask, 192), Err(GrantClosureError::Id(192)));
    assert_eq!(
        grant_is_owned(&mask, 65535),
        Err(GrantClosureError::Id(65535))
    );
}

#[test]
fn owned_zero_is_always_owned_and_large_ids_are_explicit_errors() {
    let table = synthetic(&[[0, 0, 0]]);
    let mask = grant_closure(&table, &[0]).unwrap();
    assert_eq!(mask, [0; GRANT_MASK_WORDS]);
    assert_eq!(
        grant_closure(&table, &[GRANT_MASK_BITS]),
        Err(GrantClosureError::Id(GRANT_MASK_BITS))
    );
}

#[test]
fn chain_grants_follow_owned_dependencies_in_order() {
    // 0x95 needs 7; 0x96 needs 0x95. Granting 7 pulls both, in ID order.
    let mut dependencies = [[0u16; 3]; GRANT_RECORD_COUNT];
    dependencies[0] = [7, 0, 0];
    dependencies[1] = [GRANT_FIRST_ID as u16, 0, 0];
    let table = synthetic(&dependencies);
    let mask = grant_closure(&table, &[7]).unwrap();
    assert_eq!(mask[0], 1u32 << 7);
    assert_eq!(mask[4], (1u32 << 21) | (1u32 << 22));
    assert_eq!(mask[1], 0);
    assert_eq!(mask[5], 0);
}

#[test]
fn high_bit_dependencies_are_unsigned_ids_and_fail_at_the_read_frontier() {
    for dependency in [0x8000, u16::MAX] {
        let mut dependencies = [[0u16; 3]; GRANT_RECORD_COUNT];
        dependencies[0] = [dependency, 0, 0];
        let table = synthetic(&dependencies);
        let mut mask = [0; GRANT_MASK_WORDS];
        assert_eq!(
            grant_into(&mut mask, &table, 7),
            Err(GrantClosureError::Id(u32::from(dependency)))
        );
        assert_eq!(mask, [1 << 7, 0, 0, 0, 0, 0]);
    }
}

#[test]
fn already_owned_ids_skip_the_rescan_without_changing_bits() {
    let mut dependencies = [[0u16; 3]; GRANT_RECORD_COUNT];
    dependencies[0] = [7, 0, 0];
    let table = synthetic(&dependencies);
    let mut mask = grant_closure(&table, &[7]).unwrap();
    let before = mask;
    grant_into(&mut mask, &table, 7).unwrap();
    assert_eq!(mask, before);
}

#[test]
fn incremental_grant_preserves_existing_bits_and_recurses_before_later_dependencies() {
    let mut dependencies = [[0; 3]; GRANT_RECORD_COUNT];
    dependencies[0] = [7, u16::MAX, 0];
    dependencies[1] = [GRANT_FIRST_ID as u16, 0, 0];
    let table = synthetic(&dependencies);
    let mut mask = [1 << 4, 0, 0, 0, 0, 1 << 31];
    grant_into(&mut mask, &table, 7).unwrap();
    assert_eq!(mask, [(1 << 4) | (1 << 7), 0, 0, 0, 3 << 21, 1 << 31]);
    // An invalid public DWORD never silently narrows to native ID0 or ID1.
    let before = mask;
    for id in [192, 0x1_0000, 0x1_0001, u32::MAX] {
        assert_eq!(
            grant_into(&mut mask, &table, id),
            Err(GrantClosureError::Id(id))
        );
        assert_eq!(mask, before);
    }
}

#[test]
fn ordered_merge_runs_each_grant_before_considering_the_next_source_bit() {
    let mut dependencies = [[0; 3]; GRANT_RECORD_COUNT];
    dependencies[0] = [3, u16::MAX, 0];
    let table = synthetic(&dependencies);
    let source = [(1 << 1) | (1 << 3), 0, 0, 0, 0, 0];
    let mut target = [0; GRANT_MASK_WORDS];
    assert_eq!(
        merge_grants(&mut target, &table, &source),
        Err(GrantClosureError::Id(65535))
    );
    // Grant 1 reaches the invalid second dependency before source ID 3 is read.
    assert_eq!(target, [1 << 1, 0, 0, 0, 0, 0]);
    assert_eq!(source, [(1 << 1) | (1 << 3), 0, 0, 0, 0, 0]);
}

#[test]
fn merge_preserves_target_high_bits_but_never_copies_source_ids_above_ae() {
    let table = synthetic(&[[0; 3]; GRANT_RECORD_COUNT]);
    let mut target = [1 << 4, 0, 0, 0, 0, 1 << 30];
    let source = [1, 0, 0, 0, 0, (1 << 14) | (1 << 15) | (1 << 31)];
    merge_grants(&mut target, &table, &source).unwrap();
    assert_eq!(target, [1 << 4, 0, 0, 0, 0, (1 << 14) | (1 << 30)]);
}

#[test]
fn merge_zero_and_existing_ids_do_not_retroactively_rescan_the_target() {
    // No table access is permitted when the only visited owned ID is zero.
    let mut target = [0; GRANT_MASK_WORDS];
    merge_grants(&mut target, &synthetic(&[]), &[0; GRANT_MASK_WORDS]).unwrap();
    assert_eq!(target, [0; GRANT_MASK_WORDS]);
    let mut dependencies = [[0; 3]; GRANT_RECORD_COUNT];
    dependencies[0] = [7, 0, 0];
    let table = synthetic(&dependencies);
    target[0] = 1 << 7;
    let source = target;
    merge_grants(&mut target, &table, &source).unwrap();
    assert_eq!(target, source);
    grant_into(&mut target, &table, 8).unwrap();
    assert_eq!(target[4], 1 << 21);
}
