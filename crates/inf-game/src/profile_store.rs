//! The native save owner at 00449A70..0044A9F0, in retail's own file.
//!
//! The port keeps retail's save, `Infest.sav` beside the game's data, and
//! retail and the port read and write the same file (2026-10-27: tested both
//! ways across many worlds). Every write replaces
//! the file atomically, and one changed outside since it was read is never
//! overwritten. Native profile selection, the working record, and the
//! write-attempt flag remain distinct from the directory and disk contents.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use inf_formats::{
    copy_save_state, encode_save_records, read_save_state, MissionSaveState, PlayerSaveState,
    SaveArchive, SaveError, SaveRecord, SaveStateCopyRequest, SaveStateCopySource,
    SaveStateReadRequest, SaveStateReadTarget, SAVE_MAX_RECORDS, SAVE_PAYLOAD_BYTES,
};
use thiserror::Error;

use crate::object_runtime::{ObjectArena, PlayerAttachError};
use crate::profile_name_format::{
    format_profile_name, ProfileNameClassification, ProfileNameFormatError,
};
use crate::session::SessionState;

pub const PROFILE_LIMIT: usize = SAVE_MAX_RECORDS;
const WORKING_BYTES: usize = SAVE_PAYLOAD_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDirectoryEntry {
    pub name: Vec<u8>,
    pub mission_selection: u32,
    pub elapsed_seconds: u32,
    /// Port option, not retail: a game saved in Pelagia (world 1), which
    /// the port loads into Pelagia rather than refusing as a finished
    /// campaign ([`ProfileStore::set_pelagia_loads`]).
    pub pelagia: bool,
}

/// The mission selection a finished campaign saves: the ending's
/// 00487560(1, 1, 0) leaves the next world at 1, whose name is "Completed".
pub const COMPLETED_SELECTION: u32 = 1;

/// The campaign's last world before the Alien Brain: every finished
/// campaign in a retail list has it completed (status bit 2),
/// where a game saved in Pelagia need not.
const LAST_CAMPAIGN_WORLD: usize = 24;

impl ProfileDirectoryEntry {
    /// 0044A9F0. This is a mission-selection comparison, not an OVL leaf.
    /// A Pelagia game passes when the port loads them.
    pub fn load_is_rejected(&self) -> bool {
        self.mission_selection == COMPLETED_SELECTION && !self.pelagia
    }
}

/// Whether `record` is a game saved in Pelagia rather than a finished
/// campaign: world 1 with the campaign's last world not completed.
fn saved_in_pelagia(record: &SaveRecord) -> bool {
    let mission = record.mission();
    mission.mission_selection == COMPLETED_SELECTION
        && mission.owner_3c[LAST_CAMPAIGN_WORLD] & 2 == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProfileStatus {
    Unqueried = 0,
    EmptySlot = 2,
    Ready = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDirectory {
    pub count: usize,
    pub status: ProfileStatus,
}

/// Only explicit menu writes to native +34 are portable values. A successful
/// native read derives this word from FILE* return values; no address is forged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileControlToken {
    Unknown,
    Zero,
    MinusOne,
}

#[derive(Debug, Error)]
pub enum ProfileStoreError {
    #[error("profile file {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("profile file {path}: {source}")]
    Format { path: PathBuf, source: SaveError },
    #[error(transparent)]
    Record(#[from] SaveError),
    #[error("profile slot {slot} is outside {count} occupied slots")]
    Slot { slot: usize, count: usize },
    #[error("all fourteen profile slots are occupied")]
    Full,
    #[error("profile working record has not been loaded or serialized")]
    NoWorkingRecord,
    #[error("profile name is empty; the name-entry owner must supply its fallback")]
    EmptyName,
    #[error("profile file {0} changed after it was read; refresh before writing")]
    Changed(PathBuf),
}

/// Reached 00449C10 sources. The session's elapsed counter is the authority
/// for W+34; the mission's other fields and fresh player list stay live.
pub(crate) struct WorkingProfileSerializeRequest<'a> {
    pub mission: &'a MissionSaveState,
    pub elapsed_seconds: u32,
    pub arena: &'a ObjectArena,
    pub classification: &'a ProfileNameClassification,
}

/// Reached 00449B80 destinations. Mission save fields and the session's
/// aliased elapsed counter are updated before the fresh optional player lookup.
pub(crate) struct WorkingProfileRestoreRequest<'a> {
    pub mission: &'a mut MissionSaveState,
    pub state: &'a mut SessionState,
    pub arena: &'a mut ObjectArena,
    pub classification: &'a ProfileNameClassification,
}

