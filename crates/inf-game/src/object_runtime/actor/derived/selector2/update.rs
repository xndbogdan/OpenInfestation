//! Selector 2's controlled update 00416170 (virtual +44), which the player
//! block calls every tick with the delta and the player's control block:
//! timers, the mode request and transition, the speed magnitudes, the mode
//! dispatch (the [ground drive](super::drive) for modes 1 and 2), the
//! deep-water checks, the weapon timers and selection, the ground-proximity
//! splash and the engine sound.
//!
//! Branches Level 1 never reaches stop with [`VehicleDriveError::Unported`].

use inf_formats::native_basis::isqrt_48efc0;

use super::drive::{VehicleDriveError, VehicleDriveHost, VehicleDriveRequest};
use crate::object_runtime::actor::derived::update_prefix::{
    FamilyUpdateEffect, FamilyUpdateError, FamilyUpdateHost,
};
use crate::object_runtime::actor_terrain::plane_height_at;
use crate::object_runtime::{
    ActorAngularResponseRequest, ActorDamagePacket, ActorHandle, ActorTimedDamageRequest,
    ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext, PlayerActorLookupRequest,
    PlayerCommandContext, PlayerCommandOperation, PlayerCommandRequest, PlayerControlBlock,
    PlayerId,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_sound::ProcessSoundRef;

/// 0055F280: the CRT-initialized world up vector (0040269A..004026A3).
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];
/// 00530584: the upgrade bit each mode requires, by mode.
const MODE_UPGRADES: u32 = 0x0053_0584;
/// 00530590: the weapon model word by primary weapon index.
const WEAPON_MODELS: u32 = 0x0053_0590;
/// 005305B0: the upgrade bit each weapon slot requires.
const WEAPON_UPGRADES: u32 = 0x0053_05b0;
/// 005300A8: per-mode engine sound rows (sound, gain range, pitch range).
const ENGINE_ROWS: u32 = 0x0053_00a8;
/// The weapon panels' sound as they open and close.
const PANEL_SOUND: u32 = 0x16;
/// `00416725`: a cycled weapon holds its key for 20 clock ticks.
const CYCLE_TICKS: u32 = 0x14;
/// The upgrades L toggles: "Infrared vision" and "Headlights".
const INFRARED_VISION: u32 = 0x3d;
const HEADLIGHTS: u32 = 0x14;
/// `004165DC`: "Only buggy modes are useable after that last warp jump",
/// the Alien Brain's refusal.
pub const WARP_REFUSAL_TEXT: u16 = 0x16a;
/// `00416653`: "Your craft cannot fly on this world.", world flag 400h's.
pub const FLIGHT_REFUSAL_TEXT: u16 = 0x154;

/// Constructor-unwritten fields the update owns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VehicleUpdateState {
    /// BYTE +594, cleared at the start of every update.
    pub byte_594: Option<u8>,
    /// +598: the Capture the Banner banner the vehicle carries, which only
    /// the banner claims (`004212B0`, from its update `0045B5A0` and pickup
    /// `0045B9E0`) and 00422300 validates. The constructor never writes it:
    /// `None` is that allocator residue, which the first 00422300 always
    /// turns into the null handle, since nothing yet holds a fresh vehicle.
    pub handle_598: Option<ActorHandle>,
}

#[derive(Clone, Copy)]
pub struct VehicleUpdateRequest<'a> {
    pub drive: VehicleDriveRequest<'a>,
    /// 0055F14C: attract playback returns before any work.
    pub attract: bool,
    /// Mission +A0 (G+250), the current world.
    pub current_world: u32,
    /// Process 0055F154, read by a reached status visual.
    pub status_load_factor: Option<i32>,
    /// The owners 0046C760's command text needs.
    pub command: PlayerCommandContext<'a>,
    /// Settings `0054E8A8+7C`: the aim's scorer weighs the selected weapon's
    /// accuracy and its target refresh copies the focus to `+14`.
    pub settings_7c: u32,
    /// World-manager byte `G+5C5`, the special view: the fire direction
    /// `0041FF20` then frees the target and aims nowhere.
    pub special_view: bool,
    /// The take's owners, for another machine's actor captured again in
    /// the hold (`00412468`); none stops there.
    pub carry: Option<super::carry::VehicleCarryContext<'a>>,
}

/// The skimmer's wake particles (00422400).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeParticle {
    /// Over water: the splash 00489600.
    Splash,
    /// Over ground: the sprite 004894A0, thrown up with its shadow.
    Thrown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VehicleUpdateReport {
    /// L with Infrared vision owned (`00417922..00417932`): the caller sets
    /// the world manager's +5C4 to `!+5C3`, as nothing between reads either.
    pub toggles_infrared: bool,
    /// Port fix: L with both Infrared vision and the Headlights owned; the
    /// caller steps through the lamps (`step_lamps`).
    pub steps_lamps: bool,
    /// The prefix's status retired the vehicle: the wreck's branch
    /// `0041625C` ends this update, run by the caller through
    /// [`ObjectArena::finish_vehicle_wreck`] with the owners it needs.
    pub wreck: Option<VehicleWreckPending>,
}

/// An update waiting on its wreck's branch, with the event log's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleWreckPending {
    pub(super) logged: Option<([String; 4], u32)>,
    pub(super) clock: u32,
    pub(super) controls: PlayerControlBlock,
}

