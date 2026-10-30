//! Persistent G+80 particle custody and 004875A0's post-camera update boundary.
//! Impact updates queue real positional one-shots for the later draw pass.

use super::{PortPlatform, PortSession};
pub(super) mod combat;
pub(super) mod damage;
pub(super) mod factory;
pub(super) mod radius;
use crate::{
    process_particles::update::{
        ParticleTerrainContext, ParticleUpdateBirthInputs, ParticleUpdateRequest,
    },
    session::SessionState,
};

impl<P: PortPlatform> PortSession<P> {
    pub(super) fn tick_process_particles(&mut self, delta_micros: i32, state: &SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let assets = self.catalog.assets();
        let terrain =
            world
                .particle_impact_selectors
                .map(|impact_selectors| ParticleTerrainContext {
                    terrain: &mut world.terrain,
                    sine_quarter: &assets.sine_table().values,
                    wave_clock: self.process.animation().value(),
                    world_flags: world.world_flags,
                    impact_selectors,
                });
        let executable = match inf_formats::FrontendExecutable::parse(assets.executable_bytes()) {
            Ok(executable) => executable,
            Err(error) => {
                eprintln!("Particle update executable unavailable: {error}");
                return;
            }
        };
        //0048778D transfers pending before00487795 visits physical cells.
        self.particles.transfer_pending();
        let grant_text = super::world_owners::current_grant_text(&self.profile_name_classification);
        let mut host = combat::WorldProjectileHost {
            arena: &mut world.arena,
            mission: &mut world.mission,
            tasks: &mut self.tasks,
            sounds: &mut self.sounds,
            furniture: &mut self.furniture,
            residency: &self.residency,
            model_fallback: self.pelagia_models.as_ref().map(|models| &models.runtime),
            model_process: &self.model_process,
            furniture_query: &mut self.furniture_query,
            furniture_draw: &self.furniture_draw,
            terrain_source: &world.terrain_source,
            behavior_tables: &mut self.actor_behavior_tables,
            animation_clock: &self.process,
            session: state,
            listeners: &mut world.listeners,
            sine: &assets.sine_table().values,
            world_flags: world.world_flags,
            contacts: 0,
            impact_births: 0,
            ground_bursts: 0,
            grant_text: Some(&grant_text),
            furniture_hits: &mut world.crystal_flares.furniture_hits,
        };
        let report = self.particles.tick_particles(
            ParticleUpdateRequest {
                delta_microseconds: delta_micros,
                load_factor: world.fade.performance.particle_load_factor as i32,
                birth_inputs: ParticleUpdateBirthInputs {
                    network_active: self.factory_process.network_flag != 0,
                    phase: Some(u32::from(state.current_phase().id())),
                    process: &mut self.factory_process,
                },
                terrain,
            },
            executable,
            &mut world.rng,
            &mut host,
        );
        if host.contacts != 0 || host.impact_births != 0 {
            eprintln!(
                "World projectile collision: clock {}, {} contact(s), {} impact bubble(s), {} material burst(s)",
                self.process.animation().value(), host.contacts, host.impact_births, host.ground_bursts
            );
        }
        for omission in report.omissions {
            eprintln!(
                "Particle {:?} stopped during {:?} after reached writes: {:?}",
                omission.id, omission.phase, omission.error
            );
        }
    }
}
