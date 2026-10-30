//! Production session loans, actual loaded families and the real damage chain.
use super::*;
use crate::{
    object_runtime::{ActorDamagePacket, ActorExtension, ActorHandle, DerivedActorClass, ObjectId},
    session_platform::particles::damage::{apply_world_actor_damage, WorldDamageOwners},
    world_furniture_effect::{resisted_effect_damage, DamageResistance},
};

fn loaded() -> (PortSession<Recording>, SessionState) {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    (session, state)
}

fn family(session: &PortSession<Recording>) -> ObjectId {
    let arena = &session.world().unwrap().arena;
    arena.actor_list().iter().copied().find(|actor| {
        let state = arena.actor(*actor).unwrap();
        state.health > 100 && state.word_c4 == 3 && matches!(&state.extension,
            ActorExtension::Derived(derived) if matches!(derived.class, DerivedActorClass::Family428650(_)))
    }).expect("the authored intro has live family actors")
}

fn packet_for(
    session: &PortSession<Recording>,
    actor: ObjectId,
    lethal: bool,
) -> (ActorDamagePacket, u16) {
    let world = session.world().unwrap();
    let state = world.arena.actor(actor).unwrap();
    let multiplier = if world.arena.object(actor).unwrap().base.flags & 0x4000 != 0 {
        256
    } else {
        world.mission.word_10.unwrap() as i16 as i32
    };
    for (channel, (threshold, resistance)) in state.damage_responses().into_iter().enumerate() {
        if channel == 2 {
            continue;
        }
        for strength in if lethal { 32767..=32767 } else { 1..=32767 } {
            let damage = resisted_effect_damage(
                strength as i16,
                DamageResistance {
                    threshold,
                    multiplier: resistance,
                },
                0,
            )
            .wrapping_mul(multiplier)
                >> 8;
            if (lethal && damage >= i32::from(state.health))
                || (!lethal && damage > 0 && damage < i32::from(state.health))
            {
                return (
                    ActorDamagePacket {
                        strength: strength as i16,
                        channel: channel as u8,
                        byte_03: Some(0),
                    },
                    damage as u16,
                );
            }
        }
    }
    panic!("authored family has no suitable damage channel");
}

fn hit(
    session: &mut PortSession<Recording>,
    state: &SessionState,
    actor: ObjectId,
    packet: ActorDamagePacket,
    source: ActorHandle,
) {
    with_owners(session, state, |arena, owners, rng| {
        apply_world_actor_damage(arena, actor, packet, source, owners, rng).unwrap();
    });
}

fn with_owners(
    session: &mut PortSession<Recording>,
    state: &SessionState,
    action: impl FnOnce(
        &mut crate::object_runtime::ObjectArena,
        &mut WorldDamageOwners<'_, '_>,
        &mut u32,
    ),
) {
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let world = session.world.as_mut().unwrap();
    let mut owners = WorldDamageOwners {
        grant_text: None,
        camera_settings: None,
        flash: None,
        mission: &mut world.mission,
        tasks: &mut session.tasks,
        terrain: &mut world.terrain,
        particles: &mut session.particles,
        sounds: &mut session.sounds,
        furniture: &mut session.furniture,
        residency: &session.residency,
        executable,
        factory_process: &mut session.factory_process,
        behavior_tables: &mut session.actor_behavior_tables,
        animation_clock: &session.process,
        session: state,
        particle_load_factor: world.fade.performance.particle_load_factor,
        behavior_factory: None,
        map: None,
    };
    action(&mut world.arena, &mut owners, &mut world.rng);
}

#[test]
fn combat_live_timed_damage_uses_family_reaction_without_instantaneous_suffix() {
    use crate::object_runtime::ActorTimedDamageRequest;
    use crate::session_platform::particles::damage::apply_timed_damage;
    let (mut session, state) = loaded();
    let actor = family(&session);
    let (packet, damage) = packet_for(&session, actor, false);
    let before = session.world().unwrap().arena.actor(actor).unwrap().clone();
    let task = session.tasks.owner(before.word_b4).unwrap().unwrap();
    let task_source = session.tasks.record(task).unwrap().source;
    let statistics = session
        .world()
        .unwrap()
        .mission
        .record_0c
        .as_ref()
        .unwrap()
        .clone();
    with_owners(&mut session, &state, |arena, owners, rng| {
        apply_timed_damage(
            arena,
            actor,
            ActorTimedDamageRequest {
                packet,
                source: before.handle,
                microseconds: 16,
                period_microseconds: 0,
            },
            owners,
            rng,
        )
        .unwrap();
    });
    let world = session.world().unwrap();
    assert_eq!(
        world.arena.actor(actor).unwrap().health,
        before.health - damage
    );
    assert_eq!(world.arena.actor(actor).unwrap().word_ca, Some(55));
    assert_eq!(session.tasks.record(task).unwrap().source, task_source);
    assert_eq!(
        world.mission.record_0c.as_ref().unwrap().as_ref(),
        statistics.as_ref()
    );
}

