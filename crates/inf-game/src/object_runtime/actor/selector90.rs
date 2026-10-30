//! Selector 90's owned cinematic helper, 00462660/00526B50.
//!
//! +24 = 00462BA0 is absent from the bulk decompile; the movement, arrival
//! writer and facing tail below follow the retail executable. Route targets
//! retain arena IDs rather than addresses into an unrelated placement array.

use inf_formats::native_basis::{normalize_words, orient_from_forward, NativeLengthPrecision};
use thiserror::Error;

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{
    NativeHeapRef, NativeTargetError, ObjectArena, ObjectError, ObjectId,
    WeightedTargetConstructorNull,
};

mod motion;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector90State {
    /// Signed radius/approach threshold consumed at 00462867/00462997.
    pub word_138: i32,
    /// Signed units per second consumed at 00462962.
    pub word_13c: i32,
    /// 00462BA0 writes 1 when stopped or when route advance returns false.
    pub byte_140: u8,
    pub byte_141: u8,
    pub byte_142: u8,
    /// Signed fractional position in thousandths of a raw position unit.
    pub remainders_144_14c: [i32; 3],
    pub route_150: Option<NativeHeapRef>,
    pub target_154: Option<NativeHeapRef>,
}

impl Selector90State {
    fn new() -> Self {
        Self {
            word_138: 256,
            word_13c: 0,
            byte_140: 1,
            byte_141: 0,
            byte_142: 1,
            remainders_144_14c: [0; 3],
            route_150: None,
            target_154: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector90Word {
    Approach138,
    Speed13c,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector90Byte {
    Arrival140,
    Mode142,
}

/// The actual zero/nonzero process DWORD 00538BC0 branch. The caller supplies
/// its owner; frame duration or coordinate scale cannot select this mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector90MotionMode {
    WholeUnits,
    FractionalThousandths,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selector90TickRequest {
    pub microseconds: u32,
    pub motion_mode: Selector90MotionMode,
    /// Live settings +7C passed to the shared 00482690 target refresh.
    pub settings_7c: u32,
    /// Native process vector 0055F280, consumed by 00490370.
    pub up: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Selector90Error {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("selector-90 helper {0:?} reached its target's unwritten focus +08")]
    TargetFocusUnknown(ObjectId),
    #[error("selector-90 native signed division would fault")]
    ArithmeticFault,
}

impl ObjectArena {
    /// Shared 0047A7A0 then the exact 00462660 scalar/owned-pointer writes.
    /// H13 admission belongs to construct_world_object, including resource 142
    /// used by 00462750; this method is allocation alone.
    pub fn allocate_selector90(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector90(Box::new(Selector90State::new()));
        Ok(object)
    }

    pub fn selector90(&self, object: ObjectId) -> Result<&Selector90State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector90(state) => Ok(state),
            _ => Err(ObjectError::NotSelector90(object)),
        }
    }

    fn selector90_mut(&mut self, object: ObjectId) -> Result<&mut Selector90State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector90(state) => Ok(state),
            _ => Err(ObjectError::NotSelector90(object)),
        }
    }

    pub fn write_selector90_word(
        &mut self,
        helper: ObjectId,
        field: Selector90Word,
        value: i32,
    ) -> Result<(), ObjectError> {
        let state = self.selector90_mut(helper)?;
        match field {
            Selector90Word::Approach138 => state.word_138 = value,
            Selector90Word::Speed13c => state.word_13c = value,
        }
        Ok(())
    }

    pub fn write_selector90_byte(
        &mut self,
        helper: ObjectId,
        field: Selector90Byte,
        value: u8,
    ) -> Result<(), ObjectError> {
        let state = self.selector90_mut(helper)?;
        match field {
            Selector90Byte::Arrival140 => state.byte_140 = value,
            Selector90Byte::Mode142 => state.byte_142 = value,
        }
        Ok(())
    }

    /// 00462CC0 releases the previous allocation before replacement. Its raw
    /// +150 ref survives until the new target/NULL store; a constructor error
    /// leaves that released ref and the new pending allocation. Common object
    /// pointers are accepted here; route-node motion remains a separate policy.
    /// This does not reset +140 or the fractional accumulators.
    pub fn replace_selector90_route(
        &mut self,
        helper: ObjectId,
        route: Option<ObjectId>,
        constructor_null: WeightedTargetConstructorNull<'_>,
    ) -> Result<(), NativeTargetError> {
        if let Some(old) = self.selector90(helper)?.route_150 {
            self.release_actor_service(old)?;
        }
        let target = if let Some(route) = route {
            // 00462CC0 accepts the common object pointer. Its constructor does
            // not require the separate route-node movement policy.
            self.object_position_target_resource(route, constructor_null)?
        } else {
            None
        };
        self.selector90_mut(helper)?.route_150 = target;
        Ok(())
    }

    /// 00462D40 only destroys/replaces the owned target. Its caller constructs
    /// a tracked 004825D0 target through follow_target with ResolveActor.
    pub fn replace_selector90_target(
        &mut self,
        helper: ObjectId,
        target: Option<NativeHeapRef>,
    ) -> Result<(), ObjectError> {
        if let Some(old) = self.selector90(helper)?.target_154 {
            self.release_actor_service(old)?;
        }
        self.selector90_mut(helper)?.target_154 = target;
        Ok(())
    }

    /// Retail +24 = 00462BA0. Route movement precedes target refresh/facing.
    /// Reached mutations survive an error at a later unsupported data read.
    pub fn tick_selector90(
        &mut self,
        helper: ObjectId,
        request: Selector90TickRequest,
    ) -> Result<(), Selector90Error> {
        if self.selector90(helper)?.byte_140 != 0
            || !self.advance_selector90_route(helper, request.microseconds, request.motion_mode)?
        {
            self.selector90_mut(helper)?.byte_140 = 1;
            self.actor_mut(helper)?.words_8e_92 = [0; 3];
        }
        if let Some(reference) = self.selector90(helper)?.target_154 {
            // 00462BEE: the target's +18, a follow target's refresh or the
            // fixed kinds' bare 4 (the ending director's bug looks at a node).
            let mut target = self.native_target(reference)?;
            let refreshed = self.refresh_native_target(&mut target, request.settings_7c);
            self.write_native_target(reference, target)?;
            refreshed?;
            let focus = target
                .raw_focus_08_words()
                .ok_or(Selector90Error::TargetFocusUnknown(helper))?;
            let position = self
                .object(helper)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(helper))?;
            // 00462C13..00462C83: signed WORD delta, retained Z²+Y²+X²,
            // then FSQRT spilled to f32. Route movement retains its divisor.
            let forward = normalize_words(
                motion::delta(focus, position),
                NativeLengthPrecision::Spilled,
            );
            orient_from_forward(
                &mut self.actor_mut(helper)?.rotating.basis,
                forward,
                request.up,
                false,
            );
        }
        Ok(())
    }

    fn route_position(&self, route: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.route_node(route)?;
        self.object(route)?
            .base
            .position_words
            .ok_or(ObjectError::RoutePositionUnknown(route))
    }
}
