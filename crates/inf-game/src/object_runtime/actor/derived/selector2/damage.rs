//! The vehicle's damage virtual +B4, `004212F0`.
//!
//! A collision (packet channel 1) first sounds 0x1D at the vehicle (+32),
//! louder with its speed change +578: `(min(+578, 1000) << 16) / 1000`.
//! The network arm (`0055F448`) is not ported. Offline, a collision then
//! hurts the truck (mode 2) a third as much, less 250 (no damage below 1),
//! and the VTOL and the skimmer (modes 4 and 5) by the packet's strength
//! once that exceeds 200. While the selector-70 actor at `0054ED70` lives,
//! damage leaves the vehicle at least one health. The vehicle's player
//! (`0046B090` of +84) scales the damage by `100 / (100 + Shielding)`, its
//! level byte +142, and owning upgrade 0x29 makes it immune. A nonzero hit
//! on a vehicle whose byte +D0 is set flashes at its nose, 512 along the
//! basis's forward row (`00487D70`, radius 0xF00, style 0x27 when immune,
//! else 0x28). The derived damage `004283E0` follows unless the vehicle is
//! immune to a nonzero hit.

use thiserror::Error;

use crate::object_runtime::{
    actor::{damage::ActorDamageCallback, damage_reaction::ActorDamageReactionHost},
    ActorDamageReactionError, DerivedActorError, ObjectArena, ObjectError, ObjectId,
    PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest, PlayerAttachError,
};
use crate::process_particles::factories::ParticleFactoryProcessState;

/// `00487EC0`'s sound for a collision, at gain from the speed change and
/// pitch 0x10000.
pub const VEHICLE_COLLISION_SOUND: u32 = 0x1d;
pub const VEHICLE_COLLISION_PITCH: u32 = 0x1_0000;
/// The damage channel of a collision.
const COLLISION_CHANNEL: u8 = 1;
/// The upgrade whose owner the vehicle's damage cannot reach.
const IMMUNITY_UPGRADE: u32 = 0x29;
/// The player's Shielding level, +142 (`+13C + 6`).
const SHIELDING_LEVEL: usize = 6;
/// `00487D70`'s radius and styles for the hit flash.
pub const VEHICLE_HIT_FLASH_RADIUS: u32 = 0xf00;
const HIT_FLASH_IMMUNE: u32 = 0x27;
const HIT_FLASH: u32 = 0x28;
/// `00520A64`: the flash sits this far along the forward row.
const NOSE: f32 = 512.0;

/// VehicleSpeedChangeApproximation: the speed change +578 a collision reads
/// before the vehicle's first update. Only the update publishes it
/// (`00416A62..00416B1D`); the constructor `004156E0` and the mode reset
/// `004184B0` leave it to allocator residue. A respawned vehicle meets the
/// pair pass of its own tick first: the player block that respawns it
/// (`0040F890` through `0046A890`) runs inside `00478F20`, ahead of `004791F0`
/// and `004790C0`, and a vehicle reborn at the world's start can sit in its
/// arrival gate, as on Oolbryte. A vehicle that has not moved has no speed
/// change, and a
/// fresh heap holds zero, so the collision sound plays silent. Before this,
/// the read stopped that gate's pair with the vehicle for the rest of the
/// visit.
pub const VEHICLE_SPEED_CHANGE_APPROXIMATION: u32 = 0;

fn log_speed_change_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("VehicleSpeedChangeApproximation: a collision reached a vehicle before its first update published the speed change +578; the port reads 0, a still vehicle's change (VEHICLE_DRIVE_RUNTIME.md)");
    }
}

/// The services `004212F0` adds to the derived damage's.
pub trait VehicleDamageHost: ActorDamageReactionHost {
    /// The process owners `0046B090` reads.
    fn player_process(&self) -> &ParticleFactoryProcessState;
    /// `00487EC0(0x1D, position, volume, 0x10000, 0)` and `004923C0`.
    fn collision_sound(&mut self, position: [u16; 3], volume: u32) -> Result<(), Self::Error>;
    /// Whether `0054ED70` holds the selector-70 actor (`0046E830`).
    fn selector70_alive(&mut self, arena: &ObjectArena) -> Result<bool, Self::Error>;
    /// `00487D70(position, 0xF00, style)`.
    fn hit_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        style: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum VehicleDamageError<E> {
    #[error(transparent)]
    Reaction(#[from] ActorDamageReactionError<E>),
    #[error("vehicle damage host: {0:?}")]
    Host(E),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Ball(Box<crate::object_runtime::BallError<std::convert::Infallible>>),
    #[error("vehicle {0:?}: its player's Shielding level makes 004215DF divide by zero")]
    ShieldingDivision(ObjectId),
}

/// A vehicle hit by something unmarked, or killed, before any marked hit
/// reads the clock +5A8 its constructor leaves to the allocator.
fn log_attacker_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("VehicleAttackerApproximation: a network vehicle hit by something unmarked, or killed, before its first marked hit reads the clock +5A8 its constructor leaves to the allocator; the port reads the fresh heap's zero (fresh_heap.rs)");
    }
}

impl<E> From<ObjectError> for VehicleDamageError<E> {
    fn from(error: ObjectError) -> Self {
        Self::Reaction(error.into())
    }
}

