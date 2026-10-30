//! Shared pair reaction 00427C20: virtual +A4 of the Selector10/34/35/36
//! tables, and the tail of Selector12's 00469030. Its common prefix is also
//! all of 00427BE0, the +A4 of Selector2/11. Also Selector10/36's pair
//! accept +A0, 00425B90.

use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        mark_flag_change, ActorServiceAllocationRequest, ActorServiceResourceKind,
        FixedPositionTarget, NativeTarget, NativeTargetClass, NativeTargetError, ObjectArena,
        ObjectError, ObjectId, WeightedTargetConstructorNull,
    },
};

pub struct FamilyPairReactionRequest<'a> {
    pub rng: &'a mut u32,
    /// Original executable table 0053DF00.
    pub sine_quarter: &'a [u16; 4096],
    /// 0054E894, read after the helper allocation.
    pub constructor_null: WeightedTargetConstructorNull<'a>,
}

/// 00425B90's verdict for 0047B610.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyRescueAccept {
    /// Returns 0, after any reached flag write.
    Declined,
    /// 00425BE0: a team-0 unit takes a Selector12 aboard: the caller runs
    /// [`ObjectArena::board_family_unit`], flashes at the person
    /// (00489A90(2)) and disposes of it (00479FA0), and the pair accepts.
    Boarding,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyPairReactionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("actor {0:?} requires its unwritten +14C at 00427CA2")]
    Unwritten14c(ObjectId),
}

