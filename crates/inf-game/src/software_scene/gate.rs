//! A race gate's +2C 0046FB50 on this scene, after its 0047AFC0: each post
//! a quad into the shared command arena like other deferred packets.
use super::*;
use crate::model_context::{ModelContext, ModelDispatch};
use crate::model_vertices::ModelVertexProjector;
use crate::object_runtime::{
    gate_beam, gate_sprite_slot, ribbon_sort_key, ObjectArena, ObjectDrawCallbackBoundary,
    ObjectDrawContext, ObjectDrawServiceError, ObjectId, Selector69Projected,
};
use inf_formats::native_basis::NativeBasis;

/// `0053A7F0`, the posts' mode word: 4, which nothing writes.
const GATE_MODE_0053A7F0: u32 = 4;

fn stopped<E>(reason: impl ToString) -> ObjectDrawServiceError<E> {
    ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::Selector72GateDraw {
        reason: reason.to_string(),
    })
}

impl SoftwareScene<'_, '_> {
    /// `0046FB7B..00470143`: the sprite by the side the view sees, then each
    /// post's top and foot projected through the context's projection at the
    /// context position (`0048FE60`) and joined. Each quad sorts against the
    /// renderer's current material policy.
    pub(super) fn draw_gate(
        &mut self,
        arena: &ObjectArena,
        gate: ObjectId,
        context: &ObjectDrawContext,
        posts: &[([u16; 3], [u16; 3])],
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let basis = arena.race_object(gate).map_err(stopped)?.rotating.basis;
        let view = NativeBasis {
            words: context.view.basis.words,
        };
        let texture = Some(
            self.resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: gate_sprite_slot(view, basis),
                })
                .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?,
        );
        let origin = context.view.eye.map(|value| value as u16);
        let environment = self.environment;
        let mut projector = crate::model_projection::ModelProjectionBackend {
            transform: ModelContext::new(
                origin.map(|value| i32::from(value as i16)),
                crate::menu::MenuBasis::INITIAL,
            )
            .publish(context.view, environment.coordinate_shift),
            view: context.view,
            projection: environment.projection,
            dispatch: ModelDispatch::Far,
            coordinate_shift: environment.coordinate_shift,
            fog_factor: environment.fog_factor,
        };
        let focal_x = environment.projection.focal[0];
        let mut project = |point: [u16; 3]| -> Result<Selector69Projected, String> {
            let local =
                std::array::from_fn(
                    |axis| i32::from(point[axis].wrapping_sub(origin[axis]) as i16),
                );
            let mut output = crate::model_vertices::ModelVertexProjection::default();
            let depth = match projector.project(
                crate::model_vertices::ModelVertexInput::Local(local),
                &mut output,
            ) {
                Ok(depth) => depth as i32,
                Err(error) => match error {},
            };
            Ok(Selector69Projected {
                xy: output
                    .read_u32(0)
                    .ok_or("the projection left its XY unwritten")?,
                depth,
                clip: output.bytes[8].ok_or("the projection left byte 8 unwritten")?,
                fog: output.bytes[9],
            })
        };
        let sorting = self.material_sorting();
        let effect_color = self.primitive_state.effect_color;
        // Each end projects into one stack record for all the posts
        // (0046FCB8, 0046FD8B), so a top nearer than 64, whose projection
        // writes no fog, keeps the last post's.
        let mut fogs = [None; 2];
        for &(top, foot) in posts {
            let mut top = project(top).map_err(stopped)?;
            let mut foot = project(foot).map_err(stopped)?;
            for (end, fog) in [&mut top, &mut foot].into_iter().zip(&mut fogs) {
                end.fog = end.fog.or(*fog);
                *fog = end.fog;
            }
            let Some(quad) = gate_beam(top, foot, focal_x).map_err(|_| {
                stopped("a first post's top nearer than 64 reads 0046FB50's unwritten stack fog")
            })?
            else {
                continue;
            };
            // A top at or behind the view gives the quad a reciprocal depth
            // pole or the other sign, which retail hands the driver as it is.
            if quad.corners.iter().any(|corner| corner.depth <= 0) {
                return Err(stopped(
                    "a post's top at or behind the view gives its quad reciprocal depths of both \
                     signs, which only the driver draws",
                ));
            }
            let identities = &mut self.texture_identities;
            let sort = ribbon_sort_key(
                quad.depth,
                GATE_MODE_0053A7F0,
                texture,
                sorting,
                |texture| identities.resolve(texture),
            )
            .map_err(stopped)?;
            let command = quad.command(
                sort,
                GATE_MODE_0053A7F0,
                texture,
                effect_color,
                context.far_color,
            );
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
