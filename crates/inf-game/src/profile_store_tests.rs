use super::*;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const STATE_BYTES: usize = 910 + 174;

/// Retail's executable and the port share one save list, so the default is
/// the file retail itself writes, not a port-side copy.
#[test]
fn default_save_is_retails_own_file_beside_the_game_data() {
    assert_eq!(
        ProfileStore::default_path(Path::new("install")),
        Path::new("install").join("Infest.sav")
    );
}

#[test]
fn menu_selection_and_control_writes_do_not_load_or_save() {
    let files = Files::new(&[record(b"One", 2, 10), record(b"Two", 3, 20)]);
    let mut store = files.open();
    assert_eq!(store.cached_count(), 0);
    assert_eq!(store.control_token(), ProfileControlToken::Unknown);
    store.query(false).unwrap();
    assert_eq!(store.cached_count(), 2);
    store.select_for_menu(1).unwrap();
    assert_eq!(store.selected(), 1);
    assert_eq!(store.working_record().unwrap().name(), b"Two");
    store.select_for_menu(2).unwrap(); // append index, not an occupied slot
    assert!(store.select_for_menu(3).is_err());
    store.record_control_token(ProfileControlToken::MinusOne);
    store.query(false).unwrap();
    assert_eq!(store.control_token(), ProfileControlToken::MinusOne);
    store.query(true).unwrap();
    assert_eq!(store.control_token(), ProfileControlToken::Unknown);
    store.record_control_token(ProfileControlToken::Zero);
    assert_eq!(store.control_token(), ProfileControlToken::Zero);
    assert!(!store.write_attempted());
    assert_eq!(
        fs::read(&files.save).unwrap(),
        fs::read(&files.save).unwrap()
    );
}

#[test]
fn native_format_stub_only_clears_cached_directory_until_forced_query() {
    let files = Files::new(&[record(b"One", 2, 10)]);
    let mut store = files.open();
    store.query(false).unwrap();
    store.reset_directory_after_native_format();
    assert_eq!(store.query_status(), ProfileStatus::Ready);
    assert_eq!(store.query(false).unwrap().count, 0);
    assert_eq!(store.slot_status(0).unwrap(), ProfileStatus::EmptySlot);
    assert_eq!(store.working_record().unwrap().name(), b"One");
    assert_eq!(files.disk().len(), 1);
    assert!(!store.write_attempted());
    assert_eq!(store.query(true).unwrap().count, 1);
}

struct Files {
    /// The one save, retail's and the port's alike.
    save: PathBuf,
}

impl Files {
    fn new(records: &[SaveRecord]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../.tmp/inf-profile-store-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&root).unwrap();
        let files = Self {
            save: root.join("Infest.sav"),
        };
        fs::write(&files.save, encode_save_records(records).unwrap()).unwrap();
        files
    }
    fn open(&self) -> ProfileStore {
        ProfileStore::open(&self.save).unwrap()
    }
    fn disk(&self) -> Vec<SaveRecord> {
        SaveArchive::parse(&fs::read(&self.save).unwrap())
            .unwrap()
            .records()
            .to_vec()
    }
}

fn record(name: &[u8], mission: u32, elapsed: u32) -> SaveRecord {
    let mut payload = [0xa7; WORKING_BYTES];
    payload[..name.len()].copy_from_slice(name);
    payload[name.len()] = 0;
    let start = name.len() + 1;
    payload[start..start + 4].copy_from_slice(&mission.to_le_bytes());
    payload[start + 4..start + 8].copy_from_slice(&elapsed.to_le_bytes());
    SaveRecord::from_payload(payload).unwrap()
}

