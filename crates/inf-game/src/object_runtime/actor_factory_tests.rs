//! Production factory custody and failure frontiers with canonical retail data.

use super::{
    tests::{constructed, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::{
    frontend_assets::FrontendAssets, object_runtime::*, terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
};
use inf_formats::{native_basis::NativeBasisMath, ByteSpan};
use std::{path::Path, sync::Arc};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

pub(super) fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

pub(super) fn math() -> NativeBasisMath {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    NativeBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values)
}

pub(super) fn grant_table() -> inf_formats::GrantTable {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    inf_formats::parse_grant_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap()
}

struct RejectCallbacks {
    calls: usize,
    register_before_failure: bool,
}

#[test]
fn child_factory_retains_late_error_but_cleans_native_rejection() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 88);
    let math = math();
    for rejected in [false, true] {
        let mut arena = arena();
        let parent = arena.allocate(ObjectClassKind::Base);
        arena
            .initialize_context(
                parent,
                BaseContext {
                    position_words: Some([11, 22, 33]),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        let mut listeners = CreationListeners::new();
        listeners.register(parent, CreationListenerInterface::Selector18);
        let mut effects = RejectCallbacks {
            calls: 0,
            register_before_failure: false,
        };
        let commands = Commands::codes(1, &[17]);
        let result = construct_world_object_from_source(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                actor: Some(inputs(&math, &mut listeners, &mut effects)),
                suffix: rejected.then(|| commands.request()),
                ..request(resource)
            },
            ConstructionPositionSource::ActorPosition(parent),
            ConstructionFailurePolicy::RetainOnError,
        );
        if rejected {
            assert!(matches!(
                result,
                Ok(ObjectConstructionOutcome::Rejected { code: 17, .. })
            ));
            assert_eq!(arena.len(), 1);
            assert_eq!(effects.calls, 0);
        } else {
            let Err(ObjectConstructionError::Retained { object, source }) = result else {
                panic!("late failure must retain the actual allocation")
            };
            assert!(matches!(
                *source,
                ObjectConstructionError::CreationListener(_)
            ));
            assert_eq!(effects.calls, 1);
            assert_eq!(arena.len(), 2);
            let state = arena.object(object).unwrap();
            assert_eq!(state.base.position_words, Some([11, 22, 33]));
            assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
            assert_eq!(state.base.dirty_word, 0);
            assert_eq!(arena.actor_list(), &[object]);
        }
    }
}

#[test]
fn child_factory_reads_parent_position_after_allocation_and_camera_mode1() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 16);
    let math = math();
    let mut arena = arena();
    let parent = arena.allocate(ObjectClassKind::Base);
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut input = inputs(&math, &mut listeners, &mut effects);
    input.camera_settings = Some(CameraSettings {
        first_person: 0,
        camera_style: 0,
    });
    input.spawn_mode_1 = true;
    let result = construct_world_object_from_source(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            actor: Some(input),
            ..request(resource)
        },
        ConstructionPositionSource::ActorPosition(parent),
        ConstructionFailurePolicy::RetainOnError,
    );
    let Err(ObjectConstructionError::Retained { object, source }) = result else {
        panic!("missing parent position must retain allocated camera")
    };
    assert!(
        matches!(*source, ObjectConstructionError::Object(ObjectError::ActorPositionUnknown(id)) if id == parent)
    );
    assert_ne!(arena.actor(object).unwrap().handle.raw() & 0x0400_0000, 0);
    assert_eq!(arena.object(parent).unwrap().base.position_words, None);
    assert_eq!(effects.calls, 0);
    assert_eq!(arena.len(), 2);
}
impl CreationListenerEffects for RejectCallbacks {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls += 1;
        let state = arena.object(actor)?;
        assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(state.base.dirty_word, 0);
        assert_eq!(arena.actor(actor)?.word_74, state.base.word_28 >> 3);
        if self.register_before_failure {
            listeners.register(actor, CreationListenerInterface::Selector78);
        }
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

pub(super) fn inputs<'a>(
    math: &'a NativeBasisMath,
    listeners: &'a mut CreationListeners,
    effects: &'a mut dyn CreationListenerEffects,
) -> ActorFactoryInputs<'a> {
    ActorFactoryInputs {
        terrain: None,
        world_start: None,
        // This is a controlled H12-miss constructor environment, not a claim
        // to have run every preceding actor in the recorded world load.
        allocation: camera_allocation(),
        rotation_words: Some([0; 3]),
        local_owner: 1,
        first_player_grants: None,
        grant_table: None,
        camera_settings: None,
        spawn_mode_1: false,
        network_handle: None,
        vehicle_table: None,
        math,
        derived: None,
        listeners,
        listener_effects: effects,
        network_world_active: false,
    }
}

#[test]
fn retail_selector88_placement_retains_its_whole_allocation_and_gates_later_passes() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let placement = placements.input(117).unwrap();
    assert_eq!(
        u32::from(placement.resource_argument as u16),
        resource_with_selector(&runtime, 88)
    );
    let source: Arc<[u8]> = Arc::from(placements.world_source().overlay.bytes());
    let allocation = Arc::new(
        InstanceCommandAllocation::new(
            source,
            placements.assets().worlds.main_payload,
            Arc::from(placements.assets().worlds.relocations.clone()),
        )
        .unwrap(),
    );
    let retained = Arc::downgrade(&allocation);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.rotation_words = Some(placement.rotation_words);
    let actor = constructed(construct_world_object(
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
            actor: Some(actor_inputs),
        },
    ));
    assert_eq!(effects.calls, 0);
    let state = arena.object(actor).unwrap();
    assert_eq!(state.base.position_words, Some(placement.position_words));
    assert_eq!(state.base.placement_word, 117);
    assert_eq!(state.base.flags, 0x4810_0002);
    assert_eq!(state.base.dirty_word, 0);
    let pointer = arena
        .selector88(actor)
        .unwrap()
        .retained_13c
        .as_ref()
        .unwrap();
    assert_eq!(pointer.allocation_offset, 6524);
    assert!(Arc::ptr_eq(pointer.allocation(), &allocation));
    let before = state.clone();
    assert_eq!(
        arena.initialize_after_load(actor, Default::default()),
        Err(ObjectError::ActorAfterLoadBoundary(actor))
    );
    assert_eq!(
        arena.update(actor),
        Err(ObjectError::ActorUpdateBoundary(actor))
    );
    assert_eq!(arena.object(actor).unwrap(), &before);
    drop(before);
    drop(allocation);
    assert!(retained.upgrade().is_some());
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(retained.upgrade().is_none());
}

#[test]
fn rejection_deletes_registration_before_unreached_retained_command_needs_an_owner() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 88);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let commands = Commands::codes(2, &[17, 16]);
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            suffix: Some(commands.request()),
            actor: Some(inputs(&math, &mut listeners, &mut effects)),
            ..request(resource)
        },
    )
    .unwrap();
    assert_eq!(
        result,
        ObjectConstructionOutcome::Rejected {
            command_index: 0,
            code: 17
        }
    );
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert_eq!(
        arena
            .resolve_actor_handle(ActorHandle::from_raw(0x08ff_0001))
            .unwrap(),
        None
    );
    assert_eq!(effects.calls, 0);
}

#[test]
fn failed_listener_dispatch_cleans_new_actor_bindings_but_keeps_prior_owner() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 88);
    let math = math();
    let mut arena = arena();
    let owner = arena.allocate(ObjectClassKind::Base);
    let mut listeners = CreationListeners::new();
    let listener = listeners.register(owner, CreationListenerInterface::Selector18);
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: true,
    };
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
        Err(ObjectConstructionError::CreationListener(
            CreationListenerError::UnsupportedInterface(CreationListenerInterface::Selector18)
        ))
    ));
    assert_eq!(effects.calls, 1);
    assert_eq!(arena.len(), 1);
    assert!(arena.object(owner).is_ok());
    assert!(arena.actor_list().is_empty());
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners.get(listener).unwrap().owner, owner);
}

#[test]
fn explicit_actor_dependencies_and_retained_source_identity_are_required() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 88);
    let mut arena = arena();
    assert!(matches!(
        construct_world_object(&mut arena, &runtime, request(resource)),
        Err(ObjectConstructionError::MissingActorInputs { selector: 88 })
    ));
    assert!(arena.is_empty());
    let commands = Commands::codes(1, &[16]);
    let equal_copy = Arc::new(
        InstanceCommandAllocation::new(
            Arc::from(16_i32.to_le_bytes()),
            ByteSpan::new(0, 4),
            Arc::from([]),
        )
        .unwrap(),
    );
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                suffix: Some(commands.request()),
                command_allocation: Some(&equal_copy),
                actor: Some(inputs(&math, &mut listeners, &mut effects,)),
                ..request(resource)
            }
        ),
        Err(ObjectConstructionError::InstanceAllocation(
            InstanceCommandAllocationError::SourceIdentity
        ))
    ));
    assert!(arena.is_empty());
    assert_eq!(effects.calls, 0);
}

#[test]
fn retail_selector77_placement_uses_world_limits_commands_and_postinitialization() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 77);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let matching: Vec<_> = placements
        .inputs()
        .filter(|input| u32::from(input.resource_argument as u16) == resource)
        .collect();
    assert_eq!(matching.len(), 1);
    let placement = matching[0];
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
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut rng = 0x0123_4567;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.rotation_words = Some(placement.rotation_words);
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
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
            actor: Some(actor_inputs),
        },
    ));
    assert_eq!(effects.calls, 0);
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.dirty_word, 0);
    assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(state.base.position_words, Some(placement.position_words));
    assert_eq!(
        arena.selector77_actor(object).unwrap().position_192,
        state.base.position_words
    );
    assert_eq!(
        arena.actor(object).unwrap().word_74,
        state.base.word_28 >> 3
    );
    let mut expected_rng = 0x0123_4567;
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    assert_eq!(rng, expected_rng);
    arena
        .initialize_after_load(object, Default::default())
        .unwrap();
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.update(object),
        Err(ObjectError::ActorUpdateBoundary(object))
    );
    assert_eq!(arena.object(object).unwrap(), &before);
}

#[test]
fn derived_factory_requires_rng_before_publication_and_keeps_consumed_rng_on_rejection() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 77);
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                actor: Some(inputs(&math, &mut listeners, &mut effects)),
                ..request(resource)
            }
        ),
        Err(ObjectConstructionError::MissingDerivedActorInputs { selector: 77 })
    ));
    assert!(arena.is_empty());

    let commands = Commands::codes(2, &[19]); // Rejection precedes missing second pointer.
    let mut rng = 0x1234_5678;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: [0, 0],
        rng: &mut rng,
    });
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            actor: Some(actor_inputs),
            suffix: Some(commands.request()),
            ..request(resource)
        },
    )
    .unwrap();
    assert_eq!(
        result,
        ObjectConstructionOutcome::Rejected {
            command_index: 0,
            code: 19
        }
    );
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert_eq!(
        arena
            .resolve_actor_handle(ActorHandle::from_raw(0x08ff_0001))
            .unwrap(),
        None
    );
    let mut expected_rng = 0x1234_5678;
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    assert_eq!(rng, expected_rng);
    assert_eq!(effects.calls, 0);
}

