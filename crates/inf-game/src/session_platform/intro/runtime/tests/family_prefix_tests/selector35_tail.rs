//! Actual resident H13/81 construction and the production flag1 callback.
//! Descriptor timers/target membership are supplied scheduling inputs; they
//! make no claim about allocator carry or the retail controller's cadence.
use super::*;
use crate::{
    object_runtime::{
        ActorChildResource, ActorExtension, ActorHandle, BaseContext, CameraSettings,
        DerivedActorClass, Family428650Class, FixedPositionTarget, NativeTarget,
        Selector35ClearanceHost, Selector35TailError,
    },
    process_sound::ProcessSoundRecordState,
    session_platform::{
        world_actor_factory::{
            RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
            WorldCameraFactoryContext, WorldCameraInputs, WorldFactoryPosition,
        },
        world_actor_update::{WorldSelector35ClearanceHost, WorldSelector35TailError},
    },
    world_furniture_effect::resident::resident_furniture_model_radius,
};

fn construct(
    world: &mut WorldSession,
    owners: &TickOwners,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    position: [u16; 3],
) -> ObjectId {
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut effects = RequiredListenerEffects;
    assert_eq!(
        crate::object_runtime::resident_constructor_selector(&mut world.arena, runtime, 81)
            .unwrap(),
        35,
        "H13 resource identity is distinct from an arena slot"
    );
    let mut host = WorldActorChildHost {
        residency: runtime,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        process: &owners.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: owners.clock.value(),
        sine: &assets.sine_table().values,
        math: &math,
        camera_settings: Some(CameraSettings {
            first_person: 0,
            camera_style: 0,
        }),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut effects,
    };
    let actor = host
        .create_object(
            &mut world.arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(81),
                position: WorldFactoryPosition::Local(position),
                spawn_mode_1: false,
                network_handle: None,
                initialization:
                    crate::session_platform::world_actor_factory::WorldFactoryInitialization::Bare,
            },
            &mut world.rng,
        )
        .unwrap()
        .unwrap();
    let state = world.arena.selector35_actor(actor).unwrap();
    assert!(state.retained_23c.is_some());
    assert!(state.table_270.is_some());
    assert!(state.word_258.is_some());
    actor
}

fn schedule(world: &mut WorldSession, actor: ObjectId, focus: Option<[u16; 3]>) {
    let target = world
        .arena
        .import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: ActorHandle::from_raw(0),
            focus_08: focus,
            word_10: 8,
        }));
    world
        .arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            state.word_c4 = 4;
            state.word_cc = 0;
            state.word_ce = 0;
            state.words_8e_92 = [1024, 0, 0];
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived")
            };
            derived.word_144 = 0;
            derived.target_148 = Some(target);
            let DerivedActorClass::Family428650(family) = &mut derived.class else {
                panic!("family")
            };
            assert!(matches!(family.class, Family428650Class::Selector35(_)));
            family.word_184 = -1;
            family.word_21c = 1_000_000;
            family.words_188_19c[0] = Some(0);
            family.words_188_19c[5] = Some(1_000_000);
            family.words_204_208[1] = Some(0);
            family.word_22e = 0;
            family.byte_22c = 0;
        })
        .unwrap();
    only_callbacks(world, &[actor]);
    pause_controller(world);
}

fn camera_at(
    world: &mut WorldSession,
    owners: &TickOwners,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    position: [u16; 3],
) {
    let mut context = WorldCameraFactoryContext::new(WorldCameraInputs {
        residency: runtime,
        sine_quarter: &assets.sine_table().values,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        process: Some(&owners.factory_process),
        animation_clock: owners.clock.value(),
    });
    let mut factory = context.factory(&world.terrain, world.world_flags, &mut world.listeners);
    let camera = world.arena.world_camera(&mut factory).unwrap().unwrap();
    let placement_index = u32::from(world.arena.object(camera).unwrap().base.placement_word);
    world
        .arena
        .initialize_context(
            camera,
            BaseContext {
                position_words: Some(position),
                placement_index,
            },
        )
        .unwrap();
}

