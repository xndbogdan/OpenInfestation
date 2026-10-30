//! Selector12 +24 after normal virtual+D8:00467A77..00467D98.
//! Controls come from the actual nullable target protocol. The first genuine
//! avoidance-cache miss retains its explicit owner boundary; firing joins the
//! shared retained behavior chain before the movement continuation.

use inf_formats::native_basis::{isqrt_48efc0, NativeBasis, NativeBasisMath};
use std::convert::Infallible;
use thiserror::Error;

use super::{
    kinematics::{divide, ftol},
    FamilyAvoidanceError, FamilyAvoidanceHost, FamilyObstacleProbeHost, FamilyObstacleScanError,
    FamilyTerrainProbeError, FamilyTerrainProbeMissError, FamilyTerrainProbeMissHost,
    Selector12MotionError, Selector12MotionRequest, TerrainProbeError,
};
use crate::object_runtime::{ActorTerrainHost, NativeHeapRef, ObjectArena, ObjectError, ObjectId};

pub struct Selector12TailRequest<'a, H> {
    pub microseconds: u32,
    /// Current supplied settings+7C used by the owned target refresh service.
    pub settings_7c: u32,
    /// Retained authored process BYTE0053A128, never a control-value sentinel.
    pub surface_snap_53a128: u8,
    pub math: &'a NativeBasisMath,
    pub inverse_sine: &'a [i16; 1024],
    pub sine_quarter: &'a [u16; 4096],
    pub terrain: &'a mut H,
    pub firing: crate::object_runtime::ActorWeaponFireContext<'a>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector12TailProbeError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(
        "actor {actor:?} terrain/collision miss00429EE0 failed at {position_words:?}: {error}"
    )]
    Miss {
        actor: ObjectId,
        position_words: [u16; 2],
        error: FamilyTerrainProbeMissError<E>,
    },
    #[error("actor {0:?} requires current +234 cache")]
    MissingCache(ObjectId),
    #[error("actor {0:?} replaced its captured +234 allocation")]
    ReplacedAllocation(ObjectId),
    #[error("probe DWORD {word_index} is outside current {words}-DWORD allocation")]
    StorageRange { word_index: usize, words: usize },
}

