use super::*;
use inf_formats::{encode_save_records, SaveArchive, SaveRecord, SAVE_PAYLOAD_BYTES};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    original: PathBuf,
    sidecar: PathBuf,
    store: ProfileStore,
}
impl Fixture {
    fn new(missions: &[u32]) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../.tmp/profile-menu-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let original = dir.join("retail.sav");
        let sidecar = dir.join("portable.sav");
        let records = missions
            .iter()
            .enumerate()
            .map(|(index, mission)| {
                let name = format!("Pilot{index}");
                let mut payload = [0xa7; SAVE_PAYLOAD_BYTES];
                payload[..name.len()].copy_from_slice(name.as_bytes());
                payload[name.len()] = 0;
                payload[name.len() + 1..name.len() + 5].copy_from_slice(&mission.to_le_bytes());
                SaveRecord::from_payload(payload).unwrap()
            })
            .collect::<Vec<_>>();
        let bytes = encode_save_records(&records).unwrap();
        fs::write(&original, &bytes).unwrap();
        // Repeated test runs can reuse a process id; explicitly initialize only
        // this unique fixture's sidecar, never the installed retail file.
        fs::write(&sidecar, &bytes).unwrap();
        let store = ProfileStore::open(&sidecar).unwrap();
        Self {
            original,
            sidecar,
            store,
        }
    }
}

#[derive(Default)]
struct Services {
    events: Vec<String>,
    fail_phase: bool,
    fail_flag: bool,
}
impl ProfileMenuServices for Services {
    type Error = &'static str;
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.events.push(format!("rotate:{angle}"));
        Ok(())
    }
    fn reset_profile_input(&mut self) -> Result<(), Self::Error> {
        self.events.push("input-reset".into());
        Ok(())
    }
    fn select_camera(&mut self, preset: i32) -> Result<(), Self::Error> {
        self.events.push(format!("camera:{preset}"));
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        self.events.push("transition-flag".into());
        if self.fail_flag {
            Err("transition owner missing")
        } else {
            Ok(())
        }
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.events.push(format!("phase:{phase}"));
        if self.fail_phase {
            Err("phase owner missing")
        } else {
            Ok(())
        }
    }
    fn play_sound(&mut self, slot: i16) -> Result<(), Self::Error> {
        self.events.push(format!("sound:{slot}"));
        Ok(())
    }
    fn begin_controller_fade(&mut self) -> Result<(), Self::Error> {
        self.events.push("fade".into());
        Ok(())
    }
}

fn templates() -> ProfileMenuTemplates {
    let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("installed retail executable");
    ProfileMenuTemplates::parse(&bytes).unwrap()
}
fn menu(fixture: &mut Fixture, services: &mut Services) -> ProfileMenu {
    let mut menu = ProfileMenu::new(templates(), MenuBasis::INITIAL, false);
    assert!(!menu.prepare_draw(&mut fixture.store).unwrap().draw_rows);
    menu.finish_draw();
    menu.tick(16_000, false, &mut fixture.store, services)
        .unwrap();
    menu
}
fn action(
    menu: &mut ProfileMenu,
    f: &mut Fixture,
    s: &mut Services,
    callback: u32,
    command: MenuCommand,
    index: u32,
    owner: bool,
) {
    menu.execute_callback(callback, command, [index, 0], owner, &mut f.store, s)
        .unwrap();
}

#[test]
fn installed_templates_preserve_sources_and_distinguish_terminators_from_rows() {
    let templates = templates();
    assert_eq!(
        templates.rows.map(|r| r.virtual_address),
        [0x536d88, 0x536da8, 0x536dc8, 0x536de8, 0x536e08]
    );
    assert_eq!(
        templates.rows.map(|r| r.draw_callback),
        [0x448e60, 0x449320, 0x449320, 0x448c60, 0x448d10]
    );
    assert_eq!(
        templates.rows.map(|r| r.action_callback),
        [0x4492e0, 0x449300, 0x449400, 0x449400, 0x4493f0]
    );
    assert_eq!(templates.rows.map(|r| r.flags), [13, 5, 5, 5, 5]);
    assert_eq!(templates.regular.records().len(), 2);
    assert_eq!(templates.exceptional.records().len(), 1);
    assert_eq!(templates.regular.records()[1].visible_rows, 7);
    let exception = templates.bind_exceptional();
    assert_eq!(exception.records().len(), 1);
    assert_eq!(exception.records()[0].item_count, 1);
    assert_eq!(
        templates.rows[0].span.end - templates.rows[0].span.start,
        32
    );
    assert!(ProfileMenuTemplates::parse(&[]).is_err());
}

