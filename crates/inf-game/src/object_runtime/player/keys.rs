//! The first player's key callbacks 00469450..004698E0 and the message keys
//! 0046D2E0/0046D240.
//!
//! The gameplay screen's +FC rows call each callback with the player as
//! context (ECX), the key state in EDX (nonzero while down) and the input
//! clock's two stack words. Most store a request in the control block for
//! the vehicle update to consume: a press stores it, a release does nothing.
//! Left Ctrl (+38), Right Ctrl (+40) and Numpad 0 (+41) follow the key both
//! ways. The cursor keys and Enter double as message keys: a press sets a
//! +50 bit through 0046D2E0, a release clears it, and 0046D240 acts on the
//! held bits now and every 100 ms of the player update's +54 repeat.

use std::collections::BTreeSet;

use thiserror::Error;

use super::{ObjectArena, PlayerAttachError, PlayerId};

/// Calls the port does not own. Each is skipped whole; its caller goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlayerKeyOmission {
    /// 00413410's show without the message environment, which holds the
    /// computer's menu.
    MessageToggle,
    /// 00413950's select (code 4: Enter, or Fire under a message) without
    /// the message environment, which holds the UI state it reads.
    MessageSelect,
    /// 00413950's navigation of a shown message (004391D0, 004537E0).
    MessageMenu,
}

/// One callback invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerKeyEvent {
    pub address: u32,
    /// EDX: nonzero while the key is down.
    pub value: u32,
    /// The two stack words: the input clock in microseconds.
    pub timestamp_micros: u64,
}

/// Process values the callbacks read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerKeyInputs {
    /// 0055F448: while it is set, Tab and Escape do nothing.
    pub network: u32,
    /// DWORD 0053A278: the Grenade double tap's window, in 1024 µs ticks.
    pub double_tap_window: i32,
    /// Settings `+84`, the cheat word: Tab's computer shows its cheats row
    /// for a bit outside `0xC0`.
    pub cheat_word_84: u32,
}

#[derive(Debug, Error)]
pub enum PlayerKeyError {
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error("{0:08X} is not one of the player's key callbacks")]
    NotPlayerKey(u32),
    #[error("player {player:?} key callback reads its unwritten +{offset:X}")]
    UnknownField { player: PlayerId, offset: u16 },
    #[error(transparent)]
    Message(super::PlayerMessageError),
}

#[derive(Clone, Copy)]
enum Field {
    Dword28,
    Dword2c,
    Dword30,
    Dword34,
    Byte(usize),
}

#[derive(Clone, Copy)]
enum KeyAction {
    MessageToggle,
    UnitBroadcast,
    /// 004694B0: +38 = 2 while down, 0 when up.
    Brake38,
    GrenadeDoubleTap,
    /// Stored on a press.
    Store(Field, u32),
    /// The key state as a byte, both ways.
    Level(usize),
    /// 00469790 with this +50 bit.
    MessageKey(u32),
}

fn key_action(address: u32) -> Option<KeyAction> {
    use Field::{Byte, Dword28, Dword2c, Dword30, Dword34};
    use KeyAction::{Level, MessageKey, Store};
    Some(match address {
        0x0046_9450 => KeyAction::MessageToggle,
        0x0046_9480 => KeyAction::UnitBroadcast,
        0x0046_94b0 => KeyAction::Brake38,
        0x0046_94d0 => KeyAction::GrenadeDoubleTap,
        0x0046_9580 => Store(Dword34, u32::MAX),
        0x0046_95a0 => Store(Dword34, 1),
        0x0046_95c0 => Store(Dword34, 2),
        0x0046_95e0 => Store(Dword34, 3),
        0x0046_9600 => Store(Dword34, 4),
        0x0046_9620 => Store(Dword34, 5),
        0x0046_9640 => Store(Dword34, 6),
        0x0046_9660 => Store(Byte(0x3b), 1),
        0x0046_9670 => Store(Byte(0x3c), 1),
        0x0046_9680 => Store(Dword30, 1),
        0x0046_96a0 => Store(Dword30, u32::MAX),
        0x0046_96c0 => Store(Dword28, 1),
        0x0046_96e0 => Store(Dword28, u32::MAX),
        0x0046_9700 => Store(Dword2c, 1),
        0x0046_9720 => Store(Dword2c, u32::MAX),
        0x0046_9740 => Store(Byte(0x3d), 1),
        0x0046_9750 => Store(Byte(0x3f), 1),
        0x0046_9760 => Level(0x40),
        0x0046_9770 => Level(0x41),
        0x0046_9780 => MessageKey(2),
        0x0046_97c0 => MessageKey(4),
        0x0046_97d0 => MessageKey(8),
        0x0046_97e0 => MessageKey(0x10),
        0x0046_97f0 => MessageKey(1),
        0x0046_9800 => Store(Dword28, 2),
        0x0046_9820 => Store(Dword28, 3),
        0x0046_9840 => Store(Dword28, 4),
        0x0046_9860 => Store(Dword28, 5),
        0x0046_9880 => Store(Dword28, 6),
        0x0046_98a0 => Store(Dword28, 7),
        0x0046_98c0 => Store(Dword28, 8),
        0x0046_98e0 => Store(Byte(0x42), 1),
        _ => return None,
    })
}

