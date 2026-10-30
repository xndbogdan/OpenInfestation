//! Selector12 virtual +100 = 004686E0. The captured square, secondary-first
//! scoring, per-row queries and DC-before-D4 suffix are distinct from428DB0.
//! The same typed arena/inline table remains live through each real gate;
//! raw owner rebinding, pointer/index aliases and arbitrary virtual overrides
//! remain outside admission. No task/source or update dispatch is attached.

use super::*;
use crate::object_runtime::WeightedTargetConstructorNull;

impl ObjectArena {
    pub(super) fn select_selector12_targets<H: BehaviorPredicateHost>(
        &mut self,
        request: FamilySelectionRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, FamilySelectionError<H::Error>> {
        let actor = request.actor;
        let table = FamilyTargetTable { owner: actor };
        let radius = self.derived_actor(actor)?.word_164 as u32;
        let radius_squared = radius.wrapping_mul(radius);
        let initial_count = self.family428650_actor(actor)?.count_1a0;
        if initial_count == 0 {
            // 00468882 supplies a literal NULL and zero score only to DC.
            return Ok(self.publish_family_target(
                actor,
                FamilyTargetSlot::Target1dc,
                FamilyTargetSelection {
                    score: 0,
                    candidate: FamilySelectionCandidate::Null,
                }
                .weighted_actor_request()?,
            )?);
        }
        let (mut primary, mut secondary) =
            (FamilyTargetSelection::EMPTY, FamilyTargetSelection::EMPTY);
        if (initial_count as i32) > 0 {
            let (mut index, mut dirty) = (0u32, false);
            loop {
                match self.resolve_family_table_entry(table, index, context.process)? {
                    None => dirty = true,
                    Some(candidate) => {
                        // Native reads candidate X, self X/Z, candidate Z.
                        // Complete XYZ is the typed numeric admission; Y is unused.
                        let candidate_position = self
                            .object(candidate)?
                            .base
                            .position_words
                            .ok_or(FamilySelectionError::MissingPosition(candidate))?;
                        let self_position = self
                            .object(actor)?
                            .base
                            .position_words
                            .ok_or(FamilySelectionError::MissingPosition(actor))?;
                        let x = i32::from(self_position[0] as i16)
                            - i32::from(candidate_position[0] as i16);
                        let z = i32::from(self_position[2] as i16)
                            - i32::from(candidate_position[2] as i16);
                        let distance_squared =
                            (x.wrapping_mul(x) as u32).wrapping_add(z.wrapping_mul(z) as u32);
                        if distance_squared < radius_squared {
                            let base = self.family_radius_score(actor, distance_squared)?;
                            let weight = self.family_entry(table, index)?.word_04;
                            if weight < 0x0b40_0000 {
                                secondary.improve(FamilyTargetSelection {
                                    score: weighted_score(weight, base),
                                    candidate: FamilySelectionCandidate::Actor(candidate),
                                });
                            }
                            // 004687AB calls actual pure7C, then 004687B2 reads
                            // self140 directly. Keep the two reached reads.
                            if self.actor_behavior_presence_7c(actor)?
                                && self.derived_actor(actor)?.behavior_140.is_some()
                            {
                                let weight = self.family_entry(table, index)?.word_08;
                                if weight != 0 {
                                    let score = if weight >= 0x0b40_0000 {
                                        base
                                    } else {
                                        weighted_score(weight, base)
                                    };
                                    if (score as i32) > primary.score as i32
                                        && self.test_family_candidate_gate(
                                            FamilyCandidateGateRequest {
                                                actor,
                                                candidate: Some(candidate),
                                            },
                                            context,
                                            host,
                                        )?
                                    {
                                        primary.improve(FamilyTargetSelection {
                                            score,
                                            candidate: FamilySelectionCandidate::Actor(candidate),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                // 0046881D reloads count before advancing the captured cursor.
                let count = self.family428650_actor(actor)?.count_1a0;
                index = index.wrapping_add(1);
                if (index as i32) >= count as i32 {
                    break;
                }
            }
            if dirty {
                self.prune_family_target_table(table, context.process)?;
            }
        }
        // Signed-negative count reaches this suffix without traversing/pruning.
        // First true continues; a reached error stops before the second call.
        let first = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1dc,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..secondary.weighted_actor_request()?
            },
        )?;
        let second = self.publish_family_target(
            actor,
            FamilyTargetSlot::Target1d4,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(context.process),
                ..primary.weighted_actor_request()?
            },
        )?;
        Ok(first || second)
    }
}
