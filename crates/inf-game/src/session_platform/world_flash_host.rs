//! Shared 00487D70 world-flash host over the actual 00478E00 camera, used by
//! the family dying camera effect and the furniture camera callback 004375D0.

use crate::object_runtime::{CameraSettings, CreationListeners, ObjectArena, ObjectId};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::session_platform::world_actor_factory::{WorldCameraFactoryContext, WorldCameraInputs};
use crate::terrain_runtime::TerrainRuntime;
use crate::world_flash::{WorldFlashPosition, WorldFlashRequest, WorldFlashState};
use inf_formats::native_basis::NativeBasis;

/// What `00487D70`'s camera acquisition (`00478E00`) reads from the world.
pub(in crate::session_platform) struct WorldFlashCamera<'a> {
    pub residency: &'a crate::ResidentRuntime,
    pub sine_quarter: &'a [u16; 4096],
    pub settings: CameraSettings,
    pub process: &'a ParticleFactoryProcessState,
    pub animation_clock: u32,
    pub terrain: &'a TerrainRuntime,
    pub world_flags: u32,
    pub listeners: &'a mut CreationListeners,
}

/// `00487D70(position, radius, style)`, the world flash.
pub(in crate::session_platform) fn trigger_world_flash(
    flash: &mut WorldFlashState,
    arena: &mut ObjectArena,
    request: WorldFlashRequest,
    camera: WorldFlashCamera<'_>,
    rng: &mut u32,
) -> Result<(), String> {
    let mut context = WorldCameraFactoryContext::new(WorldCameraInputs {
        residency: camera.residency,
        sine_quarter: camera.sine_quarter,
        settings: camera.settings,
        process: Some(camera.process),
        animation_clock: camera.animation_clock,
    });
    let mut host = WorldFlashCameraHost {
        camera: context.factory(camera.terrain, camera.world_flags, camera.listeners),
        residency: camera.residency,
    };
    flash
        .trigger(arena, request, &mut host, rng)
        .map(|_| ())
        .map_err(|error| format!("00487D70 world flash: {error}"))
}

/// Sonic Shock's pulse from the object draw, where the vehicle's weapons
/// fire: the map's jitter and the flash through the camera the frame is
/// drawn from, which `00478E00` returns without its factory.
pub(crate) struct DrawPulseHost<'a> {
    pub flash: &'a mut WorldFlashState,
    pub map: Option<&'a mut crate::world_map::WorldMapState>,
    pub residency: &'a crate::ResidentRuntime,
}

impl crate::object_runtime::DetonatorPulseHost for DrawPulseHost<'_> {
    fn jam_map(&mut self) -> Result<(), String> {
        self.map
            .as_deref_mut()
            .ok_or("0048D9AB: the pulse's map jitter needs the world map G+1B4")?
            .apply(crate::world_map::WorldMapJitterWrite::MissionEvent48D980)
            .map_err(|error| error.to_string())
    }

    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        self.flash
            .trigger(
                arena,
                point_flash(position, PULSE_FLASH_RADIUS, 0),
                &mut LiveCameraFlashHost {
                    residency: self.residency,
                },
                rng,
            )
            .map(|_| ())
            .map_err(|error| format!("0048D9B8 world flash: {error}"))
    }
}

/// `0048D99F`: the pulse's flash radius.
const PULSE_FLASH_RADIUS: u32 = 0xf00;

/// `00487D70`'s host when the world camera lives.
struct LiveCameraFlashHost<'a> {
    residency: &'a crate::ResidentRuntime,
}

impl crate::world_flash::WorldFlashHost for LiveCameraFlashHost<'_> {
    type Error = String;
    fn acquire_camera(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        arena
            .existing_world_camera()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "00478E00 would construct a camera, which the draw does not lend".into())
    }
    fn camera_basis(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
    ) -> Result<NativeBasis, String> {
        camera_basis(arena, camera)
    }
    fn style_word(&mut self, argument: u32) -> Result<u32, String> {
        style_word(self.residency, argument)
    }
}

/// A flash at a point (`WorldFlashPosition::Words`).
pub(in crate::session_platform) fn point_flash(
    position: [u16; 3],
    radius: u32,
    style_argument: u32,
) -> WorldFlashRequest {
    WorldFlashRequest {
        position: WorldFlashPosition::Words(position),
        radius,
        style_argument,
    }
}

pub(in crate::session_platform) struct WorldFlashCameraHost<'a> {
    pub camera: crate::object_runtime::CameraFactoryInputs<'a>,
    pub residency: &'a crate::ResidentRuntime,
}

impl crate::world_flash::WorldFlashHost for WorldFlashCameraHost<'_> {
    type Error = String;
    fn acquire_camera(
        &mut self,
        arena: &mut ObjectArena,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        arena
            .world_camera_with_rng(&mut self.camera, Some(rng))
            .map_err(|e| e.to_string())
    }
    fn camera_basis(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
    ) -> Result<NativeBasis, String> {
        camera_basis(arena, camera)
    }
    fn style_word(&mut self, argument: u32) -> Result<u32, String> {
        style_word(self.residency, argument)
    }
}

fn camera_basis(arena: &mut ObjectArena, camera: ObjectId) -> Result<NativeBasis, String> {
    //478E00's admitted final camera table supplies the rotating basis.
    arena.selector16(camera).map_err(|e| e.to_string())?;
    Ok(arena
        .actor(camera)
        .map_err(|e| e.to_string())?
        .rotating
        .basis)
}

fn style_word(residency: &crate::ResidentRuntime, argument: u32) -> Result<u32, String> {
    use crate::render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    };
    let resource = RenderResources::new(residency)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Style,
            argument: argument as i32,
        })
        .map_err(|e| e.to_string())?;
    match resource.data {
        RenderResourceData::Style(word) => Ok(word),
        _ => Err("flash H6 resource is not a style word".into()),
    }
}