#[test]
fn initial_draw_handshake_warning_timer_and_rebuild_reset_ages_in_native_order() {
    let mut f = Fixture::new(&[2, 3]);
    let mut s = Services::default();
    let mut menu = ProfileMenu::new(templates(), MenuBasis::INITIAL, true);
    assert!(!menu.base.data().records().has_items());
    assert!(menu.state.pending_index.is_none());
    menu.tick(1_999_000, false, &mut f.store, &mut s).unwrap();
    assert_eq!(
        menu.prepare_draw(&mut f.store).unwrap().timer,
        Some(ProfileTimer {
            kind: ProfileTimerKind::InitialWarning,
            millis: 1
        })
    );
    menu.finish_draw();
    menu.tick(1_000, false, &mut f.store, &mut s).unwrap();
    assert!(s.events.is_empty());
    assert!(menu.state.busy());
    menu.finish_draw();
    menu.tick(16_000, false, &mut f.store, &mut s).unwrap();
    assert_eq!(s.events, ["input-reset", "rotate:160"]);
    assert_eq!(menu.base.age_millis(), 0);
    assert!(menu.base.data().records().items.iter().all(|r| r.age == 0));
    assert!(menu.prepare_draw(&mut f.store).unwrap().draw_rows);
}

#[test]
fn rebuild_copies_fifteen_rows_and_retains_process_layout_animation() {
    let mut f = Fixture::new(&[2, 3]);
    let mut s = Services::default();
    let first = menu(&mut f, &mut s);
    let shared = first.templates.clone();
    first.base.data().records_mut().items[0].draw_arguments[0] = 999;
    shared.regular.records_mut()[1].scroll_animation = 1234;
    let mut second = ProfileMenu::new(shared.clone(), MenuBasis::INITIAL, false);
    assert_eq!(shared.regular.records()[1].scroll_animation, 1234);
    second.rebuild_status(3, 2, false);
    let rows = second.base.data().records();
    assert_eq!(rows.items.len(), 15);
    assert_eq!(rows.items[0].draw_arguments[0], 145);
    assert_eq!(rows.items[2].action_arguments[0], 1);
    assert_eq!(rows.items[14].draw_arguments[0], 13);
    assert_eq!(rows.items[14].action_arguments[0], 0);
    assert_eq!(rows.layouts[1].item_count, 14);
    assert_eq!(rows.layouts[1].visible_rows, 9);
    assert_eq!(rows.layouts[1].scroll_animation, 1234);
    assert_eq!(second.base.selection(), 1);
    assert_eq!(second.base.scroll_start(), 0);
    drop(rows);
    assert_eq!(first.base.data().records().items[0].draw_arguments[0], 999);
}

#[test]
fn mode_cycles_skip_full_copy_and_respect_owner_and_edit_lock() {
    let mut f = Fixture::new(&[2; 14]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustNext,
        0,
        false,
    );
    assert_eq!(m.state.mode(), ProfileMode::Delete);
    assert_eq!(m.base.data().records().items[0].draw_arguments[0], 0xcf);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustPrevious,
        0,
        false,
    );
    assert_eq!(m.state.mode(), ProfileMode::Load);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustNext,
        0,
        true,
    );
    assert_eq!(m.state.mode(), ProfileMode::Load);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        true,
    );
    assert!(m.base.navigation_locked());
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustPrevious,
        0,
        true,
    );
    assert!(m.state.choice());
    assert_eq!(m.state.mode(), ProfileMode::Load);
}

