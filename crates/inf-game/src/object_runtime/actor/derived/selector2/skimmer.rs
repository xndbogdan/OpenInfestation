//! Selector 2's skimmer, vehicle mode 5: the entry and exit 00422380 the
//! mode switch calls, and the drive 00422400 with its surface push 00422ED0.
//! The skimmer rides the suspension 0041F3F0 on its own table, sprays a
//! wake from its clearance, leans its normal toward up (and forward when
//! deep), pitches and rolls by rates of its own (+258, +260) that the
//! normal's slope feeds, yaws at +25C like the VTOL, and is pushed along
//! the surface by a drive whose drag depends on the gear. Infest03 reaches
//! the drive on 16 updates, on world 11.
//!
//! While `0054ED70` holds the race the boost's speed and the drive's
//! budget follow this machine's place by the catch-up scale (settings
//! `+4C`), and the drive's share `+590` eases toward its place.

use inf_formats::native_basis::{normalize_words, NativeLengthPrecision};

use super::super::kinematics::{divide, ftol};
use super::super::physics::ActorPhysicsRequest;
use super::drive::{analog_throttle, VehicleDriveError, VehicleDriveRequest};
use super::update::{VehicleUpdateHost, WakeParticle};
use crate::object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId, PlayerControlBlock};

/// 005205B0: the forward row's Y in 1/4096 units.
const PITCH_SCALE: f64 = 4096.0;
/// 00520A78/00520A74: the slope's pitch and roll torques.
const PITCH_TORQUE: f64 = 24576.0;
const ROLL_TORQUE: f64 = -24576.0;
/// 0042243D: the boost's forward speed without a selector-70 actor.
const BOOST_SPEED: i32 = 0x1194;

