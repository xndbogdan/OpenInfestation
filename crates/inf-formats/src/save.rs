//! Save records at 00449A70..0044A9F0 and the mission/player serializers.
//!
//! Physical block redundancy is separate from the variable-name logical record.
//! No filesystem, profile mutation policy, native FILE* token, or campaign
//! activation is represented here. See FORMAT_DOCUMENTATION.md's SAV section.

use thiserror::Error;

pub const SAVE_MAX_RECORDS: usize = 14;
pub const SAVE_BLOCK_PAYLOAD_BYTES: usize = 0x7c;
pub const SAVE_COPY_BYTES: usize = 0x80;
pub const SAVE_BLOCKS_PER_RECORD: usize = 9;
pub const SAVE_PAYLOAD_BYTES: usize = 0x45c;
pub const SAVE_RECORD_BYTES: usize = 0x900;
pub const SAVE_NAME_MAX_BYTES: usize = 31;
pub const MISSION_SAVE_BYTES: usize = 0x38e;
pub const PLAYER_SAVE_BYTES: usize = 0xae;
const STATE_BYTES: usize = MISSION_SAVE_BYTES + PLAYER_SAVE_BYTES;
const PLAYER_PREFIX_BYTES: usize = 150;
const PLAYER_MASK_BYTES: usize = 24;

/// Distinct native serializer phases; neither selects nor constructs a player.
#[derive(Clone, Copy, Debug)]
pub enum SaveStateCopySource<'a> {
    Mission(&'a MissionSaveState),
    Player(&'a PlayerSaveState),
}

/// A complete known working payload and the original DWORD cursor/remaining.
/// Capacity gates interpret `remaining_bytes` as signed i32. Sources, cursor
/// and destination cannot alias; raw pointer/fault fidelity is not represented.
#[derive(Debug)]
pub struct SaveStateCopyRequest<'a> {
    pub payload: &'a mut [u8; SAVE_PAYLOAD_BYTES],
    pub cursor: &'a mut u32,
    pub remaining_bytes: u32,
    pub source: SaveStateCopySource<'a>,
}

