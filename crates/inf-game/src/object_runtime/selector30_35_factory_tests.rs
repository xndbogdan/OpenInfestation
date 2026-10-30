//! Retail H13/H14 construction with real command walking, completion and custody.

use super::{
    actor_factory_tests::{arena, inputs, math},
    tests::{constructed, lists, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::{
    object_runtime::*,
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    world_start::{WorldStartContext, WorldStartState},
};
use inf_formats::{InstanceCommand, InstanceField};

#[derive(Default)]
struct ObserveListeners {
    seen: Vec<(ObjectId, ActorHandle)>,
    reject: bool,
}

impl CreationListenerEffects for ObserveListeners {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        let base = &arena.object(object)?.base;
        assert_eq!(base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(base.dirty_word, 0);
        if arena.selector35_actor(object).is_ok() {
            assert_eq!(arena.actor(object)?.word_74, base.word_28 >> 3);
        }
        self.seen.push((object, arena.actor(object)?.handle));
        if self.reject {
            listeners.register(object, CreationListenerInterface::Selector78);
            Err(CreationListenerError::UnsupportedInterface(
                listener.interface,
            ))
        } else {
            Ok(())
        }
    }
}

/// Authored count and relocated command pointers, preserving one owner for
/// retained operands. This is not an inspector-only list of decoded commands.
struct OwnedCommands {
    allocation: Arc<InstanceCommandAllocation>,
    placement: PlacementInstanceAssets,
}

impl OwnedCommands {
    fn new(inputs: &[(i32, &[u8])]) -> Self {
        let mut bytes = vec![0; 4 + inputs.len() * 4];
        bytes[..4].copy_from_slice(&(inputs.len() as i32).to_le_bytes());
        let mut relocations = vec![];
        let mut commands = vec![];
        for (index, (code, operand)) in inputs.iter().enumerate() {
            let offset = bytes.len();
            let field = 4 + index * 4;
            bytes[field..field + 4].copy_from_slice(&(offset as u32).to_le_bytes());
            bytes.extend_from_slice(&code.to_le_bytes());
            bytes.extend_from_slice(operand);
            relocations.push(AssetRelocation {
                table_span: ByteSpan::new(0, 0),
                field_offset: field as u32,
                field_span: ByteSpan::new(field, field + 4),
                target_offset: offset as u32,
            });
            commands.push(InstanceCommand {
                index: index as u32,
                pointer_span: ByteSpan::new(field, field + 4),
                allocation_offset: offset as u32,
                code: InstanceField {
                    span: ByteSpan::new(offset, offset + 4),
                    value: *code,
                },
                payload_offset: offset as u32 + 4,
            });
        }
        Self {
            allocation: Arc::new(
                InstanceCommandAllocation::new(
                    bytes.clone().into(),
                    ByteSpan::new(0, bytes.len()),
                    relocations.into(),
                )
                .unwrap(),
            ),
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: inputs.len() as i32,
                },
                command_pointer_span: ByteSpan::new(4, 4 + inputs.len() * 4),
                commands,
            },
        }
    }
}

