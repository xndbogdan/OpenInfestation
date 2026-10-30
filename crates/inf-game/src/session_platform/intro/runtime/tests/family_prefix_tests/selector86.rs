//! Loaded emitter data, real flag1 dispatcher and existing particle lifecycle.
use super::*;
use crate::process_particles::{
    audio::ParticleSoundRequest,
    update::{ParticleUpdateBirthInputs, ParticleUpdateHost, ParticleUpdateRequest},
};

fn emitter(world: &WorldSession) -> ObjectId {
    let objects: Vec<_> = world
        .arena
        .list(ObjectList::Flag1Primary)
        .into_iter()
        .filter(|n| world.arena.selector86(n.object).is_ok())
        .map(|n| n.object)
        .collect();
    assert_eq!(objects.len(), 6);
    // Placement adds the wrapped terrain height to the authored Y (0 here),
    // so match the authored X/Z and check Y against the live terrain.
    let id = objects
        .into_iter()
        .find(|id| {
            let [x, _, z] = world
                .arena
                .object(*id)
                .unwrap()
                .base
                .position_words
                .unwrap();
            [x, z] == [23072, 44497]
        })
        .unwrap();
    assert_eq!(
        world.arena.object(id).unwrap().base.position_words,
        Some([23072, world.terrain.height_at([23072, 44497]) as u16, 44497])
    );
    assert_eq!(
        world
            .arena
            .selector86(id)
            .unwrap()
            .data_40
            .as_ref()
            .unwrap()
            .words(),
        [500, 500, 3, 120]
    );
    id
}

struct NoAudio;
impl ParticleUpdateHost for NoAudio {
    type Error = &'static str;
    fn impact_sound(
        &mut self,
        _: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        Err("unexpected trail sound")
    }
}

#[test]
fn selector86_production_births_real_trail_with_live_source_rng_then_updates_and_retires() {
    let (resident, assets, mut world, _) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &resident);
    owners.state = gameplay_state();
    pause_controller(&mut world);
    let id = emitter(&world);
    only_callbacks(&mut world, &[id]);
    owners.globals.cached_position = world.arena.object(id).unwrap().base.position_words;
    let source = crate::object_runtime::ActorHandle::from_raw(0x13572468);
    owners.factory_process.process_null_handle = Some(source);
    world.rng = 0x12345678;
    let mut expected_rng = world.rng;
    // Two circle draws from the ordinary native receipt, then the real
    //489D90 animation-constructor draw, all in this same process owner.
    for _ in 0..3 {
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
    }
    owners.tick(&mut world, &resident, &assets, 121 * 1024);
    assert_eq!(world.rng, expected_rng);
    assert_eq!(world.arena.selector86(id).unwrap().elapsed_44, 1);
    assert!(!world.arena.selector86(id).unwrap().update_stopped);
    assert_eq!(owners.particles.pending(1).unwrap().len(), 1);
    let shot = *owners.particles.pending(1).unwrap().front().unwrap();
    let record = owners.particles.record(shot).unwrap();
    assert_eq!(record.read_dword(0).unwrap(), 0x5288f4);
    assert_eq!(record.read_dword(0x14).unwrap(), source.raw());
    assert_eq!(record.read_word(0x12).unwrap() & 0x800, 0x800);
    assert_eq!(record.read_dword(0x28).unwrap(), 0x53d210);
    let point = record.position().unwrap();
    assert_eq!([point[0], point[2]], [22670, 44387]);
    let ground = world.terrain.height_at([point[0], point[2]]);
    let water = world.terrain.water_surface_height(
        crate::terrain_runtime::TerrainWaterSurfaceRequest {
            xz: [point[0], point[2]],
            wave_clock: 0,
            world_flags: world.world_flags,
            terrain_height: ground,
        },
        &assets.sine_table().values,
    );
    assert_eq!(point[1], ground.max(water) as u16);
    let scale = record.read_dword(0x2c).unwrap();
    owners.particles.transfer_pending();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    // 00465660 moves (v * (delta >> 5)) >> 15: at 10 ms a ~100-unit vertical
    // velocity rounds to zero, so use 100 ms (>= 9 units) to observe motion
    // well inside the 2,447 ms lifetime, then a huge step to retire it.
    for delta in [100_000, 100_000_000] {
        let report = owners.particles.tick_particles(
            ParticleUpdateRequest {
                delta_microseconds: delta,
                load_factor: 65536,
                birth_inputs: ParticleUpdateBirthInputs {
                    network_active: false,
                    phase: Some(3),
                    process: &mut owners.factory_process,
                },
                terrain: None,
            },
            executable,
            &mut world.rng,
            &mut NoAudio,
        );
        assert!(report.omissions.is_empty(), "{:?}", report.omissions);
        if delta == 100_000 {
            for _ in 0..3 {
                crate::name_entry::name_entry_rng_next(&mut expected_rng);
            }
            assert_eq!(report.updated, 1);
            assert_ne!(
                owners.particles.record(shot).unwrap().position().unwrap(),
                point
            );
            assert!(
                owners
                    .particles
                    .record(shot)
                    .unwrap()
                    .read_dword(0x2c)
                    .unwrap()
                    > scale
            );
        } else {
            assert_eq!(report.destroyed, 1);
        }
    }
    assert!(owners.particles.active(1).unwrap().is_empty());
    assert_eq!(world.rng, expected_rng);
}

#[test]
fn selector86_production_keeps_timer_rng_and_allocated_birth_prefix_on_missing_source_without_replay(
) {
    let (resident, assets, mut world, _) = loaded();
    let mut owners = TickOwners::new(&assets, &mut world, &resident);
    owners.state = gameplay_state();
    pause_controller(&mut world);
    let id = emitter(&world);
    only_callbacks(&mut world, &[id]);
    owners.globals.cached_position = world.arena.object(id).unwrap().base.position_words;
    owners.factory_process.process_null_handle = None;
    world.rng = 0x12345678;
    owners.tick(&mut world, &resident, &assets, 121 * 1024);
    assert!(world.arena.selector86(id).unwrap().update_stopped);
    assert_eq!(world.arena.selector86(id).unwrap().elapsed_44, 1);
    let state = world.arena.object(id).unwrap().clone();
    let rng = world.rng;
    let pending = owners.particles.pending(1).unwrap().clone();
    assert!(pending.is_empty());
    let free = owners.particles.free_count();
    assert_eq!(free, crate::process_particles::PARTICLE_POOL_SIZE - 1);
    owners.factory_process.process_null_handle =
        Some(crate::object_runtime::ActorHandle::from_raw(0));
    owners.tick(&mut world, &resident, &assets, 121 * 1024);
    assert_eq!(world.arena.object(id).unwrap(), &state);
    assert_eq!(world.rng, rng);
    assert_eq!(owners.particles.pending(1).unwrap(), &pending);
    assert_eq!(owners.particles.free_count(), free);
}
