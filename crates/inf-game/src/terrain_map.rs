//! Terrain-map indices constructed by 00471AE0 through 00471C40.
//!
//! The native 256-square byte map at 0054ED78 is separate from H10's map
//! projection samples and from the palette built by 00471D40. This owner
//! constructs the initial index snapshot only. It neither registers the
//! native object's callback/list nodes; [`crate::terrain_radar`] draws it.

use inf_formats::TERRAIN_GRID_SIDE;

use crate::{
    name_entry::name_entry_rng_next,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime},
};

pub const TERRAIN_MAP_SIDE: usize = 256;
pub const TERRAIN_MAP_CELLS: usize = TERRAIN_MAP_SIDE * TERRAIN_MAP_SIDE;

/// 0048DC30 passes H14 flag 0x20 as the map constructor's low-byte boolean.
/// Classification compares a signed H12 fixed-point plane with the sampled
/// signed height byte; this does not clamp the terrain's height queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainMapPlane {
    Ignore,
    Classify { height_fixed: i32 },
}

impl TerrainMapPlane {
    pub const fn from_world_flags(flags: u32, height_fixed: i32) -> Self {
        if flags & 0x20 != 0 {
            Self::Classify { height_fixed }
        } else {
            Self::Ignore
        }
    }
}

/// Initial palette-index map. Each byte is in 0..40: eight shade groups of
/// five entries, with entry zero for cells below the enabled plane and the
/// other four entries selected by the cell's material bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainMap {
    indices: Box<[u8]>,
}

impl TerrainMap {
    /// Retains the caller's custody of 0048F060's shared process RNG. Native
    /// 00471B51 visits X outside Z, consuming exactly one RNG word per cell
    /// even when the shade/plane branch makes that word's value irrelevant.
    pub fn new(terrain: &TerrainRuntime, plane: TerrainMapPlane, rng: &mut u32) -> Self {
        let sampling = terrain.sampling();
        let (height_shift, plane_shift) = match sampling.height_scale {
            TerrainHeightScale::Units16 => (4, 9),
            TerrainHeightScale::Units32 => (5, 8),
        };
        let plane = match plane {
            TerrainMapPlane::Ignore => None,
            TerrainMapPlane::Classify { height_fixed } => Some(height_fixed >> plane_shift),
        };
        let grid_mask = !((sampling.spacing.units() - 1) >> 7);
        let mut indices = vec![0; TERRAIN_MAP_CELLS].into_boxed_slice();
        for x in 0..TERRAIN_MAP_SIDE {
            for z in 0..TERRAIN_MAP_SIDE {
                // H12 is x*512+z; the map is x+z*256. Both supported
                // spacing masks retain these even/even grid coordinates.
                let gx = usize::from((x as u16 * 2) & grid_mask);
                let gz = usize::from((z as u16 * 2) & grid_mask);
                let cell = terrain.cells()[gx * TERRAIN_GRID_SIDE + gz];
                let random = name_entry_rng_next(rng) as u8;
                let shade = cell.shade4();
                let shade = (shade / 2 + (random & 1 & shade)).min(7);
                let height = i32::from(cell.height_i8()) << height_shift;
                let material = if plane.is_some_and(|plane| height < plane) {
                    0
                } else {
                    cell.material() + 1
                };
                indices[x + z * TERRAIN_MAP_SIDE] = shade * 5 + material;
            }
        }
        Self { indices }
    }

    /// Byte indices in native x+z*256 storage order; these are not colors.
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }

    pub fn index_at(&self, [x, z]: [u8; 2]) -> u8 {
        self.indices[usize::from(x) + usize::from(z) * TERRAIN_MAP_SIDE]
    }
}

#[cfg(test)]
mod tests;
