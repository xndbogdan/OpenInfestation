//! Selector 64: constructor 004407C0 over 004404F0 (the rotating visual
//! 0047A490 plus an animation DWORD +6C and bytes +70..+72), final vtable
//! 00522A28, a course gate.
//!
//! The update +24 (00440520) eases +6C, the model value (004407B0), up to
//! 0xFFFF once the gate is passed and down to 0 before. While the first
//! player's primary actor is alive (+C4 = 3) and nearer than the gate's +28
//! extent, it records which side of the gate the actor is on (0047A660:
//! local Z above zero sets +70, otherwise +71). Seeing both sides passes the
//! gate through virtual +4C (00440810), once: a player's actor gets a 5000
//! repair (0046B2B0 kind 4) and sound 1 at the gate, and the linked actor
//! message 0x15 with the operand's second DWORD. The draw +2C (00440640)
//! lights the model with the world's H12 light, X and Z quartered, in style
//! 0x0E, or 0x28 once passed.

use std::sync::Arc;

use super::{
    ActorExtension, ActorMessage, ObjectArena, ObjectClass, ObjectError, ObjectId,
    PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest,
    PlayerCommandContext, PlayerCommandError, PlayerCommandOperation, PlayerCommandRequest,
    PlayerId, RetainedInstanceCommand, RotatingVisual,
};

/// A gate's state past the rotating visual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector64State {
    pub rotating: RotatingVisual,
    /// +6C, the animation the model reads; zeroed by 004404F0.
    pub animation_6c: i32,
    /// +70/+71: the actor was seen on the positive/other side.
    pub side_70: u8,
    pub side_71: u8,
    /// +72: the gate has been passed.
    pub passed_72: u8,
    /// +74, command 3's operand: a placement reference (an actor handle
    /// after load) and the message's argument.
    pub link_74: Option<RetainedInstanceCommand>,
}

impl Selector64State {
    pub(super) fn new() -> Self {
        Self {
            rotating: RotatingVisual::new(),
            animation_6c: 0,
            side_70: 0,
            side_71: 0,
            passed_72: 0,
            link_74: None,
        }
    }

    /// 004407B0: every selector reads +6C's low WORD.
    pub(super) fn model_value(&self) -> u16 {
        self.animation_6c as u16
    }
}

/// The message the gate's +4C sends its linked actor.
pub const SELECTOR64_LINK_MESSAGE: i32 = 0x15;

impl ObjectArena {
    pub fn selector64(&self, object: ObjectId) -> Result<&Selector64State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector64(state) => Ok(state),
            _ => Err(ObjectError::NotSelector64(object)),
        }
    }

    pub(super) fn selector64_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector64State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector64(state) => Ok(state),
            _ => Err(ObjectError::NotSelector64(object)),
        }
    }

    /// 004407E0 code 3: +74 keeps the operand's address.
    pub(super) fn set_selector64_link(
        &mut self,
        object: ObjectId,
        allocation: &Arc<crate::object_runtime::InstanceCommandAllocation>,
        allocation_offset: u32,
    ) -> Result<(), ObjectError> {
        self.selector64_mut(object)?.link_74 = Some(RetainedInstanceCommand::new(
            Arc::clone(allocation),
            allocation_offset,
        ));
        Ok(())
    }

    /// 00440890, the after-load +34: the operand's placement reference
    /// becomes that placement's actor handle (0047F7B0), in place.
    pub(super) fn initialize_selector64_after_load(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let Some(link) = self.selector64(object)?.link_74.clone() else {
            return Ok(());
        };
        let image = link.allocation().runtime_allocation();
        let reference = image
            .dword(link.allocation_offset)
            .map_err(|_| ObjectError::Selector64Link(object))? as i32;
        let handle = self
            .actor_handle_by_placement(reference)
            .map_err(|_| ObjectError::Selector64Link(object))?;
        image
            .write_dword(link.allocation_offset, handle.raw())
            .map_err(|_| ObjectError::Selector64Link(object))
    }

    /// The update 00440520. Returns the actor that passed the gate: the caller runs
    /// virtual +4C ([`ObjectArena::pass_selector64_gate`]) with it, and the
    /// side bytes are already cleared, as they are after native +4C.
    pub fn tick_selector64(
        &mut self,
        object: ObjectId,
        microseconds: u32,
    ) -> Result<Option<ObjectId>, ObjectError> {
        let step = (microseconds >> 4) as i32;
        let state = self.selector64_mut(object)?;
        let animation = state.animation_6c;
        state.animation_6c = if state.passed_72 != 0 {
            let raised = step.wrapping_add(animation);
            if raised >= 0xffff {
                0xffff
            } else {
                raised
            }
        } else if animation > step {
            animation - step
        } else {
            0
        };
        let base = &self.object(object)?.base;
        let extent = base.word_28;
        let here = base
            .position_words
            .ok_or(ObjectError::Selector64PositionUnknown(object))?;
        let actor = self
            .first_player_primary_actor()?
            .filter(|&actor| self.actor(actor).is_ok_and(|state| state.word_c4 == 3));
        let mut offset = [0i16; 3];
        let distance = match actor {
            Some(actor) => {
                let there = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::Selector64PositionUnknown(actor))?;
                offset = std::array::from_fn(|axis| there[axis].wrapping_sub(here[axis]) as i16);
                let square = offset.iter().fold(0i32, |sum, &delta| {
                    sum.wrapping_add(i32::from(delta).wrapping_mul(i32::from(delta)))
                });
                if square < 0 {
                    crate::frontend_camera::integer_sqrt(((square as u32) >> 2) as i32) << 1
                } else {
                    crate::frontend_camera::integer_sqrt(square)
                }
            }
            None => 0x7fff,
        };
        let clear = |arena: &mut Self| -> Result<(), ObjectError> {
            let state = arena.selector64_mut(object)?;
            state.side_70 = 0;
            state.side_71 = 0;
            Ok(())
        };
        if distance >= i32::from(extent) {
            clear(self)?;
            return Ok(None);
        }
        // Without a live actor native still enters here when +28 exceeds
        // 0x7FFF, then transforms its stale stack offset.
        let actor = actor.ok_or(ObjectError::Selector64StaleOffset(object))?;
        let local = self
            .selector64(object)?
            .rotating
            .basis
            .inverse_words(offset);
        let state = self.selector64_mut(object)?;
        if local[2] > 0 {
            state.side_70 = 1;
        } else {
            state.side_71 = 1;
        }
        if state.side_70 == 0 || state.side_71 == 0 {
            return Ok(None);
        }
        clear(self)?;
        Ok(Some(actor))
    }
}