#[derive(Debug, Error)]
pub enum Selector12TailError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {0:?} does not own Selector12 tail00467A77")]
    UnsupportedClass(ObjectId),
    #[error("actor {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires direct +08 focus at {address:08X}")]
    RequiredTargetFocus {
        reference: NativeHeapRef,
        address: u32,
    },
    #[error("actor {actor:?} requires behavior+18 entry {index}; current count {entries}")]
    RequiredBehaviorEntry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
    #[error(transparent)]
    Firing(#[from] crate::object_runtime::ActorWeaponFireError),
    #[error(transparent)]
    Division(#[from] super::ActorMotionError),
    #[error("native avoidance failed: {0}")]
    Avoidance(FamilyAvoidanceError<FamilyObstacleScanError<Selector12TailProbeError<E>>>),
    #[error("native +118 terrain acquisition failed: {0}")]
    Terrain(E),
    #[error("native Selector12 +118 failed: {0}")]
    Motion(Selector12MotionError<Infallible>),
}

impl ObjectArena {
    /// Consumed only after the source-owned prefix and virtual+D8 return.
    /// Every error preserves reached actor, target heap and probe-cache writes.
    pub fn tick_selector12_tail<H>(
        &mut self,
        actor: ObjectId,
        mut request: Selector12TailRequest<'_, H>,
    ) -> Result<(), Selector12TailError<<H as ActorTerrainHost>::Error>>
    where
        H: ActorTerrainHost + FamilyTerrainProbeMissHost<Error = <H as ActorTerrainHost>::Error>,
    {
        self.object(actor)?;
        if self.selector12_actor(actor).is_err() {
            return Err(Selector12TailError::UnsupportedClass(actor));
        }
        //004679FF/00467A03 initialize the stack pair before the enclosing
        //prefix. D8 has no access to these local words; this tail starts fresh.
        let mut controls = [0_i32; 2];
        if let Some(captured) = self.derived_actor(actor)?.target_148 {
            let mut target = self.native_target(captured)?;
            let status = self.refresh_native_target(&mut target, request.settings_7c);
            //The decoded view writes through to the same captured allocation,
            //including any prefix retained by an error in virtual+18.
            self.write_native_target(captured, target)?;
            self.derived_actor_mut(actor)?.word_150 = Some(status?);
            let (local, distance) = if let Some(secondary) = self.derived_actor(actor)?.target_154 {
                let mut target = self.native_target(secondary)?;
                let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
                self.write_native_target(secondary, target)?;
                refreshed?;
                let secondary = self.derived_actor(actor)?.target_154.ok_or(
                    Selector12TailError::RequiredField {
                        actor,
                        offset: 0x154,
                        address: 0x0046_7aa4,
                    },
                )?;
                let (world, distance) = self
                    .selector12_tail_geometry::<<H as ActorTerrainHost>::Error>(
                        actor,
                        secondary,
                        0x0046_7abd,
                    )?;
                self.derived_actor_mut(actor)?.word_158 = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                controls[1] = i32::from(local[2]) / 2;
                let timer = self.derived_actor(actor)?.word_15c.ok_or(
                    Selector12TailError::RequiredField {
                        actor,
                        offset: 0x15c,
                        address: 0x0046_7b3c,
                    },
                )?;
                if (timer as i32) > (request.microseconds as i32)
                    && (distance as i32) > i32::from(self.object(actor)?.base.word_28)
                {
                    self.derived_actor_mut(actor)?.word_15c =
                        Some(timer.wrapping_sub(request.microseconds));
                } else {
                    if let Some(reference) = self.derived_actor(actor)?.target_154 {
                        self.release_actor_service(reference)?;
                    }
                    //00467B73 clears only after the normal deleting callback.
                    self.derived_actor_mut(actor)?.target_154 = None;
                }
                (local, distance)
            } else {
                let primary = self.derived_actor(actor)?.target_148.ok_or(
                    Selector12TailError::RequiredField {
                        actor,
                        offset: 0x148,
                        address: 0x0046_7b7e,
                    },
                )?;
                let (world, distance) = self
                    .selector12_tail_geometry::<<H as ActorTerrainHost>::Error>(
                        actor,
                        primary,
                        0x0046_7b97,
                    )?;
                self.derived_actor_mut(actor)?.word_14c = Some(distance);
                let local = self.actor(actor)?.rotating.basis.inverse_words(world);
                let spacing = self.family428650_actor(actor)?.words_204_208[1].ok_or(
                    Selector12TailError::RequiredField {
                        actor,
                        offset: 0x208,
                        address: 0x0046_7c0b,
                    },
                )? as i32;
                let z = i32::from(local[2]);
                controls[1] = if spacing >= 0 {
                    z.wrapping_sub(spacing) / 2
                } else {
                    let mut forward = z / 2;
                    if z > 0 {
                        forward = forward.wrapping_sub(divide(
                            spacing.wrapping_mul(z),
                            distance.wrapping_add(1) as i32,
                            0x0046_7c45,
                        )?);
                    }
                    forward
                };
                self.selector12_tail_fire_gate::<<H as ActorTerrainHost>::Error>(
                    actor,
                    local,
                    distance,
                    world,
                    request.microseconds,
                    &mut request.firing,
                )?;
                (local, distance)
            };
            if distance != 0 {
                let turn = divide(
                    i32::from(local[0]).wrapping_shl(14),
                    distance as i32,
                    0x0046_7ce4,
                )?;
                controls[0] = if local[2] >= 0 {
                    turn
                } else if turn > 0 {
                    32768_i32.wrapping_sub(turn)
                } else {
                    (-32768_i32).wrapping_sub(turn)
                };
            }
        }
        let adjusted = self
            .adjust_family_avoidance(
                actor,
                &mut controls,
                request.microseconds,
                &mut TailAvoidance {
                    miss: &mut *request.terrain,
                },
            )
            .map_err(Selector12TailError::Avoidance)?;
        if !adjusted {
            let current = self.actor(actor)?;
            let velocity = current.words_8e_92.map(|word| i32::from(word as i16));
            let basis = current.rotating.basis;
            //00467D45..D60 adds Y+Z before X and retains x87 precision.
            let forward = ftol(
                f64::from(velocity[1]) * f64::from(f32::from_bits(basis.words[8]))
                    + f64::from(velocity[2]) * f64::from(f32::from_bits(basis.words[9]))
                    + f64::from(velocity[0]) * f64::from(f32::from_bits(basis.words[7])),
            );
            if forward.wrapping_abs() < 10 {
                self.actor_mut(actor)?.words_8e_92[2] = 0;
                self.actor_mut(actor)?.words_8e_92[0] = 0;
            }
        }
        //00467DF0 acquires H12 before its XYZ reads. The returned actual
        //prepared terrain is sampled without a second registry acquisition.
        let terrain = <H as ActorTerrainHost>::terrain(request.terrain, self, actor)
            .map_err(Selector12TailError::Terrain)?;
        let mut terrain = terrain;
        self.tick_selector12_motion(
            actor,
            Selector12MotionRequest {
                microseconds: request.microseconds,
                turn_control: controls[0],
                forward_control: controls[1],
                surface_snap_53a128: request.surface_snap_53a128,
                math: request.math,
                inverse_sine: request.inverse_sine,
                sine_quarter: request.sine_quarter,
                terrain: &mut terrain,
            },
        )
        .map_err(Selector12TailError::Motion)
    }

    fn selector12_tail_geometry<E>(
        &self,
        actor: ObjectId,
        reference: NativeHeapRef,
        address: u32,
    ) -> Result<([i16; 3], u32), Selector12TailError<E>> {
        let focus = self
            .native_target(reference)?
            .raw_focus_08_words()
            .ok_or(Selector12TailError::RequiredTargetFocus { reference, address })?;
        let position =
            self.object(actor)?
                .base
                .position_words
                .ok_or(Selector12TailError::RequiredField {
                    actor,
                    offset: 0x32,
                    address,
                })?;
        let world: [i16; 3] =
            std::array::from_fn(|axis| focus[axis].wrapping_sub(position[axis]) as i16);
        let squared = world.iter().fold(0_u32, |sum, &word| {
            let value = i32::from(word);
            sum.wrapping_add(value.wrapping_mul(value) as u32)
        });
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        Ok((world, distance))
    }

    fn selector12_tail_fire_gate<E>(
        &mut self,
        actor: ObjectId,
        local: [i16; 3],
        distance: u32,
        world: [i16; 3],
        microseconds: u32,
        firing: &mut crate::object_runtime::ActorWeaponFireContext<'_>,
    ) -> Result<(), Selector12TailError<E>> {
        let derived = self.derived_actor(actor)?;
        let Some(behavior) = derived.behavior_140.as_ref() else {
            return Ok(());
        };
        if (distance as i32) >= derived.word_164 {
            return Ok(());
        }
        if self.family428650_actor(actor)?.byte_22c == 0 {
            return Ok(());
        }
        if derived.word_150 != Some(3) {
            return Ok(());
        }
        let index = behavior.byte_10 as i8;
        let entry = behavior.entries_18.get(index as usize).ok_or(
            Selector12TailError::RequiredBehaviorEntry {
                actor,
                index,
                entries: behavior.entries_18.len(),
            },
        )?;
        if entry[5] != 0 || entry[0] == 0 || entry[7] >= 6_000_000 {
            return Ok(());
        }
        if local[2] <= 0 {
            return Ok(());
        }
        if divide(
            i32::from(local[0]).wrapping_shl(1),
            i32::from(local[2]),
            0x0046_7ca3,
        )? != 0
        {
            return Ok(());
        }
        if local[1] >= 750 {
            return Ok(());
        }
        //00467CB6 supplies true after this caller's geometry and aim gates.
        self.fire_actor_weapon(
            actor,
            crate::object_runtime::ActorWeaponFireRequest {
                target: self.derived_actor(actor)?.target_148,
                distance,
                world,
                microseconds,
                mode: crate::object_runtime::ActorWeaponFireMode::Predictive { publish_lead: true },
            },
            firing,
        )?;
        Ok(())
    }
}

//Final12/36+48 both4026B0. Shared installed-cache host; no copied arena.
pub(super) struct TailAvoidance<'a, H> {
    pub(super) miss: &'a mut H,
}
impl<H: FamilyTerrainProbeMissHost> FamilyAvoidanceHost for TailAvoidance<'_, H> {
    type Error = FamilyObstacleScanError<Selector12TailProbeError<H::Error>>;
    fn scan(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        reverse: bool,
    ) -> Result<(), Self::Error> {
        arena.scan_family_obstacles(
            actor,
            reverse,
            &mut TailProbe {
                miss: &mut *self.miss,
            },
        )
    }
    fn basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        //Final12 table527058+48 and final36 table527EE0+48 both4026B0:
        //the same actual embedded actor+44, authenticated independently.
        Ok(arena.actor(actor)?.rotating.basis)
    }
}

