//! Name-entry state of the phase-9/10/11 text-entry screen (vtable 00526028).
//!
//! Owns the 78-cell character grid, selection movement, the held-bit slot
//! and Tab/Retry paths, the 0045AEB0 cheat/completion-delay branch, the
//! 0045A8F0 key block over the 00445840 buffer, the timer/RNG tick, and
//! 0045AD30 completion ordering. Effects are required host traits in a
//! chain: [`NameEntryActions`] (cheat flags, phase requests) for slot
//! dispatch and confirm, [`NameEntryCompletion`] (identity persistence,
//! offline mission/player state, the 0055F448 network flag) for 0045AD30,
//! and [`NameEntryServices`] (key translation, base input, animation) for
//! the tick. The concrete 00459000/00459060 screen owner, its bindings and
//! its callback routing live in [`crate::text_entry_screen`].
//! Name-entry layout and backdrop live in [`crate::name_entry_draw`] and
//! [`crate::name_entry_scene`]; the outer frame and phase-3 world transition
//! remain separate integration boundaries.
//! Grid bytes and cell roles come from the supported retail `Infest.exe`:
//! `.data` 00537C48 holds 78 bytes, selected through the DWORD at 00537BF8.
//! Rows are `ABCDEFGHIJKLMNOPQRSTUVWXYZ`, `abcdefghijklmnopqrstuvwxyz`, and
//! `1234567890  -.,:.` followed by nine spaces. Cells 75 (delete) and 77
//! (confirm) are actions, not appended spaces.

use inf_formats::{MissionSaveState, PlayerSaveState};
use thiserror::Error;

use crate::profile_store::{ProfileStore, ProfileStoreError};

/// Number of selectable grid cells proven by the 00537C48 extent.
pub const GRID_CELLS: usize = 78;
/// Cells per row; movement divides by this with x86 IDIV truncation.
pub const ROW_CELLS: i32 = 26;
/// Grid cell that deletes the last name byte when the buffer is nonempty.
pub const DELETE_CELL: i32 = 75;
/// Grid cell that confirms the current name through 0045AEB0.
pub const CONFIRM_CELL: i32 = 77;
/// Retail phase-9/10 editable-name limit at screen +0x118.
pub const NAME_LIMIT: usize = 18;
/// Retail phase-11 initials limit at screen +0x118.
pub const INITIALS_LIMIT: usize = 4;

/// Authored 78-byte grid at 00537C48 in the supported executable.
pub const NAME_GRID: [u8; GRID_CELLS] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890  -.,:.         ";

const _: () = {
    assert!(NAME_GRID.len() == GRID_CELLS);
    assert!(DELETE_CELL >= 0 && DELETE_CELL < GRID_CELLS as i32);
    assert!(CONFIRM_CELL >= 0 && CONFIRM_CELL < GRID_CELLS as i32);
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameEntryMove {
    /// 0045AC30: `(selection + 52) % 78`.
    Up,
    /// 0045AC50: `(selection + 26) % 78`.
    Down,
    /// 0045AC70: wrap left within the current row, row animation `-2520`.
    Left,
    /// 0045ACD0: wrap right within the current row, row animation `+2520`.
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameEntryActivate {
    /// Error byte +0x14C was set: 0045AB50 requests phase 3 directly.
    ErrorPhase3,
    /// Cell 75 with a nonempty buffer truncated its final byte.
    Deleted,
    /// Cell 75 with an empty buffer: no mutation.
    DeleteEmpty,
    /// Cell 77: the caller must run 0045AEB0 (cheat check or completion delay).
    Confirm,
    /// Any other cell appended its grid byte and NUL-terminated the buffer.
    Appended(u8),
    /// Any other cell while the buffer already holds `limit` bytes: no mutation.
    AtCapacity,
}

/// Blank-confirmation fallback copied from `00537CB8` by 0045AD30.
pub const JAMESON: &[u8] = b"Jameson";

/// 0045AEB0 recognized cheats in native comparison order, with the settings
/// `+0x84` bit each grants through `00463AB0`.
pub const NAME_CHEATS: [(&str, u32); 6] = [
    ("INVINCE ME", 2),
    ("GIVE IT TO ME BABY", 1),
    ("SEMPER UBI SUB UBI", 4),
    ("I AM HUNGRY", 0x20),
    ("DR FRANKENSTEIN", 0x40),
    ("BUGGY RACING", 0x80),
];

/// Shared-renderer LCG behind 0048F060 (`0055F18C`): the caller retains custody
/// of the state word; this helper only advances it exactly as the original.
/// The event log names the call site of each step.
#[track_caller]
pub fn name_entry_rng_next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(0x13D56D).wrapping_add(0x13D573);
    crate::event_log::rng(std::panic::Location::caller(), *state);
    *state >> 16
}

/// Native `delta_micros / 1000` idiom used by both tick timers
/// (`0x10624DD3` multiply, shift 6 of the high product word).
pub fn tick_millis(delta_micros: i32) -> i32 {
    ((u64::from(delta_micros as u32).wrapping_mul(0x10624DD3)) >> 38) as i32
}

#[derive(Debug, Error)]
pub enum NameEntryError {
    #[error("name-entry selection {0} is outside 0..78")]
    Selection(i32),
    #[error("text-entry screen constructed for phase {0}; 00459060 serves 9, 10 and 11")]
    Phase(u32),
}

/// Identity buffers read by the 00459060 constructor before the editable copy:
/// the name at screen `+0x11C` (cap 31, so at most 30 bytes) and initials at
/// `+0x13C` (cap 5, so at most 4 bytes). Phase 9 reads them through 00463550;
/// phases 10/11 through 00463D10. The caller performs those reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadedIdentity {
    pub name: Vec<u8>,
    pub initials: Vec<u8>,
}

