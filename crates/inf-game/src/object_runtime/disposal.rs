//! Shared disposal drain 00479840. Capture the head's object before +30,
//! retain that object for its deleting destructor, then reload the live head.
//! The per-object stop ledger is portable containment, not a native field.

use thiserror::Error;

use super::{
    CreationListeners, LiveGrantContext, ObjectArena, ObjectError, ObjectId, ObjectList,
    SoundTeardownService, WorldCleanupError,
};
use crate::world_mission::WorldMission;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectDisposalPhase {
    CleanupCallback,
    DeletingDestructor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDisposalStop {
    pub object: ObjectId,
    pub phase: ObjectDisposalPhase,
    pub reason: String,
}

/// Retained per world. An error can follow a real release or callback write,
/// so even a later strict invocation must not replay that object's prefix.
/// Generation-bearing IDs prevent a reused slot from inheriting a stop.
#[derive(Debug, Default)]
pub struct ObjectDisposalState {
    stopped: Vec<ObjectDisposalStop>,
}

impl ObjectDisposalState {
    pub fn stopped(&self) -> &[ObjectDisposalStop] {
        &self.stopped
    }

    fn stop_for(&self, object: ObjectId) -> Option<&ObjectDisposalStop> {
        self.stopped.iter().find(|stop| stop.object == object)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectDisposalFailurePolicy {
    /// Native head order, returning the first portable service error.
    ReturnFirstError,
    /// Skip already stopped objects without unlinking them, allowing healthy
    /// disposal work to continue. Native has no fallible callback equivalent;
    /// replacing the missing service removes the need for this policy.
    RetainStoppedObjectApproximation,
}

pub struct ObjectDisposalRequest<'a, 'g> {
    pub mission: &'a mut WorldMission,
    pub grants: &'a LiveGrantContext<'g>,
    pub sound: SoundTeardownService<'a>,
    pub state: &'a mut ObjectDisposalState,
    pub failure_policy: ObjectDisposalFailurePolicy,
    /// The creation-listener list whose bindings a Force Field's destructor
    /// 004768D0 removes. None stops at a field's destruction.
    pub listeners: Option<&'a mut CreationListeners>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ObjectDisposalReport {
    pub destroyed: Vec<ObjectId>,
    pub newly_stopped: Vec<ObjectDisposalStop>,
    /// The drain paused after this ship's +30: its destructor exit 0043E120
    /// needs the world's burst and radius. Run it, then drain again; the
    /// ship's destructor follows without a second +30.
    pub exit_pending: Option<ObjectId>,
}

#[derive(Debug, Error)]
pub enum ObjectDisposalError<E> {
    #[error("object {object:?} disposal failed during {phase:?}: {error}")]
    Failed {
        object: ObjectId,
        phase: ObjectDisposalPhase,
        error: E,
    },
    #[error("object disposal previously stopped: {0:?}")]
    PreviouslyStopped(ObjectDisposalStop),
}

/// Distinct original virtual call boundaries. The production implementation
/// below uses concrete class custody. Native receipts supply only these lower
/// calls; the captured-object/head traversal still executes this same kernel.
pub(super) trait DisposalDispatch {
    type Error: std::fmt::Display;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error>;
    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error>;
}

struct NativeDisposalDispatch<'a, 'g> {
    mission: &'a mut WorldMission,
    grants: &'a LiveGrantContext<'g>,
    sound: SoundTeardownService<'a>,
    listeners: Option<&'a mut CreationListeners>,
}

impl DisposalDispatch for NativeDisposalDispatch<'_, '_> {
    type Error = WorldCleanupError;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        arena.cleanup_object_callback(object, self.mission, self.grants, None)
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        if arena.selector18(object).is_ok() || arena.selector78(object).is_ok() {
            self.listeners
                .as_deref_mut()
                .ok_or(ObjectError::ListenerOwnerBoundary(object))?
                .unregister_owner(object);
        }
        arena.destroy_with_process(
            super::ObjectDestructionRequest {
                object,
                sound: self.sound.reborrow(),
            },
            self.grants.process,
        )?;
        Ok(())
    }
}

impl ObjectArena {
    /// Portable omission policy for scans whose native gates still admit a
    /// retained allocation after a disposal error. Does not alter its nodes.
    pub(crate) fn disposal_is_stopped(&self, object: ObjectId) -> Result<bool, ObjectError> {
        Ok(self.owned(object)?.disposal_stop.is_some())
    }

    /// Allocation-owned containment also protects terminal cleanup and direct
    /// destruction, even if a caller starts a new frame diagnostic ledger.
    pub(super) fn ensure_disposal_active(&self, object: ObjectId) -> Result<(), ObjectError> {
        if let Some(stop) = &self.owned(object)?.disposal_stop {
            return Err(ObjectError::StoppedDisposal {
                object,
                phase: stop.phase,
            });
        }
        Ok(())
    }

    fn disposal_stop<'a>(
        &'a self,
        object: ObjectId,
        state: &'a ObjectDisposalState,
    ) -> Option<&'a ObjectDisposalStop> {
        self.owned(object)
            .ok()
            .and_then(|owned| owned.disposal_stop.as_ref())
            .or_else(|| state.stop_for(object))
    }

    /// 00479840 does not inhibit mission bookkeeping and does not remove the
    /// node before either callback. The destructor owns all successful unlinking.
    pub fn drain_object_disposal(
        &mut self,
        request: ObjectDisposalRequest<'_, '_>,
    ) -> Result<ObjectDisposalReport, ObjectDisposalError<WorldCleanupError>> {
        self.drain_disposal_with(
            &mut NativeDisposalDispatch {
                mission: request.mission,
                grants: request.grants,
                sound: request.sound,
                listeners: request.listeners,
            },
            request.state,
            request.failure_policy,
        )
    }

    pub(super) fn drain_disposal_with<D: DisposalDispatch>(
        &mut self,
        dispatch: &mut D,
        state: &mut ObjectDisposalState,
        policy: ObjectDisposalFailurePolicy,
    ) -> Result<ObjectDisposalReport, ObjectDisposalError<D::Error>> {
        let mut report = ObjectDisposalReport::default();
        'drain: loop {
            let Some(node) = self
                .list(ObjectList::Disposal)
                .iter()
                .copied()
                .find(|node| {
                    policy == ObjectDisposalFailurePolicy::ReturnFirstError
                        || self.disposal_stop(node.object, state).is_none()
                })
            else {
                return Ok(report);
            };
            let object = node.object; // 00479850 captures ESI before virtual +30.
            if let Some(stop) = self.disposal_stop(object, state) {
                return Err(ObjectDisposalError::PreviouslyStopped(stop.clone()));
            }
            // Resumed after an exit pause: +30 already ran. A failed exit
            // leaves the destructor's own typed boundary to stop it.
            let resumed = self.cleanup_exit_pending == Some(object);
            if resumed {
                self.cleanup_exit_pending = None;
            }
            for phase in [
                ObjectDisposalPhase::CleanupCallback,
                ObjectDisposalPhase::DeletingDestructor,
            ] {
                if resumed && phase == ObjectDisposalPhase::CleanupCallback {
                    continue;
                }
                if !resumed
                    && phase == ObjectDisposalPhase::DeletingDestructor
                    && self.selector35_exit_needs_world_effects(object)
                {
                    self.cleanup_exit_pending = Some(object);
                    report.exit_pending = Some(object);
                    return Ok(report);
                }
                let result = match phase {
                    ObjectDisposalPhase::CleanupCallback => dispatch.callback(self, object),
                    ObjectDisposalPhase::DeletingDestructor => dispatch.destroy(self, object),
                };
                if let Err(error) = result {
                    let stop = ObjectDisposalStop {
                        object,
                        phase,
                        reason: error.to_string(),
                    };
                    // A supplied destructor can have completed its unlink and
                    // allocation release before returning an error. Never mark
                    // a replacement allocation occupying the same slot.
                    if let Ok(owned) = self.owned_mut(object) {
                        owned.disposal_stop = Some(stop.clone());
                    }
                    state.stopped.push(stop.clone());
                    if policy == ObjectDisposalFailurePolicy::ReturnFirstError {
                        return Err(ObjectDisposalError::Failed {
                            object,
                            phase,
                            error,
                        });
                    }
                    report.newly_stopped.push(stop);
                    continue 'drain;
                }
            }
            report.destroyed.push(object);
            // 00479866 restarts at M+10, including any new head installed by
            // either callback. Never follow a captured successor or ID vector.
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_receipts;
