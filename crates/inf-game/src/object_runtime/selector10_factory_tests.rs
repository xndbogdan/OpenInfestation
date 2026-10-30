//! Selector10 authored construction and +26C acquisition/cleanup frontiers.

use super::{
    actor_factory_tests::inputs,
    selector34_factory_tests::OwnedCommands,
    selector36_factory_tests::{assert_retired_helpers_released, Factory, Listener},
    tests::{constructed, lists, request, resource_with_selector, Commands},
    *,
};
use crate::{object_runtime::*, terrain_runtime::TerrainSpacing, world_placement::WorldPlacements};
use inf_formats::FrontendExecutable;
use std::{fs, path::Path};

impl Factory {
    fn construct_selector10(
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
                ..request(resource_with_selector(&self.runtime, 10))
            },
        )
    }
}

fn assert_selector10_retirement(retired: ObjectState) {
    let ObjectClass::Actor(actor) = &retired.class else {
        panic!("actor")
    };
    let ActorExtension::Derived(derived) = &actor.extension else {
        panic!("derived")
    };
    let DerivedActorClass::Family428650(family) = &derived.class else {
        panic!("family")
    };
    let Family428650Class::Family429e20(probe) = &family.class else {
        panic!("probe")
    };
    let Family429e20Class::Family4236d0(family) = &probe.class else {
        panic!("family4236d0")
    };
    let Family4236d0Class::Selector10(state) = &family.class else {
        panic!("selector10")
    };
    assert!(state.helper_26c.is_none());
    assert_retired_helpers_released(retired);
}

#[test]
fn retail_selector10_placements_choose_helpers_from_full_authored_dword() {
    let mut factory = Factory::new();
    let placements = WorldPlacements::new(&factory.runtime, TerrainSpacing::Units256).unwrap();
    let selected: Vec<_> = placements
        .inputs()
        .filter_map(|placement| {
            let resource = factory
                .runtime
                .resource(13, placement.resource_argument as u16)
                .unwrap();
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
            let index = usize::from(resource.registration.local_index);
            if bank.prefixes.entries[index].constructor_selector != 10 {
                return None;
            }
            let ConstructorPayloadView::SharedActor(payload) = &bank.assets[index].payload else {
                panic!("actor")
            };
            Some((
                placement,
                payload.family425190.as_ref().unwrap().dword_74.value,
            ))
        })
        .collect();
    assert_eq!(selected.len(), 2);
    let executable =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe")).unwrap();
    let mapped = FrontendExecutable::parse(&executable).unwrap();
    let commands = Arc::new(
        InstanceCommandAllocation::new(
            Arc::from(placements.world_source().overlay.bytes()),
            placements.assets().worlds.main_payload,
            Arc::from(placements.assets().worlds.relocations.clone()),
        )
        .unwrap(),
    );
    for (placement, input) in selected {
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
        let state = factory.arena.selector10_actor(object).unwrap();
        assert_eq!(state.word_268, Some(input));
        // These are the original switch's five immutable descriptors; their
        // child counts come from the retail image, not the port helper factory.
        let table = [
            0x0053_07f8,
            0x0053_0900,
            0x0053_0a08,
            0x0053_0b10,
            0x0053_0c18,
        ];
        if let Some(&table) = table.get(input as usize) {
            let helper = state.helper_26c.as_ref().unwrap();
            let count = mapped.word(table).unwrap() as u16 as usize;
            assert_eq!(helper.table_10, table);
            assert_eq!(
                helper
                    .children_14
                    .iter()
                    .filter(|child| child.is_some())
                    .count(),
                count
            );
            assert!(helper.children_14[..count].iter().all(Option::is_some));
            assert!(helper.children_14[count..].iter().all(Option::is_none));
            assert_eq!(
                helper.class,
                if input == 4 {
                    Selector10HelperClass::Family446bc0 { phase_34: None }
                } else {
                    Selector10HelperClass::Family445ae0
                }
            );
            assert_eq!(
                helper.allocation_bytes(),
                if input == 4 { 0x38 } else { 0x34 }
            );
            assert_eq!((helper.word_08, helper.byte_0c, helper.byte_0d), (0, 1, 0));
            for child in helper.children_14.iter().flatten() {
                assert_eq!(
                    child.allocation_bytes(),
                    if input == 4 { 0x74 } else { 0x50 }
                );
                assert_eq!((child.word_14, child.word_36), (0, 0));
            }
        } else {
            assert!(state.helper_26c.is_none());
        }
        let handle = factory.arena.actor(object).unwrap().handle;
        factory
            .arena
            .initialize_after_load(object, Default::default())
            .unwrap();
        assert_eq!(
            factory.arena.update(object),
            Err(ObjectError::ActorUpdateBoundary(object))
        );
        assert_selector10_retirement(
            factory
                .arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap(),
        );
        assert_eq!(factory.arena.resolve_actor_handle(handle).unwrap(), None);
    }
    assert!(factory.arena.is_empty());
    assert!(factory.arena.actor_list().is_empty());
}