/// The services the gate's +4C reaches.
pub trait Selector64PassHost {
    /// `0044087E`: the linked actor's `+98(NULL, 0, 0x15, argument)` through
    /// the shared dispatcher ([`ObjectArena::receive_actor_message`]), with
    /// the effects its reply leaves to the world: a turret retires
    /// (`00402420`), a barrier grants its `+7A` and goes (`00411CE0`).
    fn message(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        message: ActorMessage,
    ) -> Result<(), String>;
    /// 0046B090 needs the process null handle.
    fn process(&self) -> Option<&crate::process_particles::factories::ParticleFactoryProcessState>;
    /// 0046B2B0(4, 5000) on the player whose actor passed, with the notice
    /// and pickup text it delivers.
    fn repair(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), String>;
    /// 00487F10(1, gate position, 0x10000, 0x10000), then 004923C0.
    fn sound(&mut self, position: [u16; 3]) -> Result<(), String>;
    /// The setter context for the first player's command a message sets.
    fn command_context(&self) -> PlayerCommandContext<'_>;
}

#[derive(Debug, thiserror::Error)]
pub enum Selector64PassError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Lookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Command(#[from] PlayerCommandError),
    #[error("gate {gate:?} pass service failed: {reason}")]
    Service { gate: ObjectId, reason: String },
}

impl ObjectArena {
    /// 00440810, virtual +4C, once per gate: a player's actor is repaired
    /// and the gate sounds; then the linked actor gets message 0x15 with
    /// the operand's second DWORD, and +72 is set.
    pub fn pass_selector64_gate<H: Selector64PassHost>(
        &mut self,
        gate: ObjectId,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector64PassError> {
        if self.selector64(gate)?.passed_72 != 0 {
            return Ok(());
        }
        let service = |reason| Selector64PassError::Service { gate, reason };
        let handle = self.actor(actor)?.handle;
        let player = self.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: host.process(),
            },
        )?;
        if let Some(player) = player {
            host.repair(self, player).map_err(service)?;
            let position = self
                .object(gate)?
                .base
                .position_words
                .ok_or(ObjectError::Selector64PositionUnknown(gate))?;
            host.sound(position).map_err(service)?;
        }
        if let Some(link) = self.selector64(gate)?.link_74.clone() {
            let image = link.allocation().runtime_allocation();
            let read = |offset: u32| {
                image
                    .dword(offset)
                    .map_err(|_| Selector64PassError::from(ObjectError::Selector64Link(gate)))
            };
            let handle = read(link.allocation_offset)?;
            let argument = read(link.allocation_offset.wrapping_add(4))?;
            let target = self
                .resolve_actor_handle(crate::object_runtime::ActorHandle::from_raw(handle))
                .map_err(|_| ObjectError::Selector64Link(gate))?;
            if let Some(target) = target {
                self.deliver_gate_message(gate, target, argument, host)?;
            }
        }
        self.selector64_mut(gate)?.passed_72 = 1;
        Ok(())
    }

    /// The target's +98 with code 0x15. Selector 30's 0048C470 counts
    /// +164 down by the argument and, at 0 or below, sets the first
    /// player's silent command 9. Every other class answers through the
    /// shared dispatcher: Shak'tahh's turrets retire and Defence
    /// Satellite's barriers go, and the rest ignore it.
    fn deliver_gate_message<H: Selector64PassHost>(
        &mut self,
        gate: ObjectId,
        target: ObjectId,
        argument: u32,
        host: &mut H,
    ) -> Result<(), Selector64PassError> {
        match &self.actor(target)?.extension {
            ActorExtension::Selector30(_) => {
                if self.selector30_count_down(target, argument)? {
                    if let Some(first) = self.first_player() {
                        self.apply_player_command(
                            PlayerCommandRequest {
                                player: first,
                                operation: PlayerCommandOperation::Set {
                                    raw_id: 9,
                                    silent: 1,
                                },
                            },
                            &host.command_context(),
                        )?;
                    }
                }
                Ok(())
            }
            _ => host
                .message(
                    self,
                    target,
                    ActorMessage {
                        sender: None,
                        distance_squared: 0,
                        code: SELECTOR64_LINK_MESSAGE,
                        argument,
                    },
                )
                .map_err(|reason| Selector64PassError::Service { gate, reason }),
        }
    }
}

#[cfg(test)]
#[path = "selector64_tests.rs"]
mod tests;
