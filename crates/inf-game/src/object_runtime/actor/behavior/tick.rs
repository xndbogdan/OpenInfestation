//! Retained weapon-state timer phase 0048D030 -> 0048D770. This is not
//! the family descriptor dispatcher or an enclosing virtual +24 completion.
//!
//! A constructor-null +28 bypasses 0048D000's audio stop. Reached nonnull
//! audio expiry reports its required service after the native +38 store.

use thiserror::Error;

use super::ActorBehaviorState;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorBehaviorTickError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its retained native +140 behavior object")]
    RequiredBehavior(ObjectId),
    #[error(
        "actor {actor:?} reaches native behavior group {index}, outside {groups} owned groups"
    )]
    GroupBacking {
        actor: ObjectId,
        index: usize,
        groups: usize,
    },
    #[error("actor {actor:?} reaches native +18 allocation DWORD at byte {offset:04X}, outside {entries} owned 32-byte entries")]
    EntryBacking {
        actor: ObjectId,
        offset: usize,
        entries: usize,
    },
    #[error("actor {actor:?} requires 0048D000 audio stop for retained {sound:?}")]
    RequiredSoundStop {
        actor: ObjectId,
        sound: crate::process_sound::ProcessSoundRef,
    },
}

impl ObjectArena {
    /// Complete 0048D030 for the actual retained +140 object. Counts are
    /// sign-extended BYTEs and backing is checked only at each reached read;
    /// an error retains every earlier group/entry/timer store.
    pub fn tick_actor_behavior(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
    ) -> Result<(), ActorBehaviorTickError> {
        let behavior = self
            .actor_behavior_storage_mut(actor)?
            .as_mut()
            .ok_or(ActorBehaviorTickError::RequiredBehavior(actor))?;
        tick_behavior(behavior, actor, microseconds)
    }

    /// The paused tick's release (`00438980..00438995`): the unit's virtual
    /// +7C, its +140 behavior where [`Self::actor_behavior_presence_7c`]
    /// finds one, then `0048D000` on it. A held looping sound comes back for
    /// `00487FE0` to delete, and +28 and +2C clear; with +28 NULL nothing
    /// changes.
    pub fn take_unit_behavior_sound(
        &mut self,
        unit: ObjectId,
    ) -> Result<Option<crate::process_sound::ProcessSoundRef>, ObjectError> {
        if !self.actor_behavior_presence_7c(unit)? {
            return Ok(None);
        }
        let Some(behavior) = self.actor_behavior_storage_mut(unit)?.as_mut() else {
            return Ok(None);
        };
        let sound = behavior.sound_28.take();
        if sound.is_some() {
            behavior.word_2c = 0;
        }
        Ok(sound)
    }
}

fn tick_behavior(
    behavior: &mut ActorBehaviorState,
    actor: ObjectId,
    microseconds: u32,
) -> Result<(), ActorBehaviorTickError> {
    let mut group = 0usize;
    while (group as i32) < i32::from(behavior.byte_0d as i8) {
        let groups = behavior.groups_1c.len();
        let record =
            behavior
                .groups_1c
                .get_mut(group)
                .ok_or(ActorBehaviorTickError::GroupBacking {
                    actor,
                    index: group,
                    groups,
                })?;
        record.word_0c &= !2;
        group += 1;
        // Native 0048D054 reads +0D again after the flag write.
    }

    let mut entry = 0usize;
    while (entry as i32) < i32::from(behavior.byte_0f as i8) + i32::from(behavior.byte_0e as i8) {
        let groups = i32::from(behavior.byte_0d as i8);
        tick_entry(behavior, actor, entry * 32, microseconds, groups)?;
        entry += 1;
        // Native 0048D083/87 reread both signed counts after the helper.
    }

    let timer = behavior.word_38;
    if timer != 0 {
        // 0048D09F is signed JLE, unlike the entry helpers' JB/JBE.
        if (timer as i32) > (microseconds as i32) {
            behavior.word_38 = timer.wrapping_sub(microseconds);
        } else {
            behavior.word_38 = 0;
            if let Some(sound) = behavior.sound_28 {
                return Err(ActorBehaviorTickError::RequiredSoundStop { actor, sound });
            }
            // 0048D000's actual +28=NULL branch performs no audio call and
            // no +2C write in this constructor-backed ActorBehaviorState.
        }
    }
    Ok(())
}

/// 0048D770 captures signed +0D once on entry. Its first range starts at
/// +04 and may reach an adjacent entry in a supplied larger-count domain;
/// use the actual flat +18 backing, rather than clamping to one record.
fn tick_entry(
    behavior: &mut ActorBehaviorState,
    actor: ObjectId,
    start: usize,
    microseconds: u32,
    groups: i32,
) -> Result<(), ActorBehaviorTickError> {
    let mut group = 0i32;
    while group < groups {
        let value = entry_word(behavior, actor, start + 4 + group as usize * 4)?;
        *value = if *value >= microseconds {
            (*value).wrapping_sub(microseconds)
        } else {
            0
        };
        group += 1;
    }
    let value = entry_word(behavior, actor, start + 0x14)?;
    *value = if *value >= microseconds {
        (*value).wrapping_sub(microseconds)
    } else {
        0
    };
    let value = entry_word(behavior, actor, start + 0x1c)?;
    *value = if *value > microseconds {
        (*value).wrapping_sub(microseconds)
    } else {
        0
    };
    Ok(())
}

fn entry_word(
    behavior: &mut ActorBehaviorState,
    actor: ObjectId,
    offset: usize,
) -> Result<&mut u32, ActorBehaviorTickError> {
    let entries = behavior.entries_18.len();
    let row =
        behavior
            .entries_18
            .get_mut(offset / 32)
            .ok_or(ActorBehaviorTickError::EntryBacking {
                actor,
                offset,
                entries,
            })?;
    Ok(&mut row[(offset % 32) / 4])
}

#[cfg(test)]
mod tests;
