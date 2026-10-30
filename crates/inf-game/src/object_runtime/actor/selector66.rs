//! Selector66: the shared dying actor's temporary model/effect owner.
//! Constructor405170, update4051C0, cleanup4053C0, draw4054C0, offset405490.
use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{mark_flag_change, ObjectArena, ObjectError, ObjectId};
use thiserror::Error;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector66State {
    pub saved_flags_138: Option<u32>,
    pub elapsed_13c: u32,
    pub idle_140: u32,
    pub previous_position_144: [Option<u16>; 3],
    ///Replaceable per-allocation failure containment; never native fields.
    pub update_stopped: bool,
    pub draw_stopped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector66Particle {
    Smoke(u32),
    Flash(u32),
}

pub trait Selector66Host {
    type Error;
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    fn ground_height(&mut self, arena: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, Self::Error>;
    ///The position is an actor-relative pointer, reread at the actual birth.
    fn particle(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        kind: Selector66Particle,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector66Error<E> {
    #[error(transparent)]
    Construction(#[from] ActorConstructionError),
    #[error("selector66 {0:?} update stopped after an earlier failure")]
    PreviouslyStopped(ObjectId),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("selector66 {actor:?} requires constructor-unwritten field+{offset:X}")]
    Unwritten { actor: ObjectId, offset: u16 },
    #[error("selector66 service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}

impl ObjectArena {
    ///4054C0 restores referent XYZ and flag0 only after the actual+2C
    ///callback returns. Failed prefixes and cyclic owners remain stopped.
    pub(crate) fn draw_selector66_with<E>(
        &mut self,
        actor: ObjectId,
        mut draw: impl FnMut(&mut Self, ObjectId) -> Result<(), E>,
    ) -> Result<(), Selector66Error<E>> {
        if self.selector66(actor)?.draw_stopped {
            return Err(Selector66Error::PreviouslyStopped(actor));
        }
        self.selector66_mut(actor)?.draw_stopped = true;
        let handle = self.actor(actor)?.owner_handle;
        let Some(owner) = self.resolve_actor_handle(handle)? else {
            self.dispose(actor)?;
            self.selector66_mut(actor)?.draw_stopped = false;
            return Ok(());
        };
        let saved = self.selector66_position(owner)?;
        let temporary = self.selector66_position(actor)?;
        self.owned_mut(owner)?.state.base.position_words = Some(temporary);
        self.set_flag0(owner, true)?;
        draw(self, owner).map_err(|error| Selector66Error::Service {
            address: 0x40550e,
            error,
        })?;
        self.set_flag0(owner, false)?;
        self.owned_mut(owner)?.state.base.position_words = Some(saved);
        self.selector66_mut(actor)?.draw_stopped = false;
        Ok(())
    }
    pub fn allocate_selector66(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let previous_position_144 = self.actor_allocation.initial_selector66_position();
        self.actor_mut(object)?.extension = ActorExtension::Selector66(Box::new(Selector66State {
            saved_flags_138: None,
            elapsed_13c: 0,
            idle_140: 0,
            previous_position_144,
            update_stopped: false,
            draw_stopped: false,
        }));
        Ok(object)
    }
    pub fn selector66(&self, actor: ObjectId) -> Result<&Selector66State, ObjectError> {
        match &self.actor(actor)?.extension {
            ActorExtension::Selector66(state) => Ok(state),
            _ => Err(ObjectError::NotSelector66(actor)),
        }
    }
    pub(crate) fn selector66_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector66State, ObjectError> {
        match &mut self.actor_mut(actor)?.extension {
            ActorExtension::Selector66(state) => Ok(state),
            _ => Err(ObjectError::NotSelector66(actor)),
        }
    }
    pub fn tick_selector66<H: Selector66Host + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector66Error<H::Error>> {
        if self.selector66(actor)?.update_stopped {
            return Err(Selector66Error::PreviouslyStopped(actor));
        }
        let result = self.tick_selector66_inner(actor, microseconds, host, rng);
        if result.is_err() {
            self.selector66_mut(actor)?.update_stopped = true;
        }
        result
    }
    fn tick_selector66_inner<H: Selector66Host + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector66Error<H::Error>> {
        self.selector66(actor)?;
        let source = self.actor(actor)?.owner_handle;
        let Some(owner) = self.resolve_actor_handle(source)? else {
            self.dispose(actor)?;
            return Ok(());
        };
        if self.selector66(actor)?.elapsed_13c == 0 {
            let model = self.actor(owner)?.rotating.visual.model_word;
            self.owned_mut(actor)?.state.base.dirty_word |= 0x100;
            self.actor_mut(actor)?.rotating.visual.model_word = model;
            host.refresh_model(self, actor)
                .map_err(|error| Selector66Error::Service {
                    address: 0x4051ff,
                    error,
                })?;
            self.selector66_mut(actor)?.saved_flags_138 = Some(self.object(owner)?.base.flags);
            self.set_flag0(owner, false)?;
            self.set_flag1(owner, false)?;
            let base = &mut self.owned_mut(owner)?.state.base;
            mark_flag_change(base, base.flags & 0xffff_fedb);
            let captured = self
                .selector66(actor)?
                .saved_flags_138
                .expect("written above");
            let base = &mut self.owned_mut(actor)?.state.base;
            mark_flag_change(base, (base.flags & !0x100) | (captured & 0x100));
            mark_flag_change(base, base.flags & !0x20);
            self.selector66_mut(actor)?.elapsed_13c = 1;
            return Ok(());
        }
        let elapsed = self
            .selector66(actor)?
            .elapsed_13c
            .wrapping_add(microseconds);
        self.selector66_mut(actor)?.elapsed_13c = elapsed;
        if self.object(actor)?.base.flags & 0x100 == 0 {
            let xyz = self.selector66_position(actor)?;
            let height = host
                .ground_height(self, [xyz[0], xyz[2]])
                .map_err(|error| Selector66Error::Service {
                    address: 0x4052c6,
                    error,
                })?;
            let mut xyz = self.selector66_position(actor)?;
            if (xyz[1] as i16) < (height as i16) {
                xyz[1] = height;
                self.owned_mut(actor)?.state.base.position_words = Some(xyz);
                if (self.actor(actor)?.words_8e_92[1] as i16) < 0 {
                    self.actor_mut(actor)?.words_8e_92[1] = 0;
                }
            }
        }
        let xyz = self.selector66_position(actor)?;
        self.owned_mut(owner)?.state.base.position_words = Some(xyz);
        let mut moved = false;
        for (axis, current) in xyz.into_iter().enumerate() {
            let previous = self.selector66(actor)?.previous_position_144[axis].ok_or(
                Selector66Error::Unwritten {
                    actor,
                    offset: 0x144 + axis as u16 * 2,
                },
            )?;
            if previous != current {
                moved = true;
                break;
            }
        }
        if moved {
            self.selector66_mut(actor)?.previous_position_144 = xyz.map(Some);
        }
        host.particle(
            self,
            actor,
            Selector66Particle::Smoke(u32::from(moved)),
            rng,
        )
        .map_err(|error| Selector66Error::Service {
            address: if moved { 0x405353 } else { 0x40532e },
            error,
        })?;
        if moved {
            host.particle(self, actor, Selector66Particle::Flash(2), rng)
                .map_err(|error| Selector66Error::Service {
                    address: 0x405361,
                    error,
                })?;
        }
        if self.selector66(actor)?.elapsed_13c < 6_000_000 && moved {
            self.selector66_mut(actor)?.idle_140 = 0;
        } else if self.selector66(actor)?.idle_140 < 500_000 {
            let idle = self.selector66(actor)?.idle_140.wrapping_add(microseconds);
            self.selector66_mut(actor)?.idle_140 = idle;
        } else {
            self.dispose(actor)?;
        }
        Ok(())
    }
    pub fn cleanup_selector66(
        &mut self,
        actor: ObjectId,
    ) -> Result<(), Selector66Error<std::convert::Infallible>> {
        self.selector66(actor)?;
        let source = self.actor(actor)?.owner_handle;
        let Some(owner) = self.resolve_actor_handle(source)? else {
            return Ok(());
        };
        let flags = self
            .selector66(actor)?
            .saved_flags_138
            .ok_or(Selector66Error::Unwritten {
                actor,
                offset: 0x138,
            })?
            & !3;
        self.selector66_mut(actor)?.saved_flags_138 = Some(flags);
        let base = &mut self.owned_mut(owner)?.state.base;
        mark_flag_change(base, base.flags | flags | 0x400000);
        self.set_flag0(owner, true)?;
        self.set_flag1(owner, true)?;
        Ok(())
    }
    pub fn offset_selector66(
        &mut self,
        actor: ObjectId,
        selector: u32,
        displacement: Option<[u16; 3]>,
    ) -> Result<bool, ObjectError> {
        self.selector66(actor)?;
        if selector == 5 {
            if let Some(displacement) = displacement {
                let xyz = self.selector66_position(actor)?;
                self.owned_mut(actor)?.state.base.position_words = Some(std::array::from_fn(|i| {
                    xyz[i].wrapping_add(displacement[i])
                }));
            }
        }
        Ok(false)
    }
    pub(crate) fn selector66_position(&self, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))
    }
}
