//! Selector 35: 0043D070/0043D1C0/0043D270/0043F760, vtable0052273C.
//! The shared 004293D0 prefix adds scalar state above 00428650. Constructor,
//! payload, commands and postinit leave active behavior +184 at -1. The
//! descriptor and movement modules own later dispatch and timed continuation;
//! construction does not supply the allocator-carried initial +19C timer.

use inf_formats::{
    native_basis::NativeBasisMath, InstanceCommandPayload, InstanceCommandTarget,
    InstanceCommandView,
};
use std::sync::Arc;

mod audio;
mod descriptor;
mod destruction;
mod lifecycle;
mod motion;
mod pickup;
mod tail;
pub use audio::Selector35PersistentAudioError;
pub use descriptor::{
    Selector35DescriptorContext, Selector35DescriptorError, Selector35DescriptorRequest,
    Selector35RouteProcessState,
};
pub use lifecycle::{
    Selector35LifecycleEffect, Selector35LifecycleError, Selector35LifecycleHost,
    Selector35LifecycleRequest,
};
pub use motion::{Selector35MotionControls, Selector35MotionError, Selector35MotionRequest};
pub use pickup::Selector35PickupError;
pub use tail::{Selector35ClearanceHost, Selector35TailError, Selector35TailRequest};

use super::family428650::Family428650Class;
use super::{DerivedActorConstructionRequest, DerivedActorError};
use crate::{
    object_runtime::{
        ActorCommandContext, ActorCommandError, ActorConstructionError, ActorHandle,
        ActorInitializationRequest, ObjectArena, ObjectId, RuntimeAllocation,
    },
    ResidentRuntime,
};

#[derive(Clone, Debug)]
pub struct RetainedSelector35Data {
    allocation: Arc<RuntimeAllocation>,
    pub allocation_offset: u32,
}
impl RetainedSelector35Data {
    pub fn allocation(&self) -> &Arc<RuntimeAllocation> {
        &self.allocation
    }
}
impl PartialEq for RetainedSelector35Data {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}
impl Eq for RetainedSelector35Data {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector35State {
    pub word_230: u32,
    pub word_234: u32,
    pub byte_238: u8,
    pub byte_239: u8,
    pub retained_23c: Option<RetainedSelector35Data>,
    /// Only +242/+243 are initialized. Code24 writes all four bytes, then
    /// explicitly clears +243 while preserving +242 from the input DWORD.
    pub bytes_240: [Option<u8>; 4],
    pub word_244: u32,
    pub word_248: u32,
    pub byte_250: u8,
    pub byte_251: Option<u8>,
    pub word_254: u32,
    pub word_258: Option<u32>,
    pub word_25c: Option<u32>,
    pub word_260: Option<u32>,
    pub word_264: u32,
    pub handle_268: ActorHandle,
    pub byte_26c: u8,
    /// Stable retail static-table identity, never a dereferenceable host pointer.
    pub table_270: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct Selector35InitializationRequest<'a> {
    pub actor: ActorInitializationRequest<'a>,
    pub allocation: &'a Arc<RuntimeAllocation>,
    pub payload_offset: u32,
}

impl ObjectArena {
    pub fn allocate_selector35_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family428650_actor(request, rng)?;
        let null = self
            .actor_handles()
            .expect("constructed actor registry")
            .null_handle();
        let word_260 = self
            .actor_allocation
            .initial_dword(super::super::allocation::ActorAllocationDwordCarry::Selector35Route260);
        self.family428650_actor_mut(object)?.class =
            Family428650Class::Selector35(Box::new(Selector35State {
                word_230: 500,
                word_234: 512,
                byte_238: 1,
                byte_239: 0,
                retained_23c: None,
                bytes_240: [None, None, Some(0), Some(0)],
                word_244: 0,
                word_248: 0,
                byte_250: 0,
                byte_251: None,
                word_254: 250,
                word_258: None,
                word_25c: None,
                word_260,
                word_264: 0,
                handle_268: null,
                byte_26c: 0,
                table_270: None,
            }));
        Ok(object)
    }

    pub fn selector35_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector35State, DerivedActorError> {
        match &self.family428650_actor(object)?.class {
            Family428650Class::Selector35(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector35(object)),
        }
    }
    pub(crate) fn selector35_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector35State, DerivedActorError> {
        match &mut self.family428650_actor_mut(object)?.class {
            Family428650Class::Selector35(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector35(object)),
        }
    }

    pub fn initialize_selector35_actor(
        &mut self,
        object: ObjectId,
        request: Selector35InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.selector35_actor(object)?;
        let payload = request
            .actor
            .payload
            .selector35
            .as_ref()
            .ok_or(DerivedActorError::MissingSelector35Payload)?;
        if request.payload_offset.checked_add(0x58) != Some(payload.retained_58_offset) {
            return Err(DerivedActorError::MissingSelector35Payload);
        }
        self.initialize_derived_actor(object, request.actor, runtime, math)?;
        let alternate = payload.dword_70.value != 0;
        {
            let state = self.selector35_actor_mut(object)?;
            state.byte_251 = Some(u8::from(alternate));
            state.table_270 = Some(if alternate { 0x0053_3d70 } else { 0x0053_3d68 });
        }
        self.set_selector35_retained(object, request.allocation, payload.retained_58_offset)?;
        let [x, y, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(DerivedActorError::MissingPosition(object))?;
        self.family428650_actor_mut(object)?.position_222 = Some([x, y.wrapping_add(0x400), z]);
        let value = payload.dword_6c.value.wrapping_mul(0x3d0) >> 8;
        let state = self.selector35_actor_mut(object)?;
        state.word_258 = Some(value);
        state.word_25c = Some(value.wrapping_mul(value));
        Ok(())
    }

    /// 0043D2CA stores the operand address, without reading any operand.
    pub fn set_selector35_retained(
        &mut self,
        object: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        allocation_offset: u32,
    ) -> Result<(), DerivedActorError> {
        self.selector35_actor(object)?;
        allocation.validate_range(allocation_offset, 0)?;
        self.selector35_actor_mut(object)?.retained_23c = Some(RetainedSelector35Data {
            allocation: Arc::clone(allocation),
            allocation_offset,
        });
        Ok(())
    }

    pub fn apply_selector35_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(35) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector35_actor(object)?;
        let code = command.code.value;
        if code <= 22 {
            return self.apply_family428650_command(object, command, context);
        }
        match (code, &command.payload) {
            (24, InstanceCommandPayload::Selector35Packed240(value)) => {
                let [a, b, c, _] = value.value.to_le_bytes();
                self.selector35_actor_mut(object)?.bytes_240 = [Some(a), Some(b), Some(c), Some(0)];
                self.owned_mut(object)?.state.base.flags |= 0x1000_0000;
                Ok(true)
            }
            (23, _) => Err(ActorCommandError::Payload(code)),
            (24, _) => Err(ActorCommandError::Payload(code)),
            _ => Ok(false),
        }
    }

    pub fn finish_selector35_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let retained = self
            .selector35_actor(object)?
            .retained_23c
            .clone()
            .ok_or(DerivedActorError::MissingSelector35Payload)?;
        let denominator = retained.allocation.word(retained.allocation_offset)?;
        if denominator != 0 {
            let at = retained
                .allocation_offset
                .checked_add(0x10)
                .ok_or(DerivedActorError::MissingSelector35Payload)?;
            let numerator = u32::from(retained.allocation.word(at)?) << 10;
            self.family428650_actor_mut(object)?.word_214 = numerator / u32::from(denominator);
        }
        self.finish_actor_initialization(object)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector35_tests.rs"]
mod tests;
