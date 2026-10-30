use super::*;
use crate::object_runtime::Family4236d0Class;

impl ObjectArena {
    pub(super) fn family_dying_stage<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        entering: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        let selector10 = matches!(
            self.family4236d0_actor(actor)?.class,
            Family4236d0Class::Selector10(_)
        );
        let selector36 = matches!(
            self.family4236d0_actor(actor)?.class,
            Family4236d0Class::Selector36(_)
        );
        if entering as u8 != 0 {
            match self.family4236d0_actor(actor)?.word_240 {
                0 => {
                    if selector10 && self.family4236d0_actor(actor)?.word_264 & 8 != 0 {
                        //446C60/445B90 already own all eight child frees.
                        drop(self.selector10_actor_mut(actor)?.helper_26c.take());
                    }
                    if self.family4236d0_actor(actor)?.word_264 & 2 != 0 {
                        self.dying_model_transition(actor, host, rng)?;
                    }
                }
                1 => {
                    let basis = host.actor_basis(self, actor).map_err(|error| {
                        service(if selector10 { 0x4240ae } else { 0x423f0e }, error)
                    })?;
                    let mut vector = [basis.words[7], basis.words[8], basis.words[9]];
                    for _ in 0..3 {
                        let scale = f32::from_bits(
                            host.read_dword(0x520cc0)
                                .map_err(|error| service(0x4240d5, error))?,
                        );
                        let position = self.dying_position(actor, 0x4240db)?;
                        let position = std::array::from_fn(|axis| {
                            position[axis].wrapping_add(super::super::native_trunc_low32(
                                f64::from(f32::from_bits(vector[axis])) * f64::from(scale),
                            ) as u16)
                        });
                        self.dying_effect(
                            actor,
                            FamilyDyingEffect::Pickup {
                                position,
                                kind: 0x83,
                                value: 500,
                            },
                            0x46d870,
                            host,
                            rng,
                        )?;
                        // These two native callers deliberately use different
                        // startup vectors: 424080 uses55F280,423EE0 uses55F290.
                        host.rotate_pickup_vector(
                            &mut vector,
                            if selector10 { 0x55f280 } else { 0x55f290 },
                            0x5555,
                        )
                        .map_err(|error| service(0x491d10, error))?;
                    }
                    if self.family4236d0_actor(actor)?.word_264 & 0x20 != 0 {
                        let position = self.dying_position(actor, 0x424168)?;
                        self.dying_effect(
                            actor,
                            FamilyDyingEffect::Burst(burst(position, 16, 1, [1, 1])),
                            0x48a8b0,
                            host,
                            rng,
                        )?;
                        if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
                            let source = self.actor(actor)?.handle;
                            self.dying_effect(
                                actor,
                                FamilyDyingEffect::Radius {
                                    block_address: 0x531250,
                                    source,
                                },
                                0x4887d0,
                                host,
                                rng,
                            )?;
                        }
                    }
                    if selector10 && self.family4236d0_actor(actor)?.word_264 & 0x10 != 0 {
                        drop(self.selector10_actor_mut(actor)?.helper_26c.take());
                    }
                    if self.family4236d0_actor(actor)?.word_264 & 4 != 0 {
                        self.dying_model_transition(actor, host, rng)?;
                    }
                }
                _ => {}
            }
        }
        if selector36 {
            //00481EA0 calls the complete shared423EE0 before its fresh tests.
            if self.family4236d0_actor(actor)?.word_240 == 0
                && entering as u8 != 0
                && self.actor(actor)?.word_b4 != 0
            {
                let payload = self.selector36_actor(actor)?.payload_274.as_ref().ok_or(
                    FamilyDyingError::RequiredField {
                        actor,
                        offset: 0x274,
                        address: 0x481eca,
                    },
                )?;
                let flags = payload
                    .allocation()
                    .byte(payload.allocation_offset.wrapping_add(0x74))
                    .map_err(DerivedActorError::from)?;
                if flags & 2 != 0 && self.family4236d0_actor(actor)?.word_264 & 0x1000 == 0 {
                    self.dying_effect(
                        actor,
                        FamilyDyingEffect::Selector36Child,
                        0x481ee1,
                        host,
                        rng,
                    )?;
                }
            }
            if self.family4236d0_actor(actor)?.word_240 == 1 && entering as u8 != 0 {
                let position = self.dying_position(actor, 0x481efb)?;
                let mut child_position = position;
                child_position[1] = child_position[1].wrapping_sub(50);
                let mut burst = burst(position, 4, 2, [1, 1]);
                for _ in 0..4 {
                    self.dying_effect(
                        actor,
                        FamilyDyingEffect::StageChild {
                            resource: 58,
                            position: child_position,
                        },
                        0x481f6f,
                        host,
                        rng,
                    )?;
                    self.dying_effect(actor, FamilyDyingEffect::Burst(burst), 0x481f76, host, rng)?;
                    burst.position[1] = burst.position[1].wrapping_add(100);
                    child_position[1] = child_position[1].wrapping_add(100);
                }
                if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
                    let source = self.actor(actor)?.handle;
                    self.dying_effect(
                        actor,
                        FamilyDyingEffect::Radius {
                            block_address: 0x531250,
                            source,
                        },
                        0x481fac,
                        host,
                        rng,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn dying_model_transition<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        let model = self.family4236d0_actor(actor)?.word_262;
        self.owned_mut(actor)?.state.base.dirty_word |= 0x100;
        self.actor_mut(actor)?.rotating.visual.model_word = model;
        self.dying_effect(actor, FamilyDyingEffect::RefreshModel, 0x47a110, host, rng)?;
        self.actor_mut(actor)?.word_74 = 0;
        Ok(())
    }
}
