//! Selector11 virtual +24, 004019E0, with proximity damage 00401920 and
//! retarget 004015F0. The three constant stack vectors built at
//! 004019EB..00401AE0 are overwritten by the basis copy at 00401FAF before
//! any read, so they have no observable effect.

use inf_formats::native_basis::{
    inverse_sine_q31, isqrt_48efc0, normalize_words, q31_divide, NativeBasisMath,
    NativeLengthPrecision, RotationPlane,
};
use thiserror::Error;

use super::Selector11State;
use crate::object_runtime::actor::behavior::read_row_word;
use crate::object_runtime::{
    ActorTimedDamageRequest, ActorWaterPlane, BehaviorCacheStorageError, BehaviorRowSelection,
    DerivedActorClass, DerivedActorError, FamilyUpdateEffect, FamilyUpdateError, FamilyUpdateHost,
    NativeTargetQueryError, ObjectArena, ObjectError, ObjectId, ProcessActorBehaviorTables,
};
use crate::process_tasks::{ProcessTaskRuntime, TaskStorageError};

mod proximity;
mod retarget;
pub use retarget::Selector11ScanHost;

#[derive(Clone, Copy, Debug)]
pub struct Selector11TickRequest<'a> {
    /// The raw delta. Native reads it signed in the timer comparisons and
    /// unsigned in its shifts.
    pub microseconds: u32,
    /// Process 0055F154, read by a reached 0047C990/0047DFD0 visual.
    pub status_load_factor: Option<i32>,
    /// The resident H12 water plane the 00401AF0 check compares.
    pub water_plane: ActorWaterPlane,
    pub sine_quarter: &'a [u16; 4096],
    pub inverse_sine: &'a [i16; 1024],
    /// Process word read by the target refresh, virtual +18.
    pub settings_7c: u32,
    /// Process 0055F0F8, reached only by a fixed/object target's +08.
    pub process_velocity: Option<[u16; 3]>,
}

