//! A factory particle's retirement (004661B0) through the loaded world's
//! production particle tick.
use super::*;
use crate::{
    object_runtime::{ObjectClass, ObjectList},
    process_particles::{
        factories::{
            ParticleFactoryContext, ParticleFactoryKind, ParticleFactoryRequest,
            ParticleFactorySources, ParticleVelocitySource,
        },
        ParticleVectorBytes,
    },
    session_platform::particles::factory::WorldParticleFactoryHost,
};

fn gameplay() -> (PortSession<Recording>, SessionState) {
    let (mut session, mut controller) = fixture();
    session
        .residency
        .apply(crate::ResidencyRequest::Select {
            selection: crate::ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    session.boot_frontend().unwrap();
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut session).unwrap();
    let mut state = controller.state().clone();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    // Particles go to the pool only in gameplay's phase.
    assert!(controller.request_phase(SessionPhase::Gameplay));
    controller.transition(&mut session).unwrap();
    (session, controller.state().clone())
}

/// The selector-22 visuals: the blast resource 0x36 builds.
fn blasts(session: &PortSession<Recording>) -> usize {
    let arena = &session.world().unwrap().arena;
    let mut seen = Vec::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
    ] {
        for node in arena.list(list) {
            if matches!(
                arena.object(node.object).map(|object| &object.class),
                Ok(ObjectClass::Selector22(_))
            ) && !seen.contains(&node.object)
            {
                seen.push(node.object);
            }
        }
    }
    seen.len()
}

/// A spent grenade's spark is 004665D0's selector 1. When it retires,
/// 004661CD builds the blast 0x36 where it is (a Helniazine run had
/// stopped sixteen there); any other selector births central particle 4.
#[test]
fn a_grenade_spark_leaves_a_blast_where_it_retires() {
    let (mut session, state) = gameplay();
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let world = session.world.as_mut().unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut point = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    // Clear of the vehicle, as its own sparks fly, and above the ground.
    point[0] = point[0].wrapping_add(3000);
    point[1] = (world.terrain.height_at([point[0], point[2]]) as u16).wrapping_add(200);
    let owner = world.arena.actor(vehicle).unwrap().handle;
    let mut process = session.factory_process;
    let spark = session
        .particles
        .create_factory_particle(
            ParticleFactoryRequest {
                kind: ParticleFactoryKind::Factory4665D0,
                selector: 1,
                source_handle: owner,
                flags: 0,
                velocity_source: ParticleVelocitySource::Separate,
            },
            &mut ParticleFactoryContext {
                process: &mut process,
                rng: &mut world.rng,
                executable,
            },
            &mut ParticleFactorySources {
                position: ParticleVectorBytes::from_words(point),
                separate_velocity: Some(ParticleVectorBytes::from_words([0; 3])),
            },
            &mut WorldParticleFactoryHost { session: &state },
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        session
            .particles
            .record(spark)
            .unwrap()
            .read_dword(0x3c)
            .unwrap(),
        1
    );
    let before = blasts(&session);
    for _ in 0..40 {
        session.tick_process_particles(125_000, &state);
        assert!(session.particles.stopped_phase(spark).is_none());
        if blasts(&session) != before {
            break;
        }
    }
    assert_eq!(blasts(&session), before + 1);
}
