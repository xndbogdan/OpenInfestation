//! The pair pass's class admission over the loaded Level 1.

use super::*;
use crate::object_runtime::{ActorChildResource, ActorExtension};
use crate::session_platform::world_actor_factory::{
    RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
    WorldFactoryInitialization, WorldFactoryPosition,
};
use crate::session_platform::world_contact::{pair_class, PairClass};

/// Level 1's selector-6 actor (resource 6, the shared table 00527C00,
/// collidable and static) meets other actors in the pair pass. Its +A0
/// 00402720 and +A4 00405570 are the no-op bodies the gate's and factory's
/// arms already model, so the pass admits it instead of stopping.
#[test]
fn shared_prefix_actors_have_a_pair_class() {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    let assets = session.catalog.assets();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let world = session.world.as_mut().unwrap();
    let shared = WorldActorChildHost {
        residency: &session.residency,
        executable,
        process: &session.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: session.process.animation().value(),
        sine: &assets.sine_table().values,
        math: &math,
        camera_settings: Some(session.platform.world_tick_inputs().camera_settings),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut RequiredListenerEffects,
    }
    .create_object(
        &mut world.arena,
        WorldActorFactoryRequest {
            resource: ActorChildResource::Full(6),
            position: WorldFactoryPosition::Local([0x5000, 0, 0x5000]),
            spawn_mode_1: false,
            network_handle: None,
            initialization: WorldFactoryInitialization::Bare,
        },
        &mut world.rng,
    )
    .unwrap()
    .unwrap();
    let arena = &world.arena;
    assert!(matches!(
        arena.actor(shared).unwrap().extension,
        ActorExtension::Shared
    ));
    assert_eq!(pair_class(arena, shared), Some(PairClass::SharedPrefix));
}
