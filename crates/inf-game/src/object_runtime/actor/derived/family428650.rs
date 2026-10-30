//! 00428650 construction, 004270E0 admission, 004276B0 commands and
//! 004277E0 after-load. Update/resource creation is a separate boundary.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};

use super::family429e20::Family429e20State;
use super::selector34::Selector34State;
use super::selector35::Selector35State;
use super::FamilyHelperHistoryRecord;
use super::{DerivedActorClass, DerivedActorConstructionRequest, DerivedActorError};
use crate::object_runtime::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorHandle, ObjectArena,
    ObjectError, ObjectId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Family428650Class {
    Base,
    Family429e20(Box<Family429e20State>),
    Selector34(Box<Selector34State>),
    Selector35(Box<Selector35State>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyHandleEntry {
    pub handle: ActorHandle,
    pub word_04: u32,
    pub word_08: u32,
}

/// Unwritten holes have no portable value. Six owned target pointers retain
/// their separate score words. Native constructor 00428650 writes null and
/// zero; the shared publishers preserve later resource/score asymmetries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family428650State {
    pub class: Family428650Class,
    pub word_184: i32,
    pub words_188_19c: [Option<u32>; 6],
    pub count_1a0: u32,
    pub entries_1a4: [FamilyHandleEntry; 4],
    pub targets_1d4_1fc: [Option<crate::object_runtime::NativeHeapRef>; 6],

    pub words_1d8_200: [u32; 6],
    pub words_204_208: [Option<u32>; 2],
    /// Same raw DWORD before and after +34: later calls reinterpret a prior
    /// handle as a signed placement reference. There is no phase sentinel.
    pub word_20c: u32,
    pub word_210: u32,
    pub word_214: u32,
    pub helper_218: Option<Box<FamilyHelper482ff0>>,
    pub word_21c: u32,
    pub word_220: Option<u16>,
    pub position_222: Option<[u16; 3]>,
    pub word_228: u32,
    pub byte_22c: u8,
    pub byte_22d: Option<u8>,
    pub word_22e: u16,
}

/// The distinct 0x5C allocation at +218. Original 00482FF0 writes only +08;
/// allocation backing for +00 is explicit; +04 and history remain independent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyHelper482ff0 {
    pub words_00_04: [Option<u32>; 2],
    pub word_08: u32,
    pub records: [FamilyHelperHistoryRecord; 4],
}

impl FamilyHelper482ff0 {
    pub(super) fn new(initial_handle: Option<u32>) -> Self {
        let mut helper = Self {
            words_00_04: [initial_handle, None],
            word_08: 0,
            records: [FamilyHelperHistoryRecord::UNKNOWN; 4],
        };
        helper.initialize();
        helper
    }

    /// 00482FF0 initializes only the count, preserving supplied allocation
    /// custody for every header/record span.
    pub fn initialize(&mut self) {
        self.word_08 = 0;
    }
}

