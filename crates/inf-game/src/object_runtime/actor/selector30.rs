//! Selector 30, 0048B170 over 0040D800 over the shared actor.
//! This family does not use 00425E60 or its RNG/behavior allocation. Its
//! constructor, payload, commands and postinitialization retain their separate
//! ownership; update 0048B3F0 and its +154 sound record live in `update`.

use inf_formats::{
    grant_is_owned, native_basis::NativeBasisMath, GrantClosureError, InstanceCommandPayload,
    InstanceCommandTarget, InstanceCommandView,
};
use thiserror::Error;

use super::{
    family40d800::{Actor40d800State, FamilyLinkChange},
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, ActorInitializationRequest,
};
use crate::{
    object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId},
    world_start::{WorldStartContext, WorldStartError},
    ResidentRuntime,
};

/// `0048C640`: the model a gate's death shows.
const GATE_DEATH_MODEL: u16 = 0x11e;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector30State {
    pub intermediate: Actor40d800State,
    pub word_15c: u32,
    pub word_160: u32,
    pub word_164: u16,
    pub byte_166: u8,
    pub byte_167: u8,
    pub word_168: u32,
    pub position_16c: Option<[u16; 3]>,
    pub word_174: u32,
    pub word_178: u32,
    pub word_17c: u16,
    pub word_180: u32,
    pub handle_184: ActorHandle,
    pub word_188: Option<u32>,
    pub word_18c: u16,
    pub byte_18e: u8,
    pub byte_18f: u8,
    pub byte_190: u8,
    pub byte_191: u8,
    pub byte_192: u8,
    /// Port state, no retail field: the Command Centre's dead gate 81 with
    /// Pelagia on ([`crate::pelagia`]), which a scientist answers while its
    /// lock holds and whose pass takes the Teleport's load.
    pub pelagia: bool,
}