#[test]
fn selector35_production_tail_uses_live_terrain_motion_and_persistent_sound() {
    let (runtime, assets, mut world, _maps) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let (grid, placement, radius) = world
        .terrain
        .buckets()
        .iter()
        .find_map(|bucket| {
            bucket.records.iter().find_map(|record| {
                let radius =
                    resident_furniture_model_radius(&runtime, record.resource_lookup).ok()??;
                (radius != 0).then_some((
                    [record.x, bucket.y],
                    record.resource_lookup.raw(),
                    radius,
                ))
            })
        })
        .expect("loaded H12 sparse furniture has resident H11/H8 model radius");
    let position = [grid[0].wrapping_mul(128), 20_000, grid[1].wrapping_mul(128)];
    let actor = construct(&mut world, &owners, &runtime, &assets, position);
    // This is the same reached H11/H8 resolver lent to the clearance grid.
    let mut clearance = WorldSelector35ClearanceHost {
        residency: &runtime,
        source: &world.terrain_source,
        terrain: &world.terrain,
        furniture: &owners.furniture_query,
    };
    assert_eq!(
        clearance
            .furniture_model_radius(&mut world.arena, actor, placement)
            .unwrap(),
        Some(radius)
    );
    schedule(
        &mut world,
        actor,
        Some([position[0], position[1], position[2].wrapping_add(2000)]),
    );
    camera_at(&mut world, &owners, &runtime, &assets, position);
    let rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 33_333);
    assert!(
        world.actor_updates.family_prefix_stops.is_empty(),
        "{:?}",
        world.actor_updates.family_prefix_stops
    );
    assert_eq!(world.rng, rng, "clearance, motion and sound consume no RNG");
    assert_eq!(world.arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(
        world.arena.derived_actor(actor).unwrap().word_14c,
        Some(2000)
    );
    let moved = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_ne!(moved, position, "actual47B1E0 publishes the moved position");
    assert_ne!(
        world.arena.object(actor).unwrap().base.flags & 0x0400_0000,
        0
    );
    let sound = world
        .arena
        .derived_actor(actor)
        .unwrap()
        .sound_168
        .expect("actual487F60 record");
    let record = owners.sounds.records().snapshot(sound).unwrap();
    assert_eq!(record.state, ProcessSoundRecordState::Linked);
    assert_eq!(record.position, Some(moved));
    let (volume, pitch) = if world.arena.actor(actor).unwrap().word_8c > 400 {
        (65536, 8192)
    } else {
        (16384, 65536)
    };
    assert_eq!((record.volume, record.pitch), (Some(volume), Some(pitch)));
    camera_at(&mut world, &owners, &runtime, &assets, moved);
    owners.tick(&mut world, &runtime, &assets, 33_333);
    assert!(
        world.actor_updates.family_prefix_stops.is_empty(),
        "{:?}",
        world.actor_updates.family_prefix_stops
    );
    assert_eq!(
        world.arena.derived_actor(actor).unwrap().sound_168,
        Some(sound)
    );
    assert_eq!(
        owners.sounds.records().snapshot(sound).unwrap().position,
        world.arena.object(actor).unwrap().base.position_words
    );
    let mut far = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    far[0] = far[0].wrapping_add(20_000);
    camera_at(&mut world, &owners, &runtime, &assets, far);
    owners.tick(&mut world, &runtime, &assets, 33_333);
    assert!(
        world.actor_updates.family_prefix_stops.is_empty(),
        "{:?}",
        world.actor_updates.family_prefix_stops
    );
    assert_eq!(world.arena.derived_actor(actor).unwrap().sound_168, None);
    assert_eq!(
        owners.sounds.records().snapshot(sound).unwrap().state,
        ProcessSoundRecordState::Released
    );
}

#[test]
fn selector35_production_target_failure_preserves_prefix_and_does_not_replay() {
    let (runtime, assets, mut world, _maps) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let position = [10_000, 20_000, 10_000];
    let actor = construct(&mut world, &owners, &runtime, &assets, position);
    schedule(&mut world, actor, None);
    camera_at(&mut world, &owners, &runtime, &assets, position);
    let rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 33_333);
    let stops = &world.actor_updates.family_prefix_stops;
    assert_eq!(stops.len(), 1, "{stops:?}");
    assert_eq!(stops[0].actor, actor);
    assert!(
        matches!(
            stops[0].reason,
            FamilyPrefixStopReason::Selector35TailFailed(WorldSelector35TailError::Tail(
                Selector35TailError::RequiredTarget {
                    address: 0x0043_e7a4,
                    ..
                }
            ))
        ),
        "{:?}",
        stops[0].reason
    );
    assert_eq!(world.arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(
        world.arena.family428650_actor(actor).unwrap().words_188_19c[0],
        Some(33_333)
    );
    assert_eq!(
        world.arena.object(actor).unwrap().base.position_words,
        Some(position)
    );
    assert_eq!(world.arena.derived_actor(actor).unwrap().sound_168, None);
    let before = world.arena.object(actor).unwrap().clone();
    owners.tick(&mut world, &runtime, &assets, 33_333);
    assert_eq!(world.arena.object(actor).unwrap(), &before);
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
    assert_eq!(world.rng, rng);
}

