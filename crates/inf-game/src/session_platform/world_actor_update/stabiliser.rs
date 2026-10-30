//! The Seismic Stabiliser over the world's owners: its update's terrain,
//! status smoke (`0047DFD0`) and world flash (`00487D70`), and its death's
//! grant, terrain event, doom countdown and linked retirement.

use crate::{
    object_runtime::{
        BridgeTerrain, CameraSettings, GrantOwner, LiveGrantContext, LiveGrantOperation,
        LiveGrantRequest, ObjectArena, ObjectId, PlayerId, Selector49DeathHost,
        Selector49UpdateHost,
    },
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest},
    session_platform::{
        intro::retirement::ProductionRetirementHost,
        world_actor_factory::{WorldCameraFactoryContext, WorldCameraInputs},
    },
    terrain_runtime::ProcessTerrainPolicy,
    world_flash::{WorldFlashPosition, WorldFlashRequest, WorldFlashState},
};

use super::WorldFamilyUpdateHost;

/// `004765D6`: 00487D70's radius, with style 0.
const FLASH_RADIUS: u32 = 0xf00;

/// The update on the update list.
pub(crate) struct WorldStabiliserHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub policy: &'a mut ProcessTerrainPolicy,
    /// Settings +70, which `00450370` rereads.
    pub detail: u32,
    pub world_flags: u32,
    pub flash: &'a mut WorldFlashState,
    pub camera_settings: CameraSettings,
    pub animation_clock: u32,
}

impl Selector49UpdateHost for WorldStabiliserHost<'_, '_> {
    type Error = String;

    fn terrain(&mut self) -> BridgeTerrain<'_> {
        BridgeTerrain {
            terrain: &mut *self.family.owners.terrain,
            policy: &mut *self.policy,
            detail: self.detail,
            world_flags: Some(self.world_flags),
        }
    }

    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        delta: u32,
    ) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let rng = &mut *self.family.rng;
        let load_factor = Some(owners.particle_load_factor as i32);
        let trail = arena
            .status_visual_trail(object, delta, load_factor, || {
                Ok::<_, std::convert::Infallible>(crate::name_entry::name_entry_rng_next(rng))
            })
            .map_err(|error| format!("0047DFD0 stabiliser smoke: {error:?}"))?;
        let Some(trail) = trail else {
            return Ok(());
        };
        let request = ParticleBirthRequest {
            particle_type: trail.particle_type,
            position: trail.position,
            velocity: Some(trail.velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 stabiliser smoke: {error:?}"))
    }

    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the stabiliser's flash needs the world's listeners")?;
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: owners.residency,
            sine_quarter: self.family.sine_quarter,
            settings: self.camera_settings,
            process: Some(&*owners.factory_process),
            animation_clock: self.animation_clock,
        });
        let mut host = super::super::world_flash_host::WorldFlashCameraHost {
            camera: camera.factory(&*owners.terrain, self.world_flags, &mut *loan.listeners),
            residency: owners.residency,
        };
        self.flash
            .trigger(
                arena,
                WorldFlashRequest {
                    position: WorldFlashPosition::Words(position),
                    radius: FLASH_RADIUS,
                    style_argument: 0,
                },
                &mut host,
                &mut *self.family.rng,
            )
            .map(|_| ())
            .map_err(|error| format!("004765DC 00487D70: {error}"))
    }
}

/// The death, inside the lifecycle's own host.
pub(crate) struct StabiliserDeathHost<'h, 'a> {
    pub host: &'h mut ProductionRetirementHost<'a>,
}

impl Selector49DeathHost for StabiliserDeathHost<'_, '_> {
    type Error = String;

    fn grant(&mut self, arena: &mut ObjectArena, player: PlayerId, id: u16) -> Result<(), String> {
        let owners = &self.host.owners;
        arena
            .mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw { id: u32::from(id) },
                },
                &LiveGrantContext {
                    process: Some(owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                    text_data: owners.grant_text,
                },
            )
            .map_err(|error| format!("00476535 004750C0({id:#x}): {error}"))
    }

    fn terrain_event(&mut self) {
        self.host.owners.mission.terrain_event_5483b0 = true;
    }

    fn start_doom(&mut self, micros: u32) {
        self.host.owners.mission.start_doom_countdown(micros);
    }

    fn retire(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        let null = self.host.owners.factory_process.process_null_handle;
        arena
            .retire_actor(actor, null, &mut *self.host)
            .map(|_| ())
            .map_err(|error| format!("00476594 0047CE90: {error:?}"))
    }
}
