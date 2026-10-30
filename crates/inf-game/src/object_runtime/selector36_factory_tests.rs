//! Selector36 production construction, shared retained inputs and owned helpers.

use super::{
    actor_factory_tests::{arena, inputs, math},
    selector34_factory_tests::OwnedCommands,
    tests::{constructed, lists, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::{object_runtime::*, terrain_runtime::TerrainSpacing, world_placement::WorldPlacements};
use inf_formats::native_basis::NativeBasisMath;

#[derive(Default)]
pub(super) struct Listener {
    pub(super) failed: Option<(ObjectId, ActorHandle)>,
}

impl CreationListenerEffects for Listener {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        assert_eq!(arena.object(object)?.base.dirty_word, 0);
        assert_eq!(arena.object(object)?.base.flags & INITIALIZED_FLAG, 0);
        assert!(arena.family429e20_actor(object)?.probe_234.is_some());
        assert_eq!(
            arena.family428650_actor(object)?.helper_218.is_some(),
            arena.derived_actor(object)?.behavior_140.is_some()
        );
        self.failed = Some((object, arena.actor(object)?.handle));
        listeners.register(object, CreationListenerInterface::Selector78);
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

pub(super) struct Factory {
    pub(super) runtime: crate::ResidentRuntime,
    pub(super) math: NativeBasisMath,
    pub(super) arena: ObjectArena,
    pub(super) listeners: CreationListeners,
    pub(super) effects: Listener,
    pub(super) rng: u32,
}

impl Factory {
    pub(super) fn new() -> Self {
        Self {
            runtime: retail_runtime(),
            math: math(),
            arena: arena(),
            listeners: CreationListeners::new(),
            effects: Listener::default(),
            rng: 9,
        }
    }

    fn construct(
        &mut self,
        suffix: Option<ObjectInstanceCommands<'_>>,
        allocation: Option<&Arc<InstanceCommandAllocation>>,
    ) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
        let mut actor = inputs(&self.math, &mut self.listeners, &mut self.effects);
        actor.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: [1, 1],
            rng: &mut self.rng,
        });
        construct_world_object(
            &mut self.arena,
            &self.runtime,
            ObjectConstructionRequest {
                suffix,
                command_allocation: allocation,
                actor: Some(actor),
                ..request(resource_with_selector(&self.runtime, 36))
            },
        )
    }
}

pub(super) fn assert_retired_helpers_released(retired: ObjectState) {
    let ObjectClass::Actor(actor) = retired.class else {
        panic!("actor")
    };
    let ActorExtension::Derived(derived) = actor.extension else {
        panic!("derived")
    };
    assert!(derived.behavior_140.is_none());
    let DerivedActorClass::Family428650(family) = derived.class else {
        panic!("family")
    };
    assert!(family.helper_218.is_none());
    let Family428650Class::Family429e20(probe) = family.class else {
        panic!("probe owner")
    };
    assert!(probe.probe_234.is_none());
}