impl ObjectArena {
    pub fn allocate_family428650_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_derived_actor(request, rng)?;
        let null = self
            .actor_handles()
            .expect("constructed actor registry")
            .null_handle();
        let word_220 = self.actor_allocation.initial_word();
        let word_18c = self
            .actor_allocation
            .initial_dword(super::super::allocation::ActorAllocationDwordCarry::Family18c);
        let word_19c = self
            .actor_allocation
            .initial_dword(super::super::allocation::ActorAllocationDwordCarry::Family19c);
        self.derived_actor_mut(object)?.class =
            DerivedActorClass::Family428650(Box::new(Family428650State {
                class: Family428650Class::Base,
                word_184: -1,
                // +188 and +190..+198 stay unknown; only the descriptor period
                // +18C and reselection timer +19C have a selected allocation source.
                words_188_19c: [None, word_18c, None, None, None, word_19c],
                // 00482D20: count 0, four records of the process NULL and two zeros.
                count_1a0: 0,
                entries_1a4: [FamilyHandleEntry {
                    handle: null,
                    word_04: 0,
                    word_08: 0,
                }; 4],
                targets_1d4_1fc: [None; 6],

                words_1d8_200: [0; 6],
                words_204_208: [None; 2],
                word_20c: null.raw(),
                word_210: 0,
                word_214: 0x100,
                helper_218: None,
                word_21c: 0,
                word_220,
                position_222: None,
                word_228: 0,
                byte_22c: 0,
                byte_22d: None,
                word_22e: 0,
            }));
        Ok(object)
    }

    pub fn family428650_actor(&self, object: ObjectId) -> Result<&Family428650State, ObjectError> {
        match &self.derived_actor(object)?.class {
            DerivedActorClass::Family428650(state) => Ok(state),
            _ => Err(ObjectError::NotFamily428650(object)),
        }
    }

    pub(in crate::object_runtime::actor) fn family428650_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Family428650State, ObjectError> {
        match &mut self.derived_actor_mut(object)?.class {
            DerivedActorClass::Family428650(state) => Ok(state),
            _ => Err(ObjectError::NotFamily428650(object)),
        }
    }

    /// Port fix (`PortFixes::paced_actors`):
    /// +22E bit 0, the request for a behaviour choice at the next update,
    /// put back as it was before a contact bump on a tick the paced actor
    /// skips, so bumps are sampled once each 30 fps step, as retail's were.
    pub(crate) fn keep_choice_request(
        &mut self,
        object: ObjectId,
        was: bool,
    ) -> Result<(), ObjectError> {
        let state = self.family428650_actor_mut(object)?;
        if was {
            state.word_22e |= 1;
        } else {
            state.word_22e &= !1;
        }
        Ok(())
    }

    /// An active descriptor +184, for tests that stage a behavior.
    #[cfg(test)]
    pub(crate) fn set_family_descriptor_for_tests(&mut self, object: ObjectId, index: i32) {
        self.family428650_actor_mut(object).unwrap().word_184 = index;
    }

    /// 004270E0 copies position only after successful central admission.
    /// Missing position preserves any earlier behavior replacement.
    pub fn admit_family428650_actor(
        &mut self,
        object: ObjectId,
    ) -> Result<bool, DerivedActorError> {
        self.family428650_actor(object)?;
        if !self.admit_derived_actor(object)? {
            return Ok(false);
        }
        let position = self
            .object(object)?
            .base
            .position_words
            .ok_or(DerivedActorError::MissingPosition(object))?;
        self.family428650_actor_mut(object)?.position_222 = Some(position);
        Ok(true)
    }

    pub fn apply_family428650_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        self.family428650_actor(object)?;
        if !matches!(
            command.target,
            InstanceCommandTarget::Family428650
                | InstanceCommandTarget::Family4236d0
                | InstanceCommandTarget::Constructor(9 | 10 | 12 | 21 | 34 | 35 | 36)
        ) {
            return Err(ActorCommandError::WrongTarget);
        }
        let code = command.code.value;
        if code <= 18 {
            return self.apply_derived_actor_command(object, command, context);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (19, NoPayloadRead) => {}
            (20, FamilyHomeTerrainXZ(value)) => {
                let terrain = context.terrain.ok_or(ActorCommandError::MissingTerrain)?;
                let [x, z] = value.value;
                self.family428650_actor_mut(object)?.position_222 =
                    Some([x, terrain.height_at([x, z]) as u16, z]);
            }
            (21, FamilyHomeTerrainXYZ(value)) => {
                let terrain = context.terrain.ok_or(ActorCommandError::MissingTerrain)?;
                let [x, y, z] = value.value;
                self.family428650_actor_mut(object)?.position_222 =
                    Some([x, y.wrapping_add(terrain.height_at([x, z]) as u16), z]);
            }
            (22, FamilyPlacementReference(value)) => {
                self.family428650_actor_mut(object)?.word_20c = value.value
            }
            (19..=22, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn initialize_family428650_after_load(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let value = self.family428650_actor(object)?.word_20c;
        if value != 0 {
            let handle = self
                .actor_handle_by_placement(value as i32)
                .expect("live family actor retains registry and valid published actor nodes");
            self.family428650_actor_mut(object)?.word_20c = handle.raw();
        }
        Ok(())
    }
}

#[cfg(test)]
impl ObjectArena {
    /// Test-only: forget the extended `+18C` carry so a session test can
    /// observe which descriptor owner first requires the period.
    pub(crate) fn forget_family_period_18c(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), DerivedActorError> {
        self.family428650_actor_mut(actor)?.words_188_19c[1] = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "family428650_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "family428650_native_tests.rs"]
mod native_tests;

#[cfg(test)]
#[path = "selector34_native_tests.rs"]
mod selector34_native_tests;

#[cfg(test)]
#[path = "selector36_native_tests.rs"]
mod selector36_native_tests;

#[cfg(test)]
#[path = "selector10_native_tests.rs"]
mod selector10_native_tests;