#[test]
fn retail_selector35_placements_share_h13_custody_and_run_terrain_commands() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let allocation = Arc::new(
        InstanceCommandAllocation::new(
            Arc::from(placements.world_source().overlay.bytes()),
            placements.assets().worlds.main_payload,
            Arc::from(placements.assets().worlds.relocations.clone()),
        )
        .unwrap(),
    );
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = ObserveListeners::default();
    let mut rng = 0x0123_4567;
    let mut objects = vec![];
    for index in [80, 96, 97] {
        let placement = placements.input(index).unwrap();
        let mut actor = inputs(&math, &mut listeners, &mut effects);
        actor.rotation_words = Some(placement.rotation_words);
        actor.terrain = Some(placements.terrain());
        actor.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: placements.world().actor_mask_limits(),
            rng: &mut rng,
        });
        let object = constructed(construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: placement.resource_argument,
                context: BaseContext {
                    position_words: Some(placement.position_words),
                    placement_index: placement.placement_index,
                },
                suffix: Some(allocation.commands(placement.instance)),
                command_allocation: Some(&allocation),
                network_active: false,
                actor: Some(actor),
            },
        ));
        let family = arena.family428650_actor(object).unwrap();
        let expected_home = if index == 80 {
            [
                14000,
                50_u16.wrapping_add(placements.terrain().height_at([14000, 45036]) as u16),
                45036,
            ]
        } else {
            // Admission overwrites the payload's temporary Y+0400 home.
            placement.position_words
        };
        assert_eq!(family.position_222, Some(expected_home));
        assert_eq!(family.word_214, if index == 80 { 0 } else { 0x100 });
        let state = arena.selector35_actor(object).unwrap();
        assert_eq!(state.byte_251, Some(u8::from(index != 80)));
        assert_eq!(state.word_258, Some(if index == 80 { 1952 } else { 1334 }));
        assert_ne!(
            arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
            0
        );
        objects.push(object);
    }
    let retained: Vec<_> = objects
        .iter()
        .map(|&object| {
            arena
                .selector35_actor(object)
                .unwrap()
                .retained_23c
                .clone()
                .unwrap()
        })
        .collect();
    assert!(Arc::ptr_eq(
        retained[0].allocation(),
        retained[1].allocation()
    ));
    assert!(Arc::ptr_eq(
        retained[1].allocation(),
        retained[2].allocation()
    ));
    assert_ne!(retained[0].allocation_offset, retained[1].allocation_offset);
    assert_eq!(retained[1].allocation_offset, retained[2].allocation_offset);
    assert!(!Arc::ptr_eq(
        retained[1].allocation(),
        allocation.runtime_allocation()
    ));
    let source = retained[1].allocation().source().to_vec();
    for object in objects {
        let handle = arena.actor(object).unwrap().handle;
        arena
            .initialize_after_load(object, Default::default())
            .unwrap();
        assert_eq!(arena.family428650_actor(object).unwrap().word_20c, 0);
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    }
    assert!(arena.is_empty());
    assert!(effects.seen.is_empty());
    assert_eq!(retained[1].allocation().source().as_ref(), source);
}

#[test]
fn selector35_retained_h14_alias_is_fresh_through_post_listeners_network_and_after_load() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 35);
    let xyz: Vec<_> = [14000_u16, 0xfffe, 45036]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut ratio = [0; 18];
    ratio[..2].copy_from_slice(&2_u16.to_le_bytes());
    ratio[16..].copy_from_slice(&0x8000_u16.to_le_bytes());
    let commands = OwnedCommands::new(&[
        (21, &xyz),
        (22, &1_u32.to_le_bytes()),
        (23, &ratio),
        (24, &0xfe_dc_ba_98_u32.to_le_bytes()),
    ]);
    let math = math();
    let mut arena = arena();
    let owner = arena.allocate(ObjectClassKind::Base);
    let mut listeners = CreationListeners::new();
    listeners.register(owner, CreationListenerInterface::Selector18);
    let mut effects = ObserveListeners::default();
    let mut rng = 9;
    let mut objects = vec![];
    for placement_index in 0..2 {
        let mut actor = inputs(&math, &mut listeners, &mut effects);
        actor.terrain = Some(placements.terrain());
        actor.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: [1, 1],
            rng: &mut rng,
        });
        let object = constructed(construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                context: BaseContext {
                    position_words: Some([1, 2, 3]),
                    placement_index,
                },
                suffix: Some(commands.allocation.commands(&commands.placement)),
                command_allocation: Some(&commands.allocation),
                network_active: true,
                actor: Some(actor),
                ..request(resource)
            },
        ));
        let state = arena.selector35_actor(object).unwrap();
        let retained = state.retained_23c.as_ref().unwrap();
        assert!(Arc::ptr_eq(
            retained.allocation(),
            commands.allocation.runtime_allocation()
        ));
        assert_eq!(
            retained.allocation_offset,
            commands.placement.commands[2].payload_offset
        );
        assert_eq!(
            state.bytes_240,
            [Some(0x98), Some(0xba), Some(0xdc), Some(0)]
        );
        let family = arena.family428650_actor(object).unwrap();
        assert_eq!(family.word_214, 0x0100_0000);
        assert_eq!(
            family.position_222,
            Some([
                14000,
                0xfffe_u16.wrapping_add(placements.terrain().height_at([14000, 45036]) as u16),
                45036
            ])
        );
        // 0047E2E0 stores 8000, then disposal's membership changes add bit 1.
        assert_eq!(arena.object(object).unwrap().base.dirty_word, 0x8002);
        assert!(arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == object));
        objects.push(object);
    }
    assert_eq!(effects.seen.len(), 2);
    let first_handle = arena.actor(objects[0]).unwrap().handle;
    arena
        .initialize_after_load(objects[1], Default::default())
        .unwrap();
    assert_eq!(
        arena.family428650_actor(objects[1]).unwrap().word_20c,
        first_handle.raw()
    );
    // A second invocation interprets the resulting handle as a placement number.
    arena
        .initialize_after_load(objects[1], Default::default())
        .unwrap();
    assert_eq!(arena.family428650_actor(objects[1]).unwrap().word_20c, 0);
    let source = commands.allocation.source().to_vec();
    commands
        .allocation
        .runtime_allocation()
        .write_word(commands.placement.commands[2].payload_offset + 16, 3)
        .unwrap();
    for object in objects {
        arena.finish_selector35_initialization(object).unwrap();
        assert_eq!(arena.family428650_actor(object).unwrap().word_214, 1536);
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
    assert_eq!(commands.allocation.source().as_ref(), source);
    assert_eq!(arena.len(), 1);
}

