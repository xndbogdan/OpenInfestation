//! Complete 0046CBB0 vehicle-to-player copy in its stable canonical domain.
//!
//! Each reached source read immediately publishes its scalar into the actual
//! player. Missing typed backing retains all earlier writes and registry cache
//! effects; it is not an actual NULL resolution or a native partial-memory
//! fault. Existing pickup admission, grants and later effects stay with its
//! caller. The world-retirement suffix consumes this leaf; enclosing
//! lifetime and production reload policies remain with those callers.

use super::{ObjectArena, ObjectId, PlayerAttachError, PlayerId};
use crate::object_runtime::DerivedActorError;

impl ObjectArena {
    /// 0046CBBA/0046CBC2 publishes +144 before the first behavior query.
    /// Canonical selector2 +7C is 004027A0's current +140 pointer read.
    /// The immutable class/storage domain has no arbitrary callback or
    /// concurrent pointer replacement, but every native query stays a fresh
    /// reached borrow. Source aliasing and physical faults are unrepresented.
    pub(in crate::object_runtime) fn save_vehicle_into_player(
        &mut self,
        player: PlayerId,
        vehicle: ObjectId,
    ) -> Result<(), PlayerAttachError> {
        let value = self.selector2_actor(vehicle)?.dwords_1e8_1f4[0];
        self.world_player_mut(player)?.dword_144 = value;

        // 0046CBCA/CD/D1: fresh query, signed BYTE+10, immediate DWORD store.
        let value = self
            .derived_actor(vehicle)?
            .behavior_140
            .as_ref()
            .ok_or(DerivedActorError::ShortVehicleBehavior(vehicle))?
            .byte_10;
        self.world_player_mut(player)?.dword_148 = i32::from(value as i8) as u32;

        // 0046CBDB/DE/E2 independently reaches BYTE+11 and the next store.
        let value = self
            .derived_actor(vehicle)?
            .behavior_140
            .as_ref()
            .ok_or(DerivedActorError::ShortVehicleBehavior(vehicle))?
            .byte_11;
        self.world_player_mut(player)?.dword_14c = i32::from(value as i8) as u32;

        // 0046CBF4..0046CC0D: sixteen fresh queries; each 32-byte record's
        // first raw DWORD is committed before reading the next record.
        for index in 0..16 {
            let value = self
                .derived_actor(vehicle)?
                .behavior_140
                .as_ref()
                .ok_or(DerivedActorError::ShortVehicleBehavior(vehicle))?
                .entries_18
                .get(index)
                .ok_or(DerivedActorError::ShortVehicleBehavior(vehicle))?[0];
            self.world_player_mut(player)?.values_150[index] = value as i32;
        }

        // 00412780 reads the reached element, not count/header metadata.
        // Missing typed storage stops here; only actual resolution NULL
        // selects the original 0046CC33 zero WORD store.
        for index in 0..3 {
            let handle = self
                .selector2_actor(vehicle)?
                .handles_1a4
                .handles
                .get(index)
                .copied()
                .ok_or(PlayerAttachError::VehicleItemStorageBoundary { vehicle, index })?;
            // Preserve the existing resolver error mapping for this narrow
            // shared phase, including cache writes before an error.
            let value = match self.resolve_handle(handle)? {
                Some(actor) => self.object(actor)?.base.resource_word,
                None => 0,
            };
            self.world_player_mut(player)?.items_190[index] = value;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