impl ObjectArena {
    /// 00422380(enter) with the world camera 00478E00 returned. Entering
    /// sets the skimmer's suspension table, byte +160, clears +24C and
    /// +250..+260 and the traction, and puts the camera in mode 1
    /// (00410EE0). Leaving lets the wake ribbon +264 go (00486B20: its
    /// owner the null `null`), which only a skimmer another machine drove
    /// made (00421189).
    pub fn switch_vehicle_skimmer<E>(
        &mut self,
        vehicle: ObjectId,
        enter: bool,
        camera: Option<ObjectId>,
        null: ActorHandle,
    ) -> Result<(), VehicleDriveError<E>> {
        if !enter {
            if let Some(ribbon) = self.selector2_actor_mut(vehicle)?.ribbon_264.take() {
                self.set_selector67_owner(ribbon, null)?;
            }
            return Ok(());
        }
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.words_188_194[3] = 0x0053_05d8;
        }
        self.derived_actor_mut(vehicle)?.byte_160 = 1;
        {
            let state = self.selector2_actor_mut(vehicle)?;
            state.words_24c_24e[0] = 0;
            state.dwords_250_25c = [0; 4];
            state.dword_260 = Some(0);
        }
        // 004223D5 calls 00410EE0 on the camera unchecked.
        let camera = camera.ok_or(VehicleDriveError::Unported {
            vehicle,
            address: 0x0042_23d5,
        })?;
        self.set_camera_parameter_mode(camera, 1)?;
        self.selector2_actor_mut(vehicle)?.drive.dword_57c = Some(0);
        Ok(())
    }

    /// 00422400 for an offline skimmer. `controls` is the driving player's
    /// block, which the drive only reads.
    pub fn tick_vehicle_skimmer<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        controls: &PlayerControlBlock,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let delta = request.delta;
        let step = delta >> 5;
        let steps = step as i32;
        let unknown = |offset, address| VehicleDriveError::UnknownField {
            vehicle,
            offset,
            address,
        };

        // 0042241B: the catch-up scale, times ten.
        let catchup = request
            .network
            .map_or(0, |network| network.catchup)
            .wrapping_mul(10);
        let race =
            match self.race() {
                Some(race) => {
                    let slot = request.network.map_or(1, |network| network.local_slot);
                    let state = self.race_state(race)?;
                    let record = state.records.get(usize::from(slot)).ok_or(
                        VehicleDriveError::Unported {
                            vehicle,
                            address: 0x0042_2451,
                        },
                    )?;
                    Some((i32::from(record.rank_01 as i8), state.countdown_2d4))
                }
                None => None,
            };
        // 0042242B..00422542: while the boost +5AC lasts, push the forward
        // speed toward 0x1194 along the forward row; in a race less for
        // the places ahead.
        let boost = self.selector2_actor(vehicle)?.dwords_5ac_5b0[0];
        if (boost as i32) >= delta as i32 {
            let mut speed = BOOST_SPEED;
            if let Some((rank, _)) = race {
                let behind = 15 - (rank * 2 - 2).clamp(0, 15);
                // 00422487: the division by 11250, as a multiply.
                speed = speed.wrapping_sub(behind.wrapping_mul(catchup).wrapping_mul(4500) / 11250);
            }
            let basis = self.actor(vehicle)?.rotating.basis;
            let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
            let velocity = self
                .actor(vehicle)?
                .words_8e_92
                .map(|word| f64::from(word as i16));
            let forward = ftol((velocity[2] * m(9) + velocity[1] * m(8)) + velocity[0] * m(7));
            let push = speed.wrapping_sub(forward);
            if push > 0 {
                let push = f64::from(push as i16);
                let impulse = [7, 8, 9].map(|index| ftol(push * m(index)) as u16);
                self.add_actor_linear_velocity(vehicle, impulse)?;
            }
            self.selector2_actor_mut(vehicle)?.dwords_5ac_5b0[0] = boost.wrapping_sub(delta);
        } else {
            self.selector2_actor_mut(vehicle)?.dwords_5ac_5b0[0] = 0;
        }

        // 00422542..00422574: the turn, from the keys or the stick past 2500.
        let mut turn = controls.turn_14;
        if controls.analog_04 != 0 && turn == 0 {
            let x = controls.axes_08[0];
            if x.wrapping_abs() > 2500 {
                turn = x.wrapping_mul(steps).wrapping_neg() >> 11;
            }
        }
        // 00422574..004225BB: the power, and its gear (0 ahead, 1 none,
        // 2 astern).
        let power = if controls.thrust_1c != 0 {
            controls.thrust_1c
        } else if controls.pitch_18 != 0 {
            controls.pitch_18.wrapping_neg()
        } else if controls.analog_04 != 0 {
            analog_throttle(controls).wrapping_mul(steps) >> 16
        } else {
            0
        };
        let gear: i32 = if power == 0 {
            1
        } else if power > 0 {
            0
        } else {
            2
        };
        // 004225BB..004226B2: the drive's budget; in a race the share +590
        // (held at 0x300000 while the race waits) eases toward the place's
        // and scales it, then the health does.
        let budget = match race {
            None => power.wrapping_add((delta as i32).wrapping_mul(2)),
            Some((rank, countdown)) => {
                let state = self.selector2_actor_mut(vehicle)?;
                // 004225DD counts the running race's ticks in 00546A3C,
                // which nothing reads.
                if countdown != 0 {
                    state.dword_590 = 0x30_0000;
                }
                let target = (rank * 2 - 2).clamp(0, 9) << 18;
                let share = state.dword_590 as i32;
                state.dword_590 = if share < target {
                    share.wrapping_add(delta as i32).min(target)
                } else if share > target {
                    share.wrapping_sub(delta as i32).max(target)
                } else {
                    share
                } as u32;
                let factor = ((state.dword_590 as i32 >> 10).wrapping_mul(catchup) >> 8)
                    .wrapping_add((256 - catchup).wrapping_shl(11) >> 8);
                let power = if power > 0 {
                    power.wrapping_mul(factor) / 2048
                } else {
                    power
                };
                let actor = self.actor(vehicle)?;
                let most = actor
                    .maximum_health
                    .ok_or(VehicleDriveError::UnknownField {
                        vehicle,
                        offset: 0xa2,
                        address: 0x0042_268f,
                    })?;
                let total = ((factor.wrapping_mul(delta as i32) as u32 >> 11) as i32)
                    .wrapping_add(power)
                    .wrapping_mul(i32::from(actor.health));
                if most == 0 {
                    return Err(VehicleDriveError::Division {
                        address: 0x0042_2697,
                    });
                }
                (total / i32::from(most)).wrapping_add(delta as i32)
            }
        };

        // 004226B4..004226DD: 0041F3F0 on the +194 table; its clearance
        // ratio less 0x555 is the depth.
        let table = self.selector2_actor(vehicle)?.words_188_194[3];
        let contact = self.vehicle_suspension(vehicle, table, step, request)?;
        let depth = contact.ratio.wrapping_sub(0x555);
        let lift;
        if depth < 0x3000 {
            let depth = depth.max(0);
            self.skimmer_wake(vehicle, depth, request, host)?;
            if depth < 0x1000 {
                lift = if depth > 0 {
                    0x3000 - divide_power(depth.wrapping_shl(13), 12)
                } else {
                    0x3000
                };
            } else {
                // 00422906..004229BA: lean the normal toward up.
                lift = 0x1000;
                let words = self.skimmer_normal_words(vehicle, contact.support)?;
                self.store_skimmer_normal(
                    vehicle,
                    [words[0], words[1].wrapping_add(0x400), words[2]],
                )?;
            }
        } else {
            // 004229DC..00422B0B: deep, lean it toward up and forward.
            lift = 0x1000;
            let words = self.skimmer_normal_words(vehicle, contact.support)?;
            let excess = depth.wrapping_sub(0x3000);
            let forward = if excess < 0x8000 {
                divide_power(excess.wrapping_shl(6), 12)
            } else {
                0x400
            };
            let basis = self.actor(vehicle)?.rotating.basis;
            let along = |index: usize| {
                ftol(f64::from(forward) * f64::from(f32::from_bits(basis.words[index]))) as i16
            };
            self.store_skimmer_normal(
                vehicle,
                [
                    words[0].wrapping_add(along(7)),
                    words[1].wrapping_add(0x400),
                    words[2].wrapping_add(along(9)),
                ],
            )?;
        }

        // 00422B0D..00422B9E: the yaw rate +25C and its accumulator +254,
        // as the VTOL's.
        let state = self.selector2_actor_mut(vehicle)?;
        if turn != 0 {
            let rate = divide(turn.wrapping_mul(682), steps, 0x0042_2b25)?;
            let accumulated = state.dwords_250_25c[1] as i32;
            state.dwords_250_25c[3] = rate as u32;
            if accumulated < 0x186a {
                let accumulated = (turn.wrapping_abs() >> 5).wrapping_add(accumulated);
                state.dwords_250_25c[1] = accumulated as u32;
                if accumulated < 0x186a {
                    state.dwords_250_25c[3] = divide_186a(accumulated.wrapping_mul(rate)) as u32;
                }
            }
        } else {
            let rate = state.dwords_250_25c[3] as i32;
            state.dwords_250_25c[1] = 0;
            state.dwords_250_25c[3] = rate.wrapping_sub(rate.wrapping_mul(steps) >> 12) as u32;
        }

        // 00422B9E..00422CE5: yaw about the leaned normal, scaled by the
        // traction: 0047A790 turns about the world axis -yaw * U, where U is
        // the normal times the support with 0x1000 added to its Y.
        let traction = contact.traction;
        let scale = divide_3(traction.wrapping_mul(2).wrapping_add(0x10000));
        let yaw_rate = self.selector2_actor(vehicle)?.dwords_250_25c[3] as i32;
        let yaw = ((scale.wrapping_mul(yaw_rate) >> 16).wrapping_mul(steps) >> 15) as i16 as i32;
        let words = self.skimmer_normal_words(vehicle, contact.support)?;
        let [x, y, z] = [words[0], words[1].wrapping_add(0x1000), words[2]].map(f64::from);
        let length = ((z * z + y * y) + x * x).sqrt();
        let (unit_x, unit_y, unit_z) = if length == 0.0 {
            (1.0, 0.0_f32, 0.0_f32)
        } else {
            let divisor = f64::from(length as f32);
            (x / divisor, (y / divisor) as f32, (z / divisor) as f32)
        };
        let amount = f64::from((yaw.wrapping_neg()) as i16);
        let axis = [
            ftol(amount * unit_x) as u16,
            ftol(amount * f64::from(unit_y)) as u16,
            ftol(amount * f64::from(unit_z)) as u16,
        ];
        let mut basis = self.actor(vehicle)?.rotating.basis;
        crate::object_runtime::actor::selector16::rotate_world_vector(
            &mut basis,
            axis,
            request.ground.sine_quarter,
        );
        self.actor_mut(vehicle)?.rotating.basis = basis;

        // 00422CEA..00422DF3: the normal's slope along the forward and side
        // rows feeds the pitch rate +258 and the roll rate +260, each
        // decaying by its own step; the drive is the budget times the
        // traction, less the pitch's share.
        let normal = self
            .skimmer_normal(vehicle)
            .map_err(|_| unknown(0x198, 0x0042_2cf1))?;
        let [nx, ny, nz] = normal.map(f64::from);
        let m = |index: usize| f64::from(f32::from_bits(basis.words[index]));
        let pitch_slope = ((nz * m(9) + ny * m(8)) + m(7) * nx) as f32;
        let roll_slope = (nz * m(3) + ny * m(2)) + nx * m(1);
        let share = ((budget >> 10).wrapping_mul(traction) as u32 >> 16) as i32;
        let pull =
            (ftol(f64::from(share as i16) * f64::from(pitch_slope)) as i16 as i32).wrapping_abs();
        let drive = share.wrapping_sub(pull);
        let state = self.selector2_actor_mut(vehicle)?;
        let mut pitch_rate = state.dwords_250_25c[2] as i32;
        pitch_rate =
            pitch_rate.wrapping_sub(((pitch_rate.wrapping_mul(steps)) >> 15) as i16 as i32);
        let torque = ftol(f64::from(pitch_slope) * PITCH_TORQUE) as i16 as i32;
        let torque = (torque.wrapping_mul(steps) >> 15) as i16 as i32;
        let torque = torque.wrapping_mul(lift) >> 12;
        pitch_rate = pitch_rate.wrapping_add(reinforce(pitch_rate, torque, traction));
        state.dwords_250_25c[2] = pitch_rate as u32;
        let mut roll_rate = state.dword_260.ok_or(unknown(0x260, 0x0042_2dab))? as i32;
        roll_rate = roll_rate.wrapping_sub(((roll_rate.wrapping_mul(steps)) >> 15) as i16 as i32);
        let torque = ftol(roll_slope * ROLL_TORQUE) as i16 as i32;
        let torque = (torque.wrapping_mul(steps) >> 15) as i16 as i32;
        roll_rate = roll_rate.wrapping_add(reinforce(roll_rate, torque, traction));
        state.dword_260 = Some(roll_rate as u32);

        // 00422DF9..00422E6F: 0047A760 turns by the local vector (pitch, 0,
        // roll): the yaw, less its share along each slope, feeds the roll.
        let yaw16 = yaw as i16;
        let mut along = ftol(f64::from(yaw16) * f64::from(pitch_slope)) as i16 as i32;
        if (along ^ yaw) < 0 {
            along = along.wrapping_neg();
        }
        let bank = yaw.wrapping_sub(along);
        let mut side = ftol(f64::from(bank as i16) * roll_slope) as i16 as i32;
        if (side ^ bank) < 0 {
            side = side.wrapping_neg();
        }
        let roll = (roll_rate.wrapping_mul(steps) >> 15)
            .wrapping_sub(side)
            .wrapping_add(bank);
        let pitch = pitch_rate.wrapping_mul(steps) >> 15;
        let local = [pitch as u16, 0, roll as u16];
        let world = basis.transform_words(local.map(|word| word as i16));
        crate::object_runtime::actor::selector16::rotate_world_vector(
            &mut basis,
            world,
            request.ground.sine_quarter,
        );
        self.actor_mut(vehicle)?.rotating.basis = basis;

        // 00422E86: 00422ED0, then 0047B1E0 with every term.
        self.skimmer_push(vehicle, delta, drive, traction, gear)?;
        self.tick_actor_physics(
            vehicle,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: 0x1f,
                sine_quarter: request.ground.sine_quarter,
            },
        )?;
        // 00422E95: a ribbon left from another machine's drive gains where
        // the skimmer is (004869B0).
        if let Some(ribbon) = self.selector2_actor(vehicle)?.ribbon_264 {
            let position = self.position(vehicle)?;
            host.extend_ribbon(self, ribbon, delta, position)
                .map_err(VehicleDriveError::Host)?;
        }
        // 00422EA9..00422EC0: the engine load from the drive and the gear.
        let state = self.selector2_actor_mut(vehicle)?;
        state.drive.dword_55c =
            Some(drive.wrapping_shl(8).wrapping_sub(gear.wrapping_shl(13)) as u32);
        state.drive.dword_560 =
            Some(drive.wrapping_shl(8).wrapping_sub(gear.wrapping_shl(11)) as u32);
        Ok(())
    }

    /// 004226EF..004228B5: a wake of +228 = rand * (0x3000 - depth) / 4096,
    /// one particle per 0x2000 of it, scattered over a spread that grows
    /// with the depth: a splash on water (00489600) or a sprite thrown up
    /// on ground (004894A0, with its shadow).
    fn skimmer_wake<H: VehicleUpdateHost>(
        &mut self,
        vehicle: ObjectId,
        depth: i32,
        request: &VehicleDriveRequest<'_>,
        host: &mut H,
    ) -> Result<(), VehicleDriveError<H::Error>> {
        let random = |host: &mut H| -> Result<i32, VehicleDriveError<H::Error>> {
            Ok((host.random().map_err(VehicleDriveError::Host)? & 0xffff) as i32)
        };
        let wake = divide_power(random(host)?.wrapping_mul(0x3000 - depth), 12);
        self.selector2_actor_mut(vehicle)?.dword_228 = Some(wake as u32);
        let radius = i32::from(self.object(vehicle)?.base.word_28);
        let spread = divide_power(radius.wrapping_mul(depth.wrapping_add(0x1000)), 13);
        let half = divide_power(spread, 1);
        let terrain = request.ground.terrain;
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
        let jitter = |host: &mut H| -> Result<u16, VehicleDriveError<H::Error>> {
            Ok(half.wrapping_sub(random(host)?.wrapping_mul(spread) >> 16) as u16)
        };
        loop {
            let remaining = self
                .selector2_actor(vehicle)?
                .dword_228
                .expect("written above") as i32;
            if remaining <= 0 {
                return Ok(());
            }
            self.selector2_actor_mut(vehicle)?.dword_228 =
                Some(remaining.wrapping_sub(0x2000) as u32);
            let position = self
                .object(vehicle)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
            let x = jitter(host)?.wrapping_add(position[0]);
            let z = jitter(host)?.wrapping_add(position[2]);
            let ground = terrain.height_at([x, z]);
            let velocity = self.actor(vehicle)?.words_8e_92;
            let vx = jitter(host)?.wrapping_add(velocity[0]);
            let vz = jitter(host)?.wrapping_add(velocity[2]);
            if plane > ground {
                host.wake_particle(WakeParticle::Splash, [x, plane as u16, z], [vx, 0, vz])
                    .map_err(VehicleDriveError::Host)?;
            } else {
                let vy = ((random(host)? >> 10) + 0x80) as u16;
                host.wake_particle(WakeParticle::Thrown, [x, ground as u16, z], [vx, vy, vz])
                    .map_err(VehicleDriveError::Host)?;
            }
        }
    }

    fn skimmer_normal(&self, vehicle: ObjectId) -> Result<[f32; 3], DerivedNormalError> {
        Ok(self
            .selector2_actor(vehicle)
            .map_err(|_| DerivedNormalError)?
            .drive
            .normal_198
            .ok_or(DerivedNormalError)?
            .map(f32::from_bits))
    }

    /// The +198 normal times the support's high bits, `(support >> 4)` as a
    /// signed WORD, each product truncated to a WORD.
    fn skimmer_normal_words<E>(
        &self,
        vehicle: ObjectId,
        support: i32,
    ) -> Result<[i16; 3], VehicleDriveError<E>> {
        let normal = self
            .skimmer_normal(vehicle)
            .map_err(|_| VehicleDriveError::UnknownField {
                vehicle,
                offset: 0x198,
                address: 0x0042_2922,
            })?;
        let scale = f64::from((support as u32 >> 4) as i16);
        Ok(normal.map(|component| ftol(scale * f64::from(component)) as i16))
    }

    /// The normalization the blends share: the length spilled to f32, and
    /// (1, 0, 0) for a zero vector.
    fn store_skimmer_normal(
        &mut self,
        vehicle: ObjectId,
        words: [i16; 3],
    ) -> Result<(), super::super::DerivedActorError> {
        let normal = normalize_words(words, NativeLengthPrecision::Spilled);
        self.selector2_actor_mut(vehicle)?.drive.normal_198 = Some(normal.map(f32::to_bits));
        Ok(())
    }

    /// 00422ED0(delta, drive, traction, gear): a positive drive pushes along
    /// the forward row laid onto the surface ((N x F) x N), losing push as
    /// the nose rises past 30 degrees and gaining it as it dips; then drag
    /// in the hull's frame: the grip turns sideways and vertical speed into
    /// forward speed, small speeds stop, and forward drag eases with the
    /// gear.
    fn skimmer_push<E>(
        &mut self,
        vehicle: ObjectId,
        delta: u32,
        drive: i32,
        traction: i32,
        gear: i32,
    ) -> Result<(), VehicleDriveError<E>> {
        let step = (delta >> 5) as i32;
        let grip = step.wrapping_mul(traction) >> 14;
        let basis = self.actor(vehicle)?.rotating.basis;
        if drive > 0 {
            let normal =
                self.skimmer_normal(vehicle)
                    .map_err(|_| VehicleDriveError::UnknownField {
                        vehicle,
                        offset: 0x198,
                        address: 0x0042_2efe,
                    })?;
            let [nx, ny, nz] = normal.map(f64::from);
            let f = |index: usize| f64::from(f32::from_bits(basis.words[index]));
            let t1 = f(8) * nx - f(7) * ny;
            let t2 = ny * f(9) - f(8) * nz;
            let t3 = f(7) * nz - nx * f(9);
            let surface = [
                (t3 * nz - t1 * ny) as f32,
                (t1 * nx - t2 * nz) as f32,
                (t2 * ny - t3 * nx) as f32,
            ];
            let pitch = ftol(f(8) * PITCH_SCALE);
            let mut drive = drive;
            if pitch > 0x800 {
                let excess = pitch - 0x800;
                drive = if excess >= 0x400 {
                    0
                } else {
                    drive.wrapping_sub(excess.wrapping_mul(drive) >> 10)
                };
            } else if pitch < 0 {
                drive = drive.wrapping_sub(pitch.wrapping_mul(drive) >> 12);
            }
            let push = f64::from(drive);
            let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
            for axis in 0..3 {
                velocity[axis] =
                    velocity[axis].wrapping_add(ftol(push * f64::from(surface[axis])) as u16);
            }
        }
        let basis = self.actor(vehicle)?.rotating.basis;
        let velocity = self.actor(vehicle)?.words_8e_92;
        let [lx, ly, lz] = basis.inverse_words(velocity.map(|word| word as i16));
        let mut side = lx;
        let mut up = ly;
        let mut ahead = lz;
        let gain = |ahead: i16, taken: i16| {
            let gained = (i32::from(taken) * 3) >> 2;
            if gained > 0 {
                ahead.wrapping_add(gained as i16)
            } else {
                ahead.wrapping_sub(gained as i16)
            }
        };
        if grip != 0 {
            let taken = ((i32::from(ly).wrapping_mul(grip)) >> 15) as i16;
            up = ly.wrapping_sub(taken);
            ahead = gain(ahead, taken);
            if grip > 16 {
                let taken = ((i32::from(lx).wrapping_mul(grip)) >> 15) as i16;
                let remaining = i32::from(lx.wrapping_sub(taken));
                ahead = gain(ahead, taken);
                let drag = (remaining.wrapping_mul(step) >> 14).wrapping_mul(grip) >> 16;
                side = settle(remaining, drag, 2) as i16;
            }
        }
        let up = i32::from(up);
        let up = settle(up, up.wrapping_mul(step) >> 18, 8) as i16;
        let forward = i32::from(ahead);
        let ahead = if ahead > 0 {
            settle(
                forward,
                forward.wrapping_mul(step) >> ((0x11 - gear) as u32 & 31),
                8,
            )
        } else {
            settle(forward, forward.wrapping_mul(step) >> 14, 0x10)
        } as i16;
        let drag = [
            lx.wrapping_sub(side),
            ly.wrapping_sub(up),
            lz.wrapping_sub(ahead),
        ];
        let world = basis.transform_words(drag);
        let velocity = &mut self.actor_mut(vehicle)?.words_8e_92;
        for axis in 0..3 {
            velocity[axis] = velocity[axis].wrapping_sub(world[axis]);
        }
        Ok(())
    }
}

