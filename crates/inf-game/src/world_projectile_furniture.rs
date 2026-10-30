//! 00435CB0/00435D50: ordered terrain-cell scan and precise model-auxiliary
//! query used by shared particle furniture collision. Broadphase is never a hit.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    model_auxiliary::{
        query_auxiliary_direct, AuxiliaryDirectRequest, AuxiliaryModelError, AuxiliaryModelSource,
        AuxiliaryQueryShape,
    },
    model_process::ModelProcessOwners,
    model_scratch::{ModelScratchDrawState, ModelScratchError, RegisteredMarkUnwindApproximation},
    object_runtime::{resident_model_resource, CollisionContact, ObjectError},
    process_particles::impact::collision::ParticleFurnitureContact,
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    world_furniture_effect::{
        resident::{resident_placement_table, ResidentFurnitureResourceError},
        AuxiliaryRawSource, FurnitureQueryRecord, ProcessFurnitureQueryState,
    },
    ResidentRuntime,
};

#[derive(Debug, Error)]
pub enum ProjectileFurnitureError {
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
    #[error("00435DFF reached flagged terrain cell {grid:?} without its sparse record")]
    MissingSparseRecord { grid: [u16; 2] },
    #[error("00435E88 reached NULL furniture model slot {slot}")]
    MissingModel { slot: u16 },
    #[error("portable guard stopped nonprogressing00435CB0 WORD-offset scan for radius {radius}")]
    PortableNonProgressingFurnitureScan { radius: i32 },
}

/// The stable record identity is retained for diagnostics and reached services;
/// 00466CF0 itself passes only contact X/Z to 00436020, which reacquires its cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectileFurnitureHit {
    pub contact: ParticleFurnitureContact,
    pub record: FurnitureQueryRecord,
}

pub(crate) struct WorldProjectileFurnitureQuery<'a> {
    pub residency: &'a ResidentRuntime,
    pub source: &'a WorldTerrainSource,
    pub terrain: &'a TerrainRuntime,
    pub model_process: &'a ModelProcessOwners,
    pub process: &'a mut ProcessFurnitureQueryState,
    pub furniture: &'a FurnitureDrawState,
}

