//! Real service loans with supplied constructor/callback scheduling. These
//! fixtures do not claim a complete actor+24, mission or native heap history.
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorServiceResourceState, ActorTerrainInputs,
    BehaviorActorResourceRequest, BehaviorComponentQuery, BehaviorParticleServices,
    DerivedActorConstructionRequest, FixtureActorServiceAllocation, NativeTarget, ObjectClassKind,
};
use crate::{frontend_assets::FrontendAssets, Installation, ResidencyRequest};

fn with_host<T>(
    run: impl FnOnce(&mut ObjectArena, &mut WorldFamilyBehaviorHost<'_, '_>) -> T,
) -> T {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
    .unwrap();
    let mut effects = crate::world_backend::EmptyListenerEffects;
    let mut factory_process = ParticleFactoryProcessState::native_startup();
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let metadata = crate::world_backend::load_offline_world(
        &mut residency,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut owner,
            listener_effects: &mut effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(&factory_process),
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let mut world = crate::world_backend::WorldSession::from_loaded(owner, metadata);
    let clock = crate::frontend_camera::AnimationClock::default();
    let mut behavior_tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let mut tasks = ProcessTaskRuntime::default();
    tasks
        .retain(world.arena.actor_handles().unwrap().null_handle())
        .unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut session = crate::session::SessionState::default();
    let mut furniture = crate::world_furniture_effect::FurnitureEffectQueue::default();
    let mut voices = crate::process_sound::RequiredProcessSoundVoiceTeardown;
    let mut selector35_process = crate::object_runtime::Selector35RouteProcessState::default();
    let mut host = WorldFamilyBehaviorHost {
        grant_text: None,
        residency: &residency,
        executable,
        animation_clock: &clock,
        factory_process: &mut factory_process,
        session: &mut session,
        behavior_tables: &mut behavior_tables,
        selector35_process: &mut selector35_process,
        tasks: &mut tasks,
        particles: &mut particles,
        sounds: &mut sounds,
        process_rng: &mut world.rng,
        terrain_source: &world.terrain_source,
        terrain: &mut world.terrain,
        dying: WorldFamilyDyingContext {
            mission: &mut world.mission,
            furniture: &mut furniture,
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
    run(&mut world.arena, &mut host)
}

fn constructor_actor(arena: &mut ObjectArena, host: &WorldFamilyBehaviorHost<'_, '_>) -> ObjectId {
    let mut constructor_rng = 0x8765_4321;
    arena
        .allocate_family4236d0_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    // Supplied callback-domain point. No captured position is behavior.
                    allocation_position: Some([1200, 200, 1400]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: host.sine_quarter,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut constructor_rng,
        )
        .unwrap()
}

#[test]
fn constants_and_literal_noop_bypass_actor_backing_without_rng() {
    with_host(|arena, host| {
        let nonactor = arena.allocate(ObjectClassKind::Base);
        let before = format!("{arena:?}");
        let rng = *host.process_rng;
        let profile = FamilyBehaviorProfile::Family4236d0 { mode: 0 };
        assert_eq!(
            FamilyBehaviorHost::score(host, arena, nonactor, profile, 2, true, i32::MIN).unwrap(),
            1
        );
        assert_eq!(
            FamilyBehaviorHost::score(host, arena, nonactor, profile, 0, false, i32::MAX).unwrap(),
            0
        );
        FamilyBehaviorHost::update(
            host,
            arena,
            nonactor,
            FamilyBehaviorProfile::Family4236d0 { mode: 1 },
            5,
            u32::MAX,
        )
        .unwrap();
        // The false point-entry branch likewise bypasses actor/source allocation.
        FamilyBehaviorHost::change(host, arena, nonactor, profile, 2, false).unwrap();
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(*host.process_rng, rng);
    });
}

#[test]
fn periodic_temporary_retains_actual_captured_identity_until_explicit_release() {
    with_host(|arena, host| {
        let nonactor = arena.allocate(ObjectClassKind::Base);
        let rng = *host.process_rng;
        let input = FamilyBehaviorHost::periodic_input_factory(host, arena, nonactor, 0x0042_7640)
            .unwrap()
            .unwrap();
        assert_eq!(
            arena
                .actor_service_heap()
                .radius_score_actor(input)
                .unwrap(),
            nonactor
        );
        assert_eq!(
            arena.actor_service_heap().requested_bytes(input).unwrap(),
            8
        );
        assert_eq!(arena.actor_service_heap().retained_bytes(input).unwrap(), 8);
        let profile = FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        };
        let mut record = profile.periodic_record(1).unwrap();
        record.actor_offset ^= 1; // Explicit unowned literal; no callback executes.
        assert!(matches!(
            FamilyBehaviorHost::periodic_probe(
                host,
                arena,
                nonactor,
                FamilyPeriodicProbeRequest {
                    profile,
                    index: 1,
                    record
                },
                Some(input)
            ),
            Err(WorldFamilyBehaviorError::RequiredCallback { .. })
        ));
        assert_eq!(
            arena
                .actor_service_heap()
                .radius_score_actor(input)
                .unwrap(),
            nonactor
        );
        assert_eq!(*host.process_rng, rng);
        FamilyBehaviorHost::release_periodic_input(host, arena, input).unwrap();
        assert_eq!(
            arena.actor_service_heap().state(input).unwrap(),
            ActorServiceResourceState::Released
        );
        assert_eq!(arena.actor_service_heap().retained_bytes(input).unwrap(), 0);
        assert!(arena
            .actor_service_heap()
            .radius_score_actor(input)
            .is_err());
    });
}

#[test]
fn point_change_reads_actual_process_after_allocation_and_retains_reached_status_on_error() {
    with_host(|arena, host| {
        let actor = constructor_actor(arena, host);
        let profile = FamilyBehaviorProfile::Family4236d0 { mode: 0 };
        let rng = *host.process_rng;
        let expected = arena.object(actor).unwrap().base.position_words;
        // This owner differs deliberately from the registry's sentinel.
        host.factory_process.process_null_handle = Some(ActorHandle::from_raw(0x1020_3040));
        FamilyBehaviorHost::change(host, arena, actor, profile, 2, true).unwrap();
        let reference = arena.derived_actor(actor).unwrap().target_148.unwrap();
        let NativeTarget::FixedPosition(target) = arena.native_target(reference).unwrap() else {
            panic!("actual fixed-position constructor");
        };
        assert_eq!(target.handle_04.raw(), 0x1020_3040);
        assert_eq!(target.focus_08, expected);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_228,
            0x0053_11e4
        );
        assert_eq!(*host.process_rng, rng);

        let other = constructor_actor(arena, host);
        let attempts = arena.actor_service_heap().attempts().len();
        host.factory_process.process_null_handle = None;
        assert!(matches!(
            FamilyBehaviorHost::change(host, arena, other, profile, 2, true),
            Err(WorldFamilyBehaviorError::Owner {
                phase: "descriptor point change",
                ..
            })
        ));
        assert_eq!(arena.actor_service_heap().attempts().len(), attempts + 1);
        assert_eq!(
            arena.family428650_actor(other).unwrap().word_228,
            0x0053_11e4
        );
        assert_eq!(arena.derived_actor(other).unwrap().target_148, None);
        assert_eq!(*host.process_rng, rng);
        // Native allocationNULL precedes even the absent process read.
        let null_actor = constructor_actor(arena, host);
        arena
            .actor_service_heap_mut()
            .script([FixtureActorServiceAllocation::Null]);
        FamilyBehaviorHost::change(host, arena, null_actor, profile, 2, true).unwrap();
        assert_eq!(arena.derived_actor(null_actor).unwrap().target_148, None);
    });
}

#[test]
fn canonical_components_and_unwritten_actor_resource_keep_actual_owner_prefixes() {
    with_host(|arena, host| {
        let actor = constructor_actor(arena, host);
        let nonactor = arena.allocate(ObjectClassKind::Base);
        let before = format!("{arena:?}");
        let rng = *host.process_rng;
        let mut context = BehaviorPredicateContext {
            tables: host.behavior_tables,
            process: host.factory_process,
            rng: host.process_rng,
        };
        let mut services = particles::WorldSelectionServices {
            tasks: host.tasks,
            animation_clock: host.animation_clock,
            session: &mut *host.session,
            resources: None,
        };
        assert_eq!(
            services
                .query_component(arena, &mut context, BehaviorComponentQuery::Actor18(actor))
                .unwrap(),
            Some(actor)
        );
        assert_eq!(
            services
                .query_component(
                    arena,
                    &mut context,
                    BehaviorComponentQuery::Actor18(nonactor)
                )
                .unwrap(),
            None
        );
        assert_eq!(
            services
                .query_component(
                    arena,
                    &mut context,
                    BehaviorComponentQuery::Component68(actor)
                )
                .unwrap(),
            None
        );
        assert_eq!(
            services
                .query_component(
                    arena,
                    &mut context,
                    BehaviorComponentQuery::Component6c(actor)
                )
                .unwrap(),
            None
        );
        assert!(matches!(
            services.create_actor_resource(
                arena,
                &mut context,
                BehaviorActorResourceRequest {
                    resource: 7,
                    null_handle: ActorHandle::from_raw(0),
                    scratch_bytes: [None; 8],
                }
            ),
            Err(WorldFamilyBehaviorError::RequiredActorResource460310 {
                resource: 7,
                scratch_bytes: [None, None, None, None, None, None, None, None]
            })
        ));
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(*context.rng, rng);
    });
}

#[test]
fn selected_family_particle_adapter_reads_actual_current_session_not_requested_phase() {
    use crate::object_runtime::{
        BehaviorCreationRequest, BehaviorParticleAdapter, BehaviorPredicateHost, BehaviorTemporary,
    };
    use crate::process_particles::ParticleId;
    with_host(|arena, host| {
        assert_eq!(
            host.session.current_phase(),
            crate::session::SessionPhase::Title
        );
        assert!(host
            .session
            .request_phase(crate::session::SessionPhase::Gameplay));
        let before_actor = format!("{arena:?}");
        let before_rng = *host.process_rng;
        let before_free = host.particles.free_count();
        let mut context = BehaviorPredicateContext {
            tables: host.behavior_tables,
            process: host.factory_process,
            rng: host.process_rng,
        };
        let mut services = particles::WorldSelectionServices {
            tasks: host.tasks,
            animation_clock: host.animation_clock,
            session: &mut *host.session,
            resources: None,
        };
        let mut adapter = BehaviorParticleAdapter {
            particles: host.particles,
            executable: host.executable,
            services: &mut services,
        };
        let result = BehaviorPredicateHost::create(
            &mut adapter,
            arena,
            &mut context,
            BehaviorCreationRequest::ParticleFactory {
                callback: 0x0046_6710,
                context_word: 2,
                null_handle: ActorHandle::from_raw(0),
                scratch_bytes: [None; 8],
            },
        )
        .unwrap();
        assert_eq!(
            result,
            Some(BehaviorTemporary::Particle(ParticleId::Scratch))
        );
        assert_eq!(
            adapter.services.session.current_phase(),
            crate::session::SessionPhase::Title
        );
        assert_eq!(
            adapter.services.session.requested_phase(),
            crate::session::SessionPhase::Gameplay
        );
        assert!(adapter.services.session.transition_pending());
        assert_eq!(adapter.particles.free_count(), before_free);
        assert_eq!(adapter.particles.allocation_marker(), None);
        assert!(adapter.particles.pending(3).unwrap().is_empty());
        assert_eq!(*context.rng, before_rng);
        assert_eq!(format!("{arena:?}"), before_actor);
    });
}

//Actual435170 -> final Selector12+124 RET4; supplied callback scheduling only.
#[path = "tests/selector12_empty_update.rs"]
mod selector12_empty_update;

#[path = "tests/selector35_lifecycle.rs"]
mod selector35_lifecycle;

#[test]
fn family_dying_state_zero_preserves_native_corpses_instead_of_selector12_disposal() {
    with_host(|arena, host| {
        for table in [0x0052_0b84, 0x0052_0cc8, 0x0052_7ee0] {
            assert_eq!(host.executable.word(table + 0xb0).unwrap(), 0x0040_26d0);
        }
        assert_eq!(
            host.executable.word(0x0052_7058 + 0xb0).unwrap(),
            0x0040_26e0
        );
        let actors: Vec<_> = arena
            .actor_list()
            .iter()
            .copied()
            .filter(|&actor| arena.family4236d0_actor(actor).is_ok())
            .collect();
        assert!(!actors.is_empty());
        for actor in actors {
            arena
                .with_supplied_family_dispatch_actor(actor, |state| state.word_c4 = 1)
                .unwrap();
            let before = arena
                .list(crate::object_runtime::ObjectList::Disposal)
                .to_vec();
            arena
                .change_actor_state(
                    actor,
                    crate::object_runtime::ActorStateChangeRequest {
                        target_state: 0,
                        argument: 0,
                        source: host.factory_process.process_null_handle.unwrap(),
                    },
                    &mut super::dying::DyingStateZero,
                )
                .unwrap();
            assert_eq!(arena.actor(actor).unwrap().word_c4, 0);
            assert_eq!(
                arena.list(crate::object_runtime::ObjectList::Disposal),
                before
            );
        }
    });
}
