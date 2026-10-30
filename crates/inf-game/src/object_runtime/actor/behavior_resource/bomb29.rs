//! Selector 29, the Bomb: 00455560(2000)'s class with the final table
//! 00525AB4 (case 0x1D at 00460AA5, 0x150 bytes; constructor 00457FE0,
//! which disarms it, +138 = 0, and copies the fuse +13A to +14C). Its table
//! is the missile's 00525B88 in its contacts (+A4 00455640, +A8 00455660)
//! and its state 0 base (00455680), and the shared actor's elsewhere, but
//! for the commands +04 (00458C40), the initializer +0C (00458CA0), the
//! update +24 (00458C80), the capture +8C (00458CC0), the messages +98
//! (00458010), the death +AC (00458F60) and the state-0 body +B0 (00458DA0).
//!
//! A Bomb lies on the ground (00479DD0 at its initialization) and does
//! nothing until armed. Message 9, or 10 from its live owner (the
//! Detonator), arms it with a spent fuse, so its next update sets it off;
//! flag 0x80 keeps it unarmed. Taken into a hold, it is disarmed with its
//! fuse back at +14C and the holder as its owner, and the first player sees
//! hint 0x1A; dropped, hint 0x19. An armed Bomb that touches anything goes
//! off too. Going off (state 0) builds its visual +148 through +4C, bursts,
//! then blasts its block +13E from its owner, explodes and is disposed of.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};

use super::missile::{MissileError, MissileLifecycleHost, MissileResult};
use super::{BehaviorResourceClass, Family455560State, Family455560Tail};
use crate::object_runtime::{
    ActorChildResource, ActorCommandContext, ActorCommandError, ActorHandle, ObjectArena,
    ObjectError, ObjectId,
};
use crate::process_particles::births::ParticleBurst;
use crate::terrain_runtime::TerrainRuntime;

/// The placement factory's case for the Bomb.
pub const BOMB_SELECTOR: u16 = 29;

/// 00458CEE and 00458CF4: the first player's hints when a Bomb is taken
/// and when it is dropped (0046C760, not silent).
pub const BOMB_TAKEN_HINT: u32 = 0x1a;
pub const BOMB_DROPPED_HINT: u32 = 0x19;

/// 00458D5D..00458D7B: sixteen type-1 debris round a type-1 centre within
/// 20, speed 8, no flags.
fn bomb_burst(position: [u16; 3]) -> ParticleBurst {
    ParticleBurst {
        debris_count: 0x10,
        position,
        placement_radius: 0x14,
        central_type: 1,
        debris_types: [1, 1],
        speed: 8,
        flags: 0,
    }
}

/// What 00458010 leaves to its caller; it answers 0 either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bomb29Message {
    None,
    /// 00458042..0045804B: message 10 from no sender or a dead one, in a
    /// network game: 00479FA0 on the Bomb.
    Dispose,
}

/// The services the Bomb's state 0 adds to the missiles'.
pub trait BombLifecycleHost: MissileLifecycleHost {
    /// `+4C(0, resource, 0)`, the shared child factory `0047AAF0`. 00458D28
    /// pushes the WORD +148 in a register whose high half is residue.
    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        bomb: ObjectId,
        resource: ActorChildResource,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// `0048A8B0`, a particle burst.
    fn particles(
        &mut self,
        arena: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

impl ObjectArena {
    pub fn is_bomb29(&self, object: ObjectId) -> bool {
        self.bomb29(object).is_ok()
    }

    pub(crate) fn bomb29(&self, object: ObjectId) -> Result<&Family455560State, ObjectError> {
        let state = self.behavior_resource(object)?;
        match &state.class {
            BehaviorResourceClass::Family455560(family) if state.selector == 29 => Ok(family),
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    fn bomb29_mut(&mut self, object: ObjectId) -> Result<&mut Family455560State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            crate::object_runtime::ActorExtension::BehaviorResource(state)
                if state.selector == 29 =>
            {
                match &mut state.class {
                    BehaviorResourceClass::Family455560(family) => Ok(family),
                    BehaviorResourceClass::Family485f50(_) => {
                        Err(ObjectError::NotBehaviorResource(object))
                    }
                }
            }
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    /// The fuse length +14C.
    pub fn bomb29_fuse_length(&self, bomb: ObjectId) -> Result<u32, ObjectError> {
        match self.bomb29(bomb)?.tail {
            Family455560Tail::Selector29 { dword_14c } => Ok(dword_14c),
            _ => Err(ObjectError::NotBehaviorResource(bomb)),
        }
    }

    /// 00458C40, the Bomb's +04: codes through 15 (signed) are the shared
    /// 0047ADA0's, 16 stores the fuse and every other is refused.
    pub fn apply_bomb29_command(
        &mut self,
        bomb: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(29) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.bomb29(bomb)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(bomb, command, context);
        }
        match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector29Fuse(value)) => {
                self.apply_bomb29_fuse(bomb, value.value)?;
                Ok(true)
            }
            (16, _) => Err(ActorCommandError::Payload(code)),
            _ => Ok(false),
        }
    }

    /// 00458C40 code 16: the DWORD at +14C and its low WORD at +13A.
    pub(in crate::object_runtime) fn apply_bomb29_fuse(
        &mut self,
        bomb: ObjectId,
        value: u32,
    ) -> Result<(), ObjectError> {
        let state = self.bomb29_mut(bomb)?;
        match &mut state.tail {
            Family455560Tail::Selector29 { dword_14c } => *dword_14c = value,
            _ => return Err(ObjectError::NotBehaviorResource(bomb)),
        }
        state.word_13a = value as u16;
        Ok(())
    }

    /// 00458CA0's 00479DD0, after the family's initializer: the Bomb's
    /// height becomes the ground's under it (00450A30).
    pub(in crate::object_runtime) fn drop_bomb29(
        &mut self,
        bomb: ObjectId,
        terrain: &TerrainRuntime,
    ) -> Result<(), ObjectError> {
        self.bomb29(bomb)?;
        let [x, _, z] = self
            .object(bomb)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(bomb))?;
        let y = terrain.height_at([x, z]) as u16;
        self.owned_mut(bomb)?.state.base.position_words = Some([x, y, z]);
        Ok(())
    }

