//! One actual terrain/cache-miss composition for admitted class12/36 tails.
//! The slope service and47C1D0 reacquisition are distinct lazy phases. ACTIVE
//! query consumes the retained contextB/table owner; reached failure never clears
//! the8000 guard or supplies a cache result.

use crate::{
    model_auxiliary::{query_auxiliary_pair, AuxiliaryModelSource, AuxiliaryPairRequest},
    model_process::ModelProcessOwners,
    object_runtime::{
        CollisionContact, FamilyCollisionGuardError, FamilyCollisionGuardHost,
        FamilyCollisionGuardRequest,
    },
    world_furniture_effect::AuxiliaryRawSource,
};
use crate::{
    object_runtime::{
        collide_furniture_prefix, resident_model_resource, ActorTerrainHost,
        FamilyTerrainProbeMissHost, FamilyUpdateError, FurnitureCollisionPair,
        FurnitureCollisionPairState, FurnitureCollisionPrefixError, FurnitureCollisionPrefixHost,
        FurnitureCollisionPrefixRequest, ObjectArena, ObjectError, ObjectId, ResidentModelBinding,
    },
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::{
        ProcessFurnitureRadius, TerrainRuntime, TerrainSpacing, TerrainSparseRecordId,
    },
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    world_furniture_effect::{
        resident::{resident_placement_table, ResidentFurnitureResourceError},
        FurnitureQueryRecord, LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState,
    },
    ResidentRuntime,
};
use inf_formats::{native_basis::NativeBasis, MissionPlacementSlot, TerrainResourceLookup};
use std::{convert::Infallible, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResidentFurnitureCollisionError {
    #[error(transparent)]
    Scratch(#[from] crate::model_scratch::ModelScratchError),
    #[error(transparent)]
    Auxiliary(#[from] crate::model_auxiliary::AuxiliaryModelError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error(transparent)]
    Placement(#[from] ResidentFurnitureResourceError),
    #[error("required actual actor virtual48 at0047C493: {0}")]
    BasisClass(FamilyUpdateError<Infallible>),
    #[error("required actual actor collision continuation0047BFD0; pair {state:?}")]
    RequiredContactResponse {
        state: FurnitureCollisionPairState,
        contact: CollisionContact,
    },
}

#[derive(Debug, Error)]
pub enum WorldActorCollisionError {
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error("reached furniture collision prefix failed: {0}")]
    Collision(FurnitureCollisionPrefixError<ResidentFurnitureCollisionError>),
}

///Short loan of actual prepared terrain and retained process owners. No second
///fallback, process scalar snapshot, pool or per-query furniture bases exists.
pub(crate) struct WorldActorCollisionHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub source: &'a WorldTerrainSource,
    pub prepared: &'a TerrainRuntime,
    pub radius: &'a ProcessFurnitureRadius,
    pub process: &'a mut ProcessFurnitureQueryState,
    pub model_process: &'a ModelProcessOwners,
    pub furniture: &'a FurnitureDrawState,
    pub shared_basis: &'a NativeBasis,
}

impl ActorTerrainHost for WorldActorCollisionHost<'_> {
    type Error = WorldActorCollisionError;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        //The required450BD0 source at429EE0 precedes guard/XYZ publication.
        let current = self
            .residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        Ok(self.prepared)
    }
}
impl FamilyTerrainProbeMissHost for WorldActorCollisionHost<'_> {
    type Error = WorldActorCollisionError;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        <Self as ActorTerrainHost>::terrain(self, arena, actor)
    }
    fn collide(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error> {
        #[cfg(test)]
        let observation = crate::session_platform::begin_collision_prefix_observation(
            arena,
            actor,
            self.residency,
            self.source,
            self.prepared,
            self.process,
        );
        let mut resident = ResidentCollisionHost {
            residency: self.residency,
            source: self.source,
            prepared: self.prepared,
            furniture: self.furniture,
            shared_basis: self.shared_basis,
            model_process: self.model_process,
        };
        let result = collide_furniture_prefix(
            arena,
            FurnitureCollisionPrefixRequest {
                actor,
                process: self.process,
                radius: self.radius,
                exit_registration: LiteralRetExitRegistrationApproximation,
            },
            &mut resident,
        )
        .map_err(WorldActorCollisionError::Collision);
        #[cfg(test)]
        if result.is_ok() {
            crate::session_platform::finish_collision_prefix_observation(
                observation,
                arena,
                self.process,
            );
        }
        result
    }
}