#[test]
fn retail_selector36_placements_share_h13_addresses_and_own_cleared_probe_storage() {
    let mut factory = Factory::new();
    let placements = WorldPlacements::new(&factory.runtime, TerrainSpacing::Units256).unwrap();
    let selected: Vec<_> = placements
        .inputs()
        .filter(|placement| {
            let resource = factory
                .runtime
                .resource(13, placement.resource_argument as u16)
                .unwrap();
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
            bank.prefixes.entries[usize::from(resource.registration.local_index)]
                .constructor_selector
                == 36
        })
        .collect();
    assert_eq!(selected.len(), 3);
    let commands = Arc::new(
        InstanceCommandAllocation::new(
            Arc::from(placements.world_source().overlay.bytes()),
            placements.assets().worlds.main_payload,
            Arc::from(placements.assets().worlds.relocations.clone()),
        )
        .unwrap(),
    );
    let mut retained = vec![];
    for placement in selected {
        let mut actor = inputs(&factory.math, &mut factory.listeners, &mut factory.effects);
        actor.rotation_words = Some(placement.rotation_words);
        actor.terrain = Some(placements.terrain());
        actor.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: placements.world().actor_mask_limits(),
            rng: &mut factory.rng,
        });
        let object = constructed(construct_world_object(
            &mut factory.arena,
            &factory.runtime,
            ObjectConstructionRequest {
                resource_argument: placement.resource_argument,
                context: BaseContext {
                    position_words: Some(placement.position_words),
                    placement_index: placement.placement_index,
                },
                suffix: Some(commands.commands(placement.instance)),
                command_allocation: Some(&commands),
                network_active: false,
                actor: Some(actor),
            },
        ));
        let payload = factory
            .arena
            .selector36_actor(object)
            .unwrap()
            .payload_274
            .as_ref()
            .unwrap();
        let tail = factory
            .arena
            .family4236d0_actor(object)
            .unwrap()
            .retained_23c
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(payload.allocation(), tail.allocation()));
        assert!(!Arc::ptr_eq(
            payload.allocation(),
            commands.runtime_allocation()
        ));
        assert_eq!(payload.allocation_offset + 0x58, tail.allocation_offset);
        retained.push(payload.clone());
        let probe = factory
            .arena
            .family429e20_actor(object)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap()
            .decoded_cache();
        assert_eq!((probe.dimension(), probe.stride_words()), (16, 2));
        assert_eq!((probe.word_04(), probe.word_06()), (0, 0));
        assert_eq!((probe.byte_08(), probe.byte_0b()), (None, None));
        assert_eq!(probe.words(), &[0; 32]);
        let family = factory.arena.family428650_actor(object).unwrap();
        let behavior = factory
            .arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .is_some();
        assert_eq!(family.helper_218.is_some(), behavior);
        if let Some(helper) = &family.helper_218 {
            assert_eq!(helper.word_08, 0);
            assert_eq!(helper.words_00_04, [None; 2]);
            assert_eq!(
                helper.records,
                [crate::object_runtime::FamilyHelperHistoryRecord::UNKNOWN; 4]
            );
        }
        let base = &factory.arena.object(object).unwrap().base;
        assert_eq!(base.dirty_word, 0);
        assert_ne!(base.flags & INITIALIZED_FLAG, 0);
        factory
            .arena
            .initialize_after_load(object, Default::default())
            .unwrap();
        assert_eq!(
            factory.arena.update(object),
            Err(ObjectError::ActorUpdateBoundary(object))
        );
        assert_retired_helpers_released(
            factory
                .arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap(),
        );
    }
    assert!(retained
        .windows(2)
        .all(|pair| Arc::ptr_eq(pair[0].allocation(), pair[1].allocation())));
    assert!(factory.arena.is_empty());
    assert!(factory.arena.actor_list().is_empty());
}

#[test]
fn selector36_command_modes_and_shared_h14_ratio_reads_remain_live() {
    let mut factory = Factory::new();
    for (input, mapped, mode) in [(0_i32, 0, 0), (2, 18, 1), (3, 2, 2)] {
        let mut ratio = [0; 16];
        ratio[..2].copy_from_slice(&2_u16.to_le_bytes());
        ratio[14..].copy_from_slice(&3_u16.to_le_bytes());
        let commands = OwnedCommands::new(&[(6, &input.to_le_bytes()), (23, &[]), (24, &ratio)]);
        let object = constructed(factory.construct(
            Some(commands.allocation.commands(&commands.placement)),
            Some(&commands.allocation),
        ));
        assert_eq!(factory.arena.actor(object).unwrap().word_b4, mapped);
        let behavior = factory
            .arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .is_some();
        assert!(
            behavior,
            "selected retail selector36 has an allocating admission mode"
        );
        let family = factory.arena.family428650_actor(object).unwrap();
        assert_eq!(family.word_214, 1536);
        assert!(family.helper_218.is_some());
        assert_eq!(
            factory.arena.family4236d0_actor(object).unwrap().word_266,
            mode
        );
        if mapped == 0 {
            assert_ne!(
                factory.arena.object(object).unwrap().base.flags & 0x1000_0000,
                0
            );
        }
        let pointer = factory
            .arena
            .family4236d0_actor(object)
            .unwrap()
            .retained_23c
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(
            pointer.allocation(),
            commands.allocation.runtime_allocation()
        ));
        let payload = factory
            .arena
            .selector36_actor(object)
            .unwrap()
            .payload_274
            .as_ref()
            .unwrap();
        assert!(!Arc::ptr_eq(payload.allocation(), pointer.allocation()));
        let original = commands.allocation.source().to_vec();
        let at = commands.placement.commands[2].payload_offset;
        commands
            .allocation
            .runtime_allocation()
            .write_word(at + 14, 5)
            .unwrap();
        factory
            .arena
            .finish_selector36_initialization(object)
            .unwrap();
        assert_eq!(
            factory.arena.family428650_actor(object).unwrap().word_214,
            2560
        );
        assert_eq!(commands.allocation.source().as_ref(), original);
        assert_retired_helpers_released(
            factory
                .arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap(),
        );
    }
}

