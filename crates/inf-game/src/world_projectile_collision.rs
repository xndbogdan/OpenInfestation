//! Actual 0047CEB0 actor model contact for swept particle queries. The manager
//! broadphase is a separate owner; this query rereads the retained actor pose.
//! The sight test's ray against an actor's model (0047D080) shares the owners.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    model_auxiliary::{
        query_auxiliary_direct, query_auxiliary_ray, ray::RayRequest, AuxiliaryDirectRequest,
        AuxiliaryModelError, AuxiliaryModelSource, AuxiliaryQueryShape, AuxiliaryRayRequest,
        AuxiliaryValueSource,
    },
    model_process::ModelProcessOwners,
    model_scratch::{ModelScratchDrawState, ModelScratchError, RegisteredMarkUnwindApproximation},
    object_runtime::{
        resident_model_resource, CollisionContact, ObjectError, ObjectId, ObjectModelCallbacks,
    },
    world_furniture_effect::{AuxiliaryRawSource, ProcessFurnitureQueryState},
    ResidentRuntime,
};

#[derive(Debug, Error)]
pub enum ProjectileGeometryError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Auxiliary(#[from] AuxiliaryModelError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error("0047CEB0 reached NULL model slot {slot} for actor {actor:?}")]
    MissingModel { actor: ObjectId, slot: u16 },
}

pub(crate) struct ProjectileGeometryOwners<'a> {
    pub residency: &'a ResidentRuntime,
    /// A second residency for actor models the world's own does not
    /// register (Pelagia's ships borrow the Command Centre's 665).
    pub model_fallback: Option<&'a ResidentRuntime>,
    pub model_process: &'a ModelProcessOwners,
    pub furniture_query: &'a mut ProcessFurnitureQueryState,
}

impl<'a> ProjectileGeometryOwners<'a> {
    /// An actor's model: the world's own, else the borrowed fallback's.
    fn model(
        &self,
        slot: u16,
    ) -> Result<Option<crate::object_runtime::ResidentModelBinding<'a>>, ObjectError> {
        match resident_model_resource(self.residency, slot)? {
            Some(binding) => Ok(Some(binding)),
            None => match self.model_fallback {
                Some(fallback) => resident_model_resource(fallback, slot),
                None => Ok(None),
            },
        }
    }

    ///Unsigned inclusive cube/square gate uses signed LOWWORD differences.
    ///A point selects544BF0; a nonzero radius selects544C08 at the actual caller.
    pub(crate) fn actor_contact(
        &mut self,
        callbacks: ObjectModelCallbacks<'_>,
        center: [u16; 3],
        radius: i32,
    ) -> Result<bool, ProjectileGeometryError> {
        let arena = callbacks.arena;
        let actor = callbacks.source;
        let position = arena
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let relative =
            std::array::from_fn(|i| i32::from(center[i].wrapping_sub(position[i]) as i16));
        let visual = &arena.actor(actor)?.rotating.visual;
        let combined = i32::from(visual.radius_word).wrapping_add(radius);
        let limit = combined.wrapping_mul(2) as u32;
        let mut square = 0_i32;
        for component in relative {
            if combined.wrapping_add(component) as u32 > limit {
                return Ok(false);
            }
            square = square.wrapping_add(component.wrapping_mul(component));
        }
        if square as u32 > combined.wrapping_mul(combined) as u32 {
            return Ok(false);
        }
        let slot = visual.model_word;
        let binding = self
            .model(slot)?
            .ok_or(ProjectileGeometryError::MissingModel { actor, slot })?;
        let qualification = AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        let mut contact = CollisionContact::unwritten();
        let result = query_auxiliary_direct(
            AuxiliaryDirectRequest {
                residency: Some(self.residency),
                //47CEB0 captures this actor in527B18/+0=00478A40. Its getter
                //has the same clock-zero/virtual+14 policy as packet4789B0;
                //only the native wrapper's source offset differs (+4/+3C).
                values: AuxiliaryValueSource::Object(callbacks),
                model: AuxiliaryModelSource {
                    model: binding.model,
                    qualification: &qualification,
                },
                basis: arena.actor(actor)?.rotating.basis,
                relative_center: relative,
                shape: if radius == 0 {
                    AuxiliaryQueryShape::Point
                } else {
                    AuxiliaryQueryShape::Sphere { radius }
                },
            },
            self.furniture_query,
            self.model_process,
            &mut contact,
        );
        self.contain(&result)?;
        result?;
        Ok(contact.flag() == Some(1))
    }

    /// 0047D080: the ray against the actor's model. The actor's position
    /// enters the ray's frame as the WORD difference from `origin` through
    /// the ray's 00490A80, its basis through 00490DC0, and the model's
    /// program runs over the ray table 00544C20 with the actor's 00527B18
    /// values. A hit returns the result's +10: the ray's length left past
    /// the entry.
    pub(crate) fn actor_ray(
        &mut self,
        callbacks: ObjectModelCallbacks<'_>,
        origin: [u16; 3],
        ray: RayRequest,
    ) -> Result<Option<i32>, ProjectileGeometryError> {
        let arena = callbacks.arena;
        let actor = callbacks.source;
        let position = arena
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let delta = std::array::from_fn(|i| position[i].wrapping_sub(origin[i]) as i16);
        let offset = ray.basis.inverse_words(delta).map(i32::from);
        let pose = &arena.actor(actor)?.rotating;
        let slot = pose.visual.model_word;
        let binding = self
            .model(slot)?
            .ok_or(ProjectileGeometryError::MissingModel { actor, slot })?;
        let qualification = AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        // 0047D2CC clears only the flag before 004F2C40.
        let mut contact = CollisionContact::from_native_words([Some(0), None, None, None, None]);
        let result = query_auxiliary_ray(
            AuxiliaryRayRequest {
                offset,
                basis: ray.basis.compose(pose.basis),
                model: AuxiliaryModelSource {
                    model: binding.model,
                    qualification: &qualification,
                },
                values: AuxiliaryValueSource::Object(callbacks),
                ray,
                residency: Some(self.residency),
            },
            self.furniture_query,
            self.model_process,
            &mut contact,
        );
        self.contain(&result)?;
        result?;
        if contact.flag() != Some(1) {
            return Ok(None);
        }
        contact
            .displacement()
            .map(Some)
            .ok_or(AuxiliaryModelError::RequiredContact { offset: 0x10 }.into())
    }

    //Existing RegisteredMarkUnwindApproximation contains the failed
    //feature while retaining pool bytes and graphics publication. It is
    //not a successful native88 return and the error remains observable.
    fn contain(
        &self,
        result: &Result<(), AuxiliaryModelError>,
    ) -> Result<(), ProjectileGeometryError> {
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
        Ok(())
    }
}
