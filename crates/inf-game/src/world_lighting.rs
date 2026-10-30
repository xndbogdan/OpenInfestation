//! Shared signed RGB terrain-light grid, 0045B160/0045B180/0045B1A0.
//!
//! The world draw owns ordering: clear cells, optionally deposit its player
//! light under the retained anchor, publish the current anchor, then execute
//! particle/entity light writers before terrain reads the grid. Clearing does
//! not publish an anchor. Particle radius/floor/lifetime arithmetic belongs to
//! 00465720; this owner accepts its actual signed DWORD X/Z and radius operands.

use thiserror::Error;

use crate::{
    frontend_scene::SceneView,
    terrain_runtime::{spacing_pair, TerrainSpacing},
};

#[cfg(test)]
mod tests;

pub const WORLD_LIGHT_SIDE: usize = 90;
pub const WORLD_LIGHT_BYTES: usize = WORLD_LIGHT_SIDE * WORLD_LIGHT_SIDE * 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLightRequest {
    /// Full signed DWORD operands. Callers own native WORD sign extension.
    pub world_xz: [i32; 2],
    pub radius: i32,
    /// Low three bytes of the native packed color, consumed as signed chars.
    pub rgb: [i8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainLightColor {
    Unmodified,
    /// World +5C3, infrared vision: each signed channel times these unsigned
    /// bytes (`0055F15C`, `0053C678`, `0055F15D`) /255.
    Infrared([u8; 3]),
}

impl TerrainLightColor {
    /// `0045B29E`: the colour policy for the world manager's `+5C3`.
    pub fn for_infrared(infrared: bool) -> Self {
        if infrared {
            Self::Infrared(crate::world_tint::INFRARED_RGB)
        } else {
            Self::Unmodified
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLightPolicy {
    /// Actual DWORD settings +80 zero/nonzero policy.
    pub enabled: bool,
    /// Current process pair 005372F0/F4, before AA10's temporary far detail.
    pub spacing: TerrainSpacing,
    pub color: TerrainLightColor,
}

/// The current immutable settings loan for one draw. Both 00479508's entry
/// gate and each reached 0045B1A0 deposit read this actual owner. Spacing and
/// tint have distinct world/process owners; callers supply their admitted
/// current policy rather than deriving it from an object selector.
pub struct SettingsObjectLightPolicySource<'a> {
    pub settings: &'a crate::frontend_settings::FrontendSettings,
    pub spacing: TerrainSpacing,
    pub color: TerrainLightColor,
}

impl crate::object_runtime::ObjectLightPolicySource for SettingsObjectLightPolicySource<'_> {
    fn dynamic_lighting(&mut self) -> bool {
        self.settings.words()[0x80 / 4] != 0
    }

    fn terrain_light_policy(&mut self) -> TerrainLightPolicy {
        TerrainLightPolicy {
            enabled: self.settings.words()[0x80 / 4] != 0,
            spacing: self.spacing,
            color: self.color,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainLightOutcome {
    pub samples: u64,
    /// Native store visits, including a visit whose integer weight is zero.
    pub cell_writes: u64,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TerrainLightError {
    #[error("terrain light native IDIV faults: {numerator}/{denominator}")]
    Division { numerator: i32, denominator: i32 },
}

/// A light as it was deposited (port bookkeeping for the Vulkan tracer,
/// which shadows each source; the game reads only the cells): where, how
/// far it reaches and its colour as the cells took it, whether a particle
/// shed it, which nothing carries, and where it shines from when the
/// depositor knows ([`WorldLightGrid::add_from`]): the headlamp lights the
/// ground far ahead of the hull it shines from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLightSource {
    pub world_xz: [i32; 2],
    pub radius: i32,
    pub rgb: [i8; 3],
    pub particle: bool,
    /// Signed X, Y and Z.
    pub emitter: Option<[i32; 3]>,
    /// Non-native, for thermal vision: the colour asked for, before
    /// infrared's green, and whether it warms what it lights (all but the
    /// lamp infrared hangs on the player).
    pub own: [i8; 3],
    pub warms: bool,
}

#[derive(Clone, Debug)]
pub struct WorldLightGrid {
    anchors: [u16; 2],
    // Native 00548404..0054E2EF: X-major, Z-minor, three signed RGB bytes.
    cells: Box<[u8; WORLD_LIGHT_BYTES]>,
    /// The deposits since the last clear that reached the grid.
    sources: Vec<TerrainLightSource>,
}

impl Default for WorldLightGrid {
    fn default() -> Self {
        // These three native BSS regions start zero, and the light module's
        // initialise (0045B130, at boot) zeroes the anchors again; later
        // draws retain them.
        Self {
            anchors: [0; 2],
            cells: Box::new([0; WORLD_LIGHT_BYTES]),
            sources: Vec::new(),
        }
    }
}

impl WorldLightGrid {
    pub fn anchors(&self) -> [u16; 2] {
        self.anchors
    }
    pub fn cells(&self) -> &[u8] {
        self.cells.as_slice()
    }

    /// The deposits since the last clear that reached the grid.
    pub fn sources(&self) -> &[TerrainLightSource] {
        &self.sources
    }

    /// Non-native: the deposits from the `from`th on warm nothing under
    /// thermal vision (infrared's own lamp).
    pub fn mark_cold(&mut self, from: usize) {
        for source in self.sources.iter_mut().skip(from) {
            source.warms = false;
        }
    }

    /// 0045B160 clears exactly 0x5EEC bytes and leaves both anchors intact.
    pub fn clear(&mut self) {
        self.cells.fill(0);
        self.sources.clear();
    }

    /// 0045B180 subtracts 0x1680 then publishes only the low WORD of each
    /// supplied coordinate. This remains separate from clear for world ordering.
    pub fn set_anchor(&mut self, world_xz: [i32; 2]) {
        self.anchors = world_xz.map(|v| v.wrapping_sub(0x1680) as u16);
    }

    /// [`Self::add`] for a light shining from `emitter` (signed X, Y, Z):
    /// the actor carrying it, or for a `particle` (an explosion, a
    /// projectile's glow) the particle, whose light the tracer lets anything
    /// shadow.
    pub fn add_from(
        &mut self,
        request: TerrainLightRequest,
        policy: TerrainLightPolicy,
        emitter: [i32; 3],
        particle: bool,
    ) -> Result<TerrainLightOutcome, TerrainLightError> {
        let before = self.sources.len();
        let outcome = self.add(request, policy);
        for source in &mut self.sources[before..] {
            source.particle = particle;
            source.emitter = Some(emitter);
        }
        outcome
    }

    /// Execute 0045B1A0. Native integer wrap and unusual negative-radius square
    /// sign are intentional. A native division fault retains the written prefix.
    pub fn add(
        &mut self,
        request: TerrainLightRequest,
        policy: TerrainLightPolicy,
    ) -> Result<TerrainLightOutcome, TerrainLightError> {
        let mut outcome = TerrainLightOutcome::default();
        if !policy.enabled {
            return Ok(outcome);
        }
        let radius = request.radius.wrapping_abs();
        let mut radius_squared = radius.wrapping_mul(radius);
        if request.radius < 0 {
            radius_squared = radius_squared.wrapping_neg();
        }
        let (spacing, low_mask) = spacing_pair(policy.spacing);
        let mask = !i32::from(low_mask);
        let [x, z] = request.world_xz;
        let [anchor_x, anchor_z] = self.anchors.map(i32::from);
        let endpoint = |coordinate: i32, anchor: i32, sign: i32| {
            coordinate
                .wrapping_add(radius.wrapping_mul(sign))
                .wrapping_sub(anchor)
        };
        let x_inside = |sign| endpoint(x, anchor_x, sign) & mask & 0xff80 <= 0x2d00;
        let z_inside = |sign| (endpoint(z, anchor_z, sign) >> 7) & (mask >> 7) & 511 <= 90;
        if !(x_inside(1) || x_inside(-1)) || !(z_inside(1) || z_inside(-1)) || radius < 256 {
            return Ok(outcome);
        }
        let rgb = match policy.color {
            TerrainLightColor::Unmodified => request.rgb,
            TerrainLightColor::Infrared(tint) => std::array::from_fn(|i| {
                (i32::from(request.rgb[i]) * i32::from(tint[i]) / 255) as i8
            }),
        };
        self.sources.push(TerrainLightSource {
            world_xz: request.world_xz,
            radius,
            rgb,
            particle: false,
            emitter: None,
            own: request.rgb,
            warms: true,
        });
        let start = request.world_xz.map(|v| v.wrapping_sub(radius) & mask);
        let end = request.world_xz.map(|v| v.wrapping_add(radius) & mask);
        // Both endpoints are aligned to the same native power-of-two spacing.
        // Wrapping addition and endpoint equality preserve the end-exclusive
        // traversal even across 00000000, 80000000 and FFFFFFFF.
        let mut sample_x = start[0];
        while sample_x != end[0] {
            let delta_x = sample_x.wrapping_sub(x);
            let x_squared = delta_x.wrapping_mul(delta_x);
            let mut sample_z = start[1];
            while sample_z != end[1] {
                outcome.samples += 1;
                let delta_z = sample_z.wrapping_sub(z);
                let distance = delta_z.wrapping_mul(delta_z).wrapping_add(x_squared);
                if distance < radius_squared {
                    // Original IDIV precedes the grid-index bounds checks.
                    let numerator = radius_squared.wrapping_sub(distance).wrapping_shl(3);
                    let weight = numerator.checked_div(radius_squared).ok_or(
                        TerrainLightError::Division {
                            numerator,
                            denominator: radius_squared,
                        },
                    )?;
                    let indices = [(sample_x, anchor_x), (sample_z, anchor_z)]
                        .map(|(v, a)| ((v.wrapping_sub(a) >> 7) & 511 & (mask >> 7)) as usize);
                    if indices.iter().all(|&index| index < WORLD_LIGHT_SIDE) {
                        let offset = (indices[0] * WORLD_LIGHT_SIDE + indices[1]) * 3;
                        for (channel, color) in rgb.into_iter().enumerate() {
                            let product = i32::from(color).wrapping_mul(weight);
                            // SAR with sign correction is signed division by8,
                            // including a wrapped IMUL result.
                            let contribution = product.wrapping_add((product >> 31) & 7) >> 3;
                            let value = contribution
                                .wrapping_add(i32::from(self.cells[offset + channel] as i8));
                            self.cells[offset + channel] = value.clamp(-128, 127) as i8 as u8;
                        }
                        outcome.cell_writes += 1;
                    }
                }
                sample_z = sample_z.wrapping_add(i32::from(spacing));
            }
            sample_x = sample_x.wrapping_add(i32::from(spacing));
        }
        Ok(outcome)
    }
}

/// Shared 0044BC30 ground-read policy. Settings+80 gates these reads; furniture
/// reads the process grid independently. The executable owns off-grid RGB.
pub fn terrain_lighting_inputs<'a>(
    grid: &'a WorldLightGrid,
    enabled: bool,
    executable: inf_formats::FrontendExecutable<'_>,
) -> Result<crate::terrain_draw::TerrainLighting<'a>, inf_formats::FrontendExecutableError> {
    if !enabled {
        return Ok(crate::terrain_draw::TerrainLighting::Disabled);
    }
    let outside = executable.mapped(0x0052_6068, 3)?.bytes;
    Ok(crate::terrain_draw::TerrainLighting::Dynamic {
        anchors: grid.anchors(),
        cells: grid.cells(),
        outside: outside.try_into().expect("three original light bytes"),
    })
}

/// 00488020's inputs to 0045B180: context eye minus the truncated forward
/// components times EXE 005287B0 (-3840.0). The setter owns the later -0x1680.
pub fn world_light_anchor_coordinates(view: SceneView) -> [i32; 2] {
    [0, 2].map(|axis| {
        let forward = f64::from(f32::from_bits(view.basis.words[7 + axis]));
        let integral = (forward * -3840.0).trunc();
        // CRT 00511BA8 uses FISTP QWORD then takes the low DWORD; invalid masked
        // conversion's indefinite QWORD also has a zero low DWORD.
        const BOUND: f64 = 9_223_372_036_854_775_808.0;
        let delta = if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
            integral as i64 as i32
        } else {
            0
        };
        view.eye[axis].wrapping_sub(delta)
    })
}
