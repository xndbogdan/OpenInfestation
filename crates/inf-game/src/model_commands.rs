//! Concrete model executor -> native primitive -> deferred draw queue adapter.
//! Pixel callbacks remain required when the renderer flushes the shared queue.

use std::convert::Infallible;
use thiserror::Error;

use crate::{
    draw_queue::{DrawQueue, DrawQueueError, DrawQueueFault, DrawSubmission},
    model_context::{ModelContext, ModelDispatch, ModelTransform},
    model_execution::{ModelExecutionHost, ModelPrimitiveRequest},
    model_primitives::{
        self, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState,
        PrimitiveTables, TextureSortIdentity,
    },
    model_values::{ModelValueCallback, ModelValueError, ModelValues},
    model_vertices::{
        ModelCustomVertexError, ModelCustomVertexRequest, ModelVertexFault, ModelVertices,
    },
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceError, RenderResourceKind,
        RenderResourceRequest, RenderResources,
    },
};

/// The original renderer's virtual+04 is called both for an admitted primitive
/// and before a depth group. It can publish pending commands before returning
/// its batching policy; group entry discards that return value.
pub trait ModelCommandServices<'a>: ModelValueCallback {
    fn prepare_draw(
        &mut self,
        queue: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error>;
    /// Required only for opaque material grouping. Native addresses and
    /// portable allocation ordinals remain explicit distinct policies.
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    /// Process RNG word 0055F18C for jitter controls (opcode 0x22).
    /// Defaults to none, preserving the previous unsupported boundary;
    /// the software scene overrides it with its per-draw session word.
    fn process_rng(&mut self) -> Option<&mut u32> {
        None
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        crate::model_vertices::ModelCustomVertexPolicy::Unowned
    }
    /// Required callback for the direct manager table's forced world2D.
    fn required_custom_vertex(
        &mut self,
        _: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }
    fn furniture_custom_vertex(
        &mut self,
        _: ModelCustomVertexRequest<'_>,
        _: &mut ModelVertices,
        _: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        Err(ModelVertexFault::UnownedCustomVertex.into())
    }
    /// Non-native: the world's scene feed while one is collected.
    fn scene_feed(&mut self) -> Option<&mut crate::scene_feed::SceneFeedBuilder<'a>> {
        None
    }
    /// Non-native: where model `model`, drawn in `transform`, stood in the
    /// last frame, while the Vulkan raster follows moving models.
    fn draw_motion(
        &mut self,
        _model: u16,
        _transform: ModelTransform,
    ) -> Option<crate::raster_motion::ViewMotion> {
        None
    }
    /// Non-native: a spark the model's faces are laid in again, added,
    /// under Vulkan (a struck crystal's).
    fn draw_tint(&self) -> Option<crate::crystal_flares::CrystalSpark> {
        None
    }
    /// Non-native: the world's modern sky while Vulkan's raster draws it,
    /// which takes the painting's place.
    fn raster_sky(&self) -> Option<&crate::modern_tuning::SkyTuning> {
        None
    }
    /// Non-native: the metal face `request` draws while Vulkan's raster
    /// mirrors the sky in metal ([`crate::raster_metal`]).
    fn raster_metal(
        &mut self,
        _request: &ModelPrimitiveRequest<'_>,
        _resources: &RenderResources<'a>,
    ) -> Option<std::rc::Rc<crate::raster_metal::MetalFace>> {
        None
    }
}

