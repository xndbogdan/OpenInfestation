//! Falls through the world's contact pass: Level 1's buggy dropped beside
//! its start runs its drive (`004195F0`), then the pass's terrain phase
//! (`004791F0` and the shared response `0047D2F0`), each tick, at the
//! recordings' 8 ticks a second and the port's 61, under retail's position
//! step and the port's carried one.

use super::*;
use crate::object_runtime::{
    PlayerControlBlock, VehicleDriveHost, VehicleDriveRequest, VehicleGround, VehiclePositionStep,
    WorldContactPhases,
};
use crate::process_sound::{ProcessSoundRef, ProcessSoundRuntime};
use crate::session_platform::world_contact::WorldContactPassHost;
use inf_formats::native_basis::isqrt_48efc0;

struct DriveHost<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a ResidentRuntime,
}

impl VehicleDriveHost for DriveHost<'_> {
    type Error = String;
    fn play_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.sounds
            .create_persistent(self.resident, sound as i16, gain, pitch, 0, || {
                Ok::<_, std::convert::Infallible>([0, 0, 0])
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn release_sound(&mut self, _: ProcessSoundRef) -> Result<(), String> {
        Ok(())
    }
}

pub(super) fn magnitude(vector: [i32; 3]) -> u32 {
    let sum = vector.iter().fold(0i32, |sum, value| {
        sum.wrapping_add(value.wrapping_mul(*value))
    });
    if sum as u32 & 0x8000_0000 != 0 {
        isqrt_48efc0((sum as u32 >> 2) as i32) << 1
    } else {
        isqrt_48efc0(sum)
    }
}

/// What a fall did: each response's strength (`0047B590`: the velocity
/// change times the mass, over 192) and the health it cost.
#[derive(Debug, Default)]
struct Fall {
    strengths: Vec<i32>,
    health_lost: u16,
}

/// A session has drawn before its first world tick, so the projection the
/// terrain query's vertices read is selected.
pub(super) fn select_projection(model_process: &crate::model_process::ModelProcessOwners) {
    let graphics = &model_process.graphics;
    graphics
        .publish_view(crate::frontend_scene::SceneView {
            eye: [0, 0, -1024],
            basis: crate::menu::MenuBasis::INITIAL,
        })
        .unwrap();
    graphics.publish_shift(0).unwrap();
    graphics
        .publish_setup(crate::frontend_scene::SceneSetup {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [128, 4096],
            fade_factor: 0x10000,
        })
        .unwrap();
    graphics
        .select_projection(
            crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            crate::model_context::ModelDispatch::Near,
        )
        .unwrap();
}

/// The buggy raised `height` above its start, falling for three seconds.
fn fall(delta: u32, step: VehiclePositionStep, height: u16) -> Fall {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut owners = TestRetirementOwners::new(&world, executable);
    select_projection(&owners.model_process);
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    world
        .arena
        .selector2_actor_mut_for_tests(vehicle)
        .unwrap()
        .dwords_1e8_1f4 = [1, 0xffff, 1, 0];
    world.arena.reset_vehicle_ground_drive(vehicle).unwrap();
    let mut position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    position[1] = position[1].wrapping_add(height);
    world.arena.set_position_for_tests(vehicle, position);
    let start = world.arena.actor(vehicle).unwrap().health;
    let mut outcome = Fall::default();
    let mut state = super::family_prefix_tests::gameplay_state();
    let mut elapsed = 0u64;
    while elapsed < 3_000_000 {
        {
            // 00416A62's speed magnitudes, as the update refreshes them.
            let arena = &mut world.arena;
            let velocity = arena.actor(vehicle).unwrap().words_8e_92;
            let actor_state = arena.selector2_actor_mut_for_tests(vehicle).unwrap();
            let change = std::array::from_fn(|axis| {
                i32::from(velocity[axis].wrapping_sub(actor_state.words_56c_570[axis]) as i16)
            });
            actor_state.drive.dword_574 =
                Some(magnitude(velocity.map(|word| i32::from(word as i16))));
            actor_state.drive.dword_578 = Some(magnitude(change));
            actor_state.words_56c_570 = velocity;
            let request = VehicleDriveRequest {
                delta,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: (elapsed * 48 / 1_000_000) as u32,
                    sine_quarter: &sine,
                },
                process_null: crate::object_runtime::ActorHandle::from_raw(0),
                position_step: step,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            };
            arena
                .tick_vehicle_ground_drive(
                    vehicle,
                    &mut PlayerControlBlock::default(),
                    &request,
                    &mut DriveHost {
                        sounds: &mut sounds,
                        resident: &runtime,
                    },
                )
                .unwrap();
        }
        let before = world.arena.actor(vehicle).unwrap().words_8e_92;
        let mut host = WorldContactPassHost {
            paced_skip: false,
            mission_completion: false,
            damage: WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: Some(&mut world.flash),
                mission: &mut world.mission,
                tasks: &mut owners.tasks,
                terrain: &mut world.terrain,
                particles: &mut owners.particles,
                sounds: &mut owners.sounds,
                furniture: &mut owners.furniture,
                residency: &runtime,
                executable,
                factory_process: &mut owners.factory_process,
                behavior_tables: &mut owners.behavior_tables,
                animation_clock: &owners.animation_clock,
                session: &mut state,
                particle_load_factor: 0x10000,
                behavior_factory: Some(BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine: &sine,
                    world_flags: world.world_flags,
                }),
                map: None,
            },
            rng: &mut world.rng,
            model_fallback: None,
            model_process: &owners.model_process,
            furniture_query: &mut owners.furniture_query,
            furniture_radius: &owners.furniture_radius,
            furniture_draw: &owners.furniture_draw,
            shared_basis: &owners.actor_draw.shared_basis,
            terrain_source: &world.terrain_source,
            world_flags: world.world_flags,
            sine_quarter: &sine,
            network_active: false,
            world_start: &mut world.world_start,
            camera_settings: crate::object_runtime::CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
            grant_text: None,
            factory_tables: std::cell::OnceCell::new(),
            math: &math,
        };
        world
            .arena
            .run_world_contact(
                vehicle,
                WorldContactPhases {
                    terrain: true,
                    furniture: false,
                },
                &mut host,
            )
            .unwrap();
        let after = world.arena.actor(vehicle).unwrap().words_8e_92;
        if after != before {
            let change: [i32; 3] =
                std::array::from_fn(
                    |axis| i32::from(after[axis].wrapping_sub(before[axis]) as i16),
                );
            let mass = i32::from(world.arena.actor(vehicle).unwrap().word_8c);
            outcome
                .strengths
                .push(magnitude(change) as i32 * mass / 192);
        }
        elapsed += u64::from(delta);
    }
    outcome.health_lost = start - world.arena.actor(vehicle).unwrap().health;
    outcome
}

