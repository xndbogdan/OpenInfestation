//! Shared selected weapon position 0048D6E0 and lazy group publication 0048CC00.
//! Operates on the actual current +140 storage; coordinates remain optional.

use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorBehaviorPositionRequest {
    /// Native LOWBYTE flag0 bypasses +140 completely.
    ActorPosition,
    /// Native nonzero flag selects signed entry+10 and its DWORD group+18.
    SelectedWeaponGroup,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActorBehaviorPositionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} requires its current +140 behavior position source")]
    RequiredBehavior(ObjectId),
    #[error("actor {actor:?} requires signed behavior entry {index}, current {entries}")]
    Entry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
    #[error("actor {actor:?} requires behavior group {index}, current {groups}")]
    Group {
        actor: ObjectId,
        index: u32,
        groups: usize,
    },
    #[error("actor {actor:?} requires group {index} WORD+{offset:X}")]
    Coordinate {
        actor: ObjectId,
        index: u32,
        offset: u8,
    },
}

impl ObjectArena {
    pub fn actor_behavior_position(
        &mut self,
        actor: ObjectId,
        request: ActorBehaviorPositionRequest,
    ) -> Result<[u16; 3], ActorBehaviorPositionError> {
        if request == ActorBehaviorPositionRequest::ActorPosition {
            return self
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
                .map_err(Into::into);
        }
        let behavior = self
            .actor_behavior_storage(actor)?
            .as_ref()
            .ok_or(ActorBehaviorPositionError::RequiredBehavior(actor))?;
        let entry_index = behavior.byte_10 as i8;
        let entry = behavior.entries_18.get(entry_index as usize).ok_or(
            ActorBehaviorPositionError::Entry {
                actor,
                index: entry_index,
                entries: behavior.entries_18.len(),
            },
        )?;
        let group_index = entry[6];
        self.actor_behavior_group_position(actor, group_index)
    }

    /// 0048CC00 also serves queued firing groups independently of the current
    /// selected entry. Both consumers retain the same lazy capture storage.
    pub(crate) fn actor_behavior_group_position(
        &mut self,
        actor: ObjectId,
        group_index: u32,
    ) -> Result<[u16; 3], ActorBehaviorPositionError> {
        let behavior = self
            .actor_behavior_storage(actor)?
            .as_ref()
            .ok_or(ActorBehaviorPositionError::RequiredBehavior(actor))?;
        let group = behavior.groups_1c.get(group_index as usize).ok_or(
            ActorBehaviorPositionError::Group {
                actor,
                index: group_index,
                groups: behavior.groups_1c.len(),
            },
        )?;
        if group.word_0c & 2 == 0 {
            if group.word_0c & 1 != 0 {
                //4909E0 captures all relative coordinates before output stores.
                let mut local = [0_i16; 3];
                for axis in 0..3 {
                    local[axis] = group.coordinates_00_0a[axis].ok_or(
                        ActorBehaviorPositionError::Coordinate {
                            actor,
                            index: group_index,
                            offset: (axis * 2) as u8,
                        },
                    )? as i16;
                }
                let world = self.actor(actor)?.rotating.basis.transform_words(local);
                for axis in 0..3 {
                    self.actor_behavior_storage_mut(actor)?
                        .as_mut()
                        .unwrap()
                        .groups_1c[group_index as usize]
                        .coordinates_00_0a[3 + axis] = Some(world[axis]);
                }
                //48CC1B/22/2A read actor X,Y,Z after the complete transform.
                for axis in 0..3 {
                    let position = self
                        .object(actor)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(actor))?[axis];
                    let group = &mut self
                        .actor_behavior_storage_mut(actor)?
                        .as_mut()
                        .unwrap()
                        .groups_1c[group_index as usize];
                    group.coordinates_00_0a[3 + axis] = Some(world[axis].wrapping_add(position));
                }
            } else {
                //48CC4E DWORD X/Y then48CC53 WORD Z. Existing XYZ storage
                //has all-or-none admission; unknown fails at first X read.
                let position = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let group = &mut self
                    .actor_behavior_storage_mut(actor)?
                    .as_mut()
                    .unwrap()
                    .groups_1c[group_index as usize];
                for axis in 0..3 {
                    group.coordinates_00_0a[3 + axis] = Some(position[axis]);
                }
            }
            //No cleanup/error path sets this bit. Only normal48CC00 does.
            self.actor_behavior_storage_mut(actor)?
                .as_mut()
                .unwrap()
                .groups_1c[group_index as usize]
                .word_0c |= 2;
        }
        let group = &self
            .actor_behavior_storage(actor)?
            .as_ref()
            .unwrap()
            .groups_1c[group_index as usize];
        let mut world = [0_u16; 3];
        for axis in 0..3 {
            world[axis] = group.coordinates_00_0a[3 + axis].ok_or(
                ActorBehaviorPositionError::Coordinate {
                    actor,
                    index: group_index,
                    offset: (6 + axis * 2) as u8,
                },
            )?;
        }
        Ok(world)
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
