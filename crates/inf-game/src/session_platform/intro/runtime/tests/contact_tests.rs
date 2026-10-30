//! The pair pass's reactions with the world's services, over Level 1.

use super::*;
use crate::object_runtime::{ActorPairHost, FamilyUpdateClass};
use crate::session_platform::world_contact::{pair_class, PairClass, WorldContactPassHost};

/// `00469030`: the vehicle knocks a living mutant scientist (team 0x12,
/// state 3). A velocity change of 500 leaves it to the shared reaction; one
/// above 500 kills it first (`0047CE90`: state 1, health 0), as a shot
/// does. A scientist of another team survives any knock. Level 1's course
/// targets and its bridge are admitted to the pair pass beside the vehicle.
#[test]
fn the_vehicle_kills_a_mutant_scientist_it_knocks_hard() {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &world.arena;
    let scientists: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            pair_class(arena, actor) == Some(PairClass::Family(FamilyUpdateClass::Selector12))
                && arena.actor(actor).unwrap().word_c4 == 3
        })
        .collect();
    assert!(scientists.len() >= 3, "{}", scientists.len());
    assert_eq!(pair_class(arena, vehicle), Some(PairClass::Selector2));
    let course = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector39(actor).is_ok())
        .unwrap();
    assert_eq!(pair_class(arena, course), Some(PairClass::Selector39));
    let bridge = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector37(actor).is_ok())
        .unwrap();
    assert_eq!(pair_class(arena, bridge), Some(PairClass::Selector37));

    let mut state = SessionState::default();
    let mut host = WorldContactPassHost {
        paced_skip: false,
        mission_completion: false,
        damage: WorldDamageOwners {
            grant_text: None,
            camera_settings: None,
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut owners.tasks,
            terrain: &mut world.terrain,
            particles: &mut owners.particles,
            sounds: &mut owners.sounds,
            furniture: &mut owners.furniture,
            residency: &runtime,
            executable,
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            animation_clock: &owners.animation_clock,
            session: &mut state,
            particle_load_factor: 0x10000,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &sine,
                world_flags: world.world_flags,
            }),
            map: None,
        },
        rng: &mut world.rng,
        model_fallback: None,
        model_process: &owners.model_process,
        furniture_query: &mut owners.furniture_query,
        furniture_radius: &owners.furniture_radius,
        furniture_draw: &owners.furniture_draw,
        shared_basis: &owners.actor_draw.shared_basis,
        terrain_source: &world.terrain_source,
        world_flags: world.world_flags,
        sine_quarter: &sine,
        network_active: false,
        world_start: &mut world.world_start,
        camera_settings: crate::object_runtime::CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        grant_text: None,
        factory_tables: std::cell::OnceCell::new(),
        math: &math,
    };
    let arena = &mut world.arena;
    // The knock: the velocity +8E..+92 now against the one saved before
    // the exchange, 300 in X and 400 in Z (500) or 300 and 402 (501).
    let knock = |arena: &mut ObjectArena, scientist: ObjectId, team: u32, z: u16| {
        arena
            .with_supplied_family_dispatch_actor(scientist, |actor| {
                actor.word_b4 = team;
                actor.words_8e_92 = [300, 0, z];
            })
            .unwrap();
        [0, 0, 0]
    };
    for (index, (team, z, dies)) in [(0x12, 400, false), (0x12, 402, true), (1, 2000, false)]
        .into_iter()
        .enumerate()
    {
        let scientist = scientists[index];
        let saved = knock(arena, scientist, team, z);
        host.react_pair(arena, scientist, vehicle, saved, [0; 3])
            .unwrap();
        let actor = arena.actor(scientist).unwrap();
        assert_eq!(
            (actor.word_c4, actor.health == 0),
            if dies { (1, true) } else { (3, false) },
            "team {team:#x}, z {z}"
        );
    }
}

