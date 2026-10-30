//! Shared 0044AA10 terrain traversal. Immutable H12 decoding is separate;
//! caller view/fade, live light-grid custody and required draw services remain
//! explicit. No generated mesh substitutes for native strip submissions.

mod clipping;
mod domain;
mod emission;
mod plan;
mod quad;
mod strip;
mod water;

pub use clipping::{
    clip_terrain_tile, TerrainClipContext, TerrainClipRequest, TerrainClippingHost,
};
pub use domain::{terrain_row_domain, TerrainDomain, TerrainRow};
pub use emission::{emit_terrain_particles, TerrainParticleEmissionRequest};
pub use plan::{terrain_draw_plan, TerrainDrawPlan, TerrainFootprintWidth};
pub use quad::{TerrainQuadCallback, TerrainQuadCommand};
pub use strip::{TerrainLighting, TerrainStripMode, TerrainStripRequest, TerrainVertex};
pub use water::{draw_water_strip, TerrainWaterTables, TerrainWaterTile};

use crate::{
    frontend_scene::SceneView,
    model_context::ModelProjection,
    model_primitives::{PrimitiveTables, TextureSortIdentity},
    render_resources::RenderResource,
    terrain_furniture::FurnitureDomain,
    terrain_runtime::TerrainRuntime,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainGrid {
    pub spacing: u16,
    pub mask: u16,
}

impl TerrainGrid {
    pub const FAR: Self = Self {
        spacing: 512,
        mask: 511,
    };

    fn validate(self) -> Result<(), TerrainDrawFault> {
        if matches!(
            (self.spacing, self.mask),
            (128, 127) | (256, 255) | (512, 511)
        ) {
            Ok(())
        } else {
            Err(TerrainDrawFault::Grid {
                spacing: self.spacing,
                mask: self.mask,
            })
        }
    }
}

/// `0044BD7D..0044BDDE`: infrared vision's pane colour. Each of `rgb` is
/// halved and masked to its top five bits, then shifted into the display
/// layout `+24/+28/+2C` (`shifts`): red and green left by the field plus one,
/// blue right by the field minus one.
pub fn night_material(rgb: [u8; 3], shifts: [u32; 3]) -> u32 {
    let [red, green, blue] = rgb.map(|value| u32::from(value >> 1) & 0xf8);
    red.wrapping_shl(shifts[0].wrapping_add(1))
        | green.wrapping_shl(shifts[1].wrapping_add(1))
        | blue.wrapping_shr(shifts[2].wrapping_sub(1))
}

/// Port option: `rgb` as a material word, as [`night_material`] packs one
/// at full brightness, for the fog under the water.
pub fn fog_material(rgb: [u8; 3], shifts: [u32; 3]) -> u32 {
    let [red, green, blue] = rgb.map(|value| u32::from(value) & 0xf8);
    red.wrapping_shl(shifts[0].wrapping_add(1))
        | green.wrapping_shl(shifts[1].wrapping_add(1))
        | blue.wrapping_shr(shifts[2].wrapping_sub(1))
}

/// Borrowed read-only scene operands reused by strips after furniture writes.
#[derive(Clone, Copy)]
pub struct TerrainDrawInputs<'a> {
    pub view: SceneView,
    /// Retained context+38..4C and+64/+68, not a preceding global fade.
    pub context_projection: ModelProjection,
    /// How far the footprints reach to each side: retail's, or the view's
    /// own where it sees further.
    pub footprint: TerrainFootprintWidth,
    pub coordinate_shift: u8,
    pub tint: [u8; 3],
    pub lighting: TerrainLighting<'a>,
    pub clock: u32,
    /// Actual H14 header DWORD+4; bit0x40 enables authored water waves.
    pub world_flags: u32,
    /// Native world+5C3 (infrared vision), with its pane colour from
    /// [`night_material`] over the live display layout.
    pub night: Option<u32>,
    /// Process005483B0, also read by camera shake. This is an event gate,
    /// independently of the graphics settings and the process particle bank.
    pub particles_enabled: bool,
    pub tables: &'a PrimitiveTables,
}

/// AA10 borrows live terrain mutably only during its furniture phase.
pub struct TerrainDrawRequest<'a> {
    pub terrain: &'a mut TerrainRuntime,
    pub inputs: TerrainDrawInputs<'a>,
}