/// The +198 normal is unwritten.
#[derive(Debug)]
struct DerivedNormalError;

/// A speed past `small` loses `drag` unless that would carry it to or past
/// zero; a speed within `small` stops.
fn settle(speed: i32, drag: i32, small: i32) -> i32 {
    if speed > small {
        if speed <= drag {
            0
        } else {
            speed - drag
        }
    } else if speed < -small && speed < drag {
        speed - drag
    } else {
        0
    }
}

/// A torque plus its traction share, doubled while it opposes the rate.
fn reinforce(rate: i32, torque: i32, traction: i32) -> i32 {
    let torque = torque.wrapping_add(traction.wrapping_mul(torque) >> 16);
    if (rate ^ torque) < 0 {
        torque.wrapping_shl(1)
    } else {
        torque
    }
}

/// CDQ, AND EDX with 2^n-1, ADD, SAR n: division by 2^n toward zero.
fn divide_power(value: i32, bits: u32) -> i32 {
    let bias = ((value >> 31) as u32 & ((1 << bits) - 1)) as i32;
    value.wrapping_add(bias) >> bits
}

/// 00422BA2: by 3 (0x55555556, plus one when negative).
fn divide_3(value: i32) -> i32 {
    let high = ((i64::from(value) * 0x5555_5556) >> 32) as i32;
    high.wrapping_add((high as u32 >> 31) as i32)
}

/// 00422B65: by 6250 (0x14F8B589, shift 9, plus one when negative).
fn divide_186a(value: i32) -> i32 {
    let high = (((i64::from(value) * 0x14f8_b589) >> 32) as i32) >> 9;
    high.wrapping_add((high as u32 >> 31) as i32)
}

#[cfg(test)]
#[path = "skimmer_tests.rs"]
mod tests;
