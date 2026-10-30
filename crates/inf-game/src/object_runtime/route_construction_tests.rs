use std::collections::HashSet;

use super::{
    tests::{constructed, lists, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::{
    object_runtime::{ObjectClassKind, INITIALIZED_FLAG},
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
};

#[test]
fn route_constructor_executes_raw_word_dword_and_operand_free_commands() {
    let runtime = retail_runtime();
    let slot = resource_with_selector(&runtime, 51);
    assert_eq!(slot, 30);
    let mut arena = ObjectArena::new();
    let mut pending = vec![];
    for reference in [0_u16, 1, 0x8000, 0xffff] {
        for mode in [0_u32, 1] {
            let reference_bytes = reference.to_le_bytes();
            let mode_bytes = mode.to_le_bytes();
            let commands = Commands::new(
                7,
                &[
                    (3, &reference_bytes),
                    (4, &mode_bytes),
                    (4, &1_u32.to_le_bytes()), // one does not clear an existing bit
                    (5, &[]),
                    (6, &[]),
                    (7, &[]),
                    (8, &[]), // last code has no payload bytes
                ],
            );
            let id = constructed(construct_world_object(
                &mut arena,
                &runtime,
                ObjectConstructionRequest {
                    resource_argument: 0xabcd_0000 | slot,
                    suffix: Some(commands.request()),
                    command_allocation: None,
                    network_active: true,
                    actor: None,
                    ..request(slot)
                },
            ));
            let route = arena.route_node(id).unwrap();
            assert_eq!(route.identity_word, 0x8000);
            assert_eq!(route.predecessor_word, reference.wrapping_sub(1));
            assert_eq!(route.flags, if mode == 0 { 0x13 } else { 0x12 });
            assert_eq!(route.distance_squared, 0);
            assert_eq!(route.group, None);
            let object = arena.object(id).unwrap();
            assert_eq!(object.base.resource_word, 30);
            assert_eq!(object.base.placement_word, 0x8000);
            assert_eq!(object.base.position_words, Some([0xfffe, 0x8000, 1]));
            assert_eq!(object.base.dirty_word, 0);
            assert_ne!(object.base.flags & INITIALIZED_FLAG, 0);
            pending.push(id);
            assert_eq!(arena.route_pending(), pending);
            assert!(arena.route_groups().is_empty());
        }
    }
}

#[test]
fn route_command_failure_unlinks_fourth_node_and_preserves_existing_memberships() {
    let runtime = retail_runtime();
    let slot = resource_with_selector(&runtime, 51);
    let mut arena = ObjectArena::new();
    let base = arena.allocate(ObjectClassKind::Base);
    let existing: Vec<_> = (0..3)
        .map(|index| {
            constructed(construct_world_object(
                &mut arena,
                &runtime,
                ObjectConstructionRequest {
                    context: BaseContext {
                        position_words: Some([1, 2, 3]),
                        placement_index: index,
                    },
                    ..request(slot)
                },
            ))
        })
        .collect();
    let before_common = lists(&arena);
    let before_pending = arena.route_pending().to_vec();
    let before_states: Vec<_> = std::iter::once(base)
        .chain(existing.iter().copied())
        .map(|id| (id, arena.object(id).unwrap().clone()))
        .collect();
    for mode in [2_u32, 256, 0x8000_0000, u32::MAX] {
        let mode_bytes = mode.to_le_bytes();
        let commands = Commands::new(3, &[(3, &[0, 0]), (4, &mode_bytes), (3, &[])]);
        assert_eq!(
            construct_world_object(
                &mut arena,
                &runtime,
                ObjectConstructionRequest {
                    suffix: Some(commands.request()),
                    ..request(slot)
                }
            )
            .unwrap(),
            ObjectConstructionOutcome::Rejected {
                command_index: 1,
                code: 4
            }
        );
        assert_eq!(lists(&arena), before_common);
        assert_eq!(arena.route_pending(), before_pending);
    }
    let missing = Commands::codes(2, &[6]);
    let truncated = Commands::new(2, &[(5, &[]), (3, &[0])]);
    for commands in [&missing, &truncated] {
        assert!(matches!(
            construct_world_object(
                &mut arena,
                &runtime,
                ObjectConstructionRequest {
                    suffix: Some(commands.request()),
                    ..request(slot)
                }
            ),
            Err(ObjectConstructionError::MissingCommands { .. }
                | ObjectConstructionError::Instance(_))
        ));
        assert_eq!(lists(&arena), before_common);
        assert_eq!(arena.route_pending(), before_pending);
    }
    let model_boundary = u16::try_from(runtime.resource_capacity(8).unwrap())
        .unwrap()
        .to_le_bytes();
    let commands = Commands::new(1, &[(2, &model_boundary)]);
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                suffix: Some(commands.request()),
                ..request(slot)
            }
        ),
        Err(ObjectConstructionError::Object(
            ObjectError::ModelSlotBoundary { .. }
        ))
    ));
    assert_eq!(arena.len(), before_states.len());
    assert_eq!(lists(&arena), before_common);
    assert_eq!(arena.route_pending(), before_pending);
    assert!(arena.route_groups().is_empty());
    for (id, state) in before_states {
        assert_eq!(arena.object(id).unwrap(), &state);
    }
}

