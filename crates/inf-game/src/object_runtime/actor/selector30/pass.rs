//! Selector 30's pass, 0048BE70: what the gate does with an actor that
//! crossed it. The actor's player takes the gate's grant +7A, and the
//! actor's hold (+64, the vehicle's +1A4) drops every item built from
//! resource 0x31. A gate linked to another (+180, the linked gate's handle
//! +184, which online is first found by placement) then moves the actor
//! there: its offset from this gate, seen from the gate turned half a
//! circle about its Y and pushed 10 forward, is laid out again about the
//! linked gate, and its velocity and basis turn the same way; the turned
//! spin lands on the gate itself. Any other gate ends the world (00487560)
//! after setting its completion bits (+167).

use inf_formats::native_basis::NativeBasisMath;
use thiserror::Error;

use super::Selector30Error;
use crate::object_runtime::actor::derived::kinematics::{
    transform_local_dwords, transform_world_dwords,
};
use crate::object_runtime::actor::ActorConstructionError;
use crate::object_runtime::{
    DerivedActorError, ObjectArena, ObjectError, ObjectId, PlayerAttachError, PlayerId,
};

/// 0048C0DE: half a circle, the turn about the gate's Y.
const HALF_TURN: i32 = 0x8000;
/// The aliens' team.
const ALIEN_TEAM: u32 = 0x12;

/// What `0048BD90` leaves to the world after a scientist reaches a gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateEntry {
    /// The gate changed sides: its placement's completion bit is set
    /// (`0048EAD0`) or cleared (`0048EB30`), index `+78`.
    pub record: Option<GateRecord>,
    /// A `+18E` gate then passes the scientist (`0048BE70`).
    pub pass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateRecord {
    Mark(i8),
    Clear(i8),
}
/// 0048C131: how far past the linked gate's face the actor lands.
const EXIT_DEPTH: i16 = 10;
/// 0048BEF3: the resource whose held items a gate takes off the actor.
const DROPPED_RESOURCE: u16 = 0x31;
/// 0048C3B1 and 0048C430: the completion bits +167 sets, 2 for a gate
/// whose +167 is 1 and 8 for any other nonzero +167.
const STATUS_GATE: u32 = 2;
const STATUS_OTHER: u32 = 8;
/// 0048C402: the world whose gate grants the BUGGY RACING cheat (0x80).
const CHEAT_WORLD: u32 = 4;
const CHEAT_FLAG: u32 = 0x80;

impl ObjectArena {
    /// `00468191`: a partner's `+D0` answered 0 (a gate always does), so
    /// the scientist's `+22E` bit 0 sends it looking for another target.
    pub fn scientist_refused(&mut self, scientist: ObjectId) -> Result<(), ObjectError> {
        self.family428650_actor_mut(scientist)?.word_22e |= 1;
        Ok(())
    }

    /// `0048BD90`, the gate's `+D0`, where a scientist that targeted it
    /// arrives. A gate not held by `+192` and on another team than the
    /// scientist takes the scientist's team (`+B4`, dirty 2), so its scan
    /// no longer keeps to team 0x12 and it lights for the player. The
    /// world's record of its placement follows: set while the gate's team
    /// differs from its first (`+188`) on the alien side, cleared when it
    /// returns. Answers 0.
    pub fn gate_takes_scientist(
        &mut self,
        gate: ObjectId,
        scientist: ObjectId,
    ) -> Result<GateEntry, Selector30Error> {
        let invalid = |_| Selector30Error::InvalidObject(gate);
        let state = self.selector30_actor(gate)?;
        let (held, original, pass) = (state.byte_192, state.word_188, state.byte_18e != 0);
        let mut entry = GateEntry { record: None, pass };
        let team = self
            .actor(scientist)
            .map_err(|_| Selector30Error::InvalidObject(scientist))?
            .word_b4;
        if held == 0 && self.actor(gate).map_err(invalid)?.word_b4 != team {
            let original = original.ok_or(Selector30Error::MissingInitialSelector(gate))?;
            self.owned_mut(gate).map_err(invalid)?.state.base.dirty_word |= 2;
            self.actor_mut(gate).map_err(invalid)?.word_b4 = team;
            let index = self.actor(gate).map_err(invalid)?.byte_78 as i8;
            entry.record = Some(if (original == ALIEN_TEAM) != (team == ALIEN_TEAM) {
                GateRecord::Mark(index)
            } else {
                GateRecord::Clear(index)
            });
        }
        Ok(entry)
    }
}