#[test]
fn combat_live_family_status_services_birth_real_trail_and_retire_actor() {
    use crate::object_runtime::{FamilyUpdateEffect, FamilyUpdateHost};
    use crate::session_platform::world_actor_update::WorldFamilyUpdateHost;
    let (mut session, state) = loaded();
    let actor = family(&session);
    let before = (0..400)
        .filter_map(|slot| session.particles.occupant(slot))
        .count();
    //This fixture's session is phase1. The explicit native network allocation
    //gate admits the trail independently; retirement then uses live offline0.
    session.factory_process.network_flag = 1;
    let sine = session.catalog.assets().sine_table().values;
    with_owners(&mut session, &state, |arena, owners, rng| {
        let mut host = WorldFamilyUpdateHost {
            owners: WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: None,
                mission: &mut *owners.mission,
                tasks: &mut *owners.tasks,
                terrain: &mut *owners.terrain,
                particles: &mut *owners.particles,
                sounds: &mut *owners.sounds,
                furniture: &mut *owners.furniture,
                residency: owners.residency,
                executable: owners.executable,
                factory_process: &mut *owners.factory_process,
                behavior_tables: &mut *owners.behavior_tables,
                animation_clock: owners.animation_clock,
                session: owners.session,
                particle_load_factor: owners.particle_load_factor,
                behavior_factory: None,
                map: None,
            },
            rng,
            sine_quarter: &sine,
        };
        host.effect(
            arena,
            actor,
            FamilyUpdateEffect::StatusTrail {
                particle_type: 2,
                position: [10, 20, 30],
                velocity: [1, 2, 3],
                flags: 0,
            },
        )
        .unwrap();
        assert_eq!(
            (0..400)
                .filter_map(|slot| host.owners.particles.occupant(slot))
                .count(),
            before + 1
        );
        let id = (0..400)
            .filter_map(|slot| host.owners.particles.occupant(slot))
            .find(|id| {
                host.owners
                    .particles
                    .record(*id)
                    .unwrap()
                    .position()
                    .unwrap()
                    == [10, 20, 30]
            })
            .unwrap();
        let particle = host.owners.particles.record(id).unwrap();
        assert_eq!(particle.read_dword(0).unwrap(), 0x0052_88f4);
        assert_eq!(
            [0x0a, 0x0c, 0x0e].map(|offset| particle.read_word(offset).unwrap()),
            [1, 2, 3]
        );
        host.owners.factory_process.network_flag = 0;
        host.effect(arena, actor, FamilyUpdateEffect::Retire)
            .unwrap();
        assert_eq!(arena.actor(actor).unwrap().word_c4, 1);
        assert!(host.owners.particles.record(id).is_ok());
    });
}

#[test]
fn projectile_live_family_damage_runs_reaction_and_clears_matching_task_source() {
    let (mut session, state) = loaded();
    let actor = family(&session);
    let (packet, damage) = packet_for(&session, actor, false);
    let before = session.world().unwrap().arena.actor(actor).unwrap().clone();
    let task = session.tasks.owner(before.word_b4).unwrap().unwrap();
    //47C744 only calls482C60(source,0) when the existing task source
    //matches. It clears that source; it never assigns the incoming handle.
    session.tasks.write_source(task, before.handle).unwrap();
    let stats = session
        .world()
        .unwrap()
        .mission
        .record_0c
        .as_ref()
        .unwrap()
        .clone();
    hit(&mut session, &state, actor, packet, before.handle);
    let world = session.world().unwrap();
    let after = world.arena.actor(actor).unwrap();
    assert_eq!(after.health, before.health - damage);
    assert_eq!(after.word_c4, 3);
    assert_eq!(after.word_ca, Some(55));
    assert_ne!(
        world.arena.object(actor).unwrap().base.dirty_word & 0x2000,
        0
    );
    assert_eq!(
        session.tasks.record(task).unwrap().source,
        session.factory_process.process_null_handle.unwrap()
    );
    assert_ne!(
        world.mission.record_0c.as_ref().unwrap().as_ref(),
        stats.as_ref()
    );
}

