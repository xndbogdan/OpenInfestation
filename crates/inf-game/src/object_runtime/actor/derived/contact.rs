//! Complete shared 0047A2F0/00426270 contact phases. The caller owns the
//! selected executable table and real process/world loans; no surface fallback
//! or host-address stand-in is admitted. Outputs retain their reached prefix.

use super::kinematics::{divide, ftol, ActorMotionError};
use crate::object_runtime::{ActorTerrainHost, ObjectArena, ObjectError, ObjectId};
use crate::{frontend_camera::AnimationClockSource, terrain_runtime::TerrainRuntime};
use inf_formats::{native_basis::sine_q15, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

/// Independent H14 acquisition: the initial H12 loan remains live through it.
pub trait ActorContactHeaderHost {
    type Error;
    fn header(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<&[u8], Self::Error>;
}

/// This is an address-domain proof, never a guessed numeric portable pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorContactStackPolicy {
    RequiredNativeAddress,
    /// BF09 class36 caller481C33/481D1D's real stack buffer, Windows8+,
    /// non-LAA PE32: [10000h,7FFFFFFFh] exceeds every signed WORD threshold.
    Bf09Class36Windows8PlusStack,
}

pub struct ActorContactEnvironment<'a> {
    pub animation_clock: &'a dyn AnimationClockSource,
    /// Existing retained prepared terrain owns the paired5483BC load policy.
    /// Only its current shift is read here; plane base comes from captured H12.
    pub plane_policy: &'a TerrainRuntime,
    pub up: &'a [f32; 3],
    pub sine_quarter: &'a [u16; 4096],
}

pub struct ActorContactRequest<'a> {
    pub table: u32,
    pub step: u32,
    pub executable: FrontendExecutable<'a>,
    pub environment: &'a ActorContactEnvironment<'a>,
    pub stack_policy: ActorContactStackPolicy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActorContactOutput {
    pub normal: [Option<f32>; 3],
    pub impulse: Option<u16>,
    /// Native packet+2 padding is not written by 00426270.
    pub padding_02: Option<u16>,
    pub alignment: Option<u32>,
    pub motor: Option<u32>,
}

#[derive(Debug, Error)]
pub enum ActorContactError<T, H> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Motion(#[from] ActorMotionError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
    #[error("current H12 acquisition failed: {0}")]
    Terrain(T),
    #[error("current H14 acquisition failed: {0}")]
    Header(H),
    #[error("actor {0:?} requires initialized XYZ WORDs")]
    MissingPosition(ObjectId),
    #[error("current H14 header has no byte4")]
    RequiredHeaderByte4,
    #[error("004265A6 requires the authenticated caller's native stack address domain")]
    RequiredNativeStackComparison,
}

fn position<T, H>(
    arena: &ObjectArena,
    actor: ObjectId,
) -> Result<[u16; 3], ActorContactError<T, H>> {
    arena
        .object(actor)?
        .base
        .position_words
        .ok_or(ActorContactError::MissingPosition(actor))
}

fn table_word(
    executable: FrontendExecutable<'_>,
    table: u32,
    offset: u32,
) -> Result<i32, FrontendExecutableError> {
    let bytes = executable.mapped(table.wrapping_add(offset), 2)?.bytes;
    Ok(i32::from(i16::from_le_bytes([bytes[0], bytes[1]])))
}

impl ObjectArena {
    /// 0047A2F0 captures H12 before the first actor coordinate read. Four
    /// height samples use WORD-wrapped +/-64; destination stores are Z/X/Y.
    pub fn actor_terrain_normal<T: ActorTerrainHost, H>(
        &mut self,
        actor: ObjectId,
        terrain: &mut T,
        output: &mut [Option<f32>; 3],
    ) -> Result<(), ActorContactError<T::Error, H>> {
        let current = terrain
            .terrain(self, actor)
            .map_err(ActorContactError::Terrain)?;
        let [x, _, z] = position::<T::Error, H>(self, actor)?;
        let z0 = current.height_at([x, z.wrapping_sub(64)]);
        let [x, _, z] = position::<T::Error, H>(self, actor)?;
        let zd = i32::from(current.height_at([x, z.wrapping_add(64)]).wrapping_sub(z0));
        let zl = (f64::from(zd).powi(2) + 16384.0).sqrt() as f32;
        let (za, zb, zc) = if zl == 0.0 {
            (1.0f32, 0.0f32, 0.0f32)
        } else {
            (
                0.0 / zl,
                (f64::from(zd) / f64::from(zl)) as f32,
                (128.0 / f64::from(zl)) as f32,
            )
        };
        let [x, _, z] = position::<T::Error, H>(self, actor)?;
        let x0 = current.height_at([x.wrapping_sub(64), z]);
        let [x, _, z] = position::<T::Error, H>(self, actor)?;
        let xd = i32::from(current.height_at([x.wrapping_add(64), z]).wrapping_sub(x0));
        // Raw signed X remains retained for its multiplication by spilled X.
        let xs = xd as f32;
        let xl = (f64::from(xd) * f64::from(xs) + 16384.0).sqrt() as f32;
        let (xa, xb, xc) = if xl == 0.0 {
            (0.0f32, 0.0f64, 1.0f64)
        } else {
            (
                0.0 / xl,
                f64::from(xs) / f64::from(xl),
                128.0 / f64::from(xl),
            )
        };
        output[2] = Some((xb * f64::from(za) - f64::from(zb) * xc) as f32);
        output[0] = Some((f64::from(xa) * f64::from(zb) - xb * f64::from(zc)) as f32);
        output[1] = Some((f64::from(zc) * xc - f64::from(xa) * f64::from(za)) as f32);
        Ok(())
    }