#[test]
fn open_reads_retails_save_as_it_is_and_directory_scan_owns_working_buffer_not_current_name() {
    let files = Files::new(&[record(b"Jameson", 1, 14090), record(b"BOBSTER", 11, 5033)]);
    let original = fs::read(&files.save).unwrap();
    let mut store = files.open();
    assert_eq!(fs::read(&files.save).unwrap(), original);
    assert!(!store.write_attempted());
    assert_eq!(store.query_status(), ProfileStatus::Unqueried);
    assert!(store.working_record().is_none());
    assert_eq!(store.current_name(), None);
    assert!(matches!(
        store.restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut MissionSaveState::new_game(),
            state: &mut SessionState::default(),
            arena: &mut live_arena(),
            classification: &live_classification(),
        }),
        Err(WorkingProfileStateError::Profile(
            ProfileStoreError::NoWorkingRecord
        ))
    ));
    assert_eq!(store.current_name(), None);
    let directory = store.query(false).unwrap();
    assert_eq!(directory.count, 2);
    assert!(store.entry(0).unwrap().unwrap().load_is_rejected());
    assert!(!store.entry(1).unwrap().unwrap().load_is_rejected());
    assert_eq!(store.entry(0).unwrap().unwrap().elapsed_seconds, 14090);
    assert_eq!(store.working_record().unwrap().name(), b"BOBSTER");
    assert_eq!(store.current_name(), None);
    assert_eq!(store.slot_status(13).unwrap(), ProfileStatus::EmptySlot);
    assert_eq!(store.load(0).unwrap().name(), b"Jameson");
    assert_eq!(store.selected(), 0);
    assert_eq!(store.current_name(), None);
    store
        .restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut MissionSaveState::new_game(),
            state: &mut SessionState::default(),
            arena: &mut live_arena(),
            classification: &live_classification(),
        })
        .unwrap();
    assert_eq!(store.current_name(), Some(b"Jameson".as_slice()));
    assert!(!store.write_attempted());
    assert_eq!(fs::read(&files.save).unwrap(), original);
}

#[test]
fn an_empty_save_is_an_empty_list_and_invalid_data_is_left_as_it_is() {
    let files = Files::new(&[record(b"Retail", 3, 10)]);
    fs::write(&files.save, []).unwrap();
    assert_eq!(files.open().query(false).unwrap().count, 0);
    fs::write(&files.save, b"invalid existing file").unwrap();
    assert!(matches!(
        ProfileStore::open(&files.save),
        Err(ProfileStoreError::Format { .. })
    ));
    assert_eq!(fs::read(&files.save).unwrap(), b"invalid existing file");
}

/// No save yet: an empty list, and reading creates nothing; the first save
/// writes retail's file.
#[test]
fn missing_save_starts_empty_and_name_confirmation_only_serializes_memory() {
    let files = Files::new(&[]);
    fs::remove_file(&files.save).unwrap();
    let source = record(b"Source", 22, 0);
    let mut store = files.open();
    assert_eq!(store.query(false).unwrap().count, 0);
    store.prepare_new_name(b"Pilot").unwrap();
    store
        .serialize_working(&source.mission(), &source.player())
        .unwrap();
    assert_eq!(store.selected(), 0);
    assert_eq!(store.working_record().unwrap().name(), b"Pilot");
    assert!(store
        .working_record()
        .unwrap()
        .tail()
        .iter()
        .all(|byte| *byte == 0));
    assert!(!files.save.exists());
    assert!(!store.write_attempted());
    store.save_working().unwrap();
    assert!(store.write_attempted());
    assert_eq!(files.disk()[0].name(), b"Pilot");
    assert_eq!(files.disk()[0].mission(), source.mission());
}
#[test]
fn new_profile_appends_and_serializer_preserves_working_bytes_after_new_body() {
    let existing = record(b"LongCurrentName", 4, 42);
    let files = Files::new(&[existing.clone()]);
    let mut store = files.open();
    store.prepare_new_name(b"New").unwrap();
    assert_eq!(store.selected(), 1);
    let mut mission = existing.mission();
    mission.mission_selection = 22;
    mission.elapsed_seconds = 0;
    store
        .serialize_working(&mission, &existing.player())
        .unwrap();
    let end = 4 + STATE_BYTES;
    assert_eq!(
        &store.working_record().unwrap().payload()[end..],
        &existing.payload()[end..]
    );
    assert_eq!(files.disk().len(), 1);
    store.save_working().unwrap();
    let saved = files.disk();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].payload(), existing.payload());
    assert_eq!(saved[1].name(), b"New");
    assert_eq!(saved[1].mission(), mission);
}