/// `0048BD90`, through the pair pass's scientist arm (`00468160`): Terra
/// Nova's enemy gate (team 0x12, first team `+188` 0x12) touched by a
/// scientist whose target it is takes the scientist's team, and the world
/// marks its placement (`0048EAD0`). A mutant scientist then brings it back
/// to team 0x12 and the mark clears (`0048EB30`). A scientist that has not
/// targeted the gate changes nothing.
#[test]
fn a_scientist_takes_an_enemy_gate_for_its_side() {
    use crate::object_runtime::{FixedPositionTarget, NativeTarget};
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        2,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    let arena = &mut world.arena;
    let gate = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena.selector30_actor(actor).is_ok() && arena.actor(actor).unwrap().word_b4 == 0x12
        })
        .expect("Terra Nova's enemy gate");
    assert_eq!(
        arena
            .object(gate)
            .unwrap()
            .base
            .position_words
            .map(|p| [p[0], p[2]]),
        Some([31994, 31407])
    );
    assert_eq!(arena.selector30_actor(gate).unwrap().word_188, Some(0x12));
    let index = arena.actor(gate).unwrap().byte_78 as i8;
    let scientists: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            pair_class(arena, actor) == Some(PairClass::Family(FamilyUpdateClass::Selector12))
        })
        .collect();
    assert!(scientists.len() >= 2, "{}", scientists.len());
    let (friend, mutant) = (scientists[0], scientists[1]);
    for (scientist, team) in [(friend, 2), (mutant, 0x12)] {
        arena
            .with_supplied_family_dispatch_actor(scientist, |actor| actor.word_b4 = team)
            .unwrap();
    }
    let gate_handle = arena.actor(gate).unwrap().handle;
    let point = arena.object(gate).unwrap().base.position_words;
    let target = |arena: &mut ObjectArena| {
        arena.import_fixture_target(NativeTarget::FixedPosition(
            FixedPositionTarget::from_point(gate_handle, point),
        ))
    };
    let mut state = SessionState::default();
    let mut host = WorldContactPassHost {
        paced_skip: false,
        mission_completion: false,
        damage: WorldDamageOwners {
            grant_text: None,
            camera_settings: None,
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut owners.tasks,
            terrain: &mut world.terrain,
            particles: &mut owners.particles,
            sounds: &mut owners.sounds,
            furniture: &mut owners.furniture,
            residency: &runtime,
            executable,
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            animation_clock: &owners.animation_clock,
            session: &mut state,
            particle_load_factor: 0x10000,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &sine,
                world_flags: world.world_flags,
            }),
            map: None,
        },
        rng: &mut world.rng,
        model_fallback: None,
        model_process: &owners.model_process,
        furniture_query: &mut owners.furniture_query,
        furniture_radius: &owners.furniture_radius,
        furniture_draw: &owners.furniture_draw,
        shared_basis: &owners.actor_draw.shared_basis,
        terrain_source: &world.terrain_source,
        world_flags: world.world_flags,
        sine_quarter: &sine,
        network_active: false,
        world_start: &mut world.world_start,
        camera_settings: crate::object_runtime::CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        grant_text: None,
        factory_tables: std::cell::OnceCell::new(),
        math: &math,
    };
    let marked = |host: &WorldContactPassHost<'_, '_>| {
        let words = host.damage.mission.current_world_completion().unwrap();
        words[(index as i32 / 32) as usize] & 1 << (index as u32 & 31) != 0
    };
    // Untargeted, the friend only collides.
    assert!(!host.accept_pair(arena, friend, gate).unwrap());
    assert_eq!(
        (arena.actor(gate).unwrap().word_b4, marked(&host)),
        (0x12, false)
    );
    let reference = target(arena);
    arena.set_target_148_for_tests(friend, reference);
    assert!(!host.accept_pair(arena, friend, gate).unwrap());
    assert_eq!(
        (arena.actor(gate).unwrap().word_b4, marked(&host)),
        (2, true)
    );
    let reference = target(arena);
    arena.set_target_148_for_tests(mutant, reference);
    assert!(!host.accept_pair(arena, mutant, gate).unwrap());
    assert_eq!(
        (arena.actor(gate).unwrap().word_b4, marked(&host)),
        (0x12, false)
    );
}

