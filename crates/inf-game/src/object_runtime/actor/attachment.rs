//! 00405420: spawn resource46, inherit the current parent fields, then invoke
//! the new actor's actual zero-delta update. Raw and component results differ.
use crate::object_runtime::{
    ActorHandle, ObjectArena, ObjectError, ObjectId, TargetNullHandleSource,
};
use thiserror::Error;

pub trait ActorAttachmentHost: TargetNullHandleSource {
    type Error;
    ///460310 resource46, EDX0, live parent+32, NULL rotation, placement-1,
    ///mode0 and the already captured current process NULL argument.
    fn create_attachment(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn resolve_attachment_actor(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn update_attachment(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ActorAttachmentError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("00405427 requires current process NULL0054E894")]
    MissingProcessNull,
    #[error(
        "actor attachment service at {address:08X} stopped, retained object {object:?}: {error}"
    )]
    Service {
        address: u32,
        object: Option<ObjectId>,
        error: E,
    },
}

impl ObjectArena {
    pub fn attach_actor_effect<H: ActorAttachmentHost + ?Sized>(
        &mut self,
        parent: Option<ObjectId>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), ActorAttachmentError<H::Error>> {
        let Some(parent) = parent else { return Ok(()) };
        let source = host
            .target_null_handle()
            .ok_or(ActorAttachmentError::MissingProcessNull)?;
        let Some(raw) = host
            .create_attachment(self, parent, source, rng)
            .map_err(|error| ActorAttachmentError::Service {
                address: 0x40543e,
                object: None,
                error,
            })?
        else {
            return Ok(());
        };
        let Some(child) = host.resolve_attachment_actor(self, raw).map_err(|error| {
            ActorAttachmentError::Service {
                address: 0x40544b,
                object: Some(raw),
                error,
            }
        })?
        else {
            return Ok(());
        };
        let (identity, unknown) = {
            let parent = self.actor(parent)?;
            (parent.word_6c, parent.unknown_6c)
        };
        let child_state = self.actor_mut(child)?;
        child_state.word_6c = identity;
        child_state.unknown_6c = unknown;
        let source = self.actor(parent)?.handle;
        self.owned_mut(child)?.state.base.dirty_word |= 4;
        self.actor_mut(child)?.owner_handle = source;
        let dirty = self.object(child)?.base.dirty_word | 2;
        let owner = self.actor(parent)?.word_b4;
        self.actor_mut(child)?.word_b4 = owner;
        self.owned_mut(child)?.state.base.dirty_word = dirty;
        host.update_attachment(self, child, 0, rng)
            .map_err(|error| ActorAttachmentError::Service {
                address: 0x405485,
                object: Some(child),
                error,
            })
    }
}
