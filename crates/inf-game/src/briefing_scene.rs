//! Native briefing panel frames (00455080), model preview (0040B840), and
//! decorative model (0040AD00/0040AAF0), submitted to the shared H8 executor.
//!
//! These owners use original resource slots and native scene coordinates.
//! They neither begin nor flush batches; 0040B0F0 owns their interleaving with
//! terrain, map and text. Missing resources/execution fail at the host boundary.

use inf_formats::BriefingModel;

#[cfg(test)]
use crate::panel_frame::panel_callback;
pub use crate::panel_frame::panel_visible;
use crate::{
    briefing_runtime::{BriefingPanel, BriefingPanelAnimation},
    frontend_camera::AnimationClock,
    frontend_scene::{
        FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneModelSubmission,
        SceneProjection, SceneSetup, SceneView,
    },
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
    panel_frame::{draw_panel_frame, PanelFrame},
};

#[derive(Clone, Copy, Debug)]
pub struct BriefingSceneFrame {
    /// Shared context-template fade pair, original focal words 00546F20/24,
    /// and the live renderer dimensions. Each layer supplies its own center.
    pub projection: SceneProjection,
    /// Native screen +C0 and +F8, in milliseconds.
    pub age_millis: i32,
    pub start_remaining_millis: u32,
    pub animation: AnimationClock,
    /// Shared process word 00533760 returned by 0043A270.
    pub selector_one: u16,
}

impl PanelFrame {
    /// Constructor 0040A420 stores +3C, +4C, +50, +51, and +54 in each
    /// sub-object. All five orientations (+54) are 1, so only YZ rotates.
    fn briefing(panel: BriefingPanel) -> Self {
        let (byte_50, center_1024, maximum_extent_word, flag) = match panel {
            BriefingPanel::Map => (0, [0x302, 0x122], 0x4000, 1),
            BriefingPanel::Terrain => (0, [0x348, 0x32f], 0x7800, 1),
            BriefingPanel::Model => (1, [400, 0x172], 0xa800, 0),
            BriefingPanel::Text => (1, [0xb4, 0x316], 0x8000, 0),
            BriefingPanel::Header => (1, [100, 0x82], 0xc000, 0),
        };
        Self {
            model_slot: Self::model_for(byte_50),
            center_1024,
            maximum_extent_word,
            flag,
            orientation: 1,
        }
    }
}

pub(crate) fn panel_center_pixels(panel: BriefingPanel, viewport: [i32; 2]) -> [i16; 2] {
    PanelFrame::briefing(panel).center_pixels(viewport)
}

/// A briefing panel's 00455080 frame.
pub fn draw_briefing_panel_frame<H: FrontendSceneHost>(
    panel: BriefingPanel,
    state: BriefingPanelAnimation,
    frame: BriefingSceneFrame,
    math: &MenuBasisMath,
    host: &mut H,
) -> Result<(), H::Error> {
    draw_panel_frame(
        PanelFrame::briefing(panel),
        state,
        frame.projection,
        frame.animation,
        math,
        host,
    )
}

fn preview_light<H: FrontendSceneHost>(host: &mut H) -> Result<(), H::Error> {
    // 0043B5D0 uses the stored fractions [-2365,+2365,-2365]/4096.
    host.submit(SceneCommand::DirectionalLight {
        direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
        rgb: [128; 3],
    })?;
    host.submit(SceneCommand::AmbientLight([32; 3]))
}