#[test]
fn supported_first_world_placements_construct_in_order_with_specialized_callbacks() {
    let runtime = retail_runtime();
    let grant_table = grant_table();
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
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut rng = 0x0123_4567;
    let mut counts = std::collections::BTreeMap::new();
    let mut selector60 = None;
    let mut unsupported = 0;
    let mut world_start = crate::world_start::WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    for placement in placements.inputs() {
        let selector = match construction_plan(
            &mut arena,
            &runtime,
            placement.resource_argument,
            ConstructionTarget::AuthoredClass,
        ) {
            Ok(plan) => plan.selector(),
            Err(ObjectConstructionError::UnsupportedSelector { .. }) => {
                unsupported += 1;
                continue;
            }
            Err(error) => panic!("placement {}: {error}", placement.placement_index),
        };
        let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
        actor_inputs.rotation_words = Some(placement.rotation_words);
        actor_inputs.grant_table = Some(&grant_table);
        actor_inputs.terrain = Some(placements.terrain());
        actor_inputs.world_start = Some(crate::world_start::WorldStartContext {
            state: &mut world_start,
            runtime: &runtime,
            terrain: placements.terrain(),
        });
        actor_inputs.derived = Some(DerivedActorFactoryInputs {
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
                // Exercise the real selector-11 +40 wrapper as well as postinit.
                network_active: true,
                actor: Some(actor_inputs),
            },
        ));
        *counts.entry(selector).or_insert(0) += 1;
        assert_ne!(
            arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
            0
        );
        if selector == 11 {
            assert_eq!(
                arena.object(object).unwrap().base.dirty_word & 0x0200,
                0x0200
            );
            assert_eq!(
                arena.actor(object).unwrap().word_74,
                arena.object(object).unwrap().base.word_28 >> 3
            );
            let before = arena.object(object).unwrap().base.flags;
            arena
                .initialize_after_load(object, Default::default())
                .unwrap();
            assert_eq!(
                arena.object(object).unwrap().base.flags,
                before | 0x0080_0000
            );
        }
        if selector == 60 {
            selector60 = Some(object);
        }
    }
    assert_eq!(unsupported, 0);
    assert_eq!(arena.len(), 118);
    assert_eq!(counts.get(&10), Some(&2));
    assert_eq!(counts.get(&12), Some(&6));
    assert_eq!(counts.get(&11), Some(&8));
    assert_eq!(counts.get(&30), Some(&3));
    assert_eq!(counts.get(&34), Some(&2));
    assert_eq!(counts.get(&35), Some(&3));
    assert_eq!(counts.get(&36), Some(&3));
    assert_eq!(counts.get(&60), Some(&1));
    assert_eq!(effects.calls, 0);
    let object = selector60.unwrap();
    let retained = arena
        .selector60_actor(object)
        .unwrap()
        .retained_190
        .as_ref()
        .unwrap()
        .clone();
    assert!(Arc::ptr_eq(retained.allocation(), &allocation));
    let image = allocation.runtime_allocation();
    let count = image.dword(retained.allocation_offset + 4).unwrap() as i32;
    assert!(count > 0);
    let table = image.dword(retained.allocation_offset + 8).unwrap();
    let mut expected = vec![];
    for index in 0..count as u32 {
        let row = table + index * 24;
        let capacity = image.word(row + 6).unwrap() as i16;
        assert!(capacity > 0);
        let timer = image.word(row + 10).unwrap() as i16 as i32 as u32;
        let reference = image.dword(row).unwrap() as i32;
        let target = arena.find_actor_by_placement(reference);
        let retained_handle = (image.byte(row + 16).unwrap() & 2 != 0 && target.is_some())
            .then(|| arena.actor_handle_by_placement(reference).unwrap());
        assert_eq!(image.runtime_pointer(row + 20).unwrap(), None);
        expected.push((capacity as usize, timer, retained_handle));
    }
    arena
        .initialize_after_load(object, Default::default())
        .unwrap();
    assert_eq!(
        arena.object(object).unwrap().base.flags & 0x0180_0000,
        0x0180_0000
    );
    assert_eq!(arena.selector60_live_rows(), count as usize);
    let null = arena.actor_handles().unwrap().null_handle();
    for (index, (capacity, timer, initial)) in expected.into_iter().enumerate() {
        let Some(RuntimePointer::Selector60Row(id)) = image
            .runtime_pointer(table + index as u32 * 24 + 20)
            .unwrap()
        else {
            panic!("missing live row")
        };
        let row = arena.selector60_row(id).unwrap();
        assert_eq!(row.word_04, timer);
        assert_eq!(row.handles_08.len(), capacity);
        assert_eq!(row.word_0c, u32::from(initial.is_some()));
        assert_eq!(row.word_10, 0);
        let mut handles = vec![null; capacity];
        if let Some(handle) = initial {
            handles[0] = handle;
        }
        assert_eq!(row.handles_08, handles);
    }
    assert!(matches!(
        arena.update(object),
        Err(ObjectError::ActorUpdateBoundary(_))
    ));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.selector60_live_rows(), 0);
    // Every placement constructs under explicit H12-miss/empty-listener inputs.
    // This does not establish later loader callbacks or full loading parity.
}

/// Constructs the first retail selector-12 placement in a fresh arena.
fn construct_first_selector12(
    first_player_grants: Option<&[u32; inf_formats::GRANT_MASK_WORDS]>,
    grant_table: Option<&inf_formats::GrantTable>,
) -> (
    ObjectArena,
    u32,
    Result<ObjectConstructionOutcome, ObjectConstructionError>,
) {
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
    let mut arena = arena();
    let placement = placements
        .inputs()
        .find(|placement| {
            matches!(
                construction_plan(
                    &mut arena,
                    &runtime,
                    placement.resource_argument,
                    ConstructionTarget::AuthoredClass,
                ),
                Ok(plan) if plan.selector() == 12
            )
        })
        .unwrap();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut world_start = crate::world_start::WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut rng = 0x0123_4567;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.rotation_words = Some(placement.rotation_words);
    actor_inputs.first_player_grants = first_player_grants;
    actor_inputs.grant_table = grant_table;
    actor_inputs.terrain = Some(placements.terrain());
    actor_inputs.world_start = Some(crate::world_start::WorldStartContext {
        state: &mut world_start,
        runtime: &runtime,
        terrain: placements.terrain(),
    });
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: placements.world().actor_mask_limits(),
        rng: &mut rng,
    });
    let result = construct_world_object(
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
            actor: Some(actor_inputs),
        },
    );
    (arena, rng, result)
}

#[test]
fn selector12_merges_first_player_grants_in_order_without_copying_high_bits() {
    let table = grant_table();
    // Retail offline closure plus ID 191, which lies above 00475350's 0..AE loop.
    let source = [0x2472, 0, 0x4100, 0x1440_0000, 8, 1 << 31];
    let (arena, _, result) = construct_first_selector12(Some(&source), Some(&table));
    let object = constructed(result);
    let id = u32::from(arena.actor(object).unwrap().word_7a);
    let state = arena.selector12_actor(object).unwrap();
    let mut expected = [0; inf_formats::GRANT_MASK_WORDS];
    inf_formats::merge_grants(&mut expected, &table, &source).unwrap();
    assert_eq!(expected, [0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    inf_formats::grant_into(&mut expected, &table, id).unwrap();
    assert_eq!(state.grants_24c.mask_04, expected);
    assert_eq!(state.grants_24c.notification_owner_1c, 0);
    // 00467720 queries the player's mask, not the embedded one just granted.
    assert_eq!(
        state.byte_29d,
        u8::from(!inf_formats::grant_is_owned(&source, id).unwrap())
    );
}

#[test]
fn selector12_absent_first_player_leaves_only_its_own_grant_and_byte_29d() {
    let table = grant_table();
    let (arena, _, result) = construct_first_selector12(None, Some(&table));
    let object = constructed(result);
    let id = u32::from(arena.actor(object).unwrap().word_7a);
    let state = arena.selector12_actor(object).unwrap();
    let expected = inf_formats::grant_closure(&table, &[id]).unwrap();
    assert_eq!(state.grants_24c.mask_04, expected);
    assert_eq!(state.byte_29d, 0);
    // A present player with an empty mask is distinct: every nonzero ID is unowned.
    let (arena, _, result) = construct_first_selector12(Some(&[0; 6]), Some(&table));
    let object = constructed(result);
    let state = arena.selector12_actor(object).unwrap();
    assert_eq!(state.grants_24c.mask_04, expected);
    assert_eq!(state.byte_29d, u8::from(id != 0));
}

#[test]
fn selector12_missing_grant_table_fails_before_publication_or_rng() {
    let (arena, rng, result) = construct_first_selector12(None, None);
    assert!(matches!(
        result,
        Err(ObjectConstructionError::MissingGrantTable { selector: 12 })
    ));
    assert!(arena.is_empty());
    assert_eq!(rng, 0x0123_4567);
}

/// Offline 0048DC30 placement loop: construct, then 0047F6C0 on actors.
fn construct_first_world_offline(
    world_completion: &[u32; 4],
) -> (ObjectArena, Vec<(u16, ObjectId)>, u32) {
    let (arena, objects, serial, _) = construct_first_world_offline_with_start(world_completion);
    (arena, objects, serial)
}

/// As above, also returning the world-start owner the placements mutated.
fn construct_first_world_offline_with_start(
    world_completion: &[u32; 4],
) -> (
    ObjectArena,
    Vec<(u16, ObjectId)>,
    u32,
    crate::world_start::WorldStartState,
) {
    let runtime = retail_runtime();
    let table = grant_table();
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
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut rng = 0x0123_4567;
    let mut world_start = crate::world_start::WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 0,
        harness_spawn: None,
        port_spawn: None,
    };
    // 0048DE64 sets 0054ECE4 and zeroes world +3D0 before the placement loop.
    arena.begin_world_placements();
    let mut objects = vec![];
    for placement in placements.inputs() {
        let selector = construction_plan(
            &mut arena,
            &runtime,
            placement.resource_argument,
            ConstructionTarget::AuthoredClass,
        )
        .unwrap()
        .selector();
        let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
        actor_inputs.rotation_words = Some(placement.rotation_words);
        actor_inputs.grant_table = Some(&table);
        actor_inputs.terrain = Some(placements.terrain());
        actor_inputs.world_start = Some(crate::world_start::WorldStartContext {
            state: &mut world_start,
            runtime: &runtime,
            terrain: placements.terrain(),
        });
        actor_inputs.derived = Some(DerivedActorFactoryInputs {
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
                actor: Some(actor_inputs),
            },
        ));
        if arena.actor(object).is_ok() {
            arena
                .register_loaded_actor(object, Some(world_completion))
                .unwrap();
        }
        objects.push((selector, object));
    }
    let serial = arena.world_load_state().actor_serial_3d0;
    (arena, objects, serial, world_start)
}