/// The buggy's collision channel takes `(strength - 1500) * 4` (Infest02's
/// hard landing on Level 1, strength 1545, cost 180). Dropped 400 units it
/// lands under that and keeps its health; dropped 800 it lands at about
/// 1700 and is hurt. The tick rate and the position step move the strength
/// by a few percent, not across the threshold.
#[test]
fn a_hard_fall_hurts_the_buggy_at_every_tick_rate() {
    for delta in [16_384, 125_000] {
        for step in [
            VehiclePositionStep::Retail,
            VehiclePositionStep::CarryRemainder,
        ] {
            let soft = fall(delta, step, 400);
            assert!(
                soft.strengths.iter().all(|&strength| strength < 1500),
                "{delta} {step:?} {soft:?}"
            );
            assert_eq!(soft.health_lost, 0, "{delta} {step:?} {soft:?}");
            let hard = fall(delta, step, 800);
            let hardest = hard.strengths.iter().copied().max().unwrap_or(0);
            assert!((1600..1900).contains(&hardest), "{delta} {step:?} {hard:?}");
            assert!(hard.health_lost > 0, "{delta} {step:?} {hard:?}");
        }
    }
}

/// One vehicle landing a recording holds (`vehicle-falls.js`).
#[derive(serde::Deserialize)]
struct RetailFall {
    recording: String,
    hit: String,
    world: u32,
    /// `005372F4` and `005372F8` at the response.
    spacing: [u32; 2],
    /// The player's Shielding level, `+142`.
    shielding: u8,
    /// The vehicle's bytes `+000..+5FF` at the response's entry.
    vehicle: String,
    /// The five DWORDs `0047D2F0` read: flag, normal, displacement.
    contact: [u32; 5],
    /// The DWORD of the packet `0047D2F0` built.
    packet: u32,
    /// After the damage call returned.
    position: [u16; 3],
    velocity: [u16; 3],
    health: u16,
}

#[derive(serde::Deserialize)]
struct RetailFalls {
    falls: Vec<RetailFall>,
}

