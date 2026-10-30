//! Map constructor/paired-marker custody at mission+04 / gameplay G+1B4.
//! The retained process store owns allocations even on stopped construction;
//! the existing scalar jitter leaf also serves explicit renderer fixtures.
//! Process plane/grid and foreign marker locals survive local map retirement.

use thiserror::Error;

mod construction;
mod markers;

pub use construction::{
    construct_world_map, retire_current_world_map, MapLoadContext, MapLoadState,
    PreparedWorldMapTerrain, ProcessWorldMapRuntime, WorldMapCleanupError,
    WorldMapConstructionError, WorldMapConstructionRequest, WorldMapId, WorldMapLookupError,
    WorldMapTerrainInputs,
};
pub use markers::{MarkerLinkField, MarkerListKind, MarkerStorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldMapState {
    /// Native signed DWORD +24; zero bypasses the marker jitter RNG reads.
    jitter_radius: i32,
}

/// Only reached native writes; their caller retains the admission checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldMapJitterWrite {
    /// 0048D980 after its nonnull object gate.
    MissionEvent48D980,
    /// 00420BE0 codes 0B/0C after local-player/network/owner checks.
    OwnedNetworkImpact420BE0,
    /// The same codes' nonzero-range branch, after 0048EFC0 distance.
    RadialImpact420BE0 { range: i32, distance: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum WorldMapError {
    #[error("native map-jitter signed division would overflow")]
    DivisionOverflow,
}

impl WorldMapState {
    /// 00471AE0 explicitly stores zero to +24. This is constructor state,
    /// not a replacement for an unknown live value.
    pub const fn new() -> Self {
        Self { jitter_radius: 0 }
    }

    pub const fn jitter_radius(&self) -> i32 {
        self.jitter_radius
    }

    /// 0048E742..0048E773 at the end of 0048E2E0, before actor dispatch in
    /// 004875A0. The phase-3 briefing item tick does not call this owner.
    pub fn decay(&mut self, delta_micros: i32) {
        let decrement = delta_micros / 200;
        self.jitter_radius = if self.jitter_radius <= decrement {
            0
        } else {
            self.jitter_radius.wrapping_sub(decrement)
        };
    }

    pub fn apply(&mut self, write: WorldMapJitterWrite) -> Result<(), WorldMapError> {
        match write {
            WorldMapJitterWrite::MissionEvent48D980 => self.jitter_radius = 0x1fff,
            WorldMapJitterWrite::OwnedNetworkImpact420BE0 => self.jitter_radius = 0x3fff,
            WorldMapJitterWrite::RadialImpact420BE0 { range, distance } => {
                // 00420BE0 does not reach the divide/write for range zero.
                if range != 0 {
                    self.jitter_radius = range
                        .wrapping_sub(distance)
                        .wrapping_mul(0xffff)
                        .checked_div(range)
                        .ok_or(WorldMapError::DivisionOverflow)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_state_retains_native_writes_and_signed_decay_until_new_load() {
        let mut map = WorldMapState::new();
        assert_eq!(map.jitter_radius(), 0);
        map.apply(WorldMapJitterWrite::MissionEvent48D980).unwrap();
        assert_eq!(map.jitter_radius(), 0x1fff);
        map.decay(39_999);
        assert_eq!(map.jitter_radius(), 0x1fff - 199);
        // Signed division truncates toward zero; sub-200 input has no step.
        map.decay(199);
        assert_eq!(map.jitter_radius(), 0x1fff - 199);
        map.decay(i32::MAX);
        assert_eq!(map.jitter_radius(), 0);
        map.apply(WorldMapJitterWrite::OwnedNetworkImpact420BE0)
            .unwrap();
        assert_eq!(map.jitter_radius(), 0x3fff);
        map.decay(-400);
        assert_eq!(map.jitter_radius(), 0x4001);
        assert_eq!(WorldMapState::new().jitter_radius(), 0);
    }

    #[test]
    fn radial_write_preserves_wrapped_product_zero_gate_and_signed_result() {
        let mut map = WorldMapState::new();
        map.apply(WorldMapJitterWrite::RadialImpact420BE0 {
            range: 1000,
            distance: 250,
        })
        .unwrap();
        assert_eq!(map.jitter_radius(), 49_151);
        map.apply(WorldMapJitterWrite::RadialImpact420BE0 {
            range: 0,
            distance: 123,
        })
        .unwrap();
        assert_eq!(map.jitter_radius(), 49_151);
        map.apply(WorldMapJitterWrite::RadialImpact420BE0 {
            range: 1000,
            distance: 2000,
        })
        .unwrap();
        assert_eq!(map.jitter_radius(), -65_535);
        map.decay(0);
        assert_eq!(map.jitter_radius(), 0);
        map.apply(WorldMapJitterWrite::RadialImpact420BE0 {
            range: i32::MAX,
            distance: 0,
        })
        .unwrap();
        assert_eq!(map.jitter_radius(), 0);
    }
}
