//! Minimal live player message+88 ownership. 00413153 initializes BYTE+1=1;
//! 004135EE's known-nonzero arm returns void without consuming text or flags.
//! Enabled delivery requires the separate +3D/heap/widget phase. This owner
//! does not attach the pickup/screen writers or retain a native +B8 pointer.
//! Constructor-known NULL child/backing custody also admits 004131F0's
//! complete empty destructor and distinct world cleanup; nonempty storage
//! remains a required boundary.

use thiserror::Error;

use super::message_widgets::{
    MessageBox, MessageBoxKind, PlayerMessageBoxes, PlayerMessageEffect, PlayerMessageEnvironment,
};
use super::shown_menu::{ShownMenu, ShownMenuOwner};
use super::{ObjectArena, PlayerAttachError, PlayerId};

#[derive(Debug, Error)]
pub enum PickupMessageError {
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error("pickup H0 text {index:#06x}: {reason}")]
    Source { index: u16, reason: String },
    #[error(transparent)]
    Format(#[from] crate::grant_text::GrantTextError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
}

/// The actual player+88 subobject in one arena. PlayerId alone is local to a
/// player list; the arena identity prevents another arena's coincident ID from
/// naming this message. Neither field is a native address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerMessage88 {
    arena: u64,
    player: PlayerId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerMessageState {
    identity: PlayerMessage88,
    /// BYTE+0, the shown-message gate 0046A260 tests: 0041314F stores zero,
    /// 004137F1 sets it and 0041389B/004133C7 clear it.
    shown_00: u8,
    suppression_01: u8,
    // 00413157/5A/5D and 0041316F establish these independently. None means
    // required unowned backing, never an implicit NULL or empty widget.
    children_04_08_0c: [Option<MessageAllocationCustody>; 3],
    backing_38: Option<MessageAllocationCustody>,
    /// +38's text while it is live, without its NUL.
    text_38: Option<Vec<u8>>,
    /// BYTE +3C, the last enabled flag; the constructor leaves it unwritten.
    flag_3c: Option<u8>,
    /// BYTE +3D, zeroed by the constructor; 004135B0's replay sets it.
    replay_3d: u8,
    /// BYTE +34, zeroed by the constructor (0041316C); 004137D0 sets it
    /// when its event-6 broadcast answers. Infest01 never reads it.
    byte_34: u8,
}

/// What `00413820` draws for one player's message.
#[derive(Debug)]
pub struct PlayerMessageDraw<'a> {
    /// The shown menu at +0C, drawn first and even when suppressed.
    pub shown_menu: Option<&'a mut ShownMenu>,
    /// BYTE+1 is set: the boxes are skipped.
    pub suppressed: bool,
    /// The live line and hint boxes, unless suppressed.
    pub boxes: Option<&'a mut PlayerMessageBoxes>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MessageAllocationCustody {
    Null,
    /// A live allocation: a line or hint box in the arena's message boxes,
    /// or +38's text copy.
    Live,
}

impl PlayerMessageState {
    pub(super) fn new(arena: u64, player: PlayerId) -> Self {
        Self {
            identity: PlayerMessage88 { arena, player },
            // 0041314F and 00413153, reached by the player's 00469E6A.
            shown_00: 0,
            suppression_01: 1,
            children_04_08_0c: [Some(MessageAllocationCustody::Null); 3],
            backing_38: Some(MessageAllocationCustody::Null),
            text_38: None,
            flag_3c: None,
            replay_3d: 0,
            byte_34: 0,
        }
    }

    pub fn byte_34(&self) -> u8 {
        self.byte_34
    }

    pub(super) fn set_byte_34(&mut self, value: u8) {
        self.byte_34 = value;
    }

    pub fn identity(&self) -> PlayerMessage88 {
        self.identity
    }

    /// +38's live text, without its NUL, for fixtures.
    #[cfg(test)]
    pub(crate) fn text_38(&self) -> Option<&[u8]> {
        self.text_38.as_deref()
    }

    /// BYTE+0, nonzero while a message is shown.
    pub fn shown_00(&self) -> u8 {
        self.shown_00
    }

    /// Receipt tests start from a recording's shown byte.
    #[cfg(test)]
    pub(crate) fn set_shown_00(&mut self, value: u8) {
        self.shown_00 = value;
    }

    /// The same live BYTE+1 consumed by 004135EE, independently of the
    /// message's unowned BYTE+0 and of the player's grant/command masks.
    pub fn suppression_byte(&self) -> u8 {
        self.suppression_01
    }

    /// Complete 004131F0 in its actual constructor-known NULL domain. Native
    /// reads/deletes +0C, +04, +08, then frees +38, with no NULL stores.
    /// Logical retirement belongs to the deleting player's carrier; these
    /// source fields and suppression/grants are not rewritten on success.
    pub(super) fn destroy_known_empty(&self) -> Result<(), PlayerMessageError> {
        for (field, custody) in [
            (0x0c, self.children_04_08_0c[2]),
            (0x04, self.children_04_08_0c[0]),
            (0x08, self.children_04_08_0c[1]),
            (0x38, self.backing_38),
        ] {
            // A live box or text is deleted; its owner drops the boxes.
            let deletable = field != 0x0c && custody == Some(MessageAllocationCustody::Live);
            if custody != Some(MessageAllocationCustody::Null) && !deletable {
                return Err(PlayerMessageError::RequiredDestructionStorage {
                    recipient: self.identity,
                    field,
                });
            }
        }
        Ok(())
    }

    /// Complete 00413A20. This body reaches backing +38 before children
    /// +0C/+04/+08. A live text is freed and cleared, a live box deleted and
    /// cleared (its owner drops it); the shown menu +0C is owned only while
    /// NULL. Cleanup neither enables nor suppresses delivery.
    fn cleanup_world(&mut self) -> Result<(), PlayerMessageError> {
        for (field, custody) in [
            (0x38, self.backing_38),
            (0x0c, self.children_04_08_0c[2]),
            (0x04, self.children_04_08_0c[0]),
            (0x08, self.children_04_08_0c[1]),
        ] {
            match custody {
                Some(MessageAllocationCustody::Null) => {}
                Some(MessageAllocationCustody::Live) if field != 0x0c => {}
                _ => {
                    return Err(PlayerMessageError::RequiredCleanupStorage {
                        recipient: self.identity,
                        field,
                    })
                }
            }
        }
        if self.backing_38 == Some(MessageAllocationCustody::Live) {
            self.backing_38 = Some(MessageAllocationCustody::Null);
            self.text_38 = None;
        }
        for index in 0..2 {
            if self.children_04_08_0c[index] == Some(MessageAllocationCustody::Live) {
                self.children_04_08_0c[index] = Some(MessageAllocationCustody::Null);
            }
        }
        Ok(())
    }

