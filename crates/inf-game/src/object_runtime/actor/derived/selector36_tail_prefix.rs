//! Source-owned Selector36 post-D8 prefix00481787 through00481A22.
//! Actual target refresh, aim and control publications precede counters. The returned phase is
//! an unfinished native continuation, never a completed virtual+24 callback.

use inf_formats::native_basis::{InverseSineQ31Error, NativeBasis, NativeBasisMath};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::super::behavior::{
    ActorBehaviorPositionError, ActorBehaviorPositionRequest, ActorBehaviorTickError,
};
use super::selector12_tail::TailAvoidance;
use super::{kinematics::ftol, Family4236d0Class, Selector36State};
use super::{
    FamilyAvoidanceError, FamilyObstacleScanError, FamilyTerrainProbeMissHost,
    Selector12TailProbeError,
};
use crate::object_runtime::{
    NativeHeapRef, ObjectArena, ObjectError, ObjectId, RuntimeAllocationError,
};

mod target;

/// Retained process motion input. Authored DWORD0053BDB4 has one recovered
/// direct reader and no recovered writer; it is distinct from tick delta.
/// Startup validates the source range once. No hash, parse, or file read runs
/// in an actor callback. A future recovered writer belongs to this owner.
#[derive(Debug)]
pub struct ProcessActorMotionState {
    selector36_counter_multiplier: u32,
}

impl ProcessActorMotionState {
    pub fn from_executable(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        Ok(Self {
            selector36_counter_multiplier: executable.word(0x0053_bdb4)?,
        })
    }
}

pub struct Selector36TailPrefixRequest<'a, H> {
    pub microseconds: u32,
    pub process: &'a ProcessActorMotionState,
    pub settings_7c: u32,
    pub math: &'a NativeBasisMath,
    pub inverse_sine: &'a [i16; 1024],
    /// Actual retained547590 owner, read only after the yaw publication.
    pub shared_basis: &'a NativeBasis,
    pub random_state: &'a mut u32,
    pub behavior_tables: &'a mut crate::object_runtime::ProcessActorBehaviorTables,
    pub process_velocity: Option<[u16; 3]>,
    pub terrain: &'a mut H,
    pub child:
        &'a mut dyn crate::object_runtime::ActorChildHost<Error = Selector36ChildServiceError>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector36ChildServiceError {
    #[error("dynamic child {object:?} retained after construction failure: {reason}")]
    RetainedConstruction { object: ObjectId, reason: String },
    #[error("{0}")]
    Owner(String),
}
impl From<String> for Selector36ChildServiceError {
    fn from(error: String) -> Self {
        Self::Owner(error)
    }
}
impl From<&str> for Selector36ChildServiceError {
    fn from(error: &str) -> Self {
        Self::Owner(error.to_owned())
    }
}