#[test]
fn offline_first_world_serials_actors_in_placement_order_then_runs_the_after_load_tail() {
    let (mut arena, objects, serial) = construct_first_world_offline(&[0; 4]);
    let actors: Vec<_> = objects
        .iter()
        .filter(|(_, object)| arena.actor(*object).is_ok())
        .map(|&(_, object)| object)
        .collect();
    assert_eq!(actors.len(), 30);
    assert_eq!(serial, 30);
    for (index, &object) in actors.iter().enumerate() {
        assert_eq!(arena.actor(object).unwrap().byte_78, index as u8);
    }
    let result = arena.finish_world_placements(
        ObjectAfterLoadInputs {
            world_completion: Some(&[0; 4]),
        },
        None,
        None,
    );
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    // Routes all finalize without deletion, and nothing awaited disposal.
    assert_eq!(arena.len(), 118);
    // The flag-0 pass completes. Selector 88 is parked, and its 00443930
    // stops the parked pass before a later parked selector-35 node.
    assert!(matches!(
        result,
        Err(WorldLoadError::Object(ObjectError::ActorAfterLoadBoundary(object))) if object == selector88
    ));
    for node in arena.list(ObjectList::Flag0Primary) {
        assert_ne!(
            arena.object(node.object).unwrap().base.flags & AFTER_LOAD_FLAG,
            0
        );
    }
    let unflagged: Vec<_> = objects
        .iter()
        .filter(|(_, object)| arena.object(*object).unwrap().base.flags & AFTER_LOAD_FLAG == 0)
        .map(|&(selector, object)| {
            assert!(arena
                .list(ObjectList::Parking)
                .iter()
                .any(|node| node.object == object && node.kind == ObjectNodeKind::Flag0));
            selector
        })
        .collect();
    assert_eq!(unflagged, [35, 88]);
}

/// Controlled allocation inputs for the on-demand camera: no terrain surface,
/// so the unknown allocation position is never read.
fn camera_allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 245,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

/// 00478E00's on-demand camera: H13 argument 1, null position/rotation,
/// placement -1, mode 0, no suffix.
fn construct_camera(
    arena: &mut ObjectArena,
    settings: CameraSettings,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.rotation_words = None;
    actor_inputs.camera_settings = Some(settings);
    construct_world_object(
        arena,
        &runtime,
        ObjectConstructionRequest {
            resource_argument: 1,
            context: BaseContext {
                position_words: None,
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(actor_inputs),
        },
    )
}

#[test]
fn camera_factory_requires_live_process_only_after_a_resolution_miss() {
    use crate::process_particles::factories::ParticleFactoryProcessState;

    let runtime = retail_runtime();
    let math = math();
    let absent_null = ParticleFactoryProcessState {
        process_null_handle: None,
        ..ParticleFactoryProcessState::native_startup()
    };
    for handle in [None, Some(ActorHandle::from_raw(0x0800_0001))] {
        for process in [None, Some(&absent_null)] {
            let mut arena = arena();
            arena.players.camera_74 = handle;
            let before = format!("{arena:?}");
            let mut listeners = CreationListeners::new();
            let mut effects = RejectCallbacks {
                calls: 0,
                register_before_failure: false,
            };
            let mut factory = CameraFactoryInputs {
                process,
                runtime: &runtime,
                math: &math,
                allocation: camera_allocation(),
                local_owner: 1,
                settings: CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                listeners: &mut listeners,
                listener_effects: &mut effects,
                rng: None,
            };
            assert!(matches!(
                arena.world_camera(&mut factory),
                Err(PlayerAttachError::MissingCameraFactoryProcessNullHandle)
            ));
            assert_eq!(format!("{arena:?}"), before);
            assert!(listeners.is_empty());
            assert_eq!(effects.calls, 0);
        }
    }
}

#[test]
fn camera_process_read_precedes_a_genuine_inactive_h13_factory_boundary() {
    use crate::process_particles::factories::ParticleFactoryProcessState;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    // Actual current residency before Boot has no H13 constructor bank.
    let runtime = ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    assert!(runtime.resource_capacity(13).unwrap() > 1);
    assert!(runtime.resource(13, 1).is_none());
    let math = math();
    let process = ParticleFactoryProcessState::native_startup();
    let mut arena = arena();
    let before = format!("{arena:?}");
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut factory = CameraFactoryInputs {
        process: None,
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    assert!(matches!(
        arena.world_camera(&mut factory),
        Err(PlayerAttachError::MissingCameraFactoryProcessNullHandle)
    ));
    assert_eq!(format!("{arena:?}"), before);
    factory.process = Some(&process);
    assert!(matches!(
        arena.world_camera(&mut factory),
        Err(PlayerAttachError::Construction(
            ObjectConstructionError::NotResident {
                resource_argument: 1
            }
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert!(listeners.is_empty());
    assert_eq!(effects.calls, 0);
}

#[test]
fn resolved_camera_and_null_view_bypass_unavailable_process() {
    use crate::process_particles::factories::ParticleFactoryProcessState;

    let runtime = retail_runtime();
    let math = math();
    let absent_null = ParticleFactoryProcessState {
        process_null_handle: None,
        ..ParticleFactoryProcessState::native_startup()
    };
    for is_camera in [true, false] {
        let mut arena = arena();
        let object = if is_camera {
            arena
                .allocate_selector16(
                    camera_allocation(),
                    CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                )
                .unwrap()
        } else {
            arena.allocate_actor(camera_allocation()).unwrap()
        };
        arena.players.camera_74 = Some(arena.actor(object).unwrap().handle);
        let actor_before = arena.object(object).unwrap().clone();
        let camera_before = is_camera.then(|| arena.selector16(object).unwrap().clone());
        let mut listeners = CreationListeners::new();
        let mut effects = RejectCallbacks {
            calls: 0,
            register_before_failure: false,
        };
        for process in [None, Some(&absent_null)] {
            let mut factory = CameraFactoryInputs {
                process,
                runtime: &runtime,
                math: &math,
                allocation: camera_allocation(),
                local_owner: 1,
                settings: CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                listeners: &mut listeners,
                listener_effects: &mut effects,
                rng: None,
            };
            assert_eq!(
                arena.world_camera(&mut factory).unwrap(),
                is_camera.then_some(object)
            );
            assert_eq!(*arena.object(object).unwrap(), actor_before);
            if let Some(before) = &camera_before {
                assert_eq!(arena.selector16(object).unwrap(), before);
            }
        }
        assert_eq!(arena.len(), 1);
        assert!(listeners.is_empty());
        assert_eq!(effects.calls, 0);
    }
}

#[test]
fn camera_factory_keeps_registry_null_independent_from_current_process_null() {
    use crate::process_particles::factories::ParticleFactoryProcessState;

    let runtime = retail_runtime();
    let math = math();
    let mut arena = arena();
    let process = ParticleFactoryProcessState {
        process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
        ..ParticleFactoryProcessState::native_startup()
    };
    let process_before = process;
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut rng = 0x1234_u32;
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: Some(&mut rng),
    };
    let camera = arena.world_camera(&mut factory).unwrap().unwrap();
    assert_eq!(arena.selector16(camera).unwrap().byte_187, 1);
    // No first player: 0040FB10 searches, but the camera's own flags lack
    // bit 0, so nothing scores, nothing draws and no target follows.
    let state = arena.selector16(camera).unwrap();
    assert_eq!(
        (state.byte_186, state.mode_188, state.target_18c),
        (1, 3, None)
    );
    assert_eq!(rng, 0x1234);
    assert_eq!(
        arena.camera_handle(),
        Some(arena.actor(camera).unwrap().handle)
    );
    assert_eq!(
        arena.actor_handles().unwrap().null_handle(),
        ActorHandle::from_raw(0)
    );
    assert_ne!(process.process_null_handle, Some(ActorHandle::from_raw(0)));
    assert_eq!(process, process_before);
    assert_eq!(effects.calls, 0);
}

#[test]
fn camera_constructs_from_h13_argument_one_with_recorded_prefix_state() {
    let (mut arena, objects, _) = construct_first_world_offline(&[0; 4]);
    let camera = constructed(construct_camera(
        &mut arena,
        CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
    ));
    // Recorded at native pass-2 dispatch: flags 48100002, extent 0x39, +74 7,
    // and the next registry handle after selector 88's 08E20001.
    let base = &arena.object(camera).unwrap().base;
    assert_eq!(base.flags, 0x4810_0002);
    assert_eq!(base.placement_word, 0xffff);
    assert_eq!(base.resource_word, 1);
    assert_eq!(base.word_28, 0x39);
    assert_eq!(base.dirty_word, 0);
    assert_eq!(base.position_words, None);
    let actor = arena.actor(camera).unwrap();
    assert_eq!(actor.word_74, 7);
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    assert_eq!(
        arena.actor(selector88).unwrap().handle,
        ActorHandle::from_raw(0x08e2_0001)
    );
    assert_eq!(actor.handle, ActorHandle::from_raw(0x08e1_0001));
    let state = arena.selector16(camera).unwrap();
    assert_eq!(
        (state.block_138, state.block_15c),
        (
            CameraParameterBlock {
                word_18: 0,
                table_1c: 0x0054_6960,
                table_20: 0x0054_6970
            },
            CameraParameterBlock {
                word_18: 0,
                table_1c: 0x0054_69a8,
                table_20: 0x0054_69b8
            }
        )
    );
    assert_eq!(
        [
            state.byte_184,
            state.byte_185,
            state.byte_186,
            state.byte_187
        ],
        [0, 1, 0, 0]
    );
    assert_eq!(state.mode_188, 3);
    assert_eq!(state.target_18c, None);
}

#[test]
fn camera_style_one_selects_alternate_tables_and_first_person_sets_mode_two() {
    let mut arena = arena();
    let camera = constructed(construct_camera(
        &mut arena,
        CameraSettings {
            first_person: 0x0100,
            camera_style: 1,
        },
    ));
    let state = arena.selector16(camera).unwrap();
    assert_eq!(
        [state.block_138.table_1c, state.block_138.table_20],
        [0x0054_6990, 0x0054_6980]
    );
    assert_eq!(
        [state.block_15c.table_1c, state.block_15c.table_20],
        [0x0054_69d8, 0x0054_69c8]
    );
    // 0040F800 receives only the low byte, so 0x100 is view mode zero.
    assert_eq!(state.mode_188, 3);
    let mut second = super::actor_factory_tests::arena();
    let camera = constructed(construct_camera(
        &mut second,
        CameraSettings {
            first_person: 1,
            camera_style: 2,
        },
    ));
    let state = second.selector16(camera).unwrap();
    assert_eq!(state.mode_188, 2);
    assert_eq!(state.block_138.table_1c, 0x0054_6960);
}

/// 0040F740 with a world, as F12 (00442BF0) and the Options rows through
/// 00463B50 run it: the camera M+74 names takes First Person's low byte as
/// its view mode (0040F800) and then the Camera Style's tables. Infest04's
/// four F12 presses each moved the camera's +188 between 3 and 2 this way.
/// With no camera handle, 00478E00 would build the camera.
#[test]
fn the_world_refresh_gives_the_camera_first_person_and_its_style() {
    let mut arena = arena();
    let settings = |first_person, camera_style| CameraSettings {
        first_person,
        camera_style,
    };
    assert_eq!(
        arena.refresh_world_camera(settings(1, 0)).unwrap(),
        WorldCameraRefresh::NeedsCamera
    );
    let camera = constructed(construct_camera(&mut arena, settings(0, 0)));
    arena.players.camera_74 = Some(arena.actor(camera).unwrap().handle);
    assert_eq!(
        arena.refresh_world_camera(settings(1, 1)).unwrap(),
        WorldCameraRefresh::Refreshed(camera)
    );
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.mode_188, 2);
    assert_eq!(
        [
            state.block_138.table_1c,
            state.block_138.table_20,
            state.block_15c.table_1c,
            state.block_15c.table_20
        ],
        [0x0054_6990, 0x0054_6980, 0x0054_69d8, 0x0054_69c8]
    );
    arena.refresh_world_camera(settings(0x100, 0)).unwrap();
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.mode_188, 3);
    assert_eq!(state.block_138.table_1c, 0x0054_6960);
}

#[test]
fn camera_requires_settings_before_publication() {
    let mut arena = arena();
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.rotation_words = None;
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            resource_argument: 1,
            context: BaseContext {
                position_words: None,
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: true,
            actor: Some(actor_inputs),
        },
    );
    assert!(matches!(
        result,
        Err(ObjectConstructionError::MissingCameraSettings)
    ));
    assert!(arena.is_empty());
}

#[test]
fn attaching_the_offline_player_to_selector88_builds_the_recorded_camera_target() {
    let (mut arena, objects, _) = construct_first_world_offline(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    assert_eq!(arena.first_player(), Some(player));
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    let handle = arena.actor(selector88).unwrap().handle;
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let template = camera_allocation();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: template,
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    assert!(arena.attach_player(player, handle, &mut factory).unwrap());
    assert_eq!(arena.world_player(player).unwrap().handle_d8, handle);
    let camera_handle = arena.camera_handle().unwrap();
    assert_eq!(camera_handle, ActorHandle::from_raw(0x08e1_0001));
    let camera = arena.resolve_actor_handle(camera_handle).unwrap().unwrap();
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.byte_187, 1);
    assert_eq!(state.mode_188, 3);
    assert_eq!(
        state
            .target_18c
            .map(|reference| arena.actor_follow_target(reference).unwrap()),
        Some(ActorFollowTarget {
            handle_04: ActorHandle::from_raw(0x08e2_0001),
            focus_08: Some([0x343d, 0x00e0, 0xb11e]),
            word_10: 2,
            focus_14: Some([0x343d, 0x00e0, 0xb11e]),
            velocity_1a: None,
        })
    );
    // A repeat with the same handle stores it without touching the camera.
    let before = arena.selector16(camera).unwrap().clone();
    assert!(arena.attach_player(player, handle, &mut factory).unwrap());
    assert_eq!(*arena.selector16(camera).unwrap(), before);
    assert_eq!(effects.calls, 0);
}

#[test]
fn attach_branches_store_null_refuse_unknown_and_default_actors_and_take_creatures() {
    let (mut arena, objects, _) = construct_first_world_offline(&[0; 4]);
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    // 0040FB40's pick from the same word, before the camera exists (its
    // own flags lack bit 0, so it never scores).
    let mut rng = 7_u32;
    let mut predicted_rng = rng;
    let predicted = arena.reselect_camera_target(&mut predicted_rng).unwrap();
    assert!(predicted.is_some());
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: Some(&mut rng),
    };
    // 00478E00 without any player constructs and publishes the camera and
    // marks +187: the mission-18 load probe shows retail calling it right
    // after detaching (handle_d8 null). With no actor to follow, 00478EFB
    // starts 0040FB10's search, which follows the best-scored actor until
    // the spawn tail wires the vehicle.
    let camera = arena.world_camera(&mut factory).unwrap().unwrap();
    let state = arena.selector16(camera).unwrap().clone();
    assert_eq!(
        (
            state.byte_187,
            state.byte_186,
            state.mode_188,
            state.dword_19c
        ),
        (1, 1, 3, 0)
    );
    assert_eq!(
        state
            .target_18c
            .map(|target| arena.actor_follow_target(target).unwrap().handle_04),
        predicted
    );
    let len = arena.len();
    // A known camera handle is reused through virtual +60.
    assert_eq!(arena.world_camera(&mut factory).unwrap(), Some(camera));
    assert_eq!(arena.len(), len);

    let player = arena.add_world_player([0; 6]);
    let null = ActorHandle::from_raw(0);
    assert!(arena.attach_player(player, null, &mut factory).unwrap());
    // An unregistered handle fails without storing it.
    let stale = ActorHandle::from_raw(0x0800_0001);
    assert!(!arena.attach_player(player, stale, &mut factory).unwrap());
    assert_eq!(arena.world_player(player).unwrap().handle_d8, null);
    // Every actor table but the acceptors binds 00402720, which refuses:
    // the first such actor of the world is not taken.
    const REFUSING: [u16; 52] = [
        2, 3, 4, 5, 6, 7, 11, 15, 17, 18, 20, 23, 24, 26, 27, 29, 30, 31, 32, 33, 34, 35, 37, 38,
        39, 40, 41, 44, 47, 49, 52, 53, 55, 56, 57, 59, 60, 61, 62, 66, 69, 73, 74, 77, 79, 80, 82,
        84, 85, 90, 91, 93,
    ];
    let refusing = objects
        .iter()
        .find(|(selector, object)| REFUSING.contains(selector) && arena.actor(*object).is_ok())
        .expect("a default actor in the first world")
        .1;
    let handle = arena.actor(refusing).unwrap().handle;
    assert!(!arena.attach_player(player, handle, &mut factory).unwrap());
    assert_eq!(arena.world_player(player).unwrap().handle_d8, null);
    // Selector 12's 00405750 accepts: the player takes it, and the camera
    // follows it.
    let selector12 = objects
        .iter()
        .find(|(selector, _)| *selector == 12)
        .unwrap()
        .1;
    let handle = arena.actor(selector12).unwrap().handle;
    assert!(arena.attach_player(player, handle, &mut factory).unwrap());
    assert_eq!(arena.world_player(player).unwrap().handle_d8, handle);
    let target = arena.selector16(camera).unwrap().target_18c.unwrap();
    assert_eq!(arena.actor_follow_target(target).unwrap().handle_04, handle);
}

#[test]
fn offline_first_world_after_load_binds_the_player_and_rewrites_selector88_like_the_recording() {
    // Recorded 0048DC30 entry holds A0 = 1; the prelude then selects world 22.
    let mut mission = crate::world_mission::WorldMission::new(
        inf_formats::MissionSaveState::new_game(),
        crate::world_mission::WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: Some(Box::new([0; crate::world_mission::RECORD_0C_BYTES])),
        },
    );
    mission.begin_world_load().unwrap();
    let completion = mission.current_world_completion().unwrap();
    let (mut arena, objects, _) = construct_first_world_offline(&completion);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    arena
        .finish_world_placements(
            ObjectAfterLoadInputs {
                world_completion: Some(&completion),
            },
            Some(&mut factory),
            None,
        )
        .unwrap();
    // Native pass 2 ends with selector 88, the parked selector 35, then the
    // camera created inside selector 88's callback: 119 flagged objects.
    assert_eq!(arena.len(), 119);
    let camera_handle = arena.camera_handle().unwrap();
    assert_eq!(camera_handle, ActorHandle::from_raw(0x08e1_0001));
    let camera = arena.resolve_actor_handle(camera_handle).unwrap().unwrap();
    for object in objects.iter().map(|(_, object)| *object).chain([camera]) {
        assert_ne!(
            arena.object(object).unwrap().base.flags & AFTER_LOAD_FLAG,
            0
        );
    }
    let parking_flag0: Vec<_> = arena
        .list(ObjectList::Parking)
        .iter()
        .filter(|node| node.kind == ObjectNodeKind::Flag0)
        .map(|node| arena.object(node.object).unwrap().base.placement_word)
        .collect();
    assert_eq!(
        &parking_flag0[parking_flag0.len() - 3..],
        [0x75, 0x50, 0xffff]
    );
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    assert_eq!(
        arena.world_player(player).unwrap().handle_d8,
        ActorHandle::from_raw(0x08e2_0001)
    );
    assert!(!arena.world_load_state().flag_54ece4);
    // Recorded rewrite of the retained header at H14 main offset 6524.
    let retained = arena
        .selector88(selector88)
        .unwrap()
        .retained_13c
        .clone()
        .unwrap();
    let image = retained.allocation().runtime_allocation();
    let base = retained.allocation_offset;
    assert_eq!(image.dword(base + 0x10).unwrap(), 0x08ef_0001);
    let rows = |count: u32, pointer: u32| -> Vec<u32> {
        let table = image.dword(base + pointer).unwrap();
        (0..image.dword(base + count).unwrap())
            .map(|index| image.dword(table + index * 4).unwrap())
            .collect()
    };
    assert_eq!(
        rows(0x14, 0x18),
        [
            0x08f4_0001,
            0x08f3_0001,
            0x08f2_0001,
            0x08f1_0001,
            0x08e6_0001,
            0x08e5_0001
        ]
    );
    assert_eq!(rows(0x1c, 0x20), [0x08fd_0001, 0x08fc_0001]);
    assert_eq!(
        rows(0x24, 0x28),
        [
            0x08ed_0001,
            0x08ec_0001,
            0x08f0_0001,
            0x08ea_0001,
            0x08e9_0001
        ]
    );
    assert_eq!(rows(0x2c, 0x30), [0x08e4_0001, 0x08e3_0001]);
    // The last array's actors lose flag 0x20 and flag-1 list membership.
    for raw in [0x08e4_0001, 0x08e3_0001] {
        let actor = arena
            .resolve_actor_handle(ActorHandle::from_raw(raw))
            .unwrap()
            .unwrap();
        let flags = arena.object(actor).unwrap().base.flags;
        assert_eq!(flags & 0x22, 0);
        assert!(arena
            .list(ObjectList::Parking)
            .iter()
            .any(|node| node.object == actor && node.kind == ObjectNodeKind::Flag1));
    }
    assert_eq!(effects.calls, 0);
}

