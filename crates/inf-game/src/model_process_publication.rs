//! Recording/forwarding adapter over the existing model command host.
//! Current process publications are checked against actual view/shift loans;
//! typed process custody failures stay hard at existing scene classifiers.
use crate::{
    frontend_scene::ModelSurfaceTableOrigin,
    model_context::{ModelContext, ModelDispatch, ModelTransform},
    model_execution::{ModelExecutionHost, ModelPrimitiveRequest},
    model_values::{ModelValueCallback, ModelValues},
    model_vertices::{
        ModelCustomVertexError, ModelCustomVertexPolicy, ModelCustomVertexRequest, ModelVertices,
    },
    process_model_graphics::{
        ProcessModelGraphics, ProcessModelGraphicsError, PublishedModelWorld,
    },
};

///One explicit data-bearing loan; scope borrows neither RefCell nor graphics
///values across primitive/model/resource callbacks.
pub(crate) struct ModelProcessPublication<'a, H> {
    pub inner: &'a mut H,
    pub process: &'a ProcessModelGraphics,
    pub table: ModelSurfaceTableOrigin,
}
impl<H: ModelExecutionHost> ModelValueCallback for ModelProcessPublication<'_, H> {
    type Error = H::Error;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.inner.packed_value(values, selector)
    }
}
impl<H> ModelExecutionHost for ModelProcessPublication<'_, H>
where
    H: ModelExecutionHost,
    H::Error: From<ProcessModelGraphicsError>,
{
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        //Retain existing backend running/error/cache prefix first. A process
        //loan failure then preserves that reached prefix and remains hard.
        self.inner.publish_transform(context, transform)?;
        let expected = context.publish(self.process.view()?, self.process.shift()?);
        if expected != transform {
            return Err(ProcessModelGraphicsError::PublicationSourceMismatch.into());
        }
        self.process
            .publish_model(PublishedModelWorld {
                position: context.position,
                basis: context.basis,
                transform,
            })
            .map_err(Into::into)
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.inner.select_projection(dispatch)?;
        self.process
            .select_projection(self.table, dispatch)
            .map_err(Into::into)
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.inner.primitive(request, values)
    }
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        self.inner.begin_depth_group(depth, sorting)
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.inner.end_depth_group()
    }
    fn sine_quarter_table(&self) -> Option<&[u16; 4096]> {
        self.inner.sine_quarter_table()
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        self.inner.process_rng()
    }
    fn custom_vertex_policy(&self) -> ModelCustomVertexPolicy {
        self.inner.custom_vertex_policy()
    }
    fn furniture_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.inner
            .furniture_custom_vertex(request, vertices, output)
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.inner.required_custom_vertex(request, vertices, output)
    }
}

#[cfg(test)]
mod tests;
