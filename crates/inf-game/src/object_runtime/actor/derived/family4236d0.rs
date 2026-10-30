//! 004236D0/00423850/004239E0/004250C0 construction and postinitialization.
//! Movement, collision, behavior transitions and cache mutation are separate.

mod destruction;
mod motion;

pub(in crate::object_runtime) use destruction::exit_writes_nothing;
pub use motion::{Family4236d0MotionError, Family4236d0MotionRequest};

use std::sync::Arc;

use inf_formats::{native_basis::NativeBasisMath, InstanceCommandTarget, InstanceCommandView};

use super::super::allocation::ActorAllocationDwordCarry;
use super::{
    family429e20::Family429e20Class,
    selector10::Selector10State,
    selector36::Selector36State,
    terrain_probe::{TerrainProbeAllocation, TerrainProbeCache},
    DerivedActorConstructionRequest, DerivedActorError, FamilyHelper482ff0,
};
use crate::{
    object_runtime::{
        ActorCommandContext, ActorCommandError, ActorConstructionError, ActorInitializationRequest,
        ObjectArena, ObjectError, ObjectId, RuntimeAllocation,
    },
    ResidentRuntime,
};

/// One retained address in a shared mutable H13/H14 allocation. Retention
/// performs no scalar read and permits a valid one-past address.
#[derive(Clone, Debug)]
pub struct RetainedActorAllocation {
    allocation: Arc<RuntimeAllocation>,
    pub allocation_offset: u32,
}

impl RetainedActorAllocation {
    pub fn allocation(&self) -> &Arc<RuntimeAllocation> {
        &self.allocation
    }

    pub(super) fn new(
        allocation: &Arc<RuntimeAllocation>,
        allocation_offset: u32,
    ) -> Result<Self, DerivedActorError> {
        allocation.validate_range(allocation_offset, 0)?;
        Ok(Self {
            allocation: Arc::clone(allocation),
            allocation_offset,
        })
    }
}

impl PartialEq for RetainedActorAllocation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
            && self.allocation_offset == other.allocation_offset
    }
}
impl Eq for RetainedActorAllocation {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Family4236d0Class {
    Base,
    Selector10(Box<Selector10State>),
    Selector36(Box<Selector36State>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family4236d0State {
    pub class: Family4236d0Class,
    pub retained_23c: Option<RetainedActorAllocation>,
    pub word_240: u32,
    pub table_244: u32,
    pub word_248: u32,
    pub word_24c: Option<u32>,
    /// Static executable identities, not host addresses.
    pub table_250: u32,
    pub table_254: u32,
    pub table_258: u32,
    pub table_25c: u32,
    pub byte_260: u8,
    pub byte_261: Option<u8>,
    pub word_262: u16,
    pub word_264: u16,
    pub word_266: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Family4236d0InitializationRequest<'a> {
    pub actor: ActorInitializationRequest<'a>,
    pub allocation: &'a Arc<RuntimeAllocation>,
    pub payload_offset: u32,
}

impl ObjectArena {
    pub fn allocate_family4236d0_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family429e20_actor(request, rng)?;
        let state = self.family429e20_actor_mut(object)?;
        state.class = Family429e20Class::Family4236d0(Box::new(Family4236d0State {
            class: Family4236d0Class::Base,
            retained_23c: None,
            word_240: 0,
            table_244: 0x0052_0a80,
            word_248: 0,
            word_24c: None,
            table_250: 0x0052_fe40,
            table_254: 0x0052_fe48,
            table_258: 0x0052_fe38,
            table_25c: 0x0052_fe58,
            byte_260: 1,
            byte_261: None,
            word_262: u16::MAX,
            word_264: 0,
            word_266: 0,
        }));
        // The fresh 00429E20 prefix has a null +234. The original conditional
        // acquisition follows class publication and initializes both allocations.
        state.probe_234 = Some(Box::new(TerrainProbeAllocation::new(
            TerrainProbeCache::new(8),
        )));
        Ok(object)
    }

    /// The classes whose +24 is the family's own `00424830`: the family's
    /// own class (selector 9) and selector 10. Its prefix, target tail and
    /// persistent-audio suffix are theirs alike; only +118 differs.
    pub fn require_family_update_424830(&self, object: ObjectId) -> Result<(), DerivedActorError> {
        match self.family4236d0_actor(object)?.class {
            Family4236d0Class::Base | Family4236d0Class::Selector10(_) => Ok(()),
            Family4236d0Class::Selector36(_) => {
                Err(DerivedActorError::NotFamilyUpdate424830(object))
            }
        }
    }

    pub fn family4236d0_actor(&self, object: ObjectId) -> Result<&Family4236d0State, ObjectError> {
        match &self.family429e20_actor(object)?.class {
            Family429e20Class::Family4236d0(state) => Ok(state),
            _ => Err(ObjectError::NotFamily4236d0(object)),
        }
    }

    // Captured selector36+10 writes BYTE265 and278 through this same owner.
    // Keep the exclusive state loan inside object_runtime.
    pub(in crate::object_runtime) fn family4236d0_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Family4236d0State, ObjectError> {
        match &mut self.family429e20_actor_mut(object)?.class {
            Family429e20Class::Family4236d0(state) => Ok(state),
            _ => Err(ObjectError::NotFamily4236d0(object)),
        }
    }

    /// Shared context wrapper 004239C0; selector36 adds only payload custody
    /// after this same virtual-context / 00423850 sequence.
    pub fn initialize_family4236d0_actor(
        &mut self,
        object: ObjectId,
        request: Family4236d0InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.family4236d0_actor(object)?;
        let payload = request
            .actor
            .payload
            .family4236d0
            .as_ref()
            .ok_or(DerivedActorError::MissingFamily4236d0Payload)?;
        if request.payload_offset.checked_add(0x58) != Some(payload.retained_58_offset) {
            return Err(DerivedActorError::MissingFamily4236d0Payload);
        }
        self.initialize_derived_actor(object, request.actor, runtime, math)?;
        self.set_family4236d0_retained(object, request.allocation, payload.retained_58_offset)?;
        let state = self.family4236d0_actor_mut(object)?;
        // Unmatched input preserves the existing table, including on repeated init.
        state.table_25c = match payload.dword_68.value {
            1 | 2 | 7 => 0x0052_fe68,
            3 | 8 => 0x0052_fe88,
            4 => 0x0052_fe98,
            5 => 0x0052_fe78,
            6 => 0x0052_feb8,
            _ => state.table_25c,
        };
        state.word_264 = payload.word_6c.value;
        state.word_262 = payload.word_6e.value;
        state.table_254 = match payload.dword_70.value {
            1 => 0x0052_ff38,
            2 => 0x0052_fee8,
            3 => 0x0052_fef8,
            4 => 0x0052_ff48,
            5 => 0x0052_fed8,
            6 => 0x0052_ff08,
            7 => 0x0052_ff18,
            8 => 0x0052_ff28,
            9 => 0x0052_ff88,
            _ => state.table_254,
        };
        self.derived_actor_mut(object)?.byte_160 = payload.word_6c.value as u8 & 1;
        Ok(())
    }

    pub fn set_family4236d0_retained(
        &mut self,
        object: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        allocation_offset: u32,
    ) -> Result<(), DerivedActorError> {
        self.family4236d0_actor(object)?;
        let retained = RetainedActorAllocation::new(allocation, allocation_offset)?;
        self.family4236d0_actor_mut(object)?.retained_23c = Some(retained);
        Ok(())
    }

    pub fn apply_family4236d0_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if !matches!(
            command.target,
            InstanceCommandTarget::Family4236d0
                | InstanceCommandTarget::Constructor(9 | 10 | 21 | 36)
        ) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.family4236d0_actor(object)?;
        match command.code.value {
            ..=22 => self.apply_family428650_command(object, command, context),
            23 => Ok(true),
            24 => Err(ActorCommandError::Payload(24)),
            _ => Ok(false),
        }
    }

