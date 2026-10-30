//! Real queued EXE programs and loaded world owners. Fixture isolation only
//! changes runtime candidates; callback, damage, allocation and RNG services
//! are production implementations, with their actual subsequent boundaries.
use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{ActorExtension, ActorHandle, DerivedActorClass, ObjectId},
    process_particles::ParticleId,
    terrain_runtime::TerrainSparsePayload,
    world_furniture_effect::{
        resident::{resident_furniture_model_radius, resident_placement_table},
        resisted_effect_damage, DamageResistance, ExecutableFurnitureDamageResources,
        FurnitureDamageResources, FurnitureEffectProgram, FurnitureQueueOutcome,
        FurnitureQueueRequest,
    },
    world_radius_effect::{radius_word_distance, scale_radius_effect, RadiusEffectBlock},
    world_statistics::{ActorStatisticCategory, WorldStatistics},
};

const SOUND_RADIUS_PROGRAM: u32 = 0x0053_2db4;
const RADIUS_BLOCK: u32 = 0x0053_2c68;

#[test]
fn furniture_replacement_authored_program_mutates_retained_fallback_then_finishes() {
    let (mut session, state) = loaded();
    let origin = [0, 0];
    isolate_furniture(&mut session, origin, None);
    //Actual one-second replacement record and native NULL terminator.
    enqueue_prefix(&mut session, origin, 0x0053_2dd8);
    session.world.as_mut().unwrap().rng = 0x1234_5678;
    session.tick_furniture_effects(1_000_000, &state);
    assert!(
        session.furniture_failure.is_none(),
        "{:?}",
        session.furniture_failure
    );
    assert!(session.furniture.records().is_empty());
    assert_eq!(session.world().unwrap().rng, 3_689_883_679);
    assert_eq!(
        session
            .furniture_query
            .fallback_payload()
            .resource_lookup
            .raw(),
        0xfd
    );
    assert_eq!(
        session.world().unwrap().terrain.cell_at(origin).raw() & 0xf000,
        0
    );
}

#[test]
fn furniture_terrain_flash_uses_authored_program_and_live_birth_source() {
    let (mut session, state) = loaded();
    let origin = [0, 0];
    isolate_furniture(&mut session, origin, None);
    // Start at the actual light record, so this checks the queue/dispatch/birth
    // composition independently of earlier damage and its actor population.
    enqueue_prefix(&mut session, origin, 0x0053_2d60);
    session.factory_process.process_null_handle = Some(ActorHandle::from_raw(0x1234_5678));
    let mut expected_rng = session.world().unwrap().rng;
    let random = name_entry_rng_next(&mut expected_rng);
    session.tick_furniture_effects(37, &state);
    assert!(
        session.furniture_failure.is_none(),
        "{:?}",
        session.furniture_failure
    );
    let id = *session.particles.pending(1).unwrap().front().unwrap();
    let record = session.particles.record(id).unwrap();
    assert_eq!(record.read_dword(0).unwrap(), 0x0052_8974);
    assert_eq!(record.read_dword(0x28).unwrap(), 0x0053_d620);
    assert_eq!(record.read_dword(0x14).unwrap(), 0x1234_5678);
    assert_eq!(
        record.read_dword(0x2c).unwrap(),
        ((random & 0xffff) >> 9) + 448
    );
    assert_eq!(
        record.position().unwrap(),
        [
            0,
            session.world().unwrap().terrain.raw_height_at(origin) as u16,
            0
        ]
    );
    assert_eq!(session.world().unwrap().rng, expected_rng);
    // The next zero-delay authored record is the real terminate callback.
    assert!(session.furniture.records().is_empty());
}

#[test]
fn furniture_terrain_flash_source_stop_keeps_allocator_prefix_and_stops_replay() {
    let (mut session, state) = loaded();
    isolate_furniture(&mut session, [0, 0], None);
    enqueue_prefix(&mut session, [0, 0], 0x0053_2d60);
    session.factory_process.process_null_handle = None;
    let rng = session.world().unwrap().rng;
    let free = session.particles.free_count();
    session.tick_furniture_effects(37, &state);
    assert!(session
        .furniture_failure
        .as_deref()
        .unwrap()
        .contains("TerrainFlash"));
    assert_eq!(session.particles.free_count(), free - 1);
    assert!(session.particles.pending(1).unwrap().is_empty());
    assert_eq!(session.world().unwrap().rng, rng);
    let record = session.furniture.records()[0];
    assert_eq!(record.program.address(), 0x0053_2d60);
    session.factory_process.process_null_handle = Some(ActorHandle::from_raw(0));
    session.tick_furniture_effects(37, &state);
    assert_eq!(session.furniture.records(), [record]);
    assert_eq!(session.particles.free_count(), free - 1);
    assert_eq!(session.world().unwrap().rng, rng);
}