pub trait Selector30PassHost {
    type Error;
    /// 0046B090 of the actor's handle: the player whose unit it is.
    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, Self::Error>;
    /// 004750C0 on the player's grants +9C.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
    ) -> Result<(), Self::Error>;
    /// The hold's +0 (00412110 over 00412880): the vehicle drops the item
    /// in slot `index`.
    fn drop_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(), Self::Error>;
    /// G+250, the current world, and its completion DWORD G+1EC[world].
    fn world_status(&mut self) -> Result<(u32, u32), Self::Error>;
    /// G+1EC[G+250] = `status`.
    fn set_world_status(&mut self, status: u32) -> Result<(), Self::Error>;
    /// 0048C407: `00463AB0()+84 |= bit`, the settings' cheat word.
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), Self::Error>;
    /// The port's Pelagia gate ([`crate::pelagia`]): world 1 by the
    /// Teleport's load, never 00487560, whose selection 1 is the campaign's
    /// end.
    fn enter_pelagia(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// 0048C446: 00487560(world, start, actor), the world's end.
    fn request_world(
        &mut self,
        arena: &mut ObjectArena,
        world: u32,
        start: u16,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// 0048C2A7: 00487D70(position, 0xF00, 0), the world flash.
    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), Self::Error>;
    /// 0048C2AF: 00478E00's camera, created if there is none.
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, Self::Error>;
    /// 0048C2E9..0048C356: the creation listeners' walk over the actor
    /// (00461BE0, 00461C00, 0048C6A0).
    fn notify_listeners(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector30PassError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Selector30(#[from] Selector30Error),
    #[error(transparent)]
    Registry(#[from] ActorConstructionError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    /// 0048BF93: online, with the actor list empty the link is an
    /// uninitialized stack word.
    #[error(
        "gate {gate:?}'s online link found an empty actor list (0048BF93 reads stack residue)"
    )]
    OnlineLinkEmptyList { gate: ObjectId },
    #[error("gate {gate:?}'s pass service at {address:08X} failed: {error}")]
    Host {
        gate: ObjectId,
        address: u32,
        error: E,
    },
}

impl ObjectArena {
    /// 0048BE70. Answers whether the actor moved to a linked gate (AL).
    pub fn pass_selector30<H: Selector30PassHost>(
        &mut self,
        gate: ObjectId,
        actor: ObjectId,
        network_active: bool,
        math: &NativeBasisMath,
        host: &mut H,
    ) -> Result<bool, Selector30PassError<H::Error>> {
        let at = |address| {
            move |error| Selector30PassError::Host {
                gate,
                address,
                error,
            }
        };
        // 0048BE95: a remote actor does not pass.
        if self.object(actor)?.base.flags & 0x8000_0000 != 0 {
            return Ok(false);
        }
        let player = host.find_player(self, actor).map_err(at(0x0048_bea9))?;
        let grant = self.actor(gate)?.word_7a;
        if let (Some(player), true) = (player, grant != 0) {
            host.grant(self, player, grant).map_err(at(0x0048_bec8))?;
        }
        // 0048BECD: the actor's +64 and its hold. The count is read again
        // after every drop.
        if self.actor_vehicle_component(actor)?.is_some() {
            let mut index = 0;
            while index < self.selector2_actor(actor)?.handles_1a4.handles.len() {
                let handle = self.selector2_actor(actor)?.handles_1a4.handles[index];
                if let Some(item) = self.resolve_actor_handle(handle)? {
                    if self.object(item)?.base.resource_word == DROPPED_RESOURCE {
                        host.drop_item(self, actor, index)
                            .map_err(at(0x0048_bf00))?;
                    }
                }
                index += 1;
            }
        }
        let state = self.selector30_actor(gate)?;
        if state.word_180 != 0 {
            let (reference, handle) = (state.word_180, state.handle_184);
            let null = self
                .actor_handles()
                .ok_or(ActorConstructionError::MissingRegistry)?
                .null_handle();
            let linked = if network_active && handle == null {
                self.online_gate_link(gate, reference)?
            } else {
                self.resolve_actor_handle(handle)?
            };
            let Some(linked) = linked else {
                // 0048C36A: a lost link is forgotten.
                self.selector30_actor_mut(gate)?.word_180 = 0;
                return Ok(false);
            };
            self.teleport_through_selector30(gate, linked, actor, player, math, host)?;
            return Ok(true);
        }
        // 0048C37F: only a player's vehicle leaves the world; anything else
        // that crosses is disposed.
        if player.is_none() || self.actor_vehicle_component(actor)?.is_none() {
            self.dispose(actor)?;
            return Ok(false);
        }
        if self.selector30_actor(gate)?.pelagia {
            host.enter_pelagia(self, actor).map_err(at(0x0048_c446))?;
            return Ok(false);
        }
        let state = self.selector30_actor(gate)?;
        let (completion, wanted, world, start) = (
            state.byte_167,
            state.word_18c,
            state.word_15c,
            state.word_160 as u16,
        );
        if completion != 0 {
            let (current, status) = host.world_status().map_err(at(0x0048_c3aa))?;
            if status & STATUS_GATE == 0 {
                if completion == 1 {
                    let owned = wanted == 0 || {
                        let player = player.expect("checked above");
                        let grants = self.world_player(player)?.grants_9c;
                        inf_formats::grant_is_owned(&grants, u32::from(wanted))?
                    };
                    if owned {
                        host.set_world_status(status | STATUS_GATE)
                            .map_err(at(0x0048_c3fa))?;
                        if current == CHEAT_WORLD {
                            host.grant_cheat_flag(CHEAT_FLAG).map_err(at(0x0048_c41a))?;
                        }
                    }
                } else {
                    host.set_world_status(status | STATUS_OTHER)
                        .map_err(at(0x0048_c433))?;
                }
            }
        }
        host.request_world(self, world, start, actor)
            .map_err(at(0x0048_c446))?;
        Ok(false)
    }

    /// `0048BF29`: online, a link not yet known (+184 the NULL) is the
    /// first actor on the list (G+48) whose placement WORD +2A is +180's
    /// low WORD less one, and +184 keeps its handle. With none, the walk
    /// ends holding the last actor it read, and the pass takes that one.
    fn online_gate_link<E>(
        &mut self,
        gate: ObjectId,
        reference: u32,
    ) -> Result<Option<ObjectId>, Selector30PassError<E>> {
        let wanted = (reference as u16).wrapping_sub(1);
        let mut last = None;
        for &actor in self.actor_list() {
            if self.object(actor)?.base.placement_word == wanted {
                let handle = self.actor(actor)?.handle;
                self.selector30_actor_mut(gate)?.handle_184 = handle;
                return Ok(Some(actor));
            }
            last = Some(actor);
        }
        // 0048BF93: an empty list leaves a stack word nothing wrote.
        last.map(Some)
            .ok_or(Selector30PassError::OnlineLinkEmptyList { gate })
    }

    /// 0048BFA1..0048C35B: the actor laid out about the linked gate.
    fn teleport_through_selector30<H: Selector30PassHost>(
        &mut self,
        gate: ObjectId,
        linked: ObjectId,
        actor: ObjectId,
        player: Option<PlayerId>,
        math: &NativeBasisMath,
        host: &mut H,
    ) -> Result<(), Selector30PassError<H::Error>> {
        let at = |address| {
            move |error| Selector30PassError::Host {
                gate,
                address,
                error,
            }
        };
        let position = |arena: &ObjectArena, object: ObjectId| {
            arena
                .object(object)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(object))
        };
        // 0048C09E..0048C0E7: a copy of the gate's basis (counter too),
        // turned half a circle by 00491530.
        let mut mirror = self.actor(gate)?.rotating.basis;
        math.rotate(&mut mirror, HALF_TURN);
        let linked_basis = self.actor(linked)?.rotating.basis;
        let gate_position = position(self, gate)?;
        let linked_position = position(self, linked)?;
        let actor_position = position(self, actor)?;
        // 0048C0EC..0048C164: the offset through the turned gate (00490A80),
        // 10 further, then out through the linked gate (0047A6C0).
        let offset: [i16; 3] = std::array::from_fn(|axis| {
            actor_position[axis].wrapping_sub(gate_position[axis]) as i16
        });
        let mut local = mirror.inverse_words(offset);
        local[2] = local[2].wrapping_add(EXIT_DEPTH);
        let world = linked_basis.transform_words(local);
        let landing: [u16; 3] =
            std::array::from_fn(|axis| world[axis].wrapping_add(linked_position[axis]));
        // 0048C15E..0048C1B3: the velocity the same way; 0047B510 adds the
        // difference to +8E.
        let velocity = self.actor(actor)?.words_8e_92;
        let turned = linked_basis.transform_words(mirror.inverse_words(velocity.map(|w| w as i16)));
        // 0048C1B8..0048C1F9: the spin +94 through 00490870 and 0047A6E0.
        // 0048C1ED stores the turned spin through EBP, which holds the gate
        // (0048BE89), so the gate's +94..+9C take it and the actor keeps its
        // own, as Infest04's warp shows (`gate-pass04.txt`).
        let spin = transform_local_dwords(
            linked_basis,
            transform_world_dwords(mirror, self.actor(actor)?.words_94_9c.map(|w| w as i32)),
        );
        // 0048C1FC..0048C228: the basis (00490DC0, then 00490BC0).
        let basis = linked_basis.expand(mirror.compose(self.actor(actor)?.rotating.basis));
        let state = self.actor_mut(actor)?;
        for axis in 0..3 {
            state.words_8e_92[axis] =
                velocity[axis].wrapping_add(turned[axis].wrapping_sub(velocity[axis]));
        }
        state.rotating.basis = basis;
        self.actor_mut(gate)?.words_94_9c = spin.map(|w| w as u32);
        self.owned_mut(actor)?.state.base.position_words = Some(landing);
        // 0048C27B: +5C, which only Selector12's table answers.
        if self.selector12_actor(actor).is_ok() {
            self.family428650_actor_mut(actor)?.word_22e |= 0x0c;
        }
        if player.is_some() {
            host.flash(self, gate_position).map_err(at(0x0048_c2a7))?;
            let camera = host.camera(self).map_err(at(0x0048_c2af))?;
            self.selector16_mut(camera)?.byte_187 = 1;
            self.owned_mut(linked)?.state.base.dirty_word |= 2;
            self.actor_mut(linked)?.word_b4 = 2;
        }
        host.notify_listeners(self, actor).map_err(at(0x0048_c2e9))
    }
}

#[cfg(test)]
#[path = "pass_tests.rs"]
mod tests;
