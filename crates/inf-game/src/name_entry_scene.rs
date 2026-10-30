//! Name-entry backdrop 0045A560, called by the text-entry draw 00459310.
//!
//! Lights, publishes the undoubled context projection and an identity view,
//! then queues models 748, 757 and 744 through 0049FEA0. It neither begins
//! nor flushes a batch and leaves the coordinate shift 0055F250 untouched;
//! the draw's own renderer begin precedes it and its flush follows the stars.
//! Model execution, resources and pixels remain the host's operations.

use inf_formats::native_basis::{normalize_words, NativeLengthPrecision};

use crate::{
    frontend_camera::AnimationClock,
    frontend_scene::{
        FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneModelSubmission,
        SceneProjection, SceneSetup, SceneView,
    },
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
};

/// Model slots in submission order (object word +38).
pub const NAME_ENTRY_BACKDROP_MODELS: [i16; 3] = [748, 757, 744];

#[derive(Clone, Copy, Debug)]
pub struct NameEntrySceneFrame {
    /// Focal 00546F20/24 (published as-is), surface width/height, and the
    /// shared context template's +64/+68 fade pair.
    pub projection: SceneProjection,
    /// Screen +C0 in milliseconds, signed and wrapping like the original.
    pub age_millis: i32,
    /// Shared clock 005471C0, selector 0 of the object callback.
    pub animation: AnimationClock,
    /// Process word 00533760 when the frame is drawn.
    pub selector_one: u16,
}

/// Emit 0045A560's ordered commands. Native publication interleaves the
/// projection, the first object's rotation and the fade factor, but the
/// rotation is object-local, so publishing Setup whole is equivalent.
pub fn draw_name_entry_scene<H: FrontendSceneHost>(
    frame: NameEntrySceneFrame,
    math: &MenuBasisMath,
    host: &mut H,
) -> Result<(), H::Error> {
    let age = frame.age_millis;
    // 0045A579..0045A588: a signed clamp of the wrapped product.
    let fade = age.wrapping_mul(4).min(0x7fff);
    let scaled = |factor: i32| fade.wrapping_mul(factor) >> 15;
    // 0045A5C8: 6000 - min(age, 16000), narrowed by 00417DA0's MOVSX.
    let light_z = 6000i32.wrapping_sub(age.min(16_000)) as i16;
    host.submit(SceneCommand::DirectionalLight {
        direction: normalize_words([4000, 4000, light_z], NativeLengthPrecision::Retained),
        rgb: [255, 210, 170].map(|factor| scaled(factor) as u8),
    })?;
    // 0045A5F1..0045A611 pass sign-extended words to 00490130.
    host.submit(SceneCommand::AmbientLight(
        [10, 6, 5].map(|factor| i32::from(scaled(factor) as i16)),
    ))?;
    host.submit(SceneCommand::Setup(SceneSetup::context(frame.projection)))?;
    // 0045A6EE..0045A74E publish the shared template's never-written zero
    // eye and its initializer-written identity basis.
    host.submit(SceneCommand::PublishView(SceneView {
        eye: [0; 3],
        basis: MenuBasis::INITIAL,
    }))?;

    let submission = |model_slot, position, basis| {
        SceneCommand::Model(SceneModelSubmission {
            model_slot,
            position,
            basis,
            animation: frame.animation,
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::NameEntry {
                selector_one: frame.selector_one,
            },
        })
    };
    // One stack object is refilled per model, each time with a fresh copy of
    // the process identity basis 00547590, counter included.
    let mut spinning = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut spinning, age.wrapping_mul(4), RotationPlane::Xz);
    host.submit(submission(748, [0, 0, 15_000], spinning))?;

    let mut turned = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut turned, 0x8000, RotationPlane::Xz);
    // 0045A776..0045A7C2: wrapped 30*age and 20*age, then signed /10.
    host.submit(submission(
        757,
        [
            (age.wrapping_mul(30) / 10).min(36_000),
            (age.wrapping_mul(20) / 10).min(24_000),
            50_000,
        ],
        turned,
    ))?;

    // 0045A851..0045A874: the approach stops at -28000.
    host.submit(submission(
        744,
        [
            age.wrapping_sub(2500).wrapping_mul(20).min(-28_000),
            -6000,
            30_000,
        ],
        MenuBasis::INITIAL,
    ))
}

#[cfg(test)]
#[path = "name_entry_scene_tests.rs"]
mod tests;
