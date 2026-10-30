//! 00464C80 list passes and 00466CD0/00465810 deferred animation draws.
//! Manager transfer, batch boundaries and light-grid custody belong to callers.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{ParticleError, ParticleId, ProcessParticleRuntime};
use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch, ModelProjection},
    model_primitives::{
        PrimitiveCallback, PrimitiveCommand, TextureSortError, TextureSortIdentity,
    },
    model_projection::ModelProjectionBackend,
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
    render_resources::{RenderResource, RenderResourceData, RenderResourceError},
    terrain_runtime::TerrainRuntime,
    world_lighting::TerrainLightRequest,
};

pub mod animation;
mod light_effect;
mod projectile;
mod shadow;

#[cfg(test)]
mod tests;

/// Under the tracer the water hides what lies under it: an explosion's
/// fireball (class `00528914`) whose centre is under the water's plane, over
/// water, is left to the traced water's glow from its own light, its command
/// marked as the traced world's, which only a traced frame leaves undrawn.
/// Worlds without water hold the plane at -4096 or below.
fn fireball_under_water(vtable: u32, position: [u16; 3], terrain: Option<&TerrainRuntime>) -> bool {
    let Some(terrain) = terrain else {
        return false;
    };
    let plane = terrain.plane_base() >> terrain.plane_shift();
    let [x, y, z] = position;
    vtable == super::splashes::CENTRAL_VTABLE
        && plane > -4096
        && i32::from(terrain.height_at([x, z])) < plane
        && i32::from(y as i16) < plane
}

/// Final particle classes whose draw slot +08 is the 00466CD0 wrapper over
/// 00465810 (+28 table, +2C scale, +30 shadow). 00526F20 is factory
/// 004665D0's class, whose +30 keeps the pool slot's earlier bytes.
const ANIMATED_DRAW_VTABLES: [u32; 13] = [
    0x0052_6f20,
    0x0052_87b4,
    0x0052_87f4,
    0x0052_8814,
    0x0052_8854,
    0x0052_8874,
    0x0052_8894,
    0x0052_88b4,
    0x0052_88d4,
    0x0052_88f4,
    0x0052_8914,
    0x0052_8934,
    0x0052_8954,
];

/// Final particle classes whose +08 is 00466960: the same 00465810 draw with
/// a zero shadow amount and no +30 read. 00526F60 is factory 00466850's class.
const UNSHADOWED_DRAW_VTABLES: [u32; 1] = [0x0052_6f60];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleDrawBank {
    Active,
    Pending,
}

/// Native renderer-context operands, separate from published view globals.
#[derive(Clone, Copy, Debug)]
pub struct ParticleDrawContext {
    /// Context+04/+08/+0C, also the origin subtracted as wrapping WORDs.
    pub position: [i32; 3],
    /// Context+10..+34. Only category5 uses this basis.
    pub basis: MenuBasis,
    pub view: SceneView,
    pub projection: ModelProjection,
    pub coordinate_shift: u8,
    pub fog_factor: i32,
    /// Context+50's local projection callback. Normal00544740 is Far.
    pub projection_dispatch: ModelDispatch,
    /// Context+6C, copied only by the far quad producer.
    pub far_color: u32,
}

#[derive(Clone, Copy)]
pub struct ParticleDrawRequest<'a> {
    pub bank: ParticleDrawBank,
    pub context: ParticleDrawContext,
    pub terrain: Option<&'a TerrainRuntime>,
}

/// The actual 0045B1A0 effect; no successful empty default is provided.
pub trait ParticleDrawLighting {
    type Error;
    /// Deposit `request`'s light, shed by the particle at `emitter` (signed
    /// X, Y, Z), which only the Vulkan tracer reads.
    fn terrain_light(
        &mut self,
        request: TerrainLightRequest,
        emitter: [i32; 3],
    ) -> Result<(), Self::Error>;
}

pub trait ParticleDrawHost<'r>: ParticleDrawLighting {
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error>;
    fn material_sorting(&mut self) -> Result<bool, Self::Error>;
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error>;
    /// Converted H7 DWORD. Absence becomes a typed stop at its reached read.
    fn shadow_material(&mut self, _slot: i16) -> Result<Option<u32>, Self::Error> {
        Ok(None)
    }
}

