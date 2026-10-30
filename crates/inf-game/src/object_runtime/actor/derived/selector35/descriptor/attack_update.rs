//! Selector35 virtual+F4=0043D2E0. Its secondary copied point and drift are
//! separate from the common attack helper/size/sound policy.
use super::*;
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, FixedPositionTarget, NativeTargetClass,
};

impl ObjectArena {
    pub(super) fn selector35_update_attack<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        if self.selector35_actor(actor)?.byte_250 != 0 {
            return Ok(());
        }
        let horizontal =
            self.selector35_actor(actor)?
                .word_260
                .ok_or(required(actor, 0x260, 0x0043_d2ff))? as i32;
        self.selector35_actor_mut(actor)?.word_234 = if horizontal < 501 {
            100
        } else if horizontal < 1000 {
            ((horizontal - 500) / 2 + 125) as u32
        } else {
            375
        };
        let timer = self.selector35_family_word(actor, 1, 0x18c, 0x0043_d344)?;
        if (timer as i32) > delta as i32
            && self
                .derived_actor(actor)?
                .word_150
                .ok_or(required(actor, 0x150, 0x0043_d350))? as i32
                > 2
        {
            self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_sub(delta));
        } else {
            self.selector35_reset_timer(actor, 1_000_000, context.rng)?;
            if self.selector35_actor(actor)?.word_264 == 0
                || name_entry_rng_next(context.rng) & 1 != 0
            {
                let value = (name_entry_rng_next(context.rng) & 255) + 250;
                self.selector35_actor_mut(actor)?.word_264 = value;
                if value & 1 != 0 {
                    self.selector35_actor_mut(actor)?.word_264 = value.wrapping_neg();
                }
            }
            let value = name_entry_rng_next(context.rng) & 127;
            self.selector35_actor_mut(actor)?.word_230 = value;
            let horizontal = self.selector35_actor(actor)?.word_260.ok_or(required(
                actor,
                0x260,
                0x0043_d3d0,
            ))?;
            if (horizontal as i32) < value as i32 {
                self.selector35_actor_mut(actor)?.word_230 = horizontal;
            }
            if self.family428650_actor(actor)?.targets_1d4_1fc[4].is_none()
                && self.family428650_actor(actor)?.targets_1d4_1fc[0].is_none()
            {
                self.family428650_actor_mut(actor)?.word_22e |= 1;
            } else {
                self.selector35_reset_timer(actor, 2_000_000, context.rng)?;
                let selected = self.selector35_weighted(actor, 0, 4, 0x0043_d449)?;
                let selected_handle = self.native_target(selected)?.handle_04();
                let current = self.selector35_current(actor, 0x0043_d44f)?;
                if selected_handle != self.native_target(current)?.handle_04() {
                    let mut target = self.native_target(selected)?;
                    let result = self.refresh_native_target(&mut target, context.settings_7c);
                    self.write_native_target(selected, target)?;
                    if result? as i32 > 1 {
                        let focus = self.selector35_focus(selected, 0x0043_d464)?;
                        let position = self.selector35_position(actor)?;
                        let distance = word_distance(focus, position);
                        if (distance.wrapping_mul(4) as i32)
                            < (self.selector35_distance(actor)?.wrapping_mul(3) as i32)
                        {
                            if let Some(current) = self.derived_actor(actor)?.target_148 {
                                self.release_actor_service(current)?;
                            }
                            //A stopped clone retains the old released raw pointer.
                            let clone =
                                self.clone_native_target(selected, context.constructor_null)?;
                            self.derived_actor_mut(actor)?.target_148 = clone;
                        }
                    }
                }
            }
        }
        let secondary = self.derived_actor(actor)?.target_154;
        //0043D4EF loads208 on both lanes, before the260 read.
        let speed = self.family428650_actor(actor)?.words_204_208[1].ok_or(required(
            actor,
            0x208,
            0x0043_d4ef,
        ))?;
        let horizontal =
            self.selector35_actor(actor)?
                .word_260
                .ok_or(required(actor, 0x260, 0x0043_d517))?;
        if secondary.is_some() {
            if (speed.wrapping_mul(2) as i32) < horizontal as i32 {
                self.derived_actor_mut(actor)?.word_15c = Some(1);
            }
            return Ok(());
        }
        if (horizontal as i32) >= speed as i32 / 2 {
            return Ok(());
        }
        self.selector35_actor_mut(actor)?.word_234 = 750;
        self.selector35_actor_mut(actor)?.word_230 = 500;
        let velocity = self.actor(actor)?.words_8e_92;
        //This particular call lacks the high-bit square repair and then
        //sign-extends only AX. Keep it distinct from position distance.
        let squared =
            squared_word(velocity[0] as i16).wrapping_add(squared_word(velocity[2] as i16));
        let divisor = isqrt_48efc0(squared as i32) as u16 as i16 as i32;
        let divisor = if divisor == 0 { 1 } else { divisor };
        let px = self.selector35_position(actor)?[0];
        let rx = (name_entry_rng_next(context.rng) as u16) >> 4;
        let pz = self.selector35_position(actor)?[2];
        let vx = i32::from(self.actor(actor)?.words_8e_92[0] as i16).wrapping_shl(13);
        let x = px.wrapping_add(rx).wrapping_add((vx / divisor) as u16);
        let rz = (name_entry_rng_next(context.rng) as u16) >> 4;
        let vz = i32::from(self.actor(actor)?.words_8e_92[2] as i16).wrapping_shl(13);
        let z = pz.wrapping_add(rz).wrapping_add((vz / divisor) as u16);
        let current = self.selector35_current(actor, 0x0043_d5a5)?;
        let y = self.selector35_focus(current, 0x0043_d5b1)?[1];
        let target = if let Some(reference) = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(NativeTargetError::from)?
        {
            let handle = context.constructor_null.resolve(self)?;
            self.construct_native_target(
                reference,
                NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                    handle,
                    Some([x, y, z]),
                )),
            )?;
            Some(reference)
        } else {
            None
        };
        self.derived_actor_mut(actor)?.target_154 = target;
        let draw = u32::from(name_entry_rng_next(context.rng) as u16);
        self.derived_actor_mut(actor)?.word_15c = Some((draw + 0x2625a) << 4);
        Ok(())
    }
}