/// Registry writer selected by 0045AD30. Both require the selected name;
/// initials are optional. Successful writes clear loaded-cache flag 0054EC59.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityTarget {
    /// 004633D0: `Player Name`, with optional `Player Initials` (phase 9).
    PlayerName,
    /// 00463490: `Net Player Name`, with optional initials (phases 10/11).
    NetPlayerName,
}

/// Initials pointer policy of 004633D0/00463490. A native NULL omits the
/// initials scan/write; a nonNULL empty C-string writes an empty value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityInitialsWrite<'a> {
    Keep,
    Write(&'a [u8]),
}

/// Required name and explicit optional-initials policy for either writer.
/// Byte strings use their C prefixes; no name-entry display cap is imposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityWriteRequest<'a> {
    pub target: IdentityTarget,
    pub name: &'a [u8],
    pub initials: IdentityInitialsWrite<'a>,
}

/// What 00445840 left in 0045A8F0's 15-byte key buffer at `[esp+0x10]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyRead {
    /// A reported key's name from table 00535E54, copied by `strncpy` with
    /// NUL padding; `buf[14] = 0` keeps at most fourteen bytes. Printable
    /// keys have one-byte names; unnamed codes have empty names.
    Named(Vec<u8>),
    /// No reportable key: 00445840 writes only `buf[0] = 0`. The tick still
    /// tests `buf[1]`, a stale stack byte. In the retail recording's
    /// steady-state ticks its last writer is the selected-input poll's clock
    /// sample (004F0200 -> 004A4890 at 004F0325, `push eax` at 004A48A2):
    /// bits 8..15 of `timeGetTime()` milliseconds. The first tick after
    /// construction had another, unidentified writer.
    Idle { stale_byte: u8 },
}

/// Buffer effect of one 0045A8F0 key block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBlockOutcome {
    /// `buf[1] == 0`: selection forced to cell 77, `buf[0]` appended.
    Typed(u8),
    /// `buf[1] == 0` with `buf[0] == 0`: cell 77; the NUL store changes nothing.
    Blank,
    /// `buf[1] == 0` at the limit: cell 77 selected, nothing appended.
    AtCapacity,
    /// `Delete`/`Backspace`: last byte truncated.
    Erased,
    /// Empty buffer: contained instead of the native `+0xF7` underflow write.
    EraseEmpty,
    /// `Space` below the limit: `0x20` appended.
    Spaced,
    /// `Space` at the limit.
    SpaceAtCapacity,
    /// Any other multi-byte buffer, including an idle nonzero stale byte.
    Ignored,
}

