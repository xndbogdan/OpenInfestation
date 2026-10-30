//! Selector 35's Rescue (descriptor 10: score 0043FD40 over 0043D730, entry
//! 0043FD80 over 0043D780 and update 0043FDA0 over 0043D870) and Go to space
//! (descriptor 11: 0043FDC0 over 0043D8F0, 0043FE00 over 0043D9A0 and
//! 0043FE20 over 0043DAC0). A transport with a rescue count +242 settles
//! where it is while its passengers come aboard (its +A0, 0043F790, counts
//! them in +243). With all aboard and the player's unit near, it climbs away:
//! the first departure grants the players its +7A, and above 0x7D00 it
//! disposes itself.

use super::{required, Selector35DescriptorContext, Selector35DescriptorError};
use crate::object_runtime::{
    native_trunc_low32, ActorServiceAllocationRequest, ActorServiceResourceKind, ActorTerrainHost,
    FixedPositionTarget, GrantOwner, LiveGrantOperation, LiveGrantRequest, NativeTarget,
    NativeTargetClass, ObjectArena, ObjectError, ObjectId,
};

/// The status texts +228 names: "Rescue" and "Go to space".
const RESCUE: u32 = 0x0053_3da4;
const GO_TO_SPACE: u32 = 0x0053_3dac;
/// `0052286C`: the departure point lies 15000 along the basis X row.
const SPACE_REACH: f32 = 15000.0;
/// `0043DB02`: the climb's speed +258 stops at 10000.
const SPACE_SPEED: u32 = 10_000;

