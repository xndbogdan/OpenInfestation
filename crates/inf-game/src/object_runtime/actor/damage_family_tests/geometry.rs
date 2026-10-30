use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_scene::{ModelSurfaceTableOrigin, SceneSetup, SceneView},
    menu::MenuBasis,
    model_auxiliary::{
        query_auxiliary_direct, AuxiliaryDirectRequest, AuxiliaryModelSource, AuxiliaryQueryShape,
    },
    model_process::ModelProcessOwners,
    object_runtime::{resident_model_resource, CollisionContact},
    world_furniture_effect::{
        AuxiliaryRawSource, LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState,
    },
    world_projectile_collision::{ProjectileGeometryError, ProjectileGeometryOwners},
    Installation, ResidencyRequest, ResidentRuntime,
};
use std::sync::Arc;

fn new_owners() -> (ModelProcessOwners, ProcessFurnitureQueryState) {
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let owners = ModelProcessOwners::native_startup(exe).unwrap();
    owners.selected_renderer_constructed().unwrap();
    owners
        .graphics
        .publish_view(SceneView {
            eye: [0, 0, -1024],
            basis: MenuBasis::INITIAL,
        })
        .unwrap();
    owners.graphics.publish_shift(0).unwrap();
    owners
        .graphics
        .publish_setup(SceneSetup {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [128, 4096],
            fade_factor: 65536,
        })
        .unwrap();
    owners
        .graphics
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            crate::model_context::ModelDispatch::Near,
        )
        .unwrap();
    let mut process = ProcessFurnitureQueryState::native_startup(exe).unwrap();
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    (owners, process)
}

#[test]
fn combat_actor_geometry_uses_native_wrapped_gate_and_captured_query_arguments() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let animation = crate::frontend_camera::AnimationClock::default();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_actor_direct_query_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    let mut hits = 0;
    let mut rejected = 0;
    let mut missing = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let captured = &case["request"];
        let (mut arena, actor, _, _) = setup();
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(std::array::from_fn(|i| {
                word(&input["actor_position"][i]) as u16
            }));
        let pose = &mut arena.actor_mut(actor).unwrap().rotating;
        pose.visual.radius_word = word(&input["actor_radius"]) as u16;
        //Native H8 slot3 is supplied synthetic data. Bind its captured model
        //identity to original retained223 here; NULL captures stay missing.
        pose.visual.model_word = if captured.is_null() || word(&captured["model"]) != 0 {
            223
        } else {
            u16::MAX
        };
        pose.basis.words = std::array::from_fn(|i| word(&input["basis_words"][i]));
        let (owners, mut process) = new_owners();
        let actual = ProjectileGeometryOwners {
            residency: &runtime,
            model_fallback: None,
            model_process: &owners,
            furniture_query: &mut process,
        }
        .actor_contact(
            crate::object_runtime::ObjectModelCallbacks {
                arena: &arena,
                source: actor,
                animation: &animation,
                executable,
            },
            std::array::from_fn(|i| word(&input["query_position"][i]) as u16),
            word(&input["radius"]) as i32,
        );
        if captured.is_null() {
            assert!(!actual.unwrap(), "native broad rejection case{index}");
            rejected += 1;
            continue;
        }
        if word(&captured["model"]) == 0 {
            assert!(
                matches!(
                    actual,
                    Err(ProjectileGeometryError::MissingModel { slot: u16::MAX, .. })
                ),
                "case{index}: {actual:?}"
            );
            missing += 1;
            continue;
        }
        let model = resident_model_resource(&runtime, 223).unwrap().unwrap();
        let qualification = AuxiliaryRawSource {
            registration: model.registration.clone(),
            allocation: Arc::downgrade(model.allocation),
            vertices_span: model.model.vertices_span,
        };
        let (expected_owners, mut expected_process) = new_owners();
        let mut contact = CollisionContact::unwritten();
        query_auxiliary_direct(
            AuxiliaryDirectRequest {
                residency: Some(&runtime),
                values: crate::model_auxiliary::AuxiliaryValueSource::Required,
                model: AuxiliaryModelSource {
                    model: model.model,
                    qualification: &qualification,
                },
                basis: MenuBasis {
                    words: std::array::from_fn(|i| word(&captured["basis_words"][i])),
                },
                relative_center: std::array::from_fn(|i| {
                    word(&captured["relative_words"][i]) as i32
                }),
                shape: if word(&captured["table"]) == 0x544bf0 {
                    AuxiliaryQueryShape::Point
                } else {
                    AuxiliaryQueryShape::Sphere {
                        radius: word(&captured["radius"]) as i32,
                    }
                },
            },
            &mut expected_process,
            &expected_owners,
            &mut contact,
        )
        .unwrap();
        let actual = actual.unwrap();
        assert_eq!(
            actual,
            contact.flag() == Some(1),
            "native captured arguments case{index}"
        );
        // These retained-model probes independently ensure that the bridge
        // cannot pass by rejecting every captured query. The model program's
        // final 88 requires positive depth, so an exact tangent is rejected.
        if matches!(index, 12 | 14 | 15) {
            assert!(actual, "point, sphere and wrapped contact case{index}");
        } else if index == 13 {
            assert!(!actual, "zero-depth tangent is rejected at model return");
        }
        assert_eq!(
            owners.graphics.world().unwrap().basis.words,
            std::array::from_fn(|i| word(&captured["basis_words"][i])),
            "case{index}"
        );
        hits += usize::from(actual);
    }
    assert!(
        hits >= 3,
        "point, sphere and wrapped contacts must reach geometry"
    );
    assert!(rejected > 0);
    assert_eq!(missing, 2);
}