#[test]
fn empty_row_uses_count_strict_delay_and_one_suppressed_draw() {
    let mut f = Fixture::new(&[2, 3]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    s.events.clear();
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449400,
        MenuCommand::Activate,
        13,
        false,
    );
    assert_eq!(f.store.selected(), 2);
    assert_eq!(s.events, ["transition-flag", "camera:9"]);
    assert!(!m.prepare_draw(&mut f.store).unwrap().draw_rows);
    assert!(matches!(
        m.tick(1_000_000, false, &mut f.store, &mut s).unwrap(),
        ProfileTickOutcome::Idle
    ));
    assert_eq!(m.state.remaining_millis(), 0);
    assert!(!s.events.iter().any(|e| e == "phase:9"));
    assert!(matches!(
        m.tick(1_000, false, &mut f.store, &mut s).unwrap(),
        ProfileTickOutcome::Completed(ProfileOperation::NewName)
    ));
    assert!(!m.state.busy());
    assert!(!m.prepare_draw(&mut f.store).unwrap().draw_rows);
    assert!(m.prepare_draw(&mut f.store).unwrap().draw_rows);
    assert!(!f.store.write_attempted());
}

#[test]
fn empty_store_token_and_owner_selection_fade_are_explicit_control_writes() {
    let mut f = Fixture::new(&[]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449400,
        MenuCommand::Activate,
        13,
        false,
    );
    assert_eq!(f.store.control_token(), ProfileControlToken::Zero);
    let mut m = menu(&mut f, &mut s);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449400,
        MenuCommand::Activate,
        13,
        true,
    );
    m.tick(1_000, true, &mut f.store, &mut s).unwrap();
    assert_eq!(m.state.pending(), Some(ProfileOperation::SelectAndFade));
    m.tick(1_000, true, &mut f.store, &mut s).unwrap();
    assert_eq!(f.store.selected(), 0);
    assert_eq!(f.store.control_token(), ProfileControlToken::MinusOne);
    assert_eq!(s.events.last().unwrap(), "fade");
    assert!(!m.state.busy());
    assert!(m.prepare_draw(&mut f.store).unwrap().draw_rows);
}

#[test]
fn rejected_load_plays_four_and_phase_failure_is_not_a_caught_store_failure() {
    let mut f = Fixture::new(&[1, 3]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    s.events.clear();
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    assert_eq!(s.events, ["sound:4"]);
    assert_eq!(m.state.pending(), None);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        1,
        false,
    );
    s.fail_phase = true;
    assert!(matches!(
        m.tick(1_001_000, false, &mut f.store, &mut s),
        Err(ProfileMenuError::Host("phase owner missing"))
    ));
    assert_eq!(f.store.working_record().unwrap().name(), b"Pilot1");
    assert_eq!(m.state.pending(), Some(ProfileOperation::Load));
    assert_eq!(m.state.timer(), None);
    assert!(!m.state.busy());
}

