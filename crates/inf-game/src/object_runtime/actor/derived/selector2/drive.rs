//! Selector 2's ground drive for vehicle modes 1 and 2, 004195F0, with the
//! helpers only it reaches: the turn rate 00419050 and its smoothing
//! 00419590, the analog throttle 00418F60, the ground sample 0041EDC0, the
//! suspension 0041F3F0 and the lateral grip 0041F7E0. The shared motion
//! leaves (00426860, 00426AE0, 0047D440, 0047B1E0) keep their own owners.
//!
//! The vehicle reads the player's control block at its own +0, which is
//! player +4: a vehicle-relative offset `n` is player offset `n + 4`.
//! Infest01 reaches this drive on 699 of its 707 Level-1 vehicle updates.

use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable};
use thiserror::Error;

use super::super::kinematics::{
    divide, ftol, ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorMotionError,
};
use super::super::physics::ActorPhysicsRequest;
use super::super::{transform_local_dwords, DerivedActorError};
use crate::object_runtime::actor_terrain::plane_height_at;
use crate::object_runtime::ActorAngularResponseRequest;
use crate::object_runtime::{
    ActorHandle, ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext,
    PlayerActorLookupRequest, PlayerAttachError, PlayerControlBlock, PlayerId,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_sound::ProcessSoundRef;
use crate::terrain_runtime::{TerrainRuntime, TerrainWaterSurfaceRequest};

/// VehicleTrackPhaseApproximation: 004156E0 never writes the track phase
/// +1FC, so retail starts it from allocator residue (" to " in Infest01) and
/// only adds the drive's steps. The value moves nothing but the wheel and
/// track animation (the model getter's selector 3), so the port starts it at
/// zero rather than leave the vehicle undrawable.
pub const VEHICLE_TRACK_PHASE_APPROXIMATION: u32 = 0;

/// VehicleEngineLoadApproximation: 004156E0 leaves the engine load +55C/+560
/// unwritten, and only the ground reset 004184B0 (modes 1 and 2) and the
/// skimmer's drive store it. A vehicle whose first mode is the hovercraft
/// (004185D0) or the VTOL (00418430) reaches its drive's load step
/// (00419533, 00418F0B) on allocator residue: the lab read 68637562 and
/// 00040069 there, the tail of a "Tabuchi" string, and the first tick
/// quartered them. The load moves nothing but the engine sound's pitch and
/// volume, so the port starts it at zero, as `--fresh-heap` retail does,
/// rather than stop the vehicle.
pub const VEHICLE_ENGINE_LOAD_APPROXIMATION: u32 = 0;

/// VehicleAnalogHoldApproximation: 004156E0 never writes the analog turn's
/// hold +248, and 00423200 reads it before storing when the stick is past
/// its hold threshold on a vehicle's first analog tick: retail starts that
/// tick from allocator residue. A stick inside the threshold stores zero
/// first. The hold only adds to the turn rate, which the drive clamps, so
/// the port starts it at zero, as `--fresh-heap` retail does, rather than
/// stop the vehicle.
pub const VEHICLE_ANALOG_HOLD_APPROXIMATION: u32 = 0;

/// VehicleTurnHoldApproximation: 004156E0 never writes the turn hold +240,
/// and only the ground reset 004184B0 (modes 1 and 2) and the turn rate
/// 00419050 itself store it. A vehicle that has only been the hovercraft,
/// VTOL or skimmer since it was built turns first on allocator residue
/// (00419077, 004190CF). The lab read zero there with either heap
/// (`level1-hover-turn-hold.json`); the hold only eases the turn rate past
/// 0x2000 and returns to zero once the turn is let go, so the port starts it
/// at zero rather than stop the vehicle.
pub const VEHICLE_TURN_HOLD_APPROXIMATION: u32 = 0;

/// Fields the drive owns that 004156E0 leaves unwritten. `None` is allocator
/// residue until an owned writer stores the field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VehicleDriveState {
    /// +198..+1A0: the ground normal 0041EDC0 stores as three floats.
    pub normal_198: Option<[u32; 3]>,
    /// +1FC: the track phase. 004156E0 leaves it to allocator residue and
    /// the drive only adds to it; the model getter's selector 3 animates the
    /// wheels and tracks with its low WORD. Construction starts it at
    /// [`VEHICLE_TRACK_PHASE_APPROXIMATION`].
    pub dword_1fc: Option<u32>,
    /// +21C: the turn rate 00419050 integrates; 004184B0 zeroes it.
    pub dword_21c: Option<u32>,
    /// +22C..+236: the dust vector, written once +228 exceeds 100.
    pub words_22c_236: Option<[u16; 6]>,
    /// +23C: the gear-change timer; 004184B0 zeroes it.
    pub dword_23c: Option<u32>,
    /// +240: time the turn input has been held; 004184B0 zeroes it.
    /// Construction starts it at [`VEHICLE_TURN_HOLD_APPROXIMATION`].
    pub dword_240: Option<u32>,
    /// +248: how long the analog turn 00423200 has seen the stick past its
    /// hold threshold, capped at 500. Construction starts it at
    /// [`VEHICLE_ANALOG_HOLD_APPROXIMATION`].
    pub dword_248: Option<u32>,
    /// +55C/+560: engine load halves; 004184B0 stores 2000. Construction
    /// starts them at [`VEHICLE_ENGINE_LOAD_APPROXIMATION`].
    pub dword_55c: Option<u32>,
    pub dword_560: Option<u32>,
    /// +574/+578: speed and speed-change magnitudes 00416170 publishes.
    pub dword_574: Option<u32>,
    pub dword_578: Option<u32>,
    /// +57C: the traction the drive derives each tick.
    pub dword_57c: Option<u32>,
    /// WORD +58C: the drive's sound/animation state bits.
    pub word_58c: Option<u16>,
    /// +548 as a live process sound record (the constructor stores zero).
    pub sound_548: Option<ProcessSoundRef>,
    /// Port-owned, not a retail field: the fractions of a unit the position
    /// step carries per axis under [`VehiclePositionStep::CarryRemainder`],
    /// in 1/0x8000 units with the sign of their axis's step. The mode reset
    /// `004184B0` clears them.
    pub position_remainder: [i32; 3],
}

/// The terrain the drive samples, as 00450A30/00410F60 see it.
#[derive(Clone, Copy)]
pub struct VehicleGround<'a> {
    pub terrain: &'a TerrainRuntime,
    /// H14 world flags; bit 0x40 enables 00410F60's waves.
    pub world_flags: u32,
    /// The animation clock 005471C0.
    pub clock: u32,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Clone, Copy)]
pub struct VehicleDriveRequest<'a> {
    pub delta: u32,
    /// Static tables (+188..+194 point into .data) and the 00530120 constants.
    pub executable: FrontendExecutable<'a>,
    pub ground: VehicleGround<'a>,
    /// Process 0054E894, which 0046B090 compares.
    pub process_null: ActorHandle,
    /// How the drive's last step moves the hull by its velocity.
    pub position_step: VehiclePositionStep,
    /// World-manager WORD `G+5C0`, the view zoom the free look writes.
    pub view_zoom_5c0: &'a std::cell::Cell<u16>,
    /// The network game's words, None offline (`0055F448` clear).
    pub network: Option<VehicleNetwork>,
}

/// The process words a network game's vehicle reads: the mode (settings
/// `+10`), the catch-up scale (`+4C`) and this machine's slot
/// (`0055F2F4`), whose race record the races' branches read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleNetwork {
    pub mode: u32,
    pub catchup: i32,
    pub local_slot: u8,
}

