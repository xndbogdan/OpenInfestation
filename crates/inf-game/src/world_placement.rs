//! Terrain-adjusted constructor inputs from resident H12/0 and H14/0.
//!
//! This is the placement-input portion of 0048DC30, after world resources
//! and terrain policy have been selected. It preserves authored order and
//! command custody; it does not construct actors, execute their commands or
//! claim that the later virtual initialization passes have run. Height is
//! queried for each requested input, never cached as final actor geometry.

use inf_formats::{
    parse_world_instance_bank, InstanceAssetBank, InstanceAssetError, PlacementInstanceAssets,
    WorldPlacement, WorldResource,
};
use thiserror::Error;

use crate::{
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    HandlerResources, ResidentResource, ResidentRuntime,
};

/// One 00460310 invocation's geometry and authored instance command list.
/// Resource lookup consumes the signed low WORD of `resource_argument`, but
/// its full DWORD must survive for the constructor/lazy-resource boundary.
#[derive(Clone, Copy, Debug)]
pub struct WorldConstructorInput<'a> {
    pub placement_index: u32,
    pub resource_argument: u32,
    /// Raw world WORDs: X/Z unchanged, Y includes the wrapped terrain height.
    pub position_words: [u16; 3],
    /// Stack order from authored +E,+A,+C; no axis or angle conversion.
    pub rotation_words: [u16; 3],
    pub instance: &'a PlacementInstanceAssets,
}

/// Prepared placement inputs bound to one immutable residency snapshot.
/// The terrain's runtime diagonal bits are owned separately from OVL bytes.
/// Actual retained process policy or an explicitly supplied inspection boundary.
pub enum WorldPlacementPolicy<'a> {
    Retained(&'a mut crate::terrain_runtime::ProcessTerrainPolicy),
    Supplied(TerrainSpacing),
}
impl From<TerrainSpacing> for WorldPlacementPolicy<'_> {
    fn from(value: TerrainSpacing) -> Self {
        Self::Supplied(value)
    }
}
impl<'a> From<&'a mut crate::terrain_runtime::ProcessTerrainPolicy> for WorldPlacementPolicy<'a> {
    fn from(value: &'a mut crate::terrain_runtime::ProcessTerrainPolicy) -> Self {
        Self::Retained(value)
    }
}

pub struct WorldPlacements<'r> {
    plane_height_fixed: i32,
    world_source: ResidentResource<'r>,
    terrain_source: ResidentResource<'r>,
    assets: InstanceAssetBank,
    world_index: usize,
    terrain: TerrainRuntime,
}

