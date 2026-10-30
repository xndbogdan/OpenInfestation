use serde::Deserialize;

use super::*;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::ObjectClassKind;
use crate::process_particles::births::{ParticleBirthInputs, ParticleBirthRequest};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_particles::ProcessParticleRuntime;
use inf_formats::FrontendExecutable;

#[derive(Deserialize)]
struct Receipts {
    thrower_radius: u16,
    updates: Vec<Update>,
}

#[derive(Deserialize)]
struct Update {
    at: String,
    delta: u32,
    rng_in: u32,
    rng_out: u32,
    entry: Fields,
    exit: Fields,
    calls: Vec<Call>,
}

/// The flamethrower's bytes the update reads or writes, by native offset.
#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
struct Fields {
    position: [u16; 3],
    basis: [u32; 10],
    growth: u32,
    length: u32,
    owner_position: [u16; 3],
    root: [u16; 3],
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "name", rename_all = "lowercase")]
enum Call {
    Flame { point: [u16; 3], velocity: [u16; 3] },
    Burn { block: [u8; 10], point: [u16; 3] },
    Dispose,
}

/// Records the flames and burns, and births each flame through the real
/// particle runtime so the draws it makes line up with retail's.
struct Host<'a, 'exe> {
    rng: &'a mut u32,
    terrain: &'a crate::terrain_runtime::TerrainRuntime,
    particles: ProcessParticleRuntime,
    executable: FrontendExecutable<'exe>,
    process: ParticleFactoryProcessState,
    /// 00532768, the offline burn block.
    block: [u8; 10],
    calls: Vec<Call>,
}

impl Selector76Host for Host<'_, '_> {
    type Error = String;
    fn random(&mut self) -> u32 {
        name_entry_rng_next(self.rng)
    }
    fn terrain_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<i16, String> {
        Ok(self.terrain.height_at(xz))
    }
    fn flame(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        self.calls.push(Call::Flame { point, velocity });
        self.particles
            .birth_trail(
                ParticleBirthRequest {
                    particle_type: 9,
                    position: point,
                    velocity: Some(velocity),
                    flags: 0x800,
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(4),
                        process: &self.process,
                    },
                },
                self.executable,
                self.rng,
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
    fn burn(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        radii: [u16; 2],
        _: ActorHandle,
    ) -> Result<(), String> {
        let mut block = self.block;
        block[4..6].copy_from_slice(&radii[0].to_le_bytes());
        block[6..8].copy_from_slice(&radii[1].to_le_bytes());
        self.calls.push(Call::Burn { block, point });
        Ok(())
    }
}

/// Every update 00431FE0 of the flamethrower Infest03's vehicle fired in
/// world 6, each from its captured entry with the thrower where retail had
/// it: the no-time update inside 00431EF0 that only follows the thrower,
/// then three ticks growing the flame to 240, 540 and 600 with 3, 7 and 8
/// flames on the ground, burns every 60 with the offline block's radii
/// widening from 75/90 to 200/240, and the disposal at full growth. The
/// fields, every flame and burn, and the RNG words match.
#[test]
fn selector76_update_matches_every_recorded_tick() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../tests/fixtures/selector76_updates.json")).unwrap();
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let block: [u8; 10] = executable
        .mapped(0x0053_2768, 10)
        .unwrap()
        .bytes
        .try_into()
        .unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let thrower = world.arena.actor(vehicle).unwrap().handle;
    let mut failures = Vec::new();
    for update in &receipts.updates {
        let arena = &mut world.arena;
        // The thrower as the update found it, which its exit copies.
        arena.place_for_tests(vehicle, update.exit.owner_position);
        arena
            .with_supplied_family_dispatch_actor(vehicle, |state| {
                state.rotating.basis.words = update.exit.basis;
                state.rotating.visual.radius_word = receipts.thrower_radius;
            })
            .unwrap();
        let object = arena.allocate(ObjectClassKind::Selector76);
        arena.arm_selector76(object, thrower).unwrap();
        {
            let state = arena.selector76_mut(object).unwrap();
            state.dword_70 = update.entry.growth;
            state.dword_74 = update.entry.length;
        }
        let mut rng = update.rng_in;
        let mut host = Host {
            rng: &mut rng,
            terrain: &world.terrain,
            particles: ProcessParticleRuntime::default(),
            executable,
            process: ParticleFactoryProcessState::native_startup(),
            block,
            calls: Vec::new(),
        };
        arena
            .tick_selector76(object, update.delta, &mut host)
            .unwrap();
        let mut calls = std::mem::take(&mut host.calls);
        drop(host);
        let disposed = arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .any(|node| node.object == object);
        if disposed {
            calls.push(Call::Dispose);
        }
        let state = *arena.selector76(object).unwrap();
        let actual = Fields {
            position: arena.object(object).unwrap().base.position_words.unwrap(),
            basis: state.rotating.basis.words,
            growth: state.dword_70,
            length: state.dword_74,
            owner_position: state.owner_position_78.unwrap(),
            root: state.root_7e.unwrap(),
        };
        if actual != update.exit {
            failures.push(format!(
                "{}: {actual:?}\n   != {:?}",
                update.at, update.exit
            ));
        }
        if calls != update.calls {
            failures.push(format!(
                "{}: calls {calls:?}\n   != {:?}",
                update.at, update.calls
            ));
        }
        if rng != update.rng_out {
            failures.push(format!(
                "{}: rng {rng:08x} != {:08x}",
                update.at, update.rng_out
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// 00432490 on the recorded growths: red `+70 * 127 >> 16`, green and blue
/// `+70 * 80 >> 16`, radius `(+70 >> 6) + 256`.
#[test]
fn selector76_light_glows_with_its_growth() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector76);
    arena.place_for_tests(object, [40374, 64380, 19247]);
    for (growth, rgb, radius) in [
        (31_251_u32, [60_u8, 38, 38], 744),
        (62_501, [121, 76, 76], 1232),
        (0x1_0000, [127, 80, 80], 1280),
    ] {
        arena.selector76_mut(object).unwrap().dword_70 = growth;
        assert_eq!(
            arena.selector76_light(object).unwrap(),
            Selector76Light {
                position_xz: [40374_u16 as i16 as i32, 19247],
                radius,
                rgb,
            }
        );
    }
}