/// The drive's position step `0041A438..0041A47E`: each axis moves by
/// `velocity * (delta >> 5)`, divided by 0x8000 toward zero.
///
/// Retail drops the fraction every tick. The scheduler `004945A0` admits a
/// tick after 8000 µs, so above about 60 frames a second a slow axis never
/// moves: at 125 ticks a second a velocity below 132 moves nothing. The
/// suspension `0041F3F0` then balances gravity with a vertical velocity the
/// step cannot apply, and the hull hangs where it is. A hull hanging in the
/// suspension's upper band has little traction, and the forward motion
/// `00426860` truncates small traction to no acceleration at all, so the
/// vehicle stops for good (the buggy most often: its traction band is the
/// narrowest). See VEHICLE_DRIVE_RUNTIME.md, "The high frame-rate hang".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VehiclePositionStep {
    /// Retail: the fraction is dropped every tick.
    Retail,
    /// The port's fix (on by default): each axis keeps the fraction it
    /// drops in the vehicle's [`VehicleDriveState::position_remainder`] and
    /// adds it to the next step, so the hull moves by its velocity over
    /// time at any frame rate.
    #[default]
    CarryRemainder,
}

pub trait VehicleDriveHost {
    type Error;
    /// 00487EC0 at the vehicle's +32 with flag 0; 004923C0 releases it.
    fn play_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// 00487F60 at the vehicle's +32 with flag 0.
    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, Self::Error>;
    /// 00487FC0 at the vehicle's +32.
    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        reference: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// 00487FE0.
    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum VehicleDriveError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Construction(#[from] crate::object_runtime::ActorConstructionError),
    #[error("vehicle {vehicle:?} reaches the unported branch at {address:08X}")]
    Unported { vehicle: ObjectId, address: u32 },
    #[error("vehicle {vehicle:?} field +{offset:X} is unwritten where {address:08X} reads it")]
    UnknownField {
        vehicle: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("vehicle executable data at {address:08X} is unavailable: {reason}")]
    Executable { address: u32, reason: String },
    #[error("native signed division traps at {address:08X}")]
    Division { address: u32 },
    #[error("vehicle sound service failed: {0}")]
    Sound(E),
    #[error("vehicle host service failed: {0}")]
    Host(E),
    #[error("vehicle {0}")]
    Status(String),
}

/// 004195F0 compares the ground normal's local Y against 0051F198 (0.0).
const ZERO: f64 = 0.0;
/// 00530418: four local sample offsets around the hull, signed WORD triples.
const SAMPLE_OFFSETS: u32 = 0x0053_0418;

fn i16_of(word: u16) -> i32 {
    i32::from(word as i16)
}

impl ObjectArena {
    /// 004184B0: the reset a vehicle receives when it enters mode 1 or 2.
    /// The +188..+194 tables are the offline ones; the network variants are
    /// not admitted.
    pub fn reset_vehicle_ground_drive(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let state = self.selector2_actor_mut(vehicle)?;
        let previous = state.dwords_1e8_1f4[3];
        if previous != 1 && previous != 2 {
            state.drive.dword_21c = Some(0);
            state.dword_244 = 0;
            state.dword_238 = 1;
            state.drive.dword_23c = Some(0);
            state.drive.dword_57c = Some(0);
        }
        state.dwords_220_224[1] = 0;
        state.drive.dword_560 = Some(2000);
        state.drive.dword_55c = Some(2000);
        state.bytes_20a_20c[2] = 0;
        state.dword_228 = Some(0);
        state.drive.dword_240 = Some(0);
        // Port-owned: the carried position fractions start over with the
        // mode (see VehiclePositionStep).
        state.drive.position_remainder = [0; 3];
        if state.dwords_1e8_1f4[0] == 1 {
            state.words_188_194 = [0x0053_0438, 0x0053_0440, 0x0053_0430, 0x0053_0498];
            return Ok(());
        }
        state.words_188_194 = [0x0053_04c0, 0x0053_04c8, 0x0053_04b8, 0x0053_04f8];
        let word_184 = state.word_184;
        // 004185BD: +8C = +184 << 1.
        let word_184 = word_184.ok_or(DerivedActorError::NotSelector2(vehicle))?;
        self.actor_mut(vehicle)
            .map_err(|_| DerivedActorError::NotSelector2(vehicle))?
            .word_8c = (word_184 as u16).wrapping_shl(1);
        Ok(())
    }

    /// 004195F0 for an offline vehicle in mode 1 or 2. `controls` is the
    /// driving player's block; the drive clears its +3A (jump) and its
    /// +38 bit 0 (braking) and may consume one of the owning player's +12C
    /// jump charges.
    pub fn tick_vehicle_ground_drive<H: VehicleDriveHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &mut PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let step = delta >> 5;
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };
        let basis = self.actor(vehicle)?.rotating.basis;
        let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let velocity = self.actor(vehicle)?.words_8e_92.map(i16_of);
        // 00419618..00419640: velocity on the forward row, (Y + Z) + X.
        let speed = ftol(
            (f64::from(velocity[1]) * m(8) + f64::from(velocity[2]) * m(9))
                + f64::from(velocity[0]) * m(7),
        );
        let mut forward = speed;
        let abs_speed = speed.wrapping_abs();
        self.selector2_actor_mut(vehicle)?.drive.dword_57c = Some(0);
        let mut steer: i32 = 0;
        let mut power: i32 = 0;
        let mut engaged = true;

        if self.selector2_actor(vehicle)?.byte_1c0 != 0 || controls.byte(0x3f) != 0 {
            self.vehicle_free_look(vehicle, controls, delta, request.view_zoom_5c0)?;
        }
        if controls.byte(0x3e) != 0 {
            self.selector2_actor_mut(vehicle)?.dword_238 = 1;
        }
        {
            let state = self.selector2_actor_mut(vehicle)?;
            if (state.dwords_5ac_5b0[0] as i32) > 0 {
                state.dword_238 = 2;
                state.words_188_194[1] = 0x0053_0480;
                power = 0x7ffff;
                steer = 0xffff;
                let remaining = (state.dwords_5ac_5b0[0] as i32).wrapping_sub(delta as i32);
                state.dwords_5ac_5b0[0] = remaining as u32;
                if remaining <= 0 {
                    state.words_188_194[1] = 0x0053_0460;
                    state.dwords_5ac_5b0[0] = 0;
                }
            }
        }
        if controls.byte(0x3a) != 0 {
            self.vehicle_jump(vehicle, controls, request, host)?;
            controls.set_byte(0x3a, 0);
        }
        let thrust = controls.thrust_1c;
        let pitch = controls.pitch_18;
        if (thrust < 0 || pitch > 0) && speed > 0 {
            controls.bytes_38[0] |= 1;
        }
        let timer_224 = self.selector2_actor(vehicle)?.dwords_220_224[1] as i32;
        if timer_224 != 0 {
            let state = self.selector2_actor_mut(vehicle)?;
            state.dword_238 = 2;
            power = 0x7ffff;
            steer = 0xffff;
            if timer_224 > delta as i32 {
                state.dwords_220_224[1] = timer_224.wrapping_sub(delta as i32) as u32;
            } else {
                state.dwords_220_224[1] = 0;
                state.words_188_194[1] = if state.dwords_1e8_1f4[0] == 1 {
                    0x0053_0440
                } else {
                    0x0053_04c8
                };
            }
        } else if thrust != 0 {
            power = if thrust > 0 { 0x7ffff } else { -0x7fff };
        } else if pitch != 0 {
            power = if pitch > 0 { -0x7fff } else { 0x7ffff };
        } else if controls.analog_04 != 0 {
            steer = analog_throttle(controls);
            let table = self.selector2_actor(vehicle)?.words_188_194[1];
            let radius = u32::from(self.object(vehicle)?.base.word_28);
            let raw = if steer > 0 {
                read_dword(request.executable, table)?.wrapping_mul(radius) as i32
            } else {
                (read_dword(request.executable, table + 4)?.wrapping_mul(radius) as i32)
                    .wrapping_neg()
            };
            power = (raw >> 12).wrapping_mul(steer) >> 16;
        }
        self.vehicle_turn_rate(vehicle, step, controls, abs_speed)?;
        let turn_220 = self.selector2_actor(vehicle)?.dwords_220_224[0] as i32;
        let mut turn = speed.wrapping_mul(turn_220) / 0x300;
        self.smooth_vehicle_turn(vehicle, delta)?;