    /// Actual +88 byte/gate phase, also consumed before constructor FIFO
    /// publication. Public token arena/list authentication stays in its caller.
    pub(crate) fn apply_owned_operation(
        &mut self,
        operation: PlayerMessageOperation<'_>,
    ) -> Result<(), PlayerMessageError> {
        let recipient = self.identity();
        match operation {
            PlayerMessageOperation::CleanupWorld => self.cleanup_world()?,
            PlayerMessageOperation::Suppress => self.suppression_01 = 1,
            PlayerMessageOperation::Enable => self.suppression_01 = 0,
            PlayerMessageOperation::Send { .. } => {
                // 004135EE tests BYTE+1 before caller text/flag, +3D, cached
                // storage, widgets or process/network fields. It is void RET8.
                if self.suppression_01 == 0 {
                    return Err(PlayerMessageError::EnabledMessageBoundary(recipient));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerMessageOperation<'a> {
    /// 0048E890's reached 00413A20 operation on the retained player+88.
    /// Constructor-known NULL storage is owned; nonempty widgets/backing
    /// require their actual release services, independently of BYTE+1.
    CleanupWorld,
    /// Literal1 stores at 0040631A and 0040A947. Their enclosing caller
    /// conditions and follow-up services are separate owners.
    Suppress,
    /// Literal0 stores at 0040632A and 0040AA79. A preceding lower failure
    /// does not perform this operation through an invented finally clause.
    Enable,
    Send {
        /// Absent required text backing remains unread on the suppressed arm.
        /// Enabled text ownership/encoding/termination belongs to a later phase.
        text: Option<&'a [u8]>,
        /// Captured original DWORD; the unowned enabled body uses LOWBYTE.
        flag: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerMessageRequest<'a> {
    pub recipient: PlayerMessage88,
    pub operation: PlayerMessageOperation<'a>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlayerMessageError {
    #[error("player message {0:?} belongs to another arena")]
    ForeignArena(PlayerMessage88),
    #[error("unknown player message {0:?}")]
    UnknownMessage(PlayerMessage88),
    #[error("enabled player message {0:?} requires the 004135F7 +3D/storage phase")]
    EnabledMessageBoundary(PlayerMessage88),
    #[error("enabled player message {0:?} reads a NULL text")]
    RequiredText(PlayerMessage88),
    #[error("player message {0:?} box: {1}")]
    Box(PlayerMessage88, String),
    #[error("player message {recipient:?} requires owned destruction storage at +{field:02X}")]
    RequiredDestructionStorage {
        recipient: PlayerMessage88,
        field: u16,
    },
    #[error("player message {recipient:?} requires owned world-cleanup storage at +{field:02X}")]
    RequiredCleanupStorage {
        recipient: PlayerMessage88,
        field: u16,
    },
    #[error("player message {recipient:?} requires owned update storage at +{field:02X}")]
    RequiredUpdateStorage {
        recipient: PlayerMessage88,
        field: u16,
    },
}

impl ObjectArena {
    /// 004750C0's notice of a newly granted ID: the record text alone when
    /// its flags have bit 0, else category 0x13B (bit 3) or 0x13C, a colon
    /// and the record text, formatted before 004135D0 like the pickup text.
    pub fn deliver_grant_notification(
        &mut self,
        player: PlayerId,
        notification: super::GrantNotification,
        flags: u32,
        resident: &crate::ResidentRuntime,
        data: Option<&crate::grant_text::GrantTextData<'_>>,
    ) -> Result<(), PickupMessageError> {
        use crate::grant_text::{format_grant_text, GrantTextRequest};
        use crate::object_runtime::h0_text::{capture_h0_text, H0TextCaptureRequest};
        let capture = |index| {
            capture_h0_text(H0TextCaptureRequest {
                index_word: index,
                resident: Some(resident),
            })
            .map_err(|error| PickupMessageError::Source {
                index,
                reason: format!("{error:?}"),
            })
        };
        // 00475139/004751A2 read the record before the category.
        let record = capture(notification.text)?;
        let request = if flags & 1 != 0 {
            GrantTextRequest::Single { record }
        } else {
            let category = capture(if flags & 8 != 0 { 0x13b } else { 0x13c })?;
            GrantTextRequest::Pair { category, record }
        };
        let text = format_grant_text(request, data)?;
        let recipient = self.player_message(player)?.identity();
        self.apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Send {
                text: Some(text.bytes_with_nul()),
                flag: 0,
            },
        })?;
        Ok(())
    }

    /// 0046B2B0's requests in native order: the notice, each nested grant
    /// whole, a full row's 004693C0, the class case's command, then the
    /// message. Online, only the commands run.
    pub fn deliver_pickup_outcome(
        &mut self,
        player: PlayerId,
        outcome: super::PickupOutcome,
        delivery: &PickupDelivery<'_, '_>,
    ) -> Result<(), PickupDeliveryError> {
        if let Some(sound) = outcome.sound {
            return Err(PickupDeliveryError::UnplayedSound(sound.sound));
        }
        if let (false, Some(notification)) = (delivery.online, outcome.notification) {
            self.deliver_grant_notification(
                player,
                notification,
                delivery.flags[usize::from(notification.id)],
                delivery.residency,
                delivery.text.map(|data| &data.common),
            )?;
        }
        for nested in outcome.nested {
            self.deliver_pickup_outcome(player, nested, delivery)?;
        }
        if outcome.full && !delivery.online {
            self.deliver_pickup_full_notice(player, delivery.clock, delivery.residency)?;
        }
        if let Some(operation) = outcome.command {
            self.apply_player_command(
                super::PlayerCommandRequest { player, operation },
                delivery.commands,
            )?;
        }
        if let (false, Some(message)) = (delivery.online, outcome.message) {
            self.deliver_pickup_message(player, message, delivery.residency, delivery.text)?;
        }
        Ok(())
    }

    /// 004693C0 offline: once the animation clock 005471C0 passes the gate
    /// 0054ED14 (signed), the gate moves 150 ticks past the clock and the H0
    /// string 0x304 goes to the player's message.
    pub fn deliver_pickup_full_notice(
        &mut self,
        player: PlayerId,
        clock: u32,
        resident: &crate::ResidentRuntime,
    ) -> Result<(), PickupMessageError> {
        let Some(gate) = full_notice_gate(clock, self.players.full_notice_gate) else {
            return Ok(());
        };
        self.players.full_notice_gate = gate;
        self.deliver_player_text(player, 0x304, resident)
    }

    /// `004135D0(text, 0)` with the H0 string `index`, as the factory's
    /// notices send it.
    pub fn deliver_player_text(
        &mut self,
        player: PlayerId,
        index: u16,
        resident: &crate::ResidentRuntime,
    ) -> Result<(), PickupMessageError> {
        use crate::grant_text::GrantTextSource;
        use crate::object_runtime::h0_text::{capture_h0_text, H0TextCaptureRequest};
        let source = capture_h0_text(H0TextCaptureRequest {
            index_word: index,
            resident: Some(resident),
        })
        .map_err(|error| PickupMessageError::Source {
            index,
            reason: format!("{error:?}"),
        })?;
        let recipient = self.player_message(player)?.identity();
        self.apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Send {
                text: match source {
                    GrantTextSource::Null => None,
                    GrantTextSource::Record(record) => record,
                },
                flag: 0,
            },
        })?;
        Ok(())
    }

    /// `004135D0` with `"%s: %s"` (00537CA4) of the H0 strings `category`
    /// and `record`, as a gate's missing-grant notice sends it (0048B6E3).
    pub fn deliver_player_pair_text(
        &mut self,
        player: PlayerId,
        category: u16,
        record: u16,
        resident: &crate::ResidentRuntime,
        data: Option<&crate::grant_text::GrantTextData<'_>>,
    ) -> Result<(), PickupMessageError> {
        use crate::grant_text::{format_grant_text, GrantTextRequest};
        let category = capture_text(category, resident)?;
        let record = capture_text(record, resident)?;
        let text = format_grant_text(GrantTextRequest::Pair { category, record }, data)?;
        self.send_player_text(player, text.bytes_with_nul())
    }

    /// `004135D0` with the H0 string `format` as the format of 00511B56 and
    /// the H0 string `argument` for its `%s`, as a gate names its
    /// destination (0048B9B4, 0048B9F6).
    pub fn deliver_player_warp_text(
        &mut self,
        player: PlayerId,
        format: u16,
        argument: Option<u16>,
        resident: &crate::ResidentRuntime,
        data: Option<&crate::grant_text::GrantTextData<'_>>,
    ) -> Result<(), PickupMessageError> {
        let format = capture_text(format, resident)?;
        let argument = argument
            .map(|index| capture_text(index, resident))
            .transpose()?;
        let text = crate::grant_text::format_warp_text(format, argument, data)?;
        self.send_player_text(player, text.bytes_with_nul())
    }

    /// H0 `format`'s "%s" with a name the port gives (Latin-1, no NUL),
    /// as [`Self::deliver_player_warp_text`] names an H0 world.
    pub fn deliver_player_warp_name(
        &mut self,
        player: PlayerId,
        format: u16,
        name: &[u8],
        resident: &crate::ResidentRuntime,
        data: Option<&crate::grant_text::GrantTextData<'_>>,
    ) -> Result<(), PickupMessageError> {
        let format = capture_text(format, resident)?;
        let name = [name, &[0]].concat();
        let text = crate::grant_text::format_warp_text(
            format,
            Some(crate::grant_text::GrantTextSource::Record(Some(&name))),
            data,
        )?;
        self.send_player_text(player, text.bytes_with_nul())
    }

    /// A line of the port's own (Latin-1, no NUL) on the player's message
    /// line, as `004135D0` shows retail's.
    pub fn deliver_player_port_text(
        &mut self,
        player: PlayerId,
        text: &[u8],
    ) -> Result<(), PickupMessageError> {
        self.send_player_text(player, &[text, &[0]].concat())
    }

    fn send_player_text(
        &mut self,
        player: PlayerId,
        text: &[u8],
    ) -> Result<(), PickupMessageError> {
        let recipient = self.player_message(player)?.identity();
        self.apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Send {
                text: Some(text),
                flag: 0,
            },
        })?;
        Ok(())
    }

    ///0046B2B0's message suffix. Acquire detail, name, then prefix and format
    ///the actual bytes before reaching 004135D0, even when +88 suppresses it.
    pub fn deliver_pickup_message(
        &mut self,
        player: PlayerId,
        message: super::PickupMessage,
        resident: &crate::ResidentRuntime,
        data: Option<&crate::grant_text::PickupTextData<'_>>,
    ) -> Result<(), PickupMessageError> {
        use crate::grant_text::{format_pickup_text, GrantTextSource, PickupTextRequest};
        use crate::object_runtime::h0_text::{capture_h0_text, H0TextCaptureRequest};
        let capture = |index| {
            capture_h0_text(H0TextCaptureRequest {
                index_word: index,
                resident: Some(resident),
            })
            .map_err(|error| PickupMessageError::Source {
                index,
                reason: format!("{error:?}"),
            })
        };
        let detail = if let Some(index) = message.detail_text {
            let text = capture(index)?;
            match text {
                GrantTextSource::Record(Some(bytes)) if bytes.first().copied() == Some(0) => None,
                GrantTextSource::Record(Some(bytes)) if !bytes.is_empty() => Some(text),
                _ => {
                    return Err(PickupMessageError::Source {
                        index,
                        reason: "0046BBxx detail first-byte read needs nonNULL backing".into(),
                    })
                }
            }
        } else {
            None
        };
        let name = capture(message.name_text)?;
        let prefix = capture(message.prefix_text)?;
        let text = format_pickup_text(
            PickupTextRequest {
                prefix,
                name,
                detail,
            },
            data,
        )?;
        let recipient = self.player_message(player)?.identity();
        self.apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Send {
                text: Some(text.bytes_with_nul()),
                flag: 0,
            },
        })?;
        Ok(())
    }

    /// Read the actual constructed message owner. Bare PlayerId retains its
    /// existing arena-local lookup semantics; the returned token is arena-scoped.
    pub fn player_message(
        &self,
        player: PlayerId,
    ) -> Result<&PlayerMessageState, PlayerAttachError> {
        Ok(&self.world_player(player)?.message_88)
    }

    /// Apply an actual message BYTE+1 store or 004135D0's owned send gate.
    /// This void body owns no text/cache/widget success on the enabled arm.
    pub fn apply_player_message(
        &mut self,
        request: PlayerMessageRequest<'_>,
    ) -> Result<(), PlayerMessageError> {
        let recipient = request.recipient;
        // Validate before the reached message byte, including coincident
        // PlayerId values from another arena. No actor registry is consumed.
        if recipient.arena != self.identity {
            return Err(PlayerMessageError::ForeignArena(recipient));
        }
        let message = &mut self
            .players
            .list
            .iter_mut()
            .find(|player| player.id == recipient.player)
            .ok_or(PlayerMessageError::UnknownMessage(recipient))?
            .message_88;
        match request.operation {
            PlayerMessageOperation::Send { text, flag }
                if message.suppression_01 == 0 && self.players.message_environment.is_some() =>
            {
                self.send_enabled_player_message(recipient, text, flag)
            }
            PlayerMessageOperation::CleanupWorld => {
                // 00413A39..00413A48: a live +0C is deleted with its menus;
                // the shown byte and +0C clear.
                if message.children_04_08_0c[2] == Some(MessageAllocationCustody::Live) {
                    message.children_04_08_0c[2] = Some(MessageAllocationCustody::Null);
                    message.shown_00 = 0;
                    self.destroy_shown_menu(recipient.player);
                }
                let message = &mut self
                    .players
                    .list
                    .iter_mut()
                    .find(|player| player.id == recipient.player)
                    .ok_or(PlayerMessageError::UnknownMessage(recipient))?
                    .message_88;
                message.apply_owned_operation(request.operation)?;
                // The live boxes 00413A20 deleted.
                self.players
                    .message_boxes
                    .retain(|(player, _)| *player != recipient.player);
                Ok(())
            }
            operation => message.apply_owned_operation(operation),
        }
    }

    /// 004135D0's enabled body: unless the replay gate +3D is set, the flag
    /// byte goes to +3C and the text replaces +38's copy. A zero flag adds
    /// the text to the line box +4, a nonzero one to the hint box +8, each
    /// created when absent; a hint asks for the paused UI state 6 when the
    /// settings pause on hints outside attract play.
    fn send_enabled_player_message(
        &mut self,
        recipient: PlayerMessage88,
        text: Option<&[u8]>,
        flag: u32,
    ) -> Result<(), PlayerMessageError> {
        let environment = self
            .players
            .message_environment
            .clone()
            .ok_or(PlayerMessageError::EnabledMessageBoundary(recipient))?;
        let text = text.ok_or(PlayerMessageError::RequiredText(recipient))?;
        let text = text.split(|&byte| byte == 0).next().unwrap_or_default();
        let flag = flag as u8;
        let message = &mut self
            .players
            .list
            .iter_mut()
            .find(|player| player.id == recipient.player)
            .ok_or(PlayerMessageError::UnknownMessage(recipient))?
            .message_88;
        if message.replay_3d == 0 {
            message.flag_3c = Some(flag);
            message.backing_38 = Some(MessageAllocationCustody::Live);
            message.text_38 = Some(text.to_vec());
        }
        let (index, kind) = if flag == 0 {
            (0, MessageBoxKind::Lines)
        } else {
            (1, MessageBoxKind::Hints)
        };
        // 00413230 finds the box through 004139F0: kind 1's +4, kind 2's +8.
        let custody = message.children_04_08_0c[index];
        let created = match custody {
            Some(MessageAllocationCustody::Null) => true,
            Some(MessageAllocationCustody::Live) => false,
            None => {
                return Err(PlayerMessageError::RequiredUpdateStorage {
                    recipient,
                    field: 4 + 4 * index as u16,
                })
            }
        };
        if created {
            let new = MessageBox::new(kind, &environment.menus, environment.network)
                .map_err(|error| PlayerMessageError::Box(recipient, error.to_string()))?;
            message.children_04_08_0c[index] = Some(MessageAllocationCustody::Live);
            let boxes = self.player_message_boxes_mut(recipient.player);
            match kind {
                MessageBoxKind::Lines => boxes.lines = Some(new),
                MessageBoxKind::Hints => boxes.hints = Some(new),
            }
        }
        let boxes = self.player_message_boxes_mut(recipient.player);
        let target = match kind {
            MessageBoxKind::Lines => boxes.lines.as_mut(),
            MessageBoxKind::Hints => boxes.hints.as_mut(),
        }
        .ok_or(PlayerMessageError::RequiredUpdateStorage {
            recipient,
            field: 4 + 4 * index as u16,
        })?;
        target
            .add_text(text, environment.visibility)
            .map_err(|error| PlayerMessageError::Box(recipient, error.to_string()))?;
        if kind == MessageBoxKind::Hints && environment.pause_on_hints && !environment.attract {
            self.players
                .message_effects
                .push(PlayerMessageEffect::RequestUiState(6));
        }
        Ok(())
    }

    /// 00413950(code, context) on the player's message. The select (code 4)
    /// leaves the paused UI state 6 and dismisses a live hint box by setting
    /// its display time to one microsecond, unless the UI state is 5.
    /// Otherwise only a shown message takes the code: the interface cue
    /// (004391D0: 2 for codes 0 and 1, 3 for 2 and 3, 5 for the select)
    /// and the current menu's command (004537E0), which its owner runs.
    pub(super) fn player_message_command(
        &mut self,
        player: PlayerId,
        code: u32,
        omissions: &mut std::collections::BTreeSet<super::PlayerKeyOmission>,
    ) -> Result<(), PlayerMessageError> {
        let unknown = PlayerMessageError::UnknownMessage(PlayerMessage88 {
            arena: self.identity,
            player,
        });
        let message = &self
            .world_player(player)
            .map_err(|_| unknown.clone())?
            .message_88;
        let shown = message.shown_00 != 0;
        let hints = message.children_04_08_0c[1];
        let mut dismissed = false;
        if code == 4 {
            let Some(environment) = self.players.message_environment.clone() else {
                omissions.insert(super::PlayerKeyOmission::MessageSelect);
                return Ok(());
            };
            if environment.ui_state == 6 {
                self.players
                    .message_effects
                    .push(PlayerMessageEffect::RequestUiState(4));
            }
            if environment.ui_state != 5 && hints == Some(MessageAllocationCustody::Live) {
                let recipient = PlayerMessage88 {
                    arena: self.identity,
                    player,
                };
                let boxes = self.player_message_boxes_mut(player);
                let hint =
                    boxes
                        .hints
                        .as_mut()
                        .ok_or(PlayerMessageError::RequiredUpdateStorage {
                            recipient,
                            field: 8,
                        })?;
                hint.display_micros = 1;
                dismissed = true;
            }
        }
        if shown && !dismissed {
            let cue = match code {
                0 | 1 => Some(2),
                2 | 3 => Some(3),
                4 => Some(5),
                _ => None,
            };
            self.players.ui_cues.extend(cue);
            self.players.menu_commands.push((player, code));
        }
        Ok(())
    }

    /// `004137D0(flag, menu)` up to its broadcast: the message is shown and
    /// `00413230(3, flag, menu)` (its +0C through `004139F0`) gives an
    /// existing controller the flag's
    /// viewport and reactivates it (+8), or builds one (`004140A0`); then
    /// `00453440` pushes the menu.
    pub fn show_player_menu(
        &mut self,
        player: PlayerId,
        flag: u8,
        menu: crate::menu::MenuState,
        owner: ShownMenuOwner,
    ) -> Result<(), PlayerAttachError> {
        let message = &mut self.world_player_mut(player)?.message_88;
        message.shown_00 = 1;
        let live = message.children_04_08_0c[2] == Some(MessageAllocationCustody::Live);
        message.children_04_08_0c[2] = Some(MessageAllocationCustody::Live);
        let menus = &mut self.players.shown_menus;
        if !live {
            menus.retain(|(owner, _)| *owner != player);
            menus.push((player, ShownMenu::new(flag)));
        }
        let shown = &mut menus
            .iter_mut()
            .find(|(owner, _)| *owner == player)
            .expect("a live +0C controller")
            .1;
        if live {
            shown
                .stack
                .set_viewport(super::shown_menu::shown_menu_viewport(flag));
            shown.stack.set_active(true);
        }
        shown.push(menu, owner);
        Ok(())
    }

    /// `00413410(0)`, Tab: a shown message hides as `004133C0` does;
    /// otherwise the computer (`0052FD70`, its cheats row flagged by the
    /// cheat word) shows through `004137D0(0, menu)`. False when the
    /// computer needs the message environment that holds its menu.
    pub fn toggle_player_computer(
        &mut self,
        player: PlayerId,
        cheat_word_84: u32,
    ) -> Result<bool, super::PlayerKeyError> {
        if self.world_player(player)?.message_88.shown_00 != 0 {
            self.hide_player_message(player)
                .map_err(super::PlayerKeyError::Message)?;
            return Ok(true);
        }
        let Some(environment) = self.players.message_environment.as_ref() else {
            return Ok(false);
        };
        let data = environment.menus.computer[usize::from(cheat_word_84 & !0xc0 != 0)].clone();
        let menu = crate::menu::MenuState::new(data, crate::menu::MenuBasis::INITIAL);
        self.show_player_menu_from_unit(player, 0, menu, ShownMenuOwner::Plain)?;
        Ok(true)
    }

    /// `004137D0(flag, menu)` whole: it looks the unit up (`0046AE60`; a
    /// world without actors has none), shows the menu
    /// ([`Self::show_player_menu`]) and queues the unit's broadcast of event
    /// 6 to the actors within 0x300 (`0047DD20`), which the caller runs
    /// before anything else reads them: a factory there re-arms its dock.
    pub fn show_player_menu_from_unit(
        &mut self,
        player: PlayerId,
        flag: u8,
        menu: crate::menu::MenuState,
        owner: ShownMenuOwner,
    ) -> Result<(), PlayerAttachError> {
        let unit = self.player_actor(player).ok().flatten();
        self.show_player_menu(player, flag, menu, owner)?;
        if let Some(unit) = unit {
            self.players
                .unit_broadcasts
                .push(crate::object_runtime::UnitBroadcast {
                    player,
                    unit,
                    code: crate::object_runtime::UnitBroadcastCode::MenuShown,
                });
        }
        Ok(())
    }

    /// `004135B0`, the computer's "Repeat last message": with a live `+38`
    /// text, `004135D0(+38, +3C)` sends it again under the replay gate
    /// `+3D`, which keeps `+38` and `+3C`.
    pub fn replay_player_message(&mut self, player: PlayerId) -> Result<(), PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        let message = &mut self
            .world_player_mut(player)
            .map_err(|_| PlayerMessageError::UnknownMessage(recipient))?
            .message_88;
        let (Some(text), Some(flag)) = (message.text_38.clone(), message.flag_3c) else {
            return Ok(());
        };
        message.replay_3d = 1;
        let result = self.send_enabled_player_message(recipient, Some(&text), u32::from(flag));
        if let Ok(state) = self.world_player_mut(player) {
            state.message_88.replay_3d = 0;
        }
        result
    }

    /// `004133C0`: a shown message stops showing and its controller fades
    /// out (virtual +4).
    pub fn hide_player_message(&mut self, player: PlayerId) -> Result<(), PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        let message = &mut self
            .world_player_mut(player)
            .map_err(|_| PlayerMessageError::UnknownMessage(recipient))?
            .message_88;
        if message.shown_00 == 0 {
            return Ok(());
        }
        message.shown_00 = 0;
        self.player_shown_menu_mut(player)
            .ok_or(PlayerMessageError::RequiredUpdateStorage {
                recipient,
                field: 0x0c,
            })?
            .stack
            .set_active(false);
        Ok(())
    }

    /// `00453480` on the player's controller: `menu` replaces the current
    /// menu, whose destructor then does nothing.
    pub fn replace_player_menu(
        &mut self,
        player: PlayerId,
        menu: crate::menu::MenuState,
        owner: ShownMenuOwner,
    ) -> Result<(), PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        self.player_shown_menu_mut(player)
            .ok_or(PlayerMessageError::RequiredUpdateStorage {
                recipient,
                field: 0x0c,
            })?
            .replace(menu, owner);
        Ok(())
    }

    pub fn player_shown_menu(&self, player: PlayerId) -> Option<&ShownMenu> {
        self.players
            .shown_menus
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, menu)| menu)
    }

    pub fn player_shown_menu_mut(&mut self, player: PlayerId) -> Option<&mut ShownMenu> {
        self.players
            .shown_menus
            .iter_mut()
            .find(|(owner, _)| *owner == player)
            .map(|(_, menu)| menu)
    }

    /// Drop a destroyed controller; its menus' owners wait for their
    /// destructors.
    fn destroy_shown_menu(&mut self, player: PlayerId) {
        let menus = &mut self.players.shown_menus;
        if let Some(index) = menus.iter().position(|(owner, _)| *owner == player) {
            let (_, menu) = menus.remove(index);
            for owner in menu.into_owners() {
                self.players.destroyed_menus.push((player, owner));
            }
        }
    }

    /// `00453440` on the player's live controller: `menu` becomes current.
    pub fn push_player_menu(
        &mut self,
        player: PlayerId,
        menu: crate::menu::MenuState,
        owner: ShownMenuOwner,
    ) -> Result<(), PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        self.player_shown_menu_mut(player)
            .ok_or(PlayerMessageError::RequiredUpdateStorage {
                recipient,
                field: 0x0c,
            })?
            .push(menu, owner);
        Ok(())
    }

    /// `004534D0(count)` on the player's controller; the popped menus'
    /// destructors run next. Returns whether a menu remains.
    pub fn pop_player_menu(
        &mut self,
        player: PlayerId,
        count: i32,
    ) -> Result<bool, PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        let (popped, remaining) = self
            .player_shown_menu_mut(player)
            .ok_or(PlayerMessageError::RequiredUpdateStorage {
                recipient,
                field: 0x0c,
            })?
            .pop(count);
        for owner in popped {
            self.players.destroyed_menus.push((player, owner));
        }
        Ok(remaining)
    }

    /// `00414520` (the database) and `00414610` (the hint list) before their
    /// base command `00452E20`: Previous and Next step the entry, skipping
    /// the ones the player cannot see, and the rows show it. False when the
    /// code ends there: the database takes only 0, 1 and, with the full
    /// database cheat `00546A04`, 4, whose grant goes to the session
    /// ([`super::DATABASE_SELECT`]); the hint list only 0 to 4.
    pub(crate) fn step_shown_list(
        &mut self,
        player: PlayerId,
        code: u32,
    ) -> Result<bool, PlayerMessageError> {
        let recipient = PlayerMessage88 {
            arena: self.identity,
            player,
        };
        let storage = PlayerMessageError::RequiredUpdateStorage {
            recipient,
            field: 0x0c,
        };
        let Some(owner) = self
            .player_shown_menu(player)
            .ok_or(storage.clone())?
            .current_owner()
        else {
            return Ok(true);
        };
        let environment = self
            .players
            .message_environment
            .as_ref()
            .ok_or(PlayerMessageError::EnabledMessageBoundary(recipient))?;
        let state = self
            .world_player(player)
            .map_err(|_| PlayerMessageError::UnknownMessage(recipient))?;
        let mut selected = None;
        let (step, rows): (ShownMenuOwner, [Option<u32>; 2]) = match owner {
            ShownMenuOwner::Database { entry } => {
                let database = &environment.menus.database;
                let grants = state.grants_9c;
                let full = environment.cheats.full_database;
                // 004144E0: known (004752E0 on the grant mask, 0 always) or
                // the full database cheat, and not flagged out.
                let listed = |index: i32| {
                    let index = index as usize;
                    let known = index == 0 || grants[index >> 5] & 1 << (index & 31) != 0;
                    (known || full) && database[index].flags & 2 == 0
                };
                let step: i32 = match code {
                    0 => -1,
                    1 => 1,
                    // 0041453C: under the full database cheat Select grants
                    // the shown entry; the rows show it again and the base
                    // command runs.
                    4 if full => 0,
                    _ => return Ok(false),
                };
                let mut index = i32::from(entry);
                let mut steps = 0;
                while step != 0 {
                    index += step;
                    if index < 1 {
                        index += 0xae;
                    } else if index >= i32::from(super::DATABASE_ENTRIES) {
                        index -= 0xae;
                    }
                    if listed(index) {
                        break;
                    }
                    steps += 1;
                    if steps > 0xae {
                        // Retail loops forever when no entry is listed.
                        return Err(PlayerMessageError::Box(
                            recipient,
                            "the database lists no entry; retail loops forever".into(),
                        ));
                    }
                }
                if step == 0 {
                    selected = Some(entry);
                }
                let text = u32::from(database[index as usize].text);
                (
                    ShownMenuOwner::Database {
                        entry: index as u16,
                    },
                    [Some(text), Some(index as u32)],
                )
            }
            ShownMenuOwner::HintList { hint } => {
                let seen = state.command_mask_130;
                let step: i32 = match code {
                    0 | 2 => -1,
                    1 | 3 | 4 => 1,
                    _ => return Ok(false),
                };
                let last = super::HINT_COUNT as i32 - 1;
                let mut index = hint;
                let mut steps = 0;
                loop {
                    index += step;
                    if index < 0 {
                        index = last;
                    } else if index > last {
                        index = 0;
                    }
                    // 0046C820 on the player's +130 mask.
                    if seen[(index >> 5) as usize] & 1 << (index & 31) != 0 {
                        break;
                    }
                    steps += 1;
                    if steps > last + 1 {
                        return Err(PlayerMessageError::Box(
                            recipient,
                            "the hint list has no seen hint; retail loops forever".into(),
                        ));
                    }
                }
                (
                    ShownMenuOwner::HintList { hint: index },
                    [None, Some(index as u32)],
                )
            }
            _ => return Ok(true),
        };
        if let Some(entry) = selected {
            let source = self
                .player_shown_menu(player)
                .and_then(|menu| menu.stack.current())
                .map(|current| current.data().records().source)
                .ok_or(storage.clone())?;
            self.players.menu_actions.push(super::ShownMenuAction {
                player,
                owner,
                request: crate::menu::MenuActionRequest {
                    menu_source: source,
                    item_index: 1,
                    callback_address: super::DATABASE_SELECT,
                    command: crate::menu::MenuCommand::Activate,
                    arguments: [u32::from(entry), 0],
                    context: 0,
                },
            });
        }
        let menu = self.player_shown_menu_mut(player).ok_or(storage)?;
        if let Some(owner) = menu.current_owner_mut() {
            *owner = step;
        }
        if let Some(current) = menu.stack.current_mut() {
            let mut records = current.data().records_mut();
            if let (Some(text), Some(row)) = (rows[0], records.items.get_mut(0)) {
                row.draw_arguments[0] = text;
            }
            if let (Some(index), Some(row)) = (rows[1], records.items.get_mut(1)) {
                row.draw_arguments[0] = index;
                row.age = 0;
            }
        }
        Ok(true)
    }

    /// The phase-5 screen's held slots `00443140` and `00443100`..`00443130`:
    /// `00413950(code)` on the player's message, with no Fire hold (that
    /// belongs to the player's own message keys, `0046D240`).
    pub fn in_world_menu_code(
        &mut self,
        player: PlayerId,
        code: u32,
    ) -> Result<std::collections::BTreeSet<super::PlayerKeyOmission>, PlayerMessageError> {
        let mut omissions = std::collections::BTreeSet::new();
        self.player_message_command(player, code, &mut omissions)?;
        Ok(omissions)
    }

    /// `004133E0`, the phase-5 screen's Escape: the controller's newest menu
    /// goes, and with none left the message hides (`004133C0`). Nothing
    /// happens without a controller.
    pub fn back_player_menu(&mut self, player: PlayerId) -> Result<(), PlayerMessageError> {
        if self.player_shown_menu(player).is_none() {
            return Ok(());
        }
        if !self.pop_player_menu(player, 1)? {
            self.hide_player_message(player)?;
        }
        Ok(())
    }

    /// `00438380(phase)` from a message-side owner, for the session.
    pub fn request_message_ui_state(&mut self, phase: u32) {
        self.players
            .message_effects
            .push(PlayerMessageEffect::RequestUiState(phase));
    }

    /// The row actions of menus the arena does not own since the last call.
    pub fn take_shown_menu_actions(&mut self) -> Vec<super::ShownMenuAction> {
        std::mem::take(&mut self.players.menu_actions)
    }

    /// The shown menus' commands since the last call, in order.
    pub fn take_shown_menu_commands(&mut self) -> Vec<(PlayerId, u32)> {
        std::mem::take(&mut self.players.menu_commands)
    }

    /// The menus destroyed since the last call, for their destructors.
    pub fn take_destroyed_menus(&mut self) -> Vec<(PlayerId, ShownMenuOwner)> {
        std::mem::take(&mut self.players.destroyed_menus)
    }

    /// The interface cues since the last call, in order.
    pub fn take_ui_cues(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.players.ui_cues)
    }

    fn player_message_boxes_mut(&mut self, player: PlayerId) -> &mut PlayerMessageBoxes {
        let boxes = &mut self.players.message_boxes;
        let index = match boxes.iter().position(|(owner, _)| *owner == player) {
            Some(index) => index,
            None => {
                boxes.push((player, PlayerMessageBoxes::default()));
                boxes.len() - 1
            }
        };
        &mut boxes[index].1
    }

    /// The player's live boxes, for the HUD draw.
    pub fn player_message_boxes(&self, player: PlayerId) -> Option<&PlayerMessageBoxes> {
        self.players
            .message_boxes
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, boxes)| boxes)
    }

    /// `00413820`'s reads: the shown menu +0C first, then, unless BYTE+1
    /// suppresses the message, the line (+4) and hint (+8) boxes.
    pub fn player_message_draw(
        &mut self,
        player: PlayerId,
    ) -> Result<PlayerMessageDraw<'_>, PlayerAttachError> {
        let message = &self.world_player(player)?.message_88;
        let suppressed = message.suppression_01 != 0;
        let shown_menu = self
            .players
            .shown_menus
            .iter_mut()
            .find(|(owner, _)| *owner == player)
            .map(|(_, menu)| menu);
        let boxes = if suppressed {
            None
        } else {
            self.players
                .message_boxes
                .iter_mut()
                .find(|(owner, _)| *owner == player)
                .map(|(_, boxes)| boxes)
        };
        Ok(PlayerMessageDraw {
            shown_menu,
            suppressed,
            boxes,
        })
    }

    /// The process values the message reaches; the session refreshes them.
    pub fn set_player_message_environment(&mut self, environment: PlayerMessageEnvironment) {
        self.players.message_environment = Some(environment);
    }

    pub fn player_message_environment_mut(&mut self) -> Option<&mut PlayerMessageEnvironment> {
        self.players.message_environment.as_mut()
    }

    /// UI requests the messages made since the last call.
    pub fn take_player_message_effects(&mut self) -> Vec<PlayerMessageEffect> {
        std::mem::take(&mut self.players.message_effects)
    }

    /// The update 00413870. With a zero flag, as 0046A260 calls it, the line box then
    /// the hint box update; with flag 1, as the paused tick 00438950 calls
    /// it, only the hint box updates, and only under UI state 6. A
    /// suppressed message deactivates a box first; each box ages (00413F70)
    /// and is deleted once its controller has faded out. An expired hint
    /// under UI state 6 asks for state 4. The shown menu +0C ticks first,
    /// paused or not (00414180); once faded out it is destroyed with its
    /// menus, and +0C and the shown byte clear.
    pub(crate) fn update_player_message(
        &mut self,
        player: PlayerId,
        delta_micros: i32,
        paused: bool,
    ) -> Result<(), PlayerMessageError> {
        let unknown = PlayerMessageError::UnknownMessage(PlayerMessage88 {
            arena: self.identity,
            player,
        });
        let message = &self
            .world_player(player)
            .map_err(|_| unknown.clone())?
            .message_88;
        let recipient = message.identity;
        let custody = message.children_04_08_0c;
        match custody[2] {
            Some(MessageAllocationCustody::Null) => {}
            Some(MessageAllocationCustody::Live) => {
                let math = self
                    .players
                    .message_environment
                    .as_ref()
                    .map(|environment| environment.math.clone())
                    .ok_or(PlayerMessageError::EnabledMessageBoundary(recipient))?;
                let menu = self.player_shown_menu_mut(player).ok_or(
                    PlayerMessageError::RequiredUpdateStorage {
                        recipient,
                        field: 0x0c,
                    },
                )?;
                let alive = menu
                    .tick(delta_micros, &math)
                    .map_err(|error| PlayerMessageError::Box(recipient, format!("{error:?}")))?;
                if !alive {
                    let message = &mut self
                        .world_player_mut(player)
                        .map_err(|_| unknown.clone())?
                        .message_88;
                    message.shown_00 = 0;
                    message.children_04_08_0c[2] = Some(MessageAllocationCustody::Null);
                    self.destroy_shown_menu(player);
                }
            }
            None => {
                return Err(PlayerMessageError::RequiredUpdateStorage {
                    recipient,
                    field: 0x0c,
                })
            }
        }
        let message = &self
            .world_player(player)
            .map_err(|_| unknown.clone())?
            .message_88;
        let suppressed = message.suppression_01 != 0;
        let ui_state = self
            .players
            .message_environment
            .as_ref()
            .map(|environment| environment.ui_state);
        let boxes: &[(usize, MessageBoxKind)] = if !paused {
            &[(0, MessageBoxKind::Lines), (1, MessageBoxKind::Hints)]
        } else if ui_state == Some(6) {
            &[(1, MessageBoxKind::Hints)]
        } else {
            &[]
        };
        for &(index, kind) in boxes {
            match custody[index] {
                Some(MessageAllocationCustody::Null) => continue,
                Some(MessageAllocationCustody::Live) => {}
                None => {
                    return Err(PlayerMessageError::RequiredUpdateStorage {
                        recipient,
                        field: 4 + 4 * index as u16,
                    })
                }
            }
            let environment = self
                .players
                .message_environment
                .clone()
                .ok_or(PlayerMessageError::EnabledMessageBoundary(recipient))?;
            let boxes = self.player_message_boxes_mut(player);
            let target = match kind {
                MessageBoxKind::Lines => &mut boxes.lines,
                MessageBoxKind::Hints => &mut boxes.hints,
            };
            let live = target
                .as_mut()
                .ok_or(PlayerMessageError::RequiredUpdateStorage {
                    recipient,
                    field: 4 + 4 * index as u16,
                })?;
            if suppressed {
                live.deactivate();
            }
            let alive = live
                .update(delta_micros, environment.visibility, &environment.math)
                .map_err(|error| PlayerMessageError::Box(recipient, error.to_string()))?;
            if !alive {
                *target = None;
                self.world_player_mut(player)
                    .map_err(|_| unknown.clone())?
                    .message_88
                    .children_04_08_0c[index] = Some(MessageAllocationCustody::Null);
                if kind == MessageBoxKind::Hints && environment.ui_state == 6 {
                    self.players
                        .message_effects
                        .push(PlayerMessageEffect::RequestUiState(4));
                }
            }
        }
        Ok(())
    }
}