/// Held-bit slots `+24..+38` dispatched by base input 00439470, plus the
/// Tab callback. Retry (`R`, 0045AEA0) needs the profile store and stays a
/// separate method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameEntryAction {
    /// `+24` 0045AB50.
    Activate,
    /// `+28` 0045AC30.
    Up,
    /// `+2C` 0045AC50.
    Down,
    /// `+30` 0045AC70.
    Left,
    /// `+34` 0045ACD0.
    Right,
    /// `+38` 0045AC20: request phase 2.
    Back,
    /// Tab 0045AE90 -> 0045AC00: 0045AEB0 only while the error byte is clear.
    ConfirmInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Completion delay nonzero: 0045AEB0 returns without effect.
    Idle,
    /// Recognized cheat: settings flag granted, name cleared, 1000 ms feedback.
    Cheated { bit: u32 },
    /// Ordinary name: completion delay armed.
    Armed { delay_ms: i32 },
}

#[derive(Debug)]
pub enum NameCompletion {
    /// Phases 10/11: identity persisted, phase 2 requested.
    Phase2,
    /// Offline serialization done in memory, phase 3 requested.
    Phase3,
    /// The error byte was already set: persistence and serialization ran
    /// again, but 0045AE52 requests nothing. Only Activate moves on.
    ErrorPending,
    /// `0044A0E0`/`00449C10` failure caught: error byte set, no phase request.
    /// The concrete store error is retained for diagnostics.
    Failed { error: ProfileStoreError },
}

/// Effects reachable from held-slot dispatch and the 0045AEB0 confirm path.
/// Every operation is required; a backend that does not own the native
/// effect must fail rather than manufacture success.
pub trait NameEntryActions {
    type Error;
    /// `00463AB0 + 0x84` cheat flag. Bits are proven by 0045AEB0; the
    /// settings record they belong to is owned by the future settings host.
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error>;
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error>;
}

/// Effects of completion 0045AD30 beyond the actions. Every operation is
/// required. A backend that does not own the native effect must fail rather
/// than manufacture successful persistence, construction, or phase changes.
pub trait NameEntryCompletion: NameEntryActions {
    /// `004633D0`/`00463490`: required name and optional initials write.
    fn persist_identity(&mut self, request: IdentityWriteRequest<'_>) -> Result<(), Self::Error>;
    /// Offline mission/player state serialized in memory by `00449C10`.
    /// Phase 9 consumes the caller's prepared mission/player snapshot. This
    /// does not attach the working restore wrapper's later fresh-player lookup.
    fn offline_game_state(&mut self) -> Result<(MissionSaveState, PlayerSaveState), Self::Error>;
    /// `DAT_0055F448` nonzero: an active network session. 0045A8F0 reads it
    /// after the attract branch (0045A90F) to force completion; 0045AD30
    /// reads it after `0044A0E0` to skip the in-memory serialization.
    fn network_session(&mut self) -> Result<bool, Self::Error>;
    /// Port fix, not native: the quantum the tick's frame-rate steps run in
    /// (see [`NameEntryState::tick`]). None steps them every tick, as retail.
    fn effect_quantum(&self) -> Option<u32> {
        None
    }
}

/// The tick's own host operations: key translation, base input, animation.
pub trait NameEntryServices: NameEntryCompletion {
    /// `00445840`, called only while the completion delay is zero. Its
    /// process latch 0054766C advances only on these calls.
    fn read_key(&mut self) -> Result<KeyRead, Self::Error>;
    /// `004392B0(delta, elapsed)` base-input processing. Held-bit slots
    /// dispatch into the live entry through [`NameEntryState::dispatch`]
    /// with this tick's phase.
    fn tick_base_input(
        &mut self,
        entry: &mut NameEntryState,
        tick: &NameEntryTick,
    ) -> Result<(), Self::Error>;
    /// `00439560` shared animation accumulation for this tick.
    fn tick_animation(&mut self, delta_micros: i32) -> Result<(), Self::Error>;
}

/// One 0045A8F0 invocation. `delta_micros` is the capped scheduler delta and
/// `elapsed_micros` the uncapped second argument; both reach base input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameEntryTick {
    pub delta_micros: i32,
    pub elapsed_micros: i32,
    /// Frontend attract flag 0055F14C, set by 00484F6B together with its
    /// phase-9 request: complete, then continue the tick.
    pub attract: bool,
    /// Session current phase (`+0x64C`), selecting delay/completion branches.
    pub phase: u32,
}

