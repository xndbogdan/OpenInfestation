use super::*;
use serde_json::Value;

fn basis(value: &Value) -> MenuBasis {
    MenuBasis {
        words: std::array::from_fn(|i| value[i].as_u64().unwrap() as u32),
    }
}

fn rotation(value: &Value) -> ModelAuxiliaryRotation {
    ModelAuxiliaryRotation {
        mode_raw: value["mode"].as_u64().unwrap() as u8,
        packed_angle: value["packed"].as_u64().unwrap() as u16,
        orientation_raw: value["orientation"].as_i64().unwrap() as i8,
    }
}

#[test]
fn combat_auxiliary_rotation_matches_native_modes_axes_and_stopped_value_prefixes() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/combat_auxiliary_rotation_native.json"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut normal = 0;
    let mut stopped = 0;
    for (index, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let owners = crate::model_process::ModelProcessOwners::native_startup(executable).unwrap();
        let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
        let scope = owners.scratch.begin_draw().unwrap();
        let mut run = Run {
            graphics: &owners.graphics,
            scratch: &owners.scratch,
            scope: scope.clone(),
            process: &mut process,
            basis_math: &owners.basis_math,
            residency: None,
            terrain: None,
            counterpart: None,
            ray: None,
        };
        let mut context = StackContext {
            origin: [1, 2, 3],
            basis: basis(&input["basis"]),
            table: ModelSurfaceTableOrigin::TemplateDefault,
            saved_basis: basis(&input["saved"]),
        };
        let first = rotation(&input["first"]);
        let second = rotation(&input["second"]);
        let result = run.rotate_saved(
            &mut Working::Stack(&mut context),
            &first,
            (input["second_present"] != 0).then_some(&second),
            0x5f89e,
            AuxiliaryValueSource::Required,
        );
        let unsupported = case["events"].as_array().unwrap().iter().find(|event| {
            event["kind"] == "rotate" && event["packed"].as_u64().unwrap() & 0x80 != 0
        });
        if let Some(event) = unsupported {
            assert!(
                matches!(
                    result,
                    Err(AuxiliaryModelError::RequiredPackedValue {
                        raw: 0xabc0,
                        address: 0x5f89e
                    })
                ),
                "case{index}: {result:?}"
            );
            assert_eq!(
                context.saved_basis,
                basis(&event["saved_before"]),
                "case{index}"
            );
            stopped += 1;
        } else {
            result.unwrap();
            assert_eq!(context.saved_basis, basis(&case["saved"]), "case{index}");
            normal += 1;
        }
        assert_eq!(context.basis, basis(&case["basis"]), "case{index}");
        owners
            .scratch
            .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
            .unwrap();
    }
    assert_eq!((normal, stopped), (25, 8));
}