#[test]
fn selector10_command25_reads_nothing_and_rejected_commands_stop_before_owner_reads() {
    let mut factory = Factory::new();
    // The code ends exactly at the allocation boundary: no operand exists.
    let no_operand = Commands::codes(1, &[25]);
    let object = constructed(factory.construct_selector10(Some(no_operand.request()), None));
    assert_selector10_retirement(
        factory
            .arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap(),
    );
    for code in [24, 26] {
        let before = lists(&factory.arena);
        let commands = Commands::codes(2, &[code, 24]);
        let result = factory.construct_selector10(Some(commands.request()), None);
        if code == 24 {
            assert!(matches!(
                result,
                Err(ObjectConstructionError::MissingFamily4236d0CommandOwner { selector: 10 })
            ));
        } else {
            assert_eq!(
                result.unwrap(),
                ObjectConstructionOutcome::Rejected {
                    command_index: 0,
                    code: 26
                }
            );
        }
        assert_eq!(lists(&factory.arena), before);
        assert!(factory.arena.is_empty());
        assert!(factory.arena.actor_list().is_empty());
    }
}

#[test]
fn selector10_owned_h14_ratio_is_live_without_reacquiring_helper26c() {
    let mut factory = Factory::new();
    let mut ratio = [0; 16];
    ratio[..2].copy_from_slice(&2_u16.to_le_bytes());
    ratio[14..].copy_from_slice(&3_u16.to_le_bytes());
    let commands = OwnedCommands::new(&[(24, &ratio), (25, &[])]);
    let object = constructed(factory.construct_selector10(
        Some(commands.allocation.commands(&commands.placement)),
        Some(&commands.allocation),
    ));
    let retained = factory
        .arena
        .family4236d0_actor(object)
        .unwrap()
        .retained_23c
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(
        retained.allocation(),
        commands.allocation.runtime_allocation()
    ));
    assert_eq!(
        factory.arena.family428650_actor(object).unwrap().word_214,
        1536
    );
    let helper_before = factory
        .arena
        .selector10_actor(object)
        .unwrap()
        .helper_26c
        .clone();
    let original = commands.allocation.source().to_vec();
    let offset = commands.placement.commands[0].payload_offset;
    commands
        .allocation
        .runtime_allocation()
        .write_word(offset + 14, 7)
        .unwrap();
    // Revisit the parent consumer directly; 004252B0 itself would allocate a
    // second recognized helper and lose the first pointer, a separate boundary.
    factory
        .arena
        .finish_family4236d0_initialization(object)
        .unwrap();
    assert_eq!(
        factory.arena.family428650_actor(object).unwrap().word_214,
        3584
    );
    assert_eq!(
        factory.arena.selector10_actor(object).unwrap().helper_26c,
        helper_before
    );
    if helper_before.is_some() {
        assert!(
            matches!(factory.arena.finish_selector10_initialization(object), Err(DerivedActorError::Selector10HelperReplacementBoundary(id)) if id == object)
        );
    }
    assert_eq!(commands.allocation.source().as_ref(), original);
    let weak = Arc::downgrade(commands.allocation.runtime_allocation());
    drop(commands);
    assert_selector10_retirement(
        factory
            .arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap(),
    );
    assert!(weak.upgrade().is_none());
    let empty = OwnedCommands::new(&[(24, &[])]);
    assert!(matches!(
        factory.construct_selector10(
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

#[derive(Default)]
struct HelperListener(Listener);

impl CreationListenerEffects for HelperListener {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        assert!(arena.selector10_actor(object).unwrap().helper_26c.is_some());
        self.0.invoke(listener, object, listeners, arena)
    }
}

#[test]
fn selector10_listener_failure_after_helper_acquisition_cleans_all_owned_state() {
    let mut factory = Factory::new();
    let owner = factory.arena.allocate(ObjectClassKind::Base);
    let listener = factory
        .listeners
        .register(owner, CreationListenerInterface::Selector18);
    let mut effects = HelperListener::default();
    let mut actor = inputs(&factory.math, &mut factory.listeners, &mut effects);
    actor.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: [1, 1],
        rng: &mut factory.rng,
    });
    let result = construct_world_object(
        &mut factory.arena,
        &factory.runtime,
        ObjectConstructionRequest {
            actor: Some(actor),
            ..request(resource_with_selector(&factory.runtime, 10))
        },
    );
    assert!(matches!(
        result,
        Err(ObjectConstructionError::CreationListener(_))
    ));
    let (object, handle) = effects.0.failed.unwrap();
    assert!(factory.arena.object(object).is_err());
    assert_eq!(factory.arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(factory.arena.len(), 1);
    assert!(factory.arena.actor_list().is_empty());
    assert_eq!(factory.listeners.len(), 1);
    assert_eq!(factory.listeners.get(listener).unwrap().owner, owner);
}