    /// Complete 00426270. All checked backing reads occur in native phase
    /// order. Errors preserve scalar138, packet fields, and preceding actor writes.
    pub fn apply_actor_contact<T: ActorTerrainHost, H: ActorContactHeaderHost>(
        &mut self,
        actor: ObjectId,
        request: ActorContactRequest<'_>,
        terrain: &mut T,
        header: &mut H,
        output: &mut ActorContactOutput,
    ) -> Result<(), ActorContactError<T::Error, H::Error>> {
        let ActorContactRequest {
            table,
            step,
            executable,
            environment,
            stack_policy,
        } = request;
        let clock = environment.animation_clock.animation_value();
        let initial = terrain
            .terrain(self, actor)
            .map_err(ActorContactError::Terrain)?;
        let water = self.derived_actor(actor)?.byte_160;
        let [x, _, z] = position::<T::Error, H::Error>(self, actor)?;
        let ground = i32::from(initial.height_at([x, z]));
        let mut surface = ground;
        let mut negative_water = false;
        if water != 0 {
            let world_header = header
                .header(self, actor)
                .map_err(ActorContactError::Header)?;
            let plane = initial.plane_base() >> (environment.plane_policy.plane_shift() & 31);
            let [x, _, z] = position::<T::Error, H::Error>(self, actor)?;
            let flags = *world_header
                .get(4)
                .ok_or(ActorContactError::RequiredHeaderByte4)?;
            let wave = if flags & 0x40 != 0 && plane > ground {
                let clock64 = clock.wrapping_shl(6);
                let first = sine_q15(
                    clock64
                        .wrapping_add(u32::from(x).wrapping_mul(12))
                        .wrapping_shl(1) as i32,
                    environment.sine_quarter,
                );
                let second = sine_q15(
                    u32::from(z)
                        .wrapping_add(u32::from(x))
                        .wrapping_shl(5)
                        .wrapping_add(clock64.wrapping_mul(5)) as i32,
                    environment.sine_quarter,
                );
                let third = sine_q15(
                    clock64
                        .wrapping_add(u32::from(z).wrapping_sub(u32::from(x)).wrapping_mul(6))
                        .wrapping_shl(3) as i32,
                    environment.sine_quarter,
                );
                let sum = (first.wrapping_add(second) >> 5).wrapping_add(third >> 6) as i16 as i32;
                let displacement =
                    sum.wrapping_mul(plane.wrapping_sub(ground).wrapping_add(0x200)) >> 15;
                plane.wrapping_add(displacement as i16 as i32).max(ground)
            } else {
                plane
            };
            let wave = wave as i16 as i32;
            if ground < wave {
                surface = wave;
                output.normal[0] = Some(environment.up[0]);
                output.normal[1] = Some(environment.up[1]);
                output.normal[2] = Some(environment.up[2]);
            } else {
                self.actor_terrain_normal::<T, H::Error>(actor, terrain, &mut output.normal)?;
            }
        } else {
            self.actor_terrain_normal::<T, H::Error>(actor, terrain, &mut output.normal)?;
        }
        let normal = output
            .normal
            .map(|x| x.expect("completed native normal stores"));
        let y = i32::from(position::<T::Error, H::Error>(self, actor)?[1] as i16);
        let size = i32::from(self.object(actor)?.base.word_28);
        let mut ratio = divide(
            ftol(f64::from(y.wrapping_sub(surface)) * f64::from(normal[1])).wrapping_shl(12),
            size,
            0x0042_6434,
        )?;
        self.derived_actor_mut(actor)?.word_138 = Some((ratio.wrapping_mul(100) >> 12) as u32);
        let mut complement = 0;
        let alignment = if ratio > 0x1666 {
            0
        } else if ratio <= 0 {
            0xffff
        } else {
            let value = divide(
                0x1666i32.wrapping_sub(ratio).wrapping_shl(14),
                0x1666,
                0x0042_6476,
            )?
            .wrapping_add(0x3fff);
            complement = 0xffffi32.wrapping_sub(value);
            value as u32
        };
        output.alignment = Some(alignment);
        let upper = table_word(executable, table, 6)?;
        if ratio >= upper {
            output.impulse = Some(0);
        } else {
            let lower = table_word(executable, table, 4)?;
            let impulse = table_word(executable, table, 0)?;
            if ratio > lower {
                output.impulse = Some(impulse as u16);
                let upper = table_word(executable, table, 6)?;
                let impulse = table_word(executable, table, 0)?;
                let lower = table_word(executable, table, 4)?;
                let extra = divide(
                    upper.wrapping_sub(ratio).wrapping_mul(impulse),
                    upper.wrapping_sub(lower),
                    0x0042_64dd,
                )?;
                output.impulse = Some(output.impulse.unwrap().wrapping_add(extra as u16));
                let upper = table_word(executable, table, 6)?;
                let lower = table_word(executable, table, 4)?;
                let extra = divide(
                    upper.wrapping_sub(ratio).wrapping_mul(complement),
                    upper.wrapping_sub(lower),
                    0x0042_64f6,
                )?;
                output.alignment = Some(output.alignment.unwrap().wrapping_add(extra as u32));
            } else {
                output.impulse = Some((impulse as u16).wrapping_shl(1));
                output.alignment = Some(0xffff);
                let comparison = if self.derived_actor(actor)?.byte_160 != 0 {
                    let threshold = table_word(executable, table, 2)?;
                    if ratio < threshold {
                        negative_water = ratio < 0;
                        let [x, _, z] = position::<T::Error, H::Error>(self, actor)?;
                        let fresh = terrain
                            .terrain(self, actor)
                            .map_err(ActorContactError::Terrain)?;
                        surface = i32::from(fresh.height_at([x, z]));
                        let y = i32::from(position::<T::Error, H::Error>(self, actor)?[1] as i16);
                        Some(divide(
                            ftol(f64::from(y.wrapping_sub(surface)) * f64::from(normal[1]))
                                .wrapping_shl(12),
                            size,
                            0x0042_658c,
                        )?)
                    } else {
                        None
                    }
                } else {
                    Some(ratio)
                };
                if step != 0 {
                    let threshold = table_word(executable, table, 2)?;
                    let correct = match comparison {
                        Some(value) => value < threshold,
                        None => match stack_policy {
                            ActorContactStackPolicy::Bf09Class36Windows8PlusStack => false,
                            ActorContactStackPolicy::RequiredNativeAddress => {
                                return Err(ActorContactError::RequiredNativeStackComparison)
                            }
                        },
                    };
                    if correct {
                        let next_y =
                            ((threshold.wrapping_mul(size) >> 12).wrapping_add(surface)) as u16;
                        let velocity_y = self.actor(actor)?.words_8e_92[1] as i16;
                        let mut xyz = position::<T::Error, H::Error>(self, actor)?;
                        xyz[1] = next_y;
                        self.owned_mut(actor)?.state.base.position_words = Some(xyz);
                        if velocity_y < 0 {
                            self.actor_mut(actor)?.words_8e_92[1] =
                                (-(i32::from(velocity_y) / 4)) as u16;
                        }
                        ratio = table_word(executable, table, 2)?;
                    }
                }
            }
            if output.alignment != Some(0) {
                let velocity = self.actor(actor)?.words_8e_92.map(|v| f64::from(v as i16));
                let dot = ftol(
                    (velocity[0] * f64::from(normal[0]) + velocity[2] * f64::from(normal[2]))
                        + velocity[1] * f64::from(normal[1]),
                )
                .wrapping_shl(12);
                if dot > 0 && !negative_water {
                    let impulse = divide(
                        i32::from(output.impulse.unwrap()).wrapping_shl(12),
                        (dot >> 8).wrapping_add(0x1000),
                        0x0042_6651,
                    )?;
                    output.impulse = Some(impulse as u16);
                } else if dot < 0 {
                    let magnitude = divide(dot, size, 0x0042_665a)?.wrapping_neg().min(0x1000);
                    output.impulse = Some(
                        (magnitude
                            .wrapping_add(0x1000)
                            .wrapping_mul(i32::from(output.impulse.unwrap()))
                            >> 12) as u16,
                    );
                }
                if step != 0 {
                    let magnitude = ((u32::from(output.impulse.unwrap()).wrapping_mul(step) as i32)
                        >> 15) as i16;
                    let increment =
                        normal.map(|axis| ftol(f64::from(magnitude) * f64::from(axis)) as u16);
                    self.add_actor_linear_velocity(actor, increment)?;
                }
            }
        }
        let lower = table_word(executable, table, 8)?;
        output.motor = Some(if ratio <= lower {
            0xffff
        } else {
            let upper = table_word(executable, table, 10)?;
            if ratio >= upper {
                0
            } else {
                divide(
                    upper.wrapping_sub(ratio).wrapping_mul(0xffff),
                    upper.wrapping_sub(lower),
                    0x0042_671f,
                )? as u32
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
