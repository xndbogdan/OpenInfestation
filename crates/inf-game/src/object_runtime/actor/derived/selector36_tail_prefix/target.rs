//! Native4817AD..4819E7 target/control path and complete481260 aim gates.
use super::super::kinematics::divide;
use super::*;
use inf_formats::native_basis::{inverse_sine_q31, isqrt_48efc0, q31_divide, RotationPlane};

impl ObjectArena {
    pub(super) fn selector36_target_controls<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        captured: NativeHeapRef,
        request: &mut Selector36TailPrefixRequest<'_, H>,
    ) -> Result<Option<[i32; 2]>, Selector36TailPrefixError<H::Error>> {
        let mut target = self.native_target(captured)?;
        let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
        self.write_native_target(captured, target)?;
        let status = refreshed?;
        //4817C1 fresh148 precedes4817CE150 publication and directfocus.
        let primary = self.derived_actor(actor)?.target_148.ok_or(
            Selector36TailPrefixError::RequiredField {
                actor,
                offset: 0x148,
                address: 0x4817c1,
            },
        )?;
        self.derived_actor_mut(actor)?.word_150 = Some(status);
        let (world, distance) = self.selector36_geometry::<H::Error>(actor, primary, 0x4817d4)?;
        if distance == 0 {
            return Ok(None);
        }
        self.selector36_aim::<H>(actor, distance, world, request)?;
        let (local, selected_distance, forward) =
            if let Some(captured) = self.derived_actor(actor)?.target_154 {
                let mut target = self.native_target(captured)?;
                let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
                self.write_native_target(captured, target)?;
                refreshed?;
                let secondary = self.derived_actor(actor)?.target_154.ok_or(
                    Selector36TailPrefixError::RequiredField {
                        actor,
                        offset: 0x154,
                        address: 0x48185f,
                    },
                )?;
                let (world, distance) =
                    self.selector36_geometry::<H::Error>(actor, secondary, 0x48186b)?;
                self.derived_actor_mut(actor)?.word_158 = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                let forward = i32::from(local[2]) / 2;
                let timer = self.derived_actor(actor)?.word_15c.ok_or(
                    Selector36TailPrefixError::RequiredField {
                        actor,
                        offset: 0x15c,
                        address: 0x4818f5,
                    },
                )?;
                if (timer as i32) > (request.microseconds as i32)
                    && (self.derived_actor(actor)?.word_158.unwrap() as i32)
                        > i32::from(self.object(actor)?.base.word_28)
                {
                    self.derived_actor_mut(actor)?.word_15c =
                        Some(timer.wrapping_sub(request.microseconds));
                } else {
                    if let Some(fresh) = self.derived_actor(actor)?.target_154 {
                        self.release_actor_service(fresh)?;
                    }
                    self.derived_actor_mut(actor)?.target_154 = None;
                }
                (local, distance, forward)
            } else {
                let primary = self.derived_actor(actor)?.target_148.ok_or(
                    Selector36TailPrefixError::RequiredField {
                        actor,
                        offset: 0x148,
                        address: 0x481937,
                    },
                )?;
                let (world, distance) =
                    self.selector36_geometry::<H::Error>(actor, primary, 0x481949)?;
                self.derived_actor_mut(actor)?.word_14c = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                let spacing = self.family428650_actor(actor)?.words_204_208[1].ok_or(
                    Selector36TailPrefixError::RequiredField {
                        actor,
                        offset: 0x208,
                        address: 0x4819c3,
                    },
                )? as i32;
                (
                    local,
                    distance,
                    i32::from(local[2]).wrapping_sub(spacing) / 2,
                )
            };
        let turn = if selected_distance == 0 {
            0
        } else {
            divide(
                i32::from(local[0]).wrapping_shl(15),
                selected_distance as i32,
                0x4819df,
            )?
        };
        Ok(Some([turn, forward]))
    }

    fn selector36_geometry<E>(
        &self,
        actor: ObjectId,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<([i16; 3], u32), Selector36TailPrefixError<E>> {
        let focus = self
            .native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector36TailPrefixError::RequiredTargetFocus { reference, address })?;
        let position = self.object(actor)?.base.position_words.ok_or(
            Selector36TailPrefixError::RequiredField {
                actor,
                offset: 0x32,
                address,
            },
        )?;
        let world: [i16; 3] =
            std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]) as i16);
        let squared = world.iter().fold(0_u32, |sum, &word| {
            sum.wrapping_add(i32::from(word).wrapping_mul(i32::from(word)) as u32)
        });
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        Ok((world, distance))
    }

    fn selector36_aim<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        distance: u32,
        world: [i16; 3],
        request: &mut Selector36TailPrefixRequest<'_, H>,
    ) -> Result<(), Selector36TailPrefixError<H::Error>> {
        let mut basis = NativeBasis::INITIAL; //actual402680/402690 constructor ratios
        let mut weapon_world = None;
        let predicted = if (distance as i32) < self.derived_actor(actor)?.word_164
            && self.family428650_actor(actor)?.byte_22c != 0
        {
            let point = self.actor_behavior_position(
                actor,
                ActorBehaviorPositionRequest::SelectedWeaponGroup,
            )?;
            let position = self.object(actor)?.base.position_words.ok_or(
                Selector36TailPrefixError::RequiredField {
                    actor,
                    offset: 0x32,
                    address: 0x4813a5,
                },
            )?;
            let delta: [i16; 3] =
                std::array::from_fn(|axis| point[axis].wrapping_sub(position[axis]) as i16);
            //4813DF performs its pure integer square root; its result is not
            //used as a distance or doubled bit31 repair by this caller.
            let squared = delta.iter().fold(0_u32, |sum, &v| {
                sum.wrapping_add(i32::from(v).wrapping_mul(i32::from(v)) as u32)
            });
            let _unused = isqrt_48efc0(if squared & 0x8000_0000 != 0 {
                (squared >> 2) as i32
            } else {
                squared as i32
            });
            weapon_world = Some(delta);
            self.actor(actor)?.rotating.basis.inverse_words(delta)
        } else {
            [0; 3]
        };
        let local = self.actor(actor)?.rotating.basis.inverse_words(world);
        let mut difference: [i16; 3] =
            std::array::from_fn(|axis| local[axis].wrapping_sub(predicted[axis]));
        let sign = if difference[0] > 0 { 1_i32 } else { -1_i32 };
        let behind = difference[2] < 0;
        let yaw = self.selector36_actor(actor)?.word_268 as u16 as i16;
        request
            .math
            .rotate_in_plane(&mut basis, i32::from(yaw), RotationPlane::Xz);
        difference = basis.inverse_words(difference);
        let mut angle = selector36_angle(
            i32::from(difference[0]),
            i32::from(difference[2]),
            request.inverse_sine,
        )?;
        if behind && i32::from(angle as i16).wrapping_mul(sign) < 0 {
            angle = angle.wrapping_neg();
        }
        let next = (i32::from(yaw).wrapping_add(i32::from(angle as i16).wrapping_shl(12) / 32768)
            as i16)
            .clamp(-16384, 16384) as u16;
        let state = self.selector36_tail_state_mut::<H::Error>(actor)?;
        state.word_268 = (state.word_268 & 0xffff_0000) | u32::from(next);
        //Only now capture actual547590. Pitch consumes the yaw-mutated vector.
        basis = *request.shared_basis;
        let pitch = (self.selector36_actor(actor)?.word_26c >> 16) as u16 as i16;
        request
            .math
            .rotate_in_plane(&mut basis, i32::from(pitch), RotationPlane::Yz);
        difference = basis.inverse_words(difference);
        let angle = selector36_angle(
            i32::from(difference[1]).wrapping_neg(),
            i32::from(difference[2]),
            request.inverse_sine,
        )?;
        let next = (i32::from(pitch).wrapping_add(i32::from(angle as i16).wrapping_shl(12) / 32768)
            as i16)
            .clamp(-16384, 8192) as u16;
        let range = self.derived_actor(actor)?.word_164;
        let state = self.selector36_tail_state_mut::<H::Error>(actor)?;
        state.word_26c = (state.word_26c & 0x0000_ffff) | (u32::from(next) << 16);
        if (distance as i32) >= range
            || self.family428650_actor(actor)?.byte_22c == 0
            || self.derived_actor(actor)?.word_150 != Some(3)
        {
            return Ok(());
        }
        let payload = self.selector36_actor(actor)?.payload_274.as_ref().ok_or(
            Selector36TailPrefixError::RequiredField {
                actor,
                offset: 0x274,
                address: 0x4815da,
            },
        )?;
        let flags = payload
            .allocation()
            .byte(payload.allocation_offset.wrapping_add(0x74))?;
        if flags & 1 != 0 {
            let timer = self.selector36_actor(actor)?.word_27e;
            if timer == 0 && self.object(actor)?.base.flags & 0x0200_0000 != 0 {
                if (distance as i32).wrapping_mul(4) < range.wrapping_mul(3) {
                    let random = crate::name_entry::name_entry_rng_next(request.random_state);
                    self.selector36_tail_state_mut::<H::Error>(actor)?.word_27e =
                        ((random as u16) >> 6).wrapping_add(1000);
                }
            } else if timer as i16 > 0 {
                let value = timer.wrapping_sub((request.microseconds >> 10) as u16);
                self.selector36_tail_state_mut::<H::Error>(actor)?.word_27e = value;
                if value as i16 <= 0 {
                    self.spawn_selector36_child(actor, request.child, request.random_state)
                        .map_err(Selector36TailPrefixError::Child)?;
                }
            }
        }
        let Some(behavior) = self.derived_actor(actor)?.behavior_140.as_ref() else {
            return Ok(());
        };
        let index = behavior.byte_10 as i8;
        let entry = behavior.entries_18.get(index as usize).ok_or(
            Selector36TailPrefixError::BehaviorEntry {
                actor,
                index,
                entries: behavior.entries_18.len(),
            },
        )?;
        if entry[5] != 0 || entry[0] == 0 || entry[7] >= 6_000_000 || difference[2] <= 0 {
            return Ok(());
        }
        if divide(
            i32::from(difference[0]).wrapping_shl(1),
            i32::from(difference[2]),
            0x481693,
        )? != 0
        {
            return Ok(());
        }
        if divide(i32::from(difference[1]), i32::from(difference[2]), 0x48169f)? != 0 {
            return Ok(());
        }
        //Enabled/range gate necessarily captured a real weapon delta.
        let predicted = weapon_world.expect("normal range-enabled48D6E0 captured world delta");
        let fire_world = std::array::from_fn(|axis| world[axis].wrapping_sub(predicted[axis]));
        use crate::object_runtime::{
            ActorWeaponFireContext, ActorWeaponFireMode, ActorWeaponFireRequest,
        };
        let mode = if self.derived_actor(actor)?.word_17a != 0 {
            ActorWeaponFireMode::Scattered
        } else {
            ActorWeaponFireMode::Predictive {
                publish_lead: false,
            }
        };
        //004816A9 supplies false independently of the earlier aim's behind
        //flag. Continue into counters/WeaponTick only after normal return.
        self.fire_actor_weapon(
            actor,
            ActorWeaponFireRequest {
                target: self.derived_actor(actor)?.target_148,
                distance,
                world: fire_world,
                microseconds: request.microseconds,
                mode,
            },
            &mut ActorWeaponFireContext {
                tables: request.behavior_tables,
                rng: request.random_state,
                process_velocity: request.process_velocity,
            },
        )?;
        Ok(())
    }
}

fn selector36_angle(x: i32, z: i32, table: &[i16; 1024]) -> Result<i32, InverseSineQ31Error> {
    let squared = x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z));
    let length = isqrt_48efc0(squared);
    let angle = inverse_sine_q31(q31_divide(x as u32, length) as i32, table)?;
    Ok(if z < 0 {
        if angle as i16 > 0 {
            0x8000_i32.wrapping_sub(angle)
        } else {
            (-0x8000_i32).wrapping_sub(angle)
        }
    } else {
        angle
    })
}