/// Altair Prime's scientist gate (`+18E`, linked one-based to the terrain
/// bridge's placement) and the bridge. A scientist that targeted the gate
/// takes it for its side and passes (`0048BE70`): its offset from the gate,
/// seen from the half-turned gate and 10 further, is laid out about the
/// bridge, its speed turned the same way, and the gate's refusal joins the
/// pass's bits (`+22E` 0x0D). Offered to the bridge, short of its one staff, the bridge
/// takes it (`0040DAB0`): the scientist's team, one staff, the scientist
/// queued for disposal and the bridge's placement recorded. A second
/// scientist finds the bridge full and is refused. The full bridge rises
/// under its own update with the world's services, and the rise's end
/// (`00409ED9`, `0040DCB0` with no vehicle) sends the scientist back out on
/// the bridge's team at its `+146` offset, 500 along its X, where retail's
/// came out; the emptied bridge turns team 0 with no capacity. Both retail
/// points are from tampered-run:
/// runtime_re/lab/scenarios/altair-drop-sci.json (a
/// constructed save with a scientist in the hold, the vehicle teleported
/// beside the gate), at Detail Level 1 as here.
#[test]
fn a_scientist_passes_altair_primes_gate_to_the_bridge_that_takes_it() {
    use crate::object_runtime::{FixedPositionTarget, NativeTarget, ObjectList};
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        5,
        crate::terrain_runtime::TerrainSpacing::Units128,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    let arena = &mut world.arena;
    let actors = arena.actor_list().to_vec();
    let position = |arena: &ObjectArena, actor: ObjectId| {
        let p = arena.object(actor).unwrap().base.position_words.unwrap();
        [p[0], p[2]]
    };
    let gate = actors
        .iter()
        .copied()
        .find(|&actor| {
            arena
                .selector30_actor(actor)
                .is_ok_and(|gate| gate.byte_18e != 0)
        })
        .expect("Altair Prime's scientist gate");
    assert_eq!(position(arena, gate), [47407, 30141]);
    let bridge = actors
        .iter()
        .copied()
        .find(|&actor| arena.selector37(actor).is_ok())
        .expect("Altair Prime's bridge");
    assert_eq!(position(arena, bridge), [51736, 31220]);
    assert_eq!(arena.free_staff(bridge, None).unwrap(), 1);
    let scientists: Vec<_> = actors
        .iter()
        .copied()
        .filter(|&actor| {
            pair_class(arena, actor) == Some(PairClass::Family(FamilyUpdateClass::Selector12))
        })
        .collect();
    assert!(scientists.len() >= 2, "{}", scientists.len());
    let (first, second) = (scientists[0], scientists[1]);
    for scientist in [first, second] {
        arena
            .with_supplied_family_dispatch_actor(scientist, |actor| actor.word_b4 = 2)
            .unwrap();
    }
    let target = |arena: &mut ObjectArena, of: ObjectId| {
        let handle = arena.actor(of).unwrap().handle;
        let point = arena.object(of).unwrap().base.position_words;
        arena.import_fixture_target(NativeTarget::FixedPosition(
            FixedPositionTarget::from_point(handle, point),
        ))
    };
    let bridge_index = arena.actor(bridge).unwrap().byte_78 as i8;
    let mut state = SessionState::default();
    let mut host = WorldContactPassHost {
        paced_skip: false,
        mission_completion: false,
        damage: WorldDamageOwners {
            grant_text: None,
            camera_settings: None,
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut owners.tasks,
            terrain: &mut world.terrain,
            particles: &mut owners.particles,
            sounds: &mut owners.sounds,
            furniture: &mut owners.furniture,
            residency: &runtime,
            executable,
            factory_process: &mut owners.factory_process,
            behavior_tables: &mut owners.behavior_tables,
            animation_clock: &owners.animation_clock,
            session: &mut state,
            particle_load_factor: 0x10000,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &sine,
                world_flags: world.world_flags,
            }),
            map: None,
        },
        rng: &mut world.rng,
        model_fallback: None,
        model_process: &owners.model_process,
        furniture_query: &mut owners.furniture_query,
        furniture_radius: &owners.furniture_radius,
        furniture_draw: &owners.furniture_draw,
        shared_basis: &owners.actor_draw.shared_basis,
        terrain_source: &world.terrain_source,
        world_flags: world.world_flags,
        sine_quarter: &sine,
        network_active: false,
        world_start: &mut world.world_start,
        camera_settings: crate::object_runtime::CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        grant_text: None,
        factory_tables: std::cell::OnceCell::new(),
        math: &math,
    };
    let recorded = |host: &WorldContactPassHost<'_, '_>| {
        let words = host.damage.mission.current_world_completion().unwrap();
        words[(bridge_index as i32 / 32) as usize] & 1 << (bridge_index as u32 & 31) != 0
    };
    // The gate passes the scientist that targeted it, to the bridge: its
    // offset and speed through the half-turned gate, 10 further, then out
    // about the bridge, as retail's landed.
    // Where retail's crossed, at the speed it had (pass 279).
    arena.set_position_for_tests(first, [47215, 159, 30208]);
    arena
        .with_supplied_family_dispatch_actor(first, |actor| {
            actor.words_8e_92 = [60, 0xffff, 0xfffe]
        })
        .unwrap();
    let reference = target(arena, gate);
    arena.set_target_148_for_tests(first, reference);
    assert!(!host.accept_pair(arena, first, gate).unwrap());
    assert_eq!(arena.actor(gate).unwrap().word_b4, 2);
    assert_eq!(
        arena.object(first).unwrap().base.position_words,
        Some([51664, 95, 31420])
    );
    assert_eq!(
        arena.actor(first).unwrap().words_8e_92,
        [37, 0xffff, 0xffd1]
    );
    assert_eq!(
        arena.family428650_actor(first).unwrap().word_22e & 0x0d,
        0x0d
    );
    assert!(!recorded(&host));
    // The bridge takes it.
    let reference = target(arena, bridge);
    arena.set_target_148_for_tests(first, reference);
    assert!(host.accept_pair(arena, first, bridge).unwrap());
    assert_eq!(arena.free_staff(bridge, None).unwrap(), 0);
    assert_eq!(arena.actor(bridge).unwrap().word_b4, 2);
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == first));
    assert!(recorded(&host));
    // The full bridge refuses the next.
    let reference = target(arena, bridge);
    arena.set_target_148_for_tests(second, reference);
    let before = arena.family428650_actor(second).unwrap().word_22e;
    assert!(!host.accept_pair(arena, second, bridge).unwrap());
    assert_eq!(
        arena.family428650_actor(second).unwrap().word_22e,
        before | 1
    );
    let before = arena.actor_list().to_vec();
    let state = super::family_prefix_tests::gameplay_state();
    let mut policy = crate::terrain_runtime::ProcessTerrainPolicy::supplied(
        crate::terrain_runtime::TerrainSpacing::Units128,
    );
    let mut host = crate::session_platform::world_actor_update::WorldBridgeHost {
        family: crate::session_platform::world_actor_update::WorldFamilyUpdateHost {
            owners: WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: Some(&mut world.flash),
                mission: &mut world.mission,
                tasks: &mut owners.tasks,
                terrain: &mut world.terrain,
                particles: &mut owners.particles,
                sounds: &mut owners.sounds,
                furniture: &mut owners.furniture,
                residency: &runtime,
                executable,
                factory_process: &mut owners.factory_process,
                behavior_tables: &mut owners.behavior_tables,
                animation_clock: &owners.animation_clock,
                session: &state,
                particle_load_factor: 0x10000,
                behavior_factory: Some(BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine: &sine,
                    world_flags: world.world_flags,
                }),
                map: None,
            },
            rng: &mut world.rng,
            sine_quarter: &sine,
        },
        status_load_factor: 0x10000,
        policy: &mut policy,
        detail: 1,
        world_flags: world.world_flags,
    };
    // Its updates start after the take, so the hold +16C is still the
    // constructor's 0 and only the rise's 4 s run.
    let mut ticks = 0;
    while arena.family40d800(bridge).unwrap().word_13c != 0 {
        assert!(ticks < 200, "the rise never ends");
        arena.tick_selector37(bridge, 40_000, &mut host).unwrap();
        ticks += 1;
    }
    assert_eq!(ticks, 100);
    let staff = arena.family40d800(bridge).unwrap();
    assert_eq!((staff.word_13c, staff.word_140), (0, 0));
    assert_eq!(staff.words_146_14a, Some([500, 0, 0]));
    assert_eq!(arena.actor(bridge).unwrap().word_b4, 0);
    let released: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|actor| !before.contains(actor))
        .collect();
    let [scientist] = released[..] else {
        panic!("{released:?}")
    };
    assert_eq!(
        pair_class(arena, scientist),
        Some(PairClass::Family(FamilyUpdateClass::Selector12))
    );
    assert_eq!(arena.actor(scientist).unwrap().word_b4, 2);
    assert_eq!(
        arena.object(scientist).unwrap().base.position_words,
        Some([52236, 96, 31220])
    );
}
