//! Loaded actors and real production services. Lifecycle/timer scheduling and
//! the initial session state are explicit fixture inputs, not retail timing.

use super::*;
use crate::{
    object_runtime::{
        ActorExtension, ActorHandle, DerivedActorClass, FamilyBehaviorError, FamilyBehaviorHost,
        FamilyBehaviorProfile, ObjectArena,
    },
    session_platform::world_actor_behavior::WorldFamilyBehaviorError,
};

fn selector12(world: &WorldSession) -> ObjectId {
    world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.family_update_class(actor) == Ok(FamilyUpdateClass::Selector12))
        .expect("authored intro contains selector12 actors")
}

fn with_behavior<R>(
    world: &mut WorldSession,
    owners: &mut TickOwners,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    action: impl FnOnce(&mut ObjectArena, &mut WorldFamilyBehaviorHost<'_, '_>) -> R,
) -> R {
    let mut voices = crate::process_sound::RequiredProcessSoundVoiceTeardown;
    let mut host = WorldFamilyBehaviorHost {
        grant_text: None,
        residency: runtime,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        animation_clock: &owners.clock,
        factory_process: &mut owners.factory_process,
        session: &mut owners.state,
        behavior_tables: &mut owners.behavior_tables,
        selector35_process: &mut owners.selector35_process,
        tasks: &mut owners.tasks,
        particles: &mut owners.particles,
        sounds: &mut owners.sounds,
        process_rng: &mut world.rng,
        terrain_source: &world.terrain_source,
        terrain: &mut world.terrain,
        dying: crate::session_platform::world_actor_behavior::WorldFamilyDyingContext {
            mission: &mut world.mission,
            furniture: &mut owners.furniture,
            world_start: &mut world.world_start,
            listeners: &mut world.listeners,
            voices: &mut voices,
            world_flags: world.world_flags,
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            up: &[0.0, 1.0, 0.0],
            flash: &mut world.flash,
        },
        sine_quarter: &assets.sine_table().values,
        settings_7c: 0,
        particle_load_factor: world.fade.performance.particle_load_factor,
    };
    action(&mut world.arena, &mut host)
}

fn supply_dying_schedule(world: &mut WorldSession, actor: ObjectId, entered: bool) {
    world
        .arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            state.word_c4 = 1;
            state.word_cc = 0;
            state.word_ce = 0;
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived actor")
            };
            derived.word_144 = 0;
            let DerivedActorClass::Family428650(family) = &mut derived.class else {
                panic!("family actor")
            };
            family.word_184 = if entered { 1 } else { -1 };
            family.words_188_19c[0] = Some(if entered { 10_000_001 } else { 0 });
        })
        .unwrap();
    let flags = world.arena.object(actor).unwrap().base.flags & !0x8200_0000;
    world.arena.apply_payload_flags(actor, flags).unwrap();
}

fn enter_dying(
    world: &mut WorldSession,
    owners: &mut TickOwners,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    actor: ObjectId,
) {
    let profile = world.arena.family_behavior_profile(actor).unwrap();
    assert!(matches!(profile, FamilyBehaviorProfile::Selector12 { .. }));
    assert_eq!(profile.descriptor(1).unwrap().change_address, 0x0043_fe40);
    with_behavior(world, owners, runtime, assets, |arena, host| {
        FamilyBehaviorHost::change(host, arena, actor, profile, 1, true).unwrap();
    });
}

