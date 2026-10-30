//! Fresh frontend display inputs and explicit CPU device policy.
//!
//! Resource selectors are display tiers here, not campaign difficulty. Source
//! H2 focal words are read through their actual resident registration owners.

use thiserror::Error;

use crate::{
    d3d_flat::D3dFlatState,
    d3d_texture::NativeTextureUploadState,
    frontend_app::FrontendAppStartup,
    frontend_scene::{SceneProjection, SceneView},
    menu::MenuBasis,
    model_context::ModelProjection,
    model_execution::{ModelEnvironment, ModelExecutionLimits},
    model_lighting::ModelLighting,
    presentation::NativeFrame,
    render_resources::{
        NativeColorLayout, RenderResourceData, RenderResourceError, RenderResourceKind,
        RenderResourceRequest, RenderResources,
    },
    software_frame::{D3dRasterState, D3dTextureSampling, SoftwareTextureState},
    software_scene::SoftwareSceneConfig,
    HandlerResources, ResidentRuntime,
};

#[derive(Debug, Error)]
pub enum FrontendDisplayError {
    #[error("unsupported frontend display dimensions {0:?}")]
    Dimensions([u32; 2]),
    #[error("unsupported frontend display variant {0}")]
    VariantIndex(u32),
    #[error("frontend display {dimensions:?} requires OVL variant {expected}, received {actual}")]
    Variant {
        dimensions: [u32; 2],
        expected: u8,
        actual: u8,
    },
    #[error("frontend focal H2 slot {0} is missing or has an invalid registration")]
    Focal(u16),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
}

#[cfg(test)]
#[path = "frontend_display_tests.rs"]
mod tests;

/// Native saved display index -> 00437C60 resource selector.
pub fn frontend_display_variant(dimensions: [u32; 2]) -> Result<u8, FrontendDisplayError> {
    match dimensions {
        [320, 240] => Ok(0),
        [512, 384] => Ok(1),
        [640, 480] => Ok(2),
        [800, 600] => Ok(3),
        [1024, 768] => Ok(4),
        _ => Err(FrontendDisplayError::Dimensions(dimensions)),
    }
}

/// Authored display dimensions for the saved +0C setting.
pub fn frontend_display_dimensions(variant: u32) -> Result<[u32; 2], FrontendDisplayError> {
    match variant {
        0 => Ok([320, 240]),
        1 => Ok([512, 384]),
        2 => Ok([640, 480]),
        3 => Ok([800, 600]),
        4 => Ok([1024, 768]),
        _ => Err(FrontendDisplayError::VariantIndex(variant)),
    }
}

fn focal_word(runtime: &ResidentRuntime, slot: u16) -> Result<i32, FrontendDisplayError> {
    let invalid = || FrontendDisplayError::Focal(slot);
    // 0053AA14 dispatch entry2 identifies pool0055EE08; 00437EDA..00437F22
    // reads its slots0/1 into00546F20/24. Never use a global slot as local index.
    let resource = runtime.resource(2, slot).ok_or_else(invalid)?;
    let bank = resource
        .overlay
        .handlers()
        .get(resource.registration.region_index)
        .ok_or_else(invalid)?;
    let HandlerResources::Words(bank) = bank else {
        return Err(invalid());
    };
    Ok(bank
        .entries
        .get(usize::from(resource.registration.local_index))
        .ok_or_else(invalid)?
        .raw as i32)
}

/// The canonical RGB555 display shifts of the CPU target.
const PRESENTATION_COLOR_LAYOUT: NativeColorLayout = NativeColorLayout {
    red_shift: 10,
    green_shift: 5,
    blue_shift: 0,
};

/// The H7 slot a linked presentation hands 00438120 for the next frame's
/// clear colour. The two callers are the only ones in the executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationClear {
    /// 0040D1D0, the standard +0C (frontend, name entry, briefing,
    /// results): slot 0x20, black in every bank so far.
    Standard,
    /// 00440930, the +0C of the gameplay screens (vtables 00523370 and
    /// 005233D8): slot 0x72, the world's sky colour, which shows above the
    /// sky dome's open top. Infest01 reads 2B58 (565) on Level 1 and 6A48 in
    /// the intro, the port's 15B8 and 3528 (555).
    Gameplay,
}

impl PresentationClear {
    pub const fn slot(self) -> i32 {
        match self {
            Self::Standard => 0x20,
            Self::Gameplay => 0x72,
        }
    }
}

/// Linked presentation colour: 00438120 reads the caller's H7 slot on every
/// call. A set gameplay byte `0055F160+5C4` selects packed black instead (its
/// only writers are the gameplay-object constructor, the world loader and an
/// in-world toggle). A vacant slot is the explicit `NotResident` error
/// rather than the native lazy load.
pub fn presentation_clear_rgba(
    resources: RenderResources<'_>,
    clear: PresentationClear,
    forced_black: bool,
) -> Result<[u8; 4], RenderResourceError> {
    if forced_black {
        return Ok([0, 0, 0, 255]);
    }
    Ok(presentation_word_rgba(resident_material_word(
        resources,
        clear.slot(),
    )?))
}

