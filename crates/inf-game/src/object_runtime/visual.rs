//! Visual prefix initialization and model replacement are different native
//! policies (0047A050 and 0047A110). Neither enables flag 0x4 for a model.

use super::{
    initialize_base_context, mark_flag_change, BaseContext, BaseObject, ObjectArena, ObjectClass,
    ObjectError, ObjectId, ObjectState, VisualObject,
};
use crate::{HandlerResources, LoadedOverlay, ResidentRegistration, ResidentRuntime};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ModelMetadata {
    pub(crate) extent_word: u16,
    pub(crate) radius_word: u16,
}

impl ObjectArena {
    /// 0047A050: initialize the common context, reset +28 to 0x100, then
    /// optionally consume the WORD through context +8. A null pointer keeps
    /// the old model/radius; -1 clears flag 0x4 but also keeps the old radius.
    pub fn initialize_visual_context(
        &mut self,
        object: ObjectId,
        context: BaseContext,
        model_argument: Option<u32>,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state;
        let (base, visual) = visual_parts(state).ok_or(ObjectError::NotVisual(object))?;
        let model_word = model_argument.map(|argument| argument as u16);
        let metadata = model_word
            .map(|word| resident_model_metadata(runtime, word))
            .transpose()?
            .flatten();
        initialize_visual_fields(base, visual, context, model_word, metadata);
        Ok(())
    }

    /// Model command 2 (0047A1A0) marks dirty byte +39 bit0 even if unchanged,
    /// stores the low WORD, and invokes 0047A110's refresh policy.
    pub fn set_visual_model(
        &mut self,
        object: ObjectId,
        model_word: u16,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state;
        let (base, visual) = visual_parts(state).ok_or(ObjectError::NotVisual(object))?;
        let metadata = resident_model_metadata(runtime, model_word)?;
        set_visual_fields(base, visual, model_word, metadata);
        Ok(())
    }

    /// A direct +42 store, then 0047A110 (the ball's model by the team
    /// flag): no dirty bit, unlike command 2.
    pub fn store_visual_model_word(
        &mut self,
        object: ObjectId,
        model_word: u16,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state;
        let (_, visual) = visual_parts(state).ok_or(ObjectError::NotVisual(object))?;
        visual.model_word = model_word;
        self.refresh_visual_model(object, runtime)
    }

    /// 0047A110 alone: refresh metadata for the already-stored +42 WORD.
    /// Callers that write dirty/model fields separately (00443B10) retain
    /// their native order without invoking command 2's extra writes.
    pub fn refresh_visual_model(
        &mut self,
        object: ObjectId,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state;
        let (base, visual) = visual_parts(state).ok_or(ObjectError::NotVisual(object))?;
        let metadata = resident_model_metadata(runtime, visual.model_word)?;
        refresh_visual_fields(base, visual, metadata);
        Ok(())
    }
}

fn visual_parts(state: &mut ObjectState) -> Option<(&mut BaseObject, &mut VisualObject)> {
    let visual = match &mut state.class {
        ObjectClass::Base
        | ObjectClass::Selector81(_)
        | ObjectClass::Selector94(_)
        | ObjectClass::Selector75(_)
        | ObjectClass::Selector86(_)
        | ObjectClass::Selector67(_)
        | ObjectClass::MapAnnotation(_) => return None,
        ObjectClass::Visual(visual)
        | ObjectClass::TerrainLight { visual, .. }
        | ObjectClass::RouteNode { visual, .. } => visual,
        ObjectClass::Selector22(state) => &mut state.visual,
        ObjectClass::Selector65(state) => &mut state.visual,
        ObjectClass::Selector50(state) => &mut state.visual,
        ObjectClass::Selector48(state) => &mut state.visual,
        ObjectClass::Selector43(state) => &mut state.visual,
        ObjectClass::Selector42(state) => &mut state.visual,
        ObjectClass::Selector64(gate) => &mut gate.rotating.visual,
        ObjectClass::RotatingVisual(rotating)
        | ObjectClass::RotatingTerrainLight { rotating, .. } => &mut rotating.visual,
        ObjectClass::Selector58(pad) => &mut pad.rotating.visual,
        ObjectClass::Selector45(marker) => &mut marker.rotating.visual,
        ObjectClass::Selector46(territory) => &mut territory.rotating.visual,
        ObjectClass::Race(race) => &mut race.rotating.visual,
        ObjectClass::Selector83(patch) => &mut patch.rotating.visual,
        ObjectClass::Selector76(state) => &mut state.rotating.visual,
        ObjectClass::Actor(actor) => &mut actor.rotating.visual,
        ObjectClass::TimedPickup(pickup) => &mut pickup.visual,
    };
    Some((&mut state.base, visual))
}