        // 00419A44..00419B5D: a near-stationary hard turn becomes a pivot.
        {
            let state = self.selector2_actor(vehicle)?;
            if state.dword_238 != 0
                && speed > -0x14
                && speed < 0x96
                && thrust >= 0
                && pitch <= 0
                && turn_220.wrapping_abs() > 0x1800
                && steer > -0xc000
                && turn_220 != 0
            {
                let planar = velocity[0]
                    .wrapping_mul(velocity[0])
                    .wrapping_add(velocity[2].wrapping_mul(velocity[2]));
                if (isqrt_48efc0(planar) as u16 as i16) < 0x96 {
                    let excess = if turn_220 > 0x1800 {
                        turn_220 - 0x1800
                    } else if turn_220 < -0x1800 {
                        turn_220 + 0x1800
                    } else {
                        0
                    };
                    turn = excess.wrapping_mul(150) / 0x300;
                    if turn != 0 {
                        forward = turn_220.wrapping_abs().wrapping_mul(150) / 0x14000;
                        if power < 0x4b {
                            power = 0x4b;
                        }
                    }
                }
            }
        }
        {
            let state = self.selector2_actor_mut(vehicle)?;
            if power == 0 {
                state.drive.dword_23c = Some(0);
                let mode = state.dword_238;
                if (mode == 2 && forward < -0x40) || (mode == 0 && forward > 0x40) {
                    state.dword_238 = 1;
                }
            } else {
                let gear = state.drive.dword_23c.ok_or(unknown(0x23c, 0x0041_9b67))? as i32;
                if power > 0 {
                    if gear == 0 && (state.dword_238 as i32) < 2 {
                        state.dword_238 = state.dword_238.wrapping_add(1);
                    }
                } else if gear == 0 && (state.dword_238 as i32) > 0 {
                    state.dword_238 = state.dword_238.wrapping_sub(1);
                }
                state.drive.dword_23c = Some(if gear < 250_000 {
                    gear.wrapping_add(delta as i32) as u32
                } else {
                    0
                });
            }
            if state.dword_238 == 1 {
                engaged = false;
            }
        }

        let suspension_table = self.selector2_actor(vehicle)?.words_188_194[3];
        let contact = self.vehicle_suspension(vehicle, suspension_table, step, request)?;
        // 0047AD10, halved while flag 20000000 (submerged) is set.
        let gravity = if self.object(vehicle)?.base.flags & 0x2000_0000 != 0 {
            delta >> 1
        } else {
            delta
        };
        {
            let velocity_y = &mut self.actor_mut(vehicle)?.words_8e_92[1];
            *velocity_y = velocity_y.wrapping_sub((gravity >> 10) as u16);
        }

        let mut slip: i32 = 0;
        if contact.support == 0 {
            let phase = &mut self.selector2_actor_mut(vehicle)?.drive.dword_1fc;
            if let Some(value) = phase {
                let increment = if thrust != 0 {
                    if thrust > 0 {
                        0x100
                    } else {
                        -0x100
                    }
                } else {
                    forward << 4
                };
                *value = value.wrapping_add(increment as u32);
            }
        } else {
            let mode = self.selector2_actor(vehicle)?.dwords_1e8_1f4[0];
            let align = if mode == 1 { delta >> 3 } else { delta / 10 };
            let normal = self
                .selector2_actor(vehicle)?
                .drive
                .normal_198
                .ok_or(unknown(0x198, 0x0041_9c61))?;
            // 00426730 -> 0047D440 on the basis Y row (+54).
            let gain = (contact.support as u32).wrapping_mul(align) >> 14;
            self.apply_actor_angular_response(
                vehicle,
                ActorAngularResponseRequest {
                    current_axis: [4, 5, 6].map(|index| f32::from_bits(basis.words[index])),
                    desired_axis: normal.map(f32::from_bits),
                    gain: gain as i32,
                },
            )?;
            // 0047A6A0 -> 004921E0: the normal's local Y, (Z + Y) + X.
            let [nx, ny, nz] = normal.map(|word| f64::from(f32::from_bits(word)));
            let local_y = ((nz * m(6) + ny * m(5)) + nx * m(4)) as f32;
            let lateral_table = self.selector2_actor(vehicle)?.words_188_194[0];
            // FCOMP/TEST AH,41h: zero, negative or unordered is steep.
            let upright = matches!(
                f64::from(local_y).partial_cmp(&ZERO),
                Some(std::cmp::Ordering::Greater)
            );
            if !upright {
                slip = self.vehicle_grip(
                    vehicle,
                    contact.traction,
                    step,
                    lateral_table,
                    false,
                    request,
                )?;
            } else {
                let traction = ftol(f64::from(contact.traction) * f64::from(local_y));
                self.selector2_actor_mut(vehicle)?.drive.dword_57c = Some(traction as u32);
                if traction != 0 {
                    let mode_238 = self.selector2_actor(vehicle)?.dword_238;
                    if power != 0 || mode_238 == 1 {
                        let table = self.selector2_actor(vehicle)?.words_188_194[1];
                        let mut table = forward_table(request.executable, table)?;
                        // 00419CB8..00419D50: in a race the table's
                        // acceleration loses a share for each place ahead of
                        // ninth, by the catch-up scale, until 00419D78
                        // restores it.
                        if let Some(rank) = self.race_rank(request)? {
                            let catchup = request.network.map_or(0, |network| network.catchup);
                            let behind = (8 - rank).max(0);
                            let share = catchup.wrapping_mul(behind).wrapping_mul(10) / 256;
                            table.acceleration = table
                                .acceleration
                                .wrapping_sub(table.acceleration.wrapping_mul(share) / 32);
                        }
                        self.apply_actor_forward_motion(
                            vehicle,
                            ActorForwardMotionRequest {
                                power: traction,
                                desired_velocity: power,
                                step_units: step,
                                table,
                            },
                        )?;
                    }
                    let mode_238 = self.selector2_actor(vehicle)?.dword_238;
                    let settle = mode_238 == 1 || (power == 0 && abs_speed < 4);
                    slip =
                        self.vehicle_grip(vehicle, traction, step, lateral_table, settle, request)?;
                    let mut traction = traction;
                    if slip != 0 {
                        let amount = slip.wrapping_abs();
                        traction = if amount < 0x80 {
                            (0x80 - amount).wrapping_mul(traction) >> 7
                        } else {
                            0
                        };
                    }
                    if controls.bytes_38[0] != 0 {
                        traction = traction.wrapping_shl(1);
                    }
                    self.selector2_actor_mut(vehicle)?.drive.dword_57c = Some(traction as u32);
                    let table = self.selector2_actor(vehicle)?.words_188_194[2];
                    self.apply_actor_angular_motion(
                        vehicle,
                        ActorAngularMotionRequest {
                            power: traction,
                            desired_turn: turn,
                            step_units: step,
                            table: angular_table(request.executable, table)?,
                            axis: ActorAngularAxis::WorldVector(normal.map(f32::from_bits)),
                        },
                    )?;
                }
            }
            if slip != 0 {
                // 0047B4B0: the skid rolls the hull about its local Z.
                self.add_local_angular(vehicle, [0, 0, slip.wrapping_neg().wrapping_mul(8)])?;
            }
        }

