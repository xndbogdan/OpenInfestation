//! Actual task virtual +00 relations: 004027B0/004EFBF0/00482C90/00482CE0.
//! Task allocation identity selects the callback; actor/global fields remain
//! lazy. This owner does not attach a selection scan or actor update.

use thiserror::Error;

use super::{ProcessTaskRuntime, TaskCallback, TaskOwner, TaskStorageError};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskRelationRequest {
    pub owner: TaskOwner,
    pub actor: ObjectId,
}

/// Required virtual queries receive the actual mutable owners. The selected
/// task may be destroyed/replaced during either call; the native component
/// body reads no task storage afterwards and does not revalidate that owner.
pub struct TaskRelationContext<'a> {
    pub tasks: &'a mut ProcessTaskRuntime,
    pub arena: &'a mut ObjectArena,
}

pub trait TaskRelationHost {
    type Error;
    type Component: Copy;

    /// Actual process DWORD 0055F448 at its reached load. No flag is read by
    /// either constant callback or the component-classification callback.
    /// This is a pure live read; synchronous owner/global mutation is not
    /// admitted at this scalar load.
    fn network_flag(&self) -> Option<u32>;

    fn actor_component_5c(
        &mut self,
        context: TaskRelationContext<'_>,
        actor: ObjectId,
    ) -> Result<Option<Self::Component>, Self::Error>;

    /// The native consumer tests the full returned DWORD, not low AL.
    fn component_presence_7c(
        &mut self,
        context: TaskRelationContext<'_>,
        component: Self::Component,
    ) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TaskRelationError<E> {
    #[error(transparent)]
    Storage(#[from] TaskStorageError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("task relation requires process network flag at {address:08X}")]
    MissingNetworkFlag { address: u32 },
    #[error("required task relation virtual query failed at {address:08X}: {error}")]
    Host { address: u32, error: E },
}

impl ProcessTaskRuntime {
    /// Execute the captured task's actual virtual +00. Resolving its live
    /// callback is the portable dispatch admission; constant bodies then read
    /// neither the actor nor process data. Task classification is consumed
    /// only on the team body's reached final comparison.
    pub fn query_relation<H: TaskRelationHost>(
        &mut self,
        request: TaskRelationRequest,
        arena: &mut ObjectArena,
        host: &mut H,
    ) -> Result<u32, TaskRelationError<H::Error>> {
        let callback = self.record(request.owner)?.callback;
        match callback {
            TaskCallback::Zero4027b0 | TaskCallback::Zero4efbf0 => Ok(0),
            TaskCallback::TeamClassification => {
                let classification = arena.actor(request.actor)?.word_b4;
                if classification == 18 {
                    return Ok(2);
                }
                let network = host
                    .network_flag()
                    .ok_or(TaskRelationError::MissingNetworkFlag {
                        address: 0x0048_2cf7,
                    })?;
                if network != 0
                    && classification != 0
                    && classification != 1
                    && classification != self.record(request.owner)?.classification
                {
                    Ok(2)
                } else {
                    Ok(0)
                }
            }
            TaskCallback::ComponentClassification => {
                // 00482C94 captures +B4 before either mutable virtual query.
                let classification = arena.actor(request.actor)?.word_b4;
                match classification {
                    0 | 18 => Ok(0),
                    1 => {
                        let component = host
                            .actor_component_5c(
                                TaskRelationContext { tasks: self, arena },
                                request.actor,
                            )
                            .map_err(|error| TaskRelationError::Host {
                                address: 0x0048_2caa,
                                error,
                            })?;
                        let Some(component) = component else {
                            return Ok(1);
                        };
                        let presence = host
                            .component_presence_7c(
                                TaskRelationContext { tasks: self, arena },
                                component,
                            )
                            .map_err(|error| TaskRelationError::Host {
                                address: 0x0048_2cb5,
                                error,
                            })?;
                        Ok(u32::from(presence != 0))
                    }
                    _ => Ok(2),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