#[test]
fn projectile_live_family_lethal_damage_runs_state_one_placement_and_attribution() {
    let (mut session, state) = loaded();
    let actor = family(&session);
    let (packet, _) = packet_for(&session, actor, true);
    let before = session.world().unwrap().arena.actor(actor).unwrap().clone();
    let stats = session
        .world()
        .unwrap()
        .mission
        .record_0c
        .as_ref()
        .unwrap()
        .clone();
    hit(&mut session, &state, actor, packet, before.handle);
    let world = session.world().unwrap();
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 1);
    assert_eq!(world.arena.actor(actor).unwrap().health, 0);
    assert_eq!(
        world.arena.object(actor).unwrap().base.flags & 0x1000_0000,
        0
    );
    assert_ne!(
        world.mission.record_0c.as_ref().unwrap().as_ref(),
        stats.as_ref()
    );
    assert_eq!(world.arena.actor(actor).unwrap().handle, before.handle);
}

#[test]
fn projectile_damage_dispatch_bindings_match_retained_native_tables() {
    let (session, _) = fixture();
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    for table in [
        0x0052_0f34,
        0x0052_1164,
        0x0052_0b84,
        0x0052_0cc8,
        0x0052_1eac,
        0x0052_273c,
        0x0052_7ee0,
        0x0052_7058,
    ] {
        assert_eq!(executable.word(table + 0xb4).unwrap(), 0x0042_8040);
        assert_eq!(executable.word(table + 0xac).unwrap(), 0x0042_8140);
        assert_eq!(executable.word(table + 0x64).unwrap(), 0x0040_1300);
        assert_eq!(executable.word(table + 0x68).unwrap(), 0x0040_1300);
    }
    for table in [0x0052_0e58, 0x0051_f080, 0x0051_fbb0] {
        assert_eq!(executable.word(table + 0xb4).unwrap(), 0x0042_83e0);
    }
    for table in [0x0052_7c00, 0x0052_04d0, 0x0052_4ac0, 0x0052_6b50] {
        assert_eq!(executable.word(table + 0xb4).unwrap(), 0x0047_cbc0);
    }
    for table in [0x0052_0f34, 0x0052_1eac, 0x0052_273c] {
        assert_eq!(executable.word(table + 0x108).unwrap(), 0x0046_6d40);
    }
    for table in [
        0x0052_1164,
        0x0052_0b84,
        0x0052_0cc8,
        0x0052_7ee0,
        0x0052_7058,
    ] {
        assert_eq!(executable.word(table + 0x108).unwrap(), 0x0042_9c70);
    }
    assert_eq!(executable.word(0x0052_0860 + 0xb4).unwrap(), 0x0042_12f0);
    assert_eq!(executable.word(0x0052_7de8 + 0xb4).unwrap(), 0x0048_0c80);
}

/// The shared actor's, the mining rig's and the pickup spawner's tables bind
/// +AC = 00404600 (00479FA0 on the actor) and the null +68, so a lethal
/// state change only queues the actor's disposal.
#[test]
fn shared_table_deaths_only_dispose() {
    use crate::object_runtime::{ActorStateChangeRequest, ObjectList};
    let (mut session, state) = loaded();
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    for table in [0x0052_7c00, 0x0052_6714, 0x0052_6648] {
        assert_eq!(executable.word(table + 0xac).unwrap(), 0x0040_4600);
        assert_eq!(executable.word(table + 0x68).unwrap(), 0x0040_1300);
    }
    // A live mining rig in the loaded world, standing in the intro's terrain.
    static SINE: [u16; 4096] = [0; 4096];
    let arena = &mut session.world.as_mut().unwrap().arena;
    let actor = arena
        .allocate_selector52(crate::object_runtime::ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: crate::object_runtime::ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    let source = arena.actor(actor).unwrap().handle;
    with_owners(&mut session, &state, |arena, owners, rng| {
        let mut host =
            crate::session_platform::particles::damage::world_retirement_host(owners, rng);
        arena
            .change_actor_state(
                actor,
                ActorStateChangeRequest {
                    target_state: 3,
                    argument: 0,
                    source,
                },
                &mut host,
            )
            .unwrap();
        arena
            .change_actor_state(
                actor,
                ActorStateChangeRequest {
                    target_state: 1,
                    argument: 0,
                    source,
                },
                &mut host,
            )
            .unwrap();
    });
    let arena = &session.world().unwrap().arena;
    assert_eq!(arena.actor(actor).unwrap().word_c4, 1);
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor));
}