impl ObjectArena {
    /// 00425B90, Selector10/36 virtual +A0. Both actors at +C4 3 and a
    /// partner whose +5C answers (only Selector12's table) clear the ship's
    /// flag 10000000; bit 28 lies outside the 1FF dirty mask. A ship with a
    /// team then declines.
    pub fn accept_family_rescue(
        &mut self,
        actor: ObjectId,
        partner: ObjectId,
    ) -> Result<FamilyRescueAccept, ObjectError> {
        self.family4236d0_actor(actor)?;
        if self.actor(actor)?.word_c4 != 3
            || self.actor(partner)?.word_c4 != 3
            || self.selector12_actor(partner).is_err()
        {
            return Ok(FamilyRescueAccept::Declined);
        }
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags & !0x1000_0000);
        Ok(if self.actor(actor)?.word_b4 == 0 {
            FamilyRescueAccept::Boarding
        } else {
            FamilyRescueAccept::Declined
        })
    }

    /// `00425BE0..00425C03`, a team-0 unit taking a person aboard: dirty 2,
    /// the person's team, the `+1A0` table emptied, the behavior's choice
    /// (`00428390`) and the mode for the team (`00425AB0`: 0 for team 0, 1
    /// for 0x12, else 2 with a weapon behavior `+140` and 1 without).
    pub fn board_family_unit(
        &mut self,
        actor: ObjectId,
        partner: ObjectId,
    ) -> Result<(), ObjectError> {
        self.owned_mut(actor)?.state.base.dirty_word |= 2;
        let team = self.actor(partner)?.word_b4;
        self.actor_mut(actor)?.word_b4 = team;
        self.family428650_actor_mut(actor)?.count_1a0 = 0;
        let flags = self.object(actor)?.base.flags;
        let behavior_present = match &mut self.derived_actor_mut(actor)?.behavior_140 {
            Some(behavior) => {
                crate::object_runtime::actor::behavior::select_actor_behavior(
                    behavior, flags, team,
                );
                true
            }
            None => false,
        };
        let mode = match team {
            0 => 0,
            18 => 1,
            _ => 1 + u16::from(behavior_present),
        };
        self.set_family4236d0_mode(actor, mode)
    }

    /// `00425A40`: a differing mode ends the current behavior first, its
    /// descriptor's change with false (only bodies that write nothing are
    /// admitted) and the class's `+10C` ([`Self::clear_family_behavior_targets`]),
    /// `+184` becoming -1; then `+22E` gains `0x0C` and `+266` the mode.
    fn set_family4236d0_mode(&mut self, actor: ObjectId, mode: u16) -> Result<(), ObjectError> {
        let mode = if mode < 3 { mode } else { 0 };
        let old = self.family4236d0_actor(actor)?.word_266;
        if old == mode {
            return Ok(());
        }
        let index = self.family428650_actor(actor)?.word_184;
        if index != -1 {
            let descriptor =
                crate::object_runtime::FamilyBehaviorProfile::Family4236d0 { mode: old }
                    .descriptor(index)
                    .ok_or(ObjectError::FamilyBehaviorDestructionBoundary(actor))?;
            if !super::family4236d0::exit_writes_nothing(descriptor.change_address) {
                return Err(ObjectError::FamilyBehaviorDestructionBoundary(actor));
            }
            self.clear_family_behavior_targets(actor)?;
            self.family428650_actor_mut(actor)?.word_184 = -1;
        }
        self.family428650_actor_mut(actor)?.word_22e |= 0x0c;
        self.family4236d0_actor_mut(actor)?.word_266 = mode;
        Ok(())
    }

    /// 00427BE0: a negative +144 becomes +144 * -1000, then 0042A7A0 clears
    /// 0x04600000 from the flags with the common dirty rule.
    pub fn settle_pair_contact(&mut self, actor: ObjectId) -> Result<(), ObjectError> {
        let derived = self.derived_actor_mut(actor)?;
        if derived.word_144 < 0 {
            derived.word_144 = derived.word_144.wrapping_mul(-1000);
        }
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags & !0x0460_0000);
        Ok(())
    }

    /// 00427C20. Another team (+B4) only marks +22E bit 1. A team mate that
    /// is not 8 times lighter, with +C4 3, gives an actor without a +154
    /// helper a sidestep point beside it: a fresh kind-8 copied point.
    pub fn react_family_pair(
        &mut self,
        actor: ObjectId,
        other: ObjectId,
        request: FamilyPairReactionRequest<'_>,
    ) -> Result<(), FamilyPairReactionError> {
        self.settle_pair_contact(actor)?;
        if self.actor(actor)?.word_b4 != self.actor(other)?.word_b4 {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        let partner_mass = u32::from(self.actor(other)?.word_8c) << 3;
        if partner_mass <= u32::from(self.actor(actor)?.word_8c)
            || self.actor(other)?.word_c4 != 3
            || self.derived_actor(actor)?.target_154.is_some()
        {
            return Ok(());
        }
        let extent = self
            .derived_actor(actor)?
            .word_14c
            .ok_or(FamilyPairReactionError::Unwritten14c(actor))? as i32;
        let radius = (i32::from(self.object(actor)?.base.word_28) * 4).max(extent / 2);
        let own = self.position_words(actor)?;
        let theirs = self.position_words(other)?;
        let offset: [i16; 3] =
            std::array::from_fn(|axis| theirs[axis].wrapping_sub(own[axis]) as i16);
        // 0047A660: actor +44 basis rows, world to local.
        let [local_x, _, local_z] = self.actor(actor)?.rotating.basis.inverse_words(offset);
        let angle = (name_entry_rng_next(request.rng) & 0xffff) >> 4;
        let sine = |angle: u32| -> i32 {
            let mut index = angle & 0x3ffc;
            if angle & 0x4000 != 0 {
                index = 0x3ffc - index;
            }
            let value = i32::from(request.sine_quarter[(index >> 2) as usize]);
            if angle & 0x8000 != 0 {
                -value
            } else {
                value
            }
        };
        let side = if local_z > 0 {
            // Partner ahead: step straight across, away from its side.
            let x = if local_x > 0 {
                radius.wrapping_neg()
            } else {
                radius
            };
            [x as u16, 0, 0]
        } else {
            let z = sine(angle + 0x5000).wrapping_mul(radius) >> 15;
            let mut x = sine(angle + 0x1000).wrapping_mul(radius) >> 15;
            if local_x > 0 {
                x = x.wrapping_neg();
            }
            [x as u16, 0, z as u16]
        };
        // 0047A6C0: local back to world, then the actor position.
        let world = self.transform_visual_vector_words(actor, side)?;
        let point: [u16; 3] = std::array::from_fn(|axis| world[axis].wrapping_add(own[axis]));
        let helper = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(NativeTargetError::from)?;
        if let Some(reference) = helper {
            let handle = request.constructor_null.resolve(self)?;
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(handle, Some(point))),
            )?;
        }
        self.derived_actor_mut(actor)?.target_154 = helper;
        let timer = ((name_entry_rng_next(request.rng) & 0xffff) + 0xf424) << 4;
        self.derived_actor_mut(actor)?.word_15c = Some(timer);
        Ok(())
    }

    fn position_words(&self, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }
}

#[cfg(test)]
mod tests;
