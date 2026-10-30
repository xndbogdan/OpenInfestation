//! The Force Field's dome 004777A0 on this scene, after its +2C 00477370
//! has set the light: the quads enter the shared command arena like other
//! deferred packets.
use super::*;
use crate::model_context::{ModelContext, ModelDispatch};
use crate::model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector};
use crate::object_runtime::{
    force_field_dome, ribbon_sort_key, DomePoint, ForceFieldDome, ForceFieldDrawError, ObjectArena,
    ObjectDrawCallbackBoundary, ObjectDrawContext, ObjectDrawServiceError, ObjectId,
    FORCE_FIELD_BAND_SPRITE, FORCE_FIELD_BOTTOM_SPRITE, FORCE_FIELD_DECAL_SPRITE,
    FORCE_FIELD_LIGHT_GREY, FORCE_FIELD_MODE,
};
use crate::terrain_runtime::{TerrainRuntime, TerrainWaterSurfaceRequest};

fn stopped<E>(reason: impl ToString) -> ObjectDrawServiceError<E> {
    ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::Selector18ForceFieldDraw {
        reason: reason.to_string(),
    })
}

/// The world values 004777A0 reads besides the field.
pub(super) struct ForceFieldWorld<'a> {
    /// H12/0, which 004774E0 samples for the ground.
    pub terrain: &'a TerrainRuntime,
    /// H14/0 +4, whose 0x40 lets 00410F60 wave.
    pub world_flags: u32,
    /// Settings +70.
    pub detail: u32,
    /// The animation clock 005471C0.
    pub clock: u32,
}

impl SoftwareScene<'_, '_> {
    /// `004777A0` at the field (`0048FE60`): the three sprites, each NULL
    /// past the H3 count, then each quad sorted against the renderer's
    /// current material policy.
    pub(super) fn draw_force_field(
        &mut self,
        arena: &ObjectArena,
        field: ObjectId,
        context: &ObjectDrawContext,
        world: ForceFieldWorld<'_>,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let state = arena.selector18(field).map_err(stopped)?;
        let position = arena
            .object(field)
            .map_err(stopped)?
            .base
            .position_words
            .ok_or_else(|| stopped("the field's position is unknown"))?;
        let sprite = |slot: i32| {
            if slot as usize > self.resources.pool_capacity(RenderResourceKind::Sprite) {
                return Ok(None);
            }
            self.resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: slot,
                })
                .map(Some)
                .map_err(|error| ObjectDrawServiceError::Backend(error.into()))
        };
        let band = sprite(FORCE_FIELD_BAND_SPRITE)?;
        let bottom = sprite(FORCE_FIELD_BOTTOM_SPRITE)?;
        let decal = sprite(FORCE_FIELD_DECAL_SPRITE)?;
        // 0055F26C, which 00490030 has just set from the ambient and the
        // grey; its fourth byte is never written.
        let ambient = self.environment.lighting.ambient_word.to_le_bytes();
        let light = u32::from_le_bytes([
            ambient[0].saturating_add(FORCE_FIELD_LIGHT_GREY),
            ambient[1].saturating_add(FORCE_FIELD_LIGHT_GREY),
            ambient[2].saturating_add(FORCE_FIELD_LIGHT_GREY),
            0,
        ]);
        let (segments, meridians) = ForceFieldDome::rings(world.detail);
        let dome = ForceFieldDome {
            position,
            radius: state.radius_154,
            height: state.height_158,
            fade: state.fade_160,
            segments,
            meridians,
            clock: world.clock,
            light,
        };
        let environment = self.environment;
        // 0048FE60 receives the field's sign-extended WORDs.
        let mut projector = crate::model_projection::ModelProjectionBackend {
            transform: ModelContext::new(
                position.map(|value| i32::from(value as i16)),
                crate::menu::MenuBasis::INITIAL,
            )
            .publish(context.view, environment.coordinate_shift),
            view: context.view,
            projection: environment.projection,
            dispatch: ModelDispatch::Far,
            coordinate_shift: environment.coordinate_shift,
            fog_factor: environment.fog_factor,
        };
        let sine = *self.sine_table();
        let mut quads = Vec::new();
        force_field_dome::<String>(
            &dome,
            &sine,
            |x, z| {
                // 004776AA..004776F5: the higher of the ground and its water
                // at clock 0.
                let terrain = world.terrain.height_at([x, z]);
                let water = world.terrain.water_surface_height(
                    TerrainWaterSurfaceRequest {
                        xz: [x, z],
                        wave_clock: 0,
                        world_flags: world.world_flags,
                        terrain_height: terrain,
                    },
                    &sine,
                );
                Ok(terrain.max(water))
            },
            |local| {
                let mut output = ModelVertexProjection::default();
                let depth = match projector.project(ModelVertexInput::Local(local), &mut output) {
                    Ok(depth) => depth,
                    Err(error) => match error {},
                };
                Ok(DomePoint {
                    xy: output
                        .read_u32(0)
                        .ok_or("the projection left its XY unwritten")?,
                    depth,
                    clip: output.bytes[8].ok_or("the projection left byte 8 unwritten")?,
                    fog: output.bytes[9],
                })
            },
            |quad| {
                quads.push(quad);
                Ok(())
            },
        )
        .map_err(|error| match error {
            ForceFieldDrawError::Host(error) => stopped(error),
            other => stopped(other),
        })?;
        for quad in quads {
            let texture = match quad.sprite {
                FORCE_FIELD_DECAL_SPRITE => decal,
                FORCE_FIELD_BOTTOM_SPRITE => bottom,
                _ => band,
            };
            let sorting = self.material_sorting();
            let sort = ribbon_sort_key(quad.depth, FORCE_FIELD_MODE, texture, sorting, |texture| {
                self.texture_identities.resolve(texture)
            })
            .map_err(stopped)?;
            let command = quad.command(sort, texture, light, context.far_color);
            self.queue
                .submit(DrawSubmission {
                    depth: command.depth,
                    payload_blocks: command.payload_blocks,
                    receiver_offset: 0,
                    command,
                })
                .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?;
        }
        Ok(())
    }
}
