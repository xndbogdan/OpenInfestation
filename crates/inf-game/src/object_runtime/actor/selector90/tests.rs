use super::*;
use crate::object_runtime::{
    construct_world_object, ActorFactoryInputs, ActorRegistryConfig, ActorTerrainInputs,
    BaseContext, CreationListener, CreationListenerEffects, CreationListenerError,
    CreationListenerInterface, CreationListeners, ObjectClassKind, ObjectConstructionOutcome,
    ObjectConstructionRequest, ObjectInstanceCommands, INITIALIZED_FLAG,
};
use crate::object_runtime::{ActorHandle, FollowTargetRequest};
use inf_formats::native_basis::{NativeBasis, NativeBasisMath};
use inf_formats::{
    parse_constructor_asset_bank, ByteSpan, ConstructorPayloadView, InstanceCommand, InstanceField,
    PlacementInstanceAssets,
};

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
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn route(arena: &mut ObjectArena, points: &[[u16; 3]], closed: bool) -> Vec<ObjectId> {
    let nodes: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(index, &point)| {
            let node = arena.allocate(ObjectClassKind::RouteNode);
            arena.owned_mut(node).unwrap().state.base.position_words = Some(point);
            let stored = arena.route_mut(node).unwrap();
            stored.identity_word = index as u16;
            stored.predecessor_word = index.saturating_sub(1) as u16;
            stored.flags = u8::from(closed);
            node
        })
        .collect();
    arena.finalize_routes().unwrap();
    nodes
}

fn moving(arena: &mut ObjectArena, position: [u16; 3], route: ObjectId, speed: i32) -> ObjectId {
    let helper = arena
        .allocate_selector90(allocation(Some(position)))
        .unwrap();
    arena
        .replace_selector90_route(
            helper,
            Some(route),
            crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap();
    arena
        .write_selector90_word(helper, Selector90Word::Speed13c, speed)
        .unwrap();
    arena
        .write_selector90_byte(helper, Selector90Byte::Arrival140, 0)
        .unwrap();
    helper
}

fn tick(arena: &mut ObjectArena, helper: ObjectId, microseconds: u32, mode: Selector90MotionMode) {
    arena
        .tick_selector90(
            helper,
            Selector90TickRequest {
                microseconds,
                motion_mode: mode,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
            },
        )
        .unwrap();
}

#[test]
fn constructor_and_replacement_preserve_distinct_owned_targets() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[1000, 0, 0]], false);
    let helper = arena.allocate_selector90(allocation(Some([0; 3]))).unwrap();
    assert_eq!(arena.selector90(helper).unwrap(), &Selector90State::new());
    arena
        .replace_selector90_route(
            helper,
            Some(nodes[0]),
            crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap();
    assert_eq!(arena.selector90(helper).unwrap().byte_140, 1);
    let target = arena
        .object_position_target_data(arena.selector90(helper).unwrap().route_150.unwrap())
        .unwrap();
    assert_eq!(target.object_14, nodes[0]);
    assert_eq!(target.focus_08, [1000, 0, 0]);
    assert_eq!(target.word_10, 0x10);
    assert_eq!(target.handle_04, ActorHandle::from_raw(0));
    let actor = arena.allocate_actor(allocation(Some([0, 0, 100]))).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let tracked = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
        .unwrap()
        .unwrap();
    arena
        .replace_selector90_target(helper, Some(tracked))
        .unwrap();
    arena
        .replace_selector90_route(
            helper,
            None,
            crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap();
    assert!(arena.selector90(helper).unwrap().route_150.is_none());
    assert_eq!(arena.selector90(helper).unwrap().target_154, Some(tracked));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: helper,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.object(nodes[0]).is_ok());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(actor));
}

#[test]
fn route_target_replacement_validates_domain_after_discarding_old_target() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[1000, 0, 0]], false);
    let helper = moving(&mut arena, [0; 3], nodes[0], 1000);
    arena.selector90_mut(helper).unwrap().remainders_144_14c = [1, 2, 3];
    let actor = arena.allocate_actor(allocation(None)).unwrap();
    assert_eq!(
        arena.replace_selector90_route(
            helper,
            Some(actor),
            crate::object_runtime::WeightedTargetConstructorNull::ActorRegistry
        ),
        Err(crate::object_runtime::NativeTargetError::Object(
            ObjectError::TargetObjectPositionUnknown(actor)
        ))
    );
    let state = arena.selector90(helper).unwrap();
    assert!(matches!(
        arena.actor_service_heap().state(state.route_150.unwrap()),
        Ok(crate::object_runtime::ActorServiceResourceState::Released)
    ));
    assert_eq!(state.byte_140, 0);
    assert_eq!(state.remainders_144_14c, [1, 2, 3]);
    assert!(arena.object(nodes[0]).is_ok());
    assert!(arena.object(actor).is_ok());
}

