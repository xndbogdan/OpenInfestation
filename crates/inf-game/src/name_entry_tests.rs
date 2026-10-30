//! Focused checks for 0045AB50/0045AC00/0045AC10/0045AC20/0045AC30/0045AC50/
//! 0045AC70/0045ACD0, the 0045AEB0 cheat/delay branch, the 0045A8F0 key
//! block, timer/RNG tick and forced completions, and 0045AD30 ordering.

use super::*;
use crate::profile_store::PROFILE_LIMIT;
use inf_formats::SAVE_PAYLOAD_BYTES;
use inf_formats::{encode_save_records, MissionSaveState, PlayerSaveState, SaveRecord};
use std::collections::VecDeque;

fn name_state(name: &[u8]) -> NameEntryState {
    NameEntryState::new(
        9,
        LoadedIdentity {
            name: name.to_vec(),
            initials: b"0up".to_vec(),
        },
    )
    .unwrap()
}

fn initials_state(initials: &[u8]) -> NameEntryState {
    NameEntryState::new(
        11,
        LoadedIdentity {
            name: b"Net Pilot".to_vec(),
            initials: initials.to_vec(),
        },
    )
    .unwrap()
}

fn tick(phase: u32, delta_micros: i32) -> NameEntryTick {
    NameEntryTick {
        delta_micros,
        elapsed_micros: delta_micros,
        attract: false,
        phase,
    }
}