        // 00419EDA: the engine/dust intensity +228.
        let timer_224 = self.selector2_actor(vehicle)?.dwords_220_224[1];
        let intensity = if timer_224 != 0 {
            2000
        } else {
            let velocity = self.actor(vehicle)?.words_8e_92.map(i16_of);
            let sum = velocity[0]
                .wrapping_mul(velocity[0])
                .wrapping_add(velocity[1].wrapping_mul(velocity[1]))
                .wrapping_add(velocity[2].wrapping_mul(velocity[2]));
            let mut magnitude = if sum as u32 & 0x8000_0000 != 0 {
                (isqrt_48efc0((sum as u32 >> 2) as i32) << 1) as i32
            } else {
                isqrt_48efc0(sum) as i32
            };
            if engaged && power != 0 {
                magnitude = magnitude.wrapping_add(300);
            }
            magnitude
        };
        self.selector2_actor_mut(vehicle)?.dword_228 = Some(intensity as u32);
        self.vehicle_skid_sound(vehicle, slip, request, host)?;

        {
            let state = self.selector2_actor_mut(vehicle)?;
            let mut bits: u16 = if state.dwords_220_224[1] != 0 {
                0
            } else if engaged && power != 0 {
                2
            } else {
                1
            };
            if engaged {
                bits |= 4;
            }
            if thrust > 0 {
                bits |= 8;
            }
            state.drive.word_58c = Some(bits);
        }
        let intensity = self.selector2_actor(vehicle)?.dword_228.unwrap_or(0) as i32;
        if intensity > 100 {
            self.vehicle_dust(vehicle, intensity, engaged, thrust, step)?;
        }
        if self.object(vehicle)?.base.flags & 0x4000_0000 == 0 {
            self.vehicle_water_drag(vehicle, abs_speed, step, request)?;
        }
        self.tick_actor_physics(
            vehicle,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 0x16,
                sine_quarter: request.ground.sine_quarter,
            },
        )?;
        {
            let velocity = self.actor(vehicle)?.words_8e_92;
            let mut remainder = self.selector2_actor(vehicle)?.drive.position_remainder;
            let base = &mut self.owned_mut(vehicle)?.state.base;
            base.dirty_word |= 0xc0;
            let mut position = base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
            step_vehicle_position(
                &mut position,
                velocity,
                step,
                request.position_step,
                &mut remainder,
            );
            base.position_words = Some(position);
            self.selector2_actor_mut(vehicle)?.drive.position_remainder = remainder;
        }
        {
            let state = self.selector2_actor(vehicle)?;
            let traction = state.drive.dword_57c.unwrap_or(0) as i32;
            let mode = state.dwords_1e8_1f4[0];
            let increment = if traction > 0x4000 {
                let rate = read_dword(
                    request.executable,
                    if mode == 2 { 0x0053_05d4 } else { 0x0053_05d0 },
                )? as i32;
                rate.wrapping_mul(forward)
            } else if thrust != 0 {
                if thrust > 0 {
                    0x100
                } else {
                    -0x100
                }
            } else {
                forward << 4
            };
            if let Some(value) = &mut self.selector2_actor_mut(vehicle)?.drive.dword_1fc {
                *value = value.wrapping_add(increment as u32);
            }
        }
        controls.bytes_38[0] &= 0xfe;
        self.vehicle_engine_load(vehicle, forward, thrust, pitch, request)?;
        Ok(())
    }

    /// 004196F5..00419890: a held +3A jumps or boosts the vehicle once per
    /// charge while command 3 is owned.
    fn vehicle_jump<H: VehicleDriveHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let handle = self.actor(vehicle)?.handle;
        let process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(request.process_null),
        };
        let player = self
            .find_player_for_actor_handle(
                PlayerActorLookupRequest { handle },
                PlayerActorLookupContext {
                    process: Some(&process),
                },
            )
            .map_err(|_| VehicleDriveError::Unported {
                vehicle,
                address: 0x0041_96fc,
            })?;
        // 00419703 reads player +88 before testing the player for null.
        let player: PlayerId = player.ok_or(VehicleDriveError::Unported {
            vehicle,
            address: 0x0041_9703,
        })?;
        let state = self.world_player(player)?;
        if state.message_byte_00() != 0
            || state.bits_100[0] & 1 << 3 == 0
            || self.selector2_actor(vehicle)?.dwords_220_224[1] != 0
            || (state.dword_12c as i32) <= 0
        {
            return Ok(());
        }
        let charges = state.dword_12c;
        self.world_player_mut(player)?.dword_12c = charges.wrapping_sub(1);
        let flip = controls.pitch_18 > 0;
        let mut side: i32 = if controls.turn_14 > 0 {
            1
        } else if controls.turn_14 < 0 {
            -1
        } else {
            0
        };
        let mut flip = flip;
        if !flip && controls.analog_04 != 0 {
            if controls.axes_08[1] < -0x6000 {
                flip = true;
            }
            if controls.axes_08[0] > 0x8000 {
                side = -1;
            } else if controls.axes_08[0] < -0x8000 {
                side = 1;
            }
        }
        self.selector2_actor_mut(vehicle)?.bytes_20a_20c[2] = u8::from(flip);
        if flip {
            let roll = match side {
                1 => [0, 0, 75_000],
                -1 => [0, 0, -75_000],
                _ => [10_000, 0, 0],
            };
            self.add_local_angular(vehicle, roll)?;
            let lift_x: i16 = match side {
                1 => -500,
                -1 => 500,
                _ => 0,
            };
            let world = self.transform_visual_vector_words(vehicle, [lift_x as u16, 800, 0])?;
            self.add_actor_linear_velocity(vehicle, world)?;
            self.selector2_actor_mut(vehicle)?.dwords_220_224[1] = 2_000_000;
        } else {
            let state = self.selector2_actor_mut(vehicle)?;
            state.words_188_194[1] = if state.dwords_1e8_1f4[0] == 1 {
                0x0053_04a8
            } else {
                0x0053_04e8
            };
            state.dwords_220_224[1] = 3_000_000;
        }
        host.play_sound(self, vehicle, 0x19, 0x10000, 0x10000)
            .map_err(VehicleDriveError::Sound)?;
        Ok(())
    }

    /// 0047B4B0: a local DWORD vector through 004907E0 onto the angular DWORDs.
    pub(super) fn add_local_angular(
        &mut self,
        vehicle: ObjectId,
        local: [i32; 3],
    ) -> Result<(), ObjectError> {
        let basis = self.actor(vehicle)?.rotating.basis;
        let world = transform_local_dwords(basis, local);
        self.add_actor_angular_velocity(vehicle, world.map(|word| word as u32))
    }

    /// 00419050(step, controls, |speed|): the turn rate +21C, copied to +220.
    pub(super) fn vehicle_turn_rate<E>(
        &mut self,
        vehicle: ObjectId,
        step: u32,
        controls: &PlayerControlBlock,
        abs_speed: i32,
    ) -> Result<(), VehicleDriveError<E>> {
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };
        let state = self.selector2_actor_mut(vehicle)?;
        let (mut rate, full) = if abs_speed == -1 {
            (step as i32, step as i32)
        } else if controls.bytes_38[0] != 0 {
            let rate = step.wrapping_mul(8) as i32;
            (rate, rate)
        } else {
            let held = state.drive.dword_240.ok_or(unknown(0x240, 0x0041_9077))?;
            let mut speed = abs_speed;
            let excess = (held as i32).wrapping_sub(0x2000);
            if excess > 0 {
                speed = if excess < 0x8000 {
                    (0x8000 - excess).wrapping_mul(speed) >> 15
                } else {
                    0
                };
            }
            let full = (step.wrapping_mul(5) >> 1) as i32;
            if speed >= 0x29a {
                speed = 0x29a;
            }
            (full.wrapping_sub(speed.wrapping_mul(full) >> 10), full)
        };
        let mut rate_21c = state.drive.dword_21c.ok_or(unknown(0x21c, 0x0041_90e2))? as i32;
        let turn = controls.turn_14;
        let zero = if turn != 0 {
            let held = state.drive.dword_240.ok_or(unknown(0x240, 0x0041_90cf))?;
            state.drive.dword_240 = Some(held.wrapping_add(step));
            if turn > 0 {
                rate_21c = rate_21c.wrapping_sub(rate);
                if rate_21c < -0x14000 {
                    rate_21c = -0x14000;
                } else if rate_21c > 0 {
                    rate_21c = rate_21c.wrapping_sub(rate);
                }
                rate_21c > 0
            } else {
                rate_21c = rate_21c.wrapping_add(rate);
                if rate_21c > 0x14000 {
                    rate_21c = 0x14000;
                } else if rate_21c < 0 {
                    rate_21c = rate_21c.wrapping_add(rate);
                }
                rate_21c < 0
            }
        } else {
            state.drive.dword_240 = Some(0);
            if controls.thrust_1c > 0 || controls.pitch_18 > 0 {
                rate = full;
            }
            if rate_21c > 0 {
                rate_21c = rate_21c.wrapping_sub(rate.wrapping_shl(2));
                rate_21c < 0
            } else if rate_21c < 0 {
                rate_21c = rate_21c.wrapping_add(rate.wrapping_shl(2));
                rate_21c > 0
            } else {
                false
            }
        };
        if zero {
            rate_21c = 0;
        }
        state.drive.dword_21c = Some(rate_21c as u32);
        state.dwords_220_224[0] = rate_21c as u32;
        if controls.analog_04 != 0 {
            let hold = state.drive.dword_248.ok_or(unknown(0x248, 0x0042_3251))?;
            let (rate, hold) = analog_turn(
                step,
                controls.axes_08[0],
                hold,
                state.dwords_220_224[0] as i32,
            );
            state.dwords_220_224[0] = rate as u32;
            state.drive.dword_248 = Some(hold);
        }
        Ok(())
    }

    /// 00419590(delta): +244 follows +220 by at most the scaled difference
    /// plus delta / 16, never overshooting.
    fn smooth_vehicle_turn(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
    ) -> Result<(), DerivedActorError> {
        let state = self.selector2_actor_mut(vehicle)?;
        let difference = (state.dwords_220_224[0] as i32).wrapping_sub(state.dword_244 as i32);
        if difference == 0 {
            return Ok(());
        }
        let product = i64::from(delta.wrapping_shl(13) as i32) * i64::from(difference);
        let scaled = (product >> 31) as i32;
        let increment = if difference > 0 {
            let value = scaled.wrapping_add((delta >> 4) as i32);
            if value <= difference {
                value
            } else {
                difference
            }
        } else {
            let value = scaled.wrapping_sub((delta >> 4) as i32);
            if value >= difference {
                value
            } else {
                difference
            }
        };
        state.dword_244 = (state.dword_244 as i32).wrapping_add(increment) as u32;
        Ok(())
    }

    /// 0041EDC0(normal, mode != 0): four hull samples (the first two looking
    /// ahead by a quarter of the velocity), their diagonal cross product as
    /// the +198 normal, and the summed sample height / 4 as the result.
    fn vehicle_ground_sample<E>(
        &mut self,
        vehicle: ObjectId,
        mode: u32,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<i32, VehicleDriveError<E>> {
        let ground = request.ground;
        let float = self.derived_actor(vehicle)?.byte_160 != 0;
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let velocity = self.actor(vehicle)?.words_8e_92.map(i16_of);
        // Port setting, not retail: the wheeled forms (the buggy, 1, and the
        // truck, 2) sink into the puddles' carve where the remaster sets one
        // (TerrainRuntime::wheel_height_at); retail's ground otherwise.
        let wheeled = matches!(self.selector2_actor(vehicle)?.dwords_1e8_1f4[0], 1 | 2);
        // Floating hulls sample max(ground, 00410F60's water) at the clock.
        let height = |x: u16, z: u16| -> i16 {
            if !float {
                return if wheeled {
                    ground.terrain.wheel_height_at([x, z])
                } else {
                    ground.terrain.height_at([x, z])
                };
            }
            let terrain = ground.terrain.height_at([x, z]);
            let water = ground.terrain.water_surface_height(
                TerrainWaterSurfaceRequest {
                    xz: [x, z],
                    wave_clock: ground.clock,
                    world_flags: ground.world_flags,
                    terrain_height: terrain,
                },
                ground.sine_quarter,
            );
            terrain.max(water)
        };
        let mut offsets = [0u16; 12];
        for pair in 0..6 {
            let dword = read_dword(request.executable, SAMPLE_OFFSETS + pair * 4)?;
            offsets[pair as usize * 2] = dword as u16;
            offsets[pair as usize * 2 + 1] = (dword >> 16) as u16;
        }
        let mut samples = [[0u16; 3]; 4];
        let mut sum: i32 = 0;
        for index in 0..4 {
            let offset = [
                offsets[index * 3],
                offsets[index * 3 + 1],
                offsets[index * 3 + 2],
            ];
            let world = self.transform_visual_vector_words(vehicle, offset)?;
            let x = world[0].wrapping_add(position[0]);
            let z = world[2].wrapping_add(position[2]);
            let first = height(x, z);
            samples[index] = [x, first as u16, z];
            if index >= 2 {
                sum = sum.wrapping_add(i32::from(first));
                continue;
            }
            // 0041EFA4: the front pair also looks a quarter-velocity ahead.
            let ahead_x = x.wrapping_add((velocity[0] / 4) as u16);
            let ahead_z = z.wrapping_add((velocity[2] / 4) as u16);
            let second = height(ahead_x, ahead_z);
            if second > first {
                sum = sum.wrapping_add(i32::from(if mode > 1 { second } else { first }));
                samples[index] = [ahead_x, second as u16, ahead_z];
            } else {
                sum = sum.wrapping_add(i32::from(first));
            }
        }
        let difference = |a: [u16; 3], b: [u16; 3]| -> [i16; 3] {
            std::array::from_fn(|axis| a[axis].wrapping_sub(b[axis]) as i16)
        };
        let mut a = difference(samples[0], samples[3]);
        let mut b = difference(samples[1], samples[2]);
        if a.iter().all(|&value| value == 0) {
            a[2] = 0x1000;
        }
        if b.iter().all(|&value| value == 0) {
            b[0] = 0x1000;
        }
        let [ax, ay, az] = a.map(f64::from);
        let [bx, by, bz] = b.map(f64::from);
        // 0041F326..0041F356: X and Y spill to f32; Z stays on the stack.
        let cx = (bz * ay - by * az) as f32;
        let cy = (bx * az - bz * ax) as f32;
        let cz = ax * by - bx * ay;
        let length =
            ((cz * cz + f64::from(cy) * f64::from(cy)) + f64::from(cx) * f64::from(cx)).sqrt();
        let mut normal = if length == 0.0 {
            [1.0f32, 0.0, 0.0]
        } else {
            [
                (f64::from(cx) / length) as f32,
                (f64::from(cy) / length) as f32,
                (cz / length) as f32,
            ]
        };
        if normal[1] < 0.0 {
            normal = normal.map(|value| -value);
        }
        self.selector2_actor_mut(vehicle)?.drive.normal_198 = Some(normal.map(f32::to_bits));
        Ok(sum >> 2)
    }

    /// Port option, the remaster's picture ([`crate::craft_spray`]): the
    /// four hull points 0041EDC0 samples the ground under (table 00530418,
    /// front right and left, then rear), turned by the hull and placed at
    /// it, in world words, without the front pair's look ahead; none for
    /// an object that is not a placed vehicle.
    pub(crate) fn vehicle_hull_points(
        &self,
        vehicle: ObjectId,
        executable: FrontendExecutable<'_>,
    ) -> Option<[[u16; 3]; 4]> {
        let position = self.object(vehicle).ok()?.base.position_words?;
        let table = executable.mapped(SAMPLE_OFFSETS, 24).ok()?;
        let word =
            |index: usize| u16::from_le_bytes([table.bytes[2 * index], table.bytes[2 * index + 1]]);
        let mut points = [[0; 3]; 4];
        for (index, point) in points.iter_mut().enumerate() {
            let offset = [word(index * 3), word(index * 3 + 1), word(index * 3 + 2)];
            let turned = self.transform_visual_vector_words(vehicle, offset).ok()?;
            *point = std::array::from_fn(|axis| turned[axis].wrapping_add(position[axis]));
        }
        Some(points)
    }

    /// 0041F3F0(out, table, step, +198): hold the hull above the ground
    /// sample, cancel velocity into it and add grip along the normal.
    pub(super) fn vehicle_suspension<E>(
        &mut self,
        vehicle: ObjectId,
        table_address: u32,
        step: u32,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<VehicleContact, VehicleDriveError<E>> {
        let table = suspension_table(request.executable, table_address)?;
        let mode = if self.selector2_actor(vehicle)?.dwords_1e8_1f4[0] == 5 {
            2
        } else {
            1
        };
        let average = self.vehicle_ground_sample(vehicle, mode, request)?;
        let normal = self
            .selector2_actor(vehicle)?
            .drive
            .normal_198
            .expect("0041EDC0 stored the normal")
            .map(|word| f64::from(f32::from_bits(word)));
        let radius = i32::from(self.object(vehicle)?.base.word_28);
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let clearance = i16_of(position[1]).wrapping_sub(average);
        let scaled = |height: i32| -> Result<i32, VehicleDriveError<E>> {
            divide(
                ftol(f64::from(height) * normal[1]).wrapping_shl(12),
                radius,
                0x0041_f447,
            )
            .map_err(|_| VehicleDriveError::Division {
                address: 0x0041_f447,
            })
        };
        let mut ratio = scaled(clearance)?;
        let mut contact = VehicleContact::default();
        let mut remainder: i32 = 0;
        if ratio > 0x1666 {
            contact.support = 0;
        } else if ratio > 0 {
            let value = ((0x1666 - ratio).wrapping_shl(14) / 0x1666).wrapping_add(0x3fff);
            contact.support = value;
            remainder = 0xffff - value;
        } else {
            contact.support = 0xffff;
        }
        let mut sunk = false;
        'grip: {
            if ratio >= i32::from(table[3]) {
                contact.grip = 0;
                break 'grip;
            }
            if ratio > i32::from(table[2]) {
                let span = i32::from(table[3]) - i32::from(table[2]);
                let left = i32::from(table[3]) - ratio;
                contact.grip = (table[0] as u16)
                    .wrapping_add((left.wrapping_mul(i32::from(table[0])) / span) as u16);
                contact.support = contact
                    .support
                    .wrapping_add(left.wrapping_mul(remainder) / span);
            } else {
                contact.support = 0xffff;
                contact.grip = (table[0] as u16).wrapping_shl(1);
                let mut depth = ratio;
                if self.derived_actor(vehicle)?.byte_160 != 0 && ratio < i32::from(table[1]) {
                    if ratio < 0 {
                        sunk = true;
                    }
                    let terrain = request.ground.terrain.height_at([position[0], position[2]]);
                    depth = scaled(i16_of(position[1]).wrapping_sub(i32::from(terrain)))?;
                }
                if depth < i32::from(table[1]) && clearance < radius {
                    let push = (((i32::from(table[1]) - depth).wrapping_mul(radius)) >> 12) as i16;
                    let push = f64::from(push);
                    let lift = [0, 1, 2].map(|axis| ftol(push * normal[axis]) as u16);
                    let base = &mut self.owned_mut(vehicle)?.state.base;
                    let mut position = base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
                    for (word, lift) in position.iter_mut().zip(lift) {
                        *word = word.wrapping_add(lift);
                    }
                    base.position_words = Some(position);
                    let into = self.velocity_along(vehicle, normal)?;
                    if into < 0 {
                        let back = f64::from(into.wrapping_neg() as i16);
                        let impulse = [0, 1, 2].map(|axis| ftol(back * normal[axis]) as u16);
                        self.add_actor_linear_velocity(vehicle, impulse)?;
                    }
                    ratio = i32::from(table[1]);
                }
            }
            if contact.support == 0 {
                break 'grip;
            }
            let along = self.velocity_along(vehicle, normal)?;
            if along > 0 {
                if !sunk {
                    contact.grip = (i32::from(contact.grip).wrapping_shl(12)
                        / along.wrapping_add(0x100).wrapping_shl(4))
                        as u16;
                }
            } else if along < 0 {
                let mut press = divide(along.wrapping_shl(12), radius, 0x0041_f70d)
                    .map_err(|_| VehicleDriveError::Division {
                        address: 0x0041_f70d,
                    })?
                    .wrapping_neg();
                if press > 0x1000 {
                    press = 0x1000;
                }
                contact.grip = (press
                    .wrapping_add(0x1000)
                    .wrapping_mul(i32::from(contact.grip))
                    >> 12) as u16;
            }
            let push = f64::from((i32::from(contact.grip).wrapping_mul(step as i32) >> 15) as i16);
            let impulse = [0, 1, 2].map(|axis| ftol(push * normal[axis]) as u16);
            self.add_actor_linear_velocity(vehicle, impulse)?;
        }
        contact.ratio = ratio;
        contact.traction = if ratio <= i32::from(table[4]) {
            0xffff
        } else if ratio >= i32::from(table[5]) {
            0
        } else {
            let left = i32::from(table[5]) - ratio;
            left.wrapping_mul(0xffff) / (i32::from(table[5]) - i32::from(table[4]))
        };
        Ok(contact)
    }

    /// FILD (Z, Y, X) of the velocity against the normal, then __ftol.
    fn velocity_along(&self, vehicle: ObjectId, normal: [f64; 3]) -> Result<i32, ObjectError> {
        let velocity = self
            .actor(vehicle)?
            .words_8e_92
            .map(|word| f64::from(word as i16));
        Ok(ftol(
            (velocity[2] * normal[2] + velocity[1] * normal[1]) + velocity[0] * normal[0],
        ))
    }

    /// 0041F7E0(power, step, table, settle): remove local X (and with
    /// `settle` local Z) velocity up to the table's limit; returns the X
    /// correction, the skid amount.
    pub(super) fn vehicle_grip<E>(
        &mut self,
        vehicle: ObjectId,
        power: i32,
        step: u32,
        table_address: u32,
        settle: bool,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<i32, VehicleDriveError<E>> {
        let threshold_word = read_dword(request.executable, table_address)?;
        let rate = read_dword(request.executable, table_address + 4)? as i32;
        let basis = self.actor(vehicle)?.rotating.basis;
        let velocity = self.actor(vehicle)?.words_8e_92;
        let local = basis.inverse_words(velocity.map(|word| word as i16));
        let mut correction: i32 = 0;
        let engage = local[0] != 0 || (settle && local[2] != 0);
        if engage {
            let limit = ((threshold_word >> 5) as i32).wrapping_mul(power) >> 16;
            let clamp = |value: i32| -> i32 {
                let bound =
                    ((rate.wrapping_mul(step as i32) >> 15) as i16 as i32).wrapping_mul(power);
                if value < limit.wrapping_neg() {
                    bound.wrapping_neg() >> 16
                } else if value > limit {
                    bound >> 16
                } else {
                    value
                }
            };
            let mut kept_x = local[0];
            if local[0] != 0 {
                correction = clamp(i32::from(local[0]));
                kept_x = local[0].wrapping_sub(correction as i16);
            }
            let mut kept_z = local[2];
            if settle {
                let removed = clamp(i32::from(local[2]));
                kept_z = local[2].wrapping_sub(removed as i16);
            }
            let removed = [
                local[0].wrapping_sub(kept_x),
                0,
                local[2].wrapping_sub(kept_z),
            ];
            let world =
                self.transform_visual_vector_words(vehicle, removed.map(|word| word as u16))?;
            let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
            for (word, removed) in velocity.iter_mut().zip(world) {
                *word = word.wrapping_sub(removed);
            }
        }
        let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
        for axis in [0, 2] {
            let value = velocity[axis] as i16;
            if settle {
                if value > -5 && value < 5 {
                    velocity[axis] = 0;
                }
            } else if value == -1 || value == 1 {
                velocity[axis] = 0;
            }
        }
        Ok(correction)
    }

    /// 00419F54..0041A076: the skid loop +548 and the intensity +228.
    fn vehicle_skid_sound<H: VehicleDriveHost>(
        &mut self,
        vehicle: ObjectId,
        slip: i32,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let constant = |address| read_dword(request.executable, address).map(|word| word as i32);
        let state = self.selector2_actor(vehicle)?;
        let magnitude = |value: i32| if value != 0 { value.wrapping_abs() } else { 1 };
        let quotient = |value: i32, divisor: i32| -> Result<i32, VehicleDriveError<H::Error>> {
            if divisor == 0 {
                return Err(VehicleDriveError::Division {
                    address: 0x0041_9f69,
                });
            }
            Ok(value.wrapping_div(divisor))
        };
        let change = state
            .drive
            .dword_578
            .ok_or(VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x578,
                address: 0x0041_9f71,
            })? as i32;
        let speed = state
            .drive
            .dword_574
            .ok_or(VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x574,
                address: 0x0041_9f90,
            })? as i32;
        let total = quotient(magnitude(slip), constant(0x0053_0124)?)?
            .wrapping_add(quotient(magnitude(change), constant(0x0053_0128)?)?)
            .wrapping_add(quotient(magnitude(speed), constant(0x0053_012c)?)?);
        let pitch = (constant(0x0053_0134)?.wrapping_shl(16) >> 4) as u32;
        let intensity = state.dword_228.unwrap_or(0) as i32;
        let next = if total > constant(0x0053_0120)? {
            let level = ((constant(0x0053_0130)?.wrapping_shl(16) >> 4)
                .wrapping_add(state.dwords_538_554[6] as i32))
                >> 1;
            self.selector2_actor_mut(vehicle)?.dwords_538_554[6] = level as u32;
            match self.selector2_actor(vehicle)?.drive.sound_548 {
                None => {
                    let reference = host
                        .create_sound(self, vehicle, 0x15, (level >> 1) as u32, pitch)
                        .map_err(VehicleDriveError::Sound)?;
                    self.selector2_actor_mut(vehicle)?.drive.sound_548 = Some(reference);
                }
                Some(reference) => host
                    .update_sound(self, vehicle, reference, level as u32, pitch)
                    .map_err(VehicleDriveError::Sound)?,
            }
            intensity.wrapping_shl(1)
        } else {
            let mut level = (state.dwords_538_554[6] as i32).wrapping_mul(3) >> 4;
            if level < 100 {
                level = 0;
            }
            self.selector2_actor_mut(vehicle)?.dwords_538_554[6] = level as u32;
            if level == 0 {
                if let Some(reference) = self.selector2_actor_mut(vehicle)?.drive.sound_548.take() {
                    host.release_sound(reference)
                        .map_err(VehicleDriveError::Sound)?;
                }
            }
            intensity / 2
        };
        self.selector2_actor_mut(vehicle)?.dword_228 = Some(next as u32);
        Ok(())
    }

    /// 0041A0D8..0041A1EA: the dust vector along the up row, pushed along
    /// the forward row while boosting or driving.
    fn vehicle_dust(
        &mut self,
        vehicle: ObjectId,
        intensity: i32,
        engaged: bool,
        thrust: i32,
        step: u32,
    ) -> Result<(), DerivedActorError> {
        let basis = self
            .actor(vehicle)
            .map_err(|_| DerivedActorError::NotSelector2(vehicle))?
            .rotating
            .basis;
        let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let base = (intensity / 10).wrapping_add(100);
        let rise = f64::from(base as i16);
        let mut words = [0u16; 6];
        for (axis, word) in words.iter_mut().take(3).enumerate() {
            *word = ftol(rise * m(4 + axis)) as u16;
        }
        let boosting = self.selector2_actor(vehicle)?.dwords_220_224[1] != 0;
        let push = if boosting || (engaged && thrust > 0) {
            Some(base / 4)
        } else if engaged {
            Some((base / 4).wrapping_neg())
        } else {
            None
        };
        if let Some(push) = push {
            let push = f64::from(push);
            for (axis, word) in words.iter_mut().take(3).enumerate() {
                *word = word.wrapping_add(ftol(push * m(7 + axis)) as u16);
            }
        }
        if (words[1] as i16) < 0 {
            words[1] = words[1].wrapping_neg();
        }
        // 0041A1A8: step / FFFFFFFEh, an unsigned divide that is zero here.
        let scale = (step / 0xffff_fffe) as i32;
        let velocity = self
            .actor(vehicle)
            .map_err(|_| DerivedActorError::NotSelector2(vehicle))?
            .words_8e_92;
        for axis in 0..3 {
            words[3 + axis] = (i16_of(velocity[axis]).wrapping_mul(scale) >> 15) as u16;
        }
        self.selector2_actor_mut(vehicle)?.drive.words_22c_236 = Some(words);
        Ok(())
    }

    /// 0041A1F1..0041A414: in water up to the hull's radius, speed above the
    /// mode's limit (0x60 in mode 1, else 0x30) bleeds off.
    fn vehicle_water_drag<E>(
        &mut self,
        vehicle: ObjectId,
        abs_speed: i32,
        step: u32,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<(), VehicleDriveError<E>> {
        let ground = request.ground;
        let mut limit: i32 = if self.selector2_actor(vehicle)?.dwords_1e8_1f4[0] == 1 {
            0x60
        } else {
            0x30
        };
        let position = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let terrain = ground.terrain.height_at([position[0], position[2]]);
        let plane = ground.terrain.plane_base() >> ground.terrain.plane_shift();
        let surface = plane_height_at(
            [position[0], position[2]],
            plane,
            terrain,
            ground.clock,
            true,
            ground.sine_quarter,
        );
        let depth = i16_of(position[1]).wrapping_sub(i32::from(surface));
        let radius = i32::from(self.object(vehicle)?.base.word_28);
        if depth >= radius {
            return Ok(());
        }
        if depth > 0 {
            limit = limit.wrapping_add(
                divide(
                    abs_speed.wrapping_sub(limit).wrapping_mul(depth),
                    radius,
                    0x0041_a3ab,
                )
                .map_err(|_| VehicleDriveError::Division {
                    address: 0x0041_a3ab,
                })?,
            );
        }
        if abs_speed <= limit {
            return Ok(());
        }
        let mut drag = abs_speed
            .wrapping_sub(limit)
            .wrapping_mul(abs_speed.wrapping_sub(limit))
            >> 7;
        if drag > 0xffff {
            drag = 0xffff;
        }
        let drag = (drag.wrapping_mul(step as i32) >> 15) as i16 as i32;
        let mut kept = abs_speed.wrapping_sub(drag).wrapping_sub(1);
        if kept < limit {
            kept = limit;
        }
        for axis in 0..3 {
            let value = i16_of(self.actor(vehicle)?.words_8e_92[axis]).wrapping_mul(kept);
            let next = divide(
                value,
                abs_speed,
                [0x0041_a3ea, 0x0041_a3fe, 0x0041_a412][axis],
            )
            .map_err(|_| VehicleDriveError::Division {
                address: 0x0041_a3ea,
            })?;
            self.actor_mut(vehicle)?.words_8e_92[axis] = next as u16;
        }
        Ok(())
    }

    /// 0041A519..0041A5EB: the engine load halves +55C/+560 and the smoothed
    /// pitch/volume +564/+568 that the vehicle update hands to its sound.
    fn vehicle_engine_load<E>(
        &mut self,
        vehicle: ObjectId,
        forward: i32,
        thrust: i32,
        pitch: i32,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<(), VehicleDriveError<E>> {
        let constant = |address| read_dword(request.executable, address).map(|word| word as i32);
        let state = self.selector2_actor(vehicle)?;
        let traction = state.drive.dword_57c.unwrap_or(0);
        let mut load: i32 = 0;
        if traction != 0 {
            load = forward.wrapping_abs();
        } else if thrust != 0 || pitch != 0 {
            load = load.wrapping_add(1000);
        }
        if state.dwords_220_224[1] != 0 {
            load = load.wrapping_add(1000);
        }
        let floor = constant(0x0053_0138)?;
        let k13c = constant(0x0053_013c)?;
        let k140 = constant(0x0053_0140)?;
        let k144 = constant(0x0053_0144)?;
        let k148 = constant(0x0053_0148)?;
        let state = self.selector2_actor_mut(vehicle)?;
        state.drive.dword_560 = Some((load >> 1) as u32);
        state.drive.dword_55c = Some((load >> 1) as u32);
        if load < floor {
            load = floor;
        }
        let share = 100 - floor.wrapping_mul(100) / load;
        let volume = (k148.wrapping_sub(k144).wrapping_mul(share) / 100).wrapping_add(k144);
        let pitch_target = (k140.wrapping_sub(k13c).wrapping_mul(share) / 100).wrapping_add(k13c);
        let current_564 = state.dword_564 as i32;
        let current_568 = state.dword_568 as i32;
        state.dword_564 =
            (pitch_target.wrapping_sub(current_564) >> 2).wrapping_add(current_564) as u32;
        state.dword_568 = (volume.wrapping_sub(current_568) >> 2).wrapping_add(current_568) as u32;
        Ok(())
    }
}

/// 0041A438..0041A47E: each axis moves by `velocity * step`, divided by
/// 0x8000 toward zero (CDQ/AND 7FFF/SAR 15). Retail drops the fraction;
/// [`VehiclePositionStep::CarryRemainder`] adds `remainder` first and keeps
/// what the division drops, with the sign of its axis's step.
fn step_vehicle_position(
    position: &mut [u16; 3],
    velocity: [u16; 3],
    step: u32,
    policy: VehiclePositionStep,
    remainder: &mut [i32; 3],
) {
    for axis in 0..3 {
        let mut product = i16_of(velocity[axis]).wrapping_mul(step as i32);
        if policy == VehiclePositionStep::CarryRemainder {
            product = product.wrapping_add(remainder[axis]);
            remainder[axis] = product % 0x8000;
        }
        position[axis] = position[axis].wrapping_add((product / 0x8000) as u16);
    }
}

/// 0041F3F0's out structure: WORD +0 grip, DWORD +4 support, DWORD +8 traction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct VehicleContact {
    grip: u16,
    pub(super) support: i32,
    pub(super) traction: i32,
    /// The return value: the clearance ratio, raised to the table's floor
    /// when the hull was pushed out of the ground.
    pub(super) ratio: i32,
}