#[test]
fn route_advance_keeps_changed_target_identity_before_unwritten_xyz_error() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[0; 3], [1000, 0, 0]], false);
    let helper = moving(&mut arena, [0; 3], nodes[0], 1000);
    arena.selector90_mut(helper).unwrap().remainders_144_14c = [41, 42, 43];
    arena.actor_mut(helper).unwrap().words_8e_92 = [51, 52, 53];
    arena.owned_mut(nodes[1]).unwrap().state.base.position_words = None;
    assert_eq!(
        arena.tick_selector90(
            helper,
            Selector90TickRequest {
                microseconds: 1000,
                motion_mode: Selector90MotionMode::WholeUnits,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
            }
        ),
        Err(Selector90Error::Object(
            ObjectError::TargetObjectPositionUnknown(nodes[1])
        ))
    );
    let state = arena.selector90(helper).unwrap();
    let target = arena
        .object_position_target_data(state.route_150.unwrap())
        .unwrap();
    assert_eq!(target.object_14, nodes[1]);
    assert_eq!(target.focus_08, [0; 3]);
    assert_eq!(state.byte_140, 0);
    assert_eq!(state.remainders_144_14c, [41, 42, 43]);
    assert_eq!(arena.actor(helper).unwrap().words_8e_92, [51, 52, 53]);
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([0; 3])
    );
}

#[test]
fn submillisecond_route_rejection_sets_arrival_and_clears_velocity() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[10000, 0, 0]], false);
    let helper = moving(&mut arena, [0; 3], nodes[0], 1000);
    arena.actor_mut(helper).unwrap().words_8e_92 = [1, 2, 3];
    tick(
        &mut arena,
        helper,
        999,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([0; 3])
    );
    assert_eq!(arena.selector90(helper).unwrap().byte_140, 1);
    assert_eq!(arena.actor(helper).unwrap().words_8e_92, [0; 3]);
    tick(
        &mut arena,
        helper,
        1_000_000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([0; 3])
    );
}

#[test]
fn fractional_negative_motion_preserves_signed_remainders() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[0; 3]], false);
    let helper = moving(&mut arena, [10000, 0, 0], nodes[0], 32);
    for _ in 0..31 {
        tick(
            &mut arena,
            helper,
            1000,
            Selector90MotionMode::FractionalThousandths,
        );
    }
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([10000, 0, 0])
    );
    assert_eq!(
        arena.selector90(helper).unwrap().remainders_144_14c,
        [-992, 0, 0]
    );
    tick(
        &mut arena,
        helper,
        1000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([9999, 0, 0])
    );
    assert_eq!(
        arena.selector90(helper).unwrap().remainders_144_14c,
        [-24, 0, 0]
    );
    tick(&mut arena, helper, 1000, Selector90MotionMode::WholeUnits);
    assert_eq!(
        arena.selector90(helper).unwrap().remainders_144_14c,
        [-24, 0, 0]
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([9999, 0, 0])
    );
}

#[test]
fn route_selection_and_arrival_follow_live_nodes_not_cached_points() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[0; 3], [1000, 0, 0]], false);
    let helper = moving(&mut arena, [0; 3], nodes[0], 1000);
    // A cached target point is not the route position consumed by 004627F0.
    arena.owned_mut(nodes[1]).unwrap().state.base.position_words = Some([1200, 0, 0]);
    tick(
        &mut arena,
        helper,
        1_000_000,
        Selector90MotionMode::FractionalThousandths,
    );
    let state = arena.selector90(helper).unwrap();
    assert_eq!(
        arena
            .object_position_target_data(state.route_150.unwrap())
            .unwrap()
            .object_14,
        nodes[1]
    );
    assert_eq!(
        arena
            .object_position_target_data(state.route_150.unwrap())
            .unwrap()
            .focus_08,
        [1200, 0, 0]
    );
    assert_eq!(state.byte_140, 0);
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([1000, 0, 0])
    );
    // At the open tail speed is capped to remaining distance for this tick.
    tick(
        &mut arena,
        helper,
        1_000_000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([1200, 0, 0])
    );
    assert_eq!(arena.selector90(helper).unwrap().byte_140, 0);
    tick(
        &mut arena,
        helper,
        1000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(arena.selector90(helper).unwrap().byte_140, 1);
    assert_eq!(arena.selector90(helper).unwrap().remainders_144_14c, [0; 3]);
}

