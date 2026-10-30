//! 0046DAF0. Collection precedes model admission, pulse calculation, the
//! real 00488E90 particle emission and the final 0049FEA0 submission. A
//! network game first settles a collection in flight, and a collection there
//! leaves the pickup to respawn.
use super::*;
use crate::menu::MenuBasis;
use crate::object_runtime::{
    ActorHandle, ObjectDrawContext, ObjectDrawProcessState, ObjectModelPacket, PlayerId,
};
use inf_formats::native_basis::sine_q31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupCollectionEffect {
    MarkCollected {
        index: u32,
    },
    /// 00489A90 selector3, current pickup XYZ, flags0.
    Particle,
    Sound {
        sound: u32,
        volume: u32,
        pitch: u32,
    },
}
/// 0046DAF0's network reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickupNetwork {
    /// 0055F3A0.
    pub host: bool,
    /// Settings +20, respawning power-ups.
    pub respawning: bool,
}

pub struct PickupDrawRequest<'a> {
    pub context: &'a mut ObjectDrawContext,
    pub process: &'a mut ObjectDrawProcessState,
    pub sine_quarter: &'a [u16; 4096],
}
pub trait PickupDrawHost {
    type Error;
    fn network_flag(&mut self) -> u32;
    fn player_for_handle(
        &mut self,
        arena: &ObjectArena,
        handle: ActorHandle,
    ) -> Result<Option<PlayerId>, Self::Error>;
    fn grant_pickup(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        player: PlayerId,
        rng: &mut u32,
    ) -> Result<bool, Self::Error>;
    fn collection_effect(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        effect: PickupCollectionEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    fn base_color(&mut self) -> u32;
    fn animation_clock(&mut self) -> u32;
    /// The host flag and settings +20 in a network game; None stops it.
    fn pickup_network(&mut self) -> Option<PickupNetwork> {
        None
    }
    /// 0048E930: the current world's pickup bit `index` (-1 is never set).
    fn pickup_collected(&mut self, _index: u32) -> Option<bool> {
        None
    }
    /// 0048E9C0: clear that bit.
    fn clear_pickup_collected(&mut self, _index: u32) -> Option<()> {
        None
    }
    /// Current pickup XYZ, captured basis and stack velocity [0,-32,0],
    /// strength200. The lower owner preserves its live pool and RNG loan.
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        basis: &MenuBasis,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// Publish current direction548380 and stack color via490030, then submit49FEA0.
    fn model(
        &mut self,
        arena: &mut ObjectArena,
        packet: ObjectModelPacket,
        color: u32,
        context: &ObjectDrawContext,
    ) -> Result<(), Self::Error>;
}

impl ObjectArena {
    /// 00489A90 and the two sounds (00487EC0, 004923C0) of a collection or
    /// a respawn.
    fn pickup_flash<H: PickupDrawHost + ?Sized>(
        &mut self,
        object: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), PickupError<H::Error>> {
        host.collection_effect(self, object, PickupCollectionEffect::Particle, rng)
            .map_err(|error| PickupError::Service {
                address: 0x0048_9a90,
                error,
            })?;
        for add in [0, 2] {
            let kind = self.pickup_kind(object)?;
            let pitch = ((u32::from(kind) % 6 + add) << 16) >> 3;
            host.collection_effect(
                self,
                object,
                PickupCollectionEffect::Sound {
                    sound: 2,
                    volume: 65536,
                    pitch,
                },
                rng,
            )
            .map_err(|error| PickupError::Service {
                address: 0x0048_7ec0,
                error,
            })?;
        }
        Ok(())
    }

