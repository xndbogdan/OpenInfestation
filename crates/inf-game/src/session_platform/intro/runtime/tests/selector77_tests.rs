//! Actual loaded Intro arena and production Flag1 pass, with a supplied
//! unavailable process source. No actor/model/placement whitelist chooses77.

use super::family_prefix_tests::only_callbacks;
use super::*;
use crate::object_runtime::ObjectList;

fn eye(world: &WorldSession) -> ObjectId {
    world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector77_actor(actor).is_ok())
        .expect("actual loaded Intro contains native selector77")
}

fn tick(
    world: &mut WorldSession,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    owners: &mut TestRetirementOwners,
) {
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    tick_world_actor_callbacks(
        world,
        WorldActorTickContext {
            attract: false,
            map: None,
            network_key_countdown: &mut 0,
            network_team_flag: &mut 0,
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            camera_globals: &CameraGlobalState::INITIAL,
            grant_text: None,
            pickup_text: None,
            input: &mut crate::input::InputRuntime::default(),
            voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
            residency: runtime,
            model_fallback: None,
            assets,
            clock: 0,
            animation_clock: &owners.animation_clock,
            inputs: WorldTickPlatformInputs {
                settings_20: 0,
                settings_70: 0,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                settings_78: 0,
                settings_7c: 0,
                input_mode_08: 0,
                vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                settings_84: 0,
            },
            microseconds: 0,
            paced_microseconds: Some(0),
            mission_completion: false,
            inverse_sine: &inverse.values,
            up: &[0.0, 1.0, 0.0],
            surface_snap_53a128: &authored_surface_snap(&assets),
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            selector35_process: &mut owners.selector35_process,
            actor_motion: &owners.actor_motion,
            model_process: &owners.model_process,
            actor_shared_basis: &owners.actor_draw.shared_basis,
            state: &mut SessionState::default(),
            sounds: &mut owners.sounds,
            tasks: &mut owners.tasks,
            particles: &mut owners.particles,
            furniture: &mut owners.furniture,
            furniture_query: &mut owners.furniture_query,
            furniture_radius: &owners.furniture_radius,
            furniture_draw: &owners.furniture_draw,
        },
    );
}

#[test]
fn real_loaded_eye_dispatch_is_controller_independent_and_stopped_prefix_is_not_retried() {
    let (runtime, assets, mut world, _maps) = loaded();
    let actor = eye(&world);
    world.intro_runtime = None; // Supplied callback schedule, not a phase rewrite.
    only_callbacks(&mut world, &[actor]);
    let before = world.arena.selector77_actor(actor).unwrap().clone();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    let actual_null = owners.factory_process.process_null_handle;
    assert!(actual_null.is_some());
    owners.factory_process.process_null_handle = None;
    tick(&mut world, &runtime, &assets, &mut owners);
    assert_eq!(world.actor_updates.selector77_stops.len(), 1);
    assert_eq!(world.actor_updates.selector77_stops[0].0, actor);
    assert!(world.actor_updates.selector77_stops[0]
        .1
        .contains("process NULL"));
    assert_eq!(world.arena.selector77_actor(actor).unwrap(), &before);
    assert!(!world
        .actor_updates
        .skipped_callbacks
        .iter()
        .any(|&(id, _)| id == actor));
    assert_eq!(
        world
            .arena
            .list(ObjectList::Flag1Primary)
            .iter()
            .map(|node| node.object)
            .collect::<Vec<_>>(),
        [actor]
    );
    assert!(world.arena.list(ObjectList::Flag1Secondary).is_empty());
    // Fixing the external source cannot replay a stopped feature prefix.
    owners.factory_process.process_null_handle = actual_null;
    tick(&mut world, &runtime, &assets, &mut owners);
    assert_eq!(world.actor_updates.selector77_stops.len(), 1);
    assert_eq!(world.arena.selector77_actor(actor).unwrap(), &before);
    assert!(world.intro_runtime.is_none());
}