#[test]
fn save_replaces_selected_record_and_preserves_other_profiles() {
    let originals = [record(b"One", 2, 60), record(b"Two", 7, 120)];
    let files = Files::new(&originals);
    let mut store = files.open();
    let loaded = store.load(0).unwrap().clone();
    store
        .restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut MissionSaveState::new_game(),
            state: &mut SessionState::default(),
            arena: &mut live_arena(),
            classification: &live_classification(),
        })
        .unwrap();
    let mut mission = loaded.mission();
    mission.elapsed_seconds = 3601;
    store.serialize_working(&mission, &loaded.player()).unwrap();
    store.save_working().unwrap();
    assert_eq!(files.disk()[0].mission().elapsed_seconds, 3601);
    assert_eq!(files.disk()[1].payload(), originals[1].payload());
    // 0044A110 invalidates only selected row metadata. Ordinary status/count
    // queries leave the just-saved working record alone; its invalid row query
    // causes the native directory scan, which ends at the last profile.
    assert_eq!(store.query(false).unwrap().count, 2);
    assert_eq!(store.working_record().unwrap().name(), b"One");
    assert_eq!(store.slot_status(1).unwrap(), ProfileStatus::Ready);
    assert_eq!(store.working_record().unwrap().name(), b"One");
    assert_eq!(store.slot_status(0).unwrap(), ProfileStatus::Ready);
    assert_eq!(store.working_record().unwrap().name(), b"Two");
    assert_eq!(store.entry(0).unwrap().unwrap().elapsed_seconds, 3601);
    assert!(store.write_attempted());
}

#[test]
fn copy_appends_renamed_body_then_delete_compacts() {
    let originals = [record(b"One", 2, 60), record(b"Pilot9", 7, 120)];
    let files = Files::new(&originals);
    let mut store = files.open();
    store.copy_profile(1).unwrap();
    let copied = files.disk();
    assert_eq!(copied.len(), 3);
    assert_eq!(copied[2].name(), b"Pilot91");
    assert_eq!(copied[2].mission(), originals[1].mission());
    assert_eq!(copied[2].player(), originals[1].player());
    assert_eq!(store.working_record().unwrap().name(), b"Pilot91");
    assert_eq!(store.selected(), 1);
    store.delete_profile(0).unwrap();
    let deleted = files.disk();
    assert_eq!(deleted.len(), 2);
    assert_eq!(deleted[0].payload(), originals[1].payload());
    assert_eq!(deleted[1].payload(), copied[2].payload());
}

#[test]
fn copy_name_uses_native_digit_and_eighteen_byte_rules_without_utf8_assumptions() {
    for (name, expected) in [
        (&b"Pilot"[..], &b"Pilot1"[..]),
        (b"Pilot0", b"Pilot1"),
        (b"Pilot8", b"Pilot9"),
        (b"Pilot9", b"Pilot91"),
        (b"123456789012345678", b"123456789012345679"),
        (b"1234567890123456789", b"123456789012345671"),
        (&[0xff, b'7'], &[0xff, b'8']),
    ] {
        assert_eq!(copy_name(name), expected);
    }
}

#[test]
fn full_directory_and_invalid_slots_do_not_write() {
    let files = Files::new(&vec![record(b"Full", 3, 0); PROFILE_LIMIT]);
    let mut store = files.open();
    let before = fs::read(&files.save).unwrap();
    assert!(matches!(
        store.copy_profile(0),
        Err(ProfileStoreError::Full)
    ));
    assert_eq!(store.current_name(), None);
    assert!(matches!(
        store.prepare_new_name(b"New"),
        Err(ProfileStoreError::Full)
    ));
    // 0044A0E0's name copy precedes its directory/capacity error.
    assert_eq!(store.current_name(), Some(b"New".as_slice()));
    assert!(matches!(
        store.delete_profile(14),
        Err(ProfileStoreError::Slot { .. })
    ));
    assert!(matches!(
        store.load(14),
        Err(ProfileStoreError::Slot { .. })
    ));
    assert_eq!(store.current_name(), Some(b"New".as_slice()));
    assert!(!store.write_attempted());
    assert_eq!(fs::read(&files.save).unwrap(), before);
}