#[derive(Debug, Error)]
pub enum ModelCommandError<E> {
    #[error("required model command service failed: {0}")]
    Service(E),
    #[error(transparent)]
    Process(#[from] crate::process_model_graphics::ProcessModelGraphicsError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Queue(#[from] DrawQueueFault),
    #[error("native primitive: {0}")]
    Primitive(PrimitiveError<Infallible>),
    #[error("model command backend stopped after an earlier failure")]
    Stopped,
    #[error("primitive does not match the published model transform or projection table")]
    Publication,
}

/// The renderer owns `queue` across roots and batches. Residency cannot change
/// while queued texture relocations borrow its resources. `state` is the live
/// renderer state accompanying ModelDrawRequest.environment; callers publish
/// the same projection configuration to both owners.
pub struct ModelCommandBackend<'a, 'q, S> {
    resources: RenderResources<'a>,
    tables: &'a PrimitiveTables,
    pub state: PrimitiveState,
    pub queue: &'q mut DrawQueue<PrimitiveCommand<'a>>,
    pub services: S,
    publication: Option<(ModelContext, ModelTransform)>,
    dispatch: Option<ModelDispatch>,
    stopped: bool,
    outcome: PrimitiveOutcome,
    /// The scene feed took the primitive being prepared.
    scene_fed: bool,
    /// The scene feed took the primitive's 3D additive glow faces, while the
    /// raster still draws it for the primary view.
    glow_fed: bool,
    /// The motion of the moving model whose primitive is being prepared.
    motion: Option<crate::raster_motion::ViewMotion>,
    /// Its tint, if any.
    tint: Option<crate::crystal_flares::CrystalSpark>,
    /// How the raster draws it under its own modern sky.
    sky: Option<crate::scene_capture::RasterSkyDraw>,
    /// Its metal face, while Vulkan's raster mirrors the sky in metal.
    metal: Option<std::rc::Rc<crate::raster_metal::MetalFace>>,
    /// The exact screen points of its model's corners, while its frame
    /// keeps twins.
    exact_screens: Option<std::rc::Rc<crate::fast_hash::FastMap<u32, [f32; 2]>>>,
}

impl<'a, 'q, S: ModelCommandServices<'a>> ModelCommandBackend<'a, 'q, S> {
    pub fn new(
        resources: RenderResources<'a>,
        tables: &'a PrimitiveTables,
        state: PrimitiveState,
        queue: &'q mut DrawQueue<PrimitiveCommand<'a>>,
        services: S,
    ) -> Self {
        Self {
            resources,
            tables,
            state,
            queue,
            services,
            publication: None,
            dispatch: None,
            stopped: false,
            outcome: PrimitiveOutcome::default(),
            scene_fed: false,
            glow_fed: false,
            motion: None,
            tint: None,
            sky: None,
            metal: None,
            exact_screens: None,
        }
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }
    pub fn outcome(&self) -> PrimitiveOutcome {
        self.outcome
    }
    pub fn publication(&self) -> Option<(ModelContext, ModelTransform)> {
        self.publication
    }
    pub fn dispatch(&self) -> Option<ModelDispatch> {
        self.dispatch
    }

    fn ensure_running(&self) -> Result<(), ModelCommandError<S::Error>> {
        if self.stopped {
            Err(ModelCommandError::Stopped)
        } else {
            Ok(())
        }
    }
    fn finish<T>(
        &mut self,
        result: Result<T, ModelCommandError<S::Error>>,
    ) -> Result<T, ModelCommandError<S::Error>> {
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
}

impl<'a, S: ModelCommandServices<'a>> ModelValueCallback for ModelCommandBackend<'a, '_, S> {
    type Error = ModelCommandError<S::Error>;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.ensure_running()?;
        let result = self
            .services
            .packed_value(values, selector)
            .map_err(ModelCommandError::Service);
        self.finish(result)
    }
}

