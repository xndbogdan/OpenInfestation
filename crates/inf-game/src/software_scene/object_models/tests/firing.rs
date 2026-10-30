//! The actual draw-host bridge must use and commit SoftwareScene's live RNG
//! loan while its caller still holds the stale pre-object-pass word.

use super::*;
use crate::object_runtime::{
    ActorWeaponFireContext, Flag0ObjectDrawFailurePolicy, NativeHeapRef, ProcessActorBehaviorTables,
};
use crate::process_particles::{factories::ParticleFactoryProcessState, ProcessParticleRuntime};
use crate::process_sound::ProcessSoundRuntime;

fn queued_actor(
    arena: &mut ObjectArena,
    tables: &mut ProcessActorBehaviorTables,
    assets: &FrontendAssets,
    resident: &ResidentRuntime,
) -> (ObjectId, NativeHeapRef) {
    let source = original_class11_source(arena, assets, resident);
    let row = arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .table_14 as u32;
    // A supplied mutable descriptor routes this existing actual owner to the
    // same particle branch as the initial intro weapons, one queued group.
    for (offset, bytes) in [
        (0, 2_u16.to_le_bytes()),
        (2, 3000_u16.to_le_bytes()),
        (4, 57_u16.to_le_bytes()),
        (6, 1_u16.to_le_bytes()),
    ] {
        tables
            .write_process_bytes(row + offset, &bytes.map(Some))
            .unwrap();
    }
    tables
        .write_process_bytes(row + 8, &1_u32.to_le_bytes().map(Some))
        .unwrap();
    tables.write_process_bytes(row + 0x20, &[Some(0)]).unwrap();
    arena
        .queue_actor_weapon_direction(
            source,
            1,
            [1.0_f32.to_bits(), 0, 0],
            0,
            &mut ActorWeaponFireContext {
                tables,
                rng: &mut 23,
                process_velocity: None,
            },
        )
        .unwrap();
    let node = arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .fire_head_00
        .unwrap();
    (source, node)
}

#[test]
fn firing_bridge_continues_retained_model_rng_and_commits_its_draws() {
    let (assets, primitive_tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for prior_model_draws in [2, 3] {
        let mut arena = arena();
        let mut behavior_tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
        // Supplied mutable registry state activates the shared extra-effect
        // policy. Retail mode01's initial zero flags remain unchanged on disk.
        behavior_tables
            .write_process_bytes(0x0053_dbd4, &[Some(0x10)])
            .unwrap();
        let (source, _) = queued_actor(&mut arena, &mut behavior_tables, &assets, &resident);
        let clock = AnimationClock::default();
        let mut pixels = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                config(&resident).environment,
            ),
            resources: RenderResources::new(&resident),
            tables: &primitive_tables,
            frame: &mut pixels,
            config: config(&resident),
        })
        .unwrap();
        // Explicit retained model-pass prefix. This fixture tests loan custody,
        // not the original model callback's per-frame random-draw cadence.
        let caller_rng = 0x1020_3040;
        let mut retained = caller_rng;
        for _ in 0..prior_model_draws {
            crate::name_entry::name_entry_rng_next(&mut retained);
        }
        scene.set_process_rng(Some(retained));
        let mut expected = retained;
        let draw = crate::name_entry::name_entry_rng_next(&mut expected);
        let expect_success = draw & 1 != 0;
        let mut wrong = caller_rng;
        crate::name_entry::name_entry_rng_next(&mut wrong);
        assert_ne!(
            expected, wrong,
            "fixture must distinguish the caller's stale word"
        );
        let mut particles = ProcessParticleRuntime::default();
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let process = ParticleFactoryProcessState::native_startup();
        let result = SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            scene: &mut scene,
            animation: &clock,
            executable,
            attract_demo: &false,
            failure_policy: Flag0ObjectDrawFailurePolicy::Stop,
            effects: Some(SoftwareObjectEffectServices {
                tables: Some(&mut behavior_tables),
                pickup: None,
                factory_process: &process,
                particles: &mut particles,
                statistics: None,
                sounds: Some(&mut sounds),
                residency: &resident,
                phase: Some(4),
                custom: None,
                children: None,
                pulse: None,
                settings_7c: None,
                settings_70: None,
                network_initials: None,
                network_team_flag: None,
            }),
        }
        .drain_weapon_fire(&mut arena, source);
        assert_eq!(result.as_ref().ok(), Some(&1), "{result:?}");
        assert_eq!(caller_rng, 0x1020_3040);
        assert_eq!(
            particles.pending(3).unwrap().len(),
            1,
            "the factory's particle"
        );
        let pending: usize = (0..8)
            .filter_map(|category| particles.pending(category).ok())
            .map(|list| list.len())
            .sum();
        // The scene's loan, not the caller's stale word, carries the shot's
        // draws: an odd roll ends there; an even one also flashes 00489D90(2)
        // at the emission point, with its own draws and particle.
        let committed = scene.process_rng().unwrap();
        if expect_success {
            assert_eq!(committed, expected);
            assert_eq!(pending, 1);
        } else {
            assert_ne!(committed, expected);
            assert_eq!(pending, 2);
        }
        let behavior = arena
            .derived_actor(source)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert!(behavior.fire_head_00.is_none());
        assert!(behavior.firing_stop_00.is_none());
        assert_eq!(sounds.pending_len(), 1);
    }
}

#[test]
fn firing_bridge_without_process_rng_stops_before_node_or_factory_effects() {
    let (assets, primitive_tables, mut resident) = fixture();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut arena = arena();
    let mut behavior_tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let (source, node) = queued_actor(&mut arena, &mut behavior_tables, &assets, &resident);
    let original = arena.actor_service_heap().weapon_fire_node(node).unwrap();
    let clock = AnimationClock::default();
    let mut pixels = SoftwareFrame::new(640, 480, [0, 0, 0, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            config(&resident).environment,
        ),
        resources: RenderResources::new(&resident),
        tables: &primitive_tables,
        frame: &mut pixels,
        config: config(&resident),
    })
    .unwrap();
    assert_eq!(scene.process_rng(), None);
    let mut particles = ProcessParticleRuntime::default();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let process = ParticleFactoryProcessState::native_startup();
    let result = SoftwareObjectDrawHost {
        terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
        scene: &mut scene,
        animation: &clock,
        executable,
        attract_demo: &false,
        failure_policy: Flag0ObjectDrawFailurePolicy::Stop,
        effects: Some(SoftwareObjectEffectServices {
            tables: Some(&mut behavior_tables),
            pickup: None,
            factory_process: &process,
            particles: &mut particles,
            statistics: None,
            sounds: Some(&mut sounds),
            residency: &resident,
            phase: Some(4),
            custom: None,
            children: None,
            pulse: None,
            settings_7c: None,
            settings_70: None,
            network_initials: None,
            network_team_flag: None,
        }),
    }
    .drain_weapon_fire(&mut arena, source);
    assert!(matches!(
        result,
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::WeaponFire { .. }
        ))
    ));
    assert_eq!(
        arena.actor_service_heap().weapon_fire_node(node).unwrap(),
        original
    );
    assert!(particles.pending(3).unwrap().is_empty());
    assert_eq!(sounds.pending_len(), 0);
    assert_eq!(scene.process_rng(), None);
}
