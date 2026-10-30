//! Briefing terrain-pane context, backdrop and camera static, 0040BB40.
//! The retained context is separate from the fade globals changed for H8/278.

mod noise;
pub use noise::{BriefingNoiseError, BriefingNoisePass, BriefingNoiseRows};

use thiserror::Error;

use crate::{
    briefing_runtime::BriefingPanelAnimation,
    briefing_scene::{panel_visible, BriefingSceneFrame},
    frontend_scene::{
        ModelCallbackPolicy, SceneModelSubmission, SceneProjection, SceneSetup, SceneView,
    },
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
    scene_quad::DirectFlatQuad,
};

#[derive(Clone, Copy, Debug)]
pub struct BriefingTerrainInputs {
    pub frame: BriefingSceneFrame,
    pub animation: BriefingPanelAnimation,
    /// Screen +160/+162/+164. Each WORD is sign-extended at 0040BF5E.
    pub camera_position_words: [u16; 3],
    /// Screen +1DC, passed to 00491530 before literal pitch 0x1000.
    pub heading: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingTerrainPane {
    pub clip_rect: [i16; 4],
    pub context: SceneSetup,
    pub view: SceneView,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum BriefingTerrainError {
    #[error("0040BB40 opening/closing percentage divides by zero at duration/1000")]
    DurationDivision,
}

pub fn briefing_terrain_pane(
    inputs: BriefingTerrainInputs,
    math: &MenuBasisMath,
) -> Result<Option<BriefingTerrainPane>, BriefingTerrainError> {
    let state = inputs.animation;
    if !panel_visible(state) {
        return Ok(None);
    }
    let [width, height] = inputs.frame.projection.viewport;
    // BB40 keeps these DWORDs; the enclosing 55080 frame narrows separately.
    let center = [
        width.wrapping_mul(0x348) >> 10,
        height.wrapping_mul(0x32f) >> 10,
    ];
    let mut percent = [34i32, 38i32];
    if state.current_stage == 0 || (state.current_stage == 1 && state.requested_stage == 0) {
        let denominator = state.duration_micros / 1000;
        if denominator == 0 {
            return Err(BriefingTerrainError::DurationDivision);
        }
        let progress =
            state.duration_micros.wrapping_sub(state.remaining_micros) / 10 / denominator;
        let progress = if state.current_stage == 0 {
            progress
        } else {
            100i32.wrapping_sub(progress)
        };
        percent = percent.map(|value| progress.wrapping_mul(value) / 100);
    }
    let half = [
        width.wrapping_mul(percent[0]) / 100 / 2,
        height.wrapping_mul(percent[1]) / 100 / 2,
    ];
    let right = center[0].wrapping_add(half[0]);
    let bottom = center[1].wrapping_add(half[1]);
    let clip_rect = [
        center[0].wrapping_sub(half[0]).max(0) as i16,
        center[1].wrapping_sub(half[1]).max(0) as i16,
        right.min(width.wrapping_sub(1)) as i16,
        bottom.min(height.wrapping_sub(1)) as i16,
    ];
    let mut context = SceneSetup::context(SceneProjection {
        focal: std::array::from_fn(|axis| {
            inputs.frame.projection.focal[axis].wrapping_mul(percent[axis]) / 100
        }),
        // 40BE85/40BEA2 consume raw DWORD right/bottom, not clip WORDs.
        viewport: [right.min(width), bottom.min(height)],
        fade: [0x1800, 0x1a00],
    });
    context.center = center;
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, inputs.heading, RotationPlane::Xz);
    math.rotate_in_plane(&mut basis, 0x1000, RotationPlane::Yz);
    Ok(Some(BriefingTerrainPane {
        clip_rect,
        context,
        view: SceneView {
            eye: inputs
                .camera_position_words
                .map(|value| i32::from(value as i16)),
            basis,
        },
    }))
}

impl BriefingTerrainPane {
    /// H14/0 header DWORD +10 returned by 0048EDA0 supplies Y. Its ECX
    /// argument is unused; the world subobject address is not the height.
    pub fn backdrop(&self, header_height: i32, frame: BriefingSceneFrame) -> SceneModelSubmission {
        SceneModelSubmission {
            model_slot: 0x116,
            position: [self.view.eye[0], header_height, self.view.eye[2]],
            basis: MenuBasis::INITIAL,
            animation: frame.animation,
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::InheritedRenderer,
            policy: ModelCallbackPolicy::NameEntry {
                selector_one: frame.selector_one,
            },
        }
    }

    /// Direct renderer +84 follows backdrop generation and precedes its
    /// deferred flush. Depth 100000 and descriptor zero are actual literals.
    pub fn far_fill(&self, color_word: u32) -> DirectFlatQuad {
        let [left, top, right, bottom] = self.clip_rect;
        DirectFlatQuad {
            corners: [[left, top], [right, top], [right, bottom], [left, bottom]],
            vertex_depths: [100_000; 4],
            descriptor_word: 0,
            color_word,
        }
    }
}

#[cfg(test)]
#[path = "briefing_terrain_tests.rs"]
mod tests;
