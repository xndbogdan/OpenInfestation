//! Selector10, 00425190/00425270/004252B0, final vtable00520CC8.
//!
//! Selector 21 ("Nemian", case 0x15, 0x280 bytes) is 00405590: 00425190,
//! then final vtable 0051F7A8 and four DWORDs past selector 10's. Its table
//! is selector 10's but for +00 (004055D0, the same teardown), +14 (the
//! model values 00405700) and +118 (the motion 00405600: selector 10's
//! 00425400, then two swings).

use inf_formats::{native_basis::NativeBasisMath, InstanceCommandTarget, InstanceCommandView};

use super::{
    super::behavior::select_actor_behavior, DerivedActorConstructionRequest, DerivedActorError,
    Family4236d0Class, Family4236d0InitializationRequest, Selector10Helper,
};
use crate::{
    object_runtime::{
        ActorCommandContext, ActorCommandError, ActorConstructionError, ObjectArena, ObjectId,
    },
    ResidentRuntime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector10State {
    /// 00425190 does not write +268; 00425290 copies the full payload DWORD.
    pub word_268: Option<u32>,
    pub helper_26c: Option<Box<Selector10Helper>>,
    /// Selector 21's swings; None for selector 10.
    pub swing: Option<Selector21Swing>,
}

/// Selector 21's DWORDs +270..+27C (00405590): two swings, each turned back
/// at 0x1000 by a speed redrawn from the process RNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector21Swing {
    /// +270, the model's selector 1; it starts at 0x2001.
    pub swing_270: i32,
    /// +274, the model's selector 2; it starts at 0x2001.
    pub swing_274: i32,
    /// +278, +274's speed; zero at first.
    pub speed_278: i32,
    /// +27C, +270's speed; zero at first.
    pub speed_27c: i32,
}

impl Selector21Swing {
    const INITIAL: Self = Self {
        swing_270: 0x2001,
        swing_274: 0x2001,
        speed_278: 0,
        speed_27c: 0,
    };
}

/// 0040567A..00405681 and 004056E9..004056F0: IMUL with the delta, then the
/// high DWORD of the product shifted left once (bits 31..62).
fn swing_step(delta: u32, speed: i32) -> i32 {
    ((i64::from(delta as i32) * i64::from(speed)) >> 31) as i32
}

/// 00405700, selector 21's +14: the swings' low WORDs for selectors 1 and 2;
/// None defers every other selector to the derived 004260A0.
pub(crate) fn selector21_value(state: &Selector10State, selector: u32) -> Option<u16> {
    let swing = state.swing?;
    match selector {
        1 => Some(swing.swing_270 as u16),
        2 => Some(swing.swing_274 as u16),
        _ => None,
    }
}