impl<'a, S: ModelCommandServices<'a>> PrimitiveHost<'a> for ModelCommandBackend<'a, '_, S> {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.ensure_running()?;
        // 00405540 accepts index==pool+08 despite PreLoad allocating only count
        // entries. Keep that one-past read an explicit error, not a fabricated
        // null; only negative and STRICTLY greater indices are known nulls.
        let slot = argument as i16;
        if slot < 0 || slot as usize > self.resources.pool_capacity(RenderResourceKind::Sprite) {
            return Ok(None);
        }
        let result = self
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument,
            })
            .map(Some)
            .map_err(ModelCommandError::Resource);
        self.finish(result)
    }
    fn material(&mut self, argument: i32) -> Result<u32, Self::Error> {
        self.ensure_running()?;
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        };
        let result = self
            .resources
            .resolve(request)
            .map_err(ModelCommandError::Resource)
            .and_then(|resource| match resource.data {
                RenderResourceData::Material(value) => Ok(value.raw()),
                _ => Err(RenderResourceError::Registration { request }.into()),
            });
        self.finish(result)
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.ensure_running()?;
        let result = self
            .services
            .prepare_draw(self.queue)
            .map_err(ModelCommandError::Service);
        let result = result.and_then(|value| {
            if self.queue.state().stopped {
                Err(DrawQueueFault::Stopped.into())
            } else {
                Ok(value)
            }
        });
        self.finish(result)
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.ensure_running()?;
        let result = self
            .services
            .texture_sort_identity(texture)
            .map_err(ModelCommandError::Service);
        self.finish(result)
    }
    fn submit(&mut self, mut command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.ensure_running()?;
        command.scene_fed = self.scene_fed;
        command.glow_fed = self.glow_fed;
        command.motion = self.motion;
        command.tint = self.tint;
        command.sky = self.sky;
        command.metal = self.metal.clone();
        if let Some(screens) = &self.exact_screens {
            for offset in (0..command.payload.len()).step_by(4) {
                let Some(xy) = command.read_u32(offset) else {
                    continue;
                };
                if let Some(&exact) = screens.get(&xy) {
                    if !command.exact_corners.iter().any(|(whole, _)| *whole == xy) {
                        command.exact_corners.push((xy, exact));
                    }
                }
            }
        }
        let effect_quad = matches!(
            command.callback,
            model_primitives::PrimitiveCallback::EffectQuadNear
                | model_primitives::PrimitiveCallback::EffectQuadFar
        );
        // A billboard's corners, where its command drew them.
        let corners = |command: &PrimitiveCommand<'a>| {
            let corner = |index: usize| {
                let xy = command.read_u32(index * 8)?;
                let depth = command.read_u32(index * 8 + 4)?;
                Some((
                    [i32::from(xy as i16), i32::from((xy >> 16) as i16)],
                    depth as i32,
                ))
            };
            Some([corner(0)?, corner(1)?, corner(2)?, corner(3)?])
        };
        // An additive billboard sunk under the water goes to the tracer
        // where its command drew, and the raster leaves that command; one
        // the tracer cannot place stays the raster's.
        if !self.scene_fed && effect_quad {
            if let Some(builder) = self.services.scene_feed() {
                if builder.sunk_billboard.is_some() {
                    // In the colour retail's quad draws it in (the token's
                    // pulse).
                    let color = crate::d3d_effect::read_effect_quad(&command)
                        .ok()
                        .flatten()
                        .map_or([255; 3], |quad| {
                            crate::d3d_effect::scaled_effect_rgb(quad.effect_rgb, 255.0 / 128.0)
                        });
                    if let Some(corners) = corners(&command) {
                        if builder.place_sunk_billboard(corners, color) {
                            command.scene_fed = true;
                            builder.submitted(command.depth);
                        }
                    }
                }
            }
        }
        if self.scene_fed {
            // Where the raster draws the feed's faces, for its ties.
            if let Some(builder) = self.services.scene_feed() {
                // A billboard the modern sky keeps, or a gate's flare, where
                // its command drew.
                if (builder.sky_billboard.is_some() || builder.gate_glint.is_some()) && effect_quad
                {
                    if let Some(corners) = corners(&command) {
                        builder.place_sky_billboard(corners);
                        builder.place_gate_glint(corners);
                    }
                }
                builder.submitted(command.depth);
            }
        }
        let submission = DrawSubmission {
            depth: command.depth,
            payload_blocks: command.payload_blocks,
            receiver_offset: command.header_words[0],
            command,
        };
        let result = self
            .queue
            .submit(submission)
            .map(|_| ())
            .map_err(ModelCommandError::Queue);
        self.finish(result)
    }
}

