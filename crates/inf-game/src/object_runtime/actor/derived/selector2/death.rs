//! The vehicle's death and its wreck.
//!
//! When the lifecycle `0047D670` lowers the vehicle to state 1, the
//! vehicle's virtual +AC `00420B90` reports the loss to the vehicle's
//! player, if it has one: command 0x37 with its notice (`0046C760`), the
//! offline count in the player's WORD +11C (`0046A860`), and the weapon
//! snapshot `0046CBB0`. It then gives the wreck 10 s in +268 and broadcasts
//! event 10 at zero radius (`0047DD20`), as the vehicle's cleanup does.
//!
//! Below lifecycle 3 the world calls the vehicle's +24, `00416090`: the
//! update `00416170` runs its common prefix and takes the wreck's branch
//! `0041625C..0041639F`, then the deep-water hit follows. The wreck falls
//! and spins under full physics, trails its status visual, now and then
//! bursts into debris with a radius effect and a random spin, and disposes
//! of itself once +268 runs out.

use thiserror::Error;

use super::drive::VehicleDriveError;
use super::update::{
    status_error, VehicleUpdateHost, VehicleUpdateReport, VehicleUpdateRequest, VehicleWreckPending,
};
use crate::object_runtime::actor::derived::update_prefix::{FamilyUpdateEffect, FamilyUpdateHost};
use crate::object_runtime::{
    ActorDamagePacket, ActorHandle, ActorPhysicsRequest, ActorTimedDamageRequest,
    ActorVehicleCleanupNotificationError, ActorVehicleCleanupNotificationRequest,
    DerivedActorError, ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext,
    PlayerActorLookupError, PlayerActorLookupRequest, PlayerAttachError, PlayerCommandContext,
    PlayerCommandError, PlayerCommandOperation, PlayerCommandRequest, PlayerControlBlock,
    TerrainDestructionEffect,
};
use crate::process_particles::births::ParticleBurst;

/// `00420BA8`: the command the death sets, with its notice.
const DEATH_COMMAND: u32 = 0x37;
/// `00420BC8`: the wreck lasts 10 s.
const WRECK_MICROSECONDS: u32 = 10_000_000;
/// `004162EC`: the wreck's radius effect block.
const WRECK_RADIUS_BLOCK: u32 = 0x0053_1250;
/// `0041614B`: the deep-water hit is timed over 3 s.
const DEEP_WATER_PERIOD: i32 = 3_000_000;

#[derive(Debug, Error)]
pub enum VehicleDeathError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Command(#[from] PlayerCommandError),
    #[error(transparent)]
    Cleanup(#[from] ActorVehicleCleanupNotificationError),
    #[error("vehicle {0:?}: 00420B90 requires the process owners 0046B090 reads")]
    MissingProcess(ObjectId),
}

/// The services the dying update reaches.
pub trait VehicleDyingHost {
    type Error;
    /// One draw of `0048F060`.
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// The RNG word itself, which the event log records at the inner
    /// update's entry; None for a host without the process word.
    fn rng_word(&self) -> Option<u32> {
        None
    }
    /// The status `0047C990`'s effects and the status visual `0047DFD0`'s
    /// trail.
    fn status_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error>;
    /// The wreck's debris burst `0048A8B0` and its radius effect `004887D0`.
    fn wreck_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: TerrainDestructionEffect,
    ) -> Result<(), Self::Error>;
    /// `0047C7C0` on the wreck, whose +B4 is `004212F0`: the deep-water hit.
    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error>;
    /// `00487FE0`, for `0048D000` when the wreck's firing timer runs out
    /// with a weapon's looping sound.
    fn release_sound(
        &mut self,
        reference: crate::process_sound::ProcessSoundRef,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy)]
pub struct VehicleDyingRequest<'a> {
    pub delta: u32,
    /// `0055F14C`: attract playback skips the inner update.
    pub attract: bool,
    /// Process `0054E894`.
    pub process_null: ActorHandle,
    /// Process `0055F154`, read by the status visuals.
    pub status_load_factor: Option<i32>,
    pub sine_quarter: &'a [u16; 4096],
    /// The water plane `0055EE3C` shifted by `005483BC`, as the signed WORD
    /// `004160ED` compares.
    pub water_plane: i16,
    /// H14 world flags (`G+1B0`); flag 0x200 burns rather than drowns.
    pub world_flags: u32,
}

struct DyingStatusHost<'h, H> {
    host: &'h mut H,
    null: ActorHandle,
}

impl<H: VehicleDyingHost> FamilyUpdateHost for DyingStatusHost<'_, H> {
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