pub trait Selector11UpdateHost: FamilyUpdateHost + Selector11ScanHost {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables>;
    /// Process 0055F100, read by the firing gate and filter A.
    fn tasks(&self) -> Option<&ProcessTaskRuntime>;
    /// 00401920's 0047C7C0 on the first player's primary actor.
    fn player_damage(
        &mut self,
        arena: &mut ObjectArena,
        player: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error>;
    /// 0047DD80 within range: the player actor's virtual +98 with this
    /// actor, the squared distance, 0x17 and 0.
    fn player_notice(
        &mut self,
        arena: &mut ObjectArena,
        player: ObjectId,
        source: ObjectId,
        distance_squared: u32,
    ) -> Result<(), Self::Error>;
    /// Virtual +4C, 00402530, with a null out-slot and the given position:
    /// 0047AAF0 alone, as its turn of the child by the yaw +18C and pitch
    /// +188 needs the out-slot.
    fn create_child(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// 0047A110 for the stored wreck model.
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// 0047C840 on this actor with a found target's handle as the source.
    fn retaliation(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: crate::object_runtime::ActorHandle,
    ) -> Result<(), Self::Error>;
    /// 0048D0D0(this, delta, &direction, 0).
    fn queue_shot(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        direction_bits: [u32; 3],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector11UpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error(transparent)]
    TargetQuery(#[from] NativeTargetQueryError),
    #[error(transparent)]
    Task(#[from] TaskStorageError),
    #[error(transparent)]
    Position(#[from] crate::object_runtime::ActorBehaviorPositionError),
    #[error(transparent)]
    Relation(#[from] crate::process_tasks::relation::TaskRelationError<ObjectError>),
    #[error(transparent)]
    InverseSine(#[from] inf_formats::native_basis::InverseSineQ31Error),
    #[error(transparent)]
    Status(FamilyUpdateError<E>),
    #[error("selector11 {actor:?} requires native +{offset:03X} backing")]
    RequiredField { actor: ObjectId, offset: u16 },
    #[error("selector11 requires live process behavior tables")]
    RequiredBehaviorTables,
    #[error("selector11 requires live process task storage 0055F100")]
    RequiredTasks,
    #[error("selector11 {actor:?} reads task 0055F100[{team}] that is absent")]
    MissingTask { actor: ObjectId, team: u32 },
    #[error("selector11 {actor:?} reads weapon entry {index} outside {entries}")]
    WeaponEntry {
        actor: ObjectId,
        index: i32,
        entries: usize,
    },
    #[error("selector11 {actor:?} divides by zero at {address:08X}")]
    Division { actor: ObjectId, address: u32 },
    #[error("selector11 host failed at {address:08X}: {error}")]
    Host { address: u32, error: E },
    #[error("selector11 host failed at {effect:?}: {error}")]
    Effect {
        effect: FamilyUpdateEffect,
        error: E,
    },
    #[error("selector11's 0047EBF0: {0}")]
    Remote(crate::object_runtime::actor::common_update::RemoteActorError<E>),
}

/// Selector11WobbleAngleApproximation: the wobble angles +1AC/+1AA a
/// turret's first wobble starts from. Retail reads allocator residue there;
/// in Infest04 the one turret a stun reached (Oolbryte) held 0 in both when
/// each stun arrived. No recording shows a live turret's wobble. Before this
/// the read stopped the turret's update for good, so a stunned turret froze
/// and could no longer die.
pub const SELECTOR11_WOBBLE_ANGLE_APPROXIMATION: u16 = 0;

fn log_wobble_angle_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("Selector11WobbleAngleApproximation: no writer sets a turret's wobble angles +1AC/+1AA before its first wobble; the port starts them at 0, as Infest04's stunned turret held them (ACTOR_RUNTIME.md)");
    }
}

impl ObjectArena {
    /// 004019E0. Every reached write stays when a later service fails.
    /// `00402630`, the `+3C` of a turret another machine owns: its
    /// proximity damage (`00401920`), then the shared `0047EBF0`.
    pub fn tick_remote_selector11<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>>
    where
        H::Error: std::fmt::Display,
    {
        self.selector11_actor(actor)?;
        self.selector11_proximity(actor, delta, host)?;
        self.tick_remote_actor(
            actor,
            delta,
            sine_quarter,
            load_factor,
            &mut crate::object_runtime::actor::common_update::FamilyRemoteHost(host),
        )
        .map_err(Selector11UpdateError::Remote)
    }

    pub fn tick_selector11<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: &Selector11TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        self.selector11_actor(actor)?;
        let delta = request.microseconds;
        self.tick_actor_status(actor, delta, request.status_load_factor, host)
            .map_err(Selector11UpdateError::Status)?;
        // 00401AF6..1B: signed WORD Y against the plane shifted by 005483BC.
        let plane = request.water_plane;
        let plane_y = (plane.height_fixed >> (plane.coordinate_shift & 31)) as i16;
        let y = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?[1] as i16;
        if y < plane_y {
            effect(self, actor, FamilyUpdateEffect::Retire, host)?;
        }
        if self.actor(actor)?.word_c4 == 2 {
            return Ok(());
        }
        let delay = self.derived_actor(actor)?.word_144;
        if delay != 0 {
            if delay < 0 {
                return Ok(());
            }
            self.derived_actor_mut(actor)?.word_144 = if delay > delta as i32 {
                delay - delta as i32
            } else {
                0
            };
            let effect_value = FamilyUpdateEffect::DelayedPhysics {
                microseconds: delta,
                mask: 0x1f,
            };
            return effect(self, actor, effect_value, host);
        }
        self.selector11_proximity(actor, delta, host)?;
        self.selector11_timers(actor, delta)?;
        let death = self.selector11_actor(actor)?.word_1b2;
        if death != 0 {
            return self.selector11_death(actor, death, request, host);
        }
        let Some(behavior) = self.derived_actor(actor)?.behavior_140.as_ref() else {
            return Ok(());
        };
        let inaccuracy = behavior.word_04 as i32;
        effect(
            self,
            actor,
            FamilyUpdateEffect::WeaponTick {
                microseconds: delta,
            },
            host,
        )?;
        let timer = self.selector11_actor(actor)?.word_190 as i32;
        if timer < delta as i32 {
            self.selector11_retarget(actor, host)?;
        } else {
            self.selector11_mut(actor)?.word_190 = timer.wrapping_sub(delta as i32) as u32;
        }
        let flags = self.selector11_actor(actor)?.word_1ae;
        if flags & 1 != 0 {
            let state = self.selector11_mut(actor)?;
            state.word_194 = state.word_194.wrapping_sub(delta);
        }
        let Some(reference) = self.derived_actor(actor)?.target_148 else {
            // 00402114..2C: the later aim byte is unread once flag 1 is set.
            let state = self.selector11_mut(actor)?;
            match state.word_19c {
                Some(0) => {}
                Some(_) => state.word_190 = 0,
                None => {
                    return Err(Selector11UpdateError::RequiredField {
                        actor,
                        offset: 0x19c,
                    })
                }
            }
            state.word_1ae = flags | 1;
            state.word_19c = Some(0);
            return Ok(());
        };
        let mut target = self.native_target(reference)?;
        let status = self.refresh_native_target(&mut target, request.settings_7c);
        self.write_native_target(reference, target)?;
        if status? == 0 {
            let state = self.selector11_mut(actor)?;
            state.word_190 = state.word_190.wrapping_sub(delta);
        }
        let aim = self.selector11_aim(actor, inaccuracy, request, host)?;
        if !aim.aligned {
            return Ok(());
        }
        self.selector11_fire(actor, delta, aim.local, host)
    }

    pub(in crate::object_runtime) fn selector11_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector11State, DerivedActorError> {
        match &mut self.derived_actor_mut(actor)?.class {
            DerivedActorClass::Selector11(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector11(actor)),
        }
    }

    /// 00401B85..D7: WORD timers +1B0 and +17A against signed delta >> 10.
    fn selector11_timers<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
    ) -> Result<(), Selector11UpdateError<E>> {
        let timer_17a = self.derived_actor(actor)?.word_17a;
        let timer_1b0 = self.selector11_actor(actor)?.word_1b0;
        if timer_17a | timer_1b0 == 0 {
            return Ok(());
        }
        let step = (delta as i32) >> 10;
        let next = |timer: u16| {
            if i32::from(timer as i16) > step {
                timer.wrapping_sub(step as u16)
            } else {
                0
            }
        };
        self.selector11_mut(actor)?.word_1b0 = next(timer_1b0);
        self.derived_actor_mut(actor)?.word_17a = next(timer_17a);
        Ok(())
    }

    /// 00401BEB..CD5: the death countdown, its children and the wreck.
    fn selector11_death<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        death: u16,
        request: &Selector11TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        let delta = request.microseconds;
        let step = (delta as i32) >> 10;
        if i32::from(death as i16) > step {
            self.selector11_mut(actor)?.word_1b2 = death.wrapping_sub(step as u16);
            return self
                .tick_status_visual(actor, delta, request.status_load_factor, host)
                .map_err(Selector11UpdateError::Status);
        }
        if (death as i16) > 0 {
            self.selector11_mut(actor)?.word_1b2 = 0xfffe;
            let mut position = self
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
            host.create_child(self, actor, 0x36, position)
                .map_err(|error| Selector11UpdateError::Host {
                    address: 0x0040_1c3a,
                    error,
                })?;
            // 00401C3D..65: unsigned size / 50 + 2, at most 6.
            let size = u32::from(self.object(actor)?.base.word_28);
            let count = (size / 50 + 2).min(6);
            position[1] = position[1].wrapping_sub(100);
            for _ in 0..count {
                host.create_child(self, actor, 0x3a, position)
                    .map_err(|error| Selector11UpdateError::Host {
                        address: 0x0040_1c7e,
                        error,
                    })?;
                position[1] = position[1].wrapping_add(50);
            }
            return Ok(());
        }
        let random = random(host, 0x0040_1c92)? & 0xffff;
        let math = NativeBasisMath::new(*request.sine_quarter);
        math.rotate_in_plane(
            &mut self.actor_mut(actor)?.rotating.basis,
            random as i32,
            RotationPlane::Xz,
        );
        self.owned_mut(actor)?.state.base.dirty_word |= 0x100;
        self.actor_mut(actor)?.rotating.visual.model_word = 0x46;
        host.refresh_model(self, actor)
            .map_err(|error| Selector11UpdateError::Host {
                address: 0x0040_1cb7,
                error,
            })?;
        self.actor_mut(actor)?.word_74 = 0;
        self.actor_mut(actor)?.word_c8 = 0;
        self.set_flag1(actor, false)?;
        Ok(())
    }

    /// 00401D3B..00402112: aim at the target and turn toward it.
    fn selector11_aim<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        inaccuracy: i32,
        request: &Selector11TickRequest<'_>,
        host: &mut H,
    ) -> Result<Selector11Aim, Selector11UpdateError<H::Error>> {
        let delta = request.microseconds;
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0x148,
                })?;
        let focus = self.native_target(reference)?.raw_focus_08_words().ok_or(
            Selector11UpdateError::RequiredField {
                actor,
                offset: 0x148,
            },
        )?;
        let mount = self.actor_behavior_position(
            actor,
            crate::object_runtime::ActorBehaviorPositionRequest::SelectedWeaponGroup,
        )?;
        let mut local: [i16; 3] =
            std::array::from_fn(|axis| focus[axis].wrapping_sub(mount[axis]) as i16);
        let [x, y, z] = local.map(i32::from);
        // 00401D8D..9A: X*X + Z*Z + Y*Y, wrapping.
        let squared = (x.wrapping_mul(x) as u32)
            .wrapping_add(z.wrapping_mul(z) as u32)
            .wrapping_add(y.wrapping_mul(y) as u32);
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        let lead = self
            .derived_actor(actor)?
            .behavior_140
            .as_ref()
            .is_some_and(|behavior| behavior.byte_0c != 0);
        if self.derived_actor(actor)?.word_17a != 0 {
            self.selector11_wobble(actor, delta, distance, &mut local, request, host)?;
        } else if lead {
            let velocity = self
                .native_target(reference)?
                .velocity_words(request.process_velocity)?
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0x148,
                })?;
            let behavior = self
                .derived_actor(actor)?
                .behavior_140
                .as_ref()
                .expect("lead");
            let row = BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, actor);
            let tables = host
                .behavior_tables()
                .ok_or(Selector11UpdateError::RequiredBehaviorTables)?;
            let speed = i32::from(read_row_word(self, tables, row, 2)? as i16);
            let time = (distance.wrapping_shl(10) as i32)
                .checked_div(speed)
                .ok_or(Selector11UpdateError::Division {
                    actor,
                    address: 0x0040_1e03,
                })?;
            let scale = if time < 0x1000 {
                time.wrapping_shl(17)
            } else {
                0x7fff_ffff
            };
            for (axis, word) in local.iter_mut().enumerate() {
                let product = i64::from(velocity[axis] as i16) * i64::from(scale);
                let lead = ((product >> 31) as i32).wrapping_shl(4);
                *word = word.wrapping_add(lead as i16);
            }
            let kind = i32::from(read_row_word(self, tables, row, 0)? as i16);
            if tables.read_dword(0x0053_dbb4_u32.wrapping_add(kind.wrapping_mul(16) as u32))? & 1
                != 0
            {
                let square = time.wrapping_mul(time);
                let correction = (i64::from(square) * 0x18_0000_i64) >> 31;
                local[1] = local[1].wrapping_add(correction as i16);
            }
        }
        // 00401F53 reads the +04 DWORD captured at 00401CE6, before 0048D030.
        if inaccuracy != 0 {
            let amplitude = (distance as i32).wrapping_mul(inaccuracy) >> 16;
            for (axis, address) in [0x0040_1f62, 0x0040_1f7a, 0x0040_1f92]
                .into_iter()
                .enumerate()
            {
                let random = (random(host, address)? & 0xffff) as i32;
                let correction = amplitude.wrapping_sub(random.wrapping_mul(amplitude) >> 15);
                local[axis] = local[axis].wrapping_add(correction as i16);
            }
        }
        // 004019E0's actual +48 is 004026B0: all ten basis words, then the
        // persistent yaw +18C and pitch +188 rotations on that copy.
        let mut basis = self.actor(actor)?.rotating.basis;
        let math = NativeBasisMath::new(*request.sine_quarter);
        let state = self.selector11_actor(actor)?;
        math.rotate_in_plane(&mut basis, state.word_18c as i32, RotationPlane::Xz);
        math.rotate_in_plane(&mut basis, state.word_188 as i32, RotationPlane::Yz);
        let out = super::super::kinematics::inverse_point_490970(basis, local);
        let limit = (delta >> 5) as i32;
        let mut aligned = true;
        let flags = self.selector11_actor(actor)?.word_1ae;
        let mut turn = |value: i32| -> Result<i32, Selector11UpdateError<H::Error>> {
            let mut angle = inverse_sine_q31(
                q31_divide(value as u32, distance) as i32,
                request.inverse_sine,
            )?;
            if out[2] < 0 {
                angle = if angle > 0 {
                    angle.wrapping_add(0x2000)
                } else {
                    -0x2000
                };
            }
            if angle > limit {
                angle = limit;
                aligned = false;
            } else if angle < limit.wrapping_neg() {
                angle = limit.wrapping_neg();
                aligned = false;
            }
            if flags & 8 != 0 {
                angle /= 2;
            }
            Ok(angle)
        };
        let yaw = turn(out[0])?;
        let state = self.selector11_mut(actor)?;
        state.word_18c = state.word_18c.wrapping_add(yaw as u32);
        let pitch = turn(out[1])?;
        let state = self.selector11_mut(actor)?;
        let pitch = (state.word_188 as i32).wrapping_sub(pitch);
        state.word_188 = pitch.clamp(-0x3fff, 0x1000) as u32;
        let elapsed = state.word_19c.ok_or(Selector11UpdateError::RequiredField {
            actor,
            offset: 0x19c,
        })?;
        state.word_19c = Some(elapsed.wrapping_add(delta));
        Ok(Selector11Aim { local, aligned })
    }

    /// 00401EA7..00401F4F, reached while +17A is nonzero.
    /// Selector11WobbleAngleApproximation: no constructor or command writes
    /// the wobble angles +1AC/+1AA, so a turret's first wobble walks them on
    /// from allocator residue; the port starts them at 0
    /// ([`SELECTOR11_WOBBLE_ANGLE_APPROXIMATION`]).
    fn selector11_wobble<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        distance: u32,
        local: &mut [i16; 3],
        request: &Selector11TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        let scale = delta >> 4;
        let first = random(host, 0x0040_1ead)? & 0xffff;
        let state = self.selector11_mut(actor)?;
        if state.word_1ac.is_none() || state.word_1aa.is_none() {
            log_wobble_angle_approximation();
        }
        let angle_1ac = state
            .word_1ac
            .unwrap_or(SELECTOR11_WOBBLE_ANGLE_APPROXIMATION)
            .wrapping_add((first.wrapping_mul(scale) >> 16) as u16);
        state.word_1ac = Some(angle_1ac);
        let second = random(host, 0x0040_1ec4)? & 0xffff;
        let state = self.selector11_mut(actor)?;
        let angle_1aa = state
            .word_1aa
            .unwrap_or(SELECTOR11_WOBBLE_ANGLE_APPROXIMATION)
            .wrapping_add((second.wrapping_mul(scale) >> 16) as u16);
        state.word_1aa = Some(angle_1aa);
        for (axis, angle) in [(0, angle_1ac), (2, angle_1aa)] {
            let sine = folded_sine(i32::from(angle as i16), request.sine_quarter);
            let offset = sine.wrapping_mul(distance as i32) >> 17;
            local[axis] = local[axis].wrapping_add(offset as i16);
        }
        Ok(())
    }

    /// 00402131..35: the firing gate and normalized shot direction.
    fn selector11_fire<H: Selector11UpdateHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        local: [i16; 3],
        host: &mut H,
    ) -> Result<(), Selector11UpdateError<H::Error>> {
        let state = self.selector11_actor(actor)?;
        if state.word_1ae & 1 != 0 || state.word_1b0 != 0 {
            return Ok(());
        }
        let team = self.actor(actor)?.word_b4;
        let tasks = host.tasks().ok_or(Selector11UpdateError::RequiredTasks)?;
        let owner = if team < 0x13 {
            tasks.owner(team)?
        } else {
            None
        };
        let owner = owner.ok_or(Selector11UpdateError::MissingTask { actor, team })?;
        let source = tasks.record(owner)?.source;
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(Selector11UpdateError::RequiredField {
                    actor,
                    offset: 0x148,
                })?;
        if self.native_target(reference)?.handle_04() == source {
            return Ok(());
        }
        let behavior = self.derived_actor(actor)?.behavior_140.as_ref().ok_or(
            Selector11UpdateError::RequiredField {
                actor,
                offset: 0x140,
            },
        )?;
        let index = i32::from(behavior.byte_10 as i8);
        let entry = usize::try_from(index)
            .ok()
            .and_then(|index| behavior.entries_18.get(index))
            .ok_or(Selector11UpdateError::WeaponEntry {
                actor,
                index,
                entries: behavior.entries_18.len(),
            })?;
        if entry[5] != 0 || entry[0] == 0 || entry[7] >= 6_000_000 {
            return Ok(());
        }
        let direction_bits =
            normalize_words(local, NativeLengthPrecision::Spilled).map(f32::to_bits);
        host.queue_shot(self, actor, delta, direction_bits)
            .map_err(|error| Selector11UpdateError::Host {
                address: 0x0040_2235,
                error,
            })
    }
}

struct Selector11Aim {
    local: [i16; 3],
    aligned: bool,
}

/// 00401EE9..F49: the 0053DF00 quarter table, mirrored by bit 14 and
/// negated by bit 15 of the sign-extended WORD angle.
fn folded_sine(angle: i32, table: &[u16; 4096]) -> i32 {
    let mut quarter = angle & 0x3ffc;
    if angle & 0x4000 != 0 {
        quarter = 0x3ffc - quarter;
    }
    let value = i32::from(table[(quarter >> 2) as usize]);
    if angle & 0x8000 != 0 {
        -value
    } else {
        value
    }
}

fn effect<H: FamilyUpdateHost>(
    arena: &mut ObjectArena,
    actor: ObjectId,
    effect: FamilyUpdateEffect,
    host: &mut H,
) -> Result<(), Selector11UpdateError<H::Error>> {
    host.effect(arena, actor, effect)
        .map_err(|error| Selector11UpdateError::Effect { effect, error })
}

fn random<H: FamilyUpdateHost>(
    host: &mut H,
    address: u32,
) -> Result<u32, Selector11UpdateError<H::Error>> {
    host.random()
        .map_err(|error| Selector11UpdateError::Host { address, error })
}

#[cfg(test)]
mod tests;
