//! Selector 79's +24 (00408AB0), its strike (00408DA0) and its light +28
//! (00408790), over selector 77's reacquisition (00407600), target controls
//! (00407760) and idle (00407B00). The pair's leader aims for the first
//! player's unit; its follower copies the leader's target each update.
//! When that target crosses the plane between them, within three quarters
//! of their distance across and 1000 up or down, both strike a second
//! later: a line of trail particles from each eye's origin +192 to the
//! target, and from the leader 20000 of damage. Ten seconds aimed at one
//! target strike it too.

use super::*;
use crate::object_runtime::{
    Selector79Message, TargetNullHandleSource, WeightedTargetConstructorNull,
};
use crate::world_lighting::TerrainLightRequest;

/// 00408B28: the leader's next search when its reacquisition finds nothing.
const SEARCH_DELAY: u32 = 0x51615;
/// 00408C74: half the height of the pair's plane.
const PLANE_HEIGHT: i32 = 1000;
/// 00408CD2 and 00408CF3: the strike arms for 1 s, then cools to 7.5 s.
const STRIKE_ARMED: i32 = 1_000_000;
const STRIKE_COOLED: i32 = 7_500_000;
/// 00408D25: ten seconds aimed at one target strike it.
const AIM_STRIKE: u32 = 10_000_000;
/// 00408F2C: the strike's trail particles.
const STRIKE_TRAIL: u32 = 12;
/// 00408FA3: the leader's damage, on channel 2.
const STRIKE_DAMAGE: i16 = 20_000;
const STRIKE_CHANNEL: u8 = 2;
/// 00408864: the light's radius.
const LIGHT_RADIUS: i32 = 0x200;

/// The clone's 0054E894 (00482890 and kind 8 reset their handle to it).
struct ProcessNull(Option<ActorHandle>);

impl TargetNullHandleSource for ProcessNull {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.0
    }
}

