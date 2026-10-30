//! Player +04..+47, the control block, and the offline player update 0046A260.
//!
//! The constructor's 00469F8E zeroes the whole block. Each world tick,
//! 00478F20 runs 0046A0E0 on the first player before its update loop and
//! 0046A260 after it: the block is refreshed from the player's input
//! references and player+4 goes to the vehicle's +44 (+24 below lifecycle 3).
//! The vehicle reads the block at its own +0.

use std::collections::BTreeSet;

use thiserror::Error;

use super::{
    ActorHandle, CameraFactoryInputs, ObjectArena, PlayerAttachError, PlayerId, PlayerKeyError,
    PlayerKeyOmission, PlayerMessageError,
};
use crate::input::{InputRuntime, InputStateError};
use crate::object_runtime::{mark_flag_change, FollowTargetRequest, ObjectError, ObjectId};

/// Player +04..+47. Offsets in the field names are the player's; the
/// vehicle sees each one 4 bytes lower.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerControlBlock {
    /// +04: the analog axes below are live.
    pub analog_04: u8,
    pub bytes_05_07: [u8; 3],
    /// +08/+0C/+10: analog consumers, clamped to ±0x10000.
    pub axes_08: [i32; 3],
    /// +14: Left minus Right held time.
    pub turn_14: i32,
    /// +18: Dive minus Climb.
    pub pitch_18: i32,
    /// +1C: Thrust minus the +7C action.
    pub thrust_1c: i32,
    /// +20/+24: Fire and Grenade held time.
    pub fire_20: i32,
    pub grenade_24: i32,
    /// +28/+2C: the respawn stores +148 + 2 and +14C + 2.
    pub dword_28: u32,
    pub dword_2c: u32,
    pub dword_30: u32,
    /// +34: the vehicle mode request the respawn stores and the vehicle
    /// consumes.
    pub dword_34: u32,
    /// +38..+47. +3A is set while Thrust outweighs the +7C action; +3E is
    /// the message gate.
    pub bytes_38: [u8; 16],
}

impl PlayerControlBlock {
    pub fn byte(&self, offset: usize) -> u8 {
        self.bytes_38[offset - 0x38]
    }
    pub fn set_byte(&mut self, offset: usize, value: u8) {
        self.bytes_38[offset - 0x38] = value;
    }
}

/// A test's analog reference with the player's zero-fill descriptor.
#[cfg(test)]
pub(crate) fn player_analog_fixture(
    input: &mut InputRuntime,
    identity: u32,
) -> crate::input::AnalogConsumerId {
    input
        .acquire_analog_consumer(
            crate::input::AnalogActionDescriptor {
                action: crate::input::ActionDescriptor {
                    identity,
                    callback_address: None,
                    flags: 0,
                },
                period_micros: 0,
            },
            0x77,
        )
        .unwrap()
}

/// The player's input references: the three analog ones at +5C..+64 (the
/// joystick's X, Y and Z) and the eight digital ones at +68..+84.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerInputConsumers {
    pub analog_5c: crate::input::AnalogConsumerId,
    pub analog_60: crate::input::AnalogConsumerId,
    pub analog_64: crate::input::AnalogConsumerId,
    pub left_68: crate::input::ConsumerId,
    pub right_6c: crate::input::ConsumerId,
    pub dive_70: crate::input::ConsumerId,
    pub climb_74: crate::input::ConsumerId,
    pub thrust_78: crate::input::ConsumerId,
    pub reverse_7c: crate::input::ConsumerId,
    pub fire_80: crate::input::ConsumerId,
    pub grenade_84: crate::input::ConsumerId,
}

/// Settings and session words 0046A260 reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerUpdateInputs {
    /// The scheduler delta, a signed DWORD natively.
    pub delta_micros: i32,
    /// Session byte G+5C5 quarters the input cap.
    pub slow_input_5c5: bool,
    /// Settings +8, the Controls menu's Joystick: 0 Disabled, 2 Digital,
    /// else Analogue.
    pub input_mode_08: u32,
    /// Process 0055F448.
    pub network: u32,
}

