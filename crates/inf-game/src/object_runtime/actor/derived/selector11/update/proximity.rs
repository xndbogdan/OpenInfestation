//! 00401920: timed damage to the first player's primary actor within 500
//! units, then 0047DD80's range-gated virtual +98 notice with code 0x17.

use inf_formats::native_basis::isqrt_48efc0;

use super::{Selector11UpdateError, Selector11UpdateHost};
use crate::object_runtime::{
    ActorDamagePacket, ActorTimedDamageRequest, ObjectArena, ObjectError, ObjectId,
};

impl ObjectArena {
    pub(super) fn selector11_proximity<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        // 00488AA0, then 0046B030's first-player primary +D4 handle.
        if self.first_player().is_none() {
            return Ok(());
        }
        let Some(player) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let squared = self.selector11_squared_distance(actor, player)?;
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        } as i32;
        if distance >= 500 {
            return Ok(());
        }
        let request = ActorTimedDamageRequest {
            // Byte 3 is the reused stack slot that held this actor's pointer.
            packet: ActorDamagePacket {
                strength: (600 - distance).wrapping_shl(3) as i16,
                channel: 3,
                byte_03: None,
            },
            source: self.actor(actor)?.handle,
            microseconds: delta as i32,
            period_microseconds: 2_000_000,
        };
        host.player_damage(self, player, request)
            .map_err(|error| Selector11UpdateError::Host {
                address: 0x0040_19c1,
                error,
            })?;
        // 0047DD80 rereads both positions and compares the unsigned squares.
        let squared = self.selector11_squared_distance(actor, player)?;
        if squared > 500 * 500 {
            return Ok(());
        }
        host.player_notice(self, player, actor, squared)
            .map_err(|error| Selector11UpdateError::Host {
                address: 0x0047_dde2,
                error,
            })
    }

    fn selector11_squared_distance(
        &self,
        actor: ObjectId,
        other: ObjectId,
    ) -> Result<u32, ObjectError> {
        let position = |object: ObjectId| {
            self.object(object)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(object))
        };
        let (own, theirs) = (position(actor)?, position(other)?);
        let delta: [i32; 3] =
            std::array::from_fn(|axis| i32::from(own[axis].wrapping_sub(theirs[axis]) as i16));
        Ok(delta.into_iter().fold(0u32, |sum, value| {
            sum.wrapping_add(value.wrapping_mul(value) as u32)
        }))
    }
}
