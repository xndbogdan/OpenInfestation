//! Validate scalar reads against the mutable allocation's typed pointer slots.
//! The decoder sees the byte projection; a native heap pointer must never be
//! mistaken for the zero bytes used to represent its separate host identity.

use inf_formats::{
    ByteSpan, ConditionalInstanceDword, ConditionalInstanceWord, InstanceCommandPayload,
    InstanceCommandView,
};

use super::{CommandPolicy, ObjectConstructionError};
use crate::object_runtime::{ObjectArena, ObjectId, RuntimeAllocation};

pub(super) fn validate(
    arena: &ObjectArena,
    object: ObjectId,
    policy: CommandPolicy<'_>,
    allocation: &RuntimeAllocation,
    main_payload: ByteSpan,
    command: &InstanceCommandView,
) -> Result<(), ObjectConstructionError> {
    use InstanceCommandPayload::*;
    let span = match &command.payload {
        Opaque | NoPayloadRead | RetainedPayloadPointer { .. } | RouteNodeFlagOr { .. } => None,
        ModelSlot(value) => Some(value.raw.span),
        Selector86Data { rate, .. } => Some(rate.span),
        TerrainLightChannel { value, .. } => Some(value.span),
        MapAnnotationColor { value, .. } => Some(value.span),
        TerrainLightDrawGate(value)
        | Actor40d800Flag14e(value)
        | Actor40d800Refill(value)
        | Selector30Byte15c(value)
        | Selector30Byte160(value)
        | Selector30Byte164(value)
        | Selector30Byte18f(value)
        | Selector30Byte167(value)
        | Selector30Flag18e(value)
        | Selector30Flag166(value)
        | Selector30Flag190(value)
        | Selector30Flag191(value)
        | Selector30Flag192(value)
        | Selector37Seconds164(value)
        | Selector37Flag166(value)
        | Selector37Byte165(value)
        | Selector39Byte160(value)
        | Selector39Flag(value)
        | Selector53Kind138(value)
        | Selector53Count139(value)
        | Selector53Seconds13a(value)
        | Selector53Repeat13b(value)
        | Selector52Stopped(value)
        | Selector59Wake(value)
        | Selector59Crushes144(value)
        | Selector59CrushesAny145(value)
        | Selector59Smokes146(value)
        | Selector18TeamPasses(value)
        | Selector47Seconds16c(value)
        | Selector42OnDraw(value)
        | Selector42Disabled(value)
        | Selector48Relative(value)
        | Selector43Ring(value) => Some(value.span),
        Actor40d800Maximum(value) => Some(value.span),
        Selector15Dword15c(value) => Some(value.span),
        Selector47Radius164(value) => Some(value.span),
        Selector15GrantWord(value) => Some(value.span),
        TerrainLightRadius(value)
        | PickupKind(value)
        | PickupValue(value)
        | MapAnnotationLabel(value)
        | MapAnnotationRadius(value)
        | RouteNodePlacementReference(value)
        | ActorWord8c(value)
        | ActorMaximumHealth(value)
        | ActorCurrentHealth(value)
        | ActorWord7a(value)
        | ActorWord76(value)
        | ActorWord164(value)
        | Selector11Flag04(value)
        | Selector11Flag10(value)
        | Actor40d800Word144(value)
        | Selector30Placement180(value)
        | Selector30Word18c(value)
        | Selector37Word158(value)
        | Selector37Word15c(value)
        | Selector37Word17c(value)
        | Selector37Word17e(value)
        | Selector42Radius(value)
        | Selector42WakeRadius(value)
        | Selector43OnSeconds(value)
        | Selector43OffSeconds(value)
        | Selector43Spread(value)
        | Selector48Ceiling(value)
        | Selector48Radius(value)
        | Selector48Grant(value)
        | Selector50Radius(value)
        | Selector50Seconds(value)
        | Selector18Radius(value)
        | Selector18Height(value)
        | Selector18KeyGrant(value) => Some(value.span),
        Selector37SignedWord160(value) => Some(value.span),
        RouteNodeMode(value)
        | ActorNegatedDword144(value)
        | Selector11Flag02(value)
        | FamilyPlacementReference(value)
        | Selector35Packed240(value)
        | Selector39Dword15c(value)
        | Selector50Command(value)
        | Selector50Silent(value)
        | Selector43Byte51(value)
        | Selector29Fuse(value)
        | Selector18Hint(value) => Some(value.span),
        Selector74Width(value)
        | Selector74Depth(value)
        | Selector74Height(value)
        | Selector74Refused(value) => Some(value.span),
        Selector84Link(value)
        | Selector79Link(value)
        | Selector81Count44(value)
        | Selector81Distance48(value) => Some(value.span),
        ActorWords8e(value) | FamilyHomeTerrainXYZ(value) | Actor40d800Words146(value) => {
            Some(value.span)
        }
        Selector45Team(value)
        | Selector46Score(value)
        | Selector46Seconds(value)
        | RaceDword(value) => Some(value.span),
        Selector45Offset { words, .. } => Some(words.span),
        FamilyHomeTerrainXZ(value) => Some(value.span),
        ActorSelectorB4(value)
        | ActorShiftedByte7c(value)
        | ActorShiftedByte7d(value)
        | ActorStateC4(value)
        | Selector69DoubledDword14c(value)
        | Selector69Dword144(value) => Some(value.span),
        Selector49Word158(value) | Selector49Word15a(value) => Some(value.span),
        Selector49Seconds15c(value) | Selector49Reference16c(value) => Some(value.span),
        ActorDisableUnlessBit(value) | ActorDisableIfBit(value) => {
            let player = match policy {
                CommandPolicy::SharedActor {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector88 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector78 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::DerivedActor {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector30 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector53 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector52 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector29 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector74 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector84 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector59 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector18 {
                    first_player_grants,
                    ..
                }
                | CommandPolicy::Selector69 {
                    first_player_grants,
                    ..
                } => first_player_grants,
                _ => None,
            };
            match (player, value) {
                (Some(_), ConditionalInstanceWord::Available(field)) => Some(field.span),
                _ => None,
            }
        }
        Selector11Block1a0(value) => Some(value.span),
        Selector11BehaviorWord04(value) => {
            if arena.derived_actor(object)?.behavior_140.is_some() {
                match value {
                    ConditionalInstanceDword::Available(field) => Some(field.span),
                    ConditionalInstanceDword::OutsideAllocation { .. } => None,
                }
            } else {
                None
            }
        }
        // Dispatch retains the selector-60 operand, then its real handler
        // reads each current count, pointer and row field through this owner.
        PlacementSpawnList(_) => unreachable!("inspection-only spawn table in runtime dispatch"),
    };
    if let Some(span) = span {
        let offset = u32::try_from(span.start - main_payload.start)
            .expect("decoded instance field has a u32 allocation offset");
        allocation.validate_scalar_read(offset, span.end - span.start)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "command_reads_tests.rs"]
mod tests;
