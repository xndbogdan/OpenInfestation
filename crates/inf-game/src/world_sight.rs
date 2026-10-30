//! The sight test `0043AD90`: what the line from an origin toward a target
//! meets first. The aim (`0041F9C0`) runs it from the vehicle each update.
//!
//! The ray's direction is the normalized difference (`0043B230`) and its
//! basis comes from `00490370` about the world's up, or about X when the ray
//! is steeper than 0.75 either way. The actor pass (`0043B2E0`) tests every
//! actor with flag 4 near the ray through its model (`0047D080`) and
//! shortens the ray to a hit. The ray is then marched in 64-unit samples:
//! each new cell scans the furniture around it (`0043A990`, `0043A7E0`) and
//! a sample under the cell's highest corner checks the terrain's height
//! (`00450A30`). Furniture and terrain hits end the march.

use std::sync::Arc;

use inf_formats::native_basis::{isqrt_48efc0, orient_from_forward, NativeBasis};
use thiserror::Error;

use crate::{
    frontend_camera::AnimationClockSource,
    model_auxiliary::{
        query_auxiliary_ray, ray::RayRequest, AuxiliaryModelError, AuxiliaryModelSource,
        AuxiliaryRayRequest, AuxiliaryValueSource,
    },
    model_process::ModelProcessOwners,
    model_scratch::{ModelScratchDrawState, ModelScratchError, RegisteredMarkUnwindApproximation},
    object_runtime::{
        resident_model_resource, CollisionContact, ObjectArena, ObjectError, ObjectId,
        ObjectModelCallbacks, ResidentModelBinding,
    },
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::{TerrainRuntime, TerrainSparsePayload},
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    world_furniture_effect::{
        resident::{resident_placement_table, ResidentFurnitureResourceError},
        AuxiliaryRawSource, ProcessFurnitureQueryState,
    },
    world_projectile_collision::{ProjectileGeometryError, ProjectileGeometryOwners},
    ResidentRuntime,
};

/// `0055F280` and `0055F290`: the world's Y and X axes.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];
const WORLD_X: [f32; 3] = [1.0, 0.0, 0.0];
/// `0052236C` and `00522370`: past these the ray turns about X instead.
const STEEP: f32 = 0.75;
/// The march's sample spacing (`0043B1E7`).
const SAMPLE: i32 = 64;

/// The actors the sight test has stopped for, each logged once.
static SIGHT_STOPS: std::sync::Mutex<Vec<ObjectId>> = std::sync::Mutex::new(Vec::new());

fn stop_sight_once(actor: ObjectId, slot: u16) {
    let mut stops = SIGHT_STOPS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !stops.contains(&actor) {
        stops.push(actor);
        eprintln!(
            "Sight test stopped for actor {actor:?} after reached writes: its model slot {slot} \
             is NULL, which 0047D080 would read; the aim passes it"
        );
    }
}

#[derive(Debug, Error)]
pub enum SightError {
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error(transparent)]
    Placement(#[from] ResidentFurnitureResourceError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Auxiliary(#[from] AuxiliaryModelError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error(transparent)]
    Geometry(#[from] ProjectileGeometryError),
    #[error("0043A990 reached flagged terrain cell {grid:?} without its sparse record")]
    MissingSparseRecord { grid: [u16; 2] },
}

/// What the sight met, with the point it records at `+10`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SightHit {
    /// Type 1: the first sample under the terrain.
    Terrain { point: [u16; 3] },
    /// Type 2: a furniture model; `position` (`+16`) is its cell's corner at
    /// the cell's height, and the point lies along the ray at its entry.
    Furniture { point: [u16; 3], position: [u16; 3] },
    /// Type 3: an actor's model, by the actor pass.
    Actor { point: [u16; 3], actor: ObjectId },
}

/// The record's result: the hit, if any, and the ray's length (`+20`),
/// shortened to the nearest actor or furniture entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sight {
    pub hit: Option<SightHit>,
    pub length: u16,
}

