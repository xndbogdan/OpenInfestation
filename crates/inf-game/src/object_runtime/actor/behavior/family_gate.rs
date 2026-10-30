//! 00429CB0 composes the current behavior predicate and helper query.
//! Only initial helper presence is captured. Predicate callbacks can replace
//! positions, behavior and the helper before the reached query tail.

use thiserror::Error;

use super::{
    BehaviorPredicateContext, BehaviorPredicateError, BehaviorPredicateHost,
    BehaviorPredicateRequest, BehaviorRowSelection,
};
use crate::object_runtime::{
    FamilyHelperHistoryError, FamilyHelperQueryRequest, ObjectArena, ObjectError, ObjectId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyCandidateGateRequest {
    pub actor: ObjectId,
    pub candidate: Option<ObjectId>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyCandidateGateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Predicate(#[from] BehaviorPredicateError<E>),
    #[error(transparent)]
    Helper(#[from] FamilyHelperHistoryError),
    #[error("candidate gate requires actor {0:?}'s current XYZ WORDs")]
    MissingPosition(ObjectId),
}

impl ObjectArena {
    pub fn test_family_candidate_gate<H: BehaviorPredicateHost>(
        &mut self,
        request: FamilyCandidateGateRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilyCandidateGateError<H::Error>> {
        let owner = request.actor;
        // 00429CB7: the branch survives synchronous predicate services;
        // retaining the allocation or rereading presence here would not.
        let had_helper = self.family428650_actor(owner)?.helper_218.is_some();
        let Some(candidate) = request.candidate else {
            return Ok(false);
        };
        let Some(behavior) = self.actor_behavior_storage(owner)?.as_ref() else {
            return Ok(false);
        };
        let selection =
            BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, owner);
        let accepted = self.test_behavior_damage_candidate(
            BehaviorPredicateRequest {
                selection,
                candidate: Some(candidate),
            },
            context,
            host,
        )?;
        if !had_helper || !accepted {
            return Ok(accepted);
        }
        // 00429CDD..CF1: use fresh positions after the predicate. Native
        // WORD subtraction wraps independently on each axis.
        let current_position = |object| {
            self.object(object)?
                .base
                .position_words
                .ok_or(FamilyCandidateGateError::MissingPosition(object))
        };
        let actor_position = current_position(owner)?;
        let candidate_position = current_position(candidate)?;
        let displacement_words =
            std::array::from_fn(|axis| actor_position[axis].wrapping_sub(candidate_position[axis]));
        // 00429CF5 precedes the current-helper reload at 00429D09. The
        // query resolves that current allocation, including a missing-helper
        // error after a callback removes an initially present helper.
        let handle = self.actor(candidate)?.handle;
        Ok(!self.query_family_helper_history(
            owner,
            FamilyHelperQueryRequest {
                handle,
                displacement_words,
            },
        )?)
    }
}

#[cfg(test)]
mod tests;