#[test]
fn external_change_and_stale_staging_file_preserve_previous_bytes() {
    let files = Files::new(&[record(b"One", 2, 60)]);
    let mut store = files.open();
    store.load(0).unwrap();
    fs::write(&files.save, []).unwrap();
    assert!(matches!(
        store.save_working(),
        Err(ProfileStoreError::Changed(_))
    ));
    assert!(!store.write_attempted());
    assert!(fs::read(&files.save).unwrap().is_empty());
    let files = Files::new(&[record(b"One", 2, 60)]);
    let mut store = files.open();
    store.load(0).unwrap();
    let before = fs::read(&files.save).unwrap();
    let mut filename = files.save.as_os_str().to_os_string();
    filename.push(".tmp");
    let staging = PathBuf::from(filename);
    fs::write(&staging, b"unrelated pending write").unwrap();
    assert!(matches!(
        store.save_working(),
        Err(ProfileStoreError::Io { .. })
    ));
    assert!(!store.write_attempted());
    assert_eq!(fs::read(&files.save).unwrap(), before);
    assert_eq!(fs::read(&staging).unwrap(), b"unrelated pending write");
}

fn live_classification() -> ProfileNameClassification {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = fs::read(root.join("Infest.exe")).unwrap();
    ProfileNameClassification::native_startup(
        inf_formats::FrontendExecutable::parse(&bytes).unwrap(),
    )
    .unwrap()
}

fn live_arena() -> ObjectArena {
    ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: crate::object_runtime::ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap()
}

#[test]
fn live_serialize_uses_emitted_cursor_current_elapsed_and_preserves_absent_player_suffix() {
    let files = Files::new(&[record(b"Previous", 3, 17)]);
    let mut store = files.open();
    store.prepare_new_name(b"A%%B").unwrap();
    let before = store.working_record().unwrap().clone();
    let directory = store.directory.clone();
    let records = store.records.clone();
    let source_bytes = store.source_bytes.clone();
    let selected = store.selected();
    let classification = live_classification();
    let class_before = classification.high_bytes;
    let arena = live_arena();
    let mut mission = MissionSaveState::new_game();
    mission.mission_selection = 18;
    mission.elapsed_seconds = 999; // deliberately stale world projection
    mission.owner_b0[37] = 0x8123_4567;
    mission.owner_240[42] = 0xfedc_ba98;
    mission.owner_3c[24] = 0x1122_3344;
    mission.owner_a4 = 0x8001;
    let source_mission = mission.clone();
    store
        .serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 123,
            arena: &arena,
            classification: &classification,
        })
        .unwrap();
    let working = store.working_record().unwrap();
    assert_eq!(working.name(), b"A%B");
    let mut expected_mission = mission.clone();
    expected_mission.elapsed_seconds = 123;
    assert_eq!(working.mission(), expected_mission);
    assert_eq!(&working.payload()[914..], &before.payload()[914..]);
    assert_eq!(store.current_name(), Some(b"A%%B".as_slice()));
    assert_eq!(mission, source_mission);
    assert_eq!(classification.high_bytes, class_before);
    assert!(arena.first_player().is_none());
    assert_eq!(store.directory, directory);
    assert_eq!(store.records, records);
    assert_eq!(store.source_bytes, source_bytes);
    assert_eq!(store.selected(), selected);
    assert!(!store.write_attempted());
    assert_eq!(
        fs::read(&files.save).unwrap(),
        fs::read(&files.save).unwrap()
    );
}