/// `0043ADA9..0043AF96`: the ray from `origin` toward `target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SightRay {
    pub origin: [u16; 3],
    pub delta: [i16; 3],
    /// The difference's squared length as `IMUL` sums it.
    pub squared: i32,
    /// `0043B230`'s normalized difference.
    pub direction: [f32; 3],
    /// `00490370` of the direction, rebuilding Y.
    pub basis: NativeBasis,
}

impl SightRay {
    pub(crate) fn new(origin: [u16; 3], target: [u16; 3]) -> Self {
        let delta: [i16; 3] =
            std::array::from_fn(|axis| target[axis].wrapping_sub(origin[axis]) as i16);
        let squared = delta.iter().fold(0i32, |sum, &word| {
            sum.wrapping_add(i32::from(word).wrapping_mul(i32::from(word)))
        });
        let direction = direction(delta, squared);
        // 0043B2B0: (Z product + Y product) + X product, stored as f32.
        let [x, y, z] = direction.map(f64::from);
        let [ux, uy, uz] = WORLD_UP.map(f64::from);
        let dot = ((uz * z + uy * y) + ux * x) as f32;
        let up = if !(-STEEP..=STEEP).contains(&dot) {
            WORLD_X
        } else {
            WORLD_UP
        };
        let mut basis = NativeBasis::INITIAL;
        orient_from_forward(&mut basis, direction, up, false);
        Self {
            origin,
            delta,
            squared,
            direction,
            basis,
        }
    }

    /// The basis's Z row: the direction as `00490370` stored it.
    fn forward(&self) -> [f64; 3] {
        std::array::from_fn(|axis| f64::from(f32::from_bits(self.basis.words[7 + axis])))
    }

    /// `origin + forward * distance`, each product truncated (`00511BA8`)
    /// and added as a WORD; the distance is sign-extended from its WORD.
    fn point_at(&self, distance: u16) -> [u16; 3] {
        let distance = f64::from(distance as i16);
        let forward = self.forward();
        std::array::from_fn(|axis| {
            (ftol(distance * forward[axis]) as u16).wrapping_add(self.origin[axis])
        })
    }
}

/// `0043B230`: the squared length is exact as a zero-extended QWORD; the
/// root stays in a register and divides each sign-extended WORD. A zero root
/// gives (1, 0, 0). The root it also returns is discarded by `0043ADFE`.
fn direction(delta: [i16; 3], squared: i32) -> [f32; 3] {
    let length = f64::from(squared as u32).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    delta.map(|word| (f64::from(word) / length) as f32)
}

/// MSVCRT `__ftol` (`00511BA8`): truncation to a QWORD, of which callers
/// keep the low DWORD.
fn ftol(value: f64) -> i32 {
    let value = value.trunc();
    if value.is_finite()
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        value as i64 as i32
    } else {
        0
    }
}

/// The actor list and what `0047D080`'s model values read.
#[derive(Clone, Copy)]
pub(crate) struct SightActors<'a> {
    pub arena: &'a ObjectArena,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: inf_formats::FrontendExecutable<'a>,
}

pub(crate) struct WorldSightQuery<'a> {
    pub residency: &'a ResidentRuntime,
    /// [`ProjectileGeometryOwners::model_fallback`](crate::world_projectile_collision::ProjectileGeometryOwners).
    pub model_fallback: Option<&'a ResidentRuntime>,
    pub source: &'a WorldTerrainSource,
    pub terrain: &'a TerrainRuntime,
    pub model_process: &'a ModelProcessOwners,
    pub process: &'a mut ProcessFurnitureQueryState,
    pub furniture: &'a FurnitureDrawState,
}