#[test]
fn successor_squared_distance_and_approach_speed_use_native_comparisons() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[0; 3], [1000, 0, 0]], false);
    let helper = moving(&mut arena, [400, 0, 0], nodes[0], 0);
    tick(
        &mut arena,
        helper,
        1000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena
            .object_position_target_data(arena.selector90(helper).unwrap().route_150.unwrap())
            .unwrap()
            .object_14,
        nodes[1]
    );
    arena.owned_mut(helper).unwrap().state.base.position_words = Some([800, 0, 0]);
    arena
        .write_selector90_word(helper, Selector90Word::Speed13c, 1000)
        .unwrap();
    tick(
        &mut arena,
        helper,
        1000,
        Selector90MotionMode::FractionalThousandths,
    );
    // floor(1000 * 200 / 256), limited to the open tail, not rounded.
    assert_eq!(
        arena.selector90(helper).unwrap().remainders_144_14c,
        [781, 0, 0]
    );
}

#[test]
fn wrapped_unsigned_speed_product_is_preserved() {
    let mut arena = arena();
    let nodes = route(&mut arena, &[[10000, 0, 0]], false);
    let helper = moving(&mut arena, [0; 3], nodes[0], -1);
    tick(
        &mut arena,
        helper,
        1000,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.object(helper).unwrap().base.position_words,
        Some([4294, 0, 0])
    );
    assert_eq!(
        arena.selector90(helper).unwrap().remainders_144_14c,
        [966, 0, 0]
    );
}

#[test]
fn facing_refresh_runs_when_stopped_and_tracks_target_handle() {
    let mut arena = arena();
    let helper = arena.allocate_selector90(allocation(Some([0; 3]))).unwrap();
    let actor = arena.allocate_actor(allocation(Some([100, 0, 0]))).unwrap();
    let handle = arena.actor(actor).unwrap().handle;
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
        .unwrap()
        .unwrap();
    arena
        .replace_selector90_target(helper, Some(target))
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 0, 100]);
    tick(
        &mut arena,
        helper,
        1,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena.actor(helper).unwrap().rotating.basis,
        NativeBasis::INITIAL
    );
    let target = arena
        .actor_follow_target(arena.selector90(helper).unwrap().target_154.unwrap())
        .unwrap();
    assert_eq!(target.focus_08, Some([0, 0, 100]));
    assert_eq!(target.focus_14, Some([100, 0, 0]));
    assert_eq!(target.velocity_1a, Some([0; 3]));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    tick(
        &mut arena,
        helper,
        1,
        Selector90MotionMode::FractionalThousandths,
    );
    assert_eq!(
        arena
            .actor_follow_target(arena.selector90(helper).unwrap().target_154.unwrap())
            .unwrap()
            .focus_08,
        target.focus_08
    );
}

#[test]
fn stopped_facing_preserves_spilled_signed_word_normal_bits() {
    for (point, expected) in [
        ([1, 2, 3], [0x3e88_d677, 0x3f08_d677, 0x3f4d_41b2]),
        (
            [u16::MAX, u16::MAX - 1, u16::MAX - 2],
            [0xbe88_d677, 0xbf08_d677, 0xbf4d_41b2],
        ),
        ([0; 3], [1.0f32.to_bits(), 0, 0]),
    ] {
        let mut arena = arena();
        let helper = arena.allocate_selector90(allocation(Some([0; 3]))).unwrap();
        let actor = arena.allocate_actor(allocation(Some(point))).unwrap();
        let handle = arena.actor(actor).unwrap().handle;
        let target = arena
            .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
            .unwrap()
            .unwrap();
        arena
            .replace_selector90_target(helper, Some(target))
            .unwrap();
        tick(
            &mut arena,
            helper,
            1,
            Selector90MotionMode::FractionalThousandths,
        );
        assert_eq!(
            &arena.actor(helper).unwrap().rotating.basis.words[7..10],
            &expected
        );
        assert_eq!(
            arena
                .actor_follow_target(arena.selector90(helper).unwrap().target_154.unwrap())
                .unwrap()
                .focus_08,
            Some(point)
        );
        assert_eq!(arena.selector90(helper).unwrap().byte_140, 1);
    }
}

