use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::object_runtime::{
    construct_world_object, ActorComponentQueryMode, ActorComponentQueryRequest, ActorDamageTable,
    ActorFactoryInputs, ActorGrantDifferenceDelivery, ActorGrantDifferenceNotificationRequest,
    ActorHandle, ActorInteractionNotificationContext, ActorInteractionNotificationRequest,
    ActorRegistryConfig, ActorTerrainInputs, BaseContext, CreationListener,
    CreationListenerEffects, CreationListenerError, CreationListenerInterface, CreationListeners,
    FollowTargetRequest, ObjectClassKind, ObjectConstructionError, ObjectConstructionOutcome,
    ObjectConstructionRequest, ObjectInstanceCommands, ObjectList, PlayerActorLookupContext,
    AFTER_LOAD_FLAG, INITIALIZED_FLAG,
};
use crate::{Installation, ResidencyRequest, ResidentRuntime};
use inf_formats::native_basis::NativeBasisMath;
use inf_formats::{
    parse_constructor_asset_bank, ByteSpan, ConstructorPayloadView, FrontendExecutable,
    InstanceCommand, InstanceField, PlacementInstanceAssets,
};
use std::path::Path;

const SINE: [u16; 4096] = [0; 4096];

fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0x1234_5678,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn configured_arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn retail() -> (ResidentRuntime, FrontendAssets) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    // This real selection unloads leaf2, loads leaf1 and leaf48, and exposes
    // leaf1's original H13[34h]. It is not an additive Boot+leaf1 inventory.
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    (runtime, FrontendAssets::open(&root).unwrap())
}

#[derive(Default)]
struct RequiredListener {
    calls: Vec<ObjectId>,
}

impl CreationListenerEffects for RequiredListener {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push(object);
        assert_eq!(arena.object(object)?.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(arena.object(object)?.base.dirty_word, 0);
        assert_eq!(
            arena.actor(object)?.word_74,
            arena.object(object)?.base.word_28 >> 3
        );
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

fn factory_request<'a>(
    math: &'a NativeBasisMath,
    listeners: &'a mut CreationListeners,
    effects: &'a mut RequiredListener,
    suffix: Option<ObjectInstanceCommands<'a>>,
) -> ObjectConstructionRequest<'a> {
    ObjectConstructionRequest {
        resource_argument: 0xabcd_0034,
        context: BaseContext {
            position_words: Some([0xfffe, 0x8000, 1]),
            placement_index: u32::MAX,
        },
        suffix,
        command_allocation: None,
        network_active: false,
        actor: Some(ActorFactoryInputs {
            allocation: allocation(None),
            rotation_words: None,
            local_owner: 1,
            first_player_grants: None,
            grant_table: None,
            math,
            derived: None,
            listeners,
            listener_effects: effects,
            network_world_active: false,
            terrain: None,
            world_start: None,
            camera_settings: None,
            spawn_mode_1: false,
            network_handle: None,
            vehicle_table: None,
        }),
    }
}

fn constructed(outcome: Result<ObjectConstructionOutcome, ObjectConstructionError>) -> ObjectId {
    let ObjectConstructionOutcome::Constructed(object) = outcome.unwrap() else {
        panic!("constructor93 must be admitted")
    };
    object
}

/// Explicit supplied suffix bytes exercise the shared command consumer; the
/// original resident H13 record and its retained payload stay unchanged.
struct Commands {
    placement: PlacementInstanceAssets,
    bytes: Vec<u8>,
}

impl Commands {
    fn new(count: i32, inputs: &[(i32, &[u8])]) -> Self {
        let mut bytes = Vec::new();
        let commands = inputs
            .iter()
            .enumerate()
            .map(|(index, (code, operand))| {
                let offset = bytes.len();
                bytes.extend_from_slice(&code.to_le_bytes());
                bytes.extend_from_slice(operand);
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(0, 0),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: *code,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        Self {
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: count,
                },
                command_pointer_span: ByteSpan::new(0, 0),
                commands,
            },
            bytes,
        }
    }

    fn request(&self) -> ObjectInstanceCommands<'_> {
        ObjectInstanceCommands {
            placement: &self.placement,
            source: &self.bytes,
            main_payload: ByteSpan::new(0, self.bytes.len()),
            relocations: &[],
        }
    }
}