impl WorldSightQuery<'_> {
    /// `0043AD90`.
    pub(crate) fn sight(
        &mut self,
        actors: SightActors<'_>,
        origin: [u16; 3],
        target: [u16; 3],
    ) -> Result<Sight, SightError> {
        let ray = SightRay::new(origin, target);
        let sight = self.actor_pass(&ray, actors)?;
        self.march(&ray, sight)
    }

    /// `0043B2E0`: each actor with flag 4 whose centre lies nearer than the
    /// best so far, ahead of the origin and within its radius `+40` of the
    /// ray, has its model tested over the ray up to its far side. A hit
    /// becomes the best and sets the distance to its entry. The pass returns
    /// that distance, or the root of the best square when there is none (or
    /// it is zero).
    pub(crate) fn actor_pass(
        &mut self,
        ray: &SightRay,
        actors: SightActors<'_>,
    ) -> Result<Sight, SightError> {
        let forward = ray.forward();
        let mut best = ray.squared as u32;
        let mut distance = 0x10000i32;
        let mut hit = None;
        for &actor in actors.arena.actor_list() {
            if actors.arena.object(actor)?.base.flags & 4 == 0 {
                continue;
            }
            let position = actors
                .arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))?;
            let delta: [i16; 3] =
                std::array::from_fn(|axis| position[axis].wrapping_sub(ray.origin[axis]) as i16);
            let [dx, dy, dz] = delta.map(i32::from);
            let squared = dx
                .wrapping_mul(dx)
                .wrapping_add(dy.wrapping_mul(dy))
                .wrapping_add(dz.wrapping_mul(dz)) as u32;
            if squared >= best {
                continue;
            }
            // (Y product + Z product) + X product, truncated.
            let along = ftol(
                (f64::from(dy) * forward[1] + f64::from(dz) * forward[2])
                    + f64::from(dx) * forward[0],
            );
            if along <= 0 {
                continue;
            }
            let radius = i32::from(actors.arena.actor(actor)?.rotating.visual.radius_word as i16);
            let lateral = squared.wrapping_sub(along.wrapping_mul(along) as u32);
            if lateral >= radius.wrapping_mul(radius) as u32 {
                continue;
            }
            let entry = ProjectileGeometryOwners {
                residency: self.residency,
                model_fallback: self.model_fallback,
                model_process: self.model_process,
                furniture_query: self.process,
            }
            .actor_ray(
                ObjectModelCallbacks {
                    arena: actors.arena,
                    source: actor,
                    animation: actors.animation,
                    executable: actors.executable,
                },
                ray.origin,
                RayRequest {
                    basis: ray.basis,
                    length: along.wrapping_add(radius),
                },
            );
            let entry = match entry {
                // An actor whose model is not resident (world 1's ships: only
                // levels 25 and 28 register their slot 665, and 0047D080
                // would read NULL) stops for the sight test alone, logged
                // once and passed after, as the contact passes stop it; the
                // aim goes on (it stopped the player's vehicle).
                Err(ProjectileGeometryError::MissingModel { actor, slot }) => {
                    stop_sight_once(actor, slot);
                    continue;
                }
                entry => entry?,
            };
            if let Some(left) = entry {
                distance = radius.wrapping_sub(left).wrapping_add(along);
                best = squared;
                hit = Some(actor);
            }
        }
        let Some(actor) = hit else {
            return Ok(Sight {
                hit: None,
                length: isqrt_48efc0(best as i32) as u16,
            });
        };
        let length = if distance != 0 {
            distance as u16
        } else {
            isqrt_48efc0(best as i32) as u16
        };
        Ok(Sight {
            hit: Some(SightHit::Actor {
                point: ray.point_at(distance as u16),
                actor,
            }),
            length,
        })
    }

    /// `0043AFAC..0043B21D`: the march from the actor pass's result. Each
    /// 64-unit sample (truncated per axis) whose cell differs from the last
    /// scans the furniture around it, first the whole square and then the
    /// new edges, and takes the highest of the cell's four corners. A sample
    /// under that corner reads the terrain's height there; at or above the
    /// sample it is a terrain hit. The march stops at the length.
    pub(crate) fn march(&mut self, ray: &SightRay, mut sight: Sight) -> Result<Sight, SightError> {
        if sight.length == 0 {
            return Ok(sight);
        }
        let current = self
            .residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        let sampling = self.terrain.sampling();
        let units = sampling.spacing.units();
        let grid_mask = !((units - 1) >> 7);
        let step = if units - 1 == 0x7f { 1 } else { 2 };
        let shift = sampling.height_scale.shift();
        let height = |x: u16, z: u16| {
            i32::from(self.terrain.cell_at_grid([x, z]).height_i8()).wrapping_shl(shift)
        };
        let x_edges = if ray.delta[0] > 0 { 0x06 } else { 0x0a };
        let z_edges = if ray.delta[2] > 0 { 0x22 } else { 0x42 };
        let mut flags = 1u32;
        let (mut last_x, mut last_z) = (0u16, 0u16);
        let mut highest = 0i32;
        let mut along = 0i32;
        loop {
            let t = f64::from(along as i16);
            let sample: [u16; 3] = std::array::from_fn(|axis| {
                (ftol(t * f64::from(ray.direction[axis])) as u16).wrapping_add(ray.origin[axis])
            });
            let cell_x = (sample[0] >> 7) & grid_mask;
            let cell_z = (sample[2] >> 7) & grid_mask;
            if cell_z != last_z {
                flags |= z_edges;
            }
            if cell_x != last_x {
                flags |= x_edges;
            }
            if flags != 0 {
                if self.scan_furniture(ray, &mut sight, cell_x, cell_z, flags)? {
                    return Ok(sight);
                }
                let x1 = (cell_x + step) & 0x1ff;
                let z1 = (cell_z + step) & 0x1ff;
                highest = height(cell_x, cell_z)
                    .max(height(x1, cell_z))
                    .max(height(x1, z1))
                    .max(height(cell_x, z1));
                (last_x, last_z) = (cell_x, cell_z);
                flags = 0;
            }
            if (sample[1] as i16) < highest as i16
                && self.terrain.height_at([sample[0], sample[2]]) >= sample[1] as i16
            {
                sight.hit = Some(SightHit::Terrain { point: sample });
                return Ok(sight);
            }
            along += SAMPLE;
            if along >= i32::from(sight.length) {
                return Ok(sight);
            }
        }
    }

    /// `0043A990`: the furniture around a cell, four cells each way (`R`
    /// from `005372F4`) in steps of one cell. Flag 1 scans the whole square,
    /// x outermost; otherwise flag 2 scans the column at `+R` (flag 4) or
    /// `-R`, then the row at `+R` (flag 20h) or `-R`. A furniture hit ends
    /// the scan with its point along the shortened ray.
    fn scan_furniture(
        &mut self,
        ray: &SightRay,
        sight: &mut Sight,
        cell_x: u16,
        cell_z: u16,
        flags: u32,
    ) -> Result<bool, SightError> {
        let units = i32::from(self.terrain.sampling().spacing.units());
        let reach = (!(units - 1) >> 7) & 4;
        let step = if units == 0x80 { 1 } else { 2 };
        let wrap = |cell: u16, offset: i32| (i32::from(cell).wrapping_add(offset) & 0x1ff) as u16;
        let offsets = || (-reach..=reach).step_by(step);
        if flags & 1 != 0 {
            for dx in offsets() {
                for dz in offsets() {
                    self.probe_furniture(ray, sight, wrap(cell_x, dx), wrap(cell_z, dz))?;
                }
            }
        } else if flags & 2 != 0 {
            let x = wrap(cell_x, if flags & 4 != 0 { reach } else { -reach });
            for dz in offsets() {
                self.probe_furniture(ray, sight, x, wrap(cell_z, dz))?;
            }
            let z = wrap(cell_z, if flags & 0x20 != 0 { reach } else { -reach });
            for dx in offsets() {
                self.probe_furniture(ray, sight, wrap(cell_x, dx), z)?;
            }
        }
        let Some(SightHit::Furniture { point, .. }) = &mut sight.hit else {
            return Ok(false);
        };
        *point = ray.point_at(sight.length);
        Ok(true)
    }

    /// One cell of `0043A990`: its sparse record, or the fallback, names a
    /// placement whose model (skipped when NULL or of zero radius) runs the
    /// ray (`0043A7E0`).
    fn probe_furniture(
        &mut self,
        ray: &SightRay,
        sight: &mut Sight,
        x: u16,
        z: u16,
    ) -> Result<(), SightError> {
        let payload = if self.terrain.cell_at_grid([x, z]).has_sparse_record() {
            let (_, record) = self
                .terrain
                .sparse_record_at_grid([x, z])
                .ok_or(SightError::MissingSparseRecord { grid: [x, z] })?;
            TerrainSparsePayload {
                state: record.state,
                resource_lookup: record.resource_lookup,
            }
        } else {
            self.process.fallback_payload()
        };
        let lookup = payload.resource_lookup.raw();
        if lookup == 0 {
            return Ok(());
        }
        let placements = resident_placement_table(self.residency)?;
        let slot = placements
            .slots
            .get(usize::from(lookup))
            .ok_or(ResidentFurnitureResourceError::Lookup {
                lookup,
                entries: placements.slots.len(),
            })?
            .resource_word;
        let Some(binding) = resident_model_resource(self.residency, slot)? else {
            return Ok(());
        };
        if binding.model.header.raw_0e == 0 {
            return Ok(());
        }
        self.furniture_ray(ray, sight, [x, z], payload.state, &binding)
    }

    /// `0043A7E0`: the furniture stands at its cell's corner, at the cell's
    /// height. Its position enters the ray's frame through `00490A80`, its
    /// state's basis through `00490DC0`, and the model runs over the ray
    /// table with the zero values of `005220B8`, up to the current length.
    /// A hit shortens the length to the entry.
    fn furniture_ray(
        &mut self,
        ray: &SightRay,
        sight: &mut Sight,
        [x, z]: [u16; 2],
        state: u8,
        binding: &ResidentModelBinding<'_>,
    ) -> Result<(), SightError> {
        let shift = self.terrain.sampling().height_scale.shift();
        let height = i32::from(self.terrain.cell_at_grid([x, z]).height_i8()).wrapping_shl(shift);
        let position = [x << 7, height as u16, z << 7];
        let delta: [i16; 3] =
            std::array::from_fn(|axis| position[axis].wrapping_sub(ray.origin[axis]) as i16);
        let qualification = AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        // 0043A93B clears only the flag.
        let mut contact = CollisionContact::from_native_words([Some(0), None, None, None, None]);
        let result = query_auxiliary_ray(
            AuxiliaryRayRequest {
                offset: ray.basis.inverse_words(delta).map(i32::from),
                basis: ray.basis.compose(self.furniture.basis(state)),
                model: AuxiliaryModelSource {
                    model: binding.model,
                    qualification: &qualification,
                },
                values: AuxiliaryValueSource::Zero,
                ray: RayRequest {
                    basis: ray.basis,
                    length: i32::from(sight.length),
                },
                residency: Some(self.residency),
            },
            self.process,
            self.model_process,
            &mut contact,
        );
        //Existing RegisteredMarkUnwindApproximation contains the failed
        //feature while retaining pool bytes and graphics publication.
        if result
            .as_ref()
            .is_err_and(AuxiliaryModelError::is_required_feature_boundary)
        {
            if let ModelScratchDrawState::Stopped(scope) = self.model_process.scratch.state()? {
                self.model_process
                    .scratch
                    .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)?;
            }
        }
        result?;
        if contact.flag().is_some_and(|flag| flag != 0) {
            let left = contact
                .displacement()
                .ok_or(AuxiliaryModelError::RequiredContact { offset: 0x10 })?;
            sight.length = sight.length.wrapping_sub(left as u16);
            sight.hit = Some(SightHit::Furniture {
                point: [0; 3],
                position,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "world_sight_tests.rs"]
mod tests;