#[derive(Debug, Error)]
pub(crate) enum WorkingProfileStateError {
    #[error(transparent)]
    Profile(#[from] ProfileStoreError),
    #[error(transparent)]
    Format(#[from] ProfileNameFormatError),
    #[error(transparent)]
    Record(#[from] SaveError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
}

/// Native directory records do not own the live mission/player state. Loading
/// fills the working record; its consumers must actually restore those owners.
pub struct ProfileStore {
    path: PathBuf,
    /// The file as last read or written (none: absent), which a write checks
    /// is still on disk before replacing it.
    source_bytes: Option<Vec<u8>>,
    records: Vec<SaveRecord>,
    // Native +28 query cache and each directory record's +00 validity are
    // distinct. A successful save invalidates only the selected row.
    directory: Option<Vec<Option<ProfileDirectoryEntry>>>,
    working: Option<SaveRecord>,
    // 00449A70 leaves +01's bytes unwritten. Scanning/loading working
    // records does not initialize this separate current-name owner.
    name: Option<Vec<u8>>,
    selected: usize,
    write_attempted: bool,
    control_token: ProfileControlToken,
    /// Port option: Pelagia's games load (`PortFixes::prototype_world`).
    pelagia_loads: bool,
}

impl ProfileStore {
    /// Retail's save beside a data root's game, which the port reads and
    /// writes unless `--profiles` names another: `<data root>/Infest.sav`, in
    /// whatever case a copied install spells it.
    pub fn default_path(data_root: &Path) -> PathBuf {
        crate::retail_path(data_root, "Infest.sav")
    }

    /// For tests: the save at `source` (none there: an empty one) copied to
    /// `scratch` and opened there, so a test changes no save it did not make.
    #[cfg(test)]
    pub(crate) fn open_copy(
        source: impl AsRef<Path>,
        scratch: impl AsRef<Path>,
    ) -> Result<Self, ProfileStoreError> {
        let scratch = scratch.as_ref();
        if let Some(bytes) = read_save(source.as_ref())? {
            if let Some(parent) = scratch.parent() {
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
            }
            fs::write(scratch, bytes).map_err(|error| io_error(scratch, error))?;
        }
        Self::open(scratch)
    }

    /// The save at `path`, after complete format validation: none there is
    /// an empty list, and reading creates nothing. Invalid data is an error,
    /// and the file is left as it is.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProfileStoreError> {
        let path = path.as_ref();
        let source = read_save(path)?;
        let records = parse(path, source.as_deref().unwrap_or_default())?
            .records()
            .to_vec();
        Ok(Self {
            path: path.to_path_buf(),
            source_bytes: source,
            records,
            directory: None,
            working: None,
            name: None,
            selected: 0,
            write_attempted: false,
            control_token: ProfileControlToken::Unknown,
            pelagia_loads: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    /// The native byte at +00 means a write attempt, not profile existence.
    pub fn write_attempted(&self) -> bool {
        self.write_attempted
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    /// Cached +2C, without triggering directory I/O or changing the working buffer.
    pub fn cached_count(&self) -> usize {
        self.directory.as_ref().map_or(0, Vec::len)
    }
    /// 004486E0 / operation 6 write +30 before any load or name serialization.
    pub fn select_for_menu(&mut self, slot: usize) -> Result<(), ProfileStoreError> {
        if slot > self.cached_count() || slot > PROFILE_LIMIT {
            return Err(ProfileStoreError::Slot {
                slot,
                count: self.cached_count(),
            });
        }
        self.selected = slot;
        Ok(())
    }
    pub fn control_token(&self) -> ProfileControlToken {
        self.control_token
    }
    pub fn record_control_token(&mut self, token: ProfileControlToken) {
        self.control_token = token;
    }
    /// 00449690 operation 5 calls the actual RET-true stub 0044A570, then only
    /// resets the in-memory directory. This performs no save-file formatting.
    pub(crate) fn reset_directory_after_native_format(&mut self) {
        self.directory = Some(Vec::new());
    }
    pub fn current_name(&self) -> Option<&[u8]> {
        self.name.as_deref()
    }
    /// The saves in the list that have finished the campaign.
    pub fn completed_campaigns(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.mission().mission_selection == COMPLETED_SELECTION)
            .filter(|record| !(self.pelagia_loads && saved_in_pelagia(record)))
            .count()
    }
    /// Port option, not retail: with Pelagia on, a game saved there loads
    /// into it, where retail refuses every world-1 profile as a finished
    /// campaign (2026-10-25). The list reads again.
    pub fn set_pelagia_loads(&mut self, on: bool) {
        if self.pelagia_loads != on {
            self.pelagia_loads = on;
            self.directory = None;
        }
    }
    pub fn working_record(&self) -> Option<&SaveRecord> {
        self.working.as_ref()
    }
    pub fn query_status(&self) -> ProfileStatus {
        if self.directory.is_some() {
            ProfileStatus::Ready
        } else {
            ProfileStatus::Unqueried
        }
    }

    /// 00449C90/00449CE0. A scan leaves the final successfully read profile in
    /// the shared working buffer. The desktop media-change callback is RET 0.
    /// A read that fails ends the native scan in its error frame, where
    /// 00449DF0 sets status 3 just as a complete scan does.
    /// Strict portable-file errors propagate instead of silently truncating
    /// the sidecar to the native directory scanner's accepted prefix.
    pub fn query(&mut self, force: bool) -> Result<ProfileDirectory, ProfileStoreError> {
        if force || self.directory.is_none() {
            let bytes = read_save(&self.path)?;
            let records = parse(&self.path, bytes.as_deref().unwrap_or_default())?
                .records()
                .to_vec();
            // 0044A3E0's summary: the name, then the mission and time DWORDs
            // at the formatted name's length plus one, the NUL offset for
            // every name the entry grid (00537C48, without %) can spell.
            let directory = records
                .iter()
                .map(|record| {
                    let mission = record.mission();
                    Some(ProfileDirectoryEntry {
                        name: record.name().to_vec(),
                        mission_selection: mission.mission_selection,
                        elapsed_seconds: mission.elapsed_seconds,
                        pelagia: self.pelagia_loads && saved_in_pelagia(record),
                    })
                })
                .collect();
            if let Some(last) = records.last() {
                self.working = Some(last.clone());
            }
            self.source_bytes = bytes;
            self.records = records;
            self.directory = Some(directory);
            self.control_token = ProfileControlToken::Unknown;
        }
        Ok(ProfileDirectory {
            count: self.cached_count(),
            status: ProfileStatus::Ready,
        })
    }

    pub fn slot_status(&mut self, slot: usize) -> Result<ProfileStatus, ProfileStoreError> {
        Ok(if self.entry(slot)?.is_some() {
            ProfileStatus::Ready
        } else {
            ProfileStatus::EmptySlot
        })
    }

    /// 0044A320 rescans only when an occupied directory entry is invalid.
    /// A normal count/status query after save must not overwrite the working
    /// record by reading the last profile from disk.
    pub fn entry(
        &mut self,
        slot: usize,
    ) -> Result<Option<&ProfileDirectoryEntry>, ProfileStoreError> {
        if slot >= self.query(false)?.count {
            return Ok(None);
        }
        if self.directory.as_ref().expect("queried")[slot].is_none() {
            self.query(true)?;
        }
        Ok(self
            .directory
            .as_ref()
            .expect("queried")
            .get(slot)
            .and_then(Option::as_ref))
    }

    /// 0044A170(index, false) reads the selected record without deserializing
    /// mission/player owners or assigning the current-name field.
    pub fn load(&mut self, slot: usize) -> Result<&SaveRecord, ProfileStoreError> {
        self.query(true)?;
        self.selected = slot;
        let record = self.record(slot)?.clone();
        self.working = Some(record);
        Ok(self.working.as_ref().expect("loaded record"))
    }

    /// 0044A0E0 selects profile_count, regardless of which empty menu row was
    /// chosen. Name-entry supplies Jameson for an empty confirmation first.
    pub fn prepare_new_name(&mut self, name: &[u8]) -> Result<(), ProfileStoreError> {
        let end = name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name.len());
        let name = &name[..end.min(31)];
        if name.is_empty() {
            return Err(ProfileStoreError::EmptyName);
        }
        self.name = Some(name.to_vec());
        self.selected = self.query(false)?.count;
        if self.selected >= PROFILE_LIMIT {
            return Err(ProfileStoreError::Full);
        }
        Ok(())
    }

    /// 00449C10 writes only the working buffer. Name confirmation does not
    /// save to disk. Existing bytes beyond the serialized state stay in place.
    /// With no earlier record, zero initialization replaces undefined heap
    /// contents; this is a portable memory policy, not recovered save data.
    pub fn serialize_working(
        &mut self,
        mission: &MissionSaveState,
        player: &PlayerSaveState,
    ) -> Result<(), ProfileStoreError> {
        // Preserve phase9's existing prepared-name admission. The live
        // FORMAT consumer separately distinguishes unwritten current bytes.
        let name = self
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or(ProfileStoreError::EmptyName)?;
        let record = if let Some(working) = &self.working {
            working.with_state(name, mission, player)?
        } else {
            let mut payload = [0; WORKING_BYTES];
            payload[..name.len()].copy_from_slice(name);
            SaveRecord::from_payload(payload)?.with_state(name, mission, player)?
        };
        self.working = Some(record);
        Ok(())
    }

    /// Bounded live 00449C10: complete name FORMAT, mission, then a fresh
    /// optional first player. This requires existing known working storage;
    /// phase9's prepared snapshot/fresh-buffer policy above is separate.
    /// Admission errors publish no partial CRT output. Native aliasing,
    /// unpassed variadic arguments and unknown heap bytes are not supplied.
    pub(crate) fn serialize_current_working(
        &mut self,
        request: WorkingProfileSerializeRequest<'_>,
    ) -> Result<(), WorkingProfileStateError> {
        let mut payload = *self
            .working
            .as_ref()
            .ok_or(ProfileStoreError::NoWorkingRecord)?
            .payload();
        // Known name writers own the terminal NUL even though the portable
        // name value excludes it. None remains unknown, never an empty C string.
        let format = self.name.as_ref().map(|name| {
            let mut bytes = name.clone();
            bytes.push(0);
            bytes
        });
        let name = format_profile_name(format.as_deref(), request.classification)?;
        let prefix = name.bytes_with_nul();
        payload[..prefix.len()].copy_from_slice(prefix);
        // SaveRecord cannot represent a directory-empty output. That separate
        // admission rejects it before mission/player copying or publication.
        self.working = Some(SaveRecord::from_payload(payload)?);
        let mut cursor = name.length().wrapping_add(1);
        let mut mission = request.mission.clone();
        mission.elapsed_seconds = request.elapsed_seconds;
        let remaining_bytes = (WORKING_BYTES as u32).wrapping_sub(cursor);
        // 00449C49 ignores AL; it still performs the later player lookup.
        let _ = copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes,
            source: SaveStateCopySource::Mission(&mission),
        })?;
        self.working = Some(SaveRecord::from_payload(payload)?);
        // 00449C4B occurs after the mission copy. No stored world.player or
        // prepared default player is substituted when this list is empty.
        if let Some(player) = request.arena.first_player() {
            let player = request.arena.export_player_save_state(player)?;
            let remaining_bytes = (WORKING_BYTES as u32).wrapping_sub(cursor);
            let _ = copy_save_state(SaveStateCopyRequest {
                payload: &mut payload,
                cursor: &mut cursor,
                remaining_bytes,
                source: SaveStateCopySource::Player(&player),
            })?;
            self.working = Some(SaveRecord::from_payload(payload)?);
        }
        Ok(())
    }

    /// Bounded live 00449B80: format the original working source into the
    /// current name, then restore the actual mission and fresh first player.
    /// The source stays at its original offsets: A%%B emits cursor4 while its
    /// intrinsic raw-name body begins at5. The returned owned current name
    /// permits the mandatory identity writer only after releasing this loan.
    /// Complete known backing, completed bounded formatting and callback-free
    /// stable owners are admitted; raw aliases, partial CRT/native faults and
    /// manager rebinding are not reproduced.
    pub(crate) fn restore_current_working(
        &mut self,
        request: WorkingProfileRestoreRequest<'_>,
    ) -> Result<Vec<u8>, WorkingProfileStateError> {
        let payload = self
            .working
            .as_ref()
            .ok_or(ProfileStoreError::NoWorkingRecord)?
            .payload();
        let formatted = format_profile_name(Some(payload), request.classification)?;
        let prefix = formatted.bytes_with_nul();
        self.name = Some(prefix[..prefix.len() - 1].to_vec());
        let mut cursor = formatted.length().wrapping_add(1);
        restore_live_mission_state(payload, &mut cursor, request.mission, request.state)?;
        // 00449BDE is reached only after the mission reader. No stored
        // world.player or snapshot/default player replaces an empty list.
        if let Some(player) = request.arena.first_player() {
            let mut saved = request.arena.export_player_save_state(player)?;
            let result = read_save_state(SaveStateReadRequest {
                payload,
                cursor: &mut cursor,
                target: SaveStateReadTarget::Player(&mut saved),
            });
            // Seeded from this actual player, so late read errors preserve
            // every unreached field. Pure stable custody has no observer
            // between the read and this existing destination owner.
            request.arena.restore_player_save_prefix(player, &saved)?;
            result?;
        }
        Ok(self.name.as_ref().expect("completed current name").clone())
    }

    /// 0044A110 / 0044A580(mode 0), reached by the actual game save boundary.
    pub fn save_working(&mut self) -> Result<(), ProfileStoreError> {
        let working = self
            .working
            .clone()
            .ok_or(ProfileStoreError::NoWorkingRecord)?;
        let mut records = self.records.clone();
        if self.selected > records.len() {
            return Err(ProfileStoreError::Slot {
                slot: self.selected,
                count: records.len(),
            });
        }
        if self.selected == records.len() {
            if records.len() >= PROFILE_LIMIT {
                return Err(ProfileStoreError::Full);
            }
            records.push(working);
        } else {
            records[self.selected] = working;
        }
        let mut directory = self
            .directory
            .clone()
            .unwrap_or_else(|| vec![None; records.len()]);
        directory.resize(records.len(), None);
        directory[self.selected] = None;
        self.write_records(records)?;
        self.directory = Some(directory);
        Ok(())
    }

    /// 0044A150 / mode 1 appends a renamed copy; it does not overwrite a slot.
    pub fn copy_profile(&mut self, slot: usize) -> Result<(), ProfileStoreError> {
        self.query(true)?;
        if self.records.len() >= PROFILE_LIMIT {
            return Err(ProfileStoreError::Full);
        }
        self.selected = slot;
        let record = self.record(slot)?;
        let copy = record.renamed_copy(&copy_name(record.name()))?;
        let mut records = self.records.clone();
        records.push(copy);
        self.write_records(records)?;
        self.query(true)?;
        Ok(())
    }

    /// 0044A130 / mode 2 removes one record and preserves the remaining order.
    pub fn delete_profile(&mut self, slot: usize) -> Result<(), ProfileStoreError> {
        self.query(true)?;
        self.record(slot)?;
        self.selected = slot;
        let mut records = self.records.clone();
        records.remove(slot);
        self.write_records(records)?;
        self.query(true)?;
        Ok(())
    }

    fn record(&self, slot: usize) -> Result<&SaveRecord, ProfileStoreError> {
        self.records.get(slot).ok_or(ProfileStoreError::Slot {
            slot,
            count: self.records.len(),
        })
    }

    fn write_records(&mut self, records: Vec<SaveRecord>) -> Result<(), ProfileStoreError> {
        let bytes = encode_save_records(&records)?;
        self.write_attempted = true;
        self.directory = None;
        match atomic_write(&self.path, &bytes, self.source_bytes.as_deref()) {
            Ok(()) => {
                self.records = records;
                self.source_bytes = Some(bytes);
                Ok(())
            }
            Err(failure) => {
                // 0044A580 clears +00 if opening the write fails. A later I/O
                // failure retains it. Atomic replacement protects old bytes.
                self.write_attempted = failure.opened;
                Err(failure.error)
            }
        }
    }
}

/// Exact bounded name rewrite from 0044A580(mode 1). At most eighteen bytes;
/// final digits 0..8 increment, while 9 (or a nondigit) appends/replaces with 1.
fn copy_name(name: &[u8]) -> Vec<u8> {
    let last = name[name.len() - 1];
    let mut insertion = name.len();
    let digit = if (b'0'..=b'8').contains(&last) {
        if insertion < 18 {
            insertion -= 1;
        }
        last + 1
    } else {
        b'1'
    };
    insertion = insertion.min(17);
    let mut renamed = name[..insertion].to_vec();
    renamed.push(digit);
    renamed
}

/// The save file's bytes, none when there is no file.
fn read_save(path: &Path) -> Result<Option<Vec<u8>>, ProfileStoreError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(path, source)),
    }
}