/// Every landing of the four recordings whose response reached the
/// vehicle's damage (45; eight hurt), replayed from the vehicle's recorded
/// state through the port's world contact pass: the terrain query
/// `0044EC50` finds the recorded contact, the response `0047D2F0` pushes
/// and rebounds the vehicle as it did, and the damage leaves the recorded
/// health.
#[test]
fn every_recorded_landing_lands_in_the_port_as_it_did() {
    use crate::object_runtime::WorldContactHost;
    let fixture: RetailFalls = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_falls_infest.json"
    ))
    .unwrap();
    assert_eq!(fixture.falls.len(), 45);
    let mut worlds: Vec<(u32, u32)> = fixture
        .falls
        .iter()
        .map(|fall| (fall.world, fall.spacing[0]))
        .collect();
    worlds.sort_unstable();
    worlds.dedup();
    let mut failures = Vec::new();
    let mut hurt = 0;
    for (selection, mask) in worlds {
        let spacing = match mask {
            0x7f => crate::terrain_runtime::TerrainSpacing::Units128,
            0xff => crate::terrain_runtime::TerrainSpacing::Units256,
            _ => panic!("terrain mask {mask:x}"),
        };
        let (runtime, assets, mut world) =
            crate::test_worlds::offline_world_retained(selection, spacing);
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sine = assets.sine_table().values;
        let math = inf_formats::native_basis::NativeBasisMath::new(sine);
        let mut owners = TestRetirementOwners::new(&world, executable);
        select_projection(&owners.model_process);
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let player = world.arena.first_player().unwrap();
        let mut state = super::family_prefix_tests::gameplay_state();
        for fall in fixture
            .falls
            .iter()
            .filter(|fall| (fall.world, fall.spacing[0]) == (selection, mask))
        {
            let name = format!("{} {} (world {})", fall.recording, fall.hit, fall.world);
            assert_eq!(
                world.terrain.sampling().height_scale.shift(),
                fall.spacing[1],
                "{name}: 005372F8"
            );
            let bytes: Vec<u8> = (0..fall.vehicle.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&fall.vehicle[at..at + 2], 16).unwrap())
                .collect();
            world
                .arena
                .inject_vehicle_snapshot_for_tests(vehicle, &bytes);
            world
                .arena
                .set_player_shielding_for_tests(player, fall.shielding);
            let health = world.arena.actor(vehicle).unwrap().health;
            let mut host = WorldContactPassHost {
                paced_skip: false,
                mission_completion: false,
                damage: WorldDamageOwners {
                    grant_text: None,
                    camera_settings: None,
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut owners.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut owners.particles,
                    sounds: &mut owners.sounds,
                    furniture: &mut owners.furniture,
                    residency: &runtime,
                    executable,
                    factory_process: &mut owners.factory_process,
                    behavior_tables: &mut owners.behavior_tables,
                    animation_clock: &owners.animation_clock,
                    session: &mut state,
                    particle_load_factor: 0x10000,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut world.listeners,
                        sine: &sine,
                        world_flags: world.world_flags,
                    }),
                    map: None,
                },
                rng: &mut world.rng,
                model_fallback: None,
                model_process: &owners.model_process,
                furniture_query: &mut owners.furniture_query,
                furniture_radius: &owners.furniture_radius,
                furniture_draw: &owners.furniture_draw,
                shared_basis: &owners.actor_draw.shared_basis,
                terrain_source: &world.terrain_source,
                world_flags: world.world_flags,
                sine_quarter: &sine,
                network_active: false,
                world_start: &mut world.world_start,
                camera_settings: crate::object_runtime::CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
                grant_text: None,
                factory_tables: std::cell::OnceCell::new(),
                math: &math,
            };
            let recorded = fall.contact.map(Some);
            match host.terrain_contact(&world.arena, vehicle) {
                Ok(Some(contact)) if contact.native_words() == recorded => {}
                Ok(other) => failures.push(format!(
                    "{name}: contact {other:x?}, recorded {:x?}",
                    fall.contact
                )),
                Err(error) => failures.push(format!("{name}: contact {error}")),
            }
            if let Err(error) = world.arena.run_world_contact(
                vehicle,
                WorldContactPhases {
                    terrain: true,
                    furniture: false,
                },
                &mut host,
            ) {
                failures.push(format!("{name}: {error}"));
                continue;
            }
            let after = (
                world
                    .arena
                    .object(vehicle)
                    .unwrap()
                    .base
                    .position_words
                    .unwrap(),
                world.arena.actor(vehicle).unwrap().words_8e_92,
                world.arena.actor(vehicle).unwrap().health,
            );
            if after != (fall.position, fall.velocity, fall.health) {
                failures.push(format!(
                    "{name}: packet {:x}, health {health}: port {after:?}, recorded {:?}",
                    fall.packet,
                    (fall.position, fall.velocity, fall.health)
                ));
            }
            hurt += usize::from(fall.health != health);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(hurt, 8);
}