///The captured source is immutable loan identity, not a second prepared grid.
#[derive(Clone, Copy)]
struct CapturedTerrain<'a> {
    source: &'a WorldTerrainSource,
    terrain: &'a TerrainRuntime,
}
struct ResidentCollisionHost<'a> {
    residency: &'a ResidentRuntime,
    source: &'a WorldTerrainSource,
    prepared: &'a TerrainRuntime,
    furniture: &'a FurnitureDrawState,
    shared_basis: &'a NativeBasis,
    model_process: &'a ModelProcessOwners,
}
impl<'a> FurnitureCollisionPrefixHost for ResidentCollisionHost<'a> {
    type Error = ResidentFurnitureCollisionError;
    type Terrain = CapturedTerrain<'a>;
    type Model = ResidentModelBinding<'a>;
    fn acquire_terrain(&mut self) -> Result<Option<Self::Terrain>, Self::Error> {
        //47C1E7 canonical H12 virtual0 can returnNULL. Header/process zero
        //exits occur before that captured pointer's first cell dereference.
        let Some(current) = self.residency.resource(12, 0) else {
            return Ok(None);
        };
        if !self.source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        Ok(Some(CapturedTerrain {
            source: self.source,
            terrain: self.prepared,
        }))
    }
    fn terrain(&self, source: Self::Terrain) -> Result<&TerrainRuntime, Self::Error> {
        //All mutation-capable services are outside this immutable residency
        //loan. Never reacquire a new H12 for a captured native pointer.
        Ok(source.terrain)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.prepared.sampling().spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.prepared.sampling().spacing
    }
    fn height_shift(&mut self) -> u32 {
        self.prepared.sampling().height_scale.shift()
    }
    fn model(&mut self, slot: u16) -> Result<Option<Self::Model>, Self::Error> {
        resident_model_resource(self.residency, slot)
            .map_err(ResidentFurnitureCollisionError::Object)
    }
    fn model_radius(&self, model: &Self::Model) -> u16 {
        model.model.header.raw_0e
    }
    fn placement_model(&mut self, lookup: TerrainResourceLookup) -> Result<u16, Self::Error> {
        Ok(resident_placement_slot(self.residency, lookup)?.resource_word)
    }
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        admitted_actor_basis(arena, actor)
    }
    fn shared_basis(&mut self) -> NativeBasis {
        *self.shared_basis
    }
    fn furniture_basis(&mut self, state: u8) -> NativeBasis {
        self.furniture.basis(state)
    }
    fn qualify_record(
        &self,
        terrain: Self::Terrain,
        record: TerrainSparseRecordId,
    ) -> FurnitureQueryRecord {
        terrain.source.query_record(record)
    }
    fn active_pair(
        &mut self,
        request: FurnitureCollisionPair<'_, Self::Model>,
        process: &mut ProcessFurnitureQueryState,
    ) -> Result<CollisionContact, Self::Error> {
        resident_active_pair(self.residency, self.model_process, request, process)
    }
    fn respond_contact(
        &mut self,
        arena: &mut ObjectArena,
        state: FurnitureCollisionPairState,
        contact: CollisionContact,
    ) -> Result<u8, Self::Error> {
        //Final10/12/36+A8=42A760; 34 and 35 (00521F54/005227E4) are47BFD0
        //directly. Do not substitute the family guard because a family
        //extension exists.
        let class = arena
            .family_update_class(state.actor)
            .map_err(ResidentFurnitureCollisionError::BasisClass)?;
        if matches!(
            class,
            crate::object_runtime::FamilyUpdateClass::Selector35
                | crate::object_runtime::FamilyUpdateClass::Selector34
        ) {
            return Err(ResidentFurnitureCollisionError::RequiredContactResponse {
                state,
                contact,
            });
        }
        let mut request = FamilyCollisionGuardRequest {
            actor: state.actor,
            continuation_operands: (state, contact),
        };
        match arena.dispatch_family_collision_guard(&mut request, self) {
            Ok(al) => Ok(al),
            Err(FamilyCollisionGuardError::Object(error)) => Err(error.into()),
            Err(FamilyCollisionGuardError::Host(error)) => Err(error),
        }
    }
}

