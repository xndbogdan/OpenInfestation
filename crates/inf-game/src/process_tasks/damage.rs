//! Concrete task/statistics services for the outer 0047C6E0 damage owner.
//! Class virtual+B4 and source virtual+108 remain required synchronous calls.

pub mod services;

use thiserror::Error;

use super::{ProcessTaskRuntime, TaskOwner, TaskStorageError};
use crate::{
    object_runtime::{ActorDamageCallback, ActorDamageHost, ActorHandle, ObjectArena, ObjectId},
    world_mission::WorldMission,
    world_statistics::{ActorDamageStatistic, WorldStatistics},
};

/// Reached process/world owners are mutable at the required callbacks. A B4
/// or +108 service may replace task slots or mission statistics before the
/// outer kernel reaches its own later reads.
pub struct ProcessDamageContext<'a> {
    pub tasks: &'a mut ProcessTaskRuntime,
    pub mission: &'a mut WorldMission,
}

pub trait ProcessActorDamageServices {
    type Error;

    fn process_null_handle(&self) -> Option<ActorHandle>;

    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
        context: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error>;

    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
        context: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error>;
}

/// Identity of the existing W+0C allocation at the reached capture. This is
/// compared only; it is never dereferenced as a native/portable raw pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedDamageStatistics(usize);

#[derive(Debug, Error)]
pub enum ProcessActorDamageError<E> {
    #[error(transparent)]
    Tasks(#[from] TaskStorageError),
    #[error("required actor damage callback failed")]
    Callback(E),
    #[error("native actor damage statistics allocation W+0C is absent")]
    MissingStatistics,
    #[error("actor damage statistics allocation changed after its capture")]
    ReplacedStatistics,
}

/// Complete data services for ActorDamageHost. It delegates class/source
/// behavior and owns no successful fallback for either callback. Statistics
/// classification between capture and publication is the arena's pure owned
/// query; arbitrary mutation of that captured allocation is outside this seam.
pub struct ProcessActorDamageHost<'a, S> {
    context: ProcessDamageContext<'a>,
    services: &'a mut S,
}

impl<'a, S> ProcessActorDamageHost<'a, S> {
    pub fn new(context: ProcessDamageContext<'a>, services: &'a mut S) -> Self {
        Self { context, services }
    }
}

impl<S: ProcessActorDamageServices> ActorDamageHost for ProcessActorDamageHost<'_, S> {
    type Error = ProcessActorDamageError<S::Error>;
    type TaskOwner = TaskOwner;
    type StatisticsOwner = CapturedDamageStatistics;

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.services.process_null_handle()
    }

    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error> {
        self.services
            .virtual_b4(
                arena,
                actor,
                callback,
                ProcessDamageContext {
                    tasks: &mut *self.context.tasks,
                    mission: &mut *self.context.mission,
                },
            )
            .map_err(ProcessActorDamageError::Callback)
    }

    fn task_owner(
        &mut self,
        _: &mut ObjectArena,
        classification: u32,
    ) -> Result<Option<TaskOwner>, Self::Error> {
        self.context.tasks.owner(classification).map_err(Into::into)
    }

    fn task_source(
        &mut self,
        _: &mut ObjectArena,
        owner: TaskOwner,
    ) -> Result<ActorHandle, Self::Error> {
        Ok(self.context.tasks.record(owner)?.source)
    }

    fn set_task_source(
        &mut self,
        _: &mut ObjectArena,
        owner: TaskOwner,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        // The outer 0047C6E0 kernel has already performed 00482C60's null
        // and live-equality guards before reaching this direct +8 write.
        self.context
            .tasks
            .write_source(owner, source)
            .map_err(Into::into)
    }

    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
    ) -> Result<(), Self::Error> {
        self.services
            .source_component_108(
                arena,
                component,
                target,
                ProcessDamageContext {
                    tasks: &mut *self.context.tasks,
                    mission: &mut *self.context.mission,
                },
            )
            .map_err(ProcessActorDamageError::Callback)
    }

    fn statistics_owner(
        &mut self,
        _: &mut ObjectArena,
    ) -> Result<CapturedDamageStatistics, Self::Error> {
        let record = self
            .context
            .mission
            .record_0c
            .as_ref()
            .ok_or(ProcessActorDamageError::MissingStatistics)?;
        Ok(CapturedDamageStatistics(record.as_ref().as_ptr() as usize))
    }

    fn record_statistics(
        &mut self,
        _: &mut ObjectArena,
        owner: CapturedDamageStatistics,
        statistic: ActorDamageStatistic,
    ) -> Result<(), Self::Error> {
        let record = self
            .context
            .mission
            .record_0c
            .as_mut()
            .ok_or(ProcessActorDamageError::MissingStatistics)?;
        if record.as_mut().as_mut_ptr() as usize != owner.0 {
            return Err(ProcessActorDamageError::ReplacedStatistics);
        }
        WorldStatistics::from_record(record).record_damage(statistic);
        Ok(())
    }
}

#[cfg(test)]
#[path = "damage_tests.rs"]
mod tests;