/// Distinct unconditional native reader phases; the nested Player mask is
/// part of Player, not a separately selected public mode.
#[derive(Debug)]
pub enum SaveStateReadTarget<'a> {
    Mission(&'a mut MissionSaveState),
    Player(&'a mut PlayerSaveState),
}

/// Complete known source backing and an explicit original DWORD cursor.
/// The caller chooses an intrinsic raw-name or actual emitted-format offset;
/// this reader does not parse names, select players or impose copy capacities.
#[derive(Debug)]
pub struct SaveStateReadRequest<'a> {
    pub payload: &'a [u8; SAVE_PAYLOAD_BYTES],
    pub cursor: &'a mut u32,
    pub target: SaveStateReadTarget<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveCopy {
    Primary,
    Backup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveChecksumMismatch {
    pub stored: u32,
    pub computed: u32,
}

/// 00449F50 does not inspect the backup after a valid primary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveBlockSource {
    Primary,
    Backup { primary: SaveChecksumMismatch },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SaveError {
    #[error("save has {bytes} bytes; whole records require multiples of {SAVE_RECORD_BYTES}")]
    FileLength { bytes: usize },
    #[error("save exceeds the {SAVE_MAX_RECORDS}-profile directory capacity")]
    Capacity,
    #[error("save copy at byte {offset} is truncated: {available} of {SAVE_COPY_BYTES} bytes")]
    TruncatedCopy { offset: usize, available: usize },
    #[error("both copies at byte {offset} have invalid CRCs (native error 8700)")]
    CorruptPair {
        offset: usize,
        primary: SaveChecksumMismatch,
        backup: SaveChecksumMismatch,
    },
    #[error("save name has no NUL terminator in its {SAVE_NAME_MAX_BYTES}-byte capacity plus terminator")]
    NameTerminator,
    #[error("empty save name terminates the directory (native error 8702)")]
    EmptyName,
    #[error("save name has {bytes} bytes; maximum is {SAVE_NAME_MAX_BYTES}")]
    NameLength { bytes: usize },
    #[error("save name contains an embedded NUL")]
    NameContainsNul,
    #[error("save tail has {actual} bytes; expected {expected}")]
    TailLength { expected: usize, actual: usize },
    #[error("state copy at cursor {cursor} needs {bytes} bytes of its {SAVE_PAYLOAD_BYTES}-byte backing")]
    StateCopyRange { cursor: u32, bytes: usize },
    #[error("state read at cursor {cursor} needs {bytes} bytes of its {SAVE_PAYLOAD_BYTES}-byte backing")]
    StateReadRange { cursor: u32, bytes: usize },
}

/// The first failed/empty profile is not included in the completed prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveDirectoryEnd {
    EndOfFile,
    EmptyName {
        record_index: usize,
    },
    Unreadable {
        record_index: usize,
        block_index: usize,
        copy: Option<SaveCopy>,
        error: SaveError,
    },
    CapacityExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveDirectoryScan {
    pub records: Vec<SaveRecord>,
    pub block_sources: Vec<[SaveBlockSource; SAVE_BLOCKS_PER_RECORD]>,
    pub end: SaveDirectoryEnd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveArchive {
    records: Vec<SaveRecord>,
    block_sources: Vec<[SaveBlockSource; SAVE_BLOCKS_PER_RECORD]>,
}

impl SaveArchive {
    /// Strict portable import: require complete physical records and a complete
    /// directory. Redundant copies still use the original first-valid rule.
    pub fn parse(bytes: &[u8]) -> Result<Self, SaveError> {
        if bytes.len() > SAVE_RECORD_BYTES * SAVE_MAX_RECORDS {
            return Err(SaveError::Capacity);
        }
        if bytes.len() % SAVE_RECORD_BYTES != 0 {
            return Err(SaveError::FileLength { bytes: bytes.len() });
        }
        let scan = Self::scan_directory(bytes);
        match scan.end {
            SaveDirectoryEnd::EndOfFile => Ok(Self {
                records: scan.records,
                block_sources: scan.block_sources,
            }),
            SaveDirectoryEnd::EmptyName { .. } => Err(SaveError::EmptyName),
            SaveDirectoryEnd::Unreadable { error, .. } => Err(error),
            SaveDirectoryEnd::CapacityExceeded => Err(SaveError::Capacity),
        }
    }

    pub fn records(&self) -> &[SaveRecord] {
        &self.records
    }

    pub fn block_sources(&self) -> &[[SaveBlockSource; SAVE_BLOCKS_PER_RECORD]] {
        &self.block_sources
    }

    /// 0044A170 scans profiles in order and retains only completed records.
    /// A valid primary permits seeking over even an absent backup, as in
    /// 0044A010; `parse` separately requires complete physical framing.
    /// Native's inclusive index-14 overflow is contained at fourteen records.
    pub fn scan_directory(bytes: &[u8]) -> SaveDirectoryScan {
        let mut scan = SaveDirectoryScan {
            records: Vec::new(),
            block_sources: Vec::new(),
            end: SaveDirectoryEnd::EndOfFile,
        };
        for record_index in 0..SAVE_MAX_RECORDS {
            let record_offset = record_index * SAVE_RECORD_BYTES;
            if record_offset >= bytes.len() {
                return scan;
            }
            let mut payload = [0; SAVE_PAYLOAD_BYTES];
            let mut sources = [SaveBlockSource::Primary; SAVE_BLOCKS_PER_RECORD];
            for (block_index, source) in sources.iter_mut().enumerate() {
                let offset = record_offset + block_index * SAVE_COPY_BYTES * 2;
                let (block, recovered) = match read_pair(bytes, offset) {
                    Ok(value) => value,
                    Err((copy, error)) => {
                        scan.end = SaveDirectoryEnd::Unreadable {
                            record_index,
                            block_index,
                            copy,
                            error,
                        };
                        return scan;
                    }
                };
                if block_index == 0 && block[0] == 0 {
                    scan.end = SaveDirectoryEnd::EmptyName { record_index };
                    return scan;
                }
                let start = block_index * SAVE_BLOCK_PAYLOAD_BYTES;
                payload[start..start + SAVE_BLOCK_PAYLOAD_BYTES].copy_from_slice(block);
                *source = recovered;
            }
            match SaveRecord::from_payload(payload) {
                Ok(record) => {
                    scan.records.push(record);
                    scan.block_sources.push(sources);
                }
                Err(error) => {
                    scan.end = SaveDirectoryEnd::Unreadable {
                        record_index,
                        block_index: 0,
                        copy: None,
                        error,
                    };
                    return scan;
                }
            }
        }
        if bytes.len() > SAVE_RECORD_BYTES * SAVE_MAX_RECORDS {
            scan.end = SaveDirectoryEnd::CapacityExceeded;
        }
        scan
    }
}

/// Uninterpreted bytes, including the unused tail, remain authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRecord {
    payload: [u8; SAVE_PAYLOAD_BYTES],
    name_len: usize,
}

impl SaveRecord {
    pub fn from_payload(payload: [u8; SAVE_PAYLOAD_BYTES]) -> Result<Self, SaveError> {
        let name_len = payload[..=SAVE_NAME_MAX_BYTES]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(SaveError::NameTerminator)?;
        validate_name(&payload[..name_len])?;
        Ok(Self { payload, name_len })
    }

    /// Supply the exact tail explicitly. Native heap initialization is not
    /// defined; a caller choosing zero bytes is selecting a portable policy.
    pub fn from_state(
        name: &[u8],
        mission: &MissionSaveState,
        player: &PlayerSaveState,
        tail: &[u8],
    ) -> Result<Self, SaveError> {
        validate_name(name)?;
        let end = name.len() + 1 + STATE_BYTES;
        let expected = SAVE_PAYLOAD_BYTES - end;
        if tail.len() != expected {
            return Err(SaveError::TailLength {
                expected,
                actual: tail.len(),
            });
        }
        let mut payload = [0; SAVE_PAYLOAD_BYTES];
        write_complete_state(&mut payload, name, mission, player)?;
        payload[end..].copy_from_slice(tail);
        Self::from_payload(payload)
    }

    pub fn payload(&self) -> &[u8; SAVE_PAYLOAD_BYTES] {
        &self.payload
    }

    pub fn name(&self) -> &[u8] {
        &self.payload[..self.name_len]
    }

    pub fn mission(&self) -> MissionSaveState {
        // A private decode scratch, not a constructed live mission owner.
        let mut mission = MissionSaveState {
            mission_selection: 0,
            elapsed_seconds: 0,
            owner_b0: [0; 100],
            owner_240: [0; 100],
            owner_3c: [0; 25],
            owner_a4: 0,
        };
        let mut cursor = (self.name_len + 1) as u32;
        read_save_state(SaveStateReadRequest {
            payload: &self.payload,
            cursor: &mut cursor,
            target: SaveStateReadTarget::Mission(&mut mission),
        })
        .expect("validated record contains complete mission state");
        mission
    }

    pub fn player(&self) -> PlayerSaveState {
        // Default is a private data scratch, never an absent-player policy.
        let mut player = PlayerSaveState::default();
        let mut cursor = (self.name_len + 1 + MISSION_SAVE_BYTES) as u32;
        read_save_state(SaveStateReadRequest {
            payload: &self.payload,
            cursor: &mut cursor,
            target: SaveStateReadTarget::Player(&mut player),
        })
        .expect("validated record contains complete player state");
        player
    }

    pub fn tail(&self) -> &[u8] {
        &self.payload[self.name_len + 1 + STATE_BYTES..]
    }

    /// 00449C10 overwrites the serialized prefix in the existing working
    /// buffer; bytes after the new end retain their original fixed offsets.
    /// At a 31-byte name, 00475040's strict `remaining > 24` check fails:
    /// 00449C10 ignores the result, leaving the previous final six DWORDs.
    pub fn with_state(
        &self,
        name: &[u8],
        mission: &MissionSaveState,
        player: &PlayerSaveState,
    ) -> Result<Self, SaveError> {
        validate_name(name)?;
        let mut payload = self.payload;
        payload[..name.len()].copy_from_slice(name);
        payload[name.len()] = 0;
        let mut cursor = (name.len() + 1) as u32;
        let remaining_bytes = (SAVE_PAYLOAD_BYTES as u32).wrapping_sub(cursor);
        // 00449C49 ignores AL, then derives fresh remaining for 00449C6D.
        let _ = copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes,
            source: SaveStateCopySource::Mission(mission),
        })?;
        let remaining_bytes = (SAVE_PAYLOAD_BYTES as u32).wrapping_sub(cursor);
        let _ = copy_save_state(SaveStateCopyRequest {
            payload: &mut payload,
            cursor: &mut cursor,
            remaining_bytes,
            source: SaveStateCopySource::Player(player),
        })?;
        Self::from_payload(payload)
    }

    /// Copy/rename shifts the existing body and owned tail after the new NUL.
    /// A shorter name exposes bytes beyond the original record; fill only that
    /// suffix with zero instead of reproducing 0044A580's adjacent-memory read.
    /// A longer name drops only tail bytes that no longer fit.
    pub fn renamed_copy(&self, name: &[u8]) -> Result<Self, SaveError> {
        validate_name(name)?;
        let mut payload = [0; SAVE_PAYLOAD_BYTES];
        payload[..name.len()].copy_from_slice(name);
        let old_body = &self.payload[self.name_len + 1..];
        let new_body = &mut payload[name.len() + 1..];
        let copied = old_body.len().min(new_body.len());
        new_body[..copied].copy_from_slice(&old_body[..copied]);
        Self::from_payload(payload)
    }
}

