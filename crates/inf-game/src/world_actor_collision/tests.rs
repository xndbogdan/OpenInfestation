use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_scene::{ModelSurfaceTableOrigin, SceneView},
    menu::MenuBasis,
    model_auxiliary::{AuxiliaryModelSource, AuxiliaryPairRequest},
    model_context::ModelDispatch,
    model_scratch::ModelScratchDrawState,
    Installation, ResidencyRequest,
};
fn fixture() -> (
    ResidentRuntime,
    ModelProcessOwners,
    ProcessFurnitureQueryState,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let owners = ModelProcessOwners::native_startup(exe).unwrap();
    owners
        .graphics
        .publish_view(SceneView {
            eye: [0, 0, -1024],
            basis: MenuBasis::INITIAL,
        })
        .unwrap();
    let mut query = ProcessFurnitureQueryState::native_startup(exe).unwrap();
    query.initialize_query(LiteralRetExitRegistrationApproximation);
    query.publish_pair_basis(MenuBasis::INITIAL);
    (runtime, owners, query)
}
fn query(
    runtime: &ResidentRuntime,
    owners: &ModelProcessOwners,
    process: &mut ProcessFurnitureQueryState,
    contact: &mut CollisionContact,
) -> Result<(), crate::model_auxiliary::AuxiliaryModelError> {
    let actor = resident_model_resource(runtime, 223).unwrap().unwrap();
    let counterpart = resident_model_resource(runtime, 51).unwrap().unwrap();
    let qualify = |r: &ResidentModelBinding<'_>| AuxiliaryRawSource {
        registration: r.registration.clone(),
        allocation: Arc::downgrade(r.allocation),
        vertices_span: r.model.vertices_span,
    };
    let a = qualify(&actor);
    let b = qualify(&counterpart);
    query_auxiliary_pair(
        AuxiliaryPairRequest {
            residency: Some(runtime),
            actor_origin: [0; 3],
            actor_basis: MenuBasis::INITIAL,
            actor_model: AuxiliaryModelSource {
                model: actor.model,
                qualification: &a,
            },
            counterpart: AuxiliaryModelSource {
                model: counterpart.model,
                qualification: &b,
            },
        },
        process,
        owners,
        contact,
    )
}
#[test]
fn real_null_projection_feature_retires_marks_without_contact_success_or_prefix_reset() {
    let (runtime, owners, mut process) = fixture();
    let mut contact = CollisionContact::unwritten();
    let result = query(&runtime, &owners, &mut process, &mut contact);
    assert!(matches!(&result,Err(crate::model_auxiliary::AuxiliaryModelError::VertexExecution(
        crate::model_vertices::ModelVertexError::Projection(
            crate::process_model_graphics::ProcessModelGraphicsError::NativeNullProjection { .. })))));
    let bytes = owners.scratch.read_pool::<24>(0).unwrap();
    let world = owners.graphics.world().unwrap();
    assert!(matches!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    let error = finish_auxiliary_contact(result, &owners, contact).unwrap_err();
    assert!(matches!(
        error,
        ResidentFurnitureCollisionError::Auxiliary(_)
    ));
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
    assert_eq!(owners.scratch.read_pool::<24>(0).unwrap(), bytes);
    assert_eq!(owners.graphics.world().unwrap(), world);
    assert_eq!(contact.flag(), Some(0));
}
#[test]
fn real_preexisting_draw_custody_error_is_not_feature_retired_or_returned_as_contact() {
    let (runtime, owners, mut process) = fixture();
    let scope = owners.scratch.begin_draw().unwrap();
    let mut contact = CollisionContact::from_native_words([Some(9); 5]);
    let result = query(&runtime, &owners, &mut process, &mut contact);
    assert!(matches!(
        finish_auxiliary_contact(result, &owners, contact),
        Err(ResidentFurnitureCollisionError::Auxiliary(
            crate::model_auxiliary::AuxiliaryModelError::Scratch(_)
        ))
    ));
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Running(scope.clone())
    );
    assert_eq!(contact.native_words(), [Some(9); 5]);
    owners
        .scratch
        .finish_draw(
            &scope,
            crate::model_scratch::ModelScratchDrawExit::NormalReturn,
        )
        .unwrap();
}
#[test]
fn genuine_normal_return_has_already_retired_marks_and_adapter_retains_actual_contact() {
    let (runtime, owners, mut process) = fixture();
    owners
        .graphics
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            ModelDispatch::Near,
        )
        .unwrap();
    let mut contact = CollisionContact::unwritten();
    let result = query(&runtime, &owners, &mut process, &mut contact);
    assert!(result.is_ok());
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
    assert_eq!(
        finish_auxiliary_contact(result, &owners, contact).unwrap(),
        contact
    );
    assert_eq!(contact.displacement(), Some(32));
    assert!(process.auxiliary_allocation().is_some());
}