pub trait VehicleUpdateHost: VehicleDriveHost {
    /// One draw of 0048F060.
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// `00478E00`: the world camera (the actor M+74 names), built through
    /// the camera factory when none lives; None when the factory refuses
    /// it. A host without the factory finds only a live one.
    fn world_camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        Ok(arena
            .camera_handle()
            .and_then(|handle| arena.resolve_actor_handle(handle).ok().flatten()))
    }
    /// `00486600` with the vehicle's handle: a wake ribbon, or none when the
    /// factory refuses it (as a host without the ribbon factory does).
    fn create_ribbon(
        &mut self,
        _arena: &mut ObjectArena,
        _source: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Ok(None)
    }
    /// `004869B0` on the ribbon with a point, which only a created ribbon
    /// reaches.
    fn extend_ribbon(
        &mut self,
        _arena: &mut ObjectArena,
        _ribbon: ObjectId,
        _delta: u32,
        _point: [u16; 3],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    /// The RNG word itself, which the event log records at the update's
    /// entry; None for a host without the process word.
    fn rng_word(&self) -> Option<u32> {
        None
    }
    /// `00488E90` on the vehicle's `+32` position, `+8E` velocity and `+44`
    /// basis with `strength`: the hovercraft's spray, drawing on the
    /// process RNG, the particle load factor `0055F154` and the terrain's
    /// water plane.
    fn hull_spray(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        strength: i32,
    ) -> Result<(), Self::Error>;
    /// One particle of the skimmer's wake, type 0 and flags 0, from the
    /// process RNG: [`WakeParticle::Splash`] is `00489600`,
    /// [`WakeParticle::Thrown`] is `004894A0` with its shadow.
    fn wake_particle(
        &mut self,
        kind: WakeParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// `0047AAF0(NULL, 0x3B, NULL)` on the vehicle: the splash (selector
    /// 65) it leaves on the water when it rises out of it fast.
    fn water_splash(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
    ) -> Result<(), Self::Error>;
    /// `00489BC0(kind, point, 0)`: one particle of the Tractor Beam's line,
    /// type 1 while it pulls and 0 while slack.
    fn tow_beam(&mut self, particle_type: u32, position: [u16; 3]) -> Result<(), Self::Error>;
    /// The status visual/retirement effects of 0047C990.
    fn status_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error>;
    /// `0047C7C0` on the vehicle, whose +B4 is `004212F0`: the deep-water
    /// hit.
    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error>;
    /// `00412880`: the slot array's +0 drops the carried actor of slot
    /// `index` (or a later one) beside the vehicle.
    fn drop_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(), Self::Error>;
    /// `00421810`: the vehicle takes `item` into a free +1A4 slot.
    fn carry_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        item: ObjectId,
    ) -> Result<bool, Self::Error>;
    /// `004135D0(text, 0)` on the player's message.
    fn player_text(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        text: u16,
    ) -> Result<(), Self::Error>;
    /// `0048D340` on the vehicle's behavior: the predictive shot at a
    /// target.
    fn fire_weapon(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: crate::object_runtime::ActorWeaponFireRequest,
    ) -> Result<(), Self::Error>;
    /// `0048D0D0` on the vehicle's behavior: a shot queued along a
    /// direction (float bits) with a speed fraction.
    fn queue_weapon(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        microseconds: u32,
        direction: [u32; 3],
        fraction: u32,
    ) -> Result<(), Self::Error>;
    /// The aim's sight test `0043AD90` from `origin` toward `target`.
    fn sight(
        &mut self,
        arena: &ObjectArena,
        origin: [u16; 3],
        target: [u16; 3],
    ) -> Result<crate::world_sight::Sight, Self::Error>;
    /// `0048D820` on the vehicle's behavior: whether its selected weapon
    /// can damage `candidate`.
    fn damage_predicate(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        candidate: ObjectId,
    ) -> Result<bool, Self::Error>;
}

/// `00416D65`: the deep-water hit is timed over 3 s.
const DEEP_WATER_PERIOD: i32 = 3_000_000;
/// `00416498`: the lava's burn is timed over 6 s.
const BURN_PERIOD: i32 = 6_000_000;

pub(super) struct StatusHost<'h, H> {
    pub(super) host: &'h mut H,
    pub(super) null: ActorHandle,
}

impl<H: VehicleUpdateHost> FamilyUpdateHost for StatusHost<'_, H> {
    type Error = H::Error;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
    #[track_caller]
    fn random(&mut self) -> Result<u32, H::Error> {
        self.host.random()
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), H::Error> {
        self.host.status_effect(arena, actor, effect)
    }
}

pub(super) fn i16_of(word: u16) -> i32 {
    i32::from(word as i16)
}

/// The magnitude 00416A8E/00416AE7 take: isqrt, or 2·isqrt(sum/4) when the
/// wrapped sum has its sign bit set.
pub(super) fn magnitude(sum: i32) -> u32 {
    if sum as u32 & 0x8000_0000 != 0 {
        isqrt_48efc0((sum as u32 >> 2) as i32) << 1
    } else {
        isqrt_48efc0(sum)
    }
}

pub(super) fn squares(vector: [i32; 3]) -> i32 {
    vector[0]
        .wrapping_mul(vector[0])
        .wrapping_add(vector[1].wrapping_mul(vector[1]))
        .wrapping_add(vector[2].wrapping_mul(vector[2]))
}

impl ObjectArena {
    /// 00416170 for an offline vehicle. `controls` is the driving player's
    /// block (player +4), or the zeroed block of a vehicle the player left,
    /// which the update reads and clears in place. The event log records
    /// every call, as the lab's boundary does.
    pub fn tick_vehicle_update<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<VehicleUpdateReport, VehicleDriveError<H::Error>> {
        let logged = crate::event_log::enabled()
            .then(|| crate::event_log::vehicle_fields(self, vehicle, controls))
            .flatten()
            .zip(host.rng_word());
        let mut outcome = self.vehicle_update_body(vehicle, controls, request, host);
        if let Ok(VehicleUpdateReport {
            wreck: Some(pending),
            ..
        }) = &mut outcome
        {
            // The line waits for the wreck's branch.
            pending.logged = logged;
            return outcome;
        }
        if let Some((r, rng)) = logged {
            if let Some(rx) = crate::event_log::vehicle_fields(self, vehicle, controls) {
                let clock = request.drive.ground.clock;
                crate::event_log::vehicle_update(clock, rng, request.drive.delta, &r, &rx);
            }
        }
        outcome
    }

