//! The eleven input-reference stores at 00469DA1..00469E64.
//!
//! A pending player retains these references before message/grant construction
//! and FIFO publication. Descriptor contents are supplied at each reached slot;
//! the three analog identities name virtual zero-fill storage, not executable
//! file records. This phase creates no context, binding set or device and invokes
//! no callback. It consumes InputRuntime's known, stable, untimed metadata domain;
//! mutable native descriptor pointers, allocation NULL, wrapping refcounts and
//! C++ exception unwinding are separate bounds.

use crate::input::{
    ActionDescriptor, AnalogActionDescriptor, AnalogConsumerId, ConsumerId, InputRuntime,
    InputStateError,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerInputDescriptorKind {
    Analog,
    Digital,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerInputDescriptorRequest {
    pub kind: PlayerInputDescriptorKind,
    pub identity: u32,
    pub player_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerInputDescriptor {
    Analog(AnalogActionDescriptor),
    Digital(ActionDescriptor),
}

impl PlayerInputDescriptor {
    fn kind(self) -> PlayerInputDescriptorKind {
        match self {
            Self::Analog(_) => PlayerInputDescriptorKind::Analog,
            Self::Digital(_) => PlayerInputDescriptorKind::Digital,
        }
    }

    fn identity(self) -> u32 {
        match self {
            Self::Analog(descriptor) => descriptor.action.identity,
            Self::Digital(descriptor) => descriptor.identity,
        }
    }
}

pub trait PlayerInputDescriptorSource {
    type Error;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error>;
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum PlayerInputError<E> {
    #[error("player input references have already begun acquisition")]
    AlreadyStarted,
    #[error("player input descriptor {request:?} is unavailable: {error}")]
    Source {
        request: PlayerInputDescriptorRequest,
        error: E,
    },
    #[error("player input descriptor {request:?} has family {actual:?}")]
    Family {
        request: PlayerInputDescriptorRequest,
        actual: PlayerInputDescriptorKind,
    },
    #[error("player input descriptor {request:?} has identity {actual:#010x}")]
    Identity {
        request: PlayerInputDescriptorRequest,
        actual: u32,
    },
    #[error("player input acquisition {request:?} stopped: {error}")]
    Input {
        request: PlayerInputDescriptorRequest,
        error: InputStateError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("player input release {request:?} stopped: {error}")]
pub struct PlayerInputReleaseError {
    pub request: PlayerInputDescriptorRequest,
    pub error: InputStateError,
}

const REQUESTS: [PlayerInputDescriptorRequest; 11] = [
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Analog,
        identity: 0x0054_ecf8,
        player_offset: 0x5c,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Analog,
        identity: 0x0054_ece8,
        player_offset: 0x60,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Analog,
        identity: 0x0054_ed08,
        player_offset: 0x64,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a368,
        player_offset: 0x68,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a370,
        player_offset: 0x6c,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a358,
        player_offset: 0x70,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a360,
        player_offset: 0x74,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a378,
        player_offset: 0x78,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a380,
        player_offset: 0x7c,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a388,
        player_offset: 0x80,
    },
    PlayerInputDescriptorRequest {
        kind: PlayerInputDescriptorKind::Digital,
        identity: 0x0053_a390,
        player_offset: 0x84,
    },
];

/// Ownership moves with the pending player; cloning would duplicate external
/// references. The supplied context is its stable logical callback identity,
/// not an InputContextId or a dereferenced native player pointer.
#[derive(Debug)]
pub(super) struct PlayerInputReferences {
    callback_context: u32,
    started: bool,
    analog: [Option<AnalogConsumerId>; 3],
    digital: [Option<ConsumerId>; 8],
}

impl PlayerInputReferences {
    pub(super) fn new(callback_context: u32) -> Self {
        Self {
            callback_context,
            started: false,
            analog: [None; 3],
            digital: [None; 8],
        }
    }

    pub(super) fn callback_context(&self) -> u32 {
        self.callback_context
    }

    /// +5C..+84 once every reference is held.
    pub(super) fn input_consumers(&self) -> Option<super::PlayerInputConsumers> {
        let [analog_5c, analog_60, analog_64] = self.analog;
        let [left_68, right_6c, dive_70, climb_74, thrust_78, reverse_7c, fire_80, grenade_84] =
            self.digital;
        Some(super::PlayerInputConsumers {
            analog_5c: analog_5c?,
            analog_60: analog_60?,
            analog_64: analog_64?,
            left_68: left_68?,
            right_6c: right_6c?,
            dive_70: dive_70?,
            climb_74: climb_74?,
            thrust_78: thrust_78?,
            reverse_7c: reverse_7c?,
            fire_80: fire_80?,
            grenade_84: grenade_84?,
        })
    }

    /// One constructor phase, with no replay after a stopped prefix. Cleanup is
    /// explicit: a required-source Result is not native exception unwinding.
    pub(super) fn acquire<S: PlayerInputDescriptorSource>(
        &mut self,
        input: &mut InputRuntime,
        source: &mut S,
    ) -> Result<(), PlayerInputError<S::Error>> {
        if self.started {
            return Err(PlayerInputError::AlreadyStarted);
        }
        self.started = true;
        for (index, request) in REQUESTS.into_iter().enumerate() {
            let descriptor = source
                .descriptor(request)
                .map_err(|error| PlayerInputError::Source { request, error })?;
            if descriptor.kind() != request.kind {
                return Err(PlayerInputError::Family {
                    request,
                    actual: descriptor.kind(),
                });
            }
            if descriptor.identity() != request.identity {
                return Err(PlayerInputError::Identity {
                    request,
                    actual: descriptor.identity(),
                });
            }
            match descriptor {
                PlayerInputDescriptor::Analog(descriptor) => {
                    let id = input
                        .acquire_analog_consumer(descriptor, self.callback_context)
                        .map_err(|error| PlayerInputError::Input { request, error })?;
                    self.analog[index] = Some(id);
                }
                PlayerInputDescriptor::Digital(descriptor) => {
                    let id = input
                        .acquire_consumer(descriptor, self.callback_context)
                        .map_err(|error| PlayerInputError::Input { request, error })?;
                    self.digital[index - self.analog.len()] = Some(id);
                }
            }
        }
        Ok(())
    }

    /// 00469FE0 releases +84..+68, then +64..+5C. This component phase neither
    /// unlinks the player nor destroys its message. A stopped release retains
    /// its failed and earlier slots, allowing cleanup to resume honestly.
    pub(super) fn release(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<(), PlayerInputReleaseError> {
        for index in (0..self.digital.len()).rev() {
            if let Some(id) = self.digital[index] {
                input
                    .release_consumer(id)
                    .map_err(|error| PlayerInputReleaseError {
                        request: REQUESTS[self.analog.len() + index],
                        error,
                    })?;
                self.digital[index] = None;
            }
        }
        for index in (0..self.analog.len()).rev() {
            if let Some(id) = self.analog[index] {
                input
                    .release_analog_consumer(id)
                    .map_err(|error| PlayerInputReleaseError {
                        request: REQUESTS[index],
                        error,
                    })?;
                self.analog[index] = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