#[test]
fn production_selector12_dying_entry_refreshes_model_and_births_from_live_process_owners() {
    let (runtime, assets, mut world, _maps) = loaded();
    let actor = selector12(&world);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    world.fade.performance.particle_load_factor = 65_536;
    let process_null = ActorHandle::from_raw(0x7654_3210);
    assert_ne!(
        world.arena.actor_handles().unwrap().null_handle(),
        process_null
    );
    owners.factory_process.process_null_handle = Some(process_null);
    let mut position = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    position[1] = position[1].wrapping_add(world.arena.object(actor).unwrap().base.word_28);
    let mut expected_rng = world.rng;
    // Three entry draws, three per actual type5 debris, one for type4 central.
    for _ in 0..52 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    enter_dying(&mut world, &mut owners, &runtime, &assets, actor);
    assert_eq!(
        world.arena.actor(actor).unwrap().rotating.visual.model_word,
        248
    );
    assert_eq!(
        world.arena.object(actor).unwrap().base.position_words,
        Some(position)
    );
    assert_eq!(
        world.arena.derived_actor(actor).unwrap().word_16c,
        Some(0x8000)
    );
    assert_eq!(
        world.arena.derived_actor(actor).unwrap().word_170,
        Some(0x8000)
    );
    assert_eq!(world.rng, expected_rng);
    assert_eq!(
        owners.factory_process.process_null_handle,
        Some(process_null)
    );
    assert_eq!(owners.particles.vector_cursor, 16);
    assert_eq!(owners.particles.free_count(), 400 - 17);
    let ids = owners.particles.pending(1).unwrap();
    assert_eq!(ids.len(), 17);
    let mut debris = 0;
    let mut central = 0;
    for &id in ids {
        assert!(matches!(id, ParticleId::Pool(_)));
        let particle = owners.particles.record(id).unwrap();
        assert_eq!(particle.read_dword(0x14).unwrap(), process_null.raw());
        match particle.read_dword(0).unwrap() {
            0x0052_8934 => {
                debris += 1;
                assert_eq!(particle.read_dword(0x28).unwrap(), 0x0053_d338);
                assert_eq!(particle.read_dword(0x34).unwrap(), 12);
                assert_eq!(particle.read_word(0x3a).unwrap(), 150);
            }
            0x0052_8914 => {
                central += 1;
                assert_eq!(particle.read_dword(0x28).unwrap(), 0x0053_d5e0);
                assert_eq!(particle.position().unwrap(), position);
                assert_eq!(particle.read_dword(0x30).unwrap(), 0);
            }
            class => panic!("unexpected death effect class {class:08X}"),
        }
    }
    assert_eq!((debris, central), (16, 1));
}

#[test]
fn production_selector12_dying_disposes_on_the_following_manager_pass() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = selector12(&world);
    only_callbacks(&mut world, &[actor]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    world.fade.performance.particle_load_factor = 65_536;
    enter_dying(&mut world, &mut owners, &runtime, &assets, actor);
    supply_dying_schedule(&mut world, actor, true);
    world.mission.inhibit_byte_08 = 7;
    let handle = world.arena.actor(actor).unwrap().handle;
    let before_count = world.arena.len();
    let particles = owners.particles.pending(1).unwrap().clone();
    // The real loaded world can already contain disposal work. Let the same
    // manager drain it rather than changing the fixture's native memberships.
    let previously_queued: Vec<_> = world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    assert!(!previously_queued.contains(&actor));

    owners.tick(&mut world, &runtime, &assets, 0);
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 0);
    assert_eq!(
        world.arena.resolve_actor_handle(handle).unwrap(),
        Some(actor)
    );
    let earlier_destroyed = previously_queued
        .iter()
        .filter(|&&object| world.arena.object(object).is_err())
        .count();
    assert_eq!(world.arena.len(), before_count - earlier_destroyed);
    assert_eq!(
        world
            .arena
            .list(ObjectList::Disposal)
            .iter()
            .filter(|node| node.object == actor)
            .count(),
        1
    );
    assert_eq!(world.mission.inhibit_byte_08, 7);
    let queued_count = world.arena.len();
    let previous_stops = world.actor_updates.disposal.stopped().to_vec();

    owners.tick(&mut world, &runtime, &assets, 0);
    assert!(world.arena.object(actor).is_err());
    assert_eq!(world.arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(world.arena.len(), queued_count - 1);
    assert!(!world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor));
    assert_eq!(world.actor_updates.disposal.stopped(), previous_stops);
    assert_eq!(world.mission.inhibit_byte_08, 7);
    assert_eq!(owners.particles.pending(1).unwrap(), &particles);
    assert_eq!(owners.particles.free_count(), 400 - 17);
}