#[test]
fn live_serialize_selects_actual_first_player_and_retains_native_name31_mask() {
    for name_len in [30, 31] {
        let files = Files::new(&[record(b"Old", 3, 10)]);
        let mut store = files.open();
        let name = vec![b'x'; name_len];
        store.prepare_new_name(&name).unwrap();
        let before = store.working_record().unwrap().clone();
        let mut arena = live_arena();
        let first = arena.add_world_player([0x8000_0001; 6]);
        let second = arena.add_world_player([0xffff_ffff; 6]);
        let mut first_state = PlayerSaveState::new_player([0x8000_0001; 6]);
        first_state.owner_118 = 0xf123_4567;
        first_state.owner_11c = 0x8001;
        first_state.owner_11e = 0xfffe;
        first_state.owner_120 = [1, 2, 0x8000_0000, u32::MAX];
        first_state.owner_13c = [0xff00_0080, 0x807f_0100];
        first_state.owner_130 = [0x1234_5678, 0x8123_4567, 0xfedc_ba98];
        arena
            .restore_player_save_prefix(first, &first_state)
            .unwrap();
        let mut second_state = PlayerSaveState::new_player([0xffff_ffff; 6]);
        second_state.owner_118 = 0x1122_3344;
        second_state.owner_13c = [0x0102_0304, 0x0506_0708];
        arena
            .restore_player_save_prefix(second, &second_state)
            .unwrap();
        let before_players = arena.world_players().to_vec();
        let mission = MissionSaveState::new_game();
        let classification = live_classification();
        store
            .serialize_current_working(WorkingProfileSerializeRequest {
                mission: &mission,
                elapsed_seconds: 7,
                arena: &arena,
                classification: &classification,
            })
            .unwrap();
        let mut saved = store.working_record().unwrap().player();
        assert_eq!(saved.owner_118, first_state.owner_118);
        assert_ne!(saved.owner_118, second_state.owner_118);
        if name_len == 31 {
            assert_eq!(
                &store.working_record().unwrap().payload()[1092..],
                &before.payload()[1092..]
            );
            // Compare every prefix field independently of the retained old mask.
            saved.owner_a0 = first_state.owner_a0;
        } else {
            assert_eq!(
                store.working_record().unwrap().payload()[1115],
                before.payload()[1115]
            );
        }
        assert_eq!(saved, first_state);
        assert_eq!(arena.world_players(), before_players);
        assert_eq!(store.current_name(), Some(name.as_slice()));
        assert!(!store.write_attempted());
        assert_eq!(
            fs::read(&files.save).unwrap(),
            fs::read(&files.save).unwrap()
        );
    }
}

#[test]
fn live_serialize_distinguishes_unwritten_name_and_contains_unadmitted_formatting() {
    let files = Files::new(&[record(b"Known", 3, 10)]);
    let mut store = files.open();
    let arena = live_arena();
    let mission = MissionSaveState::new_game();
    let mut classification = live_classification();
    assert!(matches!(
        store.serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 1,
            arena: &arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Profile(
            ProfileStoreError::NoWorkingRecord
        ))
    ));
    store.query(false).unwrap();
    let before = store.working_record().unwrap().clone();
    assert!(matches!(
        store.serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 1,
            arena: &arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Format(
            ProfileNameFormatError::CurrentNameUnavailable
        ))
    ));
    assert_eq!(store.working_record(), Some(&before));
    store.prepare_new_name(b"A%s").unwrap();
    assert!(matches!(
        store.serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 1,
            arena: &arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Format(
            ProfileNameFormatError::FormatBoundary { .. }
        ))
    ));
    assert_eq!(store.working_record(), Some(&before));
    store.prepare_new_name(b"AB").unwrap();
    classification.high_bytes.as_mut().unwrap()[b'B' as usize] = None;
    assert!(matches!(
        store.serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 1,
            arena: &arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Format(
            ProfileNameFormatError::MissingClassificationByte { byte: b'B', .. }
        ))
    ));
    assert_eq!(store.working_record(), Some(&before));
    // Initialized empty is distinct from unknown; the CRT completes it, then
    // the portable record domain rejects the directory-empty output.
    store.name = Some(Vec::new());
    classification.high_bytes = None;
    assert!(matches!(
        store.serialize_current_working(WorkingProfileSerializeRequest {
            mission: &mission,
            elapsed_seconds: 1,
            arena: &arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Record(SaveError::EmptyName))
    ));
    assert_eq!(store.current_name(), Some(b"".as_slice()));
    assert_eq!(store.working_record(), Some(&before));
    assert!(!store.write_attempted());
}

