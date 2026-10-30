//! The course target's life: its damage guard 0043BD90 (+B4), its death
//! 0043B910 (the +AC that state 1 calls), its update 0043BB80 (+24) and its
//! fuse's end 0043BFE0.
//!
//! Level 1's seven targets have the radiation off (+165) and a fuse (+163).
//! A target shot to death sends message 8 to its links, marks its death and
//! lights a fuse of +160 seconds. The fuse sounds 0x32 as it passes 0.2 s,
//! then the target comes back: state 3, full health and message 7 to its
//! links. The course bridge opens while all three of its targets are down.
//!
//! A generator has no countdown (+160 is 0). Within 1000 of the player it
//! irradiates the player's actor. Shot to death, it scorches the ground
//! (`0047E190(5)`) and burns for 2 s in the family update `0040DB40`, then
//! blows up into its wreck, which leaves the update list. A generator that
//! others hold up (message 7) dies once the last of them is gone.

use thiserror::Error;

use super::super::{
    family40d800::FamilyLinkChange, ActorDamageCallback, ActorDamagePacket,
    ActorDamageReactionError, ActorDamageReactionHost, ActorStateChangeError,
    ActorStateChangeRequest, ActorTimedDamageRequest,
};
use super::Selector39LinkDelivery;
use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId, PlayerId, WorldLoadError},
    process_particles::births::ParticleBurst,
    world_radius_effect::radius_word_distance,
};
use inf_formats::native_basis::{NativeBasisMath, RotationPlane};

/// 00487EC0's sound for the fuse: at the death and as it passes 0.2 s.
pub const FUSE_SOUND: u32 = 0x32;
/// The fuse sounds once it passes this many microseconds.
const FUSE_WARNING: u32 = 200_000;
/// 0043BA7B: the ground scorch's `0047E190` argument, six cells.
const DEATH_SCORCH: i32 = 5;
/// 0043BA84: the burn before the wreck, in microseconds.
const WRECK_FUSE: u32 = 2_000_000;
/// 0040DB7E..0040DB93: the explosion's two child visuals (`+4C(0, r, 0)`).
pub const WRECK_VISUALS: [u32; 2] = [0x36, 0x37];
/// 0040DC20: the wreck model when command 19 left `+144` unset.
const WRECK_MODEL: u16 = 0x46;
/// 0040DBB8..0040DBE6: the explosion's burst.
const WRECK_BURST: ParticleBurst = ParticleBurst {
    debris_count: 0x10,
    position: [0; 3],
    placement_radius: 0x14,
    central_type: 1,
    debris_types: [1, 1],
    speed: 8,
    flags: 0,
};

/// The world's services for the death and the fuse's end.
pub trait Selector39Host {
    type Error;
    /// The receiver's lifecycle change (`0047D670`) inside its `+98`.
    fn link_change(
        &mut self,
        arena: &mut ObjectArena,
        delivery: Selector39LinkDelivery,
    ) -> Result<(), Self::Error>;
    /// `00487EC0(0x32, position, 0x10000, 0x18000, 0)`, then `004923C0`.
    fn fuse_sound(&mut self, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// The death's writes to the mission G+1B0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector39MissionWrite {
    /// A fused target: +24 = 0 and +17 = 0 (G+1D4, G+1C7).
    StopCountdown,
    /// A timed target knocked out (`0043B987`): its seconds join the
    /// countdown +24, four times over after message 0xC.
    AddCountdown { seconds: u8, quadruple: bool },
    /// A target with +162: +13 = 1 (G+1C3).
    SetByte13,
}

/// The mission's countdown as a timed target's update reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector39Countdown {
    /// G+1C8 (mission +18).
    pub held: u8,
    /// G+1D4 (mission +24), in microseconds.
    pub remaining: u32,
}

/// The death's further services.
pub trait Selector39DeathHost: Selector39Host {
    fn mission_write(&mut self, write: Selector39MissionWrite) -> Result<(), Self::Error>;
    /// `0048EAD0(index, 0)`: the mission's record of the placement's death.
    fn mark_death(&mut self, arena: &mut ObjectArena, index: i8) -> Result<(), Self::Error>;
    /// `0046C760(raw_id, silent)` on the player.
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u8,
    ) -> Result<(), Self::Error>;
    /// `004750C0(id)` on the player's grants `+9C`.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
    ) -> Result<(), Self::Error>;
    /// `0047E190(argument)`: the ground cells under the target scorched.
    fn scorch(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        argument: i32,
    ) -> Result<(), Self::Error>;
}

