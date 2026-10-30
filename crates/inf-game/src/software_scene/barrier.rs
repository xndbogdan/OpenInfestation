//! The barrier's +2C 00411570 on this scene: each bolt's spark through the
//! caller, its beams into the shared command arena like other deferred
//! packets.
use super::*;
use crate::model_context::{ModelContext, ModelDispatch};
use crate::model_vertices::ModelVertexProjector;
use crate::object_runtime::{
    ribbon_sort_key, selector69_draw, ObjectArena, ObjectDrawCallbackBoundary, ObjectDrawContext,
    ObjectDrawServiceError, ObjectId, Selector69Bolt, Selector69DrawError, Selector69Projected,
    BARRIER_MODE_0052F628,
};

fn stopped<E>(reason: impl ToString) -> ObjectDrawServiceError<E> {
    ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::Selector69BarrierDraw {
        reason: reason.to_string(),
    })
}

impl SoftwareScene<'_, '_> {
    /// `00411570` after its bolts: H3 sprite 0 (`0055EDA8`), both posts
    /// projected through the context's projection at the context position
    /// (`0048FE60`) and sized, then for each bolt its spark (`spark`), its
    /// projection and, deeper than 0x40, its beams. Each beam sorts against
    /// the renderer's current material policy.
    pub(super) fn draw_barrier(
        &mut self,
        arena: &ObjectArena,
        barrier: ObjectId,
        context: &ObjectDrawContext,
        bolts: &[Selector69Bolt],
        executable: inf_formats::FrontendExecutable<'_>,
        mut spark: impl FnMut([u16; 3]) -> Result<(), String>,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let texture = Some(
            self.resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: 0,
                })
                .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?,
        );
        let posts = arena.selector69_posts(barrier).map_err(stopped)?;
        let mode = executable.word(BARRIER_MODE_0052F628).map_err(stopped)?;
        // 0048FE60 receives the context position's sign-extended low WORDs.
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
        // The renderer's +4 material-sorting query reads fields this draw
        // never writes.
        let sorting = self.material_sorting();
        let queue = &mut self.queue;
        let identities = &mut self.texture_identities;
        let effect_color = self.primitive_state.effect_color;
        selector69_draw(
            posts,
            bolts,
            focal_x,
            |point| project(point).map_err(stopped),
            |point| spark(point).map_err(stopped),
            |quad| {
                let sort = ribbon_sort_key(quad.depth, mode, texture, sorting, |texture| {
                    identities.resolve(texture)
                })
                .map_err(stopped)?;
                let command = quad.command(sort, mode, texture, effect_color, context.far_color);
                queue
                    .submit(DrawSubmission {
                        depth: command.depth,
                        payload_blocks: command.payload_blocks,
                        receiver_offset: 0,
                        command,
                    })
                    .map(|_| ())
                    .map_err(|error| ObjectDrawServiceError::Backend(error.into()))
            },
        )
        .map_err(|error| match error {
            Selector69DrawError::Fog(error) => stopped(error),
            Selector69DrawError::Host(error) => error,
        })?;
        Ok(())
    }
}