#[test]
fn live_restore_reads_original_percent_source_at_emitted_cursor_without_a_player() {
    let original = record(b"A%%B", 0x1122_3344, 0x8877_6655);
    let files = Files::new(&[original.clone()]);
    let mut store = files.open();
    store.load(0).unwrap();
    assert_eq!(store.current_name(), None);
    let directory = store.directory.clone();
    let records = store.records.clone();
    let source_bytes = store.source_bytes.clone();
    let control = store.control_token();
    let classification = live_classification();
    let class_before = classification.high_bytes;
    let mut arena = live_arena();
    let mut mission = MissionSaveState::new_game();
    mission.elapsed_seconds = 321;
    let mut state = SessionState::default();
    state.restore_mission_elapsed_seconds(987);
    let mut expected_state = state.clone();
    let name = store
        .restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut mission,
            state: &mut state,
            arena: &mut arena,
            classification: &classification,
        })
        .unwrap();
    assert_eq!(name, b"A%B");
    assert_eq!(store.current_name(), Some(b"A%B".as_slice()));
    assert_eq!(original.mission().mission_selection, 0x1122_3344);
    // Original offset4 is the raw NUL, followed by three selection bytes.
    assert_eq!(mission.mission_selection, 0x2233_4400);
    assert_eq!(mission.elapsed_seconds, 0x7766_5511);
    assert_eq!(mission.owner_b0[0], 0xa7a7_a788);
    assert_eq!(&mission.owner_b0[1..], &[0xa7a7_a7a7; 99]);
    assert_eq!(mission.owner_240, [0xa7a7_a7a7; 100]);
    assert_eq!(mission.owner_3c, [0xa7a7_a7a7; 25]);
    assert_eq!(mission.owner_a4, 0xa7a7);
    expected_state.restore_mission_elapsed_seconds(0x7766_5511);
    assert_eq!(state, expected_state);
    assert!(arena.first_player().is_none());
    assert_eq!(store.working_record(), Some(&original));
    assert_eq!(store.directory, directory);
    assert_eq!(store.records, records);
    assert_eq!(store.source_bytes, source_bytes);
    assert_eq!(store.selected(), 0);
    assert_eq!(store.control_token(), control);
    assert!(!store.write_attempted());
    assert_eq!(classification.high_bytes, class_before);
    assert_eq!(
        fs::read(&files.save).unwrap(),
        fs::read(&files.save).unwrap()
    );
}