/// Whether `address` is one of 00469450..004698E0's callbacks.
pub fn is_player_key_callback(address: u32) -> bool {
    key_action(address).is_some()
}

impl ObjectArena {
    /// A control byte store outside the key callbacks, as the computer's
    /// "Image enhancement mode" (004151A0) writes +3F.
    pub fn set_player_control_byte(
        &mut self,
        player: PlayerId,
        offset: usize,
        value: u8,
    ) -> Result<(), PlayerAttachError> {
        self.world_player_mut(player)?
            .controls
            .set_byte(offset, value);
        Ok(())
    }
}

/// 004694D0 subtracts this from the clock to forget a tap.
const DOUBLE_TAP_FORGET: i32 = 1_000_000;

impl ObjectArena {
    /// One of 00469450..004698E0 on `player`. Omitted calls are returned;
    /// their callback does nothing else.
    pub fn player_key_callback(
        &mut self,
        player: PlayerId,
        event: PlayerKeyEvent,
        inputs: PlayerKeyInputs,
    ) -> Result<BTreeSet<PlayerKeyOmission>, PlayerKeyError> {
        let action =
            key_action(event.address).ok_or(PlayerKeyError::NotPlayerKey(event.address))?;
        let pressed = event.value != 0;
        let mut omissions = BTreeSet::new();
        match action {
            KeyAction::MessageToggle => {
                if pressed && inputs.network == 0 {
                    // 00413410(0), then +F4 holds Fire off for 0.5 s.
                    if !self.toggle_player_computer(player, inputs.cheat_word_84)? {
                        omissions.insert(PlayerKeyOmission::MessageToggle);
                    }
                    self.world_player_mut(player)?.dword_f4 = 500_000;
                }
            }
            KeyAction::UnitBroadcast => {
                // 0046AE60: the remote-controlled unit +D8, else the vehicle.
                // Its 0047DD20(0x300, 0x18, 0) runs before the next input
                // event, where the receivers' services are.
                if pressed && inputs.network == 0 {
                    if let Some(unit) = self.player_actor(player)? {
                        self.players
                            .unit_broadcasts
                            .push(crate::object_runtime::UnitBroadcast {
                                player,
                                unit,
                                code: crate::object_runtime::UnitBroadcastCode::Escape,
                            });
                    }
                }
            }
            KeyAction::Brake38 => {
                self.world_player_mut(player)?
                    .controls
                    .set_byte(0x38, if pressed { 2 } else { 0 });
            }
            KeyAction::GrenadeDoubleTap => {
                self.grenade_double_tap(player, pressed, event.timestamp_micros, inputs)?;
            }
            KeyAction::Store(field, value) => {
                if pressed {
                    let controls = &mut self.world_player_mut(player)?.controls;
                    match field {
                        Field::Dword28 => controls.dword_28 = value,
                        Field::Dword2c => controls.dword_2c = value,
                        Field::Dword30 => controls.dword_30 = value,
                        Field::Dword34 => controls.dword_34 = value,
                        Field::Byte(offset) => controls.set_byte(offset, value as u8),
                    }
                }
            }
            KeyAction::Level(offset) => {
                self.world_player_mut(player)?
                    .controls
                    .set_byte(offset, u8::from(pressed));
            }
            KeyAction::MessageKey(bit) => {
                if pressed {
                    self.player_message_key(player, bit, &mut omissions)?;
                } else {
                    let state = self.world_player_mut(player)?;
                    let held = state.dword_50.ok_or(PlayerKeyError::UnknownField {
                        player,
                        offset: 0x50,
                    })?;
                    state.dword_50 = Some(held & !bit);
                }
            }
        }
        Ok(omissions)
    }

