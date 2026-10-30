use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    model_projection::ProcessModelProjector,
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
};
fn process() -> ProcessModelGraphics {
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    ProcessModelGraphics::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap()
}
#[test]
fn renderer_copy_uses_current_default_and_scene_loans_never_recopy_or_reset() {
    let graphics = process();
    let clone = graphics.clone();
    assert!(clone.same_owner(&graphics));
    assert_eq!(
        graphics
            .selected(
                ModelSurfaceTableOrigin::TemplateDefault,
                SelectedProjectionSlot::Local1c
            )
            .unwrap(),
        ProjectionEntry::NativeNull
    );
    assert!(matches!(
        graphics.selected(
            ModelSurfaceTableOrigin::InheritedRenderer,
            SelectedProjectionSlot::Local1c
        ),
        Err(ProcessModelGraphicsError::RequiredRendererTable)
    ));
    graphics
        .select_projection(ModelSurfaceTableOrigin::TemplateDefault, ModelDispatch::Far)
        .unwrap();
    graphics.publish_renderer_constructor_copy().unwrap();
    clone
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            ModelDispatch::Near,
        )
        .unwrap();
    assert_eq!(
        graphics
            .selected(
                ModelSurfaceTableOrigin::InheritedRenderer,
                SelectedProjectionSlot::World20
            )
            .unwrap(),
        ProjectionEntry::FarWorld
    );
    assert_eq!(
        graphics
            .selected(
                ModelSurfaceTableOrigin::TemplateDefault,
                SelectedProjectionSlot::World20
            )
            .unwrap(),
        ProjectionEntry::NearWorld
    );
    // Actual replacement copy is a reached explicit phase, unlike an Rc loan.
    graphics.publish_renderer_constructor_copy().unwrap();
    assert_eq!(
        graphics
            .selected(
                ModelSurfaceTableOrigin::InheritedRenderer,
                SelectedProjectionSlot::Local1c
            )
            .unwrap(),
        ProjectionEntry::NearLocal
    );
}
#[test]
fn six_window_dwords_preserve_fade_factor_and_projector_reads_fresh_selected_pointer() {
    let graphics = process();
    graphics.publish_renderer_constructor_copy().unwrap();
    graphics
        .publish_fade(SceneFade {
            bounds: [100, 500],
            factor: 77,
        })
        .unwrap();
    graphics
        .publish_window(ModelProjectionWindow {
            focal: [10, 20],
            viewport: [300, 400],
            center: [150, 200],
        })
        .unwrap();
    assert_eq!(graphics.fade().unwrap(), [100, 500]);
    assert_eq!(graphics.fog_factor().unwrap(), 77);
    let mut output = ModelVertexProjection::default();
    let mut projector = ProcessModelProjector {
        process: &graphics,
        table: ModelSurfaceTableOrigin::TemplateDefault,
    };
    assert!(matches!(
        projector.project(ModelVertexInput::Local([0, 0, 0]), &mut output),
        Err(ProcessModelGraphicsError::NativeNullProjection { .. })
    ));
    assert_eq!(
        output.bytes, [None; 12],
        "NULL selection before any projection store"
    );
    graphics
        .publish_view(SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        })
        .unwrap();
    graphics.publish_shift(0).unwrap();
    let context = crate::model_context::ModelContext::new([0; 3], MenuBasis::INITIAL);
    graphics
        .publish_model(PublishedModelWorld {
            position: context.position,
            basis: context.basis,
            transform: context.publish(graphics.view().unwrap(), 0),
        })
        .unwrap();
    graphics
        .select_projection(ModelSurfaceTableOrigin::TemplateDefault, ModelDispatch::Far)
        .unwrap();
    projector
        .project(ModelVertexInput::Local([0, 0, 256]), &mut output)
        .unwrap();
    let retained_fog = output.bytes[9];
    assert!(retained_fog.is_some());
    graphics
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            ModelDispatch::Near,
        )
        .unwrap();
    projector
        .project(ModelVertexInput::Local([0, 0, 512]), &mut output)
        .unwrap();
    assert_eq!(output.read_u32(4), Some(512));
    assert_eq!(output.bytes[9], retained_fog);
}
#[test]
fn borrowed_process_is_a_hard_custody_error_and_does_not_write_projection() {
    let graphics = process();
    let _loan = graphics.0.borrow_mut();
    let mut projector = ProcessModelProjector {
        process: &graphics,
        table: ModelSurfaceTableOrigin::TemplateDefault,
    };
    let mut output = ModelVertexProjection::default();
    assert!(matches!(
        projector.project(ModelVertexInput::World([1, 2, 3]), &mut output),
        Err(ProcessModelGraphicsError::BorrowConflict)
    ));
    assert_eq!(output.bytes, [None; 12]);
}