    /// `0046DAF6..0046DC5E`, a network game's prefix; true ends the draw. A
    /// guest's collection in flight (+54) ends when the host's bit arrives
    /// (+50 = 1, and the pickup goes without respawning power-ups), or
    /// clears the bit once +50 passes; else it asks again (message 37). A
    /// guest hides a collected pickup (+50 = 1) and flashes one come back;
    /// the host hides one respawning (0046D950 brings it back).
    fn network_pickup_prefix<H: PickupDrawHost + ?Sized>(
        &mut self,
        object: ObjectId,
        network: PickupNetwork,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, PickupError<H::Error>> {
        let state = self.timed_pickup(object)?;
        let (serial, clock_50, pending) = (state.save_index_48, state.clock_50, state.pending_54);
        let collected = |host: &mut H| host.pickup_collected(serial).ok_or(PickupError::Network);
        if pending != 0 {
            if collected(host)? {
                let state = self.timed_pickup_mut(object)?;
                state.pending_54 = 0;
                state.clock_50 = 1;
                if !network.respawning {
                    self.dispose(object)?;
                }
            } else if clock_50 != 0 && (clock_50 as i32) < host.animation_clock() as i32 {
                host.clear_pickup_collected(serial)
                    .ok_or(PickupError::Network)?;
                let state = self.timed_pickup_mut(object)?;
                state.clock_50 = 0;
                state.pending_54 = 0;
            } else {
                self.queue_network_send(37, crate::network::protocol::GameSend::Dword(serial));
            }
            return Ok(true);
        }
        if !network.host {
            if collected(host)? {
                self.timed_pickup_mut(object)?.clock_50 = 1;
                return Ok(true);
            }
            if clock_50 != 0 {
                self.timed_pickup_mut(object)?.clock_50 = 0;
                self.pickup_flash(object, host, rng)?;
            }
            return Ok(false);
        }
        Ok(clock_50 != 0)
    }

    /// `0046DD07..0046DE0F`: a network collection flashes, sounds and waits
    /// 200 ticks (30 for kind 0x77) in +50. The host marks it collected and,
    /// without respawning power-ups, disposes of it; a guest asks the host
    /// (message 37) and waits (+54).
    fn collect_network_pickup<H: PickupDrawHost + ?Sized>(
        &mut self,
        object: ObjectId,
        network: PickupNetwork,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), PickupError<H::Error>> {
        self.pickup_flash(object, host, rng)?;
        let wait = if self.pickup_kind(object)? != 0x77 {
            0xaa
        } else {
            0
        } + 0x1e;
        let clock = host.animation_clock();
        let state = self.timed_pickup_mut(object)?;
        state.clock_50 = clock.wrapping_add(wait);
        let serial = state.save_index_48;
        if network.host {
            host.collection_effect(
                self,
                object,
                PickupCollectionEffect::MarkCollected { index: serial },
                rng,
            )
            .map_err(|error| PickupError::Service {
                address: 0x0048_e980,
                error,
            })?;
            if !network.respawning {
                self.dispose(object)?;
            }
            return Ok(());
        }
        self.timed_pickup_mut(object)?.pending_54 = 1;
        self.queue_network_send(37, crate::network::protocol::GameSend::Dword(serial));
        Ok(())
    }

    pub fn draw_timed_pickup<H: PickupDrawHost + ?Sized>(
        &mut self,
        object: ObjectId,
        request: PickupDrawRequest<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), PickupError<H::Error>> {
        if self.timed_pickup(object)?.draw_stopped {
            return Err(PickupError::PreviouslyStopped(object));
        }
        let result = self.draw_pickup_body(object, request, host, rng);
        if result.is_err() {
            if let Ok(pickup) = self.timed_pickup_mut(object) {
                pickup.draw_stopped = true;
            }
        }
        result
    }

    fn draw_pickup_body<H: PickupDrawHost + ?Sized>(
        &mut self,
        object: ObjectId,
        request: PickupDrawRequest<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), PickupError<H::Error>> {
        let network = match host.network_flag() {
            0 => None,
            _ => Some(host.pickup_network().ok_or(PickupError::Network)?),
        };
        if let Some(network) = network {
            if self.network_pickup_prefix(object, network, host, rng)? {
                return Ok(());
            }
        }
        if let Some(player_actor) = self.first_player_primary_actor()? {
            let position = self.pickup_position(object)?;
            let other = self.pickup_position(player_actor)?;
            let d: [i32; 3] = std::array::from_fn(|axis| {
                i32::from(position[axis].wrapping_sub(other[axis]) as i16)
            });
            let radius = i32::from(self.object(player_actor)?.base.word_28)
                .wrapping_add(i32::from(self.object(object)?.base.word_28));
            let square = d[0]
                .wrapping_mul(d[0])
                .wrapping_add(d[1].wrapping_mul(d[1]))
                .wrapping_add(d[2].wrapping_mul(d[2]));
            if square <= radius.wrapping_mul(radius)
                && (self.actor(player_actor)?.word_c4 as i32) > 1
            {
                let source = self.actor(player_actor)?.handle;
                let player =
                    host.player_for_handle(self, source)
                        .map_err(|error| PickupError::Service {
                            address: 0x0046_b090,
                            error,
                        })?;
                if let Some(player) = player {
                    if host
                        .grant_pickup(self, object, player, rng)
                        .map_err(|error| PickupError::Service {
                            address: 0x0046_b0f0,
                            error,
                        })?
                    {
                        if let Some(network) = network {
                            return self.collect_network_pickup(object, network, host, rng);
                        }
                        let index = self.timed_pickup(object)?.save_index_48;
                        host.collection_effect(
                            self,
                            object,
                            PickupCollectionEffect::MarkCollected { index },
                            rng,
                        )
                        .map_err(|error| PickupError::Service {
                            address: 0x0048_e980,
                            error,
                        })?;
                        host.collection_effect(self, object, PickupCollectionEffect::Particle, rng)
                            .map_err(|error| PickupError::Service {
                                address: 0x0048_9a90,
                                error,
                            })?;
                        for add in [0, 2] {
                            let kind = self.pickup_kind(object)?;
                            let pitch = ((u32::from(kind) % 6 + add) << 16) >> 3;
                            host.collection_effect(
                                self,
                                object,
                                PickupCollectionEffect::Sound {
                                    sound: 2,
                                    volume: 65536,
                                    pitch,
                                },
                                rng,
                            )
                            .map_err(|error| PickupError::Service {
                                address: 0x0048_7ec0,
                                error,
                            })?;
                        }
                        self.dispose(object)?;
                        return Ok(());
                    }
                }
            }
        }
        let model = self.timed_pickup(object)?.visual.model_word;
        if model == u16::MAX {
            self.set_flag0(object, false)?;
            return Ok(());
        }
        let position = self
            .pickup_position(object)?
            .map(|word| i32::from(word as i16));
        let mut color = host.base_color().to_le_bytes();
        let angle = host.animation_clock().wrapping_mul(200) as i32;
        let pulse =
            ((i64::from(sine_q31(angle, request.sine_quarter)) * 62 >> 31) as u8).wrapping_add(128);
        let kind = self.pickup_kind(object)?;
        // The pickups' +48 (0046E0F0): kinds 0x0E..0x10 the process basis
        // 00547590, the others the shared spin at 0054ED20.
        let basis = if (0x0e..=0x10).contains(&kind) {
            color = [
                if kind == 0x10 { pulse } else { 128 },
                if kind == 0x0f { pulse } else { 128 },
                if kind == 0x0e { pulse } else { 128 },
                0,
            ];
            MenuBasis::INITIAL
        } else {
            let basis = request.process.pickup_basis;
            if self.pickup_kind(object)? != 7 {
                host.emit_trail(self, object, &basis, rng)
                    .map_err(|error| PickupError::Service {
                        address: 0x0048_8e90,
                        error,
                    })?;
            }
            color[..3].copy_from_slice(&[pulse, 255 - pulse, 255 - pulse]);
            basis
        };
        host.model(
            self,
            ObjectModelPacket {
                source: object,
                model_slot: model as i16,
                position,
                basis,
            },
            u32::from_le_bytes(color),
            request.context,
        )
        .map_err(|error| PickupError::Service {
            address: 0x0049_fea0,
            error,
        })
    }
    fn pickup_position<E>(&self, object: ObjectId) -> Result<[u16; 3], PickupError<E>> {
        self.object(object)?
            .base
            .position_words
            .ok_or(PickupError::RequiredField {
                object,
                offset: 0x32,
            })
    }
    fn pickup_kind<E>(&self, object: ObjectId) -> Result<u16, PickupError<E>> {
        self.timed_pickup(object)?
            .kind_44
            .ok_or(PickupError::RequiredField {
                object,
                offset: 0x44,
            })
    }
}
