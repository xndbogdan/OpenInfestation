//! Real radius packet, loaded family and production damage/task composition.
use super::*;
use crate::{
    object_runtime::{ActorExtension, ActorHandle, DerivedActorClass, ObjectId},
    process_particles::ParticleId,
    session_platform::particles::{
        damage::WorldDamageOwners,
        radius::{apply_radius_effect, WorldRadiusOrigin},
    },
    world_furniture_effect::{resisted_effect_damage, DamageResistance},
    world_radius_effect::RadiusEffectBlock,
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

fn isolated_family(session: &mut PortSession<Recording>, alternate: bool) -> (ObjectId, u32, u16) {
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let world = session.world.as_mut().unwrap();
    let (actor, packet, damage) = world.arena.actor_list().iter().find_map(|&id| {
        let actor = world.arena.actor(id).unwrap();
        if actor.word_c4 != 3 || !matches!(&actor.extension,
            ActorExtension::Derived(derived) if matches!(derived.class, DerivedActorClass::Family428650(_))) {
            return None;
        }
        let multiplier = if world.arena.object(id).unwrap().base.flags & 0x4000 != 0 {
            256
        } else { world.mission.word_10.unwrap() as i16 as i32 };
        [0x539eb8, 0x539ea8].into_iter().find_map(|packet| {
            let block = RadiusEffectBlock::from_authored_bytes(executable.mapped(packet, 10).unwrap().bytes.try_into().unwrap());
            let (threshold, multiplier_channel) = actor.damage_responses()[usize::from(block.channel)];
            let damage = resisted_effect_damage(block.strength, DamageResistance {threshold, multiplier: multiplier_channel}, 0)
                .wrapping_mul(multiplier) >> 8;
            (damage > 0 && damage < i32::from(actor.health)).then_some((id, packet, damage as u16))
        })
    }).expect("intro family admits an authored radius packet without lethal transition");
    for id in world.arena.actor_list().to_vec() {
        let flags = world.arena.object(id).unwrap().base.flags;
        let flags = if id == actor {
            flags | 4 | 8 | if alternate { 0x8000_0000 } else { 0 }
        } else {
            flags & !4
        };
        world.arena.apply_payload_flags(id, flags).unwrap();
    }
    let position = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    session
        .particles
        .record_mut(ParticleId::Scratch)
        .unwrap()
        .write_position(position)
        .unwrap();
    (actor, packet, damage)
}

fn apply(
    session: &mut PortSession<Recording>,
    state: &SessionState,
    packet: u32,
    source: ActorHandle,
) -> Result<(), String> {
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let world = session.world.as_mut().unwrap();
    apply_radius_effect(
        &mut world.arena,
        crate::world_radius_effect::WorldRadiusEffectRequest {
            origin: WorldRadiusOrigin::Particle {
                particle: ParticleId::Scratch,
                block_address: packet,
            },
            damage_source: source,
            replicate: true,
        },
        &mut WorldDamageOwners {
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
        },
        &mut world.rng,
    )
}

#[test]
fn projectile_radius_applies_actual_family_damage_and_clears_matching_task_source() {
    let (mut session, state) = loaded();
    let (actor, packet, damage) = isolated_family(&mut session, false);
    let before = session.world().unwrap().arena.actor(actor).unwrap().clone();
    let task = session.tasks.owner(before.word_b4).unwrap().unwrap();
    session.tasks.write_source(task, before.handle).unwrap();
    apply(&mut session, &state, packet, before.handle).unwrap();
    let after = session.world().unwrap().arena.actor(actor).unwrap();
    assert_eq!(after.health, before.health - damage);
    assert_eq!(after.word_ca, Some(55));
    assert_eq!(
        session.tasks.record(task).unwrap().source,
        session.factory_process.process_null_handle.unwrap()
    );
}

#[test]
fn projectile_radius_reaches_network_only_after_actual_alternate_hit() {
    let (mut session, state) = loaded();
    let (actor, packet, _) = isolated_family(&mut session, true);
    let before = session.world().unwrap().arena.actor(actor).unwrap().clone();
    // The blast goes to the remote actor's machine as message 19; offline
    // retail's 00492500 drops it, and the port's queue is never sent.
    apply(&mut session, &state, packet, before.handle).unwrap();
    let sends = session.world.as_mut().unwrap().arena.take_network_sends();
    assert!(
        matches!(
            sends.as_slice(),
            [(19, crate::network::protocol::GameSend::Effect { value, .. })] if *value == before.handle.raw()
        ),
        "{sends:?}"
    );
    assert_eq!(
        session.world().unwrap().arena.actor(actor).unwrap().health,
        before.health
    );
    assert_eq!(session.factory_process.network_flag, 0);
}

#[test]
fn projectile_actor_wrapper_runs_real_damage_task_and_hit_matrix_in_place() {
    use crate::{
        process_particles::{
            impact::collision::{ParticleCollisionContext, ParticleProjectileHost},
            update::{ParticleTerrainContext, ParticleUpdateBirthInputs, ParticleUpdateRequest},
        },
        session_platform::particles::combat::WorldProjectileHost,
        world_statistics::WorldStatistics,
    };
    let (mut session, state) = loaded();
    let (actor, _, _) = isolated_family(&mut session, false);
    let assets = session.catalog.assets();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let world = session.world.as_mut().unwrap();
    let before = world.arena.actor(actor).unwrap().clone();
    let multiplier = if world.arena.object(actor).unwrap().base.flags & 0x4000 != 0 {
        256
    } else {
        world.mission.word_10.unwrap() as i16 as i32
    };
    //Actual factory+08 primary packets, beginning with authored type2. Keep
    //this a concrete normal nonlethal wrapper test, not a supplied B4 callback.
    let (packet, raw_damage, damage) = [0x539f28, 0x539f38, 0x539f48, 0x539ec8, 0x539f08, 0x539f18]
        .into_iter()
        .find_map(|parameters| {
            let packet = executable.word(parameters + 8).unwrap();
            let bytes = executable.mapped(packet, 4).unwrap().bytes;
            let strength = i16::from_le_bytes([bytes[0], bytes[1]]);
            let (threshold, resistance) = before.damage_responses()[usize::from(bytes[2])];
            let raw_damage = resisted_effect_damage(
                strength,
                DamageResistance {
                    threshold,
                    multiplier: resistance,
                },
                0,
            );
            let amount = raw_damage.wrapping_mul(multiplier) >> 8;
            (amount > 0 && amount < i32::from(before.health)).then_some((
                packet,
                raw_damage as u32,
                amount as u16,
            ))
        })
        .expect("loaded family admits a real primary projectile packet");
    let task = session.tasks.owner(before.word_b4).unwrap().unwrap();
    session.tasks.write_source(task, before.handle).unwrap();
    let null_source = session.factory_process.process_null_handle.unwrap();
    let categories = world
        .arena
        .retirement_attribution(actor, before.handle)
        .unwrap();
    let record = world.mission.record_0c.as_mut().unwrap();
    let stats = WorldStatistics::from_record(record);
    let hits_before = stats.particle_hit_count(categories);
    let damage_before = stats.damage_total(categories);
    let matrix_before = record[0x28..0x8c].to_vec();
    session
        .particles
        .record_mut(ParticleId::Scratch)
        .unwrap()
        .write_dword(0x14, before.handle.raw())
        .unwrap();
    session
        .particles
        .record_mut(ParticleId::Scratch)
        .unwrap()
        .write_dword(0x34, packet)
        .unwrap();
    let sine_values = session.catalog.assets().sine_table().values;
    let mut furniture_hits = Vec::new();
    let mut host = WorldProjectileHost {
        grant_text: None,
        furniture_hits: &mut furniture_hits,
        arena: &mut world.arena,
        mission: &mut world.mission,
        tasks: &mut session.tasks,
        sounds: &mut session.sounds,
        furniture: &mut session.furniture,
        residency: &session.residency,
        model_fallback: None,
        model_process: &session.model_process,
        furniture_query: &mut session.furniture_query,
        furniture_draw: &session.furniture_draw,
        terrain_source: &world.terrain_source,
        behavior_tables: &mut session.actor_behavior_tables,
        animation_clock: &session.process,
        session: &state,
        listeners: &mut world.listeners,
        sine: &sine_values,
        world_flags: world.world_flags,
        contacts: 0,
        impact_births: 0,
        ground_bursts: 0,
    };
    let mut request = ParticleUpdateRequest {
        delta_microseconds: 0,
        load_factor: world.fade.performance.particle_load_factor as i32,
        birth_inputs: ParticleUpdateBirthInputs {
            network_active: session.factory_process.network_flag != 0,
            phase: Some(u32::from(state.current_phase().id())),
            process: &mut session.factory_process,
        },
        terrain: Some(ParticleTerrainContext {
            terrain: &mut world.terrain,
            sine_quarter: &assets.sine_table().values,
            wave_clock: session.process.animation().value(),
            world_flags: world.world_flags,
            impact_selectors: world.particle_impact_selectors.unwrap(),
        }),
    };
    host.actor_impact(
        ParticleId::Scratch,
        before.handle,
        packet,
        before.handle.raw(),
        &mut ParticleCollisionContext {
            particles: &mut session.particles,
            request: &mut request,
            executable,
            rng: &mut world.rng,
        },
    )
    .unwrap();
    assert_eq!(
        host.arena.actor(actor).unwrap().health,
        before.health - damage
    );
    assert_eq!(host.tasks.record(task).unwrap().source, null_source);
    let record = host.mission.record_0c.as_mut().unwrap();
    let stats = WorldStatistics::from_record(record);
    assert_eq!(
        stats.particle_hit_count(categories),
        hits_before.wrapping_add(1)
    );
    assert_eq!(
        stats.damage_total(categories),
        damage_before.wrapping_add(raw_damage)
    );
    let mut expected = matrix_before;
    let at = ((categories.target as usize) * 5 + categories.source as usize) * 4;
    expected[at..at + 4].copy_from_slice(&hits_before.wrapping_add(1).to_le_bytes());
    assert_eq!(&record[0x28..0x8c], expected);
}
