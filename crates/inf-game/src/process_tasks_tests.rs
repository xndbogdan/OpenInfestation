use super::*;

const NULL: ActorHandle = ActorHandle::from_raw(0);

#[test]
fn construction_order_sources_callbacks_and_unwritten_words_are_distinct() {
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(NULL).unwrap();
    assert_eq!(tasks.references(), 1);
    for (ordinal, classification) in TASK_CONSTRUCTION_ORDER.into_iter().enumerate() {
        let owner = tasks.owner(classification).unwrap().unwrap();
        assert_eq!(owner.runtime, tasks.identity);
        assert_eq!(owner.allocation, ordinal);
        let record = tasks.record(owner).unwrap();
        assert_eq!(record.classification, classification);
        assert_eq!(record.source, NULL);
        assert_eq!(record.counters, [None; 5]);
        assert_eq!(
            record.callback,
            match classification {
                0 => TaskCallback::Zero4027b0,
                1 => TaskCallback::Zero4efbf0,
                18 => TaskCallback::ComponentClassification,
                _ => TaskCallback::TeamClassification,
            }
        );
    }
    assert_eq!(tasks.allocations.len(), 19);
}

#[test]
fn source_setter_uses_live_equality_and_null_returns_before_owner_dereference() {
    let mut tasks = ProcessTaskRuntime::default();
    let first = ActorHandle::from_raw(0x0801_1234);
    let other = ActorHandle::from_raw(0x0802_5678);
    let owner = tasks.construct(2, NULL).unwrap();
    tasks.set_source(owner, first, true, NULL).unwrap();
    tasks.set_source(owner, other, false, NULL).unwrap();
    assert_eq!(tasks.record(owner).unwrap().source, first);
    tasks.set_source(owner, NULL, true, NULL).unwrap();
    assert_eq!(tasks.record(owner).unwrap().source, first);
    tasks.set_source(owner, first, false, NULL).unwrap();
    assert_eq!(tasks.record(owner).unwrap().source, NULL);
    tasks.destroy(owner).unwrap();
    // 00482C60(null,0) does not dereference its stale receiver.
    tasks.set_source(owner, NULL, false, NULL).unwrap();
    assert_eq!(
        tasks.set_source(owner, first, true, NULL),
        Err(TaskStorageError::MissingOwner(owner))
    );
}

#[test]
fn first_constructor_wins_but_duplicate_destructor_clears_registered_alias() {
    let mut tasks = ProcessTaskRuntime::default();
    let first = tasks.construct(2, NULL).unwrap();
    let source = ActorHandle::from_raw(0x0801_1234);
    tasks.write_source(first, source).unwrap();
    let duplicate = tasks.construct(2, NULL).unwrap();
    assert_ne!(first, duplicate);
    assert_eq!(tasks.owner(2).unwrap(), Some(first));
    assert_eq!(tasks.record(duplicate).unwrap().source, NULL);
    tasks.destroy(duplicate).unwrap();
    assert_eq!(tasks.owner(2).unwrap(), None);
    assert_eq!(tasks.record(first).unwrap().source, source);
    let replacement = tasks.construct(2, NULL).unwrap();
    tasks.destroy(first).unwrap();
    assert_eq!(tasks.owner(2).unwrap(), None);
    assert_eq!(tasks.record(replacement).unwrap().source, NULL);
}

#[test]
fn shared_retains_and_world_counter_reset_preserve_source_until_final_release() {
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(NULL).unwrap();
    let owner = tasks.owner(2).unwrap().unwrap();
    let source = ActorHandle::from_raw(0x0801_1234);
    tasks.write_source(owner, source).unwrap();
    tasks.retain(ActorHandle::from_raw(0xdead_beef)).unwrap();
    assert_eq!(tasks.owner(2).unwrap(), Some(owner));
    assert_eq!(tasks.record(owner).unwrap().source, source);
    tasks.reset_counters().unwrap();
    assert_eq!(tasks.record(owner).unwrap().source, source);
    assert_eq!(tasks.record(owner).unwrap().counters, [Some(0); 5]);
    tasks.release().unwrap();
    assert_eq!(tasks.owner(2).unwrap(), Some(owner));
    tasks.release().unwrap();
    assert_eq!(tasks.owner(2).unwrap(), None);
    assert_eq!(
        tasks.record(owner),
        Err(TaskStorageError::MissingOwner(owner))
    );
    tasks.retain(NULL).unwrap();
    let replacement = tasks.owner(2).unwrap().unwrap();
    assert_ne!(owner, replacement);
    assert_eq!(tasks.record(replacement).unwrap().source, NULL);
    assert_eq!(tasks.record(replacement).unwrap().counters, [None; 5]);
}

