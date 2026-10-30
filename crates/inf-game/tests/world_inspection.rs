use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

use inf_formats::{ConditionalInstanceWord, InstanceCommandPayload, InstanceCommandTarget};
use inf_game::world_inspection::{
    inspect_world_assets, ConstructorReference, ConstructorSoundReference, InitialModelReference,
    ResidentAssetSource, WorldInspectionError,
};
use inf_game::{Installation, ResidencyRequest, ResidentRuntime, ResourceSelection};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn runtime() -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 0,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
}

fn assert_resident_source(runtime: &ResidentRuntime, source: &ResidentAssetSource) {
    let registration = &source.registration;
    let current = runtime
        .resource(registration.handler, registration.global_index)
        .unwrap();
    assert_eq!(current.registration, registration);
    assert_eq!(current.overlay.key(), &source.overlay);
    assert_eq!(current.overlay.path(), source.ovl);
}

#[test]
fn all_world_placements_resolve_through_current_residency_with_two_visible_model_holes() {
    let mut runtime = runtime();
    let mut placements = 0;
    let mut models = 0;
    let mut no_binding = 0;
    let mut command_count = 0;
    let mut bit_gates = BTreeMap::new();
    let mut sound_counts = BTreeMap::new();
    let mut sound_kinds = BTreeMap::new();
    let mut duplicate_winners = [0; 2];
    let mut missing = BTreeSet::new();
    for level in 11..=43 {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        let before = runtime.summary();
        let snapshot = inspect_world_assets(&runtime, 0).unwrap();
        assert_eq!(
            runtime.summary(),
            before,
            "inspection cannot alter residency"
        );
        assert_eq!(snapshot.active_leaves, before.active_leaves);
        assert_eq!(snapshot.source.overlay.level, level);
        assert_resident_source(&runtime, &snapshot.source);
        assert_eq!(
            snapshot.world.placements.len(),
            snapshot.placement_links.len()
        );
        for (placement, links) in snapshot
            .world
            .placements
            .iter()
            .zip(&snapshot.placement_links)
        {
            assert_eq!(placement.resource_argument >> 16, 0xffff);
            assert_eq!(placement.index, links.placement_index);
            let ConstructorReference::Resolved(constructor) = &links.constructor else {
                panic!(
                    "retail H13 missing: level {level}, placement {}",
                    placement.index
                );
            };
            let commands = &snapshot.instance_assets.placements[placement.index as usize].commands;
            let views = links.command_views.as_ref().unwrap();
            assert_eq!(views.len(), commands.len());
            for (command, view) in commands.iter().zip(views) {
                assert_eq!(command.code, view.code);
                assert_eq!(
                    view.target,
                    InstanceCommandTarget::Constructor(constructor.resource.constructor_selector)
                );
                if let InstanceCommandPayload::ActorDisableUnlessBit(word)
                | InstanceCommandPayload::ActorDisableIfBit(word) = &view.payload
                {
                    let ConditionalInstanceWord::Available(field) = word else {
                        panic!("retail bit operand unavailable")
                    };
                    let h14 = runtime.resource(14, 0).unwrap();
                    assert_eq!(
                        field.value,
                        u16::from_le_bytes(
                            field
                                .span
                                .get(h14.overlay.bytes())
                                .unwrap()
                                .try_into()
                                .unwrap()
                        )
                    );
                    *bit_gates
                        .entry((view.code.value, field.value))
                        .or_insert(0usize) += 1;
                }
            }
            command_count += commands.len();
            if (level, placement.index) == (20, 102) {
                assert_eq!(
                    views.iter().map(|v| v.code.value).collect::<Vec<_>>(),
                    [2, 18, 19]
                );
                let InstanceCommandPayload::ModelSlot(model) = &views[0].payload else {
                    panic!("model input")
                };
                assert_eq!(model.signed_slot, 577);
                assert!(runtime.resource(8, 577).is_some());
                assert!(
                    matches!(&views[1].payload, InstanceCommandPayload::ActorNegatedDword144(field) if field.value == 1)
                );
                let InstanceCommandPayload::PlacementSpawnList(list) = &views[2].payload else {
                    panic!("spawn list")
                };
                assert!(!list.rows.is_empty());
                // Authored commands are not folded into a claim about final live state.
                assert!(matches!(
                    constructor.initial_model,
                    InitialModelReference::NotResident { global_slot: 390 }
                ));
            }
            if (level, placement.index) == (13, 28) {
                assert_eq!(
                    views.iter().map(|v| v.code.value).collect::<Vec<_>>(),
                    [17, 16]
                );
                assert!(
                    matches!(&views[0].payload, InstanceCommandPayload::Selector69Dword144(field) if field.value == 8)
                );
                assert!(
                    matches!(&views[1].payload, InstanceCommandPayload::Selector69DoubledDword14c(field) if field.value == 700)
                );
            }
            assert_resident_source(&runtime, &constructor.source);
            assert_eq!(
                constructor.source.registration.global_index,
                placement.constructor_slot() as u16
            );
            assert_eq!(
                constructor.source.registration.local_index as u32,
                constructor.resource.local_index
            );
            let h13 = runtime
                .resource(13, constructor.source.registration.global_index)
                .unwrap();
            let prefix = constructor
                .resource
                .prefix_span
                .get(h13.overlay.bytes())
                .unwrap();
            assert_eq!(
                u16::from_le_bytes(prefix[..2].try_into().unwrap()),
                constructor.resource.constructor_tag
            );
            for sound in &constructor.sound_bindings {
                assert_eq!(
                    sound.binding.raw.value,
                    u16::from_le_bytes(
                        sound
                            .binding
                            .raw
                            .span
                            .get(h13.overlay.bytes())
                            .unwrap()
                            .try_into()
                            .unwrap()
                    )
                );
                let status = match &sound.resource {
                    ConstructorSoundReference::DisabledZero => "zero",
                    ConstructorSoundReference::NegativeArgument { .. } => "negative",
                    ConstructorSoundReference::NotResident { .. } => "missing",
                    ConstructorSoundReference::Resolved(sound) => {
                        assert_resident_source(&runtime, &sound.source);
                        let h9 = runtime
                            .resource(9, sound.source.registration.global_index)
                            .unwrap();
                        assert!(sound.entry.record.get(h9.overlay.bytes()).is_some());
                        assert_eq!(
                            sound.entry.local_index,
                            u32::from(sound.source.registration.local_index)
                        );
                        if let Some(pcm) = sound.entry.kind.pcm_span() {
                            assert!(pcm.get(h9.overlay.bytes()).is_some());
                        }
                        *sound_kinds.entry(sound.entry.raw_type).or_insert(0usize) += 1;
                        "resolved"
                    }
                };
                *sound_counts.entry(status).or_insert(0usize) += 1;
            }
            match &constructor.initial_model {
                InitialModelReference::Resolved(model) => {
                    assert_resident_source(&runtime, &model.source);
                    assert_eq!(
                        model.source.registration.global_index as i16,
                        constructor.asset.model_binding.unwrap().signed_slot
                    );
                    let h8 = runtime
                        .resource(8, model.source.registration.global_index)
                        .unwrap();
                    assert!(model.content_span.get(h8.overlay.bytes()).is_some());
                    models += 1;
                    match (
                        model.source.overlay.level,
                        model.source.registration.global_index,
                    ) {
                        (1, 195) => {
                            assert_eq!(model.source.registration.local_index, 254);
                            duplicate_winners[0] += 1;
                        }
                        (21, 423) => {
                            assert_eq!(model.source.registration.local_index, 38);
                            duplicate_winners[1] += 1;
                        }
                        _ => {}
                    }
                }
                InitialModelReference::NotConsumed => no_binding += 1,
                InitialModelReference::NotResident { global_slot } => {
                    missing.insert((
                        level,
                        placement.index,
                        constructor.source.registration.global_index,
                        *global_slot,
                    ));
                }
                status => panic!("unexpected retail initial model status {status:?}"),
            }
            placements += 1;
        }
    }
    assert_eq!((placements, models, no_binding), (4486, 4355, 129));
    assert_eq!(command_count, 11476);
    assert_eq!(
        sound_counts,
        BTreeMap::from([("resolved", 1240), ("zero", 1756)])
    );
    assert_eq!(sound_kinds, BTreeMap::from([(1, 798), (5, 442)]));
    assert_eq!(
        bit_gates,
        BTreeMap::from([
            ((13, 23), 3),
            ((13, 33), 2),
            ((13, 48), 69),
            ((13, 53), 1),
            ((13, 66), 3),
            ((13, 132), 3),
            ((14, 23), 17),
            ((14, 33), 2),
            ((14, 48), 11),
            ((14, 49), 1),
            ((14, 53), 4),
            ((14, 63), 1),
            ((14, 66), 3),
        ])
    );
    assert_eq!(duplicate_winners, [10, 4]);
    assert_eq!(
        missing,
        BTreeSet::from([(13, 28, 5, 480), (20, 102, 45, 390)])
    );
}