/// The H0 string `index`, through the shared capture.
fn capture_text(
    index: u16,
    resident: &crate::ResidentRuntime,
) -> Result<crate::grant_text::GrantTextSource<'_>, PickupMessageError> {
    use crate::object_runtime::h0_text::{capture_h0_text, H0TextCaptureRequest};
    capture_h0_text(H0TextCaptureRequest {
        index_word: index,
        resident: Some(resident),
    })
    .map_err(|error| PickupMessageError::Source {
        index,
        reason: format!("{error:?}"),
    })
}

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[test]
fn empty_destruction_requires_each_reached_field_in_native_order_without_stores() {
    let mut initialized = PlayerMessageState::new(0x1234, PlayerId(17));
    initialized
        .apply_owned_operation(PlayerMessageOperation::Enable)
        .unwrap();
    assert_eq!(initialized.suppression_byte(), 0);
    // Supplied unavailable storage is a required source boundary, never a
    // dummy widget/delete callback. Mixed fields distinguish native priority.
    for (unknown, expected) in [
        (0b0000, None),
        (0b0100, Some(0x0c)),
        (0b0001, Some(0x04)),
        (0b0010, Some(0x08)),
        (0b1000, Some(0x38)),
        (0b1111, Some(0x0c)),
        (0b1011, Some(0x04)),
        (0b1010, Some(0x08)),
    ] {
        let mut message = initialized.clone();
        for index in 0..3 {
            if unknown & (1 << index) != 0 {
                message.children_04_08_0c[index] = None;
            }
        }
        if unknown & 0b1000 != 0 {
            message.backing_38 = None;
        }
        let before = message.clone();
        assert_eq!(
            message.destroy_known_empty(),
            expected.map_or(Ok(()), |field| {
                Err(PlayerMessageError::RequiredDestructionStorage {
                    recipient: message.identity(),
                    field,
                })
            })
        );
        assert_eq!(message, before);
    }
}