#[derive(Debug, Error)]
pub enum NameEntryTickError<E> {
    #[error("name-entry state: {0}")]
    State(#[from] NameEntryError),
    #[error("name-entry host callback failed")]
    Host(E),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEntryState {
    /// Editable NUL-terminated bytes at screen +0xF8 (without the NUL).
    name: Vec<u8>,
    /// Constructor-loaded identity at +0x11C/+0x13C, retained for the
    /// companion value 0045AD30 persists beside the edited buffer.
    loaded: LoadedIdentity,
    /// Selected grid cell at screen +0xF4.
    selection: i32,
    /// Editable limit at screen +0x118: 18 for names, 4 for initials.
    limit: usize,
    /// Row animation at screen +0x148, adjusted by left/right input and
    /// decayed 7/8 toward zero on every tick.
    row_anim: i32,
    /// Completion delay at screen +0xEC in milliseconds.
    completion_delay: i32,
    /// Cheat feedback timer at screen +0x150 in milliseconds.
    feedback: i32,
    /// Error byte at screen +0x14C; set only by failed 0045AD30 completion.
    error: bool,
    /// Port fix: delta not yet given to the row animation and the effects.
    effect_carry: u32,
}

impl NameEntryState {
    /// Constructor 00459060 after its identity read. The session's requested
    /// phase selects the editable source: phase 11 edits the initials (limit
    /// 4), phases 9/10 the name (limit 18). The copy is truncated to that
    /// limit, and cell 77 is selected only when the result is nonempty.
    pub fn new(phase: u32, identity: LoadedIdentity) -> Result<Self, NameEntryError> {
        if !(9..=11).contains(&phase) {
            return Err(NameEntryError::Phase(phase));
        }
        // 00463550/00463D10 store NUL at cap-1 on every exit.
        let loaded = LoadedIdentity {
            name: c_prefix(&identity.name, 30).to_vec(),
            initials: c_prefix(&identity.initials, 4).to_vec(),
        };
        let (source, limit) = if phase == 11 {
            (&loaded.initials, INITIALS_LIMIT)
        } else {
            (&loaded.name, NAME_LIMIT)
        };
        let name = c_prefix(source, limit).to_vec();
        let selection = if name.is_empty() { 0 } else { CONFIRM_CELL };
        Ok(Self {
            name,
            loaded,
            selection,
            limit,
            row_anim: 0,
            effect_carry: 0,
            completion_delay: 0,
            feedback: 0,
            error: false,
        })
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn loaded(&self) -> &LoadedIdentity {
        &self.loaded
    }

    pub fn selection(&self) -> i32 {
        self.selection
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn row_anim(&self) -> i32 {
        self.row_anim
    }

    /// Completion delay at screen +0xEC in milliseconds.
    pub fn completion_delay(&self) -> i32 {
        self.completion_delay
    }

    /// Cheat feedback timer at screen +0x150 in milliseconds.
    pub fn feedback(&self) -> i32 {
        self.feedback
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn set_error(&mut self, error: bool) {
        self.error = error;
    }

    pub fn set_selection(&mut self, selection: i32) -> Result<(), NameEntryError> {
        if !(0..GRID_CELLS as i32).contains(&selection) {
            return Err(NameEntryError::Selection(selection));
        }
        self.selection = selection;
        Ok(())
    }

    /// 0045AC30/0045AC50/0045AC70/0045ACD0. All arithmetic uses wrapping i32
    /// operations and IDIV truncation toward zero, matching the original.
    pub fn apply_move(&mut self, direction: NameEntryMove) {
        match direction {
            NameEntryMove::Up => {
                self.selection = (self.selection + 52).rem_euclid(GRID_CELLS as i32);
            }
            NameEntryMove::Down => {
                self.selection = (self.selection + 26).rem_euclid(GRID_CELLS as i32);
            }
            NameEntryMove::Left => {
                let original = self.selection;
                let candidate = original.wrapping_sub(1);
                // x86 IDIV truncates toward zero; Rust `/` matches for i32.
                let same_row = candidate / ROW_CELLS == original / ROW_CELLS;
                self.selection = if same_row && candidate >= 0 {
                    candidate
                } else {
                    candidate.wrapping_add(ROW_CELLS)
                };
                self.row_anim = self.row_anim.wrapping_add(-2520);
            }
            NameEntryMove::Right => {
                let original = self.selection;
                let candidate = original.wrapping_add(1);
                let same_row = candidate / ROW_CELLS == original / ROW_CELLS;
                self.selection = if same_row {
                    candidate
                } else {
                    candidate.wrapping_sub(ROW_CELLS)
                };
                self.row_anim = self.row_anim.wrapping_add(2520);
            }
        }
    }

    /// Button Activate path 0045AB50. The caller owns the 0045AEB0 confirm
    /// effect and the phase-3 request; this owner only mutates its own fields.
    pub fn activate(&mut self) -> Result<NameEntryActivate, NameEntryError> {
        if self.error {
            return Ok(NameEntryActivate::ErrorPhase3);
        }
        if !(0..GRID_CELLS as i32).contains(&self.selection) {
            return Err(NameEntryError::Selection(self.selection));
        }
        if self.selection == DELETE_CELL {
            if self.name.is_empty() {
                return Ok(NameEntryActivate::DeleteEmpty);
            }
            self.name.pop();
            return Ok(NameEntryActivate::Deleted);
        }
        if self.selection == CONFIRM_CELL {
            return Ok(NameEntryActivate::Confirm);
        }
        if self.name.len() >= self.limit {
            return Ok(NameEntryActivate::AtCapacity);
        }
        let byte = NAME_GRID[self.selection as usize];
        self.name.push(byte);
        Ok(NameEntryActivate::Appended(byte))
    }

    /// Held-bit slot and Tab dispatch. Activate on an error requests phase 3,
    /// and Activate on cell 77 runs 0045AEB0. Back requests phase 2 without
    /// touching the entry. Tab confirms only while the error byte is clear.
    pub fn dispatch<S: NameEntryActions>(
        &mut self,
        action: NameEntryAction,
        phase: u32,
        services: &mut S,
    ) -> Result<(), NameEntryTickError<S::Error>> {
        match action {
            NameEntryAction::Activate => match self.activate()? {
                NameEntryActivate::ErrorPhase3 => services
                    .request_phase(3)
                    .map_err(NameEntryTickError::Host)?,
                NameEntryActivate::Confirm => {
                    self.confirm(phase, services)?;
                }
                _ => {}
            },
            NameEntryAction::Up => self.apply_move(NameEntryMove::Up),
            NameEntryAction::Down => self.apply_move(NameEntryMove::Down),
            NameEntryAction::Left => self.apply_move(NameEntryMove::Left),
            NameEntryAction::Right => self.apply_move(NameEntryMove::Right),
            NameEntryAction::Back => services
                .request_phase(2)
                .map_err(NameEntryTickError::Host)?,
            NameEntryAction::ConfirmInput => {
                if !self.error {
                    self.confirm(phase, services)?;
                }
            }
        }
        Ok(())
    }

    /// Confirm path 0045AEB0, reached from cell-77 Activate and the Tab
    /// callback. A pending completion delay makes the call a no-op.
    /// Recognized cheats grant their settings bit, clear the buffer, and arm
    /// the feedback timer instead of completing; ordinary names arm the
    /// completion delay (1 ms in phase 10, 1000 ms otherwise).
    pub fn confirm<S: NameEntryActions>(
        &mut self,
        phase: u32,
        services: &mut S,
    ) -> Result<ConfirmOutcome, NameEntryTickError<S::Error>> {
        if self.completion_delay != 0 {
            return Ok(ConfirmOutcome::Idle);
        }
        if let Some(bit) = cheat_flag(&self.name) {
            services
                .grant_cheat_flag(bit)
                .map_err(NameEntryTickError::Host)?;
            self.name.clear();
            self.feedback = 1000;
            return Ok(ConfirmOutcome::Cheated { bit });
        }
        let delay = if phase == 10 { 1 } else { 1000 };
        self.completion_delay = delay;
        Ok(ConfirmOutcome::Armed { delay_ms: delay })
    }

    /// Retry callback `R` (0045AEA0 -> 0045AC10): completion runs again only
    /// while the error byte is set. The byte stays set, so the rerun never
    /// requests phase 3.
    pub fn retry_after_error<S: NameEntryCompletion>(
        &mut self,
        phase: u32,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<Option<NameCompletion>, NameEntryTickError<S::Error>> {
        if !self.error {
            return Ok(None);
        }
        self.complete(phase, store, services).map(Some)
    }

    /// The 0045A93C..0045AA21 key block over the buffer 00445840 produced.
    /// A zero `buf[1]` selects cell 77 and appends `buf[0]` below the limit.
    /// Otherwise `strncmp` prefixes select Delete, Backspace or Space.
    pub fn key_block(&mut self, read: &KeyRead) -> KeyBlockOutcome {
        let mut buffer = [0u8; 15];
        match read {
            KeyRead::Named(name) => {
                let copied = c_prefix(name, 14);
                buffer[..copied.len()].copy_from_slice(copied);
            }
            // Only buf[0] is written; the comparisons below stop at buf[0].
            KeyRead::Idle { stale_byte } => buffer[1] = *stale_byte,
        }
        if buffer[1] == 0 {
            self.selection = CONFIRM_CELL;
            if self.name.len() >= self.limit {
                return KeyBlockOutcome::AtCapacity;
            }
            if buffer[0] == 0 {
                return KeyBlockOutcome::Blank;
            }
            self.name.push(buffer[0]);
            return KeyBlockOutcome::Typed(buffer[0]);
        }
        let named = |pattern: &[u8]| buffer[..pattern.len()] == *pattern;
        if named(b"Delete") || named(b"Backspace") {
            if self.name.pop().is_none() {
                // Native writes NUL at `+0xF7`, the selection high byte.
                // Contain that underflow: leave all state untouched.
                return KeyBlockOutcome::EraseEmpty;
            }
            return KeyBlockOutcome::Erased;
        }
        if named(b"Space") {
            if self.name.len() >= self.limit {
                return KeyBlockOutcome::SpaceAtCapacity;
            }
            self.name.push(0x20);
            return KeyBlockOutcome::Spaced;
        }
        KeyBlockOutcome::Ignored
    }

    /// Full tick 0045A8F0: the attract then network forced completions,
    /// which fall through to the rest of the tick; the key block while no
    /// completion delay is pending; row-animation decay; base input and
    /// animation; sixteen shared-RNG effect updates; then the completion-delay
    /// and feedback countdowns. Returns every completion in execution order.
    /// Host failures stop the owner immediately, preserving partial effects.
    /// `rng` is the shared 0048F060 word 0055F18C and `effects` the process
    /// global 00546938, which another screen also updates and 00459060 never
    /// clears; both stay in host custody.
    pub fn tick<S: NameEntryServices>(
        &mut self,
        tick: NameEntryTick,
        rng: &mut u32,
        effects: &mut [u8; 16],
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<Vec<NameCompletion>, NameEntryTickError<S::Error>> {
        let mut completions = Vec::new();
        if tick.attract {
            completions.push(self.complete(tick.phase, store, services)?);
        }
        // 0045A90F reads 0055F448 only after the attract branch.
        if services
            .network_session()
            .map_err(NameEntryTickError::Host)?
        {
            completions.push(self.complete(tick.phase, store, services)?);
        }
        if self.completion_delay == 0 {
            let read = services.read_key().map_err(NameEntryTickError::Host)?;
            self.key_block(&read);
        }
        // Port fix: retail decays the row animation and the effects once a
        // tick, with a `delta >> 13` limit that is 0 below 8.192 ms, so at a
        // high frame rate the row snaps back faster, effects flash more
        // often and a flashed glyph never fades. The fix runs them only in
        // whole quanta of the delta, carrying the rest.
        let effect_delta = match services.effect_quantum() {
            None => Some(tick.delta_micros),
            Some(quantum) if tick.delta_micros > 0 => {
                let total = self.effect_carry + tick.delta_micros as u32;
                let whole = total - total % quantum;
                self.effect_carry = total - whole;
                (whole != 0).then_some(whole as i32)
            }
            Some(_) => Some(tick.delta_micros),
        };
        // Row animation decays 7/8 toward zero with wrapping multiply and
        // truncating division, matching the native shift sequence.
        if effect_delta.is_some() {
            self.row_anim = self.row_anim.wrapping_mul(7) / 8;
        }
        services
            .tick_base_input(self, &tick)
            .map_err(NameEntryTickError::Host)?;
        services
            .tick_animation(tick.delta_micros)
            .map_err(NameEntryTickError::Host)?;
        for effect in effects
            .iter_mut()
            .take(if effect_delta.is_some() { 16 } else { 0 })
        {
            // A zero draw stores 0x7F and falls through into the same decay
            // (0045AA74 -> 0045AA7B); the decay subtracts the quotient's low
            // byte with 8-bit wrapping.
            if (name_entry_rng_next(rng) & 0xFFFF) % 32 == 0 {
                *effect = 0x7F;
            }
            let current = *effect as i8 as i32;
            let limit = effect_delta.unwrap_or(tick.delta_micros) >> 13;
            *effect = if current > limit {
                (current.wrapping_sub(current.wrapping_mul(limit) / 32)) as u8
            } else {
                0
            };
        }
        if self.completion_delay != 0 {
            let millis = tick_millis(tick.delta_micros);
            // Equality fires here, unlike the profile menu's pending timer.
            if self.completion_delay <= millis {
                self.completion_delay = 1;
                if !self.error {
                    completions.push(self.complete(tick.phase, store, services)?);
                }
            } else {
                self.completion_delay = self.completion_delay.wrapping_sub(millis);
            }
        }
        if self.feedback != 0 {
            let millis = tick_millis(tick.delta_micros);
            if self.feedback <= millis {
                self.feedback = 0;
            } else {
                self.feedback = self.feedback.wrapping_sub(millis);
            }
        }
        Ok(completions)
    }

    /// Completion 0045AD30. Blank input is substituted with Jameson first.
    /// Phase 10 persists the edited name as the net name with the loaded
    /// initials; phase 11 the loaded net name with the edited initials; both
    /// request phase 2. Phase 9 persists the edited name with the loaded
    /// initials, selects the append index, reads the network flag, and
    /// serializes the offline mission/player state in memory. Caught store
    /// failures set the error byte. Phase 3 is requested only while that
    /// byte is still clear (0045AE52). The two `004391B0` calls are pure
    /// global reads with discarded results, so the port omits them.
    pub fn complete<S: NameEntryCompletion>(
        &mut self,
        phase: u32,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<NameCompletion, NameEntryTickError<S::Error>> {
        if self.name.is_empty() {
            self.name.extend_from_slice(JAMESON);
        }
        if phase == 10 || phase == 11 {
            let (name, initials) = if phase == 10 {
                (&self.name, &self.loaded.initials)
            } else {
                (&self.loaded.name, &self.name)
            };
            services
                .persist_identity(IdentityWriteRequest {
                    target: IdentityTarget::NetPlayerName,
                    name,
                    initials: IdentityInitialsWrite::Write(initials),
                })
                .map_err(NameEntryTickError::Host)?;
            services
                .request_phase(2)
                .map_err(NameEntryTickError::Host)?;
            return Ok(NameCompletion::Phase2);
        }
        services
            .persist_identity(IdentityWriteRequest {
                target: IdentityTarget::PlayerName,
                name: &self.name,
                initials: IdentityInitialsWrite::Write(&self.loaded.initials),
            })
            .map_err(NameEntryTickError::Host)?;
        let name = self.name.clone();
        if let Err(error) = store.prepare_new_name(&name) {
            self.error = true;
            return Ok(NameCompletion::Failed { error });
        }
        let network = services
            .network_session()
            .map_err(NameEntryTickError::Host)?;
        if !network {
            // Mission/player construction has no concrete owner yet; the
            // host supplies both states instead of the port inventing them.
            let (mission, player) = services
                .offline_game_state()
                .map_err(NameEntryTickError::Host)?;
            if let Err(error) = store.serialize_working(&mission, &player) {
                self.error = true;
                return Ok(NameCompletion::Failed { error });
            }
        }
        if self.error {
            return Ok(NameCompletion::ErrorPending);
        }
        services
            .request_phase(3)
            .map_err(NameEntryTickError::Host)?;
        Ok(NameCompletion::Phase3)
    }
}

/// Bytes before the first NUL, capped at `limit` like a native NUL store.
fn c_prefix(bytes: &[u8], limit: usize) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..end.min(limit)]
}

/// Native `strcmp` order in 0045AEB0; the first match wins.
fn cheat_flag(name: &[u8]) -> Option<u32> {
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    let text = &name[..end];
    NAME_CHEATS
        .iter()
        .find(|(cheat, _)| text == cheat.as_bytes())
        .map(|(_, bit)| *bit)
}

#[cfg(test)]
#[path = "name_entry_tests.rs"]
mod tests;