    fn vehicle_update_body<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<VehicleUpdateReport, VehicleDriveError<H::Error>> {
        let mut report = VehicleUpdateReport::default();
        let delta = request.drive.delta;
        let null = request.drive.process_null;
        let player = self.vehicle_player(vehicle, null)?;
        if request.attract {
            return Ok(report);
        }
        self.vehicle_update_prefix(
            vehicle,
            delta,
            null,
            request.status_load_factor,
            &mut StatusHost {
                host: &mut *host,
                null,
            },
        )?;
        if (self.actor(vehicle)?.word_c4 as i32) < 3 {
            // 0041625C: the player block calls +44 only from lifecycle 3,
            // so the prefix's status retired the vehicle.
            report.wreck = Some(VehicleWreckPending {
                logged: None,
                clock: request.drive.ground.clock,
                controls: *controls,
            });
            return Ok(report);
        }
        self.vehicle_weapon_panels(vehicle, delta, host)?;
        if (self.selector2_actor(vehicle)?.dwords_580_584[0] as i32) > delta as i32 {
            // 0041647C: the lava's burn runs down +580, each update 1000 on
            // channel 2 (2000 online, 0055F448) from the process NULL, timed
            // over 6 s (0047C7C0); the packet's byte +3 is never written.
            let network = request
                .command
                .process
                .ok_or(VehicleDriveError::Unported {
                    vehicle,
                    address: 0x0041_647c,
                })?
                .network_flag;
            let packet = ActorDamagePacket {
                strength: if network != 0 { 2000 } else { 1000 },
                channel: 2,
                byte_03: None,
            };
            host.timed_damage(
                self,
                vehicle,
                ActorTimedDamageRequest {
                    packet,
                    source: null,
                    microseconds: delta as i32,
                    period_microseconds: BURN_PERIOD,
                },
            )
            .map_err(VehicleDriveError::Host)?;
            let state = self.selector2_actor_mut(vehicle)?;
            state.dwords_580_584[0] = state.dwords_580_584[0].wrapping_sub(delta);
        } else {
            self.selector2_actor_mut(vehicle)?.dwords_580_584[0] = 0;
        }
        self.vehicle_mode_request(vehicle, controls, player, request, host)?;
        let clock = request.drive.ground.clock;
        self.vehicle_weapon_cycle(vehicle, controls, player, clock, host)?;
        self.vehicle_mode_transition(vehicle, controls, delta, null, host)?;
        self.vehicle_speed_magnitudes(vehicle)?;
        self.vehicle_mode_dispatch(vehicle, controls, &request.drive, host)?;
        if let Some(player) = player {
            self.vehicle_water_check(vehicle, player, request, host)?;
        }
        if self.derived_actor(vehicle)?.behavior_140.is_some() {
            self.vehicle_weapons(vehicle, controls, player, delta, request, host)?;
        }
        if controls.dword_30 != 0 {
            // 004176D9: 00411E70 turns the hold, forward for a positive
            // request (SETG), and the request is consumed.
            self.rotate_vehicle_slots::<VehicleDriveError<H::Error>>(
                vehicle,
                controls.dword_30 as i32 > 0,
                null,
            )?;
            controls.dword_30 = 0;
        }
        // 004176FF: 00412380 on the hold.
        self.tick_vehicle_hold(
            vehicle,
            delta,
            null,
            request.status_load_factor,
            request.carry,
            host,
        )?;
        if self.selector2_actor(vehicle)?.bytes_1b4_1bf[8] != 0 {
            self.vehicle_tow(
                vehicle,
                controls,
                player,
                null,
                delta,
                &mut |kind, point| host.tow_beam(kind, point),
            )?;
        }
        match player {
            Some(player) if self.selector2_actor(vehicle)?.dwords_200_204[1] == 0 => {
                // 00417731..004178ED: a drop (+38, unless a pickup +37 is
                // also set) and a pickup each arm the 250 ms +204 cooldown
                // and consume their byte. Each first tests the network flag
                // 0055F448: with it set and a banner held (+598), the drop
                // instead lets the banner go (+59C = 1 s, +598 cleared), and
                // the pickup only zeroes +59C.
                let null = request.drive.process_null;
                let network = request
                    .command
                    .process
                    .is_some_and(|process| process.network_flag != 0);
                let banner =
                    network && self.selector2_actor(vehicle)?.update.handle_598 != Some(null);
                let pickup = controls.byte(0x3b) != 0;
                if controls.byte(0x3c) != 0 && !pickup {
                    if banner {
                        let state = self.selector2_actor_mut(vehicle)?;
                        state.dwords_59c_5a0[0] = 1_000_000;
                        state.update.handle_598 = Some(null);
                    } else {
                        self.vehicle_drop(vehicle, null, host)?;
                    }
                    self.selector2_actor_mut(vehicle)?.dwords_200_204[1] = 250_000;
                    controls.set_byte(0x3c, 0);
                }
                let banner =
                    network && self.selector2_actor(vehicle)?.update.handle_598 != Some(null);
                if pickup && banner {
                    let state = self.selector2_actor_mut(vehicle)?;
                    state.dwords_59c_5a0[0] = 0;
                    state.dwords_200_204[1] = 250_000;
                    controls.set_byte(0x3b, 0);
                } else if pickup {
                    if let Some(item) = self.vehicle_pickup_scan(vehicle)? {
                        if !host
                            .carry_item(self, vehicle, item)
                            .map_err(VehicleDriveError::Host)?
                        {
                            host.player_text(self, player, super::items::CARGO_FULL_TEXT)
                                .map_err(VehicleDriveError::Host)?;
                        }
                    }
                    self.selector2_actor_mut(vehicle)?.dwords_200_204[1] = 250_000;
                    controls.set_byte(0x3b, 0);
                }
            }
            _ => {
                controls.set_byte(0x3b, 0);
                controls.set_byte(0x3c, 0);
            }
        }
        // 004178FB..00417953: L (the block's +39, player +3D) toggles
        // Infrared vision (upgrade 0x3D, the world's +5C4 against its +5C3)
        // when owned, else the Headlights (0x14, +208); both are read first.
        // Port fix (2026-10-23): with both owned, L steps through
        // them, where retail's toggled infrared alone over the headlights.
        if let Some(player) = player {
            if controls.byte(0x3d) != 0 {
                let infrared = self.player_upgrade_owned(player, INFRARED_VISION)?;
                let headlights = self.player_upgrade_owned(player, HEADLIGHTS)?;
                if infrared && headlights {
                    report.steps_lamps = true;
                } else if infrared {
                    report.toggles_infrared = true;
                } else if headlights {
                    let state = self.selector2_actor_mut(vehicle)?;
                    state.byte_208 = u8::from(state.byte_208 == 0);
                }
                controls.set_byte(0x3d, 0);
            }
        }
        self.vehicle_splash(vehicle, request, host)?;
        self.vehicle_engine_sound(vehicle, request, host)?;
        if let Some(network) = request.drive.network {
            self.vehicle_network_tail(vehicle, network, delta, null)?;
        }
        Ok(report)
    }

    /// `00417CF4..00417D67`, online: the vehicle heals from `+5B0`, which
    /// gathers the time, a point for each 2^14 µs in team play (mode 0),
    /// else each 2^12, to its most; then a live vehicle claims its race
    /// record (`0046F760`).
    fn vehicle_network_tail<E>(
        &mut self,
        vehicle: ObjectId,
        network: super::drive::VehicleNetwork,
        delta: u32,
        null: ActorHandle,
    ) -> Result<(), VehicleDriveError<E>> {
        let shift = if network.mode != 0 { 12 } else { 14 };
        let state = self.selector2_actor_mut(vehicle)?;
        let gathered = state.dwords_5ac_5b0[1].wrapping_add(delta);
        state.dwords_5ac_5b0[1] = gathered;
        let gain = (gathered as i32) >> shift;
        if gain != 0 {
            state.dwords_5ac_5b0[1] = gathered.wrapping_sub((gain << shift) as u32);
            let actor = self.actor_mut(vehicle)?;
            actor.health = actor.health.wrapping_add(gain as u16);
            let most = actor
                .maximum_health
                .ok_or(VehicleDriveError::UnknownField {
                    vehicle,
                    offset: 0xa2,
                    address: 0x0041_7d2f,
                })?;
            if actor.health > most {
                actor.health = most;
            }
        }
        if self.race().is_some() && self.actor(vehicle)?.word_c4 == 3 {
            self.claim_race_record(vehicle, null)
                .map_err(|error| VehicleDriveError::Status(format!("race record: {error}")))?;
        }
        Ok(())
    }

