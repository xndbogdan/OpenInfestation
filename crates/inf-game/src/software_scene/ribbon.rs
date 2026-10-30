//! The selector67 ribbon's +2C 00486B30 on this scene: its quads enter the
//! shared command arena like other deferred packets.
use super::*;
use crate::model_context::{ModelContext, ModelDispatch};
use crate::object_runtime::{
    ribbon_sort_key, ObjectArena, ObjectDrawCallbackBoundary, ObjectDrawContext,
    ObjectDrawServiceError, ObjectId, RibbonDrawInput, RIBBON_MODE_0053C568,
};

fn omitted<E>(reason: impl ToString) -> ObjectDrawServiceError<E> {
    ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::Selector67RibbonDraw {
        reason: reason.to_string(),
    })
}

impl SoftwareScene<'_, '_> {
    /// The sprite lookup comes first, as at 00486B62; each quad then sorts
    /// against the renderer's current material policy.
    pub(super) fn draw_ribbon(
        &mut self,
        arena: &ObjectArena,
        ribbon: ObjectId,
        context: &ObjectDrawContext,
        executable: inf_formats::FrontendExecutable<'_>,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let slot = arena.selector67_texture_slot(ribbon).map_err(omitted)?;
        let texture = if slot as usize > self.resources.pool_capacity(RenderResourceKind::Sprite) {
            None
        } else {
            Some(
                self.resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Sprite,
                        argument: slot,
                    })
                    .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?,
            )
        };
        // 0048FE60 receives the context position's sign-extended low WORDs.
        let origin = context.view.eye.map(|value| value as u16);
        let environment = self.environment;
        let mut input = RibbonDrawInput {
            projector: crate::model_projection::ModelProjectionBackend {
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
            },
            origin,
            focal_x: environment.projection.focal[0],
        };
        let quads = arena
            .selector67_draw_quads(ribbon, &mut input)
            .map_err(omitted)?;
        let mode = executable.word(RIBBON_MODE_0053C568).map_err(omitted)?;
        for quad in quads {
            let sorting = self.material_sorting();
            let sort = ribbon_sort_key(quad.depth, mode, texture, sorting, |texture| {
                self.texture_identities.resolve(texture)
            })
            .map_err(omitted)?;
            let command = quad.command(sort, mode, texture, context.far_color);
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
