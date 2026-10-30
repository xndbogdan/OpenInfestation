//! The doom countdown's effects in the mission tick `0048E2E0`: with G+1C7
//! set, the explosions around the first player's unit in the last 5 s
//! (`0048E380..0048E4C5`) and the end's retirement of that unit
//! (`0048E505..0048E515`). [`crate::world_mission::WorldMission::tick_countdown`]
//! owns the countdown's own fields.

use inf_formats::native_basis::NativeBasisMath;

use super::{
    particles::damage::{world_retirement_host, BehaviorFactoryLoan, WorldDamageOwners},
    world_actor_factory::{
        RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
        WorldCameraFactoryContext, WorldCameraInputs, WorldFactoryInitialization,
        WorldFactoryPosition,
    },
    world_flash_host::WorldFlashCameraHost,
    PortPlatform, PortSession,
};
use crate::{
    object_runtime::{ActorChildResource, ObjectId},
    process_particles::births::{ParticleBirthInputs, ParticleBurst, ParticleBurstRequest},
    process_sound::ProcessParticleSoundQueue,
    session::SessionState,
    world_flash::{WorldFlashPosition, WorldFlashRequest},
};

/// `0048E40E`: the world factory's resource for each explosion.
pub(crate) const COUNTDOWN_EXPLOSION_RESOURCE: u32 = 0x37;
/// `0048E433..0048E480`: the 32-byte `0048A8B0` record, four type-1 debris
/// within 20 of the point, a type-1 centre, speed 8, no flags.
pub(crate) const COUNTDOWN_BURST: ParticleBurst = ParticleBurst {
    debris_count: 4,
    position: [0; 3],
    placement_radius: 0x14,
    central_type: 1,
    debris_types: [1, 1],
    speed: 8,
    flags: 0,
};
/// `0048E4BC`: `00487D70(NULL, 0, 0x28)`.
pub(crate) const COUNTDOWN_FLASH_STYLE: u32 = 0x28;

/// 0048F060, the process RNG's next 16 bits.
#[track_caller]
fn draw(rng: &mut u32) -> u32 {
    crate::name_entry::name_entry_rng_next(rng)
}

/// `0048E385..0048E3AB` and `0048E48E..0048E4B4`: a draw against the
/// seconds left, `left / 2,000,000 + 2` (signed), succeeds on a zero rest.
pub(crate) fn countdown_chance(rng: &mut u32, left: u32) -> bool {
    draw(rng) as i32 % ((left as i32) / 2_000_000 + 2) == 0
}

/// `0048E380..0048E419`: the chance, then the point by the unit's +32
/// position: X and Z by `rest(800) - 400`, Y by `rest(400)` up, WORD adds.
pub(crate) fn countdown_explosion_point(
    rng: &mut u32,
    left: u32,
    position: [u16; 3],
) -> Option<[u16; 3]> {
    if !countdown_chance(rng, left) {
        return None;
    }
    let mut point = position;
    point[0] = point[0]
        .wrapping_add((draw(rng) % 800) as u16)
        .wrapping_sub(400);
    point[1] = point[1].wrapping_add((draw(rng) % 400) as u16);
    point[2] = point[2]
        .wrapping_add((draw(rng) % 800) as u16)
        .wrapping_sub(400);
    Some(point)
}