#[derive(Debug, Error)]
pub enum WorldPlacementError {
    #[error("world placement inputs require resident Handler-{handler} slot 0; lazy loading is not implemented")]
    NotResident { handler: u8 },
    #[error("Handler-{handler} slot 0 does not resolve to its registered local record")]
    Registration { handler: u8 },
    #[error(transparent)]
    Instance(#[from] InstanceAssetError),
}

impl<'r> WorldPlacements<'r> {
    /// The caller supplies the current terrain spacing. 0048DC30's H14
    /// flags select the height scale and can force coarse spacing; they do
    /// not supply an otherwise unknown process/settings default.
    pub fn new<'p>(
        runtime: &'r ResidentRuntime,
        policy: impl Into<WorldPlacementPolicy<'p>>,
    ) -> Result<Self, WorldPlacementError> {
        let world_source = runtime
            .resource(14, 0)
            .ok_or(WorldPlacementError::NotResident { handler: 14 })?;
        let world_region = world_source
            .overlay
            .layout()
            .regions
            .get(world_source.registration.region_index)
            .filter(|region| region.handler.get() == 14)
            .ok_or(WorldPlacementError::Registration { handler: 14 })?;
        let assets = parse_world_instance_bank(world_source.overlay.bytes(), world_region)?;
        let world_index = usize::from(world_source.registration.local_index);
        let world = assets
            .worlds
            .entries
            .get(world_index)
            .ok_or(WorldPlacementError::Registration { handler: 14 })?;
        let instance = assets
            .instances
            .get(world_index)
            .ok_or(WorldPlacementError::Registration { handler: 14 })?;
        if instance.world_local_index != world.local_index
            || instance.placements.len() != world.placements.len()
        {
            return Err(WorldPlacementError::Registration { handler: 14 });
        }
        let sampling = TerrainSampling {
            spacing: match policy.into() {
                WorldPlacementPolicy::Retained(policy) => policy.load_world(world.flags),
                WorldPlacementPolicy::Supplied(spacing) => spacing.for_world(world.flags),
            },
            height_scale: TerrainHeightScale::from_world_flags(world.flags),
        };

        let terrain_source = runtime
            .resource(12, 0)
            .ok_or(WorldPlacementError::NotResident { handler: 12 })?;
        let terrain = match terrain_source
            .overlay
            .handlers()
            .get(terrain_source.registration.region_index)
        {
            Some(HandlerResources::Terrain(bank)) => bank
                .entries
                .get(usize::from(terrain_source.registration.local_index)),
            _ => None,
        }
        .ok_or(WorldPlacementError::Registration { handler: 12 })?;
        // 0044B590 prepares 004506A0's diagonals before the placement loop.
        let plane_height_fixed = terrain.prefix.plane_height_fixed;
        let terrain = TerrainRuntime::new(terrain, sampling);
        Ok(Self {
            plane_height_fixed,
            world_source,
            terrain_source,
            assets,
            world_index,
            terrain,
        })
    }

    pub fn world_source(&self) -> &ResidentResource<'r> {
        &self.world_source
    }

    pub fn terrain_source(&self) -> &ResidentResource<'r> {
        &self.terrain_source
    }

    /// Complete original allocation/relocation context for command payloads.
    pub fn assets(&self) -> &InstanceAssetBank {
        &self.assets
    }

    pub fn world(&self) -> &WorldResource {
        &self.assets.worlds.entries[self.world_index]
    }

    pub fn terrain(&self) -> &TerrainRuntime {
        &self.terrain
    }

    /// Actual prepared H12 owner, borrowed by the load-time radius publisher.
    pub(crate) fn terrain_mut(&mut self) -> &mut TerrainRuntime {
        &mut self.terrain
    }

    /// H12 +0, the signed fixed-point water plane before 005483BC's shift.
    pub fn plane_height_fixed(&self) -> i32 {
        self.plane_height_fixed
    }

    /// Resolve one authored index against this prepared terrain. Out-of-range
    /// indices are absent, without fabricating a constructor request.
    pub fn input(&self, index: usize) -> Option<WorldConstructorInput<'_>> {
        let authored = self.world().placements.get(index)?;
        let instance = self.assets.instances[self.world_index]
            .placements
            .get(index)?;
        Some(constructor_input(authored, instance, &self.terrain))
    }

    /// The ordered 0048DC30 input walk; aliases remain distinct entries.
    /// This only queries the current prepared terrain and executes no actors.
    pub fn inputs(&self) -> impl ExactSizeIterator<Item = WorldConstructorInput<'_>> + '_ {
        self.world()
            .placements
            .iter()
            .zip(&self.assets.instances[self.world_index].placements)
            .map(|(authored, instance)| constructor_input(authored, instance, &self.terrain))
    }
}

fn constructor_input<'a>(
    authored: &WorldPlacement,
    instance: &'a PlacementInstanceAssets,
    terrain: &TerrainRuntime,
) -> WorldConstructorInput<'a> {
    let [x, y, z] = authored.position_words;
    WorldConstructorInput {
        placement_index: authored.index,
        resource_argument: authored.resource_argument,
        position_words: [x, y.wrapping_add(terrain.height_at([x, z]) as u16), z],
        rotation_words: authored.constructor_rotation_words(),
        instance,
    }
}