#[test]
fn grid_has_three_alpha_symbol_rows_with_delete_and_confirm_spaces() {
    assert_eq!(NAME_GRID.len(), GRID_CELLS);
    assert_eq!(&NAME_GRID[0..26], b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    assert_eq!(&NAME_GRID[26..52], b"abcdefghijklmnopqrstuvwxyz");
    assert_eq!(&NAME_GRID[52..62], b"1234567890");
    assert_eq!(&NAME_GRID[62..64], b"  ");
    assert_eq!(&NAME_GRID[64..69], b"-.,:.");
    assert_eq!(&NAME_GRID[69..78], b"         ");
    assert_eq!(NAME_GRID[DELETE_CELL as usize], b' ');
    assert_eq!(NAME_GRID[CONFIRM_CELL as usize], b' ');
}

#[test]
fn constructor_selects_confirm_only_for_nonempty_names() {
    let empty = name_state(b"");
    assert_eq!(empty.name(), b"");
    assert_eq!(empty.selection(), 0);
    let named = name_state(b"Pilot\0extra");
    assert_eq!(named.name(), b"Pilot");
    assert_eq!(named.selection(), CONFIRM_CELL);
    let long = name_state(b"ABCDEFGHIJKLMNOPQRSTUV");
    assert_eq!(long.name(), b"ABCDEFGHIJKLMNOPQR");
    assert_eq!(long.limit(), NAME_LIMIT);
    assert_eq!(long.selection(), CONFIRM_CELL);
}

#[test]
fn constructor_edits_initials_only_in_phase_11_and_caps_loaded_buffers() {
    let identity = LoadedIdentity {
        name: b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".to_vec(),
        initials: b"WXYZQ".to_vec(),
    };
    // The loaders store NUL at cap-1: 30 name bytes and 4 initials bytes.
    for phase in [9, 10] {
        let state = NameEntryState::new(phase, identity.clone()).unwrap();
        assert_eq!(state.loaded().name, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123");
        assert_eq!(state.loaded().initials, b"WXYZ");
        assert_eq!(state.name(), b"ABCDEFGHIJKLMNOPQR");
        assert_eq!(state.limit(), NAME_LIMIT);
    }
    let initials = NameEntryState::new(11, identity.clone()).unwrap();
    assert_eq!(initials.name(), b"WXYZ");
    assert_eq!(initials.limit(), INITIALS_LIMIT);
    assert_eq!(initials.selection(), CONFIRM_CELL);
    for phase in [2, 8, 12] {
        assert!(matches!(
            NameEntryState::new(phase, identity.clone()),
            Err(NameEntryError::Phase(p)) if p == phase
        ));
    }
}

#[test]
fn vertical_moves_add_signed_offsets_modulo_78() {
    let mut state = name_state(b"X");
    state.set_selection(0).unwrap();
    state.apply_move(NameEntryMove::Down);
    assert_eq!(state.selection(), 26);
    state.apply_move(NameEntryMove::Down);
    assert_eq!(state.selection(), 52);
    state.apply_move(NameEntryMove::Down);
    assert_eq!(state.selection(), 0);
    state.apply_move(NameEntryMove::Up);
    assert_eq!(state.selection(), 52);
    state.set_selection(77).unwrap();
    state.apply_move(NameEntryMove::Down);
    assert_eq!(state.selection(), 25);
    state.apply_move(NameEntryMove::Up);
    assert_eq!(state.selection(), 77);
}

#[test]
fn horizontal_moves_wrap_within_rows_and_step_row_animation() {
    let mut state = name_state(b"X");
    state.set_selection(26).unwrap();
    state.apply_move(NameEntryMove::Left);
    assert_eq!(state.selection(), 51);
    assert_eq!(state.row_anim(), -2520);
    state.apply_move(NameEntryMove::Right);
    assert_eq!(state.selection(), 26);
    assert_eq!(state.row_anim(), 0);
    state.set_selection(0).unwrap();
    state.apply_move(NameEntryMove::Left);
    assert_eq!(state.selection(), 25);
    state.set_selection(25).unwrap();
    state.apply_move(NameEntryMove::Right);
    assert_eq!(state.selection(), 0);
    state.set_selection(52).unwrap();
    state.apply_move(NameEntryMove::Right);
    assert_eq!(state.selection(), 53);
    state.set_selection(77).unwrap();
    state.apply_move(NameEntryMove::Right);
    assert_eq!(state.selection(), 52);
    state.set_selection(52).unwrap();
    state.apply_move(NameEntryMove::Left);
    assert_eq!(state.selection(), 77);
}

#[test]
fn activate_appends_delete_and_confirm_paths() {
    let mut state = name_state(b"");
    state.set_selection(0).unwrap();
    assert_eq!(state.activate().unwrap(), NameEntryActivate::Appended(b'A'));
    assert_eq!(state.name(), b"A");
    state.set_selection(DELETE_CELL).unwrap();
    assert_eq!(state.activate().unwrap(), NameEntryActivate::Deleted);
    assert_eq!(state.name(), b"");
    assert_eq!(state.activate().unwrap(), NameEntryActivate::DeleteEmpty);
    state.set_selection(CONFIRM_CELL).unwrap();
    assert_eq!(state.activate().unwrap(), NameEntryActivate::Confirm);
    // At capacity the grid byte is not appended.
    let mut full = name_state(b"ABCDEFGHIJKLMNOPQR");
    full.set_selection(1).unwrap();
    assert_eq!(full.activate().unwrap(), NameEntryActivate::AtCapacity);
    assert_eq!(full.name(), b"ABCDEFGHIJKLMNOPQR");
    // Error state requests phase 3 without touching the buffer.
    full.set_error(true);
    assert_eq!(full.activate().unwrap(), NameEntryActivate::ErrorPhase3);
    assert_eq!(full.name(), b"ABCDEFGHIJKLMNOPQR");
}

#[derive(Default)]
struct Harness {
    events: Vec<String>,
    cheats: Vec<u32>,
    persisted: Vec<(IdentityTarget, Vec<u8>, Vec<u8>)>,
    phases: Vec<u32>,
    keys: VecDeque<KeyRead>,
    /// Actions the base-input host dispatches into the live entry.
    held: VecDeque<NameEntryAction>,
    base_inputs: Vec<(i32, i32)>,
    anims: Vec<i32>,
    network: bool,
    state_calls: usize,
    fail_state: bool,
    fail_persist: bool,
    quantum: Option<u32>,
}

impl NameEntryActions for Harness {
    type Error = &'static str;
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error> {
        self.events.push(format!("cheat:{bit}"));
        self.cheats.push(bit);
        Ok(())
    }
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error> {
        self.events.push(format!("phase:{phase}"));
        self.phases.push(phase);
        Ok(())
    }
}

impl NameEntryCompletion for Harness {
    fn effect_quantum(&self) -> Option<u32> {
        self.quantum
    }
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        self.events.push("persist".into());
        if self.fail_persist {
            return Err("settings unavailable");
        }
        let IdentityInitialsWrite::Write(initials) = request.initials else {
            panic!("all name-entry completion phases write initials");
        };
        self.persisted
            .push((request.target, request.name.to_vec(), initials.to_vec()));
        Ok(())
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        self.events.push("offline-state".into());
        self.state_calls += 1;
        if self.fail_state {
            return Err("mission owner unavailable");
        }
        Ok((MissionSaveState::new_game(), PlayerSaveState::default()))
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        self.events.push("network".into());
        Ok(self.network)
    }
}

impl NameEntryServices for Harness {
    fn read_key(&mut self) -> Result<KeyRead, Self::Error> {
        self.events.push("key".into());
        // A nonzero stale byte leaves an idle buffer without effect.
        Ok(self
            .keys
            .pop_front()
            .unwrap_or(KeyRead::Idle { stale_byte: 0x5A }))
    }
    fn tick_base_input(
        &mut self,
        entry: &mut NameEntryState,
        tick: &NameEntryTick,
    ) -> Result<(), Self::Error> {
        self.events.push("base-input".into());
        self.base_inputs
            .push((tick.delta_micros, tick.elapsed_micros));
        while let Some(action) = self.held.pop_front() {
            entry
                .dispatch(action, tick.phase, self)
                .map_err(|_| "dispatch failed")?;
        }
        Ok(())
    }
    fn tick_animation(&mut self, delta_micros: i32) -> Result<(), Self::Error> {
        self.events.push("animation".into());
        self.anims.push(delta_micros);
        Ok(())
    }
}

/// Uniquely named sidecar directory owned by one test; removed on drop.
struct ScratchDir(std::path::PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Store backed by an isolated sidecar; the retail original is scratch data.
fn open_store(records: usize) -> (ScratchDir, ProfileStore) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.tmp/name-entry-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut payload = [0; SAVE_PAYLOAD_BYTES];
    payload[..6].copy_from_slice(b"Pilot\0");
    payload[6..10].copy_from_slice(&2u32.to_le_bytes());
    let record = SaveRecord::from_payload(payload).unwrap();
    let bytes = encode_save_records(&vec![record; records]).unwrap();
    let original = dir.join("original.sav");
    let sidecar = dir.join("portable.sav");
    std::fs::write(&original, &bytes).unwrap();
    std::fs::write(&sidecar, &bytes).unwrap();
    let store = ProfileStore::open(&sidecar).unwrap();
    (ScratchDir(dir), store)
}

#[test]
fn dispatch_routes_held_slots_back_and_the_error_gated_tab() {
    let mut host = Harness::default();
    let mut state = name_state(b"");
    state.set_selection(0).unwrap();
    for action in [
        NameEntryAction::Down,
        NameEntryAction::Right,
        NameEntryAction::Activate,
    ] {
        state.dispatch(action, 9, &mut host).unwrap();
    }
    assert_eq!(state.selection(), 27);
    assert_eq!(state.name(), b"b");
    // Up (+28) moves back a row; Left (+30) wraps within the row.
    state.dispatch(NameEntryAction::Up, 9, &mut host).unwrap();
    assert_eq!(state.selection(), 1);
    state.set_selection(26).unwrap();
    state.dispatch(NameEntryAction::Left, 9, &mut host).unwrap();
    assert_eq!(state.selection(), 51);
    assert_eq!(state.row_anim(), 0);
    state.dispatch(NameEntryAction::Back, 9, &mut host).unwrap();
    assert_eq!(host.phases, [2]);
    assert_eq!(state.name(), b"b");
    // Activate on cell 77 runs 0045AEB0.
    state.set_selection(CONFIRM_CELL).unwrap();
    state
        .dispatch(NameEntryAction::Activate, 9, &mut host)
        .unwrap();
    assert_eq!(state.completion_delay(), 1000);
    // Tab confirms only while the error byte is clear (0045AC00).
    let mut tab = name_state(b"Pilot");
    tab.set_error(true);
    tab.dispatch(NameEntryAction::ConfirmInput, 10, &mut host)
        .unwrap();
    assert_eq!(tab.completion_delay(), 0);
    // With the error byte set, Activate requests phase 3 instead.
    tab.dispatch(NameEntryAction::Activate, 10, &mut host)
        .unwrap();
    assert_eq!(host.phases, [2, 3]);
    tab.set_error(false);
    tab.dispatch(NameEntryAction::ConfirmInput, 10, &mut host)
        .unwrap();
    assert_eq!(tab.completion_delay(), 1);
}

#[test]
fn confirm_arms_delay_by_phase_and_idles_while_pending() {
    for (phase, delay) in [(9, 1000), (10, 1), (11, 1000), (2, 1000)] {
        let mut state = name_state(b"Pilot");
        let mut host = Harness::default();
        assert_eq!(
            state.confirm(phase, &mut host).unwrap(),
            ConfirmOutcome::Armed { delay_ms: delay }
        );
        assert_eq!(state.completion_delay(), delay);
        assert!(host.cheats.is_empty());
        assert_eq!(
            state.confirm(phase, &mut host).unwrap(),
            ConfirmOutcome::Idle
        );
    }
}

#[test]
fn confirm_grants_all_six_cheats_in_native_order() {
    for (text, bit) in NAME_CHEATS {
        // Every cheat fits the 18-byte editable limit, so the constructor
        // retains the full text.
        let mut state = name_state(text.as_bytes());
        assert_eq!(state.name(), text.as_bytes());
        let mut host = Harness::default();
        assert_eq!(
            state.confirm(9, &mut host).unwrap(),
            ConfirmOutcome::Cheated { bit }
        );
        assert_eq!(host.cheats, [bit]);
        assert_eq!(state.name(), b"");
        assert_eq!(state.feedback(), 1000);
        assert_eq!(state.completion_delay(), 0);
    }
    assert_eq!(NAME_CHEATS.map(|(_, bit)| bit), [2, 1, 4, 0x20, 0x40, 0x80]);
}

#[test]
fn confirm_ignores_prefixes_and_empty_names_as_ordinary() {
    let mut host = Harness::default();
    let mut prefix = name_state(b"INVINCE M");
    assert_eq!(
        prefix.confirm(9, &mut host).unwrap(),
        ConfirmOutcome::Armed { delay_ms: 1000 }
    );
    assert!(host.cheats.is_empty());
    let mut empty = name_state(b"");
    assert_eq!(
        empty.confirm(9, &mut host).unwrap(),
        ConfirmOutcome::Armed { delay_ms: 1000 }
    );
}

#[test]
fn key_block_types_single_byte_names_and_blank_names_select_confirm() {
    let mut state = name_state(b"");
    assert_eq!(
        state.key_block(&KeyRead::Named(b"Z".to_vec())),
        KeyBlockOutcome::Typed(b'Z')
    );
    assert_eq!(state.name(), b"Z");
    assert_eq!(state.selection(), CONFIRM_CELL);
    // An unnamed key copies an empty name: cell 77, no byte appended.
    state.set_selection(3).unwrap();
    assert_eq!(
        state.key_block(&KeyRead::Named(Vec::new())),
        KeyBlockOutcome::Blank
    );
    assert_eq!(state.name(), b"Z");
    assert_eq!(state.selection(), CONFIRM_CELL);
    // Native selects cell 77 before the limit check.
    let mut full = name_state(b"ABCDEFGHIJKLMNOPQR");
    full.set_selection(3).unwrap();
    assert_eq!(
        full.key_block(&KeyRead::Named(b"Q".to_vec())),
        KeyBlockOutcome::AtCapacity
    );
    assert_eq!(full.selection(), CONFIRM_CELL);
    assert_eq!(full.name(), b"ABCDEFGHIJKLMNOPQR");
}

#[test]
fn key_block_matches_delete_backspace_and_space_prefixes() {
    let mut state = name_state(b"AB");
    state.set_selection(4).unwrap();
    assert_eq!(
        state.key_block(&KeyRead::Named(b"Space".to_vec())),
        KeyBlockOutcome::Spaced
    );
    assert_eq!(state.name(), b"AB ");
    assert_eq!(
        state.key_block(&KeyRead::Named(b"Delete".to_vec())),
        KeyBlockOutcome::Erased
    );
    assert_eq!(
        state.key_block(&KeyRead::Named(b"Backspace".to_vec())),
        KeyBlockOutcome::Erased
    );
    assert_eq!(state.name(), b"A");
    // Named multi-byte keys never move the selection.
    assert_eq!(state.selection(), 4);
    for name in [&b"LShift"[..], b"Esc", b"Keypad 7", b"Del"] {
        assert_eq!(
            state.key_block(&KeyRead::Named(name.to_vec())),
            KeyBlockOutcome::Ignored
        );
    }
    state.key_block(&KeyRead::Named(b"Delete".to_vec()));
    assert_eq!(
        state.key_block(&KeyRead::Named(b"Delete".to_vec())),
        KeyBlockOutcome::EraseEmpty
    );
    assert_eq!(state.selection(), 4);
    let mut full = name_state(b"ABCDEFGHIJKLMNOPQR");
    assert_eq!(
        full.key_block(&KeyRead::Named(b"Space".to_vec())),
        KeyBlockOutcome::SpaceAtCapacity
    );
}

#[test]
fn idle_key_buffer_tests_its_stale_byte() {
    // The retail recording snapped an idle grid from cell 0 to cell 77 when
    // bits 8..15 of the platform clock sample were zero.
    let mut state = name_state(b"TIME");
    state.set_selection(0).unwrap();
    assert_eq!(
        state.key_block(&KeyRead::Idle { stale_byte: 0xCE }),
        KeyBlockOutcome::Ignored
    );
    assert_eq!(state.selection(), 0);
    assert_eq!(
        state.key_block(&KeyRead::Idle { stale_byte: 0 }),
        KeyBlockOutcome::Blank
    );
    assert_eq!(state.selection(), CONFIRM_CELL);
    assert_eq!(state.name(), b"TIME");
}

#[test]
fn tick_millis_uses_the_native_multiply_shift_idiom() {
    assert_eq!(tick_millis(0), 0);
    assert_eq!(tick_millis(1000), 1);
    assert_eq!(tick_millis(8000), 8);
    assert_eq!(tick_millis(16000), 16);
    assert_eq!(tick_millis(125000), 125);
}

#[test]
fn tick_counts_down_delay_and_fires_completion_on_equality() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    let mut host = Harness::default();
    state.confirm(9, &mut host).unwrap();
    // Strictly greater waits: 1000 ms against a 1 ms tick.
    let completions = state
        .tick(tick(9, 1000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    assert_eq!(state.completion_delay(), 999);
    assert!(completions.is_empty());
    assert!(host.phases.is_empty());
    // Phase-10 arms a 1 ms delay; a 1 ms tick meets it exactly and completes.
    let mut quick = name_state(b"Al");
    quick.confirm(10, &mut host).unwrap();
    assert_eq!(quick.completion_delay(), 1);
    let completions = quick
        .tick(tick(10, 1000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    assert!(matches!(completions.as_slice(), [NameCompletion::Phase2]));
    // Completion parks the delay at 1, as the original does before its caller
    // leaves the screen.
    assert_eq!(quick.completion_delay(), 1);
    assert_eq!(host.phases.last(), Some(&2));
}

#[test]
fn tick_reads_keys_only_while_no_completion_delay_is_pending() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    let mut host = Harness::default();
    state.confirm(9, &mut host).unwrap();
    state.set_selection(0).unwrap();
    host.keys.push_back(KeyRead::Named(b"Q".to_vec()));
    state
        .tick(tick(9, 1000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    // 00445840 and its latch are not consulted under the +EC gate.
    assert!(!host.events.contains(&"key".to_string()));
    assert_eq!(host.keys.len(), 1);
    assert_eq!(state.name(), b"Pilot");
    assert_eq!(state.selection(), 0);
}

#[test]
fn tick_error_suppresses_completion_but_still_counts_down() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    state.set_error(true);
    let mut host = Harness::default();
    state.confirm(9, &mut host).unwrap();
    let completions = state
        .tick(tick(9, 16000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    assert!(completions.is_empty());
    assert_eq!(state.completion_delay(), 984);
    // Expiry parks the delay at 1 without calling 0045AD30.
    let completions = state
        .tick(
            tick(9, 984_000),
            &mut 0,
            &mut [0; 16],
            &mut store,
            &mut host,
        )
        .unwrap();
    assert!(completions.is_empty());
    assert_eq!(state.completion_delay(), 1);
    assert!(host.phases.is_empty());
    assert!(!host.events.contains(&"persist".to_string()));
}

#[test]
fn tick_counts_feedback_down_to_zero() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"BUGGY RACING");
    let mut host = Harness::default();
    state.confirm(9, &mut host).unwrap();
    assert_eq!(state.feedback(), 1000);
    state
        .tick(tick(9, 16000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    assert_eq!(state.feedback(), 984);
    state
        .tick(
            tick(9, 1_000_000),
            &mut 0,
            &mut [0; 16],
            &mut store,
            &mut host,
        )
        .unwrap();
    assert_eq!(state.feedback(), 0);
}

#[test]
fn tick_consumes_sixteen_rng_draws_and_applies_effect_rules() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    let mut effects = [0u8; 16];
    effects[0] = 100;
    let mut host = Harness::default();
    let mut rng = 0u32;
    state
        .tick(
            tick(9, 16000),
            &mut rng,
            &mut effects,
            &mut store,
            &mut host,
        )
        .unwrap();
    // Sixteen shared draws per tick: 0 -> 0x13D573 -> 19 -> ... Hand-check the
    // first draw only (19 % 32 != 0, 100 > limit 1, 100 - 100/32 = 97); the
    // loop below pins the exact 16-step advance.
    assert_eq!(effects[0], 97);
    let mut expected = 0u32;
    for _ in 0..16 {
        name_entry_rng_next(&mut expected);
    }
    assert_eq!(rng, expected);
    // A zero draw (output % 32 == 0) stores 0x7F, then decays in the same
    // iteration: 127 - 127*1/32 = 124 at a 16 ms tick.
    let mut seed = 0u32;
    loop {
        let next = seed.wrapping_mul(0x13D56D).wrapping_add(0x13D573);
        if ((next >> 16) & 0xFFFF) % 32 == 0 {
            break;
        }
        seed = seed.wrapping_add(1);
        assert!(seed < 1_000_000, "a zero draw must occur quickly");
    }
    // The seed search pins the first draw of the tick, which drives index 0.
    let mut zero = name_state(b"Pilot");
    let mut zero_effects = [0u8; 16];
    zero.tick(
        tick(9, 16000),
        &mut seed,
        &mut zero_effects,
        &mut store,
        &mut host,
    )
    .unwrap();
    assert_eq!(zero_effects[0], 124);
}

#[test]
fn tick_orders_key_decay_base_input_and_animation() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"X");
    state.set_selection(26).unwrap();
    state.apply_move(NameEntryMove::Right);
    assert_eq!(state.row_anim(), 2520);
    let mut host = Harness::default();
    // Decay first (2520 -> 2205), then the held Right adds 2520; decaying
    // after dispatch would give (2520 + 2520) * 7 / 8 = 4410 instead.
    host.held.push_back(NameEntryAction::Right);
    state
        .tick(
            NameEntryTick {
                elapsed_micros: 20000,
                ..tick(9, 16000)
            },
            &mut 0,
            &mut [0; 16],
            &mut store,
            &mut host,
        )
        .unwrap();
    assert_eq!(state.row_anim(), 4725);
    assert_eq!(state.selection(), 28);
    assert_eq!(host.base_inputs, [(16000, 20000)]);
    assert_eq!(host.anims, [16000]);
    // 0055F448 is read every tick, before the 0045A93C key block.
    assert_eq!(host.events, ["network", "key", "base-input", "animation"]);
    // A typed key applies before base input, whose slot dispatch then moves
    // the selection away from the forced cell 77 in the same tick.
    host.events.clear();
    host.keys.push_back(KeyRead::Named(b"Q".to_vec()));
    host.held.push_back(NameEntryAction::Down);
    state
        .tick(tick(9, 16000), &mut 0, &mut [0; 16], &mut store, &mut host)
        .unwrap();
    assert_eq!(state.name(), b"XQ");
    assert_eq!(state.selection(), 25);
    assert_eq!(host.events, ["network", "key", "base-input", "animation"]);
}

#[test]
fn tick_forced_completions_fall_through_to_the_rest_of_the_tick() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    let mut host = Harness {
        network: true,
        ..Harness::default()
    };
    let mut rng = 0u32;
    let completions = state
        .tick(
            NameEntryTick {
                attract: true,
                ..tick(9, 16000)
            },
            &mut rng,
            &mut [0; 16],
            &mut store,
            &mut host,
        )
        .unwrap();
    // Attract then network: each set branch runs a full completion, and
    // 0055F448 is read only after the attract completion (0045A90F).
    assert!(matches!(
        completions.as_slice(),
        [NameCompletion::Phase3, NameCompletion::Phase3]
    ));
    assert_eq!(host.phases, [3, 3]);
    let reads: Vec<&str> = host
        .events
        .iter()
        .map(String::as_str)
        .filter(|event| matches!(*event, "persist" | "network"))
        .collect();
    assert_eq!(
        reads,
        ["persist", "network", "network", "persist", "network"]
    );
    // 0045A90F/0045A91F fall through: the key block, base input, animation
    // and the sixteen shared RNG draws all still run in this tick.
    assert!(host.events.ends_with(&[
        "key".to_string(),
        "base-input".to_string(),
        "animation".to_string()
    ]));
    let mut expected = 0u32;
    for _ in 0..16 {
        name_entry_rng_next(&mut expected);
    }
    assert_eq!(rng, expected);
}

#[test]
fn complete_substitutes_jameson_and_serializes_offline() {
    let (_dir, mut store) = open_store(0);
    let mut state = name_state(b"");
    let mut host = Harness::default();
    let completion = state.complete(9, &mut store, &mut host).unwrap();
    assert!(matches!(completion, NameCompletion::Phase3));
    assert_eq!(state.name(), b"Jameson");
    // 004633D0 writes the edited name with the loaded initials.
    assert_eq!(
        host.persisted,
        [(
            IdentityTarget::PlayerName,
            b"Jameson".to_vec(),
            b"0up".to_vec()
        )]
    );
    assert_eq!(store.selected(), 0);
    assert_eq!(store.working_record().unwrap().name(), b"Jameson");
    assert_eq!(
        store.working_record().unwrap().mission().mission_selection,
        22
    );
    assert_eq!(host.phases, [3]);
    assert_eq!(host.state_calls, 1);
    // The network flag is read after 0044A0E0 selects the append index.
    assert_eq!(
        host.events,
        ["persist", "network", "offline-state", "phase:3"]
    );
}

#[test]
fn complete_phases_10_and_11_persist_the_net_identity_pair_without_store() {
    for (phase, name, initials) in [
        (10, &b"Pilot"[..], &b"0up"[..]),
        (11, &b"Net Pilot"[..], &b"AB"[..]),
    ] {
        let (_dir, mut store) = open_store(1);
        let before = store.selected();
        let identity = LoadedIdentity {
            name: b"Net Pilot".to_vec(),
            initials: b"0up".to_vec(),
        };
        let mut state = NameEntryState::new(phase, identity).unwrap();
        if phase == 10 {
            state.name = b"Pilot".to_vec();
        } else {
            state.name = b"AB".to_vec();
        }
        let mut host = Harness::default();
        let completion = state.complete(phase, &mut store, &mut host).unwrap();
        assert!(matches!(completion, NameCompletion::Phase2));
        assert_eq!(
            host.persisted,
            [(
                IdentityTarget::NetPlayerName,
                name.to_vec(),
                initials.to_vec()
            )]
        );
        assert_eq!(store.selected(), before);
        assert_eq!(host.phases, [2]);
        assert_eq!(host.state_calls, 0);
    }
    // Blank phase-11 initials are also substituted with Jameson.
    let (_dir, mut store) = open_store(1);
    let mut blank = initials_state(b"");
    let mut host = Harness::default();
    blank.complete(11, &mut store, &mut host).unwrap();
    assert_eq!(host.persisted[0].2, b"Jameson");
}

#[test]
fn complete_network_session_skips_serialization_but_requests_phase_3() {
    let (_dir, mut store) = open_store(1);
    // A fresh name proves the working record below is the opened one, not a
    // freshly serialized one.
    let mut state = name_state(b"Zed");
    let mut host = Harness {
        network: true,
        ..Harness::default()
    };
    let completion = state.complete(9, &mut store, &mut host).unwrap();
    assert!(matches!(completion, NameCompletion::Phase3));
    assert_eq!(store.selected(), 1);
    assert_eq!(store.working_record().unwrap().name(), b"Pilot");
    assert_eq!(host.state_calls, 0);
    assert_eq!(host.phases, [3]);
}

#[test]
fn complete_store_failure_sets_error_without_phase_request() {
    let (_dir, mut store) = open_store(PROFILE_LIMIT);
    let mut state = name_state(b"Pilot");
    let mut host = Harness::default();
    let completion = state.complete(9, &mut store, &mut host).unwrap();
    assert!(matches!(completion, NameCompletion::Failed { .. }));
    assert!(state.error());
    assert!(host.phases.is_empty());
    // A failed 0044A0E0 skips the network read and serialization.
    assert_eq!(host.events, ["persist"]);
}

#[test]
fn complete_host_failure_propagates_instead_of_setting_error() {
    let (_dir, mut store) = open_store(1);
    let mut state = name_state(b"Pilot");
    let mut host = Harness {
        fail_persist: true,
        ..Harness::default()
    };
    let result = state.complete(9, &mut store, &mut host);
    assert!(matches!(
        result,
        Err(NameEntryTickError::Host("settings unavailable"))
    ));
    assert!(!state.error());
}

#[test]
fn retry_after_error_reruns_completion_without_requesting_phase_3() {
    let (_dir, mut store) = open_store(1);
    let mut clean = name_state(b"Pilot");
    let mut host = Harness::default();
    assert!(clean
        .retry_after_error(9, &mut store, &mut host)
        .unwrap()
        .is_none());
    assert!(host.events.is_empty());
    clean.set_error(true);
    let retry = clean.retry_after_error(9, &mut store, &mut host).unwrap();
    // 0045AE52 sees the still-set error byte: persistence and serialization
    // run again, but only a later Activate requests phase 3.
    assert!(matches!(retry, Some(NameCompletion::ErrorPending)));
    assert_eq!(host.events, ["persist", "network", "offline-state"]);
    assert!(host.phases.is_empty());
    assert!(clean.error());
    clean
        .dispatch(NameEntryAction::Activate, 9, &mut host)
        .unwrap();
    assert_eq!(host.phases, [3]);
}

/// Records the host effects the 0045A8F0 oracle stubs, in native order.
#[derive(Default)]
struct NativeTickHost {
    events: Vec<String>,
    key: Option<KeyRead>,
    network: bool,
}

impl NameEntryActions for NativeTickHost {
    type Error = &'static str;
    fn grant_cheat_flag(&mut self, _: u32) -> Result<(), Self::Error> {
        Err("unexpected cheat")
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl NameEntryCompletion for NativeTickHost {
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error> {
        // 0045AD30 persists first; one entry marks one completion.
        assert!(matches!(request.initials, IdentityInitialsWrite::Write(_)));
        self.events.push("complete".into());
        Ok(())
    }
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error> {
        Ok((MissionSaveState::new_game(), PlayerSaveState::default()))
    }
    fn network_session(&mut self) -> Result<bool, Self::Error> {
        Ok(self.network)
    }
}

impl NameEntryServices for NativeTickHost {
    fn read_key(&mut self) -> Result<KeyRead, Self::Error> {
        self.events.push("key".into());
        self.key.take().ok_or("key block read twice")
    }
    fn tick_base_input(
        &mut self,
        _: &mut NameEntryState,
        tick: &NameEntryTick,
    ) -> Result<(), Self::Error> {
        self.events.push(format!(
            "base_input:{}:{}",
            tick.delta_micros, tick.elapsed_micros
        ));
        Ok(())
    }
    fn tick_animation(&mut self, delta: i32) -> Result<(), Self::Error> {
        self.events.push(format!("animation:{delta}"));
        Ok(())
    }
}

#[test]
fn tick_matches_the_original_key_block_decay_rng_and_timers() {
    let receipts: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/name_entry_tick_native.json"
    ))
    .unwrap();
    let receipts = receipts.as_array().unwrap();
    assert_eq!(receipts.len(), 400);
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    let bytes = |value: &serde_json::Value| -> Vec<u8> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect()
    };
    let (_dir, mut store) = open_store(0);
    for receipt in receipts {
        let input = &receipt["input"];
        let limit = int(&input["limit"]) as usize;
        let mut state = NameEntryState::new(
            if limit == INITIALS_LIMIT { 11 } else { 9 },
            LoadedIdentity::default(),
        )
        .unwrap();
        state.name = bytes(&input["name"]);
        state.selection = int(&input["selection"]) as i32;
        state.completion_delay = int(&input["delay"]) as i32;
        state.feedback = int(&input["feedback"]) as i32;
        state.error = input["error"].as_bool().unwrap();
        state.row_anim = int(&input["row_anim"]) as i32;
        let mut effects = [0u8; 16];
        effects.copy_from_slice(&bytes(&input["effect"]));
        let key = if input["key"].is_null() {
            KeyRead::Idle {
                stale_byte: bytes(&input["stale"])[1],
            }
        } else {
            KeyRead::Named(bytes(&input["key"]))
        };
        let mut host = NativeTickHost {
            events: Vec::new(),
            key: Some(key),
            network: input["network"].as_bool().unwrap(),
        };
        let mut rng = int(&input["rng"]) as u32;
        let completions = state
            .tick(
                NameEntryTick {
                    delta_micros: int(&input["delta"]) as i32,
                    elapsed_micros: int(&input["elapsed"]) as i32,
                    attract: input["attract"].as_bool().unwrap(),
                    phase: 9,
                },
                &mut rng,
                &mut effects,
                &mut store,
                &mut host,
            )
            .unwrap();
        let native: Vec<String> = receipt["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| match event["kind"].as_str().unwrap() {
                "base_input" => format!(
                    "base_input:{}:{}",
                    int(&event["delta"]),
                    int(&event["elapsed"])
                ),
                "animation" => format!("animation:{}", int(&event["delta"])),
                kind => kind.to_string(),
            })
            .collect();
        assert_eq!(host.events, native, "{input}");
        assert_eq!(
            completions.len(),
            native.iter().filter(|event| *event == "complete").count()
        );
        assert_eq!(state.name(), bytes(&receipt["name"]).as_slice(), "{input}");
        assert_eq!(
            state.selection(),
            int(&receipt["selection"]) as i32,
            "{input}"
        );
        assert_eq!(
            state.row_anim(),
            int(&receipt["row_anim"]) as i32,
            "{input}"
        );
        assert_eq!(
            state.completion_delay(),
            int(&receipt["delay"]) as i32,
            "{input}"
        );
        assert_eq!(
            state.feedback(),
            int(&receipt["feedback"]) as i32,
            "{input}"
        );
        assert_eq!(
            effects.as_slice(),
            bytes(&receipt["effect"]).as_slice(),
            "{input}"
        );
        assert_eq!(rng, int(&receipt["rng"]) as u32, "{input}");
    }
}

/// At 250 Hz (4 ms ticks) retail's `delta >> 13` decay limit is 0, so a
/// flashed glyph never fades; the port's quantum (16384 µs) decays it as a
/// 61 Hz tick would and steps the row animation and effects once a quantum.
#[test]
fn quantized_name_entry_effects_fade_at_high_frame_rates() {
    for (quantum, fades) in [(None, false), (Some(16384), true)] {
        let (_dir, mut store) = open_store(1);
        let mut state = name_state(b"Pilot");
        let mut host = Harness {
            quantum,
            ..Harness::default()
        };
        let mut effects = [0x7f_u8; 16];
        let mut rng = 0x1234_u32;
        for _ in 0..100 {
            state
                .tick(tick(9, 4000), &mut rng, &mut effects, &mut store, &mut host)
                .unwrap();
        }
        let faded = effects.iter().filter(|&&effect| effect < 0x40).count();
        assert_eq!(faded > 8, fades, "{quantum:?}: {effects:?}");
        if !fades {
            assert!(effects.iter().all(|&effect| effect == 0x7f));
        }
    }
}