impl<P: PortPlatform> PortSession<P> {
    /// `0048E380..0048E4C5`, one tick of the last 5 s: on a chance, the
    /// world factory builds resource 0x37 at a random point by the unit
    /// (X and Z within -400..399, Y 0..399 above), a debris burst goes off
    /// there, and on a second chance the whole world flashes.
    pub(super) fn countdown_explosion(&mut self, unit: ObjectId, state: &SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let left = world.mission.dword_24.unwrap_or(0);
        let result = (|| {
            let position = world
                .arena
                .object(unit)
                .map_err(|error| error.to_string())?
                .base
                .position_words
                .ok_or("the countdown's unit has no position")?;
            let Some(point) = countdown_explosion_point(&mut world.rng, left, position) else {
                return Ok(());
            };
            let executable =
                inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                    .map_err(|error| error.to_string())?;
            let sine = &self.catalog.assets().sine_table().values;
            let math = NativeBasisMath::new(*sine);
            let camera_settings = self.platform.world_tick_inputs().camera_settings;
            // 0048E42E: 00460310(&point, NULL, -1, 0, NULL) with ECX 0x37.
            WorldActorChildHost {
                residency: &self.residency,
                executable,
                process: &self.factory_process,
                terrain: &world.terrain,
                world_flags: world.world_flags,
                clock: self.process.animation().value(),
                sine,
                math: &math,
                camera_settings: Some(camera_settings),
                world_start: Some(&mut world.world_start),
                listeners: &mut world.listeners,
                listener_effects: &mut RequiredListenerEffects,
            }
            .create_object(
                &mut world.arena,
                WorldActorFactoryRequest {
                    resource: ActorChildResource::Full(COUNTDOWN_EXPLOSION_RESOURCE),
                    position: WorldFactoryPosition::Local(point),
                    spawn_mode_1: false,
                    network_handle: None,
                    initialization: WorldFactoryInitialization::Bare,
                },
                &mut world.rng,
            )
            .map_err(|error| format!("00460310 resource 0x37: {error:?}"))?;
            // 0048E484: 0048A8B0.
            self.particles
                .birth_burst(
                    ParticleBurstRequest {
                        burst: ParticleBurst {
                            position: point,
                            ..COUNTDOWN_BURST
                        },
                        load_factor: world.fade.performance.particle_load_factor,
                        inputs: ParticleBirthInputs {
                            network_active: self.factory_process.network_flag != 0,
                            phase: Some(u32::from(state.current_phase().id())),
                            process: &self.factory_process,
                        },
                    },
                    executable,
                    &mut world.rng,
                    &mut ProcessParticleSoundQueue {
                        sounds: &mut self.sounds,
                        residency: &self.residency,
                    },
                )
                .map_err(|error| format!("0048A8B0: {error:?}"))?;
            if countdown_chance(&mut world.rng, left) {
                let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
                    residency: &self.residency,
                    sine_quarter: sine,
                    settings: camera_settings,
                    process: Some(&self.factory_process),
                    animation_clock: self.process.animation().value(),
                });
                let mut host = WorldFlashCameraHost {
                    camera: camera.factory(&world.terrain, world.world_flags, &mut world.listeners),
                    residency: &self.residency,
                };
                world
                    .flash
                    .trigger(
                        &mut world.arena,
                        WorldFlashRequest {
                            position: WorldFlashPosition::Null,
                            radius: 0,
                            style_argument: COUNTDOWN_FLASH_STYLE,
                        },
                        &mut host,
                        &mut world.rng,
                    )
                    .map_err(|error| format!("00487D70: {error}"))?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            eprintln!("World mission countdown explosion stopped after reached writes: {error}");
        }
    }

    /// `0048E505..0048E515`: the end's first stage retires the unit
    /// (`0047CE90`) when its virtual +64 gives a vehicle; the countdown's
    /// last 3 s are armed after it.
    pub(super) fn countdown_retire_unit(&mut self, unit: ObjectId, state: &SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let result = (|| {
            if !world
                .arena
                .actor_is_vehicle(unit)
                .map_err(|error| error.to_string())?
            {
                return Ok(());
            }
            let executable =
                inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                    .map_err(|error| error.to_string())?;
            let grant_text =
                super::world_owners::current_grant_text(&self.profile_name_classification);
            let mut owners = WorldDamageOwners {
                grant_text: Some(&grant_text),
                camera_settings: Some(self.platform.world_tick_inputs().camera_settings),
                flash: Some(&mut world.flash),
                mission: &mut world.mission,
                tasks: &mut self.tasks,
                terrain: &mut world.terrain,
                particles: &mut self.particles,
                sounds: &mut self.sounds,
                furniture: &mut self.furniture,
                residency: &self.residency,
                executable,
                factory_process: &mut self.factory_process,
                behavior_tables: &mut self.actor_behavior_tables,
                animation_clock: &self.process,
                session: state,
                particle_load_factor: world.fade.performance.particle_load_factor,
                behavior_factory: Some(BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine: &self.catalog.assets().sine_table().values,
                    world_flags: world.world_flags,
                }),
                map: None,
            };
            let mut host = world_retirement_host(&mut owners, &mut world.rng);
            let null = host.owners.factory_process.process_null_handle;
            world
                .arena
                .retire_actor(unit, null, &mut host)
                .map(|_| ())
                .map_err(|error| format!("0047CE90: {error:?}"))
        })();
        if let Err(error) = result {
            eprintln!(
                "World mission countdown's unit retirement stopped after reached writes: {error}"
            );
        }
    }
}

#[cfg(test)]
#[path = "countdown_tests.rs"]
mod tests;