impl<'a, S: ModelCommandServices<'a>> ModelExecutionHost for ModelCommandBackend<'a, '_, S> {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.ensure_running()?;
        self.publication = Some((*context, transform));
        Ok(())
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.ensure_running()?;
        self.dispatch = Some(dispatch);
        Ok(())
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        values: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        self.ensure_running()?;
        if self
            .publication
            .map(|(context, transform)| (context.position, context.basis, transform))
            != Some((
                request.context.position,
                request.context.basis,
                request.transform,
            ))
            || self.dispatch != Some(request.dispatch)
        {
            return self.finish(Err(ModelCommandError::Publication));
        }
        // The scene feed's copy comes first and changes nothing the game sees.
        let (mode, far_color) = (self.state.mode, self.state.far_color);
        let fade = self.state.projection.fade;
        (self.scene_fed, self.glow_fed) = match self.services.scene_feed() {
            Some(builder) => {
                builder.begin_primitive();
                let fed = crate::scene_capture::capture_primitive(
                    builder,
                    &self.resources,
                    &request,
                    crate::scene_capture::CaptureState {
                        mode,
                        far_color,
                        fade,
                    },
                );
                (fed, builder.glow_fed)
            }
            None => (false, false),
        };
        self.motion = self
            .services
            .draw_motion(request.model.identity.global_index, request.transform);
        self.tint = self.services.draw_tint();
        self.sky = self
            .services
            .raster_sky()
            .and_then(|look| crate::scene_capture::raster_sky_draw(&request, look));
        // The sky's models are never metal.
        self.metal = if self.sky.is_none() {
            self.services.raster_metal(&request, &self.resources)
        } else {
            None
        };
        self.exact_screens = request.vertices.exact_screens();
        // Opcode03 mutates only saved basis, not the published world transform.
        let result = model_primitives::prepare(&request, self.state, self.tables, values, self)
            .map_err(flatten_primitive);
        self.scene_fed = false;
        self.glow_fed = false;
        self.motion = None;
        self.tint = None;
        self.sky = None;
        self.metal = None;
        self.exact_screens = None;
        let outcome = self.finish(result)?;
        self.outcome.commands += outcome.commands;
        self.outcome.rejected += outcome.rejected;
        Ok(())
    }
    fn begin_depth_group(&mut self, depth: i32, sorting: bool) -> Result<(), Self::Error> {
        self.ensure_running()?;
        let services = &mut self.services;
        let result = self
            .queue
            .begin_group(depth as u32, sorting, |queue| {
                services.prepare_draw(queue).map(|_| ())
            })
            .map_err(|error| match error {
                DrawQueueError::Fault(fault) => ModelCommandError::Queue(fault),
                DrawQueueError::Host(error) => ModelCommandError::Service(error),
            });
        self.finish(result)
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        self.ensure_running()?;
        let result = self.queue.end_group().map_err(ModelCommandError::Queue);
        self.finish(result)
    }
    fn sine_quarter_table(&self) -> Option<&[u16; 4096]> {
        Some(self.tables.sine_table.as_ref())
    }
    fn process_rng(&mut self) -> Option<&mut u32> {
        self.services.process_rng()
    }
    fn custom_vertex_policy(&self) -> crate::model_vertices::ModelCustomVertexPolicy {
        self.services.custom_vertex_policy()
    }
    fn required_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.ensure_running()
            .map_err(ModelCustomVertexError::Host)?;
        let result = self
            .services
            .required_custom_vertex(request, vertices, output)
            .map_err(|error| match error {
                ModelCustomVertexError::Fault(fault) => ModelCustomVertexError::Fault(fault),
                ModelCustomVertexError::Host(error) => {
                    ModelCustomVertexError::Host(ModelCommandError::Service(error))
                }
            });
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn furniture_custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
    ) -> Result<(), ModelCustomVertexError<Self::Error>> {
        self.ensure_running()
            .map_err(ModelCustomVertexError::Host)?;
        let result = self
            .services
            .furniture_custom_vertex(request, vertices, output)
            .map_err(|error| match error {
                ModelCustomVertexError::Fault(fault) => ModelCustomVertexError::Fault(fault),
                ModelCustomVertexError::Host(error) => {
                    ModelCustomVertexError::Host(ModelCommandError::Service(error))
                }
            });
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
}

fn flatten_primitive<E>(error: PrimitiveError<ModelCommandError<E>>) -> ModelCommandError<E> {
    let error = match error {
        PrimitiveError::Host(error) | PrimitiveError::Value(ModelValueError::Callback(error)) => {
            return error
        }
        PrimitiveError::Resource(error) => PrimitiveError::Resource(error),
        PrimitiveError::TextureSort(error) => PrimitiveError::TextureSort(error),
        PrimitiveError::Value(ModelValueError::Fault(error)) => {
            PrimitiveError::Value(ModelValueError::Fault(error))
        }
        PrimitiveError::Vertex(error) => PrimitiveError::Vertex(error),
        PrimitiveError::Lighting(error) => PrimitiveError::Lighting(error),
        PrimitiveError::Unknown { field, index } => PrimitiveError::Unknown { field, index },
        PrimitiveError::Record(offset) => PrimitiveError::Record(offset),
        PrimitiveError::Unsupported { opcode, reason } => {
            PrimitiveError::Unsupported { opcode, reason }
        }
    };
    ModelCommandError::Primitive(error)
}

#[cfg(test)]
#[path = "model_commands_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model_commands_failure_tests.rs"]
mod failure_tests;