#[test]
fn selector36_command_frontiers_retain_lazily_and_failures_unpublish_helpers() {
    let mut factory = Factory::new();
    let no_operand = Commands::codes(1, &[23]);
    let object = constructed(factory.construct(Some(no_operand.request()), None));
    assert_retired_helpers_released(
        factory
            .arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap(),
    );
    for code in [24, 25] {
        let commands = Commands::codes(2, &[code, 24]);
        let before = lists(&factory.arena);
        let result = factory.construct(Some(commands.request()), None);
        if code == 24 {
            assert!(matches!(
                result,
                Err(ObjectConstructionError::MissingFamily4236d0CommandOwner { selector: 36 })
            ));
        } else {
            assert_eq!(
                result.unwrap(),
                ObjectConstructionOutcome::Rejected {
                    command_index: 0,
                    code: 25
                }
            );
        }
        assert_eq!(lists(&factory.arena), before);
        assert!(factory.arena.is_empty());
        assert!(factory.arena.actor_list().is_empty());
    }
    let commands = OwnedCommands::new(&[(24, &[0, 0])]);
    let object = constructed(factory.construct(
        Some(commands.allocation.commands(&commands.placement)),
        Some(&commands.allocation),
    ));
    let allocation = commands.allocation.runtime_allocation();
    let at = commands.placement.commands[0].payload_offset;
    allocation.write_word(at, 1).unwrap();
    assert!(
        matches!(factory.arena.finish_selector36_initialization(object),
        Err(DerivedActorError::Allocation(RuntimeAllocationError::Range { offset, .. })) if offset == at + 14)
    );
    let one_past = allocation.source().len() as u32;
    factory
        .arena
        .set_family4236d0_retained(object, allocation, one_past)
        .unwrap();
    assert!(
        matches!(factory.arena.finish_selector36_initialization(object),
        Err(DerivedActorError::Allocation(RuntimeAllocationError::Range { offset, .. })) if offset == one_past)
    );
    let weak = Arc::downgrade(allocation);
    drop(commands);
    let retired = factory
        .arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_retired_helpers_released(retired);
    assert!(weak.upgrade().is_none());
    let empty = OwnedCommands::new(&[(24, &[])]);
    assert!(matches!(
        factory.construct(
            Some(empty.allocation.commands(&empty.placement)),
            Some(&empty.allocation)
        ),
        Err(ObjectConstructionError::DerivedActor(
            DerivedActorError::Allocation(_)
        ))
    ));
    assert!(factory.arena.is_empty());
    assert!(factory.arena.actor_list().is_empty());
}

#[test]
fn selector36_listener_failure_removes_new_bindings_and_owned_allocations() {
    let mut factory = Factory::new();
    let owner = factory.arena.allocate(ObjectClassKind::Base);
    let listener = factory
        .listeners
        .register(owner, CreationListenerInterface::Selector18);
    let result = factory.construct(None, None);
    assert!(matches!(
        result,
        Err(ObjectConstructionError::CreationListener(_))
    ));
    let (object, handle) = factory.effects.failed.unwrap();
    assert!(factory.arena.object(object).is_err());
    assert_eq!(factory.arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(factory.arena.len(), 1);
    assert!(factory.arena.actor_list().is_empty());
    assert_eq!(factory.listeners.len(), 1);
    assert_eq!(factory.listeners.get(listener).unwrap().owner, owner);
}