#[test]
fn selector88_without_retained_commands_disposes_itself_without_binding() {
    let mut arena = arena();
    arena.begin_world_placements();
    let player = arena.add_world_player([0; 6]);
    let object = arena.allocate_selector88(camera_allocation()).unwrap();
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    arena
        .initialize_selector88_after_load(object, &mut factory)
        .unwrap();
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object));
    // Disposal precedes the 0054ECE4 clear and the player binding.
    assert!(arena.world_load_state().flag_54ece4);
    assert_eq!(
        arena.world_player(player).unwrap().handle_d8,
        ActorHandle::from_raw(0)
    );
    assert_eq!(arena.camera_handle(), None);
}

#[test]
fn first_world_header_drives_the_recorded_mission_prelude_and_tail() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let world = placements.world();
    let mut mission = crate::world_mission::WorldMission::new(
        inf_formats::MissionSaveState::new_game(),
        crate::world_mission::WorldMissionInputs {
            current_world_a0: 1,
            dword_1c: 0x2800,
            byte_13: Some(1),
            record_0c: Some(Box::new([0; crate::world_mission::RECORD_0C_BYTES])),
        },
    );
    mission.begin_world_load().unwrap();
    mission.apply_world_header(world);
    mission.finish_world_load(world).unwrap();
    // Recorded W after the loader: +10 00AA, +12 1, +13 1, +20 500, +24 0.
    assert_eq!(mission.word_10, Some(0xaa));
    assert_eq!(mission.byte_12, Some(1));
    assert_eq!(mission.byte_13, Some(1));
    assert_eq!(mission.dword_20, Some(0x500));
    assert_eq!(mission.dword_24, Some(0));
}