#[test]
fn selector35_missing_terrain_or_retained_owner_fails_before_completion_and_cleans_up() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 35);
    let math = math();
    for code in [20, 23, 25] {
        let mut arena = arena();
        let before = lists(&arena);
        let commands = Commands::new(1, &[(code, &[0; 4])]);
        let mut listeners = CreationListeners::new();
        let mut effects = ObserveListeners::default();
        let mut rng = 9;
        let mut actor = inputs(&math, &mut listeners, &mut effects);
        actor.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: [1, 1],
            rng: &mut rng,
        });
        let result = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                suffix: Some(commands.request()),
                actor: Some(actor),
                ..request(resource)
            },
        );
        match code {
            20 => assert!(matches!(
                result,
                Err(ObjectConstructionError::ActorCommand(
                    ActorCommandError::MissingTerrain
                ))
            )),
            23 => assert!(matches!(
                result,
                Err(ObjectConstructionError::MissingSelector35CommandOwner)
            )),
            25 => assert_eq!(
                result.unwrap(),
                ObjectConstructionOutcome::Rejected {
                    command_index: 0,
                    code: 25
                }
            ),
            _ => unreachable!(),
        }
        assert!(arena.is_empty());
        assert!(arena.actor_list().is_empty());
        assert_eq!(lists(&arena), before);
        assert_eq!(
            arena
                .resolve_actor_handle(ActorHandle::from_raw(0x08ff_0001))
                .unwrap(),
            None
        );
        assert!(effects.seen.is_empty());
    }
}