/// What Pelagia's gate counts its lock down by, as a hoop's link sends it.
const PELAGIA_WAKE: u32 = 1;
/// The hint a lock's end retires (retail's silent command 9, `0048C470`):
/// "Drop a scientist next to the warpgate so that it can be activated for
/// your craft".
const PELAGIA_GATE_HINT: u32 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Selector30Error {
    #[error("object {0:?} is not a selector-30 actor")]
    NotSelector30(ObjectId),
    #[error("selector-30 operation reached invalid object {0:?}")]
    InvalidObject(ObjectId),
    #[error("selector-30 payload lacks the 0040D950/0048B3C0 extension")]
    MissingPayload,
    #[error("selector-30 postinitialization requires initialized position for {0:?}")]
    MissingPosition(ObjectId),
    #[error("selector-30 after-load requires the postinitialization selector at +188 for {0:?}")]
    MissingInitialSelector(ObjectId),
    #[error(transparent)]
    Grant(#[from] GrantClosureError),
    #[error(transparent)]
    WorldStart(#[from] WorldStartError),
}

impl ObjectArena {
    pub fn allocate_selector30_actor(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let null = self.actor_handles().unwrap().null_handle();
        let word_158 = self
            .actor_allocation
            .initial_dword(super::allocation::ActorAllocationDwordCarry::Selector30Handle158);
        self.actor_mut(object)?.extension = ActorExtension::Selector30(Box::new(Selector30State {
            intermediate: Actor40d800State {
                word_158,
                ..Actor40d800State::new()
            },
            word_15c: 0,
            word_160: 0,
            word_164: 0,
            byte_166: 0,
            byte_167: 0,
            word_168: 0,
            position_16c: None,
            word_174: 0, // Immutable DWORD 00526068.
            word_178: 0,
            word_17c: 0,
            word_180: 0,
            handle_184: null,
            word_188: None,
            word_18c: 0,
            byte_18e: 0,
            byte_18f: 0,
            byte_190: 0,
            byte_191: 0,
            byte_192: 0,
            pelagia: false,
        }));
        Ok(object)
    }

    /// Gate `gate` becomes Pelagia's while it is still the hoop-locked gate
    /// to nowhere Level 1 places; anything else stays retail's.
    pub(crate) fn mark_pelagia_gate(&mut self, gate: ObjectId) {
        if let Ok(state) = self.selector30_actor_mut(gate) {
            if state.word_164 as i16 > 0 && state.word_15c == 0 && state.word_180 == 0 {
                state.pelagia = true;
            }
        }
    }

    /// Pelagia's return gate (world 1, placement 73): retail aims it at Terra
    /// Nova and holds it with command 16's neutral team. The port retargets
    /// it at the Command Centre's first start and lifts the team, so no
    /// dropped scientist is asked for; `+167` stays 1 (Mission Complete).
    pub(crate) fn prepare_pelagia_return_gate(
        &mut self,
        gate: ObjectId,
    ) -> Result<(), ObjectError> {
        let state = self
            .selector30_actor_mut(gate)
            .map_err(|_| ObjectError::NotActor(gate))?;
        state.word_15c = crate::pelagia::COMMAND_CENTRE;
        state.word_160 = 1;
        self.actor_mut(gate)
            .map_err(|_| ObjectError::NotActor(gate))?
            .word_b4 = 2;
        self.owned_mut(gate)
            .map_err(|_| ObjectError::NotActor(gate))?
            .state
            .base
            .dirty_word |= 2;
        Ok(())
    }

    /// Whether `gate` is Pelagia's with its lock still holding: a scientist
    /// of the player's side answers it, and its arrival wakes it.
    pub fn pelagia_gate_locked(&self, gate: ObjectId) -> bool {
        self.selector30_actor(gate)
            .is_ok_and(|state| state.pelagia && state.word_164 as i16 > 0)
    }

    /// A scientist reached Pelagia's locked gate: retail's message 0x15
    /// counts the lock down, which at 0 retires the gate's hint (the first
    /// player's silent command 9, `0048C470`), and the gate takes world 1,
    /// start 1. Its own update then names the destination, lights and hums.
    /// A scientist arrival lights its bulb: it has something to tell the
    /// player, as one carrying an unowned grant does ([`crate::pelagia`]).
    /// Anything else arriving wakes it without the bulb: the bulb must
    /// never break the wake.
    pub fn wake_pelagia_gate(
        &mut self,
        gate: ObjectId,
        arrival: ObjectId,
        context: &crate::object_runtime::PlayerCommandContext<'_>,
    ) -> Result<(), String> {
        if self
            .selector30_count_down(gate, PELAGIA_WAKE)
            .map_err(|error| format!("0048C470 0x15: {error}"))?
        {
            if let Some(first) = self.first_player() {
                self.apply_player_command(
                    crate::object_runtime::PlayerCommandRequest {
                        player: first,
                        operation: crate::object_runtime::PlayerCommandOperation::Set {
                            raw_id: PELAGIA_GATE_HINT,
                            silent: 1,
                        },
                    },
                    context,
                )
                .map_err(|error| format!("0046C760(9): {error}"))?;
            }
        }
        if let Ok(state) = self.selector30_actor_mut(gate) {
            state.word_15c = crate::pelagia::WORLD;
            state.word_160 = 1;
        }
        // The bulb, if a scientist arrived: retail's +29D only ever reflects
        // grants, and the report carries none, but it has something to tell.
        // World 18 places no selector 88, so the loader's flag the model's
        // selector-9 branch reads stays set and the bulb draws.
        if self.selector12_actor(arrival).is_ok() {
            self.mark_pelagia_reporter(arrival)
                .map_err(|error| format!("Pelagia reporter: {error}"))?;
        }
        Ok(())
    }

    pub fn selector30_actor(&self, object: ObjectId) -> Result<&Selector30State, Selector30Error> {
        match &self
            .actor(object)
            .map_err(|_| Selector30Error::NotSelector30(object))?
            .extension
        {
            ActorExtension::Selector30(state) => Ok(state),
            _ => Err(Selector30Error::NotSelector30(object)),
        }
    }

    fn selector30_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector30State, Selector30Error> {
        match &mut self
            .actor_mut(object)
            .map_err(|_| Selector30Error::NotSelector30(object))?
            .extension
        {
            ActorExtension::Selector30(state) => Ok(state),
            _ => Err(Selector30Error::NotSelector30(object)),
        }
    }

    /// `0048C630`, the gate's `+AC`, which `0047D670` calls at state 1: flag
    /// 1 off (`00479F20(0)`), dirty `+39 |= 1`, model `0x11E` refreshed
    /// (`0047A110`); a model the H8 pool cannot give (negative, past its
    /// count, or neither resident nor loadable, as the current world's
    /// resources answer here) disposes the gate (`00479FA0`); then `+168`
    /// is cleared.
    pub fn retire_selector30(
        &mut self,
        gate: ObjectId,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        self.selector30_actor(gate)
            .map_err(|_| ObjectError::NotActor(gate))?;
        self.set_flag1(gate, false)?;
        self.owned_mut(gate)?.state.base.dirty_word |= 0x100;
        self.actor_mut(gate)?.rotating.visual.model_word = GATE_DEATH_MODEL;
        self.refresh_visual_model(gate, runtime)?;
        if crate::object_runtime::visual::resident_model_resource(runtime, GATE_DEATH_MODEL)?
            .is_none()
        {
            self.dispose(gate)?;
        }
        self.selector30_actor_mut(gate)
            .map_err(|_| ObjectError::NotActor(gate))?
            .word_168 = 0;
        Ok(())
    }

    /// 0048B390 runs the visual context, 0047AC10 payload, then the two
    /// intermediate/class payload copies. Missing data is a host preflight.
    pub fn initialize_selector30_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.selector30_actor(object)?;
        let payload = request
            .payload
            .selector30
            .as_ref()
            .ok_or(Selector30Error::MissingPayload)?;
        self.initialize_actor(object, request, runtime, math)?;
        let state = self.selector30_actor_mut(object)?;
        state.intermediate.word_13c = payload.signed_dword_44.value;
        state.intermediate.clamp_current();
        let disable_selector = state.intermediate.word_13c != 0 && state.intermediate.word_140 == 0;
        state.intermediate.words_146_14a = Some(payload.words_48_4c.value);
        state.word_15c = payload.dword_50.value;
        state.word_160 = payload.dword_54.value;
        if disable_selector {
            self.actor_mut(object)?.word_b4 = 0;
            self.owned_mut(object)?.state.base.dirty_word |= 2;
        }
        Ok(())
    }

    /// 0048BBF0 and its 0040D9C0 parent. All specialized operands are copied
    /// scalars; no H14 address is retained by these commands.
    pub fn apply_selector30_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(30)
            || !matches!(self.actor(object)?.extension, ActorExtension::Selector30(_))
        {
            return Err(ActorCommandError::WrongTarget);
        }
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let ActorExtension::Selector30(state) = &mut self.actor_mut(object)?.extension else {
            unreachable!()
        };
        use InstanceCommandPayload::*;
        let mut dirty_0200 = false;
        match (code, &command.payload) {
            (16, Actor40d800Maximum(value)) => {
                state.intermediate.word_13c = i32::from(value.value);
                state.intermediate.clamp_current();
            }
            (17, Actor40d800Flag14e(value)) => {
                state.intermediate.byte_14e = u8::from(value.value != 0)
            }
            (18, Actor40d800Refill(value)) => {
                if value.value != 0 {
                    let delta = state.intermediate.word_13c;
                    dirty_0200 = delta != 0;
                    state.intermediate.word_140 = state.intermediate.word_140.wrapping_add(delta);
                    state.intermediate.clamp_current();
                }
            }
            (19, Actor40d800Word144(value)) => state.intermediate.word_144 = value.value,
            (20, Actor40d800Words146(value)) => {
                state.intermediate.words_146_14a = Some(value.value)
            }
            (21, Selector30Placement180(value)) => state.word_180 = u32::from(value.value),
            (22, Selector30Byte15c(value)) => state.word_15c = u32::from(value.value),
            (23, Selector30Byte160(value)) => state.word_160 = u32::from(value.value),
            (24, Selector30Byte164(value)) => state.word_164 = u16::from(value.value),
            (25, Selector30Byte18f(value)) => state.byte_18f = value.value,
            (26, Selector30Byte167(value)) => state.byte_167 = value.value,
            (27, Selector30Flag18e(value)) => state.byte_18e = u8::from(value.value != 0),
            (28, Selector30Flag166(value)) => state.byte_166 = u8::from(value.value != 0),
            (29, Selector30Flag190(value)) => state.byte_190 = u8::from(value.value != 0),
            (30, Selector30Flag191(value)) => state.byte_191 = u8::from(value.value != 0),
            (31, Selector30Flag192(value)) => state.byte_192 = u8::from(value.value != 0),
            (32, Selector30Word18c(value)) => state.word_18c = value.value,
            (16..=32, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        if dirty_0200 {
            self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
        }
        Ok(true)
    }

    /// 0048C4F0 deliberately does not call 0040E000/0047AAD0. World-start
    /// queries occur after grant handling and the +188 selector snapshot.
    pub fn finish_selector30_initialization(
        &mut self,
        object: ObjectId,
        context: ActorCommandContext<'_>,
        world: &mut WorldStartContext<'_>,
    ) -> Result<(), Selector30Error> {
        let clearable = self.selector30_actor(object)?.byte_166 != 0;
        let actor = self
            .actor(object)
            .map_err(|_| Selector30Error::InvalidObject(object))?;
        let grant = actor.word_7a;
        if clearable && grant != 0 && grant != 72 {
            if let Some(grants) = context.first_player_grants {
                if grant_is_owned(grants, u32::from(grant))? {
                    let state = self.selector30_actor_mut(object)?;
                    state.byte_166 = 0;
                    state.word_164 = 0;
                }
            }
        }
        let selector = self
            .actor(object)
            .map_err(|_| Selector30Error::InvalidObject(object))?
            .word_b4;
        self.selector30_actor_mut(object)?.word_188 = Some(selector);
        let start = world.resolve_position()?;
        let mut position = self
            .object(object)
            .map_err(|_| Selector30Error::InvalidObject(object))?
            .base
            .position_words
            .ok_or(Selector30Error::MissingPosition(object))?;
        let distance = position
            .into_iter()
            .zip(start)
            .fold(0_u32, |sum, (at, origin)| {
                let difference = i32::from(at.wrapping_sub(origin) as i16);
                sum.wrapping_add(difference.wrapping_mul(difference) as u32)
            });
        if distance < 1_000_000 {
            let selection = self.selector30_actor(object)?.byte_18f;
            let basis = self
                .actor(object)
                .map_err(|_| Selector30Error::InvalidObject(object))?
                .rotating
                .basis;
            position[0] = position[0].wrapping_add(start_offset(basis.words[7]));
            position[2] = position[2].wrapping_add(start_offset(basis.words[9]));
            world.publish_override(position, (selection != 0).then_some(selection));
            self.owned_mut(object)
                .map_err(|_| Selector30Error::InvalidObject(object))?
                .state
                .base
                .dirty_word |= 2;
            self.actor_mut(object)
                .map_err(|_| Selector30Error::InvalidObject(object))?
                .word_b4 = 2;
        }
        Ok(())
    }

    /// 0048B230 consumes the actual selected world-completion bitset and then
    /// resolves the optional one-based placement reference. +20's flag is separate.
    pub fn initialize_selector30_after_load(
        &mut self,
        object: ObjectId,
        world_completion: &[u32; 4],
    ) -> Result<(), Selector30Error> {
        self.selector30_actor(object)?;
        let signed_index = self
            .actor(object)
            .map_err(|_| Selector30Error::InvalidObject(object))?
            .byte_78 as i8;
        if crate::object_runtime::world_load::world_completion_bit(world_completion, signed_index) {
            let initial = self
                .selector30_actor(object)?
                .word_188
                .ok_or(Selector30Error::MissingInitialSelector(object))?;
            self.actor_mut(object)
                .map_err(|_| Selector30Error::InvalidObject(object))?
                .word_b4 = if initial == 2 { 18 } else { 2 };
            self.owned_mut(object)
                .map_err(|_| Selector30Error::InvalidObject(object))?
                .state
                .base
                .dirty_word |= 2;
        }
        let reference = self.selector30_actor(object)?.word_180;
        if reference != 0 {
            let handle = self
                .actor_handle_by_placement(reference as i32)
                .map_err(|_| Selector30Error::InvalidObject(object))?;
            self.selector30_actor_mut(object)?.handle_184 = handle;
        }
        Ok(())
    }

    /// 0048C470's code 0x15: the signed WORD +164 drops by the argument's
    /// low WORD; true when it reaches 0 or below, where native sets the
    /// first player's silent command 9.
    pub(in crate::object_runtime) fn selector30_count_down(
        &mut self,
        object: ObjectId,
        argument: u32,
    ) -> Result<bool, ObjectError> {
        let state = self
            .selector30_actor_mut(object)
            .map_err(|_| ObjectError::LinkMessageBoundary(object))?;
        let count = (state.word_164 as i16).wrapping_sub(argument as i16);
        state.word_164 = count as u16;
        Ok(count < 1)
    }

    /// 0048C470, selector 30's +98. Codes 0xD, 0x14 and 0x15 have their own
    /// arms, which stay boundaries here; every other code, the link messages
    /// among them, runs the family's 0040DE70.
    pub(super) fn selector30_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        if matches!(code, 0xd | 0x14 | 0x15) {
            return Err(ObjectError::LinkMessageBoundary(object));
        }
        self.family40d800_message(object, code)
    }
}

fn start_offset(bits: u32) -> u16 {
    let integral = (f64::from(f32::from_bits(bits)) * -450.0).trunc();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as u16
    } else {
        0 // Masked x87 integer-indefinite from __ftol has zero low WORD.
    }
}

mod pass;
pub use pass::{GateEntry, GateRecord, Selector30PassError, Selector30PassHost};
mod update;
pub use update::{Selector30TickRequest, Selector30UpdateError, Selector30UpdateHost};

#[cfg(test)]
#[path = "selector30_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selector30_native_tests.rs"]
mod native_tests;
