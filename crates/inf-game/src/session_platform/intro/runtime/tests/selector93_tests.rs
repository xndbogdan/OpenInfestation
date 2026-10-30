//! Supplied callback scheduling on complete current world owners. These
//! integration tests exercise the actual production pass, not attract timing.

use super::family_prefix_tests::{only_callbacks, TickOwners};
use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorCommandContext, ActorConstructionRequest, ActorTerrainInputs, ObjectList,
    },
    terrain_runtime::{TerrainFlagScatterRequest, TerrainRuntime},
    world_actor_terrain::WorldActorTerrainHost,
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, InstanceCommandPayload,
    InstanceCommandTarget, InstanceCommandView, InstanceField, TERRAIN_ENTRY_FIXED_BYTES,
    TERRAIN_PREFIX_BYTES,
};
use std::sync::Arc;

fn allocation<'a>(assets: &'a FrontendAssets, position: [u16; 3]) -> ActorConstructionRequest<'a> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(position),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &assets.sine_table().values,
        },
    }
}

fn actors(world: &mut WorldSession, assets: &FrontendAssets) -> (ObjectId, ObjectId) {
    let actor = world
        .arena
        .allocate_selector93(allocation(assets, [0, 1500, 0]))
        .unwrap();
    let referent = world
        .arena
        .allocate_actor(allocation(assets, [0, 1500, 4000]))
        .unwrap();
    (actor, referent)
}

/// Choose a supplied pre-call RNG state through the actual pure process draw.
/// The manager drains the actual pending disposal queue before selector93.
/// Use that post-drain FIFO for the supplied seed, without removing objects or
/// applying a target pointer/constructor output to the arena in the fixture.
fn select_next(world: &mut WorldSession, referent: ObjectId) -> u32 {
    let pending: Vec<_> = world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    let after_disposal: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|actor| !pending.contains(actor))
        .collect();
    let index = after_disposal
        .iter()
        .position(|&actor| actor == referent)
        .unwrap();
    let count = after_disposal.len();
    for seed in 0..1_000_000_u32 {
        let mut after = seed;
        if (name_entry_rng_next(&mut after) & 0xffff) as usize % count == index {
            world.rng = seed;
            return after;
        }
    }
    panic!("supplied FIFO scheduling seed not found");
}

fn delay(world: &mut WorldSession, runtime: &ResidentRuntime, actor: ObjectId, value: i32) {
    let command = InstanceCommandView {
        target: InstanceCommandTarget::DerivedActor,
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: 18,
        },
        proven_method: Some(0x0042_6030),
        payload_consumer: Some(0x0042_6030),
        payload: InstanceCommandPayload::ActorNegatedDword144(InstanceField {
            span: ByteSpan { start: 4, end: 8 },
            value: value.wrapping_neg() as u32,
        }),
    };
    assert!(world
        .arena
        .apply_derived_actor_command(
            actor,
            &command,
            ActorCommandContext {
                runtime,
                first_player_grants: None,
                terrain: None,
            },
        )
        .unwrap());
}

#[test]
fn ordinary_selector93_updates_without_an_intro_attachment_and_retains_its_target() {
    let (runtime, assets, mut world, _maps) = loaded();
    world.intro_runtime = None;
    let (actor, referent) = actors(&mut world, &assets);
    only_callbacks(&mut world, &[actor]);
    let referent_before = world.arena.object(referent).unwrap().clone();
    let position_before = world.arena.object(actor).unwrap().base.position_words;
    let rng_after_selection = select_next(&mut world, referent);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);

    owners.tick(&mut world, &runtime, &assets, 40_000);

    let state = world.arena.selector93(actor).unwrap();
    let reference = state.target_148.unwrap();
    let target = world.arena.actor_follow_target(reference).unwrap();
    assert_eq!(
        target.handle_04,
        world.arena.actor(referent).unwrap().handle
    );
    assert_eq!(target.focus_08, Some([0, 1500, 4000]));
    assert!(state.dword_144 > 0, "the complete speed/physics suffix ran");
    assert!(world.arena.actor(actor).unwrap().words_8e_92[2] > 0);
    assert_ne!(
        world.arena.object(actor).unwrap().base.position_words,
        position_before
    );
    assert_eq!(world.rng, rng_after_selection);
    assert!(world.actor_updates.selector93_stops.is_empty());
    assert!(world.actor_updates.skipped_callbacks.is_empty());
    assert!(world.actor_updates.family_prefix_stops.is_empty());
    assert!(world.intro_runtime.is_none());
    assert_eq!(world.arena.object(referent).unwrap(), &referent_before);

    let speed = state.dword_144;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert!(world.arena.selector93(actor).unwrap().dword_144 > speed);
    assert_eq!(
        world.arena.selector93(actor).unwrap().target_148,
        Some(reference)
    );
    assert_eq!(
        world
            .arena
            .actor_follow_target(reference)
            .unwrap()
            .handle_04,
        target.handle_04
    );
    assert_eq!(
        world.rng, rng_after_selection,
        "present148 bypasses another RNG selection"
    );
    assert_eq!(world.arena.object(referent).unwrap(), &referent_before);
    assert!(world.actor_updates.selector93_stops.is_empty());
}

