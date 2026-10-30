use std::path::{Path, PathBuf};
use std::process::Command;

use inf_formats::{OverlayVariant, RequestKind};
use inf_game::{Installation, ResourceScope, Runtime, SceneContext};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[test]
fn every_retail_scene_prepares_in_all_difficulty_language_contexts() {
    let installation = Installation::open(&data_root()).expect("retail data is required");
    let levels: Vec<_> = installation
        .preload()
        .leaves()
        .map(|leaf| leaf.overlay_id)
        .collect();
    assert_eq!(levels.len(), 46);
    for level in levels {
        for difficulty in 0..5 {
            for language in 0..6 {
                let context = SceneContext::new(level, difficulty, language);
                let scene = installation
                    .prepare_scene(context)
                    .unwrap_or_else(|error| panic!("{context:?}: {error}"));
                let summary = scene.summary();
                assert_eq!(summary.scope, ResourceScope::IndependentScene);
                let requests = context.request_tables(installation.preload()).unwrap();
                let expected_phases: Vec<_> = context
                    .overlay_keys()
                    .into_iter()
                    .zip(requests)
                    .filter(|(_, request)| request.has_active_handlers())
                    .map(|(key, _)| key.request_kind())
                    .collect();
                assert_eq!(
                    scene
                        .overlays()
                        .iter()
                        .map(|overlay| overlay.key().request_kind())
                        .collect::<Vec<_>>(),
                    expected_phases,
                    "{context:?}"
                );
                for registration in scene.registrations() {
                    let winner = scene
                        .resource(registration.handler, registration.global_index)
                        .expect("every registered slot has a winner");
                    assert!(winner.overlay_index >= registration.overlay_index);
                    assert_eq!(
                        scene.overlays()[registration.overlay_index].key().level,
                        level
                    );
                }
                for handler in &summary.handlers {
                    assert!(handler.active_slots <= handler.capacity, "{context:?}");
                    assert!(handler.active_slots <= handler.registrations, "{context:?}");
                }
            }
        }
    }
}

#[test]
fn retail_signed_mapping_and_independent_scene_replacement_retain_exact_ownership() {
    let installation = Installation::open(&data_root()).unwrap();
    let mut runtime = Runtime::new(installation);
    let common = runtime.load_scene(SceneContext::new(1, 0, 0)).unwrap();
    // The real sound mapping uses a signed delta: local 38 maps to global 19.
    // The independently prepared scene does not invent a prior registration
    // from a retained parent bank or confuse the local and global namespaces.
    let sound = common.resource(9, 19).unwrap();
    assert_eq!(sound.local_index, 38);
    assert_eq!(sound.replaces, None);
    assert_eq!(
        common.overlays()[sound.overlay_index].key().variant,
        OverlayVariant::Base
    );
    let receipt = common.summary();
    assert!(runtime.load_scene(SceneContext::new(44, 0, 0)).is_err());
    assert_eq!(runtime.scene().unwrap().summary(), receipt);

    // Retail leaf 48 has no requests. Independent-scene replacement must not
    // accidentally carry the previous scene's models/sounds into this receipt.
    let empty = runtime.load_scene(SceneContext::new(48, 0, 0)).unwrap();
    assert!(empty.overlays().is_empty());
    assert!(empty.registrations().is_empty());
    assert!(empty.resource(9, 19).is_none());
}

#[test]
fn mission_bank_keeps_opaque_resources_and_does_not_claim_parent_assets() {
    let installation = Installation::open(&data_root()).unwrap();
    let scene = installation
        .prepare_scene(SceneContext::new(11, 0, 0))
        .unwrap();
    let summary = scene.summary();
    assert_eq!(summary.handlers[12].registrations, 1);
    assert_eq!(summary.handlers[14].registrations, 1);
    assert!(summary.handlers[14].opaque);
    assert!(scene
        .overlays()
        .iter()
        .all(|overlay| overlay.key().level == 11));
    assert_eq!(scene.overlays()[0].key().request_kind(), RequestKind::Base);
}

#[test]
fn bootstrap_cli_emits_a_machine_readable_receipt_and_rejects_unknown_scenes() {
    let output = Command::new(env!("CARGO_BIN_EXE_inf-game"))
        .arg("--data-root")
        .arg(data_root())
        .args(["--mode", "independent", "--scene", "47", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["context"]["level"], 47);
    assert_eq!(receipt["scope"], "independent_scene");
    assert!(!receipt["overlays"].as_array().unwrap().is_empty());

    let output = Command::new(env!("CARGO_BIN_EXE_inf-game"))
        .arg("--data-root")
        .arg(data_root())
        .args(["--mode", "independent", "--scene", "44", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("44"));
}