/// The converted resident H7 DWORD, as the material pool slot holds it.
pub(crate) fn resident_material_word(
    resources: RenderResources<'_>,
    argument: i32,
) -> Result<u32, RenderResourceError> {
    let material = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Material,
        argument,
    })?;
    let RenderResourceData::Material(material) = material.data else {
        unreachable!("H7 material")
    };
    Ok(PRESENTATION_COLOR_LAYOUT.convert_material(material))
}

/// 0050D960's colour fill of a display word on the CPU target.
pub fn presentation_word_rgba(word: u32) -> [u8; 4] {
    // 0050D960 normalizes display-masked components by their full mask. The
    // final RGBA8 rounding/opaque presentation alpha belong to this CPU target.
    let rgb = [10, 5, 0].map(|shift| (((word >> shift) & 31) * 255 + 15) / 31);
    [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, 255]
}

/// Start a fresh background run. This does not restore an already-running
/// retail session or infer globals from a camera preset. View and lighting
/// start from PE loader-zero globals0055F190..0055F2A8, beyond initialized
/// .data's547000 end; this is not a sampled post-title phase-2 state.
///
/// The tier's H2 words give the focal pair; the viewport and its centre are
/// the frame the game draws, wider than the tier under widescreen, so the
/// same lens shows more to each side.
pub fn frontend_startup(
    runtime: &ResidentRuntime,
    frame: NativeFrame,
) -> Result<FrontendAppStartup, FrontendDisplayError> {
    let dimensions = frame.tier_size;
    let expected = frontend_display_variant(dimensions)?;
    let actual = runtime.summary().selection.difficulty;
    if actual != expected {
        return Err(FrontendDisplayError::Variant {
            dimensions,
            expected,
            actual,
        });
    }
    let focal = [focal_word(runtime, 0)?, focal_word(runtime, 1)?];
    let viewport = frame.size.map(|v| v as i32);
    // 00439880 initializes context+64/+68, independent of display dimensions.
    let fade = [0x10_0000; 2];
    let color_layout = PRESENTATION_COLOR_LAYOUT;
    Ok(FrontendAppStartup {
        projection: SceneProjection {
            focal,
            viewport,
            fade,
        },
        clear_rgba: presentation_clear_rgba(
            RenderResources::new(runtime),
            PresentationClear::Standard,
            false,
        )?,
        scene: SoftwareSceneConfig {
            environment: ModelEnvironment {
                view: SceneView {
                    eye: [0; 3],
                    basis: MenuBasis { words: [0; 10] },
                },
                projection: ModelProjection {
                    focal,
                    viewport,
                    center: viewport.map(|v| v / 2),
                    fade,
                },
                coordinate_shift: 0,
                fog_factor: 0,
                lighting: ModelLighting {
                    direction: [0.; 3],
                    directional_word: 0,
                    ambient: [0; 3],
                    ambient_word: 0,
                },
            },
            primitive_mode: 0,
            far_color: 0,
            effect_color: 0,
            color_layout,
            // 004A49B0 initializes near64, depth enable/write1 and null secondary.
            renderer: D3dFlatState {
                material_shifts: [6, 1, 4],
                near_depth: 64,
                manages_depth: true,
                depth_enable: 1,
                depth_write: 1,
            },
            pipeline: D3dRasterState {
                // Native right/bottom are inclusive display dimensions; the
                // raster framebuffer bounds separately exclude x=width/y=height.
                // The display is the drawn frame, wider under widescreen.
                viewport: [0, 0, frame.size[0] as i16, frame.size[1] as i16],
                depth_enabled: true,
                depth_write: true,
                depth_function: 4,
                alpha_blend_enabled: true,
                shade_mode: 2,
                cull_mode: 1,
            },
            // Explicit portable device: canonical RGB555 source, alpha-capable
            // ARGB8888 textures, point sampling, Gouraud shading, depth surface.
            texture_state: SoftwareTextureState {
                upload: NativeTextureUploadState {
                    source_rgb_shifts: [10, 5, 0],
                    global_policy: 8,
                },
                sampling: D3dTextureSampling {
                    min_filter: 1,
                    mag_filter: 1,
                    texture_blend: 4,
                    address: 1,
                    color_key_enabled: false,
                    alpha_test_enabled: false,
                },
                secondary_texture: false,
            },
            queue_capacity: 0xc8000,
            execution_limits: ModelExecutionLimits {
                instructions: 100_000,
                invocation_depth: 16,
            },
        },
    })
}