impl ObjectArena {
    /// `00420B90`, the vehicle's virtual +AC: the lifecycle has already
    /// cleared its health.
    pub fn retire_vehicle(
        &mut self,
        vehicle: ObjectId,
        context: &PlayerCommandContext<'_>,
    ) -> Result<(), VehicleDeathError> {
        let process = context
            .process
            .ok_or(VehicleDeathError::MissingProcess(vehicle))?;
        let lookup = PlayerActorLookupContext {
            process: Some(process),
        };
        let handle = self.actor(vehicle)?.handle;
        if let Some(player) =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle }, lookup)?
        {
            self.apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id: DEATH_COMMAND,
                        silent: 0,
                    },
                },
                context,
            )?;
            // 0046A860: offline, the player counts the vehicle lost.
            if process.network_flag == 0 {
                let state = self.world_player_mut(player)?;
                state.word_11c = state.word_11c.wrapping_add(1);
            }
            self.save_vehicle_into_player(player, vehicle)?;
        }
        self.selector2_actor_mut(vehicle)?.dword_268 = Some(WRECK_MICROSECONDS);
        self.notify_actor_vehicle_cleanup(
            ActorVehicleCleanupNotificationRequest { source: vehicle },
            lookup,
        )?;
        Ok(())
    }

    /// `00416090`, the vehicle's virtual +24. It clears the grenade charge
    /// +218 and calls its +44 `00416170` on a 0x44-byte control block, zero
    /// but for byte +3A = 1 (the block stands for player +4, so that is the
    /// player's +3E, the brake gate) and the three bytes after +00, which
    /// keep what the stack held and which the update does not read; below
    /// lifecycle 3 that update reads none of the block. Under the water
    /// plane the wreck then takes the deep-water hit, and +594 is set. A
    /// live vehicle takes [`Self::tick_vehicle_unheld`].
    pub fn tick_vehicle_dying<H: VehicleDyingHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleDyingRequest<'_>,
        host: &mut H,
    ) -> Result<VehicleUpdateReport, VehicleDriveError<H::Error>> {
        let report = VehicleUpdateReport::default();
        let delta = request.delta;
        let null = request.process_null;
        self.selector2_actor_mut(vehicle)?.dwords_210_218[2] = 0;
        // The event log's line for the inner 00416170, on the zeroed block.
        let block = PlayerControlBlock::default();
        let logged = crate::event_log::enabled()
            .then(|| crate::event_log::vehicle_fields(self, vehicle, &block))
            .flatten()
            .zip(host.rng_word());
        // 00416180: the player, which the wreck's branch never reads.
        self.vehicle_player::<H::Error>(vehicle, null)?;
        if !request.attract {
            self.vehicle_update_prefix(
                vehicle,
                delta,
                null,
                request.status_load_factor,
                &mut DyingStatusHost {
                    host: &mut *host,
                    null,
                },
            )?;
            if (self.actor(vehicle)?.word_c4 as i32) >= 3 {
                // 004163A2: a live vehicle's controlled update, which
                // callers run through tick_vehicle_unheld instead.
                return Err(VehicleDriveError::Unported {
                    vehicle,
                    address: 0x0041_63a2,
                });
            }
            self.vehicle_wreck(vehicle, request, host)?;
        }
        if let Some((r, rng)) = logged {
            if let Some(rx) = crate::event_log::vehicle_fields(self, vehicle, &block) {
                let clock = crate::event_log::tick_clock();
                crate::event_log::vehicle_update(clock, rng, delta, &r, &rx);
            }
        }
        self.tick_vehicle_water_tail(vehicle, request, host)?;
        Ok(report)
    }

    /// `00416090` on a live vehicle: the one the player left for a unit it
    /// holds (the Cranial Implanter's implant or creature, a taken truck).
    /// The grenade charge +218 is cleared, so it throws nothing, and the
    /// controlled update `00416170` runs on the zeroed block with the brake
    /// gate set, so the vehicle stands on its own physics (a ground vehicle
    /// in neutral). [`Self::tick_vehicle_water_tail`] follows.
    pub fn tick_vehicle_unheld<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleUpdateRequest<'_>,
        host: &mut H,
    ) -> Result<VehicleUpdateReport, VehicleDriveError<H::Error>> {
        self.selector2_actor_mut(vehicle)?.dwords_210_218[2] = 0;
        let mut block = PlayerControlBlock::default();
        // 004160BE: the block's +3A, the player's +3E.
        block.set_byte(0x3e, 1);
        self.tick_vehicle_update(vehicle, &mut block, request, host)
    }

    /// The wreck's branch `0041625C..0041639F` that ends a controlled update
    /// whose status retired the vehicle, then that update's event log line.
    pub fn finish_vehicle_wreck<H: VehicleDyingHost>(
        &mut self,
        vehicle: ObjectId,
        pending: VehicleWreckPending,
        request: &VehicleDyingRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        self.vehicle_wreck(vehicle, request, host)?;
        if let Some((r, rng)) = pending.logged {
            if let Some(rx) = crate::event_log::vehicle_fields(self, vehicle, &pending.controls) {
                crate::event_log::vehicle_update(pending.clock, rng, request.delta, &r, &rx);
            }
        }
        Ok(())
    }

    /// `004160C6..00416165`, after either update: under the water plane the
    /// deep-water hit, then +594 = 1.
    pub fn tick_vehicle_water_tail<H: VehicleDyingHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleDyingRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let null = request.process_null;
        let y = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?[1] as i16;
        if y < request.water_plane {
            // 0041610B..00416140: as in the controlled update, world flag
            // 200h burns and arms +580; otherwise 0047CAE0(-1000) eases the
            // channel-2 status before the drowning packet. The packet
            // reuses the delta's stack slot, so its byte +3 is the delta's
            // high byte.
            let byte_03 = Some((delta >> 24) as u8);
            let packet = if request.world_flags & 0x200 != 0 {
                self.selector2_actor_mut(vehicle)?.dwords_580_584[0] = 2_000_000;
                ActorDamagePacket {
                    strength: 32000,
                    channel: 3,
                    byte_03,
                }
            } else {
                self.selector2_actor_mut(vehicle)?.dwords_580_584[0] = 0;
                self.reduce_channel_two_status(vehicle, 1000)?;
                ActorDamagePacket {
                    strength: 10000,
                    channel: 5,
                    byte_03,
                }
            };
            host.timed_damage(
                self,
                vehicle,
                ActorTimedDamageRequest {
                    packet,
                    source: null,
                    microseconds: delta as i32,
                    period_microseconds: DEEP_WATER_PERIOD,
                },
            )
            .map_err(VehicleDriveError::Host)?;
        }
        self.selector2_actor_mut(vehicle)?.update.byte_594 = Some(1);
        Ok(())
    }

    /// `0041625C..0041639F`: +1C0 is cleared, full physics `0047B1E0(delta,
    /// 0x1F)` and the status visual `0047DFD0` run, and one draw in sixteen
    /// bursts the wreck: sixteen type-1 debris about a type-1 centre within
    /// 20 units at speed 8, the radius effect of block `00531250` from the
    /// wreck's handle (unless flag bit 31 is set), and a local spin of three
    /// draws, each `(draw & 0x3FF) - 0x200`. Then +268 runs down; once it
    /// is below the delta the wreck is disposed. The weapons still tick.
    fn vehicle_wreck<H: VehicleDyingHost>(
        &mut self,
        vehicle: ObjectId,
        request: &VehicleDyingRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        self.selector2_actor_mut(vehicle)?.byte_1c0 = 0;
        self.tick_actor_physics(
            vehicle,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 0x1f,
                sine_quarter: request.sine_quarter,
            },
        )?;
        self.tick_status_visual(
            vehicle,
            delta,
            request.status_load_factor,
            &mut DyingStatusHost {
                host: &mut *host,
                null: request.process_null,
            },
        )
        .map_err(status_error)?;
        if host.random().map_err(VehicleDriveError::Host)? & 0xf == 0 {
            let position = self
                .object(vehicle)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
            host.wreck_effect(
                self,
                vehicle,
                TerrainDestructionEffect::Burst(ParticleBurst {
                    debris_count: 16,
                    position,
                    placement_radius: 20,
                    central_type: 1,
                    debris_types: [1, 1],
                    speed: 8,
                    flags: 0,
                }),
            )
            .map_err(VehicleDriveError::Host)?;
            if self.object(vehicle)?.base.flags & 0x8000_0000 == 0 {
                let source = self.actor(vehicle)?.handle;
                host.wreck_effect(
                    self,
                    vehicle,
                    TerrainDestructionEffect::Radius {
                        block_address: WRECK_RADIUS_BLOCK,
                        source,
                    },
                )
                .map_err(VehicleDriveError::Host)?;
            }
            let mut spin = [0; 3];
            for value in &mut spin {
                let draw = host.random().map_err(VehicleDriveError::Host)?;
                *value = (draw & 0x3ff) as i32 - 0x200;
            }
            self.add_local_angular(vehicle, spin)?;
        }
        let left =
            self.selector2_actor(vehicle)?
                .dword_268
                .ok_or(VehicleDriveError::UnknownField {
                    vehicle,
                    offset: 0x268,
                    address: 0x0041_6367,
                })?;
        if left < delta {
            self.dispose(vehicle)?;
        } else {
            self.selector2_actor_mut(vehicle)?.dword_268 = Some(left - delta);
        }
        if self.derived_actor(vehicle)?.behavior_140.is_some() {
            self.tick_vehicle_behavior(vehicle, delta, |reference| host.release_sound(reference))
                .map_err(|error| match error {
                    VehicleDriveError::Sound(error) => VehicleDriveError::Host(error),
                    other => other,
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "death_tests.rs"]
mod tests;