#[test]
fn alternate_selector93_callback_remains_unowned_without_running_ordinary_update() {
    let (runtime, assets, mut world, _maps) = loaded();
    world.intro_runtime = None;
    let (actor, referent) = actors(&mut world, &assets);
    let flags = world.arena.object(actor).unwrap().base.flags | 0x8000_0000;
    world.arena.apply_payload_flags(actor, flags).unwrap();
    only_callbacks(&mut world, &[actor]);
    select_next(&mut world, referent);
    let rng = world.rng;
    let before = world.arena.object(actor).unwrap().clone();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    for _ in 0..2 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
        assert_eq!(world.arena.object(actor).unwrap(), &before);
        assert_eq!(world.rng, rng);
        assert_eq!(world.actor_updates.skipped_callbacks, [(actor, true)]);
        assert!(world.actor_updates.selector93_stops.is_empty());
    }
}

#[test]
fn current_terrain_errors_stop_only_selector93_after_selection_and_do_not_retry() {
    for replaced in [false, true] {
        let (runtime, assets, mut world, _maps) = loaded();
        let controller = world.intro_runtime.as_ref().unwrap().controller;
        let fellow = world
            .arena
            .actor_list()
            .iter()
            .copied()
            .find(|&actor| {
                world.arena.family_update_class(actor)
                    == Ok(crate::object_runtime::FamilyUpdateClass::Selector35)
                    && !world
                        .arena
                        .list(ObjectList::Disposal)
                        .iter()
                        .any(|node| node.object == actor)
            })
            .unwrap();
        delay(&mut world, &runtime, fellow, 120_000);
        let (actor, referent) = actors(&mut world, &assets);
        only_callbacks(&mut world, &[actor, fellow, controller]);
        select_next(&mut world, referent);
        let before = world.arena.object(actor).unwrap().clone();
        let current = if replaced {
            let (fresh, _, _, _fresh_maps) = loaded();
            let original = runtime.resource(12, 0).unwrap();
            let replacement = fresh.resource(12, 0).unwrap();
            assert_eq!(original.registration, replacement.registration);
            assert!(!Arc::ptr_eq(original.overlay, replacement.overlay));
            fresh
        } else {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
            let inactive = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
            assert!(inactive.resource(12, 0).is_none());
            inactive
        };
        let mut owners = TickOwners::new(&assets, &mut world, &runtime);
        owners.tick(&mut world, &current, &assets, 40_000);

        let stopped = world.arena.object(actor).unwrap().clone();
        assert_ne!(
            stopped, before,
            "the target allocation/publication prefix survives"
        );
        assert_eq!(stopped.base.position_words, before.base.position_words);
        let reference = world.arena.selector93(actor).unwrap().target_148.unwrap();
        let target = world.arena.actor_follow_target(reference).unwrap();
        assert_eq!(
            target.handle_04,
            world.arena.actor(referent).unwrap().handle
        );
        assert_eq!(target.focus_08, Some([0, 1500, 4000]));
        assert_eq!(world.arena.selector93(actor).unwrap().dword_144, 0);
        assert_eq!(world.arena.actor(actor).unwrap().words_8e_92, [0; 3]);
        assert_eq!(world.actor_updates.selector93_stops.len(), 1);
        assert_eq!(world.actor_updates.selector93_stops[0].0, actor);
        let reason = &world.actor_updates.selector93_stops[0].1;
        assert!(reason.contains(if replaced {
            "replaced the prepared world terrain"
        } else {
            "requires current H12/0"
        }));
        assert_eq!(
            world.arena.derived_actor(fellow).unwrap().word_144,
            80_000,
            "the following actual family callback still runs"
        );
        assert!(world.actor_updates.family_prefix_stops.is_empty());
        let attachment = world.intro_runtime.as_ref().unwrap();
        assert!(attachment.deadline_approximation_applied);
        if replaced {
            assert!(attachment.controller_failure.is_none());
            let sequence = world.arena.selector88_intro_sequence(controller).unwrap();
            assert_eq!(sequence.stage_148, 1);
            assert!(
                sequence.primary_helper_154.is_some(),
                "current replacement resources still serve the following controller"
            );
        } else {
            assert!(
                attachment.controller_failure.is_some(),
                "controller independently reaches its own missing-resource boundary"
            );
        }

        // Reinstalling an admitted current owner does not replay this actor's
        // half-completed callback. Fellow callbacks continue on later visits.
        owners.tick(&mut world, &runtime, &assets, 40_000);
        assert_eq!(world.arena.object(actor).unwrap(), &stopped);
        assert_eq!(world.actor_updates.selector93_stops.len(), 1);
        assert_eq!(world.arena.derived_actor(fellow).unwrap().word_144, 40_000);
    }
}