#[test]
fn actual_selector77_alternate3c_stays_separate_and_does_not_read24_process_source() {
    let (runtime, assets, mut world, _maps) = loaded();
    let actor = eye(&world);
    world.intro_runtime = None;
    let flags = world.arena.object(actor).unwrap().base.flags | 0x8000_0000;
    world.arena.apply_payload_flags(actor, flags).unwrap();
    only_callbacks(&mut world, &[actor]);
    let before = world.arena.selector77_actor(actor).unwrap().clone();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    owners.factory_process.process_null_handle = None;
    for _ in 0..2 {
        tick(&mut world, &runtime, &assets, &mut owners);
    }
    assert!(world.actor_updates.selector77_stops.is_empty());
    assert_eq!(world.actor_updates.skipped_callbacks, [(actor, true)]);
    assert_eq!(world.arena.selector77_actor(actor).unwrap(), &before);
    assert!(world.arena.list(ObjectList::Flag1Secondary).is_empty());
}

// Append to session_platform/intro/runtime/tests/selector77_tests.rs.
// Supplied callback timing/membership isolates the actual loaded Eye, without
// replacing its timer, player, target, pose, phase, constructor RNG or class.
#[test]
fn actual_loaded_eye_ordinary_flag1_updates_middle_span_and_real_expiry_player_path() {
    let (runtime, assets, mut world, _maps) = loaded();
    let actor = eye(&world);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    let mut owners = TestRetirementOwners::new(&world, executable);
    assert_eq!(
        owners.factory_process.process_null_handle,
        Some(crate::object_runtime::ActorHandle::from_raw(0))
    );
    // The clean-import world already queues an unrelated actor for disposal.
    // Run that actual manager phase before taking mission/RNG baselines, with
    // every actor callback withheld by this fixture's explicit schedule.
    let eye_before_disposal = world.arena.selector77_actor(actor).unwrap().clone();
    let rng_before_disposal = world.rng;
    only_callbacks(&mut world, &[]);
    tick(&mut world, &runtime, &assets, &mut owners);
    assert_eq!(
        world.arena.selector77_actor(actor).unwrap(),
        &eye_before_disposal
    );
    assert_eq!(world.rng, rng_before_disposal);
    assert!(world.actor_updates.disposal.stopped().is_empty());
    assert!(world.arena.list(ObjectList::Disposal).is_empty());
    let initial = world.arena.selector77_actor(actor).unwrap().clone();
    assert_eq!(
        (initial.word_19c, initial.word_1a0),
        (Some(0), Some(0)),
        "actual retained arena selected FreshHeapZeroExtendedCarryApproximation before allocation"
    );
    assert_eq!(initial.word_19e, None);
    assert_eq!(initial.words_184_190, [0; 7]);
    assert_eq!((initial.word_1a4, initial.word_1a8), (0, 0));
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 3);
    assert!(world
        .arena
        .derived_actor(actor)
        .unwrap()
        .target_148
        .is_none());

    // Read the real first player's +D4 through 0046B030. This does not choose
    // a NULL target or replace the player with a supplied far-position actor.
    let primary = world
        .arena
        .first_player_primary_actor()
        .unwrap()
        .expect("actual clean-import fixture respawns a primary player vehicle");
    let actor_xyz = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let primary_xyz = world
        .arena
        .object(primary)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let delta = std::array::from_fn::<_, 3, _>(|axis| {
        i32::from(primary_xyz[axis].wrapping_sub(actor_xyz[axis]) as i16)
    });
    let square = delta.into_iter().fold(0i32, |sum, value| {
        sum.wrapping_add(value.wrapping_mul(value))
    }) as u32;
    let distance = if square & 0x8000_0000 != 0 {
        inf_formats::native_basis::isqrt_48efc0((square >> 2) as i32).wrapping_shl(1)
    } else {
        inf_formats::native_basis::isqrt_48efc0(square as i32)
    };
    let range = world.arena.derived_actor(actor).unwrap().word_164;
    println!("actual loaded Eye primary={primary:?} eye={actor_xyz:?} primary_xyz={primary_xyz:?} distance={distance} range={range} timer={}", initial.word_198);
    // The successful owned expiry route requires the actual source to be far.
    // A failed assertion here reports a real near-allocation requirement; it
    // must never be fixed by moving the player or seeding a target NULL.
    assert!(
        distance as i32 > range,
        "actual 00407600 reaches required near allocator"
    );

    let position = world.arena.object(actor).unwrap().base.position_words;
    let basis = world.arena.actor(actor).unwrap().rotating.basis;
    let selection = world.mission.save.clone();
    let current_world = world.mission.current_world_a0;
    world.intro_runtime = None; // Only isolate scheduling; retain mission/world.
    only_callbacks(&mut world, &[actor]);
    let primary_before = world.arena.object(primary).unwrap().clone();
    let mut clock = crate::frontend_camera::AnimationClock::default();
    let mut state = SessionState::default();
    let rng_before = world.rng;
    let short_delta = (initial.word_198 / 2).min(40_000);
    assert!(
        short_delta >= 1024,
        "actual ctor timer must admit this positive pre-expiry window"
    );

    // Both initial visits are before/equal to real198 expiry. The first owns
    // the native two RNG selections; only the next visit changes WORD190.
    for visit in 0..2 {
        clock.advance(short_delta);
        tick_world_actor_callbacks(
            &mut world,
            WorldActorTickContext {
                attract: false,
                map: None,
                network_key_countdown: &mut 0,
                network_team_flag: &mut 0,
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                camera_globals: &CameraGlobalState::INITIAL,
                grant_text: None,
                pickup_text: None,
                input: &mut crate::input::InputRuntime::default(),
                voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
                residency: &runtime,
                model_fallback: None,
                assets: &assets,
                clock: clock.value(),
                animation_clock: &clock,
                inputs: WorldTickPlatformInputs {
                    settings_20: 0,
                    settings_70: 0,
                    camera_settings: crate::object_runtime::CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                    settings_78: 0,
                    settings_7c: 0,
                    input_mode_08: 0,
                    vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                    settings_84: 0,
                },
                microseconds: short_delta,
                paced_microseconds: Some(short_delta),
                mission_completion: false,
                inverse_sine: &inverse.values,
                up: &[0.0, 1.0, 0.0],
                surface_snap_53a128: &authored_surface_snap(&assets),
                factory_process: &mut owners.factory_process,
                behavior_tables: &mut owners.behavior_tables,
                selector35_process: &mut owners.selector35_process,
                actor_motion: &owners.actor_motion,
                model_process: &owners.model_process,
                actor_shared_basis: &owners.actor_draw.shared_basis,
                state: &mut state,
                sounds: &mut owners.sounds,
                tasks: &mut owners.tasks,
                particles: &mut owners.particles,
                furniture: &mut owners.furniture,
                furniture_query: &mut owners.furniture_query,
                furniture_radius: &owners.furniture_radius,
                furniture_draw: &owners.furniture_draw,
            },
        );
        assert!(
            world.actor_updates.selector77_stops.is_empty(),
            "{:?}",
            world.actor_updates.selector77_stops
        );
        assert!(world.actor_updates.skipped_callbacks.is_empty());
        let eye = world.arena.selector77_actor(actor).unwrap();
        assert_eq!(eye.word_198, initial.word_198 - short_delta * (visit + 1));
        assert_eq!(eye.word_1a0, Some(short_delta * (visit + 1)));
        assert_eq!(
            eye.word_19c,
            Some(((short_delta >> 5) * (visit + 1)) as u16)
        );
        assert_eq!(eye.word_1ac, Some(short_delta >> 6));
        if visit == 0 {
            assert_eq!(eye.words_184_190[6], 0);
            assert!(eye.word_1a4 > 0 && eye.word_1a8 >= 0x1800);
        } else {
            assert!(
                eye.words_184_190[6] > 0,
                "actual+D4 must advance original699 middle span"
            );
        }
        assert_eq!(
            world.rng,
            advance_rng(rng_before, 2),
            "same process RNG; no second-visit reseed"
        );
    }
    let animated = world.arena.selector77_actor(actor).unwrap().words_184_190[6];
    let getter = crate::object_runtime::ObjectModelCallbacks {
        arena: &world.arena,
        source: actor,
        animation: &clock,
        executable,
    };
    assert_eq!(
        getter.packed_value(6).unwrap(),
        animated,
        "original699 packed86 consumes this live WORD190, not a supplied pose"
    );

    // Now cross the real remaining timer exactly once. 00407600 re-resolves
    // the actual primary actor, takes its proven far route, and remains NULL.
    let expiry_delta = world.arena.selector77_actor(actor).unwrap().word_198 + 1;
    let rng_before_expiry = world.rng;
    clock.advance(expiry_delta);
    tick_world_actor_callbacks(
        &mut world,
        WorldActorTickContext {
            attract: false,
            map: None,
            network_key_countdown: &mut 0,
            network_team_flag: &mut 0,
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            camera_globals: &CameraGlobalState::INITIAL,
            grant_text: None,
            pickup_text: None,
            input: &mut crate::input::InputRuntime::default(),
            voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
            residency: &runtime,
            model_fallback: None,
            assets: &assets,
            clock: clock.value(),
            animation_clock: &clock,
            inputs: WorldTickPlatformInputs {
                settings_20: 0,
                settings_70: 0,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                settings_78: 0,
                settings_7c: 0,
                input_mode_08: 0,
                vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                settings_84: 0,
            },
            microseconds: expiry_delta,
            paced_microseconds: Some(expiry_delta),
            mission_completion: false,
            inverse_sine: &inverse.values,
            up: &[0.0, 1.0, 0.0],
            surface_snap_53a128: &authored_surface_snap(&assets),
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            selector35_process: &mut owners.selector35_process,
            actor_motion: &owners.actor_motion,
            model_process: &owners.model_process,
            actor_shared_basis: &owners.actor_draw.shared_basis,
            state: &mut state,
            sounds: &mut owners.sounds,
            tasks: &mut owners.tasks,
            particles: &mut owners.particles,
            furniture: &mut owners.furniture,
            furniture_query: &mut owners.furniture_query,
            furniture_radius: &owners.furniture_radius,
            furniture_draw: &owners.furniture_draw,
        },
    );
    assert!(
        world.actor_updates.selector77_stops.is_empty(),
        "{:?}",
        world.actor_updates.selector77_stops
    );
    assert!(world.actor_updates.skipped_callbacks.is_empty());
    assert!(world
        .arena
        .derived_actor(actor)
        .unwrap()
        .target_148
        .is_none());
    assert_eq!(
        world.rng,
        advance_rng(rng_before_expiry, 1),
        "far reacquisition consumes one real RNG draw"
    );
    assert_eq!(
        world.arena.selector77_actor(actor).unwrap().word_1ac,
        Some(expiry_delta >> 6)
    );
    assert_eq!(world.arena.selector77_actor(actor).unwrap().word_19e, None);
    assert_eq!(
        world.arena.object(actor).unwrap().base.position_words,
        position
    );
    assert_eq!(world.arena.actor(actor).unwrap().rotating.basis, basis);
    assert_eq!(world.arena.object(primary).unwrap(), &primary_before);
    assert_eq!(world.mission.save, selection);
    assert_eq!(world.mission.current_world_a0, current_world);
    assert!(world.intro_runtime.is_none());
    assert!(!state.transition_pending());
    assert_eq!(
        world
            .arena
            .list(ObjectList::Flag1Primary)
            .iter()
            .map(|node| node.object)
            .collect::<Vec<_>>(),
        [actor]
    );
    assert!(world.arena.list(ObjectList::Flag1Secondary).is_empty());
}