#[test]
fn unknown_target_focus_fails_at_facing_after_native_arrival_write() {
    let mut arena = arena();
    let helper = arena.allocate_selector90(allocation(Some([0; 3]))).unwrap();
    let target = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(ActorHandle::from_raw(0)))
        .unwrap()
        .unwrap();
    arena
        .replace_selector90_target(helper, Some(target))
        .unwrap();
    assert_eq!(
        arena.tick_selector90(
            helper,
            Selector90TickRequest {
                microseconds: 1000,
                motion_mode: Selector90MotionMode::FractionalThousandths,
                settings_7c: 0,
                up: [0.0, 1.0, 0.0],
            }
        ),
        Err(Selector90Error::TargetFocusUnknown(helper))
    );
    assert_eq!(arena.selector90(helper).unwrap().byte_140, 1);
}

#[derive(Default)]
struct RecordingEffects(Vec<ObjectId>);

impl CreationListenerEffects for RecordingEffects {
    fn invoke(
        &mut self,
        _listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.0.push(actor);
        Ok(())
    }
}

fn factory_request<'a>(
    math: &'a NativeBasisMath,
    listeners: &'a mut CreationListeners,
    effects: &'a mut RecordingEffects,
    suffix: Option<ObjectInstanceCommands<'a>>,
) -> ObjectConstructionRequest<'a> {
    ObjectConstructionRequest {
        resource_argument: 142,
        context: BaseContext {
            position_words: None,
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

#[test]
fn retail_resource142_uses_canonical_selector90_admission_commands_and_listener_custody() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime =
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    runtime.apply(crate::ResidencyRequest::Boot).unwrap();
    runtime
        .apply(crate::ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
        .apply(crate::ResidencyRequest::Activate { level: 48 })
        .unwrap();
    runtime
        .apply(crate::ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let resource = runtime.resource(13, 142).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    let index = usize::from(resource.registration.local_index);
    assert_eq!(bank.prefixes.entries[index].constructor_selector, 90);
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[index].payload else {
        panic!("resource142 must use 0047AD70 shared payload");
    };
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (offset, function) in [
        (0x18, 0x0040_4610),
        (0x24, 0x0046_2ba0),
        (0x34, 0x0040_1310),
        (0x48, 0x0040_26b0),
        (0x58, 0x0040_1300),
    ] {
        assert_eq!(executable.word(0x0052_6b50 + offset).unwrap(), function);
    }
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut arena = arena();
    let owner = arena.allocate_actor(allocation(None)).unwrap();
    let mut listeners = CreationListeners::new();
    listeners.register(owner, CreationListenerInterface::Selector18);
    let mut effects = RecordingEffects::default();
    let result = construct_world_object(
        &mut arena,
        &runtime,
        factory_request(&math, &mut listeners, &mut effects, None),
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(helper) = result else {
        panic!("helper admitted")
    };
    assert_eq!(arena.selector90(helper).unwrap(), &Selector90State::new());
    assert_eq!(
        arena.actor(helper).unwrap().health,
        payload.initial_health.value
    );
    assert_eq!(
        arena.actor(helper).unwrap().word_74,
        arena.object(helper).unwrap().base.word_28 >> 3
    );
    assert_eq!(arena.object(helper).unwrap().base.resource_word, 142);
    assert_eq!(arena.object(helper).unwrap().base.position_words, None);
    assert_ne!(
        arena.object(helper).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
    assert_eq!(effects.0, [helper]);

    // Shared virtual +04 accepts code8, then rejects16 without reading its
    // absent operand. Deleting rejection retires the real handle/list nodes.
    let bytes = [
        8_u32.to_le_bytes(),
        123_u32.to_le_bytes(),
        16_u32.to_le_bytes(),
    ]
    .concat();
    let commands = [0_u32, 8]
        .into_iter()
        .enumerate()
        .map(|(index, offset)| InstanceCommand {
            index: index as u32,
            pointer_span: ByteSpan::new(0, 0),
            allocation_offset: offset,
            code: InstanceField {
                span: ByteSpan::new(offset as usize, offset as usize + 4),
                value: if index == 0 { 8 } else { 16 },
            },
            payload_offset: offset + 4,
        })
        .collect();
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 2,
        },
        command_pointer_span: ByteSpan::new(0, 0),
        commands,
    };
    let suffix = ObjectInstanceCommands {
        placement: &placement,
        source: &bytes,
        main_payload: ByteSpan::new(0, bytes.len()),
        relocations: &[],
    };
    let live_before = arena.actor_list().to_vec();
    let result = construct_world_object(
        &mut arena,
        &runtime,
        factory_request(&math, &mut listeners, &mut effects, Some(suffix)),
    )
    .unwrap();
    assert_eq!(
        result,
        ObjectConstructionOutcome::Rejected {
            command_index: 1,
            code: 16
        }
    );
    assert_eq!(arena.actor_list(), live_before);
    assert_eq!(effects.0, [helper]);
}
