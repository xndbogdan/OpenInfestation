//! 0048D820's shared damage cache. Row address selection is captured before
//! candidate admission and survives synchronous creation/query/deletion calls.
//! The enclosing 00429CB0 gate composes this owner; lower factories and
//! complete actor update attachment remain separate services.

use thiserror::Error;

use super::ActorBehaviorTable;
use crate::object_runtime::{ActorDamagePacket, ActorHandle, ObjectArena, ObjectError, ObjectId};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::world_furniture_effect::{resisted_effect_damage, DamageResistance};

mod initialization;
mod packets;
mod particle_adapter;
pub(super) mod storage;
pub use particle_adapter::{
    BehaviorActorResourceRequest, BehaviorComponentPacketSourceRequest, BehaviorParticleAdapter,
    BehaviorParticleAdapterError, BehaviorParticleCreationSources, BehaviorParticleEffectRequest,
    BehaviorParticleServiceContext, BehaviorParticleServices,
};
pub use storage::{BehaviorCacheStorageError, ProcessActorBehaviorTables};

/// Live process operands shared with particle factories. The cache owns no
/// duplicate 0054E894 scalar, and no callback requires a final synchronization.
pub struct BehaviorPredicateContext<'a> {
    pub tables: &'a mut ProcessActorBehaviorTables,
    pub process: &'a mut ParticleFactoryProcessState,
    pub rng: &'a mut u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorRowSelection {
    SharedAddress {
        address: u32,
    },
    /// Native +26C is inline storage; its owner and byte offset identify it,
    /// independently of the Rust Box used to retain the copied DWORDs.
    VehicleCopy {
        owner: ObjectId,
        byte_offset: i32,
    },
}

impl BehaviorRowSelection {
    pub fn from_table(table: ActorBehaviorTable, index: u8, owner: ObjectId) -> Self {
        let offset = i32::from(index as i8).wrapping_mul(44);
        if table == ActorBehaviorTable::Selector2Copy {
            Self::VehicleCopy {
                owner,
                byte_offset: offset,
            }
        } else {
            Self::SharedAddress {
                address: (table as u32).wrapping_add(offset as u32),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorPredicateRequest {
    pub selection: BehaviorRowSelection,
    /// A nonnull candidate is not dereferenced until the ready-cache tail.
    pub candidate: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorPacketSlot {
    First,
    Second,
}

impl BehaviorPacketSlot {
    fn offset(self) -> u32 {
        match self {
            Self::First => 0x24,
            Self::Second => 0x28,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorCreationRequest {
    ParticleFactory {
        callback: u32,
        context_word: u32,
        null_handle: ActorHandle,
        scratch_bytes: [Option<u8>; 8],
    },
    ActorResource {
        resource: i16,
        null_handle: ActorHandle,
        scratch_bytes: [Option<u8>; 8],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorTemporary<P> {
    Particle(P),
    Actor(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorComponentQuery<C> {
    Actor18(ObjectId),
    Component6c(C),
    Component68(C),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorPacketSourceRequest<P, C> {
    ParticlePointer {
        particle: P,
        slot: BehaviorPacketSlot,
    },
    Channel148(C),
    Strength146(C),
    Complete13e(C),
}

/// Available source data can carry unknown bytes. Copying them preserves
/// custody; it does not consume their numeric interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorPacketCopy {
    Full([Option<u8>; 4]),
    Byte(Option<u8>),
    Word([Option<u8>; 2]),
}

impl From<ActorDamagePacket> for BehaviorPacketCopy {
    fn from(packet: ActorDamagePacket) -> Self {
        let [low, high] = packet.strength.to_le_bytes();
        Self::Full([Some(low), Some(high), Some(packet.channel), packet.byte_03])
    }
}

pub trait BehaviorPredicateHost {
    type Error;
    /// Real allocation/component identities retained by the lower owner.
    type Particle: Copy;
    type Component: Copy;

    /// Original factory or 00460310 arguments. Scratch is uninitialized;
    /// lower services must not silently replace its unknown bytes with zero.
    /// None is actual allocation null; unavailable services return an error.
    fn create(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorCreationRequest,
    ) -> Result<Option<BehaviorTemporary<Self::Particle>>, Self::Error>;
    fn query_component(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<Self::Component>,
    ) -> Result<Option<Self::Component>, Self::Error>;
    /// Pure reached pointer/data reads. The second particle source is read
    /// after the first copy. None denotes its actual null source pointer;
    /// absent source storage instead fails. Unknown copied bytes are retained.
    fn packet_source(
        &mut self,
        arena: &ObjectArena,
        context: &BehaviorPredicateContext<'_>,
        request: BehaviorPacketSourceRequest<Self::Particle, Self::Component>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error>;
    /// Required deleting virtual +00(1), including mutable effects. Errors do
    /// not trigger an invented cleanup or release the token automatically.
    fn delete_particle(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        particle: Self::Particle,
        flags: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BehaviorPredicateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error("actor {0:?} has no current behavior allocation")]
    MissingBehavior(ObjectId),
    #[error("required process null 0054E894 is unavailable at {address:08X}")]
    MissingProcessNull { address: u32 },
    #[error("required behavior service failed at {address:08X}: {error}")]
    Host { address: u32, error: E },
    #[error("behavior service returned a different temporary class at {address:08X}")]
    TemporaryClass { address: u32 },
    #[error("behavior source does not match its reached packet-copy phase at {address:08X}")]
    PacketSource { address: u32 },
    #[error("candidate {actor:?} channel {channel} lies outside its retained +A4 table")]
    UnsupportedChannel { actor: ObjectId, channel: u8 },
}

impl ObjectArena {
    pub fn actor_behavior_row_selection(
        &self,
        owner: ObjectId,
    ) -> Result<BehaviorRowSelection, BehaviorPredicateError<std::convert::Infallible>> {
        let behavior = self
            .actor_behavior_storage(owner)?
            .as_ref()
            .ok_or(BehaviorPredicateError::MissingBehavior(owner))?;
        Ok(BehaviorRowSelection::from_table(
            behavior.table_14,
            behavior.byte_10,
            owner,
        ))
    }

    pub fn test_behavior_damage_candidate<H: BehaviorPredicateHost>(
        &mut self,
        request: BehaviorPredicateRequest,
        context: &mut BehaviorPredicateContext<'_>,
        host: &mut H,
    ) -> Result<bool, BehaviorPredicateError<H::Error>> {
        let row = request.selection;
        let Some(candidate) = request.candidate else {
            return Ok(false);
        };
        if storage::read_row_byte(self, context.tables, row, 0x22)? == 0 {
            initialization::CacheInitialization {
                arena: self,
                context,
                host,
                row,
            }
            .initialize()?;
        }
        if storage::read_row_byte(self, context.tables, row, 0x22)? == 0 {
            return Ok(true);
        }
        for slot in [BehaviorPacketSlot::First, BehaviorPacketSlot::Second] {
            // Actual candidate+A4 precedes packet channel/strength reads.
            let responses = self.actor(candidate)?.damage_responses();
            let channel = storage::read_row_byte(self, context.tables, row, slot.offset() + 2)?;
            let strength = storage::read_row_word(self, context.tables, row, slot.offset())? as i16;
            let &(threshold, multiplier) = responses.get(usize::from(channel)).ok_or(
                BehaviorPredicateError::UnsupportedChannel {
                    actor: candidate,
                    channel,
                },
            )?;
            if resisted_effect_damage(
                strength,
                DamageResistance {
                    threshold,
                    multiplier,
                },
                0,
            ) != 0
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod native_receipts;
#[cfg(test)]
mod tests;