fn loaded() -> (PortSession<Recording>, SessionState) {
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
    // The shared particle allocator uses the actual current phase. A direct
    // LoadWorld call leaves the Title/Frontend phase intact and correctly
    // constructs scratch particles, so enter Gameplay through its real owner.
    assert!(controller.request_phase(SessionPhase::Gameplay));
    controller.transition(&mut session).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Gameplay);
    (session, controller.state().clone())
}

fn block(session: &PortSession<Recording>, address: u32) -> RadiusEffectBlock {
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    RadiusEffectBlock::from_authored_bytes(
        executable
            .mapped(address, 10)
            .unwrap()
            .bytes
            .try_into()
            .unwrap(),
    )
}

fn isolate_actors(session: &mut PortSession<Recording>, target: Option<ObjectId>) {
    let arena = &mut session.world.as_mut().unwrap().arena;
    for actor in arena.actor_list().to_vec() {
        let old = arena.object(actor).unwrap().base.flags;
        // Explicit fixture population: suppress unrelated radius candidates
        // and mobile impulse, preserving the selected actor's other policies.
        let flags = if Some(actor) == target {
            (old | 4 | 8) & !0x8000_0000
        } else {
            old & !4
        };
        arena.apply_payload_flags(actor, flags).unwrap();
    }
}

fn isolate_furniture(
    session: &mut PortSession<Recording>,
    origin: [u16; 2],
    keep: Option<[u16; 2]>,
) {
    let terrain = &mut session.world.as_mut().unwrap().terrain;
    let spacing = terrain.sampling().spacing.units();
    // The same loaded H12 allocation remains live. Explicitly supply an
    // otherwise empty radius population, retaining the selected authored row.
    for dx in (-1024_i32..=1024).step_by(usize::from(spacing)) {
        for dz in (-1024_i32..=1024).step_by(usize::from(spacing)) {
            let xz = [
                origin[0].wrapping_add(dx as u16),
                origin[1].wrapping_add(dz as u16),
            ];
            if Some(xz) == keep {
                continue;
            }
            if let Some((record, _)) = terrain.sparse_record_at(xz) {
                terrain
                    .write_sparse_payload(
                        record,
                        TerrainSparsePayload {
                            state: 0,
                            resource_lookup: inf_formats::TerrainResourceLookup::from_raw(0),
                        },
                    )
                    .unwrap();
            }
        }
    }
}

fn enqueue_prefix(session: &mut PortSession<Recording>, origin: [u16; 2], program: u32) {
    assert!(session.furniture.records().is_empty());
    let terrain = &session.world.as_ref().unwrap().terrain;
    assert_eq!(
        session
            .furniture
            .enqueue(
                terrain,
                FurnitureQueueRequest {
                    // Deliberately unaligned: executor callbacks use the key
                    // and raw height, not these original interpolated WORDs.
                    xz: [origin[0].wrapping_add(7), origin[1].wrapping_add(11)],
                    program: FurnitureEffectProgram::new(program).unwrap(),
                },
            )
            .unwrap(),
        FurnitureQueueOutcome::Enqueued { index: 0 }
    );
    let record = session.furniture.record_mut(0).unwrap();
    record.position_words = [0x7000, 0x7000, 0x7000];
    record.remaining_microseconds = 500;
}