/// 0048EBA0 tests only the reset argument's LOWBYTE after clearing progress.
/// Native callers select `Preserve` for zero and `Clear` for nonzero; the
/// scheduler's fractional remainder is outside this mission field owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionElapsedPolicy {
    Preserve,
    Clear,
}

/// Full DWORD selection and the explicit elapsed branch of 0048EBA0.
/// Selection is not validated as a phase, current world or overlay leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionResetRequest {
    pub selection: u32,
    pub elapsed_policy: MissionElapsedPolicy,
}

/// 0048EC10/0048ECC0 copy these fields in this order without alignment padding.
/// The fields keep offset names; [`crate::SaveProfileReport`] names each
/// after its consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionSaveState {
    pub mission_selection: u32,
    pub elapsed_seconds: u32,
    pub owner_b0: [u32; 100],
    pub owner_240: [u32; 100],
    pub owner_3c: [u32; 25],
    pub owner_a4: u16,
}

impl MissionSaveState {
    /// Offline 00487410 -> 0048EBA0(22, true), serialized fields only.
    pub fn new_game() -> Self {
        // This explicit fresh portable backing is not a native heap initializer.
        let mut state = Self {
            mission_selection: 0,
            elapsed_seconds: 0,
            owner_b0: [0; 100],
            owner_240: [0; 100],
            owner_3c: [0; 25],
            owner_a4: 0,
        };
        state.reset(MissionResetRequest {
            selection: 22,
            elapsed_policy: MissionElapsedPolicy::Clear,
        });
        state
    }