#[cfg(test)]
#[test]
fn empty_world_cleanup_reaches_backing_first_and_performs_no_null_branch_stores() {
    for suppression in [0, 1, 0xff] {
        let mut initialized = PlayerMessageState::new(0x5678, PlayerId(23));
        initialized.suppression_01 = suppression;
        // Unknown supplied storage is a late boundary, never a scripted
        // successful child/free service. Mixed fields distinguish the two
        // real cleanup/destructor orders without any native execution.
        for (unknown, expected) in [
            (0b0000, None),
            (0b1000, Some(0x38)),
            (0b0100, Some(0x0c)),
            (0b0001, Some(0x04)),
            (0b0010, Some(0x08)),
            (0b1111, Some(0x38)),
            (0b0111, Some(0x0c)),
            (0b0011, Some(0x04)),
        ] {
            let mut message = initialized.clone();
            for index in 0..3 {
                if unknown & (1 << index) != 0 {
                    message.children_04_08_0c[index] = None;
                }
            }
            if unknown & 0b1000 != 0 {
                message.backing_38 = None;
            }
            let before = message.clone();
            assert_eq!(
                message.apply_owned_operation(PlayerMessageOperation::CleanupWorld),
                expected.map_or(Ok(()), |field| {
                    Err(PlayerMessageError::RequiredCleanupStorage {
                        recipient: message.identity(),
                        field,
                    })
                })
            );
            assert_eq!(message, before);
        }
    }
}

/// The process values 0046B2B0's requests reach.
pub struct PickupDelivery<'a, 'd> {
    /// 0055F448: online, the notices, the full row and the messages stay
    /// silent.
    pub online: bool,
    /// The grant records' flags, for 004750C0's notice.
    pub flags: &'a [u32],
    pub residency: &'a crate::ResidentRuntime,
    pub text: Option<&'a crate::grant_text::PickupTextData<'d>>,
    pub commands: &'a super::PlayerCommandContext<'a>,
    /// The animation clock 005471C0, for 004693C0.
    pub clock: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum PickupDeliveryError {
    #[error("the pickup's 004231C0 sound {0:#x} has no caller to play it")]
    UnplayedSound(u32),
    #[error(transparent)]
    Message(#[from] PickupMessageError),
    #[error(transparent)]
    Command(#[from] super::PlayerCommandError),
}

/// 004693C0's gate 0054ED14: once the clock passes it (signed), the notice
/// shows and the gate moves 150 ticks past the clock.
pub fn full_notice_gate(clock: u32, gate: u32) -> Option<u32> {
    (clock as i32 > gate as i32).then(|| clock.wrapping_add(150))
}
