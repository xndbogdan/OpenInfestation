//! Complete 004750C0 setter over the actual reached grant and message storage.
//! Public arena lookup and unpublished constructor custody remain distinct.

use inf_formats::GRANT_MASK_WORDS;

use super::{resources, GrantOwner, LiveGrantContext, LiveGrantError};
use crate::{
    grant_text::{format_grant_text, GrantTextRequest},
    object_runtime::{
        h0_text::{capture_h0_text, H0TextCaptureRequest},
        ObjectArena, PlayerId, PlayerMessage88, PlayerMessageOperation, PlayerMessageRequest,
        PlayerMessageState,
    },
};

/// Both arms borrow real storage; neither copies masks/messages or publishes a
/// player. The constructor arm requires its reserved actual player identity,
/// mask and constructed +88 message with the normal +B8 binding to that message.
/// Arbitrary raw aliases or changed notification bindings remain outside this
/// fixed-normal, immutable-current-source domain.
pub(crate) enum GrantSetTarget<'a> {
    Published {
        arena: &'a mut ObjectArena,
        owner: GrantOwner,
    },
    ConstructingPlayer {
        player: PlayerId,
        mask: &'a mut [u32; GRANT_MASK_WORDS],
        message: &'a mut PlayerMessageState,
    },
}

impl GrantSetTarget<'_> {
    fn owner(&self) -> GrantOwner {
        match self {
            Self::Published { owner, .. } => *owner,
            Self::ConstructingPlayer { player, .. } => GrantOwner::Player9c(*player),
        }
    }

    fn word(&self, word_index: usize) -> Result<u32, LiveGrantError> {
        match self {
            Self::Published { arena, owner } => arena.live_grant_word(*owner, word_index),
            Self::ConstructingPlayer { mask, .. } => {
                mask.get(word_index)
                    .copied()
                    .ok_or(LiveGrantError::MaskBoundary {
                        owner: self.owner(),
                        word_index,
                    })
            }
        }
    }

    fn store_word(&mut self, word_index: usize, value: u32) -> Result<(), LiveGrantError> {
        let owner = self.owner();
        match self {
            Self::Published { arena, .. } => arena.store_live_grant_word(owner, word_index, value),
            Self::ConstructingPlayer { mask, .. } => {
                *mask
                    .get_mut(word_index)
                    .ok_or(LiveGrantError::MaskBoundary { owner, word_index })? = value;
                Ok(())
            }
        }
    }

    fn has(&self, id: u16) -> Result<bool, LiveGrantError> {
        // 004752E0's LOWWORD0 returns before authenticating any storage owner.
        if id == 0 {
            return Ok(true);
        }
        Ok(self.word(usize::from(id >> 5))? & (1_u32 << (id & 31)) != 0)
    }

    fn notification_recipient(
        &self,
    ) -> Result<Option<(PlayerId, PlayerMessage88)>, LiveGrantError> {
        match self {
            Self::Published { arena, owner } => match *owner {
                GrantOwner::Player9c(player) => {
                    Ok(Some((player, arena.player_message(player)?.identity())))
                }
                GrantOwner::Selector12Actor24c(actor) => {
                    let raw = arena
                        .selector12_actor(actor)?
                        .grants_24c
                        .notification_owner_1c;
                    if raw == 0 {
                        Ok(None)
                    } else {
                        Err(LiveGrantError::NotificationBinding { actor, raw })
                    }
                }
                GrantOwner::Selector15Factory198(actor) => {
                    let raw = arena.selector15(actor)?.grants_198.notification_owner_1c;
                    if raw == 0 {
                        Ok(None)
                    } else {
                        Err(LiveGrantError::NotificationBinding { actor, raw })
                    }
                }
            },
            Self::ConstructingPlayer {
                player, message, ..
            } => Ok(Some((*player, message.identity()))),
        }
    }

    fn send_notification(&mut self, player: PlayerId, text: &[u8]) -> Result<(), LiveGrantError> {
        let operation = PlayerMessageOperation::Send {
            text: Some(text),
            flag: 0,
        };
        match self {
            Self::Published { arena, .. } => {
                // 00475270 rereads the same normal player's actual message.
                let recipient = arena.player_message(player)?.identity();
                arena.apply_player_message(PlayerMessageRequest {
                    recipient,
                    operation,
                })?;
            }
            Self::ConstructingPlayer { message, .. } => {
                // The real +88 gate rereads this owned message identity/byte;
                // the unpublished player never enters the public list lookup.
                message.apply_owned_operation(operation)?;
            }
        }
        Ok(())
    }
}

/// One complete setter shared by published dispatch and the pending player
/// constructor. Earlier mask stores survive any reached source/message error;
/// the original returns void and recursively completes each satisfied child.
pub(crate) fn set_live_grant(
    target: &mut GrantSetTarget<'_>,
    raw_id: u32,
    context: &LiveGrantContext<'_>,
) -> Result<(), LiveGrantError> {
    crate::event_log::grant(raw_id);
    let id = raw_id as u16;
    if target.has(id)? {
        return Ok(());
    }
    let word_index = usize::from(id >> 5);
    // 004750FB/0047510A reread and commit after the separate owned query.
    let before = target.word(word_index)?;
    target.store_word(word_index, before | (1_u32 << (id & 31)))?;
    // 00475320 calculates the address without reading or admitting a record.
    let _record_address = resources::record_address(id);
    let network = context
        .process
        .ok_or(LiveGrantError::MissingProcess {
            owner: target.owner(),
        })?
        .network_flag;
    if network == 0 {
        // 00475121 precedes text/flags. Canonical H0's constant-NULL leaf does
        // not consume unused native upper pointer bits from this binding.
        if let Some((player, _captured_recipient)) = target.notification_recipient()? {
            let text = resources::read_text(context, id)?;
            if text != 0 {
                let flags = resources::read_flags(context, id)?;
                let record = capture_h0_text(H0TextCaptureRequest {
                    index_word: text,
                    resident: context.resident,
                })?;
                let request = if flags & 1 != 0 {
                    GrantTextRequest::Single { record }
                } else {
                    let category = capture_h0_text(H0TextCaptureRequest {
                        index_word: if flags & 8 != 0 { 0x13b } else { 0x13c },
                        resident: context.resident,
                    })?;
                    GrantTextRequest::Pair { category, record }
                };
                let formatted = format_grant_text(request, context.text_data)?;
                target.send_notification(player, formatted.bytes_with_nul())?;
            }
        }
    }
    for candidate in 0x95..0xaf {
        // 00475280 queries fresh storage before any dependency record read.
        if target.has(candidate)? {
            continue;
        }
        for slot in 0..3 {
            let dependency = resources::read_dependency(context, candidate, slot)?;
            if dependency != 0 && target.has(dependency)? {
                // 004752B7 completes this child before later words/IDs.
                set_live_grant(target, u32::from(candidate), context)?;
                break;
            }
        }
    }
    Ok(())
}
