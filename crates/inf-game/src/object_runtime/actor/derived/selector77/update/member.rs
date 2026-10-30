//! Selector 80's +24 (00409070): a member world 19's spawner raises. Put a
//! radius (+28) under the liquid, it rises 1/2048 of the delta a tick until
//! it reaches the surface, then aims as selector 77 does (00407CA0). After
//! 15 s up, or 5 s up without a target, it sinks at the same rate, and once
//! it is no longer drawn near or its origin lies 100 under the liquid it
//! disposes of itself. Ground above the liquid under it retires it.

use super::*;

/// 0040909D and 0040911E: the members rise and sink `delta >> 11` a tick.
const RISE_SHIFT: u32 = 11;
/// 004091F3: fifteen seconds up.
const UP_TIME: i32 = 15_000_000;
/// 0040923C: five seconds up without a target.
const IDLE_TIME: i32 = 5_000_000;
/// 00409154: a sinking member's origin may lie this far under the liquid.
const SINK_DEPTH: i32 = 100;
/// 00409123: +2C bit 0x2000000, set while it is drawn near.
const DRAWN_NEAR: u32 = 0x0200_0000;

impl ObjectArena {
    /// 00409070, the member's +24.
    pub fn tick_selector80<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: Selector77TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        let member = self.selector80(actor)?;
        let step = (request.microseconds >> RISE_SHIFT) as i32;
        if member.rise_1bc != 0 {
            let y = i32::from(self.selector80_position(actor)?[1] as i16);
            let y = if member.rise_1bc > 0 {
                let level = i32::from(liquid(host, actor)?);
                if member.up_1b8 == 0 {
                    self.selector80_mut(actor)?.up_1b8 = 1;
                    level.wrapping_sub(i32::from(self.object(actor)?.base.word_28))
                } else if y.wrapping_add(step) >= level {
                    self.selector80_mut(actor)?.rise_1bc = 0;
                    level
                } else {
                    y.wrapping_add(step)
                }
            } else {
                let sunk = if self.object(actor)?.base.flags & DRAWN_NEAR == 0 {
                    true
                } else {
                    let origin = self.selector77_actor(actor)?.position_192.ok_or(
                        Selector77UpdateError::RequiredField {
                            actor,
                            offset: 0x194,
                        },
                    )?;
                    i32::from(origin[1] as i16).wrapping_add(SINK_DEPTH)
                        < i32::from(liquid(host, actor)?)
                };
                if sunk {
                    self.dispose(actor)?;
                }
                y.wrapping_sub(step)
            };
            if let Some(position) = self.owned_mut(actor)?.state.base.position_words.as_mut() {
                position[1] = y as u16;
            }
        }
        self.tick_selector77(actor, request, host)?;
        let [x, _, z] = self.selector80_position(actor)?;
        let ground = host
            .ground_height([x, z])
            .ok_or(Selector77UpdateError::VacantTerrain(actor))?;
        if ground > liquid(host, actor)? {
            let effect = FamilyUpdateEffect::Retire;
            host.effect(self, actor, effect)
                .map_err(|error| Selector77UpdateError::Host { effect, error })?;
        }
        if self.selector80(actor)?.rise_1bc != 0 {
            return Ok(());
        }
        let member = self.selector80_mut(actor)?;
        if member.up_1b8 as i32 > UP_TIME {
            member.rise_1bc = -1;
        } else {
            member.up_1b8 = member.up_1b8.wrapping_add(request.microseconds);
        }
        if self.derived_actor(actor)?.target_148.is_some() {
            self.selector80_mut(actor)?.idle_1b4 = 0;
            return Ok(());
        }
        let member = self.selector80_mut(actor)?;
        member.idle_1b4 = member.idle_1b4.wrapping_add(request.microseconds);
        if member.idle_1b4 as i32 > IDLE_TIME {
            member.rise_1bc = -1;
        }
        Ok(())
    }

    fn selector80_position(&self, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }
}

fn liquid<H: Selector77UpdateHost>(
    host: &H,
    actor: ObjectId,
) -> Result<i16, Selector77UpdateError<H::Error>> {
    host.liquid_level()
        .ok_or(Selector77UpdateError::VacantTerrain(actor))
}