impl ObjectArena {
    pub fn allocate_selector10_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_family4236d0_actor(request, rng)?;
        self.family4236d0_actor_mut(object)?.class =
            Family4236d0Class::Selector10(Box::new(Selector10State {
                word_268: None,
                helper_26c: None,
                swing: None,
            }));
        Ok(object)
    }

    /// Case 0x15 (0046085A..0046087D): 0x280 bytes, then 00405590.
    pub fn allocate_selector21_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_selector10_actor(request, rng)?;
        if let Family4236d0Class::Selector10(state) =
            &mut self.family4236d0_actor_mut(object)?.class
        {
            state.swing = Some(Selector21Swing::INITIAL);
        }
        Ok(object)
    }

    /// 00405615..004056F2, the rest of selector 21's motion after selector
    /// 10's 00425400: in state 3, a swing past 0x1000 either way takes a
    /// new speed toward the other side (+270's from 0048F060 & 0xFFFC, then
    /// +274's & 0xFFF8), and each swing moves by its speed times the delta.
    pub fn tick_selector21_swing(
        &mut self,
        actor: ObjectId,
        delta: u32,
        rng: &mut u32,
    ) -> Result<(), DerivedActorError> {
        if self.actor(actor)?.word_c4 != 3 {
            return Ok(());
        }
        let state = self.selector10_actor_mut(actor)?;
        let swing = state
            .swing
            .as_mut()
            .ok_or(DerivedActorError::NotSelector21(actor))?;
        if swing.swing_270 > 0x1000 {
            let draw = crate::name_entry::name_entry_rng_next(rng) & 0xfffc;
            swing.speed_27c = 0xfe00_0000_u32.wrapping_sub(draw << 9) as i32;
        } else if swing.swing_270 < -0x1000 {
            let draw = crate::name_entry::name_entry_rng_next(rng) & 0xfffc;
            swing.speed_27c = ((draw + 0x1_0000) << 9) as i32;
        }
        swing.swing_270 = swing
            .swing_270
            .wrapping_add(swing_step(delta, swing.speed_27c));
        if swing.swing_274 > 0x1000 {
            let draw = crate::name_entry::name_entry_rng_next(rng) & 0xfff8;
            swing.speed_278 = 0xff00_0000_u32.wrapping_sub(draw << 8) as i32;
        } else if swing.swing_274 < -0x1000 {
            let draw = crate::name_entry::name_entry_rng_next(rng) & 0xfff8;
            swing.speed_278 = ((draw + 0x2_1000) << 8) as i32;
        }
        swing.swing_274 = swing
            .swing_274
            .wrapping_add(swing_step(delta, swing.speed_278));
        Ok(())
    }

    pub fn selector10_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector10State, DerivedActorError> {
        match &self.family4236d0_actor(object)?.class {
            Family4236d0Class::Selector10(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector10(object)),
        }
    }

    pub(in crate::object_runtime) fn selector10_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector10State, DerivedActorError> {
        match &mut self.family4236d0_actor_mut(object)?.class {
            Family4236d0Class::Selector10(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector10(object)),
        }
    }

    pub fn initialize_selector10_actor(
        &mut self,
        object: ObjectId,
        request: Family4236d0InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.selector10_actor(object)?;
        let payload = request
            .actor
            .payload
            .family425190
            .as_ref()
            .ok_or(DerivedActorError::MissingSelector10Payload)?;
        self.initialize_family4236d0_actor(object, request, runtime, math)?;
        self.selector10_actor_mut(object)?.word_268 = Some(payload.dword_74.value);
        Ok(())
    }

    /// 00425240: code 25 succeeds without reading the operand address.
    pub fn apply_selector10_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        // Selector 21 keeps 00425240.
        if !matches!(command.target, InstanceCommandTarget::Constructor(10 | 21)) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector10_actor(object)?;
        match command.code.value {
            ..=24 => self.apply_family4236d0_command(object, command, context),
            25 => Ok(true),
            _ => Ok(false),
        }
    }

    /// 004252B0 acquires +26C before behavior selection and 004250C0.
    /// Native re-entry with a recognized selector loses the old pointer without
    /// deleting it. The factory calls this once; refuse that separate leak path
    /// instead of silently releasing or reusing an existing allocation.
    pub fn finish_selector10_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let state = self.selector10_actor_mut(object)?;
        let selector = state
            .word_268
            .ok_or(DerivedActorError::MissingSelector10Payload)?;
        if selector <= 4 && state.helper_26c.is_some() {
            return Err(DerivedActorError::Selector10HelperReplacementBoundary(
                object,
            ));
        }
        if let Some(mut helper) = Selector10Helper::for_selector(selector) {
            // 00446BC0 never writes the walker's stride phase +34, nor most
            // of its legs' bytes (fresh 0x74-byte blocks): their foot
            // targets +24..+28 are read by 00445CD0 before a swing writes them.
            if let super::Selector10HelperClass::Family446bc0 { phase_34 } = &mut helper.class {
                *phase_34 = self.actor_allocation.initial_extended_word();
                if let Some(zero) = self.actor_allocation.initial_extended_word() {
                    for child in helper.children_14.iter_mut().flatten() {
                        child.fill_unknown(zero as u8);
                    }
                }
            }
            self.selector10_actor_mut(object)?.helper_26c = Some(helper);
        }
        let flags = self.object(object)?.base.flags;
        let selector = self.actor(object)?.word_b4;
        if let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 {
            select_actor_behavior(behavior, flags, selector);
        }
        self.finish_family4236d0_initialization(object)
    }
}

#[cfg(test)]
#[path = "selector21_tests.rs"]
mod selector21_tests;