impl WorldProjectileFurnitureQuery<'_> {
    pub(crate) fn query(
        &mut self,
        center: [u16; 3],
        radius: i32,
        wave_clock: u32,
    ) -> Result<Option<ProjectileFurnitureHit>, ProjectileFurnitureError> {
        let mut output = CollisionContact::from_native_words([Some(0), None, None, None, None]);
        let mut last = None;
        //435CB0 tests signed low WORD offsets, while its upper bound adds512
        //AFTER sign extension. Large radii can therefore wrap without exiting.
        walk_cells(
            center,
            radius,
            self.terrain.sampling().spacing.units(),
            |xz| {
                self.probe_cell(
                    center,
                    xz,
                    radius as i16,
                    wave_clock,
                    &mut output,
                    &mut last,
                )?;
                Ok(output.flag() == Some(1))
            },
        )?;
        Ok(if output.flag().is_some_and(|flag| flag != 0) {
            last
        } else {
            None
        })
    }

    fn probe_cell(
        &mut self,
        center: [u16; 3],
        sample_xz: [u16; 2],
        radius: i16,
        _wave_clock: u32,
        output: &mut CollisionContact,
        last: &mut Option<ProjectileFurnitureHit>,
    ) -> Result<(), ProjectileFurnitureError> {
        //435DA4 acquires actual currentH12 for each probe. All reached model
        //callbacks have an immutable residency loan, so no resource is cloned.
        let current = self
            .residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        let mask = self.terrain.sampling().spacing.units() - 1;
        let grid = sample_xz.map(|word| (word >> 7) & !(mask >> 7));
        let (record, payload) = if self.terrain.cell_at_grid(grid).has_sparse_record() {
            let (id, record) = self
                .terrain
                .sparse_record_at_grid(grid)
                .ok_or(ProjectileFurnitureError::MissingSparseRecord { grid })?;
            (
                self.source.query_record(id),
                crate::terrain_runtime::TerrainSparsePayload {
                    state: record.state,
                    resource_lookup: record.resource_lookup,
                },
            )
        } else {
            (
                FurnitureQueryRecord::Fallback,
                self.process.fallback_payload(),
            )
        };
        let lookup = payload.resource_lookup.raw();
        //No lookup is an unchanged output return, not an unconditional miss.
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
        let binding = resident_model_resource(self.residency, slot)?;
        //435E79 selects retained00546B88 state>>4 basis before the radius gate.
        let basis = self.furniture.basis(payload.state);
        self.process.publish_probe_basis(payload.state);
        let binding = binding.ok_or(ProjectileFurnitureError::MissingModel { slot })?;
        let model_radius = binding.model.header.raw_0e;
        if model_radius == 0 {
            clear_flag(output);
            *last = None;
            return Ok(());
        }
        let xz = sample_xz.map(|word| word & !mask);
        let height = i32::from(self.terrain.cell_at(xz).height_i8())
            .wrapping_shl(self.terrain.sampling().height_scale.shift()) as u16;
        let origin = [xz[0], height, xz[1]];
        let relative = std::array::from_fn(|i| i32::from(center[i].wrapping_sub(origin[i]) as i16));
        if !model_sphere_admits(relative, radius, model_radius) {
            clear_flag(output);
            *last = None;
            return Ok(());
        }
        let qualification = AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        let result = query_auxiliary_direct(
            AuxiliaryDirectRequest {
                residency: Some(self.residency),
                values: crate::model_auxiliary::AuxiliaryValueSource::Required,
                model: AuxiliaryModelSource {
                    model: binding.model,
                    qualification: &qualification,
                },
                basis,
                relative_center: relative,
                shape: if radius == 0 {
                    AuxiliaryQueryShape::Point
                } else {
                    AuxiliaryQueryShape::Sphere {
                        radius: i32::from(radius),
                    }
                },
            },
            self.process,
            self.model_process,
            output,
        );
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
        *last = output
            .flag()
            .is_some_and(|flag| flag != 0)
            .then_some(ProjectileFurnitureHit {
                contact: ParticleFurnitureContact { xz },
                record,
            });
        Ok(())
    }
}

fn clear_flag(output: &mut CollisionContact) {
    let mut words = output.native_words();
    words[0] = Some(0);
    *output = CollisionContact::from_native_words(words);
}

///435F31 uses signed JLE, unlike the unsigned actor47CEB0 broadphase. Radius
///is a signedWORD and all square/add operations retain their lowDWORD.
fn model_sphere_admits(relative: [i32; 3], radius: i16, model_radius: u16) -> bool {
    let combined = i32::from(radius).wrapping_add(i32::from(model_radius));
    let square = relative.into_iter().fold(0_i32, |sum, value| {
        sum.wrapping_add(value.wrapping_mul(value))
    });
    square <= combined.wrapping_mul(combined)
}

fn walk_cells(
    center: [u16; 3],
    radius: i32,
    spacing: u16,
    mut probe: impl FnMut([u16; 2]) -> Result<bool, ProjectileFurnitureError>,
) -> Result<(), ProjectileFurnitureError> {
    let lower = (-512_i32).wrapping_sub(radius);
    let upper = i32::from(radius as i16) + 512;
    let mut dx = lower;
    while i32::from(dx as i16) <= upper {
        let mut dz = lower;
        while i32::from(dz as i16) <= upper {
            if probe([
                center[0].wrapping_add(dx as u16),
                center[2].wrapping_add(dz as u16),
            ])? {
                return Ok(());
            }
            dz = dz.wrapping_add(i32::from(spacing));
            if dz as u16 == lower as u16 {
                return Err(
                    ProjectileFurnitureError::PortableNonProgressingFurnitureScan { radius },
                );
            }
        }
        dx = dx.wrapping_add(i32::from(spacing));
        if dx as u16 == lower as u16 {
            return Err(ProjectileFurnitureError::PortableNonProgressingFurnitureScan { radius });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