/// What 0046A260's network branch reads and keeps beyond the player: the
/// keys' countdown `0054ED18`, the host's team flag `0054E81C`, settings
/// `+10` and the host flag `0055F3A0`; the race `0054ED70` is the arena's.
#[derive(Debug, PartialEq, Eq)]
pub struct PlayerNetworkState<'a> {
    pub key_countdown_ed18: &'a mut u32,
    pub team_flag_81c: &'a mut u8,
    pub mode: u32,
    pub host: bool,
}

/// What 0046A260 asks of the player's actor after the block is refreshed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerVehicleCall {
    /// No actor, or the actor's flag 2 is clear.
    None,
    /// Lifecycle below 3: the actor's +24 with the delta.
    Update24,
    /// Lifecycle 3 or above: the actor's +44 with the delta and player+4.
    Update44,
}

/// 0046A260's result: the actor call, the calls its +54 repeat's
/// 0046D240 skipped, and the placements (+2A) of the banners the host's
/// restart sent home, which 0046A632 builds again (0048E170 with the
/// process NULL).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerControlsUpdate {
    pub call: PlayerVehicleCall,
    pub omissions: BTreeSet<PlayerKeyOmission>,
    pub placements: Vec<i16>,
    /// `0046A644`, the host's Powerball restart, which the caller runs
    /// with the ball's services (`restart_powerball`) before the vehicle
    /// call.
    pub restart_powerball: bool,
}