#[test]
fn furniture_radius_queue_runs_real_family_damage_with_current_process_source() {
    let (mut session, state) = loaded();
    let radius = block(&session, RADIUS_BLOCK);
    let world = session.world.as_ref().unwrap();
    let mask = !(world.terrain.sampling().spacing.units() - 1);
    let (actor, origin, raw_damage, damage) = world
        .arena
        .actor_list()
        .iter()
        .find_map(|&actor| {
            let value = world.arena.actor(actor).unwrap();
            if value.word_c4 != 3 || value.handle.marked() || !matches!(&value.extension,
                ActorExtension::Derived(derived) if matches!(derived.class, DerivedActorClass::Family428650(_))) {
                return None;
            }
            let position = world.arena.object(actor).unwrap().base.position_words?;
            let origin = [position[0] & mask, position[2] & mask];
            let callback_position = [origin[0], world.terrain.raw_height_at(origin) as u16, origin[1]];
            let distance = radius_word_distance(std::array::from_fn(|axis| {
                position[axis].wrapping_sub(callback_position[axis]) as i16
            })) as i32;
            let effect = scale_radius_effect(radius, distance, None)?;
            let (threshold, multiplier) = value.damage_responses()[usize::from(effect.channel)];
            let raw = resisted_effect_damage(effect.strength, DamageResistance { threshold, multiplier }, 0);
            let difficulty = if world.arena.object(actor).unwrap().base.flags & 0x4000 != 0 {
                256
            } else {
                world.mission.word_10.unwrap() as i16 as i32
            };
            let damage = raw.wrapping_mul(difficulty) >> 8;
            (damage > 0 && damage < i32::from(value.health)).then_some((actor, origin, raw, damage as u16))
        })
        .expect("loaded family admits the authored furniture block at its real terrain height");
    isolate_actors(&mut session, Some(actor));
    isolate_furniture(&mut session, origin, None);
    let world = session.world.as_mut().unwrap();
    let before = world.arena.actor(actor).unwrap().clone();
    // A deliberately unequal live source catches any registry-NULL or stale
    // world/source substitution in4375A0 and the actual47C6E0 suffix.
    assert_ne!(
        before.handle,
        world.arena.actor_handles().unwrap().null_handle()
    );
    session.factory_process.process_null_handle = Some(before.handle);
    let categories = world
        .arena
        .retirement_attribution(actor, before.handle)
        .unwrap();
    assert_ne!(
        categories.source,
        ActorStatisticCategory::B4ZeroOrUnresolved
    );
    let stats_before = WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap())
        .damage_total(categories);
    session
        .particles
        .record_mut(ParticleId::Scratch)
        .unwrap()
        .write_position([0x7000; 3])
        .unwrap();
    enqueue_prefix(&mut session, origin, SOUND_RADIUS_PROGRAM);
    let sounds = session.sounds.pending_len();
    session.tick_furniture_effects(37, &state);
    assert!(
        session.furniture_failure.is_none(),
        "{:?}",
        session.furniture_failure
    );
    let world = session.world.as_mut().unwrap();
    assert_eq!(
        world.arena.actor(actor).unwrap().health,
        before.health - damage
    );
    assert_eq!(world.arena.actor(actor).unwrap().word_ca, Some(55));
    assert_eq!(
        WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap())
            .damage_total(categories),
        stats_before.wrapping_add(raw_damage as u32)
    );
    assert_eq!(session.sounds.pending_len(), sounds + 1);
    assert_eq!(
        session.furniture.records()[0].program.address(),
        SOUND_RADIUS_PROGRAM + 36
    );
    assert_eq!(session.furniture.records()[0].age_microseconds, 37);
    assert_eq!(session.furniture.records()[0].remaining_microseconds, 463);
    let actor_after = world.arena.object(actor).unwrap().clone();
    let rng_after = world.rng;
    let emitter = *session.particles.pending(3).unwrap().front().unwrap();
    let expected_position = [
        origin[0],
        world.terrain.raw_height_at(origin) as u16,
        origin[1],
    ];
    let record = session.particles.record(emitter).unwrap();
    assert_eq!(record.read_dword(0).unwrap(), 0x526f80);
    assert_eq!(record.read_dword(0x14).unwrap(), before.handle.raw());
    assert_eq!(record.position().unwrap(), expected_position);
    assert_eq!(record.read_dword(0x3c).unwrap(), 10);
    assert_eq!(record.read_word(0x10).unwrap(), 1535);
    session.tick_furniture_effects(900_000, &state);
    assert_eq!(
        session.world().unwrap().arena.object(actor).unwrap(),
        &actor_after
    );
    assert_eq!(session.world().unwrap().rng, rng_after);
    assert_eq!(
        session.furniture.records()[0].program.address(),
        SOUND_RADIUS_PROGRAM + 36
    );
    assert_eq!(session.furniture.records()[0].age_microseconds, 900_037);
    assert_eq!(session.sounds.pending_len(), sounds + 1);
    assert_eq!(session.particles.pending(3).unwrap().len(), 1);
    // The actual following record is delayed terrain modification4370F0.
    // It now completes replacement and reaches the actual NULL terminator.
    session.tick_furniture_effects(99_963, &state);
    assert!(session.furniture_failure.is_none());
    assert!(session.furniture.records().is_empty());
    let replacement_rng = session.world().unwrap().rng;
    assert_ne!(replacement_rng, rng_after);
    session.tick_furniture_effects(123_456, &state);
    assert!(session.furniture.records().is_empty());
    assert_eq!(session.world().unwrap().rng, replacement_rng);
    assert_eq!(session.sounds.pending_len(), sounds + 1);
    assert_eq!(
        session.world().unwrap().arena.object(actor).unwrap(),
        &actor_after
    );

    // Run the production transfer/update loan. Parent stays fixed while its
    // real child receives four jitter draws and one trail-constructor draw.
    session
        .world
        .as_mut()
        .unwrap()
        .fade
        .performance
        .particle_load_factor = 65536;
    let mut expected_rng = replacement_rng;
    for _ in 0..5 {
        name_entry_rng_next(&mut expected_rng);
    }
    session.tick_process_particles(33333, &state);
    assert_eq!(session.world().unwrap().rng, expected_rng);
    assert!(session.particles.stopped_phase(emitter).is_none());
    assert!(session.particles.active(3).unwrap().contains(&emitter));
    assert_eq!(
        session
            .particles
            .record(emitter)
            .unwrap()
            .position()
            .unwrap(),
        expected_position
    );
    let child = *session.particles.pending(1).unwrap().front().unwrap();
    assert_eq!(
        session
            .particles
            .record(child)
            .unwrap()
            .read_dword(0)
            .unwrap(),
        0x5288f4
    );
    assert_eq!(
        session
            .particles
            .record(child)
            .unwrap()
            .read_dword(0x14)
            .unwrap(),
        before.handle.raw()
    );
    draw_actual_trail(&mut session);
    // Actual elapsed lifetime expires the parent; the child's longer authored
    // lifetime and actual update preserve its independent allocation.
    session.tick_process_particles(1_600_000, &state);
    assert!(!session.particles.active(3).unwrap().contains(&emitter));
    assert!(session.particles.active(1).unwrap().contains(&child));
    assert!(session.particles.stopped_phase(child).is_none());
}