#[test]
fn all_40_first_world_route_nodes_construct_and_finalize_into_nine_groups() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let slot = resource_with_selector(&runtime, 51);
    let h13 = runtime.resource(13, slot as u16).unwrap();
    let h13_before = h13.overlay.bytes().to_vec();
    let h14 = placements.world_source();
    let h14_before = h14.overlay.bytes().to_vec();
    let revision = runtime.revision();
    let mut arena = ObjectArena::new();
    let mut ids = vec![];
    for input in placements
        .inputs()
        .filter(|input| input.resource_argument as u16 == slot as u16)
    {
        let mut expected_reference = u16::MAX;
        let mut expected_flags = 0;
        for command in &input.instance.commands {
            let at =
                placements.assets().worlds.main_payload.start + command.payload_offset as usize;
            match command.code.value {
                3 => {
                    expected_reference =
                        u16::from_le_bytes(h14.overlay.bytes()[at..at + 2].try_into().unwrap())
                            .wrapping_sub(1)
                }
                5 => expected_flags |= 2,
                6 | 7 => {}
                8 => expected_flags |= 0x10,
                code => panic!("unexpected first-world route-node command {code}"),
            }
        }
        let id = constructed(construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: input.resource_argument,
                context: BaseContext {
                    position_words: Some(input.position_words),
                    placement_index: input.placement_index,
                },
                suffix: Some(ObjectInstanceCommands {
                    placement: input.instance,
                    source: h14.overlay.bytes(),
                    main_payload: placements.assets().worlds.main_payload,
                    relocations: &placements.assets().worlds.relocations,
                }),
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        ));
        let route = arena.route_node(id).unwrap();
        assert_eq!(route.identity_word, input.placement_index as u16);
        assert_eq!(route.predecessor_word, expected_reference);
        assert_eq!(route.flags, expected_flags);
        assert_eq!(route.group, None);
        assert_eq!(route.distance_squared, 0);
        let object = arena.object(id).unwrap();
        assert_eq!(object.base.position_words, Some(input.position_words));
        assert_eq!(object.base.placement_word, input.placement_index as u16);
        assert_eq!(object.base.resource_word, 30);
        assert_ne!(object.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(object.base.dirty_word, 0);
        ids.push(id);
    }
    assert_eq!(ids.len(), 40);
    assert_eq!(arena.route_pending(), ids);
    assert!(arena.route_groups().is_empty());
    let common_before = lists(&arena);
    arena.finalize_routes().unwrap();
    assert!(arena.route_pending().is_empty());
    assert_eq!(arena.route_groups().len(), 9);
    assert_eq!(arena.len(), 40);
    assert_eq!(lists(&arena), common_before);
    let finalized: HashSet<_> = arena
        .route_groups()
        .iter()
        .flat_map(|group| group.members.iter().copied())
        .collect();
    assert_eq!(finalized, ids.iter().copied().collect());
    for group in arena.route_groups() {
        assert!(!group.members.is_empty());
        let flags = group.flags.unwrap();
        for &id in &group.members {
            let route = arena.route_node(id).unwrap();
            assert_eq!(route.group, Some(group.id));
            assert_eq!(route.flags, flags);
            assert_eq!(
                arena.find_route_node(u32::from(route.identity_word) + 1),
                Some(id)
            );
        }
    }
    assert_eq!(runtime.revision(), revision);
    assert_eq!(h13.overlay.bytes(), h13_before);
    assert_eq!(h14.overlay.bytes(), h14_before);
    // This fixture constructs the route subset only. Other mission classes,
    // player attachment and phase-3 admission are separate requirements.
}