    /// 0041619A..0041624F, before the lifecycle test: +594 is cleared, the
    /// +200/+204 timers run down, 0047C990 ticks the status, the dirty word
    /// takes 0x62F0 and +D8 is cleared, 00422300 checks the held handle, and
    /// the +5A0 timer rearms at 2 s while the squared speed exceeds 0xF424.
    pub(super) fn vehicle_update_prefix<S: FamilyUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        null: ActorHandle,
        status_load_factor: Option<i32>,
        status: &mut S,
    ) -> Result<(), VehicleDriveError<S::Error>> {
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.update.byte_594 = Some(0);
            for timer in &mut state.dwords_200_204 {
                *timer = if (*timer as i32) > delta as i32 {
                    timer.wrapping_sub(delta)
                } else {
                    0
                };
            }
        }
        // 0047C990, the shared status timers.
        self.tick_actor_status(vehicle, delta, status_load_factor, status)
            .map_err(status_error)?;
        self.owned_mut(vehicle)?.state.base.dirty_word |= 0x62f0;
        self.actor_mut(vehicle)?.word_d8 = 0;
        self.vehicle_target_check(vehicle, delta, null)?;
        let velocity = self.actor(vehicle)?.words_8e_92.map(i16_of);
        let state = self.selector2_actor_mut(vehicle)?;
        let timer = &mut state.dwords_59c_5a0[1];
        *timer = if squares(velocity) as u32 > 0xf424 {
            2_000_000
        } else if *timer >= delta {
            timer.wrapping_sub(delta)
        } else {
            0
        };
        Ok(())
    }

    /// 0046B090 on the vehicle's +84.
    pub(super) fn vehicle_player<E>(
        &self,
        vehicle: ObjectId,
        null: ActorHandle,
    ) -> Result<Option<PlayerId>, VehicleDriveError<E>> {
        let handle = self.actor(vehicle)?.handle;
        let process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(null),
        };
        self.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: Some(&process),
            },
        )
        .map_err(|_| VehicleDriveError::Unported {
            vehicle,
            address: 0x0041_6180,
        })
    }

    /// 00422300: the +59C countdown and the +598 handle, cleared unless it
    /// still names an actor whose derived component's +144 is this vehicle.
    pub(super) fn vehicle_target_check<E>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        null: ActorHandle,
    ) -> Result<(), VehicleDriveError<E>> {
        let state = self.selector2_actor_mut(vehicle)?;
        let timer = state.dwords_59c_5a0[0] as i32;
        state.dwords_59c_5a0[0] = if (delta as i32) < timer {
            (timer - delta as i32) as u32
        } else {
            0
        };
        let held = match state.update.handle_598 {
            Some(handle) => handle,
            None => {
                // The residue survives 00422300 only if it names a live actor
                // whose +B8 component's +144 is this vehicle: a banner or hold
                // item the vehicle carries, which a vehicle has not yet
                // picked up at its first update. Every other value, the null
                // handle included, leaves the null handle (Infest01 too).
                state.update.handle_598 = Some(null);
                return Ok(());
            }
        };
        if held == null {
            return Ok(());
        }
        // 00422325..00422374: kept while it names a banner (+B8) whose
        // carrier +144 is this vehicle.
        let own = self.actor(vehicle)?.handle;
        let banner = self
            .resolve_actor_handle(held)
            .map_err(|_| ObjectError::MissingActorRegistry)?
            .and_then(|held| self.banner_of(held));
        let kept = match banner {
            Some(banner) => self.selector40(banner)?.carrier_144 == own,
            None => false,
        };
        if !kept {
            self.selector2_actor_mut(vehicle)?.update.handle_598 = Some(null);
        }
        Ok(())
    }

    /// 004164C2..004166C7: a mode request in control +30 (player +34).
    fn vehicle_mode_request<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        player: Option<PlayerId>,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let requested = controls.dword_34 as i32;
        if requested == 0 {
            return Ok(());
        }
        let executable = request.drive.executable;
        let owned = |arena: &ObjectArena,
                     player: PlayerId,
                     bit: u32|
         -> Result<bool, VehicleDriveError<H::Error>> {
            Ok(arena.player_upgrade_owned(player, bit)?)
        };
        let mode_bit = |mode: u32| -> Result<u32, VehicleDriveError<H::Error>> {
            read_word(executable, MODE_UPGRADES.wrapping_add(mode.wrapping_mul(2)))
        };
        if let Some(player) = player.filter(|_| {
            self.selector2_actor(vehicle)
                .is_ok_and(|state| state.byte_1c0 == 0)
        }) {
            let state = self.selector2_actor_mut(vehicle)?;
            if requested < 0 {
                state.dwords_1e8_1f4[2] = state.dwords_1e8_1f4[2].wrapping_add(1);
            } else if requested < 6 {
                state.dwords_1e8_1f4[2] = requested as u32;
            }
            let mut mode = state.dwords_1e8_1f4[2];
            if !owned(self, player, mode_bit(mode)?)? {
                while (mode as i32) < 6 {
                    mode = mode.wrapping_add(1);
                    self.selector2_actor_mut(vehicle)?.dwords_1e8_1f4[2] = mode;
                    if owned(self, player, mode_bit(mode)?)? {
                        break;
                    }
                }
            }
            if self.selector2_actor(vehicle)?.dwords_1e8_1f4[2] == 6 {
                self.selector2_actor_mut(vehicle)?.dwords_1e8_1f4[2] = 1;
            } else {
                // 0041657E/004165FB: two reads of upgrade 29h; world 13h
                // refuses modes 3..5, else world flag 400h modes 4 and 5,
                // back to mode 1 with the player's message (004135D0).
                let mode = self.selector2_actor(vehicle)?.dwords_1e8_1f4[2];
                let refusal = if !owned(self, player, 0x29)?
                    && request.current_world == 0x13
                    && matches!(mode, 3..=5)
                {
                    Some(WARP_REFUSAL_TEXT)
                } else if !owned(self, player, 0x29)?
                    && request.drive.ground.world_flags & 0x400 != 0
                    && matches!(mode, 4 | 5)
                {
                    Some(FLIGHT_REFUSAL_TEXT)
                } else {
                    None
                };
                if let Some(text) = refusal {
                    self.selector2_actor_mut(vehicle)?.dwords_1e8_1f4[2] = 1;
                    host.player_text(self, player, text)
                        .map_err(VehicleDriveError::Host)?;
                }
            }
        }
        let state = self.selector2_actor_mut(vehicle)?;
        state.byte_1c0 = 0;
        controls.dword_34 = 0;
        if state.dwords_1e8_1f4[2] != state.dwords_1e8_1f4[0] && state.dwords_1e8_1f4[0] != 0 {
            host.play_sound(self, vehicle, 0x17, 0x4000, 0x10000)
                .map_err(VehicleDriveError::Sound)?;
        }
        Ok(())
    }

    /// 0041685A..00416A5C: the transition toward +1F0.
    fn vehicle_mode_transition<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        delta: u32,
        null: ActorHandle,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let state = self.selector2_actor(vehicle)?;
        let [current, progress, requested, _] = state.dwords_1e8_1f4;
        if requested == current {
            if progress != 0xffff {
                let next = (progress as i32).wrapping_add((delta / 12) as i32);
                self.selector2_actor_mut(vehicle)?.dwords_1e8_1f4[1] =
                    if next > 0xffff { 0xffff } else { next as u32 };
            }
            return Ok(());
        }
        controls.set_byte(0x3f, 0);
        let step = (delta >> 2) as i32;
        if (progress as i32) > step && current != 0 {
            let progress = (progress as i32) - step;
            let state = self.selector2_actor_mut(vehicle)?;
            state.dwords_1e8_1f4[1] = progress as u32;
            if progress < 0xc000 && matches!((current, requested), (1, 2) | (2, 1)) {
                state.dwords_1e8_1f4[3] = current;
                state.dwords_1e8_1f4[0] = requested;
                self.restore_vehicle_hull_height(vehicle)?;
                self.reset_vehicle_ground_drive(vehicle)?;
            }
            return Ok(());
        }
        // 004168F9: the full switch, with the world camera (built anew when
        // the player's choice of team retired it).
        let camera = host.world_camera(self).map_err(VehicleDriveError::Host)?;
        self.restore_vehicle_hull_height(vehicle)?;
        if let Some(reference) = self.derived_actor_mut(vehicle)?.sound_168.take() {
            host.release_sound(reference)
                .map_err(VehicleDriveError::Sound)?;
        }
        if self.selector2_actor(vehicle)?.dwords_538_554[5] != 0 {
            // No writer but the constructor's zero stores +54C, so its
            // release (0041693F) never runs.
            return Err(VehicleDriveError::Unported {
                vehicle,
                address: 0x0041_693f,
            });
        }
        if let Some(reference) = self.selector2_actor_mut(vehicle)?.drive.sound_548.take() {
            host.release_sound(reference)
                .map_err(VehicleDriveError::Sound)?;
        }
        // 00416969..00416986: leaving VTOL or skimmer.
        match current {
            4 => self.switch_vehicle_vtol(vehicle, false, host)?,
            5 => self.switch_vehicle_skimmer(vehicle, false, camera, null)?,
            _ => {}
        }
        self.derived_actor_mut(vehicle)?.byte_160 = 0;
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.dwords_1e8_1f4[3] = current;
            state.dwords_1e8_1f4[0] = requested;
        }
        controls.set_byte(0x3a, 0);
        {
            let base = &mut self.owned_mut(vehicle)?.state.base;
            let flags = base.flags | 0x100;
            crate::object_runtime::mark_flag_change(base, flags);
        }
        // 004169C6: a refused camera skips its mode.
        if let Some(camera) = camera {
            self.set_camera_parameter_mode(camera, 0)?;
        }
        // 004169DF's table: each mode's entry.
        match requested {
            1 | 2 => self.reset_vehicle_ground_drive(vehicle)?,
            3 => self.reset_vehicle_hover(vehicle)?,
            4 => self.switch_vehicle_vtol(vehicle, true, host)?,
            5 => self.switch_vehicle_skimmer(vehicle, true, camera, null)?,
            _ => {}
        }
        let previous = self.selector2_actor(vehicle)?.dwords_1e8_1f4[3];
        self.selector2_actor_mut(vehicle)?.dwords_1e8_1f4[1] =
            if previous != 0 { 0 } else { 0xffff };
        Ok(())
    }

    /// 0048D030 on the vehicle's behavior. When its firing timer runs out
    /// with a weapon's looping sound, 0048D000 deletes the sound (00487FE0)
    /// and clears +28 and +2C.
    pub(super) fn tick_vehicle_behavior<E>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        release: impl FnOnce(ProcessSoundRef) -> Result<(), E>,
    ) -> Result<(), VehicleDriveError<E>> {
        match self.tick_actor_behavior(vehicle, delta) {
            Err(crate::object_runtime::ActorBehaviorTickError::RequiredSoundStop {
                sound, ..
            }) => {
                release(sound).map_err(VehicleDriveError::Sound)?;
                let behavior = self.vehicle_behavior_mut(vehicle)?;
                behavior.sound_28 = None;
                behavior.word_2c = 0;
                Ok(())
            }
            result => result.map_err(|error| VehicleDriveError::Status(error.to_string())),
        }
    }

    /// +8C = +184 before a mode change.
    fn restore_vehicle_hull_height(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<(), DerivedActorErrorAlias> {
        let height = self
            .selector2_actor(vehicle)?
            .word_184
            .ok_or(super::super::DerivedActorError::NotSelector2(vehicle))?;
        self.actor_mut(vehicle)
            .map_err(|_| super::super::DerivedActorError::NotSelector2(vehicle))?
            .word_8c = height as u16;
        Ok(())
    }

    /// 00416A62..00416B1D: +574 speed, +578 change of velocity, +56C..+570.
    fn vehicle_speed_magnitudes(&mut self, vehicle: ObjectId) -> Result<(), ObjectError> {
        let velocity = self.actor(vehicle)?.words_8e_92;
        let speed = magnitude(squares(velocity.map(i16_of)));
        let last = self
            .selector2_actor(vehicle)
            .map_err(|_| ObjectError::ActorPositionUnknown(vehicle))?
            .words_56c_570;
        let change: [i32; 3] =
            std::array::from_fn(|axis| i16_of(velocity[axis].wrapping_sub(last[axis])));
        let change = magnitude(squares(change));
        let state = self
            .selector2_actor_mut(vehicle)
            .map_err(|_| ObjectError::ActorPositionUnknown(vehicle))?;
        state.drive.dword_574 = Some(speed);
        state.drive.dword_578 = Some(change);
        state.words_56c_570 = velocity;
        Ok(())
    }

    /// 00416B26..00416B9E: the settled mode's physics, else the transition
    /// pose (00426730 toward the world up) and virtual +D4 (00415FF0).
    fn vehicle_mode_dispatch<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let [mode, progress, _, _] = self.selector2_actor(vehicle)?.dwords_1e8_1f4;
        if progress == 0xffff {
            match mode {
                1 | 2 => return self.tick_vehicle_ground_drive(vehicle, controls, request, host),
                3 => return self.tick_vehicle_hover(vehicle, controls, request, host),
                4 => return self.tick_vehicle_vtol(vehicle, controls, request, host),
                5 => return self.tick_vehicle_skimmer(vehicle, controls, request, host),
                _ => {}
            }
        } else {
            let basis = self.actor(vehicle)?.rotating.basis;
            let gain = 0xffff_u32.wrapping_mul(request.delta >> 5) >> 14;
            self.apply_actor_angular_response(
                vehicle,
                ActorAngularResponseRequest {
                    current_axis: [4, 5, 6].map(|index| f32::from_bits(basis.words[index])),
                    desired_axis: WORLD_UP,
                    gain: gain as i32,
                },
            )?;
        }
        self.vehicle_settle(vehicle, request)
    }

    /// 00415FF0: the suspension and full physics while the hull has a
    /// table; otherwise the height is clamped above the ground.
    fn vehicle_settle<E>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<(), VehicleDriveError<E>> {
        let table = self.selector2_actor(vehicle)?.words_188_194[3];
        if table != 0 {
            self.vehicle_suspension(vehicle, table, request.delta >> 5, request)?;
            self.tick_actor_physics(
                vehicle,
                super::super::physics::ActorPhysicsRequest {
                    microseconds: request.delta,
                    mask: 0x1f,
                    sine_quarter: request.ground.sine_quarter,
                },
            )?;
            return Ok(());
        }
        let base = &mut self.owned_mut(vehicle)?.state.base;
        let mut position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let ground = i32::from(request.ground.terrain.height_at([position[0], position[2]]));
        let y = i16_of(position[1]);
        if y < ground + 0x80 {
            position[1] = (ground + 0x80) as u16;
        } else if ground + 0x3b18 < y {
            position[1] = (ground + 0x3b18) as u16;
        }
        base.position_words = Some(position);
        Ok(())
    }

    /// 00416BA4..00416D73: near the water plane, a wheeled hull over deep
    /// water owns command 6 (with its message); below the plane it takes
    /// damage.
    fn vehicle_water_check<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        player: PlayerId,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let terrain = request.drive.ground.terrain;
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16 as i32;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let y = i16_of(position[1]);
        if y >= plane + 0x200 {
            return Ok(());
        }
        let deep = match self.selector2_actor(vehicle)?.dwords_1e8_1f4[0] {
            1 => i32::from(terrain.raw_height_at([position[0], position[2]])) < plane,
            2 => i32::from(terrain.height_at([position[0], position[2]])) + 0x50 < plane,
            _ => false,
        };
        if deep {
            self.selector2_actor_mut(vehicle)?.bytes_20a_20c[0] = 1;
            self.apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id: 6,
                        silent: 0,
                    },
                },
                &request.command,
            )
            .map_err(|error| VehicleDriveError::Status(error.to_string()))?;
        }
        if y < plane as i16 as i32 {
            // 00416D08: world flag 200h burns (32000, channel 3) and arms
            // +580; otherwise 0047CAE0(-1000) eases the channel-2 status
            // before the (10000, channel 5) packet, whose byte +3 is never
            // written. 0047C7C0 times it over 3 s from the process NULL.
            let packet = if request.drive.ground.world_flags & 0x200 != 0 {
                self.selector2_actor_mut(vehicle)?.dwords_580_584[0] = 2_000_000;
                ActorDamagePacket {
                    strength: 32000,
                    channel: 3,
                    byte_03: None,
                }
            } else {
                self.selector2_actor_mut(vehicle)?.dwords_580_584[0] = 0;
                self.reduce_channel_two_status(vehicle, 1000)?;
                ActorDamagePacket {
                    strength: 10000,
                    channel: 5,
                    byte_03: None,
                }
            };
            host.timed_damage(
                self,
                vehicle,
                ActorTimedDamageRequest {
                    packet,
                    source: request.drive.process_null,
                    microseconds: request.drive.delta as i32,
                    period_microseconds: DEEP_WATER_PERIOD,
                },
            )
            .map_err(VehicleDriveError::Host)?;
        }
        Ok(())
    }

    /// 00416D78..004176C9: the weapon timers, the switch animation and the
    /// selection requests in controls +24/+28 (player +28/+2C).
    #[allow(clippy::too_many_arguments)]
    fn vehicle_weapons<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        player: Option<PlayerId>,
        delta: u32,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let executable = request.drive.executable;
        self.tick_vehicle_behavior(vehicle, delta, |reference| host.release_sound(reference))?;
        self.vehicle_aim(vehicle, request, host)?;
        let primary = self.vehicle_behavior(vehicle)?.byte_10 as i8;
        let model = read_word(
            executable,
            WEAPON_MODELS.wrapping_add((i32::from(primary) * 2) as u32),
        )? as u16;
        let state = self.selector2_actor_mut(vehicle)?;
        // 00416D9F: JBE, an unsigned countdown that stops at zero.
        state.dwords_210_218[1] = state.dwords_210_218[1].saturating_sub(delta);
        let switching = (delta >> 4) as i32;
        if state.word_20e != model {
            let progress = state.dwords_210_218[0] as i32;
            if progress > switching {
                state.dwords_210_218[0] = (progress - switching) as u32;
            } else {
                state.word_20e = model;
                state.dwords_210_218[0] = 0;
            }
        } else if state.dwords_210_218[0] != 0xffff {
            let next = (state.dwords_210_218[0] as i32).wrapping_add(switching);
            state.dwords_210_218[0] = if next > 0xffff { 0xffff } else { next as u32 };
        } else {
            self.vehicle_fire(vehicle, controls, request, host)?;
        }
        let Some(player) = player else {
            return Ok(());
        };
        let primary_request = controls.dword_28 as i32;
        if primary_request != 0 {
            if primary_request <= 1 {
                let start = self.vehicle_behavior(vehicle)?.byte_10;
                let mut moved: i32 = 0;
                loop {
                    self.cycle_primary_weapon(vehicle, primary_request as i8)?;
                    moved = moved.wrapping_add(primary_request);
                    let index = self.vehicle_behavior(vehicle)?.byte_10 as i8;
                    let bit = read_word(
                        executable,
                        WEAPON_UPGRADES.wrapping_add((i32::from(index) * 2) as u32),
                    )?;
                    if self.player_upgrade_owned(player, bit)? {
                        break;
                    }
                }
                if self.vehicle_behavior(vehicle)?.byte_10 != start {
                    self.slide_weapon_panel(vehicle, moved, false, host)?;
                }
            } else {
                let index = primary_request - 2;
                let bit = read_word(executable, WEAPON_UPGRADES.wrapping_add((index * 2) as u32))?;
                if self.player_upgrade_owned(player, bit)? {
                    let behavior = self.vehicle_behavior_mut(vehicle)?;
                    let count = i32::from(behavior.byte_0e as i8);
                    if count == 0 {
                        return Err(VehicleDriveError::Division {
                            address: 0x0041_7525,
                        });
                    }
                    behavior.byte_10 = (index % count) as u8;
                    host.play_sound(self, vehicle, 0x16, 0x5555, 0x10000)
                        .map_err(VehicleDriveError::Sound)?;
                }
            }
            controls.dword_28 = 0;
        }
        let secondary_request = controls.dword_2c as i32;
        if secondary_request != 0 {
            self.step_secondary_weapon(vehicle, 0)?;
            if secondary_request <= 1 {
                let start = self.vehicle_behavior(vehicle)?.byte_11;
                let mut moved: i32 = 0;
                loop {
                    self.step_secondary_weapon(vehicle, secondary_request as i8)?;
                    moved = moved.wrapping_add(secondary_request);
                    let index = self.vehicle_behavior(vehicle)?.byte_11 as i8;
                    let bit = read_word(
                        executable,
                        WEAPON_UPGRADES.wrapping_add((i32::from(index) * 2) as u32),
                    )?;
                    if self.player_upgrade_owned(player, bit)?
                        || self.vehicle_behavior(vehicle)?.byte_11 == start
                    {
                        break;
                    }
                }
                let changed = self.vehicle_behavior(vehicle)?.byte_11 != start;
                if changed || self.selector2_actor(vehicle)?.dwords_52c_530[1] != 0 {
                    self.slide_weapon_panel(vehicle, moved, true, host)?;
                }
            } else {
                self.vehicle_behavior_mut(vehicle)?.byte_11 = (secondary_request - 2) as u8;
                host.play_sound(self, vehicle, 0x16, 0x5555, 0x10000)
                    .map_err(VehicleDriveError::Sound)?;
            }
            controls.dword_2c = 0;
        }
        Ok(())
    }

    fn vehicle_behavior(
        &self,
        vehicle: ObjectId,
    ) -> Result<&crate::object_runtime::actor::behavior::ActorBehaviorState, ObjectError> {
        self.derived_actor(vehicle)
            .map_err(|_| ObjectError::ActorPositionUnknown(vehicle))?
            .behavior_140
            .as_ref()
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))
    }

    fn vehicle_behavior_mut(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<&mut crate::object_runtime::actor::behavior::ActorBehaviorState, ObjectError> {
        self.derived_actor_mut(vehicle)
            .map_err(|_| ObjectError::ActorPositionUnknown(vehicle))?
            .behavior_140
            .as_mut()
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))
    }

    /// 0048D2D0: the primary index +10 moves by `step`, modulo the count +0E.
    fn cycle_primary_weapon<E>(
        &mut self,
        vehicle: ObjectId,
        step: i8,
    ) -> Result<(), VehicleDriveError<E>> {
        let behavior = self.vehicle_behavior_mut(vehicle)?;
        let next = step.wrapping_add(behavior.byte_10 as i8);
        let count = behavior.byte_0e as i8;
        if count == 0 {
            return Err(VehicleDriveError::Division {
                address: 0x0048_d2e4,
            });
        }
        let index = next.wrapping_rem(count);
        behavior.byte_10 = if index < 0 {
            count.wrapping_add(index)
        } else {
            index
        } as u8;
        Ok(())
    }

    /// 0048D300: the secondary index +11 moves by `step`, held inside the
    /// secondary range [+0E, +0E + +0F).
    fn step_secondary_weapon<E>(
        &mut self,
        vehicle: ObjectId,
        step: i8,
    ) -> Result<(), VehicleDriveError<E>> {
        let behavior = self.vehicle_behavior_mut(vehicle)?;
        let index = (behavior.byte_11 as i8).wrapping_add(step);
        let first = behavior.byte_0e as i8;
        behavior.byte_11 = index as u8;
        let end = i32::from(first) + i32::from(behavior.byte_0f as i8);
        if i32::from(index) < end {
            if first <= index {
                return Ok(());
            }
            behavior.byte_11 = first.wrapping_add(behavior.byte_0f as i8).wrapping_sub(1) as u8;
        } else {
            behavior.byte_11 = first as u8;
        }
        Ok(())
    }

    /// 004163A2..0041646E: each open weapon panel (+534, +535) counts its
    /// timer (+52C, +530) down in milliseconds and closes with sound 16h;
    /// after each, its slide (+540, +544) halves.
    fn vehicle_weapon_panels<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let milliseconds = delta / 1000;
        for (panel, slide) in [(0, 2), (1, 3)] {
            let state = self.selector2_actor_mut(vehicle)?;
            if state.bytes_534_535[panel] != 0 {
                if state.dwords_52c_530[panel] > milliseconds {
                    state.dwords_52c_530[panel] -= milliseconds;
                } else {
                    host.play_sound(self, vehicle, PANEL_SOUND, 0x5555, 0x10000)
                        .map_err(VehicleDriveError::Sound)?;
                    let state = self.selector2_actor_mut(vehicle)?;
                    state.dwords_52c_530[panel] = 0;
                    state.bytes_534_535[panel] = 0;
                }
            }
            let state = self.selector2_actor_mut(vehicle)?;
            let value = state.dwords_538_554[slide] as i32;
            if value != 0 {
                state.dwords_538_554[slide] = (value / 2) as u32;
            }
        }
        Ok(())
    }

    /// 004166CD..0041685A: the weapon cycling keys, player +40 for the
    /// primary weapon and +41 for the secondary, while no message is shown.
    /// With a key held and its timer (+538, +53C) behind the clock, a turn
    /// left or right asks for the next or previous weapon (+28 or +2C = 1 or
    /// -1, the timer 20 clock ticks ahead); without a turn the key slides
    /// its weapon panel open. Before the timer it slides the panel open and,
    /// without a turn, restarts the timer at the clock. Either way the turn
    /// is consumed. A key not held keeps its timer at the clock.
    fn vehicle_weapon_cycle<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        player: Option<PlayerId>,
        clock: u32,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        for (key, secondary) in [(0x40, false), (0x41, true)] {
            let timer = usize::from(secondary);
            let held = controls.byte(key) != 0;
            let shown = match player {
                Some(player) if held => self.world_player(player)?.message_byte_00() != 0,
                // 004166DB reads the player's message through a null player.
                None if held => {
                    return Err(VehicleDriveError::Unported {
                        vehicle,
                        address: 0x0041_66db,
                    })
                }
                _ => false,
            };
            if !held || shown {
                self.selector2_actor_mut(vehicle)?.dwords_538_554[timer] = clock;
                continue;
            }
            let due = self.selector2_actor(vehicle)?.dwords_538_554[timer];
            let turn = controls.turn_14;
            if (due as i32) < clock as i32 {
                if turn != 0 {
                    let select = if turn < 0 { 1 } else { u32::MAX };
                    if secondary {
                        controls.dword_2c = select;
                    } else {
                        controls.dword_28 = select;
                    }
                    self.selector2_actor_mut(vehicle)?.dwords_538_554[timer] =
                        clock.wrapping_add(CYCLE_TICKS);
                } else {
                    self.slide_weapon_panel(vehicle, 0, secondary, host)?;
                }
            } else {
                self.slide_weapon_panel(vehicle, 0, secondary, host)?;
                if turn == 0 {
                    self.selector2_actor_mut(vehicle)?.dwords_538_554[timer] = clock;
                }
            }
            controls.turn_14 = 0;
        }
        Ok(())
    }

    /// 004216B0 (primary) / 00421750 (secondary): the weapon panel slides
    /// by `moved` and, if idle, starts with sound 16h.
    fn slide_weapon_panel<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        moved: i32,
        secondary: bool,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let state = self.selector2_actor_mut(vehicle)?;
        let scaled = moved.wrapping_mul(0x10000);
        let (slide, own, other, flag) = if secondary {
            (3, 1, 0, 1)
        } else {
            (2, 0, 1, 0)
        };
        let shift = if secondary { 8 } else { 16 };
        state.dwords_538_554[slide] =
            (state.dwords_538_554[slide] as i32).wrapping_add(scaled / shift) as u32;
        if state.bytes_534_535[flag] == 0 {
            state.dwords_52c_530[own] = 1000;
            state.bytes_534_535[flag] = 1;
            state.dwords_52c_530[other] = 0xfa;
            host.play_sound(self, vehicle, PANEL_SOUND, 0x5555, 0x10000)
                .map_err(VehicleDriveError::Sound)?;
            return Ok(());
        }
        let timer = state.dwords_52c_530[own];
        if timer < 0xfa {
            state.dwords_52c_530[own] = 1000 - timer;
        } else if timer < 0x2ee {
            state.dwords_52c_530[own] = 0x2ee;
        }
        Ok(())
    }

    /// 00417957..00417BED: crossing the water surface (within 0053014C of
    /// the wave height) draws RNG for a rare splash sound; leaving that band
    /// faster than 00530154 leaves the splash object and sound 0x13; deep
    /// under the surface a random bubble sound plays.
    fn vehicle_splash<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let executable = request.drive.executable;
        let ground = request.drive.ground;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let height = ground.terrain.height_at([position[0], position[2]]);
        let plane = ground.terrain.plane_base() >> ground.terrain.plane_shift();
        let surface = i32::from(plane_height_at(
            [position[0], position[2]],
            plane,
            height,
            ground.clock,
            true,
            ground.sine_quarter,
        ));
        let band = read_dword(executable, 0x0053_014c)? as i32;
        let y = i16_of(position[1]);
        let near = (y - surface).wrapping_abs() < band;
        let was = self.selector2_actor(vehicle)?.byte_209 != 0;
        if was != near {
            let draw = host.random().map_err(VehicleDriveError::Sound)? & 0xffff & 0x7f;
            if draw < request.drive.delta / 1000 / 1000 {
                let choice = host.random().map_err(VehicleDriveError::Sound)?;
                let sound = if choice & 1 != 0 { 9 } else { 10 };
                let pitch =
                    ((host.random().map_err(VehicleDriveError::Sound)? & 0xffff) >> 2) + 0xc000;
                host.play_sound(self, vehicle, sound, 0x10000, pitch)
                    .map_err(VehicleDriveError::Sound)?;
            }
            if was {
                // 00417B44..00417B90: leaving the surface band faster than
                // 00530154 leaves a splash, then sound 0x13.
                let fall = i16_of(self.actor(vehicle)?.words_8e_92[1]).wrapping_abs();
                if fall > read_dword(executable, 0x0053_0154)? as i32 {
                    host.water_splash(self, vehicle)
                        .map_err(VehicleDriveError::Sound)?;
                    let pitch =
                        ((host.random().map_err(VehicleDriveError::Sound)? & 0xffff) >> 2) + 0xc000;
                    host.play_sound(self, vehicle, 0x13, 0x10000, pitch)
                        .map_err(VehicleDriveError::Sound)?;
                }
            }
        }
        if y < surface - band {
            let draw = host.random().map_err(VehicleDriveError::Sound)? & 0xff;
            if draw as i32 > read_dword(executable, 0x0053_0150)? as i32 {
                let pitch =
                    ((host.random().map_err(VehicleDriveError::Sound)? & 0xffff) >> 2) + 0xc000;
                host.play_sound(self, vehicle, 0x44, 0x10000, pitch)
                    .map_err(VehicleDriveError::Sound)?;
            }
        }
        self.selector2_actor_mut(vehicle)?.byte_209 = u8::from(near);
        Ok(())
    }

    /// 00417BE7..00417CE6: the engine loop +168 follows the drive's load
    /// halves +55C/+560 through the mode's 005300A8 row.
    fn vehicle_engine_sound<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let executable = request.drive.executable;
        let mode = self.selector2_actor(vehicle)?.dwords_1e8_1f4[0];
        let row = ENGINE_ROWS.wrapping_add(mode.wrapping_mul(20));
        let floor = read_dword(executable, 0x0053_0138)? as i32;
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };
        let state = self.selector2_actor(vehicle)?;
        let load = [
            state.drive.dword_55c.ok_or(unknown(0x55c, 0x0041_7bfd))? as i32,
            state.drive.dword_560.ok_or(unknown(0x560, 0x0041_7c40))? as i32,
        ];
        let current = [state.dwords_538_554[7] as i32, state.dword_558 as i32];
        let mut next = [0i32; 2];
        for channel in 0..2 {
            let low = read_dword(executable, row + 4 + channel as u32 * 8)? as i32;
            let high = read_dword(executable, row + 8 + channel as u32 * 8)? as i32;
            let share = 100 - floor.wrapping_mul(100) / load[channel].max(0).wrapping_add(floor);
            let target = (share.wrapping_mul(high.wrapping_sub(low)) / 100).wrapping_add(low);
            let target = target.wrapping_shl(16) >> 8;
            next[channel] =
                (target.wrapping_sub(current[channel]) >> 2).wrapping_add(current[channel]);
        }
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.dwords_538_554[7] = next[0] as u32;
            state.dword_558 = next[1] as u32;
        }
        match self.derived_actor(vehicle)?.sound_168 {
            None => {
                let sound = read_dword(executable, row)?;
                let reference = host
                    .create_sound(self, vehicle, sound, next[0] as u32, next[1] as u32)
                    .map_err(VehicleDriveError::Sound)?;
                self.derived_actor_mut(vehicle)?.sound_168 = Some(reference);
            }
            Some(reference) => host
                .update_sound(self, vehicle, reference, next[0] as u32, next[1] as u32)
                .map_err(VehicleDriveError::Sound)?,
        }
        Ok(())
    }
}