impl ObjectArena {
    /// `0047CE52`: a network death's credit reads the vehicle's +5A8, the
    /// clock of its last marked hit.
    pub fn vehicle_attacked_clock(&self, vehicle: ObjectId) -> Result<u32, DerivedActorError> {
        Ok(self
            .selector2_actor(vehicle)?
            .attacked_5a8
            .unwrap_or_else(|| {
                log_attacker_approximation();
                0
            }))
    }

    /// `0047CE62`: within 200 ticks of that hit, +5A6, the attacker handle's
    /// high WORD.
    pub fn vehicle_attacker_word(&self, vehicle: ObjectId) -> Result<u16, DerivedActorError> {
        Ok((self.selector2_actor(vehicle)?.attacker_5a4 >> 16) as u16)
    }

    /// `004212F0(damage, packet, source)` on the vehicle.
    pub fn react_vehicle_damage<H: VehicleDamageHost>(
        &mut self,
        vehicle: ObjectId,
        callback: ActorDamageCallback,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), VehicleDamageError<H::Error>> {
        let packet = callback.packet;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        if packet.channel == COLLISION_CHANNEL {
            let change = match self.selector2_actor(vehicle)?.drive.dword_578 {
                Some(change) => change,
                None => {
                    log_speed_change_approximation();
                    VEHICLE_SPEED_CHANGE_APPROXIMATION
                }
            } as i32;
            let volume = change.min(1000).wrapping_shl(16) / 1000;
            host.collision_sound(position, volume as u32)
                .map_err(VehicleDamageError::Host)?;
        }
        // 0042134D..004213C2: a live local vehicle in a network game keeps
        // its marked attacker and the clock of the hit (a later hit by
        // something unmarked within 200 ticks moves the clock on). Buggy
        // Madness's vehicles (mode 2) and Powerball's (mode 4) take no
        // damage, though a hard blow knocks Powerball's ball loose.
        if host.network_active().map_err(VehicleDamageError::Host)? {
            let clock = host.animation_tick().map_err(VehicleDamageError::Host)?;
            let local = self.object(vehicle)?.base.flags & 0x8000_0000 == 0;
            if local && self.actor(vehicle)?.word_c4 == 3 {
                let state = self.selector2_actor_mut(vehicle)?;
                if callback.source.marked() {
                    state.attacked_5a8 = Some(clock);
                    state.attacker_5a4 = callback.source.raw();
                } else if clock.wrapping_sub(state.attacked_5a8.unwrap_or_else(|| {
                    log_attacker_approximation();
                    0
                })) < 200
                {
                    state.attacked_5a8 = Some(clock);
                }
            }
            match host.player_process().settings[0x10 / 4] {
                2 => return Ok(()),
                4 => {
                    let null = host.player_process().process_null_handle;
                    return self
                        .powerball_hit(vehicle, packet, callback.source, null, rng)
                        .map_err(|error| VehicleDamageError::Ball(Box::new(error)));
                }
                _ => {}
            }
        }
        let mut damage = callback.adjusted_damage;
        if packet.channel == COLLISION_CHANNEL {
            match self.selector2_actor(vehicle)?.dwords_1e8_1f4[0] {
                2 => {
                    damage = (damage / 3).wrapping_sub(250);
                    if damage < 1 {
                        return Ok(());
                    }
                }
                4 | 5 if packet.strength > 200 => damage = i32::from(packet.strength),
                _ => {}
            }
        }
        if host
            .selector70_alive(self)
            .map_err(VehicleDamageError::Host)?
        {
            let health = i32::from(self.actor(vehicle)?.health);
            if damage >= health {
                damage = health - 1;
            }
        }
        let handle = self.actor(vehicle)?.handle;
        let player = self.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: Some(host.player_process()),
            },
        )?;
        let mut immune = false;
        if let Some(player) = player {
            let owner = self.world_player(player)?;
            immune = inf_formats::grant_is_owned(&owner.bits_100, IMMUNITY_UPGRADE)
                .expect("upgrade 0x29 lies inside the mask");
            let divisor = i32::from(owner.bytes_13c_143[SHIELDING_LEVEL] as i8) + 100;
            damage = damage
                .wrapping_mul(100)
                .checked_div(divisor)
                .ok_or(VehicleDamageError::ShieldingDivision(vehicle))?;
        }
        if damage != 0 {
            if self.actor(vehicle)?.byte_d0 != 0 {
                let basis = self.actor(vehicle)?.rotating.basis;
                let nose: [u16; 3] = std::array::from_fn(|axis| {
                    let offset = (f32::from_bits(basis.words[7 + axis]) * NOSE) as i32;
                    position[axis].wrapping_add(offset as u16)
                });
                let style = if immune { HIT_FLASH_IMMUNE } else { HIT_FLASH };
                host.hit_flash(self, nose, style, rng)
                    .map_err(VehicleDamageError::Host)?;
            }
            if immune {
                return Ok(());
            }
        }
        self.react_derived_actor_damage(
            vehicle,
            ActorDamageCallback {
                adjusted_damage: damage,
                ..callback
            },
            rng,
            host,
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "damage_tests.rs"]
mod tests;