    /// 004250C0. Only fresh behavior-index -1 is reachable through the owned
    /// construction paths; entering a later state transition requires callbacks.
    pub fn finish_family4236d0_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let retained = self
            .family4236d0_actor(object)?
            .retained_23c
            .clone()
            .ok_or(DerivedActorError::MissingFamily4236d0Payload)?;
        let denominator = retained.allocation.word(retained.allocation_offset)?;
        if denominator != 0 {
            let at = retained
                .allocation_offset
                .checked_add(0x0e)
                .ok_or(DerivedActorError::MissingFamily4236d0Payload)?;
            let numerator = u32::from(retained.allocation.word(at)?) << 10;
            self.family428650_actor_mut(object)?.word_214 = numerator / u32::from(denominator);
        }
        let selector = self.actor(object)?.word_b4;
        let behavior_present = self.derived_actor(object)?.behavior_140.is_some();
        if selector == 0 {
            self.owned_mut(object)?.state.base.flags |= 0x1000_0000;
        }
        let mode = if selector == 0 {
            0
        } else if selector == 18 {
            1
        } else {
            1 + u16::from(behavior_present)
        };
        // 00425AB0's mode; 00425A40 changes only a differing one.
        if self.family4236d0_actor(object)?.word_266 != mode {
            if self.family428650_actor(object)?.word_184 != -1 {
                return Err(DerivedActorError::FamilyBehaviorBoundary(object));
            }
            self.family428650_actor_mut(object)?.word_22e |= 0x0c;
            self.family4236d0_actor_mut(object)?.word_266 = mode;
        }
        self.finish_actor_initialization(object)?;
        // This exact family's +7C is 004027A0: return +140. It is not the
        // similarly located field in the probe or movement allocations.
        if self.family428650_actor(object)?.helper_218.is_none() && behavior_present {
            // Only the reached fresh allocation receives modeled +00 backing.
            let initial_handle = self
                .actor_allocation
                .initial_dword(ActorAllocationDwordCarry::FamilyHelper00);
            self.family428650_actor_mut(object)?.helper_218 =
                Some(Box::new(FamilyHelper482ff0::new(initial_handle)));
        }
        Ok(())
    }
}