#[test]
fn world_inspection_observes_unloads_reloads_and_selected_resource_context() {
    let mut runtime = runtime();
    assert!(matches!(
        inspect_world_assets(&runtime, 0),
        Err(WorldInspectionError::WorldNotResident { global_slot: 0 })
    ));
    runtime
        .apply(ResidencyRequest::Activate { level: 11 })
        .unwrap();
    let initial = inspect_world_assets(&runtime, 0).unwrap();
    for level in [29, 30, 32, 47] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        if level == 47 {
            assert!(matches!(
                inspect_world_assets(&runtime, 0),
                Err(WorldInspectionError::WorldNotResident { .. })
            ));
        } else {
            let snapshot = inspect_world_assets(&runtime, 0).unwrap();
            assert_eq!(snapshot.source.overlay.level, level);
            for link in snapshot.placement_links {
                if let ConstructorReference::Resolved(constructor) = link.constructor {
                    assert_resident_source(&runtime, &constructor.source);
                }
            }
        }
    }
    runtime
        .apply(ResidencyRequest::Activate { level: 11 })
        .unwrap();
    assert_eq!(inspect_world_assets(&runtime, 0).unwrap(), initial);
    let selection = ResourceSelection {
        difficulty: 4,
        language: 5,
    };
    runtime
        .apply(ResidencyRequest::Select { selection })
        .unwrap();
    let changed = inspect_world_assets(&runtime, 0).unwrap();
    assert_eq!(changed.selection, selection);
    // H8/H13/H14 registrations are base-only here; selector-dependent assets
    // still reload through the actual runtime, rather than a snapshot cache.
    assert_eq!(changed.world, initial.world);
    assert_eq!(changed.placement_links, initial.placement_links);
}

#[test]
fn cli_exposes_resident_world_links_and_rejects_isolated_inspection() {
    let output = Command::new(env!("CARGO_BIN_EXE_inf-game"))
        .arg("--data-root")
        .arg(root())
        .args([
            "--mode",
            "resident",
            "--activate",
            "11",
            "--inspect-world",
            "0",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["world_assets"]["schema_version"], 5);
    assert_eq!(json["world_assets"]["source"]["overlay"]["level"], 11);
    assert_eq!(
        json["world_assets"]["world"]["placements"]
            .as_array()
            .unwrap()
            .len(),
        json["world_assets"]["placement_links"]
            .as_array()
            .unwrap()
            .len()
    );
    assert!(json["transitions"].as_array().unwrap().len() >= 2);
    let invalid = Command::new(env!("CARGO_BIN_EXE_inf-game"))
        .args(["--inspect-world", "0"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("frontend mode does not accept"));
}