impl PlayerControlBlock {
    /// Whether a pad in analogue mode drives forward this tick, as the
    /// drive reads it (00418F60's throttle above zero).
    pub(crate) fn analog_forward(&self) -> bool {
        self.analog_04 != 0 && analog_throttle(self) > 0
    }
}

/// 00418F60: the analog axes +C and +8 (player +10/+0C) past their dead
/// zones, scaled and summed; a reverse reading is dead-zoned again.
pub(super) fn analog_throttle(controls: &PlayerControlBlock) -> i32 {
    let dead = |value: i32, zone: i32| {
        if value > zone {
            value - zone
        } else if value < -zone {
            value + zone
        } else {
            0
        }
    };
    let forward = dead(controls.axes_08[2], 0x1c20);
    let lift = dead(controls.axes_08[1], 0x2a30);
    if forward == 0 && lift == 0 {
        return 0;
    }
    let total = lift.wrapping_shl(13) / 0x1aba + forward.wrapping_shl(13) / 0x1c7c;
    if total > 0xffff {
        return 0xffff;
    }
    if total >= 0 {
        return total;
    }
    let reverse = dead(total, 0x4000).wrapping_shl(13) / 0x1800;
    reverse.max(-0xffff)
}

pub(super) fn read_dword<E>(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<u32, VehicleDriveError<E>> {
    executable
        .word(address)
        .map_err(|error| VehicleDriveError::Executable {
            address,
            reason: error.to_string(),
        })
}

/// The +194 table: six signed WORDs.
fn suspension_table<E>(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<[i16; 6], VehicleDriveError<E>> {
    let mut words = [0i16; 6];
    for pair in 0..3 {
        let dword = read_dword(executable, address + pair * 4)?;
        words[pair as usize * 2] = dword as u16 as i16;
        words[pair as usize * 2 + 1] = (dword >> 16) as u16 as i16;
    }
    Ok(words)
}

impl ObjectArena {
    /// This machine's place in a race (`0054ED70` in network mode 3): its
    /// record's `+01`, signed.
    pub(super) fn race_rank<E>(
        &self,
        request: &VehicleDriveRequest<'_>,
    ) -> Result<Option<i32>, VehicleDriveError<E>> {
        let (Some(network), Some(race)) = (request.network, self.race()) else {
            return Ok(None);
        };
        if network.mode != 3 {
            return Ok(None);
        }
        let record = self
            .race_state(race)?
            .records
            .get(usize::from(network.local_slot))
            .ok_or(VehicleDriveError::Unported {
                vehicle: race,
                address: 0x0041_9cf0,
            })?;
        Ok(Some(i32::from(record.rank_01 as i8)))
    }
}

pub(super) fn forward_table<E>(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<ActorForwardMotionTable, VehicleDriveError<E>> {
    Ok(ActorForwardMotionTable {
        maximum: read_dword(executable, address)? as i32,
        minimum: read_dword(executable, address + 4)? as i32,
        acceleration: read_dword(executable, address + 8)? as i32,
        deceleration: read_dword(executable, address + 12)? as i32,
    })
}

pub(super) fn angular_table<E>(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<ActorAngularMotionTable, VehicleDriveError<E>> {
    Ok(ActorAngularMotionTable {
        maximum: read_dword(executable, address)? as i32,
        response: read_dword(executable, address + 4)? as i32,
    })
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;

/// 00423200(step, controls), with the analog axes live: the stick's X past
/// a 6000 dead zone turns the vehicle, by a curve of half the excess (capped
/// at 0x6C48) scaled to 40960, plus the hold +248, which grows by step / 32
/// to 500 while the excess is past 0x56A0 and is zero otherwise. The rate
/// +220 takes it with the stick's sign and stays within +-0x14000. Returns
/// the rate and the hold.
pub(crate) fn analog_turn(step: u32, x: i32, hold: u32, rate: i32) -> (i32, u32) {
    let magnitude = if x > 0 { x } else { x.wrapping_neg() };
    let mut hold = hold as i32;
    let mut rate = rate;
    if magnitude < 6000 {
        hold = 0;
    } else {
        let excess = (magnitude - 6000) / 2;
        let excess = if excess > 0x6c48 { 0x6c48 } else { excess };
        if excess > 0x56a0 {
            hold = hold.wrapping_add((step >> 5) as i32).min(500);
        } else {
            hold = 0;
        }
        let curve = if excess > 0x4830 {
            let over = excess - 0x4830;
            over.wrapping_mul(over).wrapping_mul(6)
        } else {
            0
        };
        // 004232B7: at most 768398400 (0x2DCCD440), the cap's own value.
        let total = (excess * 27720) / 3 + curve;
        let mut divisor = 0x2dcc_d440;
        let mut numerator = total;
        while numerator >= 0xcccc {
            numerator >>= 1;
            divisor >>= 1;
        }
        let turn = (numerator * 40960) / divisor + hold.wrapping_mul(40960) / 500;
        rate = if x > 0 {
            rate.wrapping_add(turn)
        } else {
            rate.wrapping_sub(turn)
        };
    }
    (rate.clamp(-0x14000, 0x14000), hold as u32)
}

#[cfg(test)]
#[path = "drive_frame_rate_tests.rs"]
mod frame_rate_tests;
