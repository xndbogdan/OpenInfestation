use super::*;
use crate::object_runtime::{mark_flag_change, Family4236d0Class};
use inf_formats::native_basis::isqrt_48efc0;

impl ObjectArena {
    pub(super) fn family_dying_entry<H: FamilyDyingHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        entering: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), FamilyDyingError<H::Error>> {
        // Shared424280 and wrapper425840 both test only the low byte.
        if entering as u8 == 0 {
            return Ok(());
        }
        let selector10 = matches!(
            self.family4236d0_actor(actor)?.class,
            Family4236d0Class::Selector10(_)
        );
        self.family4236d0_actor_mut(actor)?.word_240 = 0;
        host.stage_change(self, actor, 1, rng)
            .map_err(|error| service(0x4242a5, error))?;
        self.family428650_actor_mut(actor)?.word_228 = 0x0053_0d3c;
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags | 0x100);
        let kind = self
            .derived_actor(actor)?
            .word_174
            .ok_or(FamilyDyingError::RequiredField {
                actor,
                offset: 0x174,
                address: 0x4242cc,
            })?;
        if kind == 2 {
            self.dying_effect(
                actor,
                FamilyDyingEffect::CameraEffect {
                    strength: 0xf00,
                    argument: 0,
                },
                0x4242e7,
                host,
                rng,
            )?;
            let source = self.actor(actor)?.handle;
            self.dying_effect(
                actor,
                FamilyDyingEffect::Radius {
                    block_address: 0x5307d8,
                    source,
                },
                0x424300,
                host,
                rng,
            )?;
        }
        let sound = self.derived_actor(actor)?.sound_words_17c_182.ok_or(
            FamilyDyingError::RequiredField {
                actor,
                offset: 0x182,
                address: 0x424305,
            },
        )?[3];
        if sound != 0
            && host
                .sound_present(sound as i16)
                .map_err(|error| service(0x424317, error))?
        {
            let sound = self.derived_actor(actor)?.sound_words_17c_182.ok_or(
                FamilyDyingError::RequiredField {
                    actor,
                    offset: 0x182,
                    address: 0x424320,
                },
            )?[3] as i16 as i32;
            self.dying_effect(
                actor,
                FamilyDyingEffect::Audio(FamilyDyingAudio::OneShot {
                    sound,
                    volume: 65_536,
                    pitch: 65_536,
                }),
                0x424334,
                host,
                rng,
            )?;
        }
        let position = self.dying_position(actor, 0x424340)?;
        self.derived_actor_mut(actor)?.word_16c = Some(0x8000);
        self.derived_actor_mut(actor)?.word_170 = Some(0x8000);
        let flags = self.family4236d0_actor(actor)?.word_264;
        let types = if flags & 0x40 != 0 { [5, 5] } else { [1, 1] };
        let mut burst = burst(position, 16, 1, types);
        if flags & 0x0a != 0 {
            burst.position[1] = burst.position[1].wrapping_sub(25);
            self.dying_effect(actor, FamilyDyingEffect::Burst(burst), 0x4243bd, host, rng)?;
            burst.position[1] = burst.position[1].wrapping_add(75);
        }
        self.dying_effect(actor, FamilyDyingEffect::Burst(burst), 0x4243cc, host, rng)?;
        if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
            let source = self.actor(actor)?.handle;
            self.dying_effect(
                actor,
                FamilyDyingEffect::Radius {
                    block_address: 0x531250,
                    source,
                },
                0x4243ee,
                host,
                rng,
            )?;
        }
        let count = (self.family4236d0_actor(actor)?.word_264 & 0x0380) >> 7;
        if count != 0 {
            let player = host
                .player_position(self)
                .map_err(|error| service(0x42440d, error))?;
            let position = self.dying_position(actor, 0x42441a)?;
            let spawn_position = if let Some(player) = player {
                let delta = std::array::from_fn::<_, 3, _>(|axis| {
                    player[axis].wrapping_sub(position[axis]) as i16 as i32
                });
                // Native adds Z²+X², then Y², and scales a sign-overflowed
                // squared sum down before the integer square root.
                let square = delta[2]
                    .wrapping_mul(delta[2])
                    .wrapping_add(delta[0].wrapping_mul(delta[0]))
                    .wrapping_add(delta[1].wrapping_mul(delta[1]));
                let length = if square < 0 {
                    isqrt_48efc0(((square as u32) >> 2) as i32).wrapping_mul(2)
                } else {
                    isqrt_48efc0(square)
                } as i32;
                let radius = i32::from(self.object(actor)?.base.word_28);
                let mut result = position;
                for axis in 0..3 {
                    let increment = if length == 0 {
                        delta[axis]
                    } else {
                        delta[axis].wrapping_mul(radius).checked_div(length).ok_or(
                            FamilyDyingError::Division {
                                address: [0x424497, 0x4244a5, 0x4244b5][axis],
                            },
                        )?
                    };
                    result[axis] = result[axis].wrapping_add(increment as u16);
                }
                result
            } else {
                position
            };
            for _ in 0..count {
                let resource = if self.actor(actor)?.word_b4 == 18 {
                    84
                } else {
                    2
                };
                let source = host
                    .target_null_handle()
                    .ok_or(FamilyDyingError::MissingProcessNull)?;
                let raw = host
                    .create_death_object(
                        self,
                        FamilyDyingSpawnRequest {
                            resource,
                            position: spawn_position,
                            source,
                        },
                        rng,
                    )
                    .map_err(|error| service(0x424538, error))?;
                if let Some(raw) = raw {
                    if let Some(child) = host
                        .resolve_spawned_actor(self, raw)
                        .map_err(|error| service(0x424545, error))?
                    {
                        let owner = self.actor(actor)?.word_b4;
                        self.owned_mut(child)?.state.base.dirty_word |= 2;
                        self.actor_mut(child)?.word_b4 = owner;
                        self.dying_effect(
                            actor,
                            FamilyDyingEffect::AttachSpawnedActor { child },
                            0x42455e,
                            host,
                            rng,
                        )?;
                    }
                }
            }
        }
        if self.derived_actor(actor)?.sound_168.is_some() {
            self.dying_effect(
                actor,
                FamilyDyingEffect::Audio(FamilyDyingAudio::StopPersistent),
                0x42457b,
                host,
                rng,
            )?;
            self.derived_actor_mut(actor)?.sound_168 = None;
        }
        if selector10 {
            // Both helper tables524EA8/524EC8 +08 select445F60 (MOV byte0C).
            if let Some(helper) = &mut self.selector10_actor_mut(actor)?.helper_26c {
                helper.byte_0c = 0;
            }
        }
        Ok(())
    }
}