#[test]
fn production_selector12_dying_failure_keeps_the_effect_prefix_without_replay() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = selector12(&world);
    only_callbacks(&mut world, &[actor]);
    supply_dying_schedule(&mut world, actor, false);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    owners.factory_process.process_null_handle = None;
    world.fade.performance.particle_load_factor = 65_536;
    let mut expected_rng = world.rng;
    for _ in 0..3 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    owners.tick(&mut world, &runtime, &assets, 0);
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
    assert!(matches!(
        &world.actor_updates.family_prefix_stops[0].reason,
        FamilyPrefixStopReason::BehaviorFailed(FamilyBehaviorError::Host {
            effect: "behavior entry",
            error: WorldFamilyBehaviorError::Owner {
                phase: "Selector12 dying descriptor",
                ..
            },
        })
    ));
    assert_eq!(
        world.arena.actor(actor).unwrap().rotating.visual.model_word,
        248
    );
    assert_eq!(
        world.arena.derived_actor(actor).unwrap().word_16c,
        Some(0x8000)
    );
    assert_eq!(world.arena.family428650_actor(actor).unwrap().word_184, -1);
    assert_eq!(world.rng, expected_rng);
    // The first real debris allocation precedes its required process source.
    assert_eq!(owners.particles.free_count(), 399);
    assert_eq!(owners.particles.vector_cursor, 1);
    assert!(owners.particles.pending(1).unwrap().is_empty());
    let stopped = world.arena.object(actor).unwrap().clone();
    owners.factory_process.process_null_handle = Some(ActorHandle::from_raw(0));
    owners.tick(&mut world, &runtime, &assets, 0);
    assert_eq!(world.arena.object(actor).unwrap(), &stopped);
    assert_eq!(world.rng, expected_rng);
    assert_eq!(owners.particles.free_count(), 399);
    assert_eq!(owners.particles.vector_cursor, 1);
    assert!(owners.particles.pending(1).unwrap().is_empty());
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
}

