//! Fresh selected-pointer/process inputs for the shared projection arithmetic.
use super::{local_coordinates, project_with_inputs, world_coordinates, ProjectionInputs};
use crate::{
    frontend_scene::ModelSurfaceTableOrigin,
    model_context::ModelDispatch,
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
    process_model_graphics::{
        ProcessModelGraphics, ProcessModelGraphicsError, ProjectionEntry, SelectedProjectionSlot,
    },
};

///One mandatory real process source and actual context+3C alias. This reads
///the selected callback FIRST, without fabricating unused projection inputs.
pub(crate) struct ProcessModelProjector<'a> {
    pub process: &'a ProcessModelGraphics,
    pub table: ModelSurfaceTableOrigin,
}
impl ModelVertexProjector for ProcessModelProjector<'_> {
    type Error = ProcessModelGraphicsError;
    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        project_with_inputs(self, input, output)
    }

    fn view_point(&mut self, input: ModelVertexInput) -> Option<[i32; 3]> {
        let dispatch = self.selected_dispatch(input).ok()?;
        self.coordinates(input, dispatch).ok()
    }
}
impl ProjectionInputs for ProcessModelProjector<'_> {
    type Error = ProcessModelGraphicsError;
    fn selected_dispatch(&mut self, input: ModelVertexInput) -> Result<ModelDispatch, Self::Error> {
        let slot = match input {
            ModelVertexInput::Local(_) => SelectedProjectionSlot::Local1c,
            ModelVertexInput::World(_) => SelectedProjectionSlot::World20,
        };
        match self.process.selected(self.table, slot)? {
            ProjectionEntry::NativeNull => Err(ProcessModelGraphicsError::NativeNullProjection {
                table: self.table,
                slot,
            }),
            ProjectionEntry::NearLocal | ProjectionEntry::NearWorld => Ok(ModelDispatch::Near),
            ProjectionEntry::FarLocal | ProjectionEntry::FarWorld => Ok(ModelDispatch::Far),
        }
    }
    fn coordinates(
        &mut self,
        input: ModelVertexInput,
        dispatch: ModelDispatch,
    ) -> Result<[i32; 3], Self::Error> {
        match input {
            ModelVertexInput::Local(point) => Ok(local_coordinates(
                point,
                self.process.world()?.transform,
                dispatch,
            )),
            ModelVertexInput::World(point) => Ok(world_coordinates(
                point,
                self.process.view()?,
                self.process.shift()?,
            )),
        }
    }
    fn focal(&mut self) -> Result<[i32; 2], Self::Error> {
        self.process.focal()
    }
    fn center(&mut self) -> Result<[i32; 2], Self::Error> {
        self.process.center()
    }
    fn viewport(&mut self) -> Result<[i32; 2], Self::Error> {
        self.process.viewport()
    }
    fn fog(&mut self, depth: i32) -> Result<u8, Self::Error> {
        let [near, far] = self.process.fade()?;
        if depth < near {
            Ok(0)
        } else if depth >= far {
            Ok(255)
        } else {
            Ok((depth
                .wrapping_sub(near)
                .wrapping_mul(self.process.fog_factor()?)
                >> 16) as u8)
        }
    }
}