#[test]
fn furniture_radius_queue_admits_nested_authored_effect_and_uses_live_rng_once() {
    let (mut session, state) = loaded();
    let radius = block(&session, RADIUS_BLOCK);
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let table = resident_placement_table(&session.residency).unwrap();
    let terrain = &session.world.as_ref().unwrap().terrain;
    let spacing = terrain.sampling().spacing.units();
    let mut resources = ExecutableFurnitureDamageResources {
        placement_table: table,
        executable,
    };
    // Select a real probabilistic damage row on a flat loaded patch. The
    // fixture then withholds other nearby candidates, so exactly one actual
    //436020 RNG decision has an independently known input and output.
    let selected = terrain
        .buckets()
        .iter()
        .enumerate()
        .find_map(|(z, bucket)| {
            bucket.records.iter().find_map(|record| {
                if record.resource_lookup.raw() == 0 || record.resource_lookup.is_special() {
                    return None;
                }
                let target = [record.x.wrapping_mul(128), (z as u16).wrapping_mul(128)];
                let (_, actual) = terrain.sparse_record_at(target)?;
                if actual != *record {
                    return None;
                }
                let model_radius =
                    resident_furniture_model_radius(&session.residency, record.resource_lookup)
                        .ok()??;
                let _ = model_radius;
                let slot = table.slots[usize::from(record.resource_lookup.raw())];
                let rule = resources
                    .damage_rule(slot.parameter_word as u8, radius.channel)
                    .ok()?;
                let program = rule.program?;
                let height = terrain.raw_height_at(target);
                if [[spacing, 0], [0, spacing], [spacing, spacing]]
                    .into_iter()
                    .any(|offset| {
                        terrain.raw_height_at([
                            target[0].wrapping_add(offset[0]),
                            target[1].wrapping_add(offset[1]),
                        ]) != height
                    })
                {
                    return None;
                }
                let origin = [target[0].wrapping_sub(spacing), target[1]];
                if terrain.raw_height_at(origin) != height {
                    return None;
                }
                let effect = scale_radius_effect(radius, i32::from(spacing), None)?;
                let damage = resisted_effect_damage(effect.strength, rule.resistance, 0);
                (damage > 0 && damage < rule.threshold).then_some((
                    origin,
                    target,
                    program,
                    damage,
                    rule.threshold,
                ))
            })
        })
        .expect(
            "loaded terrain has an authored probabilistic furniture damage row on a flat patch",
        );
    let (origin, target, nested_program, damage, threshold) = selected;
    isolate_actors(&mut session, None);
    isolate_furniture(&mut session, origin, Some(target));
    let ratio = (damage.wrapping_shl(16).wrapping_sub(damage) / threshold) as u32;
    let (seed, expected_rng) = (0..1_000_000)
        .find_map(|seed| {
            let mut next = seed;
            ((name_entry_rng_next(&mut next) & 0xffff) <= ratio).then_some((seed, next))
        })
        .unwrap();
    session.world.as_mut().unwrap().rng = seed;
    let target_before = session
        .world()
        .unwrap()
        .terrain
        .sparse_record_at(target)
        .unwrap();
    enqueue_prefix(&mut session, origin, SOUND_RADIUS_PROGRAM);
    session.tick_furniture_effects(37, &state);
    assert!(
        session.furniture_failure.is_none(),
        "{:?}",
        session.furniture_failure
    );
    assert_eq!(session.world().unwrap().rng, expected_rng);
    assert_ne!(seed, expected_rng);
    assert_eq!(
        session
            .world()
            .unwrap()
            .terrain
            .sparse_record_at(target)
            .unwrap(),
        target_before,
        "damage queues the effect; the new program has not removed its furniture"
    );
    assert_eq!(session.furniture.records().len(), 2);
    let nested = session.furniture.records()[1];
    assert_eq!(nested.program, nested_program);
    assert_eq!(
        nested.position_words,
        [
            target[0],
            session.world().unwrap().terrain.height_at(target) as u16,
            target[1]
        ]
    );
    assert_eq!(
        nested.age_microseconds, 0,
        "saved-next excludes a new tail this pass"
    );
    assert_eq!(nested.remaining_microseconds, 0);
    assert_eq!(
        session.furniture.records()[0].program.address(),
        SOUND_RADIUS_PROGRAM + 36
    );
    let emitter = *session.particles.pending(3).unwrap().front().unwrap();
    assert_eq!(
        session
            .particles
            .record(emitter)
            .unwrap()
            .read_dword(0)
            .unwrap(),
        0x526f80
    );
    assert_eq!(
        session
            .particles
            .record(emitter)
            .unwrap()
            .position()
            .unwrap(),
        [
            origin[0],
            session.world().unwrap().terrain.raw_height_at(origin) as u16,
            origin[1]
        ]
    );
}

