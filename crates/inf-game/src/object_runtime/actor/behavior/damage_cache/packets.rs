use super::{
    storage, BehaviorPacketCopy, BehaviorPacketSlot, BehaviorPredicateError, BehaviorRowSelection,
    ProcessActorBehaviorTables,
};
use crate::object_runtime::ObjectArena;

#[derive(Clone, Copy)]
pub(super) enum PacketCopyPhase {
    Particle466120(BehaviorPacketSlot),
    Channel4863da,
    Strength4863e4,
    Complete458f50,
}

pub(super) fn copy<E>(
    arena: &mut ObjectArena,
    tables: &mut ProcessActorBehaviorTables,
    row: BehaviorRowSelection,
    phase: PacketCopyPhase,
    input: BehaviorPacketCopy,
) -> Result<(), BehaviorPredicateError<E>> {
    match (phase, input) {
        (PacketCopyPhase::Particle466120(slot), BehaviorPacketCopy::Full(bytes)) => {
            storage::write_row(arena, tables, row, slot.offset(), &bytes)?
        }
        (PacketCopyPhase::Complete458f50, BehaviorPacketCopy::Full(bytes)) => {
            storage::write_row(arena, tables, row, 0x24, &bytes)?
        }
        (PacketCopyPhase::Channel4863da, BehaviorPacketCopy::Byte(channel)) => {
            storage::write_row(arena, tables, row, 0x26, &[channel])?
        }
        (PacketCopyPhase::Strength4863e4, BehaviorPacketCopy::Word(bytes)) => {
            storage::write_row(arena, tables, row, 0x24, &bytes)?
        }
        (phase, _) => {
            return Err(BehaviorPredicateError::PacketSource {
                address: match phase {
                    PacketCopyPhase::Particle466120(_) => 0x0046_6120,
                    PacketCopyPhase::Channel4863da => 0x0048_63d4,
                    PacketCopyPhase::Strength4863e4 => 0x0048_63dd,
                    PacketCopyPhase::Complete458f50 => 0x0045_8f50,
                },
            })
        }
    }
    Ok(())
}