#[test]
fn selector30_factory_uses_one_world_start_owner_in_construction_order_and_requires_completion() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 30);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = ObserveListeners::default();
    let mut world = WorldStartState {
        pair_index: u16::MAX,
        override_position: Some([1000, 999, 1000]),
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut objects = vec![];
    for (index, selection) in [255_u8, 0].into_iter().enumerate() {
        let world_context = WorldStartContext {
            state: &mut world,
            runtime: &runtime,
            terrain: placements.terrain(),
        };
        let position = world_context.resolve_position().unwrap();
        let commands = Commands::new(2, &[(25, &[selection]), (21, &1_u16.to_le_bytes())]);
        let mut actor = inputs(&math, &mut listeners, &mut effects);
        actor.world_start = Some(world_context);
        let object = constructed(construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                context: BaseContext {
                    position_words: Some(position),
                    placement_index: index as u32,
                },
                suffix: Some(commands.request()),
                actor: Some(actor),
                ..request(resource)
            },
        ));
        assert_eq!(
            world.override_position,
            Some([position[0], position[1], position[2].wrapping_sub(450)])
        );
        assert_eq!(world.vehicle_selection, 255);
        assert_eq!(arena.actor(object).unwrap().word_b4, 2);
        assert!(arena.selector30_actor(object).unwrap().word_188.is_some());
        assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
        objects.push(object);
    }
    let before = arena.object(objects[1]).unwrap().clone();
    assert_eq!(
        arena.initialize_after_load(objects[1], Default::default()),
        Err(ObjectError::MissingWorldCompletion(objects[1]))
    );
    assert_eq!(arena.object(objects[1]).unwrap(), &before);
    let initial = arena
        .selector30_actor(objects[1])
        .unwrap()
        .word_188
        .unwrap();
    arena
        .initialize_after_load(
            objects[1],
            ObjectAfterLoadInputs {
                world_completion: Some(&[u32::MAX; 4]),
            },
        )
        .unwrap();
    assert_eq!(
        arena.actor(objects[1]).unwrap().word_b4,
        if initial == 2 { 18 } else { 2 }
    );
    assert_eq!(
        arena.selector30_actor(objects[1]).unwrap().handle_184,
        arena.actor(objects[0]).unwrap().handle
    );
    assert_eq!(world.override_position.unwrap()[2], 100);
    for object in objects {
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
    assert!(arena.is_empty());
}

#[test]
fn selector30_factory_failure_frontiers_keep_world_publication_and_remove_actor_ownership() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 30);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = ObserveListeners::default();
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            actor: Some(inputs(&math, &mut listeners, &mut effects)),
            ..request(resource)
        },
    );
    assert!(matches!(
        result,
        Err(ObjectConstructionError::MissingWorldStart { selector: 30 })
    ));
    assert!(arena.is_empty());
    let mut world = WorldStartState {
        pair_index: 0,
        override_position: Some([1000, 999, 1000]),
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let before = world.clone();
    let reject = Commands::codes(2, &[33]);
    let mut actor = inputs(&math, &mut listeners, &mut effects);
    actor.world_start = Some(WorldStartContext {
        state: &mut world,
        runtime: &runtime,
        terrain: placements.terrain(),
    });
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            suffix: Some(reject.request()),
            actor: Some(actor),
            ..request(resource)
        },
    );
    assert_eq!(
        result.unwrap(),
        ObjectConstructionOutcome::Rejected {
            command_index: 0,
            code: 33
        }
    );
    assert_eq!(world, before);
    assert!(arena.is_empty());
    // A listener failure is later than the real postinitializer's world writes.
    let owner = arena.allocate(ObjectClassKind::Base);
    let listener = listeners.register(owner, CreationListenerInterface::Selector18);
    effects.reject = true;
    let world_context = WorldStartContext {
        state: &mut world,
        runtime: &runtime,
        terrain: placements.terrain(),
    };
    let position = world_context.resolve_position().unwrap();
    let commands = Commands::new(1, &[(25, &[255])]);
    let mut actor = inputs(&math, &mut listeners, &mut effects);
    actor.world_start = Some(world_context);
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            context: BaseContext {
                position_words: Some(position),
                placement_index: 5,
            },
            suffix: Some(commands.request()),
            actor: Some(actor),
            ..request(resource)
        },
    );
    assert!(matches!(
        result,
        Err(ObjectConstructionError::CreationListener(_))
    ));
    assert_eq!(
        world.override_position,
        Some([position[0], position[1], position[2].wrapping_sub(450)])
    );
    assert_eq!(world.vehicle_selection, 255);
    assert_eq!(effects.seen.len(), 1);
    let (failed, handle) = effects.seen[0];
    assert!(arena.object(failed).is_err());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(arena.len(), 1);
    assert!(arena.actor_list().is_empty());
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners.get(listener).unwrap().owner, owner);
}