#[test]
fn constructor93_extends_the_actual_shared_constructor_without_extra_inputs() {
    let mut arena = configured_arena();
    let shared = arena.allocate_actor(allocation(None)).unwrap();
    let demo = arena.allocate_selector93(allocation(None)).unwrap();
    let mut expected = arena.actor(shared).unwrap().clone();
    expected.handle = arena.actor(demo).unwrap().handle;
    expected.extension = ActorExtension::Selector93(Box::new(Selector93State::new()));
    assert_eq!(arena.actor(demo).unwrap(), &expected);
    assert_eq!(
        arena.object(demo).unwrap().base,
        arena.object(shared).unwrap().base
    );
    assert_eq!(arena.selector93(demo).unwrap(), &Selector93State::new());
    assert_eq!(arena.object(demo).unwrap().base.position_words, None);
    assert_eq!(arena.actor_list(), [shared, demo]);
    assert_eq!(
        arena.selector93(shared),
        Err(ObjectError::NotSelector93(shared))
    );

    let mut unconfigured = ObjectArena::new();
    let before = format!("{unconfigured:?}");
    assert_eq!(
        unconfigured.allocate_selector93(allocation(None)),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert_eq!(format!("{unconfigured:?}"), before);
}

#[test]
fn shared_packed_position_copy_preserves_whole_actor_and_arena_custody() {
    let mut arena = configured_arena();
    let shared = arena.allocate_actor(allocation(None)).unwrap();
    let helper = arena.allocate_selector90(allocation(None)).unwrap();
    let demo = arena.allocate_selector93(allocation(None)).unwrap();
    {
        let ActorExtension::Selector93(state) = &mut arena.actor_mut(demo).unwrap().extension
        else {
            unreachable!()
        };
        state.dwords_138_140 = [0xfedc_ba98, 0x8000_0001, 0xffff_ffff];
        state.dword_144 = 0xdead_8001_u32 as i32;
    }
    for actor in [shared, helper, demo] {
        arena.actor_mut(actor).unwrap().words_8e_92 = [0x1234, 0x8000, 0xffff];
        let before = format!("{arena:?}");
        let mut expected = arena.object(actor).unwrap().clone();
        let old_position = expected.base.position_words;
        expected.base.position_words = Some([0x8001, 0xffff, 0x7fff]);
        arena
            .copy_actor_position(actor, [0x8001, 0xffff, 0x7fff])
            .unwrap();
        assert_eq!(arena.object(actor).unwrap(), &expected);
        // Normalize only the tested XYZ delta to compare every remaining
        // registry/cache/list/allocation/player/actor field, then restore it.
        arena.owned_mut(actor).unwrap().state.base.position_words = old_position;
        assert_eq!(format!("{arena:?}"), before);
        arena.owned_mut(actor).unwrap().state.base.position_words = expected.base.position_words;
    }

    let nonactor = arena.allocate(ObjectClassKind::Base);
    let mut foreign = configured_arena();
    let foreign_actor = foreign.allocate_selector93(allocation(None)).unwrap();
    let expired = arena.allocate_selector93(allocation(None)).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: expired,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let before = format!("{arena:?}");
    for (actor, error) in [
        (nonactor, ObjectError::NotActor(nonactor)),
        (foreign_actor, ObjectError::UnknownObject(foreign_actor)),
        (expired, ObjectError::UnknownObject(expired)),
    ] {
        assert_eq!(arena.copy_actor_position(actor, [1, 2, 3]), Err(error));
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn destruction_releases_real_type2_child_without_destroying_its_referent() {
    let mut arena = configured_arena();
    let referent = arena
        .allocate_actor(allocation(Some([11, 22, 33])))
        .unwrap();
    let handle = arena.actor(referent).unwrap().handle;
    let target = arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    assert_eq!(target.word_10, 2);
    let demo = arena
        .allocate_selector93(allocation(Some([44, 55, 66])))
        .unwrap();
    let demo_handle = arena.actor(demo).unwrap().handle;
    let target =
        arena.import_fixture_target(crate::object_runtime::NativeTarget::ActorFollow(target));
    let ActorExtension::Selector93(state) = &mut arena.actor_mut(demo).unwrap().extension else {
        unreachable!()
    };
    state.target_148 = Some(target);
    state.dword_144 = 0xdead_8001_u32 as i32;
    let referent_before = arena.object(referent).unwrap().clone();
    // 440070 releases the allocation without NULL-storing native +148.
    let expected_retired = arena.object(demo).unwrap().clone();
    assert_eq!(
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: demo,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
            })
            .unwrap(),
        expected_retired
    );
    assert_eq!(
        arena.actor_service_heap().state(target).unwrap(),
        crate::object_runtime::ActorServiceResourceState::Released,
    );
    assert_eq!(arena.object(referent).unwrap(), &referent_before);
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(referent));
    assert_eq!(arena.resolve_actor_handle(demo_handle).unwrap(), None);
    assert_eq!(arena.actor_list(), [referent]);
    for role in [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ] {
        assert!(arena.list(role).iter().all(|node| node.object != demo));
    }
}

