//! Selector 34: 00433B20/00433C00/00433C90/00434FA0, vtable 00521EAC.
//! Destructor 00433B90 exits any selected 005329F8 descriptor, then
//! delegates to 00428750; ribbon +25C is not released by the ship.
//! The +24 update 00434210 is the shared family prefix (Selector35 shape),
//! +D8 00435050, then controls, motion (+118 00434C10) and engine audio.

mod attack;
mod audio;
mod controls;
mod destruction;
mod dying;
mod motion;
pub use attack::{Selector34AttackContext, Selector34AttackError};
pub use audio::Selector34PersistentAudioError;
pub use controls::{Selector34ControlsError, Selector34ControlsRequest, Selector34MotionControls};
pub use dying::{
    Selector34DyingEffect, Selector34DyingError, Selector34DyingHost, Selector34DyingRequest,
};
pub use motion::{Selector34MotionError, Selector34MotionHost, Selector34MotionRequest};

use std::sync::Arc;

use inf_formats::{native_basis::NativeBasisMath, InstanceCommandTarget, InstanceCommandView};

use super::super::behavior::select_actor_behavior;
use super::{family428650::Family428650Class, DerivedActorConstructionRequest, DerivedActorError};
use crate::{
    object_runtime::{
        ActorCommandContext, ActorCommandError, ActorConstructionError, ActorInitializationRequest,
        ObjectArena, ObjectId, RuntimeAllocation,
    },
    terrain_runtime::TerrainRuntime,
    ResidentRuntime,
};

#[derive(Clone, Debug)]
pub struct RetainedSelector34Data {
    allocation: Arc<RuntimeAllocation>,
    pub allocation_offset: u32,
}

impl RetainedSelector34Data {
    pub fn allocation(&self) -> &Arc<RuntimeAllocation> {
        &self.allocation
    }
}

impl PartialEq for RetainedSelector34Data {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}
impl Eq for RetainedSelector34Data {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector34State {
    pub word_230: u32,
    pub word_234: u32,
    pub byte_238: u8,
    pub byte_239: u8,
    pub retained_23c: Option<RetainedSelector34Data>,
    pub word_240: u32,
    pub words_244_248: [Option<u32>; 2],
    /// Controls' local direction WORDs +244/+246/+248. Native writes only
    /// LOWORD+248, so the DWORD view above keeps +24A's upper WORD separate.
    pub local_direction_244: Option<[u16; 3]>,
    /// Executable data identities, never dereferenceable host pointers.
    pub table_24c: Option<u32>,
    pub table_250: Option<u32>,
    pub table_254: Option<u32>,
    pub table_258: Option<u32>,
    /// Ribbon owner 005286D8 created and updated by motion; None is NULL.
    pub trail_25c: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug)]
pub struct Selector34InitializationRequest<'a> {
    pub actor: ActorInitializationRequest<'a>,
    pub allocation: &'a Arc<RuntimeAllocation>,
    pub payload_offset: u32,
}

