//! Real current H12/H11/EXE and retained Target284 loans for468E30.
use super::WorldFamilyBehaviorError;
use crate::{
    object_runtime::{
        FamilyGrantGridHost, FamilyGrantGridPointRequest, FamilyTargetSlot, ObjectArena, ObjectId,
        WeightedTargetConstructorNull, WeightedTargetReferent, WeightedTargetRequest,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSpacing},
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    HandlerResources, ResidentRuntime,
};
use inf_formats::FrontendExecutable;

//Only an authenticated current resource acquisition creates this token. The
//actual backing remains the retained WorldTerrainSource/prepared-terrain loan.
#[derive(Clone, Copy)]
pub(crate) struct PreparedTerrainToken;
pub(crate) struct WorldGrantGridHost<'a, 'exe> {
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub source: &'a WorldTerrainSource,
    pub prepared: &'a TerrainRuntime,
    pub process: &'a ParticleFactoryProcessState,
}
impl FamilyGrantGridHost for WorldGrantGridHost<'_, '_> {
    type Error = WorldFamilyBehaviorError;
    type Terrain = PreparedTerrainToken;
    fn acquire_terrain(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<Option<Self::Terrain>, Self::Error> {
        let Some(current) = self.residency.resource(12, 0) else {
            return Ok(None);
        };
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        Ok(Some(PreparedTerrainToken))
    }
    fn terrain(&self, _source: Self::Terrain) -> Result<&TerrainRuntime, Self::Error> {
        Ok(self.prepared)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.prepared.sampling().spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.prepared.sampling().spacing
    }
    fn height_scale(&mut self) -> TerrainHeightScale {
        self.prepared.sampling().height_scale
    }
    fn placement_kind(&mut self, lookup: u8) -> Result<u8, Self::Error> {
        let source =
            self.residency
                .resource(11, 0)
                .ok_or(WorldFamilyBehaviorError::RequiredResource {
                    handler: 11,
                    slot: 0,
                    phase: "436BC0 H11",
                })?;
        let Some(HandlerResources::MissionPlacements(bank)) = source
            .overlay
            .handlers()
            .get(source.registration.region_index)
        else {
            return Err(WorldFamilyBehaviorError::ResourceCustody {
                handler: 11,
                slot: 0,
                phase: "436BC0 H11",
                reason: "registration selects another handler",
            });
        };
        let table = bank
            .entries
            .get(usize::from(source.registration.local_index))
            .ok_or(WorldFamilyBehaviorError::ResourceCustody {
                handler: 11,
                slot: 0,
                phase: "436BC0 H11",
                reason: "registration exceeds its bank",
            })?;
        let row = table.slots.get(usize::from(lookup)).ok_or(
            WorldFamilyBehaviorError::ResourceCustody {
                handler: 11,
                slot: 0,
                phase: "436BC0 H11",
                reason: "lookup exceeds retained rows",
            },
        )?;
        Ok(row.parameter_word as u8)
    }
    fn furniture_grant_id(&mut self, kind: u8) -> Result<u16, Self::Error> {
        let span = self
            .executable
            .mapped(0x0053_2f88 + u32::from(kind) * 16 + 12, 2)?;
        Ok(u16::from_le_bytes(
            span.bytes.try_into().expect("exact WORD"),
        ))
    }
    fn publish_grid_point(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilyGrantGridPointRequest,
    ) -> Result<bool, Self::Error> {
        Ok(arena.publish_family_target(
            request.actor,
            FamilyTargetSlot::Target284,
            WeightedTargetRequest {
                constructor_null: WeightedTargetConstructorNull::Process(self.process),
                referent: WeightedTargetReferent::Point(request.position_words),
                score: request.score,
            },
        )?)
    }
}