#[test]
fn failed_host_before_pending_index_writer_remains_an_explicit_error_on_retry() {
    let mut f = Fixture::new(&[2]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    s.fail_flag = true;
    assert!(matches!(
        m.execute_callback(
            0x449300,
            MenuCommand::Activate,
            [0, 0],
            false,
            &mut f.store,
            &mut s
        ),
        Err(ProfileMenuError::Host("transition owner missing"))
    ));
    assert_eq!(m.state.pending(), Some(ProfileOperation::Load));
    assert!(m.state.pending_index.is_none());
    assert!(matches!(
        m.tick(1_001_000, false, &mut f.store, &mut s),
        Err(ProfileMenuError::MissingPendingIndex(
            ProfileOperation::Load
        ))
    ));
    assert_eq!(m.state.timer(), None);
}

#[test]
fn load_io_failure_is_caught_and_timer_expiry_still_requires_draw() {
    let mut f = Fixture::new(&[2]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    fs::write(&f.sidecar, b"corrupt").unwrap();
    assert!(matches!(
        m.tick(1_001_000, false, &mut f.store, &mut s).unwrap(),
        ProfileTickOutcome::OperationFailed {
            operation: ProfileOperation::Load,
            error: ProfileStoreError::Format { .. }
        }
    ));
    assert_eq!(m.state.timer().unwrap().kind, ProfileTimerKind::LoadFailure);
    assert_eq!(m.state.pending(), None);
    assert!(!m.state.queried);
    m.finish_draw();
    m.tick(2_000_000, false, &mut f.store, &mut s).unwrap();
    assert!(m.state.busy());
    assert_eq!(m.state.timer(), None);
    // Expiry itself cannot rebuild and encounter the corrupt sidecar.
    m.finish_draw();
    assert!(matches!(
        m.tick(1_000, false, &mut f.store, &mut s),
        Err(ProfileMenuError::Store(ProfileStoreError::Format { .. }))
    ));
}

#[test]
fn confirmed_copy_and_delete_mutate_only_sidecar_and_reset_mode() {
    let mut f = Fixture::new(&[2, 3]);
    let original = fs::read(&f.original).unwrap();
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustNext,
        0,
        false,
    );
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    assert!(m.base.navigation_locked());
    assert!(!m.state.choice());
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Auxiliary7,
        0,
        false,
    );
    assert!(!m.base.navigation_locked());
    assert_eq!(m.state.pending(), None);
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Auxiliary6,
        0,
        false,
    );
    m.tick(0, false, &mut f.store, &mut s).unwrap();
    assert_eq!(m.state.pending(), Some(ProfileOperation::Copy));
    assert!(matches!(
        m.tick(1_000, false, &mut f.store, &mut s).unwrap(),
        ProfileTickOutcome::Completed(ProfileOperation::Copy)
    ));
    assert_eq!(f.store.cached_count(), 3);
    assert_eq!(m.state.mode(), ProfileMode::Load);
    assert!(m.state.busy());
    m.finish_draw();
    m.tick(1_000, false, &mut f.store, &mut s).unwrap();
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustPrevious,
        0,
        false,
    );
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::AdjustPrevious,
        0,
        false,
    );
    action(
        &mut m,
        &mut f,
        &mut s,
        0x449300,
        MenuCommand::Activate,
        0,
        false,
    );
    assert!(matches!(
        m.tick(1_000, false, &mut f.store, &mut s).unwrap(),
        ProfileTickOutcome::Completed(ProfileOperation::Delete)
    ));
    let saved = fs::read(&f.sidecar).unwrap();
    let archive = SaveArchive::parse(&saved).unwrap();
    assert_eq!(
        archive
            .records()
            .iter()
            .map(|r| r.name())
            .collect::<Vec<_>>(),
        [b"Pilot1".as_slice(), b"Pilot1".as_slice()]
    );
    assert_eq!(fs::read(&f.original).unwrap(), original);
}

#[test]
fn source_exception_status_matrix_and_format_operation_never_claim_disk_formatting() {
    let mut f = Fixture::new(&[2]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    for (status, owner, label, locked) in [
        (5, false, 157, false),
        (6, false, 150, false),
        (4, true, 153, false),
        (5, true, 147, true),
    ] {
        m.rebuild_status(status, 1, owner);
        assert_eq!(m.base.data().records().items.len(), 1);
        assert_eq!(m.base.data().records().items[0].draw_arguments[0], label);
        assert_eq!(m.base.navigation_locked(), locked);
        assert_eq!(m.base.data().records().layouts.len(), 1);
    }
    let before = fs::read(&f.sidecar).unwrap();
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4493f0,
        MenuCommand::Auxiliary6,
        0,
        true,
    );
    assert!(m.state.busy());
    assert!(m.base.navigation_locked());
    m.finish_draw();
    m.tick(2_000, true, &mut f.store, &mut s).unwrap();
    assert_eq!(m.state.format_state(), 1);
    assert!(!m.base.navigation_locked());
    assert_eq!(f.store.query(false).unwrap().count, 0);
    assert_eq!(fs::read(&f.sidecar).unwrap(), before);
    assert!(!f.store.write_attempted());
    assert_eq!(f.store.query(true).unwrap().count, 1);
}

#[test]
fn signed_callback_indices_and_unknown_callbacks_fail_explicitly() {
    let mut f = Fixture::new(&[2]);
    let mut s = Services::default();
    let mut m = menu(&mut f, &mut s);
    assert!(matches!(
        m.execute_callback(
            0x449300,
            MenuCommand::Activate,
            [u32::MAX, 0],
            false,
            &mut f.store,
            &mut s
        ),
        Err(ProfileMenuError::InvalidSlot(-1))
    ));
    assert!(matches!(
        m.execute_callback(
            0,
            MenuCommand::Activate,
            [0, 0],
            false,
            &mut f.store,
            &mut s
        ),
        Err(ProfileMenuError::UnsupportedCallback(0))
    ));
    // Header's intentional signed -1 never becomes a slot lookup for mode keys.
    action(
        &mut m,
        &mut f,
        &mut s,
        0x4492e0,
        MenuCommand::AdjustNext,
        0,
        false,
    );
    assert_eq!(m.state.mode(), ProfileMode::Copy);
}
