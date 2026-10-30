//! Shared nearby-actor retaliation 0047C840 and family reaction 00429340.
//! Classification/radii are cached, positions and handles remain live, and
//! the reached family +D4 uses its concrete target-table owner.

use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{mark_flag_change, ActorHandle, ObjectArena, ObjectError, ObjectId},
};

use super::{ActorFamilyTargetError, ActorFamilyTargetRequest};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorRetaliationError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] ActorFamilyTargetError),
    #[error("actor {0:?} retaliation reached unwritten position WORDs")]
    MissingPosition(ObjectId),
    #[error("actor {0:?} family retaliation 00429340 requires the current process null handle")]
    MissingProcessNullHandle(ObjectId),
}

impl ObjectArena {
    /// Complete 0047C840 for the arena's proven +58/+CC and family +D4
    /// vtables. This does not invoke source resolution or change the supplied
    /// damage-source handle, including a native null handle.
    pub fn propagate_actor_retaliation(
        &mut self,
        victim: ObjectId,
        damage_source: ActorHandle,
        process_null_handle: Option<ActorHandle>,
        rng: &mut u32,
    ) -> Result<(), ActorRetaliationError> {
        let state = self.actor(victim)?;
        let classification = state.word_b4;
        let target_radius_squared = (u32::from(state.byte_7c).pow(2)) << 16;
        let reaction_radius_squared = (u32::from(state.byte_7d).pow(2)) << 16;
        let maximum_squared = target_radius_squared.max(reaction_radius_squared);

        let mut index = 0;
        while let Some(&candidate) = self.actor_list().get(index) {
            // Portable containment preserves the node while withholding both
            // target admission and reaction from an incomplete constructor.
            if !self.construction_is_stopped(candidate)?
                && self.actor(candidate)?.word_b4 == classification
                && self.object(candidate)?.base.flags & 0xa0 == 0x20
                && (self.actor(candidate)?.word_c4 as i32) > 1
            {
                let position = self
                    .object(candidate)?
                    .base
                    .position_words
                    .ok_or(ActorRetaliationError::MissingPosition(candidate))?;
                let origin = self
                    .object(victim)?
                    .base
                    .position_words
                    .ok_or(ActorRetaliationError::MissingPosition(victim))?;
                // Native subtracts WORDs before sign extension. The three
                // DWORD squares/additions are compared as unsigned at47C907.
                let squared = position
                    .into_iter()
                    .zip(origin)
                    .map(|(point, origin)| i32::from(point.wrapping_sub(origin) as i16))
                    .map(|delta| delta.wrapping_mul(delta) as u32)
                    .fold(0u32, u32::wrapping_add);
                if squared < maximum_squared
                    && self.actor(candidate)?.handle != self.actor(victim)?.handle
                {
                    // Supported virtual queries return either self or null;
                    // the active +22C byte does not participate in these queries.
                    if let Some(component) = self.actor_damage_source_component(candidate)? {
                        if squared < target_radius_squared {
                            self.notify_family_damage_target(
                                component,
                                ActorFamilyTargetRequest {
                                    source: damage_source,
                                    word_08: 0x0780_0000,
                                    word_04: 0,
                                },
                            )?;
                        }
                        //47C954 rereads the component's flags after +D4.
                        if squared < reaction_radius_squared
                            && self.object(component)?.base.flags & 0x1000 != 0
                        {
                            //47C95C reads the victim's current +84 here.
                            let source = self.actor(victim)?.handle;
                            self.react_family_retaliation(
                                component,
                                source,
                                process_null_handle,
                                rng,
                            )?;
                        }
                    }
                }
            }
            // Both reached callbacks have concrete, non-destructive owners:
            // +D4 changes target slots, and429340 changes scalar actor fields.
            // Neither can unlink/admit actor-list nodes in this supported domain.
            index += 1;
        }
        Ok(())
    }

    ///00429340. The two independent RNG draws and the +20C/+210 target
    /// custody are retained; rejecting an existing target never clears it.
    pub fn react_family_retaliation(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        process_null_handle: Option<ActorHandle>,
        rng: &mut u32,
    ) -> Result<(), ActorRetaliationError> {
        //429343 reads0054E894 before retained+20C. The registry retains its
        // construction null and is not the owner of this current process value.
        let null =
            process_null_handle.ok_or(ActorRetaliationError::MissingProcessNullHandle(actor))?;
        let family = self.family428650_actor(actor)?;
        let retained = family.word_20c;
        if retained != null.raw() {
            if family.word_210 == 0 {
                return Ok(());
            }
            if retained != source.raw() && (name_entry_rng_next(rng) as u16) >= 0x800 {
                return Ok(());
            }
        }
        let draw = name_entry_rng_next(rng);
        let delay = self.derived_actor(actor)?.word_144;
        let family = self.family428650_actor_mut(actor)?;
        family.word_20c = source.raw();
        // Native (low16(RNG)+156250)<<7: a wrapping microsecond timer.
        family.word_210 = ((draw & 0xffff) + 156_250) << 7;
        if delay < 0 {
            self.derived_actor_mut(actor)?.word_144 = delay.wrapping_mul(-1000);
        }
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags & 0xfb9f_ffff);
        Ok(())
    }
}

#[cfg(test)]
#[path = "retaliation_tests.rs"]
mod tests;