    /// Complete 0048EBA0..0048EC07 over known, exclusively borrowed fields.
    /// Only this serialized owner changes: live mission header/current-world
    /// fields, players and the session clock remainder are not inputs. Raw
    /// aliases, faulting storage and native store cadence are outside the domain.
    pub fn reset(&mut self, request: MissionResetRequest) {
        // 0048EBA6 stores the DWORD; 0048EBAA clears the WORD before the loop.
        self.mission_selection = request.selection;
        self.owner_a4 = 0;
        // 0048EBC9..0048EBED: 25 status / four B0 / four 240 DWORD groups.
        for index in 0..self.owner_3c.len() {
            self.owner_3c[index] = 0;
            self.owner_b0[index * 4..index * 4 + 4].fill(0);
            self.owner_240[index * 4..index * 4 + 4].fill(0);
        }
        // 0048EBF2/0048EBF9 branch on LOWBYTE; 0048EBFD is the late store.
        if request.elapsed_policy == MissionElapsedPolicy::Clear {
            self.elapsed_seconds = 0;
        }
    }

    fn read_into(
        &mut self,
        payload: &[u8; SAVE_PAYLOAD_BYTES],
        cursor: &mut u32,
    ) -> Result<(), SaveError> {
        // 0048ECCF/D2 and 0048ECE4/E7: separate scalar stores/advances.
        read_state_block(payload, cursor, |bytes| {
            self.mission_selection = u32::from_le_bytes(bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.elapsed_seconds = u32::from_le_bytes(bytes)
        })?;
        read_state_block::<400>(payload, cursor, |bytes| {
            self.owner_b0 = decode_words(&bytes)
        })?;
        read_state_block::<400>(payload, cursor, |bytes| {
            self.owner_240 = decode_words(&bytes)
        })?;
        read_state_block::<100>(payload, cursor, |bytes| {
            self.owner_3c = decode_words(&bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_a4 = u16::from_le_bytes(bytes)
        })
    }

    fn write_blocks(
        &self,
        mut copy: impl FnMut(&[u8]) -> Result<(), SaveError>,
    ) -> Result<(), SaveError> {
        copy(&self.mission_selection.to_le_bytes())?;
        copy(&self.elapsed_seconds.to_le_bytes())?;
        copy(&word_bytes(&self.owner_b0))?;
        copy(&word_bytes(&self.owner_240))?;
        copy(&word_bytes(&self.owner_3c))?;
        copy(&self.owner_a4.to_le_bytes())
    }
}

/// 0046C870/0046C9F0's 150-byte prefix, then 00475040/00475080's
/// six DWORDs from the nested owner at player+9C (its payload begins at +A0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerSaveState {
    pub owner_100: [u32; 6],
    pub owner_118: u32,
    pub owner_11c: u16,
    pub owner_11e: u16,
    pub owner_120: [u32; 4],
    pub owner_144: u32,
    pub owner_148: u32,
    pub owner_14c: u32,
    pub owner_150: [u32; 16],
    pub owner_13c: [u32; 2],
    pub owner_190: [u16; 3],
    pub owner_130: [u32; 3],
    pub owner_a0: [u32; 6],
}

impl PlayerSaveState {
    /// 00469D60 offline construction: a zeroed prefix except `+144 = 1`,
    /// `+14C = 8` and the first DWORD at `+150 = FFFFFFFF`. The nested
    /// six-DWORD mask at `+A0` is the 00413140→004750C0 ownership closure;
    /// see `grant_closure`. Grant notification and live-owner effects stay
    /// with the runtime host, as does the player object itself.
    pub fn new_player(mask: [u32; 6]) -> Self {
        Self {
            owner_144: 1,
            owner_14c: 8,
            owner_150: {
                let mut words = [0; 16];
                words[0] = 0xFFFF_FFFF;
                words
            },
            owner_a0: mask,
            ..Self::default()
        }
    }