    /// 00458C80, the Bomb's +24: an armed Bomb runs the fuse (004555B0),
    /// which counts +13A down by the delta's milliseconds and +13C up, and
    /// at zero goes to state 0 (0047D670(0, +84, 0)).
    pub fn tick_bomb29<H: MissileLifecycleHost>(
        &mut self,
        bomb: ObjectId,
        delta: u32,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        if self.bomb29(bomb)?.byte_138 == 0 {
            return Ok(());
        }
        let step = delta >> 10;
        let state = self.bomb29_mut(bomb)?;
        state.word_13a = if u32::from(state.word_13a) < step {
            0
        } else {
            state.word_13a.wrapping_sub(step as u16)
        };
        state.word_13c = state.word_13c.wrapping_add(step as u16);
        if state.word_13a == 0 {
            let source = self.actor(bomb)?.handle;
            host.detonate(self, bomb, source, rng)
                .map_err(MissileError::Host)?;
        }
        Ok(())
    }

    /// 00458010, the Bomb's +98. Message 9 arms it, as does 10 from a live
    /// sender (+C4 at least 3) whose handle is the Bomb's owner +88; arming
    /// (unless flag 0x80) sets +138 and spends the fuse +13A. A 10 from no
    /// sender or a dead one disposes of it in a network game.
    pub fn bomb29_message(
        &mut self,
        bomb: ObjectId,
        sender: Option<ObjectId>,
        code: i32,
        network_active: bool,
    ) -> Result<Bomb29Message, ObjectError> {
        self.bomb29(bomb)?;
        let arm = match code {
            9 => true,
            10 => {
                let live = match sender {
                    Some(sender) if self.actor(sender)?.word_c4 as i32 >= 3 => Some(sender),
                    _ => None,
                };
                match live {
                    Some(sender) => self.actor(sender)?.handle == self.actor(bomb)?.owner_handle,
                    None => {
                        return Ok(if network_active {
                            Bomb29Message::Dispose
                        } else {
                            Bomb29Message::None
                        })
                    }
                }
            }
            _ => false,
        };
        if arm && self.object(bomb)?.base.flags & 0x80 == 0 {
            let state = self.bomb29_mut(bomb)?;
            state.byte_138 = 1;
            state.word_13a = 0;
        }
        Ok(Bomb29Message::None)
    }

    /// 00458CC0's own part, before the shared capture 0047D7C0: a nonzero
    /// low byte of the operation (a hold taking it) disarms it, refills the
    /// fuse +13A from +14C and makes the holder its owner +88. Returns the
    /// hint the first player then sees.
    pub(crate) fn bomb29_capture(
        &mut self,
        bomb: ObjectId,
        operation: u32,
        holder: ActorHandle,
    ) -> Result<u32, ObjectError> {
        let length = self.bomb29_fuse_length(bomb)?;
        if operation as u8 == 0 {
            return Ok(BOMB_DROPPED_HINT);
        }
        let state = self.bomb29_mut(bomb)?;
        state.byte_138 = 0;
        state.word_13a = length as u16;
        self.actor_mut(bomb)?.owner_handle = holder;
        Ok(BOMB_TAKEN_HINT)
    }

    /// 00455640 (+A4) and 00455660 (+A8), the missiles' contacts: an armed
    /// Bomb that touches anything goes to state 0 (0047D670(0, +84, 0)).
    pub fn bomb29_touch<H: MissileLifecycleHost>(
        &mut self,
        bomb: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        if self.bomb29(bomb)?.byte_138 == 0 {
            return Ok(());
        }
        let source = self.actor(bomb)?.handle;
        host.detonate(self, bomb, source, rng)
            .map_err(MissileError::Host)
    }

    /// 00458F60, the Bomb's +AC: state 0 with its own handle
    /// (0047D670(0, +84, 0)).
    pub fn bomb29_state_one<H: MissileLifecycleHost>(
        &mut self,
        bomb: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        self.bomb29(bomb)?;
        let source = self.actor(bomb)?.handle;
        host.detonate(self, bomb, source, rng)
            .map_err(MissileError::Host)
    }

    /// 00458DA0, the Bomb's +B0: its visual +148 through +4C (00458D20),
    /// the burst at its position, then the missiles' 00455680: unless flag
    /// bit 31, its block +13E from its owner, the explosion and the
    /// disposal.
    pub fn bomb29_state_zero<H: BombLifecycleHost>(
        &mut self,
        bomb: ObjectId,
        rng: &mut u32,
        host: &mut H,
    ) -> MissileResult<(), H::Error> {
        let resource = self.bomb29(bomb)?.dword_148 as u16;
        host.visual(
            self,
            bomb,
            ActorChildResource::RegisterLowWord(resource),
            rng,
        )
        .map_err(MissileError::Host)?;
        let position = self
            .object(bomb)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(bomb))?;
        host.particles(self, bomb_burst(position), rng)
            .map_err(MissileError::Host)?;
        self.detonate_family455560(bomb, rng, host)
    }
}

#[cfg(test)]
#[path = "bomb29_tests.rs"]
mod tests;
