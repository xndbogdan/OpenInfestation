//! Selector-34 production construction, retained H13/H14 custody and failure cleanup.

use super::{
    actor_factory_tests::{arena, inputs, math},
    tests::{constructed, lists, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::{object_runtime::*, terrain_runtime::TerrainSpacing, world_placement::WorldPlacements};
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
        assert_eq!(arena.actor(object)?.word_74, base.word_28 >> 3);
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

/// An authored count/pointer walk with a real allocation owner for code23.
pub(super) struct OwnedCommands {
    pub(super) allocation: Arc<InstanceCommandAllocation>,
    pub(super) placement: PlacementInstanceAssets,
}

impl OwnedCommands {
    pub(super) fn new(inputs: &[(i32, &[u8])]) -> Self {
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
fn retail_selector34_placements_run_the_factory_and_share_resident_h13_custody() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let selected: Vec<_> = placements
        .inputs()
        .filter(|placement| {
            let resource = runtime
                .resource(13, placement.resource_argument as u16)
                .unwrap();
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
            bank.prefixes.entries[usize::from(resource.registration.local_index)]
                .constructor_selector
                == 34
        })
        .collect();
    assert_eq!(selected.len(), 2);
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
    let mut retained = vec![];
    for placement in selected {
        // Admission copies placement XYZ before the authored suffix runs.
        // Read the original operands directly, independently of the runtime
        // command decoder, and preserve the last home write in suffix order.
        let source = placements.world_source().overlay.bytes();
        let mut expected_home = placement.position_words;
        for command in &placement.instance.commands {
            let at =
                placements.assets().worlds.main_payload.start + command.payload_offset as usize;
            let word = |relative: usize| {
                u16::from_le_bytes(source[at + relative..at + relative + 2].try_into().unwrap())
            };
            match command.code.value {
                20 => {
                    let [x, z] = [word(0), word(2)];
                    expected_home = [x, placements.terrain().height_at([x, z]) as u16, z];
                }
                21 => {
                    let [x, y, z] = [word(0), word(2), word(4)];
                    expected_home = [
                        x,
                        y.wrapping_add(placements.terrain().height_at([x, z]) as u16),
                        z,
                    ];
                }
                _ => {}
            }
        }
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
        let state = arena.selector34_actor(object).unwrap();
        assert_eq!(
            (
                state.word_230,
                state.word_234,
                state.byte_238,
                state.byte_239
            ),
            (500, 512, 1, 0)
        );
        assert_eq!((state.word_240, state.trail_25c), (750, None));
        assert_eq!(state.words_244_248, [None; 2]);
        assert_eq!(
            (
                state.table_24c,
                state.table_250,
                state.table_254,
                state.table_258
            ),
            (
                Some(0x0053_29f0),
                Some(0x0053_29e8),
                Some(0x0053_29e0),
                Some(0x0053_29d0)
            )
        );
        retained.push(state.retained_23c.clone().unwrap());
        assert_eq!(arena.family428650_actor(object).unwrap().word_184, -1);
        assert_eq!(
            arena.family428650_actor(object).unwrap().position_222,
            Some(expected_home)
        );
        let base = &arena.object(object).unwrap().base;
        assert_ne!(base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(base.dirty_word, 0);
        assert_eq!(base.placement_word, placement.placement_index as u16);
        objects.push(object);
    }
    assert!(Arc::ptr_eq(
        retained[0].allocation(),
        retained[1].allocation()
    ));
    assert!(!Arc::ptr_eq(
        retained[0].allocation(),
        allocation.runtime_allocation()
    ));
    let original = retained[0].allocation().source().to_vec();
    for object in objects {
        let handle = arena.actor(object).unwrap().handle;
        arena
            .initialize_after_load(object, Default::default())
            .unwrap();
        assert_eq!(
            arena.update(object),
            Err(ObjectError::ActorUpdateBoundary(object))
        );
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    }
    assert_eq!(retained[0].allocation().source().as_ref(), original);
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert!(effects.seen.is_empty());
}

#[test]
fn selector34_h14_alias_is_fresh_through_postinit_listeners_network_and_after_load() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 34);
    let xyz: Vec<_> = [14000_u16, 0xfffe, 45036]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut ratio = [0; 16];
    ratio[..2].copy_from_slice(&2_u16.to_le_bytes());
    ratio[14..].copy_from_slice(&0x8000_u16.to_le_bytes());
    let commands = OwnedCommands::new(&[
        (19, &[]),
        (21, &xyz),
        (22, &1_u32.to_le_bytes()),
        (23, &ratio),
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
        let retained = arena
            .selector34_actor(object)
            .unwrap()
            .retained_23c
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(
            retained.allocation(),
            commands.allocation.runtime_allocation()
        ));
        assert_eq!(
            retained.allocation_offset,
            commands.placement.commands[3].payload_offset
        );
        let family = arena.family428650_actor(object).unwrap();
        assert_eq!(family.word_214, 0x0100_0000);
        assert_eq!(
            family.position_222,
            Some([
                14000,
                0xfffe_u16.wrapping_add(placements.terrain().height_at([14000, 45036]) as u16),
                45036,
            ])
        );
        assert_ne!(arena.object(object).unwrap().base.flags & 0x1000, 0);
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
    arena
        .initialize_after_load(objects[1], Default::default())
        .unwrap();
    assert_eq!(arena.family428650_actor(objects[1]).unwrap().word_20c, 0);
    let original = commands.allocation.source().to_vec();
    commands
        .allocation
        .runtime_allocation()
        .write_word(commands.placement.commands[3].payload_offset + 14, 3)
        .unwrap();
    for object in objects {
        arena
            .finish_selector34_initialization(object, Some(placements.terrain()))
            .unwrap();
        assert_eq!(arena.family428650_actor(object).unwrap().word_214, 1536);
        assert_eq!(arena.object(object).unwrap().base.dirty_word, 0x8002);
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
    assert_eq!(commands.allocation.source().as_ref(), original);
    assert_eq!(arena.len(), 1);
}

#[test]
fn selector34_zero_denominator_does_not_read_missing_numerator_and_retention_is_lazy() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 34);
    let commands = OwnedCommands::new(&[(23, &[0, 0])]);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = ObserveListeners::default();
    let mut rng = 9;
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
            suffix: Some(commands.allocation.commands(&commands.placement)),
            command_allocation: Some(&commands.allocation),
            actor: Some(actor),
            ..request(resource)
        },
    ));
    assert_eq!(arena.family428650_actor(object).unwrap().word_214, 256);
    let offset = commands.placement.commands[0].payload_offset;
    let allocation = Arc::clone(commands.allocation.runtime_allocation());
    allocation.write_word(offset, 1).unwrap();
    assert!(
        matches!(arena.finish_selector34_initialization(object, Some(placements.terrain())),
        Err(DerivedActorError::Allocation(RuntimeAllocationError::Range { offset: at, .. }))
            if at == offset + 14)
    );
    let one_past = allocation.source().len() as u32;
    arena
        .set_selector34_retained(object, &allocation, one_past)
        .unwrap();
    assert!(
        matches!(arena.finish_selector34_initialization(object, Some(placements.terrain())),
        Err(DerivedActorError::Allocation(RuntimeAllocationError::Range { offset: at, .. }))
            if at == one_past)
    );
    let before = arena.selector34_actor(object).unwrap().retained_23c.clone();
    assert!(arena
        .set_selector34_retained(object, &allocation, one_past + 1)
        .is_err());
    assert_eq!(arena.selector34_actor(object).unwrap().retained_23c, before);
    drop(before);
    let weak = Arc::downgrade(&allocation);
    drop(allocation);
    drop(commands);
    assert!(weak.upgrade().is_some());
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn selector34_missing_terrain_or_command_owner_and_rejected_commands_clean_up() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 34);
    let math = math();
    for code in [20, 23, 24] {
        let mut arena = arena();
        let before = lists(&arena);
        // The rejected command must not attempt the later owner-less code23.
        let commands = Commands::new(2, &[(code, &[0; 4]), (23, &[])]);
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
                Err(ObjectConstructionError::MissingSelector34CommandOwner)
            )),
            24 => assert_eq!(
                result.unwrap(),
                ObjectConstructionOutcome::Rejected {
                    command_index: 0,
                    code: 24
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
fn selector34_listener_failure_cleans_new_bindings_and_preserves_prior_owner() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let resource = resource_with_selector(&runtime, 34);
    let math = math();
    let mut arena = arena();
    let owner = arena.allocate(ObjectClassKind::Base);
    let mut listeners = CreationListeners::new();
    let listener = listeners.register(owner, CreationListenerInterface::Selector18);
    let mut effects = ObserveListeners {
        reject: true,
        ..Default::default()
    };
    let mut rng = 9;
    let mut actor = inputs(&math, &mut listeners, &mut effects);
    actor.terrain = Some(placements.terrain());
    actor.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: [1, 1],
        rng: &mut rng,
    });
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            actor: Some(actor),
            ..request(resource)
        },
    );
    assert!(matches!(
        result,
        Err(ObjectConstructionError::CreationListener(
            CreationListenerError::UnsupportedInterface(CreationListenerInterface::Selector18)
        ))
    ));
    assert_eq!(effects.seen.len(), 1);
    let (failed, handle) = effects.seen[0];
    assert!(arena.object(failed).is_err());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(arena.len(), 1);
    assert!(arena.object(owner).is_ok());
    assert!(arena.actor_list().is_empty());
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners.get(listener).unwrap().owner, owner);
}
