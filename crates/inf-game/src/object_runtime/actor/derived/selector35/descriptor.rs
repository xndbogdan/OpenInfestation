//! Actual selector35 route/attack descriptor virtuals. The process-wide lazy
//! altitude threshold is distinct from per-actor +190/+194 and the uninitialized
//! common descriptor timer +19C. No callback here supplies that timer's backing.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionError, ActorTerrainHost, DerivedActorError, LiveGrantContext,
        LiveGrantError, NativeHeapRef, NativeTarget, NativeTargetError, ObjectArena, ObjectError,
        ObjectId, WeightedTargetConstructorNull,
    },
};

mod attack_update;
mod evade;
mod landing;
mod rescue;

/// DAT00547638/DAT0054762C are zero-initialized process storage. 0043F590
/// sets flag bit0 before the first threshold write; subsequent actors reuse it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selector35RouteProcessState {
    pub flags_547638: u8,
    pub threshold_54762c: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector35DescriptorRequest {
    ChangeObject {
        entering: bool,
    },
    UpdateObject {
        microseconds: u32,
    },
    SetRoute {
        route: ObjectId,
    },
    ChangeAttack {
        entering: bool,
    },
    UpdateAttack {
        microseconds: u32,
    },
    UpdateEvade {
        microseconds: u32,
    },
    /// Landing (descriptor 7): entry 0043FCA0 and update 0043FCC0.
    ChangeLanding {
        entering: bool,
    },
    UpdateLanding {
        microseconds: u32,
    },
    /// Rescue (descriptor 10): entry 0043FD80 and update 0043FDA0.
    ChangeRescue {
        entering: bool,
    },
    UpdateRescue {
        microseconds: u32,
    },
    /// Go to space (descriptor 11): entry 0043FE00 and update 0043FE20.
    ChangeSpace {
        entering: bool,
    },
    UpdateSpace {
        microseconds: u32,
    },
}

#[derive(Debug)]
pub struct Selector35DescriptorContext<'a> {
    pub rng: &'a mut u32,
    pub process: &'a mut Selector35RouteProcessState,
    pub constructor_null: WeightedTargetConstructorNull<'a>,
    pub settings_7c: u32,
    /// The grants Go to space's first departure gives every player
    /// (`004750C0`), with their notices.
    pub live_grants: LiveGrantContext<'a>,
}

#[derive(Debug, Error)]
pub enum Selector35DescriptorError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error(transparent)]
    Motion(#[from] crate::object_runtime::ActorMotionError),
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
    #[error("selector35 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires raw focus +08 at {address:08X}")]
    RequiredFocus {
        reference: NativeHeapRef,
        address: u32,
    },
    #[error("selector35 route terrain acquisition failed: {0}")]
    Terrain(E),
}