#[test]
fn production_selector36_dying_entry_runs_the_attached_effect_until_manager_disposal() {
    let (runtime, assets, mut world, _maps) = loaded();
    let actor = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.family_update_class(actor) == Ok(FamilyUpdateClass::Selector36))
        .unwrap();
    let profile = world.arena.family_behavior_profile(actor).unwrap();
    assert!(matches!(
        profile,
        FamilyBehaviorProfile::Family4236d0 { .. }
    ));
    assert_eq!(profile.descriptor(1).unwrap().change_address, 0x0043_fe40);
    assert_eq!(profile.descriptor(1).unwrap().update_address, 0x0043_5240);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    world.fade.performance.particle_load_factor = 65_536;
    let rng = world.rng;
    with_behavior(&mut world, &mut owners, &runtime, &assets, |arena, host| {
        FamilyBehaviorHost::change(host, arena, actor, profile, 1, true).unwrap();
    });
    assert_eq!(world.arena.family4236d0_actor(actor).unwrap().word_240, 0);
    assert_ne!(world.arena.object(actor).unwrap().base.flags & 0x100, 0);
    assert_ne!(world.rng, rng);
    assert!(owners.particles.free_count() < 400);

    // The real H13/46 constructor and immediate +24(0) update already ran
    // inside 00405420. The newly spawned referent has transferred its model
    // and callback memberships to this separate, retained helper allocation.
    let helper = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&object| world.arena.selector66(object).is_ok())
        .expect("actual dying child receives a selector66 effect helper");
    let helper_handle = world.arena.actor(helper).unwrap().handle;
    let referent_handle = world.arena.actor(helper).unwrap().owner_handle;
    let referent = world
        .arena
        .resolve_actor_handle(referent_handle)
        .unwrap()
        .unwrap();
    assert_ne!(referent, actor);
    assert_eq!(world.arena.selector66(helper).unwrap().elapsed_13c, 1);
    assert!(world
        .arena
        .selector66(helper)
        .unwrap()
        .saved_flags_138
        .is_some());
    assert_eq!(
        world
            .arena
            .actor(helper)
            .unwrap()
            .rotating
            .visual
            .model_word,
        world
            .arena
            .actor(referent)
            .unwrap()
            .rotating
            .visual
            .model_word
    );
    assert_eq!(world.arena.object(referent).unwrap().base.flags & 3, 0);

    // Supply movement and scheduling only. The next actual manager pass runs
    // the shared helper update, live terrain query, and both particle births.
    pause_controller(&mut world);
    only_callbacks(&mut world, &[helper]);
    world
        .arena
        .offset_selector66(helper, 5, Some([1, 0, 0]))
        .unwrap();
    let pending_before = owners.particles.pending(1).unwrap().clone();
    let free_before = owners.particles.free_count();
    let process_null = ActorHandle::from_raw(0x7654_3210);
    owners.factory_process.process_null_handle = Some(process_null);
    let mut expected_rng = world.rng;
    for _ in 0..2 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    owners.tick(&mut world, &runtime, &assets, 10_000);
    assert_eq!(world.rng, expected_rng);
    assert_eq!(owners.particles.free_count(), free_before - 2);
    let position = world
        .arena
        .object(helper)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(
        world.arena.object(referent).unwrap().base.position_words,
        Some(position)
    );
    let helper_state = world.arena.selector66(helper).unwrap();
    assert_eq!(helper_state.elapsed_13c, 10_001);
    assert_eq!(helper_state.previous_position_144, position.map(Some));
    assert!(!helper_state.update_stopped);
    let born: Vec<_> = owners
        .particles
        .pending(1)
        .unwrap()
        .iter()
        .copied()
        .filter(|id| !pending_before.contains(id))
        .collect();
    assert_eq!(born.len(), 2);
    let mut classes = Vec::new();
    for &id in &born {
        let particle = owners.particles.record(id).unwrap();
        classes.push(particle.read_dword(0).unwrap());
        assert_eq!(particle.read_dword(0x14).unwrap(), process_null.raw());
        assert_eq!(particle.position().unwrap(), position);
    }
    classes.sort_unstable();
    assert_eq!(classes, [0x0052_8854, 0x0052_8874]);

    // An explicitly expired timer enters the real queue after another smoke
    // birth. The following manager pass performs +30 and destruction, keeping
    // the separate referent and already constructed particles alive.
    let state = world.arena.selector66_mut(helper).unwrap();
    state.elapsed_13c = 6_000_000;
    state.idle_140 = 500_000;
    owners.tick(&mut world, &runtime, &assets, 0);
    assert!(world.arena.object(helper).is_ok());
    assert!(world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == helper));
    let retained_particles = owners.particles.pending(1).unwrap().clone();
    owners.tick(&mut world, &runtime, &assets, 0);
    assert!(world.arena.object(helper).is_err());
    assert_eq!(
        world.arena.resolve_actor_handle(helper_handle).unwrap(),
        None
    );
    assert_eq!(
        world.arena.resolve_actor_handle(referent_handle).unwrap(),
        Some(referent)
    );
    assert_eq!(world.arena.object(referent).unwrap().base.flags & 3, 3);
    assert_ne!(
        world.arena.object(referent).unwrap().base.flags & 0x0040_0000,
        0
    );
    for id in retained_particles {
        assert!(owners.particles.record(id).is_ok());
        assert!(owners.particles.pending(1).unwrap().contains(&id));
    }
}