impl ObjectArena {
    /// 00408AB0, the eye's +24.
    pub fn tick_selector79<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: Selector77TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        self.selector79(actor)?;
        let delta = request.microseconds;
        let null = request
            .process_null_handle
            .ok_or(Selector77UpdateError::MissingProcessNullHandle)?;
        let source = self.selector77_actor(actor)?.handle_1b0;
        if source != null {
            host.virtual_b4(
                self,
                actor,
                ActorDamageCallback {
                    adjusted_damage: 10_000,
                    packet: ActorDamagePacket {
                        strength: 10_000,
                        channel: 9,
                        byte_03: None,
                    },
                    source,
                },
            )?;
        }
        self.tick_actor_status(actor, delta, request.status_load_factor, host)
            .map_err(Selector77UpdateError::Status)?;
        if self.actor(actor)?.word_c4 as i32 <= 2 {
            return Ok(());
        }
        if self.selector79(actor)?.leader_1d4 != 0 {
            let timer = self.selector77_actor(actor)?.word_198;
            if timer < delta {
                self.selector77_reacquire(actor, host)?;
                if self.derived_actor(actor)?.target_148.is_none() {
                    self.selector77_update_state_mut(actor)?.word_198 = SEARCH_DELAY;
                }
            } else {
                self.selector77_update_state_mut(actor)?.word_198 = timer - delta;
            }
        } else {
            // 00408B3E: the follower copies its partner's target, or leads
            // when the partner is gone.
            let partner = self.selector79(actor)?.partner_1c4;
            let partner = self.resolve_actor_handle(ActorHandle::from_raw(partner))?;
            self.selector79_mut(actor)?.leader_1d4 = 1;
            if let Some(partner) = partner {
                // An eye's +58 (00404610) answers itself and its +D0(0)
                // (00402790) its +148; another partner's are not ported.
                self.selector79(partner)?;
                self.selector79_mut(actor)?.leader_1d4 = 0;
                let target = self.derived_actor(partner)?.target_148;
                if let Some(old) = self.derived_actor_mut(actor)?.target_148.take() {
                    self.release_actor_service(old)?;
                }
                let copy = match target {
                    Some(target) => self.clone_native_target(
                        target,
                        WeightedTargetConstructorNull::Process(&ProcessNull(Some(null))),
                    )?,
                    None => None,
                };
                self.derived_actor_mut(actor)?.target_148 = copy;
            }
        }
        let Some(reference) = self.derived_actor(actor)?.target_148 else {
            // 00408D69.
            self.selector79_mut(actor)?.target_1c8 = null;
            self.selector77_update_state_mut(actor)?.word_1ac = Some(delta >> 6);
            return self.selector77_idle(actor, delta, request.sine_quarter, host);
        };
        self.selector77_update_state_mut(actor)?.word_1ac = Some(delta >> 5);
        let mut target = self.native_target(reference)?;
        let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
        self.write_native_target(reference, target)?;
        let leader = self.selector79(actor)?.leader_1d4 != 0;
        if refreshed? == 0 {
            let timer = self.selector77_actor(actor)?.word_198;
            self.selector77_update_state_mut(actor)?.word_198 = timer.wrapping_sub(delta);
        } else if leader {
            let handle = target.handle_04();
            let pair = self.selector79_mut(actor)?;
            if pair.target_1c8 != handle {
                pair.target_1c8 = handle;
                pair.aimed_1c0 = Some(0);
                pair.side_1bc = Some(0);
            }
        }
        if leader {
            if self.selector79(actor)?.strike_1d0 == 0 {
                self.selector79_crossing(actor, reference)?;
            }
            let clock = self.selector79(actor)?.strike_1d0 as i32;
            if clock != 0 {
                if clock <= STRIKE_ARMED {
                    let clock = clock.wrapping_add(delta as i32);
                    self.selector79_mut(actor)?.strike_1d0 = clock as u32;
                    if clock > STRIKE_ARMED {
                        self.selector79_strike(actor, request.process_null_handle, host)?;
                    }
                } else {
                    let clock = clock.wrapping_add(delta as i32);
                    self.selector79_mut(actor)?.strike_1d0 = if clock > STRIKE_COOLED {
                        0
                    } else {
                        clock as u32
                    };
                }
            }
        }
        let (_, aligned) = self.selector77_target_controls(actor, &request, host)?;
        if !aligned || self.selector79(actor)?.leader_1d4 == 0 {
            return Ok(());
        }
        let aimed =
            self.selector79(actor)?
                .aimed_1c0
                .ok_or(Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x1c0,
                })?;
        if aimed <= AIM_STRIKE {
            self.selector79_mut(actor)?.aimed_1c0 = Some(aimed.wrapping_add(delta));
        } else if self.selector79(actor)?.strike_1d0 == 0 {
            let pair = self.selector79_mut(actor)?;
            pair.aimed_1c0 = Some(0);
            pair.struck_1cc = pair.target_1c8;
            self.selector79_strike(actor, request.process_null_handle, host)?;
        }
        Ok(())
    }

    /// 00408C0C: the target's point in the leader's frame (0047A660) about
    /// the pair's midpoint. Within three quarters of their distance across
    /// and 1000 up or down, a change of side arms the strike at the target.
    fn selector79_crossing<E>(
        &mut self,
        actor: ObjectId,
        reference: NativeHeapRef,
    ) -> Result<(), Selector77UpdateError<E>> {
        let focus = self.native_target(reference)?.raw_focus_08_words().ok_or(
            Selector77UpdateError::RequiredTargetWords {
                reference,
                field: "focus08",
            },
        )?;
        let pair = self.selector79(actor)?;
        let (midpoint, reach) =
            pair.midpoint_1b4
                .zip(pair.reach_1ba)
                .ok_or(Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x1b4,
                })?;
        let offset: [i16; 3] =
            std::array::from_fn(|axis| focus[axis].wrapping_sub(midpoint[axis]) as i16);
        let [x, y, z] = self.actor(actor)?.rotating.basis.inverse_words(offset);
        if wrapping_abs(i32::from(x)) >= i32::from(reach as i16)
            || wrapping_abs(i32::from(y)) >= PLANE_HEIGHT
        {
            self.selector79_mut(actor)?.side_1bc = Some(0);
            return Ok(());
        }
        if z == 0 {
            return Ok(());
        }
        let side =
            self.selector79(actor)?
                .side_1bc
                .ok_or(Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x1bc,
                })?;
        let pair = self.selector79_mut(actor)?;
        if side != 0 && ((side ^ z as u16) as i16) < 0 && pair.strike_1d0 == 0 {
            pair.strike_1d0 = 1;
            pair.struck_1cc = pair.target_1c8;
        }
        pair.side_1bc = Some(z as u16);
        Ok(())
    }

    /// 00408DA0: with the struck actor +1CC and the partner both live, a
    /// line of type-12 trail particles from the origin +192 to the struck
    /// actor, one per 50 of the distance (1 to 20, both ends); a leader then
    /// has its follower strike too (its +98, message 2) and deals 20000 on
    /// channel 2 (0047C6E0). The clock +1D0 goes past its arming either way.
    fn selector79_strike<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        process_null_handle: Option<ActorHandle>,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        let pair = self.selector79(actor)?;
        let (struck, partner) = (pair.struck_1cc, pair.partner_1c4);
        let struck = self.resolve_actor_handle(struck)?;
        let partner = self.resolve_actor_handle(ActorHandle::from_raw(partner))?;
        if self.selector79(actor)?.strike_1d0 as i32 <= STRIKE_ARMED {
            self.selector79_mut(actor)?.strike_1d0 = STRIKE_ARMED as u32 + 1;
        }
        let (Some(struck), Some(partner)) = (struck, partner) else {
            return Ok(());
        };
        let origin = self.selector77_actor(actor)?.position_192.ok_or(
            Selector77UpdateError::RequiredField {
                actor,
                offset: 0x192,
            },
        )?;
        let position = self
            .object(struck)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(struck))?;
        let velocity = self.actor(struck)?.words_8e_92;
        let d: [i32; 3] =
            std::array::from_fn(|axis| i32::from(origin[axis].wrapping_sub(position[axis]) as i16));
        let squared = d.iter().fold(0_i32, |sum, &axis| {
            sum.wrapping_add(axis.wrapping_mul(axis))
        });
        // A sum with bit 31 set is quartered before the root, then doubled.
        let distance = if squared < 0 {
            isqrt_48efc0(((squared as u32) >> 2) as i32) << 1
        } else {
            isqrt_48efc0(squared)
        } as i32;
        let steps = (distance / 50).clamp(1, 20);
        let mut sum = [0_i32; 3];
        for step in 0..=steps {
            let point: [u16; 3] =
                std::array::from_fn(|axis| origin[axis].wrapping_sub((sum[axis] / steps) as u16));
            let velocity: [u16; 3] = std::array::from_fn(|axis| {
                (i32::from(velocity[axis] as i16).wrapping_mul(step) / steps).wrapping_add(d[axis])
                    as u16
            });
            let effect = FamilyUpdateEffect::StatusTrail {
                particle_type: STRIKE_TRAIL,
                position: point,
                velocity,
                flags: 0,
            };
            host.effect(self, actor, effect)
                .map_err(|error| Selector77UpdateError::Host { effect, error })?;
            for axis in 0..3 {
                sum[axis] = sum[axis].wrapping_add(d[axis]);
            }
        }
        if self.selector79(actor)?.leader_1d4 == 0 {
            return Ok(());
        }
        if self.selector79_message(partner, Some(struck), 2)? == Selector79Message::Strike {
            self.selector79_strike(partner, process_null_handle, host)?;
        }
        let source = self.actor(actor)?.handle;
        // The packet's last byte is the stack's: the Z sum's top byte.
        let packet = ActorDamagePacket {
            strength: STRIKE_DAMAGE,
            channel: STRIKE_CHANNEL,
            byte_03: Some((sum[2] as u32 >> 24) as u8),
        };
        host.apply_damage(self, struck, packet, source)
    }

    /// 00408790, the eye's +28: a light of radius 0x200 at it, coloured by
    /// three sines of its phase +19C a third of a turn apart, each >> 12.
    pub(in crate::object_runtime) fn selector79_light(
        &self,
        actor: ObjectId,
        phase: u16,
        sine: &[u16; 4096],
    ) -> Result<TerrainLightRequest, ObjectError> {
        let channel = |turn: u16| {
            let value = selector77_sine(u32::from(phase.wrapping_add(turn)), sine) as u16;
            ((value as i16) >> 12) as i8
        };
        let [x, _, z] = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        Ok(TerrainLightRequest {
            world_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius: LIGHT_RADIUS,
            rgb: [channel(0), channel(0x5555), channel(0xaaaa)],
        })
    }
}