/// The update's further services.
pub trait Selector39UpdateHost: Selector39Host {
    /// `0047C990`, the shared status update.
    fn status(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        delta: u32,
    ) -> Result<(), Self::Error>;
    /// `0047C7C0` on the player's actor: the radiation hit.
    fn radiation(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error>;
    /// `0047DD80`'s delivery within range: the actor's `+98` with code
    /// 0x17 and argument 0, from the target.
    fn radiation_notice(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        actor: ObjectId,
        distance_squared: u32,
    ) -> Result<(), Self::Error>;
    /// `0047CE90`: state 1 through `0047D670`, whose `+AC` is the death.
    fn retire(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), Self::Error>;
    /// `0047DFD0(delta)`, the burning target's smoke.
    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        delta: u32,
    ) -> Result<(), Self::Error>;
    /// `+4C(0, resource, 0)`, the shared child factory `0047AAF0`.
    fn child_visual(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        resource: u32,
    ) -> Result<(), Self::Error>;
    /// `0048A8B0`, a particle burst.
    fn burst(&mut self, arena: &mut ObjectArena, burst: ParticleBurst) -> Result<(), Self::Error>;
    /// `004887D0(00531250, +32, +84, 1)`: the blast from the target's
    /// position with its own handle as the source.
    fn blast(&mut self, arena: &mut ObjectArena, target: ObjectId) -> Result<(), Self::Error>;
    /// `0047A110` after a model change.
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
    ) -> Result<(), Self::Error>;
    /// `0048F060`, the process RNG.
    fn random(&mut self) -> u32;
    /// The sine table `00491530` turns the basis with.
    fn basis_math(&self) -> &NativeBasisMath;
    /// The mission's countdown, G+1C8 and G+1D4.
    fn countdown(&mut self) -> Result<Selector39Countdown, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector39Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Link(#[from] WorldLoadError),
    #[error("course target {target:?} service at {address:08X}: {error}")]
    Host {
        target: ObjectId,
        address: u32,
        error: E,
    },
    /// A branch no Level 1 target or recording reaches.
    #[error("course target {target:?} reaches the unported {address:08X}")]
    Boundary { target: ObjectId, address: u32 },
    #[error("course target {0:?}'s death needs a first player (00488AA0)")]
    MissingPlayer(ObjectId),
    #[error("course target {0:?} has no position +32")]
    MissingPosition(ObjectId),
}

impl ObjectArena {
    /// 0043BD90, the target's +B4. A target with +161 or +166 set takes
    /// nothing, and so does a collision (channel 1) whose source resolves to
    /// an object outside resources 0x29..0x2C. Everything else reaches the
    /// family's 0040E030.
    pub fn react_selector39_damage<H: ActorDamageReactionHost>(
        &mut self,
        target: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), ActorDamageReactionError<H::Error>> {
        let state = self.selector39(target)?;
        if state.byte_161 != 0 || state.byte_166 != 0 {
            return Ok(());
        }
        if callback.packet.channel == 1 {
            let source = self
                .resolve_actor_handle(callback.source)
                .map_err(|_| ObjectError::MissingActorRegistry)?;
            if let Some(source) = source {
                if !(0x29..=0x2c).contains(&self.object(source)?.base.resource_word) {
                    return Ok(());
                }
            }
        }
        self.react_family40d800_damage(target, callback, rng, host)
    }