/// Actual next instruction phase after this visit's reached prefix. A caller
/// must continue from this phase or stop the feature; repeating the prefix in
/// the same visit would repeat its decay, shifts and counter publications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector36TailNextPhase {
    /// Counter, reached WeaponTick48D030, and fresh148 avoidance42A2C0 have
    /// returned normally. Actual+118=481C10 and later481B10 audio remain
    /// required; this is not Selector12 motion467DF0 or virtual+24 completion.
    Motion { controls: [i32; 2] },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Selector36TailPrefixError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] super::DerivedActorError),
    #[error("actor {0:?} does not own Selector36 tail00481787")]
    UnsupportedClass(ObjectId),
    #[error("actor {actor:?} requires field+{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("target {reference:?} requires focus08 at {address:08X}")]
    RequiredTargetFocus {
        reference: NativeHeapRef,
        address: u32,
    },
    #[error(transparent)]
    BehaviorPosition(#[from] ActorBehaviorPositionError),
    #[error(transparent)]
    BehaviorTick(#[from] ActorBehaviorTickError),
    #[error("native class36 avoidance failed: {0}")]
    Avoidance(FamilyAvoidanceError<FamilyObstacleScanError<Selector12TailProbeError<E>>>),
    #[error(transparent)]
    Allocation(#[from] RuntimeAllocationError),
    #[error(transparent)]
    InverseSine(#[from] InverseSineQ31Error),
    #[error(transparent)]
    Division(#[from] super::ActorMotionError),
    #[error("actor {actor:?} requires behavior entry {index}; current {entries}")]
    BehaviorEntry {
        actor: ObjectId,
        index: i8,
        entries: usize,
    },
    #[error("selector36 child creation failed after27E publication: {0}")]
    Child(crate::object_runtime::ActorChildError<Selector36ChildServiceError>),
    #[error(transparent)]
    Firing(#[from] crate::object_runtime::ActorWeaponFireError),
}

impl ObjectArena {
    /// Called only after normal Selector36 prefix/status/virtual+D8 return.
    /// Retains all reached writes; no target, weapon, avoidance, motion, audio
    /// or field initialization is synthesized beyond this native phase.
    pub fn tick_selector36_tail_prefix<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        mut request: Selector36TailPrefixRequest<'_, H>,
    ) -> Result<Selector36TailNextPhase, Selector36TailPrefixError<H::Error>> {
        self.object(actor)?;
        if self.selector36_actor(actor).is_err() {
            return Err(Selector36TailPrefixError::UnsupportedClass(actor));
        }
        //00481787..004817AD: DWORD subtract, LOWWORD store, signedAX clamp.
        //A zero17A skips the subtraction even if delta>>10 has a set high bit.
        let value = self.derived_actor(actor)?.word_17a;
        if value != 0 {
            let value = value.wrapping_sub((request.microseconds >> 10) as u16);
            self.derived_actor_mut(actor)?.word_17a = if value as i16 >= 0 { value } else { 0 };
        }
        //Actual nonNULL148 refresh/aim/control path, before either counter.
        if let Some(captured) = self.derived_actor(actor)?.target_148 {
            if let Some(controls) =
                self.selector36_target_controls(actor, captured, &mut request)?
            {
                self.publish_selector36_counters::<H::Error>(actor, request.process)?;
                return self.selector36_counter_continuation(
                    actor,
                    controls,
                    request.microseconds,
                    request.terrain,
                );
            }
            //The genuine primary-distance0 lane joins the native shifts.
        }
        //004819E7: logical WORD268 shift;004819EE: logical WORD26E shift.
        //The neighboring halfwords are separate packed model parameters.
        {
            let state = self.selector36_tail_state_mut::<H::Error>(actor)?;
            state.word_268 =
                (state.word_268 & 0xffff_0000) | u32::from((state.word_268 as u16) >> 1);
            state.word_26c = (state.word_26c & 0x0000_ffff)
                | (u32::from(((state.word_26c >> 16) as u16) >> 1) << 16);
        }
        self.publish_selector36_counters::<H::Error>(actor, request.process)?;
        self.selector36_counter_continuation(actor, [0; 2], request.microseconds, request.terrain)
    }

    fn selector36_counter_continuation<H: FamilyTerrainProbeMissHost>(
        &mut self,
        actor: ObjectId,
        mut controls: [i32; 2],
        microseconds: u32,
        terrain: &mut H,
    ) -> Result<Selector36TailNextPhase, Selector36TailPrefixError<H::Error>> {
        //4819FC fresh140 follows counters. Only its actual nonNULL branch
        //calls48D030; that service may clear cached group bits and write timers.
        if self.derived_actor(actor)?.behavior_140.is_some() {
            self.tick_actor_behavior(actor, microseconds)?;
        }
        //481A0D fresh148 occurs only after normal weapon return. Both final
        //Selector12/36+48=4026B0 expose the same embedded actor basis. Shared
        //TailAvoidance retains the actual installed234 throughout cache misses.
        if self.derived_actor(actor)?.target_148.is_some() {
            self.adjust_family_avoidance(
                actor,
                &mut controls,
                microseconds,
                &mut TailAvoidance { miss: terrain },
            )
            .map_err(Selector36TailPrefixError::Avoidance)?;
        }
        Ok(Selector36TailNextPhase::Motion { controls })
    }

    pub(crate) fn publish_selector36_counters<E>(
        &mut self,
        actor: ObjectId,
        process: &ProcessActorMotionState,
    ) -> Result<(), Selector36TailPrefixError<E>> {
        //00481185..004811BF: signed velocity WORDs with forward basis
        //components+60/+64/+68, retained53-bit arithmetic in Z/Y then X order.
        let state = self.actor(actor)?;
        let [x, y, z] = state.words_8e_92.map(|word| f64::from(word as i16));
        let coefficient = |index| f64::from(f32::from_bits(state.rotating.basis.words[index]));
        let forward = ftol((z * coefficient(9) + y * coefficient(8)) + x * coefficient(7));
        //004811C4: actual retained process input, after projection/FTOL.
        let multiplier = process.selector36_counter_multiplier as i32;
        //Magic signed divide1B4E81B5/SAR5/sign repair is truncation /300.
        let linear = forward.wrapping_mul(multiplier) / 300;
        //004811E0: fresh signedDWORD98, then lowDWORD IMUL and arithmeticSAR15.
        let angular = (self.actor(actor)?.words_94_9c[1] as i32).wrapping_mul(multiplier) >> 15;
        let left = linear.wrapping_sub(angular) as u16;
        let right = linear.wrapping_add(angular) as u16;
        let state = self.selector36_tail_state_mut::<E>(actor)?;
        let low = (state.word_270 as u16).wrapping_add(left);
        let high = ((state.word_270 >> 16) as u16).wrapping_add(right);
        //004811F3 then004811FA: two independent WORD adds, with no carry
        //propagation between270 and272. No host can observe an intermediate.
        state.word_270 = u32::from(low) | (u32::from(high) << 16);
        Ok(())
    }

    fn selector36_tail_state_mut<E>(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector36State, Selector36TailPrefixError<E>> {
        match &mut self.family4236d0_actor_mut(actor)?.class {
            Family4236d0Class::Selector36(state) => Ok(state),
            _ => Err(Selector36TailPrefixError::UnsupportedClass(actor)),
        }
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod target_tests;
#[cfg(test)]
mod tests;