#[test]
fn live_restore_name31_restores_full_mask_only_to_the_actual_first_player() {
    let name = [b'x'; 31];
    let mut saved_mission = MissionSaveState::new_game();
    saved_mission.mission_selection = 18;
    saved_mission.elapsed_seconds = 73;
    saved_mission.owner_b0[0] = 0x8123_4567;
    saved_mission.owner_240[99] = 0xf123_4567;
    saved_mission.owner_3c[24] = 0xffff_fffe;
    saved_mission.owner_a4 = 0x8001;
    let mut saved_player = PlayerSaveState::new_player([0x8000_0001; 6]);
    saved_player.owner_100 = [0xffff_fffd; 6];
    saved_player.owner_118 = 0xf123_4567;
    saved_player.owner_11c = 0x8001;
    saved_player.owner_11e = 0xfffe;
    saved_player.owner_120 = [1, 2, 0x8000_0000, u32::MAX];
    saved_player.owner_148 = 0x8000_0001;
    saved_player.owner_150 = std::array::from_fn(|index| (index as u32).wrapping_sub(8));
    saved_player.owner_13c = [0xff00_0080, 0x807f_0100];
    saved_player.owner_190 = [0x8001, 0xffff, 0x7ffe];
    saved_player.owner_130 = [0x1234_5678, 0x8123_4567, 0xfedc_ba98];
    let original = SaveRecord::from_state(&name, &saved_mission, &saved_player, &[]).unwrap();
    let files = Files::new(&[original.clone()]);
    let mut store = files.open();
    store.load(0).unwrap();
    let mut arena = live_arena();
    let first = arena.add_world_player([0x0102_0304; 6]);
    let second = arena.add_world_player([0xffff_ffff; 6]);
    let first_before = arena.world_player(first).unwrap().clone();
    let second_before = arena.world_player(second).unwrap().clone();
    let mut mission = MissionSaveState::new_game();
    let mut state = SessionState::default();
    state.restore_mission_elapsed_seconds(999);
    let classification = live_classification();
    assert_eq!(
        store
            .restore_current_working(WorkingProfileRestoreRequest {
                mission: &mut mission,
                state: &mut state,
                arena: &mut arena,
                classification: &classification,
            })
            .unwrap(),
        name
    );
    assert_eq!(mission, saved_mission);
    assert_eq!(state.gameplay_time().seconds, 73);
    assert_eq!(arena.export_player_save_state(first).unwrap(), saved_player);
    assert_eq!(arena.world_player(second).unwrap(), &second_before);
    let actual = arena.world_player(first).unwrap();
    assert_eq!(actual.id, first_before.id);
    assert_eq!(actual.handle_d4, first_before.handle_d4);
    assert_eq!(actual.handle_d8, first_before.handle_d8);
    assert_eq!(actual.byte_ed, first_before.byte_ed);
    assert_eq!(actual.dword_f0, first_before.dword_f0);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.world_players().len(), 2);
    assert_eq!(store.working_record(), Some(&original));
    assert!(!store.write_attempted());
    assert_eq!(
        fs::read(&files.save).unwrap(),
        fs::read(&files.save).unwrap()
    );
}

#[test]
fn live_restore_admission_failure_keeps_current_name_and_actual_destinations() {
    let files = Files::new(&[record(b"A%s", 2, 19)]);
    let mut store = files.open();
    let mut mission = MissionSaveState::new_game();
    mission.elapsed_seconds = 321;
    let original_mission = mission.clone();
    let mut state = SessionState::default();
    state.restore_mission_elapsed_seconds(987);
    let original_state = state.clone();
    let mut arena = live_arena();
    arena.add_world_player([0x8000_0001; 6]);
    let original_players = arena.world_players().to_vec();
    let mut classification = live_classification();
    assert!(matches!(
        store.restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut mission,
            state: &mut state,
            arena: &mut arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Profile(
            ProfileStoreError::NoWorkingRecord
        ))
    ));
    assert_eq!(store.current_name(), None);
    store.load(0).unwrap();
    store.name = Some(b"Prior".to_vec());
    let original = store.working_record().unwrap().clone();
    assert!(matches!(
        store.restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut mission,
            state: &mut state,
            arena: &mut arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Format(
            ProfileNameFormatError::FormatBoundary { .. }
        ))
    ));
    classification.high_bytes = None;
    assert!(matches!(
        store.restore_current_working(WorkingProfileRestoreRequest {
            mission: &mut mission,
            state: &mut state,
            arena: &mut arena,
            classification: &classification,
        }),
        Err(WorkingProfileStateError::Format(
            ProfileNameFormatError::MissingClassificationBinding { .. }
        ))
    ));
    assert_eq!(store.current_name(), Some(b"Prior".as_slice()));
    assert_eq!(store.working_record(), Some(&original));
    assert_eq!(mission, original_mission);
    assert_eq!(state, original_state);
    assert_eq!(arena.world_players(), original_players);
    assert!(!store.write_attempted());
    assert_eq!(
        fs::read(&files.save).unwrap(),
        fs::read(&files.save).unwrap()
    );
}