#[test]
fn unchanged_current_allocation_serves_the_retained_modified_prepared_copy() {
    let (runtime, assets, mut world, _maps) = loaded();
    world.intro_runtime = None;
    let current = runtime.clone();
    let original = runtime.resource(12, 0).unwrap();
    let cloned = current.resource(12, 0).unwrap();
    assert_eq!(original.registration, cloned.registration);
    assert!(Arc::ptr_eq(original.overlay, cloned.overlay));

    // Supplied prepared cells are explicit mutable world state, not modified
    // retail OVL bytes or a claim that a native height-deformation writer ran.
    // The actual existing scatter owner changes their flags before acquisition.
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for cell in bytes[TERRAIN_PREFIX_BYTES..]
        .chunks_exact_mut(2)
        .take(512 * 512)
    {
        cell.copy_from_slice(&0xf000_u16.to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    world.terrain = TerrainRuntime::new(&bank.entries[0], world.terrain.sampling());
    let raw_before: Vec<_> = world
        .terrain
        .cells()
        .iter()
        .map(|cell| cell.raw())
        .collect();
    let mut scatter_rng = 0;
    let scatter = world.terrain.scatter_cell_flags(
        TerrainFlagScatterRequest {
            position_words: [0; 3],
            count_minus_one: 0,
        },
        &mut scatter_rng,
    );
    assert!(scatter.height_gate_passed);
    assert_eq!(scatter.operations, 1);
    let prepared: Vec<_> = world
        .terrain
        .cells()
        .iter()
        .map(|cell| cell.raw())
        .collect();
    assert_ne!(prepared, raw_before);
    let (actor, referent) = actors(&mut world, &assets);
    only_callbacks(&mut world, &[actor]);
    select_next(&mut world, referent);
    let mut host = WorldActorTerrainHost {
        residency: &current,
        source: &world.terrain_source,
        prepared: &world.terrain,
    };
    let acquired =
        crate::object_runtime::ActorTerrainHost::terrain(&mut host, &mut world.arena, actor)
            .unwrap();
    assert_eq!(acquired.cells(), world.terrain.cells());
    assert_eq!(acquired.height_at([0, 0]), 0);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.tick(&mut world, &current, &assets, 40_000);
    assert!(world.arena.selector93(actor).unwrap().dword_144 > 0);
    assert!(world.actor_updates.selector93_stops.is_empty());
    assert_eq!(
        world
            .terrain
            .cells()
            .iter()
            .map(|cell| cell.raw())
            .collect::<Vec<_>>(),
        prepared
    );
}

#[test]
fn no_controller_and_no_owned_class_still_visit_and_log_unknown_callbacks_once() {
    let (runtime, assets, mut world, _maps) = loaded();
    world.intro_runtime = None;
    assert!(!world
        .arena
        .actor_list()
        .iter()
        .any(|&actor| world.arena.selector93(actor).is_ok()));
    let ordinary = world
        .arena
        .allocate_actor(allocation(&assets, [3, 4, 5]))
        .unwrap();
    let alternate = world
        .arena
        .allocate_actor(allocation(&assets, [6, 7, 8]))
        .unwrap();
    // Another machine's actor (+2C bit 31), held still by +2C bit 3.
    let flags = world.arena.object(alternate).unwrap().base.flags | 0x8000_0008;
    world.arena.apply_payload_flags(alternate, flags).unwrap();
    only_callbacks(&mut world, &[ordinary, alternate]);
    assert!(world.arena.family_update_class(ordinary).is_err());
    assert!(world.arena.family_update_class(alternate).is_err());
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    let mut process = ParticleFactoryProcessState::native_startup();
    let mut selector35_process = crate::object_runtime::Selector35RouteProcessState::default();
    let clock = crate::frontend_camera::AnimationClock::default();
    let mut behavior_tables =
        crate::object_runtime::ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let actor_draw =
        crate::object_runtime::ObjectDrawProcessState::native_startup(executable).unwrap();
    let actor_motion =
        crate::object_runtime::ProcessActorMotionState::from_executable(executable).unwrap();
    let mut state = SessionState::default();
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let mut tasks = ProcessTaskRuntime::default();
    tasks
        .retain(world.arena.actor_handles().unwrap().null_handle())
        .unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let mut furniture = FurnitureEffectQueue::default();
    let mut furniture_query =
        crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(executable)
            .unwrap();
    //This alternate-dispatch fixture bypasses all family collision tails.
    let furniture_radius =
        crate::terrain_runtime::ProcessFurnitureRadius::native_startup(executable).unwrap();
    let furniture_draw = crate::terrain_furniture::draw::FurnitureDrawState::new(
        &NativeBasisMath::new(assets.sine_table().values),
    );
    let model_process =
        crate::model_process::ModelProcessOwners::native_startup(executable).unwrap();
    // Establish this list-swap fixture's baseline after the real manager
    // disposal subphase, using the same live mission/process/sound owners as
    // its following passes. Queued loaded actors genuinely release here.
    let queued: Vec<_> = world
        .arena
        .list(ObjectList::Disposal)
        .iter()
        .map(|node| node.object)
        .collect();
    let report = world
        .arena
        .drain_object_disposal(crate::object_runtime::ObjectDisposalRequest {
            mission: &mut world.mission,
            grants: &crate::object_runtime::LiveGrantContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&runtime),
                text_data: None,
            },
            sound: crate::object_runtime::SoundTeardownService::Available {
                sounds: &mut sounds,
                voices: &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
            },
            state: &mut world.actor_updates.disposal,
            failure_policy: crate::object_runtime::ObjectDisposalFailurePolicy::ReturnFirstError,
            listeners: Some(&mut world.listeners),
        })
        .unwrap();
    assert_eq!(report.destroyed, queued);
    for object in queued {
        assert!(world.arena.object(object).is_err());
    }
    let before = format!("{world:?}");
    let before_arena = format!("{:?}", world.arena);
    let before_updates = format!("{:?}", world.actor_updates);
    let actor_order = world.arena.actor_list().to_vec();
    let objects_before: Vec<_> = actor_order
        .iter()
        .map(|&actor| world.arena.object(actor).unwrap().clone())
        .collect();
    let process_before = format!("{process:?}");
    // The real pass transfers and swaps the physical Flag1 owners. Two
    // visits restore those owner identities, so whole arena/world custody
    // can be compared without cloning or normalizing away private owners.
    for visit in 0..2 {
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
                clock: 0,
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
                microseconds: 40_000,
                paced_microseconds: Some(40_000),
                mission_completion: false,
                inverse_sine: &inverse.values,
                up: &[0.0, 1.0, 0.0],
                surface_snap_53a128: &authored_surface_snap(&assets),
                factory_process: &mut process,
                behavior_tables: &mut behavior_tables,
                selector35_process: &mut selector35_process,
                actor_motion: &actor_motion,
                model_process: &model_process,
                actor_shared_basis: &actor_draw.shared_basis,
                state: &mut state,
                sounds: &mut sounds,
                tasks: &mut tasks,
                particles: &mut particles,
                furniture: &mut furniture,
                furniture_query: &mut furniture_query,
                furniture_radius: &furniture_radius,
                furniture_draw: &furniture_draw,
            },
        );
        assert_eq!(world.arena.actor_list(), actor_order.as_slice());
        for (&actor, expected) in actor_order.iter().zip(&objects_before) {
            assert_eq!(world.arena.object(actor).unwrap(), expected);
        }
        // The plain shared actor's +24 is the countdown 0047AD30 and its +3C
        // 0047EBF0, which bit 3 keeps from moving it; a zero +C8 leaves
        // either unchanged.
        assert!(world.actor_updates.skipped_callbacks.is_empty());
        assert!(world.actor_updates.unowned_callbacks_logged);
        assert!(world.actor_updates.family_prefix_stops.is_empty());
        assert!(world.actor_updates.selector93_stops.is_empty());
        assert!(world.actor_updates.list_failure.is_none());
        assert_eq!(
            world
                .arena
                .list(ObjectList::Flag1Primary)
                .iter()
                .map(|node| node.object)
                .collect::<Vec<_>>(),
            if visit == 0 {
                [alternate, ordinary]
            } else {
                [ordinary, alternate]
            }
        );
        assert!(world.arena.list(ObjectList::Flag1Secondary).is_empty());
        assert_eq!(format!("{:?}", world.arena) == before_arena, visit == 1);
        assert_eq!(format!("{process:?}"), process_before);
    }
    // 00478F3E: each tick stores its delta for the draw's lights.
    assert_eq!(world.light_delta, 2 * 40_000);
    world.light_delta = 0;
    let after = format!("{world:?}");
    let after_updates = format!("{:?}", world.actor_updates);
    assert_eq!(after.matches(&after_updates).count(), 1);
    assert_eq!(after.replacen(&after_updates, &before_updates, 1), before);
}
