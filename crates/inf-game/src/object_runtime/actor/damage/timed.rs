//! 0047C7C0: signed Q31 time ratio, resistance, then conditional virtual+B4.
//! It does not execute instantaneous 0047C6E0's task/source/statistics suffix.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorTimedDamageRequest {
    pub packet: ActorDamagePacket,
    pub source: ActorHandle,
    pub microseconds: i32,
    pub period_microseconds: i32,
}

pub trait ActorTimedDamageHost {
    type Error;
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorTimedDamageError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {actor:?} damage channel {channel} lies outside its retained+A4 table")]
    UnsupportedChannel { actor: ObjectId, channel: u8 },
    #[error("0047C801 signed division overflows: ({microseconds} <<31) / {period_microseconds}")]
    Division {
        microseconds: i32,
        period_microseconds: i32,
    },
    #[error("timed damage virtual+B4 failed: {0}")]
    Host(E),
}

impl ObjectArena {
    pub fn apply_actor_timed_damage<H: ActorTimedDamageHost>(
        &mut self,
        actor: ObjectId,
        request: ActorTimedDamageRequest,
        host: &mut H,
    ) -> Result<(), ActorTimedDamageError<H::Error>> {
        let Some(modifier) = timed_modifier(request.microseconds, request.period_microseconds)?
        else {
            return Ok(());
        };
        let responses = self.actor(actor)?.damage_responses();
        let &(threshold, multiplier) = responses.get(usize::from(request.packet.channel)).ok_or(
            ActorTimedDamageError::UnsupportedChannel {
                actor,
                channel: request.packet.channel,
            },
        )?;
        let damage = resisted_effect_damage(
            request.packet.strength,
            DamageResistance {
                threshold,
                multiplier,
            },
            modifier,
        );
        if damage != 0 {
            host.virtual_b4(
                self,
                actor,
                ActorDamageCallback {
                    adjusted_damage: damage,
                    packet: request.packet,
                    source: request.source,
                },
            )
            .map_err(ActorTimedDamageError::Host)?;
        }
        Ok(())
    }
}

fn timed_modifier<E>(elapsed: i32, period: i32) -> Result<Option<i32>, ActorTimedDamageError<E>> {
    let modifier = if period == 0 {
        // Native zero period deliberately passes modifier0 to42A8F0,
        // whose modifier0 branch means the full resisted amount.
        0
    } else {
        // Native XOR/SUB absolute values wrap at I32MIN and are compared
        // as signedDWORDs. Do not substitute unsigned absolute values.
        let ratio = if elapsed.wrapping_abs() >= period.wrapping_abs() {
            ((elapsed ^ period) >> 31) ^ i32::MAX
        } else {
            i32::try_from((i64::from(elapsed) << 31) / i64::from(period)).map_err(|_| {
                ActorTimedDamageError::Division {
                    microseconds: elapsed,
                    period_microseconds: period,
                }
            })?
        };
        if ratio == 0 {
            return Ok(None);
        }
        ratio
    };
    Ok(Some(modifier))
}

#[cfg(test)]
mod tests;