#[derive(Debug, Error)]
pub enum PlayerUpdateError {
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
    #[error("0046A260's network branch (0055F448 = {0}) needs the network game's state")]
    Network(u32),
    #[error("the arena's restart (0046A587) needs the network game's state")]
    RestartState,
    #[error("0046A72C reaches {0:?}'s +64 view, which is ported only for the vehicle")]
    MessageBrakeView(ObjectId),
    #[error(transparent)]
    Key(#[from] PlayerKeyError),
    #[error("player {0:?} field +{1:X} is unwritten where 0046A260 reads it")]
    UnknownField(PlayerId, u16),
}

impl ObjectArena {
    /// 0046A0E0, offline: the actor the player updates, returned for 00479040
    /// and 0046A260. It is the +D8 actor while +D8 is not the process NULL
    /// (the camera after the vehicle's death), else the +D4 vehicle; it takes
    /// flag 0x10 (dirty bit 2 when that changes a low flag). A missing actor
    /// returns None.
    ///
    /// While +D8 is set, a mode request in +34 lapses once the vehicle is off
    /// its update list (flag 2 clear). With byte +58 set, a +D8 actor and a
    /// vehicle below lifecycle 3 (the wreck) re-attach the player to the
    /// camera `00478E00`; otherwise a mode request, or a +D8 handle that no
    /// longer resolves, returns the player to a vehicle at lifecycle 3 by
    /// attaching the process NULL. Either way the camera then follows the +D4
    /// vehicle (`004825D0`, `00410E70`) and +34 is cleared.
    pub fn player_pre_update(
        &mut self,
        player: PlayerId,
        process_null: ActorHandle,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<Option<ObjectId>, PlayerUpdateError> {
        let state = self.world_player(player)?;
        let (d4, d8) = (state.handle_d4, state.handle_d8);
        let vehicle = self.resolve_handle(d4)?;
        let mut unit = vehicle;
        if d8 != process_null {
            let remote = self.resolve_handle(d8)?;
            if self.world_player(player)?.controls.dword_34 != 0 {
                if let Some(vehicle) = vehicle {
                    if self.object(vehicle)?.base.flags & 2 == 0 {
                        self.world_player_mut(player)?.controls.dword_34 = 0;
                    }
                }
            }
            let shown = self
                .world_player(player)?
                .byte_58
                .ok_or(PlayerUpdateError::UnknownField(player, 0x58))?;
            if shown != 0 {
                let lifecycle = match vehicle {
                    Some(vehicle) => Some(self.actor(vehicle)?.word_c4 as i32),
                    None => None,
                };
                let request = self.world_player(player)?.controls.dword_34;
                let attach = match (remote, lifecycle) {
                    // 0046A15F: the camera stays on the wreck.
                    (Some(_), Some(state)) if state < 3 => Some(true),
                    // 0046A1A2: back to the live vehicle.
                    (_, Some(3)) if remote.is_none() || request != 0 => Some(false),
                    _ => None,
                };
                if let Some(attach) = attach {
                    let camera = self
                        .world_camera(factory)?
                        .ok_or(PlayerAttachError::MissingCamera)?;
                    let handle = if attach {
                        self.actor(camera)?.handle
                    } else {
                        process_null
                    };
                    self.attach_player(player, handle, factory)?;
                    let d4 = self.world_player(player)?.handle_d4;
                    let target =
                        self.follow_target_resource(FollowTargetRequest::resolve_actor(d4))?;
                    self.replace_camera_target(camera, target, factory.settings)?;
                    self.world_player_mut(player)?.controls.dword_34 = 0;
                }
            }
            if remote.is_some() {
                unit = remote;
            }
        }
        let Some(actor) = unit else {
            return Ok(None);
        };
        let base = &mut self.owned_mut(actor)?.state.base;
        let flags = base.flags | 0x10;
        mark_flag_change(base, flags);
        Ok(Some(actor))
    }

    /// 0046A260 up to its vehicle call, offline. The message subobject's
    /// BYTE+0 gates the controls; while it is clear, the block takes each
    /// reference's held time (004F0040) capped at the delta (a quarter of it
    /// with G+5C5). While it is set, Fire selects in the message and the
    /// controls brake. Returns the call the caller makes on the actor;
    /// `unit` is that actor, which the brake reads.
    pub fn update_player_controls(
        &mut self,
        player: PlayerId,
        inputs: PlayerUpdateInputs,
        input: &mut InputRuntime,
        consumers: &PlayerInputConsumers,
        actor_flags_lifecycle: Option<(u32, i32)>,
        unit: Option<ObjectId>,
        network: Option<PlayerNetworkState<'_>>,
    ) -> Result<PlayerControlsUpdate, PlayerUpdateError> {
        if inputs.network != 0 && network.is_none() {
            return Err(PlayerUpdateError::Network(inputs.network));
        }
        let delta = inputs.delta_micros;
        let shown = self.world_player(player)?.message_byte_00() != 0;
        let mut omissions = BTreeSet::new();
        // 0046A27D..0046A2BE: under a shown message the Fire reference
        // (+80), held this tick (capped at the delta), presses message key
        // bit 1 (0046D2E0) once, latched by +FC; released, +FC and bit 1 of
        // +50 clear.
        if shown {
            let fire = (input.held_micros(consumers.fire_80)? as i32).min(delta);
            if fire == 0 {
                let state = self.world_player_mut(player)?;
                state.byte_fc = 0;
                let held = state
                    .dword_50
                    .ok_or(PlayerUpdateError::UnknownField(player, 0x50))?;
                state.dword_50 = Some(held & !1);
            } else if self.world_player(player)?.byte_fc == 0 {
                self.player_message_key(player, 1, &mut omissions)?;
                self.world_player_mut(player)?.byte_fc = 1;
            }
        }
        // 0046A2C1..0046A308: the +54 millisecond timer; its expiry runs
        // 0046D240, a no-op while no +50 key is held. An unwritten +54 only
        // paces that no-op; 0046D2E0 writes it before any key is held.
        let ms_signed = delta / 1000;
        let ms_unsigned = (delta as u32 / 1000) as i32;
        let state = self.world_player_mut(player)?;
        let flags_50 = state
            .dword_50
            .ok_or(PlayerUpdateError::UnknownField(player, 0x50))?;
        match state.dword_54 {
            Some(timer) if timer > ms_signed => {
                state.dword_54 = Some(timer.wrapping_sub(ms_unsigned));
            }
            Some(timer) => {
                state.dword_54 = Some(if flags_50 != 0 {
                    timer.wrapping_sub(ms_unsigned).wrapping_add(100)
                } else {
                    0
                });
                self.player_message_keys(player, &mut omissions)?;
            }
            None if flags_50 == 0 => {}
            None => return Err(PlayerUpdateError::UnknownField(player, 0x54)),
        }
        if shown {
            // 0046A719..0046A79C: the controls brake. Turn, pitch, Fire and
            // Grenade are zero; +3A, +42, the analog block and the +F4
            // lockout keep their values.
            let thrust = match unit {
                Some(unit) => self.shown_message_brake(unit)?,
                None => 0,
            };
            let controls = &mut self.world_player_mut(player)?.controls;
            controls.set_byte(0x3e, 1);
            controls.pitch_18 = 0;
            controls.turn_14 = 0;
            controls.thrust_1c = thrust;
            controls.fire_20 = 0;
            controls.grenade_24 = 0;
            return Ok(PlayerControlsUpdate {
                call: vehicle_call(actor_flags_lifecycle),
                omissions,
                placements: Vec::new(),
                restart_powerball: false,
            });
        }
        let cap = if inputs.slow_input_5c5 {
            delta / 4
        } else {
            delta
        };
        // The joystick's published X, Y and Z (004F0140 only reads them).
        let joystick = if inputs.input_mode_08 == 0 {
            None
        } else {
            Some([
                input.analog_value(consumers.analog_5c)?,
                input.analog_value(consumers.analog_60)?,
                input.analog_value(consumers.analog_64)?,
            ])
        };
        let mut held =
            |id| -> Result<i32, PlayerUpdateError> { Ok((input.held_micros(id)? as i32).min(cap)) };
        let dive = held(consumers.dive_70)?;
        let climb = held(consumers.climb_74)?;
        let left = held(consumers.left_68)?;
        let right = held(consumers.right_6c)?;
        let thrust = held(consumers.thrust_78)?;
        let reverse = held(consumers.reverse_7c)?;
        let state = self.world_player_mut(player)?;
        let controls = &mut state.controls;
        controls.set_byte(0x3e, 0);
        controls.pitch_18 = dive.wrapping_sub(climb);
        controls.turn_14 = left.wrapping_sub(right);
        controls.thrust_1c = thrust.wrapping_sub(reverse);
        controls.analog_04 = 0;
        // 0046A3B8..0046A46C: the Controls menu's Joystick (settings +8).
        match (inputs.input_mode_08, joystick) {
            (_, None) => {}
            // Digital: Y (+60) and X (+5C) past half deflection hold pitch
            // and turn at the uncapped delta, against their sign.
            (2, Some([x, y, _])) => {
                let toward = |value: i32| {
                    if value < 0 {
                        delta
                    } else {
                        delta.wrapping_neg()
                    }
                };
                if y.wrapping_abs() > 0x8000 {
                    controls.pitch_18 = toward(y);
                }
                if x.wrapping_abs() > 0x8000 {
                    controls.turn_14 = toward(x);
                }
            }
            // Analogue: X to +8 and Y to +C, both clamped to a unit; Z to
            // +10 as it is.
            (_, Some([x, y, z])) => {
                controls.analog_04 = 1;
                let unit = |value: i32| value.clamp(-0x10000, 0x10000);
                controls.axes_08 = [unit(x), unit(y), z];
            }
        }
        // 0046A46C: +F4 holds Fire and Grenade off; the raw delta counts it.
        if state.dword_f4 > delta {
            state.dword_f4 = state.dword_f4.wrapping_sub(delta);
            let controls = &mut state.controls;
            controls.fire_20 = 0;
            controls.grenade_24 = 0;
            controls.set_byte(0x3a, 0);
        } else {
            state.dword_f4 = 0;
            let fire = held(consumers.fire_80)?;
            let grenade = held(consumers.grenade_84)?;
            let controls = &mut self.world_player_mut(player)?.controls;
            controls.fire_20 = fire;
            controls.grenade_24 = grenade;
        }
        let controls = &mut self.world_player_mut(player)?.controls;
        let thrusting = controls.thrust_1c > 0;
        controls.set_byte(0x3a, u8::from(thrusting));
        let (placements, restart_powerball) = match network.filter(|_| inputs.network != 0) {
            Some(network) => self.network_player_keys(player, delta, network)?,
            None => {
                self.world_player_mut(player)?.controls.set_byte(0x42, 0);
                (Vec::new(), false)
            }
        };
        Ok(PlayerControlsUpdate {
            call: vehicle_call(actor_flags_lifecycle),
            omissions,
            placements,
            restart_powerball,
        })
    }

    /// 0046A4CE..0046A714, the network keys: `0054ED18` counts down
    /// (unsigned), holding the restart key +42 off while it runs. In a race
    /// (mode 3) a mode request +34 clears and tells the race's actor; in
    /// Powerball (mode 4) +28, +2C and +3F clear and the team key +3D flips
    /// the host's team flag. The host's restart +42 resets the race, the
    /// ball or the arena. The lockout +19C then counts down (signed), and
    /// +42 always ends cleared. Returns the placements an arena's restart
    /// builds again, and whether Powerball's restart runs.
    fn network_player_keys(
        &mut self,
        player: PlayerId,
        delta: i32,
        network: PlayerNetworkState<'_>,
    ) -> Result<(Vec<i16>, bool), PlayerUpdateError> {
        let PlayerNetworkState {
            key_countdown_ed18,
            team_flag_81c,
            mode,
            host,
        } = network;
        let race = self.race();
        let state = self.world_player_mut(player)?;
        if *key_countdown_ed18 > delta as u32 {
            *key_countdown_ed18 -= delta as u32;
            state.controls.set_byte(0x42, 0);
        } else {
            *key_countdown_ed18 = 0;
        }
        let controls = &mut state.controls;
        let mut lockout_only = false;
        let mut put_back = false;
        match mode {
            3 => {
                // 0046A54F: the race puts this machine's racer back (+34C).
                if controls.dword_34 != 0 {
                    controls.dword_34 = 0;
                    put_back = race.is_some();
                }
            }
            4 => {
                controls.dword_2c = 0;
                controls.dword_28 = 0;
                controls.set_byte(0x3f, 0);
                if controls.byte(0x3d) != 0 {
                    controls.set_byte(0x3d, 0);
                    if host {
                        *team_flag_81c = u8::from(*team_flag_81c == 0);
                    } else {
                        lockout_only = true;
                    }
                }
            }
            _ => {}
        }
        let restart = !lockout_only && host && controls.byte(0x42) != 0;
        let placements = if restart && !matches!(mode, 3 | 4) {
            self.restart_arena()?
        } else {
            Vec::new()
        };
        let state = self.world_player_mut(player)?;
        if state.dword_19c > delta {
            state.dword_19c -= delta;
        } else {
            state.dword_19c = 0;
        }
        state.controls.set_byte(0x42, 0);
        if let Some(race) = race {
            let race = self.race_state_mut(race)?;
            if put_back {
                race.restart_34c = 1;
            }
            // 0046A566: the host's restart stops a running race (0046F3F0)
            // and starts or ends one (0046F390).
            if restart && mode == 3 {
                race.stop();
                race.restart();
            }
        }
        Ok((placements, restart && mode == 4))
    }

    /// 0046A587, the host's restart of an arena that is neither a race nor
    /// Powerball: every score and the game's end clear, as do the teams'
    /// words (+0C..+14); each banner (+B8) away from home (+138) is
    /// disposed (00479FA0) and its placement (+2A) returned, in the actor
    /// list's order, to be built again. Those come home, so the walk never
    /// reaches one.
    pub(in crate::object_runtime) fn restart_arena(
        &mut self,
    ) -> Result<Vec<i16>, PlayerUpdateError> {
        let game = &mut self
            .network_game_mut()
            .ok_or(PlayerUpdateError::RestartState)?
            .game;
        game.scores = Default::default();
        game.ended = 0;
        for team in game.teams.iter_mut().flatten() {
            *team = crate::network::game::TeamScore {
                index: team.index,
                ..Default::default()
            };
        }
        let mut placements = Vec::new();
        for actor in self.actor_list().to_vec() {
            let Some(banner) = self.banner_of(actor) else {
                continue;
            };
            if self.selector40(banner)?.away_138 != 0 {
                placements.push(self.object(banner)?.base.placement_word as i16);
                self.dispose(banner)?;
            }
        }
        Ok(placements)
    }

    /// 0046A727..0046A794: when the unit's view (+64, the vehicle's own) is
    /// in mode 1 or 2 (+1E8: buggy or truck), the thrust is -100 times the
    /// unit's velocity along its basis row +60..+68; otherwise zero, as for
    /// the camera a spectator holds, whose +64 is the null 00401300.
    fn shown_message_brake(&self, unit: ObjectId) -> Result<i32, PlayerUpdateError> {
        let vehicle = match self.selector2_actor(unit) {
            Ok(vehicle) => vehicle,
            Err(_) if self.selector16(unit).is_ok() => return Ok(0),
            Err(_) => return Err(PlayerUpdateError::MessageBrakeView(unit)),
        };
        if !matches!(vehicle.dwords_1e8_1f4[0], 1 | 2) {
            return Ok(0);
        }
        let actor = self.actor(unit)?;
        let [x, y, z] = actor.words_8e_92.map(|word| f64::from(word as i16));
        let row = |index: usize| f64::from(f32::from_bits(actor.rotating.basis.words[index]));
        let forward = ftol((z * row(9) + y * row(8)) + x * row(7));
        Ok(forward.wrapping_mul(-100))
    }

    /// Stores a block the vehicle update read and cleared in place.
    pub fn set_player_controls(
        &mut self,
        player: PlayerId,
        controls: PlayerControlBlock,
    ) -> Result<(), PlayerAttachError> {
        self.world_player_mut(player)?.controls = controls;
        Ok(())
    }

    /// 0046A7C9..0046A85A, after the vehicle call: while command 3 is owned,
    /// +F8 counts the delta and every 8 s adds a +12C charge (an owned count
    /// above 3 resets to 3). Then 00413870(delta, 0) updates the message.
    pub fn finish_player_update(
        &mut self,
        player: PlayerId,
        delta_micros: i32,
    ) -> Result<(), PlayerUpdateError> {
        // 0046BDC0(3): bit 3 of the first +100 DWORD.
        if self.world_player(player)?.bits_100[0] & 1 << 3 != 0 {
            let state = self.world_player_mut(player)?;
            state.dword_f8 = state.dword_f8.wrapping_add(delta_micros);
            if state.dword_f8 > 8_000_000 {
                state.dword_f8 = 0;
                let old = state.dword_12c as i32;
                state.dword_12c = if old <= 3 { old + 1 } else { 3 } as u32;
            }
        }
        self.update_player_message(player, delta_micros, false)?;
        Ok(())
    }
}

/// The actor call after the block: none without an actor or its flag 2,
/// +24 below lifecycle 3, else +44.
fn vehicle_call(actor_flags_lifecycle: Option<(u32, i32)>) -> PlayerVehicleCall {
    match actor_flags_lifecycle {
        Some((flags, _)) if flags & 2 == 0 => PlayerVehicleCall::None,
        Some((_, lifecycle)) if lifecycle < 3 => PlayerVehicleCall::Update24,
        Some(_) => PlayerVehicleCall::Update44,
        None => PlayerVehicleCall::None,
    }
}

/// 00511BA8: truncation; an out-of-range conversion is DWORD zero.
fn ftol(value: f64) -> i32 {
    let value = value.trunc();
    if value.is_finite() && value.abs() < 9_223_372_036_854_775_808.0 {
        value as i64 as i32
    } else {
        0
    }
}

#[cfg(test)]
#[path = "controls_tests.rs"]
mod tests;