/// 0040B840's authored kind-2 payload. `visibility_extent` is the first DWORD
/// of that actual H8 resource, not a diagnostic bound or inferred mesh size.
/// The caller owns the model-panel stage gate and its following frame draw.
/// `map_frame_drawn`: the map panel's frame ran first in this pass, so the
/// model's root starts with that frame's context bases
/// (`ModelStackRegisterPolicy::BriefingPreviewMapFrameBases`).
pub fn draw_briefing_model_preview<H: FrontendSceneHost>(
    authored: BriefingModel,
    model_age_millis: i32,
    visibility_extent: u32,
    map_frame_drawn: bool,
    frame: BriefingSceneFrame,
    math: &MenuBasisMath,
    host: &mut H,
) -> Result<(), H::Error> {
    preview_light(host)?;
    let product = (model_age_millis as u32).wrapping_shl(4);
    let fade = product.min(0x7fff) as i32;
    let z = visibility_extent
        .wrapping_add(i32::from(authored.depth_bias) as u32)
        .wrapping_add(600) as i32;
    let fade_z = z.wrapping_add(i32::from((fade.wrapping_mul(1500) >> 15) as i16));
    let mut setup = SceneSetup::context(SceneProjection {
        fade: [fade_z.wrapping_sub(1000), fade_z.wrapping_sub(100)],
        ..frame.projection
    });
    setup.center = [
        (frame.projection.viewport[0].wrapping_mul(400) >> 10).wrapping_add(
            frame.projection.viewport[0]
                .wrapping_mul(i32::from(authored.screen_shift_per_thousand[0]))
                / 1000,
        ),
        (frame.projection.viewport[1].wrapping_mul(0x172) >> 10)
            .wrapping_add(
                frame.projection.viewport[1]
                    .wrapping_mul(i32::from(authored.screen_shift_per_thousand[1]))
                    / 1000,
            )
            .wrapping_add(frame.projection.viewport[1].wrapping_mul(-25) / 100),
    ];
    host.submit(SceneCommand::Setup(setup))?;
    // B840's template copy retains the incoming global coordinate shift.
    // 0048FB00 only publishes fade arithmetic, not0055F250.
    host.submit(SceneCommand::FarColor(0))?;
    host.submit(SceneCommand::PublishView(SceneView {
        eye: [0, z.wrapping_mul(2) / 5, 0],
        basis: MenuBasis::INITIAL,
    }))?;
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(
        &mut basis,
        frame.age_millis.wrapping_shl(2),
        RotationPlane::Xz,
    );
    host.submit(SceneCommand::Model(SceneModelSubmission {
        model_slot: authored.model_slot,
        position: [
            i32::from(authored.position_xy[0]),
            i32::from(authored.position_xy[1]),
            z,
        ],
        basis,
        animation: frame.animation,
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: if map_frame_drawn {
            crate::frontend_scene::ModelFailurePolicy::BriefingPreviewMapFrameBases
        } else {
            crate::frontend_scene::ModelFailurePolicy::StopScene
        },
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
        policy: ModelCallbackPolicy::NameEntry {
            selector_one: frame.selector_one,
        },
    }))
}

/// 0040AD00/0040AAF0: original H8 slot98 after both menus. H6 slot5 supplies
/// the light's three low bytes; opening and Start use signed WORD fade values.
pub fn draw_briefing_decoration<H: FrontendSceneHost>(
    visibility_extent: u32,
    style_word: u32,
    frame: BriefingSceneFrame,
    math: &MenuBasisMath,
    host: &mut H,
) -> Result<(), H::Error> {
    let fade = if frame.start_remaining_millis != 0 {
        frame
            .start_remaining_millis
            .wrapping_mul(0xffff_8000)
            .wrapping_add(0x3e8_0000)
            / 2000
    } else if frame.age_millis < 1000 {
        (1000i32.wrapping_sub(frame.age_millis).wrapping_mul(0x8000) / 1000) as u32
    } else {
        0
    } as i16;
    let rgb = style_word.to_le_bytes();
    host.submit(SceneCommand::DirectionalLight {
        direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
        rgb: [rgb[0], rgb[1], rgb[2]],
    })?;
    host.submit(SceneCommand::AmbientLight(
        [rgb[0], rgb[1], rgb[2]].map(|value| i32::from(value >> 2)),
    ))?;
    let extent = visibility_extent as i32;
    let adjustment = i32::from((i32::from(fade).wrapping_mul(extent).wrapping_mul(3) >> 15) as i16);
    let mut setup = SceneSetup::context(SceneProjection {
        fade: [
            500i32.wrapping_sub(extent).wrapping_mul(2),
            extent.wrapping_add(1000).wrapping_sub(adjustment),
        ],
        ..frame.projection
    });
    setup.center = [
        frame.projection.viewport[0].wrapping_mul(0x8c) >> 8,
        frame.projection.viewport[1].wrapping_mul(0x104) >> 8,
    ];
    host.submit(SceneCommand::Setup(setup))?;
    // AD00 copies the incoming context; AAF0 clears its +6C far color.
    // It does not change global coordinate shift0055F250.
    host.submit(SceneCommand::FarColor(0))?;
    host.submit(SceneCommand::PublishView(SceneView {
        eye: [0; 3],
        basis: MenuBasis::INITIAL,
    }))?;
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, 0x1000, RotationPlane::Xy);
    math.rotate_in_plane(
        &mut basis,
        frame.age_millis.wrapping_shl(2),
        RotationPlane::Xz,
    );
    host.submit(SceneCommand::Model(SceneModelSubmission {
        model_slot: 98,
        position: [0, 0, 1000],
        basis,
        animation: frame.animation,
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::InheritedRenderer,
        policy: ModelCallbackPolicy::NameEntry {
            selector_one: frame.selector_one,
        },
    }))
}

#[cfg(test)]
#[path = "briefing_scene_tests.rs"]
mod tests;
