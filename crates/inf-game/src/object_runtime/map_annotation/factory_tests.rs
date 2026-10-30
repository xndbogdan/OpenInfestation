use super::tests::{constructed, request, resource_with_selector, Commands};
use super::*;
use crate::{terrain_runtime::TerrainSpacing, world_placement::WorldPlacements, ResidencyRequest};
use inf_formats::parse_constructor_asset_bank;
use std::sync::Arc;

fn retail_annotations(level: u8) -> crate::ResidentRuntime {
    let mut runtime = super::tests::retail_runtime();
    runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    runtime
}

#[test]
fn authored_level1_and_radius_world_construct_annotations_from_live_canonical_commands() {
    for (level, expected_count, expected_labels, expected_radii) in [
        (28, 5, vec![1, 2, 3, 4, 2], 0),
        (26, 9, vec![2, 1, 3, 4], 5),
    ] {
        let runtime = retail_annotations(level);
        let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
        let original = placements.world_source().overlay.bytes().to_vec();
        let allocation = Arc::new(
            InstanceCommandAllocation::new(
                Arc::from(original.clone()),
                placements.assets().worlds.main_payload,
                placements.assets().worlds.relocations.clone().into(),
            )
            .unwrap(),
        );
        let mut arena = ObjectArena::new();
        let mut count = 0;
        for index in 0..placements.world().placements.len() {
            let input = placements.input(index).unwrap();
            let resource = runtime
                .resource(13, input.resource_argument as u16)
                .unwrap();
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
            if bank.prefixes.entries[resource.registration.local_index as usize]
                .constructor_selector
                != 87
            {
                continue;
            }
            let outcome = construct_world_object(
                &mut arena,
                &runtime,
                ObjectConstructionRequest {
                    resource_argument: input.resource_argument,
                    context: BaseContext {
                        position_words: Some(input.position_words),
                        placement_index: input.placement_index,
                    },
                    suffix: Some(allocation.commands(input.instance)),
                    command_allocation: Some(&allocation),
                    network_active: true,
                    actor: None,
                },
            )
            .unwrap();
            let ObjectConstructionOutcome::Constructed(object) = outcome else {
                panic!("authored annotation rejected");
            };
            assert_eq!(
                arena.object(object).unwrap().base.position_words,
                Some(input.position_words)
            );
            assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
            count += 1;
        }
        assert_eq!(count, expected_count);
        let labels: Vec<_> = arena
            .map_annotation_list()
            .iter()
            .map(|id| arena.map_annotation(*id).unwrap().label)
            .filter(|label| *label != 0)
            .collect();
        assert_eq!(labels, expected_labels);
        let circles: Vec<_> = arena
            .map_annotation_list()
            .iter()
            .map(|id| arena.map_annotation(*id).unwrap())
            .filter(|state| state.label == 0)
            .collect();
        assert_eq!(circles.len(), expected_radii);
        if expected_radii != 0 {
            assert_eq!(
                circles
                    .iter()
                    .map(|state| (state.radius, state.rgb))
                    .collect::<Vec<_>>(),
                vec![
                    (10000, [0, 60, 0]),
                    (10000, [0, 30, 60]),
                    (8000, [60, 60, 0]),
                    (10000, [30, 0, 50]),
                    (10000, [60, 0, 0])
                ]
            );
        }
        assert_eq!(allocation.source(), original);
        assert_eq!(allocation.runtime_allocation().source_snapshot(), original);
        let ids = arena.map_annotation_list().to_vec();
        for object in ids {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        assert!(arena.map_annotation_list().is_empty());
    }
}

#[test]
fn rejected_or_malformed_construction_cleans_up_both_annotation_and_common_memberships() {
    let runtime = retail_annotations(28);
    let slot = resource_with_selector(&runtime, 87);
    let mut arena = ObjectArena::new();
    let existing = constructed(construct_world_object(&mut arena, &runtime, request(slot)));
    let before: Vec<_> = (0..6).map(|index| arena.lists[index].clone()).collect();
    let rejected = Commands::new(2, &[(2, &[0xff, 0xff]), (7, &[])]);
    let mut inputs = request(slot);
    inputs.suffix = Some(rejected.request());
    assert!(matches!(
        construct_world_object(&mut arena, &runtime, inputs).unwrap(),
        ObjectConstructionOutcome::Rejected {
            command_index: 1,
            code: 7
        }
    ));
    assert_eq!(arena.map_annotation_list(), &[existing]);
    assert_eq!(arena.lists.to_vec(), before);
    let malformed = Commands::new(1, &[(2, &[0xff])]);
    let mut inputs = request(slot);
    inputs.suffix = Some(malformed.request());
    assert!(construct_world_object(&mut arena, &runtime, inputs).is_err());
    assert_eq!(arena.map_annotation_list(), &[existing]);
    assert_eq!(arena.lists.to_vec(), before);
    assert_eq!(arena.len(), 1);
}