    /// 0043B910, the +AC that 0047D670 calls at state 1.
    pub fn retire_selector39<H: Selector39DeathHost>(
        &mut self,
        target: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        self.send_selector39_links(target, 8, host)?;
        let state = self.selector39(target)?;
        let (seconds, fused) = (state.byte_160, state.byte_163 != 0);
        let links = (state.intermediate.byte_14d, state.intermediate.byte_14c);
        if seconds != 0 && !fused && (links.0 == 0 || links.1 != 0) {
            // 0043B987..0043BA01: a timed target knocked out adds to the
            // world's countdown instead of dying for good.
            let quadruple = self.selector39(target)?.byte_164 != 0;
            host.mission_write(Selector39MissionWrite::AddCountdown { seconds, quadruple })
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_b9fb,
                    error,
                })?;
        } else {
            self.mark_selector39_death(target, seconds, host)?;
        }
        self.finish_selector39_death(target, host)
    }

    /// `0043BA06..0043BA84`: the placement's death, then a countdown
    /// target's fuse or a generator's scorch and burn.
    fn mark_selector39_death<H: Selector39DeathHost>(
        &mut self,
        target: ObjectId,
        seconds: u8,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let index = self.actor(target)?.byte_78 as i8;
        host.mark_death(self, index)
            .map_err(|error| Selector39Error::Host {
                target,
                address: 0x0043_ba12,
                error,
            })?;
        if seconds == 0 {
            // 0043BA7B: the ground scorched and the burn before the wreck.
            host.scorch(self, target, DEATH_SCORCH)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_ba7f,
                    error,
                })?;
            self.selector39_mut(target)?.intermediate.word_150 = WRECK_FUSE;
        } else {
            host.mission_write(Selector39MissionWrite::StopCountdown)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_ba25,
                    error,
                })?;
            self.selector39_mut(target)?.intermediate.word_150 = fuse_micros(seconds);
            let position = self.selector39_position(target)?;
            host.fuse_sound(position)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_ba6d,
                    error,
                })?;
        }
        Ok(())
    }

    /// `0043BA8E..0043BB6F`: the death's shared tail.
    fn finish_selector39_death<H: Selector39DeathHost>(
        &mut self,
        target: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        if self.selector39(target)?.byte_162 != 0 {
            host.mission_write(Selector39MissionWrite::SetByte13)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_ba9b,
                    error,
                })?;
        }
        let first = self
            .first_player()
            .ok_or(Selector39Error::MissingPlayer(target))?;
        let command = |arena: &mut ObjectArena, host: &mut H, player, raw_id, silent| {
            host.player_command(arena, player, raw_id, silent)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0046_c760,
                    error,
                })
        };
        let order = self.selector39(target)?.word_15c;
        if order != 0 {
            command(self, host, first, order, 0)?;
        }
        for raw_id in [0x39, 0x3a, 0x3b] {
            command(self, host, first, raw_id, 1)?;
        }
        // 0043BADD: another live actor of the same resource and grant ID
        // leaves the grant for later.
        let actor = self.actor(target)?;
        let (handle, grant) = (actor.handle, actor.word_7a);
        let resource = self.object(target)?.base.resource_word;
        for &other in self.actor_list() {
            let state = self.actor(other)?;
            if state.handle != handle
                && self.object(other)?.base.resource_word == resource
                && state.word_c4 == 3
                && state.word_7a == grant
            {
                return Ok(());
            }
        }
        let players: Vec<_> = self
            .world_players()
            .iter()
            .map(|player| player.id)
            .collect();
        let mut last = first;
        for player in players {
            host.grant(self, player, grant)
                .map_err(|error| Selector39Error::Host {
                    target,
                    address: 0x0043_bb47,
                    error,
                })?;
            last = player;
        }
        if self.selector39(target)?.word_15c == 0xc {
            command(self, host, last, 0x23, 1)?;
        }
        Ok(())
    }

    /// 0043BB80, the target's +24.
    pub fn tick_selector39<H: Selector39UpdateHost>(
        &mut self,
        target: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let player = self.first_player();
        let state = &self.selector39(target)?.intermediate;
        if state.word_13c != 0 && state.word_140 < state.word_13c {
            return Ok(());
        }
        if player.is_some() && self.selector39(target)?.byte_165 == 0 {
            self.selector39_radiation(target, delta, host)?;
        }
        let state = self.selector39(target)?;
        if state.byte_160 == 0 {
            // 0043BD7E: the family's own update.
            return self.tick_family40d800_wreck(target, delta, host);
        }
        if state.byte_163 == 0 {
            // 0043BCCD: the world's countdown drives an unfused target.
            return self.tick_selector39_countdown(target, host);
        }
        let fuse = state.intermediate.word_150;
        if fuse > delta {
            let left = fuse - delta;
            self.selector39_mut(target)?.intermediate.word_150 = left;
            if left < FUSE_WARNING && fuse > FUSE_WARNING {
                let position = self.selector39_position(target)?;
                host.fuse_sound(position)
                    .map_err(|error| Selector39Error::Host {
                        target,
                        address: 0x0043_bc8c,
                        error,
                    })?;
            }
        } else if fuse != 0 {
            self.selector39_mut(target)?.intermediate.word_150 = 0;
            self.end_selector39_fuse(target, host)?;
        }
        host.status(self, target, delta)
            .map_err(|error| Selector39Error::Host {
                target,
                address: 0x0047_c990,
                error,
            })
    }

    /// `0043BCCD..0043BD7B`, a timed target without a fuse. One its links
    /// held up (+14D) dies once their count +14C falls to 0. A live one
    /// (state 3) waits. A knocked-out one comes back (`0043BFE0`) when the
    /// world's countdown has ended (G+1D4 is 0) or is held (G+1C8);
    /// meanwhile its health climbs back as the countdown runs: the maximum
    /// times the elapsed milliseconds of its +160 seconds (signed). No
    /// branch runs the shared status update.
    fn tick_selector39_countdown<H: Selector39UpdateHost>(
        &mut self,
        target: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let at = |address| {
            move |error| Selector39Error::Host {
                target,
                address,
                error,
            }
        };
        let state = self.selector39(target)?;
        let seconds = state.byte_160;
        if state.intermediate.byte_14d != 0 && state.intermediate.byte_14c == 0 {
            return host.retire(self, target).map_err(at(0x0043_bce3));
        }
        if self.actor(target)?.word_c4 == 3 {
            return Ok(());
        }
        let countdown = host.countdown().map_err(at(0x0043_bd03))?;
        if countdown.held != 0 || countdown.remaining == 0 {
            return self.end_selector39_fuse(target, host);
        }
        let total = i32::from(seconds).wrapping_mul(1000);
        let remaining = countdown.remaining as i32 / 1000;
        let maximum = self
            .actor(target)?
            .maximum_health
            .ok_or(Selector39Error::Boundary {
                target,
                address: 0x0043_bd62,
            })?;
        let health = total
            .wrapping_sub(remaining)
            .wrapping_mul(i32::from(maximum))
            / total;
        self.actor_mut(target)?.health = health as u16;
        Ok(())
    }

    /// `0040DB40`, the family update a target without a countdown runs. An
    /// unlit fuse (+150) lets a target that links held up (+14D) die once
    /// the last of them is gone (+14C). A lit one burns (`0047DFD0`) until
    /// it ends; then the target blows up: two child visuals, a burst, the
    /// blast of `00531250` (a local target), the wreck model (+144, 0x46
    /// when unset), +74 cleared, the basis turned by a random angle in XZ,
    /// the fuse and +C8 cleared, and the target leaves the update list
    /// (`00479F20(0)`). The shared status update (`0047C990`) follows each
    /// branch.
    fn tick_family40d800_wreck<H: Selector39UpdateHost>(
        &mut self,
        target: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let at = |address| {
            move |error| Selector39Error::Host {
                target,
                address,
                error,
            }
        };
        let state = &self.selector39(target)?.intermediate;
        let (fuse, held, links) = (state.word_150, state.byte_14d, state.byte_14c);
        if fuse == 0 {
            if held != 0 && links == 0 {
                host.retire(self, target).map_err(at(0x0040_dc9a))?;
            }
        } else if fuse > delta {
            self.selector39_mut(target)?.intermediate.word_150 = fuse - delta;
            host.status_visual(self, target, delta)
                .map_err(at(0x0040_db66))?;
        } else {
            for resource in WRECK_VISUALS {
                host.child_visual(self, target, resource)
                    .map_err(at(0x0040_db86))?;
            }
            let position = self.selector39_position(target)?;
            host.burst(
                self,
                ParticleBurst {
                    position,
                    ..WRECK_BURST
                },
            )
            .map_err(at(0x0040_dbee))?;
            if self.object(target)?.base.flags & 0x8000_0000 == 0 {
                host.blast(self, target).map_err(at(0x0040_dc11))?;
            }
            let state = &mut self.selector39_mut(target)?.intermediate;
            if state.word_144 == u16::MAX {
                state.word_144 = WRECK_MODEL;
            }
            let model = state.word_144;
            self.owned_mut(target)?.state.base.dirty_word |= 0x0100;
            self.actor_mut(target)?.rotating.visual.model_word = model;
            host.refresh_model(self, target).map_err(at(0x0040_dc3a))?;
            self.actor_mut(target)?.word_74 = 0;
            let angle = (host.random() & 0xffff) as i32;
            let mut basis = self.actor(target)?.rotating.basis;
            host.basis_math()
                .rotate_in_plane(&mut basis, angle, RotationPlane::Xz);
            self.actor_mut(target)?.rotating.basis = basis;
            self.selector39_mut(target)?.intermediate.word_150 = 0;
            self.actor_mut(target)?.word_c8 = 0;
            self.set_flag1(target, false)?;
        }
        host.status(self, target, delta).map_err(at(0x0047_c990))
    }

    /// 0043BB9D..0043BC38: within 1000 of the first player's primary actor
    /// (0046B030), the target hits it with radiation, `(1200 - distance) *
    /// 4` on channel 3 for 2 s. 0047DD80 then rereads both positions and
    /// sends the notice 0x17 within 1000 (unsigned squares).
    fn selector39_radiation<H: Selector39UpdateHost>(
        &mut self,
        target: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let Some(actor) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let own = self.selector39_position(target)?;
        let theirs = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let offset = std::array::from_fn(|axis| own[axis].wrapping_sub(theirs[axis]) as i16);
        let distance = radius_word_distance(offset) as i32;
        if distance >= 1000 {
            return Ok(());
        }
        let request = ActorTimedDamageRequest {
            // The packet is built in the delta's argument slot, so its
            // fourth byte is the delta's.
            packet: ActorDamagePacket {
                strength: (1200 - distance).wrapping_shl(2) as i16,
                channel: 3,
                byte_03: Some((delta >> 24) as u8),
            },
            source: self.actor(target)?.handle,
            microseconds: delta as i32,
            period_microseconds: 2_000_000,
        };
        host.radiation(self, actor, request)
            .map_err(|error| Selector39Error::Host {
                target,
                address: 0x0043_bc27,
                error,
            })?;
        let own = self.selector39_position(target)?;
        let theirs = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let squared = (0..3).fold(0_u32, |sum, axis| {
            let delta = i32::from(own[axis].wrapping_sub(theirs[axis]) as i16);
            sum.wrapping_add(delta.wrapping_mul(delta) as u32)
        });
        if squared > 1000 * 1000 {
            return Ok(());
        }
        host.radiation_notice(self, target, actor, squared)
            .map_err(|error| Selector39Error::Host {
                target,
                address: 0x0047_dde2,
                error,
            })
    }

    /// 0043BFE0: the target comes back (0047D670 to state 3), at full
    /// health, and sends message 7 to its links.
    fn end_selector39_fuse<H: Selector39Host>(
        &mut self,
        target: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let null = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        self.change_actor_state(
            target,
            ActorStateChangeRequest {
                target_state: 3,
                argument: 0,
                source: null,
            },
            &mut UpwardOnly,
        )
        .map_err(|error| match error {
            ActorStateChangeError::Object(error) => Selector39Error::Object(error),
            _ => Selector39Error::Boundary {
                target,
                address: 0x0047_d670,
            },
        })?;
        self.selector39_mut(target)?.intermediate.word_150 = 0;
        let actor = self.actor_mut(target)?;
        actor.health = actor
            .maximum_health
            .ok_or(Selector39Error::<H::Error>::Boundary {
                target,
                address: 0x0043_bffa,
            })?;
        self.send_selector39_links(target, 7, host)
    }

    /// 0047DD40(handle, 0, code, 0) for each retained link, in order; the
    /// count is reread per row.
    fn send_selector39_links<H: Selector39Host>(
        &mut self,
        target: ObjectId,
        code: i32,
        host: &mut H,
    ) -> Result<(), Selector39Error<H::Error>> {
        let Some(retained) = self.selector39(target)?.retained_158.clone() else {
            return Ok(());
        };
        let owner = retained.allocation();
        let image = owner.runtime_allocation();
        let base = retained.allocation_offset;
        let field = base.checked_add(4).ok_or(WorldLoadError::OffsetOverflow)?;
        let mut row = 0_u32;
        while (row as i32) < image.dword(base).map_err(WorldLoadError::from)? as i32 {
            let at = super::super::selector88::selector88_row(owner, field, row)?;
            let handle = crate::object_runtime::ActorHandle::from_raw(
                image.dword(at).map_err(WorldLoadError::from)?,
            );
            if let Some(delivery) = self.deliver_link_message(handle, code)? {
                if delivery.change != FamilyLinkChange::None {
                    host.link_change(self, delivery)
                        .map_err(|error| Selector39Error::Host {
                            target,
                            address: 0x0047_dd40,
                            error,
                        })?;
                }
            }
            row += 1;
        }
        Ok(())
    }

    fn selector39_position(&self, target: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(target)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(target))
    }
}

/// The seconds BYTE, read unsigned, times 1,000,000.
fn fuse_micros(seconds: u8) -> u32 {
    u32::from(seconds).wrapping_mul(1_000_000)
}

/// 0047D670 upward to state 3 calls no service.
struct UpwardOnly;

impl super::super::ActorStateChangeHost for UpwardOnly {
    type Error = ();
    type AttributionOwner = ();
    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: super::super::ActorStateChangeQuery,
    ) -> Result<bool, ()> {
        Err(())
    }
    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: crate::object_runtime::ActorHandle,
    ) -> Result<(), ()> {
        Err(())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: super::super::ActorStateChangeEffect<()>,
    ) -> Result<(), ()> {
        Err(())
    }
}

#[cfg(test)]
#[path = "course_tests.rs"]
mod tests;