#[derive(Debug, Error)]
pub enum ParticleDrawError<E> {
    #[error(transparent)]
    Particle(#[from] ParticleError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Animation(#[from] animation::ParticleAnimationError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    TextureSort(#[from] TextureSortError),
    #[error("particle reached unowned draw vtable {0:08X}")]
    Vtable(u32),
    #[error("particle {id:?} was stopped during {phase:?}; drawing cannot replay its prefix")]
    PreviouslyStopped {
        id: ParticleId,
        phase: super::ParticleStopPhase,
    },
    #[error("particle light requires resident terrain")]
    MissingTerrain,
    #[error("particle frame sprite {0} reached a null native entry")]
    MissingSprite(i16),
    #[error("particle size division faults at native depth {0}")]
    SizeDivision(i32),
    #[error("far particle draw consumes unwritten projection fog")]
    UnwrittenFog,
    #[error("particle shadow requires converted H7 material {0}")]
    MissingShadowMaterial(i16),
    #[error("particle shadow size division faults for scale {0}")]
    ShadowDivision(i32),
    #[error("particle terrain-flash division faults for table {table:08X}, lifetime {lifetime}")]
    LightDivision { table: u32, lifetime: u16 },
    #[error("required particle draw service failed")]
    Host(E),
}

#[derive(Debug)]
pub struct ParticleDrawOmission<E> {
    pub id: ParticleId,
    pub error: ParticleDrawError<E>,
}

#[derive(Debug)]
pub struct ParticleDrawReport<E> {
    pub visited: usize,
    pub stopped: usize,
    pub commands: usize,
    pub culled: usize,
    pub omissions: Vec<ParticleDrawOmission<E>>,
}

impl<E> Default for ParticleDrawReport<E> {
    fn default() -> Self {
        Self {
            visited: 0,
            stopped: 0,
            commands: 0,
            culled: 0,
            omissions: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct ParticleDrawExecution<'a, 'exe> {
    request: ParticleDrawRequest<'a>,
    executable: FrontendExecutable<'exe>,
    projector: ModelProjectionBackend,
}

/// Arguments supplied by the actual class wrapper to shared 00465810.
/// The ordinary 00466CD0 wrapper reads +30; 004663E0 supplies zero without
/// reading that constructor-unwritten DWORD.
struct ParticleAnimationSubmission {
    table: u32,
    scale: i32,
    shadow_amount: i32,
}

/// Exact 00465720 operands, independent from sprite animation submission.
struct ParticleLightParameters {
    amount: i16,
    rgb: [i8; 3],
}

impl ProcessParticleRuntime {
    /// Each feature failure preserves native mutations and submitted prefix,
    /// then lets the other particles draw. No list transfer or flush occurs.
    pub fn draw_particles<'r, H: ParticleDrawHost<'r>>(
        &mut self,
        request: ParticleDrawRequest<'_>,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> ParticleDrawReport<H::Error> {
        let mut report = ParticleDrawReport::default();
        let ordinary_transform = ModelContext::new(request.context.position, MenuBasis::INITIAL)
            .publish(request.context.view, request.context.coordinate_shift);
        let category_five_transform =
            ModelContext::new(request.context.position, request.context.basis)
                .publish(request.context.view, request.context.coordinate_shift);
        for category in [0, 1, 2, 3, 5] {
            let projector = ModelProjectionBackend {
                transform: if category == 5 {
                    category_five_transform
                } else {
                    ordinary_transform
                },
                view: request.context.view,
                projection: request.context.projection,
                dispatch: request.context.projection_dispatch,
                coordinate_shift: request.context.coordinate_shift,
                fog_factor: request.context.fog_factor,
            };
            let list = match request.bank {
                ParticleDrawBank::Active => self.active[category].clone(),
                ParticleDrawBank::Pending => self.pending[category].clone(),
            };
            for id in list {
                report.visited += 1;
                if self.stopped_phase(id).is_some() {
                    report.stopped += 1;
                    continue;
                }
                if let Err(error) = self.draw_particle(
                    id,
                    ParticleDrawExecution {
                        request,
                        executable,
                        projector,
                    },
                    host,
                    &mut report,
                ) {
                    if !matches!(error, ParticleDrawError::PreviouslyStopped { .. }) {
                        self.stop_particle(id, super::ParticleStopPhase::Draw);
                    }
                    report.omissions.push(ParticleDrawOmission { id, error });
                }
            }
        }
        report
    }

    fn draw_particle<'r, H: ParticleDrawHost<'r>>(
        &mut self,
        id: ParticleId,
        execution: ParticleDrawExecution<'_, '_>,
        host: &mut H,
        report: &mut ParticleDrawReport<H::Error>,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        if let Some(phase) = self.stopped_phase(id) {
            return Err(ParticleDrawError::PreviouslyStopped { id, phase });
        }
        let record = self.record(id)?;
        let vtable = record.read_dword(0)?;
        // 526F80+08=466D40: RET4, including constructor-unwritten+30.
        if matches!(vtable, 0x0052_6f80 | 0x0052_8834) {
            return Ok(());
        }
        if vtable == 0x0052_6efc {
            return self.draw_projectile(id, execution, host, report);
        }
        if vtable == 0x0052_8974 {
            return self.draw_terrain_flash(id, execution, host);
        }
        let unshadowed = UNSHADOWED_DRAW_VTABLES.contains(&vtable);
        if !unshadowed && !ANIMATED_DRAW_VTABLES.contains(&vtable) {
            return Err(ParticleDrawError::Vtable(vtable));
        }
        let shadow_amount = if unshadowed {
            0
        } else {
            record.read_dword(0x30)? as i32
        };
        let scale = record.read_dword(0x2c)? as i32;
        let table = record.read_dword(0x28)?;
        self.draw_animation(
            id,
            execution,
            ParticleAnimationSubmission {
                table,
                scale,
                shadow_amount,
            },
            host,
            report,
        )
    }

    fn draw_animation<'r, H: ParticleDrawHost<'r>>(
        &mut self,
        id: ParticleId,
        execution: ParticleDrawExecution<'_, '_>,
        submission: ParticleAnimationSubmission,
        host: &mut H,
        report: &mut ParticleDrawReport<H::Error>,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        let ParticleDrawExecution {
            request,
            executable,
            mut projector,
        } = execution;
        let ParticleAnimationSubmission {
            table,
            scale,
            shadow_amount,
        } = submission;
        if table == 0 {
            self.flag(id, 0x40)?;
            report.culled += 1;
            return Ok(());
        }
        let position = self.record(id)?.position()?;
        let delta = std::array::from_fn(|axis| {
            i32::from(position[axis].wrapping_sub(request.context.position[axis] as u16) as i16)
        });
        let mut projected = ModelVertexProjection::default();
        let depth = match projector.project(ModelVertexInput::Local(delta), &mut projected) {
            Ok(depth) => depth as i32,
            Err(error) => match error {},
        };
        let xy = projected.read_u32(0).expect("projector writes XY");
        let x = i32::from(xy as i16);
        let y = i32::from((xy >> 16) as i16);
        let clip = projected.bytes[8].expect("projector writes clip");
        let projection = request.context.projection;
        if clip & 0x6d != 0
            && (clip & 0x40 != 0
                || depth >= projection.fade[1]
                || (projection.viewport[0] >> 1).wrapping_add(x).wrapping_abs()
                    > projection.viewport[0].wrapping_mul(2)
                || (projection.viewport[1] >> 1).wrapping_add(y).wrapping_abs()
                    > projection.viewport[1].wrapping_mul(2))
        {
            if self.record(id)?.read_word(0x12)? & 2 == 0 {
                self.flag(id, 0x40)?;
            }
            report.culled += 1;
            return Ok(());
        }
        let frame = animation::ParticleAnimationFrame::read(
            executable,
            animation::frame_address(executable, table, self.record(id)?.read_word(0x10)?)?,
        )?;
        self.draw_light(
            id,
            ParticleLightParameters {
                amount: frame.light_amount,
                rgb: frame.light_rgb,
            },
            request.terrain,
            host,
        )?;
        if depth >= projection.fade[1] {
            report.culled += 1;
            return Ok(());
        }
        let texture = host
            .sprite(frame.sprite_slot)
            .map_err(ParticleDrawError::Host)?
            .ok_or(ParticleDrawError::MissingSprite(frame.sprite_slot))?;
        let dimensions = texture.sprite_dimensions()?;
        let factor = i32::from(frame.scale).wrapping_mul(scale);
        let divisor = depth.wrapping_shl(9);
        let mut half = [0; 2];
        for axis in 0..2 {
            let value = (i32::from(dimensions[axis]).wrapping_mul(factor) >> 8)
                .wrapping_mul(projection.focal[axis]);
            half[axis] = value
                .checked_div(divisor)
                .ok_or(ParticleDrawError::SizeDivision(depth))?;
        }
        let flags = executable.mapped(table.wrapping_add(0x0e), 1)?.bytes[0];
        if flags & 1 != 0 {
            half = half.map(|value| value.max(1));
        }
        if clip & 0x6d != 0
            && ((x.wrapping_add(half[0]) as u32)
                >= projection.viewport[0].wrapping_add(half[0].wrapping_mul(2)) as u32
                || (y.wrapping_add(half[1]) as u32)
                    >= projection.viewport[1].wrapping_add(half[1].wrapping_mul(2)) as u32)
        {
            report.culled += 1;
            return Ok(());
        }
        self.flag(id, 0x10)?;
        let bytes = executable.mapped(table.wrapping_add(8), 2)?.bytes;
        let vertex_depth = depth.wrapping_add(i32::from(i16::from_le_bytes(
            bytes.try_into().expect("WORD"),
        ))) as u32;
        let mut sort = vertex_depth;
        if host.material_sorting().map_err(ParticleDrawError::Host)? {
            let RenderResourceData::Sprite(entry) = texture.data else {
                return Err(RenderResourceError::Registration {
                    request: texture.request,
                }
                .into());
            };
            let selector = entry.selectors.mode0;
            if selector == 255 {
                sort = (sort >> 1).wrapping_add(0xc000_0000);
            } else {
                let variant = entry.variants.get(usize::from(selector)).ok_or(
                    RenderResourceError::Registration {
                        request: texture.request,
                    },
                )?;
                sort = if variant.flags.raw() & 0x39 != 0 {
                    (sort >> 1).wrapping_add(0xc000_0000)
                } else {
                    host.texture_sort_identity(texture)
                        .map_err(ParticleDrawError::Host)?
                        .sort_word(selector)?
                };
            }
        }
        let far = depth >= projection.fade[0];
        let mut command = PrimitiveCommand::new(
            if far {
                PrimitiveCallback::TexturedQuadFar
            } else {
                PrimitiveCallback::TexturedQuadNear
            },
            sort,
            3,
        );
        let center_y = if flags & 4 == 0 {
            i32::from((y as u16).wrapping_sub(half[1] as u16) as i16)
        } else {
            y
        };
        let mut left = x.wrapping_sub(half[0]);
        let mut right = x.wrapping_add(half[0]);
        let mut top = center_y.wrapping_sub(half[1]);
        let mut bottom = center_y.wrapping_add(half[1]);
        if flags & 2 != 0 {
            std::mem::swap(&mut left, &mut right);
        }
        if flags & 0x10 != 0 {
            std::mem::swap(&mut top, &mut bottom);
        }
        for (index, [x, y]) in [[left, top], [right, top], [right, bottom], [left, bottom]]
            .into_iter()
            .enumerate()
        {
            command.write_u32(index * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
            command.write_u32(index * 8 + 4, vertex_depth);
        }
        command.write_u32(0x20, 0);
        command.write_texture(0x24, Some(texture));
        if far {
            command.write_u32(0x28, request.context.far_color);
            command.write_u32(
                0x2c,
                u32::from(projected.bytes[9].ok_or(ParticleDrawError::UnwrittenFog)?) * 0x0101_0101,
            );
        }
        let vtable = self.record(id)?.read_dword(0)?;
        command.scene_fed = fireball_under_water(vtable, position, request.terrain);
        command.glow_fed = vtable == super::splashes::SPLASH_VTABLE;
        host.submit(command).map_err(ParticleDrawError::Host)?;
        report.commands += 1;
        if shadow_amount != 0 && flags & 8 != 0 {
            self.draw_shadow(
                id,
                execution,
                shadow::ShadowSubmission {
                    table,
                    scale,
                    shadow_amount,
                    half_width: half[0],
                    depth,
                    delta,
                    projected,
                },
                host,
                report,
            )?;
        }
        Ok(())
    }

    fn flag(&mut self, id: ParticleId, bit: u16) -> Result<(), ParticleError> {
        let flags = self.record(id)?.read_word(0x12)?;
        self.record_mut(id)?.write_word(0x12, flags | bit)
    }

    fn draw_light<H: ParticleDrawLighting>(
        &mut self,
        id: ParticleId,
        light: ParticleLightParameters,
        terrain: Option<&TerrainRuntime>,
        host: &mut H,
    ) -> Result<(), ParticleDrawError<H::Error>> {
        if self.record(id)?.read_word(0x12)? & 0x40 != 0 || light.amount == 0 {
            return Ok(());
        }
        let amount = i32::from(light.amount).wrapping_shl(8);
        let position = self
            .record(id)?
            .position()?
            .map(|word| i32::from(word as i16));
        let radius = if amount > 0 {
            let floor = if self.record(id)?.read_word(0x12)? & 0x20 != 0 {
                self.record(id)?.read_word(0x24)? as i16
            } else {
                self.cached_terrain_floor(id, terrain.ok_or(ParticleDrawError::MissingTerrain)?)?
            };
            amount / position[1].wrapping_sub(i32::from(floor)).max(256)
        } else {
            let lifetime = self.record(id)?.read_word(0x10)?;
            let radius = amount.wrapping_neg();
            if lifetime < 1024 {
                i32::from(lifetime).wrapping_mul(radius) >> 10
            } else {
                radius
            }
        };
        host.terrain_light(
            TerrainLightRequest {
                world_xz: [position[0], position[2]],
                radius,
                rgb: light.rgb,
            },
            position,
        )
        .map_err(ParticleDrawError::Host)
    }
}
