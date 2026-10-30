use super::*;
use crate::{name_entry::name_entry_rng_next, object_runtime::Family4236d0Class};

impl ObjectArena {
    pub(super) fn family_dying_update<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        let selector36 = matches!(
            self.family4236d0_actor(actor)?.class,
            Family4236d0Class::Selector36(_)
        );
        self.shared_dying_update(actor, microseconds, host, rng)?;
        if selector36 {
            self.selector36_dying_trail(actor, host, rng)?;
        }
        Ok(())
    }

    /// 00424590. Callback mutations are observed by the next native load.
    fn shared_dying_update<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        let family = self.family4236d0_actor(actor)?;
        let stage = family.word_240;
        let table = family.table_244;
        let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(
            FamilyDyingError::RequiredField {
                actor,
                offset: 0x188,
                address: 0x4245a0,
            },
        )?;
        let threshold = host
            .read_dword(table.wrapping_add(stage.wrapping_mul(4)))
            .map_err(|error| service(0x4245a6, error))?;
        if elapsed as i32 > threshold as i32 {
            self.family428650_actor_mut(actor)?.words_188_19c[0] = Some(0);
            if stage as i32 >= 3 {
                return self.finish_family_dying(actor, host, rng);
            }
            host.stage_change(self, actor, 0, rng)
                .map_err(|error| service(0x4245c0, error))?;
            let next = self.family4236d0_actor(actor)?.word_240.wrapping_add(1);
            self.family4236d0_actor_mut(actor)?.word_240 = next;
            host.stage_change(self, actor, 1, rng)
                .map_err(|error| service(0x4245d9, error))?;
            if self.family4236d0_actor(actor)?.word_240 == 1
                && self.family4236d0_actor(actor)?.word_264 & 0x20 == 0
            {
                host.stage_change(self, actor, 0, rng)
                    .map_err(|error| service(0x4245f7, error))?;
                let next = self.family4236d0_actor(actor)?.word_240.wrapping_add(1);
                self.family4236d0_actor_mut(actor)?.word_240 = next;
                host.stage_change(self, actor, 1, rng)
                    .map_err(|error| service(0x424610, error))?;
            }
        }
        match self.family4236d0_actor(actor)?.word_240 {
            1 => {
                // Native captures each scale before its random draw, keeps
                // low32 IMUL, then performs signed SAR8 and comparison.
                for (offset, ceiling_address, step_address, address) in [
                    (0x16c, 0x5307f0, 0x5307e4, 0x424673),
                    (0x170, 0x5307ec, 0x5307e8, 0x4246b2),
                ] {
                    let derived = self.derived_actor(actor)?;
                    let current = if offset == 0x16c {
                        derived.word_16c
                    } else {
                        derived.word_170
                    }
                    .ok_or(FamilyDyingError::RequiredField {
                        actor,
                        offset,
                        address,
                    })?;
                    let random = name_entry_rng_next(rng);
                    let ceiling = host
                        .read_dword(ceiling_address)
                        .map_err(|error| service(address, error))?;
                    let bias = (0xffff_ff80u32.wrapping_sub(random) & 0xff).wrapping_mul(ceiling)
                        as i32
                        >> 8;
                    let step = host
                        .read_dword(step_address)
                        .map_err(|error| service(address, error))?;
                    let next = if (bias as u32).wrapping_add(current) as i32 > ceiling as i32 {
                        current.wrapping_sub(step)
                    } else {
                        current.wrapping_add(step)
                    };
                    let derived = self.derived_actor_mut(actor)?;
                    if offset == 0x16c {
                        derived.word_16c = Some(next);
                    } else {
                        derived.word_170 = Some(next);
                    }
                }
                let derived = self.derived_actor(actor)?;
                let volume = derived.word_16c.unwrap();
                let pitch = derived.word_170.unwrap();
                let operation = if derived.sound_168.is_some() {
                    FamilyDyingAudio::UpdatePersistent { volume, pitch }
                } else {
                    FamilyDyingAudio::CreatePersistent {
                        sound: 47,
                        volume,
                        pitch,
                    }
                };
                self.dying_effect(
                    actor,
                    FamilyDyingEffect::Audio(operation),
                    0x424700,
                    host,
                    rng,
                )?;
                if name_entry_rng_next(rng) as u8 > 250 {
                    // This seemingly unused draw is present at00424747.
                    name_entry_rng_next(rng);
                    self.dying_effect(
                        actor,
                        FamilyDyingEffect::Audio(FamilyDyingAudio::OneShot {
                            sound: 66,
                            volume: 65_536,
                            pitch: 65_536,
                        }),
                        0x42475d,
                        host,
                        rng,
                    )?;
                }
                let kind =
                    self.derived_actor(actor)?
                        .word_174
                        .ok_or(FamilyDyingError::RequiredField {
                            actor,
                            offset: 0x174,
                            address: 0x424769,
                        })?;
                if kind != 1 {
                    self.dying_effect(
                        actor,
                        FamilyDyingEffect::StatusVisual { microseconds },
                        0x424779,
                        host,
                        rng,
                    )?;
                }
            }
            2 if self.derived_actor(actor)?.sound_168.is_some() => {
                self.dying_effect(
                    actor,
                    FamilyDyingEffect::Audio(FamilyDyingAudio::StopPersistent),
                    0x424663,
                    host,
                    rng,
                )?;
                // Native writes NULL again only after normal deletion.
                self.derived_actor_mut(actor)?.sound_168 = None;
            }
            3 if self.object(actor)?.base.flags & 0x0200_0000 == 0 => {
                self.finish_family_dying(actor, host, rng)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_family_dying<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        let source = self.actor(actor)?.handle;
        self.dying_effect(
            actor,
            FamilyDyingEffect::ChangeState(ActorStateChangeRequest {
                target_state: 0,
                source,
                argument: 0,
            }),
            0x424643,
            host,
            rng,
        )
    }

    /// 00481FC0 always consumes its first RNG after normal shared return,
    /// including a state-zero transition. Integer division precedes stage>3.
    fn selector36_dying_trail<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        if name_entry_rng_next(rng) as u16 >= 0x2000 {
            return Ok(());
        }
        let random = i32::from(name_entry_rng_next(rng) as u16);
        let family = self.family4236d0_actor(actor)?;
        let stage = family.word_240;
        let table = family.table_244;
        let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(
            FamilyDyingError::RequiredField {
                actor,
                offset: 0x188,
                address: 0x481ff5,
            },
        )? as i32;
        let divisor = host
            .read_dword(table.wrapping_add(stage.wrapping_mul(4)))
            .map_err(|error| service(0x481ffb, error))? as i32
            >> 16;
        let quotient = elapsed
            .checked_div(divisor)
            .ok_or(FamilyDyingError::Division { address: 0x482008 })?;
        let kind = match stage {
            0 => {
                if random <= quotient {
                    11
                } else {
                    8
                }
            }
            1 if random < quotient / 4 => {
                let position = self.dying_position(actor, 0x482043)?;
                self.dying_effect(
                    actor,
                    FamilyDyingEffect::Burst(burst(position, 16, 2, [1, 1])),
                    0x482098,
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
                        0x4820be,
                        host,
                        rng,
                    )?;
                }
                return Ok(());
            }
            1 if random <= quotient / 2 => {
                if random <= quotient {
                    6
                } else {
                    5
                }
            }
            2 => 6,
            3 if random >= quotient => 6,
            _ => return Ok(()),
        };
        let x = 32u16.wrapping_sub((name_entry_rng_next(rng) as u16) >> 11);
        let y = 64u16.wrapping_add((name_entry_rng_next(rng) as u16) >> 9);
        let z = 32u16.wrapping_sub((name_entry_rng_next(rng) as u16) >> 11);
        self.dying_effect(
            actor,
            FamilyDyingEffect::Trail {
                kind,
                velocity: [x, y, z],
                flags: 0x800,
            },
            0x482138,
            host,
            rng,
        )
    }
}