#[test]
fn loader_player_branch_requests_offline_respawn_or_attaches_network_players_to_the_camera() {
    for (network, setting, expect_camera) in [
        (false, 0, false),
        (true, 2, false),
        (true, 0, true),
        (true, 4, true),
    ] {
        let (mut arena, objects, _) = construct_first_world_offline(&[0; 4]);
        let player = arena.add_world_player([0; 6]);
        let runtime = retail_runtime();
        let math = math();
        let mut listeners = CreationListeners::new();
        let mut effects = RejectCallbacks {
            calls: 0,
            register_before_failure: false,
        };
        let process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let mut factory = CameraFactoryInputs {
            process: Some(&process),
            runtime: &runtime,
            math: &math,
            allocation: camera_allocation(),
            local_owner: 1,
            settings: CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            listeners: &mut listeners,
            listener_effects: &mut effects,
            rng: None,
        };
        // As selector 88's after-load leaves it.
        let selector88 = objects
            .iter()
            .find(|(selector, _)| *selector == 88)
            .unwrap()
            .1;
        let handle88 = arena.actor(selector88).unwrap().handle;
        assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
        let camera = arena
            .resolve_actor_handle(arena.camera_handle().unwrap())
            .unwrap()
            .unwrap();
        let camera_before = arena.selector16(camera).unwrap().clone();
        let setup = arena
            .run_loader_player_setup(network, setting, &mut factory)
            .unwrap();
        let state = arena.world_player(player).unwrap();
        assert_eq!((state.word_11c, state.byte_58), (0, Some(1)));
        if expect_camera {
            assert_eq!(setup, LoaderPlayerSetup::AttachedToCamera(player));
            assert_eq!(state.handle_d8, arena.actor(camera).unwrap().handle);
            let after = arena.selector16(camera).unwrap();
            assert_eq!(after.dword_180, Some(0));
            // 0046AE90 returns before retargeting when the camera is the actor.
            assert_eq!(after.target_18c, camera_before.target_18c);
        } else {
            assert_eq!(setup, LoaderPlayerSetup::Respawn(player));
            assert_eq!(state.handle_d8, handle88);
            assert_eq!(*arena.selector16(camera).unwrap(), camera_before);
        }
    }
    let mut arena = arena();
    let runtime = retail_runtime();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    assert_eq!(
        arena
            .run_loader_player_setup(false, 0, &mut factory)
            .unwrap(),
        LoaderPlayerSetup::NoPlayer
    );
    assert!(arena.is_empty());
}

#[test]
fn offline_player_spawn_setup_matches_the_recorded_factory_inputs() {
    let (mut arena, objects, _, mut state) = construct_first_world_offline_with_start(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    let handle88 = arena.actor(selector88).unwrap().handle;
    assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
    assert_eq!(
        arena
            .run_loader_player_setup(false, 0, &mut factory)
            .unwrap(),
        LoaderPlayerSetup::Respawn(player)
    );
    // Selector 30 published its start override during the placement loop.
    // The spawn keeps its X/Z but resamples Y (FFBE -> FFFC); the heading
    // still comes from H1 pair 0.
    assert_eq!(state.override_position, Some([0x724e, 0xffbe, 0xb559]));
    assert_eq!(state.vehicle_selection, 0);
    let start = crate::world_start::WorldStartContext {
        state: &mut state,
        runtime: &runtime,
        terrain: placements.terrain(),
    };
    let header_18 = placements.world().header_words_00_to_34[6];
    let request = arena
        .prepare_player_spawn(player, &start, header_18, None, &mut factory)
        .unwrap();
    assert_eq!(request.position, [0x724e, 0xfffc, 0xb559]);
    assert_eq!(request.rotation, Some([0, 0xc000, 0]));
    let state = arena.world_player(player).unwrap();
    assert_eq!(state.handle_d8, ActorHandle::from_raw(0));
    assert_eq!(
        (
            state.controls.dword_28,
            state.controls.dword_2c,
            state.controls.dword_34,
            state.dword_50
        ),
        (2, 0x0a, 1, Some(0))
    );
    assert_eq!((state.dword_12c, state.byte_ee), (0, 1));
    // The camera keeps following selector 88; a null detach never retargets.
    let camera = arena
        .resolve_actor_handle(arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        arena
            .actor_follow_target(arena.selector16(camera).unwrap().target_18c.unwrap())
            .unwrap()
            .handle_04,
        handle88
    );
}

#[test]
fn slot0_probe() {
    let runtime = retail_runtime();
    let mut arena = arena();
    match construction_plan(&mut arena, &runtime, 0, ConstructionTarget::AuthoredClass) {
        Ok(plan) => println!("slot0 selector={}", plan.selector()),
        Err(error) => println!("slot0 error={error:?}"),
    }
}

#[test]
fn offline_vehicle_spawn_constructs_selector2_like_the_recording() {
    let (mut arena, objects, _, mut start_state) =
        construct_first_world_offline_with_start(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let table = inf_formats::parse_vehicle_behavior_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let request = {
        let process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let mut factory = CameraFactoryInputs {
            process: Some(&process),
            runtime: &runtime,
            math: &math,
            allocation: camera_allocation(),
            local_owner: 1,
            settings: CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            listeners: &mut listeners,
            listener_effects: &mut effects,
            rng: None,
        };
        let selector88 = objects
            .iter()
            .find(|(selector, _)| *selector == 88)
            .unwrap()
            .1;
        let handle88 = arena.actor(selector88).unwrap().handle;
        assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
        arena
            .run_loader_player_setup(false, 0, &mut factory)
            .unwrap();
        let start = crate::world_start::WorldStartContext {
            state: &mut start_state,
            runtime: &runtime,
            terrain: placements.terrain(),
        };
        arena
            .prepare_player_spawn(
                player,
                &start,
                placements.world().header_words_00_to_34[6],
                None,
                &mut factory,
            )
            .unwrap()
    };
    let mut rng = 0x0123_4567;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    // Recorded pre-context allocation WORDs at +32/+34/+36.
    actor_inputs.allocation.allocation_position = Some([0, 2, 0]);
    actor_inputs.allocation.terrain = ActorTerrainInputs {
        terrain: Some(crate::object_runtime::ActorTerrainSurface {
            terrain: placements.terrain(),
            plane_height_fixed: placements.plane_height_fixed(),
        }),
        coordinate_shift: 8,
        clock: 245,
        world_flags: Some(placements.world().flags),
        sine_quarter: &UNUSED_SINE,
    };
    actor_inputs.rotation_words = request.rotation;
    actor_inputs.spawn_mode_1 = true;
    actor_inputs.vehicle_table = Some(crate::object_runtime::VehicleFactoryInputs {
        table: &table,
        network_settings: None,
    });
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: placements.world().actor_mask_limits(),
        rng: &mut rng,
    });
    let vehicle = constructed(construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            resource_argument: 0,
            context: BaseContext {
                position_words: Some(request.position),
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(actor_inputs),
        },
    ));
    // Recorded at 0046ABB8: flags 48106927, extent 006D, handle 0CE00001
    // (the 004625D0 mode-1 mark), health/maximum 1388 and +B4 = 2.
    let base = &arena.object(vehicle).unwrap().base;
    assert_eq!(base.flags, 0x4810_6927);
    assert_eq!(base.word_28, 0x6d);
    assert_eq!(base.placement_word, 0xffff);
    assert_eq!(base.position_words, Some([0x724e, 0xfffc, 0xb559]));
    assert_eq!(base.dirty_word, 0);
    let actor = arena.actor(vehicle).unwrap();
    assert_eq!(actor.handle, ActorHandle::from_raw(0x0ce0_0001));
    assert_eq!((actor.health, actor.maximum_health), (0x1388, Some(0x1388)));
    assert_eq!(actor.word_b4, 2);
    let state = arena.selector2_actor(vehicle).unwrap();
    assert_eq!(state.word_184, Some(0xc8));
    assert_eq!(state.byte_209, 0);
    assert_eq!(state.handles_1a4.handles, [ActorHandle::from_raw(0); 3]);
    assert_eq!(state.bytes_1b4_1bf[8], 1);
    assert_eq!(state.dwords_1e8_1f4, [0, 0, 1, 0]);
    let behavior = arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_eq!(
        [
            behavior.byte_0c,
            behavior.byte_0d,
            behavior.byte_0e,
            behavior.byte_0f,
            behavior.byte_10,
            behavior.byte_11
        ],
        [1, 2, 8, 8, 0, 8]
    );
    assert_eq!((behavior.word_04, behavior.word_08), (0, 0));
    assert_eq!(behavior.entries_18.len(), 16);
    assert_eq!(behavior.groups_1c.len(), 2);
    assert_eq!(
        behavior.table_14,
        crate::object_runtime::ActorBehaviorTable::Selector2Copy
    );
    assert_eq!(state.table_26c.as_deref(), Some(&table));

    // 0046A890 after the factory, recorded at the first 0046B2B0 call.
    let limits = inf_formats::parse_vehicle_upgrade_limits(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        limits,
        [-1, -1, -1, -1, 8, 8, -1, -1, 8, 8, 8, 8, 8, 8, 8, 8]
    );
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    let camera = arena
        .finish_player_spawn(
            player,
            vehicle,
            22,
            &limits,
            &mut factory,
            &mut 0,
            None,
            None,
            None,
        )
        .unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    assert_eq!(arena.world_player(player).unwrap().handle_d4, handle);
    let base = &arena.object(vehicle).unwrap().base;
    assert_eq!((base.flags, base.dirty_word), (0x4810_6937, 2));
    assert_eq!(arena.actor(vehicle).unwrap().word_b4, 2);
    let clamped = [-1, -1, -1, -1, 0, 0, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(arena.world_player(player).unwrap().values_150, clamped);
    let behavior = arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    for (entry, value) in behavior.entries_18.iter().zip(clamped) {
        assert_eq!(entry[0], value as u32);
    }
    // WORD +134 already held 1C in the executable copy.
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().table_26c.as_deref(),
        Some(&table)
    );
    let camera_state = arena.selector16(camera).unwrap();
    assert_eq!(camera_state.byte_187, 1);
    assert_eq!(
        camera_state
            .target_18c
            .map(|reference| arena.actor_follow_target(reference).unwrap()),
        Some(ActorFollowTarget {
            handle_04: handle,
            focus_08: Some([0x724e, 0x0009, 0xb559]),
            word_10: 2,
            focus_14: Some([0x724e, 0x0009, 0xb559]),
            velocity_1a: None,
        })
    );

    // Owned upgrade IDs patch the +26C copy; +B0 keeps A4 as the last writer.
    let mut bits = [0_u32; 6];
    for id in [0x95_u32, 0x96, 0x9a, 0x9d, 0xa0, 0x1e, 0xa4, 0xa2, 0x27] {
        bits[(id >> 5) as usize] |= 1 << (id & 31);
    }
    let mut values = [0; 16];
    arena
        .apply_vehicle_upgrades(vehicle, None, &bits, &mut values, &limits, false)
        .unwrap();
    let patched = arena
        .selector2_actor(vehicle)
        .unwrap()
        .table_26c
        .as_deref()
        .unwrap();
    let half = |offset: usize| (patched[offset / 4] >> ((offset % 4) * 8)) as u16;
    assert_eq!(patched[0x08 / 4], 120_000);
    assert_eq!(half(0x06), 2);
    assert_eq!(patched[0x34 / 4], 150_000);
    assert_eq!(patched[0x70 / 4], 0x3000);
    assert_eq!(half(0xb0), 0x0c);
    assert_eq!(half(0x8a), 1);
    // ID 27's +58 write is network-only; unowned 97/9C/9E/9F stay unchanged.
    assert_eq!(half(0x58), (table[0x58 / 4] >> ((0x58 % 4) * 8)) as u16);
    for offset in [0x18, 0x2c, 0x44] {
        assert_eq!(patched[offset / 4], table[offset / 4]);
    }
    assert_eq!(values, [0; 16]);
}