#[test]
fn live_mission_elapsed_alias_changes_only_after_its_reached_field() {
    for (start, completed_elapsed) in [(1112u32, false), (1108u32, true)] {
        let mut payload = [0x5a; SAVE_PAYLOAD_BYTES];
        let start_index = start as usize;
        payload[start_index..start_index + 4].copy_from_slice(&0x8123_4567u32.to_le_bytes());
        if completed_elapsed {
            payload[start_index + 4..start_index + 8].copy_from_slice(&73u32.to_le_bytes());
        }
        let mut cursor = start;
        let mut mission = MissionSaveState::new_game();
        mission.elapsed_seconds = 321; // stale projection must not overwrite987
        let mut expected_mission = mission.clone();
        let mut state = SessionState::default();
        state.restore_mission_elapsed_seconds(987);
        let mut expected_state = state.clone();
        assert_eq!(
            restore_live_mission_state(&payload, &mut cursor, &mut mission, &mut state),
            Err(SaveError::StateReadRange {
                cursor: 1116,
                bytes: if completed_elapsed { 400 } else { 4 },
            })
        );
        expected_mission.mission_selection = 0x8123_4567;
        if completed_elapsed {
            expected_mission.elapsed_seconds = 73;
            expected_state.restore_mission_elapsed_seconds(73);
        }
        assert_eq!(cursor, 1116);
        assert_eq!(mission, expected_mission);
        assert_eq!(state, expected_state);
    }
}

#[test]
fn finished_campaigns_count_wherever_they_sit_in_the_list() {
    // A retail list begins like this: a finished Jameson, an
    // unfinished game, a finished BOBSTER, then unfinished games.
    let files = Files::new(&[
        record(b"Jameson", 1, 14090),
        record(b"Jameson", 11, 5033),
        record(b"BOBSTER", 1, 2634),
        record(b"Jameson", 6, 2789),
    ]);
    let mut store = files.open();
    assert_eq!(store.completed_campaigns(), 2);
    // Copies and deletions keep the count current.
    store.copy_profile(2).unwrap();
    assert_eq!(store.completed_campaigns(), 3);
    store.delete_profile(0).unwrap();
    store.delete_profile(1).unwrap();
    assert_eq!(files.disk()[2].name(), b"BOBSTER1");
    assert_eq!(store.completed_campaigns(), 1);
    let unfinished = Files::new(&[record(b"Jameson", 22, 0), record(b"Pilot", 2, 60)]);
    assert_eq!(unfinished.open().completed_campaigns(), 0);
}

/// With Pelagia on, a game saved in world 1 without the campaign's last
/// world completed loads (into Pelagia) and is no finished campaign; a
/// finished campaign, world 24 completed, is still refused. Off, both are
/// refused as retail refuses them.
#[test]
fn pelagia_games_load_where_finished_campaigns_do_not() {
    let mut pelagia = record(b"Pelagia", 1, 0);
    let mut payload = *pelagia.payload();
    let world_24 = b"Pelagia".len() + 1 + 8 + 400 + 400 + 24 * 4;
    payload[world_24..world_24 + 4].copy_from_slice(&0u32.to_le_bytes());
    pelagia = SaveRecord::from_payload(payload).unwrap();
    let files = Files::new(&[record(b"Jameson", 1, 14090), pelagia]);
    let mut store = files.open();
    store.query(false).unwrap();
    assert!(store.entry(0).unwrap().unwrap().load_is_rejected());
    assert!(store.entry(1).unwrap().unwrap().load_is_rejected());
    assert_eq!(store.completed_campaigns(), 2);
    store.set_pelagia_loads(true);
    store.query(false).unwrap();
    let finished = store.entry(0).unwrap().unwrap();
    assert!(finished.load_is_rejected() && !finished.pelagia);
    let game = store.entry(1).unwrap().unwrap();
    assert!(!game.load_is_rejected() && game.pelagia);
    assert_eq!(store.completed_campaigns(), 1);
}