struct TailProbe<'a, H> {
    miss: &'a mut H,
}
impl<H: FamilyTerrainProbeMissHost> FamilyObstacleProbeHost for TailProbe<'_, H> {
    type Error = Selector12TailProbeError<H::Error>;
    fn probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        position_words: [u16; 2],
    ) -> Result<u32, Self::Error> {
        arena
            .probe_family_terrain_cache(actor, position_words, |arena, current, point| {
                arena
                    .probe_family_terrain_miss(current, point, self.miss)
                    .map(u32::from)
                    .map_err(|error| Selector12TailProbeError::Miss {
                        actor: current,
                        position_words: point,
                        error,
                    })
            })
            .map(u32::from)
            .map_err(|error| match error {
                FamilyTerrainProbeError::Object(error) => Selector12TailProbeError::Object(error),
                FamilyTerrainProbeError::MissingCache(actor) => {
                    Selector12TailProbeError::MissingCache(actor)
                }
                FamilyTerrainProbeError::ReplacedAllocation(actor) => {
                    Selector12TailProbeError::ReplacedAllocation(actor)
                }
                FamilyTerrainProbeError::Cache(TerrainProbeError::Host(error)) => error,
                FamilyTerrainProbeError::Cache(TerrainProbeError::StorageRange {
                    word_index,
                    words,
                }) => Selector12TailProbeError::StorageRange { word_index, words },
            })
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
