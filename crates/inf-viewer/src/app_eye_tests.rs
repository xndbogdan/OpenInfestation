use super::*;

#[test]
fn explicit_eye_cli_selects_a_separate_tier_and_bounds_its_shift() {
    let args = Args::try_parse_from([
        "inf-viewer",
        "--model",
        "zonec",
        "--inspection-eye=-128,256,-1024",
        "--eye-shift",
        "17",
    ])
    .unwrap();
    let state = AppState::new(&AssetCatalog::default(), &args);
    assert_eq!(state.model_presentation, ModelPresentation::ExplicitEye);
    assert_eq!(
        state.inspection_eye,
        Some(InspectionEye([-128, 256, -1024]))
    );
    assert_eq!(state.eye_shift, 17);
    for args in [
        vec!["inf-viewer", "--inspection-eye=1,2"],
        vec!["inf-viewer", "--eye-shift", "32"],
        vec!["inf-viewer", "--inspection-eye=1,2,3", "--capture", "fake"],
    ] {
        assert!(Args::try_parse_from(args).is_err());
    }
}

#[test]
fn editor_applies_only_valid_input_and_keeps_eye_independent_of_orbit() {
    let catalog = AssetCatalog::default();
    let args = Args::try_parse_from(["inf-viewer", "--presentation", "explicit-eye"]).unwrap();
    let mut state = AppState::new(&catalog, &args);
    assert_eq!(state.inspection_eye, None);
    state.apply_action(
        UiAction::AdjustInspectionEye {
            axis: 2,
            delta: 1024,
        },
        &catalog,
    );
    assert_eq!(
        state.inspection_eye, None,
        "adjustment must not invent a starting eye"
    );
    state.apply_action(UiAction::FocusInspectionEye, &catalog);
    state.inspection_eye_text = "1,2".to_owned();
    state.handle_text_input_key(Keycode::Return, &catalog);
    assert_eq!(state.text_input_focus, TextInputFocus::InspectionEye);
    assert_eq!(state.inspection_eye, None);
    state.inspection_eye_text = "-128,256,0".to_owned();
    state.handle_text_input_key(Keycode::Return, &catalog);
    assert_eq!(state.text_input_focus, TextInputFocus::None);
    assert_eq!(state.inspection_eye, Some(InspectionEye([-128, 256, 0])));

    state.camera.orbit(44.0, -22.0);
    state.camera.zoom_by_wheel(2);
    state.camera.pan(30.0, -40.0);
    assert_eq!(state.inspection_eye, Some(InspectionEye([-128, 256, 0])));
    let orbit = (
        state.camera.yaw,
        state.camera.pitch,
        state.camera.zoom,
        state.camera.pan_x,
        state.camera.pan_y,
    );
    state.apply_action(
        UiAction::AdjustInspectionEye {
            axis: 2,
            delta: 1024,
        },
        &catalog,
    );
    assert_eq!(state.inspection_eye, Some(InspectionEye([-128, 256, 1024])));
    assert_eq!(
        orbit,
        (
            state.camera.yaw,
            state.camera.pitch,
            state.camera.zoom,
            state.camera.pan_x,
            state.camera.pan_y
        )
    );
    state.apply_action(UiAction::FocusInspectionEye, &catalog);
    state.inspection_eye_text = "0,0,0".to_owned();
    state.handle_text_input_key(Keycode::Escape, &catalog);
    assert_eq!(state.inspection_eye, Some(InspectionEye([-128, 256, 1024])));

    state.eye_shift = 31;
    state.apply_action(UiAction::CycleInspectionShift, &catalog);
    assert_eq!(state.eye_shift, 0);
    state.model_presentation = ModelPresentation::LocalShell;
    state.apply_action(
        UiAction::AdjustInspectionEye {
            axis: 0,
            delta: 1024,
        },
        &catalog,
    );
    assert_eq!(state.inspection_eye, Some(InspectionEye([-128, 256, 1024])));
}