    /// 004694D0: the clock goes through __allshr by 10. A press after the
    /// window arms the tap (state 1); a press within it, after a release,
    /// waits (state 2) for a release within the window of the first press,
    /// which sets +39, the double-tapped Grenade.
    fn grenade_double_tap(
        &mut self,
        player: PlayerId,
        pressed: bool,
        timestamp: u64,
        inputs: PlayerKeyInputs,
    ) -> Result<(), PlayerKeyError> {
        let now = ((timestamp as i64) >> 10) as i32;
        let forget = now.wrapping_sub(DOUBLE_TAP_FORGET) as u32;
        let state = self.world_player_mut(player)?;
        let unknown = |offset| PlayerKeyError::UnknownField { player, offset };
        let since = |last: Option<u32>| -> Result<i32, PlayerKeyError> {
            Ok(now.wrapping_sub(last.ok_or(unknown(0x48))? as i32))
        };
        match state.dword_4c.ok_or(unknown(0x4c))? {
            0 if pressed => {
                if since(state.dword_48)? > inputs.double_tap_window {
                    state.dword_4c = Some(1);
                    state.dword_48 = Some(now as u32);
                } else {
                    state.dword_4c = Some(2);
                }
            }
            0 => state.dword_48 = Some(forget),
            1 => {
                if pressed {
                    state.dword_48 = Some(forget);
                }
                state.dword_4c = Some(0);
            }
            2 => {
                if !pressed && since(state.dword_48)? < inputs.double_tap_window {
                    state.controls.set_byte(0x39, 1);
                }
                state.dword_48 = Some(forget);
                state.dword_4c = Some(0);
            }
            _ => {}
        }
        Ok(())
    }

    /// 0046D2E0: a newly held bit runs 0046D240 at once and restarts the
    /// +54 repeat at 250 ms.
    pub(super) fn player_message_key(
        &mut self,
        player: PlayerId,
        bit: u32,
        omissions: &mut BTreeSet<PlayerKeyOmission>,
    ) -> Result<(), PlayerKeyError> {
        let state = self.world_player_mut(player)?;
        let held = state.dword_50.ok_or(PlayerKeyError::UnknownField {
            player,
            offset: 0x50,
        })?;
        state.dword_50 = Some(held | bit);
        if held & bit == 0 {
            self.player_message_keys(player, omissions)?;
            self.world_player_mut(player)?.dword_54 = Some(250);
        }
        Ok(())
    }

    /// 0046D240: each held bit calls 00413950 on the message at +88 — bit
    /// 1 the select (code 4, then the bit clears), 2/4/8/10h the cursor
    /// codes 0..3. Without a shown message the cursor codes do nothing.
    /// Any reached call under a shown message holds Fire off for 0.5 s.
    pub(super) fn player_message_keys(
        &mut self,
        player: PlayerId,
        omissions: &mut BTreeSet<PlayerKeyOmission>,
    ) -> Result<(), PlayerKeyError> {
        let held = |arena: &Self| -> Result<u32, PlayerKeyError> {
            arena
                .world_player(player)?
                .dword_50
                .ok_or(PlayerKeyError::UnknownField {
                    player,
                    offset: 0x50,
                })
        };
        let message_error = |error: super::PlayerMessageError| PlayerKeyError::Message(error);
        let mut reached = false;
        if held(self)? & 1 != 0 {
            self.player_message_command(player, 4, omissions)
                .map_err(message_error)?;
            let bits = held(self)?;
            self.world_player_mut(player)?.dword_50 = Some(bits & !1);
            reached = true;
        }
        for (bit, code) in [(2, 0), (4, 1), (8, 2), (0x10, 3)] {
            if held(self)? & bit != 0 {
                self.player_message_command(player, code, omissions)
                    .map_err(message_error)?;
                reached = true;
            }
        }
        // 0046D2C0 reads the shown byte after the calls.
        let shown = self.world_player(player)?.message_byte_00() != 0;
        if reached && shown {
            self.world_player_mut(player)?.dword_f4 = 500_000;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