#[test]
fn original_leaf1_slot34_admits93_model77_shared_payload_and_afterload() {
    let (runtime, assets) = retail();
    let resource = runtime.resource(13, 0x34).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    let index = usize::from(resource.registration.local_index);
    assert_eq!(bank.prefixes.entries[index].constructor_selector, 93);
    let asset = &bank.assets[index];
    let dispatch = asset.dispatch.unwrap();
    assert_eq!(
        (
            dispatch.constructor,
            dispatch.vtable,
            dispatch.context_initializer,
            dispatch.payload_initializer
        ),
        (0x0044_0010, 0x0052_28f0, 0x0047_a610, 0x0047_ad70)
    );
    assert_eq!(asset.model_binding.unwrap().raw_word, 77);
    let ConstructorPayloadView::SharedActor(payload) = &asset.payload else {
        panic!("leaf1 H13[34h] must use actual shared47AD70 payload")
    };
    let source_before = resource.overlay.bytes().to_vec();
    let revision = runtime.revision();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut arena = configured_arena();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredListener::default();
    let demo = constructed(construct_world_object(
        &mut arena,
        &runtime,
        factory_request(&math, &mut listeners, &mut effects, None),
    ));
    assert!(effects.calls.is_empty());
    let object = arena.object(demo).unwrap();
    let actor = arena.actor(demo).unwrap();
    assert_eq!(actor.rotating.visual.model_word, 77);
    assert_eq!(actor.health, payload.initial_health.value);
    assert_eq!(actor.maximum_health, Some(payload.initial_health.value));
    assert_eq!(actor.word_8c, payload.word_08.value);
    assert_eq!(actor.word_76, Some(payload.word_40.value));
    assert!(matches!(actor.damage_table, ActorDamageTable::Payload(_)));
    assert_eq!(actor.word_74, object.base.word_28 >> 3);
    assert_eq!(object.base.position_words, Some([0xfffe, 0x8000, 1]));
    assert_eq!(object.base.resource_word, 0x34);
    assert_eq!(object.base.placement_word, u16::MAX);
    assert_eq!(object.base.dirty_word, 0);
    assert_ne!(object.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(arena.selector93(demo).unwrap(), &Selector93State::new());
    let mut expected = object.clone();
    expected.base.flags |= AFTER_LOAD_FLAG;
    arena
        .initialize_after_load(demo, Default::default())
        .unwrap();
    assert_eq!(arena.object(demo).unwrap(), &expected);
    assert_eq!(
        arena.update(demo),
        Err(ObjectError::ActorUpdateBoundary(demo))
    );
    assert_eq!(arena.object(demo).unwrap(), &expected);
    // The actual shared +40 is retained for93. Unplaced local actors publish
    // dirty80 after listener traversal, independently of the constructor ID.
    let mut network = factory_request(&math, &mut listeners, &mut effects, None);
    network.network_active = true;
    let network_demo = constructed(construct_world_object(&mut arena, &runtime, network));
    assert_eq!(arena.object(network_demo).unwrap().base.dirty_word, 0x80);
    assert_ne!(
        arena.object(network_demo).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
    assert_eq!(runtime.revision(), revision);
    assert_eq!(resource.overlay.bytes(), source_before);
}

#[test]
fn missing_inputs_and_shared_command_failures_retain_existing_factory_cleanup() {
    let (runtime, assets) = retail();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut arena = configured_arena();
    let keeper = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let keeper_before = arena.object(keeper).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredListener::default();
    let mut missing = factory_request(&math, &mut listeners, &mut effects, None);
    missing.actor = None;
    assert!(matches!(
        construct_world_object(&mut arena, &runtime, missing),
        Err(ObjectConstructionError::MissingActorInputs { selector: 93 })
    ));
    assert_eq!(arena.actor_list(), [keeper]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let inactive = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &inactive,
            factory_request(&math, &mut listeners, &mut effects, None)
        ),
        Err(ObjectConstructionError::NotResident {
            resource_argument: 0xabcd_0034
        })
    ));
    assert_eq!(arena.actor_list(), [keeper]);
    let mut unconfigured = ObjectArena::new();
    assert!(matches!(
        construct_world_object(
            &mut unconfigured,
            &runtime,
            factory_request(&math, &mut listeners, &mut effects, None)
        ),
        Err(ObjectConstructionError::ActorConstruction(
            ActorConstructionError::MissingRegistry
        ))
    ));
    // The source acquisition cache may retain its real allocation, but no
    // object, actor handle or membership was published by a failed ctor.
    assert!(unconfigured.is_empty());
    assert!(unconfigured.actor_list().is_empty());

    let health = 123_u32.to_le_bytes();
    // Command13 has no operand: actual absent-first-player branch skips it.
    let accepted = Commands::new(2, &[(8, &health), (13, &[])]);
    let demo = constructed(construct_world_object(
        &mut arena,
        &runtime,
        factory_request(
            &math,
            &mut listeners,
            &mut effects,
            Some(accepted.request()),
        ),
    ));
    assert_eq!(arena.actor(demo).unwrap().health, 123);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: demo,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let live_before = arena.actor_list().to_vec();
    let reject = Commands::new(2, &[(8, &health), (16, &[])]);
    assert_eq!(
        construct_world_object(
            &mut arena,
            &runtime,
            factory_request(&math, &mut listeners, &mut effects, Some(reject.request()))
        )
        .unwrap(),
        ObjectConstructionOutcome::Rejected {
            command_index: 1,
            code: 16
        }
    );
    assert_eq!(arena.actor_list(), live_before);
    let missing_command = Commands::new(2, &[(8, &health)]);
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &runtime,
            factory_request(
                &math,
                &mut listeners,
                &mut effects,
                Some(missing_command.request())
            )
        ),
        Err(ObjectConstructionError::MissingCommands {
            count: 2,
            available: 1
        })
    ));
    assert_eq!(arena.actor_list(), live_before);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert!(effects.calls.is_empty());
}