#[test]
fn selector35_production_follow_target_queues_predictive_fire_on_the_live_rng() {
    use crate::object_runtime::FollowTargetRequest;
    use inf_formats::native_basis::NativeBasis;
    let (runtime, assets, mut world, _maps) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let position = [10_000, 20_000, 10_000];
    let actor = construct(&mut world, &owners, &runtime, &assets, position);
    let target_actor = construct(
        &mut world,
        &owners,
        &runtime,
        &assets,
        [10_000, 20_000, 12_000],
    );
    let raised = world.arena.actor(target_actor).unwrap().word_74;
    world
        .arena
        .initialize_context(
            target_actor,
            BaseContext {
                position_words: Some([10_000, 20_000u16.wrapping_sub(raised), 12_000]),
                placement_index: u32::MAX,
            },
        )
        .unwrap();
    schedule(&mut world, actor, None);
    let handle = world.arena.actor(target_actor).unwrap().handle;
    let follow = world
        .arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    assert_eq!(
        follow.velocity_1a, None,
        "the actual refresh must publish velocity"
    );
    let target = world
        .arena
        .import_fixture_target(NativeTarget::ActorFollow(follow));
    world
        .arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            state.rotating.basis = NativeBasis::INITIAL;
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived")
            };
            derived.target_148 = Some(target);
            derived.word_164 = 4000;
            let behavior = derived.behavior_140.as_mut().expect("real H13/81 behavior");
            assert!(behavior.byte_0d > 0);
            behavior.byte_10 = 0;
            behavior.byte_0c = 1;
            behavior.word_04 = 2048;
            behavior.word_08 = 0;
            // Supply readiness and one available shot, retaining the actual loaded
            // profile and group records. This is fixture scheduling, not defaults.
            behavior.entries_18[0][0] = 1;
            behavior.entries_18[0][5] = 0;
            behavior.entries_18[0][6] = 0;
            behavior.entries_18[0][7] = 0;
            let DerivedActorClass::Family428650(family) = &mut derived.class else {
                panic!("family")
            };
            family.byte_22c = 1;
        })
        .unwrap();
    camera_at(&mut world, &owners, &runtime, &assets, position);
    let mut expected_rng = world.rng;
    for _ in 0..3 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    owners.tick(&mut world, &runtime, &assets, 33_333);
    assert!(
        world.actor_updates.family_prefix_stops.is_empty(),
        "{:?}",
        world.actor_updates.family_prefix_stops
    );
    assert_eq!(world.arena.derived_actor(actor).unwrap().word_150, Some(3));
    let NativeTarget::ActorFollow(refreshed) = world.arena.native_target(target).unwrap() else {
        panic!("follow")
    };
    assert_eq!(refreshed.focus_08, Some([10_000, 20_000, 12_000]));
    assert_eq!(
        refreshed.velocity_1a,
        Some(world.arena.actor(target_actor).unwrap().words_8e_92)
    );
    let behavior = world
        .arena
        .derived_actor(actor)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap();
    assert_ne!(
        behavior.word_20, 0,
        "actual48D340 publishes the predictive lead"
    );
    let head = behavior
        .fire_head_00
        .expect("actual48D0D0 queued node survives until draw");
    let node = world
        .arena
        .actor_service_heap()
        .weapon_fire_node(head)
        .unwrap();
    assert_eq!(node.compensation, 0);
    assert_eq!(node.group, 0);
    assert!(f32::from_bits(node.direction_bits[2]) > 0.0);
    assert_eq!(
        world.rng, expected_rng,
        "three native spread draws use the shared world RNG"
    );
    assert_eq!(behavior.entries_18[0][0], 0);
    assert_ne!(
        world.arena.object(actor).unwrap().base.flags & 0x0100_0000,
        0
    );
}