impl ObjectArena {
    /// `0043D780`: entering, the transport settles over the ground below it.
    /// `+14C` is its height above the ground, `+190` the ground and `+194`
    /// its own height (both shifted up by 14), and a fresh kind-8 point at
    /// its position becomes `+148` (the old one is not released).
    pub(super) fn selector35_change_rescue<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        entering: bool,
        context: &mut Selector35DescriptorContext<'_>,
        terrain: &mut H,
    ) -> Result<(), Selector35DescriptorError<H::Error>> {
        if !entering {
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.words_204_208[1] = Some(0);
        self.selector35_actor_mut(actor)?.word_230 = 0;
        self.family428650_actor_mut(actor)?.word_228 = RESCUE;
        let [x, y, z] =
            self.object(actor)?
                .base
                .position_words
                .ok_or(required(actor, 0x32, 0x0043_d7a0))?;
        // 0043D7E2: 00450A30 at the transport's X and Z.
        let ground = terrain
            .terrain(self, actor)
            .map_err(Selector35DescriptorError::Terrain)?
            .height_at([x, z]);
        let (ground, y) = (i32::from(ground), i32::from(y as i16));
        let state = self.selector35_actor_mut(actor)?;
        state.word_260 = Some(0);
        state.byte_238 = 0;
        self.derived_actor_mut(actor)?.word_14c =
            Some(y.wrapping_sub(ground).wrapping_abs() as u32);
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[2] = Some(ground.wrapping_shl(14) as u32);
        family.words_188_19c[3] = Some(y.wrapping_shl(14) as u32);
        self.selector35_point_at_self(actor, context)
    }

    /// `0043D870`: with a rescue count, `+194` comes down by the delta toward
    /// `+190`, and a kind-8 `+148` takes its height (`+194` shifted down by
    /// 14).
    pub(super) fn selector35_update_rescue<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
    ) -> Result<(), Selector35DescriptorError<E>> {
        let count = self.selector35_actor(actor)?.bytes_240[2].ok_or(required(
            actor,
            0x242,
            0x0043_d870,
        ))?;
        if count == 0 {
            return Ok(());
        }
        let ground = self.selector35_family_word(actor, 2, 0x190, 0x0043_d885)? as i32;
        let height = self.selector35_family_word(actor, 3, 0x194, 0x0043_d87f)? as i32;
        if height <= ground {
            return Ok(());
        }
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(required(actor, 0x148, 0x0043_d88f))?;
        let lowered = height.wrapping_sub(delta as i32).max(ground);
        self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(lowered as u32);
        let mut target = self.native_target(reference)?;
        if target.kind_word() == 8 {
            let [x, _, z] =
                target
                    .raw_focus_08_words()
                    .ok_or(Selector35DescriptorError::RequiredFocus {
                        reference,
                        address: 0x0043_d899,
                    })?;
            target.set_focus_08_words([x, (lowered >> 14) as u16, z]);
            self.write_native_target(reference, target)?;
        }
        Ok(())
    }

    /// `0043D9A0`: entering, the departure. `+18C`/`+190` are the basis X
    /// row's x and z (virtual `+48`, `004026B0`) times 15000, truncated;
    /// `+194` is the speed `+258` shifted up by 14; and a fresh kind-8 point
    /// at the transport becomes `+148`. The first departure sets `+22E` bit
    /// 1, marks the counts gone (0xFF) and grants `+7A` to every player
    /// (`004750C0`).
    pub(super) fn selector35_change_space<E>(
        &mut self,
        actor: ObjectId,
        entering: bool,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        if !entering {
            return Ok(());
        }
        let basis = self.actor(actor)?.rotating.basis;
        let reach = |word: u32| {
            native_trunc_low32(f64::from(f32::from_bits(word)) * f64::from(SPACE_REACH)) as u32
        };
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[1] = Some(reach(basis.words[1]));
        family.words_188_19c[2] = Some(reach(basis.words[3]));
        let speed =
            self.selector35_actor(actor)?
                .word_258
                .ok_or(required(actor, 0x258, 0x0043_d9e4))?;
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[3] = Some(speed.wrapping_shl(14));
        family.words_204_208[1] = Some(0);
        self.selector35_actor_mut(actor)?.word_230 = 0;
        self.family428650_actor_mut(actor)?.word_228 = GO_TO_SPACE;
        let state = self.selector35_actor_mut(actor)?;
        state.byte_238 = 1;
        state.word_234 = 750;
        self.selector35_point_at_self(actor, context)?;
        let aboard = self.selector35_actor(actor)?.bytes_240[3].ok_or(required(
            actor,
            0x243,
            0x0043_da5c,
        ))?;
        if aboard == 0xff {
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.word_22e |= 2;
        let state = self.selector35_actor_mut(actor)?;
        state.bytes_240[3] = Some(0xff);
        state.bytes_240[2] = Some(0xff);
        self.selector35_grant_rescue(actor, &context.live_grants)?;
        Ok(())
    }

    /// The departure's grant: the transport's `+7A`, when nonzero, to every
    /// player (`004750C0`). The port fix `PortFixes::mission_completion`
    /// also calls it as the last scientist boards; a grant already held
    /// sets nothing again.
    pub fn selector35_grant_rescue(
        &mut self,
        actor: ObjectId,
        live_grants: &crate::object_runtime::LiveGrantContext<'_>,
    ) -> Result<(), crate::object_runtime::LiveGrantError> {
        let grant = self.actor(actor)?.word_7a;
        if grant == 0 {
            return Ok(());
        }
        let players: Vec<_> = self
            .world_players()
            .iter()
            .map(|player| player.id)
            .collect();
        for player in players {
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw {
                        id: u32::from(grant),
                    },
                },
                live_grants,
            )?;
        }
        Ok(())
    }

    /// `0043DAC0`: the climb. `+148` (when kind 8) goes to the transport's
    /// X and Z plus `+18C`/`+190` and 1500 above it, while the speed `+258`
    /// grows with `+194` (by the delta, shifted down by 14) up to 10000 and
    /// `+25C` keeps its square. From 1500 below the top the point stays at
    /// 0x7FFF, and above 0x7D00 the transport disposes itself (`00479FA0`).
    pub(super) fn selector35_update_space<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
    ) -> Result<(), Selector35DescriptorError<E>> {
        let [x, y, z] =
            self.object(actor)?
                .base
                .position_words
                .ok_or(required(actor, 0x32, 0x0043_dac7))?;
        let along = self.selector35_family_word(actor, 1, 0x18c, 0x0043_dacb)? as u16;
        let across = self.selector35_family_word(actor, 2, 0x190, 0x0043_dad2)? as u16;
        let point_x = along.wrapping_add(x);
        self.family428650_actor_mut(actor)?.word_22e |= 2;
        let point_z = across.wrapping_add(z);
        let above = i32::from(y as i16) + 1500;
        let point_y = if above >= 0x8000 {
            if (y as i16) > 0x7d00 {
                self.dispose(actor)?;
            }
            0x7fff
        } else {
            let speed = self.selector35_actor(actor)?.word_258.ok_or(required(
                actor,
                0x258,
                0x0043_dafc,
            ))?;
            if speed < SPACE_SPEED {
                let climb = self
                    .selector35_family_word(actor, 3, 0x194, 0x0043_db10)?
                    .wrapping_add(delta);
                self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(climb);
                let speed = (((climb as i32) >> 14) as u32).min(SPACE_SPEED);
                let state = self.selector35_actor_mut(actor)?;
                state.word_258 = Some(speed);
                state.word_25c = Some(speed.wrapping_mul(speed));
            }
            above as u16
        };
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(required(actor, 0x148, 0x0043_db61))?;
        let mut target = self.native_target(reference)?;
        if target.kind_word() == 8 {
            target.set_focus_08_words([point_x, point_y, point_z]);
            self.write_native_target(reference, target)?;
        }
        Ok(())
    }

    /// `0043D820`/`0043DA24`/`0043E66F`: `+148` becomes a fresh kind-8 point
    /// at the transport (`+04` the process NULL `0054E894`), or NULL when
    /// the allocation fails. The old target is not released.
    pub(super) fn selector35_point_at_self<E>(
        &mut self,
        actor: ObjectId,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        let allocation = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(ObjectError::from)?;
        if let Some(reference) = allocation {
            let null = context.constructor_null.resolve(self)?;
            let point = self.object(actor)?.base.position_words;
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(null, point)),
            )?;
        }
        self.derived_actor_mut(actor)?.target_148 = allocation;
        Ok(())
    }
}

#[cfg(test)]
#[path = "rescue_tests.rs"]
mod tests;
