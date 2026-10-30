//! The `0040D6A0` panel's frame: `00455080` draws H8 slot 99 or 100 with the
//! `00455360` packed-value callback. The briefing's five panels and the
//! player message's two boxes share it. Each owner stores the frame's shape
//! (+3C/+3E centre, +4C extent, +50 model, +51 flag, +54 orientation) and
//! animates it with `004554E0` ([`PanelAnimation`]).
//!
//! The frame neither begins nor flushes a batch; its owners do.

use crate::{
    frontend_camera::AnimationClock,
    frontend_scene::{
        FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneModelSubmission,
        SceneProjection, SceneSetup, SceneView,
    },
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
    panel_animation::PanelAnimation,
};

/// A panel's stored frame shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelFrame {
    /// +38, from +50: slot 99 when +50 is set, otherwise 100.
    pub model_slot: i16,
    /// +3C/+3E, in 1024ths of the surface.
    pub center_1024: [i16; 2],
    /// +4C: the extent WORD once the panel is open.
    pub maximum_extent_word: u16,
    /// +51, the callback's selector 4.
    pub flag: u8,
    /// +54: after the fixed YZ quarter turn, 0, 2 and 3 turn the frame in XZ
    /// (`00491530`) by -0x4000, 0x4000 and 0x8000; other values leave it.
    pub orientation: u32,
}

impl PanelFrame {
    /// The slot `004551AF` stores at +38 for a +50 byte.
    pub const fn model_for(byte_50: u8) -> i16 {
        if byte_50 != 0 {
            99
        } else {
            100
        }
    }

    /// `00455320` narrows each product to a signed WORD.
    pub fn center_pixels(self, viewport: [i32; 2]) -> [i16; 2] {
        std::array::from_fn(|axis| {
            (viewport[axis].wrapping_mul(i32::from(self.center_1024[axis])) >> 10) as i16
        })
    }

    fn orientation_angle(self) -> Option<i32> {
        match self.orientation {
            0 => Some(-0x4000),
            2 => Some(0x4000),
            3 => Some(0x8000),
            _ => None,
        }
    }
}

pub fn panel_visible(state: PanelAnimation) -> bool {
    state.current_stage != 0 || state.requested_stage != 0
}

/// 00455360 divides after shifting both countdown DWORDs right ten bits.
/// Keeping this quantization matters near the stage boundary. Native
/// multiplication wraps at 32 bits, division is signed, and return narrows.
fn countdown_product(value: u16, state: PanelAnimation) -> i32 {
    i32::from(value).wrapping_mul(state.remaining_micros >> 10) / (state.duration_micros >> 10)
}

pub(crate) fn panel_callback(shape: PanelFrame, state: PanelAnimation) -> ModelCallbackPolicy {
    let current = state.current_stage;
    let requested = state.requested_stage;
    let maximum = shape.maximum_extent_word;
    let extent = match current {
        0 if requested > current => {
            i32::from(maximum).wrapping_add(countdown_product(0xffff - maximum, state)) as u16
        }
        0 => 0xffff,
        1 if requested < current => {
            0xffffi32.wrapping_sub(countdown_product(0xffff - maximum, state)) as u16
        }
        _ => maximum,
    };
    let unfold = match current {
        0 if requested > current => countdown_product(0xffff, state) as u16,
        0 => 0xffff,
        1 if requested < current => 0xffffi32.wrapping_sub(countdown_product(0xffff, state)) as u16,
        _ => 0,
    };
    let scale = match current {
        1 if requested > current => i32::from(state.current_scale_word)
            .wrapping_sub(countdown_product(state.current_scale_word, state))
            as u16,
        2 if requested < current => countdown_product(state.current_scale_word, state) as u16,
        2 => state.current_scale_word,
        _ => 0,
    };
    ModelCallbackPolicy::Panel {
        extent,
        unfold,
        scale,
        flag: shape.flag,
    }
}

/// Original 00455080, including the 00455360 packed-value callback. A closed
/// panel emits no operations. Context 0043B4A0 supplies an identity view and
/// its default fade bounds; the frame scales its focal words by 32.
pub fn draw_panel_frame<H: FrontendSceneHost>(
    shape: PanelFrame,
    state: PanelAnimation,
    projection: SceneProjection,
    animation: AnimationClock,
    math: &MenuBasisMath,
    host: &mut H,
) -> Result<(), H::Error> {
    if !panel_visible(state) {
        return Ok(());
    }
    // Unlike 0043B5D0, this draw uses +2356/-2356 for the last two values.
    // 00402680 constructs stored floats from signed integers /4096;
    // 00402690 is a vector constructor, not a normalization operation.
    host.submit(SceneCommand::DirectionalLight {
        direction: [-2365.0 / 4096.0, 2356.0 / 4096.0, -2356.0 / 4096.0],
        rgb: [128; 3],
    })?;
    host.submit(SceneCommand::AmbientLight([32; 3]))?;
    let mut setup = SceneSetup::context(projection);
    setup.focal = setup.focal.map(|value| value.wrapping_shl(5));
    setup.center = shape.center_pixels(projection.viewport).map(i32::from);
    host.submit(SceneCommand::Setup(setup))?;
    // Context+0 is the renderer pointer, +6C the far color. Neither the
    // template copy nor this layer writes global coordinate shift0055F250.
    host.submit(SceneCommand::FarColor(0))?;
    host.submit(SceneCommand::PublishView(SceneView {
        eye: [0; 3],
        basis: MenuBasis::INITIAL,
    }))?;
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, -16384, RotationPlane::Yz);
    if let Some(angle) = shape.orientation_angle() {
        math.rotate_in_plane(&mut basis, angle, RotationPlane::Xz);
    }
    host.submit(SceneCommand::Model(SceneModelSubmission {
        model_slot: shape.model_slot,
        position: [0, 0, 9600],
        basis,
        animation,
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
        policy: panel_callback(shape, state),
    }))
}

#[cfg(test)]
#[path = "panel_frame_tests.rs"]
mod tests;
