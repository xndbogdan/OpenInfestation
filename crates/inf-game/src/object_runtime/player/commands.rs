//! Explicit 0046C760/0046C820 player command policies, without enclosing callers.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use crate::{
    grant_text::GrantTextSource, process_particles::factories::ParticleFactoryProcessState,
    ResidentRuntime,
};

use super::super::h0_text::{capture_h0_text, H0TextCaptureError, H0TextCaptureRequest};
use super::{
    ObjectArena, PlayerAttachError, PlayerId, PlayerMessageError, PlayerMessageOperation,
    PlayerMessageRequest,
};

const COMMAND_TEXT_ADDRESS: u32 = 0x0053_a27c;
const COMMAND_TEXT_WORDS: u32 = 0x49;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCommandOperation {
    Query { raw_id: u32 },
    Set { raw_id: u32, silent: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCommandRequest {
    pub player: PlayerId,
    pub operation: PlayerCommandOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCommandOutcome {
    Owned(bool),
    /// The original setter returns void, including its early-return arms.
    Applied,
}

/// Actual owners borrowed through one request. Query consumes none of these;
/// Set reads only the process network field. Reborrow between requests for
/// owner changes; no admitted lower callback mutates them synchronously.
#[derive(Clone, Copy, Debug)]
pub struct PlayerCommandContext<'a> {
    pub process: Option<&'a ParticleFactoryProcessState>,
    pub executable: Option<FrontendExecutable<'a>>,
    pub resident: Option<&'a ResidentRuntime>,
}

#[derive(Debug, Error)]
pub enum PlayerCommandError {
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error("0046C760 requires the current 0055F448 network field for {player:?}")]
    MissingProcess { player: PlayerId },
    #[error("command {raw_id:#010x} on {player:?} reaches unowned mask DWORD {word_index}")]
    MaskBoundary {
        player: PlayerId,
        raw_id: u32,
        word_index: i32,
    },
    #[error("command {raw_id:#010x} requires executable bytes {address:#010x}+{bytes}")]
    MissingExecutable {
        raw_id: u32,
        address: u32,
        bytes: usize,
    },
    #[error("command {raw_id:#010x} executable read {address:#010x}+{bytes}: {source}")]
    Executable {
        raw_id: u32,
        address: u32,
        bytes: usize,
        #[source]
        source: FrontendExecutableError,
    },
    #[error("command {raw_id:#010x} reaches adjacent text bytes {address:#010x}+{bytes}")]
    AdjacentTextTable {
        raw_id: u32,
        address: u32,
        bytes: usize,
    },
    #[error("command string {index:#06x} requires the current H0 pool")]
    MissingResidency { index: u16 },
    #[error("command string {index:#06x} reaches one-past H0 storage at count {count}")]
    H0OnePast { index: u16, count: usize },
    #[error("command string {index:#06x} has unavailable registration/record backing")]
    StringRegistration { index: u16 },
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
}

impl From<H0TextCaptureError> for PlayerCommandError {
    fn from(error: H0TextCaptureError) -> Self {
        match error {
            H0TextCaptureError::MissingResidency { index } => Self::MissingResidency { index },
            H0TextCaptureError::OnePast { index, count } => Self::H0OnePast { index, count },
            H0TextCaptureError::InvalidRegistration { index } => Self::StringRegistration { index },
        }
    }
}

impl ObjectArena {
    /// Execute one explicit command body on a live player. The signed raw
    /// DWORD and LOWBYTE silent operand retain their original distinctions.
    /// Complete three-word masks and the original 73-WORD text table are
    /// admitted; raw aliases and partial backing remain reached boundaries.
    /// Bare PlayerId retains its existing arena-local lookup semantics.
    pub fn apply_player_command(
        &mut self,
        request: PlayerCommandRequest,
        context: &PlayerCommandContext<'_>,
    ) -> Result<PlayerCommandOutcome, PlayerCommandError> {
        match request.operation {
            PlayerCommandOperation::Query { raw_id } => Ok(PlayerCommandOutcome::Owned(
                self.player_command_owned(request.player, raw_id)?,
            )),
            PlayerCommandOperation::Set { raw_id, silent } => {
                self.set_player_command(request.player, raw_id, silent, context)?;
                Ok(PlayerCommandOutcome::Applied)
            }
        }
    }

    fn player_command_word(
        &self,
        player: PlayerId,
        raw_id: u32,
    ) -> Result<u32, PlayerCommandError> {
        let mask = &self.world_player(player)?.command_mask_130;
        // 0046C828..0046C843: CDQ/bias/SAR implements signed truncation.
        let word_index = (raw_id as i32) / 32;
        mask.get(word_index as usize)
            .copied()
            .ok_or(PlayerCommandError::MaskBoundary {
                player,
                raw_id,
                word_index,
            })
    }

    fn player_command_owned(
        &self,
        player: PlayerId,
        raw_id: u32,
    ) -> Result<bool, PlayerCommandError> {
        Ok(self.player_command_word(player, raw_id)? & (1u32 << (raw_id & 31)) != 0)
    }

    fn set_player_command(
        &mut self,
        player: PlayerId,
        raw_id: u32,
        silent: u32,
        context: &PlayerCommandContext<'_>,
    ) -> Result<(), PlayerCommandError> {
        crate::event_log::hint(raw_id, silent as u8);
        // 0046C760 precedes all player, ID, mask and silent reads.
        let process = context
            .process
            .ok_or(PlayerCommandError::MissingProcess { player })?;
        if process.network_flag != 0 || self.player_command_owned(player, raw_id)? {
            return Ok(());
        }
        // Native rereads the mask at 0046C7A4 before committing 0046C7AD.
        let before = self.player_command_word(player, raw_id)?;
        let word_index = (raw_id as i32) / 32;
        let mask = &mut self.world_player_mut(player)?.command_mask_130;
        let word = mask
            .get_mut(word_index as usize)
            .ok_or(PlayerCommandError::MaskBoundary {
                player,
                raw_id,
                word_index,
            })?;
        *word = before | (1u32 << (raw_id & 31));
        if silent as u8 != 0 {
            return Ok(());
        }
        let index_word = command_text_word(context, raw_id)?;
        let source = capture_h0_text(H0TextCaptureRequest {
            index_word,
            resident: context.resident,
        })?;
        // 0046C7F4/0046C809 selects the original requested player's +88.
        // Unlike grants, this phase has no formatter or late +1C binding.
        let recipient = self.player_message(player)?.identity();
        self.apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Send {
                text: match source {
                    GrantTextSource::Null => None,
                    GrantTextSource::Record(record) => record,
                },
                flag: 1,
            },
        })?;
        Ok(())
    }
}

fn command_text_word(
    context: &PlayerCommandContext<'_>,
    raw_id: u32,
) -> Result<u16, PlayerCommandError> {
    let address = COMMAND_TEXT_ADDRESS.wrapping_add(raw_id.wrapping_mul(2));
    let executable = context
        .executable
        .ok_or(PlayerCommandError::MissingExecutable {
            raw_id,
            address,
            bytes: 2,
        })?;
    // Required backing precedes the adjacent-table domain boundary.
    if raw_id >= COMMAND_TEXT_WORDS {
        return Err(PlayerCommandError::AdjacentTextTable {
            raw_id,
            address,
            bytes: 2,
        });
    }
    let bytes = executable
        .mapped(address, 2)
        .map_err(|source| PlayerCommandError::Executable {
            raw_id,
            address,
            bytes: 2,
            source,
        })?
        .bytes;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

#[cfg(test)]
mod native_evidence;
#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
