//! Family virtual+B4 00428040, embedded target admission 00482E30,
//! nearby notification 004281B0 and family virtual+D4 004282C0.
//!
//! Target entries and reached actor writes are owned. The actual virtual+100
//! callback behind 0043F3D0 remains required, alongside shared reaction effects.

use thiserror::Error;

use super::{
    ActorDamageCallback, ActorDamageReactionError, ActorDamageReactionHost, Family428650Class,
    Family429e20Class,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        mark_flag_change, ActorConstructionError, ActorHandle, ObjectArena, ObjectError, ObjectId,
    },
};

/// The three native arguments of 00482E30/004282C0/004281B0.
/// The second argument is stored at entry+08, the third at entry+04.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorFamilyTargetRequest {
    pub source: ActorHandle,
    pub word_08: u32,
    pub word_04: u32,
}

pub trait ActorFamilyDamageHost: ActorDamageReactionHost {
    /// Packet+2 at 004280AB (only for zero damage) or 0042811D (suffix).
    /// The caller retains the original packet identity/context, including
    /// changes made by earlier callbacks. Copied initial bytes alone do not
    /// admit an arbitrary mutable packet alias.
    fn packet_channel(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<u8, Self::Error>;
    /// Virtual+114=0043F3D0 calls the current virtual+100 with two zero
    /// arguments. This must execute the class's real target-scoring/selection owner;
    /// a missing owner must fail after the preceding target-table writes.
    fn select_target_100(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorFamilyDamageOperation {
    PacketChannel,
    SelectTarget100,
    PlacementDeath,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorFamilyTargetError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {actor:?} target count {count:#x} reaches beyond its four native entries")]
    TargetCount { actor: ObjectId, count: u32 },
    #[error("family target broadcast actor {0:?} has unwritten position WORDs")]
    MissingPosition(ObjectId),
    #[error("family target broadcast lost its current native list node {0:?}")]
    RemovedCurrentNode(ObjectId),
}

#[derive(Debug, Error)]
pub enum ActorFamilyDamageError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Actor(#[from] ActorConstructionError),
    #[error(transparent)]
    Target(#[from] ActorFamilyTargetError),
    #[error("shared/derived damage reaction failed")]
    Reaction(ActorDamageReactionError<E>),
    #[error("family damage host operation {operation:?} failed")]
    Host {
        operation: ActorFamilyDamageOperation,
        error: E,
    },
}

impl ObjectArena {
    ///0047C6E0's source component virtual+108 under the actual final tables:
    ///Family429E20 binds 00429C70; Base, Selector34 and Selector35 bind RET4
    ///00466D40. Every damage host reaches this one binding.
    pub fn source_component_108(
        &mut self,
        component: ObjectId,
        target: ObjectId,
    ) -> Result<(), ObjectError> {
        match &self.family428650_actor(component)?.class {
            Family428650Class::Family429e20(_) => {
                self.notify_family_projectile_hit(component, target)
            }
            Family428650Class::Base
            | Family428650Class::Selector34(_)
            | Family428650Class::Selector35(_) => Ok(()),
        }
    }

    ///00429C70 source virtual+108. The hit actor handle is compared with the
    ///retained target's raw+04; resolving that target's referent is different.
    pub fn notify_family_projectile_hit(
        &mut self,
        source: ObjectId,
        target: ObjectId,
    ) -> Result<(), ObjectError> {
        if self.family428650_actor(source)?.helper_218.is_none() {
            return Ok(());
        }
        let Some(reference) = self.derived_actor(source)?.target_148 else {
            return Ok(());
        };
        let expected = self.native_target(reference)?.handle_04();
        if self.actor(target)?.handle != expected {
            return Ok(());
        }
        self.family429e20_actor_mut(source)?.word_238 = 3000;
        self.family428650_actor_mut(source)?
            .helper_218
            .as_mut()
            .expect("the callback has no intervening helper mutation")
            .words_00_04[1] = Some(0);
        Ok(())
    }

    ///00428140 family virtual+AC. The lifecycle caller has already published
    ///state1/health0. Retaliation and the second placement-death call are real
    ///suffixes, even if 0047D670 previously recorded the same placement.
    pub fn retire_family_actor<H: super::ActorStateChangeHost>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        host: &mut H,
    ) -> Result<(), ActorFamilyDamageError<H::Error>> {
        self.family428650_actor(actor)?;
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags & !0x1000_0000);
        if let Some(resolved) = self.resolve_actor_handle(source)? {
            if self.actor(resolved)?.word_b4 != self.actor(actor)?.word_b4 {
                self.broadcast_family_damage_target(
                    actor,
                    ActorFamilyTargetRequest {
                        source,
                        word_08: 0x0f00_0000,
                        word_04: 0x12c0_0000,
                    },
                )?;
            }
        }
        let index = self.actor(actor)?.byte_78 as i8;
        host.effect(
            self,
            super::ActorStateChangeEffect::RecordPlacementDeath { index },
        )
        .map_err(|error| ActorFamilyDamageError::Host {
            operation: ActorFamilyDamageOperation::PlacementDeath,
            error,
        })
    }

    /// Complete 00428040. The original damage argument remains cached across
    /// shared reaction; every suffix field is read from the reached live actor.
    pub fn react_family_actor_damage<H: ActorFamilyDamageHost>(
        &mut self,
        actor: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorFamilyDamageError<H::Error>> {
        self.family428650_actor_mut(actor)?.word_22e |= 1;
        self.react_derived_actor_damage(actor, callback, rng, host)
            .map_err(ActorFamilyDamageError::Reaction)?;
        self.activate_family_damage_prefix(actor)?;
        self.family428650_actor_mut(actor)?.word_22e |= 1;

        if (callback.adjusted_damage != 0
            || host
                .packet_channel(self, actor, callback)
                .map_err(|error| ActorFamilyDamageError::Host {
                    operation: ActorFamilyDamageOperation::PacketChannel,
                    error,
                })?
                != 1)
            && self.actor(actor)?.handle != callback.source
        {
            if let Some(source) = self.resolve_actor_handle(callback.source)? {
                if self.actor(source)?.word_b4 == self.actor(actor)?.word_b4 {
                    // 004280D1 jumps directly to RET, bypassing channel8 too.
                    return Ok(());
                }
            }
            let word_04 = if callback.adjusted_damage == 0 {
                1
            } else {
                0x0780_0000
            };
            let admitted = self.record_family_damage_target(
                actor,
                ActorFamilyTargetRequest {
                    source: callback.source,
                    word_08: 0x0780_0000,
                    word_04,
                },
            )?;
            let broadcast = if admitted {
                // Base428650/429E20 and selector12 have the actual RET-only
                // 00401310. Other admitted family classes use 0043F3D0.
                if self.family_damage_select_target_100(actor)? {
                    host.select_target_100(self, actor, rng).map_err(|error| {
                        ActorFamilyDamageError::Host {
                            operation: ActorFamilyDamageOperation::SelectTarget100,
                            error,
                        }
                    })?;
                }
                true
            } else {
                name_entry_rng_next(rng) & 3 == 0
            };
            if broadcast {
                self.broadcast_family_damage_target(
                    actor,
                    ActorFamilyTargetRequest {
                        source: callback.source,
                        word_08: 0x03c0_0000,
                        word_04: word_04 >> 1,
                    },
                )?;
            }
        }
        let channel = host
            .packet_channel(self, actor, callback)
            .map_err(|error| ActorFamilyDamageError::Host {
                operation: ActorFamilyDamageOperation::PacketChannel,
                error,
            })?;
        if channel == 8 && (self.derived_actor(actor)?.word_17a as i16) < 5000 {
            self.derived_actor_mut(actor)?.word_17a = 5000;
        }
        Ok(())
    }

    /// 00482E30 returns true only for a new slot or stronger replacement.
    /// Matching handles update unsigned maxima but return false even when
    /// those writes increase their fields. Unknown allocation holes are unused.
    pub fn record_family_damage_target(
        &mut self,
        actor: ObjectId,
        request: ActorFamilyTargetRequest,
    ) -> Result<bool, ActorFamilyTargetError> {
        let state = self.family428650_actor_mut(actor)?;
        let count = state.count_1a0;
        let mut index = 0usize;
        while (index as i32) < count as i32 {
            let entry = state
                .entries_1a4
                .get_mut(index)
                .ok_or(ActorFamilyTargetError::TargetCount { actor, count })?;
            if entry.handle == request.source {
                entry.word_08 = entry.word_08.max(request.word_08);
                entry.word_04 = entry.word_04.max(request.word_04);
                return Ok(false);
            }
            index += 1;
        }
        if index < 4 {
            let entry = &mut state.entries_1a4[index];
            entry.handle = request.source;
            entry.word_08 = request.word_08;
            entry.word_04 = request.word_04;
            // Signed-nonpositive counts also write slot0, then increment the
            // original DWORD. They are not normalized to a fabricated zero.
            state.count_1a0 = count.wrapping_add(1);
            return Ok(true);
        }
        let mut minimum = u32::MAX;
        let mut selected = 0;
        for (index, entry) in state.entries_1a4.iter().enumerate() {
            let score = target_score(entry.word_08, entry.word_04);
            if score < minimum {
                minimum = score;
                selected = index;
            }
        }
        if target_score(request.word_08, request.word_04) <= minimum {
            return Ok(false);
        }
        let entry = &mut state.entries_1a4[selected];
        entry.handle = request.source;
        entry.word_08 = request.word_08;
        entry.word_04 = request.word_04;
        Ok(true)
    }

    /// Family virtual+D4=004282C0. Activation precedes target admission, so a
    /// reached malformed-table error retains the negative-delay/flag writes.
    pub fn notify_family_damage_target(
        &mut self,
        actor: ObjectId,
        request: ActorFamilyTargetRequest,
    ) -> Result<(), ActorFamilyTargetError> {
        self.activate_family_damage_prefix(actor)?;
        self.record_family_damage_target(actor, request)?;
        Ok(())
    }

    /// Complete 004281B0 on the admitted +58/+CC/D4 vtable domain. Radius,
    /// classification and resource WORD +30 (`004281D1`, compared at
    /// `0042820F`) are cached once; positions, handles and current-node
    /// continuation remain live at their actual consumers.
    pub fn broadcast_family_damage_target(
        &mut self,
        actor: ObjectId,
        request: ActorFamilyTargetRequest,
    ) -> Result<(), ActorFamilyTargetError> {
        let radius = self.derived_actor(actor)?.word_164 as u32;
        let square_limit = radius.wrapping_mul(radius) >> 4;
        let classification = self.actor(actor)?.word_b4;
        let resource = self.object(actor)?.base.resource_word;
        let mut index = 0;
        while let Some(&candidate) = self.actor_list().get(index) {
            // Retained constructor prefixes are not notification recipients.
            if !self.construction_is_stopped(candidate)?
                && self.actor(candidate)?.word_b4 == classification
                && self.object(candidate)?.base.resource_word == resource
                && self.object(candidate)?.base.flags & 0xa0 == 0x20
            {
                let position = self
                    .object(candidate)?
                    .base
                    .position_words
                    .ok_or(ActorFamilyTargetError::MissingPosition(candidate))?;
                let origin = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ActorFamilyTargetError::MissingPosition(actor))?;
                let square = position
                    .into_iter()
                    .zip(origin)
                    .fold(0u32, |sum, (value, origin)| {
                        let delta = i32::from(value.wrapping_sub(origin) as i16);
                        sum.wrapping_add(delta.wrapping_mul(delta) as u32)
                    });
                if square < square_limit
                    && self.actor(candidate)?.handle != self.actor(actor)?.handle
                {
                    if let Some(component) = self.actor_damage_source_component(candidate)? {
                        self.notify_family_damage_target(component, request)?;
                    }
                }
            }
            index = self
                .actor_list()
                .iter()
                .position(|&value| value == candidate)
                .ok_or(ActorFamilyTargetError::RemovedCurrentNode(candidate))?
                + 1;
        }
        Ok(())
    }

    fn activate_family_damage_prefix(&mut self, actor: ObjectId) -> Result<(), ObjectError> {
        let delay = self.derived_actor(actor)?.word_144;
        if delay < 0 {
            self.derived_actor_mut(actor)?.word_144 = delay.wrapping_mul(-1000);
        }
        let base = &mut self.owned_mut(actor)?.state.base;
        let flags = base.flags & 0xfb9f_ffff;
        mark_flag_change(base, flags);
        Ok(())
    }

    fn family_damage_select_target_100(&self, actor: ObjectId) -> Result<bool, ObjectError> {
        Ok(match &self.family428650_actor(actor)?.class {
            Family428650Class::Base => false,
            Family428650Class::Family429e20(state) => {
                matches!(state.class, Family429e20Class::Family4236d0(_))
            }
            Family428650Class::Selector34(_) | Family428650Class::Selector35(_) => true,
        })
    }
}

/// 00482EC5..00482F2E's unsigned replacement score. A nonzero +08 below
/// 65536 receives the same high priority group as 65536; +04 contributes
/// only its high WORD. Strict comparisons retain the earliest minimum tie.
fn target_score(word_08: u32, word_04: u32) -> u32 {
    let upper = if word_08 == 0 {
        0
    } else {
        (word_08 & 0xffff_0000).max(0x1_0000)
    };
    upper | (word_04 >> 16)
}

#[cfg(test)]
#[path = "damage_family_tests.rs"]
mod tests;
