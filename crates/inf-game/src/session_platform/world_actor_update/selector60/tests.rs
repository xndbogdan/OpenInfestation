//! The actual intro spawn rows, retained H14 commands and live factory.
use super::*;
use crate::{
    object_runtime::{ActorExtension, DerivedActorClass, ObjectArena, ObjectId},
    session_platform::{
        world_actor_factory::{RequiredListenerEffects, WorldActorChildHost},
        world_actor_update::WorldSelector60Host,
    },
};

fn loaded() -> PortSession<Recording> {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
}

fn with_host(
    session: &mut PortSession<Recording>,
    action: impl FnOnce(&mut ObjectArena, &mut WorldSelector60Host<'_, '_>, &mut u32),
) {
    let assets = session.catalog.assets();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let world = session.world.as_mut().unwrap();
    let mut effects = RequiredListenerEffects;
    let mut host = WorldSelector60Host {
        factory: WorldActorChildHost {
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
            listener_effects: &mut effects,
        },
        terrain_source: &world.terrain_source,
        instances: &world.report.instances,
        mission: &world.mission,
    };
    action(&mut world.arena, &mut host, &mut world.rng);
}

fn arm(arena: &mut ObjectArena) -> ObjectId {
    let actor = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&id| arena.selector60_actor(id).is_ok())
        .unwrap();
    // Explicit supplied scheduling, not an inferred controller deadline.
    arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived")
            };
            derived.word_144 = 0;
            let DerivedActorClass::Selector60(spawner) = &mut derived.class else {
                panic!("selector60")
            };
            spawner.word_1a4 = 1;
            spawner.word_194 = 0;
            spawner.word_19e = u16::MAX;
        })
        .unwrap();
    actor
}

#[test]
fn selector60_live_spawn_uses_retained_commands_and_initializes_before_transform() {
    let mut session = loaded();
    with_host(&mut session, |arena, host, rng| {
        let actor = arm(arena);
        let before = arena.actor_list().to_vec();
        arena
            .run_selector60_update(actor, 100_000, host, rng)
            .unwrap();
        let children: Vec<_> = arena
            .actor_list()
            .iter()
            .copied()
            .filter(|id| !before.contains(id))
            .collect();
        assert_eq!(
            children.len(),
            1,
            "real first spawn row must create one actor"
        );
        let child = children[0];
        assert_eq!(arena.object(child).unwrap().base.placement_word, u16::MAX);
        assert_ne!(
            arena.object(child).unwrap().base.flags & crate::object_runtime::AFTER_LOAD_FLAG,
            0
        );
        assert!(!arena.selector60_actor(actor).unwrap().update_stopped);
        // A newborn from an actual placement command chain carries native
        // constructor/instance state, not the naked dynamic-factory defaults.
        let resource = arena.object(child).unwrap().base.resource_word;
        let original = before
            .into_iter()
            .find(|&id| arena.object(id).unwrap().base.resource_word == resource)
            .expect("authored source placement remains live");
        assert_eq!(
            arena.actor(child).unwrap().word_b4,
            arena.actor(original).unwrap().word_b4
        );
        assert_eq!(
            arena.actor(child).unwrap().maximum_health,
            arena.actor(original).unwrap().maximum_health
        );
        assert_ne!(
            arena.actor(child).unwrap().handle,
            arena.actor(original).unwrap().handle
        );
    });
}

#[test]
fn selector60_live_residency_failure_stops_before_factory_without_retrying_census() {
    let mut session = loaded();
    // Preserve the retained row/H14 owners but replace current residency.
    let state = session.world.as_mut().unwrap();
    arm(&mut state.arena);
    session
        .residency
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    with_host(&mut session, |arena, host, rng| {
        let actor = arena
            .actor_list()
            .iter()
            .copied()
            .find(|&id| arena.selector60_actor(id).is_ok())
            .unwrap();
        let before = arena.len();
        let error = arena
            .run_selector60_update(actor, 100_000, host, rng)
            .unwrap_err();
        assert!(error.to_string().contains("H14"), "{error}");
        assert_eq!(arena.len(), before);
        let stopped = arena.selector60_actor(actor).unwrap().clone();
        let rng_before = *rng;
        assert!(arena
            .run_selector60_update(actor, 100_000, host, rng)
            .is_err());
        assert_eq!(arena.selector60_actor(actor).unwrap(), &stopped);
        assert_eq!(*rng, rng_before);
    });
}

#[test]
fn selector60_placement_factory_rejection_precedes_suffix_resolution() {
    use crate::{
        object_runtime::ActorChildResource,
        session_platform::world_actor_factory::{
            WorldActorFactoryRequest, WorldFactoryInitialization, WorldFactoryPosition,
        },
    };
    let mut session = loaded();
    with_host(&mut session, |arena, host, rng| {
        let count = arena.len();
        let before_rng = *rng;
        let result = host
            .factory
            .create_object(
                arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(0x1234_ffff),
                    position: WorldFactoryPosition::Local([1, 2, 3]),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: WorldFactoryInitialization::Placement {
                        placement: u32::MAX,
                        rotation: [5, 6, 7],
                        suffix_offset: u32::MAX,
                        instances: host.instances,
                    },
                },
                rng,
            )
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(arena.len(), count);
        assert_eq!(*rng, before_rng);
    });
}