    fn read_into(
        &mut self,
        payload: &[u8; SAVE_PAYLOAD_BYTES],
        cursor: &mut u32,
    ) -> Result<(), SaveError> {
        read_state_block::<24>(payload, cursor, |bytes| {
            self.owner_100 = decode_words(&bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_118 = u32::from_le_bytes(bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_11c = u16::from_le_bytes(bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_11e = u16::from_le_bytes(bytes)
        })?;
        // 0046CA4F..0046CA91 advances after each +120/+124/+128/+12C DWORD.
        for word in &mut self.owner_120 {
            read_state_block(payload, cursor, |bytes| *word = u32::from_le_bytes(bytes))?;
        }
        read_state_block(payload, cursor, |bytes| {
            self.owner_144 = u32::from_le_bytes(bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_148 = u32::from_le_bytes(bytes)
        })?;
        read_state_block(payload, cursor, |bytes| {
            self.owner_14c = u32::from_le_bytes(bytes)
        })?;
        read_state_block::<64>(payload, cursor, |bytes| {
            self.owner_150 = decode_words(&bytes)
        })?;
        // 0046CAFC/0046CB1D/0046CB3C: one advance per compound block.
        read_state_block::<8>(payload, cursor, |bytes| {
            self.owner_13c = decode_words(&bytes)
        })?;
        read_state_block::<6>(payload, cursor, |bytes| {
            self.owner_190 = std::array::from_fn(|index| {
                u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
            })
        })?;
        read_state_block::<12>(payload, cursor, |bytes| {
            self.owner_130 = decode_words(&bytes)
        })?;
        // 0046CB40 ->00475080: unconditional nested24, unlike copy475040.
        read_state_block::<24>(payload, cursor, |bytes| {
            self.owner_a0 = decode_words(&bytes)
        })
    }

    fn write_prefix_blocks(
        &self,
        mut copy: impl FnMut(&[u8]) -> Result<(), SaveError>,
    ) -> Result<(), SaveError> {
        copy(&word_bytes(&self.owner_100))?;
        copy(&self.owner_118.to_le_bytes())?;
        copy(&self.owner_11c.to_le_bytes())?;
        copy(&self.owner_11e.to_le_bytes())?;
        // 0046C8E7..0046C92B advances after each of these four scalars.
        for word in self.owner_120 {
            copy(&word.to_le_bytes())?;
        }
        copy(&self.owner_144.to_le_bytes())?;
        copy(&self.owner_148.to_le_bytes())?;
        copy(&self.owner_14c.to_le_bytes())?;
        copy(&word_bytes(&self.owner_150))?;
        copy(&word_bytes(&self.owner_13c))?;
        let xyz: Vec<_> = self
            .owner_190
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        copy(&xyz)?;
        copy(&word_bytes(&self.owner_130))
    }

    fn write_mask_block(
        &self,
        mut copy: impl FnMut(&[u8]) -> Result<(), SaveError>,
    ) -> Result<(), SaveError> {
        copy(&word_bytes(&self.owner_a0))
    }
}

/// 0048ECC0 or 0046C9F0→00475080 on complete known source backing.
/// These bodies have no capacity gate and their completed AL is always1.
/// Each native cursor-advance block is checked before mutation; later errors
/// retain completed destination blocks and cursor advances. Partial native
/// stores within a block, pointer/cursor aliases and raw faults are excluded.
pub fn read_save_state(request: SaveStateReadRequest<'_>) -> Result<(), SaveError> {
    let SaveStateReadRequest {
        payload,
        cursor,
        target,
    } = request;
    match target {
        SaveStateReadTarget::Mission(mission) => mission.read_into(payload, cursor),
        SaveStateReadTarget::Player(player) => player.read_into(payload, cursor),
    }
}

fn read_state_block<const N: usize>(
    payload: &[u8; SAVE_PAYLOAD_BYTES],
    cursor: &mut u32,
    store: impl FnOnce([u8; N]),
) -> Result<(), SaveError> {
    let start = *cursor as usize;
    let end = start
        .checked_add(N)
        .filter(|end| *end <= payload.len())
        .ok_or(SaveError::StateReadRange {
            cursor: *cursor,
            bytes: N,
        })?;
    store(payload[start..end].try_into().expect("checked state block"));
    *cursor = (*cursor).wrapping_add(N as u32);
    Ok(())
}

/// 0048EC10 or 0046C870→00475040, on complete typed source/backing.
/// `false` preserves the original absolute failure cursor stores. A reached
/// physical range error instead retains completed blocks and their cursor
/// advances. Each cursor-advance block is bounded before copying; partial
/// stores within a block, pointer aliases and native faults are outside admission.
pub fn copy_save_state(request: SaveStateCopyRequest<'_>) -> Result<bool, SaveError> {
    let SaveStateCopyRequest {
        payload,
        cursor,
        remaining_bytes,
        source,
    } = request;
    match source {
        SaveStateCopySource::Mission(mission) => {
            // 0048EC16/1B: signed inclusive gate, before old cursor/source reads.
            if (remaining_bytes as i32) < MISSION_SAVE_BYTES as i32 {
                *cursor = MISSION_SAVE_BYTES as u32;
                return Ok(false);
            }
            mission.write_blocks(|bytes| copy_state_block(payload, cursor, bytes))?;
            Ok(true)
        }
        SaveStateCopySource::Player(player) => {
            // 0046C87B captures this before the strict signed gate at 0046C883.
            let old_cursor = *cursor;
            if (remaining_bytes as i32) <= PLAYER_PREFIX_BYTES as i32 {
                *cursor = PLAYER_PREFIX_BYTES as u32;
                return Ok(false);
            }
            player.write_prefix_blocks(|bytes| copy_state_block(payload, cursor, bytes))?;
            // 0046C9D4/E0, with the fresh post-prefix cursor and DWORD wrapping.
            let nested_remaining = old_cursor
                .wrapping_sub(*cursor)
                .wrapping_add(remaining_bytes);
            copy_player_mask(payload, cursor, nested_remaining, player)
        }
    }
}

fn copy_player_mask(
    payload: &mut [u8; SAVE_PAYLOAD_BYTES],
    cursor: &mut u32,
    remaining_bytes: u32,
    player: &PlayerSaveState,
) -> Result<bool, SaveError> {
    // 00475040/45: no old cursor, output or mask read before this signed gate.
    if (remaining_bytes as i32) <= PLAYER_MASK_BYTES as i32 {
        *cursor = PLAYER_MASK_BYTES as u32;
        return Ok(false);
    }
    player.write_mask_block(|bytes| copy_state_block(payload, cursor, bytes))?;
    Ok(true)
}

fn copy_state_block(
    payload: &mut [u8; SAVE_PAYLOAD_BYTES],
    cursor: &mut u32,
    bytes: &[u8],
) -> Result<(), SaveError> {
    let start = *cursor as usize;
    let end = start
        .checked_add(bytes.len())
        .filter(|end| *end <= payload.len())
        .ok_or(SaveError::StateCopyRange {
            cursor: *cursor,
            bytes: bytes.len(),
        })?;
    payload[start..end].copy_from_slice(bytes);
    *cursor = (*cursor).wrapping_add(bytes.len() as u32);
    Ok(())
}

/// 00449E00/00449E40: reflected EDB88320, initial zero, no final XOR.
pub fn save_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}

/// 00449E90 writes identical payload+CRC copies, each through 00449EC0, which
/// pads a copy to 128 bytes from 00547680 (never needed after 124 payload
/// bytes). This creates canonical output;
/// callers importing an archive verbatim should retain its original file bytes.
pub fn encode_save_records(records: &[SaveRecord]) -> Result<Vec<u8>, SaveError> {
    if records.len() > SAVE_MAX_RECORDS {
        return Err(SaveError::Capacity);
    }
    let mut bytes = Vec::with_capacity(records.len() * SAVE_RECORD_BYTES);
    for record in records {
        for block in record.payload.chunks_exact(SAVE_BLOCK_PAYLOAD_BYTES) {
            let crc = save_crc32(block).to_le_bytes();
            for _ in 0..2 {
                bytes.extend_from_slice(block);
                bytes.extend_from_slice(&crc);
            }
        }
    }
    Ok(bytes)
}

fn validate_name(name: &[u8]) -> Result<(), SaveError> {
    if name.is_empty() {
        return Err(SaveError::EmptyName);
    }
    if name.len() > SAVE_NAME_MAX_BYTES {
        return Err(SaveError::NameLength { bytes: name.len() });
    }
    if name.contains(&0) {
        return Err(SaveError::NameContainsNul);
    }
    Ok(())
}

fn write_complete_state(
    payload: &mut [u8; SAVE_PAYLOAD_BYTES],
    name: &[u8],
    mission: &MissionSaveState,
    player: &PlayerSaveState,
) -> Result<(), SaveError> {
    let mut bytes = Vec::with_capacity(name.len() + 1 + STATE_BYTES);
    bytes.extend_from_slice(name);
    bytes.push(0);
    let mut append = |block: &[u8]| {
        bytes.extend_from_slice(block);
        Ok(())
    };
    // Intrinsic construction is unconditional, including the final mask.
    // It uses the same leaves without applying the native capacity policy.
    mission.write_blocks(&mut append)?;
    player.write_prefix_blocks(&mut append)?;
    player.write_mask_block(&mut append)?;
    payload[..bytes.len()].copy_from_slice(&bytes);
    Ok(())
}

fn read_pair(
    bytes: &[u8],
    offset: usize,
) -> Result<(&[u8], SaveBlockSource), (Option<SaveCopy>, SaveError)> {
    let primary = read_copy(bytes, offset).map_err(|error| (Some(SaveCopy::Primary), error))?;
    match check_copy(primary) {
        Ok(block) => Ok((block, SaveBlockSource::Primary)),
        Err(primary) => {
            let backup = read_copy(bytes, offset + SAVE_COPY_BYTES)
                .map_err(|error| (Some(SaveCopy::Backup), error))?;
            check_copy(backup)
                .map(|block| (block, SaveBlockSource::Backup { primary }))
                .map_err(|backup| {
                    (
                        None,
                        SaveError::CorruptPair {
                            offset,
                            primary,
                            backup,
                        },
                    )
                })
        }
    }
}

fn read_copy(bytes: &[u8], offset: usize) -> Result<&[u8], SaveError> {
    bytes
        .get(offset..offset + SAVE_COPY_BYTES)
        .ok_or(SaveError::TruncatedCopy {
            offset,
            available: bytes.len().saturating_sub(offset).min(SAVE_COPY_BYTES),
        })
}

fn check_copy(bytes: &[u8]) -> Result<&[u8], SaveChecksumMismatch> {
    let block = &bytes[..SAVE_BLOCK_PAYLOAD_BYTES];
    let stored = u32::from_le_bytes(bytes[SAVE_BLOCK_PAYLOAD_BYTES..].try_into().unwrap());
    let computed = save_crc32(block);
    if stored == computed {
        Ok(block)
    } else {
        Err(SaveChecksumMismatch { stored, computed })
    }
}

fn decode_words<const N: usize>(bytes: &[u8]) -> [u32; N] {
    std::array::from_fn(|index| {
        u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    })
}

fn word_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