type DerivedActorErrorAlias = super::super::DerivedActorError;

/// 0047C990's failures in the vehicle update's terms.
pub(super) fn status_error<E>(error: FamilyUpdateError<E>) -> VehicleDriveError<E> {
    match error {
        FamilyUpdateError::Host { error, .. }
        | FamilyUpdateError::StatusVisualRandom { error, .. } => VehicleDriveError::Host(error),
        FamilyUpdateError::Object(error) => VehicleDriveError::Object(error),
        FamilyUpdateError::Uninitialized { actor, offset } => VehicleDriveError::UnknownField {
            vehicle: actor,
            offset,
            address: 0x0047_c990,
        },
        FamilyUpdateError::StatusDivisionByZero(_) => VehicleDriveError::Division {
            address: 0x0047_ca68,
        },
        FamilyUpdateError::StatusVisualDivision { address, .. } => {
            VehicleDriveError::Division { address }
        }
        FamilyUpdateError::MissingStatusLoadFactor(actor) => {
            VehicleDriveError::Status(format!("{actor:?} status visual requires process 0055F154"))
        }
        FamilyUpdateError::MissingProcessNullHandle(actor) => {
            VehicleDriveError::Status(format!("{actor:?} status requires process 0054E894"))
        }
        FamilyUpdateError::UnsupportedClass(actor)
        | FamilyUpdateError::MissingWaterPlane(actor) => {
            VehicleDriveError::Status(format!("{actor:?} status prefix boundary"))
        }
    }
}

fn read_dword<E>(
    executable: inf_formats::FrontendExecutable<'_>,
    address: u32,
) -> Result<u32, VehicleDriveError<E>> {
    executable
        .word(address)
        .map_err(|error| VehicleDriveError::Executable {
            address,
            reason: error.to_string(),
        })
}

/// A signed WORD of static data, sign-extended as the native MOV CX/PUSH ECX
/// pairs pass it.
fn read_word<E>(
    executable: inf_formats::FrontendExecutable<'_>,
    address: u32,
) -> Result<u32, VehicleDriveError<E>> {
    let aligned = address & !3;
    let dword = read_dword(executable, aligned)?;
    let word = if address & 2 != 0 {
        (dword >> 16) as u16
    } else {
        dword as u16
    };
    Ok(u32::from(word))
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