/// The first offline load through 0046A890's camera retarget: world,
/// selector-88 binding, loader player branch, spawn setup, vehicle
/// construction and post-factory wiring, as recorded.
fn spawned_first_world_vehicle() -> (ObjectArena, PlayerId, ObjectId) {
    let (mut arena, objects, _, mut start_state) =
        construct_first_world_offline_with_start(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let table = inf_formats::parse_vehicle_behavior_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let request = {
        let process =
            crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
        let mut factory = CameraFactoryInputs {
            process: Some(&process),
            runtime: &runtime,
            math: &math,
            allocation: camera_allocation(),
            local_owner: 1,
            settings: CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            listeners: &mut listeners,
            listener_effects: &mut effects,
            rng: None,
        };
        let selector88 = objects
            .iter()
            .find(|(selector, _)| *selector == 88)
            .unwrap()
            .1;
        let handle88 = arena.actor(selector88).unwrap().handle;
        assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
        arena
            .run_loader_player_setup(false, 0, &mut factory)
            .unwrap();
        let start = crate::world_start::WorldStartContext {
            state: &mut start_state,
            runtime: &runtime,
            terrain: placements.terrain(),
        };
        arena
            .prepare_player_spawn(
                player,
                &start,
                placements.world().header_words_00_to_34[6],
                None,
                &mut factory,
            )
            .unwrap()
    };
    let mut rng = 0x0123_4567;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    // Recorded pre-context allocation WORDs at +32/+34/+36.
    actor_inputs.allocation.allocation_position = Some([0, 2, 0]);
    actor_inputs.allocation.terrain = ActorTerrainInputs {
        terrain: Some(crate::object_runtime::ActorTerrainSurface {
            terrain: placements.terrain(),
            plane_height_fixed: placements.plane_height_fixed(),
        }),
        coordinate_shift: 8,
        clock: 245,
        world_flags: Some(placements.world().flags),
        sine_quarter: &UNUSED_SINE,
    };
    actor_inputs.rotation_words = request.rotation;
    actor_inputs.spawn_mode_1 = true;
    actor_inputs.vehicle_table = Some(crate::object_runtime::VehicleFactoryInputs {
        table: &table,
        network_settings: None,
    });
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: placements.world().actor_mask_limits(),
        rng: &mut rng,
    });
    let vehicle = constructed(construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            resource_argument: 0,
            context: BaseContext {
                position_words: Some(request.position),
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(actor_inputs),
        },
    ));
    let limits = inf_formats::parse_vehicle_upgrade_limits(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    arena
        .finish_player_spawn(
            player,
            vehicle,
            22,
            &limits,
            &mut factory,
            &mut 0,
            None,
            None,
            None,
        )
        .unwrap();
    (arena, player, vehicle)
}

#[test]
fn offline_spawn_grants_match_the_recorded_pickups_and_player_refresh() {
    let (mut arena, player, vehicle) = spawned_first_world_vehicle();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let tables = PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active: false,
    };
    // 0046A890's offline grants, in order; the recording returned 1, 0, 1 and
    // showed one message: 0x139 "Upgrade", 384 and its 385 detail.
    assert_eq!(
        arena.grant_player_pickup(player, 1, 0, tables).unwrap(),
        PickupOutcome {
            accepted: true,
            message: Some(PickupMessage {
                prefix_text: 0x139,
                name_text: 384,
                detail_text: Some(385),
            }),
            notification: None,
            nested: Vec::new(),
            full: false,
            command: None,
            sound: None,
        }
    );
    assert_eq!(
        arena.grant_player_pickup(player, 4, 5000, tables).unwrap(),
        PickupOutcome {
            accepted: false,
            message: None,
            notification: None,
            nested: Vec::new(),
            full: false,
            command: None,
            sound: None,
        }
    );
    assert_eq!(
        arena.grant_player_pickup(player, 0x32, 0, tables).unwrap(),
        PickupOutcome {
            accepted: true,
            message: None,
            notification: None,
            nested: Vec::new(),
            full: false,
            command: None,
            sound: None,
        }
    );
    let state = arena.world_player(player).unwrap();
    assert_eq!(
        state.grants_9c,
        [0x2472, 0x0004_0000, 0x4100, 0x1440_0000, 8, 0]
    );
    assert_eq!(state.bits_100, [0x12, 0x0004_0000, 0, 0, 0, 0]);
    assert_eq!(
        (state.dword_144, state.dword_148, state.dword_14c),
        (0, 0, 8)
    );
    assert_eq!(
        state.values_150,
        [-1, -1, -1, -1, 0, 0, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(state.items_190, [0; 3]);
    let actor = arena.actor(vehicle).unwrap();
    assert_eq!((actor.health, actor.maximum_health), (0x1388, Some(0x1388)));
    // A repeat pickup is accepted silently.
    assert_eq!(
        arena
            .grant_player_pickup(player, 1, 0, tables)
            .unwrap()
            .message,
        None
    );
    // A first grenade pickup stocks row 8 and is announced in full.
    let outcome = arena.grant_player_pickup(player, 0x25, 4, tables).unwrap();
    assert!(outcome.accepted);
    assert_eq!(
        outcome.message,
        Some(PickupMessage {
            prefix_text: 0x139,
            name_text: 494,
            detail_text: Some(495),
        })
    );
    assert_eq!(arena.world_player(player).unwrap().values_150[8], 4);
}

#[test]
fn healing_pickup_excess_uses_live_wrapping_status_and_full_health_skips_it() {
    let (mut arena, player, vehicle) = spawned_first_world_vehicle();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let tables = PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active: false,
    };
    for (ce, expected_ce, expected_cc) in [(200, 0, 500), (1, 65337, 900)] {
        let actor = arena.actor_mut(vehicle).unwrap();
        actor.health = 4900;
        actor.word_ce = ce;
        actor.word_cc = 900;
        assert!(
            arena
                .grant_player_pickup(player, 4, 200, tables)
                .unwrap()
                .accepted
        );
        let actor = arena.actor(vehicle).unwrap();
        assert_eq!(
            (actor.health, actor.word_ce, actor.word_cc),
            (5000, expected_ce, expected_cc)
        );
    }
    assert!(
        !arena
            .grant_player_pickup(player, 4, 200, tables)
            .unwrap()
            .accepted
    );
    let actor = arena.actor(vehicle).unwrap();
    assert_eq!((actor.word_ce, actor.word_cc), (65337, 900));
}