fn parse(path: &Path, bytes: &[u8]) -> Result<SaveArchive, ProfileStoreError> {
    SaveArchive::parse(bytes).map_err(|source| ProfileStoreError::Format {
        path: path.to_path_buf(),
        source,
    })
}

fn io_error(path: &Path, source: io::Error) -> ProfileStoreError {
    ProfileStoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Mission W+34 physically aliases session+248. In this synchronous pure
/// reader domain, project the second completed field before a later error or
/// player lookup. First-only failure must preserve the authoritative seconds,
/// even when mission.save contains a stale projection; remainder is untouched.
fn restore_live_mission_state(
    payload: &[u8; SAVE_PAYLOAD_BYTES],
    cursor: &mut u32,
    mission: &mut MissionSaveState,
    state: &mut SessionState,
) -> Result<(), SaveError> {
    const MISSION_ELAPSED_FIELD_END_BYTES: u32 = 8;
    let start = *cursor;
    let result = read_save_state(SaveStateReadRequest {
        payload,
        cursor,
        target: SaveStateReadTarget::Mission(mission),
    });
    // Every completed span lies inside1116; completed progress cannot wrap.
    if cursor.wrapping_sub(start) >= MISSION_ELAPSED_FIELD_END_BYTES {
        state.restore_mission_elapsed_seconds(mission.elapsed_seconds);
    }
    result
}

struct WriteFailure {
    error: ProfileStoreError,
    opened: bool,
}

fn atomic_write(path: &Path, bytes: &[u8], previous: Option<&[u8]>) -> Result<(), WriteFailure> {
    let before_open = |source| WriteFailure {
        error: io_error(path, source),
        opened: false,
    };
    // Read before creating staging state, and check again immediately before
    // replacement. This detects external edits; it is not a filesystem lock.
    let unchanged = || -> Result<(), ProfileStoreError> {
        match (previous, fs::read(path)) {
            (Some(expected), Ok(current)) if expected == current => Ok(()),
            (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            (_, Err(error)) if error.kind() != io::ErrorKind::NotFound => {
                Err(io_error(path, error))
            }
            _ => Err(ProfileStoreError::Changed(path.to_path_buf())),
        }
    };
    unchanged().map_err(|error| WriteFailure {
        error,
        opened: false,
    })?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(before_open)?;
    }
    let mut filename = path.as_os_str().to_os_string();
    filename.push(".tmp");
    let temporary = PathBuf::from(filename);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(before_open)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = written
        .map_err(|source| io_error(&temporary, source))
        .and_then(|()| unchanged())
        .and_then(|()| fs::rename(&temporary, path).map_err(|source| io_error(path, source)));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| WriteFailure {
        error,
        opened: true,
    })
}

#[cfg(test)]
#[path = "profile_store_tests.rs"]
mod tests;
