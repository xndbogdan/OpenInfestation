//! 004824A0: selector36 child publication followed by its model transition.

#[cfg(test)]
mod native_receipts;

use super::{Family4236d0Class, RetainedActorAllocation};
use crate::object_runtime::{
    ActorChildError, ActorChildHost, ActorChildRequest, ActorChildResource, ObjectArena, ObjectId,
};

#[cfg(test)]
mod tests;

impl ObjectArena {
    pub fn spawn_selector36_child<H: ActorChildHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), ActorChildError<H::Error>> {
        self.selector36_actor(actor)?;
        if self.actor(actor)?.word_b4 == 0 || self.family4236d0_actor(actor)?.word_264 & 0x1000 != 0
        {
            return Ok(());
        }
        let position =
            if self.object(actor)?.base.flags & 0x0200_0000 != 0
                && self.family4236d0_actor(actor)?.word_264 & 0x0800 != 0
            {
                let words = self.selector36_actor(actor)?.words_278_27c;
                let mut position = [0; 3];
                for (axis, word) in words.into_iter().enumerate() {
                    position[axis] = word.ok_or(ActorChildError::RequiredField {
                        actor,
                        offset: 0x278 + axis as u16 * 2,
                        address: 0x0048_24d8,
                    })?;
                }
                position
            } else {
                let mut position = self.object(actor)?.base.position_words.ok_or(
                    ActorChildError::RequiredField {
                        actor,
                        offset: 0x32,
                        address: 0x0048_24f0,
                    },
                )?;
                position[1] = position[1].wrapping_add(self.object(actor)?.base.word_28);
                position
            };
        let payload = self.selector36_child_payload::<H::Error>(actor, 0x0048_2509)?;
        let resource = payload
            .allocation()
            .word(payload.allocation_offset.wrapping_add(0x78))?;
        let mut child = None;
        self.create_actor_child(
            actor,
            ActorChildRequest {
                resource: ActorChildResource::StackPositionLowWord(resource),
                position: Some(position),
            },
            Some(&mut child),
            host,
            rng,
        )?;
        let Some(child) = child else { return Ok(()) };

        // Reacquire the actual +274 after virtual+4C, which may have changed
        // parent payload/size/flags. The radius shift is unsigned WORD SHR.
        let payload = self.selector36_child_payload::<H::Error>(actor, 0x0048_2531)?;
        let flags = payload
            .allocation()
            .byte(payload.allocation_offset.wrapping_add(0x74))?;
        let radius = self.object(actor)?.base.word_28;
        let y = if flags & 4 != 0 {
            radius.wrapping_mul(2)
        } else {
            radius >> 1
        };
        let z = if flags & 8 != 0 {
            radius.wrapping_mul(2)
        } else {
            0
        };
        self.add_actor_local_linear_velocity(child, [0, y, z])?;

        let flags = self.family4236d0_actor(actor)?.word_264;
        // MOV EDX,[actor+274] captures even NULL; only the later WORD read
        // dereferences it. The flag publication therefore precedes failure.
        let payload = self.selector36_actor(actor)?.payload_274.clone();
        self.family4236d0_actor_mut(actor)?.word_264 = (flags & 0xf7ff) | 0x1000;
        let payload = payload.ok_or(ActorChildError::RequiredField {
            actor,
            offset: 0x274,
            address: 0x0048_259a,
        })?;
        let model = payload
            .allocation()
            .word(payload.allocation_offset.wrapping_add(0x76))?;
        self.owned_mut(actor)?.state.base.dirty_word |= 0x100;
        self.actor_mut(actor)?.rotating.visual.model_word = model;
        host.refresh_child_parent_model(self, actor)
            .map_err(ActorChildError::Service)?;
        self.actor_mut(actor)?.word_74 = 0;
        let Family4236d0Class::Selector36(state) = &mut self.family4236d0_actor_mut(actor)?.class
        else {
            unreachable!("selector36 checked above")
        };
        state.word_27e = u16::MAX;
        Ok(())
    }

    fn selector36_child_payload<E>(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<RetainedActorAllocation, ActorChildError<E>> {
        self.selector36_actor(actor)?
            .payload_274
            .clone()
            .ok_or(ActorChildError::RequiredField {
                actor,
                offset: 0x274,
                address,
            })
    }
}
