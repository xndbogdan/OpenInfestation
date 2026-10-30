use std::path::{Path, PathBuf};
use std::process::Command;

use inf_game::{
    Installation, ResidencyEvent, ResidencyRequest, ResidentRuntime, ResourceScope,
    ResourceSelection,
};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn leaves(events: &[ResidencyEvent]) -> Vec<(bool, u8)> {
    events
        .iter()
        .filter_map(|event| match event {
            ResidencyEvent::LeafActivated { level } => Some((true, *level)),
            ResidencyEvent::LeafReleased { level } => Some((false, *level)),
            _ => None,
        })
        .collect()
}

#[test]
fn native_startup_frontend_loading_and_world_transitions_match_authored_ownership() {
    let mut runtime = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    let boot = runtime.apply(ResidencyRequest::Boot).unwrap();
    assert_eq!(leaves(&boot.events), [(true, 0), (true, 2)]);
    assert_eq!(runtime.summary().active_leaves, [0, 2]);
    // The startup string bank survives the complete frontend/world roundtrip
    // at the same owned allocation, rather than being silently reread.
    let startup = runtime.resource(0, 0).unwrap().overlay.bytes().as_ptr();
    let cases = [
        (47, vec![(false, 2), (true, 47)], vec![0, 47]),
        (48, vec![(false, 47), (true, 1), (true, 48)], vec![0, 1, 48]),
        (
            11,
            vec![(false, 48), (true, 6), (true, 11)],
            vec![0, 1, 11, 6],
        ),
        (19, vec![(false, 11), (true, 19)], vec![0, 1, 19, 6]),
        (
            12,
            vec![(false, 19), (false, 6), (true, 9), (true, 12)],
            vec![0, 1, 12, 9],
        ),
        (
            47,
            vec![(false, 1), (false, 12), (false, 9), (true, 47)],
            vec![0, 47],
        ),
    ];
    for (level, expected, resident) in cases {
        let change = runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        assert_eq!(leaves(&change.events), expected, "request {level}");
        assert_eq!(runtime.summary().active_leaves, resident);
        assert_eq!(
            runtime.resource(0, 0).unwrap().overlay.bytes().as_ptr(),
            startup
        );
    }
    assert!(runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap()
        .events
        .is_empty());
}

#[test]
fn every_retail_leaf_can_be_requested_without_losing_slot_custody() {
    let installation = Installation::open(&data_root()).unwrap();
    let levels: Vec<_> = installation
        .preload()
        .leaves()
        .map(|leaf| leaf.overlay_id)
        .collect();
    let capacities = installation.preload().header.handler_capacities;
    let mut runtime = ResidentRuntime::new(installation).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in levels {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        let summary = runtime.summary();
        assert_eq!(summary.scope, ResourceScope::ResidentResources);
        assert!(summary.active_leaves.contains(&level));
        for (handler, capacity) in capacities.into_iter().enumerate() {
            let mut resolved = 0;
            for global in 0..capacity {
                if let Some(resource) = runtime.resource(handler as u8, global) {
                    resolved += 1;
                    assert!(summary.active_leaves.contains(&resource.registration.level));
                    assert_eq!(resource.overlay.key().level, resource.registration.level);
                    assert_eq!(
                        resource.overlay.key().request_kind(),
                        resource.registration.phase
                    );
                    assert_eq!(resource.registration.global_index, global);
                    assert_eq!(resource.registration.handler, handler as u8);
                    let region =
                        &resource.overlay.layout().regions[resource.registration.region_index];
                    assert_eq!(region.handler.get(), handler as u8);
                }
            }
            assert_eq!(
                resolved, summary.active_slots[handler],
                "every occupied slot owns resident bytes"
            );
        }
    }
}

#[test]
fn retail_selector_reloads_preserve_base_allocations_and_rollback_invalid_selection() {
    let mut runtime = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 11 })
        .unwrap();
    let base = runtime.resource(8, 0).unwrap().overlay.bytes().as_ptr();
    for difficulty in 0..5 {
        for language in 0..6 {
            let selection = ResourceSelection {
                difficulty,
                language,
            };
            runtime
                .apply(ResidencyRequest::Select { selection })
                .unwrap();
            assert_eq!(runtime.summary().selection, selection);
            assert_eq!(
                runtime.resource(8, 0).unwrap().overlay.bytes().as_ptr(),
                base
            );
        }
    }
    let previous = runtime.summary();
    assert!(runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 255,
                language: 255
            },
        })
        .is_err());
    assert_eq!(runtime.summary(), previous);
    assert_eq!(
        runtime.resource(8, 0).unwrap().overlay.bytes().as_ptr(),
        base
    );
}

#[test]
fn resident_cli_reports_ordered_boot_and_activation_without_ambiguous_scene_modes() {
    let output = Command::new(env!("CARGO_BIN_EXE_inf-game"))
        .arg("--data-root")
        .arg(data_root())
        .args(["--mode", "resident", "--activate", "47,48,11", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["scope"], "resident_resources");
    assert_eq!(
        report["summary"]["active_leaves"],
        serde_json::json!([0, 1, 11, 6])
    );
    assert_eq!(report["transitions"].as_array().unwrap().len(), 4);
    for args in [
        vec!["--activate", "47"],
        vec!["--mode", "resident", "--scene", "47"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_inf-game"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn revision_changes_only_with_load_or_release_events() {
    let mut runtime = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    assert_eq!(runtime.revision(), 0);
    runtime.apply(ResidencyRequest::Boot).unwrap();
    assert_eq!(runtime.revision(), 1);
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert_eq!(runtime.revision(), 2);
    let snapshot = runtime.clone();
    // The 9->2 frontend factory re-requests its already active leaf: no
    // events, so a renderer bound to a snapshot of this runtime stays valid.
    let repeat = runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert!(repeat.events.is_empty());
    assert_eq!(runtime.revision(), 2);
    let selection = runtime.summary().selection;
    assert!(runtime
        .apply(ResidencyRequest::Select { selection })
        .unwrap()
        .events
        .is_empty());
    assert_eq!(runtime.revision(), 2);
    // Slot records are cloned on every apply; allocator reuse says nothing
    // about resource changes. The existing snapshot must retain equivalent
    // provenance and the same immutable overlay allocation after no-op applies.
    let retained = snapshot.resource(8, 0).unwrap();
    let current = runtime.resource(8, 0).unwrap();
    assert_eq!(current.registration, retained.registration);
    assert!(std::sync::Arc::ptr_eq(current.overlay, retained.overlay));
    assert_eq!(snapshot.revision(), 2);
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: (selection.difficulty + 1) % 5,
                ..selection
            },
        })
        .unwrap();
    assert_eq!(runtime.revision(), 3);
    assert_eq!(snapshot.revision(), 2);
    assert!(runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 255,
                language: 255
            },
        })
        .is_err());
    assert_eq!(runtime.revision(), 3);
}