fn initialize_visual_fields(
    base: &mut BaseObject,
    visual: &mut VisualObject,
    context: BaseContext,
    model_word: Option<u16>,
    metadata: Option<ModelMetadata>,
) {
    initialize_base_context(base, context);
    base.word_28 = 0x100;
    let Some(word) = model_word else { return };
    visual.model_word = word;
    if word == u16::MAX {
        mark_flag_change(base, base.flags & !4);
    } else if let Some(metadata) = metadata {
        visual.radius_word = metadata.radius_word;
        if visual.radius_word == 0 {
            mark_flag_change(base, base.flags & !4);
        }
        base.word_28 = if metadata.extent_word == 0 {
            0x100
        } else {
            metadata.extent_word
        };
    }
}

/// 0047A110 differs from initial context: -1 resets the radius, and every
/// path checks the resulting/retained radius before possibly clearing flag4.
fn refresh_visual_fields(
    base: &mut BaseObject,
    visual: &mut VisualObject,
    metadata: Option<ModelMetadata>,
) {
    if visual.model_word == u16::MAX {
        base.word_28 = 0x100;
        visual.radius_word = 0;
    } else if let Some(metadata) = metadata {
        visual.radius_word = metadata.radius_word;
        base.word_28 = if metadata.extent_word == 0 {
            0x100
        } else {
            metadata.extent_word
        };
    }
    if visual.radius_word == 0 {
        mark_flag_change(base, base.flags & !4);
    }
}

/// The model command's mutation order is separate from initial binding.
fn set_visual_fields(
    base: &mut BaseObject,
    visual: &mut VisualObject,
    model_word: u16,
    metadata: Option<ModelMetadata>,
) {
    base.dirty_word |= 0x100;
    visual.model_word = model_word;
    refresh_visual_fields(base, visual, metadata);
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResidentModelBinding<'a> {
    pub(crate) registration: &'a ResidentRegistration,
    pub(crate) allocation: &'a Arc<LoadedOverlay>,
    pub(crate) model: &'a inf_formats::ModelEntry,
}

///Actual H8 inclusive-bound/NULL loader policy shared by metadata and queries.
pub(crate) fn resident_model_resource(
    runtime: &ResidentRuntime,
    model_word: u16,
) -> Result<Option<ResidentModelBinding<'_>>, ObjectError> {
    if (model_word as i16) < 0 {
        return Ok(None);
    }
    let capacity = runtime.resource_capacity(8).unwrap_or(0);
    let slot = usize::from(model_word);
    if slot > capacity {
        return Ok(None);
    }
    if slot == capacity {
        return Err(ObjectError::ModelSlotBoundary {
            slot: model_word,
            capacity,
        });
    }
    let Some(resource) = runtime.resource(8, model_word) else {
        return Ok(None);
    };
    let model = match resource
        .overlay
        .handlers()
        .get(resource.registration.region_index)
    {
        Some(HandlerResources::Models(bank)) => bank
            .entries
            .get(usize::from(resource.registration.local_index)),
        _ => None,
    }
    .ok_or(ObjectError::ModelRegistration { slot: model_word })?;
    Ok(Some(ResidentModelBinding {
        registration: resource.registration,
        allocation: resource.overlay,
        model,
    }))
}

pub(crate) fn resident_model_metadata(
    runtime: &ResidentRuntime,
    model_word: u16,
) -> Result<Option<ModelMetadata>, ObjectError> {
    resident_model_resource(runtime, model_word).map(|binding| {
        binding.map(|binding| ModelMetadata {
            extent_word: binding.model.header.visibility_extent as u16,
            radius_word: binding.model.header.raw_0e,
        })
    })
}

#[cfg(test)]
mod tests;