/// 0046B2B0 against every grant both recordings made on a resolved vehicle
/// (respawns, crystals, counter pickups, the factory's Turbo charges and the
/// healing trigger), each from the captured grant mask, +100..+12C and hull.
/// The fields the grant's own cases write and its result match, the truck
/// grant and Infest02's two grenade pickups included.
#[test]
fn pickup_grants_match_both_recordings() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/pickup_grants_native.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let tables = PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active: false,
    };
    let (mut arena, player, vehicle) = spawned_first_world_vehicle();
    let (mut failures, mut unported, mut notices) = (Vec::new(), Vec::new(), 0);
    let mut commands = Vec::new();
    let calls = fixture["calls"].as_array().unwrap();
    for (index, call) in calls.iter().enumerate() {
        let word = |at: usize| call[at].as_u64().unwrap() as u32;
        // [ret, id, amount, mask in (6), +100..+19F in (40), hull in, EAX,
        //  mask out (6), +100..+19F out (40), hull out]
        let (input, output) = (3, 51);
        let state = arena.world_player_mut(player).unwrap();
        state.grants_9c = std::array::from_fn(|i| word(input + i));
        state.bits_100 = std::array::from_fn(|i| word(input + 6 + i));
        state.dword_118 = word(input + 12);
        state.dwords_120_128 = std::array::from_fn(|i| word(input + 14 + i));
        state.dword_12c = word(input + 17);
        let hull = word(49);
        {
            let actor = arena.actor_mut(vehicle).unwrap();
            actor.health = hull as u16;
            actor.maximum_health = Some((hull >> 16) as u16);
        }
        let id = word(1) as u16;
        match arena.grant_player_pickup(player, id, word(2) as u16 as i16, tables) {
            Err(PlayerAttachError::PickupCaseBoundary(case)) => unported.push(case),
            Err(error) => failures.push(format!("call {index} id {id:#x}: {error}")),
            Ok(outcome) => {
                notices += usize::from(outcome.notification.is_some());
                commands.extend(outcome.command);
                let state = arena.world_player(player).unwrap();
                let actor = arena.actor(vehicle).unwrap();
                let port: Vec<u32> = state
                    .grants_9c
                    .iter()
                    .chain(&state.bits_100)
                    .copied()
                    .chain([state.dword_118])
                    .chain(state.dwords_120_128)
                    .chain([
                        state.dword_12c,
                        u32::from(actor.health) | u32::from(actor.maximum_health.unwrap()) << 16,
                        u32::from(outcome.accepted),
                    ])
                    .collect();
                let native: Vec<u32> = (0..6)
                    .map(|i| word(output + i))
                    .chain((0..6).map(|i| word(output + 6 + i)))
                    .chain([word(output + 12)])
                    .chain((0..3).map(|i| word(output + 14 + i)))
                    .chain([word(output + 17), word(97), word(50) & 0xff])
                    .collect();
                if port != native {
                    failures.push(format!(
                        "call {index} id {id:#x}: port {port:x?} native {native:x?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} grants differ:\n{}",
        failures.len(),
        calls.len(),
        failures.join("\n")
    );
    assert!(unported.is_empty(), "{unported:x?}");
    // The truck (2) sets command 0x44, announced.
    assert_eq!(
        commands,
        [crate::object_runtime::PlayerCommandOperation::Set {
            raw_id: 0x44,
            silent: 0
        }]
    );
    // 004750C0 announced the first crystal and the first 0x0F of each run.
    assert_eq!(notices, 4);
}

fn pickup_tables_from_retail() -> ([u16; inf_formats::GRANT_TEXT_IDS], [i32; 16]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    (texts, limits)
}

/// Writes a grant's entry state: the grant mask, +100..+19F, the hull, the
/// behavior's bytes +0C..+11 and each row's count.
fn inject_grant_state(
    arena: &mut ObjectArena,
    player: PlayerId,
    vehicle: ObjectId,
    mask: [u32; 6],
    words: &[u32],
    hull: u32,
    bytes: &[u8],
    counts: &[u32],
) {
    let state = arena.world_player_mut(player).unwrap();
    state.grants_9c = mask;
    state.bits_100 = std::array::from_fn(|i| words[i]);
    state.dword_118 = words[6];
    state.dwords_120_128 = std::array::from_fn(|i| words[8 + i]);
    state.dword_12c = words[11];
    state.values_150 = std::array::from_fn(|i| words[20 + i] as i32);
    let actor = arena.actor_mut(vehicle).unwrap();
    actor.health = hull as u16;
    actor.maximum_health = Some((hull >> 16) as u16);
    let behavior = arena
        .derived_actor_mut(vehicle)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    [
        behavior.byte_0c,
        behavior.byte_0d,
        behavior.byte_0e,
        behavior.byte_0f,
        behavior.byte_10,
        behavior.byte_11,
    ] = std::array::from_fn(|i| bytes[i]);
    for (entry, &count) in behavior.entries_18.iter_mut().zip(counts) {
        entry[0] = count;
    }
}

fn vehicle_weapons(arena: &ObjectArena, vehicle: ObjectId) -> ([u8; 2], Vec<u32>) {
    let behavior = arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    (
        [behavior.byte_10, behavior.byte_11],
        behavior.entries_18.iter().map(|entry| entry[0]).collect(),
    )
}

/// Every 0046B2B0 call of Infest02 and Infest03 from its captured entry
/// state, with the behavior's selections and row counts: the respawns,
/// crystals and counters, and Infest02's two grenade pickups (0x25, four
/// each), the first selecting the grenades and both stocking row 8. The
/// player's values +150, the selections, the counts, the grant bits and the
/// result match, and each call's notice and message are the recorded ones.
#[test]
fn pickup_grants_match_the_recorded_weapon_rows() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/pickup_grants_weapons_native.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let (texts, limits) = pickup_tables_from_retail();
    let tables = PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active: false,
    };
    let (mut arena, player, vehicle) = spawned_first_world_vehicle();
    let words = |value: &serde_json::Value| -> Vec<u32> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_u64().unwrap() as u32)
            .collect()
    };
    let mut failures = Vec::new();
    let calls = fixture["calls"].as_array().unwrap();
    for (index, call) in calls.iter().enumerate() {
        let id = call["id"].as_u64().unwrap() as u16;
        let entry = &call["in"];
        let exit = &call["out"];
        let bytes: Vec<u8> = words(&entry["bb"]).iter().map(|&b| b as u8).collect();
        inject_grant_state(
            &mut arena,
            player,
            vehicle,
            std::array::from_fn(|i| words(&entry["g"])[i]),
            &words(&entry["p"]),
            entry["hull"].as_u64().unwrap() as u32,
            &bytes,
            &words(&entry["counts"]),
        );
        let amount = call["amount"].as_u64().unwrap() as u16 as i16;
        let outcome = match arena.grant_player_pickup(player, id, amount, tables) {
            Ok(outcome) => outcome,
            Err(error) => {
                failures.push(format!("call {index} id {id:#x}: {error}"));
                continue;
            }
        };
        let state = arena.world_player(player).unwrap();
        let native_words = words(&exit["p"]);
        let (selection, counts) = vehicle_weapons(&arena, vehicle);
        let native_bytes = words(&exit["bb"]);
        let nested = call["nested"].as_array().unwrap();
        let native_texts = |ret: u64| {
            nested
                .iter()
                .filter(|event| event[0] == "text" && event[1].as_u64() == Some(ret))
                .count()
        };
        let port = (
            outcome.accepted,
            state.grants_9c.to_vec(),
            state.bits_100.to_vec(),
            state.values_150.map(|value| value as u32).to_vec(),
            selection.to_vec(),
            counts,
            usize::from(outcome.notification.is_some()),
            usize::from(outcome.message.is_some()),
            outcome.full,
        );
        let native = (
            exit["eax"].as_u64().unwrap() & 0xff != 0,
            words(&exit["g"]),
            native_words[..6].to_vec(),
            native_words[20..36].to_vec(),
            vec![native_bytes[4] as u8, native_bytes[5] as u8],
            words(&exit["counts"]),
            native_texts(0x0047_5278),
            native_texts(0x0046_bbda),
            nested.iter().any(|event| event[0] == "full"),
        );
        if port != native {
            failures.push(format!(
                "call {index} id {id:#x}:\n  port   {port:x?}\n  native {native:x?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} grants differ:\n{}",
        failures.len(),
        calls.len(),
        failures.join("\n")
    );
    assert_eq!(calls.len(), 47);
}

/// One grant's visible requests in native order: nested grants whole, then
/// the full row's 004693C0, the command and the message.
fn flatten_pickup(outcome: &PickupOutcome, out: &mut Vec<String>) {
    for nested in &outcome.nested {
        flatten_pickup(nested, out);
    }
    if outcome.full {
        out.push("full".into());
    }
    if let Some(PlayerCommandOperation::Set { raw_id, silent }) = outcome.command {
        out.push(format!("command {raw_id:#x} {silent}"));
    }
    if let Some(message) = outcome.message {
        out.push(match message.detail_text {
            Some(detail) => format!(
                "message {} {} {}",
                message.prefix_text, message.name_text, detail
            ),
            None => format!("message {} {}", message.prefix_text, message.name_text),
        });
    }
}

/// 0046B2B0's weapon cases against the routine itself (Unicorn, every weapon
/// ID in seven states, `weapon_grants_oracle.json`): a first pickup, a
/// repeat, a full row, a full row whose notice the gate holds back, three
/// primaries (the IDIV), a negative amount, and rows past the limit. The
/// result, +100, the player's rows (as 0041A600 then clamps them), the
/// selections, the order of nested grants, full rows, commands and messages
/// (with or without the detail) and the gate 0054ED14 match.
#[test]
fn weapon_pickups_match_the_native_grant() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/weapon_grants_oracle.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let (texts, limits) = pickup_tables_from_retail();
    let tables = PickupTables {
        grants: &grants,
        texts: &texts,
        upgrade_limits: &limits,
        network_active: false,
    };
    let (mut arena, player, vehicle) = spawned_first_world_vehicle();
    let number = |value: &serde_json::Value| value.as_i64().unwrap();
    let list = |value: &serde_json::Value| -> Vec<i64> {
        value.as_array().unwrap().iter().map(number).collect()
    };
    let mut failures = Vec::new();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let id = number(&case["id"]) as u16;
        let label = case["label"].as_str().unwrap();
        let counts: Vec<u32> = list(&case["counts"]).iter().map(|&c| c as u32).collect();
        let mut words = vec![0u32; 40];
        for (i, word) in list(&case["bits"]).iter().enumerate() {
            words[i] = *word as u32;
        }
        words[20..36].copy_from_slice(&counts);
        let bytes: Vec<u8> = list(&case["behavior"]).iter().map(|&b| b as u8).collect();
        // Every ID already granted: the oracle's 004750C0 is a stub.
        inject_grant_state(
            &mut arena,
            player,
            vehicle,
            [u32::MAX; 6],
            &words,
            number(&case["hull"]) as u32,
            &bytes,
            &counts,
        );
        let amount = number(&case["amount"]) as u16 as i16;
        let outcome = match arena.grant_player_pickup(player, id, amount, tables) {
            Ok(outcome) => outcome,
            Err(error) => {
                failures.push(format!("{id:#x} {label}: {error}"));
                continue;
            }
        };
        let mut visible = Vec::new();
        flatten_pickup(&outcome, &mut visible);
        let mut gate = number(&case["gate"]) as u32;
        let clock = number(&case["clock"]) as u32;
        for _ in visible.iter().filter(|event| *event == "full") {
            if let Some(next) = crate::object_runtime::player::full_notice_gate(clock, gate) {
                gate = next;
            }
        }
        let state = arena.world_player(player).unwrap();
        let (selection, _) = vehicle_weapons(&arena, vehicle);
        let port = (
            outcome.accepted,
            state.bits_100.to_vec(),
            state.values_150.to_vec(),
            selection.to_vec(),
            visible,
            gate,
        );
        let native_values: Vec<i32> = list(&case["values_out"])
            .iter()
            .zip(&limits)
            .map(|(&value, &limit)| {
                let value = value as i32;
                if limit < value {
                    limit
                } else {
                    value
                }
            })
            .collect();
        let behavior = list(&case["behavior_out"]);
        let mut native_visible = Vec::new();
        for event in case["events"].as_array().unwrap() {
            match event[0].as_str().unwrap() {
                "full" => native_visible.push("full".to_string()),
                "command" => native_visible.push(format!(
                    "command {:#x} {}",
                    number(&event[1]),
                    number(&event[2])
                )),
                "format" => {
                    let ids: Vec<String> = event.as_array().unwrap()[2..]
                        .iter()
                        .map(|id| number(id).to_string())
                        .collect();
                    native_visible.push(format!("message {}", ids.join(" ")));
                }
                _ => {}
            }
        }
        let native = (
            number(&case["al"]) != 0,
            list(&case["bits_out"])
                .iter()
                .map(|&w| w as u32)
                .collect::<Vec<_>>(),
            native_values,
            vec![behavior[4] as u8, behavior[5] as u8],
            native_visible,
            number(&case["gate_out"]) as u32,
        );
        if port != native {
            failures.push(format!(
                "{id:#x} {label}:\n  port   {port:?}\n  native {native:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert_eq!(cases.len(), 161);
}

#[test]
fn pickup_message_formats_real_h0_before_the_live_suppression_gate() {
    let (mut arena, player, _) = spawned_first_world_vehicle();
    let runtime = retail_runtime();
    let message = PickupMessage {
        prefix_text: 0x139,
        name_text: 384,
        detail_text: Some(385),
    };
    assert_eq!(arena.player_message(player).unwrap().suppression_byte(), 1);
    assert!(matches!(
        arena.deliver_pickup_message(player, message, &runtime, None),
        Err(PickupMessageError::Format(
            crate::grant_text::GrantTextError::MissingClassificationBinding(_)
        ))
    ));
    // Supplied current CRT binding is independent from the real H0 strings.
    let data = crate::grant_text::PickupTextData {
        common: crate::grant_text::GrantTextData {
            null_substitute: None,
            pair_classification: Some(crate::grant_text::GrantTextPairClassification {
                colon_high: None,
                space_high: Some(0),
            }),
        },
        hyphen_high: Some(0),
    };
    arena
        .deliver_pickup_message(player, message, &runtime, Some(&data))
        .unwrap();
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    assert!(
        matches!(arena.deliver_pickup_message(player, message, &runtime, Some(&data)),
        Err(PickupMessageError::Message(PlayerMessageError::EnabledMessageBoundary(id))) if id == recipient)
    );
}

#[test]
fn the_completion_bonus_bit_gives_each_respawn_the_cranial_implanter() {
    let (mut arena, objects, _, mut start_state) =
        construct_first_world_offline_with_start(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    let handle88 = arena.actor(selector88).unwrap().handle;
    assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
    arena
        .run_loader_player_setup(false, 0, &mut factory)
        .unwrap();
    let mut rng = 0x0123_4567;
    let mut respawn = |arena: &mut ObjectArena, factory: &mut CameraFactoryInputs<'_>| {
        arena
            .respawn_player(
                player,
                PlayerRespawnInputs {
                    start: crate::world_start::WorldStartContext {
                        state: &mut start_state,
                        runtime: &runtime,
                        terrain: placements.terrain(),
                    },
                    world_header_18: placements.world().header_words_00_to_34[6],
                    vehicle_allocation: ActorConstructionRequest {
                        constructor_null:
                            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([0, 2, 0]),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: Some(crate::object_runtime::ActorTerrainSurface {
                                terrain: placements.terrain(),
                                plane_height_fixed: placements.plane_height_fixed(),
                            }),
                            coordinate_shift: 8,
                            clock: 245,
                            world_flags: Some(placements.world().flags),
                            sine_quarter: &UNUSED_SINE,
                        },
                    },
                    world_mask_limits: placements.world().actor_mask_limits(),
                    rng: &mut rng,
                    vehicle_table: &vehicle_table,
                    current_world_a0: 22,
                    // Bit 0x40 beside the gate's 0x80, which it ignores.
                    settings_84: 0xc0,
                    pickups: PickupTables {
                        grants: &grants,
                        texts: &texts,
                        upgrade_limits: &limits,
                        network_active: false,
                    },
                    equipment: None,
                    network: None,
                },
                factory,
            )
            .unwrap()
    };
    // 0046ADFB: after the ACAM (1) and the repairs (4), the Cranial
    // Implanter's pickup: 004750C0's notice ("Possession missiles", text
    // 538), then the pickup's message with its detail. The cannon (0x32)
    // follows silently, its record having no text.
    let first = respawn(&mut arena, &mut factory);
    assert_eq!(
        first.messages,
        [
            RespawnMessage::Pickup(PickupMessage {
                prefix_text: 0x139,
                name_text: 384,
                detail_text: Some(385),
            }),
            RespawnMessage::Notice(GrantNotification {
                id: 0x33,
                text: 538,
            }),
            RespawnMessage::Pickup(PickupMessage {
                prefix_text: 0x139,
                name_text: 538,
                detail_text: Some(539),
            }),
        ]
    );
    let state = arena.world_player(player).unwrap();
    assert_eq!(
        state.grants_9c,
        [0x2472, 0x000c_0000, 0x4100, 0x1440_0000, 8, 0]
    );
    assert_eq!(state.bits_100, [0x12, 0x000c_0000, 0, 0, 0, 0]);

    // The next respawn finds the implanter owned: no notice, and a repeat
    // pickup of it is silent.
    arena.actor_mut(first.vehicle.unwrap()).unwrap().word_c4 = 1;
    let again = respawn(&mut arena, &mut factory);
    assert_eq!(again.messages, []);
    let state = arena.world_player(player).unwrap();
    assert_eq!(state.bits_100, [0x12, 0x000c_0000, 0, 0, 0, 0]);
}

#[test]
fn vehicle_preflights_network_and_surface_and_its_guard_requires_the_table() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let surface = crate::object_runtime::ActorTerrainSurface {
        terrain: placements.terrain(),
        plane_height_fixed: placements.plane_height_fixed(),
    };
    for (network, with_surface, expect) in [(true, true, 0), (false, false, 1), (false, true, 2)] {
        let mut arena = arena();
        let mut listeners = CreationListeners::new();
        let mut effects = RejectCallbacks {
            calls: 0,
            register_before_failure: false,
        };
        let mut rng = 1;
        let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
        actor_inputs.allocation.allocation_position = Some([0, 2, 0]);
        actor_inputs.allocation.terrain.terrain = with_surface.then_some(surface);
        actor_inputs.allocation.terrain.world_flags = Some(placements.world().flags);
        actor_inputs.spawn_mode_1 = true;
        actor_inputs.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: [0, 0],
            rng: &mut rng,
        });
        let result = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: 0,
                context: BaseContext {
                    position_words: Some([0, 0, 0]),
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: network,
                actor: Some(actor_inputs),
            },
        );
        match expect {
            0 => assert!(matches!(
                result,
                Err(ObjectConstructionError::MissingVehicleNetworkSettings)
            )),
            1 => assert!(matches!(
                result,
                Err(ObjectConstructionError::ActorConstruction(
                    ActorConstructionError::Terrain(ActorTerrainError::MissingSurfaceForVehicle)
                ))
            )),
            _ => assert!(matches!(
                result,
                Err(ObjectConstructionError::MissingVehicleTable)
            )),
        }
        // Preflights never publish; the guard failure destroys the object.
        assert!(arena.is_empty());
        if expect < 2 {
            assert_eq!(rng, 1);
        }
    }
}