impl ObjectArena {
    pub fn run_selector35_descriptor<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        request: Selector35DescriptorRequest,
        context: &mut Selector35DescriptorContext<'_>,
        terrain: &mut H,
    ) -> Result<(), Selector35DescriptorError<H::Error>> {
        //43F700's zero entering byte bypasses even the receiver. 43D620's
        //zero byte instead clears264, so it must not use this early return.
        if request == (Selector35DescriptorRequest::ChangeObject { entering: false }) {
            return Ok(());
        }
        self.selector35_actor(actor)?;
        match request {
            Selector35DescriptorRequest::ChangeObject { .. } => {
                self.family428650_actor_mut(actor)?.word_228 = 0x0053_11f0;
                let Some(reference) = self.family428650_actor(actor)?.targets_1d4_1fc[5] else {
                    return Ok(());
                };
                let Some(route) = self.native_target(reference)?.object_referent() else {
                    return Ok(());
                };
                let reference = self.family428650_actor(actor)?.targets_1d4_1fc[5]
                    .ok_or(required(actor, 0x1fc, 0x0043_f732))?;
                let clone = self.clone_native_target(reference, context.constructor_null)?;
                self.derived_actor_mut(actor)?.target_148 = clone;
                self.selector35_set_route(actor, route, terrain)?;
                self.selector35_actor_mut(actor)?.word_230 = 0;
                Ok(())
            }
            Selector35DescriptorRequest::SetRoute { route } => {
                self.selector35_set_route(actor, route, terrain)
            }
            Selector35DescriptorRequest::UpdateObject { microseconds } => {
                self.selector35_update_route(actor, microseconds, context.process, terrain)
            }
            Selector35DescriptorRequest::ChangeAttack { entering } => {
                self.selector35_change_attack(actor, entering, context)
            }
            Selector35DescriptorRequest::UpdateAttack { microseconds } => {
                self.selector35_update_attack(actor, microseconds, context)
            }
            Selector35DescriptorRequest::UpdateEvade { microseconds } => {
                self.selector35_update_evade(actor, microseconds, context)
            }
            Selector35DescriptorRequest::ChangeLanding { entering } => {
                self.selector35_change_landing(actor, entering, context)
            }
            Selector35DescriptorRequest::UpdateLanding { microseconds } => {
                self.selector35_update_landing(actor, microseconds, context, terrain)
            }
            Selector35DescriptorRequest::ChangeRescue { entering } => {
                self.selector35_change_rescue(actor, entering, context, terrain)
            }
            Selector35DescriptorRequest::UpdateRescue { microseconds } => {
                self.selector35_update_rescue(actor, microseconds)
            }
            Selector35DescriptorRequest::ChangeSpace { entering } => {
                self.selector35_change_space(actor, entering, context)
            }
            Selector35DescriptorRequest::UpdateSpace { microseconds } => {
                self.selector35_update_space(actor, microseconds)
            }
        }
    }

    ///0043F4A0 captures route XZ before acquiring H12, then reads its fresh Y.
    ///The reached target replacement commits its identity even on a XYZ error.
    fn selector35_set_route<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        route: ObjectId,
        terrain: &mut H,
    ) -> Result<(), Selector35DescriptorError<H::Error>> {
        let point = self.selector35_position(route)?;
        let height = terrain
            .terrain(self, actor)
            .map_err(Selector35DescriptorError::Terrain)?
            .height_at([point[0], point[2]]);
        let y = self.selector35_position(route)?[1] as i16;
        self.selector35_actor_mut(actor)?.word_234 = (i32::from(y) - i32::from(height)) as u32;
        if (self.selector35_actor(actor)?.word_234 as i32) < 100 {
            self.selector35_actor_mut(actor)?.word_234 = 100;
        }
        if (self.selector35_actor(actor)?.word_234 as i32) > 2000 {
            self.selector35_actor_mut(actor)?.word_234 = 2000;
        }
        let height = self.selector35_actor(actor)?.word_234.wrapping_shl(14);
        self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(height);
        self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(height);
        let point = self.selector35_position(route)?;
        let actor_point = self.selector35_position(actor)?;
        self.derived_actor_mut(actor)?.word_14c = Some(word_distance(point, actor_point));
        let reference = self.selector35_current(actor, 0x0043_f572)?;
        let mut target = self.object_position_target_data(reference)?;
        let result = self.replace_object_position_target(&mut target, route);
        self.write_native_target(reference, NativeTarget::ObjectPosition(target))?;
        result?;
        Ok(())
    }

    fn selector35_update_route<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        process: &mut Selector35RouteProcessState,
        terrain: &mut H,
    ) -> Result<(), Selector35DescriptorError<H::Error>> {
        //148 is captured before208=0; a NULL dereference retains that store.
        let captured = self.derived_actor(actor)?.target_148;
        self.family428650_actor_mut(actor)?.words_204_208[1] = Some(0);
        let captured = captured.ok_or(required(actor, 0x148, 0x0043_f5a0))?;
        let source = self.native_target(captured)?.object_referent();
        self.selector35_actor_mut(actor)?.byte_238 = 1;
        let Some(source) = source else {
            self.family428650_actor_mut(actor)?.word_22e |= 4;
            return Ok(());
        };
        let distance = self.selector35_distance(actor)? as i32;
        if distance < i32::from(self.object(actor)?.base.word_28 >> 1) + 256 {
            let Some(next) = self.route_next(source)? else {
                self.family428650_actor_mut(actor)?.word_22e |= 4;
                return Ok(());
            };
            return self.selector35_set_route(actor, next, terrain);
        }
        if self.route_node(source)?.distance_squared != 0 {
            if let Some(next) = self.route_next(source)? {
                //43F60C tests the source segment again, not a new distance.
                if self.route_node(source)?.distance_squared == 0 {
                    return self.selector35_set_route(actor, next, terrain);
                }
            }
        }
        let horizontal =
            self.selector35_actor(actor)?
                .word_260
                .ok_or(required(actor, 0x260, 0x0043_f617))? as i32;
        if horizontal < 150 {
            if process.flags_547638 & 1 == 0 {
                process.flags_547638 |= 1;
                process.threshold_54762c =
                    (self.selector35_distance(actor)? as i32 / 2).wrapping_neg();
            }
            let vy = i32::from(self.actor(actor)?.words_8e_92[1] as i16);
            let height = self.selector35_family_word(actor, 3, 0x194, 0x0043_f65b)?;
            if vy > process.threshold_54762c {
                if (height as i32) <= delta as i32 {
                    self.selector35_actor_mut(actor)?.byte_238 = 0;
                    self.selector35_actor_mut(actor)?.word_234 = ((height as i32) >> 14) as u32;
                    return Ok(());
                }
                if vy > self.selector35_distance(actor)? as i32 {
                    self.family428650_actor_mut(actor)?.words_188_19c[3] =
                        Some(height.wrapping_sub(delta));
                    self.selector35_actor_mut(actor)?.word_234 =
                        ((height.wrapping_sub(delta) as i32) >> 14) as u32;
                    let fresh = self.selector35_family_word(actor, 3, 0x194, 0x0043_f69c)?;
                    self.selector35_actor_mut(actor)?.word_234 = ((fresh as i32) >> 14) as u32;
                    return Ok(());
                }
            }
            self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(height.wrapping_add(delta));
        } else if horizontal > 200 {
            let initial = self.selector35_family_word(actor, 2, 0x190, 0x0043_f6bc)?;
            self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(initial);
        }
        let height = self.selector35_family_word(actor, 3, 0x194, 0x0043_f6d2)?;
        self.selector35_actor_mut(actor)?.word_234 = ((height as i32) >> 14) as u32;
        Ok(())
    }

    fn selector35_change_attack<E>(
        &mut self,
        actor: ObjectId,
        entering: bool,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        if !entering {
            self.selector35_actor_mut(actor)?.word_264 = 0;
            return Ok(());
        }
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_1244;
        let draw = u32::from(name_entry_rng_next(context.rng) as u16) >> 8;
        self.family428650_actor_mut(actor)?.words_204_208[1] = Some(draw + 500);
        self.selector35_actor_mut(actor)?.word_260 = Some(draw + 501);
        let draw = u32::from(name_entry_rng_next(context.rng) as u16) >> 10;
        self.selector35_actor_mut(actor)?.word_230 = draw + 75;
        if self.family428650_actor(actor)?.targets_1d4_1fc[4].is_none()
            && self.family428650_actor(actor)?.targets_1d4_1fc[0].is_none()
        {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        self.selector35_reset_timer(actor, 2_000_000, context.rng)?;
        let selected = self.selector35_weighted(actor, 0, 4, 0x0043_d6f0)?;
        let clone = self.clone_native_target(selected, context.constructor_null)?;
        self.derived_actor_mut(actor)?.target_148 = clone;
        let clone = clone.ok_or(required(actor, 0x148, 0x0043_d701))?;
        if self
            .resolve_target_actor(&self.native_target(clone)?)?
            .is_some()
        {
            self.family428650_actor_mut(actor)?.byte_22c = 1;
        }
        Ok(())
    }

    fn selector35_reset_timer<E>(
        &mut self,
        actor: ObjectId,
        initial: u32,
        rng: &mut u32,
    ) -> Result<(), Selector35DescriptorError<E>> {
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(initial);
        let draw = u32::from(name_entry_rng_next(rng) as u16) << 4;
        let timer = self.selector35_family_word(actor, 1, 0x18c, 0x0043_d39b)?;
        self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_add(draw));
        Ok(())
    }

    fn selector35_weighted<E>(
        &self,
        actor: ObjectId,
        first: usize,
        preferred: usize,
        address: u32,
    ) -> Result<NativeHeapRef, Selector35DescriptorError<E>> {
        let family = self.family428650_actor(actor)?;
        let left = family.targets_1d4_1fc[first];
        let right = family.targets_1d4_1fc[preferred];
        match (left, right) {
            (Some(_), Some(right))
                if family.words_1d8_200[preferred] > family.words_1d8_200[first] =>
            {
                Ok(right)
            }
            (Some(left), _) => Ok(left),
            (None, Some(right)) => Ok(right),
            (None, None) => Err(required(actor, 0x1d4 + first as u16 * 8, address)),
        }
    }
    fn selector35_family_word<E>(
        &self,
        actor: ObjectId,
        index: usize,
        offset: u16,
        address: u32,
    ) -> Result<u32, Selector35DescriptorError<E>> {
        self.family428650_actor(actor)?.words_188_19c[index].ok_or(required(actor, offset, address))
    }
    fn selector35_distance<E>(&self, actor: ObjectId) -> Result<u32, Selector35DescriptorError<E>> {
        self.derived_actor(actor)?
            .word_14c
            .ok_or(required(actor, 0x14c, 0x0043_f5c5))
    }
    fn selector35_current<E>(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<NativeHeapRef, Selector35DescriptorError<E>> {
        self.derived_actor(actor)?
            .target_148
            .ok_or(required(actor, 0x148, address))
    }
    fn selector35_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector35DescriptorError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(required(actor, 0x32, 0x0043_f4a9))
    }
    fn selector35_focus<E>(
        &self,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<[u16; 3], Selector35DescriptorError<E>> {
        self.native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector35DescriptorError::RequiredFocus { reference, address })
    }
}

fn required<E>(actor: ObjectId, offset: u16, address: u32) -> Selector35DescriptorError<E> {
    Selector35DescriptorError::RequiredField {
        actor,
        offset,
        address,
    }
}
fn squared_word(word: i16) -> u32 {
    i32::from(word).wrapping_mul(i32::from(word)) as u32
}
fn integer_distance(squared: u32) -> u32 {
    if squared & 0x8000_0000 != 0 {
        isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
    } else {
        isqrt_48efc0(squared as i32)
    }
}
fn word_distance(a: [u16; 3], b: [u16; 3]) -> u32 {
    integer_distance(a.into_iter().zip(b).fold(0u32, |s, (a, b)| {
        s.wrapping_add(squared_word(a.wrapping_sub(b) as i16))
    }))
}

#[cfg(test)]
mod tests;
