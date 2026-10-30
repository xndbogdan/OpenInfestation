//! Shared damage reaction 0047CBC0 and derived activation 004283E0.
//! Health/status writes are concrete; retaliation, lifecycle and network
//! effects require their actual services and retain preceding writes on error.

use thiserror::Error;

use super::damage::ActorDamageCallback;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{mark_flag_change, ActorHandle, ObjectArena, ObjectError, ObjectId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorDamageReactionEffect {
    ///00492500(1,0x12): packet data, target handle and source handle.
    /// The caller's service must retain the incoming packet's identity.
    RemoteDamage {
        callback: ActorDamageCallback,
        target: ActorHandle,
    },
    ///0047C840 after the reached+80 cooldown write. Its actor scan is required.
    Retaliation { source: ActorHandle },
    ///0047CE90 from a positive channel2 response and nonzero virtual+68.
    StatusRetirement,
    ///0047D670(packet,source,1). The service must retain incoming packet identity.
    Lethal { callback: ActorDamageCallback },
    ///0045DA50's fully written four-byte stack packet.
    NetworkDamageNotice {
        kind: u16,
        local_player: u8,
        source_owner: u8,
    },
    ///0045D8F0. The first WORD retains the original incoming damage bits;
    /// it is not the difficulty/bonus-adjusted local damage.
    NetworkDeathNotice {
        incoming_damage_low_word: u16,
        local_player: u8,
        source_owner: u8,
    },
}

pub trait ActorDamageReactionHost {
    type Error;
    /// Identity returned by virtual+64, retained across the clock query.
    type RecentSource: Copy;

    /// Actual0054E894 at47CD05, after status and states2/3 guards and
    /// before reading reaction-radius bytes or consuming the animation clock.
    fn process_null_handle(&self) -> Option<ActorHandle>;

    fn network_active(&mut self) -> Result<bool, Self::Error>;
    ///0048E7C0 signed low AX, consumed only by the ordinary offline arm.
    fn difficulty_multiplier(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<i16, Self::Error>;
    ///0046B090's optional player and its six signed bytes+13C..141.
    /// This is reached only for a mode1-marked source in that offline arm.
    fn source_bonuses(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error>;
    fn animation_tick(&mut self) -> Result<u32, Self::Error>;
    fn network_role(&mut self) -> Result<i32, Self::Error>;
    fn local_player_byte(&mut self) -> Result<u8, Self::Error>;
    /// Virtual+68 at0047CAEE, before positive channel2 status writes.
    fn excludes_status(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<bool, Self::Error>;
    /// Virtual+64 at0047CE45; this query must not snapshot the later fields.
    fn recent_damage_source(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<Self::RecentSource>, Self::Error>;
    /// Source+5A8 at0047CE52, after the global animation-clock read.
    /// The value uses005471C0's50Hz animation ticks, not microseconds.
    fn recent_source_tick(
        &mut self,
        arena: &mut ObjectArena,
        source: Self::RecentSource,
    ) -> Result<u32, Self::Error>;
    /// Source+5A6 at0047CE62, reached only when the unsigned delta is below200.
    /// This is the HIGH WORD of the source's retained actor handle.
    fn recent_source_owner_word(
        &mut self,
        arena: &mut ObjectArena,
        source: Self::RecentSource,
    ) -> Result<u16, Self::Error>;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: ActorDamageReactionEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorDamageReactionError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} damage reaction requires process0054E894 at0047CD05")]
    MissingProcessNullHandle(ObjectId),
    #[error("damage reaction host failed: {0:?}")]
    Host(E),
    #[error("damage bonus channel{0} is outside the native ten-word mapping")]
    BonusChannel(u8),
    #[error("actor{0:?} requires maximum health at the reached healing read")]
    MissingMaximumHealth(ObjectId),
}

impl ObjectArena {
    ///004283E0, not the extra 00428040 behavior/reaction suffix.
    pub fn react_derived_actor_damage<H: ActorDamageReactionHost>(
        &mut self,
        actor: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorDamageReactionError<H::Error>> {
        let delay = self.derived_actor(actor)?.word_144;
        if delay < 0 {
            self.derived_actor_mut(actor)?.word_144 = delay.wrapping_mul(-1000);
        }
        let base = &mut self.owned_mut(actor)?.state.base;
        let flags = base.flags & 0xfb9f_ffff;
        mark_flag_change(base, flags);
        if callback.adjusted_damage != 0 && matches!(self.actor(actor)?.word_c4, 2 | 3) {
            let scaled = callback.adjusted_damage.wrapping_mul(512).min(0x7fff);
            let state = self.actor_mut(actor)?;
            state.word_ca = Some(55);
            state.word_c8 = ((i32::from(scaled as i16) * 0x7a12) >> 15) as u16;
            self.react_actor_damage(actor, callback, rng, host)?;
        }
        Ok(())
    }

    /// `0040E030`, the +B4 of the 0040D800 family's factory (selector 15),
    /// gate (30) and terrain bridge (37): nothing for zero damage or at a
    /// lifecycle of 1 or less (signed); otherwise the hit colour 55 at +CA
    /// and +C8 from the damage, as `004283E0` writes them, then `0047CBC0`.
    /// Unlike `004283E0` it neither rescales +144 nor clears flags.
    pub fn react_family40d800_damage<H: ActorDamageReactionHost>(
        &mut self,
        actor: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorDamageReactionError<H::Error>> {
        if callback.adjusted_damage == 0 || self.actor(actor)?.word_c4 as i32 <= 1 {
            return Ok(());
        }
        let scaled = callback.adjusted_damage.wrapping_mul(512).min(0x7fff);
        let state = self.actor_mut(actor)?;
        state.word_ca = Some(55);
        state.word_c8 = ((i32::from(scaled as i16) * 0x7a12) >> 15) as u16;
        self.react_actor_damage(actor, callback, rng, host)
    }

    ///0047CBC0. Every host failure leaves the reached arena and RNG prefix.
    pub fn react_actor_damage<H: ActorDamageReactionHost>(
        &mut self,
        actor: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorDamageReactionError<H::Error>> {
        let flags = self.object(actor)?.base.flags;
        if flags & 0x8000_0000 != 0 {
            if callback.adjusted_damage != 0 {
                let target = self.actor(actor)?.handle;
                host.effect(
                    self,
                    actor,
                    ActorDamageReactionEffect::RemoteDamage { callback, target },
                    rng,
                )
                .map_err(ActorDamageReactionError::Host)?;
            }
            return Ok(());
        }
        let mut damage = callback.adjusted_damage;
        if flags & 0x4000 == 0
            && !host
                .network_active()
                .map_err(ActorDamageReactionError::Host)?
        {
            let multiplier = host
                .difficulty_multiplier(self, actor)
                .map_err(ActorDamageReactionError::Host)?;
            damage = damage.wrapping_mul(i32::from(multiplier)) >> 8;
            if callback.source.marked() {
                if let Some(bonuses) = host
                    .source_bonuses(self, callback.source)
                    .map_err(ActorDamageReactionError::Host)?
                {
                    const MAP: [Option<usize>; 10] = [
                        None,
                        None,
                        Some(3),
                        None,
                        Some(4),
                        None,
                        Some(0),
                        Some(1),
                        Some(2),
                        Some(5),
                    ];
                    let index = MAP.get(usize::from(callback.packet.channel)).ok_or(
                        ActorDamageReactionError::BonusChannel(callback.packet.channel),
                    )?;
                    let bonus = index.map_or(0, |index| i32::from(bonuses[index]));
                    damage = damage.wrapping_add(bonus.wrapping_mul(damage) / 25);
                }
            }
        }
        if callback.packet.channel == 2 {
            self.apply_channel_two_status(actor, damage, rng, host)?;
            damage = 0;
        }
        if !matches!(self.actor(actor)?.word_c4, 2 | 3) {
            return Ok(());
        }
        let null = host
            .process_null_handle()
            .ok_or(ActorDamageReactionError::MissingProcessNullHandle(actor))?;
        if callback.source != null
            && (self.actor(actor)?.byte_7c != 0 || self.actor(actor)?.byte_7d != 0)
        {
            let now = host
                .animation_tick()
                .map_err(ActorDamageReactionError::Host)?;
            if (self.actor(actor)?.word_80 as i32) < now as i32 {
                let draw = name_entry_rng_next(rng);
                //47CD35 rereads the global clock after the process RNG call.
                let now = host
                    .animation_tick()
                    .map_err(ActorDamageReactionError::Host)?;
                self.actor_mut(actor)?.word_80 = now.wrapping_add(draw & 0x7f).wrapping_add(150);
                host.effect(
                    self,
                    actor,
                    ActorDamageReactionEffect::Retaliation {
                        source: callback.source,
                    },
                    rng,
                )
                .map_err(ActorDamageReactionError::Host)?;
            }
        }
        if damage < 0 {
            let health = i32::from(self.actor(actor)?.health).wrapping_sub(damage);
            let maximum = self
                .actor(actor)?
                .maximum_health
                .ok_or(ActorDamageReactionError::MissingMaximumHealth(actor))?;
            self.actor_mut(actor)?.health = health.min(i32::from(maximum)) as u16;
            return Ok(());
        }
        if damage < i32::from(self.actor(actor)?.health) {
            self.owned_mut(actor)?.state.base.dirty_word |= 0x2000;
            self.actor_mut(actor)?.health = self.actor(actor)?.health.wrapping_sub(damage as u16);
            if self.object(actor)?.base.flags & 0x10 != 0
                && host
                    .network_active()
                    .map_err(ActorDamageReactionError::Host)?
                && callback.source.marked()
                && host
                    .network_role()
                    .map_err(ActorDamageReactionError::Host)?
                    != 2
            {
                let kind = if matches!(callback.packet.channel, 6 | 8) {
                    1
                } else {
                    10
                };
                let local_player = host
                    .local_player_byte()
                    .map_err(ActorDamageReactionError::Host)?;
                host.effect(
                    self,
                    actor,
                    ActorDamageReactionEffect::NetworkDamageNotice {
                        kind,
                        local_player,
                        source_owner: callback.source.owner(),
                    },
                    rng,
                )
                .map_err(ActorDamageReactionError::Host)?;
            }
        } else {
            host.effect(
                self,
                actor,
                ActorDamageReactionEffect::Lethal { callback },
                rng,
            )
            .map_err(ActorDamageReactionError::Host)?;
            if self.object(actor)?.base.flags & 0x10 != 0
                && host
                    .network_active()
                    .map_err(ActorDamageReactionError::Host)?
            {
                let source_owner = if callback.source.marked() {
                    callback.source.owner()
                } else if let Some(recent) = host
                    .recent_damage_source(self, actor)
                    .map_err(ActorDamageReactionError::Host)?
                {
                    let now = host
                        .animation_tick()
                        .map_err(ActorDamageReactionError::Host)?;
                    let source_tick = host
                        .recent_source_tick(self, recent)
                        .map_err(ActorDamageReactionError::Host)?;
                    if now.wrapping_sub(source_tick) < 200 {
                        let owner_word = host
                            .recent_source_owner_word(self, recent)
                            .map_err(ActorDamageReactionError::Host)?;
                        (owner_word >> 11) as u8
                    } else {
                        0
                    }
                } else {
                    0
                };
                let local_player = host
                    .local_player_byte()
                    .map_err(ActorDamageReactionError::Host)?;
                host.effect(
                    self,
                    actor,
                    ActorDamageReactionEffect::NetworkDeathNotice {
                        incoming_damage_low_word: callback.adjusted_damage as u16,
                        local_player,
                        source_owner,
                    },
                    rng,
                )
                .map_err(ActorDamageReactionError::Host)?;
            }
        }
        Ok(())
    }

    ///0047CAE0. The consumer ignores its register return; only writes/effects
    /// are represented, including nonpositive healing/status wraparound.
    pub fn apply_channel_two_status<H: ActorDamageReactionHost>(
        &mut self,
        actor: ObjectId,
        damage: i32,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorDamageReactionError<H::Error>> {
        if damage <= 0 {
            self.reduce_channel_two_status(actor, damage.unsigned_abs())?;
        } else if host
            .excludes_status(self, actor)
            .map_err(ActorDamageReactionError::Host)?
        {
            host.effect(
                self,
                actor,
                ActorDamageReactionEffect::StatusRetirement,
                rng,
            )
            .map_err(ActorDamageReactionError::Host)?;
        } else {
            let amount = damage / 5;
            let state = self.actor_mut(actor)?;
            if state.word_ce != 0 {
                state.word_cc = i32::from(state.word_cc).max(amount) as u16;
                state.word_ce = state.word_ce.wrapping_add(damage as u16);
            } else {
                state.word_cc = amount.max(100) as u16;
                state.word_ce = damage as u16;
            }
        }
        Ok(())
    }

    ///0047CAE0's nonpositive arm, also reached by healing pickup excess.
    /// Amount is the magnitude of the negative input; its LOWWORD arithmetic
    /// deliberately wraps rather than saturating at zero. No callback or RNG
    /// service is reached on this arm.
    pub(crate) fn reduce_channel_two_status(
        &mut self,
        actor: ObjectId,
        amount: u32,
    ) -> Result<(), ObjectError> {
        let state = self.actor_mut(actor)?;
        if state.word_ce != 0 {
            state.word_ce = state.word_ce.wrapping_sub((amount as u16).wrapping_mul(2));
        }
        if state.word_ce == 0 {
            state.word_cc = state.word_cc.min(500);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "damage_reaction_tests.rs"]
mod tests;