impl FamilyCollisionGuardHost<(FurnitureCollisionPairState, CollisionContact)>
    for ResidentCollisionHost<'_>
{
    type Error = ResidentFurnitureCollisionError;
    fn continue_collision(
        &mut self,
        _: &mut ObjectArena,
        request: &mut FamilyCollisionGuardRequest<(FurnitureCollisionPairState, CollisionContact)>,
    ) -> Result<u32, Self::Error> {
        let (state, contact) = request.continuation_operands;
        Err(ResidentFurnitureCollisionError::RequiredContactResponse { state, contact })
    }
}

/// H11/0 entry behind a placement lookup: `resource_word` is the counterpart
/// model, and the low byte of `parameter_word` is entry byte +2, the kind read
/// by 0047BFD0 and 00436020.
pub(crate) fn resident_placement_slot(
    residency: &ResidentRuntime,
    lookup: TerrainResourceLookup,
) -> Result<MissionPlacementSlot, ResidentFurnitureCollisionError> {
    let table = resident_placement_table(residency)?;
    let slot = table.slots.get(usize::from(lookup.raw())).ok_or(
        ResidentFurnitureResourceError::Lookup {
            lookup: lookup.raw(),
            entries: table.slots.len(),
        },
    )?;
    Ok(slot.clone())
}

/// Exact final 10/12/34/35/36 tables share virtual +48 = 004026B0. Actual
/// shared field presence does not admit a different virtual callback.
pub(crate) fn admitted_actor_basis(
    arena: &ObjectArena,
    actor: ObjectId,
) -> Result<NativeBasis, ResidentFurnitureCollisionError> {
    arena
        .family_update_class(actor)
        .map_err(ResidentFurnitureCollisionError::BasisClass)?;
    Ok(arena.actor(actor)?.rotating.basis)
}

/// 004F2BE0 over the captured actor root and counterpart H8 models.
pub(crate) fn resident_active_pair(
    residency: &ResidentRuntime,
    model_process: &ModelProcessOwners,
    request: FurnitureCollisionPair<'_, ResidentModelBinding<'_>>,
    process: &mut ProcessFurnitureQueryState,
) -> Result<CollisionContact, ResidentFurnitureCollisionError> {
    let qualify = |source: &ResidentModelBinding<'_>| AuxiliaryRawSource {
        registration: source.registration.clone(),
        allocation: Arc::downgrade(source.allocation),
        vertices_span: source.model.vertices_span,
    };
    let actor_source = qualify(request.actor_root);
    let counterpart_source = qualify(request.counterpart);
    let mut contact = CollisionContact::unwritten();
    let result = query_auxiliary_pair(
        AuxiliaryPairRequest {
            residency: Some(residency),
            actor_origin: request.state.relative_actor_origin,
            actor_basis: request.state.actor_basis,
            actor_model: AuxiliaryModelSource {
                model: request.actor_root.model,
                qualification: &actor_source,
            },
            counterpart: AuxiliaryModelSource {
                model: request.counterpart.model,
                qualification: &counterpart_source,
            },
        },
        process,
        model_process,
        &mut contact,
    );
    finish_auxiliary_contact(result, model_process, contact)
}

///Explicit per-actor containment policy, never an auxiliary88 return. Retire
///registered marks LIFO while keeping bytes, B/table/pose and contact prefix;
///the original required-feature error is still returned. Hard custody stays
///stopped and supersedes producer errors when mark recovery itself fails.
fn finish_auxiliary_contact(
    result: Result<(), crate::model_auxiliary::AuxiliaryModelError>,
    owners: &ModelProcessOwners,
    contact: CollisionContact,
) -> Result<CollisionContact, ResidentFurnitureCollisionError> {
    if let Err(error) = &result {
        if error.is_required_feature_boundary() {
            if let crate::model_scratch::ModelScratchDrawState::Stopped(scope) =
                owners.scratch.state()?
            {
                owners.scratch.recover_stopped_draw(
                    &scope,
                    crate::model_scratch::RegisteredMarkUnwindApproximation,
                )?;
            }
        }
    }
    result?;
    Ok(contact)
}
#[cfg(test)]
mod tests;