/// The Powerball's +98 (004864B0) on message 5: 0047CE90 on the ball, then
/// on the sender when that is resource 0xE. A live ball's death (00486420)
/// stops at its child; a ball already out of states 2/3 leaves only the
/// sender's retirement, which a shared sender's 00404600 completes.
#[test]
fn powerball_message_five_retires_a_partner_sender() {
    use crate::object_runtime::{ActorMessage, ObjectList};
    use crate::session_platform::world_actor_update::deliver_world_actor_message;
    let (mut session, state) = loaded();
    static SINE: [u16; 4096] = [0; 4096];
    let request = || crate::object_runtime::ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: crate::object_runtime::ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    };
    let arena = &mut session.world.as_mut().unwrap().arena;
    let ball = arena.allocate_selector57(request()).unwrap();
    let sender = arena.allocate_actor(request()).unwrap();
    let five = ActorMessage {
        sender: Some(sender),
        distance_squared: 0,
        code: 5,
        argument: 0,
    };
    let disposed = |session: &PortSession<Recording>, actor: ObjectId| {
        session
            .world()
            .unwrap()
            .arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == actor)
    };
    let states = |session: &PortSession<Recording>| {
        let arena = &session.world().unwrap().arena;
        (
            arena.actor(ball).unwrap().word_c4,
            arena.actor(sender).unwrap().word_c4,
        )
    };
    // A live ball's death holds; the sender is not reached.
    let set = |session: &mut PortSession<Recording>, ball_state: u32, resource: u16| {
        let arena = &mut session.world.as_mut().unwrap().arena;
        arena.stage_actor_for_tests(ball, ball_state, 39);
        arena.stage_actor_for_tests(sender, 3, resource);
    };
    set(&mut session, 3, 0xe);
    let mut result = Ok(());
    with_owners(&mut session, &state, |arena, owners, rng| {
        result = deliver_world_actor_message(arena, ball, five, owners, rng);
    });
    assert!(result.unwrap_err().contains("00486420"));
    assert_eq!(states(&session), (1, 3));
    // A ball out of states 2/3: another sender is left alone ...
    set(&mut session, 1, 0xd);
    with_owners(&mut session, &state, |arena, owners, rng| {
        deliver_world_actor_message(arena, ball, five, owners, rng).unwrap();
    });
    assert_eq!(states(&session), (1, 3));
    assert!(!disposed(&session, sender));
    // ... and resource 0xE dies through the shared 00404600.
    set(&mut session, 1, 0xe);
    with_owners(&mut session, &state, |arena, owners, rng| {
        deliver_world_actor_message(arena, ball, five, owners, rng).unwrap();
    });
    assert_eq!(states(&session), (1, 1));
    assert!(disposed(&session, sender));
    assert!(!disposed(&session, ball));
}

/// A selector-11 turret's death (`00402250`) sets off two blasts through
/// `004887D0`, which reach every actor in range through that actor's own
/// +B4. With a family actor beside it, the turret's death runs to its end and
/// the neighbour takes the blast. The retirement's damage owner used to admit
/// only another selector 11, so such a death stopped part-way: the turret
/// never finished dying, nor did whatever had set it off.
#[test]
fn a_turrets_death_blast_reaches_a_family_actor_beside_it() {
    let (mut session, state) = loaded();
    let turret = {
        let arena = &session.world().unwrap().arena;
        arena
            .actor_list()
            .iter()
            .copied()
            .find(|actor| {
                arena.selector11_actor(*actor).is_ok() && arena.actor(*actor).unwrap().word_c4 == 3
            })
            .expect("Level 1 has a live selector-11 turret")
    };
    let neighbour = family(&session);
    let world = session.world.as_mut().unwrap();
    let at = world
        .arena
        .object(turret)
        .unwrap()
        .base
        .position_words
        .unwrap();
    world
        .arena
        .set_position_for_tests(neighbour, [at[0].wrapping_add(40), at[1], at[2]]);
    let health = world.arena.actor(neighbour).unwrap().health;
    let (packet, _) = packet_for(&session, turret, true);
    let source = session
        .world()
        .unwrap()
        .arena
        .actor(neighbour)
        .unwrap()
        .handle;
    hit(&mut session, &state, turret, packet, source);
    let world = session.world().unwrap();
    assert_eq!(world.arena.actor(turret).unwrap().word_c4, 1);
    assert_eq!(world.arena.actor(turret).unwrap().health, 0);
    assert!(
        world.arena.actor(neighbour).unwrap().health < health,
        "{health} -> {}",
        world.arena.actor(neighbour).unwrap().health
    );
}