impl ObjectArena {
    pub fn allocate_selector34_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family428650_actor(request, rng)?;
        self.family428650_actor_mut(object)?.class =
            Family428650Class::Selector34(Box::new(Selector34State {
                word_230: 500,
                word_234: 512,
                byte_238: 1,
                byte_239: 0,
                retained_23c: None,
                word_240: 750,
                words_244_248: [None; 2],
                local_direction_244: None,
                table_24c: None,
                table_250: None,
                table_254: None,
                table_258: None,
                trail_25c: None,
            }));
        Ok(object)
    }

    pub fn selector34_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector34State, DerivedActorError> {
        match &self.family428650_actor(object)?.class {
            Family428650Class::Selector34(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector34(object)),
        }
    }

    fn selector34_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector34State, DerivedActorError> {
        match &mut self.family428650_actor_mut(object)?.class {
            Family428650Class::Selector34(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector34(object)),
        }
    }

    /// 00433C70 invokes virtual +08 before 00433C00. The subsequent admission
    /// guard replaces this intermediate home Y with the current actor Y.
    pub fn initialize_selector34_actor(
        &mut self,
        object: ObjectId,
        request: Selector34InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.selector34_actor(object)?;
        let payload = request
            .actor
            .payload
            .selector34
            .as_ref()
            .ok_or(DerivedActorError::MissingSelector34Payload)?;
        if request.payload_offset.checked_add(0x58) != Some(payload.retained_58_offset) {
            return Err(DerivedActorError::MissingSelector34Payload);
        }
        self.initialize_derived_actor(object, request.actor, runtime, math)?;
        self.selector34_actor_mut(object)?.table_250 = Some(0x0053_29e8);
        self.set_selector34_retained(object, request.allocation, payload.retained_58_offset)?;
        let [x, y, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(DerivedActorError::MissingPosition(object))?;
        self.family428650_actor_mut(object)?.position_222 = Some([x, y.wrapping_add(0x400), z]);
        let state = self.selector34_actor_mut(object)?;
        state.table_24c = Some(0x0053_29f0);
        state.table_254 = Some(0x0053_29e0);
        state.table_258 = Some(0x0053_29d0);
        Ok(())
    }

    /// Code23 retains the operand address without reading its contents.
    pub fn set_selector34_retained(
        &mut self,
        object: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        allocation_offset: u32,
    ) -> Result<(), DerivedActorError> {
        self.selector34_actor(object)?;
        allocation.validate_range(allocation_offset, 0)?;
        self.selector34_actor_mut(object)?.retained_23c = Some(RetainedSelector34Data {
            allocation: Arc::clone(allocation),
            allocation_offset,
        });
        Ok(())
    }

    pub fn apply_selector34_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(34) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector34_actor(object)?;
        match command.code.value {
            ..=22 => self.apply_family428650_command(object, command, context),
            // The enclosing command walk supplies the retained allocation.
            23 => Err(ActorCommandError::Payload(23)),
            _ => Ok(false),
        }
    }

    /// 00434FA0: behavior selection, lazy ratio/flag, shared postinit, then
    /// optional signed terrain floor. The direct Y write does not dirty state.
    pub fn finish_selector34_initialization(
        &mut self,
        object: ObjectId,
        terrain: Option<&TerrainRuntime>,
    ) -> Result<(), DerivedActorError> {
        self.selector34_actor(object)?;
        let flags = self.object(object)?.base.flags;
        let selector = self.actor(object)?.word_b4;
        if let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 {
            select_actor_behavior(behavior, flags, selector);
        }
        let retained = self
            .selector34_actor(object)?
            .retained_23c
            .clone()
            .ok_or(DerivedActorError::MissingSelector34Payload)?;
        let denominator = retained.allocation.word(retained.allocation_offset)?;
        if denominator != 0 {
            let at = retained
                .allocation_offset
                .checked_add(0x0e)
                .ok_or(DerivedActorError::MissingSelector34Payload)?;
            let numerator = u32::from(retained.allocation.word(at)?) << 10;
            self.family428650_actor_mut(object)?.word_214 = numerator / u32::from(denominator);
            // 1000 does not intersect the native low-nine-bit dirty mask.
            self.owned_mut(object)?.state.base.flags |= 0x1000;
        }
        self.finish_actor_initialization(object)?;
        if self.object(object)?.base.flags & 0x0020_0000 != 0 {
            let terrain = terrain.ok_or(DerivedActorError::MissingSelector34Terrain)?;
            let [x, y, z] = self
                .object(object)?
                .base
                .position_words
                .ok_or(DerivedActorError::MissingPosition(object))?;
            let radius = self.actor(object)?.rotating.visual.radius_word;
            let floor = i32::from(terrain.height_at([x, z])) + i32::from(radius >> 2);
            if i32::from(y as i16) < floor {
                self.owned_mut(object)?.state.base.position_words = Some([x, floor as u16, z]);
            }
        }
        Ok(())
    }
}
