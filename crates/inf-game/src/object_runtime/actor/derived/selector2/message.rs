//! The vehicle's +98, 00420BE0: the messages other actors send it. It first
//! looks up the player whose unit the vehicle is (0046B090 of +84), then
//! acts on six codes:
//!
//! - 4: with a player, H0 string `argument` on the player's message
//!   (004135D0(text, 0)).
//! - 5: a 400-strength channel-7 hit from the process NULL (0047C6E0).
//! - 0xB/0xC: with a player, the radar jam. Online, a hit from the player's
//!   own actor sets the map's jitter to 0x3FFF; otherwise a nonzero range
//!   `argument` sets it to (range - distance) * 0xFFFF / range.
//! - 0xD: +200 = 1,000,000.
//! - 0x11..0x13: online with a player, the grants 0xAA, 0xAB and 0xAC
//!   (0046B2B0).
//! - 0x17: the second HUD notice +20B (H0 string 0x16D), which a turret or
//!   target near the vehicle sends.
//!
//! Every code returns 0.

use thiserror::Error;

use inf_formats::native_basis::isqrt_48efc0;

use crate::{
    object_runtime::{
        ActorDamagePacket, ActorHandle, DerivedActorError, ObjectArena, ObjectError, ObjectId,
        PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest, PlayerId,
    },
    world_map::WorldMapJitterWrite,
};

/// One +98 call from 0047DAF0 or 0047DD80.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorMessage {
    /// The sender, NULL for a zero-radius broadcast's.
    pub sender: Option<ObjectId>,
    /// The unsigned squared distance the sender measured.
    pub distance_squared: u32,
    pub code: i32,
    pub argument: u32,
}

/// What 00420BE0 leaves to the world's services.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleMessageEffect {
    None,
    /// Code 4: 004135D0(H0 string, 0) on the player's message.
    PlayerText {
        player: PlayerId,
        string: u16,
    },
    /// Code 5: 0047C6E0 on the vehicle, from the process NULL.
    Damage {
        packet: ActorDamagePacket,
        source: ActorHandle,
    },
    /// Codes 0xB/0xC: the map owner's jitter, G+1B4 +24.
    MapJitter(WorldMapJitterWrite),
}

#[derive(Debug, Error)]
pub enum VehicleMessageError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Lookup(#[from] PlayerActorLookupError),
    /// Codes 0x11..0x13 online grant through 0046B2B0, outside the port's
    /// offline session.
    #[error("vehicle message {0:#x} grants online (0046B2B0)")]
    OnlineGrant(i32),
}

impl ObjectArena {
    /// 00420BE0, the vehicle's +98. `network_active` is 0055F448.
    pub fn vehicle_message(
        &mut self,
        vehicle: ObjectId,
        message: ActorMessage,
        context: PlayerActorLookupContext<'_>,
        network_active: bool,
    ) -> Result<VehicleMessageEffect, VehicleMessageError> {
        self.selector2_actor(vehicle)?;
        let handle = self.actor(vehicle)?.handle;
        let player =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle }, context)?;
        let effect = match (message.code, player) {
            (4, Some(player)) => VehicleMessageEffect::PlayerText {
                player,
                string: message.argument as u16,
            },
            (5, _) => {
                let null = context
                    .process
                    .and_then(|process| process.process_null_handle)
                    .ok_or(PlayerActorLookupError::MissingProcessNullHandle)?;
                // The packet is built in the argument's stack slot, so its
                // fourth byte is the argument's.
                VehicleMessageEffect::Damage {
                    packet: ActorDamagePacket {
                        strength: 400,
                        channel: 7,
                        byte_03: Some((message.argument >> 24) as u8),
                    },
                    source: null,
                }
            }
            (0xb | 0xc, Some(_)) => {
                let owned = match message.sender {
                    Some(sender) if network_active => self.actor(sender)?.owner_handle == handle,
                    _ => false,
                };
                let range = message.argument as i32;
                if owned {
                    VehicleMessageEffect::MapJitter(WorldMapJitterWrite::OwnedNetworkImpact420BE0)
                } else if range != 0 {
                    VehicleMessageEffect::MapJitter(WorldMapJitterWrite::RadialImpact420BE0 {
                        range,
                        distance: isqrt_48efc0(message.distance_squared as i32) as i32,
                    })
                } else {
                    VehicleMessageEffect::None
                }
            }
            (0xd, _) => {
                self.selector2_actor_mut(vehicle)?.dwords_200_204[0] = 1_000_000;
                VehicleMessageEffect::None
            }
            (0x11..=0x13, Some(_)) if network_active => {
                return Err(VehicleMessageError::OnlineGrant(message.code))
            }
            (0x17, _) => {
                self.selector2_actor_mut(vehicle)?.bytes_20a_20c[1] = 1;
                VehicleMessageEffect::None
            }
            _ => VehicleMessageEffect::None,
        };
        Ok(effect)
    }
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