#[test]
fn reached_creation_listener_error_cleans93_after_shared_completion() {
    let (runtime, assets) = retail();
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut arena = configured_arena();
    let owner = arena.allocate_actor(allocation(None)).unwrap();
    let before = arena.object(owner).unwrap().clone();
    let mut listeners = CreationListeners::new();
    listeners.register(owner, CreationListenerInterface::Selector78);
    let mut effects = RequiredListener::default();
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &runtime,
            factory_request(&math, &mut listeners, &mut effects, None)
        ),
        Err(ObjectConstructionError::CreationListener(
            CreationListenerError::UnsupportedInterface(CreationListenerInterface::Selector78)
        ))
    ));
    assert_eq!(effects.calls.len(), 1);
    assert_eq!(arena.actor_list(), [owner]);
    assert_eq!(arena.object(owner).unwrap(), &before);
    assert!(matches!(
        arena.object(effects.calls[0]),
        Err(ObjectError::UnknownObject(_))
    ));
}

#[test]
fn table_bound_queries_modes_and_notifications_preserve93_fields() {
    let (_, assets) = retail();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (offset, body) in [
        (0x18, 0x0040_4610),
        (0x20, 0x0040_1320),
        (0x24, 0x0044_0170),
        (0x34, 0x0040_1310),
        (0x48, 0x0040_26b0),
        (0x4c, 0x0047_aaf0),
        (0x50, 0x0040_1300),
        (0x54, 0x0040_1300),
        (0x58, 0x0040_1300),
        (0x5c, 0x0040_1300),
        (0x60, 0x0040_1300),
        (0x64, 0x0040_1300),
        (0x70, 0x0040_1300),
        (0x7c, 0x0040_1300),
        (0x80, 0x0040_74d0),
        (0x98, 0x0040_45f0),
        (0x9c, 0x0040_26d0),
        (0xb8, 0x0040_1300),
    ] {
        assert_eq!(executable.word(0x0052_28f0 + offset).unwrap(), body);
    }
    let mut arena = configured_arena();
    let demo = arena.allocate_selector93(allocation(None)).unwrap();
    let before = format!("{arena:?}");
    for mode in [
        ActorComponentQueryMode::Self50,
        ActorComponentQueryMode::Interaction54,
    ] {
        assert_eq!(
            arena
                .query_actor_component(ActorComponentQueryRequest { actor: demo, mode })
                .unwrap(),
            None
        );
    }
    assert_eq!(arena.actor_derived_component(demo).unwrap(), None);
    assert_eq!(arena.camera_derived_component(demo).unwrap(), None);
    assert!(!arena.actor_behavior_presence_7c(demo).unwrap());
    for mode in [0, 1, 0x80, 0xff] {
        assert_eq!(arena.apply_actor_view_mode(demo, mode).unwrap(), mode == 0);
    }
    assert_eq!(
        arena
            .notify_actor_grant_difference(
                ActorGrantDifferenceNotificationRequest {
                    delivery: ActorGrantDifferenceDelivery::Actor(demo)
                },
                PlayerActorLookupContext { process: None }
            )
            .unwrap(),
        0
    );
    arena
        .notify_actor_interaction(
            ActorInteractionNotificationRequest {
                candidate: demo,
                source: None,
                distance_squared: u32::MAX,
            },
            ActorInteractionNotificationContext {
                animation_clock: None,
            },
        )
        .unwrap();
    assert_eq!(
        arena.selector88(demo),
        Err(ObjectError::NotSelector88(demo))
    );
    assert_eq!(format!("{arena:?}"), before);
}