fn draw_actual_trail(session: &mut PortSession<Recording>) {
    use crate::{
        frontend_display::frontend_startup,
        frontend_scene::SceneView,
        menu::MenuBasis,
        model_context::{ModelDispatch, ModelProjection},
        model_primitives::PrimitiveTables,
        process_particles::draw::{
            ParticleDrawBank, ParticleDrawContext, ParticleDrawLighting, ParticleDrawRequest,
        },
        render_resources::RenderResources,
        software_frame::SoftwareFrame,
        software_scene::{SoftwareScene, SoftwareSceneRequest},
        world_lighting::TerrainLightRequest,
    };
    struct Lighting;
    impl ParticleDrawLighting for Lighting {
        type Error = &'static str;
        fn terrain_light(
            &mut self,
            _: TerrainLightRequest,
            _: [i32; 3],
        ) -> Result<(), Self::Error> {
            Err("unexpected trail light")
        }
    }
    let executable =
        inf_formats::FrontendExecutable::parse(session.catalog.assets().executable_bytes())
            .unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let startup = frontend_startup(
        &session.residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let child = *session.particles.pending(1).unwrap().front().unwrap();
    let position = session
        .particles
        .record(child)
        .unwrap()
        .position()
        .unwrap()
        .map(i32::from);
    let mut frame = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: session.model_process.clone(),
        resources: RenderResources::new(&session.residency),
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    scene.begin_frame().unwrap();
    let report = scene.draw_particles(
        &mut session.particles,
        ParticleDrawRequest {
            bank: ParticleDrawBank::Pending,
            context: ParticleDrawContext {
                position,
                basis: MenuBasis::INITIAL,
                view: SceneView {
                    eye: [position[0], position[1], position[2] - 4096],
                    basis: MenuBasis::INITIAL,
                },
                projection: ModelProjection {
                    focal: [320, 240],
                    viewport: [640, 480],
                    center: [320, 240],
                    fade: [10000, 20000],
                },
                coordinate_shift: 0,
                fog_factor: 0,
                projection_dispatch: ModelDispatch::Far,
                far_color: 0,
            },
            terrain: Some(&session.world.as_ref().unwrap().terrain),
        },
        executable,
        &mut Lighting,
    );
    assert!(report.omissions.is_empty(), "{:?}", report.omissions);
    assert_eq!(report.visited, 1);
    assert_eq!(report.commands, 1);
    crate::pixel_backend::flush_pixels(&mut scene.queue, &mut scene.pixels).unwrap();
    assert!(scene
        .pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .any(|pixel| pixel != [17, 23, 31, 255]));
}

#[test]
fn furniture_radius_missing_process_source_retains_sound_and_executor_prefix_without_retry() {
    let (mut session, state) = loaded();
    let origin = [0, 0];
    isolate_actors(&mut session, None);
    isolate_furniture(&mut session, origin, None);
    enqueue_prefix(&mut session, origin, SOUND_RADIUS_PROGRAM);
    session.factory_process.process_null_handle = None;
    let rng = session.world().unwrap().rng;
    let sounds = session.sounds.pending_len();
    session.tick_furniture_effects(37, &state);
    let failure = session.furniture_failure.as_deref().unwrap();
    assert!(failure.contains("process NULL0054E894"), "{failure}");
    assert_eq!(session.sounds.pending_len(), sounds + 1);
    assert_eq!(session.furniture.records().len(), 1);
    let record = session.furniture.records()[0];
    assert_eq!(record.program.address(), SOUND_RADIUS_PROGRAM + 12);
    assert_eq!(record.age_microseconds, 37);
    assert_eq!(record.remaining_microseconds, 463);
    assert_eq!(session.world().unwrap().rng, rng);
    session.factory_process.process_null_handle = Some(ActorHandle::from_raw(0));
    session.tick_furniture_effects(37, &state);
    assert_eq!(session.furniture.records(), [record]);
    assert_eq!(session.sounds.pending_len(), sounds + 1);
    assert_eq!(session.world().unwrap().rng, rng);
}

#[test]
fn furniture_camera_record_runs_the_world_flash_gate_and_continues() {
    let (mut session, state) = loaded();
    let origin = [0, 0];
    isolate_furniture(&mut session, origin, None);
    // Actual record 00532D84: delay zero, 004375D0 with argument 1400h, then
    // the authored five-second record that keeps the program queued.
    enqueue_prefix(&mut session, origin, 0x0053_2d84);
    let before = session.world().unwrap().flash.clone();
    session.tick_furniture_effects(37, &state);
    assert!(
        session.furniture_failure.is_none(),
        "{:?}",
        session.furniture_failure
    );
    // The camera exists after the callback's own 00478E00 acquisition.
    let world = session.world().unwrap();
    let camera = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&object| world.arena.selector16(object).is_ok())
        .expect("004375D0 acquires the world camera");
    let camera_position = world.arena.object(camera).unwrap().base.position_words;
    // The callback's stack XYZ: aligned key XZ with the raw terrain height.
    let key = session.furniture.records()[0].aligned_key;
    let xz = [key as u16, (key >> 16) as u16];
    let position = [xz[0], world.terrain.raw_height_at(xz) as u16, xz[1]];
    let admitted = crate::world_furniture_effect::furniture_camera_flash_admitted(
        position,
        || camera_position,
        0x1400,
    )
    .unwrap();
    // A rejected camera gate leaves the flash untouched; an accepted one may
    // still be rejected by 00487D70's own forward-radius test.
    if !admitted {
        assert_eq!(session.world().unwrap().flash, before);
    }
    // The callback returned AL one: the program advanced to the authored
    // five-second record 00532D90, which waits on the record's age.
    assert_eq!(session.furniture.records().len(), 1);
    assert_eq!(
        session.furniture.records()[0].program.address(),
        0x0053_2d90
    );
}