#[test]
fn reset_missing_slot_retains_the_written_prefix_and_does_not_invent_null_owners() {
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(NULL).unwrap();
    let missing = tasks.owner(2).unwrap().unwrap();
    let before = tasks.owner(1).unwrap().unwrap();
    let after = tasks.owner(3).unwrap().unwrap();
    tasks.destroy(missing).unwrap();
    assert_eq!(tasks.reset_counters(), Err(TaskStorageError::NullOwner(2)));
    assert_eq!(tasks.record(before).unwrap().counters, [Some(0); 5]);
    assert_eq!(tasks.record(after).unwrap().counters, [None; 5]);
    assert_eq!(tasks.owner(19), Err(TaskStorageError::Classification(19)));
    assert_eq!(
        tasks.construct(u32::MAX, NULL),
        Err(TaskStorageError::Classification(u32::MAX))
    );
}

#[test]
fn registered_slot_cleanup_does_not_destroy_an_unregistered_allocation() {
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(NULL).unwrap();
    let duplicate = tasks.construct(2, NULL).unwrap();
    tasks.release().unwrap();
    assert_eq!(tasks.owner(2).unwrap(), None);
    assert_eq!(tasks.record(duplicate).unwrap().classification, 2);
}

#[test]
fn an_owner_from_another_process_cannot_alias_the_same_allocation_index() {
    let mut first = ProcessTaskRuntime::default();
    let mut second = ProcessTaskRuntime::default();
    let old = first.construct(2, NULL).unwrap();
    let fresh = second.construct(2, NULL).unwrap();
    assert_eq!(old.allocation, fresh.allocation);
    assert_ne!(old, fresh);
    assert_eq!(second.record(old), Err(TaskStorageError::MissingOwner(old)));
    assert_eq!(
        second.write_source(old, ActorHandle::from_raw(0x0801_1234)),
        Err(TaskStorageError::MissingOwner(old))
    );
    assert_eq!(
        second.destroy(old),
        Err(TaskStorageError::MissingOwner(old))
    );
    assert_eq!(second.owner(2).unwrap(), Some(fresh));
    assert_eq!(second.record(fresh).unwrap().source, NULL);
}

#[test]
fn retail_process_dependency_tables_and_task_callbacks_match_storage_custody() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let word = |address| {
        u32::from_le_bytes(
            executable
                .mapped(address, 4)
                .unwrap()
                .bytes
                .try_into()
                .unwrap(),
        )
    };
    // Main module retains actor module; actor module retains handle storage
    // before the task module. These are file-backed dependency descriptors.
    assert_eq!(word(0x0052_2158), 0x0047_8a70);
    assert_eq!(word(0x0052_215c), 0x0047_8a80);
    assert_eq!(word(0x0053_b89c), 0x0052_7ab8);
    assert_eq!(word(0x0052_7af8), 0x0046_1cc0);
    assert_eq!(word(0x0052_7b08), 0x0048_2c40);
    assert_eq!(word(0x0052_7b0c), 0x0048_2c50);
    assert_eq!(word(0x0053_bdc4), 0x0052_8088);
    assert_eq!(word(0x0052_8088), 0x0048_29a0);
    assert_eq!(word(0x0052_808c), 0x0048_2c00);
    assert_eq!(word(0x0052_809c), 0x0048_2ce0);
    assert_eq!(word(0x0052_80a0), 0x0048_2c90);
    assert_eq!(word(0x0052_80a4), 0x004e_fbf0);
    assert_eq!(word(0x0052_80a8), 0x0040_27b0);
    // FrontendExecutable intentionally maps only data/rdata. This source
    // fixture validates that profile first, then reads just the actual .text
    // section's bounded constructor range without expanding the data parser.
    let bytes = assets.executable_bytes();
    let dword = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let half = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
    let pe = dword(0x3c);
    let section_start = pe + 24 + half(pe + 20);
    let text = bytes[section_start..section_start + half(pe + 6) * 40]
        .chunks_exact(40)
        .find(|section| &section[..8] == b".text\0\0\0")
        .unwrap();
    let text_rva = u32::from_le_bytes(text[12..16].try_into().unwrap());
    let text_size = u32::from_le_bytes(text[16..20].try_into().unwrap()) as usize;
    let text_file = u32::from_le_bytes(text[20..24].try_into().unwrap()) as usize;
    let offset = (0x0048_29a0_u32 - 0x0040_0000 - text_rva) as usize;
    let text_bytes = &bytes[text_file..text_file + text_size];
    let code = &text_bytes[offset..offset + 0x250];
    let mut actual_order = Vec::new();
    for index in 0..code.len() - 8 {
        if code[index] == 0x6a && code[index + 2..index + 5] == [0x8b, 0xce, 0xe8] {
            let relative = i32::from_le_bytes(code[index + 5..index + 9].try_into().unwrap());
            let target = (0x0048_29a0 + index as u32 + 9).wrapping_add(relative as u32);
            if target == 0x0048_2950 {
                actual_order.push(u32::from(code[index + 1]));
            }
        }
    }
    assert_eq!(actual_order, TASK_CONSTRUCTION_ORDER);
}
