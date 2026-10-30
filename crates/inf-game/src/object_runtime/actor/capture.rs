//! Direct shared capture/release 0047D7C0. This is not virtual+8C dispatch:
//! Selector12/13 and other wrappers retain their separate reached prefixes.
//! Caller saved storage is independent; raw aliases/list-owner rebinding and
//! arena replacement/ObjectId ABA are outside the typed ownership domain.

use thiserror::Error;

use crate::{
    object_runtime::{
        mark_flag_change, ActorConstructionError, ActorHandle, ObjectArena, ObjectError, ObjectId,
        PlayerId,
    },
    process_particles::factories::ParticleFactoryProcessState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorCaptureRequest {
    pub actor: ObjectId,
    /// Original stack DWORD.0047D7C0 consumes only its low BYTE:100h releases,
    /// while101h captures. High bits are not a Boolean mode discriminator.
    pub operation: u32,
    pub source: ActorHandle,
}

/// The caller's independent four-byte saved-flags allocation. None is missing
/// backing; Some retains each known/unknown byte until its actual consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorCaptureSavedFlags {
    pub bytes: Option<[Option<u8>; 4]>,
}

pub struct ActorCaptureContext<'a> {
    /// Borrow the actual live process owner. No registry/default NULL fallback.
    pub process: &'a mut ParticleFactoryProcessState,
    pub saved_flags: &'a mut ActorCaptureSavedFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorCapturePlayerCommand {
    pub player: PlayerId,
    pub code: u32,
    pub value: u32,
}

pub trait ActorCaptureHost {
    type Error;

    /// Required 0046C760(second fresh 00488AA0 player,42h,0). This is a mutable
    /// lower-service boundary, not an implemented player-command body. Effects
    /// and failures retain actual arena/process/saved-storage custody.
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut ActorCaptureContext<'_>,
        request: ActorCapturePlayerCommand,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ActorCaptureError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Construction(#[from] ActorConstructionError),
    #[error("shared capture requires the caller's saved-flags storage")]
    MissingSavedFlagsStorage,
    #[error("saved-flags byte+{offset} is unknown at the reached consumer")]
    UninitializedSavedFlags { offset: u8 },
    #[error("0047D8F8 requires the actual process NULL handle0054E894")]
    MissingProcessNullHandle,
    /// A Force Field's bit in either +6C came from stack residue.
    #[error("capture compares +6C of {capturer:?} and {actor:?}, part stack residue")]
    UnknownIdentity { capturer: ObjectId, actor: ObjectId },
    #[error("shared capture service{address:08X} failed: {error}")]
    Host { address: u32, error: E },
}

enum CaptureOperation {
    Capture,
    Release,
}

impl CaptureOperation {
    fn from_native_word(value: u32) -> Self {
        if value as u8 == 0 {
            Self::Release
        } else {
            Self::Capture
        }
    }
}

impl ActorCaptureSavedFlags {
    fn low_byte<E>(&self) -> Result<u8, ActorCaptureError<E>> {
        self.bytes
            .as_ref()
            .ok_or(ActorCaptureError::MissingSavedFlagsStorage)?[0]
            .ok_or(ActorCaptureError::UninitializedSavedFlags { offset: 0 })
    }

    fn dword<E>(&self) -> Result<u32, ActorCaptureError<E>> {
        let supplied = self
            .bytes
            .as_ref()
            .ok_or(ActorCaptureError::MissingSavedFlagsStorage)?;
        let mut bytes = [0; 4];
        for (index, byte) in supplied.iter().enumerate() {
            bytes[index] = byte.ok_or(ActorCaptureError::UninitializedSavedFlags {
                offset: index as u8,
            })?;
        }
        Ok(u32::from_le_bytes(bytes))
    }