pub struct TerrainFurnitureRequest<'a> {
    pub terrain: &'a mut TerrainRuntime,
    pub domain: &'a FurnitureDomain,
    pub inputs: TerrainDrawInputs<'a>,
    /// AA10's initial H7/74 context+6C, already converted by the backend.
    pub far_color: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainFeature {
    Furniture,
    Particles,
    NearStrip,
    WaterStrip,
    FarStrip,
}

/// Required operations at native reached boundaries. Services may return an
/// error after a committed prefix; the traversal reports that feature and
/// continues the next native strip/pass. `omitted` must log the supplied cause.
pub trait TerrainDrawHost<'a> {
    type Error;
    fn material(&mut self, slot: i16) -> Result<u32, Self::Error>;
    fn texture(&mut self, slot: i16) -> Result<Option<RenderResource<'a>>, Self::Error>;
    fn material_sorting(&mut self) -> Result<bool, Self::Error>;
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    fn publish_far_color(&mut self, color: u32) -> Result<(), Self::Error>;
    fn publish_fade(&mut self, fade: [i32; 2]) -> Result<(), Self::Error>;
    fn begin_timing(&mut self) -> Result<(), Self::Error>;
    fn end_timing(&mut self) -> Result<(), Self::Error>;
    fn furniture(&mut self, request: TerrainFurnitureRequest<'_>) -> Result<(), Self::Error>;
    fn particles(&mut self, request: TerrainParticleEmissionRequest<'_>)
        -> Result<(), Self::Error>;
    fn water(
        &mut self,
        request: TerrainStripRequest<'_>,
    ) -> Result<TerrainDrawOutcome, Self::Error>;
    fn clipped_tile(&mut self, request: TerrainClipRequest<'a>) -> Result<usize, Self::Error>;
    fn submit_quad(&mut self, command: TerrainQuadCommand<'a>) -> Result<(), Self::Error>;
    fn omitted(&mut self, feature: TerrainFeature, error: TerrainDrawError<Self::Error>);
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TerrainDrawFault {
    #[error("unsupported native terrain grid {spacing}/{mask:#x}")]
    Grid { spacing: u16, mask: u16 },
    #[error("native terrain footprint has four horizontal edges")]
    FlatFootprint,
    #[error("terrain row {left}..{right} exceeds the native fifteen-vertex buffer")]
    RowCapacity { left: i16, right: i16 },
    #[error("terrain tile references an unwritten {field}")]
    Unwritten { field: &'static str },
    #[error("terrain strip vertex {index} is outside its initialized buffer of {count}")]
    Vertex { index: i32, count: usize },
    #[error("terrain texture blend orientation {0:#x} is not a permutation")]
    Orientation(u32),
    #[error("terrain texture has an incompatible resident record")]
    Texture,
    #[error("terrain native division overflowed or divided by zero")]
    Division,
    #[error("terrain dynamic light grid has {0} bytes rather than90*90*3")]
    LightGrid(usize),
}

#[derive(Debug, Error)]
pub enum TerrainDrawError<E> {
    #[error(transparent)]
    Fault(#[from] TerrainDrawFault),
    #[error("required terrain draw host operation failed")]
    Host(E),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainDrawOutcome {
    pub quads: usize,
    pub rejected: usize,
    pub omitted_features: usize,
}

/// AA10 publishes original fade before furniture,30%/55% before near strips,
/// then125%/150% before far strips. At return it restores the caller CONTEXT
/// fade/spacing, but does not republish 0048FB00: the far global fade survives.
pub fn draw_terrain<'a, H: TerrainDrawHost<'a>>(
    request: TerrainDrawRequest<'_>,
    host: &mut H,
) -> Result<TerrainDrawOutcome, TerrainDrawError<H::Error>> {
    let TerrainDrawRequest { terrain, inputs } = request;
    let grid = TerrainGrid {
        spacing: terrain.sampling().spacing.units(),
        mask: terrain.sampling().spacing.units() - 1,
    };
    // H7/74 precedes footprint arithmetic and every batch, including at night.
    let material74 = host.material(0x74).map_err(TerrainDrawError::Host)?;
    //44AA8E writes context+6C before footprint work; the alternate policy
    //44AAC4 replaces it with packed black. Do not inherit a prior scene color.
    let initial_far_color = if inputs.night.is_some() {
        0
    } else {
        material74
    };
    host.publish_far_color(initial_far_color)
        .map_err(TerrainDrawError::Host)?;
    let plan = terrain_draw_plan(
        inputs.view.basis,
        inputs.context_projection.fade,
        grid,
        inputs.footprint,
    )?;
    let mut outcome = TerrainDrawOutcome::default();
    host.publish_fade(inputs.context_projection.fade)
        .map_err(TerrainDrawError::Host)?;
    host.begin_timing().map_err(TerrainDrawError::Host)?;
    let domain = FurnitureDomain {
        origin_x: inputs.view.eye[0] as i16,
        origin_z: inputs.view.eye[2] as i16,
        scale: grid.spacing,
        mask: grid.mask,
        coords: plan
            .furniture
            .rows
            .iter()
            .flat_map(|row| [row.z as u16, row.left as u16, row.right as u16])
            .collect(),
        rows: plan.furniture.rows.len(),
    };
    if let Err(error) = host.furniture(TerrainFurnitureRequest {
        terrain: &mut *terrain,
        domain: &domain,
        inputs,
        far_color: initial_far_color,
    }) {
        host.omitted(TerrainFeature::Furniture, TerrainDrawError::Host(error));
        outcome.omitted_features += 1;
    }
    host.end_timing().map_err(TerrainDrawError::Host)?;
    host.begin_timing().map_err(TerrainDrawError::Host)?;
    host.publish_fade(plan.near_fade)
        .map_err(TerrainDrawError::Host)?;
    for rows in plan.near.strips() {
        if inputs.particles_enabled {
            if let Err(error) = host.particles(TerrainParticleEmissionRequest {
                terrain,
                context_position: inputs.view.eye,
                rows: &rows,
            }) {
                host.omitted(TerrainFeature::Particles, TerrainDrawError::Host(error));
                outcome.omitted_features += 1;
                // Native setjmp aborts the remaining ground/water operations
                // of this strip when the earlier particle service fails.
                continue;
            }
        }
        // BC30 resolves H7/74 again for each strip, before accessing H12.
        let pane_material = host.material(0x74).map_err(TerrainDrawError::Host)?;
        let strip = TerrainStripRequest {
            terrain: &*terrain,
            draw: inputs,
            rows: &rows,
            grid,
            mode: TerrainStripMode::Near,
            fade: plan.near_fade,
            sort_bias: plan.sort_bias,
            material: inputs.night.unwrap_or(pane_material),
            far_color: if inputs.night.is_some() {
                0
            } else {
                material74
            },
        };
        match strip::draw_strip(strip, host) {
            Ok(result) => {
                outcome.quads += result.quads;
                outcome.rejected += result.rejected;
            }
            Err(error) => {
                host.omitted(TerrainFeature::NearStrip, error);
                outcome.omitted_features += 1;
                continue;
            }
        }
        if (terrain.plane_base() >> terrain.plane_shift()) as i16 > -4096 {
            match host.water(strip) {
                Ok(result) => {
                    outcome.quads += result.quads;
                    outcome.rejected += result.rejected;
                }
                Err(error) => {
                    host.omitted(TerrainFeature::WaterStrip, TerrainDrawError::Host(error));
                    outcome.omitted_features += 1;
                }
            }
        }
    }
    host.end_timing().map_err(TerrainDrawError::Host)?;
    host.begin_timing().map_err(TerrainDrawError::Host)?;
    let far_color = host
        .material(if inputs.night.is_some() { 0x20 } else { 0x73 })
        .map_err(TerrainDrawError::Host)?;
    host.publish_fade(plan.far_fade)
        .map_err(TerrainDrawError::Host)?;
    for rows in plan.far.strips() {
        let pane_material = host.material(0x74).map_err(TerrainDrawError::Host)?;
        let strip = TerrainStripRequest {
            terrain: &*terrain,
            draw: inputs,
            rows: &rows,
            grid: TerrainGrid::FAR,
            mode: TerrainStripMode::Far,
            fade: plan.far_fade,
            sort_bias: plan.sort_bias,
            material: inputs.night.unwrap_or(pane_material),
            far_color,
        };
        match strip::draw_strip(strip, host) {
            Ok(result) => {
                outcome.quads += result.quads;
                outcome.rejected += result.rejected;
            }
            Err(error) => {
                host.omitted(TerrainFeature::FarStrip, error);
                outcome.omitted_features += 1;
            }
        }
    }
    host.end_timing().map_err(TerrainDrawError::Host)?;
    Ok(outcome)
}

/// Every byte of `bytes`, if all were written.
fn all_written<const N: usize>(bytes: [Option<u8>; N]) -> Option<[u8; N]> {
    bytes
        .iter()
        .all(Option::is_some)
        .then(|| bytes.map(Option::unwrap_or_default))
}

#[cfg(test)]
mod tests;