#[test]
fn offline_respawn_player_composes_the_recorded_0046a890() {
    let (mut arena, objects, _, mut start_state) =
        construct_first_world_offline_with_start(&[0; 4]);
    let player = arena.add_world_player([0x2472, 0, 0x4100, 0x1440_0000, 8, 0]);
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let math = math();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = || inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable()).unwrap();
    let grants = inf_formats::parse_grant_table(executable()).unwrap();
    let texts = inf_formats::parse_grant_texts(executable()).unwrap();
    let limits = inf_formats::parse_vehicle_upgrade_limits(executable()).unwrap();
    let mut listeners = CreationListeners::new();
    let mut effects = RejectCallbacks {
        calls: 0,
        register_before_failure: false,
    };
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut factory = CameraFactoryInputs {
        process: Some(&process),
        runtime: &runtime,
        math: &math,
        allocation: camera_allocation(),
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners: &mut listeners,
        listener_effects: &mut effects,
        rng: None,
    };
    let selector88 = objects
        .iter()
        .find(|(selector, _)| *selector == 88)
        .unwrap()
        .1;
    let handle88 = arena.actor(selector88).unwrap().handle;
    assert!(arena.attach_player(player, handle88, &mut factory).unwrap());
    assert_eq!(
        arena
            .run_loader_player_setup(false, 0, &mut factory)
            .unwrap(),
        LoaderPlayerSetup::Respawn(player)
    );
    let start = crate::world_start::WorldStartContext {
        state: &mut start_state,
        runtime: &runtime,
        terrain: placements.terrain(),
    };
    let mut rng = 0x0123_4567;
    let respawn = arena
        .respawn_player(
            player,
            PlayerRespawnInputs {
                start,
                world_header_18: placements.world().header_words_00_to_34[6],
                vehicle_allocation: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0, 2, 0]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: Some(crate::object_runtime::ActorTerrainSurface {
                            terrain: placements.terrain(),
                            plane_height_fixed: placements.plane_height_fixed(),
                        }),
                        coordinate_shift: 8,
                        clock: 245,
                        world_flags: Some(placements.world().flags),
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: placements.world().actor_mask_limits(),
                rng: &mut rng,
                vehicle_table: &vehicle_table,
                current_world_a0: 22,
                settings_84: 0,
                pickups: PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: false,
                },
                equipment: None,
                network: None,
            },
            &mut factory,
        )
        .unwrap();
    assert_eq!(
        respawn.messages,
        [RespawnMessage::Pickup(PickupMessage {
            prefix_text: 0x139,
            name_text: 384,
            detail_text: Some(385),
        })]
    );
    let vehicle = respawn.vehicle.unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    assert_eq!(handle, ActorHandle::from_raw(0x0ce0_0001));
    assert_eq!(arena.object(vehicle).unwrap().base.flags, 0x4810_6937);
    let state = arena.world_player(player).unwrap();
    assert_eq!(state.handle_d4, handle);
    assert_eq!(state.byte_ee, 1);
    assert_eq!(
        state.grants_9c,
        [0x2472, 0x0004_0000, 0x4100, 0x1440_0000, 8, 0]
    );
    assert_eq!(state.bits_100, [0x12, 0x0004_0000, 0, 0, 0, 0]);
    assert_eq!(
        (state.dword_144, state.dword_148, state.dword_14c),
        (0, 0, 8)
    );
    let camera = arena
        .resolve_actor_handle(arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        arena
            .actor_follow_target(arena.selector16(camera).unwrap().target_18c.unwrap())
            .unwrap()
            .handle_04,
        handle
    );

    // The vehicle dies (lifecycle 1; 00420B90 counts it in +11C). The held
    // camera's respawn passes argument 0, so a nonzero H14 +24 limit that the
    // losses reach refuses it, before any player write (the caller starts
    // the doom countdown); Level 1's limit is 0.
    arena.actor_mut(vehicle).unwrap().word_c4 = 1;
    arena.world_player_mut(player).unwrap().word_11c = 1;
    assert!(arena.respawn_loss_limit_reached(player, 1).unwrap());
    assert!(!arena.respawn_loss_limit_reached(player, 2).unwrap());
    assert!(!arena.respawn_loss_limit_reached(player, 0).unwrap());
    assert_eq!(arena.world_player(player).unwrap().handle_d4, handle);
    {
        let result = arena.respawn_player(
            player,
            PlayerRespawnInputs {
                start: crate::world_start::WorldStartContext {
                    state: &mut start_state,
                    runtime: &runtime,
                    terrain: placements.terrain(),
                },
                world_header_18: placements.world().header_words_00_to_34[6],
                vehicle_allocation: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0, 2, 0]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: Some(crate::object_runtime::ActorTerrainSurface {
                            terrain: placements.terrain(),
                            plane_height_fixed: placements.plane_height_fixed(),
                        }),
                        coordinate_shift: 8,
                        clock: 245,
                        world_flags: Some(placements.world().flags),
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: placements.world().actor_mask_limits(),
                rng: &mut rng,
                vehicle_table: &vehicle_table,
                current_world_a0: 22,
                settings_84: 0,
                pickups: PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: false,
                },
                equipment: None,
                network: None,
            },
            &mut factory,
        );
        // Over the wreck the respawn binds a fresh vehicle (0046CB50 moves
        // flag 0x10 to it) and the camera follows it.
        let fresh = result.unwrap().vehicle.unwrap();
        let fresh_handle = arena.actor(fresh).unwrap().handle;
        assert_ne!(fresh, vehicle);
        assert_eq!(arena.world_player(player).unwrap().handle_d4, fresh_handle);
        assert_eq!(arena.object(vehicle).unwrap().base.flags & 0x10, 0);
        assert_eq!(arena.object(fresh).unwrap().base.flags & 0x10, 0x10);
        assert_eq!(arena.actor(fresh).unwrap().word_c4, 3);
        assert_eq!(
            arena
                .actor_follow_target(arena.selector16(camera).unwrap().target_18c.unwrap())
                .unwrap()
                .handle_04,
            fresh_handle
        );
    }

    // Infest02's respawn at its save pad (+ED): the fresh vehicle stands at
    // the saved position, in the saved mode, and 004911F0 turns it to the
    // saved angles (`savespawn` query: counter 6 and the recorded rows).
    let fresh = arena.first_player_primary_actor().unwrap().unwrap();
    arena.actor_mut(fresh).unwrap().word_c4 = 1;
    let state = arena.world_player_mut(player).unwrap();
    state.byte_ed = 1;
    state.saved_e0 = Some([0x4eca, 0xd7, 0x81a6]);
    state.byte_ec = Some(1);
    state.dword_e8 = 0x3fb049dd;
    let saved = arena
        .respawn_player(
            player,
            PlayerRespawnInputs {
                start: crate::world_start::WorldStartContext {
                    state: &mut start_state,
                    runtime: &runtime,
                    terrain: placements.terrain(),
                },
                world_header_18: placements.world().header_words_00_to_34[6],
                vehicle_allocation: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0, 2, 0]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: Some(crate::object_runtime::ActorTerrainSurface {
                            terrain: placements.terrain(),
                            plane_height_fixed: placements.plane_height_fixed(),
                        }),
                        coordinate_shift: 8,
                        clock: 245,
                        world_flags: Some(placements.world().flags),
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: placements.world().actor_mask_limits(),
                rng: &mut rng,
                vehicle_table: &vehicle_table,
                current_world_a0: 22,
                settings_84: 0,
                pickups: PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: false,
                },
                equipment: None,
                network: None,
            },
            &mut factory,
        )
        .unwrap()
        .vehicle
        .unwrap();
    assert_eq!(
        arena.object(saved).unwrap().base.position_words,
        Some([0x4eca, 0xd7, 0x81a6])
    );
    assert_eq!(arena.world_player(player).unwrap().controls.dword_34, 1);
    assert_eq!(
        arena.actor(saved).unwrap().rotating.basis.words,
        [
            0x6, 0xbf7a3126, 0xbcf67c7a, 0xbe5661a8, 0xbbc97b2d, 0x3f7e502c, 0xbde99a39,
            0x3e587ba6, 0xbde1b1c3, 0xbf78972b,
        ]
    );
}