    fn store<E>(&mut self, flags: u32) -> Result<(), ActorCaptureError<E>> {
        let storage = self
            .bytes
            .as_mut()
            .ok_or(ActorCaptureError::MissingSavedFlagsStorage)?;
        *storage = flags.to_le_bytes().map(Some);
        Ok(())
    }
}

impl ObjectArena {
    ///0047D7C0 on an actual shared actor prefix. This explicit shared-body API
    /// does not execute a concrete class's outer virtual+8C policy. The sole
    /// unowned lower command is required only after a rejected source lookup.
    pub fn apply_shared_actor_capture<H: ActorCaptureHost>(
        &mut self,
        request: ActorCaptureRequest,
        context: &mut ActorCaptureContext<'_>,
        host: &mut H,
    ) -> Result<bool, ActorCaptureError<H::Error>> {
        let actor = request.actor;
        // Typed receiver admission; concrete constructor/class/B4 does not
        // select the operation. Both original arms next consume actual flags.
        self.actor(actor)?;
        match CaptureOperation::from_native_word(request.operation) {
            CaptureOperation::Capture => {
                if self.object(actor)?.base.flags & 0xc0 != 0x40 {
                    return Ok(false);
                }
                if let Some(source) = self.resolve_actor_handle(request.source)? {
                    if self.actor(source)?.unknown_6c | self.actor(actor)?.unknown_6c != 0 {
                        return Err(ActorCaptureError::UnknownIdentity {
                            capturer: source,
                            actor,
                        });
                    }
                    let source_identity = self.actor(source)?.word_6c;
                    let actor_identity = self.actor(actor)?.word_6c;
                    if source_identity == actor_identity {
                        context.saved_flags.store(self.object(actor)?.base.flags)?;
                        //0047D801 rereads after the caller's full saved write.
                        let flags = self.object(actor)?.base.flags;
                        if flags & 0x8000_0000 == 0 {
                            let retained_dirty = {
                                let base = &mut self.owned_mut(actor)?.state.base;
                                base.dirty_word |= 8;
                                let dirty = base.dirty_word;
                                base.flags = flags | 0x80;
                                dirty
                            };
                            //0047D821/27 precede the retained dirty WORD store.
                            let state = self.actor_mut(actor)?;
                            state.capture_flags_d4 = Some(flags);
                            state.capture_handle_70 = Some(request.source);
                            if ((flags | 0x80) ^ flags) & 0x1ff != 0 {
                                self.owned_mut(actor)?.state.base.dirty_word = retained_dirty | 2;
                            }
                        }
                        let base = &mut self.owned_mut(actor)?.state.base;
                        let flags = base.flags & 0xffff_fefb;
                        mark_flag_change(base, flags);
                        self.set_flag0(actor, false)?;
                        self.set_flag1(actor, false)?;
                        return Ok(true);
                    }
                }
                // Both 00488AA0 calls are pure reads of the same actual list.
                // Rebinding/alias effects between native queries are not owned.
                if self.first_player().is_some() {
                    let player = self
                        .first_player()
                        .expect("pure first-player reads preserve the same live list");
                    host.player_command(
                        self,
                        context,
                        ActorCapturePlayerCommand {
                            player,
                            code: 0x42,
                            value: 0,
                        },
                    )
                    .map_err(|error| ActorCaptureError::Host {
                        address: 0x0046_c760,
                        error,
                    })?;
                }
                Ok(false)
            }
            CaptureOperation::Release => {
                if self.object(actor)?.base.flags & 0xc0 != 0xc0 {
                    return Ok(false);
                }
                //0047D899 and0047D8A7 read separately before the late DWORD.
                if context.saved_flags.low_byte()? & 1 != 0 {
                    self.set_flag0(actor, true)?;
                }
                if context.saved_flags.low_byte()? & 2 != 0 {
                    self.set_flag1(actor, true)?;
                }
                let flags = {
                    let base = &mut self.owned_mut(actor)?.state.base;
                    let flags = base.flags & !0x80;
                    mark_flag_change(base, flags);
                    base.dirty_word &= !8;
                    flags
                };
                let saved = context.saved_flags.dword()?;
                if flags == saved {
                    return Ok(false);
                }
                let base = &mut self.owned_mut(actor)?.state.base;
                mark_flag_change(base, flags | saved);
                let null = context
                    .process
                    .process_null_handle
                    .ok_or(ActorCaptureError::MissingProcessNullHandle)?;
                self.actor_mut(actor)?.capture_handle_70 = Some(null);
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests;
